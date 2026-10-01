//! `jpp check --explain` 与 `jpp run --explain`（Z0190 后一半，`20` §2.3 `explain`、`20a` D8）。
//! 预注册：`地基/过程记录/工程-Z0190-EXPLAIN.md` §二。
//!
//! 钉的是：示例的 EXPLAIN 给出预注册的区间与站点；被拒的计划照样先打印再报错（`check` 与 `run` 都是）；
//! `--json` 时 stdout 仍是一个文档、报告只多一个 `explain` 键；不带 `--explain` 时输出与原来逐字节相同；
//! 站点行的调用数之和等于整程序的调用数上界（分类不漂移）；`run` 的 `explain` 与报告 `plan` 段同源。

use serde_json::Value;
use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Output},
};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap()
}

fn tmp(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("jpp-explain-{tag}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&d);
    fs::create_dir_all(&d).unwrap();
    d
}

/// 在 `rust-jpp` 根目录跑 `jpp`（示例与夹具用相对路径，与金样同口径）
fn jpp(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_jpp"))
        .current_dir(root())
        .args(args)
        .output()
        .unwrap()
}

fn out(o: &Output) -> String {
    String::from_utf8_lossy(&o.stdout).into_owned()
}

fn err(o: &Output) -> String {
    String::from_utf8_lossy(&o.stderr).into_owned()
}

fn line_of<'a>(text: &'a str, head: &str) -> &'a str {
    text.lines()
        .find(|l| l.trim_start().starts_with(head))
        .unwrap_or_else(|| panic!("找不到以「{head}」开头的行：\n{text}"))
}

/// 三层依赖链，`calls: 2`：可靠下界 3 次调用，首跑计划期被拒（同 `plan_estimate.rs` 的三层链）
const 三层链: &str = r#"budget {calls: 2, cost: 1};
let a = cut(judge(state(mat("甲")), test("行吗", "k")));
let b = cut(judge(state(mat(exit_kind(a))), test("行吗", "k")));
let c = cut(judge(state(mat(exit_kind(b))), test("行吗", "k")));
consume([a, b, c], "drop");
1
"#;

fn 写(dir: &Path, name: &str, text: &str) -> String {
    let p = dir.join(name);
    fs::write(&p, text).unwrap();
    p.display().to_string()
}

#[test]
fn check_explain_sieve_states_range_site_and_budget_comparison() {
    let o = jpp(&["check", "examples/sieve.jpp", "--explain"]);
    assert!(o.status.success(), "{}", err(&o));
    let t = out(&o);
    assert!(t.starts_with("EXPLAIN"), "{t}");
    assert!(
        t.contains("口径：账本为空、缓存关闭（首跑）；没有给判断器单价"),
        "{t}"
    );
    assert!(
        t.contains("预算：calls 12 · cost 0.01 美元 · latency_p95 未设"),
        "{t}"
    );
    let calls = line_of(&t, "调用数");
    assert!(calls.contains("1 至 90 次"), "{calls}");
    assert!(
        calls.contains("预算 calls 12：上界超出预算，下界没超"),
        "{calls}"
    );
    assert!(line_of(&t, "判断层数").contains("1 至 30 层"), "{t}");
    // 没给单价、没给画像：费用与时延如实标未估，不写 0
    assert!(
        line_of(&t, "费用").contains("未估：宿主没有给判断器单价"),
        "{t}"
    );
    assert!(
        line_of(&t, "时延").contains("未估：画像字段 concurrency.latency_s.p95 未测"),
        "{t}"
    );
    // 站点：一行 sieve，源码位置与那一行源码，必经、层 ≥ 1、30 次执行 × 3 = 90 次调用
    assert!(
        t.contains("sieve.jpp:20:15  sieve  let results = sieve(notes, mention,"),
        "{t}"
    );
    assert!(
        t.contains("必经 · 层 ≥ 1 · 最多 90 次调用（执行 30 次）"),
        "{t}"
    );
    assert!(t.contains("拒绝：无") && t.contains("告警：无"), "{t}");
    assert!(t.contains("未估（由 Plan 推出，不写 0）"), "{t}");
    // EXPLAIN 在最后一行 `Checked …` 之前
    let (e, c) = (
        t.find("EXPLAIN").unwrap(),
        t.find("Checked examples/sieve.jpp").unwrap(),
    );
    assert!(e < c, "{t}");
}

