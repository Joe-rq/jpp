//! 从账本派生的只读聚合（`20` §2.3 `jpp-ledger`）：不写账本，不读校准库。

use std::collections::BTreeMap;

use jpp_value::value::Answer;

use crate::{Entry, Ledger};

/// 一条从账本派生的校准观察（无标注）：哪个校准键、标量概率、物理形式。
///
/// 与运行时交给宿主的 `Outcome.evidence` 逐条对应（同序、同键、同 `p`、同 `phys`）：两者都只来自
/// 本次运行真正发出的判断。账本条目不记置换测量（`perms`、`mode_share`），所以这里没有这两项；
/// 宿主折证据进校准记录仍用 `Outcome.evidence`，本函数供只读工具与审计用。
#[derive(Clone, Debug, PartialEq)]
pub struct Observation {
    /// 题声明的校准键（`Entry::Judge.calib_ref.declared`）
    pub calib: String,
    /// 是非题的概率；选择题、打分题没有标量 `p`，为 `None`（与 `Sample.p` 同一口径）
    pub p: Option<f64>,
    /// 物理形式（`JudgeKey.phys`）
    pub phys: String,
    /// 账本键
    pub key: String,
}

/// 派生校准观察。剔除三类条目：宿主手写的（`layer == 0`）、复用来的（`reused_from` 非空，B40 修订：
/// 不重复计入）、没有结构化键或校准引用的。顺序即账本顺序。
pub fn observations(l: &Ledger) -> Vec<Observation> {
    l.entries
        .iter()
        .filter_map(|e| match e {
            Entry::Judge {
                key,
                jkey: Some(jk),
                answer,
                calib_ref: Some(c),
                layer,
                reused_from: None,
                ..
            } if *layer > 0 => Some(Observation {
                calib: c.declared.clone(),
                p: match answer {
                    Answer::Noul(p) => Some(*p),
                    _ => None,
                },
                phys: jk.phys.clone(),
                key: key.clone(),
            }),
            _ => None,
        })
        .collect()
}

/// 按跳数（`hop`，B59）统计判断条目：`(hop, 条目数, 缺席数)`，按 `hop` 升序（验收 2 的取数函数）。
///
/// 出口不进账本（出口 = f(读数, 线)），所以「未决」在这里只数账本看得见的那一种：缺席与超时
/// （`Entry::Absent`）。复用条目不计入。
///
/// 步 17c（B84）起 `hop` 按值级来源计：经普通值、下标、`content()` 传递的依赖都算，控制流不算。
/// 模型发出的条目 `hop ≥ 1`，宿主手写条目为 0。
///
/// **缺席归跳**（17c）：缺席条目没有 `hop` 字段，按「同一状态、同一站点」的判断条目的 `hop` 归
/// （有多条取最大）；没有这样的条目记 1（B59 无父为 1）。整组缺席时同组没有判断条目，记 1 是下界；
/// 精确归跳要给 `Absent` 加字段（格式步），主会话 2026-09-24 定不走。
pub fn depth_profile(l: &Ledger) -> Vec<(u32, u64, u64)> {
    let mut hop_at: BTreeMap<(String, usize), u32> = BTreeMap::new();
    let mut table: BTreeMap<u32, (u64, u64)> = BTreeMap::new();
    for e in &l.entries {
        if let Entry::Judge {
            jkey,
            hop,
            reused_from: None,
            ..
        } = e
        {
            if let Some(k) = jkey {
                let h = hop_at.entry((k.state.clone(), k.site)).or_insert(0);
                *h = (*h).max(*hop);
            }
            table.entry(*hop).or_default().0 += 1;
        }
    }
    for e in &l.entries {
        if let Entry::Absent { jkey, .. } = e {
            let hop = jkey
                .as_ref()
                .and_then(|k| hop_at.get(&(k.state.clone(), k.site)).copied())
                .unwrap_or(1);
            table.entry(hop).or_default().1 += 1;
        }
    }
    table.into_iter().map(|(h, (n, u))| (h, n, u)).collect()
}
