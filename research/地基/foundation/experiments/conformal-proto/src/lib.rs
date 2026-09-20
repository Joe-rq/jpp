//! 保形弃权域的原型：**先回答「这条线能不能被认证」，再谈线定在哪**。
//!
//! 宪法借用登记表那一行（`00-宪法.md`:41）把条件写得很清楚：
//! 「可交换标注集 + 损失函数 → 阈值带风险保证」是**当年的条件**；在 Jev 下
//! 「可交换性会被打破、δ 非单调」。所以本 crate 的主出口不是一条线，是一张
//! **证书或一次拒绝**——拒绝时把「差多少才能认证」一起给出来。
//!
//! 三个部件：
//! 1. [`certify`]：给定标注集与 (α, δ)，返回带有限样本保证的线，**或拒绝**。
//! 2. [`drift`]：**无标签**漂移统计（`12`:410 那一行里唯一带「必备」的东西）。
//! 3. [`Certificate::untested_carrier`]：拒绝时该点亮 J-15 哪个载体。

use std::collections::BTreeMap;

/// 二项比例的**精确**上置信界（Clopper–Pearson）。
///
/// 为什么不用 Hoeffding：在我们的 n 上 Hoeffding 的松弛是
/// `sqrt(ln(1/δ)/2n)`——n=20 时 **0.24**，对任何低于 24% 的风险目标都是空的。
/// 精确二项界在小 n 上紧得多，而且这里的损失本来就是 0/1，用不着次高斯放缩。
pub fn binomial_upper(k: usize, n: usize, delta: f64) -> f64 {
    // **`n == 0` 返回 1.0 是承重的，不是防御性写法。** 空放行区的风险**没有定义**，
    // 返回 0.0（「零错所以零风险」）会让「全弃权」在算术上满足任何 α，于是全弃权被
    // 报成「认证通过的线」。返回 1.0 是「说不准时往拒绝那边倒」的直接实例。
    if n == 0 || k >= n {
        return 1.0;
    }
    let (mut lo, mut hi) = (k as f64 / n as f64, 1.0);
    for _ in 0..200 {
        let mid = (lo + hi) / 2.0;
        // P(Bin(n, mid) ≤ k)
        let mut cdf = 0.0;
        let mut term = (1.0 - mid).powi(n as i32);
        for i in 0..=k {
            if i > 0 {
                term *= (n - i + 1) as f64 / i as f64 * mid / (1.0 - mid);
            }
            cdf += term;
        }
        if cdf > delta { lo = mid } else { hi = mid }
    }
    (lo + hi) / 2.0
}

/// 零错时要认证到 α 所需的**放行条数**：`ln δ / ln(1−α)`。
/// 这是「就算一条都不错，样本也得有这么多」的下限，与读数准不准无关。
pub fn n_needed_zero_error(alpha: f64, delta: f64) -> usize {
    (delta.ln() / (1.0 - alpha).ln()).ceil() as usize
}

/// 一次认证的结果。**拒绝是一等出口**，不是错误。
#[derive(Clone, Debug, PartialEq)]
pub enum Certificate {
    /// 认证成功：`hi` 之上放行，风险上界 `ucb ≤ α`。
    Line { hi: f64, n_accepted: usize, n_errors: usize, ucb: f64 },
    /// **认证失败**：任何非平凡的线都给不出 ≤ α 的上界。
    /// `best_ucb` 是这批数据上能拿到的**最紧**上界（对应最保守的非空放行区）。
    Refused { best_ucb: f64, best_hi: f64, best_n_accepted: usize, n_needed: usize },
}

impl Certificate {
    /// 拒绝时该点亮的 J-15 载体名。**不新造机制**：
    /// 「这条线没被认证」与「置换没测过」「线是冷的」是同一个性质——
    /// **一个被声明为判据、但在本次路径上没有被测量的量**。
    pub fn untested_carrier(&self) -> Option<&'static str> {
        match self {
            Certificate::Line { .. } => None,
            Certificate::Refused { .. } => Some("conformal_line"),
        }
    }
    pub fn is_refused(&self) -> bool {
        matches!(self, Certificate::Refused { .. })
    }
}