#[test]
fn check_explain_examples_match_preregistered_ranges() {
    // tally：1 至 15 次、1 至 5 层，站点 5 次执行 × 3 = 15 次调用
    let t = out(&jpp(&["check", "examples/tally.jpp", "--explain"]));
    assert!(line_of(&t, "调用数").contains("1 至 15 次"), "{t}");
    assert!(line_of(&t, "判断层数").contains("1 至 5 层"), "{t}");
    assert!(t.contains("tally.jpp:13:9  sieve"), "{t}");
    assert!(
        t.contains("必经 · 层 ≥ 1 · 最多 15 次调用（执行 5 次）"),
        "{t}"
    );
    assert!(
        line_of(&t, "调用数").contains("预算 calls 10：上界超出预算"),
        "{t}"
    );
    // adaptive：含 gen/do，上界未知，只写下界，不写出区间、不写 0 当上界
    let t = out(&jpp(&["check", "examples/adaptive.jpp", "--explain"]));
    let calls = line_of(&t, "调用数");
    assert!(calls.contains("至少 0 次，上界未知"), "{calls}");
    assert!(calls.contains("上界未知，不能保证在预算内"), "{calls}");
    assert!(!calls.contains(" 至 "), "{calls}");
    // 效应都藏在规划器解析不了的地方：站点表明说没解析出来，不写「没有站点」
    assert!(t.contains("逐站点信息未估：规划器没有解析出站点"), "{t}");
    assert!(!t.contains("没有会发调用的站点"), "{t}");
}

#[test]
fn check_explain_still_prints_when_plan_is_rejected() {
    let d = tmp("reject");
    let f = 写(&d, "chain.jpp", 三层链);
    let o = jpp(&["check", &f, "--explain"]);
    assert!(!o.status.success());
    let t = out(&o);
    assert!(err(&o).contains("E-budget-plan"), "{}", err(&o));
    assert!(
        t.contains("拒绝：E-budget-plan：计划期拒绝（B42，J-07b）"),
        "{t}"
    );
    let calls = line_of(&t, "调用数");
    assert!(calls.contains("3 至 9 次"), "{calls}");
    assert!(calls.contains("下界已超预算：一定超"), "{calls}");
    assert!(line_of(&t, "判断层数").contains("3 层"), "{t}");
    // 三个站点，层 ≥ 1、2、3，均必经
    for n in 1..=3 {
        assert!(
            t.contains(&format!("必经 · 层 ≥ {n} · 最多 3 次调用（执行 1 次）")),
            "{t}"
        );
    }
}

#[test]
fn check_explain_with_profile_gives_latency_range() {
    let o = jpp(&[
        "check",
        "examples/window-over.jpp",
        "--profile",
        "profiles/jev-1.13.0.json",
        "--explain",
    ]);
    assert!(o.status.success(), "{}", err(&o));
    let t = out(&o);
    assert!(line_of(&t, "时延").contains("1.026 至 36.936 秒"), "{t}");
    assert!(line_of(&t, "时延").contains("预算 latency_p95 未设"), "{t}");
    // 有单价、没有 token 上界：费用只有下界
    let cost = line_of(&t, "费用");
    assert!(
        cost.contains("至少 ") && cost.contains("美元，上界未知"),
        "{cost}"
    );
    assert!(line_of(&t, "调用数").contains("1 至 108 次"), "{t}");
    assert!(
        line_of(&t, "调用数").contains("预算 calls 3：上界超出预算"),
        "{t}"
    );
}

