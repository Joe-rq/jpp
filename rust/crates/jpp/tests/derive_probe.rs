//! 步 28（derive）开工前的两项核实（过程记录 `工程-步28.md` §7.4 第 1 步）。
//!
//! (a) 同一读数能否切两次：划分细化的「近边界」触发按主控答复 Q1 走「同一读数上再切一次作探测」
//!     （围绕多数边界的声明带），原出口照判断器的回答走。这里钉住：第二次 `cut` 能做、给出 band，
//!     第一次的出口不受影响。
//! (b) 派生题记来源出口：题的选项记录里放出口 `{from: e}`，分派处的来源合并（`eval.rs` 对
//!     `test`/`select`/`measure`/`fill`/`form` 并实参来源）是否已经把出口账本键并进 `from_key`，
//!     使下一条判断条目 `parents` 含来源键、`hop` 加一。成立则 K1 不改内核。

mod common;
use common::run_关 as run;
use jpp::effects::{CalibStore, EffectError, FnPort, JudgeResult, Ports};
use jpp::ledger::{Entry, Ledger};
use jpp::value::Answer;
use jpp::{ActionRegistry, lower, syntax::parse};

fn 端口(p: f64) -> Ports<'static> {
    Ports::new()
        .with(FnPort::judge("fixed-0", move |_s, qs| {
            Ok(JudgeResult {
                answers: qs.iter().map(|_| Answer::Noul(p)).collect(),
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
        }))
}

fn 跑(src: &str, p: f64) -> (serde_json::Value, Ledger) {
    let program = lower(&parse(src).expect("解析")).expect("lower");
    let calib = CalibStore::new();
    let mut ledger = Ledger::new();
    let out = run(
        &program,
        端口(p),
        &calib,
        &ActionRegistry::new(),
        &mut ledger,
    )
    .unwrap_or_else(|e| panic!("程序应当跑完：{}", e.render()));
    (out.value_json(), ledger)
}

#[test]
fn a_同一读数切两次_原出口照回答_探测给_band() {
    let src = r#"
budget {calls: 2, cost: 1, depth: 16};
let m = mat("一段话");
let r = judge(state(m), test("这段话语气积极吗", "probe"));
let e = cut(r);
let probe = cut(r, {declare: {hi: 0.6, lo: 0.4}});
let out = [exit_kind(e), exit_kind(probe)];
consume(probe, "drop");
consume(e, "drop");
out
"#;
    let (v, ledger) = 跑(src, 0.55);
    let 判断数 = ledger
        .entries
        .iter()
        .filter(|e| matches!(e, Entry::Judge { .. }))
        .count();
    assert_eq!(判断数, 1, "两次 cut 只读同一条读数，不另发判断");
    let ks: Vec<String> = serde_json::from_value(v.clone()).expect("两个出口种类");
    assert_eq!(ks[0], "act", "没有线时原出口按多数块走：{v}");
    assert!(
        ks[1].starts_with("unsure") && ks[1].contains("band"),
        "探测切给 band：{v}"
    );
}

#[test]
fn b_选项里放出口_派生题带来源键_跳数加一() {
    let src = r#"
budget {calls: 2, cost: 1, depth: 16};
let m = mat("一段话");
let r = judge(state(m), test("这段话语气积极吗", "probe"));
let e = cut(r);
let r2 = judge(state(m), test("这段话里有感谢的话吗", "probe2", {from: e}));
let e2 = cut(r2);
let out = [exit_kind(e), exit_kind(e2)];
consume(e2, "drop");
consume(e, "drop");
out
"#;
    let (v, ledger) = 跑(src, 0.55);
    let js: Vec<(String, Vec<String>, u32)> = ledger
        .entries
        .iter()
        .filter_map(|e| match e {
            Entry::Judge {
                key, parents, hop, ..
            } => Some((key.clone(), parents.clone(), *hop)),
            _ => None,
        })
        .collect();
    assert_eq!(js.len(), 2, "两道题两条判断条目：{v}");
    assert_eq!(js[0].2, 1, "第一跳 hop 1");
    assert!(
        js[1].1.contains(&js[0].0),
        "派生题的 parents 含来源出口的账本键：{:?}",
        js
    );
    assert_eq!(js[1].2, 2, "派生题 hop 2");
}

/// (c) 最小复现（不涉及 `from`）：后一道题的题面只经纯内置 `exit_kind` 依赖前面 `cut` 出的名字。
/// 这道 `judge` 若被提前登记到 `let e = cut(r)` 之前，报 `E-rt-name`（Z0160，已在 jpp-plan lift 修好）。钉住应有行为。
#[test]
fn c_题面纯依赖前一个出口时不得提前登记() {
    let src = r#"
budget {calls: 2, cost: 1, depth: 16};
let m = mat("一段话");
let r = judge(state(m), test("这段话语气积极吗", "probe"));
let e = cut(r);
let r2 = judge(state(m), test(exit_kind(e), "probe2"));
let e2 = cut(r2);
let out = [exit_kind(e), exit_kind(e2)];
consume(e2, "drop");
consume(e, "drop");
out
"#;
    let (_v, ledger) = 跑(src, 0.55);
    let n = ledger
        .entries
        .iter()
        .filter(|e| matches!(e, Entry::Judge { .. }))
        .count();
    assert_eq!(n, 2);
}
