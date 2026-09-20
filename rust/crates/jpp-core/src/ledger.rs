//! 只增账本（§2.10）：判断读数、效应记录、成本。重放时同键零调用。

use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use serde_json::Value as Json;

use crate::ast::Span;
use crate::value::{Answer, hash_of};

pub const RENDER_VERSION: &str = "r1";

pub fn judge_key(
    model_id: &str,
    state_hash: &str,
    q_hash: &str,
    phys: &str,
    perm_seed: u64,
    run_seq: u64,
) -> String {
    hash_of(&[
        "judge",
        model_id,
        state_hash,
        q_hash,
        phys,
        RENDER_VERSION,
        &perm_seed.to_string(),
        &run_seq.to_string(),
    ])
}

pub fn effect_key(kind: &str, parts: &[&str]) -> String {
    let mut v = vec![kind];
    v.extend_from_slice(parts);
    hash_of(&v)
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Entry {
    Judge {
        key: String,
        answer: Answer,
        tokens: u64,
        cost: f64,
        model_id: String,
    },
    Effect {
        key: String,
        kind: String,
        output: Json,
        cost: f64,
    },
    Ask {
        key: String,
        answer: Option<Answer>,
    },
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
        self.index = self
            .entries
            .iter()
            .enumerate()
            .map(|(i, e)| (e.key().to_string(), i))
            .collect();
    }
    /// 账本头（J-18）：不同即报 W-header，不承诺重放一致。
    pub fn set_header(&mut self, h: Header) {
        if let Some(old) = &self.header {
            if old.model_id != h.model_id
                || old.render_version != h.render_version
                || old.handler_version != h.handler_version
                || old.budget_calls != h.budget_calls
                || old.budget_cost != h.budget_cost
            {
                self.header_warning = Some(format!(
                    "W-header: 账本头不同，不承诺重放一致：旧 {old:?} 新 {h:?}"
                ));
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
    pub fn push(
        &mut self,
        kind: &str,
        key: &str,
        replayed: bool,
        cost: f64,
        site: Span,
        note: String,
    ) {
        self.events.push(TraceEvent {
            kind: kind.into(),
            key: key.into(),
            replayed,
            cost,
            site,
            note,
        });
    }
    pub fn warn(&mut self, w: String) {
        self.warnings.push(w);
    }
    pub fn count(&self, kind: &str, replayed: bool) -> usize {
        self.events
            .iter()
            .filter(|e| e.kind == kind && e.replayed == replayed)
            .count()
    }
    pub fn render(&self) -> String {
        let mut s = String::new();
        for (i, e) in self.events.iter().enumerate() {
            s.push_str(&format!(
                "{:>3} {:<9} {} {:<8} ${:.6} @{}..{} {}\n",
                i,
                e.kind,
                &e.key[..12.min(e.key.len())],
                if e.replayed { "replay" } else { "live" },
                e.cost,
                e.site.start,
                e.site.end,
                e.note
            ));
        }
        for w in &self.warnings {
            s.push_str(&format!("    ! {w}\n"));
        }
        s
    }
}
