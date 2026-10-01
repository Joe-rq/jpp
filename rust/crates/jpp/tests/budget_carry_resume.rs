//! C-3 第三轮（主控复核后答复）：续跑以交回余额为准（G3），计划期拒绝的修法指向上游（G2），两条测试漏洞。
//!
//! 主控读法：交回余额是权威余额。续跑时宿主传上一趟交回的余额；运行时认出「续跑同一轮」（账本头已有本段的余额）时，
//! 深度不再 +1，问人次数不重复计，花费只扣本趟新增；宿主拿更早的余额来续跑时告警并按账本收紧；宿主显式给一份
//! 不同的余额是重新授权，从这一趟起开新的一段。
//!
//! 依据：`地基/规划/主控核查/复核-Z0171-预算传递.md`；预注册附注二 `地基/过程记录/工程-C3-预算传递.md` §十
//! （编号 P20–P25 与附注的预测表一一对应）。

use std::cell::Cell;

use jpp::effects::{CalibStore, FnPort, GenResult, JudgeResult, Ports};
use jpp::ledger::{CarryRecord, Entry, Ledger};
use jpp::value::Answer;
use jpp::{ActionRegistry, Budget, BudgetCarry, EntryArgs, Outcome, Session};
use jpp::{lower, syntax::parse};

/// 判断恒 0.9；问人：`答` 为假时没人答（挂起），为真时答 0.9；生成每次费用 0.25。各自计数
fn 端口<'a>(判: &'a Cell<u64>, 问: &'a Cell<u64>, 答: bool) -> Ports<'a> {
    Ports::new()
        .with(FnPort::judge("fixed-0", move |_s, qs| {
            判.set(判.get() + 1);
            Ok(JudgeResult {
                answers: qs.iter().map(|_| Answer::Noul(0.9)).collect(),
                tokens: 0,
                cost: 0.0,
                mode_share: qs.iter().map(|_| None).collect(),
                perms: qs.iter().map(|_| 0).collect(),
                confidence: vec![],
            })
        }))
        .with(FnPort::ask("fixed-0", move |_s, _q| {
            问.set(问.get() + 1);
            Ok(答.then_some(Answer::Noul(0.9)))
        }))
        .with(FnPort::generate("fixed-0", move |_p, _c, n, _r| {
            Ok(GenResult {
                outputs: (0..n)
                    .map(|i| serde_json::json!(format!("生成{i}")))
                    .collect(),
                tokens: 0,
                cost: 0.25,
                ..Default::default()
            })
        }))
}

fn 程序(src: &str) -> jpp::Program {
    lower(&parse(src).expect("解析")).expect("lower")
}

/// 判断甲 → 问人 → 判断乙
const 问一句: &str = r#"budget {calls: 10, cost: 0, depth: 16, escalate: 2};
let a = cut(judge(state(mat("甲")), test("行吗", "k")));
let h = handle(ask(state(mat("问")), test("行吗", "k")), {act: fn() { 1 }, ignore: fn() { 0 }, unsure: fn(u) { consume(u, "drop"); -1 }});
let b = cut(judge(state(mat("乙")), test("行吗", "k")));
{a: exit_kind(a), h: h, b: exit_kind(b), e: [a, b]}
"#;

/// 一趟：返回（结果、判断次数、问人次数）；账本由调用者持有（续跑在同一本上）
fn 跑一趟(
    src: &str,
    carry: Option<BudgetCarry>,
    ledger: &mut Ledger,
    答: bool,
) -> (Result<Outcome, String>, u64, u64) {
    跑一趟_授权(src, carry, ledger, 答, false)
}

/// 同 `跑一趟`，`重授` 为真时显式重新授权（C-3 R1）
fn 跑一趟_授权(
    src: &str,
    carry: Option<BudgetCarry>,
    ledger: &mut Ledger,
    答: bool,
    重授: bool,
) -> (Result<Outcome, String>, u64, u64) {
    let (判, 问) = (Cell::new(0), Cell::new(0));
    let calib = CalibStore::new();
    // 段起点按账本条目下标钉（P39），伴随题会多出条目：整文件固定关（B0492 S5，主控 2026-09-30：第二类）
    let r = Session::new(端口(&判, &问, 答), &calib, &ActionRegistry::new())
        .with_companions(jpp::interp::CompanionMode::Off)
        .with_carry(carry)
        .reauthorize_carry(重授)
        .resume(&程序(src), &EntryArgs::default(), ledger)
        .map_err(|e| e.render());
    (r, 判.get(), 问.get())
}

