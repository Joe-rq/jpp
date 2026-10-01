//! 步 28 K1：`fill(题式, 填法, {from: 出口})` 声明这道题由哪个出口派生：出口的账本键进题的 from_key，
//! 判断条目的 parents 含它、hop 加一；第三个参数只收 {from: 出口 | [出口…]}。

mod common;
use common::run_关 as run;
use jpp::effects::{CalibStore, EffectError, FnPort, JudgeResult, Ports};
use jpp::ledger::{Entry, Ledger};
use jpp::value::Answer;
use jpp::{ActionRegistry, lower, syntax::parse};

fn 跑(src: &str) -> Result<Ledger, String> {
    let program = lower(&parse(src).expect("解析")).expect("lower");
    let ports = Ports::new()
        .with(FnPort::judge("fixed-0", |_s, qs| {
            Ok::<_, EffectError>(JudgeResult {
                answers: qs.iter().map(|_| Answer::Noul(0.8)).collect(),
                tokens: 0,
                cost: 0.0,
                perms: vec![],
                mode_share: vec![],
                confidence: vec![],
            })
        }))
        .with(FnPort::generate("fixed-0", |_p, _c, _n, _r| {
            Err(EffectError("不该 gen".into()))
        }))
        .with(FnPort::ask("fixed-0", |_s, _q| {
            Err(EffectError("不该 ask".into()))
        }));
    let mut ledger = Ledger::new();
    run(
        &program,
        ports,
        &CalibStore::new(),
        &ActionRegistry::new(),
        &mut ledger,
    )
    .map(|_| ledger)
    .map_err(|e| e.render())
}

fn 派生题(from: &str) -> String {
    format!(
        r#"
budget {{calls: 2, cost: 1, depth: 16}};
let m = mat("a 与 b 的合作记录");
let f = form("test", "a 与 b 在{{城市}}有共同客户吗？", {{calib: "k2"}});
fn verdict(q) -> Exit {{ cut(judge(state(m), q)) }}
let e = verdict(test("a 与 b 有共同客户吗？", "k1"));
let e2 = verdict(fill(f, {{城市: "杭州"}}{from}));
let out = [exit_kind(e), exit_kind(e2)];
consume(e2, "drop");
consume(e, "drop");
out
"#
    )
}

fn 判断(l: &Ledger) -> Vec<(String, Vec<String>, u32)> {
    l.entries
        .iter()
        .filter_map(|e| match e {
            Entry::Judge {
                key, parents, hop, ..
            } => Some((key.clone(), parents.clone(), *hop)),
            _ => None,
        })
        .collect()
}

#[test]
fn a_带_from_的填题记来源出口_hop_加一() {
    let js = 判断(&跑(&派生题(", {from: e}")).unwrap());
    assert_eq!(js.len(), 2);
    assert!(js[1].1.contains(&js[0].0), "{js:?}");
    assert_eq!(js[1].2, 2);
}

#[test]
fn b_不带_from_与原来相同() {
    let js = 判断(&跑(&派生题("")).unwrap());
    assert_eq!(js[1].2, 1, "{js:?}");
    assert!(js[1].1.is_empty());
}

#[test]
fn c_第三个参数只收_from() {
    let err = 跑(&派生题(", {origin: e}")).unwrap_err();
    assert!(err.contains("只收 {from"), "{err}");
    let err = 跑(&派生题(", {from: \"e\"}")).unwrap_err();
    assert!(err.contains("只收 {from"), "{err}");
}
