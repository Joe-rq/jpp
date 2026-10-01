//! `E-unsure-cause`（B197，步 36 G3）：`unsure(text)` 的 `text` 必须是十六种未决原因之一。
//!
//! 只核文字字面量；实参不是字面量的交运行期（`host_builtins/unsure.rs::b_unsure` 报同一个码）。传未决值重新包装的
//! `unsure(u)` 不在此列。
//!
//! 依据：B197（地基/附注/2026-09-30-骨架v1落地前提-裁定.md §B197）；`12` §2.3 第 188 行；过程记录
//! `地基/过程记录/工程-G3-原因封闭化.md`。

use super::{CallSite, Cx, Hooks, Rule};
use crate::*;
use jpp_ir::cause::UnsureCause;

/// 依据：B197（`unsure(text)` 的 text 须为枚举成员，否则 `E-unsure-cause`）
pub(crate) const RULE: Rule = Rule {
    code: "E-unsure-cause",
    requires: &[],
    hooks: Hooks {
        call: Some(call),
        ..Hooks::NONE
    },
};

fn call(_cx: &Cx, s: &CallSite) -> Vec<Diagnostic> {
    if s.name != "unsure" {
        return vec![];
    }
    let Some(ExprKind::Text(t)) = s.args.first().map(|a| a.kind()) else {
        return vec![];
    };
    if UnsureCause::parse(t).is_some() {
        return vec![];
    }
    vec![Diagnostic::error(
        "E-unsure-cause",
        format!(
            "unsure 的原因「{t}」不是未决原因之一。可用：{}（B197）",
            UnsureCause::ALL.map(|c| c.name()).join("、")
        ),
        s.span,
    )]
}
