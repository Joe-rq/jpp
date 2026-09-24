//! 预算块解析（`20` §2.3 `lower`：「`budget` 块解析进 `Program.budget`，缺字段报 `J-07a`」）。
//! 步 12d 从原 `lower.rs` 搬来，只改产出类型与缺字段的规则号。

use crate::{Diagnostic, ast as a};
use jpp_ir::ir;

pub(super) fn budget(expr: &a::Expr) -> Result<ir::Budget, Diagnostic> {
    let a::ExprKind::Record(fields) = &expr.kind else {
        return Err(Diagnostic::new(
            "budget must be a record of literal limits",
            expr.span,
        ));
    };
    let mut calls = None;
    let mut cost = None;
    let mut depth = None;
    let mut escalate = None;
    let mut unsure = None;
    let mut latency_p95 = None;
    let mut absent = None;
    let num = |value: &a::Expr| -> Option<f64> {
        match value.kind {
            a::ExprKind::Integer(n) => Some(n as f64),
            a::ExprKind::Decimal(n) => Some(n),
            _ => None,
        }
    };
    for (name, value) in fields {
        let invalid = || Diagnostic::new(format!("invalid budget limit '{name}'"), value.span);
        match name.as_str() {
            "calls" | "depth" | "escalate" => {
                let a::ExprKind::Integer(n) = value.kind else {
                    return Err(invalid());
                };
                let n = u64::try_from(n).map_err(|_| invalid())?;
                match name.as_str() {
                    "calls" => calls = Some(n),
                    "depth" => depth = Some(u32::try_from(n).map_err(|_| invalid())?),
                    _ => escalate = Some(n),
                }
            }
            "cost" => {
                let n = match value.kind {
                    a::ExprKind::Integer(n) => n as f64,
                    a::ExprKind::Decimal(n) => n,
                    _ => return Err(invalid()),
                };
                if !n.is_finite() || n < 0.0 {
                    return Err(invalid());
                }
                cost = Some(n);
            }
            // J-10 的 unsure 预算（只报不停）
            "unsure" => {
                let n = num(value)
                    .filter(|n| n.is_finite() && *n >= 0.0)
                    .ok_or_else(invalid)?;
                unsure = Some(n);
            }
            // B32 时延预算（秒）
            "latency_p95" => {
                let n = num(value)
                    .filter(|n| n.is_finite() && *n > 0.0)
                    .ok_or_else(invalid)?;
                latency_p95 = Some(n);
            }
            // B32 判断力缺席策略 {retry, backoff, then, breaker}
            "absent" => {
                let a::ExprKind::Record(fs) = &value.kind else {
                    return Err(invalid());
                };
                let mut retry = 2u32;
                let mut backoff = 1.0f64;
                let mut then = "escalate".to_string();
                let mut breaker = 3u32;
                for (k, v) in fs {
                    let bad = || Diagnostic::new(format!("invalid absent field '{k}'"), v.span);
                    match k.as_str() {
                        "retry" => {
                            retry = match v.kind {
                                a::ExprKind::Integer(n) if n >= 0 => n as u32,
                                _ => return Err(bad()),
                            }
                        }
                        "breaker" => {
                            breaker = match v.kind {
                                a::ExprKind::Integer(n) if n >= 1 => n as u32,
                                _ => return Err(bad()),
                            }
                        }
                        "backoff" => {
                            backoff = num(v)
                                .filter(|n| n.is_finite() && *n >= 0.0)
                                .ok_or_else(bad)?
                        }
                        "then" => {
                            then = match &v.kind {
                                a::ExprKind::Text(t)
                                    if matches!(
                                        t.as_str(),
                                        "escalate" | "conservative" | "fail"
                                    ) =>
                                {
                                    t.clone()
                                }
                                _ => {
                                    return Err(Diagnostic::new(
                                        "absent.then must be \"escalate\", \"conservative\" or \"fail\"",
                                        v.span,
                                    ));
                                }
                            }
                        }
                        _ => {
                            return Err(Diagnostic::new(
                                format!("unknown absent field '{k}'"),
                                v.span,
                            ));
                        }
                    }
                }
                absent = Some(ir::AbsentPolicy {
                    retry,
                    backoff,
                    then,
                    breaker,
                });
            }
            _ => {
                return Err(Diagnostic::new(
                    format!("unknown budget field '{name}'"),
                    value.span,
                ));
            }
        }
    }
    // 依据：J-07（12 §5）；20 §2.3 `lower`「`budget` 块缺字段在这里报 `J-07a`」
    Ok(ir::Budget {
        calls: calls.ok_or_else(|| Diagnostic::new("J-07a: budget requires 'calls'", expr.span))?,
        cost: cost.ok_or_else(|| Diagnostic::new("J-07a: budget requires 'cost'", expr.span))?,
        depth,
        escalate,
        unsure,
        absent,
        latency_p95,
    })
}