#[test]
fn check_explain_json_is_one_document_on_stdout_and_stderr_is_empty() {
    let o = jpp(&["check", "examples/tally.jpp", "--json", "--explain"]);
    assert!(o.status.success(), "{}", err(&o));
    assert!(err(&o).is_empty(), "stderr 不出东西：{}", err(&o));
    let doc: Value = serde_json::from_str(&out(&o)).expect("stdout 是一个 JSON 文档");
    assert_eq!(doc["ok"], true);
    let e = &doc["explain"];
    // 键名与 Plan 一一对应（裁定三十二）：五个 Estimate，形状 {lo,hi} / {lo} / {unknown: 原因}
    assert_eq!(e["calls_est"], serde_json::json!({"lo": 1, "hi": 15}));
    assert_eq!(e["layers_est"], serde_json::json!({"lo": 1, "hi": 5}));
    assert_eq!(
        e["cost_est"],
        serde_json::json!({"unknown": "price_not_given"})
    );
    assert_eq!(
        e["latency_est"],
        serde_json::json!({"unknown": "untested:concurrency.latency_s.p95"})
    );
    assert_eq!(
        e["unsure_bound"],
        serde_json::json!({"unknown": "not_computed"})
    );
    // 预算结论就是 rejected 有没有值；不另加布尔、比对表与未估数组，也没有自然语言口径键
    assert!(e["rejected"].is_null());
    assert_eq!(e["warnings"], serde_json::json!([]));
    for gone in ["comparison", "unestimated", "basis"] {
        assert!(e.get(gone).is_none(), "{gone} 不该出现：{e}");
    }
    // 输入键：预算与结构化的规划语境
    assert_eq!(e["budget"]["calls"], 10);
    assert!(e["budget"]["escalate"].is_null() && e["budget"]["unsure"].is_null());
    assert_eq!(
        e["ctx"],
        serde_json::json!({"ledger_empty": true, "cache_off": true, "judge_price": "not_given"})
    );
    // per_site：以站点号为键，照 SitePlan 的字段，另有派生的 kind、at、class、calls_hi
    let sites = e["per_site"].as_object().unwrap();
    assert_eq!(sites.len(), 1, "{sites:?}");
    let s = sites.values().next().unwrap();
    assert_eq!(s["kind"], "sieve");
    assert_eq!(s["at"], "tally.jpp:13:9");
    assert_eq!(s["class"], "reading");
    assert_eq!(s["must"], true);
    assert_eq!(s["layer_lo"], 1);
    assert_eq!(s["execs_hi"], 5);
    assert_eq!(s["calls_hi"], 15);
    // 不带 --explain：文档没有 explain 键
    let o = jpp(&["check", "examples/tally.jpp", "--json"]);
    let doc: Value = serde_json::from_str(&out(&o)).unwrap();
    assert!(doc.get("explain").is_none(), "{doc}");
}

/// 被拒的计划：`rejected` 照 `IrDiag` 的 serde（code、message、span）
#[test]
fn check_explain_json_rejected_follows_the_diagnostic_serde() {
    let d = tmp("jsonreject");
    let f = 写(&d, "chain.jpp", 三层链);
    let o = jpp(&["check", &f, "--json", "--explain"]);
    assert!(!o.status.success());
    let doc: Value = serde_json::from_str(&out(&o)).unwrap();
    let r = &doc["explain"]["rejected"];
    assert_eq!(r["code"], "E-budget-plan");
    assert!(
        r["message"].as_str().unwrap().contains("调用数下界 3 次"),
        "{r}"
    );
    assert!(
        r["span"]["start"].is_number() && r["span"]["end"].is_number(),
        "{r}"
    );
    assert_eq!(
        doc["explain"]["calls_est"],
        serde_json::json!({"lo": 3, "hi": 9})
    );
}

/// 固定观察跑 `sieve`，报告写到 `dir/<name>`；`extra` 是附加参数
fn run_sieve(dir: &Path, name: &str, extra: &[&str]) -> (Output, PathBuf) {
    let report = dir.join(name);
    let r = report.display().to_string();
    let mut args = vec![
        "run",
        "examples/sieve.jpp",
        "--fixtures",
        "examples/fixtures/sieve.json",
        "--output",
        &r,
    ];
    args.extend_from_slice(extra);
    (jpp(&args), report)
}

