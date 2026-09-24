//! 值级来源标签（B84，步 17c）：`Provenance = (taint, sources)`。
//!
//! taint 是 B33 的内容来源属性（有人为内容担保吗），sources 是直接来源读数的账本键集合（B59 的跳、
//! B45 派生题、谱系放行都读它）。两者是同一标签的两个分量，沿同一张边界表（`20` §3.10）传播，
//! 合并只在 [`join`] 一处：taint 取 ∨，sources 取 ∪。只有「按计算键取下标或字段」一行两者不同：
//! sources 并入键的 sources，taint 取元素自身的位（B33 第 3 条）。控制流两者都不传播。
//!
//! sources 只记直接来源（保一跳，不做闭包）；更早的祖先经账本 `Entry::Judge.parents` 链按需算。
//! 依据：B84（`地基/附注/2026-09-24-B83续接缺口裁定.md` §二）。

use std::collections::BTreeSet;
use std::rc::Rc;

use crate::value::Taint;

/// 直接来源读数的账本键集合。`None` = 空集（不分配）。排序去重（D13.5：传播确定，重放逐字节）。
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Sources(Option<Rc<BTreeSet<String>>>);

impl Sources {
    pub fn empty() -> Sources {
        Sources(None)
    }
    /// 一个账本键；空键即空集（预算未观察的出口没有键）。
    pub fn from_key(k: &str) -> Sources {
        if k.is_empty() {
            return Sources(None);
        }
        Sources(Some(Rc::new(BTreeSet::from([k.to_string()]))))
    }
    pub fn from_set(s: BTreeSet<String>) -> Sources {
        let s: BTreeSet<String> = s.into_iter().filter(|k| !k.is_empty()).collect();
        if s.is_empty() {
            Sources(None)
        } else {
            Sources(Some(Rc::new(s)))
        }
    }
    pub fn is_empty(&self) -> bool {
        self.0.is_none()
    }
    pub fn iter(&self) -> impl Iterator<Item = &String> {
        self.0.iter().flat_map(|s| s.iter())
    }
    pub fn to_set(&self) -> BTreeSet<String> {
        self.iter().cloned().collect()
    }
    /// ∪。任一侧为空时共享另一侧，不分配。
    pub fn union(&self, other: &Sources) -> Sources {
        match (&self.0, &other.0) {
            (None, _) => other.clone(),
            (_, None) => self.clone(),
            (Some(a), Some(b)) => {
                if Rc::ptr_eq(a, b) || b.is_subset(a) {
                    self.clone()
                } else if a.is_subset(b) {
                    other.clone()
                } else {
                    Sources(Some(Rc::new(a.union(b).cloned().collect())))
                }
            }
        }
    }
}

/// 值级来源标签：B33 的 taint 与 B59/B84 的 sources。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Provenance {
    pub taint: Taint,
    pub sources: Sources,
}

impl Provenance {
    /// 程序自己造的值：trusted、无来源读数。
    pub fn trusted() -> Provenance {
        Provenance {
            taint: Taint::Trusted,
            sources: Sources::empty(),
        }
    }
    pub fn new(taint: Taint, sources: Sources) -> Provenance {
        Provenance { taint, sources }
    }
    /// 只有来源、taint 取 trusted（∨ 的单位元）：用于「sources 并入、taint 不并」的边。
    pub fn sources_only(sources: Sources) -> Provenance {
        Provenance {
            taint: Taint::Trusted,
            sources,
        }
    }
    /// join 的单位元吗（trusted 且无来源）：是则 `with_prov` 原样返回。
    pub fn is_unit(&self) -> bool {
        self.taint == Taint::Trusted && self.sources.is_empty()
    }
}

impl From<Taint> for Provenance {
    fn from(t: Taint) -> Provenance {
        Provenance {
            taint: t,
            sources: Sources::empty(),
        }
    }
}

/// 唯一合并处：taint ∨、sources ∪。
pub fn join(a: &Provenance, b: &Provenance) -> Provenance {
    Provenance {
        taint: Taint::join(a.taint, b.taint),
        sources: a.sources.union(&b.sources),
    }
}
