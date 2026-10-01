//! Z0160：题只经纯表达式依赖前面 `let e = cut(r)` 绑定的名字时，不得被提前登记到 `cut` 之前。
//!
//! 复现来自出题代理分支的 `derive_probe.rs` 测试 c；原因与修法见 `地基/过程记录/工程-Z0160-提前登记.md`。
//! 提前登记（`lift`）只该提**此刻算得出状态与题**的判断：依赖「被越过的那一句」绑定的名字的，
//! 一律不提，等它自己的真站点登记。

mod common;
use common::run_关 as run;
use std::cell::RefCell;

use jpp::effects::{CalibStore, EffectError, FnPort, JudgeResult, Ports};
use jpp::ledger::{Entry, Ledger};
use jpp::value::Answer;
use jpp::{ActionRegistry, lower, syntax::parse};

fn 端口<'a>(p: f64, 每次题数: &'a RefCell<Vec<usize>>) -> Ports<'a> {
    Ports::new()
        .with(FnPort::judge("fixed-0", move |_s, qs| {
            每次题数.borrow_mut().push(qs.len());
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

/// 返回（输出、账本、每次调用的题数、层数）
fn 跑(src: &str) -> (serde_json::Value, Ledger, Vec<usize>, usize) {
    let program = lower(&parse(src).expect("解析")).expect("lower");
    let calib = CalibStore::new();
    let mut ledger = Ledger::new();
    let 题数 = RefCell::new(vec![]);
    let out = run(
        &program,
        端口(0.55, &题数),
        &calib,
        &ActionRegistry::new(),
        &mut ledger,
    )
    .unwrap_or_else(|e| panic!("程序应当跑完：{}", e.render()));
    let v = out.value_json();
    let n = out.layers.len();
    let t = 题数.borrow().clone();
    (v, ledger, t, n)
}

fn 判断条数(l: &Ledger) -> usize {
    l.entries
        .iter()
        .filter(|e| matches!(e, Entry::Judge { .. }))
        .count()
}

/// 最小复现：后一道题的题面只经纯内置 `exit_kind` 依赖前面 `cut` 出的名字。
#[test]
fn 题面纯依赖前一个出口时不提前登记() {
    let src = r#"
budget {calls: 4, cost: 1, depth: 16};
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
    let (_v, ledger, 题数, 层数) = 跑(src);
    assert_eq!(判断条数(&ledger), 2);
    assert_eq!(
        题数,
        vec![1, 1],
        "依赖关系上后一道必须等前一道的出口：各自一次调用"
    );
    assert_eq!(层数, 2);
}

/// 选项记录 `{from: e}` 依赖出口（来源键，出题库的用法）。
#[test]
fn 选项记录里放出口时不提前登记() {
    let src = r#"
budget {calls: 4, cost: 1, depth: 16};
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
    let (_v, ledger, 题数, 层数) = 跑(src);
    assert_eq!(判断条数(&ledger), 2);
    assert_eq!(题数, vec![1, 1]);
    assert_eq!(层数, 2);
}

/// 依赖出口的那一道不提，不拖累后面互不依赖的同状态判断：它照旧随首个站点一起发，融合成一次调用。
#[test]
fn 依赖出口的一道不提_后面独立的同状态判断照提() {
    let src = r#"
budget {calls: 4, cost: 1, depth: 16};
let m = mat("一段话");
let r = judge(state(m), test("这段话语气积极吗", "probe"));
let e = cut(r);
let r2 = judge(state(m), test(exit_kind(e), "probe2"));
let r3 = judge(state(m), test("这段话有感谢吗", "probe3"));
let e2 = cut(r2);
let e3 = cut(r3);
let out = [exit_kind(e), exit_kind(e2), exit_kind(e3)];
consume(e3, "drop");
consume(e2, "drop");
consume(e, "drop");
out
"#;
    let (_v, ledger, 题数, 层数) = 跑(src);
    assert_eq!(判断条数(&ledger), 3);
    assert_eq!(题数, vec![2, 1], "r 与 r3 同层融合，r2 等 e");
    assert_eq!(层数, 2);
}
