//! Z0398 A 步（过程记录 5.19；裁定五十一、五十二 (b)、五十五）：`lacks` 在题式与题值上，默认链的候选四级
//! （作者 `unsure_source.need` > 题 `lacks` > 通用表），没写 `unsure_source` 不放弃，没有取法走路 C
//! （问出缺哪类、类别进报告 `needed`、记 Handoff，不记 Drop 与 Enrich）。固定关伴随题：测的是默认链自己的选路。

mod derive_support;
use jpp::effects::{CalibStore, FnPort, JudgeResult, Ports};
use jpp::interp::CompanionMode;
use jpp::ledger::{Entry, Ledger};
use jpp::value::Answer;
use jpp::{ActionRegistry, EntryArgs, Outcome, Session, lower, syntax::parse};
use serde_json::{Value as Json, json};
use std::cell::RefCell;

/// 「为什么拿不准」选第 `选` 项；别的题一律 0.5（无线并列，未决）
fn 端口<'a>(calls: &'a RefCell<u64>, 选: usize) -> Ports<'a> {
    Ports::new().with(FnPort::judge("fixed-0", move |s, qs| {
        *calls.borrow_mut() += 1;
        Ok(JudgeResult {
            answers: qs
                .iter()
                .map(|q| {
                    if q.text.contains("为什么拿不准") {
                        let mut v = vec![0.02; s.over.len()];
                        v[选] = 0.9;
                        Answer::Choice(v)
                    } else {
                        Answer::Noul(0.5)
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

fn 跑(src: &str, 选: usize) -> 跑出 {
    let program = lower(&parse(src).expect("解析")).expect("lower");
    let calls = RefCell::new(0);
    let mut ledger = Ledger::new();
    let (calib, a) = (CalibStore::new(), ActionRegistry::new());
    let outcome = Session::new(端口(&calls, 选), &calib, &a)
        .with_companions(CompanionMode::Off)
        .run(&program, &EntryArgs::default(), &mut ledger)
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

/// 一道没写 unsure 臂的题；`前` 放在它前面（题式、unsure_source 等），`题` 是题值表达式
fn 程序(前: &str, 题: &str) -> String {
    format!(
        "budget {{calls: 8, cost: 0, depth: 16}};\n{前}\nlet e = cut(judge(state(mat(\"甲方与乙方的合作意向\")), {题}));\nhandle(e, {{act: fn() {{ 1 }}, ignore: fn() {{ 0 }}}})\n"
    )
}

/// 路 C 的共同断言：一行默认链记录，`end: handoff`、`needed` 是选中的类别、`source` 是 `来源`；账本只有 Handoff
fn 路c(r: &跑出, 来源: &str, needed: &str) -> Json {
    let o = r.outcome.as_ref().unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(o.unsure_default.len(), 1, "{:?}", o.unsure_default);
    let row = o.unsure_default[0].clone();
    assert_eq!(row["end"], "handoff", "{row}");
    assert_eq!(row["source"], 来源, "{row}");
    assert_eq!(row["needed"], json!([needed]), "{row}");
    let ev = 事件(&r.ledger);
    assert!(
        matches!(ev.as_slice(), [Entry::Handoff { .. }]),
        "路 C 只记 Handoff，不记 Drop 与 Enrich：{ev:?}"
    );
    assert_eq!(r.calls, 2, "原题一次、「为什么拿不准」一次");
    assert!(o.pending.is_empty(), "路 C 不进报告 pending");
    row
}

#[test]
fn 通用表_没写unsure_source也不放弃_路c交出() {
    let r = 跑(&程序("", "test(\"这两方适合合作吗？\", \"k\")"), 1);
    let row = 路c(&r, "generic", "语境");
    assert_eq!(row["asked"], json!(["材料", "语境", "参照"]));
}

#[test]
fn 程序自己的unsure_lacks盖过通用表() {
    let r = 跑(
        &程序(
            "let unsure_lacks = [\"预算\", \"工期\"];",
            "test(\"这两方适合合作吗？\", \"k\")",
        ),
        0,
    );
    路c(&r, "generic", "预算");
}

#[test]
fn 手写题式的lacks() {
    let 前 = "let f = form(\"test\", \"{a}与{b}适合合作吗？\", {calib: \"k\", lacks: [\"过往合作\", \"预算\", \"过往合作\"]});";
    let r = 跑(&程序(前, "fill(f, {a: \"甲方\", b: \"乙方\"})"), 1);
    let row = 路c(&r, "lacks", "预算");
    assert_eq!(row["asked"], json!(["过往合作", "预算"]), "lacks 去重");
}

#[test]
fn 作者need盖过题式lacks_只给need没有fetch走路c() {
    let 前 = "let f = form(\"test\", \"{a}适合合作吗？\", {calib: \"k\", lacks: [\"过往合作\"]});\nunsure_source({need: [\"双方目标\", \"资金\"]});";
    let r = 跑(&程序(前, "fill(f, {a: \"甲方\"})"), 1);
    路c(&r, "unsure_source", "资金");
}

#[test]
fn 只有一类候选不问为什么() {
    let 前 = "let f = form(\"test\", \"{a}适合合作吗？\", {calib: \"k\", lacks: [\"过往合作\"]});";
    let r = 跑(&程序(前, "fill(f, {a: \"甲方\"})"), 0);
    let o = r.outcome.as_ref().unwrap();
    assert_eq!(o.unsure_default[0]["needed"], json!(["过往合作"]));
    assert_eq!(o.unsure_default[0]["end"], "handoff");
    assert_eq!(r.calls, 1, "只有一类，代码能定，不发「为什么」");
}

#[test]
fn lacks可读_不进哈希() {
    let src = "budget {calls: 0, cost: 0, depth: 16};
let f = form(\"test\", \"{a}适合吗？\", {calib: \"k\", lacks: [\"材料\"]});
let g = form(\"test\", \"{a}适合吗？\", {calib: \"k\"});
let q = fill(f, {a: \"甲\"});
{f: f.lacks, q: q.lacks, g: g.lacks, 同式: f.hash == g.hash, 同题: q.hash == fill(g, {a: \"甲\"}).hash, t: test(\"x\", \"k\").lacks}";
    let r = 跑(src, 0);
    let v = r.outcome.unwrap().value_json();
    assert_eq!(
        v,
        json!({"f": ["材料"], "q": ["材料"], "g": [], "同式": true, "同题": true, "t": []})
    );
}

#[test]
fn lacks写在test上或不是文本列表_报错() {
    for (src, 词) in [
        (
            "budget {calls: 0, cost: 0, depth: 16};\ntest(\"x\", \"k\", {lacks: [\"材料\"]})",
            "写在题式上",
        ),
        (
            "budget {calls: 0, cost: 0, depth: 16};\nform(\"test\", \"x\", {calib: \"k\", lacks: \"材料\"})",
            "文本列表",
        ),
        (
            "budget {calls: 0, cost: 0, depth: 16};\nform(\"test\", \"x\", {calib: \"k\", lacks: [1]})",
            "文本列表",
        ),
    ] {
        let e = 跑(src, 0).outcome.expect_err("应报错");
        assert!(e.contains("E-rt-question") && e.contains(词), "{e}");
    }
}

#[test]
fn unsure_source至少给一项() {
    let e = 跑(&程序("unsure_source({});", "test(\"x\", \"k\")"), 0)
        .outcome
        .expect_err("空的 unsure_source 报错");
    assert!(e.contains("E-rt-arg") && e.contains("至少给一项"), "{e}");
}

/// 守则填的 lacks：前提反面派生题带 `derive_rules().lacks.premise`
#[test]
fn 守则填的lacks随派生题走() {
    let src = r#"budget {calls: 8, cost: 0, depth: 64};
let q0 = test("甲适合合作吗？", "k", {presupposition: "甲在找合作方"});
let n = derive_node("n0", {on: mat("甲的情况")}, q0, {});
let e = cut(judge(n.st, q0));
let kids = derive_premise(n, e, {});
{lacks: kids[0].q.lacks, rules: derive_rules().lacks.premise, version: derive_rules().version, e: e}"#;
    let r = derive_support::跑_关(src, |_t, _q, _s| Answer::Noul(0.9), vec![])
        .unwrap_or_else(|e| panic!("{e}"));
    let v = r.out.value_json();
    assert_eq!(v["lacks"], json!(["材料", "语境"]));
    assert_eq!(v["lacks"], v["rules"]);
    // 守则版本：Z0398 升到 derive-4，线 A 之后 Z0511、Z0511 返修、Z0535、Z0559 依次升到 derive-8，W-shadow 改名升到 derive-9，B0470 前提派生升到 derive-10，臂 3 空跑 3.1 的请求缺省值修补升到 derive-11，3.2 的前提题只看条目升到 derive-12，B0478 作者入口升到 derive-13，Z0860 前提三层升到 derive-14；第二靶子迭代：前提≠证据升到 derive-15，if_false 自报升到 derive-16，
    // if_false 结构化并进账本升到 derive-17，协议值改名 unanswerable 升到 derive-18，裁定七十五过程入口不派生前提升到 derive-19，Z0913 过程动作题的 lacks 加中间判断升到 derive-20，第三靶子第四圈完成条件进过程入口升到 derive-21，第五圈计分途径带可达条件、子题 S″ 升到 derive-22
    assert_eq!(v["version"], "derive-22");
}