fn 告警<'a>(o: &'a Outcome, 前缀: &str) -> Vec<&'a String> {
    o.trace
        .warnings
        .iter()
        .filter(|w| w.starts_with(前缀))
        .collect()
}

fn 余额(calls: u64, escalate: u64) -> BudgetCarry {
    BudgetCarry::session(&Budget {
        calls,
        cost: 0.0,
        depth: None,
        escalate: Some(escalate),
        unsure: None,
        absent: None,
        latency_p95: None,
    })
}

fn 三项(c: &BudgetCarry) -> (u64, u64, u32) {
    (c.calls(), c.escalate(), c.depth_at())
}

fn 花费条(l: &Ledger) -> Vec<(u64, u64, bool)> {
    l.entries
        .iter()
        .filter_map(|e| match e {
            Entry::Spent {
                calls,
                asks,
                started,
                ..
            } => Some((*calls, *asks, *started)),
            _ => None,
        })
        .collect()
}

fn w_carry(o: &Outcome) -> Vec<&String> {
    o.trace
        .warnings
        .iter()
        .filter(|w| w.starts_with("W-carry"))
        .collect()
}

/// 首趟：问人没人答，挂起；交回的余额
fn 首趟(e: &BudgetCarry, l: &mut Ledger) -> BudgetCarry {
    let (r, n判, n问) = 跑一趟(问一句, Some(e.clone()), l, false);
    let o = r.expect("首趟挂起返回");
    assert_eq!(
        o.pending.first().map(|p| p.cause.as_str()),
        Some("ask"),
        "{:?}",
        o.pending
    );
    assert_eq!((n判, n问), (1, 1));
    assert_eq!(
        花费条(l),
        [(2, 0, true)],
        "甲 + 问人各一次调用；没人答不算问人次数"
    );
    o.carry.expect("交回余额")
}

/// P21：用交回文件续跑——深度不再 +1、问人不重复计、花费只扣本趟新增
#[test]
fn p21_用交回余额续跑不偏紧() {
    let e = 余额(10, 2);
    let mut l = Ledger::new();
    let h1 = 首趟(&e, &mut l);
    assert_eq!(三项(&h1), (8, 2, 1));
    let (r, n判, n问) = 跑一趟(问一句, Some(h1), &mut l, true);
    let o = r.expect("续跑跑完");
    assert!(o.pending.is_empty(), "{:?}", o.pending);
    assert!(
        w_carry(&o).is_empty(),
        "传的是交回余额，不告警：{:?}",
        w_carry(&o)
    );
    assert_eq!((n判, n问), (1, 1), "甲命中不花钱，问人答一次、乙一次");
    assert_eq!(花费条(&l), [(2, 0, true), (2, 1, true)]);
    assert_eq!(三项(&o.carry.clone().unwrap()), (6, 1, 1));
    let h = l.header.as_ref().unwrap();
    assert_eq!(h.carry.as_ref(), Some(e.record()), "本段进门余额不改写");
    assert_eq!(h.carry_from, Some(0));
}

/// P22：用原文件续跑会漏出整场——告警、按账本收紧；结果与传交回余额逐字节相同
#[test]
fn p22_用原余额续跑告警并按账本收紧() {
    let 跑两趟 = |第二趟给原的: bool| {
        let e = 余额(3, 2);
        let mut l = Ledger::new();
        let h1 = 首趟(&e, &mut l);
        assert_eq!(三项(&h1), (1, 2, 1));
        let 给 = if 第二趟给原的 { e.clone() } else { h1 };
        let (r, n判, n问) = 跑一趟(问一句, Some(给), &mut l, true);
        (r.expect("续跑跑完"), n判, n问)
    };
    let (o, n判, n问) = 跑两趟(true);
    let w = w_carry(&o);
    assert_eq!(w.len(), 1, "{w:?}");
    assert!(w[0].contains("以账本的交回余额为准"), "{}", w[0]);
    assert_eq!((n判, n问), (0, 1), "账本剩 1：问人用掉，乙没得问");
    assert_eq!(o.value_json()["b"], "unsure(budget)");
    let 原 = serde_json::to_string(o.carry.as_ref().unwrap().record()).unwrap();
    assert_eq!(三项(o.carry.as_ref().unwrap()), (0, 1, 1));
    let (o2, _, _) = 跑两趟(false);
    assert!(w_carry(&o2).is_empty());
    assert_eq!(
        serde_json::to_string(o2.carry.as_ref().unwrap().record()).unwrap(),
        原,
        "给原文件与给交回文件，交回余额逐字节相同"
    );
}

