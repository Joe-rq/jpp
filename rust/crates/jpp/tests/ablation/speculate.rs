//! 层内挑选与推测的白花账（步 22 / B0487；`20·B43`、`20·B51` C1、`21`:291）。
//!
//! - `W-spec-unused` 只在「发出、跨状态、没用上」三者同时成立时出现（主控 Z0209 Q4）：同状态并进真站点调用的、
//!   因预算没发出的不报；
//! - 本层超出剩余预算时 `select_within` 让真站点先发、跨状态推测先让；开关关时退回登记顺序（消融矩阵第 ⑧ 行）；
//! - 不超预算时两臂账本逐字节相同；审计重放停在同一处、零新调用（B35）；续跑重发的真站点排在推测前面。
//!
//! 每条的数值在过程记录 `地基/过程记录/工程-步22-层内挑选.md` §3.3 预注册。

use std::cell::RefCell;

use jpp::ActionRegistry;
use jpp::effects::{CalibStore, EffectError, FnPort, JudgeResult, Ports};
use jpp::interp::{Interp, Passes};
use jpp::ledger::Ledger;
use jpp::value::Answer;
use jpp::{lower, syntax::parse};
use serde_json::{Value, json};

fn 端口<'a>(每次题数: &'a RefCell<Vec<usize>>) -> Ports<'a> {
    Ports::new()
        .with(FnPort::judge("fixed-0", move |_s, qs| {
            每次题数.borrow_mut().push(qs.len());
            Ok(JudgeResult {
                answers: qs.iter().map(|_| Answer::Noul(0.9)).collect(),
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
    /// 端口实际收到的调用，每次几道题
    每次题数: Vec<usize>,
    calls: u64,
}

impl 结果 {
    fn 有(&self, 前缀: &str) -> bool {
        self.告警.iter().any(|w| w.starts_with(前缀))
    }
}

#[derive(Clone, Copy)]
enum 方式 {
    首跑,
    续跑,
    审计,
}

fn 跑(
    src: &str, calls: u64, 挑选: bool, fuse: bool, ledger: &mut Ledger, 方式: 方式
) -> 结果 {
    let program = lower(&parse(src).expect("解析")).expect("lower");
    let mut calib = CalibStore::new();
    calib.put("k", 0.65, 0.35, 100, "上岗", Some(0.05)).unwrap();
    let 每次题数 = RefCell::new(vec![]);
    let actions = ActionRegistry::new();
    let mut budget = program.budget.clone();
    budget.calls = calls;
    let mut it = Interp::new(端口(&每次题数), ledger, &calib, &actions, budget);
    if matches!(方式, 方式::审计) {
        it = it.audit_replay();
    }
    it.passes = Passes {
        select_within: 挑选,
        // 步 30（B0488）：这批断言钉步 22 那一臂（类别先后、重发预判），关掉关键路径与价值密度排序；
        // 开时的行为在 ablation/plan.rs
        critical_path: false,
        fuse,
        ..Passes::default()
    };
    let out = it
        .run(&program)
        .unwrap_or_else(|e| panic!("程序应当跑完：{}", e.render()));
    结果 {
        值: out.value_json(),
        告警: out.trace.warnings.clone(),
        每次题数: 每次题数.into_inner(),
        calls: out.cost.calls,
    }
}

/// 夹具甲：`let c` 之前推测登记分支里的甲、乙（`eval.rs` 在 `let` 值求值前推测），本层登记顺序是甲、乙（推测）、条件（真）。
const 甲: &str = r#"
budget {calls: 10, cost: 1, depth: 8};
let 甲 = state(mat("甲料"));
let 乙 = state(mat("乙料"));
let q = test("行吗", "k");
let c = cut(judge(state(mat("条件料")), q));
let ck = exit_kind(c);
let 结果 = if ck == "act" {
    let e = cut(judge(甲, q));
    {side: "甲", k: exit_kind(e), e: e}
} else {
    let e = cut(judge(乙, q));
    {side: "乙", k: exit_kind(e), e: e}
};
{条件: ck, c: c, r: 结果}
"#;

fn 看(x: &结果) -> (Value, Value, Value) {
    (
        x.值["条件"].clone(),
        x.值["r"]["side"].clone(),
        x.值["r"]["k"].clone(),
    )
}

/// 预注册 §3.3 表：calls 3 两臂相同（不超额不重排），账本逐字节相同
#[test]
fn 不超预算时两臂逐字节相同() {
    let (mut l开, mut l关) = (Ledger::new(), Ledger::new());
    let 开 = 跑(甲, 3, true, true, &mut l开, 方式::首跑);
    let 关 = 跑(甲, 3, false, true, &mut l关, 方式::首跑);
    for x in [&开, &关] {
        assert_eq!(x.calls, 3);
        assert_eq!(看(x), (json!("act"), json!("甲"), json!("act")));
        assert!(!x.有("W-budget"), "{:?}", x.告警);
        assert!(
            x.有("W-spec-unused"),
            "乙发出了、跨状态、没用上：{:?}",
            x.告警
        );
    }
    assert_eq!(
        serde_json::to_string(&l开.entries).unwrap(),
        serde_json::to_string(&l关.entries).unwrap(),
        "不超额时发出集合与顺序不变"
    );
}

/// calls 2：开臂先发条件、再发甲，乙让出预算；关臂把两次都花在推测上，条件 Unsure(budget)
#[test]
fn 两次预算_真站点先发() {
    let 开 = 跑(甲, 2, true, true, &mut Ledger::new(), 方式::首跑);
    assert_eq!(开.calls, 2);
    assert_eq!(看(&开), (json!("act"), json!("甲"), json!("act")));
    assert!(!开.有("W-budget"), "{:?}", 开.告警);
    assert!(!开.有("W-spec-unused"), "乙没发出，不报：{:?}", 开.告警);

    let 关 = 跑(甲, 2, false, true, &mut Ledger::new(), 方式::首跑);
    assert_eq!(关.calls, 2);
    assert_eq!(
        看(&关),
        (json!("unsure(budget)"), json!("乙"), json!("act"))
    );
    assert!(关.有("W-budget"), "{:?}", 关.告警);
    assert!(关.有("W-spec-unused"), "甲发出了没用上：{:?}", 关.告警);
}

/// calls 1：开臂那一次给条件；关臂给了推测甲，条件与乙都 Unsure(budget)
#[test]
fn 一次预算_真站点先发() {
    let 开 = 跑(甲, 1, true, true, &mut Ledger::new(), 方式::首跑);
    assert_eq!(开.calls, 1);
    assert_eq!(
        看(&开),
        (json!("act"), json!("甲"), json!("unsure(budget)"))
    );
    assert!(开.有("W-budget"), "{:?}", 开.告警);
    assert!(!开.有("W-spec-unused"), "{:?}", 开.告警);

    let 关 = 跑(甲, 1, false, true, &mut Ledger::new(), 方式::首跑);
    assert_eq!(关.calls, 1);
    assert_eq!(
        看(&关),
        (
            json!("unsure(budget)"),
            json!("乙"),
            json!("unsure(budget)")
        )
    );
    assert!(关.有("W-budget"), "{:?}", 关.告警);
    assert!(关.有("W-spec-unused"), "{:?}", 关.告警);
}

/// 审计重放（B35）：开臂首跑的账本只凭账本重放，出口与告警有无相同，零新调用
#[test]
fn 审计重放停在同一处() {
    for calls in [2, 1] {
        let mut l = Ledger::new();
        let 首 = 跑(甲, calls, true, true, &mut l, 方式::首跑);
        let 重 = 跑(甲, calls, true, true, &mut l, 方式::审计);
        assert!(重.每次题数.is_empty(), "重放不发新调用：{:?}", 重.每次题数);
        assert_eq!(看(&重), 看(&首), "calls {calls}");
        for w in ["W-budget", "W-spec-unused"] {
            assert_eq!(重.有(w), 首.有(w), "calls {calls} {w}：{:?}", 重.告警);
        }
    }
}

/// 夹具乙：分支里的 q2 与条件同材料，进了条件那次调用（同状态推测）；走甲那侧，q2 没用上
const 同材料: &str = r#"
budget {calls: 10, cost: 1, depth: 8};
let 甲 = state(mat("甲料"));
let q = test("行吗", "k");
let q2 = test("另一问", "k");
let c = cut(judge(state(mat("条件料")), q));
let ck = exit_kind(c);
let 结果 = if ck == "act" {
    let e = cut(judge(甲, q));
    {side: "甲", k: exit_kind(e), e: e}
} else {
    let e = cut(judge(state(mat("条件料")), q2));
    {side: "条件料", k: exit_kind(e), e: e}
};
{条件: ck, c: c, r: 结果}
"#;

#[test]
fn 同状态并进真站点调用_没用上不报() {
    let x = 跑(同材料, 10, true, true, &mut Ledger::new(), 方式::首跑);
    assert_eq!(x.calls, 2);
    let mut 题数 = x.每次题数.clone();
    题数.sort();
    assert_eq!(
        题数,
        vec![1, 2],
        "条件料一次两道（条件 + q2），甲料一次一道"
    );
    assert_eq!(x.值["r"]["side"], json!("甲"));
    assert!(!x.有("W-spec-unused"), "同状态合并不算推测：{:?}", x.告警);
}

/// 复核缺口 4（过程记录 §六）：本层超额时同状态推测不被剥——q2 随条件那次调用整组发出，甲让出预算
#[test]
fn 超额时同状态推测随真站点整组发出() {
    let x = 跑(同材料, 1, true, true, &mut Ledger::new(), 方式::首跑);
    assert_eq!(x.calls, 1);
    assert_eq!(x.每次题数, vec![2], "条件与 q2 一次两道");
    assert_eq!(看(&x), (json!("act"), json!("甲"), json!("unsure(budget)")));
    assert!(x.有("W-budget"), "{:?}", x.告警);
    assert!(!x.有("W-spec-unused"), "{:?}", x.告警);
}

#[test]
fn 关融合时同材料也各付一次_没用上照报() {
    let x = 跑(同材料, 10, true, false, &mut Ledger::new(), 方式::首跑);
    assert_eq!(x.calls, 3);
    assert_eq!(x.值["r"]["side"], json!("甲"));
    assert!(x.有("W-spec-unused"), "q2 自付一次调用：{:?}", x.告警);
}

/// 夹具丙：本层登记顺序甲、乙（推测）、丙、条件（真）。首跑 calls 1 发丙、条件停发；续跑同账本仍只 1 次，
/// 条件（首因 budget 重发）要排在推测前面——预判少算重发的组，推测就会再次抢先
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

#[test]
fn 续跑时重发的真站点先于推测() {
    let mut l = Ledger::new();
    let 首 = 跑(续跑, 1, true, true, &mut l, 方式::首跑);
    assert_eq!(首.calls, 1);
    assert_eq!(首.值["a"], json!("act"));
    assert_eq!(
        看(&首),
        (
            json!("unsure(budget)"),
            json!("乙"),
            json!("unsure(budget)")
        )
    );
    let 续 = 跑(续跑, 1, true, true, &mut l, 方式::续跑);
    assert_eq!(续.calls, 1);
    assert_eq!(续.值["a"], json!("act"));
    assert_eq!(
        看(&续),
        (json!("act"), json!("甲"), json!("unsure(budget)"))
    );
}

/// 补充预注册 §3.5 第 1 例：续跑预算 2。预判算对（数上首因 budget 要重发的条件）才会重排，条件先发；
/// 少算或关臂时甲、乙先把两次花掉，条件再次 Unsure(budget)
#[test]
fn 续跑预算二_预判数上重发的真站点() {
    let mut l = Ledger::new();
    let 首 = 跑(续跑, 1, true, true, &mut l, 方式::首跑);
    assert_eq!(看(&首).0, json!("unsure(budget)"));
    let 续 = 跑(续跑, 2, true, true, &mut l, 方式::续跑);
    assert_eq!(续.calls, 2);
    assert_eq!(看(&续), (json!("act"), json!("甲"), json!("act")));

    let mut l = Ledger::new();
    跑(续跑, 1, false, true, &mut l, 方式::首跑);
    let 关 = 跑(续跑, 2, false, true, &mut l, 方式::续跑);
    assert_eq!(关.calls, 2);
    assert_eq!(
        看(&关),
        (json!("unsure(budget)"), json!("乙"), json!("act"))
    );
}

/// 补充预注册 §3.5 第 2 例：前一层用完预算后，用户函数体里再推测登记 `if` 两侧。只含推测的组被整组拿空，
/// 不再排进本层（改前 `flush_before_send` 取 `group[0]` 越界）
const 用完后再推测: &str = r#"
budget {calls: 10, cost: 1, depth: 8};
let 甲 = state(mat("甲料"));
let 乙 = state(mat("乙料"));
let q = test("行吗", "k");
fn 分支(x, y, qq) !{judge} {
    let c = cut(judge(state(mat("条件料")), qq));
    let ck = exit_kind(c);
    let r = if ck == "act" {
        let e = cut(judge(x, qq));
        {side: "甲", k: exit_kind(e), e: e}
    } else {
        let e = cut(judge(y, qq));
        {side: "乙", k: exit_kind(e), e: e}
    };
    {条件: ck, c: c, r: r}
}
let a = cut(judge(state(mat("丙料")), q));
let ak = exit_kind(a);
let b = 分支(甲, 乙, q);
{ak: ak, a: a, 条件: b.条件, r: b.r, c: b.c}
"#;

#[test]
fn 预算用完后再推测_两臂都不越界() {
    for 挑选 in [true, false] {
        let x = 跑(用完后再推测, 1, 挑选, true, &mut Ledger::new(), 方式::首跑);
        assert_eq!(x.calls, 1, "挑选 {挑选}");
        assert_eq!(x.值["ak"], json!("act"));
        assert_eq!(
            看(&x),
            (
                json!("unsure(budget)"),
                json!("乙"),
                json!("unsure(budget)")
            ),
            "挑选 {挑选}"
        );
        assert!(x.有("W-budget"), "{:?}", x.告警);
        assert!(!x.有("W-spec-unused"), "{:?}", x.告警);
    }
}
