//! J-05 默认链（主控板 B0492 S2）：`handle` 没写 `unsure` 臂时，未决走语言的默认去向——问缺哪类信息 →
//! 从 `unsure_source` 声明的来源取来补进材料 → 在补过的材料上用同一条线再判；补不到或补了仍拿不准就记账放弃
//! （`Drop`），缺席类原因转交（`Handoff`）。每一轮在账本记 `Enrich`，被再判取代的旧出口记 `Refine{how: default}`。
//!
//! 未决都是真的：无线的判断读数恰为 0.5 是并列 `Unsure(tie)`（B187），或读数落进作者划的区间 `Unsure(band)`
//! （主会话裁定六）。预注册见过程记录 `工程-未决去向.md` 5.3，用例编号与那里的表一一对应。

mod common;
use common::{run_replay_关 as run_replay, run_关 as run};
use jpp::effects::{CalibStore, FnPort, JudgeResult, Ports};
use jpp::ledger::{Entry, Ledger};
use jpp::value::Answer;
use jpp::{ActionRegistry, Outcome, lower, syntax::parse};
use serde_json::{Value as Json, json};
use std::cell::RefCell;

/// 判断器替身：「为什么拿不准」那道 select 回 `为什么`（按 over 长度截断、补零）；别的题：补过（`ctx` 非空）读 `补后`，
/// 没补过读 0.5。每次调用记一笔。
fn 端口<'a>(calls: &'a RefCell<u64>, 为什么: Vec<f64>, 补后: f64) -> Ports<'a> {
    Ports::new().with(FnPort::judge("fixed-0", move |s, qs| {
        *calls.borrow_mut() += 1;
        Ok(JudgeResult {
            answers: qs
                .iter()
                .map(|q| {
                    if q.text.contains("为什么拿不准") {
                        let mut v = 为什么.clone();
                        v.resize(s.over.len(), 0.0);
                        Answer::Choice(v)
                    } else {
                        Answer::Noul(if s.ctx.is_empty() { 0.5 } else { 补后 })
                    }
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

fn 跑(src: &str, 为什么: Vec<f64>, guard: bool) -> 跑出 {
    跑_补后(src, 为什么, 0.9, guard)
}

fn 跑_补后(src: &str, 为什么: Vec<f64>, 补后: f64, guard: bool) -> 跑出 {
    let mut program = lower(&parse(src).expect("解析")).expect("lower");
    program.entry.guard = guard;
    let calls = RefCell::new(0);
    let mut ledger = Ledger::new();
    let outcome = run(
        &program,
        端口(&calls, 为什么, 补后),
        &CalibStore::new(),
        &ActionRegistry::new(),
        &mut ledger,
    )
    .map_err(|e| e.render());
    let calls = *calls.borrow();
    跑出 {
        outcome,
        ledger,
        calls,
    }
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

fn 未决告警(o: &Outcome) -> Vec<&String> {
    o.trace
        .warnings
        .iter()
        .filter(|w| w.starts_with("W-unsure-default"))
        .collect()
}

/// 一道是非题、臂表没有 unsure；`来源` 是 `unsure_source` 那一行（可空），`线` 是 cut 的第二个参数（可空）。
fn 程序(来源: &str, 线: &str, calls: u32) -> String {
    format!(
        r#"budget {{calls: {calls}, cost: 0, depth: 16}};
{来源}
let e = cut(judge(state(mat("甲方与乙方的合作意向")), test("这两方适合合作吗？", "k")){线});
handle(e, {{act: fn() {{ "合作" }}, ignore: fn() {{ "不合作" }}}})
"#
    )
}

const 两类: &str = r#"unsure_source({need: ["双方目标", "过往合作"], fetch: fn(q, need, m) { "关于" + need + "的补充" }});"#;
const 一类: &str =
    r#"unsure_source({need: ["双方目标"], fetch: fn(q, need, m) { "关于" + need + "的补充" }});"#;
const 取不到: &str =
    r#"unsure_source({need: ["双方目标"], fetch: fn(q, need, m) { fail("没有这一类") }});"#;

fn 值(o: &Outcome) -> Json {
    o.value_json()
}

#[test]
fn 用例1_两类_选中第二类_补后已决进作者的臂() {
    let r = 跑(&程序(两类, "", 8), vec![0.2, 0.6, 0.1, 0.1], false);
    let o = r.outcome.expect("跑通");
    assert_eq!(值(&o), json!("合作"), "补过材料再判 0.9，进 act 臂");
    assert_eq!(r.calls, 3, "原题、为什么、再判");
    let 键 = 判断键(&r.ledger);
    assert_eq!(键.len(), 3);
    let ev = 事件(&r.ledger);
    let [
        Entry::Enrich {
            of,
            need,
            round: 1,
            got: true,
            asked_by,
            cause,
            ..
        },
        Entry::Refine {
            of: of2, how, to, ..
        },
    ] = ev.as_slice()
    else {
        panic!("应是 Enrich、Refine：{ev:?}")
    };
    assert_eq!(of, &vec![键[0].clone()]);
    assert_eq!(cause, "tie", "无线恰 0.5 是并列（B187）");
    assert_eq!(need, "过往合作");
    assert_eq!(
        asked_by.as_deref(),
        Some(键[1].as_str()),
        "asked_by 是「为什么」那道题"
    );
    assert_eq!(of2, &vec![键[0].clone()]);
    assert_eq!(how, "default");
    assert_eq!(to.as_deref(), Some(键[2].as_str()));
    assert!(未决告警(&o).is_empty());
    assert_eq!(o.unsure_default.len(), 1);
    assert_eq!(o.unsure_default[0]["end"], "decided");
    assert_eq!(
        o.unsure_default[0]["asked"],
        json!(["双方目标", "过往合作"])
    );
    assert_eq!(o.unsure_default[0]["fetched"], json!(["过往合作"]));
}

#[test]
fn 用例2_一类_补后仍并列_记账放弃() {
    // 替身对补过的材料也读 0.5：无线仍是并列
    let r = 跑_补后(&程序(一类, "", 8), vec![], 0.5, false);
    let o = r.outcome.expect("跑通");
    assert_eq!(r.calls, 2, "只有一类不发「为什么」");
    let 键 = 判断键(&r.ledger);
    let ev = 事件(&r.ledger);
    assert!(
        matches!(ev.as_slice(), [
            Entry::Enrich { got: true, asked_by: None, .. },
            Entry::Refine { to: Some(t), .. },
            Entry::Drop { of, cause, .. }
        ] if t == &键[1] && of == &vec![键[1].clone()] && cause == "tie"),
        "{ev:?}"
    );
    assert_eq!(未决告警(&o).len(), 1);
    assert!(未决告警(&o)[0].contains("1 个"), "{:?}", 未决告警(&o));
    assert_eq!(o.unsure_default[0]["end"], "drop");
}

/// 配置了取法、各级都取不到：Z0398 返修起照裁定五十五走路 C（主控板 Z0502 默认读法，过程记录 5.23）——
/// 取不到的类别进 needed、转交，不再记放弃；取过的那一轮仍记 Enrich{got: false}
#[test]
fn 用例3_取不到_路c转交() {
    let r = 跑(&程序(取不到, "", 8), vec![], false);
    let o = r.outcome.expect("跑通");
    assert_eq!(r.calls, 1);
    let 键 = 判断键(&r.ledger);
    let ev = 事件(&r.ledger);
    assert!(
        matches!(ev.as_slice(), [Entry::Enrich { got: false, .. }, Entry::Handoff { of, .. }] if of == &键),
        "{ev:?}"
    );
    assert_eq!(o.unsure_default[0]["needed"], json!(["双方目标"]));
    assert_eq!(o.unsure_default[0]["end"], "handoff");
    assert_eq!(o.unsure_default[0]["missed"], json!(["双方目标"]));
    assert_eq!(未决告警(&o).len(), 1);
}

/// Z0398 A 步（过程记录 5.19，裁定五十二 (b)）起没写 unsure_source 不再直接放弃：候选取通用表，照样发「为什么拿不准」。
/// 这里替身对「为什么」给全 0，诊断题自己拿不准：记账放弃它、停，原题末端照旧记 Drop。路 C 另见 `unsure_lacks.rs`
#[test]
fn 用例4_没有声明来源_照取通用表() {
    let r = 跑(&程序("", "", 8), vec![], false);
    let o = r.outcome.expect("跑通");
    assert_eq!(r.calls, 2, "原题与「为什么拿不准」");
    let 键 = 判断键(&r.ledger);
    let ev = 事件(&r.ledger);
    assert!(
        matches!(ev.as_slice(), [Entry::Drop { of: w, .. }, Entry::Drop { of, cause, .. }] if w == &vec![键[1].clone()] && of == &vec![键[0].clone()] && cause == "tie"),
        "{ev:?}"
    );
    assert_eq!(o.unsure_default[0]["end"], "drop");
    assert_eq!(o.unsure_default[0]["source"], "generic");
    assert_eq!(o.unsure_default[0]["why"], "unsure");
}

#[test]
fn 用例5_选两可_不补() {
    let r = 跑(&程序(两类, "", 8), vec![0.1, 0.1, 0.7, 0.1], false);
    let o = r.outcome.expect("跑通");
    assert_eq!(r.calls, 2);
    let 键 = 判断键(&r.ledger);
    let ev = 事件(&r.ledger);
    assert!(
        matches!(ev.as_slice(), [Entry::Drop { of, .. }] if of == &vec![键[0].clone()]),
        "{ev:?}"
    );
    assert_eq!(o.unsure_default[0]["why"], "ambiguous");
}

#[test]
fn 用例6_选题不清_不补() {
    let r = 跑(&程序(两类, "", 8), vec![0.1, 0.1, 0.1, 0.7], false);
    let o = r.outcome.expect("跑通");
    assert_eq!(r.calls, 2);
    let ev = 事件(&r.ledger);
    assert!(matches!(ev.as_slice(), [Entry::Drop { .. }]), "{ev:?}");
    assert_eq!(o.unsure_default[0]["why"], "unclear");
}

#[test]
fn 用例7_为什么自己并列_两道都记账放弃() {
    let r = 跑(&程序(两类, "", 8), vec![0.4, 0.4, 0.1, 0.1], false);
    let o = r.outcome.expect("跑通");
    assert_eq!(r.calls, 2);
    let 键 = 判断键(&r.ledger);
    let ev = 事件(&r.ledger);
    assert!(
        matches!(ev.as_slice(), [Entry::Drop { of: a, .. }, Entry::Drop { of: b, .. }]
            if a == &vec![键[1].clone()] && b == &vec![键[0].clone()]),
        "{ev:?}"
    );
    assert_eq!(o.unsure_default[0]["why"], "unsure");
}

#[test]
fn 用例8_作者划的区间_再判用同一条线() {
    let r = 跑(
        &程序(一类, r#", {declare: {hi: 0.7, lo: 0.3}}"#, 8),
        vec![],
        false,
    );
    let o = r.outcome.expect("跑通");
    assert_eq!(值(&o), json!("合作"), "补后 0.9 高于作者的 0.7");
    assert_eq!(r.calls, 2);
    let ev = 事件(&r.ledger);
    assert!(
        matches!(ev.as_slice(), [Entry::Enrich { cause, .. }, Entry::Refine { .. }] if cause == "band"),
        "{ev:?}"
    );
    assert_eq!(o.unsure_default[0]["end"], "decided");
}

#[test]
fn 用例9_预算用完_转交() {
    let r = 跑(&程序(一类, "", 1), vec![], false);
    let o = r.outcome.expect("跑通");
    assert_eq!(r.calls, 1, "再判没发出去");
    let 键 = 判断键(&r.ledger);
    let ev = 事件(&r.ledger);
    assert!(
        matches!(ev.as_slice(), [Entry::Enrich { .. }, Entry::Refine { to: Some(_), .. }, Entry::Handoff { cause, to, .. }]
            if cause == "budget" && to == "program"),
        "{ev:?}；判断键 {键:?}"
    );
    // Z0602（过程记录 5.30）：缺席类在链末端随值走，报告行写 carry；handle 的值是程序结果，随返回值交出时由程序结束
    // 写上面那条 Handoff
    assert_eq!(o.unsure_default[0]["end"], "carry");
    assert_eq!(未决告警(&o).len(), 1);
}

#[test]
fn 用例10_guard下缺臂仍是运行期错() {
    let r = 跑(&程序(两类, "", 8), vec![], true);
    // G2（步 35，附录二）：原断言「--guard 下缺 unsure 臂是运行期 J-05 错」。改后不当场报错、不走默认链：
    // 这笔作为未决值往下传，本程序把它作为程序值返回（转交宿主），没有违规
    let o = r.outcome.unwrap_or_else(|e| panic!("不再当场报错：{e}"));
    assert_eq!(o.returned_unsure.len(), 1, "{:?}", o.returned_unsure);
    assert!(o.violations.is_empty());
    assert!(o.unsure_default.is_empty(), "守卫下不走默认链");
    assert!(
        事件(&r.ledger)
            .iter()
            .all(|e| matches!(e, Entry::Handoff { .. })),
        "只有转交"
    );
}

#[test]
fn 用例11_同一份账本审计重放_事件相同零调用() {
    let src = 程序(两类, "", 8);
    let mut r = 跑(&src, vec![0.2, 0.6, 0.1, 0.1], false);
    let 首跑: Vec<Entry> = 事件(&r.ledger).into_iter().cloned().collect();
    let program = lower(&parse(&src).expect("解析")).expect("lower");
    let calls = RefCell::new(0);
    let o = run_replay(
        &program,
        端口(&calls, vec![0.2, 0.6, 0.1, 0.1], 0.9),
        &CalibStore::new(),
        &ActionRegistry::new(),
        &mut r.ledger,
    )
    .map_err(|e| e.render())
    .expect("重放");
    assert_eq!(*calls.borrow(), 0);
    assert_eq!(值(&o), json!("合作"));
    let 再跑: Vec<Entry> = 事件(&r.ledger).into_iter().cloned().collect();
    assert_eq!(再跑, 首跑);
}

#[test]
fn 用例12_refine内置记补信息与细化() {
    let src = r#"budget {calls: 4, cost: 0, depth: 16};
let e = cut(judge(state(mat("甲")), test("模糊题", "k")));
let e2 = cut(judge(state(mat("甲"), {ctx: [mat("补来的")]}), test("模糊题", "k")));
handle(e, {act: fn() { 1 }, ignore: fn() { 0 },
           unsure: fn(u) { refine(u, e2, {need: "证据", round: 1}); handle(e2, {act: fn() { 1 }, ignore: fn() { 0 }, unsure: fn(v) { {exit: v} }}) }})
"#;
    let r = 跑(src, vec![], false);
    let o = r.outcome.expect("跑通");
    assert_eq!(值(&o), json!(1));
    let 键 = 判断键(&r.ledger);
    let ev = 事件(&r.ledger);
    assert!(
        matches!(ev.as_slice(), [
            Entry::Enrich { of, need, round: 1, got: true, asked_by: None, .. },
            Entry::Refine { of: of2, how, to: Some(t), .. }
        ] if of == &vec![键[0].clone()] && need == "证据" && of2 == of && how == "default" && t == &键[1]),
        "{ev:?}"
    );
    assert!(
        o.unsure_default.is_empty(),
        "作者写了 unsure 臂，不走默认链"
    );
}

#[test]
fn 检查器_缺臂只报提示() {
    let program = lower(&parse(&程序(两类, "", 8)).expect("解析")).expect("lower");
    let report = jpp::check(&program);
    assert!(report.find("J-05").is_none(), "{}", report.render());
    assert!(
        report.find("N-unsure-default").is_some(),
        "{}",
        report.render()
    );
}

// ---- S2c：「无作者去向」站点当场走默认链（预注册 5.6）----

fn 编跑(src: &str, guard: bool) -> 跑出 {
    // 与 `跑` 相同，但经 Session::compile 写进「无作者去向」站点（lower 就是它的薄包装）
    跑(src, vec![], guard)
}

const 只读出口: &str = r#"budget {calls: 8, cost: 0, depth: 16};
unsure_source({need: ["双方目标"], fetch: fn(q, need, m) { "关于" + need + "的补充" }});
let e = cut(judge(state(mat("甲方与乙方的合作意向")), test("这两方适合合作吗？", "k")));
exit_kind(e)
"#;

#[test]
fn s2c_只读出口的cut站点当场走链_下游读到补后的出口() {
    let r = 编跑(只读出口, false);
    let o = r.outcome.expect("不写任何 handle 也能跑完");
    assert_eq!(
        值(&o),
        json!("act"),
        "补过材料再判 0.9，下游 exit_kind 读到 act"
    );
    assert_eq!(r.calls, 2);
    let ev = 事件(&r.ledger);
    assert!(
        matches!(ev.as_slice(), [Entry::Enrich { .. }, Entry::Refine { .. }]),
        "{ev:?}"
    );
    assert_eq!(o.unsure_default[0]["end"], "decided");
}

#[test]
fn s2c_补后仍并列_记账放弃_不报j05() {
    let r = 跑_补后(只读出口, vec![], 0.5, false);
    let o = r.outcome.expect("跑完");
    assert_eq!(值(&o), json!("unsure(tie)"));
    let ev = 事件(&r.ledger);
    assert!(matches!(ev.last(), Some(Entry::Drop { .. })), "{ev:?}");
    assert_eq!(未决告警(&o).len(), 1);
}

#[test]
fn s2c_guard下不走链_运行期j05() {
    let r = 编跑(只读出口, true);
    // G2（步 35）：原断言「--guard 下没人接的未决是运行期 J-05」，改为程序结束记一笔违规，值照带
    let o = r
        .outcome
        .unwrap_or_else(|e| panic!("违规不再是运行期错误：{e}"));
    assert_eq!(o.violations.len(), 1);
    assert_eq!(值(&o), json!("unsure(tie)"));
    assert!(事件(&r.ledger).is_empty());
}

// ---- S2b：读数触发（预注册 5.7）----

use jpp::effects::Profile;

/// 判断器替身：没补过读 `原`，补过读 `补后`
fn 读数端口<'a>(calls: &'a RefCell<u64>, 原: f64, 补后: f64) -> Ports<'a> {
    Ports::new().with(FnPort::judge("fixed-0", move |s, qs| {
        *calls.borrow_mut() += 1;
        Ok(JudgeResult {
            answers: qs
                .iter()
                .map(|_| Answer::Noul(if s.ctx.is_empty() { 原 } else { 补后 }))
                .collect(),
            tokens: 0,
            cost: 0.0,
            mode_share: vec![],
            perms: vec![],
            confidence: vec![],
        })
    }))
}

/// `jev-1.13.0` 的副本：`band` 给了就把 `delta.noul.mid` 设成它（三列 mid 都在，才算已测）；`None` = 画像完全没有 δ
/// （Z0398 C 步起读数带取画像中段 δ，过程记录 5.21）
fn 画像(band: Option<f64>) -> Profile {
    let Some(b) = band else {
        return Profile {
            hash: Some("测试".into()),
            ..Profile::untested()
        };
    };
    let p = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../profiles/jev-1.13.0.json");
    let mut j: Json = serde_json::from_str(&std::fs::read_to_string(p).unwrap()).unwrap();
    j["delta"]["noul"]["mid"]["immediate"]["p99"] = json!(b);
    Profile::from_json(&j).unwrap()
}

fn 带宽跑(src: &str, 原: f64, 补后: f64, band: Option<f64>, guard: bool) -> 跑出 {
    画像跑(src, 原, 补后, 画像(band), guard)
}

fn 画像跑(src: &str, 原: f64, 补后: f64, profile: Profile, guard: bool) -> 跑出 {
    let mut program = lower(&parse(src).expect("解析")).expect("lower");
    program.entry.guard = guard;
    let mut calib = CalibStore::new();
    calib.profile = profile;
    let calls = RefCell::new(0);
    let mut ledger = Ledger::new();
    let outcome = run(
        &program,
        读数端口(&calls, 原, 补后),
        &calib,
        &ActionRegistry::new(),
        &mut ledger,
    )
    .map_err(|e| e.render());
    let calls = *calls.borrow();
    跑出 {
        outcome,
        ledger,
        calls,
    }
}

fn 读数程序(来源: &str) -> String {
    format!(
        r#"budget {{calls: 8, cost: 0, depth: 16}};
{来源}
let e = cut(judge(state(mat("甲方与乙方的合作意向")), test("这两方适合合作吗？", "k")));
handle(e, {{act: fn() {{ "合作" }}, ignore: fn() {{ "不合作" }}, unsure: fn(u) {{ {{exit: u}} }}}})
"#
    )
}

#[test]
fn s2b_1_近边界且有来源_补后定出口() {
    let r = 带宽跑(&读数程序(一类), 0.45, 0.9, Some(0.1), false);
    let o = r.outcome.expect("跑通");
    assert_eq!(
        值(&o),
        json!("合作"),
        "0.45 按多数块本是 ignore，补后 0.9 定为 act"
    );
    assert_eq!(r.calls, 2);
    let ev = 事件(&r.ledger);
    assert!(
        matches!(ev.as_slice(), [Entry::Enrich { cause: a, .. }, Entry::Refine { cause: b, how, .. }]
            if a == "near_boundary" && b == "near_boundary" && how == "default"),
        "{ev:?}"
    );
    assert_eq!(o.unsure_default[0]["end"], "decided");
    assert_eq!(o.unsure_default[0]["cause"], "near_boundary");
}

#[test]
fn s2b_2_近边界但没有来源_按多数块并记位() {
    let r = 带宽跑(&读数程序(""), 0.45, 0.9, Some(0.1), false);
    let o = r.outcome.expect("跑通");
    assert_eq!(值(&o), json!("不合作"), "补不了按多数块");
    assert_eq!(r.calls, 1);
    assert!(
        o.exits.iter().any(|x| x["near_boundary"] == json!(true)),
        "{:?}",
        o.exits
    );
    assert_eq!(o.unsure_default[0]["end"], "near_boundary");
    assert!(事件(&r.ledger).is_empty());
}

#[test]
fn s2b_3_离边界够远_不触发() {
    let r = 带宽跑(&读数程序(一类), 0.7, 0.9, Some(0.1), false);
    assert_eq!(r.calls, 1);
    assert!(r.outcome.expect("跑通").unsure_default.is_empty());
}

/// 画像完全没有 δ（裁定五十六，过程记录 5.21）：不触发、不编 0；出口行带 delta_unknown
#[test]
fn s2b_4_画像没有delta_不触发并带位() {
    let r = 带宽跑(&读数程序(一类), 0.45, 0.9, None, false);
    assert_eq!(r.calls, 1);
    let o = r.outcome.expect("跑通");
    assert_eq!(值(&o), json!("不合作"));
    assert!(
        o.exits.iter().any(|x| x["delta_unknown"] == json!(true)),
        "{:?}",
        o.exits
    );
    assert!(o.unsure_default.is_empty());
    // --guard 下报一次 W-delta-unknown
    let r = 带宽跑(&读数程序(一类), 0.45, 0.9, None, true);
    let o = r.outcome.expect("跑通");
    assert_eq!(
        o.trace
            .warnings
            .iter()
            .filter(|w| w.starts_with("W-delta-unknown"))
            .count(),
        1,
        "{:?}",
        o.trace.warnings
    );
}

#[test]
fn s2b_5_guard下不触发() {
    let r = 带宽跑(&读数程序(一类), 0.45, 0.9, Some(0.1), true);
    assert_eq!(r.calls, 1);
    assert_eq!(值(&r.outcome.expect("跑通")), json!("不合作"));
}

/// 带宽取画像中段 δ（裁定五十二 (a)）：`boundary_band` 不再是画像字段；测了尾段而中段不全报 E-delta-mid（裁定四十五），
/// `--guard` 下不触发也就不报
#[test]
fn s2b_6_带宽取中段delta_缺mid报错() {
    let e = Profile::from_json(&json!({"boundary_band": 0.1}))
        .expect_err("boundary_band 不再是画像字段");
    assert!(e.contains("boundary_band"), "{e}");
    let p = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../profiles/jev-1.13.0.json");
    let mut j: Json = serde_json::from_str(&std::fs::read_to_string(p).unwrap()).unwrap();
    // 发行画像的 δ_mid（是非 0.1281）：0.45 离边界 0.05 在带内，触发
    let r = 画像跑(
        &读数程序(一类),
        0.45,
        0.9,
        Profile::from_json(&j).unwrap(),
        false,
    );
    assert_eq!(值(&r.outcome.expect("跑通")), json!("合作"));
    j["delta"]["noul"].as_object_mut().unwrap().remove("mid");
    let 缺 = Profile::from_json(&j).unwrap();
    assert!(缺.delta_mid_missing());
    let e = 画像跑(&读数程序(一类), 0.45, 0.9, 缺.clone(), false)
        .outcome
        .expect_err("缺 mid 报错");
    assert!(e.contains("E-delta-mid"), "{e}");
    let r = 画像跑(&读数程序(一类), 0.45, 0.9, 缺, true);
    assert_eq!(
        值(&r.outcome.expect("--guard 下不触发、不报")),
        json!("不合作")
    );
}

// ---- 主控复核 2026-09-30 的回归 ----

/// B162：作者随返回值交出的那份，不被另一视图的默认链记成 Drop（复核探针 p2、p3）。
/// 站点都不标。G1 步 33（裁定五十九，过程记录 5.25）起检查器对这类别名写法报 `W-unsure-untracked`、不拒，交运行期：
/// p3（两次 `cut(rs[0])`）记 Handoff；p2（嵌套函数里第二个 `cut(r)`）函数 f 返回时 b 这一视图
/// 既不在返回值里、键也还没解除（同键的 a 在外层帧，返回检查看不到它），按 B162 的帧返回规则报 J-05——
/// 这是 B162 原有的口径，不是本线引入的（Z0396）
#[test]
fn 复核_p2_p3_随返回值交出记handoff() {
    let p3 = "budget {calls: 8, cost: 0, depth: 16};\nlet rs = [judge(state(mat(\"甲\")), test(\"未决吗\", \"k\"))];\nlet a = cut(rs[0]);\nlet b = cut(rs[0]);\n{a: a}\n";
    let p2 = "budget {calls: 8, cost: 0, depth: 16};\nlet r = judge(state(mat(\"甲\")), test(\"未决吗\", \"k\"));\nlet a = cut(r);\nlet f = fn() { let b = cut(r); 1 };\nlet z = f();\n{a: a, z: z}\n";
    for src in [p3, p2] {
        let rep = jpp::check(&lower(&parse(src).expect("解析")).expect("lower"));
        assert!(
            rep.find("J-05").is_none() && rep.find("W-unsure-untracked").is_some(),
            "{}",
            rep.render()
        );
    }
    let r = 跑(p3, vec![], false);
    let o = r.outcome.unwrap_or_else(|e| panic!("{e}"));
    let ev = 事件(&r.ledger);
    assert!(
        ev.iter().any(|e| matches!(e, Entry::Handoff { .. })),
        "{ev:?}"
    );
    assert!(
        !ev.iter().any(|e| matches!(e, Entry::Drop { .. })),
        "{ev:?}"
    );
    assert!(o.unsure_default.is_empty());

    let r = 跑(p2, vec![], false);
    // G2（步 35）：原断言「按 B162 的帧返回规则在函数 f 返回处报运行期 J-05」。改后函数返回处不报错，这份欠账挂到
    // 程序顶层；程序结束时同键的 a 在返回值里（B162：同一责任的另一视图随返回值交出），记转交、不记违规
    let o = r
        .outcome
        .unwrap_or_else(|e| panic!("不再在函数返回处报错：{e}"));
    assert!(o.violations.is_empty(), "{:?}", o.violations);
    assert!(
        事件(&r.ledger)
            .iter()
            .any(|e| matches!(e, Entry::Handoff { .. }))
    );
    assert!(
        !事件(&r.ledger)
            .iter()
            .any(|e| matches!(e, Entry::Drop { .. }))
    );
}

/// `refine(u, unit, {need, round, got: false})`：只记 Enrich{got: false}，责任不动
#[test]
fn 复核_refine取不到只记补信息() {
    let src = r#"budget {calls: 4, cost: 0, depth: 16};
let e = cut(judge(state(mat("甲")), test("模糊题", "k")));
handle(e, {act: fn() { 1 }, ignore: fn() { 0 },
           unsure: fn(u) { refine(u, unit, {need: "证据", round: 1, got: false}); {exit: u} }})
"#;
    let r = 跑(src, vec![], false);
    r.outcome.expect("跑通");
    let ev = 事件(&r.ledger);
    assert!(
        matches!(ev.as_slice(), [Entry::Enrich { got: false, need, .. }, Entry::Handoff { .. }] if need == "证据"),
        "{ev:?}"
    );
}

/// 复查 2026-09-30 阻断项（p5、p6、p7）：读数经函数参数或 `let` 别名传过来时站点不标。G1 步 33 起检查器报
/// `W-unsure-untracked`、不拒；运行期作者随返回值交出的那份不被另一视图的默认链记成 Drop：p6、p7 在同一帧里记 Handoff，
/// p5 的 b 在函数 g 的帧里，与 p2 同样按 B162 的帧返回规则报 J-05（见上一条）
#[test]
fn 复查_p5_p6_p7_别名与参数传来的读数不记drop() {
    let j = r#"judge(state(mat("甲")), test("未决吗", "k"))"#;
    let 头 = "budget {calls: 8, cost: 0, depth: 16};\n";
    let p5 = format!(
        "{头}let r0 = {j};\nlet a = cut(r0);\nlet g = fn(r) {{ let b = cut(r); 1 }};\nlet z = g(r0);\n{{a: a, z: z}}\n"
    );
    let p6 =
        format!("{头}let r = {j};\nlet a = cut(r);\nlet r2 = r;\nlet b = cut(r2);\n{{a: a}}\n");
    let p7 = format!(
        "{头}let rs = [{j}];\nlet a = cut(rs[0]);\nlet r = rs[0];\nlet b = cut(r);\n{{a: a}}\n"
    );
    for (名, src) in [("p5", &p5), ("p6", &p6), ("p7", &p7)] {
        let rep = jpp::check(&lower(&parse(src).expect("解析")).expect("lower"));
        assert!(
            rep.find("J-05").is_none() && rep.find("W-unsure-untracked").is_some(),
            "{名}：{}",
            rep.render()
        );
    }
    for (名, src) in [("p6", &p6), ("p7", &p7)] {
        let r = 跑(src, vec![], false);
        let o = r.outcome.unwrap_or_else(|e| panic!("{名}：{e}"));
        let ev = 事件(&r.ledger);
        assert!(
            ev.iter().any(|e| matches!(e, Entry::Handoff { .. })),
            "{名}：{ev:?}"
        );
        assert!(
            !ev.iter().any(|e| matches!(e, Entry::Drop { .. })),
            "{名}：{ev:?}"
        );
        assert!(o.unsure_default.is_empty(), "{名}");
    }
    let r = 跑(&p5, vec![], false);
    // G2（步 35）：原断言「p5 在函数 g 返回处报运行期 J-05」，改为同 p2：挂到程序顶层，同键的 a 随返回值交出，记转交
    let o = r
        .outcome
        .unwrap_or_else(|e| panic!("p5：不再在函数返回处报错：{e}"));
    assert!(o.violations.is_empty(), "p5：{:?}", o.violations);
    assert!(
        事件(&r.ledger)
            .iter()
            .any(|e| matches!(e, Entry::Handoff { .. })),
        "p5"
    );
    assert!(
        !事件(&r.ledger)
            .iter()
            .any(|e| matches!(e, Entry::Drop { .. })),
        "p5"
    );
}

/// 二次复查 p1 与过程记录 5.18：同组一处有去向时过检查器、只记 Handoff；同组都没有去向时两处都走链，记一条 Drop，
/// 第二处被 W-duty-twice 挡住
#[test]
fn 同组cut_p1记handoff_都无去向记一条drop() {
    let 头 = "budget {calls: 8, cost: 0, depth: 16};\nlet r = judge(state(mat(\"甲\")), test(\"未决吗\", \"k\"));\n";
    let p1 = format!("{头}let a = cut(r);\nlet b = cut(r);\n{{a: a}}\n");
    let r = 跑(&p1, vec![], false);
    let o = r
        .outcome
        .unwrap_or_else(|e| panic!("p1 应当过检查器并跑通：{e}"));
    let ev = 事件(&r.ledger);
    assert!(matches!(ev.as_slice(), [Entry::Handoff { .. }]), "{ev:?}");
    assert!(
        !o.trace
            .warnings
            .iter()
            .any(|w| w.starts_with("W-drop-then-return") || w.starts_with("W-duty-twice")),
        "{:?}",
        o.trace.warnings
    );
    assert!(o.unsure_default.is_empty());

    // Z0398 A 步（过程记录 5.19）起没写 unsure_source 也照取通用表、发「为什么拿不准」：替身对它给全 0，诊断题自己
    // 拿不准，记账放弃它并停。原题的键只记一条 Drop，第二处被 W-duty-twice 挡住；两处各问一次「为什么」
    let 都无 = format!("{头}let a = cut(r);\nlet b = cut(r);\n1\n");
    let r = 跑(&都无, vec![], false);
    let o = r.outcome.unwrap_or_else(|e| panic!("{e}"));
    let 原键 = 判断键(&r.ledger)[0].clone();
    let ev = 事件(&r.ledger);
    assert!(ev.iter().all(|e| matches!(e, Entry::Drop { .. })), "{ev:?}");
    assert_eq!(
        ev.iter()
            .filter(|e| matches!(e, Entry::Drop { of, .. } if of == &vec![原键.clone()]))
            .count(),
        1,
        "{ev:?}"
    );
    assert_eq!(
        o.trace
            .warnings
            .iter()
            .filter(|w| w.starts_with("W-duty-twice"))
            .count(),
        1,
        "{:?}",
        o.trace.warnings
    );
    // 过程记录 5.22：报文注明是默认链两次走链产生的
    assert!(
        o.trace
            .warnings
            .iter()
            .any(|w| w.starts_with("W-duty-twice")
                && w.contains("同一判断的另一视图已由默认链交代过")),
        "{:?}",
        o.trace.warnings
    );
    assert_eq!(
        判断键(&r.ledger).len(),
        3,
        "原题一道，两处各问一次「为什么」"
    );
}
