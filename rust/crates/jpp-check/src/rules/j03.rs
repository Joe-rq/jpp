//! J-03：线不可字面。`cut`、`test`、`select`、`measure` 的 calib 位与 `form` 选项里的 `calib`
//! 只收校准记录的键（Text），不收数字字面量。文件面（`load` 重跑认证）在校准侧。

#![allow(unused_imports)]
use super::{CallSite, Cx, Hooks, Rule};
use crate::*;

/// 依据：12 §5 J-03（线不可字面；calib 参数必须是校准记录的键）。
pub(crate) const RULE: Rule = Rule {
    code: "J-03",
    requires: &[],
    hooks: Hooks {
        call: Some(call),
        ..Hooks::NONE
    },
};

fn call(_cx: &Cx, s: &CallSite) -> Vec<Diagnostic> {
    let mut out = vec![];
    match s.name {
        "cut" | "test" | "select" => calib_literal(&mut out, s.name, s.args, 1),
        "measure" => calib_literal(&mut out, s.name, s.args, 2),
        // 题式上：calib 写在选项记录里，同样不能是数字字面量
        "form" => {
            if let Some(ExprKind::Record(fields)) = s.args.get(2).map(|a| a.kind()) {
                if let Some((_, v)) = fields.iter().find(|(k, _)| k == "calib") {
                    if matches!(
                        v.kind(),
                        ExprKind::Decimal | ExprKind::Integer(_) | ExprKind::Bool
                    ) {
                        out.push(Diagnostic::error(
                            "J-03",
                            "form 的 calib 是数字字面量：线不可字面，这一位只收校准记录的键（Text）。修法：form(…, {calib: \"校准键\"})",
                            v.span,
                        ));
                    }
                }
            }
        }
        _ => {}
    }
    out
}

fn calib_literal(out: &mut Vec<Diagnostic>, name: &str, args: &[&Expr], idx: usize) {
    let Some(a) = args.get(idx) else { return };
    if matches!(
        a.kind(),
        ExprKind::Decimal | ExprKind::Integer(_) | ExprKind::Bool
    ) {
        out.push(Diagnostic::error(
            "J-03",
            format!("{name} 的第 {} 个参数是数字字面量：线不可字面，这一位只收校准记录的键（Text）。修法：{name}(…, \"校准键\")", idx + 1),
            a.span,
        ));
    }
}