fn 大额() -> BudgetCarry {
    BudgetCarry::from_record(CarryRecord {
        calls: 20,
        cost: 0.0,
        latency_p95: None,
        escalate: 2,
        hop: 0,
        round: 0,
        depth_cap: 256,
    })
}

/// P23′（附注三，原 P23 的预测改变）：**显式**重新授权开新段——出 `W-carry-reauth`（写明上一段花了多少、新段从哪起），
/// 不出 `W-carry`（测试漏洞 (a)：续跑账本上 `skip(carry_from)` 改坏能被抓）
#[test]
fn p23_显式重新授权开新段() {
    let e = 余额(10, 2);
    let mut l = Ledger::new();
    let _ = 首趟(&e, &mut l);
    let 起点 = l.entries.len() as u64;
    let x = 大额();
    let (r, _, _) = 跑一趟_授权(问一句, Some(x.clone()), &mut l, true, true);
    let o = r.expect("续跑跑完");
    let 授 = 告警(&o, "W-carry-reauth");
    assert_eq!(授.len(), 1, "{:?}", o.trace.warnings);
    assert!(
        授[0].contains("上一段")
            && 授[0].contains("已记花费 calls 2")
            && 授[0].contains("新段从账本第"),
        "{}",
        授[0]
    );
    assert!(
        告警(&o, "W-carry:").is_empty(),
        "显式重新授权不按旧文件处理"
    );
    let h = l.header.as_ref().unwrap();
    assert_eq!(h.carry.as_ref(), Some(x.record()));
    assert_eq!(h.carry_from, Some(起点));
    assert_eq!(
        三项(o.carry.as_ref().unwrap()),
        (18, 1, 1),
        "只扣新段的 Spent（改成 skip(0) 会是 16）"
    );
}

/// P23b（附注三）：同一份更大的余额、**没开**重新授权——按旧文件处理，以账本为准
#[test]
fn p23b_不开重新授权按旧文件处理() {
    let e = 余额(10, 2);
    let mut l = Ledger::new();
    let h1 = 首趟(&e, &mut l);
    let (r, _, _) = 跑一趟(问一句, Some(大额()), &mut l, true);
    let o = r.expect("续跑跑完");
    let w = 告警(&o, "W-carry:");
    assert_eq!(w.len(), 1, "{:?}", o.trace.warnings);
    assert!(w[0].contains("以账本的交回余额为准"), "{}", w[0]);
    assert_eq!(
        l.header.as_ref().unwrap().carry.as_ref(),
        Some(e.record()),
        "没开新段"
    );
    let 旧 = serde_json::to_string(o.carry.as_ref().unwrap().record()).unwrap();
    assert_eq!(三项(o.carry.as_ref().unwrap()), (6, 1, 1));
    // 与传交回文件逐字节相同
    let mut l2 = Ledger::new();
    let _ = 首趟(&e, &mut l2);
    let (r2, _, _) = 跑一趟(问一句, Some(h1), &mut l2, true);
    assert_eq!(
        serde_json::to_string(r2.unwrap().carry.as_ref().unwrap().record()).unwrap(),
        旧
    );
}

/// P28（R4）：续跑时账本记着余额、宿主没给——出 `W-carry-missing`，照常跑完
#[test]
fn p28_续跑不带余额告警() {
    let e = 余额(10, 2);
    let mut l = Ledger::new();
    let _ = 首趟(&e, &mut l);
    let (r, _, _) = 跑一趟(问一句, None, &mut l, true);
    let o = r.expect("续跑跑完");
    assert!(o.pending.is_empty());
    let w = 告警(&o, "W-carry-missing");
    assert_eq!(w.len(), 1, "{:?}", o.trace.warnings);
    assert!(w[0].contains("calls 8"), "{}", w[0]);
    assert!(o.carry.is_none());
}

