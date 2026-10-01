//! 没有操作系统级沙箱时的执行器（B164、B180；B187 批 9 第 8 格改写）。
//!
//! PR #36 复核 P1 + B164 原来的规定是「没有沙箱时 `exec_py`/`check_tests`/`exec_sql` 一律拒绝执行，
//! 检查期报 `E-action-no-sandbox`，运行期 `Fail(NoSandbox)`」。意图汇编 11a 与批 9 裁定 B187 起：沙箱本身保留
//! （探测到就用），探测不到时执行器在普通子进程里照常跑（静态拒绝表、断网补丁、每次新建的临时工作目录照旧，
//! 没有系统级隔离），检查期报 `W-action-no-sandbox` 告警（文本写明装法），不再产生 `Fail(NoSandbox)`。
//! 事实表的 `reversible` 仍照 B164 由 `kind != "none"` 派生：默认只是多写一份账本（缺省路径），开 `--guard`
//! 时它按不可逆动作算、要守卫（J-08）与 `--ledger-out`（`E-ledger-required`）。
//!
//! 用 `JPP_FORCE_NO_SANDBOX` 只作用于**子进程**的环境（`Command::env`，不碰当前测试进程
//! 自己的环境变量）——不会和同一 `cargo test` 二进制里并发跑的其它测试互相干扰；`sandbox::tool()`
//! 每个进程只探测一次（`OnceLock`），子进程是全新进程，探测在其中正常发生。
//! 预注册：`地基/过程记录/工程-执行器动作安全修补.md`、`地基/过程记录/工程-默认相信判断器.md`；实现：
//! `crates/jpp/src/actions/sandbox.rs`、`crates/jpp-check/src/rules/e_action_no_sandbox.rs`。
use serde_json::Value;
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};

