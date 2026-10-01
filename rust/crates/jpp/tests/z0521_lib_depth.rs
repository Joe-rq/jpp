//! Z0521：库的「没观察到」认 depth 与 deadline（预注册 `地基/过程记录/工程-Z0521-库认depth.md` §二 L-1、L-2）。
//!
//! 依据：B95（缺席类原因不能 drop）；B197（缺席类 `absent, budget, depth, latency, deadline`）；G4（`depth` 归缺席类）。

use std::cell::Cell;

use jpp::effects::{CalibStore, FnPort, JudgeResult, Ports};
use jpp::ledger::{CarryRecord, Ledger};
use jpp::value::Answer;
use jpp::{ActionRegistry, BudgetCarry, EntryArgs, Session};
use jpp::{lower, syntax::parse};
use serde_json::{Value as Json, json};

fn 端口(calls: &Cell<u64>) -> Ports<'_> {
    Ports::new().with(FnPort::judge("fixed-0", move |_s, qs| {
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
}

/// 上游传来的深度已到上限：这一趟判断一律不发，出 `Unsure(depth)`（G4）
fn 到限余额() -> BudgetCarry {
    BudgetCarry::from_record(CarryRecord {
        calls: 50,
        cost: 0.0,
        latency_p95: None,
        escalate: 3,
        hop: 2,
        round: 0,
        depth_cap: 2,
    })
}

fn 库(路径: &str) -> String {
    std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../..")
            .join(路径),
    )
    .expect("读得到库文件")
}

/// 库文本接在 budget 行后面（同 `bypass_25_3_unobserved.rs`）
fn 跑(body: &str) -> Result<Json, String> {
    let src = format!(
        "budget {{calls: 20, cost: 0, depth: 16}};\n{}\n{body}",
        库("lib/outcome.jpp")
    );
    let program =
        lower(&parse(&src).map_err(|e| format!("{e:?}"))?).map_err(|e| format!("{e:?}"))?;
    let calls = Cell::new(0);
    let calib = CalibStore::new();
    let acts = ActionRegistry::new();
    let o = Session::new(端口(&calls), &calib, &acts)
        .with_companions(jpp::interp::CompanionMode::Off)
        .with_carry(Some(到限余额()))
        .run(&program, &EntryArgs::default(), &mut Ledger::new())
        .map_err(|e| e.render())?;
    assert_eq!(calls.get(), 0, "深度到限：一道题都不发");
    Ok(o.value_json())
}

const 筛: &str = "let o = sieve([mat(\"甲\"), mat(\"乙\")], test(\"行吗\", \"k\"));\n";

/// L-1：深度到限停发的项归「没观察到」：`unobserved` 收进它们，`undecided` 为空；drop `undecided(o)` 照常
#[test]
fn l1_深度到限的项归没观察到() {
    let v = 跑(&format!(
        "{筛}{{u: len(unobserved(o)), d: len(undecided(o)), c: map(o.pending, fn(p) {{ p.cause }})}}\n"
    ))
    .unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(v["c"], json!(["depth", "depth"]), "{v}");
    assert_eq!(
        (v["u"].clone(), v["d"].clone()),
        (json!(2), json!(0)),
        "{v}"
    );
    let v = 跑(&format!(
        "{筛}consume(undecided(o), \"drop\");\n{{rest: unobserved(o)}}\n"
    ))
    .unwrap_or_else(|e| panic!("drop 判过而拿不准的那一类应当照常：{e}"));
    assert_eq!(v["rest"].as_array().map(|a| a.len()), Some(2));
}

/// L-2：`lib/compose/graph.jpp` 的 `graph_unobserved` 与 `lib/outcome.jpp` 的 `unobserved_cause` 同一集合（B197 缺席类五种）
#[test]
fn l2_两处库函数与缺席类同一集合() {
    let graph = 库("lib/compose/graph.jpp");
    let 行 = graph
        .lines()
        .find(|l| l.starts_with("fn graph_unobserved"))
        .expect("graph_unobserved 在");
    let src = format!(
        "budget {{calls: 1, cost: 0, depth: 8}};\n{}\n{行}\nlet cs = [\"absent\", \"budget\", \"depth\", \"latency\", \"deadline\", \"band\", \"tie\"];\n{{o: map(cs, fn(c) {{ unobserved_cause(c) }}), g: map(cs, fn(c) {{ graph_unobserved(c) }})}}\n",
        库("lib/outcome.jpp")
    );
    let program = lower(&parse(&src).expect("解析")).expect("lower");
    let calls = Cell::new(0);
    let o = jpp::run(
        &program,
        端口(&calls),
        &CalibStore::new(),
        &ActionRegistry::new(),
        &mut Ledger::new(),
    )
    .unwrap_or_else(|e| panic!("{}", e.render()));
    let want = json!([true, true, true, true, true, false, false]);
    assert_eq!(o.value_json(), json!({"o": want, "g": want}));
}