/// P29（R6）：开新段之后再续跑一趟，`for_trip` 只数新段的 `Spent`
#[test]
fn p29_新段之后续跑只数新段() {
    let e = 余额(10, 2);
    let mut l = Ledger::new();
    let _ = 首趟(&e, &mut l);
    let (r, _, _) = 跑一趟_授权(问一句, Some(大额()), &mut l, true, true);
    let h2 = r.unwrap().carry.unwrap();
    assert_eq!(三项(&h2), (18, 1, 1));
    let t = jpp::TripCarry::for_trip(Some(&h2), &l, false, false).expect("带余额");
    assert!(t.warning.is_none(), "{:?}", t.warning);
    assert_eq!(
        (t.effective.calls(), t.effective.depth_at()),
        (18, 0),
        "段起点若误用 0 会是 16"
    );
    let (r, n判, n问) = 跑一趟(问一句, Some(h2), &mut l, true);
    let o = r.expect("再续跑");
    assert_eq!((n判, n问), (0, 0), "全部命中");
    assert!(告警(&o, "W-carry").is_empty(), "{:?}", o.trace.warnings);
    assert_eq!(三项(o.carry.as_ref().unwrap()), (18, 1, 1));
}

/// 先生成一句（不读），判断一次，再递归撞 J-06
const 生成后失败: &str = r#"budget {calls: 100, cost: 1, depth: 4};
let g = gen("写一句", [], 1, 0);
let a = cut(judge(state(mat("甲")), test("行吗", "k")));
let k = exit_kind(a);
fn f(n) { if n > 0 { f(n - 1) } else { 0 } }
let z = f(10);
{k: k, z: z, e: a}
"#;

/// P24：失败的一轮里 `gen` 在飞（测试漏洞 (b)）：`Spent` 在收齐生成之后写，含生成的费用
#[test]
fn p24_失败轮收齐生成再记花费() {
    let e = BudgetCarry::session(&Budget {
        calls: 10,
        cost: 1.0,
        depth: None,
        escalate: None,
        unsure: None,
        absent: None,
        latency_p95: None,
    });
    let mut l = Ledger::new();
    let (r, n判, _) = 跑一趟(生成后失败, Some(e.clone()), &mut l, false);
    let err = r.expect_err("递归撞 J-06");
    assert!(err.contains("J-06"), "{err}");
    assert_eq!(n判, 1);
    let 花: Vec<(u64, f64)> = l
        .entries
        .iter()
        .filter_map(|x| match x {
            Entry::Spent { calls, usd, .. } => Some((*calls, *usd)),
            _ => None,
        })
        .collect();
    assert_eq!(花, [(2, 0.25)], "生成与判断各一次，生成的费用收齐了");
    let c = BudgetCarry::handed_back(Some(&e), &l).unwrap();
    assert_eq!((c.calls(), c.cost()), (8, 0.75));
}

/// P25：前一趟被杀（花费没被记上），续跑开跑前补结清；交回 0，再续跑也不长回来
#[test]
fn p25_被杀后续跑补结清() {
    let e = 余额(10, 2);
    let mut l = Ledger::new();
    let _ = 首趟(&e, &mut l);
    l.entries.retain(|x| !matches!(x, Entry::Spent { .. }));
    l.rebuild_index();
    let h = BudgetCarry::handed_back(Some(&e), &l).unwrap();
    assert_eq!(三项(&h), (0, 0, 1), "被杀：算不出花了多少，不再给");
    let (r, n判, n问) = 跑一趟(问一句, Some(h.clone()), &mut l, true);
    let o = r.expect("续跑返回");
    assert_eq!((n判, n问), (0, 0), "余额 0，一次都不发");
    assert_eq!(
        o.pending.first().map(|p| p.cause.as_str()),
        Some("budget.escalate")
    );
    assert_eq!(
        花费条(&l),
        [(10, 2, true), (0, 0, true)],
        "先补结清，再记本趟"
    );
    assert_eq!(三项(o.carry.as_ref().unwrap()), (0, 0, 1));
    let (r, n判, _) = 跑一趟(问一句, o.carry.clone(), &mut l, true);
    assert_eq!(n判, 0);
    assert_eq!(
        三项(r.unwrap().carry.as_ref().unwrap()),
        (0, 0, 1),
        "不长回来"
    );
}