#[test]
fn run_explain_prints_to_stderr_and_report_is_byte_identical_to_without_flag() {
    let d = tmp("run");
    let (plain, r0) = run_sieve(&d, "plain.json", &[]);
    assert!(plain.status.success(), "{}", err(&plain));
    assert!(!err(&plain).contains("EXPLAIN"), "不带标志不出 EXPLAIN");
    let (o, r1) = run_sieve(&d, "explain.json", &["--explain"]);
    assert!(o.status.success(), "{}", err(&o));
    let e = err(&o);
    assert!(e.contains("EXPLAIN（计划"), "{e}");
    // 固定观察：单价 0，费用「0 美元」，不是未估
    assert!(
        e.contains("口径：账本为空、缓存关闭（首跑）；判断器单价 0（固定观察，不付费）"),
        "{e}"
    );
    assert!(line_of(&e, "费用").contains("0 美元"), "{e}");
    assert!(line_of(&e, "调用数").contains("1 至 90 次"), "{e}");
    // stdout 留给报告：不出 EXPLAIN
    assert!(!out(&o).contains("EXPLAIN"), "{}", out(&o));
    assert_eq!(
        fs::read(&r0).unwrap(),
        fs::read(&r1).unwrap(),
        "报告逐字节相同"
    );
}

#[test]
fn run_explain_replay_says_ledger_not_empty_and_lower_bound_zero() {
    let d = tmp("replay");
    let ledger = d.join("l.json").display().to_string();
    let (first, _) = run_sieve(&d, "first.json", &["--ledger-out", &ledger]);
    assert!(first.status.success(), "{}", err(&first));
    let report = d.join("replay.json").display().to_string();
    let o = jpp(&[
        "run",
        "examples/sieve.jpp",
        "--replay",
        &ledger,
        "--output",
        &report,
        "--explain",
    ]);
    assert!(o.status.success(), "{}", err(&o));
    let e = err(&o);
    assert!(e.contains("账本非空（续接或重放）"), "{e}");
    // 下界记 0，上界不变；层数同
    assert!(line_of(&e, "调用数").contains("0 至 90 次"), "{e}");
    assert!(line_of(&e, "判断层数").contains("0 至 30 层"), "{e}");
    assert!(
        e.contains("必经 · 层 ≥ 1 · 最多 90 次调用（执行 30 次）"),
        "{e}"
    );
}

#[test]
fn run_json_explain_puts_explain_key_in_report_and_keeps_stderr_clean_of_text() {
    let d = tmp("runjson");
    let (o, r) = run_sieve(&d, "j.json", &["--json", "--explain"]);
    assert!(o.status.success(), "{}", err(&o));
    assert!(!err(&o).contains("EXPLAIN"), "{}", err(&o));
    let report: Value = serde_json::from_str(&fs::read_to_string(&r).unwrap()).unwrap();
    let e = &report["explain"];
    assert!(e.is_object(), "{report}");
    // 与报告 `plan` 段同源：四项估计逐项相等（报告 `plan` 段带 `kind`，`explain` 键是 {lo,hi}/{lo}/{unknown}）
    for (plan_key, est_key) in [
        ("calls", "calls_est"),
        ("layers", "layers_est"),
        ("cost_usd", "cost_est"),
        ("latency_s", "latency_est"),
    ] {
        let p = &report["plan"][plan_key];
        let want = match p["kind"].as_str().unwrap() {
            "known" => serde_json::json!({"lo": p["lo"], "hi": p["hi"]}),
            "at_least" => serde_json::json!({"lo": p["lo"]}),
            _ => serde_json::json!({"unknown": p["reason"]}),
        };
        assert_eq!(e[est_key], want, "{est_key}");
    }
    let site = e["per_site"].as_object().unwrap().values().next().unwrap();
    assert_eq!(site["calls_hi"], 90);
    // 不带 --explain：报告没有 explain 键
    let (_, r2) = run_sieve(&d, "j2.json", &["--json"]);
    let plain: Value = serde_json::from_str(&fs::read_to_string(&r2).unwrap()).unwrap();
    assert!(plain.get("explain").is_none());
}

