//! 规划目标两臂（步 30 / B0488；`21`:311 `tests/ablation/plan.rs`）：本层超出剩余预算时，开臂（`critical_path`）让
//! 关键路径上的真站点组先拿预算（K-135 层数目标、主控 Q5），关臂退回步 22 的登记顺序。每次挑选进报告 `selections`，
//! 带本趟逐键计数（读数进「接下来先问什么」）。不超预算时两臂逐字节相同；审计重放不调钩子。
//!
//! 数值在过程记录 `地基/过程记录/工程-步30-价值函数.md` §3.3、§3.6、§3.7 预注册。B 段（价值密度）的两臂随后加在本文件。

use std::cell::RefCell;

use jpp::ActionRegistry;
use jpp::effects::{CalibStore, EffectError, FnPort, JudgeResult, Ports};
use jpp::interp::{Interp, Passes};
use jpp::ledger::Ledger;
use jpp::value::Answer;
use jpp::{lower, syntax::parse};
use serde_json::{Value, json};

#[path = "../value_support/mod.rs"]
mod value_support;

/// 材料里有「平」答 0.5，其余答 0.9；K 选一答第一个候选 0.97
fn 端口<'a>(每次题数: &'a RefCell<Vec<usize>>) -> Ports<'a> {
    Ports::new()
        .with(FnPort::judge("fixed-0", move |s, qs| {
            每次题数.borrow_mut().push(qs.len());
            let p = if s.on_text().contains('平') {
                0.5
            } else {
                0.9
            };
            Ok(JudgeResult {
                answers: qs
                    .iter()
                    .map(|q| match q.op {
                        jpp::value::Op::Select => Answer::Choice(vec![0.97, 0.01, 0.01, 0.01]),
                        _ => Answer::Noul(p),
                    })
                    .collect(),
                tokens: 0,
                cost: 0.0,
                mode_share: vec![],
                perms: vec![],
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

struct 结果 {
    值: Value,
    告警: Vec<String>,
    calls: u64,
    挑选: Vec<Value>,
    账本: String,
    /// 报告 `budget` 段（exhausted, unsent, first_site）
    预算: Option<(bool, u64, usize)>,
}

impl 结果 {
    fn 有(&self, 前缀: &str) -> bool {
        self.告警.iter().any(|w| w.starts_with(前缀))
    }
    fn 告警文(&self, 前缀: &str) -> Vec<String> {
        self.告警
            .iter()
            .filter(|w| w.starts_with(前缀))
            .cloned()
            .collect()
    }
    /// 第 n 条挑选里各组的（站点所在材料的登记位置, 是否在发出段）
    fn 顺序(&self, n: usize) -> Vec<(u64, bool)> {
        self.挑选[n]["groups"]
            .as_array()
            .unwrap()
            .iter()
            .map(|g| (g["pos"].as_u64().unwrap(), g["send"].as_bool().unwrap()))
            .collect()
    }
}

#[derive(Clone, Copy, PartialEq)]
enum 方式 {
    首跑,
    续跑,
    审计,
}

fn 跑(src: &str, calls: u64, 规划目标: bool, ledger: &mut Ledger, 方式: 方式) -> 结果 {
    let mut calib = CalibStore::new();
    calib.put("k", 0.65, 0.35, 100, "上岗", Some(0.05)).unwrap();
    calib.put("t", 0.65, 0.35, 100, "上岗", Some(0.05)).unwrap();
    跑全(src, calls, 规划目标, true, &calib, ledger, 方式)
}

/// 两个开关与校准库都由调用者给（B 段四臂）
fn 跑全(
    src: &str,
    calls: u64,
    关键路径: bool,
    价值: bool,
    calib: &CalibStore,
    ledger: &mut Ledger,
    方式: 方式,
) -> 结果 {
    let program = lower(&parse(src).expect("解析")).expect("lower");
    let 每次题数 = RefCell::new(vec![]);
    let actions = ActionRegistry::new();
    let mut budget = program.budget.clone();
    budget.calls = calls;
    let mut it = Interp::new(端口(&每次题数), ledger, calib, &actions, budget);
    if 方式 == 方式::审计 {
        it = it.audit_replay();
    }
    it.passes = Passes {
        critical_path: 关键路径,
        value_density: 价值,
        ..Passes::default()
    };
    let out = it
        .run(&program)
        .unwrap_or_else(|e| panic!("程序应当跑完：{}", e.render()));
    结果 {
        值: out.value_json(),
        告警: out.trace.warnings.clone(),
        calls: out.cost.calls,
        挑选: out.selections.clone(),
        账本: serde_json::to_string(&ledger.entries).unwrap(),
        预算: out
            .budget
            .as_ref()
            .map(|b| (b.exhausted, b.unsent, b.first_site)),
    }
}

// ---------------------------------------------------------------- P1：同层三个真站点，乙在关键路径上

/// 登记顺序甲、丙、乙（同一层，第一次检视时一起发）；乙的出口决定走丁还是戊，下游层数 1，甲、丙为 0
const P1: &str = r#"
budget {calls: 10, cost: 1, depth: 8};
let q = test("行吗", "k");
let r1 = judge(state(mat("甲料")), q);
let r3 = judge(state(mat("丙料")), q);
let r2 = judge(state(mat("乙料")), q);
let e1 = cut(r1);
let e3 = cut(r3);
let e2 = cut(r2);
let 结果 = if exit_kind(e2) == "act" { cut(judge(state(mat("丁料")), q)) } else { cut(judge(state(mat("戊料")), q)) };
{e1: exit_kind(e1), e3: exit_kind(e3), e2: exit_kind(e2), 结果: exit_kind(结果), x: [e1, e2, e3, 结果]}
"#;

fn 看1(x: &结果) -> (Value, Value, Value, Value) {
    (
        x.值["e1"].clone(),
        x.值["e3"].clone(),
        x.值["e2"].clone(),
        x.值["结果"].clone(),
    )
}

#[test]
fn p1_开臂关键路径上的乙先发() {
    let x = 跑(P1, 2, true, &mut Ledger::new(), 方式::首跑);
    assert_eq!(x.calls, 2);
    assert_eq!(
        看1(&x),
        (
            json!("act"),
            json!("unsure(budget)"),
            json!("act"),
            json!("unsure(budget)")
        )
    );
    assert!(x.有("W-budget"), "{:?}", x.告警);
    assert_eq!(x.挑选.len(), 2, "{:#?}", x.挑选);
    assert_eq!(x.挑选[0]["need"], json!(5));
    // B 段起价值密度默认开（这里记录都没有证书，价值全 0，次序不变）
    assert_eq!(x.挑选[0]["arm"], json!("critical_path+value_density"));
    // 登记位置：丁、戊两个推测组先（`let r1` 求值前已推测登记，0、1），再甲 2、丙 3、乙 4。
    // 开臂顺序乙、甲、丙，再丁、戊
    let 序 = x.顺序(0);
    assert_eq!(
        序,
        vec![(4, true), (2, true), (3, false), (0, false), (1, false)]
    );
    assert_eq!(序.len(), 5);
    let ds: Vec<u64> = x.挑选[0]["groups"]
        .as_array()
        .unwrap()
        .iter()
        .map(|g| g["downstream"].as_u64().unwrap())
        .collect();
    assert_eq!(&ds[..3], &[1, 0, 0]);
    assert_eq!(x.挑选[1]["need"], json!(1));
    assert_eq!(x.挑选[1]["left"]["calls"], json!(0));
}

#[test]
fn p1_关臂按登记顺序() {
    let x = 跑(P1, 2, false, &mut Ledger::new(), 方式::首跑);
    assert_eq!(x.calls, 2);
    assert_eq!(
        看1(&x),
        (
            json!("act"),
            json!("act"),
            json!("unsure(budget)"),
            json!("unsure(budget)")
        )
    );
    assert!(x.有("W-budget"), "{:?}", x.告警);
    assert_eq!(x.挑选.len(), 2, "{:#?}", x.挑选);
    assert_eq!(x.挑选[0]["arm"], json!("value_density"));
    assert_eq!(
        x.顺序(0),
        vec![(2, true), (3, true), (4, false), (0, false), (1, false)]
    );
}

#[test]
fn p1_审计重放两臂同首跑_零新调用_不重算挑选() {
    for 开 in [true, false] {
        let mut l = Ledger::new();
        let 首 = 跑(P1, 2, 开, &mut l, 方式::首跑);
        let 重 = 跑(P1, 2, 开, &mut l, 方式::审计);
        assert_eq!(看1(&重), 看1(&首), "规划目标 {开}");
        assert_eq!(重.calls, 0, "规划目标 {开}");
        assert!(重.挑选.is_empty(), "规划目标 {开}：{:?}", 重.挑选);
    }
}

#[test]
fn p1_不超预算时两臂逐字节相同() {
    let mut la = Ledger::new();
    let mut lb = Ledger::new();
    let a = 跑(P1, 10, true, &mut la, 方式::首跑);
    let b = 跑(P1, 10, false, &mut lb, 方式::首跑);
    assert_eq!(a.值, b.值);
    assert_eq!(a.账本, b.账本);
    assert!(a.挑选.is_empty() && b.挑选.is_empty());
}

// ---------------------------------------------------------------- P2：本趟逐键计数进挑选明细

const P2: &str = r#"
budget {calls: 10, cost: 1, depth: 8};
let q = test("行吗", "t");
let x1 = cut(judge(state(mat("平料")), q));
let x2 = cut(judge(state(mat("好料")), q));
let k1 = exit_kind(x1);
let k2 = exit_kind(x2);
let y1 = judge(state(mat("甲料")), q);
let y2 = judge(state(mat("乙料")), q);
let y3 = judge(state(mat("丙料")), q);
let e = [cut(y1), cut(y2), cut(y3)];
{k1: k1, k2: k2, e: map(e, fn(z) { exit_kind(z) }), x: [x1, x2, e]}
"#;

#[test]
fn p2_逐键计数进第二层的挑选() {
    let x = 跑(P2, 3, true, &mut Ledger::new(), 方式::首跑);
    assert_eq!(x.calls, 3);
    assert_eq!(x.值["k1"], json!("unsure(band)"));
    assert_eq!(x.值["k2"], json!("act"));
    assert_eq!(
        x.值["e"],
        json!(["act", "unsure(budget)", "unsure(budget)"])
    );
    assert_eq!(x.挑选.len(), 1, "{:#?}", x.挑选);
    assert_eq!(x.挑选[0]["layer"], json!(2));
    assert_eq!(x.挑选[0]["need"], json!(3));
    assert_eq!(x.挑选[0]["left"]["calls"], json!(1));
    assert_eq!(x.顺序(0), vec![(0, true), (1, false), (2, false)]);
    for g in x.挑选[0]["groups"].as_array().unwrap() {
        assert_eq!(
            g["keys"],
            json!([{"key": "t", "seen": 2, "unsure": 1, "u": null, "n": null, "u_run": null}])
        );
    }
}

// ---------------------------------------------------------------- 步 22 夹具「续跑」在开臂（§3.6）

/// 惰性过桥下 a 与条件 c 同层；c 控制 if 两侧，下游层数 1
const 续跑: &str = r#"
budget {calls: 10, cost: 1, depth: 8};
let 甲 = state(mat("甲料"));
let 乙 = state(mat("乙料"));
let q = test("行吗", "k");
let a = cut(judge(state(mat("丙料")), q));
let c = cut(judge(state(mat("条件料")), q));
let ck = exit_kind(c);
let 结果 = if ck == "act" {
    let e = cut(judge(甲, q));
    {side: "甲", k: exit_kind(e), e: e}
} else {
    let e = cut(judge(乙, q));
    {side: "乙", k: exit_kind(e), e: e}
};
{a: exit_kind(a), ea: a, 条件: ck, c: c, r: 结果}
"#;

fn 看续(x: &结果) -> (Value, Value, Value, Value) {
    (
        x.值["a"].clone(),
        x.值["条件"].clone(),
        x.值["r"]["side"].clone(),
        x.值["r"]["k"].clone(),
    )
}

#[test]
fn 续跑夹具_开臂条件先发() {
    let mut l = Ledger::new();
    let 首 = 跑(续跑, 1, true, &mut l, 方式::首跑);
    assert_eq!(首.calls, 1);
    assert_eq!(
        看续(&首),
        (
            json!("unsure(budget)"),
            json!("act"),
            json!("甲"),
            json!("unsure(budget)")
        )
    );
    assert_eq!(首.挑选.len(), 2, "{:#?}", 首.挑选);
    assert_eq!(首.挑选[0]["need"], json!(4));
    assert_eq!(
        首.顺序(0).iter().map(|x| x.1).collect::<Vec<_>>(),
        vec![true, false, false, false]
    );
    assert_eq!(首.挑选[1]["need"], json!(1));
    assert_eq!(首.挑选[1]["left"]["calls"], json!(0));

    let 续 = 跑(续跑, 1, true, &mut l, 方式::续跑);
    assert_eq!(续.calls, 1);
    assert_eq!(
        看续(&续),
        (
            json!("act"),
            json!("act"),
            json!("甲"),
            json!("unsure(budget)")
        )
    );
}

#[test]
fn 续跑夹具_开臂续跑预算二() {
    let mut l = Ledger::new();
    跑(续跑, 1, true, &mut l, 方式::首跑);
    let 续 = 跑(续跑, 2, true, &mut l, 方式::续跑);
    assert_eq!(续.calls, 2);
    // §3.6 这里预测甲 unsure(budget)、有 W-spec-unused，落空（过程记录 §4.2 第 3 条）：首跑记了 Absent{budget} 的甲
    // 在续跑仍被推测登记，本层 a（真站点）、甲、乙（推测）need 3 > 2，a 先、甲次之，乙推迟未发
    assert_eq!(
        看续(&续),
        (json!("act"), json!("act"), json!("甲"), json!("act"))
    );
    assert!(!续.有("W-spec-unused"), "{:?}", 续.告警);
    assert_eq!(续.挑选.len(), 1, "{:#?}", 续.挑选);
    assert_eq!(续.挑选[0]["need"], json!(3));
    assert_eq!(续.挑选[0]["left"]["calls"], json!(2));
}

// ---------------------------------------------------------------- 费用模型：有单价、缺回归（复核 B0488-A 缺口 7）

/// 预注册 §3.3 的写法：画像去掉 `cost.regression`，单价还在，费用模型为 `None`
#[test]
fn 费用模型_有单价缺回归即无() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut j: Value = serde_json::from_str(
        &std::fs::read_to_string(root.join("profiles/jev-1.13.0.json")).expect("画像文件"),
    )
    .expect("画像 JSON");
    j["cost"]
        .as_object_mut()
        .expect("cost 节")
        .remove("regression")
        .expect("原画像有 regression");
    let pr = jpp_effects::Profile::from_json(&j).expect("去掉回归仍是合法画像");
    assert!(pr.price_per_input_token().is_some());
    assert!(pr.token_regression().is_none());
    let program = lower(
        &parse("budget {calls: 2, cost: 1, depth: 8};\ncut(judge(state(mat(\"甲\")), test(\"行吗\", \"k\")))")
            .expect("解析"),
    )
    .expect("lower");
    let pl = jpp_plan::plan_with(
        &program,
        &Passes::default(),
        Some(&pr),
        &jpp::interp::PlanCtx::unknown(),
    );
    assert!(pl.cost_model.is_none());
    // 对照：原画像有费用模型
    let full = jpp_effects::Profile::load(&root.join("profiles/jev-1.13.0.json")).expect("画像");
    let pl = jpp_plan::plan_with(
        &program,
        &Passes::default(),
        Some(&full),
        &jpp::interp::PlanCtx::unknown(),
    );
    assert!(pl.cost_model.is_some());
}

// ---------------------------------------------------------------- 首个停发站点：重排后与审计重放同口径（B93 第 6 条）

/// 同层登记甲、乙、丙三个真站点；丙的出口控制两层（下游 2），乙控制一层（下游 1），甲为 0。预算 1：发丙，推迟乙、甲。
/// 首个停发站点按原登记位置记为甲，与审计重放按登记顺序停发的第一组相同（复核 B0488-A 缺口 1）
const F: &str = r#"
budget {calls: 10, cost: 1, depth: 8};
let q = test("行吗", "k");
let r1 = judge(state(mat("甲料")), q);
let r2 = judge(state(mat("乙料")), q);
let r3 = judge(state(mat("丙料")), q);
let e1 = cut(r1);
let e2 = cut(r2);
let e3 = cut(r3);
let t = if exit_kind(e3) == "act" {
    let f = cut(judge(state(mat("丁料")), q));
    if exit_kind(f) == "act" { cut(judge(state(mat("戊料")), q)) } else { f }
} else { e3 };
let u = if exit_kind(e2) == "act" { cut(judge(state(mat("己料")), q)) } else { e2 };
{e1: exit_kind(e1), e2: exit_kind(e2), e3: exit_kind(e3), t: exit_kind(t), u: exit_kind(u), x: [e1, e2, e3, t, u]}
"#;

fn 看f(x: &结果) -> Value {
    json!([x.值["e1"], x.值["e2"], x.值["e3"], x.值["t"], x.值["u"]])
}

#[test]
fn f_重排后首个停发站点按登记位置_与审计重放相同() {
    let 甲 = F.find(r#"judge(state(mat("甲料"))"#).expect("甲的站点");
    let mut l = Ledger::new();
    let 首 = 跑(F, 1, true, &mut l, 方式::首跑);
    assert_eq!(首.calls, 1);
    assert_eq!(
        看f(&首),
        json!([
            "unsure(budget)",
            "unsure(budget)",
            "act",
            "unsure(budget)",
            "unsure(budget)"
        ])
    );
    // 第 1 层是重排过的：丙先发
    assert_eq!(首.挑选[0]["groups"][0]["downstream"], json!(2));
    let (耗尽, _, 首站) = 首.预算.expect("预算耗尽");
    assert!(耗尽);
    assert_eq!(首站, 甲, "首个停发站点应是登记最早的甲");
    let wb = 首.告警文("W-budget");
    assert_eq!(wb.len(), 1, "{wb:?}");
    assert!(wb[0].starts_with(&format!("W-budget: @{甲} ")), "{wb:?}");

    let 重 = 跑(F, 1, true, &mut l, 方式::审计);
    assert_eq!(重.calls, 0);
    assert_eq!(看f(&重), 看f(&首));
    assert_eq!(
        重.预算, 首.预算,
        "budget 段（exhausted、unsent、first_site）逐项相等"
    );
    assert_eq!(重.告警文("W-budget"), wb, "W-budget 逐字相同");
    assert!(重.挑选.is_empty());
}

#[test]
fn f_关键路径关时首跑与审计重放同样相同() {
    let mut l = Ledger::new();
    let 首 = 跑(F, 1, false, &mut l, 方式::首跑);
    let 重 = 跑(F, 1, false, &mut l, 方式::审计);
    assert_eq!(重.calls, 0);
    assert_eq!(看f(&重), 看f(&首));
    assert_eq!(重.预算, 首.预算);
    assert_eq!(重.告警文("W-budget"), 首.告警文("W-budget"));
}

// ---------------------------------------------------------------- 逐键计数：推迟的读数不计、同一读数切两次只计一次

const P3: &str = r#"
budget {calls: 10, cost: 1, depth: 8};
let q = test("行吗", "t");
let r1 = judge(state(mat("平料")), q);
let r2 = judge(state(mat("好料")), q);
let r3 = judge(state(mat("又料")), q);
let x1 = cut(r1);
let x1b = cut(r1);
let x2 = cut(r2);
let x3 = cut(r3);
let k = [exit_kind(x1), exit_kind(x1b), exit_kind(x2), exit_kind(x3)];
let y = cut(judge(state(mat("甲料")), q));
{k: k, y: exit_kind(y), x: [x1, x1b, x2, x3, y]}
"#;

#[test]
fn p3_推迟的读数不计_切两次只计一次() {
    let x = 跑(P3, 2, true, &mut Ledger::new(), 方式::首跑);
    assert_eq!(x.calls, 2);
    assert_eq!(
        x.值["k"],
        json!(["unsure(band)", "unsure(band)", "act", "unsure(budget)"])
    );
    assert_eq!(x.值["y"], json!("unsure(budget)"));
    assert_eq!(x.挑选.len(), 2, "{:#?}", x.挑选);
    assert_eq!(x.挑选[0]["layer"], json!(1));
    assert_eq!(x.挑选[0]["need"], json!(3));
    assert_eq!(x.挑选[0]["left"]["calls"], json!(2));
    for g in x.挑选[0]["groups"].as_array().unwrap() {
        assert_eq!(
            g["keys"],
            json!([{"key": "t", "seen": 0, "unsure": 0, "u": null, "n": null, "u_run": null}])
        );
    }
    assert_eq!(x.挑选[1]["layer"], json!(2));
    assert_eq!(x.挑选[1]["need"], json!(1));
    assert_eq!(x.挑选[1]["left"]["calls"], json!(0));
    assert_eq!(
        x.挑选[1]["groups"][0]["keys"],
        json!([{"key": "t", "seen": 2, "unsure": 1, "u": null, "n": null, "u_run": null}])
    );
}

// ---------------------------------------------------------------- B 段：四臂（夹具 V，过程记录 §7.4）

/// 同一层四组，各自材料与校准键：c（是非、无记录）、b（是非、u 0.5）、a（是非、u 0.02）、d（K 选一 4 候选、u 0.02）；
/// 价值（第二版，过程记录 §7.9；K 元伪计数按 §10 改后）0、0.362869、0.822475、1.750613；下游层数全 0
const V: &str = r#"
budget {calls: 10, cost: 1, depth: 8};
let st = state(mat("丁料"), {over: [mat("一"), mat("二"), mat("三"), mat("四")]});
let r0 = judge(state(mat("甲料")), test("行吗", "c"));
let r1 = judge(state(mat("乙料")), test("行吗", "b"));
let r2 = judge(state(mat("丙料")), test("行吗", "a"));
let r3 = judge(st, select("哪一个？", "d"));
let es = [cut(r0), cut(r1), cut(r2), cut(r3)];
{k: map(es, fn(e) { exit_kind(e) }), es: es}
"#;

fn v库() -> CalibStore {
    value_support::认证库(&[("b", 40, None), ("a", 0, None), ("d", 0, None)])
}

fn 推迟的(x: &结果) -> Vec<bool> {
    x.值["k"]
        .as_array()
        .unwrap()
        .iter()
        .map(|k| k == "unsure(budget)")
        .collect()
}

#[test]
fn v_四臂() {
    let calib = v库();
    for (cp, vd, arm) in [
        (false, true, "value_density"),
        (true, true, "critical_path+value_density"),
    ] {
        let x = 跑全(V, 2, cp, vd, &calib, &mut Ledger::new(), 方式::首跑);
        assert_eq!(x.calls, 2, "{arm}");
        // 发 d、a，推迟 b、c
        assert_eq!(
            推迟的(&x),
            vec![true, true, false, false],
            "{arm}：{}",
            x.值
        );
        assert_eq!(x.挑选[0]["arm"], json!(arm));
        assert_eq!(
            x.顺序(0).iter().map(|g| g.0).collect::<Vec<_>>(),
            vec![3, 2, 1, 0]
        );
        let 价值: Vec<f64> = x.挑选[0]["groups"]
            .as_array()
            .unwrap()
            .iter()
            .map(|g| g["value"].as_f64().unwrap())
            .collect();
        for (a, b) in 价值.iter().zip([1.750613, 0.822475, 0.362869, 0.0]) {
            assert!((a - b).abs() < 1e-6, "{arm}：{价值:?}");
        }
    }
    for (cp, vd, arm) in [(false, false, "order"), (true, false, "critical_path")] {
        let x = 跑全(V, 2, cp, vd, &calib, &mut Ledger::new(), 方式::首跑);
        assert_eq!(x.calls, 2, "{arm}");
        // 发 c、b，推迟 a、d
        assert_eq!(
            推迟的(&x),
            vec![false, false, true, true],
            "{arm}：{}",
            x.值
        );
        assert_eq!(x.挑选[0]["arm"], json!(arm));
        assert!(x.挑选[0]["groups"][0]["value"].is_null());
    }
}

#[test]
fn v_审计重放各臂同首跑() {
    let calib = v库();
    for (cp, vd) in [(false, false), (true, false), (false, true), (true, true)] {
        let mut l = Ledger::new();
        let 首 = 跑全(V, 2, cp, vd, &calib, &mut l, 方式::首跑);
        let 重 = 跑全(V, 2, cp, vd, &calib, &mut l, 方式::审计);
        assert_eq!(重.calls, 0);
        assert_eq!(重.值["k"], 首.值["k"], "{cp} {vd}");
        assert_eq!(重.预算, 首.预算, "{cp} {vd}");
        assert_eq!(重.告警文("W-budget"), 首.告警文("W-budget"));
    }
}

// ---------------------------------------------------------------- B 段：层边界重规划（夹具 R，过程记录 §7.4）

fn r程序(料: &str) -> String {
    format!(
        r#"
budget {{calls: 10, cost: 1, depth: 8}};
let q1 = test("行吗", "a2");
let x1 = cut(judge(state(mat("{料}一")), q1));
let x2 = cut(judge(state(mat("{料}二")), q1));
let k1 = [exit_kind(x1), exit_kind(x2)];
let ya = judge(state(mat("甲料")), q1);
let yb = judge(state(mat("乙料")), test("行吗", "b2"));
let e = [cut(ya), cut(yb)];
{{k1: k1, e: map(e, fn(z) {{ exit_kind(z) }}), x: [x1, x2, e]}}
"#
    )
}

fn r库() -> CalibStore {
    // a2：模板前 10 条（[4,0,0] / [0,6,0]）；b2：模板 j = 28（0.516013）
    value_support::认证库(&[("a2", 0, Some(10)), ("b2", 28, None)])
}

#[test]
fn r_本趟弃权让同键的题退后() {
    let x = 跑全(
        &r程序("平"),
        3,
        true,
        true,
        &r库(),
        &mut Ledger::new(),
        方式::首跑,
    );
    assert_eq!(x.值["k1"], json!(["unsure(band)", "unsure(band)"]));
    assert_eq!(x.值["e"], json!(["unsure(budget)", "act"]), "{}", x.值);
    assert_eq!(x.挑选.len(), 1, "{:#?}", x.挑选);
    let g = &x.挑选[0]["groups"];
    // 先发 b2（登记位置 1），a2 推迟
    assert_eq!(x.顺序(0), vec![(1, true), (0, false)]);
    let a2 = &g[1]["keys"][0];
    assert_eq!(a2["key"], json!("a2"));
    assert_eq!(
        (a2["seen"].clone(), a2["unsure"].clone()),
        (json!(2), json!(2))
    );
    assert!((a2["u"].as_f64().unwrap() - 1.0 / 13.0).abs() < 1e-6);
    assert!((a2["n"].as_f64().unwrap() - 13.0).abs() < 1e-9);
    assert!((a2["u_run"].as_f64().unwrap() - 0.2).abs() < 1e-6);
    assert!((g[1]["value"].as_f64().unwrap() - 0.463438).abs() < 1e-6);
    assert!((g[0]["value"].as_f64().unwrap() - 0.516013).abs() < 1e-6);
}

#[test]
fn r_对照_本趟不弃权时同键的题照旧在前() {
    let x = 跑全(
        &r程序("好"),
        3,
        true,
        true,
        &r库(),
        &mut Ledger::new(),
        方式::首跑,
    );
    assert_eq!(x.值["k1"], json!(["act", "act"]));
    assert_eq!(x.值["e"], json!(["act", "unsure(budget)"]), "{}", x.值);
    assert_eq!(x.顺序(0), vec![(0, true), (1, false)]);
    let g = &x.挑选[0]["groups"];
    assert!((g[0]["keys"][0]["u_run"].as_f64().unwrap() - 1.0 / 15.0).abs() < 1e-6);
    assert!((g[0]["value"].as_f64().unwrap() - 0.540677).abs() < 1e-6);
}

// ---------------------------------------------------------------- 裁定三十八推翻条件 (1) 与第二版（过程记录 §7.5、§7.9）

/// 2026-09-30 首次跑第一版（期望熵降 × (1 − u)(1 − H_b(ε))）对精确互信息：不一致 1/3（json_field 对 answers_question），
/// 推翻条件触发，按裁定四十六换第二版。这条测试现在检验第二版：三条是非题式按记录混淆矩阵（Jeffreys 平滑）算的互信息等于
/// 按定义算好的数；对称近似（K = 2，三格各 +1，伪计数总量与精确形式相同，§10.1 第 3 条）与之差 < 0.01 比特、三对排序一致。
/// 3 对的检验力很弱，照实写
#[test]
fn 第二版_题库三条是非题式() {
    use jpp_effects::views::CalibView;
    use jpp_ir::plan::Channel;
    let root = value_support::root();
    let dirs = [
        (
            "C01",
            root.join("../题库/第二批/C01/calib110"),
            0.848993,
            0.845826,
        ),
        (
            "json_field",
            root.join("bank/entries/ceb097176786d327fe1a0ba3/calib"),
            0.864533,
            0.858897,
        ),
        (
            "answers_question",
            root.join("bank/entries/1d77f7a203b4258048dfefea/calib"),
            0.822475,
            0.812902,
        ),
    ];
    let mut rows = vec![];
    for (name, d, want, want_sym) in dirs {
        if !d.exists() {
            // 公开仓库没有研究区的 题库/第二批/ 校准目录：跳过这一条（tools/sync-rust-from-research.sh 改写）
            eprintln!("跳过 {name}：校准目录 {} 不在本仓库", d.display());
            continue;
        }
        let s = CalibStore::load(&d).expect("装载");
        let key = s.certified().first().expect("一条已认证记录").0.clone();
        let rec = s.lookup(&key).unwrap();
        let delta = s.line_delta(&rec).unwrap();
        let mut n = [[0f64; 3]; 2];
        for (p, 真) in s.labelled(&key) {
            let 列 = if jpp::conformal::decided_up(p, rec.hi, delta) {
                0
            } else if jpp::conformal::decided_down(p, rec.lo, delta) {
                1
            } else {
                2
            };
            n[usize::from(!真)][列] += 1.0;
        }
        assert!(s.binary(&key), "{name} 是是非题记录");
        let v2 = jpp_plan::value::confusion_value(&n, None);
        let (c, w, u) = (n[0][0] + n[1][1], n[0][1] + n[1][0], n[0][2] + n[1][2]);
        let sym = jpp_plan::value::symmetric_value(c, w, u, 2, None);
        println!(
            "V2 {name}: 混淆={n:?} 第二版={v2:.6} 对称近似={sym:.6} 差={:+.6}",
            sym - v2
        );
        assert!((v2 - want).abs() < 1e-6, "{name}：{v2}");
        assert!((sym - want_sym).abs() < 1e-6, "{name}：对称近似 {sym}");
        assert!((sym - v2).abs() < 0.01, "{name}：对称近似 {sym} 对 {v2}");
        // 与规划器同一条路：Channel 进 channel_value
        let cv = jpp_plan::value::channel_value(
            jpp_ir::question_kind::Request::Whether,
            0,
            &Channel::Binary { n },
            0,
            0,
        );
        assert!((cv - v2).abs() < 1e-12);
        rows.push((name, v2, sym));
    }
    for i in 0..rows.len() {
        for j in i + 1..rows.len() {
            let a = (rows[i].1 - rows[j].1).signum();
            let b = (rows[i].2 - rows[j].2).signum();
            assert!(
                a * b > 0.0,
                "对称近似与第二版在 {} 对 {} 上排序相反",
                rows[i].0,
                rows[j].0
            );
        }
    }
}

// ---------------------------------------------------------------- 复核后补（过程记录 §10.2）

/// 夹具 R 的仓内重放：平、好两种材料各跑首跑、审计重放、只凭账本重放，出口、`budget`、`W-budget` 三者相同，
/// 重放零新调用、不重算挑选
#[test]
fn r_首跑_审计重放_只凭账本重放三者相同() {
    use jpp::Session;
    for 料 in ["平", "好"] {
        let src = r程序(料);
        let calib = r库();
        let mut l = Ledger::new();
        let 首 = 跑全(&src, 3, true, true, &calib, &mut l, 方式::首跑);
        assert_eq!(首.calls, 3, "{料}");
        let mut l2 = l.clone();
        let 审 = 跑全(&src, 3, true, true, &calib, &mut l2, 方式::审计);
        let mut restored = CalibStore::new();
        Session::restore_calib(&mut restored, &l).unwrap();
        let mut l3 = l.clone();
        let 账 = 跑全(&src, 3, true, true, &restored, &mut l3, 方式::审计);
        for (名, x) in [("审计重放", &审), ("只凭账本重放", &账)] {
            assert_eq!(x.calls, 0, "{料} {名}");
            assert_eq!(x.值["e"], 首.值["e"], "{料} {名}");
            assert_eq!(x.值["k1"], 首.值["k1"], "{料} {名}");
            assert_eq!(x.预算, 首.预算, "{料} {名}");
            assert_eq!(x.告警文("W-budget"), 首.告警文("W-budget"), "{料} {名}");
            assert!(x.挑选.is_empty(), "{料} {名}");
        }
    }
}

/// 计数记在线实际命中的记录上（主控 B0488 缺口 12）：a2 的记录挂在题式键上（题用 form + fill，题键「fa」无记录），第 1 层
/// 两道题由题式线切出 unsure(band)，计数记在题式键；第 2 层挑选按题式键取到计数，a2 价值 0.463438 < b2 0.516013，先发 b2
#[test]
fn 计数记在题式级记录上() {
    let h = jpp::value::Form::new(
        jpp::value::Op::Test,
        "行吗",
        "fa",
        vec![],
        vec![],
        None,
        None,
    )
    .unwrap()
    .hash;
    let fk = format!("\u{1f}form\u{1f}{h}");
    let calib = value_support::认证库(&[(fk.as_str(), 0, Some(10)), ("b2", 28, None)]);
    let src = r#"
budget {calls: 10, cost: 1, depth: 8};
let F = form("test", "行吗", {calib: "fa"});
let q1 = fill(F, {});
let x1 = cut(judge(state(mat("平一")), q1));
let x2 = cut(judge(state(mat("平二")), q1));
let k1 = [exit_kind(x1), exit_kind(x2)];
let ya = judge(state(mat("甲料")), q1);
let yb = judge(state(mat("乙料")), test("行吗", "b2"));
let e = [cut(ya), cut(yb)];
{k1: k1, e: map(e, fn(z) { exit_kind(z) }), x: [x1, x2, e]}
"#;
    let x = 跑全(src, 3, true, true, &calib, &mut Ledger::new(), 方式::首跑);
    assert_eq!(
        x.值["k1"],
        json!(["unsure(band)", "unsure(band)"]),
        "{}",
        x.值
    );
    assert_eq!(x.值["e"], json!(["unsure(budget)", "act"]), "{}", x.值);
    let g = &x.挑选[0]["groups"];
    assert_eq!(x.顺序(0), vec![(1, true), (0, false)]);
    let a2 = &g[1]["keys"][0];
    assert_eq!(a2["key"], json!(fk));
    assert_eq!(
        (a2["seen"].clone(), a2["unsure"].clone()),
        (json!(2), json!(2))
    );
    assert!((g[1]["value"].as_f64().unwrap() - 0.463438).abs() < 1e-6);
}

/// 作者声明线切出的读数没有命中记录，不计（主控 B0488 缺口 12）
#[test]
fn 声明线切出的读数不计() {
    let calib = value_support::认证库(&[("t2", 0, None)]);
    let src = r#"
budget {calls: 10, cost: 1, depth: 8};
let q = test("行吗", "t2");
let x = cut(judge(state(mat("好一")), q), {declare: {hi: 0.95, lo: 0.05}});
let k = exit_kind(x);
let r1 = judge(state(mat("甲料")), q);
let r2 = judge(state(mat("乙料")), q);
let r3 = judge(state(mat("丙料")), q);
let es = [cut(r1), cut(r2), cut(r3)];
{k: k, e: map(es, fn(z) { exit_kind(z) }), x: [x, es]}
"#;
    let x = 跑全(src, 2, true, true, &calib, &mut Ledger::new(), 方式::首跑);
    assert_eq!(x.值["k"], json!("unsure(band)"), "{}", x.值);
    assert_eq!(x.挑选.len(), 1, "{:#?}", x.挑选);
    for g in x.挑选[0]["groups"].as_array().unwrap() {
        let k = &g["keys"][0];
        assert_eq!(k["key"], json!("t2"));
        assert_eq!(
            (k["seen"].clone(), k["unsure"].clone()),
            (json!(0), json!(0)),
            "{g}"
        );
    }
}

/// 降为夹具的记录价值为 0（复核 B0488-B 缺口 1）：夹具 V 的 b（改过样本）经 `CalibStore::load` 重跑认证后降为夹具
#[test]
fn v_降为夹具的记录价值为零() {
    use value_support::记;
    let calib = value_support::认证库2(
        &[
            记 {
                key: "b",
                j: 40,
                keep: None,
                kind: None,
                status: None,
            },
            记 {
                key: "a",
                j: 0,
                keep: None,
                kind: None,
                status: None,
            },
            记 {
                key: "d",
                j: 0,
                keep: None,
                kind: None,
                status: None,
            },
        ],
        true,
    );
    let x = 跑全(V, 2, true, true, &calib, &mut Ledger::new(), 方式::首跑);
    let b = x.挑选[0]["groups"]
        .as_array()
        .unwrap()
        .iter()
        .find(|g| g["pos"] == json!(1))
        .unwrap()
        .clone();
    assert_eq!(b["value"].as_f64().unwrap(), 0.0, "{b}");
    // 未改样本的 a、d 重跑认证后仍是已认证（复查 B0488-B 小处：全部降级的错实现也会让上一句过）
    let 值 = |pos: u64| {
        x.挑选[0]["groups"]
            .as_array()
            .unwrap()
            .iter()
            .find(|g| g["pos"] == json!(pos))
            .unwrap()["value"]
            .as_f64()
            .unwrap()
    };
    assert!((值(2) - 0.822475).abs() < 1e-6, "a");
    assert!((值(3) - 1.750613).abs() < 1e-6, "d");
}

// ---------------------------------------------------------------- Z0385（过程记录 §12.2）

/// T1：代价线没选到证书、出口按判断器回答走的读数不计（Z0378「冷的不计」）
#[test]
fn z0385_代价线没选到证书不计() {
    let calib = value_support::认证库(&[("t2", 0, None)]);
    let src = r#"
budget {calls: 10, cost: 1, depth: 8};
let q = test("行吗", "t2");
let x = cut(judge(state(mat("好一")), q), {cost: [1, 2]});
let k = exit_kind(x);
let r1 = judge(state(mat("甲料")), q);
let r2 = judge(state(mat("乙料")), q);
let r3 = judge(state(mat("丙料")), q);
let es = [cut(r1), cut(r2), cut(r3)];
{k: k, e: map(es, fn(z) { exit_kind(z) }), x: [x, es]}
"#;
    let x = 跑全(src, 2, true, true, &calib, &mut Ledger::new(), 方式::首跑);
    assert_eq!(x.值["k"], json!("act"), "{}", x.值);
    assert!(x.有("W-untested"), "{:?}", x.告警);
    assert_eq!(x.挑选.len(), 1, "{:#?}", x.挑选);
    for g in x.挑选[0]["groups"].as_array().unwrap() {
        let k = &g["keys"][0];
        assert_eq!(k["key"], json!("t2"));
        assert_eq!(
            (k["seen"].clone(), k["unsure"].clone()),
            (json!(0), json!(0)),
            "{g}"
        );
    }
}

/// T2–T4 共用：题式「行吗」（calib「fa」）的题在甲料上，与无记录的题在乙料上同一层，预算只够 1，读这道题那一组的价值
fn z0385_价值(calib: &CalibStore) -> f64 {
    let src = r#"
budget {calls: 10, cost: 1, depth: 8};
let F = form("test", "行吗", {calib: "fa"});
let ya = judge(state(mat("甲料")), fill(F, {}));
let yc = judge(state(mat("乙料")), test("行吗", "c"));
let e = [cut(ya), cut(yc)];
{e: map(e, fn(z) { exit_kind(z) }), x: e}
"#;
    let x = 跑全(src, 1, true, true, calib, &mut Ledger::new(), 方式::首跑);
    assert_eq!(x.挑选.len(), 1, "{:#?}", x.挑选);
    x.挑选[0]["groups"]
        .as_array()
        .unwrap()
        .iter()
        .find(|g| g["pos"] == json!(0))
        .unwrap()["value"]
        .as_f64()
        .unwrap()
}

fn z0385_题式键() -> String {
    let h = jpp::value::Form::new(
        jpp::value::Op::Test,
        "行吗",
        "fa",
        vec![],
        vec![],
        None,
        None,
    )
    .unwrap()
    .hash;
    format!("\u{1f}form\u{1f}{h}")
}

/// T2：题级是夹具线（改过样本、重跑认证降为夹具），`cut` 用它切出口；价值不能改取题式记录，应为 0
#[test]
fn z0385_题级夹具线时价值为零() {
    use value_support::记;
    let fk: &'static str = Box::leak(z0385_题式键().into_boxed_str());
    let calib = value_support::认证库2(
        &[
            记 {
                key: "fa",
                j: 40,
                keep: None,
                kind: None,
                status: None,
            },
            记 {
                key: fk,
                j: 0,
                keep: None,
                kind: None,
                status: None,
            },
        ],
        true,
    );
    assert_eq!(z0385_价值(&calib), 0.0);
}

/// T3：题级记录是「停岗候选」，`cut` 用它切出口；价值只认「上岗」，应为 0，不往下取题式记录
#[test]
fn z0385_题级停岗候选时价值为零() {
    use value_support::记;
    let fk: &'static str = Box::leak(z0385_题式键().into_boxed_str());
    let calib = value_support::认证库2(
        &[
            记 {
                key: "fa",
                j: 0,
                keep: None,
                kind: None,
                status: Some("停岗候选"),
            },
            记 {
                key: fk,
                j: 0,
                keep: None,
                kind: None,
                status: None,
            },
        ],
        false,
    );
    assert_eq!(z0385_价值(&calib), 0.0);
}

/// T4：题式停岗时 `cut` 不借类线（按冷切）；价值也不能取类记录，应为 0
#[test]
fn z0385_题式停岗不借类时价值为零() {
    use value_support::记;
    let fk: &'static str = Box::leak(z0385_题式键().into_boxed_str());
    let calib = value_support::认证库2(
        &[
            记 {
                key: fk,
                j: 0,
                keep: None,
                kind: None,
                status: Some("停岗"),
            },
            记 {
                key: "\u{1f}class\u{1f}fa",
                j: 0,
                keep: None,
                kind: None,
                status: None,
            },
        ],
        false,
    );
    assert_eq!(z0385_价值(&calib), 0.0);
}
