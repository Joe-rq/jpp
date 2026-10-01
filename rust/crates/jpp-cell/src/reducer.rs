//! 多写者单元与归约器（`12` §2.13 R7；原型乙 `reducer.rs`，硬伤 2 的更正）。
//!
//! 归约器分两类。保住每个写者贡献的（并集、按键合并；同键不同写者写了不同值两份都留，记「冲突」）可以声明在
//! 多写者单元上。覆盖类里，「取最新」只能用在单写者单元上（第二个写者来写就报错）；多写者单元确实要覆盖，写显式的
//! 「覆盖」，每次留下旧值与旧写者。「占用」是全有或全无的互斥：组里有成员被别人占着就整组被拒。
//! 合并顺序随先后而变，每次合并按到达顺序记事件、带版本号；重放按这个顺序合并（C3）。

use crate::value::CellValue;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Reducer {
    Union,
    MergeByKey,
    /// 取最新：只许单写者
    Latest,
    /// 显式覆盖：留旧值与旧写者
    Overwrite,
    /// 占用：写者写的是它要占的整组成员
    Claim,
}

impl Reducer {
    /// 保住每个写者贡献的归约器才能声明在多写者单元上（外加占用这个互斥原语、显式覆盖）。
    pub fn keeps_contributions(self) -> bool {
        matches!(self, Reducer::Union | Reducer::MergeByKey)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Writers {
    Single,
    Multi,
}

/// 合并事件的种类。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MergeKind {
    Merge,
    Conflict,
    Overwritten,
    ClaimGranted,
    ClaimRejected,
}

impl MergeKind {
    pub fn name(self) -> &'static str {
        match self {
            MergeKind::Merge => "merge",
            MergeKind::Conflict => "conflict",
            MergeKind::Overwritten => "overwritten",
            MergeKind::ClaimGranted => "claim_granted",
            MergeKind::ClaimRejected => "claim_rejected",
        }
    }
}

/// 一次合并（按合并顺序）。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MergeEvent {
    pub seq: usize,
    pub kind: MergeKind,
    pub writer: String,
    pub key: String,
    /// 写入值的内容哈希（占用为空）
    pub hash: String,
    /// 覆盖：旧写者与旧值哈希
    pub old: Option<(String, String)>,
    /// 占用：申请的组、释放的成员、占着冲突成员的写者
    pub members: Vec<String>,
    pub released: Vec<String>,
    pub held_by: Vec<String>,
}

/// 读多写者单元某个键得到的东西。
#[derive(Clone, Debug)]
pub enum SharedRead<V> {
    Empty,
    One(V),
    /// 并集的多个值，或按键合并时同键不同写者的冲突值（按写者排序），读者看得见
    Many(Vec<(String, V)>),
}

impl<V> SharedRead<V> {
    pub fn values(&self) -> Vec<&V> {
        match self {
            SharedRead::Empty => vec![],
            SharedRead::One(v) => vec![v],
            SharedRead::Many(xs) => xs.iter().map(|x| &x.1).collect(),
        }
    }
}

/// 一个写者最近一次占用申请的结果。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClaimState {
    pub members: Vec<String>,
    pub granted: bool,
    pub held_by: Vec<String>,
    /// 被拒的申请：冲突成员现在还被别人占着吗（占着的一方释放后，被拒方据此重新申请）
    pub still_held_by: Vec<String>,
}

/// 多写者单元的状态（归约器的当前值）。
#[derive(Clone)]
pub struct Cell<V> {
    pub reducer: Reducer,
    pub writers: Writers,
    first_writer: Option<String>,
    /// 按键：写者与值（并集按值哈希去重，按键合并与覆盖见各自规则）
    entries: BTreeMap<String, Vec<(String, V)>>,
    /// 占用：成员 → 占着它的写者
    owner: BTreeMap<String, String>,
    /// 占用：写者 → (组, 是否占上, 冲突时占着的写者)
    claims: BTreeMap<String, (Vec<String>, bool, Vec<String>)>,
    pub events: Vec<MergeEvent>,
}

/// 声明。多写者上声明「取最新」直接拒掉。
pub fn declare<V>(reducer: Reducer, writers: Writers) -> Result<Cell<V>, String> {
    if writers == Writers::Multi && reducer == Reducer::Latest {
        return Err("取最新只能用在单写者单元上；多写者单元用保住贡献的归约器（并集、按键合并），确实要覆盖就写显式的「覆盖」".into());
    }
    Ok(Cell {
        reducer,
        writers,
        first_writer: None,
        entries: BTreeMap::new(),
        owner: BTreeMap::new(),
        claims: BTreeMap::new(),
        events: vec![],
    })
}

