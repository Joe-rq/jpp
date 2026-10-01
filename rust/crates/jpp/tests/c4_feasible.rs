//! C-4：`cut` 的 `feasible: fn(k) -> Bool` 选项——判断之后、按作者的代码谓词，在已决 `pick` 上取最高概率的可行候选，
//! 被跳过的候选记账（路 A：报告 `exits` 行的 `feasible` 字段与一条 `N-feasible-skip` trace；账本事件分类型 C-1 落地后再写账本）。
//!
//! 预注册见 `地基/过程记录/工程-C4-可行候选.md` 第二节（P1–P13）；设计出处 `规划/骨架候选.md` 2.0.2 C-4、
//! `附注/2026-09-27-world港城线-回报.md` 第二节第 2 条、意图汇编 7a。
//! 无可行候选出 `Unsure("infeasible")`（不当 ignore、不悄悄丢弃；J-05 与 B187）。

mod common;

use common::{run, run_replay};
use jpp::effects::{CalibStore, FnPort, JudgeResult, Ports};
use jpp::interp::ActionRegistry;
use jpp::ledger::Ledger;
use jpp::value::{Answer, State};
use jpp::{lower, syntax::parse};
use serde_json::{Value as Json, json};

/// 判断器：每道题回同一个答案；置换测过且一致（所以 select 出 Pick）
fn 端口<'a>(a: Answer) -> Ports<'a> {
    Ports::new().with(common::伴随中性judge("m", move |_s: &State, qs| {
        Ok(JudgeResult {
            answers: qs.iter().map(|_| a.clone()).collect(),
            tokens: 0,
            cost: 0.0,
            mode_share: qs.iter().map(|_| Some(1.0)).collect(),
            perms: qs.iter().map(|_| 2).collect(),
            confidence: vec![],
        })
    }))
}

struct 跑出 {
    kind: String,
    exits: Vec<Json>,
    warnings: Vec<String>,
    ledger: Ledger,
    calls: u64,
}

fn 跑(src: &str, a: Answer) -> Result<跑出, String> {
    let program = lower(&parse(src).expect("解析")).expect("lower");
    let mut l = Ledger::new();
    let o = run(
        &program,
        端口(a),
        &CalibStore::new(),
        &ActionRegistry::new(),
        &mut l,
    )
    .map_err(|e| e.render())?;
    Ok(跑出 {
        kind: o.value_json().as_str().unwrap_or_default().to_string(),
        exits: o.exits.clone(),
        warnings: o.trace.warnings.clone(),
        ledger: l,
        calls: o.cost.calls,
    })
}

/// 一次 select 的 cut，出口种类作返回值。`选项` 是 cut 的第二位起（含前导逗号），`前言` 是先于 cut 的 let
fn 程序(前言: &str, 选项: &str) -> String {
    format!(
        r#"
budget {{calls: 4, cost: 0, depth: 16}};
let can = [false, true, true];
{前言}
let e = cut(judge(state(mat("甲"), {{over: [mat("浇筑"), mat("砌墙"), mat("装窗")]}}), select("下一道工序", "k")){选项});
let k = exit_kind(e);
consume(e, "drop");
k
"#
    )
}

const 谓词_只有后两个可行: &str = r#", {feasible: fn(k) { can[k] }}"#;
const 读数_甲: [f64; 3] = [0.6, 0.3, 0.1];

fn 跳过的k(行: &Json) -> Vec<i64> {
    行["feasible"]["skipped"]
        .as_array()
        .unwrap()
        .iter()
        .map(|x| x["k"].as_i64().unwrap())
        .collect()
}

/// P1：k=0 不可行 → 改选 pick(1)；判断调用数不变；行带 feasible、等级 Answer；trace 恰一条
#[test]
fn p1_首选不可行改选次选() {
    let o = 跑(
        &程序("", 谓词_只有后两个可行),
        Answer::Choice(读数_甲.to_vec()),
    )
    .unwrap();
    assert_eq!(o.kind, "pick(1)");
    assert_eq!(o.calls, 1, "谓词是代码，不发判断");
    let 行 = &o.exits[0];
    assert_eq!(行["grade"], "Answer");
    assert_eq!(
        行["feasible"],
        json!({"from": 0, "chosen": 1, "skipped": [{"k": 0, "p": 0.6}]}),
        "没有线时原来就是 Answer，不写降级"
    );
    assert_eq!(
        o.warnings
            .iter()
            .filter(|w| w.contains("N-feasible-skip"))
            .count(),
        1,
        "{:?}",
        o.warnings
    );
    // 判断器的读数一个字没改：账本与不带 feasible 的同程序相同（P11）
    let 基线 = 跑(&程序("", ""), Answer::Choice(读数_甲.to_vec())).unwrap();
    assert_eq!(基线.kind, "pick(0)");
    // C-1 落地后改选写进账本（`Reselect`）：去掉它，判断条目与基线逐个相同
    let 非改选: Vec<_> = o
        .ledger
        .entries
        .iter()
        .filter(|e| !matches!(e, jpp::ledger::Entry::Reselect { .. }))
        .cloned()
        .collect();
    assert_eq!(o.ledger.header, 基线.ledger.header, "头行与基线相同");
    assert_eq!(非改选, 基线.ledger.entries, "读数不改，账本只多改选事件");
    assert_eq!(
        o.ledger.entries.len(),
        基线.ledger.entries.len() + 1,
        "一次改选一条 Reselect"
    );
}

