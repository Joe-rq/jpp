//! E7（`11` §诊断）：`map` / `filter`（`for … yield`）是纯映射，元素之间必须独立，
//! 体内不能含 `loop`、`iterate`、`stop`。纯静态。

#![allow(unused_imports)]
use super::{CallSite, Cx, Hooks, Rule};
use crate::*;

/// 依据：11 §诊断 E7（for…yield 是纯映射）。
pub(crate) const RULE: Rule = Rule {
    code: "E7",
    requires: &[],
    hooks: Hooks {
        call: Some(call),
        ..Hooks::NONE
    },
};

fn call(cx: &Cx, s: &CallSite) -> Vec<Diagnostic> {
    if !cx.sites.in_yield(s.function) {
        return vec![];
    }
    let msg = match s.name {
        "loop" => {
            "map / filter（for…yield）的体内不能含 loop：它是纯映射，元素之间必须独立。修法：整段改用 loop 或 fold"
        }
        "iterate" => {
            "map / filter（for…yield）的体内不能含 iterate：它是纯映射。修法：整段改用 iterate 或 fold"
        }
        "stop" => {
            "map / filter（for…yield）的体内不能 stop：stop 是 loop 的控制。修法：整段改用 loop 或 fold"
        }
        _ => return vec![],
    };
    vec![Diagnostic::error("E7", msg, s.span)]
}
