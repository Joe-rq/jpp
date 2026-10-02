//! 世界动作 `env:step`（B159 写法二第一次落实现；主控 Z0885，推进 B0670、B0672；第三靶子预注册 §5.1）。
//!
//! **为什么要它。** B159（Fable 2026-09-26）定了环境循环不加语言构造，三种写法之二是「宿主动作 `env:step`，
//! 环境状态留在宿主，结果进账本、重放不重算」；当时只做了写法一的示例（`examples/env-snake.jpp`）。行为编译方向
//! （意图汇编 26、27）的第三靶子 botcraft 要宿主在循环里推进一个外部仿真，这里是最薄一层：L-025「跑」组的
//! `act`+`sense` 合成一步，run/bench/test/upgrade 仍随 B0477 后置。
//!
//! **契约。** `do("env:step", [{env: 名字, state: 世界状态 | unit, action: 动作 | unit, reset?: 开局参数}], seq)`。
//! 宿主按 `--env <名字>=<命令>` 登记的命令起一个子进程，把这一个记录（JSON 一行）写进 stdin，读 stdout 的最后一个
//! 非空行作 JSON 结果。结果须是记录且含 `state`、`obs`、`actions`（列表）、`idle`、`done`（布尔）；`hash`、`result`
//! 可选，原样交回。`state` 为 unit 时环境按 `reset` 开局。字段名是动作契约，任务无关。
//! 另有两个可选字段（Z0895、Z0896，第三靶子第二圈预注册 §1.1、§1.3）：`effect`——上一步动作引起的、世界自己记下的事件，
//! 原样给（`purpose_drive` 把「上一步所选的候选 + effect」放进下一步的语境）；`applied`——世界实际执行的动作（候选展开后），
//! 只供验收按世界收到的动作计数。两者都由世界自报，语言不加工。
//!
//! **无状态。** 世界状态整份进出，环境进程两次调用之间不留状态——这是可逆、成本 0 成立的前提（裁定六十九，
//! `12` §2.7 B159 附注）：同一输入必得同一输出，重放按输入哈希命中、不启动进程。子进程在操作系统沙箱里跑（B164，
//! 与执行器动作同一套：写限定在调用专属临时目录、断网）；没有沙箱时以普通子进程跑，可逆位登记为不可逆。
//!
//! **替代与放弃。** 常驻、有状态的子进程（MuJoCo 一类状态不便序列化的仿真要它）与 B159「可逆」冲突，这一圈不做
//! （缺口 N-B1）；用 shell 解析命令行（引号、管道）不做，命令按空白切成程序与参数，避免把任意 shell 交给宿主。
//! **推翻条件。** 有一个真实世界的状态不能整份序列化、又必须接进来时，改为有状态协议并另请裁定。

use super::subprocess_util::run_subprocess;
use super::{sandbox, Ctx};
use crate::interp::json_to_value;
use crate::value::Value;
use serde_json::Value as Json;
use std::time::Duration;

/// 一次 `env:step` 的子进程超时（秒）。世界一步的计算应当很快；超时是结构化失败（程序拿到失败值）。
const ENV_STEP_TIMEOUT_S: u64 = 60;

/// 契约要求的输出字段
const REQUIRED: [&str; 5] = ["state", "obs", "actions", "idle", "done"];

/// 解析 `--env` 的值 `<名字>=<命令>`：命令按空白切成程序与参数（不经 shell）
pub fn parse_env_flag(v: &str) -> Result<(String, Vec<String>), String> {
    let (name, cmd) = v
        .split_once('=')
        .filter(|(n, c)| !n.trim().is_empty() && !c.trim().is_empty())
        .ok_or_else(|| format!("--env expects <name>=<command>, got '{v}'"))?;
    let argv: Vec<String> = cmd.split_whitespace().map(str::to_string).collect();
    Ok((name.trim().to_string(), argv))
}

/// 命令名（不含路径分隔符）按 PATH 解析成绝对路径；找不到或本来就是路径的原样返回
fn resolve_program(p: &str) -> String {
    if p.contains('/') {
        return p.to_string();
    }
    std::env::var_os("PATH")
        .and_then(|path| {
            std::env::split_paths(&path)
                .map(|d| d.join(p))
                .find(|c| c.is_file())
        })
        .map(|c| c.to_string_lossy().into_owned())
        .unwrap_or_else(|| p.to_string())
}

/// 没有沙箱时 `env:step` 不可逆（裁定六十九）；有沙箱时可逆
pub(super) fn reversible() -> bool {
    sandbox::kind() != sandbox::SandboxKind::None
}