/// P20：上游收紧时计划期拒绝的修法指向上游余额；程序自己声明太小时照旧
#[test]
fn p20_计划期拒绝的修法分清来源() {
    let src = r#"budget {calls: 5, cost: 0, depth: 16};
let a = cut(judge(state(mat("甲")), test("行吗", "k")));
{k: exit_kind(a), e: a}
"#;
    let (r, n, _) = 跑一趟(src, Some(余额(0, 0)), &mut Ledger::new(), false);
    let e = r.expect_err("余额 0 < 下界 1");
    assert_eq!(n, 0);
    assert!(
        e.contains("E-budget-plan") && e.contains("上游余额") && e.contains("程序声明 calls 5"),
        "{e}"
    );
    assert!(!e.contains("修法：放宽 budget"), "{e}");
    let (r, _, _) = 跑一趟(
        &src.replace("calls: 5", "calls: 0"),
        None,
        &mut Ledger::new(),
        false,
    );
    let e = r.expect_err("程序自己声明 0");
    assert!(
        e.contains("修法：放宽 budget") && !e.contains("上游"),
        "{e}"
    );
}

/// P27（复查 C1）：同一段第二趟被杀（花费没被记上），宿主拿上一趟交回的文件续跑——
/// 应当按旧文件处理：告警、补结清、交回 0；不能被当成重新授权放宽整场
#[test]
fn p27_第二趟被杀后拿上一趟交回文件续跑() {
    let e = 余额(10, 2);
    let mut l = Ledger::new();
    let h1 = 首趟(&e, &mut l);
    assert_eq!(三项(&h1), (8, 2, 1));
    let (r, n判, n问) = 跑一趟(问一句, Some(h1.clone()), &mut l, true);
    assert!(r.expect("第二趟跑完").pending.is_empty());
    assert_eq!((n判, n问), (1, 1));
    // 模拟第二趟在写 Spent 之前被杀：删掉它的 Spent（第一条保留）
    let 第二条 = l
        .entries
        .iter()
        .enumerate()
        .filter(|(_, x)| matches!(x, Entry::Spent { .. }))
        .map(|(i, _)| i)
        .nth(1)
        .expect("第二趟的 Spent");
    l.entries.remove(第二条);
    l.rebuild_index();
    let (r, n判, n问) = 跑一趟(问一句, Some(h1), &mut l, true);
    let o = r.expect("第三趟返回");
    let w = w_carry(&o);
    assert_eq!(w.len(), 1, "{w:?}");
    assert!(w[0].contains("以账本的交回余额为准"), "{}", w[0]);
    assert_eq!(
        (n判, n问),
        (0, 0),
        "账本记着花费没被记上，剩余全部结清，一次都不发"
    );
    assert_eq!(
        花费条(&l),
        [(2, 0, true), (8, 2, true), (0, 0, true)],
        "首趟、补结清、本趟"
    );
    assert_eq!(三项(o.carry.as_ref().unwrap()), (0, 0, 1));
}

/// P32（复查二 D1，按探针 Q1 原序）：某一趟不带余额续跑，账本头的余额不能被清掉；下一趟传更大的余额、不开开关，
/// 按旧文件处理（告警、补结清、交回 0），不能当成新链的起点
#[test]
fn p32_不带余额的续跑不清账本余额() {
    let e = 余额(10, 2);
    let mut l = Ledger::new();
    let _ = 首趟(&e, &mut l);
    let (r, n判, n问) = 跑一趟(问一句, None, &mut l, true);
    let o = r.expect("第二趟跑完");
    assert_eq!((n判, n问), (1, 1));
    let w = 告警(&o, "W-carry-missing");
    assert_eq!(w.len(), 1, "{:?}", o.trace.warnings);
    let 缺 = w[0].clone();
    let h = l.header.as_ref().unwrap();
    assert_eq!(h.carry.as_ref(), Some(e.record()), "头里的余额不清");
    assert_eq!(h.carry_from, Some(0));
    // Z0384 R11（附注五，P32′ 预测改变）：不带余额的一趟照记花费
    assert_eq!(
        花费条(&l),
        [(2, 0, true), (2, 1, true)],
        "不带余额的一趟照记 Spent"
    );
    let x = BudgetCarry::from_record(CarryRecord {
        calls: 50,
        cost: 0.0,
        latency_p95: None,
        escalate: 5,
        hop: 1,
        round: 0,
        depth_cap: 256,
    });
    let (r, n判, n问) = 跑一趟(问一句, Some(x), &mut l, true);
    let o = r.expect("第三趟返回");
    let w = 告警(&o, "W-carry:");
    assert_eq!(w.len(), 1, "{:?}", o.trace.warnings);
    assert!(告警(&o, "W-carry-reauth").is_empty());
    assert_eq!((n判, n问), (0, 0));
    assert_eq!(
        花费条(&l),
        [(2, 0, true), (2, 1, true), (0, 0, true)],
        "首趟、不带余额的一趟、本趟；不再补结清"
    );
    assert_eq!(三项(o.carry.as_ref().unwrap()), (6, 1, 1));
    assert_eq!(l.header.as_ref().unwrap().carry.as_ref(), Some(e.record()));
    assert!(缺.contains("照记进账本"), "文案说清后果：{缺}");
}

