//! 未决值传播（主控板 B0492 S3；J-05 草案第三稿 (5a)）：结果依赖未决值内容的运算，结果就是那个未决值。
//!
//! 未决值 `x` 由默认链产生：无线的判断读数恰为 0.5（并列，B187），`handle` 没写 unsure 臂、程序没声明取材料来源，
//! 默认链记账放弃，`handle` 的值是这份责任。用例编号对应过程记录 `工程-未决去向.md` 5.4 的表。

// 伴随题「最缺哪类」是 K 选一：替身经 common::伴随中性judge 给伴随元题中性读数（Z0398 返修，过程记录 5.23）
mod common;
use jpp::effects::{CalibStore, FnPort, JudgeResult, Ports};
use jpp::interp::TaintOut;
use jpp::ledger::{Entry, Ledger};
use jpp::value::{Answer, Value};
use jpp::{ActionRegistry, Outcome, lower, run, syntax::parse};
use serde_json::{Value as Json, json};
use std::cell::{Cell, RefCell};
use std::rc::Rc;

fn 端口<'a>(calls: &'a RefCell<u64>) -> Ports<'a> {
    Ports::new().with(common::伴随中性judge("fixed-0", move |_s, qs| {
        *calls.borrow_mut() += 1;
        Ok(JudgeResult {
            answers: qs.iter().map(|_| Answer::Noul(0.5)).collect(),
            tokens: 0,
            cost: 0.0,
            mode_share: vec![],
            perms: vec![],
            confidence: vec![],
        })
    }))
}

const 前言: &str = r#"budget {calls: 8, cost: 0, depth: 16};
let x = handle(cut(judge(state(mat("甲")), test("甲方合适吗？", "k"))), {act: fn() { 1 }, ignore: fn() { 0 }});
let y = handle(cut(judge(state(mat("乙")), test("乙方合适吗？", "k"))), {act: fn() { 1 }, ignore: fn() { 0 }});
"#;

struct 跑出 {
    outcome: Outcome,
    ledger: Ledger,
    calls: u64,
    记录: u64,
}

fn 跑_用(src: &str, guard: bool) -> 跑出 {
    let mut program = lower(&parse(src).expect("解析")).expect("lower");
    program.entry.guard = guard;
    let calls = RefCell::new(0);
    let 记录次数 = Rc::new(Cell::new(0u64));
    let n = 记录次数.clone();
    let mut a = ActionRegistry::new();
    a.register(
        "记录",
        0.0,
        true,
        TaintOut::Trusted,
        move |_: &[Value]| {
            n.set(n.get() + 1);
            Ok(Value::Unit)
        },
    );
    let mut ledger = Ledger::new();
    let outcome = run(&program, 端口(&calls), &CalibStore::new(), &a, &mut ledger)
        .unwrap_or_else(|e| panic!("运行失败：{}", e.render()));
    let calls = *calls.borrow();
    跑出 {
        outcome,
        ledger,
        calls,
        记录: 记录次数.get(),
    }
}

/// 在前言之后求值 `{x: x, y: y, r: 表达式}`
fn 求(表达式: &str) -> (Json, 跑出) {
    let r = 跑_用(&format!("{前言}{{x: x, y: y, r: {表达式}}}\n"), false);
    (r.outcome.value_json(), r)
}

fn 是x(v: &Json) {
    assert_eq!(v["r"], v["x"], "结果应是 x 本身：{v}");
    assert_eq!(v["x"]["unsure"], "tie", "{v}");
}

#[test]
fn 用例1_比较算术拼接一元() {
    for e in [
        "x == \"合作\"",
        "x + 1",
        "\"前缀\" + x",
        "-x",
        "x != y && false",
    ] {
        let (v, _) = 求(e);
        if e.ends_with("false") {
            assert_eq!(v["r"], json!(false), "{e}：{v}");
        } else {
            是x(&v);
        }
    }
}

#[test]
fn 用例2_取字段与下标() {
    for e in ["x.字段", "x[0]", "[1, 2][x]"] {
        let (v, _) = 求(e);
        是x(&v);
    }
}

#[test]
fn 用例3_if条件未决两支都不执行() {
    let (_, 基线) = 求("1");
    let (v, r) = 求(
        r#"if x { judge(state(mat("丙")), test("丙呢？", "k")) } else { judge(state(mat("丁")), test("丁呢？", "k")) }"#,
    );
    是x(&v);
    assert_eq!(r.calls, 基线.calls, "两支都没执行，判断调用数不增加");
}

