//! C2b（步 41）：解释器经单元图求值的判断单元、账本单元与程序单元；「关掉单元图」开关（H1a 对照臂）。
//! 预注册：`地基/过程记录/工程-C2-单元图求值.md` 附录一 A1.4（C2b-4、C2b-5、C2b-6）与 A1.5（C2b-12）。

use std::cell::Cell;

use jpp::effects::{CalibStore, EffectError, FnPort, GenResult, JudgeResult, Ports};
use jpp::ledger::{Entry, Ledger};
use jpp::store::CacheIndex;
use jpp::value::Answer;
use jpp::{ActionRegistry, EntryArgs, Outcome, Session, lower, syntax::parse};
use serde_json::{Value as Json, json};

/// 同运行复用（同一判断换调用位置）、生成复用（同一生成换调用位置）、一道缺席的题。
const 程序: &str = r#"
budget {calls: 8, cost: 0, escalate: 2, absent: {retry: 1, backoff: 0, then: "conservative"}};
let g1 = gen("写三个候选名字", [mat("需求")], 3, 0);
let e1 = cut(judge(state(mat("候选甲")), test("合适吗", "k")));
let a1 = handle(e1, {act: fn() { "act" }, ignore: fn() { "ignore" }, unsure: fn(u) { consume(u, "drop"); "unsure" }});
let g2 = gen("写三个候选名字", [mat("需求")], 3, 0);
let e2 = cut(judge(state(mat("候选甲")), test("合适吗", "k")));
let a2 = handle(e2, {act: fn() { "act" }, ignore: fn() { "ignore" }, unsure: fn(u) { consume(u, "drop"); "unsure" }});
let e3 = cut(judge(state(mat("候选乙")), test("好记吗", "k")));
let a3 = handle(e3, {act: fn() { "act" }, ignore: fn() { "ignore" }, unsure: fn(u) { escalate(u, state(mat("候选乙")), test("好记吗", "k")) }});
{a1: a1, a2: a2, a3: a3, n1: if is_fail(g1) { 0 } else { len(g1) }, n2: if is_fail(g2) { 0 } else { len(g2) }}
"#;

fn 编译(src: &str) -> jpp::Program {
    lower(&parse(src).expect("解析")).expect("lower")
}

/// 判断恒 0.9，题面含「好记」的缺席；生成给固定三项；问人回答「好记」。
fn 端口(判: &Cell<u32>) -> Ports<'_> {
    Ports::new()
        .with(FnPort::judge("fixed-0", move |_s, qs| {
            判.set(判.get() + 1);
            if qs.iter().any(|q| q.text.contains("好记")) {
                return Err(EffectError("缺席".into()));
            }
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
            Ok(GenResult {
                outputs: vec![json!("甲"), json!("乙"), json!("丙")],
                tokens: 0,
                cost: 0.0,
                failure: None,
                taint_out: None,
            })
        }))
        .with(FnPort::ask("fixed-0", |_s, _q| Ok(Some(Answer::Noul(0.9)))))
}

struct 一趟 {
    out: Outcome,
    ledger: Ledger,
    calls: u32,
}

fn 跑(src: &str, cells: bool, ledger: Ledger, cache: Option<&CacheIndex>, replay: bool) -> 一趟 {
    let calib = CalibStore::new();
    let acts = ActionRegistry::new();
    let 判 = Cell::new(0);
    let mut l = ledger;
    let mut s = Session::new(端口(&判), &calib, &acts)
        .with_companions(jpp::interp::CompanionMode::Off)
        .with_cells(cells);
    if let Some(c) = cache {
        s = s.with_cache(c);
    }
    let out = if replay {
        s.replay(&编译(src), &EntryArgs::default(), &mut l)
    } else {
        s.run(&编译(src), &EntryArgs::default(), &mut l)
    }
    .unwrap_or_else(|e| panic!("{}", e.render()));
    一趟 {
        out,
        ledger: l,
        calls: 判.get(),
    }
}

/// 外部可见的全部：值、追踪事件与告警、费用、复用计数、层、出口行、违规、账本逐条。
fn 外部(t: &一趟) -> Json {
    let o = &t.out;
    json!({
        "value": o.value_json(),
        "events": format!("{:?}", o.trace.events),
        "warnings": o.trace.warnings,
        "cost": format!("{:?}", o.cost),
        "cache": format!("{:?}", o.cache),
        "layers": format!("{:?}", o.layers),
        "exits": o.exits,
        "questions": o.questions,
        "violations": format!("{:?}", o.violations),
        "returned_unsure": o.returned_unsure,
        "ledger": serde_json::to_value(&t.ledger.entries).unwrap(),
        "calls": t.calls,
    })
}