/// P33′（R8；Z0384 附注五预测改变）：不开开关、宿主给的比账本剩余更小时，这一趟取小，账本记一条只收紧的
/// `CarryCap`，交回余额不超过给的
#[test]
fn p33_给得更小时取小且交回不超过给的() {
    let e = 余额(10, 2);
    let mut l = Ledger::new();
    let _ = 首趟(&e, &mut l);
    let x = BudgetCarry::from_record(CarryRecord {
        calls: 1,
        cost: 0.0,
        latency_p95: None,
        escalate: 2,
        hop: 1,
        round: 0,
        depth_cap: 256,
    });
    let (r, n判, n问) = 跑一趟(问一句, Some(x), &mut l, true);
    let o = r.expect("续跑返回");
    let w = 告警(&o, "W-carry:");
    assert_eq!(w.len(), 1, "{:?}", o.trace.warnings);
    assert!(
        w[0].contains("逐项取小") && w[0].contains("calls 1"),
        "文案写实际生效的余额：{}",
        w[0]
    );
    assert_eq!((n判, n问), (0, 1), "生效 calls 1：问人用掉，乙没得发");
    assert_eq!(o.value_json()["b"], "unsure(budget)");
    // 只收紧的记录：按账本编码数（写成文本，改前的代码没有这种条目也能编译）
    let 编码 = l.encode();
    // G4b（步 37b）：首趟程序声明 depth 16 小于余额上限 256，已记一条只收紧深度的 CarryCap；本趟给得更小再记一条
    assert_eq!(
        编码.matches("\"CarryCap\"").count(),
        2,
        "只收紧的记录：首趟深度一条、本趟一条"
    );
    assert!(编码.contains("\"CarryCap\":{\"calls\":1,"), "封顶 calls 1");
    assert_eq!(
        三项(o.carry.as_ref().unwrap()),
        (0, 1, 1),
        "交回不超过给的 1"
    );
}

/// P37（Z0384 R11，复查三 QJ 原序）：不带余额的一趟只重问没人答的问人，花费照记进账本，下一趟从余额里扣
#[test]
fn p37_不带余额只重问也记花费() {
    let e = 余额(10, 2);
    let mut l = Ledger::new();
    let h1 = 首趟(&e, &mut l);
    let (r, n判, n问) = 跑一趟(问一句, None, &mut l, false);
    assert!(r.is_ok());
    assert_eq!((n判, n问), (0, 1), "甲命中，只重问一次");
    let (r, _, n问) = 跑一趟(问一句, Some(h1), &mut l, false);
    let o = r.expect("第三趟挂起返回");
    assert_eq!(n问, 1);
    assert_eq!(
        告警(&o, "W-carry:").len(),
        1,
        "账本交回 (7,2,1) 与给的 H1 不等：{:?}",
        o.trace.warnings
    );
    assert_eq!(花费条(&l), [(2, 0, true), (1, 0, true), (1, 0, true)]);
    assert_eq!(三项(o.carry.as_ref().unwrap()), (6, 2, 1));
}

