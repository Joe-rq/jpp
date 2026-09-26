//! 跨运行缓存索引（`20` v2 §2.3 `jpp::store::CacheIndex`、§九缓存索引行；B40、B151，步 19）。
//!
//! 从一组账本建：判断条目按判断缓存键（`JudgeKey::cache_key`，不含调用位置与运行序号，选择题含 `perm_seed`），
//! `gen`、`transform` 的效应条目按 `EffectKey::cache_digest`（不含调用位置；`gen` 带生成器模型，取该账本头的
//! `gen_model`，没有则取 `model_id`）。只收有结构化键、不是复用条目、不是失败值的条目；同键先到先得（来源按给定
//! 顺序、条目按序号）。它是派生物，不落盘，可丢弃重建（`20` v2 §九「可丢弃」）。
//! 依据：B40（`20` v2 附录）；B151（`21` 步 19 追加项）；B74（本模块只依赖 `jpp_ledger` 与 `jpp_effects` 的视图）

use std::collections::HashMap;

use jpp_effects::ReuseRule;
use jpp_effects::views::{CacheLookup, CachedReading};
use jpp_ir::key::CacheKey;
use jpp_ledger::{Entry, Ledger};
use serde_json::Value as Json;

/// 跨运行缓存索引。
#[derive(Clone, Debug, Default)]
pub struct CacheIndex {
    judges: HashMap<String, CachedReading>,
    effects: HashMap<String, CachedReading>,
}

/// 各类进了索引的条目数（宿主报给人看）。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CacheIndexCounts {
    pub judge: usize,
    pub gen_: usize,
    pub transform: usize,
}

impl CacheIndex {
    /// 从账本建索引。`ledgers` 的每项是（来源名，账本）；来源名进复用条目的 `reused_from`（`ext:<来源>#<序号>`）。
    pub fn build(ledgers: &[(String, Ledger)]) -> CacheIndex {
        let mut ix = CacheIndex::default();
        for (source, l) in ledgers {
            let gen_model = l
                .header
                .as_ref()
                .map(|h| h.gen_model_or_default().to_string())
                .unwrap_or_default();
            for (i, e) in l.entries.iter().enumerate() {
                let record = || CachedReading {
                    record: serde_json::to_value(e).expect("账本条目可序列化"),
                    source: format!("{source}#{}", i + 1),
                };
                match e {
                    Entry::Judge {
                        jkey: Some(jk),
                        reused_from: None,
                        ..
                    } => {
                        ix.judges
                            .entry(jk.cache_key().digest())
                            .or_insert_with(record);
                    }
                    Entry::Effect {
                        ekey: Some(ek),
                        reused_from: None,
                        output,
                        ..
                    } if !是失败值(output) => {
                        // 复用规则只在注册表里定（`EffectSpec.reuse`）：生成物带来源账本的生成器模型
                        let d = match jpp_effects::by_name(&ek.kind).map(|s| s.reuse) {
                            Some(ReuseRule::Generator) => ek.cache_digest(Some(&gen_model)),
                            Some(ReuseRule::Method) => ek.cache_digest(None),
                            _ => None,
                        };
                        if let Some(d) = d {
                            ix.effects.entry(d).or_insert_with(record);
                        }
                    }
                    _ => {}
                }
            }
        }
        ix
    }

    /// 各类条目数。
    pub fn counts(&self) -> CacheIndexCounts {
        let mut c = CacheIndexCounts {
            judge: self.judges.len(),
            ..Default::default()
        };
        for r in self.effects.values() {
            let rule = r
                .record
                .get("Effect")
                .and_then(|e| e.get("kind"))
                .and_then(Json::as_str)
                .and_then(jpp_effects::by_name)
                .map(|s| s.reuse);
            match rule {
                Some(ReuseRule::Generator) => c.gen_ += 1,
                _ => c.transform += 1,
            }
        }
        c
    }
}

fn 是失败值(output: &Json) -> bool {
    output.get("__fail").is_some()
}

impl CacheLookup for CacheIndex {
    fn get(&self, k: &CacheKey) -> Option<CachedReading> {
        self.judges.get(&k.digest()).cloned()
    }
    fn get_effect(&self, digest: &str) -> Option<CachedReading> {
        self.effects.get(digest).cloned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::value::Answer;
    use jpp_ledger::{EffectKey, Header, JudgeKey};

    fn 判断(key: &str, site: usize, p: f64, reused: Option<&str>) -> Entry {
        let jk = JudgeKey::new("m", "s", "q", "noul", 0, 0, site);
        let mut e = Entry::judge(key, Answer::Noul(p), 0, 0.0, "m", 1);
        if let Entry::Judge {
            jkey, reused_from, ..
        } = &mut e
        {
            *jkey = Some(jk);
            *reused_from = reused.map(String::from);
        }
        e
    }

    fn 生成(site: &str, out: Json) -> Entry {
        let ek = EffectKey::new("gen", &[site, "提示", "", "1", "0"]);
        Entry::effect(ek, "gen", out, 0.0)
    }

    #[test]
    fn 先到先得_跳过复用条目与失败值_生成键带来源账本的模型() {
        let mut a = Ledger::new();
        a.set_header(Header::new(1, 1.0, "m", "r1", "h").with_gen(Some("甲".into()), None));
        a.put(判断("k1", 10, 0.9, Some("别处")));
        a.put(判断("k2", 20, 0.8, None));
        a.put(判断("k3", 30, 0.1, None));
        a.put(生成("5", serde_json::json!({"__fail": "timeout"})));
        a.put(生成("6", serde_json::json!(["一"])));
        let ix = CacheIndex::build(&[("a.jsonl".into(), a)]);
        let ck = JudgeKey::new("m", "s", "q", "noul", 0, 0, 99).cache_key();
        let hit = ix.get(&ck).expect("同缓存键命中");
        assert_eq!(
            hit.source, "a.jsonl#2",
            "复用条目不作来源，先到先得取第 2 条"
        );
        let ek = EffectKey::new("gen", &["77", "提示", "", "1", "0"]);
        let hit = ix
            .get_effect(&ek.cache_digest(Some("甲")).unwrap())
            .expect("生成命中（调用位置不同）");
        assert_eq!(hit.source, "a.jsonl#5", "失败值不作来源");
        assert!(
            ix.get_effect(&ek.cache_digest(Some("乙")).unwrap())
                .is_none(),
            "换模型不命中"
        );
        assert_eq!(
            ix.counts(),
            CacheIndexCounts {
                judge: 1,
                gen_: 1,
                transform: 0
            }
        );
    }
}
