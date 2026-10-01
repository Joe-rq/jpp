//! Z0556（含 Z0562）：语言自己发的元题——伴随题（含 K 选一「最缺哪类」）与默认链「为什么拿不准」——回答形状不符时降级：
//! 丢掉这道读数、记 `Absent{fail:shape}`、报 `W-companion-shape`，程序照常；作者自己的题形状不符照旧中止。
//! 「最缺哪类」的「没选出」与裁定五十 select 并列同一判法（差 ≤ δ_mid，δ 未知退化为恰好相等），没选出时 improve 的
//! `pick` 为 null。预注册见过程记录 5.26。

use jpp::effects::{CalibStore, FnPort, JudgeResult, Ports, Profile};
use jpp::interp::CompanionMode;
use jpp::ledger::{Entry, Ledger};
use jpp::value::{Answer, Op};
use jpp::{ActionRegistry, EntryArgs, Outcome, Session, lower, syntax::parse};
use serde_json::{Value as Json, json};
use std::cell::RefCell;

/// `选`：「最缺哪类」那道给的概率（None = 对所有题都答是非读数，即只会答是非的端口）；原题读 `原`
fn 端口<'a>(calls: &'a RefCell<u64>, 原: f64, 选: Option<Vec<f64>>) -> Ports<'a> {
    Ports::new().with(FnPort::judge("fixed-0", move |_s, qs| {
        *calls.borrow_mut() += 1;
        Ok(JudgeResult {
            answers: qs
                .iter()
                .map(|q| match (&选, q.op) {
                    (Some(v), Op::Select) => Answer::Choice(v.clone()),
                    _ if q.calib.starts_with("unsure-companion-")
                        || q.calib == "diag-two-judgments" =>
                    {
                        Answer::Noul(0.9)
                    }
                    _ => Answer::Noul(原),
                })
                .collect(),
            tokens: 0,
            cost: 0.0,
            mode_share: vec![],
            perms: vec![],
            confidence: vec![],
        })
    }))
}

struct 跑出 {
    outcome: Result<Outcome, String>,
    ledger: Ledger,
    calls: u64,
}

fn 跑(
    src: &str,
    mode: CompanionMode,
    原: f64,
    选: Option<Vec<f64>>,
    profile: Option<Profile>,
    ledger: Ledger,
    重放: bool,
) -> 跑出 {
    let program = lower(&parse(src).expect("解析")).expect("lower");
    let calls = RefCell::new(0);
    let mut calib = CalibStore::new();
    if let Some(p) = profile {
        calib.profile = p;
    }
    let a = ActionRegistry::new();
    let mut ledger = ledger;
    let s = Session::new(端口(&calls, 原, 选), &calib, &a).with_companions(mode);
    let outcome = if 重放 {
        s.replay(&program, &EntryArgs::default(), &mut ledger)
    } else {
        s.run(&program, &EntryArgs::default(), &mut ledger)
    }
    .map_err(|e| e.render());
    let calls = *calls.borrow();
    跑出 {
        outcome,
        ledger,
        calls,
    }
}

const 一题: &str = r#"budget {calls: 8, cost: 0, depth: 16};
let e = cut(judge(state(mat("甲方与乙方的合作意向")), test("这两方适合合作吗？", "k")), {declare: {hi: 0.7, lo: 0.3}});
handle(e, {act: fn() { "合作" }, ignore: fn() { "不合作" }, unsure: fn(u) { {exit: u} }})
"#;

fn 告警数(o: &Outcome, 码: &str) -> usize {
    o.trace
        .warnings
        .iter()
        .filter(|w| w.starts_with(码))
        .count()
}

/// 只会答是非的端口跑带伴随题的程序：照常完成，报 W-companion-shape，「最缺哪类」那道记 Absent{fail:shape}、不记 Judge；
/// 审计重放零调用、与首跑一致
#[test]
fn 只答是非的端口_伴随题降级_程序照常() {
    let r = 跑(
        一题,
        CompanionMode::Same,
        0.9,
        None,
        None,
        Ledger::new(),
        false,
    );
    let o = r
        .outcome
        .as_ref()
        .unwrap_or_else(|e| panic!("应照常完成：{e}"));
    assert_eq!(o.value_json(), json!("合作"));
    assert_eq!(告警数(o, "W-companion-shape"), 1, "{:?}", o.trace.warnings);
    let 形状: Vec<&Entry> = r
        .ledger
        .entries
        .iter()
        .filter(|e| matches!(e, Entry::Absent { cause, .. } if cause == "fail:shape"))
        .collect();
    assert_eq!(形状.len(), 1, "{形状:?}");
    let 选题条目 = r
        .ledger
        .entries
        .iter()
        .filter(|e| matches!(e, Entry::Judge { jkey: Some(k), .. } if k.phys == "choice"))
        .count();
    assert_eq!(选题条目, 0, "丢掉的那道不写 Judge");
    let 材 = o.improve[0]["companions"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["kind"] == "unsure-companion-material")
        .unwrap()
        .clone();
    assert_eq!(材["pick"], Json::Null);
    assert_eq!(材["p"], Json::Null);
    assert_eq!(o.improve[0]["lacks"], Json::Null);
    // 审计重放：照 Absent 记录给同一个标记，不再发
    let r2 = 跑(
        一题,
        CompanionMode::Same,
        0.9,
        None,
        None,
        r.ledger.clone(),
        true,
    );
    let o2 = r2.outcome.unwrap_or_else(|e| panic!("重放：{e}"));
    assert_eq!(o2.value_json(), o.value_json());
    assert_eq!(r2.calls, 0);
    assert!(
        !o2.trace
            .warnings
            .iter()
            .any(|w| w.starts_with("W-replay-duty")),
        "{:?}",
        o2.trace.warnings
    );
}

