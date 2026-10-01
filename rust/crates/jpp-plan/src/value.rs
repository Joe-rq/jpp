//! 一道题的价值：B7 与 B43 已定的效用族，写在这里一处，规划器（B0488，步 30）与出题库 derive（B0468，步 28）共用；
//! 两处的入口按主会话裁定四十三分开：
//!
//! - **规划器**（[`channel_value`]）：层内挑选用，无记录为 0——规划器问的是「现在值不值得为这道题付钱」，冷题超预算先砍；
//!   冷题发不发由校准预算管，不由价值管（裁定三十八、四十三）。
//! - **闸门**（[`gate_info`]）：derive 验题闸门第④段用。有记录的候选与规划器同一个值；无记录的取题库同题类已认证记录的最低
//!   折扣（[`record_discount`]）× 期望熵降，没有同题类取全库最低，全库为空只按熵——无记录落不利侧但不为 0（B39）。
//!
//! **第二版（裁定四十六；过程记录 工程-步30 §7.9）**：第一版「期望熵降 × (1 − u)(1 − H_b(ε))」在题库三条是非题式上与精确
//! 互信息排序不一致 1/3，超过裁定三十八写的两成，按事先的判据换成第二版：**价值 = 记录混淆矩阵上的互信息（均匀先验）**。
//! 混淆矩阵每格 +0.5（Jeffreys 先验），否则 40 条 0 错会被读成无噪信道。证书 α 只是认证保证，价值不读它；`unsure_rate` 字段
//! 也不读，未决比例从同一张混淆矩阵来。
//!
//! - 是非题的记录（带标注样本是 (p, 真值)）：2 × 3 混淆矩阵（真是 / 真否 × act、ignore、unsure），精确算（[`Channel::Binary`]）。
//! - K 选一与打分的记录（样本只有 (p_max, 对错)）：**对称错误近似**（[`Channel::Symmetric`]）——已决且对 c、已决且错 w、
//!   未决 u 三格，错的均摊到 K − 1 个错误标签：I = −(c+w)·log₂((c+w)/K) + c·log₂c + w·log₂(w/(K−1))。这是近似；三格各加 1，
//!   伪计数总量与精确形式（六格各 0.5）相同。在三条是非题式上它比精确值低 0.003–0.010 比特、排序一致（过程记录 §10.1）。
//!
//! 效用族（`20` v2 附录 B43）的「按请求的选择规模计的期望熵降」仍在：它是互信息的上界，也是闸门无记录时的乘数：
//!
//! | 请求 | 选择规模 | 期望熵降（比特） |
//! |---|---|---|
//! | `whether`（是非） | 两块 | 1 |
//! | `one`（K 选一） | K 块 | log₂K |
//! | `degree`（m 档） | m 块 | log₂m |
//! | `all`（K 项各自是非） | 2^K 种 | K |
//!
//! 本趟更新（层边界重规划，K-135；施工者提议）：记录平滑后的未决比例 u₀、总数 n；本趟该键 seen、unsure 条，
//! u′ = (u₀·n + unsure) / (n + seen)，把每一行的未决概率换成 u′、已决格按比例缩放。seen = 0 时不替换。
//!
//! 切分点（B7 后半、宪法 :68）：[`channel_capacity`] 在校准后的信道上求 u*，不默认一半（Jedynak 2012）。

use jpp_ir::plan::Channel;
use jpp_ir::question_kind::Request;

/// Jeffreys 先验：是非记录 2 × 3 混淆矩阵每格加的伪计数（裁定四十六），合计 3
pub const JEFFREYS: f64 = 0.5;
/// 对称近似三格每格的伪计数：与精确形式的伪计数总量取齐（3 格 × 1 = 6 格 × 0.5 = 3），否则近似平滑得轻、系统性偏高
/// （复核 B0488-B 缺口 13；过程记录 工程-步30 §10.1 第 3 条）
pub const SYM_PSEUDO: f64 = 1.0;

/// 一道题在问之前的期望熵降（比特）。`k` 是选择规模：K 选一的候选数、打分的档数；是非题不看 `k`。
/// 选择规模小于 2 的划分是平凡的（只有一块），信息值为 0。
pub fn expected_entropy_drop(request: Request, k: usize) -> f64 {
    match request {
        Request::Whether => 1.0,
        Request::One | Request::Degree => {
            if k < 2 {
                0.0
            } else {
                (k as f64).log2()
            }
        }
        Request::All => k as f64,
    }
}

/// 二元熵 H_b(p)（比特）；p 夹在 [0, 1]，端点为 0
pub fn binary_entropy(p: f64) -> f64 {
    let p = p.clamp(0.0, 1.0);
    let h = |x: f64| if x <= 0.0 { 0.0 } else { -x * x.log2() };
    h(p) + h(1.0 - p)
}

