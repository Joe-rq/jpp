//! J-07 预算的静态面（原 `check.rs` 预算节，只搬不改）。

#![allow(unused_imports)]
use super::{Cx, Hooks, Rule};
use crate::*;

/// 依据：12 §5 J-07（program.budget 是对总和的断言）。
pub(crate) const RULE: Rule = Rule {
    code: "J-07",
    requires: &[],
    hooks: Hooks {
        before: Some(run),
        ..Hooks::NONE
    },
};

fn run(cx: &Cx) -> Vec<Diagnostic> {
    budget(cx.p)
}

fn budget(p: &Program) -> Vec<Diagnostic> {
    let mut out = vec![];
    // 缺预算在降级处报 `J-07a`（步 12d），到这里预算必在
    let b = &p.budget;
    if b.escalate.unwrap_or(0) == 0 {
        let mut site = None;
        walk_block(&p.body, &mut |e| {
            if site.is_none() && matches!(call_name(e), Some("ask") | Some("escalate")) {
                site = Some(e.span);
            }
        });
        if let Some(span) = site {
            out.push(Diagnostic::error(
                    "J-07",
                    "程序里有 ask / escalate 而 budget 的 escalate 是 0：问人的次数上限没给，问出去只会直接挂起。修法：budget 里写 `escalate: N`",
                    span,
                ));
        }
    }
    out
}
