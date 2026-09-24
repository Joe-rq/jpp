//! J-05 的静态面：未决必须被消费。五处：`handle` 的臂表要有能收下责任的 `unsure` 臂；出口或
//! 契约值绑定后必须再被提到；契约值不能在语句位置丢掉；函数直接带出出口时返回类型要提 Exit；
//! 捕获了责任的 `Fn¹` 不能交给高阶操作。运行期那一半（出口 `consumed` 标记，返回前核）在运行时。

#![allow(unused_imports)]
use super::{AnalysisId, CallSite, Cx, Hooks, Rule};
use crate::*;

/// 依据：12 §5 J-05（未决必须被消费；静态面见 §5 分工表「字面 match 穷尽」）。
pub(crate) const RULE: Rule = Rule {
    code: "J-05",
    requires: &[AnalysisId::Names],
    hooks: Hooks {
        scan_call: Some(fn1_to_higher_order),
        call: Some(handle_arms),
        bind: Some(exit_binding),
        stmt: Some(dropped_outcome),
        function: Some(exit_returned),
        ..Hooks::NONE
    },
};

/// Fnω 才能交给任意长度的高阶操作：捕获了责任的 Fn¹ 不可重复调用、不可丢弃
fn fn1_to_higher_order(cx: &Cx, n: &str, arguments: &[&Expr]) -> Vec<Diagnostic> {
    let names = cx.names.expect("名字趟的钩子点给出名字视图");
    let mut out = vec![];
    for i in method_positions(n) {
        let Some(arg) = arguments.get(*i) else {
            continue;
        };
        let (ExprKind::Name(f), span) = (arg.kind(), &arg.span) else {
            continue;
        };
        let Some(t) = names.annotation(f) else {
            continue;
        };
        if matches!(t.as_method(), Some((_, true))) {
            out.push(Diagnostic::error(
                "J-05",
                format!("{f} 的类型是 Fn¹（捕获了未决责任），不能交给 {n}：它可能被调用任意次、也可能一次都不调，责任会被复制或丢掉。修法：把责任用 accumulator 显式传下去，或让这个方法每次调用自己产生并处理责任（Fnω）"),
                *span,
            ));
        }
    }
    out
}

/// unsure 必须有显式一臂（通配兜不住），而且这一臂要收得下未决责任
fn handle_arms(_cx: &Cx, s: &CallSite) -> Vec<Diagnostic> {
    let mut out = vec![];
    if s.name != "handle" {
        return out;
    }
    let Some(a) = s.args.get(1) else { return out };
    let ExprKind::Record(fields) = a.kind() else {
        return out;
    };
    match fields.iter().find(|(n, _)| n == "unsure").map(|(_, v)| v) {
        None => out.push(Diagnostic::error(
            "J-05",
            "handle 的臂表没有 unsure 去向：三种题的出口都可能是 unsure，缺这一臂就是静默丢弃，otherwise 也兜不住它。修法：补 unsure: fn(u) {…}",
            a.span,
        )),
        Some(arm) => match arm.kind() {
            ExprKind::Function(f) if f.parameters.is_empty() => out.push(Diagnostic::error(
                "J-05",
                "unsure 的臂没有参数，接不到未决责任。修法：写成 unsure: fn(u) { … }",
                arm.span,
            )),
            ExprKind::Integer(_) | ExprKind::Decimal | ExprKind::Bool | ExprKind::Text(_) | ExprKind::Unit | ExprKind::List(_) | ExprKind::Record(_) => {
                out.push(Diagnostic::error(
                    "J-05",
                    "unsure 的臂是个字面量，收不下未决责任：进臂不等于销账。修法：写成 unsure: fn(u) { … }，在体内 escalate / literalize / consume(u, \"drop\")，或把 u 包进返回值",
                    arm.span,
                ));
            }
            _ => {}
        },
    }
    out
}

fn is_outcome_call(e: &Expr) -> bool {
    matches!(
        call_name(e),
        Some("sieve") | Some("pair") | Some("tally") | Some("first_k") | Some("outcome")
    )
}

/// 契约值在语句位置被丢掉
fn dropped_outcome(_cx: &Cx, e: &Expr) -> Vec<Diagnostic> {
    let mut out = vec![];
    if is_outcome_call(e) {
        out.push(Diagnostic::error(
            "J-05",
            format!("{} 的结果（契约值）在语句位置被丢掉：它的未决清单随包转移，丢掉就是静默丢弃未决（13 §3）。修法：绑定并返回它、交给下一个构造，或 consume(…, \"drop\")", call_name(e).unwrap_or("")),
            e.span,
        ));
    }
    out
}

/// 出口绑定之后在本块里再没被提到 = 静默丢弃
fn exit_binding(_cx: &Cx, value: &Expr, name: &str, span: Span, block: &Block) -> Vec<Diagnostic> {
    let mut out = vec![];
    let is_outcome = is_outcome_call(value);
    if !matches!(call_name(value), Some("cut") | Some("unsure") | Some("ask")) && !is_outcome {
        return out;
    }
    let mut used = false;
    let mut note = |e: &Expr| {
        if let ExprKind::Name(n) = e.kind() {
            if n == name {
                used = true;
            }
        }
    };
    for s in &block.statements {
        match s {
            Statement::Let {
                value: v, name: n, ..
            } if n == name && std::ptr::eq(v, value) => {}
            Statement::Let { value: v, .. } => walk_expr(v, &mut note),
            Statement::Function { function, .. } => walk_block(&function.body, &mut note),
            Statement::Expr(e) => walk_expr(e, &mut note),
        }
    }
    if let Some(r) = &block.result {
        walk_expr(r, &mut note);
    }
    if !used && is_outcome {
        out.push(Diagnostic::error(
            "J-05",
            format!("契约值 {name} 绑定之后再没被提到：它的未决清单（pending）随包转移给了你，丢掉它就是静默丢弃未决（13 §3）。修法：返回它、交给下一个构造，或 consume({name}, \"drop\") 显式丢并记账"),
            span,
        ));
    } else if !used {
        out.push(Diagnostic::error(
            "J-05",
            format!("出口 {name} 绑定之后再没被提到：未消费的 unsure 就是静默丢弃。修法：handle({name}, {{…, unsure: …}})，或 consume({name}, \"drop\") 显式丢并记账"),
            span,
        ));
    }
    out
}

/// 函数的结果就是出口名字本身，而返回类型没提 Exit
fn exit_returned(_cx: &Cx, f: &Function) -> Vec<Diagnostic> {
    let mut out = vec![];
    let Some(result) = &f.body.result else {
        return out;
    };
    let ExprKind::Name(n) = result.kind() else {
        return out;
    };
    let bound = f.body.statements.iter().any(|s| match s {
        Statement::Let { name, value, .. } => {
            name == n && matches!(call_name(value), Some("cut") | Some("unsure") | Some("ask"))
        }
        _ => false,
    });
    if !bound {
        return out;
    }
    if !f
        .result_type
        .as_ref()
        .map(|t| t.mentions("Exit"))
        .unwrap_or(false)
    {
        out.push(Diagnostic::error(
            "J-05",
            format!("函数把出口 {n} 直接带出，返回类型却没提 Exit：调用者不知道自己要消费它。修法：标注 -> Exit（或含 Exit 的类型），或在函数里 handle / consume 掉"),
            result.span,
        ));
    }
    out
}