/// P39（Z0384，复查三 QI）：不带余额的一趟照抄的段起点不能被写死为 0
#[test]
fn p39_照抄的段起点不是写死的0() {
    let e = 余额(10, 2);
    let mut l = Ledger::new();
    let _ = 首趟(&e, &mut l);
    let (r, _, _) = 跑一趟_授权(问一句, Some(余额(30, 3)), &mut l, false, true);
    let h2 = r.unwrap().carry.unwrap();
    assert_eq!(三项(&h2), (29, 3, 1));
    let 起点 = l.header.as_ref().unwrap().carry_from;
    // G4b（步 37b）：首趟多一条只收紧深度的 CarryCap（声明 16 < 256），段起点随之后移一条
    assert_eq!(起点, Some(4));
    let (r, _, _) = 跑一趟(问一句, None, &mut l, true);
    assert!(r.is_ok());
    assert_eq!(
        l.header.as_ref().unwrap().carry_from,
        起点,
        "不带余额的一趟照抄段起点"
    );
    let (r, _, _) = 跑一趟(问一句, Some(h2), &mut l, true);
    let o = r.expect("最后一趟");
    assert_eq!(告警(&o, "W-carry:").len(), 1, "{:?}", o.trace.warnings);
    assert_eq!(
        三项(o.carry.as_ref().unwrap()),
        (27, 2, 1),
        "段起点若写死 0 会是 (25,2,1)"
    );
}

/// P38（Z0384）：对不上账本时逐项取小——调用、花费、时延、问人、深度上限都取小；给的在某一项上更小时记一条只收紧的
/// `CarryCap`，逐项都更大时不记
#[test]
fn p38_逐项取小多字段() {
    let e = BudgetCarry::session(&Budget {
        calls: 10,
        cost: 1.0,
        depth: None,
        escalate: Some(2),
        unsure: None,
        absent: None,
        latency_p95: Some(5.0),
    });
    let mut l = Ledger::new();
    let _ = 首趟(&e, &mut l);
    let 给 = |calls, cost, lat, esc, cap| {
        BudgetCarry::from_record(CarryRecord {
            calls,
            cost,
            latency_p95: Some(lat),
            escalate: esc,
            hop: 1,
            round: 0,
            depth_cap: cap,
        })
    };
    let t = jpp::TripCarry::for_trip(Some(&给(9, 0.5, 2.0, 1, 100)), &l, false, false).unwrap();
    let f = &t.effective;
    assert_eq!(
        (
            f.calls(),
            f.cost(),
            f.latency_p95(),
            f.escalate(),
            f.depth_cap()
        ),
        // G4b（步 37b）：首趟程序声明 depth 16，本段深度上限已收紧到 16，给的 100 不再更小
        (8, 0.5, Some(2.0), 1, 16)
    );
    assert_eq!(f.depth_at(), 0, "起算深度仍取这一轮的");
    match t.cap {
        Some(Entry::CarryCap {
            calls,
            usd,
            secs,
            asks,
            depth_cap,
        }) => assert_eq!(
            (calls, usd, secs, asks, depth_cap),
            (8, 0.5, Some(2.0), 1, 16)
        ),
        other => panic!("应有只收紧记录：{other:?}"),
    }
    let t = jpp::TripCarry::for_trip(Some(&给(50, 9.0, 99.0, 5, 256)), &l, false, false).unwrap();
    assert!(t.warning.is_some(), "对不上照样告警");
    assert!(t.cap.is_none(), "逐项都更大时不记");
    assert_eq!(t.effective.calls(), 8, "按账本剩余");
}

/// P41（Z0384 复核 B1，K6 原序）：被杀的一趟之后接一趟不带余额的，被杀那趟的花费不能漏记——不带余额的一趟开跑前也补结清
#[test]
fn p41_被杀后接不带余额的一趟仍结清() {
    let e = 余额(10, 2);
    let mut l = Ledger::new();
    let h1 = 首趟(&e, &mut l);
    let (r, _, _) = 跑一趟(问一句, Some(h1.clone()), &mut l, true);
    assert!(r.expect("第二趟跑完").pending.is_empty());
    let 第二条 = l
        .entries
        .iter()
        .enumerate()
        .filter(|(_, x)| matches!(x, Entry::Spent { .. }))
        .map(|(i, _)| i)
        .nth(1)
        .expect("第二趟的 Spent");
    l.entries.remove(第二条);
    l.rebuild_index();
    let (r, n判, n问) = 跑一趟(问一句, None, &mut l, true);
    assert!(r.is_ok());
    assert_eq!((n判, n问), (0, 0), "第三趟全部命中");
    let (r, _, _) = 跑一趟(问一句, Some(h1), &mut l, true);
    let o = r.expect("第四趟返回");
    assert_eq!(告警(&o, "W-carry:").len(), 1, "{:?}", o.trace.warnings);
    assert_eq!(
        三项(o.carry.as_ref().unwrap()),
        (0, 0, 1),
        "被杀那趟的花费没被放过"
    );
    assert_eq!(
        花费条(&l),
        [(2, 0, true), (8, 2, true), (0, 0, true), (0, 0, true)],
        "首趟、第三趟开跑前的结清、第三趟、第四趟"
    );
}