fn tmp(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("jpp-no-sandbox-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&d);
    fs::create_dir_all(&d).unwrap();
    d
}

fn jpp_forced_no_sandbox(cwd: &Path, args: &[&str]) -> (bool, String) {
    let out = Command::new(env!("CARGO_BIN_EXE_jpp"))
        .current_dir(cwd)
        .env("JPP_FORCE_NO_SANDBOX", "1")
        .args(args)
        .output()
        .unwrap();
    (
        out.status.success(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

/// 三个执行器动作没有沙箱时照常执行：`run` 成功、出报告，stderr 有 `W-action-no-sandbox`、没有 E 级诊断；
/// 执行器按不可逆登记，没给 `--ledger-out` 时账本写到缺省路径。
#[test]
fn 三个执行器动作没有沙箱时照常执行并告警() {
    for (name, args_literal) in [
        ("exec_py", r#"["print(1+1)", "", 5]"#),
        ("check_tests", r#"["x = 1", ["assert x == 1"], 5]"#),
        ("exec_sql", r#"["nonexistent.db", "select 1"]"#),
    ] {
        let d = tmp(name);
        let src = format!(
            "budget {{calls: 2, cost: 0, depth: 8}};\ncontent(do(\"{name}\", {args_literal}, 0))\n"
        );
        fs::write(d.join("p.jpp"), src).unwrap();
        let (ok, err) = jpp_forced_no_sandbox(&d, &["run", "p.jpp", "--output", "r.json"]);
        assert!(ok, "{name}: 没有沙箱时照常跑：{err}");
        assert!(err.contains("W-action-no-sandbox"), "{name}: {err}");
        assert!(!err.contains("E-action-no-sandbox"), "{name}: {err}");
        assert!(
            !err.contains("NoSandbox"),
            "{name}: 不再产生 Fail(NoSandbox)：{err}"
        );
        assert!(d.join("r.json").exists(), "{name}: 应出报告");
        assert!(
            d.join("p.ledger.jsonl").exists(),
            "{name}: 不可逆动作的账本写到缺省路径"
        );
        let _ = fs::remove_dir_all(&d);
    }
}

/// `check` 在两种模式下都只告警、不报错。
#[test]
fn 没有沙箱时check只告警() {
    let d = tmp("check");
    let src = "budget {calls: 2, cost: 0, depth: 8};\ncontent(do(\"exec_py\", [\"print(1)\", \"\", 5], 0))\n";
    fs::write(d.join("p.jpp"), src).unwrap();
    for args in [vec!["check", "p.jpp"], vec!["check", "p.jpp", "--guard"]] {
        let (ok, err) = jpp_forced_no_sandbox(&d, &args);
        assert!(ok, "{args:?}: {err}");
        assert!(
            err.contains("W-action-no-sandbox") && !err.contains("E-action-no-sandbox"),
            "{args:?}: {err}"
        );
    }
    let _ = fs::remove_dir_all(&d);
}

/// 开 `--guard` 时没有沙箱的执行器按不可逆算：由不可信材料上的判断单独守它，检查期 J-08；没有守卫的顶层
/// 调用不给 `--ledger-out` 报 `E-ledger-required`，给了就在沙箱外执行。
#[test]
fn 开把关时没有沙箱的执行器按不可逆算() {
    let d = tmp("guarded");
    let 守卫 = r#"
budget {calls: 2, cost: 1, depth: 8};
let raw = do("read_json", ["x.json"], 0);
let ok = handle(cut(judge(state(raw), test("行吗", "k"))), {act: fn() { true }, ignore: fn() { false }, unsure: fn(u) { consume(u, "drop"); false }});
if ok { content(do("exec_py", ["print(1)", "", 5], 0)) } else { "没做" }
"#;
    fs::write(d.join("p.jpp"), 守卫).unwrap();
    let (ok, err) = jpp_forced_no_sandbox(&d, &["check", "p.jpp", "--guard"]);
    assert!(!ok && err.contains("J-08"), "{err}");
    let (ok, err) = jpp_forced_no_sandbox(&d, &["check", "p.jpp"]);
    assert!(ok && !err.contains("J-08"), "默认不拦：{err}");
    // G2（步 35）：`--guard` 下不可逆 do 推迟到结论之后，同一次运行里不能读它的结果；随返回值交出，结算后报告里看得到
    let 顶层 =
        "budget {calls: 2, cost: 0, depth: 8};\n(do(\"exec_py\", [\"print(1+1)\", \"\", 5], 0))\n";
    fs::write(d.join("q.jpp"), 顶层).unwrap();
    let (ok, err) = jpp_forced_no_sandbox(&d, &["run", "q.jpp", "--guard"]);
    assert!(!ok && err.contains("E-ledger-required"), "{err}");
    let (ok, err) = jpp_forced_no_sandbox(
        &d,
        &[
            "run",
            "q.jpp",
            "--guard",
            "--ledger-out",
            "l.jsonl",
            "--output",
            "r.json",
        ],
    );
    assert!(ok, "{err}");
    let r: Value = serde_json::from_str(&fs::read_to_string(d.join("r.json")).unwrap()).unwrap();
    assert_eq!(r["value"]["content"]["stdout"], "2\n", "{r}");
    let _ = fs::remove_dir_all(&d);
}

/// 意图汇编 11a：不开 `--guard`（默认），没有沙箱时 `exec_py` 直接跑出结果，`check` 与 `run` 只报
/// `W-action-no-sandbox` 告警、不报错；报告顶层 `ledger_path` 记下缺省账本路径。
#[test]
fn 默认没有沙箱时直接执行并告警() {
    let d = tmp("default");
    let src = "budget {calls: 2, cost: 0, depth: 8};\ncontent(do(\"exec_py\", [\"print(1+1)\", \"\", 5], 0))\n";
    fs::write(d.join("p.jpp"), src).unwrap();
    let (ok, err) = jpp_forced_no_sandbox(&d, &["run", "p.jpp", "--output", "r.json"]);
    assert!(ok, "{err}");
    assert!(err.contains("W-action-no-sandbox"), "{err}");
    let r: Value = serde_json::from_str(&fs::read_to_string(d.join("r.json")).unwrap()).unwrap();
    assert_eq!(r["value"]["stdout"], "2\n", "{r}");
    assert!(
        r["ledger_path"]
            .as_str()
            .is_some_and(|p| p.ends_with("p.ledger.jsonl")),
        "{r}"
    );
    let _ = fs::remove_dir_all(&d);
}

/// 反证：不设 `JPP_FORCE_NO_SANDBOX` 时，同样的 `exec_py` 调用应该正常跑通——确认上面几条
/// 测的是「没有沙箱」这个条件本身，不是别的东西碰巧总是失败。这条本身需要本机有真能用的
/// 沙箱（探测到且冒烟测试通过），公开仓库 CI 上可能没有，跳过时打印原因、不判失败——
/// 与上面几条「测无沙箱路径」的用例不同，那几条不受这条判断影响（主会话原话）。
#[test]
fn exec_py有沙箱时正常执行() {
    if !jpp::actions::sandbox_available() {
        eprintln!(
            "跳过 exec_py有沙箱时正常执行：本机没有可用（探测到且冒烟测试通过）的沙箱工具，环境依赖，非失败"
        );
        return;
    }
    let d = tmp("exec-py-with-sandbox");
    let src = r#"
budget {calls: 2, cost: 0, depth: 8};
content(do("exec_py", ["print(1+1)", "", 5], 0))
"#;
    fs::write(d.join("p.jpp"), src).unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_jpp"))
        .current_dir(&d)
        .args(["run", "p.jpp", "--output", "r.json"])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let r: Value = serde_json::from_str(&fs::read_to_string(d.join("r.json")).unwrap()).unwrap();
    assert_eq!(r["value"]["stdout"], "2\n", "{r}");
    let _ = fs::remove_dir_all(&d);
}