/// Z0562：默认链「为什么拿不准」那道形状不符同样降级：切出 Unsure(fail:shape)，按诊断题自己拿不准记账放弃它、停，程序照常
#[test]
fn 为什么拿不准形状不符_降级_链停() {
    let src = r#"budget {calls: 8, cost: 0, depth: 16};
unsure_source({need: ["双方目标", "过往合作"]});
let e = cut(judge(state(mat("甲方与乙方的合作意向")), test("这两方适合合作吗？", "k")));
handle(e, {act: fn() { "合作" }, ignore: fn() { "不合作" }})
"#;
    let r = 跑(
        src,
        CompanionMode::Off,
        0.5,
        None,
        None,
        Ledger::new(),
        false,
    );
    let o = r
        .outcome
        .as_ref()
        .unwrap_or_else(|e| panic!("应照常完成：{e}"));
    assert_eq!(告警数(o, "W-companion-shape"), 1, "{:?}", o.trace.warnings);
    assert_eq!(
        o.unsure_default[0]["why"], "unsure",
        "{:?}",
        o.unsure_default
    );
    assert!(
        r.ledger
            .entries
            .iter()
            .any(|e| matches!(e, Entry::Absent { cause, .. } if cause == "fail:shape"))
    );
}

/// 作者自己的 K 选一题形状不符照旧中止
#[test]
fn 作者自己的题形状不符照旧中止() {
    let src = r#"budget {calls: 8, cost: 0, depth: 16};
let e = cut(judge(state(mat("甲"), {over: [mat("A"), mat("B")]}), select("哪个？", "k")));
{e: e}
"#;
    let e = 跑(
        src,
        CompanionMode::Off,
        0.5,
        None,
        None,
        Ledger::new(),
        false,
    )
    .outcome
    .err()
    .expect("E-rt-answer");
    assert!(e.contains("E-rt-answer"), "{e}");
}

fn 带choice_delta的画像() -> Profile {
    let p = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../profiles/jev-1.13.0.json");
    let j: Json = serde_json::from_str(&std::fs::read_to_string(p).unwrap()).unwrap();
    let pr = Profile::from_json(&j).unwrap();
    assert!(
        pr.delta_prior(Op::Select).is_some_and(|d| d > 0.05),
        "本测试假设 choice 中段 δ 大于 0.05"
    );
    pr
}

/// 「没选出」与裁定五十同一判法：最大两个概率差 ≤ δ_mid 没选出（pick null），差 > δ_mid 选出；δ 未知时只有恰好相等才没选出
#[test]
fn 没选出判法按delta_mid() {
    // 候选：通用表三类 + 「材料不缺」
    let 近 = vec![0.40, 0.36, 0.14, 0.10];
    let 远 = vec![0.70, 0.10, 0.10, 0.10];
    let 看 = |r: &跑出| {
        let o = r.outcome.as_ref().unwrap_or_else(|e| panic!("{e}"));
        let 材 = o.improve[0]["companions"]
            .as_array()
            .unwrap()
            .iter()
            .find(|c| c["kind"] == "unsure-companion-material")
            .unwrap()
            .clone();
        (材["pick"].clone(), o.improve[0]["lacks"].clone())
    };
    let r = 跑(
        一题,
        CompanionMode::Same,
        0.9,
        Some(近.clone()),
        Some(带choice_delta的画像()),
        Ledger::new(),
        false,
    );
    assert_eq!(看(&r), (Json::Null, Json::Null), "差 0.04 ≤ δ_mid：没选出");
    let r = 跑(
        一题,
        CompanionMode::Same,
        0.9,
        Some(远),
        Some(带choice_delta的画像()),
        Ledger::new(),
        false,
    );
    assert_eq!(看(&r), (json!(0), json!("材料")), "差 0.6 > δ_mid：选出");
    // δ 未知：差 0.04 也算选出（只有恰好相等才没选出）
    let r = 跑(
        一题,
        CompanionMode::Same,
        0.9,
        Some(近),
        None,
        Ledger::new(),
        false,
    );
    assert_eq!(看(&r), (json!(0), json!("材料")));
    let r = 跑(
        一题,
        CompanionMode::Same,
        0.9,
        Some(vec![0.3, 0.3, 0.2, 0.2]),
        None,
        Ledger::new(),
        false,
    );
    assert_eq!(看(&r), (Json::Null, Json::Null), "恰好相等：没选出");
}
