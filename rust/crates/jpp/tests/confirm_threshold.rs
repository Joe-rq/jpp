//! `jpp run` 的费用确认（Z0236，`11` §5.5「超阈值（默认 $0.10）要 `--confirm`」）。
//! 预注册：`地基/过程记录/工程-Z0236-确认阈值.md` §五。
//!
//! 钉的是两面：费用上界超阈值又没给 `--confirm`，一次调用都不发就拒（`E-confirm-required`）；给了 `--confirm`
//! 照跑。加上不核的情形（固定观察、重放、单价未测）、EXPLAIN 里标出、`--json` 的形状、选项的校验。
//! 上界 = 程序声明的 `budget.cost` 与计划估计取小（今天规划器在有单价时没有上界，等于 `budget.cost`）。
//! 「有价格」的运行用 `--backend stub`（不发 API 请求）加带价格的画像：一分钱不花。

use serde_json::Value;
use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Output},
};

const 付费画像: &str = "profiles/judge-claude-p-priced.json";
const 无价画像: &str = "profiles/judge-claude-p.json";
/// 固定观察的夹具：与 `examples/partial.jpp`（`budget cost 1`）配套
const 夹具: &str = "examples/fixtures/partial.json";

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap()
}

fn tmp(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("jpp-confirm-{tag}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&d);
    fs::create_dir_all(&d).unwrap();
    d
}

/// 在 `rust-jpp` 根目录跑 `jpp`（示例与画像用相对路径）
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

fn write(dir: &Path, name: &str, text: &str) -> String {
    let p = dir.join(name);
    fs::write(&p, text).unwrap();
    p.display().to_string()
}

/// 一次判断、声明的 `budget cost` 可调
fn program(cost: &str) -> String {
    format!(
        "budget {{calls: 5, cost: {cost}}};\n\
         let a = cut(judge(state(mat(\"甲\")), test(\"行吗\", \"k\")));\n\
         consume([a], \"drop\");\n\
         1\n"
    )
}

/// 报告里的 `confirm` 键（没有为 `Null`）
fn report_confirm(o: &Output) -> Value {
    let r: Value = serde_json::from_str(&out(o)).unwrap_or_else(|e| panic!("{e}\n{}", out(o)));
    r["confirm"].clone()
}

fn assert_refused(o: &Output) {
    assert!(!o.status.success(), "应被拒：{}", out(o));
    assert!(out(o).is_empty(), "被拒不出报告：{}", out(o));
    let e = err(o);
    assert!(e.contains("E-confirm-required"), "{e}");
    assert!(e.contains("--confirm"), "{e}");
}

// ---------- 两面：超阈值无 --confirm 拒，带 --confirm 跑 ----------

#[test]
fn under_the_default_threshold_runs_without_confirm() {
    let o = jpp(&[
        "run",
        "examples/sieve.jpp",
        "--backend",
        "stub",
        "--profile",
        付费画像,
    ]);
    assert!(o.status.success(), "{}", err(&o));
    let c = report_confirm(&o);
    assert_eq!(c["verdict"], "within", "{c}");
    assert_eq!(c["threshold_usd"], 0.1);
    assert_eq!(c["threshold_is_default"], true);
    assert_eq!(c["upper_usd"], 0.01);
    assert_eq!(c["upper_from"], "budget.cost");
    assert_eq!(c["confirmed"], false);
}

#[test]
fn over_a_lowered_threshold_is_refused_without_confirm() {
    let d = tmp("refuse");
    let ledger = d.join("l.json");
    let report = d.join("r.json");
    let o = jpp(&[
        "run",
        "examples/sieve.jpp",
        "--backend",
        "stub",
        "--profile",
        付费画像,
        "--confirm-above",
        "0.001",
        "--ledger-out",
        &ledger.display().to_string(),
        "--output",
        &report.display().to_string(),
    ]);
    assert_refused(&o);
    let e = err(&o);
    assert!(e.contains("0.01"), "报文写上界：{e}");
    assert!(e.contains("0.001"), "报文写阈值：{e}");
    // 一次判断都没发：没有报告；账本（若落了盘）里没有任何判断条目，与计划期拒绝（`E-budget-plan`）同
    assert!(!report.exists(), "不该写报告");
    let l = fs::read_to_string(&ledger).unwrap_or_default();
    assert!(!l.contains("\"Judge"), "账本里不该有判断条目：{l}");
    assert!(l.lines().count() <= 1, "账本只有空壳：{l}");
}

#[test]
fn the_same_run_with_confirm_goes_ahead_and_records_it() {
    let base = [
        "run",
        "examples/sieve.jpp",
        "--backend",
        "stub",
        "--profile",
        付费画像,
    ];
    let plain = jpp(&base);
    let mut args = base.to_vec();
    args.extend(["--confirm-above", "0.001", "--confirm"]);
    let o = jpp(&args);
    assert!(o.status.success(), "{}", err(&o));
    let c = report_confirm(&o);
    assert_eq!(c["verdict"], "over", "{c}");
    assert_eq!(c["confirmed"], true);
    assert_eq!(c["threshold_is_default"], false);
    // 确认不改运行本身：同一程序同一后端，调用数相同
    let a: Value = serde_json::from_str(&out(&plain)).unwrap();
    let b: Value = serde_json::from_str(&out(&o)).unwrap();
    assert_eq!(a["cost"]["calls"], b["cost"]["calls"]);
    assert_eq!(a["value"], b["value"]);
    // 正面对照：放行的同一运行落盘的账本里有 `Judge` 条目，所以上面「拒绝时账本没有 Judge」的断言抓得到「拒了却发了」
    let d = tmp("ledger-positive");
    let ledger = d.join("l.json");
    let ledger_path = ledger.display().to_string();
    let mut args = base.to_vec();
    args.extend([
        "--confirm-above",
        "0.001",
        "--confirm",
        "--ledger-out",
        &ledger_path,
    ]);
    let o = jpp(&args);
    assert!(o.status.success(), "{}", err(&o));
    let l = fs::read_to_string(&ledger).unwrap();
    assert!(
        l.contains("\"Judge"),
        "放行的运行账本里应有 Judge 条目：{l}"
    );
}

#[test]
fn a_large_declared_budget_needs_confirm_at_the_default_threshold() {
    let d = tmp("big");
    let p = write(&d, "big.jpp", &program("1"));
    let o = jpp(&["run", &p, "--backend", "stub", "--profile", 付费画像]);
    assert_refused(&o);
    assert!(err(&o).contains("默认阈值"), "{}", err(&o));
    let o = jpp(&[
        "run",
        &p,
        "--backend",
        "stub",
        "--profile",
        付费画像,
        "--confirm",
    ]);
    assert!(o.status.success(), "{}", err(&o));
    assert_eq!(report_confirm(&o)["verdict"], "over");
}

#[test]
fn over_means_strictly_greater_the_threshold_itself_is_within() {
    let d = tmp("edge");
    let p = write(&d, "big.jpp", &program("1"));
    let o = jpp(&[
        "run",
        &p,
        "--backend",
        "stub",
        "--profile",
        付费画像,
        "--confirm-above",
        "1",
    ]);
    assert!(o.status.success(), "{}", err(&o));
    assert_eq!(report_confirm(&o)["verdict"], "within");
}

#[test]
fn threshold_zero_asks_for_confirmation_on_any_declared_spend() {
    let o = jpp(&[
        "run",
        "examples/sieve.jpp",
        "--backend",
        "stub",
        "--profile",
        付费画像,
        "--confirm-above",
        "0",
    ]);
    assert_refused(&o);
}

// ---------- 不核：固定观察、重放、单价未测 ----------

#[test]
fn fixed_observation_never_needs_confirm_and_the_report_is_unchanged() {
    // `partial.jpp` 声明 `budget cost 1`，换成付费画像就要确认；固定观察（夹具）不核
    let base = ["run", "examples/partial.jpp", "--fixtures", 夹具];
    let plain = jpp(&base);
    assert!(plain.status.success(), "{}", err(&plain));
    let r: Value = serde_json::from_str(&out(&plain)).unwrap();
    assert!(r.get("confirm").is_none(), "固定观察的报告没有 confirm 键");
    // 给了 --confirm-above 与 --confirm 也不改报告一个字节
    let mut args = base.to_vec();
    args.extend(["--confirm", "--confirm-above", "0.001"]);
    let with = jpp(&args);
    assert!(with.status.success(), "{}", err(&with));
    assert_eq!(out(&plain), out(&with));
}

#[test]
fn replay_never_needs_confirm() {
    let d = tmp("replay");
    let ledger = d.join("l.json");
    let first = jpp(&[
        "run",
        "examples/partial.jpp",
        "--fixtures",
        夹具,
        "--ledger-out",
        &ledger.display().to_string(),
    ]);
    assert!(first.status.success(), "{}", err(&first));
    // 重放不初始化后端；带付费画像与 stub 也不核（`budget cost 1` 本来要确认）
    let o = jpp(&[
        "run",
        "examples/partial.jpp",
        "--backend",
        "stub",
        "--profile",
        付费画像,
        "--replay",
        &ledger.display().to_string(),
    ]);
    assert!(o.status.success(), "{}", err(&o));
    let r: Value = serde_json::from_str(&out(&o)).unwrap();
    assert!(r.get("confirm").is_none(), "重放的报告没有 confirm 键");
}

#[test]
fn a_judge_profile_without_a_price_goes_ahead_and_says_unchecked() {
    let d = tmp("unpriced");
    let p = write(&d, "big.jpp", &program("999"));
    let o = jpp(&["run", &p, "--backend", "stub", "--profile", 无价画像]);
    assert!(o.status.success(), "{}", err(&o));
    let c = report_confirm(&o);
    assert_eq!(c["verdict"], "unchecked", "{c}");
    assert_eq!(c["upper_usd"], Value::Null);
    assert_eq!(c["upper_from"], Value::Null);
}

// ---------- 拒绝的形状：文本、--json、--explain ----------

#[test]
fn refusal_in_json_mode_is_one_json_line_on_stderr_and_stdout_is_empty() {
    let d = tmp("json");
    let p = write(&d, "big.jpp", &program("1"));
    let o = jpp(&[
        "run",
        &p,
        "--backend",
        "stub",
        "--profile",
        付费画像,
        "--json",
    ]);
    assert!(!o.status.success());
    assert!(out(&o).is_empty(), "{}", out(&o));
    // stderr 是 JSON 行；画像的 W-untested 等告警也在里面，找拒绝那一行
    let v: Value = err(&o)
        .lines()
        .filter(|l| l.starts_with('{'))
        .filter_map(|l| serde_json::from_str::<Value>(l).ok())
        .find(|v| v["code"] == "E-confirm-required")
        .unwrap_or_else(|| panic!("stderr 没有 E-confirm-required 的 JSON 行：{}", err(&o)));
    assert_eq!(v["code"], "E-confirm-required");
    assert_eq!(v["level"], "error");
    assert!(v["fix"].as_str().unwrap().contains("--confirm"), "{v}");
    assert!(v["explain"].is_string(), "宿主编号表里有说明：{v}");
}

#[test]
fn refused_run_with_explain_prints_the_plan_first_then_the_error() {
    let d = tmp("explain-refused");
    let p = write(&d, "big.jpp", &program("1"));
    let o = jpp(&[
        "run",
        &p,
        "--backend",
        "stub",
        "--profile",
        付费画像,
        "--explain",
    ]);
    assert!(!o.status.success());
    let e = err(&o);
    let plan = e.find("EXPLAIN").expect("有 EXPLAIN");
    let mark = e.find("超阈值").expect("EXPLAIN 里标出超阈值");
    let refuse = e.find("E-confirm-required").expect("有拒绝");
    assert!(
        plan < mark && mark < refuse,
        "顺序：EXPLAIN、标注、拒绝\n{e}"
    );
    assert!(e.contains("run 需要 --confirm"), "{e}");
}

#[test]
fn explain_marks_within_and_exempt_runs_too() {
    let o = jpp(&[
        "run",
        "examples/sieve.jpp",
        "--backend",
        "stub",
        "--profile",
        付费画像,
        "--explain",
    ]);
    assert!(o.status.success(), "{}", err(&o));
    assert!(err(&o).contains("未超：费用上界 0.01 美元"), "{}", err(&o));
    let o = jpp(&[
        "run",
        "examples/partial.jpp",
        "--fixtures",
        夹具,
        "--explain",
    ]);
    assert!(o.status.success(), "{}", err(&o));
    assert!(err(&o).contains("没有会花钱的端口"), "{}", err(&o));
}

#[test]
fn run_json_explain_confirm_equals_the_report_confirm() {
    let o = jpp(&[
        "run",
        "examples/sieve.jpp",
        "--backend",
        "stub",
        "--profile",
        付费画像,
        "--confirm-above",
        "0.001",
        "--confirm",
        "--json",
        "--explain",
    ]);
    assert!(o.status.success(), "{}", err(&o));
    let r: Value = serde_json::from_str(&out(&o)).unwrap();
    assert_eq!(r["explain"]["confirm"], r["confirm"], "同源");
    assert_eq!(r["confirm"]["verdict"], "over");
}

// ---------- 次序：计划期拒绝先于确认 ----------

/// 三层依赖链、`calls: 2`：可靠下界 3 次调用，首跑计划期被拒；`cost: 1` 又超默认阈值
const 三层链: &str = r#"budget {calls: 2, cost: 1};
let a = cut(judge(state(mat("甲")), test("行吗", "k")));
let b = cut(judge(state(mat(exit_kind(a))), test("行吗", "k")));
let c = cut(judge(state(mat(exit_kind(b))), test("行吗", "k")));
consume([a, b, c], "drop");
1
"#;

#[test]
fn a_plan_that_is_rejected_reports_the_plan_rejection_not_the_confirmation() {
    let d = tmp("order");
    let p = write(&d, "chain.jpp", 三层链);
    let base = ["run", &p, "--backend", "stub", "--profile", 付费画像];
    // 不带 --confirm 与带 --confirm 结局相同：都是计划期拒绝
    for extra in [&[][..], &["--confirm"][..]] {
        let mut args = base.to_vec();
        args.extend(extra);
        let o = jpp(&args);
        assert!(!o.status.success(), "{}", out(&o));
        assert!(out(&o).is_empty(), "{}", out(&o));
        let e = err(&o);
        assert!(e.contains("E-budget-plan"), "{extra:?}\n{e}");
        assert!(!e.contains("E-confirm-required"), "{extra:?}\n{e}");
    }
    // 加 --explain：EXPLAIN 里仍标出超阈值与计划拒绝，随后才是错误，仍没有 E-confirm-required
    let mut args = base.to_vec();
    args.push("--explain");
    let e = err(&jpp(&args));
    assert!(e.contains("超阈值"), "{e}");
    assert!(e.contains("拒绝：E-budget-plan"), "{e}");
    assert!(!e.contains("E-confirm-required"), "{e}");
}

// ---------- check --explain：只用默认阈值预览 ----------

#[test]
fn check_explain_previews_the_default_threshold_and_never_refuses() {
    let d = tmp("check");
    let p = write(&d, "big.jpp", &program("1"));
    let o = jpp(&["check", &p, "--explain", "--profile", 付费画像]);
    assert!(o.status.success(), "check 不拒：{}", err(&o));
    let t = out(&o);
    assert!(t.contains("确认阈值"), "{t}");
    assert!(t.contains("超阈值：费用上界 1 美元"), "{t}");
    assert!(t.contains("默认阈值"), "{t}");
}

#[test]
fn check_explain_without_a_profile_says_unchecked() {
    let o = jpp(&["check", "examples/sieve.jpp", "--explain"]);
    assert!(o.status.success(), "{}", err(&o));
    assert!(out(&o).contains("阈值未核"), "{}", out(&o));
}

#[test]
fn check_json_explain_has_the_confirm_key_in_one_document() {
    let o = jpp(&[
        "check",
        "examples/sieve.jpp",
        "--explain",
        "--json",
        "--profile",
        付费画像,
    ]);
    assert!(o.status.success(), "{}", err(&o));
    assert!(err(&o).is_empty(), "{}", err(&o));
    let v: Value = serde_json::from_str(&out(&o)).unwrap();
    assert_eq!(v["explain"]["confirm"]["verdict"], "within", "{v}");
    assert_eq!(v["explain"]["confirm"]["threshold_is_default"], true);
}

// ---------- 选项校验 ----------

#[test]
fn bad_confirm_options_are_usage_errors() {
    for argv in [
        vec!["run", "examples/sieve.jpp", "--confirm-above", "abc"],
        vec!["run", "examples/sieve.jpp", "--confirm-above", "-1"],
        vec!["run", "examples/sieve.jpp", "--confirm-above", "nan"],
        vec!["run", "examples/sieve.jpp", "--confirm-above"],
        vec!["run", "examples/sieve.jpp", "--confirm", "--confirm"],
        vec!["check", "examples/sieve.jpp", "--confirm"],
    ] {
        let o = jpp(&argv);
        assert_eq!(o.status.code(), Some(2), "{argv:?}\n{}", err(&o));
        assert!(err(&o).contains("Usage"), "{argv:?}");
    }
}

#[test]
fn help_lists_the_confirm_options() {
    let o = jpp(&[]);
    let t = format!("{}{}", out(&o), err(&o));
    assert!(t.contains("--confirm-above <usd>"), "{t}");
    assert!(t.contains("E-confirm-required"), "{t}");
}
