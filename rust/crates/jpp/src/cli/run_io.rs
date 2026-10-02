//! File and client setup after the caller has checked the shared core program.
use crate::{
    fixture::Fixture,
    options::{Backend, RunOptions},
    profile_resolve::{Resolved, uses_live_backend},
    runner,
};
use jpp::{
    Program,
    backends::BackendPorts,
    effects::{CalibStore, FixedPorts},
    ledger::Ledger,
};
use jpp_effects::Ports;
use serde::{Serialize, de::DeserializeOwned};
use std::{
    fs,
    path::{Path, PathBuf},
};

/// 这一趟的观察后端（步 15b 起是端口表）：固定观察或注册表里的后端（步 15g-0）。
enum Observer {
    Fixed(Box<FixedPorts>),
    Backend(Box<dyn BackendPorts>),
}

impl Observer {
    fn model_id(&self) -> String {
        match self {
            Observer::Fixed(_) => jpp_effects::builtin_ports::FIXED_MODEL.to_string(),
            Observer::Backend(p) => p.model_id(),
        }
    }
    fn ports(&mut self) -> Ports<'_> {
        match self {
            Observer::Fixed(p) => p.ports(),
            Observer::Backend(p) => p.ports(),
        }
    }
}

// 真机客户端的构造与传输超时步 15g-0 起在注册表条目里（`jpp::backends::jev::SPEC.build`）。

/// 装载的题库题式里有非在岗的（退役、被取代、提出、诊断通过）：报 `W-bank-status`（stderr 与 `trace.warnings`
/// 各一条）。只报，不拦、不改结果（意图汇编 11a：不加默认限制）。停岗候选按 B25 仍在岗，不报。
fn mark_bank_status(
    report: &mut serde_json::Value,
    non_service: &[(String, jpp::store::bank::Status)],
) {
    for (slug, st) in non_service {
        let w = format!(
            // 依据：B48（题库生命周期）、意图汇编 11a；补缺 3（`地基/过程记录/工程-步27.md`）
            "W-bank-status: 题库题式 {slug} 现在是「{}」，`bank.json` 已判它不在岗，程序仍 import 了 lib/bank/{slug}.jpp（线与题式照旧供给，不拦）",
            st.as_str()
        );
        eprintln!("warning: {w}");
        if let Some(ws) = report["trace"]["warnings"].as_array_mut() {
            ws.push(serde_json::json!(w));
        }
    }
}

/// 画像没有 `transport.timeout_s`（J-15 形式：字段未测报 `W-untested`）：真机请求不设超时，照跑。
fn mark_timeout_untested(report: &mut serde_json::Value, profile_path: &Path) {
    let w = format!(
        // 依据：地基/过程记录/工程-传输超时.md（主会话 2026-09-24 确认：缺字段照跑、不编秒数）
        "W-untested: transport.timeout_s — 画像 {} 没有给单次请求超时，本次真机请求没有超时上限（挂起的请求会一直等）",
        profile_path.display()
    );
    eprintln!("warning: {w}");
    if let Some(ws) = report["trace"]["warnings"].as_array_mut() {
        ws.push(serde_json::json!(w));
    }
}