/// `do("env:step", [{env, state, action, reset?}], seq)`
pub(super) fn env_step(ctx: &Ctx, args: &[Value]) -> Result<Value, String> {
    let [req] = args else {
        return Err("expects ([{env, state, action, reset?}])".into());
    };
    let req = match req {
        Value::Mat(m) => m.content.clone(),
        other => other.to_json(),
    };
    let Json::Object(obj) = &req else {
        return Err("实参要是记录 {env, state, action, reset?}".into());
    };
    let name = obj
        .get("env")
        .and_then(Json::as_str)
        .ok_or("记录要有文本字段 env（--env 登记的名字）")?;
    let argv = ctx.envs.get(name).ok_or_else(|| {
        let mut 已登记: Vec<&str> = ctx.envs.keys().map(String::as_str).collect();
        已登记.sort();
        format!(
            "环境 {name} 没有登记。用 --env {name}=<命令> 登记；本次登记了：{}",
            if 已登记.is_empty() { "（无）".to_string() } else { 已登记.join("、") }
        )
    })?;
    let (program0, rest) = argv.split_first().ok_or("登记的命令为空")?;
    // 沙箱工具（sandbox-exec、bwrap）不按 PATH 找程序：不是路径的命令名先在 PATH 里解析成绝对路径
    let program = resolve_program(program0);
    let program = program.as_str();
    let line = serde_json::to_string(&req).map_err(|e| format!("序列化请求失败：{e}"))?;
    let call_dir = sandbox::new_call_dir("env")?;
    let (mut cmd, 沙箱) = sandbox::command(program, rest, &call_dir)?;
    // 相对路径的命令参数按程序文件所在目录解析（同 `read_json`）；程序在当前目录时 parent 是空路径，不改
    if let Some(d) = ctx.program_dir.as_ref().filter(|d| !d.as_os_str().is_empty()) {
        cmd.current_dir(d);
    }
    cmd.env("PYTHONDONTWRITEBYTECODE", "1");
    let res = run_subprocess(cmd, &format!("{line}\n"), Duration::from_secs(ENV_STEP_TIMEOUT_S));
    let _ = std::fs::remove_dir_all(&call_dir);
    let r = res.map_err(|e| format!("{e}（环境：{name}；沙箱：{沙箱}）"))?;
    if r.timed_out {
        return Err(format!("环境 {name} 超时（{ENV_STEP_TIMEOUT_S} 秒）"));
    }
    if r.exit_code != Some(0) {
        let tail: String = r.stderr.lines().rev().take(5).collect::<Vec<_>>().join(" | ");
        return Err(format!(
            "环境 {name} 退出码 {:?}；stderr 末几行：{tail}",
            r.exit_code
        ));
    }
    let last = r
        .stdout
        .lines()
        .rev()
        .find(|l| !l.trim().is_empty())
        .ok_or_else(|| format!("环境 {name} 没有输出"))?;
    let out: Json = serde_json::from_str(last)
        .map_err(|e| format!("环境 {name} 的输出不是 JSON：{e}"))?;
    check_contract(name, &out)?;
    Ok(json_to_value(&out))
}

/// 输出契约：记录，含 state、obs、actions（列表）、idle、done（布尔）
pub(super) fn check_contract(name: &str, out: &Json) -> Result<(), String> {
    let Json::Object(o) = out else {
        return Err(format!("环境 {name} 的输出要是记录"));
    };
    let lack: Vec<&str> = REQUIRED.iter().copied().filter(|k| !o.contains_key(*k)).collect();
    if !lack.is_empty() {
        return Err(format!("环境 {name} 的输出缺字段：{}", lack.join("、")));
    }
    if !o["actions"].is_array() {
        return Err(format!("环境 {name} 的 actions 要是列表"));
    }
    if !o["done"].is_boolean() {
        return Err(format!("环境 {name} 的 done 要是布尔"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 解析登记() {
        let (n, a) = parse_env_flag("w=python3 /x/a.py --iface A").unwrap();
        assert_eq!(n, "w");
        assert_eq!(a, vec!["python3", "/x/a.py", "--iface", "A"]);
        assert!(parse_env_flag("w=").is_err());
        assert!(parse_env_flag("=x").is_err());
        assert!(parse_env_flag("nope").is_err());
    }

    #[test]
    fn 契约核对() {
        let ok = serde_json::json!({"state": {}, "obs": {}, "actions": ["a"], "idle": "a", "done": false});
        assert!(check_contract("w", &ok).is_ok());
        let lack = serde_json::json!({"state": {}, "obs": {}, "actions": ["a"]});
        assert!(check_contract("w", &lack).unwrap_err().contains("idle"));
        let bad = serde_json::json!({"state": {}, "obs": {}, "actions": "a", "idle": "a", "done": false});
        assert!(check_contract("w", &bad).is_err());
    }
}
