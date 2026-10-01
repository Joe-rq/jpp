//! C-1（主控板 Z0169）：未决责任的每种去向在账本里是一种单独类型的事件（账本 v4）。
//!
//! 每种有运行时产生点的去向各一条，断言账本里出现的变体与字段：丢弃 `Drop`、细化 `Refine`（branch 与
//! literalize）、升级 `Escalate`、转交 `Handoff`、改选 `Reselect`（C-4 `feasible`，含没有可行候选）。另测：同一判断的责任被消费两次只记一条（不变式）；
//! 同一份账本续跑不重复写；开不开 `--guard` 事件相同；六种变体的编解码（缺省字段宽容、未知字段拒绝）与
//! 账本版本（v3 照读、v4 照读、别的版本拒读）。`Enrich` 的运行时产生点在 S2 默认链，这里只测它的编解码
//! （主控 2026-09-29 同意，运行时断言在 S2）。
//!
//! 依据：主控板 Z0169、Z0207；裁定纸面阶段第五节「共 2」；过程记录 `工程-未决去向.md` 5.1。

mod common;
use common::run;
use std::cell::RefCell;

use jpp::effects::{CalibStore, FnPort, JudgeResult, Ports};
use jpp::ledger::{Entry, Ledger};
use jpp::value::Answer;
use jpp::{ActionRegistry, Outcome};
use jpp::{lower, syntax::parse};

/// 题面含「字面」的读 0.99（已决），其余读 0.5（落在线带 [0.25, 0.75] 内 → `Unsure(band)`）。
/// 问人一律答 0.9。
fn 端口<'a>(calls: &'a RefCell<u64>) -> Ports<'a> {
    Ports::new()
        .with(common::伴随中性judge("fixed-0", move |_s, qs| {
            *calls.borrow_mut() += 1;
            Ok(JudgeResult {
                answers: qs
                    .iter()
                    .map(|q| Answer::Noul(if q.text.contains("字面") { 0.99 } else { 0.5 }))
                    .collect(),
                tokens: 0,
                cost: 0.0,
                mode_share: vec![],
                perms: vec![],
                confidence: vec![],
            })
        }))
        .with(FnPort::ask("fixed-0", |_s, _q| Ok(Some(Answer::Noul(0.9)))))
}

fn 跑在(src: &str, guard: bool, l: &mut Ledger) -> Outcome {
    let mut program = lower(&parse(src).expect("解析")).expect("lower");
    program.entry.guard = guard;
    let mut calib = CalibStore::new();
    calib.put("k", 0.75, 0.25, 50, "上岗", Some(0.05)).unwrap();
    let calls = RefCell::new(0);
    run(&program, 端口(&calls), &calib, &ActionRegistry::new(), l)
        .unwrap_or_else(|e| panic!("运行失败：{}", e.render()))
}

fn 跑(src: &str) -> Ledger {
    let mut l = Ledger::new();
    跑在(src, false, &mut l);
    l
}

fn 事件(l: &Ledger) -> Vec<&Entry> {
    l.entries.iter().filter(|e| e.is_duty_event()).collect()
}

fn 判断键(l: &Ledger) -> Vec<String> {
    l.entries
        .iter()
        .filter_map(|e| match e {
            Entry::Judge { key, .. } => Some(key.clone()),
            _ => None,
        })
        .collect()
}

