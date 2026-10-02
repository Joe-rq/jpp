//! Z0602（主控板 Z0601；过程记录 5.30；复核 `复核-Z0577.md` 第五节）：缺席类未决在默认链末端随值走——不当场写
//! Handoff，随返回值离开程序时由程序结束写，值被丢掉则段末记违规（裁定五十七备案 (2)、B95）。路 C 不变。
//! 另：合并出口末端只写未解除的键。

mod common;
use jpp::effects::{CalibStore, FnPort, JudgeResult, Ports};
use jpp::ledger::{Entry, Ledger};
use jpp::value::Answer;
use jpp::{ActionRegistry, Outcome, lower, syntax::parse};

/// 「甲」读 0.9，其余读 0.5（并列）
fn 跑(src: &str) -> (Outcome, Ledger) {
    let program = lower(&parse(src).expect("解析")).expect("lower");
    let ports = Ports::new().with(FnPort::judge("fixed-0", |s, qs| {
        let 甲 = s.on.iter().any(|m| m.text().contains('甲'));
        Ok(JudgeResult {
            answers: qs
                .iter()
                .map(|_| Answer::Noul(if 甲 { 0.9 } else { 0.5 }))
                .collect(),
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

fn 转交(l: &Ledger) -> Vec<Vec<String>> {
    l.entries
        .iter()
        .filter_map(|e| match e {
            Entry::Handoff { of, .. } => Some(of.clone()),
            _ => None,
        })
        .collect()
}

/// 预算只够一次调用：第二道 Unsure(budget)，交给缺臂的 handle 走默认链
const D3头: &str = "budget {calls: 1, cost: 0, depth: 16};
let e1 = cut(judge(state(mat(\"甲\")), test(\"行吗\", \"k\")));
let e2 = cut(judge(state(mat(\"乙\")), test(\"行吗\", \"k\")));
let v1 = handle(e1, {act: fn() { 1 }, ignore: fn() { 0 }, unsure: fn(u) { {exit: u} }});
let v2 = handle(e2, {act: fn() { 1 }, ignore: fn() { 0 }});
";

/// d3：值被丢掉 → 段末记违规，没有当场写的 Handoff；报告行 end 为 carry
#[test]
fn d3_缺席类走链后被丢_记违规() {
    let (o, l) = 跑(&format!("{D3头}{{v1: v1}}\n"));
    assert_eq!(o.unsure_default.len(), 1, "{:?}", o.unsure_default);
    assert_eq!(o.unsure_default[0]["cause"], "budget");
    assert_eq!(o.unsure_default[0]["end"], "carry");
    assert!(转交(&l).is_empty(), "不当场写 Handoff：{:?}", 转交(&l));
    assert_eq!(o.violations.len(), 1, "{:?}", o.violations);
    assert_eq!(o.violations[0].mark.cause, "budget");
}

/// d3 把值返回：程序结束时随返回值写一条 Handoff，无违规
#[test]
fn d3_缺席类走链后随值返回_一条转交() {
    let (o, l) = 跑(&format!("{D3头}{{v1: v1, v2: v2}}\n"));
    assert!(o.violations.is_empty(), "{:?}", o.violations);
    assert_eq!(转交(&l).len(), 1, "{:?}", 转交(&l));
    assert_eq!(o.unsure_default[0]["end"], "carry");
}

/// 路 C 不变：缺的类别进 needed、当场转交；值丢掉也只一条 Handoff、无违规
#[test]
fn 路c不变_值丢掉也不记违规() {
    let src = "budget {calls: 4, cost: 0, depth: 16};
unsure_source({need: [\"过往项目\"]});
let e = cut(judge(state(mat(\"乙\")), test(\"行吗\", \"k\")));
let v = handle(e, {act: fn() { 1 }, ignore: fn() { 0 }});
1
";
    let (o, l) = 跑(src);
    assert_eq!(o.unsure_default[0]["end"], "handoff");
    assert_eq!(
        o.unsure_default[0]["needed"],
        serde_json::json!(["过往项目"])
    );
    assert_eq!(转交(&l).len(), 1);
    assert!(o.violations.is_empty(), "{:?}", o.violations);
}

/// 合并出口：一部分键已由别处解除时，程序结束写的 Handoff 只含其余键
#[test]
fn 合并出口只写未解除的键() {
    let src = "budget {calls: 4, cost: 0, depth: 16};
let a = cut(judge(state(mat(\"乙\")), test(\"行吗\", \"k\")));
let b = cut(judge(state(mat(\"丙\")), test(\"行吗\", \"k\")));
let c = compose([a, b], \"all\");
consume(a, \"drop\");
{c: c}
";
    let (o, l) = 跑(src);
    let 丢: Vec<Vec<String>> = l
        .entries
        .iter()
        .filter_map(|e| match e {
            Entry::Drop { of, .. } => Some(of.clone()),
            _ => None,
        })
        .collect();
    assert_eq!(丢.len(), 1, "{:?}", l.entries);
    let 交 = 转交(&l);
    assert!(!交.is_empty(), "{:?}", l.entries);
    for of in &交 {
        assert!(
            !of.contains(&丢[0][0]),
            "已解除的键不再写进 Handoff：{交:?}"
        );
    }
    assert!(o.violations.is_empty(), "{:?}", o.violations);
}