#[test]
fn 用例4_强kleene() {
    for (e, want) in [
        ("false && x", json!(false)),
        ("x && false", json!(false)),
        ("x || true", json!(true)),
        ("true || x", json!(true)),
    ] {
        let (v, _) = 求(e);
        assert_eq!(v["r"], want, "{e}：{v}");
    }
    for e in ["true && x", "false || x", "x || false", "x && true"] {
        let (v, _) = 求(e);
        是x(&v);
    }
}

#[test]
fn 用例5_按值计算的内置() {
    for e in [
        "text(x)",
        "join([x, \"a\"], \"\")",
        "sum([1, x])",
        "contains([x], 1)",
    ] {
        let (v, _) = 求(e);
        是x(&v);
    }
}

fn 跳过(l: &Ledger) -> Vec<(Vec<String>, String)> {
    l.entries
        .iter()
        .filter_map(|e| match e {
            Entry::Skip { of, kind, .. } => Some((of.clone(), kind.clone())),
            _ => None,
        })
        .collect()
}

#[test]
fn 用例6_判断的参数未决_不发出() {
    let (_, 基线) = 求("1");
    let (v, r) = 求(r#"cut(judge(state(mat(x)), test("丙呢？", "k")))"#);
    是x(&v);
    assert_eq!(r.calls, 基线.calls, "judge 没发出");
    let s = 跳过(&r.ledger);
    assert_eq!(s.len(), 1, "{s:?}");
    assert_eq!(s[0].1, "judge");
    assert_eq!(s[0].0.len(), 1);
}

#[test]
fn 用例7_do的参数未决_不发出() {
    let (v, r) = 求(r#"do("记录", [x], 0)"#);
    是x(&v);
    assert_eq!(r.记录, 0, "动作没执行");
    let s = 跳过(&r.ledger);
    assert_eq!(
        s.iter().map(|x| x.1.as_str()).collect::<Vec<_>>(),
        vec!["do"]
    );
}

#[test]
fn 用例8_filter谓词未决() {
    let (v, _) = 求("filter([1, 2], fn(i) { x })");
    是x(&v);
}

#[test]
fn 用例9_容器照放() {
    let (v, _) = 求(r#"[{a: x}, [x], with({}, "k", x)]"#);
    assert_eq!(v["r"][0]["a"], v["x"]);
    assert_eq!(v["r"][1][0], v["x"]);
    assert_eq!(v["r"][2]["k"], v["x"]);
}

#[test]
fn 用例10_两个未决值合并() {
    let (v, _) = 求("x == y");
    assert_eq!(v["r"]["unsure"], "tie", "原因相同取之：{v}");
    assert_ne!(v["r"]["duty"], v["x"]["duty"]);
    assert_ne!(v["r"]["duty"], v["y"]["duty"]);
}

const 作者臂: &str = r#"budget {calls: 8, cost: 0, depth: 16};
let e = cut(judge(state(mat("甲")), test("甲方合适吗？", "k")));
handle(e, {act: fn() { {v: 1} }, ignore: fn() { {v: 0} }, unsure: fn(u) { {v: u == 1, exit: u} }})
"#;

fn 用例11断言(r: &跑出) {
    let v = r.outcome.value_json();
    assert_eq!(v["v"], v["exit"], "u == 1 就是 u：{v}");
    let hs: Vec<&Entry> = r
        .ledger
        .entries
        .iter()
        .filter(|e| matches!(e, Entry::Handoff { .. }))
        .collect();
    assert_eq!(hs.len(), 1, "{hs:?}");
    assert_eq!(r.outcome.returned_unsure.len(), 1);
}

#[test]
fn 用例11_unsure臂里的u参与运算() {
    用例11断言(&跑_用(作者臂, false));
}

#[test]
fn 用例12_guard下相同() {
    用例11断言(&跑_用(作者臂, true));
}

/// 预注册表里「交给 handle 照常」一行的直接断言（表外补的一条）：未决值交给 handle，走作者的 unsure 臂
#[test]
fn 交给handle照常() {
    let (v, _) =
        求("handle(x, {act: fn() { 1 }, ignore: fn() { 0 }, unsure: fn(u) { {exit: u} }})");
    assert_eq!(v["r"]["exit"], v["x"], "{v}");
}

/// 主控复核 2026-09-30：`to_json(未决值)` 仍是那个未决值，接着进 mat、judge 时记 Skip、不发出
#[test]
fn to_json_不把未决值洗成文本() {
    let (v, _) = 求("to_json(x)");
    是x(&v);
    let (_, 基线) = 求("1");
    let (v, r) = 求(r#"cut(judge(state(mat(to_json(x))), test("丙呢？", "k")))"#);
    是x(&v);
    assert_eq!(r.calls, 基线.calls, "judge 没发出");
    assert!(
        r.ledger
            .entries
            .iter()
            .any(|e| matches!(e, Entry::Skip { .. }))
    );
}
