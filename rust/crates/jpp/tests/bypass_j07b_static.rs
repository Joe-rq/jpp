//! J-07b 静态面（`20` §十 强制点表：`jpp-plan/passes/plan.rs` 可靠下界拒，B42）：端到端经 `Session`。
//!
//! 计划被拒时一次调用都不发、动作不执行；报文写明「按效应都成功计」与下界、预算两个数（主会话裁定
//! 2026-09-29 第十六条）。续接与开缓存不拒只告警；失败隐式往上传照拒，显式 `if is_fail(x)` 分支不算必经。
//!
//! 依据：`21` 步 22；B42；预注册 `地基/过程记录/工程-步22.md` §五·3。

// 伴随题「最缺哪类」是 K 选一：替身经 common::伴随中性judge 给伴随元题中性读数（Z0398 返修，过程记录 5.23）
mod common;
use std::cell::Cell;
use std::rc::Rc;

use jpp::effects::{CalibStore, JudgeResult, Ports};
use jpp::interp::JudgePrice;
use jpp::ledger::Ledger;
use jpp::store::CacheIndex;
use jpp::value::{Answer, Value};
use jpp::{ActionRegistry, EntryArgs, Error, Outcome, Session, TaintOut};
use jpp::{lower, syntax::parse};

/// 画像 `jev-1.13.0` 的单价（美元每 input token）
const 真机单价: f64 = 4.2e-8;

fn 端口<'a>(calls: &'a Cell<u64>) -> Ports<'a> {
    Ports::new().with(common::伴随中性judge("fixed-0", move |_s, qs| {
        calls.set(calls.get() + 1);
        Ok(JudgeResult {
            answers: qs.iter().map(|_| Answer::Noul(0.9)).collect(),
            tokens: 0,
            cost: 0.0,
            mode_share: qs.iter().map(|_| None).collect(),
            perms: qs.iter().map(|_| 0).collect(),
            confidence: vec![],
        })
    }))
}

fn 程序(src: &str) -> jpp::Program {
    lower(&parse(src).expect("解析")).expect("lower")
}

fn 跑(
    src: &str,
    price: JudgePrice,
    acts: &ActionRegistry,
    ledger: &mut Ledger,
    cache: Option<&CacheIndex>,
    calls: &Cell<u64>,
) -> Result<Outcome, Error> {
    let calib = CalibStore::new();
    let mut s = Session::new(端口(calls), &calib, acts).with_judge_price(price);
    if let Some(c) = cache {
        s = s.with_cache(c);
    }
    s.resume(&程序(src), &EntryArgs::default(), ledger)
}

fn 拒绝(r: Result<Outcome, Error>) -> String {
    match r {
        Err(Error::Runtime(e)) => {
            assert_eq!(e.rule.as_deref(), Some("E-budget-plan"), "{}", e.message);
            assert!(e.message.contains("按效应都成功计"), "{}", e.message);
            e.message
        }
        Err(e) => panic!("应当计划期拒绝，得到 {}", e.render()),
        Ok(o) => panic!("应当计划期拒绝，却跑完了：{}", o.value_json()),
    }
}

const 一题: &str = r#"budget {calls: 4, cost: 0};
let e = cut(judge(state(mat("甲")), test("行吗", "k")));
consume(e, "drop");
1
"#;

#[test]
fn 真机单价空账本cost零被拒零调用() {
    let calls = Cell::new(0);
    let m = 拒绝(跑(
        一题,
        JudgePrice::Known(真机单价),
        &ActionRegistry::new(),
        &mut Ledger::new(),
        None,
        &calls,
    ));
    assert!(
        m.contains("费用下界 4.2e-8 美元") && m.contains("预算 cost 0"),
        "{m}"
    );
    assert_eq!(calls.get(), 0, "被拒的计划一次调用都不发");
}

#[test]
fn 固定观察价格为零不拒() {
    let calls = Cell::new(0);
    let o = 跑(
        一题,
        JudgePrice::Known(0.0),
        &ActionRegistry::new(),
        &mut Ledger::new(),
        None,
        &calls,
    )
    .expect("固定观察下不变");
    assert_eq!(calls.get(), 1);
    assert!(!o.trace.warnings.iter().any(|w| w.starts_with("W-cost")));
}

