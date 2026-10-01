//! G5（步 38）：缺席可以再问（预注册 `地基/过程记录/工程-G5-缺席可再问.md` §二·2 A-2、A-3、A-5；A-1、A-4 在
//! `field_stability.rs::a`）。
//!
//! 依据：裁定五十九第 16 条（缺席不是答案，后续刷新可以再问同一内容键，账本记两次）；裁定六十一主控暂定 (b)
//! （续跑与后续刷新都重发缺席的题，PR #48 的读法作废）；`12` §2.13 R1、B32。

use std::cell::RefCell;

use jpp::effects::{CalibStore, EffectError, FnPort, JudgeResult, Ports};
use jpp::ledger::{Entry, Ledger};
use jpp::value::Answer;
use jpp::{ActionRegistry, lower, run, run_replay, syntax::parse};
use serde_json::json;

/// 前 `坏` 次调用失败（判断器缺席），之后答 0.9
fn 时好时坏(坏: u64, calls: &RefCell<u64>) -> Ports<'_> {
    Ports::new().with(FnPort::judge("fixed-0", move |_s, qs| {
        *calls.borrow_mut() += 1;
        if *calls.borrow() <= 坏 {
            return Err(EffectError("连接中断".into()));
        }
        Ok(JudgeResult {
            answers: qs.iter().map(|_| Answer::Noul(0.9)).collect(),
            tokens: 0,
            cost: 0.0,
            mode_share: vec![],
            perms: vec![],
            confidence: vec![],
        })
    }))
}

fn 线() -> CalibStore {
    let mut c = CalibStore::new();
    c.put("k", 0.65, 0.35, 50, "上岗", Some(0.05)).unwrap();
    c
}

/// 同一个站点（函数里）问两次同一道题：同一内容键
fn 程序(breaker: u32) -> String {
    format!(
        r#"budget {{calls: 10, cost: 0, depth: 8, absent: {{retry: 0, backoff: 0, then: "conservative", breaker: {breaker}}}}};
fn 判() {{
    let e = cut(judge(state(mat("材料")), test("行吗", "k")));
    handle(e, {{act: fn() {{ "act" }}, ignore: fn() {{ "ignore" }}, unsure: fn(u) {{ {{c: unsure_cause(u), exit: u}} }}}})
}}
let a = 判();
let b = 判();
{{a: a, b: b}}
"#
    )
}

fn 条目(l: &Ledger) -> (usize, usize) {
    let 缺 = l
        .entries
        .iter()
        .filter(|e| matches!(e, Entry::Absent { .. }))
        .count();
    let 答 = l
        .entries
        .iter()
        .filter(|e| matches!(e, Entry::Judge { .. }))
        .count();
    (缺, 答)
}

/// A-2：同一趟里第一次缺席，后面再问同一内容键：发出、答到；先前那个读数仍是缺席；账本一条缺席、一条答案
/// A-3：这份账本审计重放零调用、两处出口与首跑相同
#[test]
fn a2_a3_同一趟再问_账本记两次_重放按趟复现() {
    let src = 程序(5);
    let program = lower(&parse(&src).expect("解析")).expect("lower");
    let calls = RefCell::new(0);
    let mut l = Ledger::new();
    let o = run(
        &program,
        时好时坏(1, &calls),
        &线(),
        &ActionRegistry::new(),
        &mut l,
    )
    .unwrap_or_else(|e| panic!("{}", e.render()));
    let v = o.value_json();
    assert_eq!(v["a"]["c"], json!("absent"), "第一次缺席：{v}");
    assert_eq!(v["b"], json!("act"), "再问答到：{v}");
    assert_eq!(*calls.borrow(), 2, "缺席一次、再问一次");
    assert_eq!(条目(&l), (1, 1), "账本记两次：一次缺席、一次答案");
    // A-3：审计重放
    let c2 = RefCell::new(0);
    let o2 = run_replay(
        &program,
        时好时坏(0, &c2),
        &线(),
        &ActionRegistry::new(),
        &mut l,
    )
    .unwrap_or_else(|e| panic!("重放：{}", e.render()));
    assert_eq!(*c2.borrow(), 0, "重放零调用");
    assert_eq!(o2.value_json()["a"]["c"], json!("absent"));
    assert_eq!(o2.value_json()["b"], json!("act"));
}

