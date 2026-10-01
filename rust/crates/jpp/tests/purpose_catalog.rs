//! Z0511：已认证题式目录 lib/bank/catalog.jpp 与 bank/bank.json 的「已认证」条目一一对应（slug、题类、题型、题式哈希）。
//! 题库加减或改条目而目录没跟上，这里即红：目录不做第二个维护点（意图汇编 12）。

mod derive_support;
use derive_support::*;
use jpp::value::Answer;
use serde_json::{Value as Json, json};
use std::collections::BTreeSet;

#[test]
fn 目录与题库的已认证条目一一对应() {
    let src = "import \"../../lib/bank/catalog.jpp\";\nbudget {calls: 1, cost: 0, depth: 64};\n\
               map(bank_catalog(), fn(e) { {slug: e.slug, kind: e.kind, op: e.op, hash: e.form.hash} })";
    let r = 跑(src, |_t, _q, _s| Answer::Noul(0.5), vec![]).unwrap();
    let got: BTreeSet<String> = r
        .out
        .value_json()
        .as_array()
        .unwrap()
        .iter()
        .map(|e| {
            format!(
                "{}|{}|{}|{}",
                e["slug"].as_str().unwrap(),
                e["kind"].as_str().unwrap(),
                e["op"].as_str().unwrap(),
                e["hash"].as_str().unwrap()
            )
        })
        .collect();
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let bank: Json =
        serde_json::from_slice(&std::fs::read(root.join("bank/bank.json")).unwrap()).unwrap();
    let want: BTreeSet<String> = bank["entries"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|e| {
            e["status"]
                .as_str()
                .is_some_and(|s| s.starts_with("已认证"))
        })
        .map(|e| {
            format!(
                "{}|{}|{}|{}",
                e["slug"].as_str().unwrap(),
                e["kind"].as_str().unwrap(),
                e["op"].as_str().unwrap(),
                e["form_hash"].as_str().unwrap()
            )
        })
        .collect();
    assert_eq!(
        got,
        want,
        "{}",
        json!({"catalog_only": got.difference(&want).collect::<Vec<_>>(), "bank_only": want.difference(&got).collect::<Vec<_>>()})
    );
}