/// P2：原 pick 可行 → 与不带 feasible 逐字节相同，没有 feasible 字段，没有 trace
#[test]
fn p2_首选可行时什么都不变() {
    let 前言 = "let can = [true, false, false];";
    let o = 跑(
        &程序(前言, 谓词_只有后两个可行),
        Answer::Choice(读数_甲.to_vec()),
    )
    .unwrap();
    let 基线 = 跑(&程序(前言, ""), Answer::Choice(读数_甲.to_vec())).unwrap();
    assert_eq!(o.kind, "pick(0)");
    assert_eq!(o.exits, 基线.exits);
    assert!(o.exits[0].get("feasible").is_none());
    assert!(o.warnings.iter().all(|w| !w.contains("N-feasible")));
    // C-2：两份程序不同，默认根上下文（行外壳 `trace`，由程序标识推导）不同；头行与条目本体逐字节相同
    assert_eq!(o.ledger.header, 基线.ledger.header);
    assert_eq!(o.ledger.entries, 基线.ledger.entries);
}

/// P3：三个都不可行 → Unsure(infeasible)，三项跳过按 k=0,1,2 的顺序
#[test]
fn p3_全不可行出infeasible() {
    let o = 跑(
        &程序("let can = [false, false, false];", 谓词_只有后两个可行),
        Answer::Choice(读数_甲.to_vec()),
    )
    .unwrap();
    assert_eq!(o.kind, "unsure(infeasible)");
    let f = &o.exits[0]["feasible"];
    assert_eq!(f["chosen"], Json::Null);
    assert_eq!(f["from"], 0);
    assert_eq!(
        f["skipped"],
        json!([{"k": 0, "p": 0.6}, {"k": 1, "p": 0.3}, {"k": 2, "p": 0.1}])
    );
    // J-05：这是未决，必须被消费；上面 consume(e, "drop") 显式丢弃才不报
    // （没写去向的程序在 J-05 下照报，见 p3b）
}

/// P3b：全不可行且没人消费 → 不悄悄当成 ignore。B0492 S2c 起：这个 cut 在作用域里没有作者去向，运行时在站点
/// 当场走默认链；infeasible 不补信息（Z0207 第 9 条），转交程序结果（账本 Handoff），不再是 J-05。`--guard` 下照旧 J-05。
#[test]
fn p3b_infeasible是未决不消费即报错() {
    let src = r#"
budget {calls: 4, cost: 0, depth: 16};
let e = cut(judge(state(mat("甲"), {over: [mat("a"), mat("b")]}), select("哪个", "k")), {feasible: fn(k) { false }});
1
"#;
    let o = 跑(src, Answer::Choice(vec![0.7, 0.3])).expect("默认链转交");
    assert!(
        o.ledger.entries.iter().any(
            |e| matches!(e, jpp::ledger::Entry::Handoff { cause, .. } if cause == "infeasible")
        ),
        "{:?}",
        o.ledger.entries
    );
    assert!(
        o.warnings.iter().any(|w| w.starts_with("W-unsure-default")),
        "{:?}",
        o.warnings
    );
    let mut program = lower(&parse(src).expect("解析")).expect("lower");
    program.entry.guard = true;
    let mut l = Ledger::new();
    // G2（步 35）：原断言「--guard 下运行期 J-05」，改为程序结束记一笔违规（原因 infeasible），值照带
    let o = run(
        &program,
        端口(Answer::Choice(vec![0.7, 0.3])),
        &CalibStore::new(),
        &ActionRegistry::new(),
        &mut l,
    )
    .unwrap_or_else(|e| panic!("违规不再是运行期错误：{}", e.render()));
    assert_eq!(o.violations.len(), 1);
    assert_eq!(o.violations[0].mark.cause, "infeasible");
    assert_eq!(o.value_json(), serde_json::json!(1));
}