/// A-5：熔断打开后同一趟的再问不发（B32 照旧）
#[test]
fn a5_熔断打开后不再问() {
    let src = 程序(1);
    let program = lower(&parse(&src).expect("解析")).expect("lower");
    let calls = RefCell::new(0);
    let mut l = Ledger::new();
    let o = run(
        &program,
        时好时坏(1, &calls),
        &线(),
        &ActionRegistry::new(),
        &mut l,
    )
    .unwrap_or_else(|e| panic!("{}", e.render()));
    let v = o.value_json();
    assert_eq!(v["a"]["c"], json!("absent"));
    assert_eq!(*calls.borrow(), 1, "熔断打开，第二次不发：{v}");
    assert!(v["b"]["c"].is_string(), "第二次仍是未决：{v}");
}

/// 缺席记录的键，按账本顺序
fn 缺席键(l: &Ledger) -> Vec<String> {
    l.entries
        .iter()
        .filter_map(|e| match e {
            Entry::Absent { key, .. } => Some(key.clone()),
            _ => None,
        })
        .collect()
}

/// 附录一 N-1（原「已知缺口」测试翻过来，主控 Z0573）：同一趟里同一内容键缺席两次、之后才答到。第二次缺席记
/// `absent:<k>#2`；审计重放零调用，a、b 缺席、c 答到，与首跑相同
#[test]
fn n1_同一趟两次缺席后答到_重放与首跑一致() {
    let src = r#"budget {calls: 10, cost: 0, depth: 8, absent: {retry: 0, backoff: 0, then: "conservative", breaker: 5}};
fn 判() {
    let e = cut(judge(state(mat("材料")), test("行吗", "k")));
    handle(e, {act: fn() { "act" }, ignore: fn() { "ignore" }, unsure: fn(u) { {c: unsure_cause(u), exit: u} }})
}
let a = 判();
let b = 判();
let c = 判();
{a: a, b: b, c: c}
"#;
    let program = lower(&parse(src).expect("解析")).expect("lower");
    let calls = RefCell::new(0);
    let mut l = Ledger::new();
    let o = run(
        &program,
        时好时坏(2, &calls),
        &线(),
        &ActionRegistry::new(),
        &mut l,
    )
    .unwrap_or_else(|e| panic!("{}", e.render()));
    let v = o.value_json();
    assert_eq!(v["a"]["c"], json!("absent"), "{v}");
    assert_eq!(v["b"]["c"], json!("absent"), "{v}");
    assert_eq!(v["c"], json!("act"), "{v}");
    let 键 = 缺席键(&l);
    assert_eq!(键.len(), 2, "{键:?}");
    assert!(
        !键[0].contains('#') && 键[1] == format!("{}#2", 键[0]),
        "{键:?}"
    );
    assert_eq!(条目(&l), (2, 1));
    let n0 = l.entries.len();
    let c2 = RefCell::new(0);
    let o2 = run_replay(
        &program,
        时好时坏(0, &c2),
        &线(),
        &ActionRegistry::new(),
        &mut l,
    )
    .unwrap_or_else(|e| panic!("重放：{}", e.render()));
    assert_eq!(*c2.borrow(), 0, "重放零调用");
    assert_eq!(o2.value_json(), v, "重放与首跑一致");
    assert_eq!(l.entries.len(), n0, "审计重放不写新的缺席记录");
}

/// 附录一 N-2：续跑时判断器仍然缺席，记 `absent:<k>#2`（续跑那一趟）；这份账本审计重放零调用、出口与续跑相同
#[test]
fn n2_续跑仍缺席记下一个序号_重放复现续跑() {
    let src = r#"budget {calls: 10, cost: 0, depth: 8, absent: {retry: 0, backoff: 0, then: "conservative", breaker: 5}};
let e = cut(judge(state(mat("材料")), test("行吗", "k")));
handle(e, {act: fn() { "act" }, ignore: fn() { "ignore" }, unsure: fn(u) { {c: unsure_cause(u), exit: u} }})
"#;
    let program = lower(&parse(src).expect("解析")).expect("lower");
    let mut l = Ledger::new();
    for 趟 in 0..2 {
        let calls = RefCell::new(0);
        let o = run(
            &program,
            时好时坏(9, &calls),
            &线(),
            &ActionRegistry::new(),
            &mut l,
        )
        .unwrap_or_else(|e| panic!("{}", e.render()));
        assert_eq!(o.value_json()["c"], json!("absent"), "第 {趟} 趟");
        assert_eq!(*calls.borrow(), 1, "第 {趟} 趟发 1 次（续跑重发）");
    }
    let 键 = 缺席键(&l);
    assert_eq!(键.len(), 2, "{键:?}");
    assert_eq!(键[1], format!("{}#2", 键[0]));
    let c2 = RefCell::new(0);
    let o2 = run_replay(
        &program,
        时好时坏(0, &c2),
        &线(),
        &ActionRegistry::new(),
        &mut l,
    )
    .unwrap_or_else(|e| panic!("重放：{}", e.render()));
    assert_eq!(*c2.borrow(), 0);
    assert_eq!(o2.value_json()["c"], json!("absent"));
}

