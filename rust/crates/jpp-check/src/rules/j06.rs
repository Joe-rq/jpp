//! J-06 / E5 有界循环的静态面：`loop`、`iterate` 必带正整数 bound；bound 不是字面量时告警
//! `W-bound`（静态估不出上界，只有运行期能核）。运行期那一半（键重复即停）在运行时。

#![allow(unused_imports)]
use super::{CallSite, Cx, Hooks, Rule};
use crate::*;

/// 依据：12 §5 J-06（loop 必带 bound；静态面判存在性）；11 §诊断 E5。
pub(crate) const RULE: Rule = Rule {
    code: "J-06",
    requires: &[],
    hooks: Hooks {
        call: Some(call),
        ..Hooks::NONE
    },
};

fn call(_cx: &Cx, s: &CallSite) -> Vec<Diagnostic> {
    let mut out = vec![];
    let (args, span) = (s.args, s.span);
    match s.name {
        // E5：有界循环必带 bound
        "loop" => {
            if args.len() != 3 {
                out.push(Diagnostic::error(
                    "J-06",
                    format!("loop 缺 bound：有界循环必须带 bound，要写成 loop(bound, 初值, fn(acc, i))，这里给了 {} 个参数", args.len()),
                    span,
                ));
            } else {
                match args[0].kind() {
                    ExprKind::Integer(n) if n > 0 => {}
                    ExprKind::Integer(n) => out.push(Diagnostic::error(
                        "J-06",
                        format!("loop 的 bound 是 {n}：bound 必须是正整数，否则循环体一次都不跑"),
                        args[0].span,
                    )),
                    _ => out.push(Diagnostic::warning(
                        "W-bound",
                        "loop 的 bound 不是字面量：静态估不出上界，只有运行期能核。修法：写成整数字面量",
                        args[0].span,
                    )),
                }
            }
        }
        // E5 / J-06：iterate 同样是有界循环，bound 必带
        "iterate" => {
            if args.len() != 4 {
                out.push(Diagnostic::error(
                    "J-06",
                    format!("iterate 要写成 iterate(bound, 初值, fn(acc, i), measure)：measure 是 fn(acc) -> Int 或 \"tokens\"，这里给了 {} 个参数", args.len()),
                    span,
                ));
            } else if let ExprKind::Integer(n) = args[0].kind() {
                if n <= 0 {
                    out.push(Diagnostic::error(
                        "J-06",
                        format!("iterate 的 bound 是 {n}：bound 必须是正整数"),
                        args[0].span,
                    ));
                }
            } else {
                out.push(Diagnostic::warning("W-bound", "iterate 的 bound 不是字面量：静态估不出上界，只有运行期能核。修法：写成整数字面量", args[0].span));
            }
        }
        _ => {}
    }
    out
}
