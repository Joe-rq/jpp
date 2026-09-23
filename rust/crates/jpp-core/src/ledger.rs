//! 只增账本（§2.10）：判断读数、效应记录、成本。重放时同键零调用。

use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use serde_json::Value as Json;

use crate::ast::Span;
use crate::value::{Answer, hash_of};

pub const RENDER_VERSION: &str = "r1";

/// 账本键。`site` 是**调用点**（`.jpp` 源码里的字节偏移），与 Python 的
/// `foundation/jv/store.py:26` 同一组成分——那边的 `site` 是「第一个不在 jv 包内的栈帧，
/// `文件名:行号`，同程序重放时稳定」，这边用 `Span.start` 干同一件事。
///
/// **缺了它会撞键**：同状态同题的两个不同站点会合成一条记录，于是第二个站点从账本里
/// 命中第一个站点的答案。那不是漏记，是**命中一条本不该命中的记录**——同一程序里问同一道题
/// 两次是两次判断，合成一次，第二次就不再是一次观察，而是复制第一次。
pub fn judge_key(model_id: &str, state_hash: &str, q_hash: &str, phys: &str, perm_seed: u64, run_seq: u64, site: usize) -> String {
    hash_of(&["judge", model_id, state_hash, q_hash, phys, RENDER_VERSION, &perm_seed.to_string(), &run_seq.to_string(), &site.to_string()])
}

pub fn effect_key(kind: &str, parts: &[&str]) -> String {
    let mut v = vec![kind];
    v.extend_from_slice(parts);
    hash_of(&v)
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Entry {
    Judge { key: String, answer: Answer, tokens: u64, cost: f64, model_id: String },
    Effect { key: String, kind: String, output: Json, cost: f64 },
    Ask { key: String, answer: Option<Answer> },
}

impl Entry {
    pub fn key(&self) -> &str {
        match self {
            Entry::Judge { key, .. } | Entry::Effect { key, .. } | Entry::Ask { key, .. } => key,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Header {
    pub budget_calls: u64,
    pub budget_cost: f64,
    pub model_id: String,
    pub render_version: String,
    pub handler_version: String,
    /// 这次运行用的模型档案的哈希（`12` §J-18）。**`None` = 本次未加载档案**，
    /// 线与 δ 是代码兜底值——这个痕迹必须留下，否则「用了兜底」与「档案恰好等于兜底」
    /// 在账本上分不开，换档案重放也察觉不到。
    #[serde(default)]
    pub profile_hash: Option<String>,
    /// 行为承载子集的摘要。**两个都记**：`profile_hash` 不同即不承诺重放一致（判定不变），
    /// 而这一个告诉看账本的人**那次不同是线和 δ 也变了，还是只改了说明**。
    #[serde(default)]
    pub behavior_hash: Option<String>,
    /// 这次运行用的**校准库**的哈希（J-18）。`profile_hash` 记的是档案里的**缺省线**，
    /// 而 `cut` 用的是**按键的线**——**出口 = f(读数, 线)，读数进了账本，线以前没有。**
    ///
    /// **`None` 只有一个意思：这份账本早于这个字段。** 空库也有自己的哈希，
    /// 否则「一条记录都没有」与「老账本」在头上分不开。老账本重放因此必报 `W-header`，
    /// 这是对的——**那次运行用了哪些线，我们确实不知道**。
    #[serde(default)]
    pub calib_hash: Option<String>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Ledger {
    pub header: Option<Header>,
    pub entries: Vec<Entry>,
    #[serde(skip)]
    index: HashMap<String, usize>,
    pub header_warning: Option<String>,
}

impl Ledger {
    pub fn new() -> Ledger {
        Ledger::default()
    }
    pub fn rebuild_index(&mut self) {
        self.index = self.entries.iter().enumerate().map(|(i, e)| (e.key().to_string(), i)).collect();
    }
    /// 账本头（J-18）：不同即报 W-header，不承诺重放一致。
    pub fn set_header(&mut self, h: Header) {
        if let Some(old) = &self.header {
            if old.model_id != h.model_id || old.render_version != h.render_version || old.handler_version != h.handler_version
                || old.budget_calls != h.budget_calls || old.budget_cost != h.budget_cost
                // 换了档案 = 线与 δ 可能都变了，重放结果不保证一致
                || old.profile_hash != h.profile_hash
                // 换了校准记录 = 按键的线可能变了，读数照旧而出口可以不同
                || old.calib_hash != h.calib_hash
            {
                self.header_warning = Some(format!("W-header: 账本头不同，不承诺重放一致：旧 {old:?} 新 {h:?}"));
            }
        }
        self.header = Some(h);
    }
    pub fn get(&self, key: &str) -> Option<&Entry> {
        self.index.get(key).map(|i| &self.entries[*i])
    }
    pub fn put(&mut self, e: Entry) {
        let k = e.key().to_string();
        if self.index.contains_key(&k) {
            return;
        }
        self.index.insert(k, self.entries.len());
        self.entries.push(e);
    }
    pub fn len(&self) -> usize {
        self.entries.len()
    }
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

/// 执行轨迹：每个效应一行，可打印。
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TraceEvent {
    pub kind: String,
    pub key: String,
    pub replayed: bool,
    pub cost: f64,
    pub site: Span,
    pub note: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Trace {
    pub events: Vec<TraceEvent>,
    pub warnings: Vec<String>,
}

impl Trace {
    pub fn push(&mut self, kind: &str, key: &str, replayed: bool, cost: f64, site: Span, note: String) {
        self.events.push(TraceEvent { kind: kind.into(), key: key.into(), replayed, cost, site, note });
    }
    pub fn warn(&mut self, w: String) {
        self.warnings.push(w);
    }
    pub fn count(&self, kind: &str, replayed: bool) -> usize {
        self.events.iter().filter(|e| e.kind == kind && e.replayed == replayed).count()
    }
    pub fn render(&self) -> String {
        let mut s = String::new();
        for (i, e) in self.events.iter().enumerate() {
            s.push_str(&format!("{:>3} {:<9} {} {:<8} ${:.6} @{}..{} {}\n", i, e.kind, e.key.chars().take(12).collect::<String>(), if e.replayed { "replay" } else { "live" }, e.cost, e.site.start, e.site.end, e.note));
        }
        for w in &self.warnings {
            s.push_str(&format!("    ! {w}\n"));
        }
        s
    }
}
