//! Z0589（过程记录 5.28）：默认链给 `insufficient` 出口取补充信息，取到的材料补进它缺的那个证据槽（J-09，如 `ref`），
//! 不补进 `ctx`。其余照现行：没有取法走路 C；取到了仍拿不准，末端 Drop（裁定六）。固定关伴随题。

mod common;
use jpp::effects::{CalibStore, FnPort, JudgeResult, Ports};
use jpp::ledger::{Entry, Ledger};
use jpp::value::Answer;
use jpp::{ActionRegistry, Outcome, lower, syntax::parse};
use std::cell::RefCell;

/// 判断器替身：状态带了 ref 读 `有参照`，否则 0.5；记下每次见到的 ref 与 ctx 条数
fn 跑(src: &str, 有参照: f64) -> (Outcome, Ledger, Vec<(usize, usize)>) {
    let program = lower(&parse(src).expect("解析")).expect("lower");
    let 见 = RefCell::new(vec![]);
    let ports = Ports::new().with(FnPort::judge("fixed-0", |s, qs| {
        见.borrow_mut().push((s.r#ref.len(), s.ctx.len()));
        Ok(JudgeResult {
            answers: qs
                .iter()
                .map(|_| Answer::Noul(if s.r#ref.is_empty() { 0.5 } else { 有参照 }))
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
    let 见 = 见.into_inner();
    (o, l, 见)
}

fn 程序(来源: &str, 证据: &str) -> String {
    format!(
        "budget {{calls: 8, cost: 0, depth: 16}};\n{来源}\nlet e = cut(judge(state(mat(\"甲方与乙方的合作意向\")), test(\"比起过往合作，这次更好吗？\", \"k\", {{evidence: {证据}}})));\nhandle(e, {{act: fn() {{ \"更好\" }}, ignore: fn() {{ \"不更好\" }}}})\n"
    )
}

const 取参照: &str =
    r#"unsure_source({need: ["过往合作"], fetch: fn(q, need, m) { "过往合作的记录" }});"#;

fn 事件(l: &Ledger) -> Vec<&Entry> {
    l.entries.iter().filter(|e| e.is_duty_event()).collect()
}

#[test]
fn insufficient_ref_取到补进参照槽_再判已决() {
    let (o, l, 见) = 跑(&程序(取参照, "[\"ref\"]"), 0.9);
    assert_eq!(
        o.value_json(),
        serde_json::json!("更好"),
        "{:?}",
        o.unsure_default
    );
    let row = &o.unsure_default[0];
    assert_eq!(row["cause"], "insufficient");
    assert_eq!(row["end"], "decided");
    assert!(
        见.iter().any(|(r, c)| *r == 1 && *c == 0),
        "补进 ref、不补进 ctx：{见:?}"
    );
    let ev = 事件(&l);
    assert!(
        matches!(
            ev.as_slice(),
            [Entry::Enrich { got: true, .. }, Entry::Refine { .. }]
        ),
        "{ev:?}"
    );
}

/// 没有取法：路 C 转交。needed 照裁定五十五写信息类别，slot 写缺的证据槽（5.28 附录补）
#[test]
fn insufficient_没有取法_路c() {
    let (o, l, _) = 跑(
        &程序(r#"unsure_source({need: ["过往合作"]});"#, "[\"ref\"]"),
        0.9,
    );
    let row = &o.unsure_default[0];
    assert_eq!(row["end"], "handoff");
    assert_eq!(row["needed"], serde_json::json!(["过往合作"]));
    assert_eq!(row["slot"], "ref");
    assert!(matches!(事件(&l).as_slice(), [Entry::Handoff { .. }]));
}

/// 证据要 ref 与 ctx、候选只有一类：补进 ref 后仍缺 ctx，候选用完。5.28 附录起链末转交，slot 写缺的证据槽
#[test]
fn insufficient_候选用完_转交带缺的槽() {
    let (o, l, 见) = 跑(&程序(取参照, "[\"ref\", \"ctx\"]"), 0.9);
    let row = &o.unsure_default[0];
    assert_eq!(row["end"], "handoff", "{row}");
    assert!(
        row.get("needed").is_none(),
        "类别取到过，不进 needed（空时不写）：{row}"
    );
    assert_eq!(row["slot"], "ctx", "{row}");
    assert!(见.iter().all(|(_, c)| *c == 0), "没补进 ctx：{见:?}");
    let ev = 事件(&l);
    assert!(
        !ev.iter().any(|e| matches!(e, Entry::Drop { .. })),
        "{ev:?}"
    );
    assert!(
        matches!(ev.last(), Some(Entry::Handoff { cause, .. }) if cause == "insufficient"),
        "{ev:?}"
    );
}

/// 缺 over 的是非题（B155：是非题线上看不到 over，取什么都补不上）：不进轮次、不取材料，直接转交，slot 为 over
#[test]
fn insufficient_over_不进轮次_直接转交() {
    let (o, l, 见) = 跑(&程序(取参照, "[\"over\"]"), 0.9);
    let row = &o.unsure_default[0];
    assert_eq!(row["end"], "handoff", "{row}");
    assert!(
        row.get("needed").is_none() || row["needed"] == serde_json::json!([]),
        "没问类别：{row}"
    );
    assert_eq!(row["slot"], "over", "{row}");
    assert_eq!(row["fetched"], serde_json::json!([]), "{row}");
    assert!(见.len() <= 1, "只有原题一次：{见:?}");
    let ev = 事件(&l);
    assert!(matches!(ev.as_slice(), [Entry::Handoff { .. }]), "{ev:?}");
}

/// 候选多于一类、「最缺哪一类」拿不准（替身对它给全 0，诊断题自己拿不准）第一轮就停：一样没取，也按路 C 转交带缺的槽
#[test]
fn insufficient_为什么拿不准第一轮停_转交带缺的槽() {
    let 两类 = r#"unsure_source({need: ["过往合作", "预算"], fetch: fn(q, need, m) { "补来的" + need }});"#;
    let (o, l, _) = 跑_为什么全零(&程序(两类, "[\"ref\"]"));
    let row = &o.unsure_default[0];
    assert_eq!(row["why"], "unsure", "{row}");
    assert_eq!(row["end"], "handoff", "{row}");
    assert!(
        row.get("needed").is_none() || row["needed"] == serde_json::json!([]),
        "{row}"
    );
    assert_eq!(row["slot"], "ref", "{row}");
    assert!(
        事件(&l)
            .iter()
            .any(|e| matches!(e, Entry::Handoff { cause, .. } if cause == "insufficient"))
    );
}

/// 「为什么拿不准」给全 0（诊断题自己拿不准），原题读 0.5
fn 跑_为什么全零(src: &str) -> (Outcome, Ledger, ()) {
    let program = lower(&parse(src).expect("解析")).expect("lower");
    let ports = Ports::new().with(FnPort::judge("fixed-0", |s, qs| {
        Ok(JudgeResult {
            answers: qs
                .iter()
                .map(|q| {
                    if q.op == jpp::value::Op::Select {
                        Answer::Choice(vec![0.0; s.over.len()])
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
    (o, l, ())
}
