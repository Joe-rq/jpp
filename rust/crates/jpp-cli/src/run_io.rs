//! File and client setup after the caller has checked the shared core program.
use crate::{
    fixture::Fixture,
    options::{Backend, DEFAULT_LIVE_MODEL, RunOptions},
    profile_resolve::{Resolved, uses_live_backend},
    runner,
};
use jpp_core::{
    Program,
    effects::{CalibStore, Client, FixedClient, Profile},
    ledger::Ledger,
};
use serde::{Serialize, de::DeserializeOwned};
use std::{fs, path::Path};

/// `--backend live`：只在 `live` feature 开着时才真的接 `JevClient::live`
/// （凭据只从 `~/.typesafe-key` 读，这条路上不碰任何 CLI 参数或日志）。
/// 价格从画像来（B73）：画像已由 `profile_resolve` 解析，这里不会是没有画像的情形。
#[cfg(feature = "live")]
fn live_client(model: &str, profile: &Profile) -> Result<Box<dyn Client>, String> {
    jpp_core::effects::JevClient::live(model, profile.price_per_input_token)
        .map(|c| Box::new(c) as Box<dyn Client>)
        .map_err(|e| e.0)
}

#[cfg(not(feature = "live"))]
fn live_client(_model: &str, _profile: &Profile) -> Result<Box<dyn Client>, String> {
    Err("--backend live requires jpp-cli built with `--features live`".into())
}

/// 真机运行的画像没有价格时（B73；`20` §3.9 `cost` 未测行）：报告的费用记 `Unknown`，
/// 报 `W-cost-unknown`（stderr 与 `trace.warnings` 各一条）。预算里这部分按 0 累计（B42 不拒）。
fn mark_cost_unknown(report: &mut serde_json::Value, profile_path: &Path) {
    let w = format!(
        // 依据：B73（地基/附注/2026-09-24-评估①裁定.md §二；21 步 15d-0）
        "W-cost-unknown: 画像 {} 没有 cost.price_usd_per_input_token，本次真机费用记为 Unknown，预算的费用上限没有核到",
        profile_path.display()
    );
    eprintln!("warning: {w}");
    report["cost"]["usd"] = serde_json::json!("Unknown");
    if let Some(ws) = report["trace"]["warnings"].as_array_mut() {
        ws.push(serde_json::json!(w));
    }
}

/// 重放专用：拒绝一切调用（同 `jpp_core::effects::NoCallClient`），但 `model_id` 是
/// **传进来的那个**，不是硬写的 `"fixed-0"`。
///
/// 账本键含 `model_id`（`ledger.rs` 的 `judge_key`），而 `Interp::new` 用**替它跑重放的
/// 那个 client** 的 `model_id()` 去算查表键（`interp.rs:363`）——这与「用哪个 client 记的」
/// 完全无关，是重放调用者自己声明的。`NoCallClient` 硬写 `"fixed-0"`，这在此前只对
/// `FixedClient`（同样是 `"fixed-0"`）成立；真机记的账本 `model_id` 是 `--model`（例如
/// `jev-1.13.0`），照抄 `NoCallClient` 会让重放的查表键与记录时的键对不上，
/// 表现为「重放中不应发调用」——这不是抖动，是这一条越界接线必须自己解决的键匹配。
///
/// 值从账本自己的 `header.model_id` 读（`write_json` 落盘前由 `Interp::new` 写入），
/// **不必用户在重放时重新声明 `--backend`/`--model`**；旧账本没有 header 时退回
/// `"fixed-0"`，与接线前的行为逐字节相同。
struct ReplayClient(String);
impl Client for ReplayClient {
    fn model_id(&self) -> String {
        self.0.clone()
    }
    fn judge(&mut self, _s: &jpp_core::value::State, q: &[&jpp_core::value::Question]) -> Result<jpp_core::effects::JudgeResult, jpp_core::effects::EffectError> {
        Err(jpp_core::effects::EffectError(format!(
            "重放中不应发调用（题 {}）",
            q.iter().map(|x| x.text.as_str()).collect::<Vec<_>>().join("|")
        )))
    }
    fn generate(&mut self, p: &str, _c: &[serde_json::Value], _n: usize, _r: u64) -> Result<jpp_core::effects::GenResult, jpp_core::effects::EffectError> {
        Err(jpp_core::effects::EffectError(format!("重放中不应发 gen：{p}")))
    }
    fn ask(&mut self, _s: &jpp_core::value::State, _q: &jpp_core::value::Question) -> Result<Option<jpp_core::value::Answer>, jpp_core::effects::EffectError> {
        Err(jpp_core::effects::EffectError("重放中不应发 ask".into()))
    }
    fn calls(&self) -> u64 {
        0
    }
}