impl<V: CellValue> Cell<V> {
    /// 合并一次写。返回有没有变（同一写者重复写同样的值不算一次合并）。
    pub fn write(&mut self, writer: &str, key: &str, v: V) -> Result<bool, String> {
        let seq = self.events.len();
        if self.writers == Writers::Single {
            match &self.first_writer {
                None => self.first_writer = Some(writer.to_string()),
                Some(w) if w != writer => {
                    return Err(format!(
                        "单写者单元被第二个写者 {writer} 写（第一个是 {w}）"
                    ));
                }
                _ => {}
            }
        }
        let h = v.content_hash();
        let ev = |kind, old| MergeEvent {
            seq,
            kind,
            writer: writer.to_string(),
            key: key.to_string(),
            hash: h.clone(),
            old,
            members: vec![],
            released: vec![],
            held_by: vec![],
        };
        match self.reducer {
            Reducer::Claim => Err("占用单元只经 claim 写".into()),
            Reducer::Union => {
                let e = self.entries.entry(key.to_string()).or_default();
                if e.iter().any(|(_, old)| old.content_hash() == h) {
                    return Ok(false);
                }
                e.push((writer.to_string(), v));
                e.sort_by(|a, b| (a.1.content_hash(), &a.0).cmp(&(b.1.content_hash(), &b.0)));
                self.events.push(ev(MergeKind::Merge, None));
                Ok(true)
            }
            Reducer::MergeByKey => {
                let e = self.entries.entry(key.to_string()).or_default();
                if e.iter()
                    .any(|(w, old)| w == writer && old.content_hash() == h)
                {
                    return Ok(false);
                }
                let conflict = e
                    .iter()
                    .any(|(w, old)| w != writer && old.content_hash() != h);
                e.retain(|(w, _)| w != writer);
                e.push((writer.to_string(), v));
                e.sort_by(|a, b| a.0.cmp(&b.0));
                self.events.push(ev(
                    if conflict {
                        MergeKind::Conflict
                    } else {
                        MergeKind::Merge
                    },
                    None,
                ));
                Ok(true)
            }
            Reducer::Latest | Reducer::Overwrite => {
                let e = self.entries.entry(key.to_string()).or_default();
                if e.last()
                    .is_some_and(|(w, old)| w == writer && old.content_hash() == h)
                {
                    return Ok(false);
                }
                let old = e.pop();
                e.push((writer.to_string(), v));
                match (self.reducer, old) {
                    (Reducer::Overwrite, Some((ow, ov))) => self
                        .events
                        .push(ev(MergeKind::Overwritten, Some((ow, ov.content_hash())))),
                    _ => self.events.push(ev(MergeKind::Merge, None)),
                }
                Ok(true)
            }
        }
    }

    /// 占用申请：全有或全无。返回有没有变（同一写者、同一组、同样结果不是新的合并，否则读者被标脏、重跑、再写，停不下来）。
    pub fn claim(&mut self, writer: &str, group: &[String]) -> bool {
        let seq = self.events.len();
        let mut group = group.to_vec();
        group.sort();
        group.dedup();
        let held: BTreeSet<String> = group
            .iter()
            .filter_map(|m| self.owner.get(m))
            .filter(|w| *w != writer)
            .cloned()
            .collect();
        let held: Vec<String> = held.into_iter().collect();
        let granted = held.is_empty();
        if self
            .claims
            .get(writer)
            .is_some_and(|c| c.0 == group && c.1 == granted && c.2 == held)
        {
            return false;
        }
        let mut released = vec![];
        if granted {
            released = self
                .owner
                .iter()
                .filter(|(m, w)| *w == writer && !group.contains(m))
                .map(|(m, _)| m.clone())
                .collect();
            for m in &released {
                self.owner.remove(m);
            }
            for m in &group {
                self.owner.insert(m.clone(), writer.to_string());
            }
        }
        self.events.push(MergeEvent {
            seq,
            kind: if granted {
                MergeKind::ClaimGranted
            } else {
                MergeKind::ClaimRejected
            },
            writer: writer.to_string(),
            key: writer.to_string(),
            hash: String::new(),
            old: None,
            members: group.clone(),
            released,
            held_by: held.clone(),
        });
        self.claims
            .insert(writer.to_string(), (group, granted, held));
        true
    }

    /// 写者最近一次占用申请的结果；没申请过为 `None`。
    pub fn claim_of(&self, writer: &str) -> Option<ClaimState> {
        let (g, ok, held) = self.claims.get(writer)?;
        let still: BTreeSet<String> = g
            .iter()
            .filter_map(|m| self.owner.get(m))
            .filter(|w| *w != writer)
            .cloned()
            .collect();
        Some(ClaimState {
            members: g.clone(),
            granted: *ok,
            held_by: held.clone(),
            still_held_by: still.into_iter().collect(),
        })
    }

    /// 占用单元里各成员现在被谁占着。
    pub fn holders(&self) -> &BTreeMap<String, String> {
        &self.owner
    }

    /// 读一个键。
    pub fn read(&self, key: &str) -> SharedRead<V> {
        match self.entries.get(key) {
            None => SharedRead::Empty,
            Some(e) if e.is_empty() => SharedRead::Empty,
            Some(e) if e.len() == 1 => SharedRead::One(e[0].1.clone()),
            Some(e) => SharedRead::Many(e.clone()),
        }
    }
}
