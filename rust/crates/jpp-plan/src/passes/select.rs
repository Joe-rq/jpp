//! 层内挑选（步 22 / B0487；`20·B43`、`20·B51` C1、`21`:291；步 30 / B0488 换上规划目标）：本层要发的组超出剩余预算时，
//! 排出发出顺序。
//!
//! 丢弃顺序是「跨状态推测 → 同状态推测 → 真站点按价值密度」，反过来就是发出顺序：真站点先发，跨状态推测后发。
//! - **真站点**按规划目标排（步 30）：开关 `critical_path` 开时先按下游层数从大到小（K-135「状态数与层数」的层数一半：
//!   关键路径上的组先拿预算，主控 B0488 Q5）；同下游层数再按价值密度（B7、B43；B 段接上，另设开关）；最后按本层登记位置。
//!   开关都关时即步 22 的占位：只按登记位置。价值密度见 [`crate::passes::plan::value_density`]（B 段）。
//! - **同状态推测**与真站点在同一次调用里（B51-C1 的判据是 `StateHash`；B155 起分组按材料，零边际的实质是共用一次
//!   请求，所以这里按「进了含真站点的那次调用」认）。在调用数预算下剥掉它们省不下调用；只有费用是约束、且有逐题
//!   单价估计时才有可省的。发出前运行时没有 usd 估计（`charge(1, 0.0)`），所以今天这一段不剥，只把位置留在
//!   [`PendingSite::same_state_spec`]（主控 Z0209 Q3；意图汇编 7c：不为省调用丢掉免费的判断）。
//! - **跨状态推测**（只含提前登记的组）自己要付一次调用，最先让出预算，组内先后同样按上面的规则。
//!
//! 调用数是约束时按组排，不除以估计的美元费用（过程记录 工程-步30 §一 (d)）：否则同题不同材料的组里短材料会排到前面，
//! 而调用数预算下每组都是一次调用。

use jpp_ir::plan::{BudgetLeft, PendingSite, Selection, SiteClass};

/// 按丢弃顺序排好发出先后，再按剩余调用数切成发出与推迟。同一类别内：`critical_path` 开时先按下游层数（大的先），
/// `value_density` 开时再按价值密度（大的先，[`crate::passes::plan::value_density`]），最后按登记位置；两个都关即步 22 占位。
pub fn select(
    layer: &[PendingSite],
    left: BudgetLeft,
    critical_path: bool,
    value_density: bool,
) -> Selection {
    let 价值: Vec<f64> = layer
        .iter()
        .map(|p| {
            if value_density {
                crate::passes::plan::value_density(p)
            } else {
                0.0
            }
        })
        .collect();
    let by = |class: SiteClass| {
        let mut v: Vec<usize> = (0..layer.len())
            .filter(|&i| layer[i].class == class)
            .collect();
        // 稳定排序：下游层数（开时）→ 价值密度（开时）→ 登记位置
        v.sort_by(|&a, &b| {
            let 层 = if critical_path {
                layer[b].downstream.cmp(&layer[a].downstream)
            } else {
                std::cmp::Ordering::Equal
            };
            层.then(
                价值[b]
                    .partial_cmp(&价值[a])
                    .unwrap_or(std::cmp::Ordering::Equal),
            )
            .then(layer[a].pos.cmp(&layer[b].pos))
        });
        v
    };
    let mut order = by(SiteClass::Real);
    order.extend(by(SiteClass::CrossStateSpec));
    let mut sel = Selection::cut(order, layer, left);
    if value_density {
        sel.value = 价值;
        sel.key_stats = layer
            .iter()
            .map(|p| p.keys.iter().map(crate::passes::plan::key_stats).collect())
            .collect();
    }
    sel
}