/// P42（Z0384 R2-a）：宿主给的深度上限更小时也记只收紧记录，交回的深度上限取小
#[test]
fn p42_深度上限也封顶() {
    let e = 余额(10, 2);
    let mut l = Ledger::new();
    let _ = 首趟(&e, &mut l);
    let x = BudgetCarry::from_record(CarryRecord {
        calls: 8,
        cost: 0.0,
        latency_p95: None,
        escalate: 2,
        hop: 1,
        round: 0,
        depth_cap: 3,
    });
    let (r, _, _) = 跑一趟(问一句, Some(x), &mut l, true);
    let o = r.expect("续跑跑完");
    assert_eq!(告警(&o, "W-carry:").len(), 1, "{:?}", o.trace.warnings);
    // G4b（步 37b）：首趟程序声明 depth 16 已记一条（16 < 256）；本趟给的深度上限 3 更小再记一条
    assert_eq!(
        l.encode().matches("\"CarryCap\"").count(),
        2,
        "深度上限更小也记"
    );
    assert_eq!(
        o.carry.as_ref().unwrap().depth_cap(),
        3,
        "交回的深度上限不超过给的"
    );
}

/// P43（Z0384 小项）：只有 cost 一项更小时，这一趟与交回都取小
#[test]
fn p43_只有花费更小时取小() {
    let e = BudgetCarry::session(&Budget {
        calls: 10,
        cost: 1.0,
        depth: None,
        escalate: Some(2),
        unsure: None,
        absent: None,
        latency_p95: None,
    });
    let mut l = Ledger::new();
    let _ = 首趟(&e, &mut l);
    let x = BudgetCarry::from_record(CarryRecord {
        calls: 8,
        cost: 0.3,
        latency_p95: None,
        escalate: 2,
        hop: 1,
        round: 0,
        depth_cap: 256,
    });
    let t = jpp::TripCarry::for_trip(Some(&x), &l, false, false).unwrap();
    assert_eq!((t.effective.calls(), t.effective.cost()), (8, 0.3));
    let (r, _, _) = 跑一趟(问一句, Some(x), &mut l, true);
    let o = r.expect("续跑跑完");
    assert_eq!(告警(&o, "W-carry:").len(), 1, "{:?}", o.trace.warnings);
    assert!(
        l.encode().contains("\"usd\":0.3"),
        "只收紧记录记下 cost 0.3"
    );
    let c = o.carry.unwrap();
    assert_eq!((c.calls(), c.cost()), (6, 0.3));
}

/// P45（Z0384 R2-b）：读到不认识的条目种类时报错，指出行号与种类名，不跳过
/// （`CarryCap` 对 Z0384 之前的二进制就是这条路径）
#[test]
fn p45_不认识的条目种类报错不跳过() {
    let e = 余额(10, 2);
    let mut l = Ledger::new();
    let _ = 首趟(&e, &mut l);
    let 原 = l.encode();
    assert!(原.ends_with('\n'));
    assert_eq!(原.matches("{\"Spent\":").count(), 1);
    let 改 = 原.replace("{\"Spent\":", "{\"FutureKind\":");
    let err = Ledger::decode(&改)
        .map(|_| ())
        .expect_err("不认识的种类不能被跳过");
    assert!(
        err.contains("E-ledger-corrupt")
            && err.contains("unknown variant")
            && err.contains("FutureKind"),
        "{err}"
    );
    assert!(
        err.contains("第 ") && err.contains(" 行"),
        "指出行号：{err}"
    );
}
