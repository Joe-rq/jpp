//! 跨运行缓存索引（`20` v2 §2.3 `jpp::store::CacheIndex`、§九缓存索引行；B40、B151，步 19）。
//!
//! 从一组账本建：判断条目按判断缓存键（`JudgeKey::cache_key`，不含调用位置与运行序号，选择题含 `perm_seed`），
//! `gen`、`transform` 的效应条目按 `EffectKey::cache_digest`（不含调用位置；`gen` 带生成器模型，取该账本头的
//! `gen_model`，没有则取 `model_id`）。只收有结构化键、不是复用条目、不是失败值的条目；同键先到先得（来源按给定
//! 顺序、条目按序号）。宿主变换带「读外部状态，不跨运行缓存」位（`HostTransform.reads_external_state`，如料库标记的读写）的不收：
//! 结果随外部状态变、键里没有那份状态（Z0206；位由宿主变换注册表定，Z0222）。
//! 它是派生物，不落盘，可丢弃重建（`20` v2 §九「可丢弃」）。
//! 依据：B40（`20` v2 附录）；B151（`21` 步 19 追加项）；B74（本模块依赖 `jpp_ledger` 与 `jpp_effects` 的视图；`build` 的默认宿主变换表取自 `jpp_lib::standard_transforms`）

use std::collections::HashMap;

use jpp_effects::views::{CacheLookup, CachedReading};
use jpp_effects::{ReuseRule, TransformTable};
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
    /// 宿主变换的语义位取标准库的宿主变换表（`jpp_lib::standard_transforms`）；自带宿主变换表的用 [`CacheIndex::build_with`]。
    pub fn build(ledgers: &[(String, Ledger)]) -> CacheIndex {
        CacheIndex::build_with(ledgers, &jpp_lib::standard_transforms(None))
    }

    /// [`CacheIndex::build`]，宿主变换的「读外部状态」位按给定的表查。键的方法位在表里查不到的（闭包变换、
    /// 表外的宿主变换）照进索引：没有登记就没有位可查，也没有别的依据不收。
    pub fn build_with(ledgers: &[(String, Ledger)], transforms: &TransformTable) -> CacheIndex {
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
                            Some(ReuseRule::Method) if !读外部状态(ek, transforms) => {
                                ek.cache_digest(None)
                            }
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

/// 方法位（键的第二段，`host:<名>@<版本>`）在宿主变换表里带「读外部状态」位（`mat_marks`、`mat_mark` 等，Z0206）。
/// 这类结果取决于外部状态当时的内容，键里没有那份状态：另一本账本里同输入的记录拿来当本次的复用，读会读到旧状态，
/// 写会被跳过、外部状态一点没写。所以不进跨运行索引；本运行内的复用与账本重放不经索引，不受影响。
fn 读外部状态(ek: &jpp_ledger::EffectKey, transforms: &TransformTable) -> bool {
    ek.parts
        .get(1)
        .and_then(|m| transforms.by_identity(m))
        .is_some_and(|t| t.reads_external_state)
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

    fn 变换(site: &str, 方法: &str, 输入: &str, out: Json) -> Entry {
        let ek = EffectKey::new("transform", &[site, 方法, "", 输入]);
        Entry::effect(ek, "transform", out, 0.0)
    }

    #[test]
    fn 料库标记变换不进索引_其他变换照进() {
        let mut a = Ledger::new();
        a.put(变换(
            "1",
            "host:mat_marks@1",
            "h",
            serde_json::json!([{"found": false}]),
        ));
        a.put(变换(
            "2",
            "host:mat_mark@1",
            "h",
            serde_json::json!({"written": 1}),
        ));
        a.put(变换("3", "host:diagnose@1", "h", serde_json::json!([])));
        a.put(变换("4", "闭包哈希", "h", serde_json::json!(["x"])));
        let ix = CacheIndex::build(&[("a.jsonl".into(), a)]);
        assert_eq!(
            ix.counts(),
            CacheIndexCounts {
                judge: 0,
                gen_: 0,
                transform: 2
            },
            "只有 diagnose 与闭包变换进索引"
        );
        for 方法 in ["host:mat_marks@1", "host:mat_mark@1"] {
            let ek = EffectKey::new("transform", &["9", 方法, "", "h"]);
            assert!(
                ix.get_effect(&ek.cache_digest(None).unwrap()).is_none(),
                "{方法} 不跨运行复用"
            );
        }
        let ek = EffectKey::new("transform", &["9", "host:diagnose@1", "", "h"]);
        assert!(ix.get_effect(&ek.cache_digest(None).unwrap()).is_some());
    }
}