/// 离散分布的熵（比特）
fn entropy(p: &[f64]) -> f64 {
    p.iter()
        .map(|&x| if x <= 0.0 { 0.0 } else { -x * x.log2() })
        .sum()
}

/// 已知信道上一次提问的互信息 φ(u)（Jedynak 等 2012）：真值为是的先验质量 u，`f1`、`f0` 是真值为是、为否时各出口的比例
pub fn channel_info(u: f64, f1: &[f64], f0: &[f64]) -> f64 {
    let mix: Vec<f64> = f1
        .iter()
        .zip(f0)
        .map(|(a, b)| u * a + (1.0 - u) * b)
        .collect();
    entropy(&mix) - u * entropy(f1) - (1.0 - u) * entropy(f0)
}

/// 切分点与信道容量（B7 后半）：在 [0, 1] 上求 φ(u) 的最大（φ 凹，三分法到 1e-9），返回 (u*, C)。对称信道 u* = 1/2；
/// 不对称信道不是一半，候选集应按后验质量 u* 切
pub fn channel_capacity(f1: &[f64], f0: &[f64]) -> (f64, f64) {
    let (mut lo, mut hi) = (0.0f64, 1.0f64);
    while hi - lo > 1e-9 {
        let a = lo + (hi - lo) / 3.0;
        let b = hi - (hi - lo) / 3.0;
        if channel_info(a, f1, f0) < channel_info(b, f1, f0) {
            lo = a;
        } else {
            hi = b;
        }
    }
    let u = (lo + hi) / 2.0;
    (u, channel_info(u, f1, f0).max(0.0))
}

/// 本趟读数更新未决比例：把记录（平滑后）的总数 n 当先验，本趟该键已切 `seen` 条、其中未决 `unsure` 条
pub fn updated_unsure(u: f64, n: f64, seen: u64, unsure: u64) -> f64 {
    let d = n + seen as f64;
    if d <= 0.0 {
        return u;
    }
    (u * n + unsure as f64) / d
}

/// 平滑后的未决比例与总数（报告与本趟更新用）
pub fn unsure_stats(ch: &Channel) -> (f64, f64) {
    match ch {
        Channel::Binary { n } => {
            let tot: f64 = n.iter().flatten().sum::<f64>() + 6.0 * JEFFREYS;
            ((n[0][2] + n[1][2] + 2.0 * JEFFREYS) / tot, tot)
        }
        Channel::Symmetric { c, w, u } => {
            let tot = c + w + u + 3.0 * SYM_PSEUDO;
            ((u + SYM_PSEUDO) / tot, tot)
        }
    }
}

/// 一行平滑后的出口比例（每格加 `pseudo`）；`u_new` 给了就把未决概率换成它、已决格按比例缩放
fn row(cells: &[f64], pseudo: f64, u_new: Option<f64>) -> Vec<f64> {
    let m: Vec<f64> = cells.iter().map(|x| x + pseudo).collect();
    let t: f64 = m.iter().sum();
    let mut f: Vec<f64> = m.iter().map(|x| x / t).collect();
    if let Some(u) = u_new {
        let last = f.len() - 1;
        let dec: f64 = f[..last].iter().sum();
        let k = if dec > 0.0 { (1.0 - u) / dec } else { 0.0 };
        for x in &mut f[..last] {
            *x *= k;
        }
        f[last] = u;
    }
    f
}

/// 是非记录的精确互信息（均匀先验，Jeffreys 平滑）
pub fn confusion_value(n: &[[f64; 3]; 2], u_new: Option<f64>) -> f64 {
    let f1 = row(&n[0], JEFFREYS, u_new);
    let f0 = row(&n[1], JEFFREYS, u_new);
    channel_info(0.5, &f1, &f0)
}

/// 是非记录的切分点与信道容量（B7 后半）：混淆矩阵按 Jeffreys 平滑（与价值同口径），再按 [`channel_capacity`] 求 u*
pub fn split_point(n: &[[f64; 3]; 2]) -> (f64, f64) {
    channel_capacity(&row(&n[0], JEFFREYS, None), &row(&n[1], JEFFREYS, None))
}

/// 对称错误近似的互信息（K 个标签均匀先验，错的均摊到 K − 1 个错误标签；三格各加 [`SYM_PSEUDO`]，伪计数总量与精确形式相同）
pub fn symmetric_value(c: f64, w: f64, u: f64, k: usize, u_new: Option<f64>) -> f64 {
    if k < 2 {
        return 0.0;
    }
    let f = row(&[c, w, u], SYM_PSEUDO, u_new);
    let (c, w) = (f[0], f[1]);
    let cw = c + w;
    let kk = k as f64;
    let t = |x: f64, y: f64| if x > 0.0 { x * y.log2() } else { 0.0 };
    (-t(cw, cw / kk) + t(c, c) + t(w, w / (kk - 1.0))).max(0.0)
}

