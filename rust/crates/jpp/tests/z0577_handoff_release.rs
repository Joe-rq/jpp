//! Z0577（过程记录 5.27；复核 `复核-Z0494-G1.md` 第四节）：默认链末端的转交（路 C 等不放弃的原因）按 B162 在键上登记
//! 已解除，同一判断的其余视图随之解除——不再随返回值补记第二条 Handoff，也不被当成没人接（原为假 J-05，G2 起为假违规）。

mod common;
use jpp::effects::{CalibStore, FnPort, JudgeResult, Ports};
use jpp::ledger::{Entry, Ledger};
use jpp::value::Answer;
use jpp::{ActionRegistry, Outcome, lower, syntax::parse};

fn 跑(体: &str) -> (Outcome, Ledger) {
    let src = format!(
        "budget {{calls: 8, cost: 0, depth: 16}};\nunsure_source({{need: [\"过往项目\"]}});\nlet r = judge(state(mat(\"甲\")), test(\"未决吗\", \"k\"));\n{体}"
    );
    let program = lower(&parse(&src).expect("解析")).expect("lower");
    let ports = Ports::new().with(FnPort::judge("fixed-0", |_s, qs| {
        Ok(JudgeResult {
            answers: qs.iter().map(|_| Answer::Noul(0.5)).collect(),
            tokens: 0,
            cost: 0.0,
            mode_share: vec![],
            perms: vec![],
            confidence: vec![],
        })
    }));
    let mut l = Ledger::new();
    let o = common::run_关(
        &program,
        ports,
        &CalibStore::new(),
        &ActionRegistry::new(),
        &mut l,
    )
    .unwrap_or_else(|e| panic!("{}", e.render()));
    (o, l)
}

fn 转交数(l: &Ledger) -> usize {
    l.entries
        .iter()
        .filter(|e| matches!(e, Entry::Handoff { .. }))
        .count()
}

fn 告警数(o: &Outcome, 码: &str) -> usize {
    o.trace
        .warnings
        .iter()
        .filter(|w| w.starts_with(码))
        .count()
}

/// c8：别名 b 没被提到，a 交给缺 unsure 臂的 handle 走默认链、落路 C 转交；只记一条 Handoff
#[test]
fn c8_别名视图_一条转交() {
    let (o, l) = 跑(
        "let r2 = r;\nlet a = cut(r);\nlet b = cut(r2);\nlet v = handle(a, {act: fn() { 1 }, ignore: fn() { 0 }});\n{v: v}\n",
    );
    assert_eq!(转交数(&l), 1, "{:?}", l.entries);
    assert_eq!(告警数(&o, "W-duty-twice"), 0);
    assert!(o.violations.is_empty());
    assert_eq!(o.unsure_default[0]["end"], "handoff");
}

/// c11：同组两次 cut(r)，a 交给缺臂的 handle；同样一条
#[test]
fn c11_同组两次cut_一条转交() {
    let (o, l) = 跑(
        "let a = cut(r);\nlet b = cut(r);\nlet v = handle(a, {act: fn() { 1 }, ignore: fn() { 0 }});\n{v: v}\n",
    );
    assert_eq!(转交数(&l), 1, "{:?}", l.entries);
    assert!(o.violations.is_empty());
}

/// c12：c8 的结果换成 1。键已由 a 的路 C 转交，b 不再被当成没人接（原为假 J-05，G2 起为假违规）
#[test]
fn c12_结果不带值_不报违规() {
    let (o, l) = 跑(
        "let r2 = r;\nlet a = cut(r);\nlet b = cut(r2);\nlet v = handle(a, {act: fn() { 1 }, ignore: fn() { 0 }});\n1\n",
    );
    assert!(o.violations.is_empty(), "{:?}", o.violations);
    assert_eq!(转交数(&l), 1, "{:?}", l.entries);
}

/// 同组两处都没有作者去向、都走链且都落路 C：第一处转交并解除，第二处报 W-duty-twice、不再写
#[test]
fn 两处都走链落路c_第二处报duty_twice() {
    let (o, l) = 跑("let a = cut(r);\nlet b = cut(r);\n1\n");
    assert_eq!(转交数(&l), 1, "{:?}", l.entries);
    assert_eq!(告警数(&o, "W-duty-twice"), 1, "{:?}", o.trace.warnings);
    assert!(
        o.trace
            .warnings
            .iter()
            .any(|w| w.starts_with("W-duty-twice")
                && w.contains("同一判断的另一视图已由默认链交代过"))
    );
    assert!(o.violations.is_empty());
}
