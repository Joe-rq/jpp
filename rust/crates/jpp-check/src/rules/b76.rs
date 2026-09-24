//! B76 题类（`附注/2026-09-24-评估①裁定.md` §六）：题式槽声明与可见结构矛盾报 `E-kind-conflict`。
//! 推断与收集在 `diag/kind.rs`；这里只做注册。

use super::{Cx, Hooks, Rule};
use crate::*;

/// 依据：B76（题类映射表与不唯一时的处理 (3)）。
pub(crate) const RULE: Rule = Rule {
    code: "B76",
    requires: &[],
    hooks: Hooks {
        after: Some(run),
        ..Hooks::NONE
    },
};

fn run(cx: &Cx) -> Vec<Diagnostic> {
    crate::diag::kind::conflicts(cx.p)
}
