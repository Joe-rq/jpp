//! G4（步 37）：深度口径（预注册 `地基/过程记录/工程-G4-深度口径.md` §二·2 D-3、D-4、D-5）。
//!
//! 深度到限的一趟只停发、照常求值：`gen`、`do` 产出失败值且动作不执行，`ask` 不问人、给 `Unsure(depth)`；
//! `Unsure(depth)` 属缺席类，`drop` 报 `E-drop-unobserved`；`--resume` 同一轮时 `hop` 不变、`round` 加一，
//! 事件的 `attempt.n = round + 1`。
//!
//! 依据：裁定五十九第 7、17 条；裁定六十二第 1 条；`12` §2.13 R11 第 4、5 条、R13、R15；主控 2026-09-30 对 G4 五处的答复。

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use jpp::effects::{CalibStore, FnPort, GenResult, JudgeResult, Ports};
use jpp::ledger::{CarryRecord, Entry, Ledger, StopCause};
use jpp::value::{Answer, Value};
use jpp::{ActionRegistry, BudgetCarry, EntryArgs, Outcome, Session, TaintOut};
use jpp::{lower, syntax::parse};

fn 端口<'a>(calls: &'a Cell<u64>) -> Ports<'a> {
    Ports::new()
        .with(FnPort::judge("fixed-0", move |_s, qs| {
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
        .with(FnPort::generate("fixed-0", move |_p, _c, n, _r| {
            Ok(GenResult {
                outputs: (0..n)
                    .map(|i| serde_json::json!(format!("生成{i}")))
                    .collect(),
                tokens: 0,
                cost: 0.0,
                ..Default::default()
            })
        }))
}

fn 程序(src: &str) -> jpp::Program {
    lower(&parse(src).expect("解析")).expect("lower")
}

fn 余额(hop: u32, cap: u32) -> BudgetCarry {
    BudgetCarry::from_record(CarryRecord {
        calls: 50,
        cost: 0.0,
        latency_p95: None,
        escalate: 3,
        hop,
        round: 0,
        depth_cap: cap,
    })
}

fn 跑(
    src: &str,
    carry: BudgetCarry,
    l: &mut Ledger,
    calls: &Cell<u64>,
    acts: &ActionRegistry,
) -> Result<Outcome, (Option<String>, String)> {
    let calib = CalibStore::new();
    Session::new(端口(calls), &calib, acts)
        .with_companions(jpp::interp::CompanionMode::Off)
        .with_carry(Some(carry))
        .run(&程序(src), &EntryArgs::default(), l)
        .map_err(|e| match e {
            jpp::Error::Runtime(r) => (r.rule.clone(), r.message.clone()),
            other => (None, other.render()),
        })
}

/// D-3：深度到限，`gen`、`do` 失败值、动作不执行；`ask` 给 `Unsure(depth)`、不挂起；判断给 `Unsure(depth)`
#[test]
fn d3_深度到限_效应失败值_问人给未决() {
    let src = r#"
budget {calls: 20, cost: 0, depth: 16, escalate: 3};
let a = gen("写一句", [], 1, 0);
let c = do("落盘", ["x"], 0);
let q = ask(state(mat("甲")), test("行吗", "k"));
let j = cut(judge(state(mat("乙")), test("好吗", "k")));
{a: is_fail(a), c: is_fail(c), q: exit_kind(q), j: exit_kind(j), pq: q, pj: j}
"#;
    let 执行 = Rc::new(RefCell::new(0));
    let e2 = 执行.clone();
    let mut acts = ActionRegistry::new();
    acts.register("落盘", 0.0, true, TaintOut::Trusted, move |_| {
        *e2.borrow_mut() += 1;
        Ok(Value::text("写了"))
    });
    let calls = Cell::new(0);
    let mut l = Ledger::new();
    let o =
        跑(src, 余额(4, 4), &mut l, &calls, &acts).unwrap_or_else(|e| panic!("应当跑完：{e:?}"));
    let v = o.value_json();
    assert_eq!(v["a"], true);
    assert_eq!(v["c"], true);
    assert_eq!(v["q"], "unsure(depth)");
    assert_eq!(v["j"], "unsure(depth)");
    assert_eq!(*执行.borrow(), 0, "动作不执行");
    assert_eq!(calls.get(), 0, "一道题都不发");
    assert_eq!(o.cost.asks, 0, "不问人");
    assert_eq!(
        o.budget.as_ref().and_then(|b| b.cause.as_deref()),
        Some("depth")
    );
    let 停下: Vec<&Entry> = l
        .entries
        .iter()
        .filter(|e| matches!(e, Entry::Stop { .. }))
        .collect();
    assert_eq!(停下.len(), 1, "整趟一条停下：{停下:?}");
    assert!(matches!(
        停下[0],
        Entry::Stop {
            cause: StopCause::Depth,
            ..
        }
    ));
    let 未问 = l
        .entries
        .iter()
        .filter(|e| {
            matches!(
                e,
                Entry::Unasked {
                    reason: StopCause::Depth,
                    ..
                }
            )
        })
        .count();
    assert_eq!(未问, 1, "判断题一条未问（ask 不是题，不记未问）");
    assert!(
        !l.entries
            .iter()
            .any(|e| matches!(e, Entry::Intent { .. } | Entry::Effect { .. }))
    );
}

/// D-4：`Unsure(depth)` 属缺席类，不能 drop
#[test]
fn d4_深度未决不能drop() {
    let src = r#"
budget {calls: 20, cost: 0, depth: 16};
let e = cut(judge(state(mat("甲")), test("行吗", "k")));
consume(e, "drop");
0
"#;
    let calls = Cell::new(0);
    let Err((rule, msg)) = 跑(
        src,
        余额(2, 2),
        &mut Ledger::new(),
        &calls,
        &ActionRegistry::new(),
    ) else {
        panic!("drop Unsure(depth) 应当报 E-drop-unobserved")
    };
    assert_eq!(rule.as_deref(), Some("E-drop-unobserved"), "{msg}");
    assert!(
        msg.contains("depth") && msg.contains("深度到限停发"),
        "{msg}"
    );
}

/// D-5：`--resume` 同一轮时 `hop` 不变、`round` 加一，事件的 `attempt.n = round + 1`；交回 `hop + 1`、`round` 归 0
#[test]
fn d5_续跑_round加一_hop不变() {
    let src = r#"
budget {calls: 5, cost: 0, depth: 16};
let a = cut(judge(state(mat("甲")), test("行吗", "k")));
{a: exit_kind(a), pa: a}
"#;
    let calls = Cell::new(0);
    let mut l = Ledger::new();
    let 未问n = |l: &Ledger| -> Vec<u64> {
        l.entries
            .iter()
            .filter_map(|e| match e {
                Entry::Unasked { attempt, .. } => Some(attempt.n),
                _ => None,
            })
            .collect()
    };
    let 层 = |o: &Outcome| {
        o.trace
            .warnings
            .iter()
            .find(|w| w.starts_with("W-budget"))
            .cloned()
            .unwrap_or_default()
    };
    let o1 = 跑(src, 余额(3, 3), &mut l, &calls, &ActionRegistry::new()).expect("首趟");
    assert_eq!(o1.value_json()["a"], "unsure(depth)");
    assert_eq!(未问n(&l), [1], "首趟 round 0 → n 1");
    assert!(层(&o1).contains("第 3 层"), "{}", 层(&o1));
    let 交回1 = o1.carry.clone().unwrap();
    assert_eq!(
        (交回1.depth_at(), 交回1.record().round),
        (4, 0),
        "交回给下游：hop + 1、round 归 0"
    );
    // 续跑同一轮：宿主传上一趟交回的余额；本趟 hop 仍是这一轮的 3（续跑不加），round 1
    let o2 = 跑(src, 交回1, &mut l, &calls, &ActionRegistry::new()).expect("续跑");
    assert_eq!(o2.value_json()["a"], "unsure(depth)");
    assert_eq!(未问n(&l), [1, 2], "续跑 round 1 → n 2");
    assert!(层(&o2).contains("第 3 层"), "续跑不加 hop：{}", 层(&o2));
    let 交回2 = o2.carry.clone().unwrap();
    assert_eq!((交回2.depth_at(), 交回2.record().round), (4, 0));
    assert_eq!(calls.get(), 0);
}

fn 余额_calls(hop: u32, cap: u32, calls: u64) -> BudgetCarry {
    BudgetCarry::from_record(CarryRecord {
        calls,
        cost: 0.0,
        latency_p95: None,
        escalate: 0,
        hop,
        round: 0,
        depth_cap: cap,
    })
}

/// D-9（附录二，复核反例 B3）：余额 `hop 2 = cap 2` 且 calls 0：计划期不按下界拒绝，照常求值、逐题 `Unsure(depth)`
#[test]
fn d9_深度到限且余额为零_计划期不拒() {
    let src = r#"
budget {calls: 5, cost: 0, depth: 16};
let a = cut(judge(state(mat("甲")), test("行吗", "k")));
{a: exit_kind(a), pa: a}
"#;
    let calls = Cell::new(0);
    let o = 跑(
        src,
        余额_calls(2, 2, 0),
        &mut Ledger::new(),
        &calls,
        &ActionRegistry::new(),
    )
    .unwrap_or_else(|e| panic!("不该在计划期拒绝：{e:?}"));
    assert_eq!(o.value_json()["a"], "unsure(depth)");
    assert_eq!(
        o.budget.as_ref().and_then(|b| b.cause.as_deref()),
        Some("depth")
    );
    assert_eq!(calls.get(), 0);
}

/// D-10（附录二，复核反例 B4）：余额 `hop 2 = cap 2`、calls 1，两个必经判断层：照常求值、两题都 `Unsure(depth)`、调用 0
#[test]
fn d10_深度到限且下界超余额_计划期不拒() {
    let src = r#"
budget {calls: 5, cost: 0, depth: 16};
let a = cut(judge(state(mat("甲")), test("行吗", "k")));
let b = cut(judge(state(mat(exit_kind(a))), test("好吗", "k")));
{a: exit_kind(a), b: exit_kind(b), pa: a, pb: b}
"#;
    let calls = Cell::new(0);
    let o = 跑(
        src,
        余额_calls(2, 2, 1),
        &mut Ledger::new(),
        &calls,
        &ActionRegistry::new(),
    )
    .unwrap_or_else(|e| panic!("不该在计划期拒绝：{e:?}"));
    assert_eq!(o.value_json()["a"], "unsure(depth)");
    assert_eq!(o.value_json()["b"], "unsure(depth)");
    assert_eq!(calls.get(), 0);
}

/// D-11（附录二）：没到深度上限时计划期拒绝照旧
#[test]
fn d11_没到深度上限_计划期照旧拒() {
    let src = r#"
budget {calls: 5, cost: 0, depth: 16};
let a = cut(judge(state(mat("甲")), test("行吗", "k")));
{a: exit_kind(a), pa: a}
"#;
    let calls = Cell::new(0);
    let Err((rule, msg)) = 跑(
        src,
        余额_calls(0, 2, 0),
        &mut Ledger::new(),
        &calls,
        &ActionRegistry::new(),
    ) else {
        panic!("余额 calls 0、没到深度上限：应当计划期拒绝")
    };
    assert_eq!(rule.as_deref(), Some("E-budget-plan"), "{msg}");
    assert_eq!(calls.get(), 0);
}

// ---------- G4b（步 37b，预注册 `地基/过程记录/工程-G4b-深度只数跳数.md` E-1 至 E-6） ----------

fn 递归判(depth: Option<u32>, n: u32) -> String {
    let d = depth.map(|d| format!(", depth: {d}")).unwrap_or_default();
    format!(
        "budget {{calls: 5, cost: 0{d}}};\nfn f(n) {{ if n > 0 {{ f(n - 1) }} else {{ 0 }} }}\nlet r = f({n});\nlet a = cut(judge(state(mat(\"甲\")), test(\"行吗\", \"k\")));\n{{r: r, a: exit_kind(a), pa: a}}\n"
    )
}

fn 深度余额(hop: u32, cap: u32) -> BudgetCarry {
    余额_calls(hop, cap, 20)
}

fn carry_caps(l: &Ledger) -> Vec<u32> {
    l.entries
        .iter()
        .filter_map(|e| match e {
            Entry::CarryCap { depth_cap, .. } => Some(*depth_cap),
            _ => None,
        })
        .collect()
}

/// E-1（证明一）：递归 20 层深于余额 depth_cap 3、没有跨程序跳：照常跑完，判断照常发
#[test]
fn e1_递归深于depth_cap且无跨程序跳_照常跑完() {
    let calls = Cell::new(0);
    let o = 跑(
        &递归判(Some(32), 19),
        深度余额(0, 3),
        &mut Ledger::new(),
        &calls,
        &ActionRegistry::new(),
    )
    .unwrap_or_else(|e| panic!("递归不受余额上限限：{e:?}"));
    assert_eq!(o.value_json()["r"], 0);
    assert_eq!(o.value_json()["a"], "act");
    assert_eq!(calls.get(), 1);
}

/// E-2：不带余额、声明 depth 3、递归 4 层：仍报 J-06「递归无界」
#[test]
fn e2_递归超声明仍报j06() {
    let calib = CalibStore::new();
    let calls = Cell::new(0);
    let e = Session::new(端口(&calls), &calib, &ActionRegistry::new())
        .with_companions(jpp::interp::CompanionMode::Off)
        .run(
            &程序(&递归判(Some(3), 3)),
            &EntryArgs::default(),
            &mut Ledger::new(),
        )
        .expect_err("递归 4 层超声明 3");
    let m = e.render();
    assert!(m.contains("J-06") && m.contains("递归无界"), "{m}");
}

/// E-3：余额 hop 2、depth_cap 8，程序声明 depth 2：停发（hop ≥ min(2, 8)）；一条 CarryCap(2)；交回 cap 2、hop 3
#[test]
fn e3_声明更小_按声明停发并收紧交回() {
    let calls = Cell::new(0);
    let mut l = Ledger::new();
    let o = 跑(
        &递归判(Some(2), 0),
        深度余额(2, 8),
        &mut l,
        &calls,
        &ActionRegistry::new(),
    )
    .expect("跑完");
    assert_eq!(o.value_json()["a"], "unsure(depth)");
    assert_eq!(calls.get(), 0);
    assert_eq!(carry_caps(&l), [2]);
    let c = o.carry.clone().unwrap();
    assert_eq!((c.depth_cap(), c.depth_at()), (2, 3));
}

/// E-4：余额 hop 0、depth_cap 256，声明 depth 16：不停发；一条 CarryCap(16)；交回 cap 16；续跑不再写第二条
#[test]
fn e4_声明收紧交回上限_续跑不重复记() {
    let calls = Cell::new(0);
    let mut l = Ledger::new();
    let o = 跑(
        &递归判(Some(16), 0),
        深度余额(0, 256),
        &mut l,
        &calls,
        &ActionRegistry::new(),
    )
    .expect("首趟");
    assert_eq!(o.value_json()["a"], "act");
    assert_eq!(carry_caps(&l), [16]);
    let c = o.carry.clone().unwrap();
    assert_eq!(c.depth_cap(), 16);
    let o2 = 跑(
        &递归判(Some(16), 0),
        c,
        &mut l,
        &calls,
        &ActionRegistry::new(),
    )
    .expect("续跑");
    assert!(
        o2.trace.warnings.iter().all(|w| !w.starts_with("W-carry")),
        "{:?}",
        o2.trace.warnings
    );
    assert_eq!(carry_caps(&l), [16], "续跑不再写第二条");
    assert_eq!(o2.carry.unwrap().depth_cap(), 16);
}

/// E-5（证明二）：不声明 depth、引擎默认 5 跑：账本头记 5；换引擎默认 3 审计重放，值相同、零调用（J-06 取账本头的 5）
#[test]
fn e5_引擎默认进账本头_重放不读引擎配置() {
    let calib = CalibStore::new();
    let calls = Cell::new(0);
    let mut l = Ledger::new();
    let src = 递归判(None, 3); // 递归 4 层：默认 5 放得下，默认 3 放不下
    let o = Session::new(端口(&calls), &calib, &ActionRegistry::new())
        .with_companions(jpp::interp::CompanionMode::Off)
        .with_depth_cap_default(5)
        .run(&程序(&src), &EntryArgs::default(), &mut l)
        .unwrap_or_else(|e| panic!("默认 5：{}", e.render()));
    assert_eq!(l.header.as_ref().unwrap().depth_cap_default, Some(5));
    let calls2 = Cell::new(0);
    let o2 = Session::new(端口(&calls2), &calib, &ActionRegistry::new())
        .with_companions(jpp::interp::CompanionMode::Off)
        .with_depth_cap_default(3)
        .replay(&程序(&src), &EntryArgs::default(), &mut l)
        .unwrap_or_else(|e| panic!("重放应取账本头的 5：{}", e.render()));
    assert_eq!(o2.value_json(), o.value_json());
    assert_eq!(calls2.get(), 0);
    // 对照：首跑就用默认 3，同一程序报 J-06
    let e = Session::new(端口(&calls), &calib, &ActionRegistry::new())
        .with_companions(jpp::interp::CompanionMode::Off)
        .with_depth_cap_default(3)
        .run(&程序(&src), &EntryArgs::default(), &mut Ledger::new())
        .expect_err("默认 3 放不下 4 层递归");
    assert!(e.render().contains("J-06"), "{}", e.render());
}

/// E-6：声明了 depth、不带余额：账本头不写 depth_cap_default
#[test]
fn e6_没用到默认不写账本头() {
    let calib = CalibStore::new();
    let calls = Cell::new(0);
    let mut l = Ledger::new();
    Session::new(端口(&calls), &calib, &ActionRegistry::new())
        .with_companions(jpp::interp::CompanionMode::Off)
        .run(&程序(&递归判(Some(16), 0)), &EntryArgs::default(), &mut l)
        .unwrap_or_else(|e| panic!("{}", e.render()));
    assert_eq!(l.header.as_ref().unwrap().depth_cap_default, None);
    assert!(!l.encode().contains("depth_cap_default"));
}

// ---------- G4b 附录一（复核 C1、Q5、Q6） ----------

/// E-7、E-8：旧账本（头里没有 depth_cap_default）、程序没声明 depth、递归 4 层：宿主默认设 3 审计重放，
/// 取历史默认 256——值同、零调用、不报 J-06；写回的头仍然没有这个字段
#[test]
fn e7_旧账本缺字段_审计重放取历史默认_不补写() {
    let calib = CalibStore::new();
    let calls = Cell::new(0);
    let mut l = Ledger::new();
    let src = 递归判(None, 3);
    let o = Session::new(端口(&calls), &calib, &ActionRegistry::new())
        .with_companions(jpp::interp::CompanionMode::Off)
        .run(&程序(&src), &EntryArgs::default(), &mut l)
        .unwrap_or_else(|e| panic!("首跑：{}", e.render()));
    // 造一份 G4b 之前的账本：去掉头里的字段
    l.header.as_mut().unwrap().depth_cap_default = None;
    let calls2 = Cell::new(0);
    let o2 = Session::new(端口(&calls2), &calib, &ActionRegistry::new())
        .with_companions(jpp::interp::CompanionMode::Off)
        .with_depth_cap_default(3)
        .replay(&程序(&src), &EntryArgs::default(), &mut l)
        .unwrap_or_else(|e| panic!("重放应取历史默认 256：{}", e.render()));
    assert_eq!(o2.value_json(), o.value_json());
    assert_eq!(calls2.get(), 0);
    assert_eq!(
        l.header.as_ref().unwrap().depth_cap_default,
        None,
        "审计重放不补写"
    );
}

/// E-9：账本头有 5、宿主默认设 3 审计重放：写回的头仍是 5
#[test]
fn e9_审计重放照抄头里的默认() {
    let calib = CalibStore::new();
    let calls = Cell::new(0);
    let mut l = Ledger::new();
    let src = 递归判(None, 3);
    Session::new(端口(&calls), &calib, &ActionRegistry::new())
        .with_companions(jpp::interp::CompanionMode::Off)
        .with_depth_cap_default(5)
        .run(&程序(&src), &EntryArgs::default(), &mut l)
        .unwrap_or_else(|e| panic!("首跑：{}", e.render()));
    Session::new(端口(&calls), &calib, &ActionRegistry::new())
        .with_companions(jpp::interp::CompanionMode::Off)
        .with_depth_cap_default(3)
        .replay(&程序(&src), &EntryArgs::default(), &mut l)
        .unwrap_or_else(|e| panic!("重放：{}", e.render()));
    assert_eq!(l.header.as_ref().unwrap().depth_cap_default, Some(5));
}

/// E-10：根余额跟随宿主设的引擎默认
#[test]
fn e10_根余额跟随引擎默认() {
    let 未声明 = jpp::Budget {
        calls: 5,
        cost: 0.0,
        depth: None,
        escalate: None,
        unsure: None,
        absent: None,
        latency_p95: None,
    };
    assert_eq!(BudgetCarry::session_with_default(&未声明, 5).depth_cap(), 5);
    assert_eq!(BudgetCarry::session(&未声明).depth_cap(), 256);
    let 声明 = jpp::Budget {
        depth: Some(2),
        ..未声明.clone()
    };
    assert_eq!(BudgetCarry::session_with_default(&声明, 5).depth_cap(), 2);
    let calib = CalibStore::new();
    let calls = Cell::new(0);
    let acts = ActionRegistry::new();
    let s = Session::new(端口(&calls), &calib, &acts).with_depth_cap_default(5);
    assert_eq!(s.root_carry(&未声明).depth_cap(), 5);
}
