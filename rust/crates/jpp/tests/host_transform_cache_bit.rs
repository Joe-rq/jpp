//! Z0222：宿主变换的「读外部状态，不跨运行缓存」位（`HostTransform.reads_external_state`）决定它的条目进不进
//! 跨运行索引，取代 Z0206 的名字前缀 `host:mat_` 判断。预注册见 `地基/过程记录/工程-Z0222-缓存语义位.md` 第二节。
//!
//! 测试用一张自造的表：`peek` 名字不带 `mat_` 但带位，`mat_pure` 名字带 `mat_` 前缀但不带位。
//! 按位判断时 `peek` 不进、`mat_pure` 进；按名字前缀判断则正好相反，所以这个测试能分清两种实现。

use jpp::ledger::{Entry, Ledger};
use jpp::store::{CacheIndex, CacheIndexCounts};
use jpp_effects::views::CacheLookup;
use jpp_effects::{HostTaint, HostTransform, TransformTable};
use jpp_ledger::EffectKey;
use serde_json::{Value as Json, json};

fn 变换(名: &str, 读外部: bool) -> HostTransform {
    HostTransform {
        name: 名.into(),
        version: "1".into(),
        taint_out: HostTaint::Inherit,
        doc: String::new(),
        reads_external_state: 读外部,
        run: Box::new(|_| Ok(Json::Null)),
    }
}

fn 表(peek读外部: bool) -> TransformTable {
    let mut t = TransformTable::new();
    t.register(变换("peek", peek读外部));
    t.register(变换("mat_pure", false));
    t
}

fn 条目(site: &str, 名: &str) -> Entry {
    let ek = EffectKey::new("transform", &[site, &format!("host:{名}@1"), "", "h"]);
    Entry::effect(ek, "transform", json!(["产物"]), 0.0)
}

fn 账本() -> Vec<(String, Ledger)> {
    let mut l = Ledger::new();
    l.put(条目("1", "peek"));
    l.put(条目("2", "mat_pure"));
    l.put(条目("3", "表外的宿主变换"));
    vec![("a.jsonl".to_string(), l)]
}

fn 命中(ix: &CacheIndex, 名: &str) -> bool {
    let ek = EffectKey::new("transform", &["99", &format!("host:{名}@1"), "", "h"]);
    ix.get_effect(&ek.cache_digest(None).unwrap()).is_some()
}

#[test]
fn 带位的变换不进索引_不带的照进_不看名字() {
    let ix = CacheIndex::build_with(&账本(), &表(true));
    assert!(!命中(&ix, "peek"), "带位：不进索引（名字里没有 mat_）");
    assert!(
        命中(&ix, "mat_pure"),
        "不带位：照进（名字里有 mat_ 也照进）"
    );
    assert!(
        命中(&ix, "表外的宿主变换"),
        "表里没登记的方法位：没有位可查，照进"
    );
    assert_eq!(
        ix.counts(),
        CacheIndexCounts {
            judge: 0,
            gen_: 0,
            transform: 2
        }
    );
}

#[test]
fn 位取自给定的表_同一份账本换表结果变() {
    let ix = CacheIndex::build_with(&账本(), &表(false));
    assert!(命中(&ix, "peek"), "同一变换在不带位的表里就进索引");
    assert_eq!(ix.counts().transform, 3);
}