#[test]
fn 续接账本非空不拒报w_cost() {
    let mut l = Ledger::new();
    let c1 = Cell::new(0);
    跑(
        一题,
        JudgePrice::Known(0.0),
        &ActionRegistry::new(),
        &mut l,
        None,
        &c1,
    )
    .expect("首跑");
    let c2 = Cell::new(0);
    let o = 跑(
        一题,
        JudgePrice::Known(真机单价),
        &ActionRegistry::new(),
        &mut l,
        None,
        &c2,
    )
    .expect("续接不拒");
    assert_eq!(c2.get(), 0, "账本命中，不再发");
    assert!(
        o.trace
            .warnings
            .iter()
            .any(|w| w.starts_with("W-cost:") && w.contains("按首跑计（效应都成功）")),
        "{:?}",
        o.trace.warnings
    );
}

#[test]
fn 开缓存不拒报w_cost() {
    let ix = CacheIndex::default();
    let calls = Cell::new(0);
    let o = 跑(
        一题,
        JudgePrice::Known(真机单价),
        &ActionRegistry::new(),
        &mut Ledger::new(),
        Some(&ix),
        &calls,
    )
    .expect("开缓存不拒");
    assert_eq!(calls.get(), 1);
    assert!(o.trace.warnings.iter().any(|w| w.starts_with("W-cost:")));
}

fn 取材动作(执行: Rc<Cell<u64>>) -> ActionRegistry {
    let mut acts = ActionRegistry::new();
    acts.register("取材", 0.0, true, TaintOut::Trusted, move |_| {
        执行.set(执行.get() + 1);
        Ok(Value::text("材料"))
    });
    acts
}

#[test]
fn 失败隐式往上传照拒() {
    let src = r#"budget {calls: 4, cost: 0};
let x = do("取材", ["a"], 0);
let e = cut(judge(state(mat(content(x))), test("行吗", "k")));
consume(e, "drop");
1
"#;
    let 执行 = Rc::new(Cell::new(0));
    let calls = Cell::new(0);
    let m = 拒绝(跑(
        src,
        JudgePrice::Known(真机单价),
        &取材动作(执行.clone()),
        &mut Ledger::new(),
        None,
        &calls,
    ));
    assert!(m.contains("费用下界"), "{m}");
    assert_eq!((执行.get(), calls.get()), (0, 0), "动作不执行、判断不发");
}

#[test]
fn 显式is_fail分支里的判断不算必经不拒() {
    let src = r#"budget {calls: 4, cost: 0};
let x = do("取材", ["a"], 0);
let v = if is_fail(x) { 0 } else {
    let e = cut(judge(state(mat(content(x))), test("行吗", "k")));
    consume(e, "drop");
    1
};
v
"#;
    let 执行 = Rc::new(Cell::new(0));
    let calls = Cell::new(0);
    跑(
        src,
        JudgePrice::Known(真机单价),
        &取材动作(执行.clone()),
        &mut Ledger::new(),
        None,
        &calls,
    )
    .expect("作者写了失败分支，判断不在必经路径上，不拒");
    assert_eq!(执行.get(), 1);
}

#[test]
fn 调用数下界超calls固定观察也拒() {
    let src = r#"budget {calls: 2, cost: 1};
let a = cut(judge(state(mat("甲")), test("行吗", "k")));
let b = cut(judge(state(mat(exit_kind(a))), test("行吗", "k")));
let c = cut(judge(state(mat(exit_kind(b))), test("行吗", "k")));
consume([a, b, c], "drop");
1
"#;
    let calls = Cell::new(0);
    let m = 拒绝(跑(
        src,
        JudgePrice::Known(0.0),
        &ActionRegistry::new(),
        &mut Ledger::new(),
        None,
        &calls,
    ));
    assert!(m.contains("调用数下界 3 次 > 预算 calls 2"), "{m}");
    assert_eq!(calls.get(), 0);
}