#[test]
fn run_explain_rejected_plan_prints_explain_before_the_error() {
    let d = tmp("runreject");
    let f = 写(&d, "chain.jpp", 三层链);
    let report = d.join("never.json");
    let r = report.display().to_string();
    let o = jpp(&["run", &f, "--output", &r, "--explain"]);
    assert!(!o.status.success());
    let e = err(&o);
    let (at_explain, at_error) = (
        e.find("EXPLAIN（计划").expect("先有 EXPLAIN"),
        e.find("E-budget-plan: 计划期拒绝").expect("再有拒绝的错误"),
    );
    assert!(at_explain < at_error, "EXPLAIN 要在报错之前：\n{e}");
    assert!(e.contains("下界已超预算：一定超"), "{e}");
    assert!(e.contains("拒绝：E-budget-plan"), "{e}");
    assert!(!report.exists(), "被拒的计划一次调用都不发，不写报告");
}

#[test]
fn explain_flag_is_rejected_on_other_verbs_and_when_repeated() {
    let o = jpp(&["parse", "examples/sieve.jpp", "--explain"]);
    assert_eq!(o.status.code(), Some(2));
    assert!(
        err(&o).contains("--explain is accepted only by check and run"),
        "{}",
        err(&o)
    );
    let o = jpp(&["check", "examples/sieve.jpp", "--explain", "--explain"]);
    assert_eq!(o.status.code(), Some(2));
    assert!(
        err(&o).contains("--explain was supplied twice"),
        "{}",
        err(&o)
    );
    let o = jpp(&["bank", "list", "--explain"]);
    assert_eq!(o.status.code(), Some(2));
    // 用法文本里有它
    let o = jpp(&["--help"]);
    assert!(
        out(&o).contains("jpp check <file.jpp> [--json] [--explain]"),
        "{}",
        out(&o)
    );
    assert!(
        out(&o).contains("jpp run <file.jpp> [--json] [--explain]"),
        "{}",
        out(&o)
    );
}

#[test]
fn explain_off_leaves_check_output_unchanged() {
    let o = jpp(&["check", "examples/sieve.jpp"]);
    assert!(o.status.success());
    assert_eq!(
        out(&o),
        "Checked examples/sieve.jpp: no static errors (1 warnings)\n"
    );
    assert!(!err(&o).contains("EXPLAIN"));
}

/// 站点行的调用数是整程序调用数上界的逐站点拆分；对全部可运行示例，凡整程序上界已知者，
/// 各站点调用数之和必须等于它——分类（哪些站点每次发几次调用）与 `passes/plan.rs` 不漂移的钉子。
#[test]
fn explain_site_calls_sum_matches_plan_upper_bound_on_all_examples() {
    let r = root();
    let manifest: Value =
        serde_json::from_str(&fs::read_to_string(r.join("tests/golden/manifest.json")).unwrap())
            .unwrap();
    let mut checked = 0;
    let mut known = 0;
    for case in manifest["cases"].as_array().unwrap() {
        if case["expect"] == "error" {
            continue;
        }
        let src = case["source"].as_str().unwrap();
        let loaded = jpp_syntax::loader::load(&r.join(src)).expect("装载示例");
        let program = jpp::Session::compile(&loaded.program, &jpp::EntryArgs::default().decl())
            .expect("降级");
        let ctx = jpp::interp::PlanCtx {
            ledger_empty: true,
            cache_off: true,
            judge_price: jpp::interp::JudgePrice::Known(0.0),
        };
        let plan = jpp::interp::plan_with(&program, &jpp::interp::Passes::default(), None, &ctx);
        checked += 1;
        let Some(hi) = plan.calls_est.hi() else {
            continue;
        };
        known += 1;
        let per = jpp_plan::explain::site_calls_hi(&plan, &program);
        let mut sum = 0u64;
        for (id, c) in &per {
            let c =
                c.unwrap_or_else(|| panic!("{src}：整程序上界已知，站点 {} 的调用数却未知", id.0));
            sum += c;
        }
        assert_eq!(
            sum, hi,
            "{src}：各站点调用数之和 {sum} ≠ 整程序调用数上界 {hi}"
        );
    }
    assert!(checked >= 45, "只核了 {checked} 个示例");
    assert!(known >= 15, "上界已知的示例只有 {known} 个，钉子太松");
}

