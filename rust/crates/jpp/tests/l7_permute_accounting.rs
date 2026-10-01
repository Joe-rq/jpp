//! L7（2026-09-28）核实 `{permute: true}` 的记账（赛后欠账、现状图「`{permute}` 真机记账与预期不符、未闭环核实」）。
//! 真机端口 `JevPorts` 加本地计数传输（不发网络、$0）：声明置换的 `select` 发正逆两遍请求。
//! L7: verify how `{permute: true}` is accounted (requests, calls, tokens, cost, ledger entries).

mod common;
use common::{run_replay_关 as run_replay, run_关 as run}; // 数账本条目，固定关伴随题（主控 2026-09-30：第二类）
use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
};

use jpp::effects::{CalibStore, EffectError, JevClient, JevPorts, ReplayPorts};
use jpp::ledger::{Entry, Ledger};
use jpp::{ActionRegistry, lower, syntax::parse};
use serde_json::{Value as Json, json};

const 价: f64 = 1e-6;

fn 端口(n: Arc<AtomicU64>) -> JevPorts {
    let mut c = JevClient::with_transport(
        "jev-1.13.0",
        Box::new(move |body: &Json| -> Result<Json, EffectError> {
            n.fetch_add(1, Ordering::SeqCst);
            let mut ans = serde_json::Map::new();
            for (qid, q) in body["questions"].as_object().expect("有题") {
                let a = match q["type"].as_str() {
                    Some("choice") => {
                        // 把 0.8 给内容是「乙」的候选（按值找，正逆序都指向同一个候选）
                        let mut probs = serde_json::Map::new();
                        for (k, v) in q["criteria"].as_object().expect("候选") {
                            probs.insert(k.clone(), json!(if v == "乙" { 0.8 } else { 0.1 }));
                        }
                        json!({"probabilities": probs})
                    }
                    _ => json!({"noul": 0.9}),
                };
                ans.insert(qid.clone(), a);
            }
            Ok(json!({"answers": ans, "usage": {"input_tokens": 10}}))
        }),
    );
    c.usd_per_input_token = Some(价);
    JevPorts::new(c)
}

const 程序: &str = r#"budget {calls: 10, cost: 1, depth: 16};
let s = state(mat("一段话"), {over: [mat("甲"), mat("乙"), mat("丙")]});
let e = cut(judge(s, select("哪一个", "k", {permute: true})));
let k = exit_kind(e);
consume(e, "drop");
k
"#;

#[test]
fn 置换两遍请求_记账口径() {
    let p = lower(&parse(程序).expect("解析")).expect("lower");
    let n = Arc::new(AtomicU64::new(0));
    let mut jp = 端口(n.clone());
    let mut l = Ledger::new();
    let o = run(
        &p,
        jp.ports(),
        &CalibStore::new(),
        &ActionRegistry::new(),
        &mut l,
    )
    .unwrap_or_else(|e| panic!("{}", e.render()));
    let 请求 = n.load(Ordering::SeqCst);
    let 条目: Vec<_> = l
        .entries
        .iter()
        .filter_map(|e| match e {
            Entry::Judge {
                call,
                tokens,
                cost,
                perm,
                ..
            } => Some((*call, *tokens, *cost, *perm)),
            _ => None,
        })
        .collect();
    println!(
        "请求 {请求}；cost.calls {}；usd {}；条目 {条目:?}；值 {}",
        o.cost.calls,
        o.cost.usd,
        o.value_json()
    );
    // 事实（2026-09-28 核实）：
    assert_eq!(请求, 2, "置换发正逆两遍请求");
    assert_eq!(条目.len(), 1, "一道题一条账本条目");
    let (_, tokens, cost, perm) = 条目[0];
    let perm = perm.expect("置换测量进账本");
    assert_eq!(perm.perms, 2);
    assert_eq!(perm.mode_share, 1.0, "两序众数一致");
    assert_eq!(tokens, 20, "两遍的 token 相加");
    assert!(
        (cost - 20.0 * 价).abs() < 1e-12,
        "费用按两遍 token 计：{cost}"
    );
    assert!((o.cost.usd - 20.0 * 价).abs() < 1e-12);
    assert_eq!(o.value_json(), json!("pick(1)"), "两序一致，出口 Pick");
    // **已知缺口（钉住现状，不是期望）**：`cost.calls`（`budget.calls` 核的数、报告的 calls、账本 `call` 编号）
    // 按「一次判断调用」计为 1，没有按实际发出的两遍请求计 2；设计文本（`jev.rs` 注释、M1 文档、K-122）说置换让
    // select 的调用数 ×2。费用与 token 已按两遍计，对。修法与要先定的事见 `地基/附注/2026-09-28-L7回报.md` 第 7 项；
    // 修好后把这条改成 2。
    assert_eq!(o.cost.calls, 1, "现状：置换的两遍请求只计一次调用");
    assert_eq!(条目[0].0, 1, "账本 call 编号同样只进一");

    // 审计重放同值、0 请求
    l.rebuild_index();
    let r = run_replay(
        &p,
        ReplayPorts::ports("jev-1.13.0"),
        &CalibStore::new(),
        &ActionRegistry::new(),
        &mut l,
    )
    .unwrap_or_else(|e| panic!("{}", e.render()));
    assert_eq!(r.value_json(), o.value_json());
}