/// 画像由 `profile_resolve` 解析（B73），装进校准库；账本头的 `profile_hash` 从这里来
/// （`interp/outcome.rs` 读 `calib.profile().hash`）。
fn install_profile(store: &mut CalibStore, 画像: Option<&Resolved>) {
    if let Some(r) = 画像 {
        store.profile = r.profile.clone();
    }
}

fn read_json<T: DeserializeOwned>(path: &Path) -> Result<T, String> {
    let bytes = fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
    serde_json::from_slice(&bytes).map_err(|e| format!("{}: {e}", path.display()))
}

fn write_json<T: Serialize>(path: &Path, value: &T) -> Result<(), String> {
    let mut bytes = serde_json::to_vec_pretty(value).map_err(|e| e.to_string())?;
    bytes.push(b'\n');
    fs::write(path, bytes).map_err(|e| format!("{}: {e}", path.display()))
}

pub fn run_checked(
    program: &Program,
    options: &RunOptions,
    loaded: &jpp_syntax::loader::LoadedProgram,
    画像: Option<Resolved>,
) -> Result<(), String> {
    // 真机分支一定有画像：`profile_resolve::resolve` 解析不到时已报 `E-profile-missing`。
    let 真机画像 = match (&画像, uses_live_backend(options)) {
        (Some(r), true) => Some(r),
        // 依据：B73（地基/附注/2026-09-24-评估①裁定.md §二；21 步 15d-0）
        (None, true) => return Err("E-profile-missing: --backend live 需要能力画像（B73）".into()),
        _ => None,
    };
    // 越界接线：`--calib` 先装目录里的记录，`--fixtures` 的 `calibrations` 再覆盖同名键。
    // **顺序是「夹具优先」**，因为夹具是这一次跑的显式布置，而目录是常备资产；
    // 两边都给同一个键时**谁赢要说得出来**，所以下面会把被覆盖的键报出来。
    let mut 目录记录 = match &options.calib {
        Some(dir) => CalibStore::load(dir).map_err(|e| format!("{}: {e}", dir.display()))?,
        None => CalibStore::new(),
    };
    let (mut client, calibrations, description): (Box<dyn Client>, CalibStore, Option<String>) = match &options.fixtures {
        Some(path) => {
            let fixture: Fixture = read_json(path)?;
            let (client, calibrations) = fixture
                .build()
                .map_err(|e| format!("{}: {e}", path.display()))?;
            (Box::new(client), calibrations, Some(fixture.description))
        }
        None => match options.backend {
            Backend::Fixed => (Box::new(FixedClient::new()), CalibStore::new(), None),
            // 重放不发调用，客户端换成 `ReplayClient`：这里不初始化真实后端，
            // 否则没开 `live` feature 或没有 `~/.typesafe-key` 时，纯离线的重放也会失败
            // （Codex 评审 PR #28）。`--resume` 会发新调用，照常初始化。
            Backend::Live if options.replay.is_some() => (Box::new(FixedClient::new()), CalibStore::new(), None),
            Backend::Live => {
                let model = options.model.as_deref().unwrap_or(DEFAULT_LIVE_MODEL);
                // 依据：B73（地基/附注/2026-09-24-评估①裁定.md §二；21 步 15d-0）
                let r = 真机画像.ok_or("E-profile-missing: --backend live 需要能力画像（B73）")?;
                (live_client(model, &r.profile)?, CalibStore::new(), None)
            }
        },
    };
    // 合并：夹具里的键覆盖目录里的同名键，**并把覆盖了哪些键打出来**
    let mut 被覆盖: Vec<String> = vec![];
    for (k, rec) in &calibrations.records {
        if 目录记录.records.contains_key(k) {
            被覆盖.push(k.clone());
        }
        目录记录.records.insert(k.clone(), rec.clone());
    }
    被覆盖.sort();
    if !被覆盖.is_empty() {
        eprintln!("注意：--fixtures 的 calibrations 覆盖了 --calib 目录里的同名键：{}", 被覆盖.join("、"));
    }
    install_profile(&mut 目录记录, 画像.as_ref());
    // **「这次用了哪份画像」要说得出来**，无条件打印（B73）：固定观察在步 15d 删除
    // 代码兜底之前可以无画像运行，但线与 δ 用的是代码兜底这件事每次都要看得见。
    eprintln!(
        "档案：{}；校准记录：{} 条（{}）",
        match (&画像, &目录记录.profile.hash) {
            (Some(r), Some(h)) => format!("{} (hash {h})", r.path.display()),
            _ => "未加载，线与 δ 用的是代码兜底".to_string(),
        },
        目录记录.records.len(),
        match &options.calib { Some(d) => format!("--calib {}", d.display()), None => "仅来自 --fixtures".into() }
    );
    let mut calibrations = 目录记录;
    // 账本 v2（步 7）：只经 `Ledger::decode` 读；v1 报 E-ledger-archived，末行半写截断并报告
    let mut ledger = match options.replay.as_ref().or(options.resume.as_ref()) {
        Some(path) => {
            let text = fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
            let (l, truncated) = Ledger::decode(&text).map_err(|e| format!("{}: {e}", path.display()))?;
            if let Some(t) = truncated {
                eprintln!("{}: {}", path.display(), t.render());
            }
            l
        }
        None => Ledger::new(),
    };
    ledger.rebuild_index();
    // **只凭账本重放时补回当时的线**：账本记着那一趟 `cut` 实际查到的校准记录。
    // 这次显式给了的记录（`--calib` / `--fixtures`）优先；没给的键才从账本补。
    // 补回的键打出来——出口一致，但线的来源是账本，不是这次的校准目录。
    let mut 补回: Vec<String> = vec![];
    for (k, v) in &ledger.calib_used {
        if calibrations.records.contains_key(k) {
            continue;
        }
        let rec: jpp_core::effects::CalibRecord = serde_json::from_value(v["record"].clone())
            .map_err(|e| format!("账本里的校准记录 {k:?} 读不成：{e}"))?;
        calibrations.records.insert(k.clone(), rec);
        补回.push(k.replace('\u{1f}', ":"));
    }
    if !补回.is_empty() {
        eprintln!("从账本补回 {} 条校准记录（本次没有另给）：{}", 补回.len(), 补回.join("、"));
    }
    // **重放要用记录时那个 model_id**，不是硬写的常量——见 `ReplayClient` 的注释。
    // 旧账本没有 `header` 时退回 `"fixed-0"`，与接线前逐字节相同。
    let replay_model_id = ledger
        .header
        .as_ref()
        .map(|h| h.model_id().to_string())
        .unwrap_or_else(|| "fixed-0".to_string());
    let mut evidence: Vec<(String, jpp_core::effects::Sample)> = vec![];
    let result = if options.replay.is_some() {
        let mut replay_client = ReplayClient(replay_model_id.clone());
        runner::execute(program, &mut replay_client, &calibrations, &mut ledger, true, &mut evidence)
    } else {
        runner::execute(program, client.as_mut(), &calibrations, &mut ledger, false, &mut evidence)
    };
    // **出料那一半**：把这一趟判出来的读数折进记录并落盘。
    // **只有这样那条环才闭得上**——`--calib` 是入料，J-03 决定了程序自己写不了线。
    //
    // **折进去的是无标注观察**：`absorb` 不让记录上岗（`n` 不动、冷记录推到「待真值」），
    // **上岗仍要 `commission`，而认证是校准过程不是程序行为。**
    if let Some(dir) = &options.calib_out {
        let mut 出 = calibrations.clone();
        for (k, sm) in &evidence {
            出.absorb(k, sm.clone()).map_err(|e| format!("折证据进 {k} 失败：{e}"))?;
        }
        // **停岗候选**（B25）：本趟漂移信号自动标出的键写成「停岗候选」，正式停岗由人确认
        // （`jpp calib-confirm <目录> <键> --suspend | --keep`）。只动上岗记录。
        if let Ok(report) = &result {
            for k in report["suspend_candidates"].as_array().into_iter().flatten().filter_map(|v| v.as_str()) {
                if let Some(r) = 出.records.get_mut(k) {
                    if r.status == "上岗" {
                        r.status = "停岗候选".into();
                        eprintln!("停岗候选：{}（漂移信号超线；用 jpp calib-confirm 确认停岗或保留）", k.replace('\u{1f}', ":"));
                    }
                }
            }
        }
        出.save(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        eprintln!("校准记录已写回 {}（{} 条记录，本趟折进 {} 条观察）", dir.display(), 出.records.len(), evidence.len());
    }
    // Preserve any completed effects even when execution ends in a runtime error.
    if let Some(path) = &options.ledger_out {
        fs::write(path, ledger.encode()).map_err(|e| format!("{}: {e}", path.display()))?;
    }
    // **说清楚这一趟实际用了哪个后端**：replay 从不碰 client（哪怕 `--backend live`
    // 也构造了一个，只是没被 `runner::execute` 用到），真机与固定观察之外没有第三档。
    let (mode_label, backend_label): (&str, String) = if options.replay.is_some() {
        ("replay; no model API requests", replay_model_id)
    } else {
        match options.backend {
            Backend::Live => ("live model backend (~/.typesafe-key)", client.model_id()),
            Backend::Fixed => ("fixed observations; no model API requests", client.model_id()),
        }
    };
    // 检查诊断与运行期错误走渲染层（步 9a）：同码同址折叠，`--json` 时每条一行 JSON
    let mut report = result.map_err(|error| {
        let items = match error {
            jpp_core::Error::Runtime(e) => vec![crate::diag_json::from_runtime(&e)],
            jpp_core::Error::Check(report) => report.diagnostics.iter().map(crate::diag_json::from_check).collect(),
        };
        crate::diag_json::render_all(loaded, items).join("\n")
    })?;
    report["fixture_description"] = serde_json::json!(description);
    report["replay"] = serde_json::json!(options.replay.is_some());
    report["resumed"] = serde_json::json!(options.resume.is_some());
    report["mode"] = serde_json::json!(mode_label);
    report["backend"] = serde_json::json!(backend_label);
    if let Some(r) = 真机画像.filter(|r| r.profile.price_per_input_token.is_none()) {
        mark_cost_unknown(&mut report, &r.path);
    }
    // `run --json`：运行期告警（`trace.warnings` 里带编号的行）折叠后以 JSON Lines 写到 stderr；报告不动
    if crate::diag_json::json_mode() {
        let items = report["trace"]["warnings"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|w| w.as_str().and_then(crate::diag_json::Item::from_warning))
            .collect();
        for line in crate::diag_json::render_all(loaded, items) {
            eprintln!("{line}");
        }
    }
    if let Some(path) = &options.output {
        write_json(path, &report)?;
        println!(
            "{}: {} (new model calls: {}); report: {}",
            options.source.display(),
            report["status"].as_str().unwrap_or("unknown"),
            report["cost"]["calls"],
            path.display()
        );
    } else {
        println!(
            "{}",
            serde_json::to_string_pretty(&report).map_err(|e| e.to_string())?
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use jpp_core::effects::{EffectError, JevClient};
    use serde_json::json;
    use std::{cell::RefCell, rc::Rc};

    /// 与 `crates/jpp-cli/tests/wiring.rs` 的 `calib目录让unsure_bound不再恒等于n` 同一份
    /// 程序骨架：单道 `test` 题喂进 `unsure_bound`，产出是一个数，便于比较真实跑与重放。
    fn 程序() -> Program {
        jpp_core::lower(
            &jpp_syntax::parse("budget {calls: 4, cost: 0};\nlet r = judge(state(mat(\"材料\")), test(\"行吗\",\"k\"));\nunsure_bound([r])\n")
                .expect("解析"),
        )
        .expect("lower")
    }

    /// **选 live 后端走 transport**：`runner::execute` 接的是 `JevClient`（用 `with_transport`
    /// 代替真网络），真实调用要经过 transport 且记进 `cost.calls`；随后用同一份账本重放，
    /// 新增调用必须是 0——这正是 B1 验收要的「重放新增调用为 0」。
    #[test]
    fn live后端走transport_重放新增调用为零() {
        let program = 程序();
        let calls = Rc::new(RefCell::new(0));
        let c2 = calls.clone();
        let mut client = JevClient::with_transport(
            "jev-1.13.0",
            Box::new(move |_body| {
                *c2.borrow_mut() += 1;
                Ok(json!({"answers": {"q0": {"noul": 0.8}}}))
            }),
        );
        let calib = CalibStore::new();
        let mut ledger = Ledger::new();
        ledger.rebuild_index();
        let mut evidence = vec![];
        let report = runner::execute(&program, &mut client, &calib, &mut ledger, false, &mut evidence)
            .expect("真机路径跑得完");
        assert_eq!(*calls.borrow(), 1, "接线要真的经过 transport，不是绕过它");
        assert_eq!(report["cost"]["calls"], json!(1), "报告要记 1 次真实调用");
        assert_eq!(report["cost"]["replayed"], json!(0), "这一趟没有从账本重放");

        ledger.rebuild_index();
        // **重放要用记录时那个 model_id**（`ReplayClient` 头顶的注释）：直接套
        // `NoCallClient`（硬写 `"fixed-0"`）在这里会撞上与生产代码同一个键不匹配的坑。
        let mut replay_client = ReplayClient("jev-1.13.0".to_string());
        let mut evidence2 = vec![];
        let replay_report =
            runner::execute(&program, &mut replay_client, &calib, &mut ledger, true, &mut evidence2)
                .expect("用刚写的账本重放跑得完");
        assert_eq!(replay_report["cost"]["calls"], json!(0), "重放不应产生任何新调用");
        assert_eq!(replay_report["value"], report["value"], "重放结果要与真实运行一致");
    }

    fn 发行画像选项() -> RunOptions {
        RunOptions {
            source: "p.jpp".into(),
            fixtures: None,
            output: None,
            ledger_out: None,
            replay: None,
            resume: None,
            profile: None,
            calib: None,
            calib_out: None,
            backend: Backend::Live,
            model: None,
            profiles_dir: Some(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../profiles")),
        }
    }

    /// B73：真机路径按 `--profiles-dir/<model>.json` 解析发行画像，装进校准库，
    /// 账本头 `profile_hash` 非空（= 发行画像的哈希），费用 = input tokens × 画像价格。
    #[test]
    fn 真机画像进账本头_价格从画像来() {
        let program = 程序();
        let 画像 = crate::profile_resolve::resolve(&发行画像选项()).expect("解析").expect("真机必有画像");
        let price = 画像.profile.price_per_input_token.expect("发行画像带价格");
        let mut calib = CalibStore::new();
        install_profile(&mut calib, Some(&画像));
        let mut client = JevClient::with_transport(
            "jev-1.13.0",
            Box::new(|_body| Ok(json!({"answers": {"q0": {"noul": 0.8}}, "usage": {"input_tokens": 1000}}))),
        )
        .with_price(画像.profile.price_per_input_token);
        let mut ledger = Ledger::new();
        ledger.rebuild_index();
        let mut evidence = vec![];
        let report = runner::execute(&program, &mut client, &calib, &mut ledger, false, &mut evidence).expect("跑得完");
        let h = ledger.header.as_ref().expect("有账本头");
        assert_eq!(h.compared.profile_hash.as_deref(), Some("56817be03183293c"), "账本头记发行画像的哈希");
        assert_eq!(report["cost"]["tokens"], json!(1000));
        assert_eq!(report["cost"]["usd"].as_f64(), Some(1000.0 * price), "费用 = tokens × 画像价格");
    }

    /// B73：画像没有价格 → 报告费用记 `Unknown`，`trace.warnings` 有 `W-cost-unknown`。
    #[test]
    fn 画像无价格时费用记unknown() {
        let mut report = json!({"cost": {"usd": 0.0}, "trace": {"warnings": []}});
        mark_cost_unknown(&mut report, std::path::Path::new("p.json"));
        assert_eq!(report["cost"]["usd"], json!("Unknown"));
        // 依据：B73（地基/附注/2026-09-24-评估①裁定.md §二；21 步 15d-0）
        assert!(report["trace"]["warnings"][0].as_str().unwrap().starts_with("W-cost-unknown"));
    }

    /// **transport 失败时报错要说得清楚**：错误原因要冒泡到 `jpp_core::Error::Runtime`，
    /// 不能被吞成一句通用的失败提示。
    #[test]
    fn transport失败时报错清楚() {
        let program = 程序();
        let mut client = JevClient::with_transport(
            "jev-1.13.0",
            Box::new(|_body| Err(EffectError("连接超时".into()))),
        );
        let calib = CalibStore::new();
        let mut ledger = Ledger::new();
        ledger.rebuild_index();
        let mut evidence = vec![];
        let err = runner::execute(&program, &mut client, &calib, &mut ledger, false, &mut evidence)
            .expect_err("transport 出错应当冒泡成 Err，不能被吞掉");
        let jpp_core::Error::Runtime(e) = err else {
            panic!("应为运行期错误，不是静态检查错误");
        };
        assert!(e.message.contains("连接超时"), "错误要带上 transport 的原因：{}", e.message);
    }
}
