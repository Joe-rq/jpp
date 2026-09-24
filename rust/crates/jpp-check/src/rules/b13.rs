//! B13 诊断层（`12` §3 J-17 后「诊断层规则集第一批」）：题面字面量的题式诊断，全部是告警。
//! 规则本体在 `diag/b13.rs`，收集题字面量在 `diag/mod.rs`；这里只做注册。

use super::{Cx, Hooks, Rule};
use crate::*;

/// 依据：B13（诊断层规则集第一批）。
pub(crate) const RULE: Rule = Rule {
    code: "B13",
    requires: &[],
    hooks: Hooks {
        after: Some(run),
        ..Hooks::NONE
    },
};

fn run(cx: &Cx) -> Vec<Diagnostic> {
    crate::diag::diagnose(cx.p)
}
