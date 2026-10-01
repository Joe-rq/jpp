//! Z0157：`jpp check --profile` 的时延拒绝只看可靠下界（B42）。
//! Z0157: `jpp check --profile` rejects on the reliable latency lower bound only (B42).
//!
//! 上界超预算、下界不超 → 只报 `W-latency`，退出 0；必经判断的下界超预算 → `E-budget-plan`（规划器），
//! 退出非 0。画像 p95 = 1.026s。
use std::{fs, path::PathBuf, process::Command};

fn 画像() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../profiles/jev-1.13.0.json")
}

/// 在临时目录里写 `p.jpp` 并 `check --profile`，返回（退出成功，stderr）
fn check(name: &str, src: &str) -> (bool, String) {
    let d = std::env::temp_dir().join(format!("jpp-z0157-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&d);
    fs::create_dir_all(&d).unwrap();
    fs::write(d.join("p.jpp"), src).unwrap();
    let p = 画像();
    let out = Command::new(env!("CARGO_BIN_EXE_jpp"))
        .current_dir(&d)
        .env("HOME", &d)
        .args(["check", "p.jpp", "--profile", p.to_str().unwrap()])
        .output()
        .unwrap();
    let _ = fs::remove_dir_all(&d);
    (
        out.status.success(),
        String::from_utf8_lossy(&out.stderr).into(),
    )
}

const 处理: &str =
    r#"{act: fn() { 1 }, ignore: fn() { 0 }, unsure: fn(u) { consume(u, "drop"); 2 }}"#;

fn 程序(预算: &str, 体: &str) -> String {
    format!("budget {{calls: 8, cost: 1, latency_p95: {预算}}};\n{体}\n")
}

fn 独立两个() -> String {
    format!(
        r#"let a = cut(judge(state(mat("甲")), test("行吗", "k1")));
let b = cut(judge(state(mat("乙")), test("行吗", "k2")));
handle(a, {处理});
handle(b, {处理})"#
    )
}

fn 分支两个() -> String {
    format!(
        r#"let flag = 1;
if flag == 1 {{
  handle(cut(judge(state(mat("甲")), test("行吗", "k1"))), {处理})
}} else {{
  handle(cut(judge(state(mat("乙")), test("行吗", "k2"))), {处理})
}}"#
    )
}

/// 上界面：两个互相独立的判断，预算 1.5s，上界 2.05s、下界 1.026s → 只告警。
#[test]
fn 独立两判断_只告警不拒() {
    let (ok, err) = check("indep", &程序("1.5", &独立两个()));
    assert!(ok, "{err}");
    assert!(err.contains("W-latency"), "{err}");
    assert!(
        !err.contains("E-latency") && !err.contains("E-budget-plan"),
        "{err}"
    );
}

/// 上界面：`if` 两支各一个判断，一次只走一支 → 不拒。
#[test]
fn if两支各一判断_不拒() {
    let (ok, err) = check("branch", &程序("1.5", &分支两个()));
    assert!(ok, "{err}");
    assert!(
        !err.contains("E-latency") && !err.contains("E-budget-plan"),
        "{err}"
    );
}

/// 下界面：必经判断，一层 p95 1.026s 超预算 0.5s → `E-budget-plan`，无 `E-latency`。
#[test]
fn 必经判断下界超预算_报规划拒绝() {
    let (ok, err) = check("floor", &程序("0.5", &独立两个()));
    assert!(!ok, "{err}");
    assert!(err.contains("E-budget-plan"), "{err}");
    assert!(err.contains("按效应都成功计"), "{err}");
    assert!(!err.contains("E-latency"), "{err}");
}

/// 非必经：只在 `if` 一支里有判断，预算 0.5s → 不拒（下界 0）。
#[test]
fn 只在分支里的判断_不算必经不拒() {
    let src = format!(
        r#"let flag = 1;
if flag == 1 {{
  handle(cut(judge(state(mat("甲")), test("行吗", "k1"))), {处理})
}} else {{
  0
}}"#
    );
    let (ok, err) = check("optional", &程序("0.5", &src));
    assert!(ok, "{err}");
    assert!(err.contains("W-latency"), "{err}");
    assert!(
        !err.contains("E-budget-plan") && !err.contains("E-latency"),
        "{err}"
    );
}

/// 循环面：顶层一个判断加 `map` 体内一个判断，预算 1.5s，只告警不拒（与改前相同）。
#[test]
fn 顶层加map体内_只告警() {
    let src = format!(
        r#"let top = handle(cut(judge(state(mat("甲")), test("行吗", "k1"))), {处理});
let xs = map([1, 2], fn(i) {{ handle(cut(judge(state(mat("乙")), test("行吗", "k2"))), {处理}) }});
top"#
    );
    let (ok, err) = check("map", &程序("1.5", &src));
    assert!(ok, "{err}");
    assert!(err.contains("W-latency"), "{err}");
    assert!(
        !err.contains("E-budget-plan") && !err.contains("E-latency"),
        "{err}"
    );
}
