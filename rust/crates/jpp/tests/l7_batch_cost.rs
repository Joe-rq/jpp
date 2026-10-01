//! L7（2026-09-28）赛后欠账「合批花费重复记账」：一次合批调用（同材料多题，B155）里每条判断条目都记整次
//! 调用的花费，按条目相加多算（案例 05 约 5 倍）。改为只在同调用号的首条记整次 tokens/cost、其余记 0；
//! 按调用号去重的读者（审计重放、契约值 `spent`）按同调用号取最大值，新旧账本都对。
//! L7: merged-call cost is recorded on the first entry of the call only; readers that dedupe by call take the max.

mod common;
use common::{run_replay_关 as run_replay, run_关 as run};
use jpp::effects::{CalibStore, EffectError, JevClient, JevPorts, ReplayPorts};
use jpp::ledger::{Entry, Ledger};
use jpp::{ActionRegistry, lower, syntax::parse};
use serde_json::{Value as Json, json};

const 价: f64 = 1e-6;

/// 每次请求 10 个 input token，每题答 0.9
fn 端口() -> JevPorts {
    let mut c = JevClient::with_transport(
        "jev-1.13.0",
        Box::new(|body: &Json| -> Result<Json, EffectError> {
            let mut ans = serde_json::Map::new();
            for (qid, _) in body["questions"].as_object().expect("有题") {
                ans.insert(qid.clone(), json!({"noul": 0.9}));
            }
            Ok(json!({"answers": ans, "usage": {"input_tokens": 10}}))
        }),
    );
    c.usd_per_input_token = Some(价);
    JevPorts::new(c)
}

/// 两份材料 × 两道题：两次调用，每次合两道题
const 程序: &str = r#"budget {calls: 10, cost: 1, depth: 64};
let o = sieve(["甲", "乙"], [test("第一问？", "k1"), test("第二问？", "k2")]);
{all: o.spent, sub: by_q(o, 1).spent}"#;

/// 读法函数（`accepted`、`by_q`）来自 `lib/outcome.jpp`，与 `bypass_b81_b82.rs` 同法接在 budget 行之后
fn 带库(src: &str) -> jpp::Program {
    let lib = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../lib/outcome.jpp"),
    )
    .expect("读得到 lib/outcome.jpp");
    let (budget, rest) = src.split_once('\n').expect("第一行是 budget");
    lower(&parse(&format!("{budget}\n{lib}\n{rest}")).expect("解析")).expect("lower")
}

fn 判断费(l: &Ledger) -> Vec<(u64, u64, f64)> {
    l.entries
        .iter()
        .filter_map(|e| match e {
            Entry::Judge {
                call, tokens, cost, ..
            } => Some((*call, *tokens, *cost)),
            _ => None,
        })
        .collect()
}

fn 近(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-12
}

#[test]
fn 合批只在首条记费_相加等于实际花费_spent按整次调用() {
    let p = 带库(程序);
    let mut jp = 端口();
    let mut l = Ledger::new();
    let o = run(
        &p,
        jp.ports(),
        &CalibStore::new(),
        &ActionRegistry::new(),
        &mut l,
    )
    .unwrap_or_else(|e| panic!("{}", e.render()));
    let 一次 = 10.0 * 价;
    assert_eq!(o.cost.calls, 2);
    assert!(近(o.cost.usd, 2.0 * 一次), "{}", o.cost.usd);
    let 条目 = 判断费(&l);
    assert_eq!(条目.len(), 4, "{条目:?}");
    // 每次调用：首条整次、其余 0
    let 首 = 条目[0].0;
    assert_eq!(条目[1].0, 首);
    assert!(近(条目[0].2, 一次) && 条目[0].1 == 10, "{条目:?}");
    assert!(条目[1].2 == 0.0 && 条目[1].1 == 0, "{条目:?}");
    let 和: f64 = 条目.iter().map(|x| x.2).sum();
    assert!(
        近(和, o.cost.usd),
        "按条目相加 = 实际花费：{和} vs {}",
        o.cost.usd
    );
    // 契约值 spent：整体与只含第二题的子契约值都按整次调用计（证据里没有首条也一样）
    let v = o.value_json();
    assert_eq!(v["all"]["calls"], json!(2));
    assert_eq!(v["sub"]["calls"], json!(2));
    assert!(近(v["all"]["usd"].as_f64().unwrap(), 2.0 * 一次), "{v}");
    assert!(
        近(v["sub"]["usd"].as_f64().unwrap(), 2.0 * 一次),
        "子契约值的证据只有每次调用的第二条（记 0），spent 仍按整次调用：{v}"
    );

    // 审计重放：值（含 spent）逐字相同
    l.rebuild_index();
    let r = run_replay(
        &p,
        ReplayPorts::ports("jev-1.13.0"),
        &CalibStore::new(),
        &ActionRegistry::new(),
        &mut l,
    )
    .unwrap_or_else(|e| panic!("{}", e.render()));
    assert_eq!(r.value_json(), v, "重放同值");

    // 旧账本（2026-09-28 前每条都记整次费用）：按调用号取最大，重放出的 spent 不变
    let mut 旧 = l.clone();
    for e in 旧.entries.iter_mut() {
        if let Entry::Judge { cost, tokens, .. } = e {
            *cost = 一次;
            *tokens = 10;
        }
    }
    旧.rebuild_index();
    let r = run_replay(
        &p,
        ReplayPorts::ports("jev-1.13.0"),
        &CalibStore::new(),
        &ActionRegistry::new(),
        &mut 旧,
    )
    .unwrap_or_else(|e| panic!("{}", e.render()));
    assert_eq!(r.value_json(), v, "旧格式账本重放同值");
}

/// 审计重放按账本计入预算：预算不到一次调用的费用时（发出前按已花核，第一次照发），首跑第二次调用停发（Unsure(budget)），
/// 重放在同一处停——同调用号先读到哪条都按整次调用计。
#[test]
fn 审计重放按整次调用计费_停在同一处() {
    let 一次 = 10.0 * 价;
    let src = format!(
        r#"budget {{calls: 10, cost: {}, depth: 64}};
let o = sieve(["甲", "乙"], [test("第一问？", "k1"), test("第二问？", "k2")]);
{{n: len(accepted(o)), p: o.pending, all: o.spent}}"#,
        一次 * 0.5
    );
    let p = 带库(&src);
    let mut jp = 端口();
    let mut l = Ledger::new();
    let o = run(
        &p,
        jp.ports(),
        &CalibStore::new(),
        &ActionRegistry::new(),
        &mut l,
    )
    .unwrap_or_else(|e| panic!("{}", e.render()));
    assert_eq!(o.cost.calls, 1, "第二次调用发不起");
    let v = o.value_json();
    l.rebuild_index();
    let r = run_replay(
        &p,
        ReplayPorts::ports("jev-1.13.0"),
        &CalibStore::new(),
        &ActionRegistry::new(),
        &mut l,
    )
    .unwrap_or_else(|e| panic!("{}", e.render()));
    assert_eq!(r.value_json(), v, "重放停在同一处、同值");
}