/// P4：argmax 不在首位（[0.1, 0.7, 0.2]），全不可行：检验顺序 k=1,2,0，钉住 k 是 over 原序
#[test]
fn p4_检验顺序按概率降序且k是原序() {
    let o = 跑(
        &程序("let can = [false, false, false];", 谓词_只有后两个可行),
        Answer::Choice(vec![0.1, 0.7, 0.2]),
    )
    .unwrap();
    assert_eq!(o.kind, "unsure(infeasible)");
    assert_eq!(o.exits[0]["feasible"]["from"], 1);
    assert_eq!(跳过的k(&o.exits[0]), vec![1, 2, 0]);
}

/// P5：同读数，只有 k=2 可行 → pick(2)，只跳过 k=1
#[test]
fn p5_取最高概率的可行候选() {
    let o = 跑(
        &程序("let can = [false, false, true];", 谓词_只有后两个可行),
        Answer::Choice(vec![0.1, 0.7, 0.2]),
    )
    .unwrap();
    assert_eq!(o.kind, "pick(2)");
    assert_eq!(
        o.exits[0]["feasible"]["skipped"],
        json!([{"k": 1, "p": 0.7}])
    );
}

/// P6：unsure(band) 与 P8：缺席——谓词零调用（谓词返回整数会报 E-rt-type，没报就是没调）
#[test]
fn p6_未决出口不调谓词() {
    let 炸: &str = r#", {declare: {hi: 0.9}, feasible: fn(k) { 1 }}"#;
    let o = 跑(&程序("", 炸), Answer::Choice(读数_甲.to_vec())).unwrap();
    assert_eq!(o.kind, "unsure(band)");
    assert!(o.exits[0].get("feasible").is_none());
}

/// P7：作者声明线担保原 pick；改选后降为 Answer 并写明；原 pick 可行时仍是 Declared
#[test]
fn p7_改选后线的担保不再成立() {
    let 声明: &str = r#", {declare: {hi: 0.5}, feasible: fn(k) { can[k] }}"#;
    let o = 跑(&程序("", 声明), Answer::Choice(读数_甲.to_vec())).unwrap();
    assert_eq!(o.kind, "pick(1)");
    let 行 = &o.exits[0];
    assert_eq!(行["grade"], "Answer");
    assert_eq!(行["releases"], false);
    assert_eq!(行["feasible"]["downgraded"], "因改选降级");
    let o = 跑(
        &程序("let can = [true, true, true];", 声明),
        Answer::Choice(读数_甲.to_vec()),
    )
    .unwrap();
    assert_eq!(o.kind, "pick(0)");
    assert_eq!(o.exits[0]["grade"], "Declared");
    assert!(o.exits[0].get("feasible").is_none());
}

/// P7b：改选后出口不留线来源（线担保的是原 pick）；原 pick 可行时线来源照旧
#[test]
fn p7b_改选后不留线来源() {
    let 读来源 = |前言: &str| {
        let src = format!(
            r#"
budget {{calls: 4, cost: 0, depth: 16}};
{前言}
let e = cut(judge(state(mat("甲"), {{over: [mat("浇筑"), mat("砌墙"), mat("装窗")]}}), select("下一道工序", "k")), {{declare: {{hi: 0.5}}, feasible: fn(k) {{ can[k] }}}});
let s = line_source(e);
consume(e, "drop");
s
"#
        );
        跑(&src, Answer::Choice(读数_甲.to_vec())).unwrap().kind
    };
    assert_eq!(读来源("let can = [false, true, true];"), "");
    assert!(读来源("let can = [true, true, true];").contains("作者声明"));
}

/// P6b：并列（tie）出口同样不调谓词——谓词返回整数，调了就会 E-rt-type
#[test]
fn p6b_tie不调谓词() {
    let 炸: &str = r#", {feasible: fn(k) { 1 }}"#;
    let o = 跑(&程序("", 炸), Answer::Choice(vec![0.5, 0.5, 0.0])).unwrap();
    assert_eq!(o.kind, "unsure(tie)");
    assert!(o.exits[0].get("feasible").is_none());
}

