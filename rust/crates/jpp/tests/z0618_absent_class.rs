//! Z0618（过程记录 5.31）：缺席类判定统一用 `UnsureCause::is_absent_class()`，`deadline` 与 absent、budget、latency、
//! depth 同样不能 drop；走默认链时随值走（Z0602），被丢掉记违规。

mod common;
use jpp::effects::{CalibStore, Ports};
use jpp::ledger::Ledger;
use jpp::{ActionRegistry, Outcome, lower, syntax::parse};

fn 跑(体: &str) -> Result<Outcome, String> {
    let src = format!("budget {{calls: 0, cost: 0, depth: 16}};\n{体}");
    let program = lower(&parse(&src).expect("解析")).expect("lower");
    common::run_关(
        &program,
        Ports::new(),
        &CalibStore::new(),
        &ActionRegistry::new(),
        &mut Ledger::new(),
    )
    .map_err(|e| e.render())
}

#[test]
fn deadline出口不能drop() {
    let e = 跑("let u = unsure(\"deadline\");\nconsume(u, \"drop\");\n1\n")
        .err()
        .expect("E-drop-unobserved");
    assert!(e.contains("E-drop-unobserved"), "{e}");
    // 对照：判过而拿不准的可以放弃
    跑("let u = unsure(\"tie\");\nconsume(u, \"drop\");\n1\n")
        .unwrap_or_else(|e| panic!("tie 可以 drop：{e}"));
}

#[test]
fn deadline走默认链随值走_被丢掉记违规() {
    let o = 跑("let u = unsure(\"deadline\");\nlet v = handle(u, {act: fn() { 1 }, ignore: fn() { 0 }});\n1\n").unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(
        o.unsure_default[0]["end"], "carry",
        "{:?}",
        o.unsure_default
    );
    assert_eq!(o.violations.len(), 1, "{:?}", o.violations);
    let o = 跑("let u = unsure(\"deadline\");\nlet v = handle(u, {act: fn() { 1 }, ignore: fn() { 0 }});\n{v: v}\n").unwrap_or_else(|e| panic!("{e}"));
    assert!(
        o.violations.is_empty(),
        "随值返回不记违规：{:?}",
        o.violations
    );
}