/// C2b-4：开、关单元图，首跑、跨运行缓存、审计重放三种运行的外部行为逐字节相同。
#[test]
fn c2b_4_开关两臂外部行为相同() {
    let 开 = 跑(程序, true, Ledger::new(), None, false);
    let 关 = 跑(程序, false, Ledger::new(), None, false);
    assert_eq!(外部(&开), 外部(&关));
    // 同运行复用确实发生了（判断一条、生成一条复用条目），不是两边都没复用
    let 复用: Vec<&Entry> = 开
        .ledger
        .entries
        .iter()
        .filter(|e| {
            matches!(
                e,
                Entry::Judge {
                    reused_from: Some(_),
                    ..
                } | Entry::Effect {
                    reused_from: Some(_),
                    ..
                }
            )
        })
        .collect();
    assert!(!复用.is_empty(), "同运行复用没有发生，对照不成立");

    // 跨运行缓存（持久层）
    let ix = CacheIndex::build(&[("a.jsonl".to_string(), 开.ledger.clone())]);
    let 开2 = 跑(程序, true, Ledger::new(), Some(&ix), false);
    let 关2 = 跑(程序, false, Ledger::new(), Some(&ix), false);
    assert_eq!(外部(&开2), 外部(&关2));
    assert!(
        开2.out.cache.as_ref().is_some_and(|c| c.cross_run > 0),
        "跨运行缓存命中，对照成立"
    );

    // 审计重放
    let 开3 = 跑(程序, true, 开.ledger.clone(), None, true);
    let 关3 = 跑(程序, false, 开.ledger.clone(), None, true);
    assert_eq!(外部(&开3), 外部(&关3));
}

/// C2b-5：判断单元数 = 本趟已答判断的不同缓存键数；缺席的不进判断单元。账本单元数 = 不同的生成缓存键数。
#[test]
fn c2b_5_判断单元只收答案() {
    let t = 跑(程序, true, Ledger::new(), None, false);
    let c = t.out.cells.clone().expect("单元图开着");
    // 「合适吗」答了（两处同键 → 1 个单元）；「好记吗」缺席，不进
    assert_eq!(c.判断单元, 1, "{c:?}");
    assert_eq!(c.账本单元, 1, "{c:?}");
    let 关 = 跑(程序, false, Ledger::new(), None, false);
    assert!(关.out.cells.is_none(), "关着不建单元图");
}

/// C2b-6 与 C2b-12：一趟运行在单元图里开一个新宿主纪元、留一次尝试、发布一版；状态按 R2。
#[test]
fn c2b_6_12_程序单元一次尝试_状态按r2() {
    let 跑一 = |体: &str| {
        let src = format!(
            "budget {{calls: 4, cost: 0, absent: {{retry: 0, backoff: 0, then: \"conservative\"}}}};\n{体}"
        );
        跑(&src, true, Ledger::new(), None, false)
            .out
            .cells
            .expect("单元图开着")
    };
    // 无未决 → 已定
    let c = 跑一(
        "let e = cut(judge(state(mat(\"候选甲\")), test(\"合适吗\", \"k\")));\nhandle(e, {act: fn() { 1 }, ignore: fn() { 0 }, unsure: fn(u) { consume(u, \"drop\"); 2 }})\n",
    );
    assert_eq!((c.发布版本, c.最新状态), (1, Some("已定")), "{c:?}");
    assert_eq!(
        (c.宿主纪元, c.尝试宿主纪元),
        (1, Some(1)),
        "每趟开一个新宿主纪元（C2a 复核第 1 条）"
    );
    // 返回值带着未决（随返回值转交）→ 未决
    let c =
        跑一("let e = cut(judge(state(mat(\"候选乙\")), test(\"好记吗\", \"k\")));\n{e: e}\n");
    assert_eq!((c.发布版本, c.最新状态), (1, Some("未决")), "{c:?}");
    // G2 违规（未决丢了）→ 单次形态返回「未决（violation）」→ 未决
    let c = 跑一(
        "let e = cut(judge(state(mat(\"候选乙\")), test(\"好记吗\", \"k\")));\nlet x = handle(e, {act: fn() { 1 }, ignore: fn() { 0 }});\n1\n",
    );
    assert_eq!((c.发布版本, c.最新状态), (1, Some("未决")), "{c:?}");
}

/// C2b-6：挂起（问人没回答）的这一次尝试没有结论，发布「进行中」；单元图关着时统计为空。
#[test]
fn c2b_6_挂起发布进行中() {
    let src = "budget {calls: 4, cost: 0, escalate: 1};\nhandle(ask(state(mat(\"问\")), test(\"行吗\", \"k\")), {act: fn() { 1 }, ignore: fn() { 0 }, unsure: fn(u) { consume(u, \"drop\"); -1 }})\n";
    let calib = CalibStore::new();
    let acts = ActionRegistry::new();
    for cells in [true, false] {
        let ports = Ports::new().with(FnPort::ask("fixed-0", |_s, _q| Ok(None)));
        let o = Session::new(ports, &calib, &acts)
            .with_companions(jpp::interp::CompanionMode::Off)
            .with_cells(cells)
            .run(&编译(src), &EntryArgs::default(), &mut Ledger::new())
            .unwrap_or_else(|e| panic!("{}", e.render()));
        assert!(o.value.is_none(), "挂起");
        match o.cells {
            Some(c) => {
                assert!(cells);
                assert_eq!(
                    (c.发布版本, c.最新状态, c.尝试宿主纪元),
                    (1, Some("进行中"), Some(1)),
                    "{c:?}"
                );
            }
            None => assert!(!cells),
        }
    }
}