// ───────────── 附录二（复核修补，Z0580）─────────────

/// `坏` 中列出的那几次调用失败（判断器缺席），其余答 0.9
fn 按次失败<'a>(坏: &'a [u64], calls: &'a RefCell<u64>) -> Ports<'a> {
    Ports::new().with(FnPort::judge("fixed-0", move |_s, qs| {
        *calls.borrow_mut() += 1;
        if 坏.contains(&*calls.borrow()) {
            return Err(EffectError("连接中断".into()));
        }
        Ok(JudgeResult {
            answers: qs.iter().map(|_| Answer::Noul(0.9)).collect(),
            tokens: 0,
            cost: 0.0,
            mode_share: vec![],
            perms: vec![],
            confidence: vec![],
        })
    }))
}

fn 线三() -> CalibStore {
    let mut c = CalibStore::new();
    for k in ["k", "k2", "k3"] {
        c.put(k, 0.65, 0.35, 50, "上岗", Some(0.05)).unwrap();
    }
    c
}

const 头: &str = r#"budget {calls: 30, cost: 0, depth: 8, absent: {retry: 0, backoff: 0, then: "conservative", breaker: 5}};
fn 问(m, t, c) {
    let e = cut(judge(state(mat(m)), test(t, c)));
    handle(e, {act: fn() { "act" }, ignore: fn() { "ignore" }, unsure: fn(u) { {c: unsure_cause(u), exit: u} }})
}
"#;

fn 跑一趟(src: &str, 坏: &[u64], l: &mut Ledger) -> serde_json::Value {
    let program = lower(&parse(src).expect("解析")).expect("lower");
    let calls = RefCell::new(0);
    run(
        &program,
        按次失败(坏, &calls),
        &线三(),
        &ActionRegistry::new(),
        l,
    )
    .unwrap_or_else(|e| panic!("{}", e.render()))
    .value_json()
}

/// 审计重放：零调用，返回值
fn 重放(src: &str, l: &mut Ledger) -> serde_json::Value {
    let program = lower(&parse(src).expect("解析")).expect("lower");
    let calls = RefCell::new(0);
    let o = run_replay(
        &program,
        按次失败(&[], &calls),
        &线三(),
        &ActionRegistry::new(),
        l,
    )
    .unwrap_or_else(|e| panic!("重放：{}", e.render()));
    assert_eq!(*calls.borrow(), 0, "重放零调用");
    o.value_json()
}

fn 趟标记数(l: &Ledger) -> usize {
    l.entries
        .iter()
        .filter(|e| matches!(e, Entry::Attempt { .. }))
        .count()
}

/// P-1（复核 R10，B1）：同一材料上 k 问两次、k2 问一次，y 由提升与 x0 同批发出、第 1 次调用失败。
/// 首跑 x0 缺席、x1 act、y act；审计重放与首跑相同
#[test]
fn p1_两题同批提升先缺后答_重放与首跑一致() {
    let src = format!(
        "{头}let x0 = 问(\"材料\", \"行吗\", \"k\");\nlet x1 = 问(\"材料\", \"行吗\", \"k\");\nlet y = 问(\"材料\", \"可以吗\", \"k2\");\n{{x0: x0, x1: x1, y: y}}\n"
    );
    let mut l = Ledger::new();
    let v = 跑一趟(&src, &[1], &mut l);
    assert_eq!(v["x0"]["c"], json!("absent"), "{v}");
    assert_eq!(v["x1"], json!("act"), "{v}");
    assert_eq!(v["y"], json!("act"), "{v}");
    // 复核的情形：y 的题经提升与 x0 同批发出而缺席，这条缺席记录不属于任何真站点登记
    let 提升缺席 = l
        .entries
        .iter()
        .filter(|e| matches!(e, Entry::Absent { nth: None, cause, .. } if cause == "absent"))
        .count();
    assert_eq!(提升缺席, 1, "y 经提升同批发出、缺席：{:?}", 缺席键(&l));
    assert_eq!(重放(&src, &mut l), v, "重放与首跑一致");
}

