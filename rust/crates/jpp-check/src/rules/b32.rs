//! B32 时延预算的静态面（原 `check.rs` 预算节）。跨度收集移到 `analysis/spans.rs`。

#![allow(unused_imports)]
use super::sites::site_of;
use super::{Cx, Hooks, Rule};
use crate::*;

/// 依据：B32（时延预算的静态面）。
pub(crate) const RULE: Rule = Rule {
    code: "B32",
    requires: &[],
    hooks: Hooks {
        before: Some(run),
        ..Hooks::NONE
    },
};

fn run(cx: &Cx) -> Vec<Diagnostic> {
    latency(cx)
}

/// **B32 时延预算的静态面**：只告警，不拒（B42 修订，Z0157）。
///
/// 估计按「每个判断站点（`judge` / `sieve`）至少一层」计；同状态的题会融合进同一层，
/// 所以这是**上界**，在循环或函数体里的站点另计为「不止一遍」。上界超预算 → `W-latency`。
/// 拒绝只由规划器的可靠下界负责（`jpp-plan/passes/plan.rs`，`E-budget-plan`：必经直线层数 × p95，
/// 按效应都成功计；J-07b 静态面）：检查器不自己按上界或「至少一层」拒。
/// 没有档案 p95 → `W-untested`。
fn latency(cx: &Cx) -> Vec<Diagnostic> {
    let (p, profile) = (cx.p, cx.profile);
    let mut out = vec![];
    let b = &p.budget;
    let Some(limit) = b.latency_p95 else {
        return out;
    };
    let mut 一次 = 0usize;
    let mut 多次 = 0usize;
    walk_block(&p.body, &mut |e| {
        if matches!(call_name(e), Some("judge") | Some("sieve")) {
            if cx.sites.repeated(site_of(e)) {
                多次 += 1;
            } else {
                一次 += 1;
            }
        }
    });
    if 一次 + 多次 == 0 {
        return out;
    }
    let Some(p95) = profile.and_then(|pr| pr.latency_p95()) else {
        out.push(Diagnostic::warning(
                "W-untested",
                format!("budget.latency_p95 = {limit}s，但没有档案的 p95 时延，估计不了层数 × p95（B32）。修法：--profile 加载档案"),
                b_span(p),
            ));
        return out;
    };
    let 上界 = (一次 + 多次) as f64 * p95;
    if 上界 > limit {
        let 缘由 = if 多次 > 0 {
            format!("{多次} 个判断站点在循环或函数体里，层数静态估不出上界；已知站点")
        } else {
            format!("{一次} 个判断站点各占一层的上界（同状态的题会融合，互斥分支只走一支）")
        };
        out.push(Diagnostic::warning(
            "W-latency",
            format!("{缘由} × p95 {p95}s = {上界:.2}s 已超时延预算 {limit}s（B32）。这只是上界，检查器不拒；必经下界超预算由规划器在计划期拒。运行期超出预算的站点会转 Unsure(latency)"),
            b_span(p),
        ));
    }
    out
}