#[test]
fn drop_记_drop事件() {
    let l = 跑(r#"budget {calls: 4, cost: 0, depth: 8};
let e = cut(judge(state(mat("甲")), test("模糊题", "k")));
handle(e, {act: fn() { 1 }, ignore: fn() { 0 }, unsure: fn(u) { consume(u, "drop"); 0 }})
"#);
    let 键 = 判断键(&l);
    let ev = 事件(&l);
    assert_eq!(ev.len(), 1, "{ev:?}");
    let Entry::Drop { of, cause, site } = ev[0] else {
        panic!("应是 Drop：{ev:?}")
    };
    assert_eq!(of, &键);
    assert_eq!(cause, "band");
    assert!(*site > 0);
}

#[test]
fn branch_记_refine事件() {
    let l = 跑(r#"budget {calls: 4, cost: 0, depth: 8};
let e = cut(judge(state(mat("甲")), test("模糊题", "k")));
handle(e, {act: fn() { 1 }, ignore: fn() { 0 }, unsure: fn(u) { consume(u, "branch"); 0 }})
"#);
    let ev = 事件(&l);
    assert!(
        matches!(ev.as_slice(), [Entry::Refine { how, to: None, cause, .. }] if how == "branch" && cause == "band"),
        "{ev:?}"
    );
}

#[test]
fn literalize_记_refine事件并指向重问的判断() {
    let l = 跑(r#"budget {calls: 4, cost: 0, depth: 8};
let s = state(mat("甲"));
let e = cut(judge(s, test("模糊题", "k")));
handle(e, {act: fn() { 1 }, ignore: fn() { 0 },
           unsure: fn(u) { handle(literalize(u, s, test("字面题", "k")), {act: fn() { 1 }, ignore: fn() { 0 }, unsure: fn(v) { {exit: v} }}) }})
"#);
    let 键 = 判断键(&l);
    assert_eq!(键.len(), 2, "原题与重问各一条判断");
    let ev = 事件(&l);
    assert_eq!(ev.len(), 1, "重问已决，只有一条细化：{ev:?}");
    let Entry::Refine { of, how, to, .. } = ev[0] else {
        panic!("应是 Refine：{ev:?}")
    };
    assert_eq!(how, "literalize");
    assert_eq!(of, &vec![键[0].clone()]);
    assert_eq!(
        to.as_deref(),
        Some(键[1].as_str()),
        "to 是重问那道题的账本键"
    );
}

#[test]
fn escalate_记_escalate事件并指向问人条目() {
    let l = 跑(r#"budget {calls: 4, cost: 0, depth: 8, escalate: 1};
let s = state(mat("甲"));
let e = cut(judge(s, test("模糊题", "k")));
handle(e, {act: fn() { 1 }, ignore: fn() { 0 },
           unsure: fn(u) { handle(escalate(u, s, test("人来判", "k")), {act: fn() { 1 }, ignore: fn() { 0 }, unsure: fn(v) { {exit: v} }}) }})
"#);
    let 键 = 判断键(&l);
    let ev = 事件(&l);
    assert_eq!(ev.len(), 1, "{ev:?}");
    let Entry::Escalate { of, ask, cause, .. } = ev[0] else {
        panic!("应是 Escalate：{ev:?}")
    };
    assert_eq!(of, &键);
    assert_eq!(cause, "band");
    let 问人 = l
        .entries
        .iter()
        .position(|e| matches!(e, Entry::Ask { key, .. } if Some(key) == ask.as_ref()))
        .expect("ask 字段指向账本里问人那一条");
    let 升级 = l.entries.iter().position(|e| e.is_duty_event()).unwrap();
    assert!(升级 < 问人, "Escalate 写在 Ask 之前");
}

#[test]
fn 转交到程序结果记_handoff事件() {
    let l = 跑(r#"budget {calls: 4, cost: 0, depth: 8};
let e = cut(judge(state(mat("甲")), test("模糊题", "k")));
handle(e, {act: fn() { {v: 1} }, ignore: fn() { {v: 0} }, unsure: fn(u) { {v: unit, exit: u} }})
"#);
    let 键 = 判断键(&l);
    let ev = 事件(&l);
    assert!(
        matches!(ev.as_slice(), [Entry::Handoff { of, to, cause, .. }] if of == &键 && to == "program" && cause == "band"),
        "{ev:?}"
    );
}

#[test]
fn 同一判断的责任消费两次只记一条() {
    let l = 跑(r#"budget {calls: 4, cost: 0, depth: 8};
let r = judge(state(mat("甲")), test("模糊题", "k"));
let a = cut(r);
let b = cut(r);
consume(a, "drop");
consume(b, "drop");
0
"#);
    let ev = 事件(&l);
    assert_eq!(ev.len(), 1, "第二次消费报 W-duty-twice、不再记账：{ev:?}");
    assert!(matches!(ev[0], Entry::Drop { .. }));
}

const 两种去向: &str = r#"budget {calls: 4, cost: 0, depth: 8};
let a = cut(judge(state(mat("甲")), test("模糊题", "k")));
let b = cut(judge(state(mat("乙")), test("模糊题", "k")));
consume(a, "drop");
{exit: b}
"#;

#[test]
fn 同一份账本续跑不重复写() {
    let mut l = Ledger::new();
    跑在(两种去向, false, &mut l);
    let 首跑: Vec<Entry> = 事件(&l).into_iter().cloned().collect();
    assert_eq!(首跑.len(), 2, "{首跑:?}");
    let n = l.entries.len();
    跑在(两种去向, false, &mut l);
    assert_eq!(l.entries.len(), n, "续跑没有新条目");
    let 续跑: Vec<Entry> = 事件(&l).into_iter().cloned().collect();
    assert_eq!(续跑, 首跑);
}

#[test]
fn 开不开_guard_事件相同() {
    let mut 关 = Ledger::new();
    跑在(两种去向, false, &mut 关);
    let mut 开 = Ledger::new();
    跑在(两种去向, true, &mut 开);
    let a: Vec<&Entry> = 事件(&关);
    let b: Vec<&Entry> = 事件(&开);
    assert_eq!(a, b);
    assert!(
        matches!(a.as_slice(), [Entry::Drop { .. }, Entry::Handoff { .. }]),
        "{a:?}"
    );
}

fn 一行(e: &Entry) -> String {
    let mut l = Ledger::new();
    l.put(e.clone());
    l.encode()
}

#[test]
fn 六种变体编解码往返() {
    use jpp::ledger::Skipped;
    let 全 = vec![
        Entry::Drop {
            of: vec!["k1".into()],
            cause: "band".into(),
            site: 3,
        },
        Entry::Refine {
            of: vec!["k1".into()],
            cause: "tie".into(),
            site: 3,
            how: "literalize".into(),
            to: Some("k2".into()),
        },
        Entry::Enrich {
            of: vec!["k1".into()],
            cause: "band".into(),
            site: 3,
            need: "证据".into(),
            round: 1,
            got: true,
            asked_by: Some("k3".into()),
        },
        Entry::Handoff {
            of: vec![],
            cause: "budget".into(),
            site: 9,
            to: "program".into(),
        },
        Entry::Escalate {
            of: vec!["k1".into()],
            cause: "band".into(),
            site: 3,
            ask: Some("a1".into()),
        },
        Entry::Reselect {
            of: vec!["k4".into()],
            site: 5,
            from: 0,
            chosen: None,
            skipped: vec![Skipped { k: 0, p: 0.6 }, Skipped { k: 1, p: 0.4 }],
        },
    ];
    let mut l = Ledger::new();
    for e in &全 {
        l.put(e.clone());
    }
    let (读回, t) = Ledger::decode(&l.encode()).expect("v4 可读");
    assert!(t.is_none());
    assert_eq!(读回.entries, 全);
    assert!(
        全.iter().all(|e| e.key().is_empty() && e.is_duty_event()),
        "都不进键索引"
    );
}

#[test]
fn 缺省字段宽容_未知字段拒绝() {
    let 头 = 一行(&Entry::Drop {
        of: vec![],
        cause: String::new(),
        site: 0,
    });
    let head = 头.lines().next().unwrap();
    let 链 = |entry: &str| {
        let line = format!(
            r#"{{"seq":1,"prev":"{}","entry":{entry}}}"#,
            jpp::ledger::line_hash(head)
        );
        format!("{head}\n{line}\n")
    };
    let (l, _) = Ledger::decode(&链(r#"{"Enrich":{}}"#)).expect("字段都可缺省");
    assert!(matches!(
        &l.entries[0],
        Entry::Enrich {
            round: 0,
            got: false,
            asked_by: None,
            ..
        }
    ));
    let e = Ledger::decode(&链(r#"{"Drop":{"of":["k"],"extra":1}}"#)).expect_err("未知字段拒绝");
    assert!(e.starts_with("E-ledger-corrupt"), "{e}");
}

#[test]
fn v3照读_别的版本拒读() {
    // 步 34 V5：账本升到 v5，v3、v4 照读，更新的版本报 E-ledger-newer
    let v5 = 一行(&Entry::Drop {
        of: vec!["k".into()],
        cause: "band".into(),
        site: 1,
    });
    assert!(v5.starts_with(r#"{"version":5,"#), "{v5}");
    let 空头 = |v: u32| format!("{{\"version\":{v},\"header\":null}}\n");
    assert!(Ledger::decode(&空头(3)).is_ok(), "v3 照读");
    assert!(Ledger::decode(&空头(4)).is_ok(), "v4 照读");
    assert!(Ledger::decode(&空头(5)).is_ok());
    let e = Ledger::decode(&空头(6)).expect_err("v6 拒读");
    assert!(e.starts_with("E-ledger-newer"), "{e}");
}

/// 预算停机没观察到的项没有账本键：它们的 Handoff `of` 为空，同一份账本再跑一趟也不能重复写（按出现次数比，
/// 不按「有没有」）。第二趟用审计重放：续跑不把上一趟的调用计入预算，会多判一项、去向本来就不同；
/// 重放照记录计预算，去向与首跑相同。
#[test]
fn 没有账本键的转交再跑一趟也不重复写() {
    let src = r#"budget {calls: 1, cost: 0, depth: 8};
let o = sieve(["甲", "乙", "丙"], test("模糊题", "k"));
{pending: o.pending}
"#;
    let mut l = Ledger::new();
    跑在(src, false, &mut l);
    let 首跑: Vec<Entry> = 事件(&l).into_iter().cloned().collect();
    assert!(
        首跑
            .iter()
            .filter(|e| matches!(e, Entry::Handoff { of, .. } if of.is_empty()))
            .count()
            >= 2,
        "要有两条以上无键的转交，才测得到按次数比：{首跑:?}"
    );
    let program = lower(&parse(src).expect("解析")).expect("lower");
    let mut calib = CalibStore::new();
    calib.put("k", 0.75, 0.25, 50, "上岗", Some(0.05)).unwrap();
    let calls = RefCell::new(0);
    common::run_replay(
        &program,
        端口(&calls),
        &calib,
        &ActionRegistry::new(),
        &mut l,
    )
    .unwrap_or_else(|e| panic!("重放失败：{}", e.render()));
    assert_eq!(*calls.borrow(), 0, "重放零调用");
    let 再跑: Vec<Entry> = 事件(&l).into_iter().cloned().collect();
    assert_eq!(再跑, 首跑);
}

/// K 选一判断器：每道题回同一组概率，置换测过且一致（select 出 Pick）。
fn 选择端口<'a>(v: Vec<f64>) -> Ports<'a> {
    Ports::new().with(common::伴随中性judge("m", move |_s, qs| {
        Ok(JudgeResult {
            answers: qs.iter().map(|_| Answer::Choice(v.clone())).collect(),
            tokens: 0,
            cost: 0.0,
            mode_share: qs.iter().map(|_| Some(1.0)).collect(),
            perms: qs.iter().map(|_| 2).collect(),
            confidence: vec![],
        })
    }))
}

fn 改选跑(can: &str) -> Ledger {
    let src = format!(
        r#"budget {{calls: 4, cost: 0, depth: 16}};
let can = {can};
let e = cut(judge(state(mat("甲"), {{over: [mat("浇筑"), mat("砌墙"), mat("装窗")]}}), select("下一道工序", "k")), {{feasible: fn(k) {{ can[k] }}}});
let k = exit_kind(e);
consume(e, "drop");
k
"#
    );
    let program = lower(&parse(&src).expect("解析")).expect("lower");
    let mut l = Ledger::new();
    // 数去向事件条数，固定关伴随题（主控 2026-09-30：第二类）
    common::run_关(
        &program,
        选择端口(vec![0.6, 0.3, 0.1]),
        &CalibStore::new(),
        &ActionRegistry::new(),
        &mut l,
    )
    .unwrap_or_else(|e| panic!("运行失败：{}", e.render()));
    l
}

#[test]
fn feasible_改选记_reselect事件() {
    let l = 改选跑("[false, true, true]");
    let 键 = 判断键(&l);
    let ev = 事件(&l);
    assert!(
        matches!(ev.as_slice(), [Entry::Reselect { of, from: 0, chosen: Some(1), skipped, .. }]
            if of == &键 && skipped.len() == 1 && skipped[0].k == 0 && skipped[0].p == 0.6),
        "{ev:?}"
    );
}

#[test]
fn 没有可行候选记_reselect_未决另记去向() {
    let l = 改选跑("[false, false, false]");
    let 键 = 判断键(&l);
    let ev = 事件(&l);
    assert!(
        matches!(ev.as_slice(), [Entry::Reselect { of, from: 0, chosen: None, skipped, .. }, Entry::Drop { of: d, cause, .. }]
            if of == &键 && skipped.len() == 3 && d == &键 && cause == "infeasible"),
        "{ev:?}"
    );
}

/// 审计重放核去向事件（主控 2026-09-29，W-replay-duty）：账本少了一条去向事件，重放时多产生的那条报出来；正常重放不报
#[test]
fn 审计重放核去向事件() {
    let mut l = Ledger::new();
    跑在(两种去向, false, &mut l);
    let program = lower(&parse(两种去向).expect("解析")).expect("lower");
    let mut calib = CalibStore::new();
    calib.put("k", 0.75, 0.25, 50, "上岗", Some(0.05)).unwrap();
    let 重放 = |l: &mut Ledger| {
        let calls = RefCell::new(0);
        common::run_replay(&program, 端口(&calls), &calib, &ActionRegistry::new(), l)
            .unwrap_or_else(|e| panic!("重放失败：{}", e.render()))
    };
    let mut 正常 = l.clone();
    let o = 重放(&mut 正常);
    assert!(
        !o.trace
            .warnings
            .iter()
            .any(|w| w.starts_with("W-replay-duty")),
        "{:?}",
        o.trace.warnings
    );

    // 删掉那条 Drop（经编解码重建链，账本本身仍合法）
    let mut 缺 = Ledger::new();
    缺.header = l.header.clone();
    for e in l
        .entries
        .iter()
        .filter(|e| !matches!(e, Entry::Drop { .. }))
    {
        缺.put(e.clone());
    }
    let (mut 缺, _) = Ledger::decode(&缺.encode()).expect("可读");
    let o = 重放(&mut 缺);
    let w: Vec<&String> = o
        .trace
        .warnings
        .iter()
        .filter(|w| w.starts_with("W-replay-duty"))
        .collect();
    assert_eq!(w.len(), 1, "{:?}", o.trace.warnings);
    assert!(w[0].contains("1 条") && w[0].contains("Drop"), "{}", w[0]);
}

/// 审计重放核去向事件 (b)（主控 2026-09-30，C-2 段编号合入后）：账本本段有、重放没产生的去向事件也报
#[test]
fn 审计重放核少了的去向事件() {
    let program = lower(&parse(两种去向).expect("解析")).expect("lower");
    let mut calib = CalibStore::new();
    calib.put("k", 0.75, 0.25, 50, "上岗", Some(0.05)).unwrap();
    let a = ActionRegistry::new();
    let mut l = Ledger::new();
    {
        let calls = RefCell::new(0);
        jpp::Session::new(端口(&calls), &calib, &a)
            .with_companions(common::伴随())
            .run(&program, &jpp::EntryArgs::default(), &mut l)
            .unwrap_or_else(|e| panic!("{}", e.render()));
    }
    let 段 = l.last_trace().cloned().expect("C-2：会话给账本盖了段编号");
    // 同一段里多记一条重放不会产生的去向事件
    l.set_trace(Some(段));
    l.put(Entry::Drop {
        of: vec!["不存在的键".into()],
        cause: "band".into(),
        site: 1,
    });
    let (mut l, _) = Ledger::decode(&l.encode()).expect("可读");
    let calls = RefCell::new(0);
    let o = jpp::Session::new(端口(&calls), &calib, &a)
        .with_companions(common::伴随())
        .replay(&program, &jpp::EntryArgs::default(), &mut l)
        .unwrap_or_else(|e| panic!("{}", e.render()));
    let w: Vec<&String> = o
        .trace
        .warnings
        .iter()
        .filter(|w| w.starts_with("W-replay-duty"))
        .collect();
    assert_eq!(w.len(), 1, "{:?}", o.trace.warnings);
    assert!(w[0].contains("少产生了 1 条"), "{}", w[0]);
}
