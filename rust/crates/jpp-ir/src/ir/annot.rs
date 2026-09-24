//! 标注表：L3（检查器、规划）写，L4（运行时）读（`20` §2.3）。步 12a 只定义类型，步 12b 起检查器产出。

use crate::key::{EffectId, NodeId, SiteId};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

/// 效应行：一个节点或函数可能产生的效应集合。
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct EffectRow {
    /// 已知的效应（按源码名字，`judge`/`gen`/`do`/`ask`，与现行检查器的效应行同口径）
    pub known: BTreeSet<String>,
    /// 含看不透的调用（形参方法、未标注的函数值），按「可能有效应」对待
    pub open: bool,
}

impl EffectRow {
    pub fn of(effects: impl IntoIterator<Item = String>) -> EffectRow {
        EffectRow {
            known: effects.into_iter().collect(),
            open: false,
        }
    }
    pub fn has(&self, e: EffectId) -> bool {
        let n = serde_json::to_value(e).ok();
        n.as_ref()
            .and_then(|v| v.as_str())
            .is_some_and(|s| self.known.contains(s))
    }
}

/// 结构摘要 Φ（宪法登记表「效应行多态之外再加结构摘要」行）：推测许可用。
/// 步 13b 扩成可组合的过程间摘要；本步只有两位。
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Phi {
    pub may_touch_world: bool,
    pub may_refresh: bool,
}

/// 责任种类（`13` §3）：`Fnω` 可重复可丢弃；`Fn¹` 捕获了未决责任。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DutyKind {
    Omega,
    Linear,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Annot {
    pub effect: Option<EffectRow>,
    pub summary: Option<Phi>,
    pub duty_kind: Option<DutyKind>,
    pub refresh_point: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct SiteAnnot {
    pub effect: Option<EffectRow>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct AnnotTable {
    pub nodes: BTreeMap<NodeId, Annot>,
    pub sites: BTreeMap<SiteId, SiteAnnot>,
}

impl AnnotTable {
    pub fn node(&self, id: NodeId) -> Option<&Annot> {
        self.nodes.get(&id)
    }
    pub fn node_mut(&mut self, id: NodeId) -> &mut Annot {
        self.nodes.entry(id).or_default()
    }
}
