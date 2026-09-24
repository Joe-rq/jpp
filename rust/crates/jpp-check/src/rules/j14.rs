//! J-14 的静态面：`state` 的 on 槽恰一个判断对象（关系用一对）。字面列表超过两个即报。

#![allow(unused_imports)]
use super::{CallSite, Cx, Hooks, Rule};
use crate::*;

/// 依据：12 §5 J-14（状态 on 恰一个判断对象）。
pub(crate) const RULE: Rule = Rule {
    code: "J-14",
    requires: &[],
    hooks: Hooks {
        call: Some(call),
        ..Hooks::NONE
    },
};

fn call(_cx: &Cx, s: &CallSite) -> Vec<Diagnostic> {
    let mut out = vec![];
    if s.name != "state" {
        return out;
    }
    if let Some(ExprKind::List(items)) = s.args.first().map(|a| a.kind()) {
        if items.len() > 2 {
            out.push(Diagnostic::error(
                "J-14",
                format!(
                    "state 的 on 槽放了 {} 个对象：一题一对象（关系用一对）。修法：逐个对象建状态，或把它们放进 over 槽当候选",
                    items.len()
                ),
                s.args[0].span,
            ));
        }
    }
    out
}