/// 真机运行的画像没有价格时（B73；`20` §3.9 `cost` 未测行）：报告的费用记 `Unknown`，
/// 报 `W-cost-unknown`（stderr 与 `trace.warnings` 各一条）。预算里这部分按 0 累计（B42 不拒）。
fn mark_cost_unknown(report: &mut serde_json::Value, profile_path: &Path) {
    let w = format!(
        // 依据：B73（地基/附注/2026-09-24-评估①裁定.md §二；21 步 15d-0）
        "W-cost-unknown: 画像 {} 没有 cost.price_usd_per_input_token，本次真机费用记为 Unknown，预算的费用上限没有核到",
        profile_path.display()
    );
    report["cost"]["usd"] = serde_json::json!("Unknown");
    if let Some(ws) = report["trace"]["warnings"].as_array_mut() {
        // 步 22：计划期已报过 `W-cost-unknown`（`jpp-plan` 的 `plan` pass）就不再追加第二条，只打到 stderr
        let 已报 = ws
            .iter()
            .find_map(|x| x.as_str().filter(|s| s.starts_with("W-cost-unknown")))
            .map(str::to_string);
        match 已报 {
            Some(前) => eprintln!("warning: {前}"),
            None => {
                eprintln!("warning: {w}");
                ws.push(serde_json::json!(w));
            }
        }
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

/// `jpp ledger-migrate <v2> <v3>`（步 18a，B124 Q3）：v2 账本改写为 v3，写到另一个文件（可与输入同名）。
pub fn ledger_migrate(from: &Path, to: &Path) -> Result<(), String> {
    let text = fs::read_to_string(from).map_err(|e| format!("{}: {e}", from.display()))?;
    let m = jpp::store::migrations::ledger_v2::migrate(&text)
        .map_err(|e| format!("{}: {e}", from.display()))?;
    fs::write(to, &m.text).map_err(|e| format!("{}: {e}", to.display()))?;
    if let Some(b) = m.truncated_bytes {
        eprintln!("{}: v2 末行半写（{b} 字节），迁移时已截掉", from.display());
    }
    eprintln!(
        "{} → {}：账本 v2 已迁移为 v3（命中校准记录 {} 条改为 CalibUsed 条目）",
        from.display(),
        to.display(),
        m.calib_used_keys
    );
    Ok(())
}

/// 宿主给的追踪上下文（C-2）：`--trace-parent` 是调用者的上下文，本段由它与标签推导；`--trace-seed` 起一条新链，
/// 本段是根段；标签缺省是程序标识。都不给返回 `None`，由 `Session` 按账本推导（首跑的根、续跑的新段）。
fn 宿主追踪(
    program: &Program,
    options: &RunOptions,
) -> Result<Option<jpp::ledger::TraceCtx>, String> {
    let 标签 = || {
        options
            .trace_label
            .clone()
            .unwrap_or_else(|| jpp::Session::program_id(program))
    };
    if let Some(tp) = &options.trace_parent {
        return Ok(Some(
            jpp::ledger::TraceCtx::from_traceparent(tp)?.enter(&标签()),
        ));
    }
    if let Some(seed) = &options.trace_seed {
        return Ok(Some(jpp::ledger::TraceCtx::start(seed, &标签())));
    }
    Ok(None)
}

/// `jpp ledger-tree <账本>...`（C-2）：把几份账本按追踪编号拼成调用树，打印文本或（`--json`）JSON。
/// 账本按 `read_any` 读（v2 在内存里迁移，末行半写截断并报告）；来源名是命令行上给的路径。
pub fn ledger_tree(paths: &[PathBuf]) -> Result<(), String> {
    let mut ledgers: Vec<(String, Ledger)> = vec![];
    for path in paths {
        let text = fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
        let (l, truncated, _) = jpp::store::migrations::ledger_v2::read_any(&text)
            .map_err(|e| format!("{}: {e}", path.display()))?;
        if let Some(t) = truncated {
            eprintln!("{}: {}", path.display(), t.render());
        }
        ledgers.push((path.display().to_string(), l));
    }
    let refs: Vec<(String, &Ledger)> = ledgers.iter().map(|(n, l)| (n.clone(), l)).collect();
    let tree = jpp::ledger::TraceTree::build(&refs);
    if super::diag_json::json_mode() {
        println!(
            "{}",
            serde_json::to_string(&tree).map_err(|e| e.to_string())?
        );
    } else {
        print!("{}", tree.render());
    }
    Ok(())
}

/// 有撤不回动作而没给 `--ledger-out` 时的缺省账本路径（意图汇编 11a；B187 批 9 第 9 格）：源文件同目录的
/// `<源文件名去后缀>.ledger.jsonl`；`--resume` 读的正是这个文件时改写 `<源文件名>.resumed.ledger.jsonl`，
/// 不覆盖续接的来源。
fn default_ledger_path(source: &Path, resume: Option<&Path>) -> std::path::PathBuf {
    let stem = source
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "program".into());
    let path = source.with_file_name(format!("{stem}.ledger.jsonl"));
    let 同一个 = resume.is_some_and(|r| {
        r == path
            || matches!(
                (std::fs::canonicalize(r), std::fs::canonicalize(&path)),
                (Ok(a), Ok(b)) if a == b
            )
    });
    if 同一个 {
        source.with_file_name(format!("{stem}.resumed.ledger.jsonl"))
    } else {
        path
    }
}

/// 读 `--input` 文件（步 14b-0）：合法 JSON、整数在 J++ Int 范围内（与 `read_json` 同一条校验），
/// 以名字 `input` 作一条值条目交给程序（B105，步 14b）；`trusted` 来自 CLI `--input-trusted`
/// （步 14b-1，B108），缺省仍是不可信（`EntryValue::new` 的缺省）。依据：规划建议 6（21 步 14b-0）；
/// B105 / B108
pub fn read_host_input(path: &Path, trusted: bool) -> Result<jpp::EntryArgs, String> {
    let value: serde_json::Value = read_json(path)?;
    jpp::actions::validate_numbers(&value).map_err(|e| format!("{}: {e}", path.display()))?;
    let mut v = jpp::EntryValue::new("input", value);
    if trusted {
        v = v.with_taint(jpp::Taint::Trusted);
    }
    Ok(jpp::EntryArgs {
        values: vec![v],
        ..Default::default()
    })
}

/// 读 `--mat <name>=<file>`（B0472）：`.json` 按 JSON 读（整数范围与 `--input` 同一条校验），其余按 UTF-8 文本读、
/// 内容是一个字符串；缺省不可信（`EntryMat::untrusted`），运行入口盖 `origin = ["input"]`（B105-1）
pub fn read_entry_mat(name: &str, path: &Path) -> Result<jpp::EntryMat, String> {
    let content = if path.extension().is_some_and(|e| e == "json") {
        let v: serde_json::Value = read_json(path)?;
        jpp::actions::validate_numbers(&v).map_err(|e| format!("{}: {e}", path.display()))?;
        v
    } else {
        serde_json::Value::String(
            std::fs::read_to_string(path)
                .map_err(|e| format!("--mat {name}: {}: {e}", path.display()))?,
        )
    };
    Ok(jpp::EntryMat::untrusted(name, content))
}

pub fn run_checked(
    program: &Program,
    options: &RunOptions,
    loaded: &jpp_syntax::loader::LoadedProgram,
    画像: Option<Resolved>,
    输入: jpp::EntryArgs,
    explain: bool,
) -> Result<(), String> {
    // 步 18b（B55；主会话 2026-09-25 裁定）：有不可逆 `do` 的程序，首跑与续接的账本要落盘，
    // 否则写前意向不落盘，续接会重做不可逆动作；只凭账本重放不要求。
    // 意图汇编 11a（2026-09-26）：账本是记录，不是防御。默认不停下，没给 `--ledger-out` 就写到默认路径并提示；
    // 开放行把关（`--guard`）时照旧 `E-ledger-required`，执行前（任何效应之前）报错。
    let 默认账本 = if options.replay.is_none()
        && options.ledger_out.is_none()
        && let Some(名) = runner::irreversible_action_in(program)
    {
        if program.entry.guard {
            // 依据：B55（20 v2 附录 B55 条）；主会话 2026-09-25 对步 18b 的裁定
            return Err(format!(
                "E-ledger-required: 程序里有不可逆动作 do「{名}」，要给 --ledger-out <账本文件>：不可逆动作执行前的写前意向要落盘（B55），否则中断后续接会重做它。只凭账本重放（--replay）不要求（开了 --guard；不开时账本自动写到默认路径）"
            ));
        }
        let path = default_ledger_path(&options.source, options.resume.as_deref());
        // B187（批 9 第 9 格）：首行打印缺省路径，报告顶层另记 `ledger_path`
        eprintln!(
            "账本：{}（程序里有撤不回的动作 do「{名}」，没给 --ledger-out，写到缺省路径；续接用 --resume 这个文件）",
            path.display()
        );
        Some(path)
    } else {
        None
    };
    let ledger_out = options.ledger_out.clone().or(默认账本);
    // 真机分支一定有画像：`profile_resolve::resolve` 解析不到时已报 `E-profile-missing`。
    let 真机画像 = match (&画像, uses_live_backend(options)) {
        (Some(r), true) => Some(r),
        // 依据：B73（地基/附注/2026-09-24-评估①裁定.md §二；21 步 15d-0）
        (None, true) => {
            return Err(format!(
                "E-profile-missing: --backend {} 需要能力画像（B73）",
                options.backend.spec().map_or("", |s| s.name)
            ));
        }
        _ => None,
    };
    // 越界接线：`--calib` 先装目录里的记录，`--fixtures` 的 `calibrations` 再覆盖同名键。
    // **顺序是「夹具优先」**，因为夹具是这一次跑的显式布置，而目录是常备资产；
    // 两边都给同一个键时**谁赢要说得出来**，所以下面会把被覆盖的键报出来。
    let mut 目录记录 = match &options.calib {
        // 步 20c：装载即按证书重跑认证（B117，与画像无关）；降级与改写报到 stderr
        Some(dir) => {
            let s = jpp::store::calib::open(dir)?;
            for line in &s.load_report {
                eprintln!("{line}");
            }
            s
        }
        None => CalibStore::new(),
    };
    let (mut client, calibrations, description): (Observer, CalibStore, Option<String>) =
        match &options.fixtures {
            Some(path) => {
                let fixture: Fixture = read_json(path)?;
                let (client, calibrations) = fixture
                    .build()
                    .map_err(|e| format!("{}: {e}", path.display()))?;
                (
                    Observer::Fixed(Box::new(client)),
                    calibrations,
                    Some(fixture.description),
                )
            }
            None => match options.backend {
                Backend::Fixed => (Observer::Fixed(Box::default()), CalibStore::new(), None),
                // 重放不发调用，端口换成 `ReplayPorts`：这里不初始化真实后端，
                // 否则没开 `live` feature 或没有 `~/.typesafe-key` 时，纯离线的重放也会失败
                // （Codex 评审 PR #28）。`--resume` 会发新调用，照常初始化。
                Backend::Registered(_) if options.replay.is_some() => {
                    (Observer::Fixed(Box::default()), CalibStore::new(), None)
                }
                Backend::Registered(spec) => {
                    let model = options.model.as_deref().unwrap_or(spec.default_model);
                    // 依据：B73（地基/附注/2026-09-24-评估①裁定.md §二；21 步 15d-0）
                    let r = 真机画像.ok_or_else(|| {
                        format!(
                            "E-profile-missing: --backend {} 需要能力画像（B73）",
                            spec.name
                        )
                    })?;
                    (
                        Observer::Backend((spec.build)(model, &r.profile)?),
                        CalibStore::new(),
                        None,
                    )
                }
            },
        };
    // 合并：夹具里的键覆盖目录里的同名键，**并把覆盖了哪些键打出来**（策略在 `jpp::store::calib`，步 18-0）
    let 被覆盖 = jpp::store::calib::merge_fixture(&mut 目录记录, &calibrations);
    if !被覆盖.is_empty() {
        eprintln!(
            "注意：--fixtures 的 calibrations 覆盖了 --calib 目录里的同名键：{}",
            被覆盖.join("、")
        );
    }
    install_profile(&mut 目录记录, 画像.as_ref());
    // **「这次用了哪份画像」要说得出来**，无条件打印（B73）。步 15d-2 起没有代码兜底：未加载画像时
    // 画像字段全部未测（窗口不核、报 W-window-untested），线与 δ 只从校准记录取。
    eprintln!(
        "档案：{}；校准记录：{} 条（{}）",
        match (&画像, &目录记录.profile.hash) {
            (Some(r), Some(h)) => format!("{} (hash {h})", r.path.display()),
            // 依据：21 步 15d-2（删 Profile::default，δ 只从校准记录取）
            _ => "未加载，画像字段全部未测（线与 δ 只从校准记录取）".to_string(),
        },
        目录记录.records.len(),
        match &options.calib {
            Some(d) => format!("--calib {}", d.display()),
            None => "仅来自 --fixtures".into(),
        }
    );
    // 生成器（步 15h-1，B149）：画像按 B73 同一口径解析，说清楚这一趟用了哪份生成器画像
    let 生成器 = crate::profile_resolve::resolve_gen(options)?;
    if let Some(g) = &生成器 {
        eprintln!(
            "生成器：{} {}；画像 {} (hash {})",
            g.spec.name,
            g.model,
            g.path.display(),
            g.profile.hash
        );
    }
    let mut gen_port = 生成器
        .as_ref()
        .map(|g| (g.spec.build)(&g.model, &g.profile));
    // 跨运行缓存（步 19，B151 两段式；取代 15h-2 的 `--gen-cache`）：运行前从目录里的账本建索引。
    // 只凭账本的审计重放不用缓存（`Session` 在重放时不交给运行时）。
    let 缓存 = match &options.cache {
        Some(dir) => Some(load_cache_dir(dir)?),
        None => None,
    };
    let mut calibrations = 目录记录;
    // 账本 v3（步 18a）：v3 经 `Ledger::decode` 读；v2 在内存里迁移后读（B124 Q3，文件不改写）；
    // v1 报 E-ledger-archived，末行半写截断并报告
    let mut ledger = match options.replay.as_ref().or(options.resume.as_ref()) {
        Some(path) => {
            let text = fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
            let (l, truncated, note) = jpp::store::migrations::ledger_v2::read_any(&text)
                .map_err(|e| format!("{}: {e}", path.display()))?;
            if let Some(n) = note {
                eprintln!("{}: {n}", path.display());
            }
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
    let 补回 = jpp::Session::restore_calib(&mut calibrations, &ledger)?;
    if !补回.is_empty() {
        eprintln!(
            "从账本补回 {} 条校准记录（本次没有另给）：{}",
            补回.len(),
            补回.join("、")
        );
    }
    // **重放要用记录时那个 model_id**，不是硬写的常量：账本键含 `model_id`，运行时用替它跑重放的
    // 端口表里判断实例的模型去算查表键。真机记的账本 `model_id` 是 `--model`（例如 `jev-1.13.0`），
    // 硬写 `"fixed-0"` 会让查表键与记录时的对不上，表现为「重放中不应发调用」。值从账本自己的
    // `header.model_id` 读，不必用户在重放时重新声明 `--backend`/`--model`；
    // 旧账本没有 `header` 时退回 `"fixed-0"`，与接线前逐字节相同。（步 15c 前这段写在 `ReplayClient` 上）
    let replay_model_id = jpp::Session::replay_model_id(&ledger);
    let 账本头lib = ledger
        .header
        .as_ref()
        .and_then(|h| h.compared.lib_version.clone());
    let mut evidence: Vec<(String, jpp::effects::Sample)> = vec![];
    // 步 18b（B55）：`--ledger-out` 的账本逐行落盘——头在运行入口定稿时整份原子写出（续接写的是新文件：
    // 新头、旧条目按新链重串），不可逆 `do` 的意向与结果即刻落盘，其余条目每层末落盘。不给就只在内存里。
    let mut 文件 = ledger_out.as_ref().map(|path| {
        let dir = match path.parent() {
            Some(d) if !d.as_os_str().is_empty() => d.to_path_buf(),
            _ => std::path::PathBuf::from("."),
        };
        let key = path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();
        jpp::store::LedgerFile::new(
            jpp::store::DirBlob::new(dir),
            &key,
            std::mem::take(&mut ledger),
        )
    });
    let port: &mut dyn jpp::LedgerPort = match 文件.as_mut() {
        Some(f) => f,
        None => &mut ledger,
    };
    // 标准库与题库版本（步 27，B48）：由本次装载的源文件算，首跑、续接、重放同一口径
    // 伴随题序言（B0492 S5，主控 2026-09-30 路 A）：发法不为关时 `lib/unsure.jpp` 随程序一起算进 `lib_version`
    // 审计重放带不带伴随题只看账本（主控复核 2026-09-30），`lib_version` 也按账本头：首跑没带序言（关伴随题）的账本，
    // 重放算不带序言的那个版本，不报假的头不一致（复查 2026-09-30 小项 2）
    let 序言路径 = std::path::PathBuf::from("lib/unsure.jpp");
    let 算版本 = |带序言: bool| {
        jpp::store::bank::versions_of(
            loaded
                .sources
                .iter()
                .map(|f| (f.path.as_path(), f.text.as_bytes()))
                .chain(
                    带序言.then_some((序言路径.as_path(), jpp::session::UNSURE_PRELUDE.as_bytes())),
                ),
        )
    };
    // Z0398 起序言一律装载（通用类别表，过程记录 5.19），首跑总带；重放按账本头挑（旧账本可能是不带序言录的）
    let (lib_version, bank_version) = 选版本(
        options.replay.is_some(),
        账本头lib.as_deref(),
        算版本(false),
        || 算版本(true),
    );
    // `--explain`（Z0190 后一半）：运行前一刻由 `execute_with` 调；文本进 stderr（stdout 留给报告），
    // `--json` 时改为返回一个对象并入报告的 `explain` 键
    // C-3 G1：预算行按规划实际用的程序（带上游余额时是收紧后的那份）
    let explain_cb = |plan: &jpp::interp::Plan,
                      ctx: &jpp::interp::PlanCtx,
                      计划用: &jpp::Program,
                      confirm: Option<&jpp_plan::explain::ConfirmView>| {
        if crate::diag_json::json_mode() {
            Some(runner::explain_json_with(
                plan, ctx, 计划用, loaded, confirm,
            ))
        } else {
            eprint!(
                "{}",
                runner::explain_text_with(plan, ctx, 计划用, loaded, confirm)
            );
            None
        }
    };
    // 费用确认（Z0236，`11` §5.5）：阈值取 `--confirm-above`，没给取默认；判定在 `runner::execute_with` 里
    let confirm_args = runner::ConfirmArgs {
        confirmed: options.confirm,
        threshold_usd: options
            .confirm_above
            .unwrap_or(crate::options::CONFIRM_DEFAULT_USD),
        threshold_is_default: options.confirm_above.is_none(),
    };
    let explain_ref: Option<&runner::ExplainFn<'_>> =
        if explain { Some(&explain_cb) } else { None };
    let 追踪 = 宿主追踪(program, options)?;
    let result = if options.replay.is_some() {
        let replay_ports = jpp_effects::ReplayPorts::ports(&replay_model_id);
        runner::execute_with(
            program,
            replay_ports,
            &calibrations,
            port,
            true,
            &mut evidence,
            &输入,
            runner::CacheArgs {
                lib_version: Some(lib_version.clone()),
                bank_version: bank_version.clone(),
                explain: explain_ref,
                trace: 追踪.clone(),
                confirm: Some(confirm_args),
                // C-3：重放的交回余额取账本头与 `Spent`，与首跑逐字节相同
                carry_out: options.carry_out.clone(),
                ..Default::default()
            },
            None,
        )
    } else {
        let mut ports = client.ports();
        // 步 15h-1（B149）：给了 `--gen-model` 就把占位的 `gen` 实例换成生成器端口（非阻塞 submit/poll）
        if let Some(g) = gen_port.as_mut() {
            ports.replace(Box::new(g.as_mut()));
        }
        runner::execute_with(
            program,
            ports,
            &calibrations,
            port,
            false,
            &mut evidence,
            &输入,
            runner::CacheArgs {
                cache: 缓存.as_ref().map(|c| c as &dyn jpp::effects::CacheLookup),
                // 生成器身份进账本头（步 19）：给了 `--gen-model` 才有
                gen_model: 生成器.as_ref().map(|g| g.model.clone()),
                gen_profile_hash: 生成器.as_ref().map(|g| g.profile.hash.clone()),
                lib_version: Some(lib_version.clone()),
                bank_version: bank_version.clone(),
                trace: 追踪.clone(),
                // 步 22（B42）：判断器的实际单价按后端给——真机为画像价格（没有价格为未测，计划期报
                // `W-cost-unknown`），固定观察为 0（画像带价格也不算，固定观察不付费）
                judge_price: match 真机画像 {
                    Some(r) => match r.profile.price_per_input_token() {
                        Some(p) => jpp::interp::JudgePrice::Known(p),
                        None => jpp::interp::JudgePrice::Untested,
                    },
                    None => jpp::interp::JudgePrice::Known(0.0),
                },
                explain: explain_ref,
                confirm: Some(confirm_args),
                // C-3：上游余额进出（重放不给，重放取账本头）
                carry_in: options.carry_in.clone(),
                carry_out: options.carry_out.clone(),
                carry_reauthorize: options.carry_reauthorize,
                companions: options.companions,
                // B0472：`--mat-store` 装文件料库（重放分支不给：变换结果取自账本）
                mat_store: options.mat_store.clone(),
                envs: options.envs.clone(),
                cells: options.cells,
                cells_stats: options.cells_stats,
            },
            // 程序文件所在目录（现场稳定性三修 (2)）：`read_json` 与 `import` 一样先按它找
            options.source.parent(),
        )
    };
    // Preserve any completed effects even when execution ends in a runtime error.
    if let (Some(f), Some(path)) = (文件, &ledger_out) {
        f.finish().map_err(|e| format!("{}: {e}", path.display()))?;
    }
    // **出料那一半**：把这一趟判出来的读数折进记录并落盘。
    // **只有这样那条环才闭得上**——`--calib` 是入料，J-03 决定了程序自己写不了线。
    //
    // **折进去的是无标注观察**：`absorb` 不让记录上岗（`n` 不动、冷记录推到「待真值」），
    // **上岗仍要 `commission`，而认证是校准过程不是程序行为。**
    if let Some(dir) = &options.calib_out {
        // **停岗候选**（B25）：本趟漂移信号自动标出的键写成「停岗候选」，正式停岗由人确认
        // （`jpp calib-confirm <目录> <键> --suspend | --keep`）。只动上岗记录。折证据与标候选
        // 步 14a 起在 `jpp-calib`（`Session::feed_calib`），这里只传数据、打报文、落盘。
        let 候选: Vec<&str> = match &result {
            Ok(report) => report["suspend_candidates"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|v| v.as_str())
                .collect(),
            Err(_) => vec![],
        };
        let (出, 标出) = jpp::Session::feed_calib(&calibrations, &evidence, &候选)?;
        for k in &标出 {
            eprintln!(
                "停岗候选：{}（漂移信号超线；用 jpp calib-confirm 确认停岗或保留）",
                k.replace('\u{1f}', ":")
            );
        }
        jpp::store::calib::save(&出, dir)?;
        eprintln!(
            "校准记录已写回 {}（{} 条记录，本趟折进 {} 条观察）",
            dir.display(),
            出.records.len(),
            evidence.len()
        );
    }
    // **说清楚这一趟实际用了哪个后端**：replay 从不碰 client（哪怕 `--backend live`
    // 也构造了一个，只是没被 `runner::execute` 用到），真机与固定观察之外没有第三档。
    let (mode_label, backend_label): (&str, String) = if options.replay.is_some() {
        ("replay; no model API requests", replay_model_id)
    } else {
        match options.backend {
            Backend::Registered(spec) => (spec.mode_label, client.model_id()),
            Backend::Fixed => (
                "fixed observations; no model API requests",
                client.model_id(),
            ),
        }
    };
    // 检查诊断与运行期错误走渲染层（步 9a）：同码同址折叠，`--json` 时每条一行 JSON
    let mut report = result.map_err(|error| {
        let items = match error {
            jpp::Error::Runtime(e) => vec![crate::diag_json::from_runtime(&e)],
            jpp::Error::Check(report) => report
                .diagnostics
                .iter()
                .map(crate::diag_json::from_check)
                .collect(),
        };
        crate::diag_json::render_all(loaded, items).join("\n")
    })?;
    // Z0594：运行期自己产生的告警（下面 run_io 追加的 W-cost-unknown、W-untested、W-bank-status 追加时已各自打过 stderr）
    let 运行期告警: Vec<String> = report["trace"]["warnings"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|w| w.as_str().map(String::from))
        .collect();
    report["fixture_description"] = serde_json::json!(description);
    // 缺省账本路径（B187）：只在用了缺省路径时出现，给了 --ledger-out 的报告逐字节不变
    if options.ledger_out.is_none()
        && let Some(p) = &ledger_out
    {
        report["ledger_path"] = serde_json::json!(p.display().to_string());
    }
    report["replay"] = serde_json::json!(options.replay.is_some());
    report["resumed"] = serde_json::json!(options.resume.is_some());
    report["mode"] = serde_json::json!(mode_label);
    report["backend"] = serde_json::json!(backend_label);
    // 用了生成器才出现（步 15h-1）；生成器画像哈希自步 19 起也进账本头（`gen_profile_hash`）
    if let Some(g) = &生成器 {
        report["gen_backend"] = serde_json::json!({
            "name": g.spec.name, "model": g.model, "profile_hash": g.profile.hash,
        });
    }
    if let Some(r) = 真机画像.filter(|r| r.profile.price_per_input_token().is_none()) {
        mark_cost_unknown(&mut report, &r.path);
    }
    // 真机发了请求（首跑或续接）而画像没给超时：报 W-untested（重放不发请求，不报）
    if options.replay.is_none()
        && options.backend.spec().is_some_and(|s| s.transport)
        && let Some(r) = 真机画像.filter(|r| r.profile.transport_timeout_s().is_none())
    {
        mark_timeout_untested(&mut report, &r.path);
    }
    // 题库：装载了非在岗条目的题式文件（补缺 3）
    mark_bank_status(
        &mut report,
        &jpp::store::bank::non_service_loaded(loaded.sources.iter().map(|f| f.path.as_path())),
    );
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
    // G2（`12` R9 单次形态，主控定第 2 条）：违规逐笔在 stderr 报（形状与原运行期 J-05 行相同），末尾一行汇总，
    // 报告照写，进程以退出码 3 结束（`main` 读 `VIOLATION_EXIT`）
    let 违规数 = report["violations"].as_array().map_or(0, |v| v.len());
    let 违规行: Vec<String> = report["violations"]
        .as_array()
        .map(|vs| {
            let items = vs
                .iter()
                .map(|v| {
                    crate::diag_json::Item::new(
                        "J-05",
                        crate::diag_json::Level::Error,
                        Some(jpp_syntax::ast::Span {
                            start: v["site"].as_u64().unwrap_or(0) as usize,
                            end: v["site_end"].as_u64().unwrap_or(0) as usize,
                        }),
                        v["message"].as_str().unwrap_or(""),
                    )
                })
                .collect();
            crate::diag_json::render_all(loaded, items)
        })
        .unwrap_or_default();
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
    // Z0594（过程记录 5.29）：文本模式也把运行期告警打到 stderr（`--json` 时上面已以 JSON Lines 打过）：
    // 同码同址折叠、带文件行列，报告写完之后、违规行之前
    if !crate::diag_json::json_mode() {
        let items = 运行期告警
            .iter()
            .filter_map(|w| crate::diag_json::Item::from_warning(w))
            .collect();
        for line in crate::diag_json::render_all(loaded, items) {
            eprintln!("{line}");
        }
    }
    if !违规行.is_empty() {
        for line in &违规行 {
            eprintln!("{line}");
        }
        eprintln!("违规 {违规数} 笔：本次结论为未决（violation），值照带；退出码 3");
        crate::VIOLATION_EXIT.store(true, std::sync::atomic::Ordering::SeqCst);
    }
    Ok(())
}

/// `--cache <目录>`（步 19）：目录里（不含子目录）每个能按账本读的文件（v3；v2 在内存迁移）进索引；
/// 不是账本的文件（解码不成、不是 UTF-8）跳过并计数。目录不存在按空缓存（stderr 说一句）；
/// 其余读错误（路径不是目录、没有权限、I/O 出错）带路径报 `E-cache`，不悄悄当空缓存重新付费。
/// 来源名取文件名。依据：B151（`21` 步 19 追加项）；公开 PR #37 评审 P2（读错误不许当空缓存）
fn load_cache_dir(dir: &Path) -> Result<jpp::store::CacheIndex, String> {
    let io = |p: &Path, e: std::io::Error| format!("E-cache: --cache {}: {e}", p.display());
    let rd = match fs::read_dir(dir) {
        Ok(rd) => rd,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            eprintln!("缓存：{} 不存在，按空缓存", dir.display());
            return Ok(jpp::store::CacheIndex::default());
        }
        Err(e) => return Err(io(dir, e)),
    };
    let mut files: Vec<std::path::PathBuf> = vec![];
    for e in rd {
        let p = e.map_err(|e| io(dir, e))?.path();
        if p.is_file() {
            files.push(p);
        }
    }
    files.sort();
    let mut ledgers = vec![];
    let mut 跳过 = 0usize;
    for f in &files {
        let text = match fs::read_to_string(f) {
            Ok(t) => t,
            // 不是 UTF-8：不是账本，跳过
            Err(e) if e.kind() == std::io::ErrorKind::InvalidData => {
                跳过 += 1;
                continue;
            }
            Err(e) => return Err(io(f, e)),
        };
        match jpp::store::migrations::ledger_v2::read_any(&text) {
            Ok((l, _, _)) => {
                let name = f
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_default();
                ledgers.push((name, l));
            }
            Err(_) => 跳过 += 1,
        }
    }
    let ix = jpp::store::CacheIndex::build(&ledgers);
    let c = ix.counts();
    eprintln!(
        "缓存：从 {} 份账本建索引（判断 {} 条、生成 {} 条、变换 {} 条），跳过 {} 个文件",
        ledgers.len(),
        c.judge,
        c.gen_,
        c.transform,
        跳过
    );
    Ok(ix)
}

/// 本次运行的 `(lib_version, bank_version)`：首跑总带序言；审计重放时不带序言算出的等于账本头就用它（旧账本、
/// 关着伴随题录的），否则带序言（两个都不等时照常由账本口报 `W-header`）。复查 2026-09-30 小项 2、过程记录 5.19
fn 选版本(
    重放: bool,
    账本头: Option<&str>,
    不带: (String, Option<String>),
    带: impl FnOnce() -> (String, Option<String>),
) -> (String, Option<String>) {
    if 重放 && 账本头 == Some(不带.0.as_str()) {
        不带
    } else {
        带()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use jpp::effects::{EffectError, JevClient, JevPorts, Profile, ReplayPorts, transport_timeout};
    use serde_json::json;
    use std::{cell::RefCell, rc::Rc};

    /// 伴随题开关（B0492 S5，主控 2026-09-30 路 3）：与伴随题无关的测试显式关掉；`JPP_TEST_COMPANIONS=on` 整体开着跑
    fn 伴随() -> jpp::interp::CompanionMode {
        if std::env::var("JPP_TEST_COMPANIONS").is_ok_and(|v| v == "on") {
            jpp::interp::CompanionMode::Same
        } else {
            jpp::interp::CompanionMode::Off
        }
    }

    /// Jev 传输桩的回答：伴随题（按题面认，线上请求不带校准键）给 `jpp::testing` 约定的中性读数，其余题给 `原`。
    /// 伴随题关着时请求里只有原题
    /// 挑 `lib_version` 的三支：首跑总带序言；重放时账本头等于不带序言的版本取它，否则带
    #[test]
    fn 选版本三支() {
        let 不带 = || ("甲".to_string(), None);
        let 带 = || ("乙".to_string(), None);
        assert_eq!(
            super::选版本(false, Some("甲"), 不带(), 带).0,
            "乙",
            "首跑总带序言"
        );
        assert_eq!(
            super::选版本(true, Some("甲"), 不带(), 带).0,
            "甲",
            "旧账本不带序言"
        );
        assert_eq!(super::选版本(true, Some("乙"), 不带(), 带).0, "乙");
        assert_eq!(
            super::选版本(true, Some("丙"), 不带(), 带).0,
            "乙",
            "都不等照带，由账本口报 W-header"
        );
    }

    fn 桩答(body: &serde_json::Value, 原: f64) -> serde_json::Value {
        let mut out = serde_json::Map::new();
        for (id, q) in body["questions"].as_object().into_iter().flatten() {
            let 中性 = jpp::testing::伴随题面(q["instructions"].as_str().unwrap_or(""));
            // K 选一的伴随题（「最缺哪类」）：各候选键均匀
            if 中性 && q["type"] == "choice" {
                let ks: Vec<&String> = q["criteria"]
                    .as_object()
                    .map(|c| c.keys().collect())
                    .unwrap_or_default();
                let p = 1.0 / ks.len().max(1) as f64;
                out.insert(id.clone(), json!({"probabilities": ks.iter().map(|k| ((*k).clone(), json!(p))).collect::<serde_json::Map<_, _>>()}));
                continue;
            }
            out.insert(id.clone(), json!({"noul": if 中性 { 0.5 } else { 原 }}));
        }
        serde_json::Value::Object(out)
    }

    /// `runner::execute` 带 [`伴随`]
    fn 执行(
        program: &Program,
        ports: jpp_effects::Ports<'_>,
        calib: &CalibStore,
        ledger: &mut dyn jpp::LedgerPort,
        replay_only: bool,
        evidence: &mut Vec<(String, jpp::effects::Sample)>,
        entry: &jpp::EntryArgs,
    ) -> Result<serde_json::Value, jpp::Error> {
        runner::execute_with(
            program,
            ports,
            calib,
            ledger,
            replay_only,
            evidence,
            entry,
            runner::CacheArgs {
                companions: Some(伴随()),
                ..Default::default()
            },
            None,
        )
    }

    /// 与 `crates/jpp-cli/tests/wiring.rs` 的 `calib目录让unsure_bound不再恒等于n` 同一份
    /// 程序骨架：单道 `test` 题喂进 `unsure_bound`，产出是一个数，便于比较真实跑与重放。
    fn 程序() -> Program {
        jpp::lower(
            &jpp_syntax::parse("budget {calls: 4, cost: 0};\nlet r = judge(state(mat(\"材料\")), test(\"行吗\",\"k\"));\nunsure_bound([r])\n")
                .expect("解析"),
        )
        .expect("lower")
    }

    /// **选 live 后端走 transport**：`runner::execute` 接的是 `JevPorts`（`JevClient` 用 `with_transport`
    /// 代替真网络），真实调用要经过 transport 且记进 `cost.calls`；随后用同一份账本重放，
    /// 新增调用必须是 0——这正是 B1 验收要的「重放新增调用为 0」。
    #[test]
    fn live后端走transport_重放新增调用为零() {
        let program = 程序();
        let calls = Rc::new(RefCell::new(0));
        let c2 = calls.clone();
        let client = JevClient::with_transport(
            "jev-1.13.0",
            Box::new(move |body| {
                *c2.borrow_mut() += 1;
                Ok(json!({"answers": 桩答(&body, 0.8)}))
            }),
        );
        let calib = CalibStore::new();
        let mut ledger = Ledger::new();
        ledger.rebuild_index();
        let mut evidence = vec![];
        let report = 执行(
            &program,
            JevPorts::new(client).ports(),
            &calib,
            &mut ledger,
            false,
            &mut evidence,
            &jpp::EntryArgs::default(),
        )
        .expect("真机路径跑得完");
        assert_eq!(*calls.borrow(), 1, "接线要真的经过 transport，不是绕过它");
        assert_eq!(report["cost"]["calls"], json!(1), "报告要记 1 次真实调用");
        assert_eq!(report["cost"]["replayed"], json!(0), "这一趟没有从账本重放");

        ledger.rebuild_index();
        // **重放要用记录时那个 model_id**（`run_checked` 里 `replay_model_id` 旁的注释）：直接套
        // `NoCallPorts`（固定 `"fixed-0"`）在这里会撞上与生产代码同一个键不匹配的坑。

        let mut evidence2 = vec![];
        let replay_report = 执行(
            &program,
            ReplayPorts::ports("jev-1.13.0"),
            &calib,
            &mut ledger,
            true,
            &mut evidence2,
            &jpp::EntryArgs::default(),
        )
        .expect("用刚写的账本重放跑得完");
        assert_eq!(
            replay_report["cost"]["calls"],
            json!(0),
            "重放不应产生任何新调用"
        );
        assert_eq!(
            replay_report["value"], report["value"],
            "重放结果要与真实运行一致"
        );
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
            backend: Backend::Registered(&jpp::backends::jev::SPEC),
            model: None,
            profiles_dir: Some(
                std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../profiles"),
            ),
            input: None,
            input_trusted: false,
            mat_store: None,
            envs: Vec::new(),
            release_on_declared: false,
            guard: false,
            confirm: false,
            confirm_above: None,
            gen_model: None,
            companions: None,
            cells: None,
            cells_stats: false,
            gen_profile: None,
            cache: None,
            trace_parent: None,
            trace_seed: None,
            trace_label: None,
            carry_in: None,
            carry_out: None,
            carry_reauthorize: false,
        }
    }

    /// B73：真机路径按 `--profiles-dir/<model>.json` 解析发行画像，装进校准库，
    /// 账本头 `profile_hash` 非空（= 发行画像的哈希），费用 = input tokens × 画像价格。
    #[test]
    fn 真机画像进账本头_价格从画像来() {
        let program = 程序();
        let 画像 = crate::profile_resolve::resolve(&发行画像选项())
            .expect("解析")
            .expect("真机必有画像");
        let price = 画像
            .profile
            .price_per_input_token()
            .expect("发行画像带价格");
        let mut calib = CalibStore::new();
        install_profile(&mut calib, Some(&画像));
        let client = JevClient::with_transport(
            "jev-1.13.0",
            Box::new(|body| {
                Ok(json!({"answers": 桩答(&body, 0.8), "usage": {"input_tokens": 1000}}))
            }),
        )
        .with_price(画像.profile.price_per_input_token());
        let mut ledger = Ledger::new();
        ledger.rebuild_index();
        let mut evidence = vec![];
        let report = 执行(
            &program,
            JevPorts::new(client).ports(),
            &calib,
            &mut ledger,
            false,
            &mut evidence,
            &jpp::EntryArgs::default(),
        )
        .expect("跑得完");
        let h = ledger.header.as_ref().expect("有账本头");
        assert_eq!(
            h.compared.profile_hash.as_deref(),
            Some("6dd82b3f651fed3f"),
            "账本头记发行画像的哈希（传输超时步加 transport 节后）"
        );
        assert_eq!(report["cost"]["tokens"], json!(1000));
        assert_eq!(
            report["cost"]["usd"].as_f64(),
            Some(1000.0 * price),
            "费用 = tokens × 画像价格"
        );
    }

    /// 传输超时：发行画像带 `transport.timeout_s = 30`，真机客户端的超时从这里来。
    #[test]
    fn 发行画像给出传输超时() {
        let 画像 = crate::profile_resolve::resolve(&发行画像选项())
            .expect("解析")
            .expect("真机必有画像");
        assert_eq!(画像.profile.transport_timeout_s(), Some(30.0));
        assert_eq!(
            transport_timeout(&画像.profile),
            Some(std::time::Duration::from_secs(30))
        );
    }

    /// 传输超时：画像缺字段 → 不设超时、`W-untested: transport.timeout_s`（主会话 2026-09-24 确认）。
    #[test]
    fn 画像缺超时字段时报w_untested() {
        assert_eq!(
            transport_timeout(&Profile::untested()),
            None,
            "代码里不编秒数"
        );
        let mut report = json!({"trace": {"warnings": []}});
        mark_timeout_untested(&mut report, std::path::Path::new("p.json"));
        // 依据：地基/过程记录/工程-传输超时.md（画像缺 transport.timeout_s 报 W-untested）
        assert!(
            report["trace"]["warnings"][0]
                .as_str()
                .unwrap()
                .starts_with("W-untested: transport.timeout_s")
        );
    }

    /// B73：画像没有价格 → 报告费用记 `Unknown`，`trace.warnings` 有 `W-cost-unknown`。
    #[test]
    fn 画像无价格时费用记unknown() {
        let mut report = json!({"cost": {"usd": 0.0}, "trace": {"warnings": []}});
        mark_cost_unknown(&mut report, std::path::Path::new("p.json"));
        assert_eq!(report["cost"]["usd"], json!("Unknown"));
        // 依据：B73（地基/附注/2026-09-24-评估①裁定.md §二；21 步 15d-0）
        assert!(
            report["trace"]["warnings"][0]
                .as_str()
                .unwrap()
                .starts_with("W-cost-unknown")
        );
    }

    /// **transport 失败时报错要说得清楚**：错误原因要冒泡到 `jpp::Error::Runtime`，
    /// 不能被吞成一句通用的失败提示。
    #[test]
    fn transport失败时报错清楚() {
        let program = 程序();
        let client = JevClient::with_transport(
            "jev-1.13.0",
            Box::new(|_body| Err(EffectError("连接超时".into()))),
        );
        let calib = CalibStore::new();
        let mut ledger = Ledger::new();
        ledger.rebuild_index();
        let mut evidence = vec![];
        let err = 执行(
            &program,
            JevPorts::new(client).ports(),
            &calib,
            &mut ledger,
            false,
            &mut evidence,
            &jpp::EntryArgs::default(),
        )
        .expect_err("transport 出错应当冒泡成 Err，不能被吞掉");
        let jpp::Error::Runtime(e) = err else {
            panic!("应为运行期错误，不是静态检查错误");
        };
        assert!(
            e.message.contains("连接超时"),
            "错误要带上 transport 的原因：{}",
            e.message
        );
    }
}