/// 保形风险控制（RCPS 形状）：**从最宽的线往紧里走，取第一个上界 ≤ α 的线**。
///
/// `samples`：`(读数 p, 这条读数蕴含的判断对不对)`。损失 = 放行区里的假放行。
///
/// **空放行区不算解。** 这一条是纪律不是实现细节：`t = 1.0` 上「放行 0 条、
/// 假放行 0 条」在算术上满足任何 α，但它说的是「全弃权」——把全弃权报成
/// 「认证通过的线」，正是**结论把注意力从依据上引开**的那个形状。
pub fn certify(samples: &[(f64, bool)], alpha: f64, delta: f64) -> Certificate {
    let mut pts: Vec<(f64, bool)> = samples.to_vec();
    pts.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
    let mut ps: Vec<f64> = pts.iter().map(|x| x.0).collect();
    ps.dedup();
    let mut cands = vec![0.0];
    for w in ps.windows(2) {
        cands.push((w[0] + w[1]) / 2.0);
    }
    // **不放 1.0**：那是全弃权，不是线。（这是三道保护的第二道，见 `binomial_upper` 的 `n == 0`。）
    let mut best: Option<(f64, f64, usize)> = None; // (ucb, hi, n_accepted)
    for t in cands {
        let acc: Vec<&(f64, bool)> = pts.iter().filter(|x| x.0 >= t).collect();
        // 第三道。**实测：单独去掉这一道不会变红**——候选表里本来就没有让放行区为空的阈值。
        // 三道一起去掉才红（tests/ecal.rs 的「空放行区不算解」，6 条里红 4 条）。
        if acc.is_empty() {
            continue;
        }
        let k = acc.iter().filter(|x| !x.1).count();
        let ucb = binomial_upper(k, acc.len(), delta);
        if best.as_ref().map(|b| ucb < b.0).unwrap_or(true) {
            best = Some((ucb, t, acc.len()));
        }
        if ucb <= alpha {
            return Certificate::Line { hi: t, n_accepted: acc.len(), n_errors: k, ucb };
        }
    }
    let (best_ucb, best_hi, best_n) = best.unwrap_or((1.0, 1.0, 0));
    Certificate::Refused {
        best_ucb,
        best_hi,
        best_n_accepted: best_n,
        n_needed: n_needed_zero_error(alpha, delta),
    }
}

/// **无标签**漂移统计。`12`:410 那一行里唯一带「必备」二字的东西，而它今天两边都没有：
/// `jv/calib.py` 的 `drift_stat` 是声明了从不算的字段（全仓只有写死的 `0.0`），
/// `core/calib.py::should_suspend` 是另一个系统的、**要标签**的错误率监控。
///
/// 这里给的是只用读数分布的两个量——**不需要真值，所以每次运行都能算**：
/// - `ks`：两样本 Kolmogorov–Smirnov 统计量（读数分布位移）
/// - `psi`：population stability index（分桶质量迁移，工业上常用 0.1 / 0.25 两档）
#[derive(Clone, Debug, PartialEq)]
pub struct DriftReport {
    pub ks: f64,
    pub psi: f64,
    pub n_ref: usize,
    pub n_recent: usize,
    /// **样本够不够算这个数**。不够时上面两个数照样算得出来，但不该拿去停岗
    /// ——这一位与 J-15 同族：量本身没被**可信地**测量。
    pub underpowered: bool,
}

pub fn drift(reference: &[f64], recent: &[f64], bins: usize) -> DriftReport {
    let ks = {
        let mut a = reference.to_vec();
        let mut b = recent.to_vec();
        a.sort_by(|x, y| x.partial_cmp(y).unwrap());
        b.sort_by(|x, y| x.partial_cmp(y).unwrap());
        let mut all: Vec<f64> = a.iter().chain(b.iter()).copied().collect();
        all.sort_by(|x, y| x.partial_cmp(y).unwrap());
        all.iter()
            .map(|t| {
                let fa = a.iter().filter(|x| *x <= t).count() as f64 / a.len().max(1) as f64;
                let fb = b.iter().filter(|x| *x <= t).count() as f64 / b.len().max(1) as f64;
                (fa - fb).abs()
            })
            .fold(0.0, f64::max)
    };
    let hist = |v: &[f64]| -> Vec<f64> {
        let mut h = vec![0.0; bins];
        for x in v {
            let i = ((x * bins as f64).floor() as usize).min(bins - 1);
            h[i] += 1.0;
        }
        let n = v.len().max(1) as f64;
        h.into_iter().map(|c| c / n).collect()
    };
    let (ha, hb) = (hist(reference), hist(recent));
    let eps = 1e-4;
    let psi = ha
        .iter()
        .zip(hb.iter())
        .map(|(a, b)| {
            let (a, b) = (a.max(eps), b.max(eps));
            (b - a) * (b / a).ln()
        })
        .sum::<f64>();
    // KS 的 α=0.05 临界值 ≈ 1.36·sqrt(1/n1 + 1/n2)；小于它就分不出移没移。
    let crit = 1.36 * (1.0 / reference.len().max(1) as f64 + 1.0 / recent.len().max(1) as f64).sqrt();
    DriftReport { ks, psi, n_ref: reference.len(), n_recent: recent.len(), underpowered: ks < crit }
}

/// 把标注集按**对象段**分簇，每簇取一条——簇级保形。
///
/// 为什么需要它：可交换性在我们这里**不是被时间打破的，是被材料复用打破的**。
/// E-CAL 那 297 条读数只由 61 个不同片段重组而成；按对象段分簇后
/// noul 36 簇 / choice 37 簇 / score 28 簇。**同一段落的多条读数不是多次独立观察。**
pub fn cluster_subsample(samples: &[(f64, bool, String)], seed: u64) -> Vec<(f64, bool)> {
    let mut by: BTreeMap<&str, Vec<(f64, bool)>> = BTreeMap::new();
    for (p, l, seg) in samples {
        by.entry(seg.as_str()).or_default().push((*p, *l));
    }
    let mut s = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
    by.values()
        .map(|v| {
            s = s.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            v[(s >> 33) as usize % v.len()]
        })
        .collect()
}
