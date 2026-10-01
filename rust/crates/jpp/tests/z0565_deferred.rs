//! Z0565：守卫下推迟动作的遗留（预注册 `地基/过程记录/工程-Z0565-推迟动作遗留.md` §二 R-1 至 R-4）。
//!
//! 依据：G2 复核七·5、七·6；主控板 Z0565 决定条；B200、B93、B38。

mod common;

use std::cell::Cell;
use std::rc::Rc;

use jpp::effects::{CalibStore, FnPort, JudgeResult, Ports};
use jpp::ledger::{Entry, Ledger};
use jpp::value::{Answer, Value};
use jpp::{ActionRegistry, Outcome, TaintOut, lower, run, run_replay, syntax::parse};

/// 是非题：「该发吗」0.9（放行），其余 0.5（带内，未决）
fn 端口<'a>(calls: &'a Cell<u64>) -> Ports<'a> {
    Ports::new().with(FnPort::judge("fixed-0", move |_s, qs| {
        calls.set(calls.get() + 1);
        Ok(JudgeResult {
            answers: qs
                .iter()
                .map(|q| Answer::Noul(if q.text.contains("该发") { 0.9 } else { 0.5 }))
                .collect(),
            tokens: 0,
            cost: 0.0,
            mode_share: qs.iter().map(|_| None).collect(),
            perms: qs.iter().map(|_| 0).collect(),
            confidence: vec![],
        })
    }))
}

fn 库() -> CalibStore {
    let mut c = CalibStore::new();
    for k in ["k", "k2"] {
        common::certified(&mut c, k, 0.75, 0.25, 100);
    }
    c
}

/// 不可逆动作「发邮件」：计执行次数；实参为 "坏" 时返回错误
fn 动作表(执行: &Rc<Cell<u32>>) -> ActionRegistry {
    let mut acts = ActionRegistry::new();
    let e2 = 执行.clone();
    acts.register("发邮件", 0.0, false, TaintOut::Trusted, move |args| {
        e2.set(e2.get() + 1);
        if matches!(args.first(), Some(Value::Text(t, _)) if t.as_ref() == "坏") {
            return Err("邮件服务拒收".into());
        }
        Ok(Value::text("已发"))
    });
    acts
}

fn 程序(src: &str) -> jpp::Program {
    let mut p = lower(&parse(src).expect("解析")).expect("lower");
    p.entry.guard = true;
    p
}

fn 跑(src: &str, 执行: &Rc<Cell<u32>>, l: &mut Ledger) -> Outcome {
    let calls = Cell::new(0);
    run(&程序(src), 端口(&calls), &库(), &动作表(执行), l)
        .unwrap_or_else(|e| panic!("{}", e.render()))
}

const 放行: &str = r#"let ok = handle(cut(judge(state(mat("程序自己写的材料")), test("该发吗", "k"))), {act: fn() { true }, ignore: fn() { false }, unsure: fn(u) { consume(u, "drop"); false }});
"#;

/// R-1、R-2：`budget.calls: 2`，判断用掉 1 次，两个不可逆 `do` 先后到达。修前两个都推迟、结算都执行（`calls` 3，
/// 超预算）；修后第二个到达时就停发，只执行一个，`calls` 为 2
#[test]
fn r2_推迟动作计入预算已用() {
    let src = format!(
        "budget {{calls: 2, cost: 0, depth: 16}};\n{放行}{{r: if ok {{ [do(\"发邮件\", [\"甲\"], 0), do(\"发邮件\", [\"乙\"], 0)] }} else {{ [] }}}}\n"
    );
    let 执行 = Rc::new(Cell::new(0));
    let mut l = Ledger::new();
    let o = 跑(&src, &执行, &mut l);
    println!(
        "执行 {}，cost.calls {}，值 {}",
        执行.get(),
        o.cost.calls,
        o.value_json()
    );
    assert_eq!(执行.get(), 1, "只执行一个");
    assert_eq!(o.cost.calls, 2, "判断 1 次 + 执行 1 次，不超预算");
    let v = o.value_json().to_string();
    assert!(v.contains("已发"), "{v}");
    assert!(v.contains("预算"), "第二个是预算失败值：{v}");
    let 意向 = l
        .entries
        .iter()
        .filter(|e| matches!(e, Entry::Intent { .. }))
        .count();
    assert_eq!(意向, 1, "停发的那个不写意向");
}

/// R-3：有违规时两个推迟动作都扣下，花费只有判断
#[test]
fn r3_扣下的动作不计花费() {
    let src = format!(
        "budget {{calls: 4, cost: 0, depth: 16}};\n{放行}let 欠 = cut(judge(state(mat(\"另一份\")), test(\"行吗\", \"k2\")));\n{{r: if ok {{ [do(\"发邮件\", [\"甲\"], 0), do(\"发邮件\", [\"乙\"], 0)] }} else {{ [] }}}}\n"
    );
    let 执行 = Rc::new(Cell::new(0));
    let o = 跑(&src, &执行, &mut Ledger::new());
    assert_eq!(o.violations.len(), 1);
    assert_eq!(执行.get(), 0);
    assert_eq!(o.cost.calls, 2, "两次判断，扣下的动作不计");
}

/// R-4：结算时动作失败：账本 Effect 记失败；W-settle-failed；Outcome.settle_failed 一项；不是违规；审计重放同样列出
#[test]
fn r4_结算后失败报告列出() {
    let src = format!(
        "budget {{calls: 4, cost: 0, depth: 16}};\n{放行}{{r: if ok {{ do(\"发邮件\", [\"坏\"], 0) }} else {{ \"不发\" }}}}\n"
    );
    let 执行 = Rc::new(Cell::new(0));
    let mut l = Ledger::new();
    let o = 跑(&src, &执行, &mut l);
    assert_eq!(执行.get(), 1);
    assert!(o.violations.is_empty());
    assert!(o.value_json()["r"].to_string().contains("邮件服务拒收"));
    assert_eq!(o.settle_failed.len(), 1, "{:?}", o.settle_failed);
    assert_eq!(o.settle_failed[0]["action"], "发邮件");
    assert!(
        o.trace
            .warnings
            .iter()
            .any(|w| w.starts_with("W-settle-failed")),
        "{:?}",
        o.trace.warnings
    );
    assert!(l.entries.iter().any(|e| matches!(e, Entry::Effect { .. })));
    // 审计重放：零执行，同样列出
    let calls = Cell::new(0);
    let o2 = run_replay(&程序(&src), 端口(&calls), &库(), &动作表(&执行), &mut l)
        .unwrap_or_else(|e| panic!("重放：{}", e.render()));
    assert_eq!(执行.get(), 1, "重放不执行");
    assert_eq!(o2.settle_failed, o.settle_failed);
    assert!(
        o2.trace
            .warnings
            .iter()
            .any(|w| w.starts_with("W-settle-failed"))
    );
    // 复核 Z0565 小项：同一账本非审计再跑，do 复用失败的记录，同样列出、同样告警，不重执行
    let o3 = 跑(&src, &执行, &mut l);
    assert_eq!(执行.get(), 1, "再跑复用记录，不重执行");
    assert_eq!(o3.settle_failed, o.settle_failed);
    assert!(
        o3.trace
            .warnings
            .iter()
            .any(|w| w.starts_with("W-settle-failed"))
    );
}