/// P-2（复核 R11）：y 的材料取 x1 的结果（不能提升），首跑与重放相同
#[test]
fn p2_不能提升的对照_重放与首跑一致() {
    let src = format!(
        "{头}let x0 = 问(\"材料\", \"行吗\", \"k\");\nlet x1 = 问(\"材料\", \"行吗\", \"k\");\nlet y = 问(x1, \"可以吗\", \"k2\");\n{{x0: x0, x1: x1, y: y}}\n"
    );
    let mut l = Ledger::new();
    let v = 跑一趟(&src, &[1], &mut l);
    assert_eq!(v["x0"]["c"], json!("absent"), "{v}");
    assert_eq!(v["x1"], json!("act"), "{v}");
    assert_eq!(v["y"], json!("act"), "{v}");
    assert_eq!(重放(&src, &mut l), v);
}

/// P-3（复核 R1，B2）：趟一 x0 缺席、x1 act；趟二全复用（只写一条趟标记）得 act、act；重放与趟二相同
#[test]
fn p3_全复用的一趟_重放复现它() {
    let src = format!(
        "{头}let x0 = 问(\"材料\", \"行吗\", \"k\");\nlet x1 = 问(\"材料\", \"行吗\", \"k\");\n{{x0: x0, x1: x1}}\n"
    );
    let mut l = Ledger::new();
    let v1 = 跑一趟(&src, &[1], &mut l);
    assert_eq!(v1["x0"]["c"], json!("absent"), "{v1}");
    assert_eq!(v1["x1"], json!("act"));
    let n = l.entries.len();
    let v2 = 跑一趟(&src, &[], &mut l);
    assert_eq!(v2, json!({"x0": "act", "x1": "act"}));
    assert_eq!(l.entries.len(), n + 1, "全复用：只多一条趟标记");
    assert_eq!(趟标记数(&l), 2);
    assert_eq!(重放(&src, &mut l), v2, "重放复现最后一趟");
}

/// P-4（复核 R12，B2）：趟一 x0 缺席、x1 act、y 缺席；趟二 k 复用、y 重问答到，全是 act；重放全是 act
#[test]
fn p4_两趟混合_重放不拼趟() {
    let src = format!(
        "{头}let x0 = 问(\"材料\", \"行吗\", \"k\");\nlet x1 = 问(\"材料\", \"行吗\", \"k\");\nlet y = 问(x1, \"可以吗\", \"k2\");\n{{x0: x0, x1: x1, y: y}}\n"
    );
    let mut l = Ledger::new();
    let v1 = 跑一趟(&src, &[1, 3], &mut l);
    assert_eq!(v1["x0"]["c"], json!("absent"), "{v1}");
    assert_eq!(v1["x1"], json!("act"), "{v1}");
    assert_eq!(v1["y"]["c"], json!("absent"), "{v1}");
    let v2 = 跑一趟(&src, &[], &mut l);
    assert_eq!(v2, json!({"x0": "act", "x1": "act", "y": "act"}));
    assert_eq!(重放(&src, &mut l), v2, "重放复现最后一趟，不逐题各取一趟");
}

/// 主控补（Z0580 之后）：趟一没缺席（不写标记）、趟二同一趟先缺后答、趟三全复用；重放与趟三相同
/// （z 的材料取 x0 的结果：只有推测登记的组发不出去时记「推测放弃」、不算缺席，这里要真站点缺席）
#[test]
fn p5_没缺席_缺席_全复用三趟_重放复现第三趟() {
    let a = format!("{头}let x0 = 问(\"材料\", \"行吗\", \"k\");\n{{x0: x0}}\n");
    let b = format!(
        "{头}let x0 = 问(\"材料\", \"行吗\", \"k\");\nlet z0 = 问(x0, \"对吗\", \"k3\");\nlet z1 = 问(x0, \"对吗\", \"k3\");\n{{x0: x0, z0: z0, z1: z1}}\n"
    );
    let mut l = Ledger::new();
    assert_eq!(跑一趟(&a, &[], &mut l), json!({"x0": "act"}));
    assert_eq!(趟标记数(&l), 0, "账本里没有缺席，不写趟标记");
    let v2 = 跑一趟(&b, &[1], &mut l);
    assert_eq!(v2["z0"]["c"], json!("absent"), "{v2}");
    assert_eq!(v2["z1"], json!("act"), "{v2}");
    let v3 = 跑一趟(&b, &[], &mut l);
    assert_eq!(v3, json!({"x0": "act", "z0": "act", "z1": "act"}));
    assert_eq!(趟标记数(&l), 2);
    assert_eq!(重放(&b, &mut l), v3, "重放复现第三趟");
}

