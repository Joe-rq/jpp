//! J-13：循环里的序号必须随轮次变。`do` 的 `iter_seq`、`gen` 的 `retry_seq` 在循环体内每轮不变
//! 时，第二轮起的键与第一轮相同，会被当成重放。其余参数随轮次变时键还不碰撞，降为
//! `W-seq-const`（与 Python 检查器同口径）。纯静态。

#![allow(unused_imports)]
use super::{AnalysisId, CallSite, Cx, Hooks, Rule};
use crate::*;

/// 依据：12 §5 J-13（循环里常量序号是错）。
pub(crate) const RULE: Rule = Rule {
    code: "J-13",
    requires: &[AnalysisId::Names],
    hooks: Hooks {
        call: Some(call),
        ..Hooks::NONE
    },
};

/// 带轮次序号的效应与序号实参的位置，只从效应表取：键含 `IterSeq`/`RetrySeq` 的效应，
/// 序号实参是同名输入槽（`EffectSpec::seq_slot`）。
fn seq_slot(name: &str) -> Option<(usize, &'static str)> {
    jpp_effects::ALL
        .iter()
        .map(|id| jpp_effects::spec(*id))
        .find(|s| s.name == name)
        .and_then(|s| s.seq_slot())
}

fn call(cx: &Cx, s: &CallSite) -> Vec<Diagnostic> {
    if !cx.sites.in_iteration(s.function) {
        return vec![];
    }
    match seq_slot(s.name) {
        Some((idx, field)) => seq_const(s, idx, field).into_iter().collect(),
        None => vec![],
    }
}

fn seq_const(s: &CallSite, idx: usize, field: &str) -> Option<Diagnostic> {
    let (name, args, iter_params) = (s.name, s.args, s.iter_params);
    let a = args.get(idx)?;
    // **认的是「这一轮会不会变」，不是「写没写成字面量」。**
    //
    // 原来只认 `ExprKind::Integer`，于是 `let s = 0;` 放在循环外再传进去
    // **四种写法都撞不出告警**——而那个键每轮一模一样，后果与写字面量 `0` 完全相同。
    // **「常量」是一个语义性质，不是一个语法形状。**
    let 每轮不变 = match a.kind() {
        ExprKind::Integer(_) => true,
        // 名字：不在「这一轮会变的名字」里，就是循环不变量
        ExprKind::Name(n) => !iter_params.iter().any(|p| p == n),
        _ => false,
    };
    if !每轮不变 {
        return None;
    }
    // 其它参数随轮次变时键不会碰撞，降为提示（与 Python 检查器同口径）
    let varies = args
        .iter()
        .enumerate()
        .any(|(i, x)| i != idx && iter_params.iter().any(|p| mentions(x, p)));
    Some(if varies {
        Diagnostic::warning(
            "W-seq-const",
            format!(
                "循环里的 {name} 用了常量 {field}；这一轮参数随循环变量变，键还不碰撞，但改个写法就会。修法：把轮次 i 传给 {field}"
            ),
            s.span,
        )
    } else {
        Diagnostic::error(
            "J-13",
            format!(
                "循环里的 {name} 用常量 {field}：每轮的键都一样，第二轮起会被当成重放，效应不再发生。修法：把 loop 的轮次 i 传给 {field}"
            ),
            s.span,
        )
    })
}