/// P9：错误路径
#[test]
fn p9_选项与谓词的错误() {
    let 读数 = || Answer::Choice(读数_甲.to_vec());
    // 谓词返回非 Bool
    let e = 跑(&程序("", r#", {feasible: fn(k) { 1 }}"#), 读数())
        .err()
        .unwrap();
    assert!(e.contains("E-rt-type"), "{e}");
    // 不是函数、参数个数不对
    let e = 跑(&程序("", r#", {feasible: 3}"#), 读数()).err().unwrap();
    assert!(e.contains("E-cut-options") && e.contains("feasible"), "{e}");
    let e = 跑(&程序("", r#", {feasible: fn() { true }}"#), 读数())
        .err()
        .unwrap();
    assert!(e.contains("E-cut-options"), "{e}");
    // 配 test 读数
    let src = r#"
budget {calls: 4, cost: 0, depth: 16};
let e = cut(judge(state(mat("甲")), test("行吗", "k")), {feasible: fn(k) { true }});
consume(e, "drop");
1
"#;
    let e = 跑(src, Answer::Noul(0.9)).err().unwrap();
    assert!(
        e.contains("E-cut-options") && e.contains("没有 pick 可改"),
        "{e}"
    );
    // 配 stat: mass、stat: expect（expect 只配 measure，另有自己的报错，两者都报 E-cut-options）
    let e = 跑(
        &程序(
            "",
            r#", {stat: {mass: [0, 1]}, declare: {hi: 0.5}, feasible: fn(k) { true }}"#,
        ),
        读数(),
    )
    .err()
    .unwrap();
    assert!(e.contains("E-cut-options"), "{e}");
    // 谓词里调效应
    let 前言 = r#"fn 坏(k) !{judge} { let r = judge(state(mat("x")), test("q", "k2")); true }"#;
    let e = 跑(&程序(前言, ", {feasible: 坏}"), 读数()).err().unwrap();
    assert!(e.contains("E-fit-declare-effect"), "{e}");
    // sieve 不支持
    let src = r#"
budget {calls: 4, cost: 0, depth: 16};
let r = sieve(["a", "b"], test("行吗", "k"), {line: {feasible: fn(k) { true }}});
r
"#;
    let e = 跑(src, Answer::Noul(0.9)).err().unwrap();
    assert!(
        e.contains("E-cut-options") && e.contains("sieve 只收是非题"),
        "{e}"
    );
}

/// P10：重放。首跑落盘、解码、只凭账本重放：判断 0 次调用，出口与 feasible 字段相同
#[test]
fn p10_重放相同() {
    let src = 程序("", 谓词_只有后两个可行);
    let program = lower(&parse(&src).expect("解析")).expect("lower");
    let mut l = Ledger::new();
    let o1 = run(
        &program,
        端口(Answer::Choice(读数_甲.to_vec())),
        &CalibStore::new(),
        &ActionRegistry::new(),
        &mut l,
    )
    .map_err(|e| e.render())
    .unwrap();
    let (mut l2, trunc) = Ledger::decode(&l.encode()).expect("解码");
    assert!(trunc.is_none());
    l2.rebuild_index();
    let o2 = run_replay(
        &program,
        端口(Answer::Choice(vec![0.0, 0.0, 1.0])), // 重放不该发；给个不同的答案，发了就会露馅
        &CalibStore::new(),
        &ActionRegistry::new(),
        &mut l2,
    )
    .map_err(|e| e.render())
    .unwrap();
    assert_eq!(o1.value_json(), o2.value_json());
    assert_eq!(o1.exits, o2.exits);
    assert_eq!(o2.exits[0]["feasible"]["chosen"], 1);
    assert_eq!(o2.cost.calls, 0);
}

/// P12（改）：示例 `examples/feasible-pick.jpp` 在夹具下经 CLI 跑：判断器读数 [0.5, 0.35, 0.15]，首选（浇筑地基）
/// 已经做过、代码谓词否决，改选「砌一层墙」；判断 1 次；报告带改选记录。
/// 已进 `tests/golden/manifest.json`（用例 `feasible-pick`，小缺陷线补录）。
#[test]
fn p12_示例经cli跑() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let 源 = root.join("examples/feasible-pick.jpp");
    let 夹具 = root.join("examples/fixtures/feasible-pick.json");
    let out = std::process::Command::new(env!("CARGO_BIN_EXE_jpp"))
        .args([
            "run",
            源.to_str().unwrap(),
            "--fixtures",
            夹具.to_str().unwrap(),
        ])
        .output()
        .expect("跑 jpp");
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let v: Json = serde_json::from_slice(&out.stdout).expect("报告 JSON");
    assert_eq!(v["value"], json!({"下一步": "砌一层墙", "序号": 1}));
    assert_eq!(v["cost"]["calls"], 1);
    assert_eq!(
        v["exits"][0]["feasible"],
        json!({"from": 0, "chosen": 1, "skipped": [{"k": 0, "p": 0.5}]})
    );
    assert_eq!(v["exits"][0]["grade"], "Answer");
}