/// `gen`、`do`、`ask` 不是读数站点：规划器没给它们记必经与层，EXPLAIN 不能把「没记」写成「非必经」
#[test]
fn check_explain_gen_site_is_not_reported_as_non_must() {
    let t = out(&jpp(&["check", "examples/gen-choose.jpp", "--explain"]));
    assert!(t.contains("gen-choose.jpp:8:13  gen"), "{t}");
    assert!(
        t.contains("不占判断层 · 是否必经规划器未记录 · 最多 1 次调用（执行 1 次）"),
        "{t}"
    );
    let o = jpp(&["check", "examples/gen-choose.jpp", "--json", "--explain"]);
    let doc: Value = serde_json::from_str(&out(&o)).unwrap();
    let sites = doc["explain"]["per_site"].as_object().unwrap();
    let gen_site = sites
        .values()
        .find(|s| s["kind"] == "gen")
        .expect("gen 站点");
    assert_eq!(gen_site["class"], "call");
    assert!(gen_site["must"].is_null(), "{gen_site}");
    assert_eq!(gen_site["calls_hi"], 1);
}

/// `order`/`agg`/`repeat`/`allocate` 是规划器眼里「会发调用但口径未建」的构造：EXPLAIN 不能写成「不占判断层」（G2）
#[test]
fn check_explain_opaque_construct_sites_say_call_shape_not_built() {
    let t = out(&jpp(&["check", "examples/search-stop.jpp", "--explain"]));
    let rows: Vec<&str> = t
        .lines()
        .filter(|l| l.contains("  order  ") || l.contains("  agg  "))
        .collect();
    assert!(!rows.is_empty(), "search-stop 应有 order 站点：\n{t}");
    // 同一语句里的两个分支站点靠列号分开
    let places: std::collections::BTreeSet<&str> = rows
        .iter()
        .map(|l| l.split_whitespace().next().unwrap())
        .collect();
    assert_eq!(places.len(), rows.len(), "位置要带列号才分得开：{rows:?}");
    for r in &rows {
        let next = t.lines().skip_while(|l| l != r).nth(1).unwrap();
        assert!(next.contains("会发调用 · 次数与层口径未建"), "{r}\n{next}");
    }
    assert!(!t.contains("规划器没有解析出站点"), "{t}");
    let o = jpp(&["check", "examples/search-stop.jpp", "--json", "--explain"]);
    let doc: Value = serde_json::from_str(&out(&o)).unwrap();
    let opaque: Vec<&Value> = doc["explain"]["per_site"]
        .as_object()
        .unwrap()
        .values()
        .filter(|s| s["class"] == "opaque")
        .collect();
    assert!(!opaque.is_empty());
    assert!(opaque.iter().all(|s| s["calls_hi"].is_null()), "{opaque:?}");
}

/// `--cache` 时口径写「开了跨运行缓存」、调用数下界 0：`run` 侧的输入装配（`plan_of`）此前只在一个输入上钉过
#[test]
fn run_explain_with_cache_says_cache_on_and_lower_bound_zero() {
    let d = tmp("cache");
    let cache = d.join("cache");
    fs::create_dir_all(&cache).unwrap();
    let c = cache.display().to_string();
    let (o, _) = run_sieve(&d, "c.json", &["--cache", &c, "--explain"]);
    assert!(o.status.success(), "{}", err(&o));
    let e = err(&o);
    assert!(e.contains("开了跨运行缓存"), "{e}");
    assert!(line_of(&e, "调用数").contains("0 至 90 次"), "{e}");
}