// ───────────── 复查补的回归（Z0585：复查 R13、R14b）─────────────

/// 复查 R13：同一道题两个真站点登记在同一层（两个惰性 `cut`，去重成一次发出），缺席；之后第三处答到。
/// 首跑 a、b 缺席、c act，重放与首跑相同；再全复用跑一趟，重放与趟二相同
#[test]
fn r13_同题两个真站点同批缺席_第三处答到() {
    let src = format!(
        "{头}fn 看(e) {{ handle(e, {{act: fn() {{ \"act\" }}, ignore: fn() {{ \"ignore\" }}, unsure: fn(u) {{ {{c: unsure_cause(u), exit: u}} }}}}) }}\nlet ea = cut(judge(state(mat(\"材料\")), test(\"行吗\", \"k\")));\nlet eb = cut(judge(state(mat(\"材料\")), test(\"行吗\", \"k\")));\nlet a = 看(ea);\nlet b = 看(eb);\nlet c = 问(\"材料\", \"行吗\", \"k\");\n{{a: a, b: b, c: c}}\n"
    );
    let mut l = Ledger::new();
    let v1 = 跑一趟(&src, &[1], &mut l);
    assert_eq!(v1["a"]["c"], json!("absent"), "{v1}");
    assert_eq!(v1["b"]["c"], json!("absent"), "{v1}");
    assert_eq!(v1["c"], json!("act"), "{v1}");
    assert_eq!(重放(&src, &mut l.clone()), v1, "重放与首跑相同");
    let v2 = 跑一趟(&src, &[], &mut l);
    assert_eq!(v2, json!({"a": "act", "b": "act", "c": "act"}));
    assert_eq!(重放(&src, &mut l), v2, "重放与全复用的趟二相同");
}

/// 复查 R14b：提升 + 三趟，末趟被杀（去掉末趟的趟标记）。趟一全缺席；趟二 x0 缺席、x1 与 y 答到；趟三全复用，
/// 只写一条标记，把它截掉模拟没写完就被杀。重放复现趟二
#[test]
fn r14b_提升三趟末趟被杀_重放复现趟二() {
    let src = format!(
        "{头}let x0 = 问(\"材料\", \"行吗\", \"k\");\nlet x1 = 问(\"材料\", \"行吗\", \"k\");\nlet y = 问(\"材料\", \"可以吗\", \"k2\");\n{{x0: x0, x1: x1, y: y}}\n"
    );
    let mut l = Ledger::new();
    let v1 = 跑一趟(&src, &[1, 2, 3, 4, 5, 6], &mut l);
    assert_eq!(v1["x0"]["c"], json!("absent"), "{v1}");
    assert_eq!(v1["x1"]["c"], json!("absent"), "{v1}");
    assert_eq!(v1["y"]["c"], json!("absent"), "{v1}");
    let v2 = 跑一趟(&src, &[1], &mut l);
    assert_eq!(v2["x0"]["c"], json!("absent"), "{v2}");
    assert_eq!(v2["x1"], json!("act"), "{v2}");
    assert_eq!(v2["y"], json!("act"), "{v2}");
    let 提升缺席 = l
        .entries
        .iter()
        .filter(|e| matches!(e, Entry::Absent { nth: None, cause, .. } if cause == "absent"))
        .count();
    assert!(提升缺席 >= 1, "有提升登记的缺席：{:?}", 缺席键(&l));
    let v3 = 跑一趟(&src, &[], &mut l);
    assert_eq!(v3, json!({"x0": "act", "x1": "act", "y": "act"}));
    assert_eq!(趟标记数(&l), 3);
    // 末趟全复用，账本只多了它的趟标记：截掉这最后一行，等于趟三没写完标记就被杀
    assert!(matches!(l.entries.last(), Some(Entry::Attempt { .. })));
    let 全 = l.encode();
    let 行: Vec<&str> = 全.lines().collect();
    let mut s = 行[..行.len() - 1].join("\n");
    s.push('\n');
    let (mut 杀后, t) = Ledger::decode(&s).expect("解码");
    assert!(t.is_none());
    assert_eq!(趟标记数(&杀后), 2);
    assert_eq!(重放(&src, &mut 杀后), v2, "重放复现最后一个完整结束的趟二");
}