/// 把是非记录折成对称近似的三格（已决且对、已决且错、未决）
fn collapse(ch: &Channel) -> (f64, f64, f64) {
    match ch {
        Channel::Binary { n } => (n[0][0] + n[1][1], n[0][1] + n[1][0], n[0][2] + n[1][2]),
        Channel::Symmetric { c, w, u } => (*c, *w, *u),
    }
}

/// 规划器的价值（第二版）：记录的互信息；是非题配是非记录精确算，其余按对称近似（K 取这道题的选择规模；`all` 按 K 道
/// 是非题各一份）。`seen`、`unsure` 是本趟该键的计数，seen = 0 时不替换未决比例
///
/// 这就是主会话裁定四十三所称的 `planner_value`（入口名不改，主控 Z0378）；第二版的依据是裁定四十六。
pub fn channel_value(request: Request, k: usize, ch: &Channel, seen: u64, unsure: u64) -> f64 {
    let u_new = if seen > 0 {
        let (u0, n) = unsure_stats(ch);
        Some(updated_unsure(u0, n, seen, unsure))
    } else {
        None
    };
    match (request, ch) {
        (Request::Whether, Channel::Binary { n }) => confusion_value(n, u_new),
        (Request::Whether, _) => {
            let (c, w, u) = collapse(ch);
            symmetric_value(c, w, u, 2, u_new)
        }
        (Request::One | Request::Degree, _) => {
            let (c, w, u) = collapse(ch);
            symmetric_value(c, w, u, k, u_new)
        }
        (Request::All, _) => k as f64 * channel_value(Request::Whether, 0, ch, seen, unsure),
    }
}

/// 本趟更新后的未决比例（报告 `u_run`）
pub fn run_unsure(ch: &Channel, seen: u64, unsure: u64) -> f64 {
    let (u0, n) = unsure_stats(ch);
    if seen == 0 {
        u0
    } else {
        updated_unsure(u0, n, seen, unsure)
    }
}

/// 一条已认证记录作为「最低折扣」候选时的折扣（裁定四十三 + 四十六）：记录的互信息 ÷ 它自身形态的熵。是非记录按精确互信息
/// （熵 1）；K 选一与打分记录没有 K，按对称近似取 K = 2
pub fn record_discount(ch: &Channel) -> f64 {
    match ch {
        Channel::Binary { n } => confusion_value(n, None),
        Channel::Symmetric { c, w, u } => symmetric_value(*c, *w, *u, 2, None),
    }
}

/// 闸门的信息值（裁定四十三）：期望熵降 × 闸门折扣；折扣 `None` 即只按熵
pub fn gate_info(request: Request, k: usize, discount: Option<f64>) -> f64 {
    expected_entropy_drop(request, k) * discount.map_or(1.0, |d| d.clamp(0.0, 1.0))
}

/// 按信息值从大到小排候选的下标；相等时保持原顺序（稳定）。
pub fn rank(values: &[f64]) -> Vec<usize> {
    let mut idx: Vec<usize> = (0..values.len()).collect();
    idx.sort_by(|a, b| {
        values[*b]
            .partial_cmp(&values[*a])
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    idx
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 期望熵降按请求规模() {
        assert_eq!(expected_entropy_drop(Request::Whether, 0), 1.0);
        assert!((expected_entropy_drop(Request::One, 3) - 3f64.log2()).abs() < 1e-12);
        assert_eq!(expected_entropy_drop(Request::Degree, 4), 2.0);
        assert_eq!(expected_entropy_drop(Request::All, 5), 5.0);
        assert_eq!(
            expected_entropy_drop(Request::One, 1),
            0.0,
            "只有一块的划分没有信息"
        );
    }

    #[test]
    fn 闸门无折扣只按熵() {
        assert_eq!(gate_info(Request::Whether, 0, None), 1.0);
        assert!((gate_info(Request::Whether, 0, Some(0.25)) - 0.25).abs() < 1e-12);
    }

    #[test]
    fn 排序稳定且从大到小() {
        // 示例二的打分：K 选一三项胜过是非题；两道是非题打平时保持原顺序
        let v = [
            gate_info(Request::Whether, 0, None),
            gate_info(Request::One, 3, None),
            gate_info(Request::Whether, 0, None),
        ];
        assert_eq!(rank(&v), vec![1, 0, 2]);
    }
}
