//! 运行期诊断闸门的接口（步 26，B47）。
//!
//! 诊断规则只有一份：`jpp-check::diagnose_question` / `diagnose_fill`（B13，步 5）。写程序时检查器对字面题
//! 直接调；运行时要对「运行中生成的题」调同一份规则，而运行时不依赖 `jpp-check`（`20` §2.2 第 1 条、B47）。
//! 做法与 [`crate::plan::PlanHooks`] 相同：trait 定在这里（运行时与检查器都看得见），实现在 `jpp-check::diag::gate`，
//! 由宿主构造后注入解释器。未注入时运行期不诊断。
//!
//! 闸门只给提示，不改走向（批 9 裁定 :42、:136「诊断层 B13/B47/B182 只提示，不改走向」）：运行时拿到
//! [`GateNote`] 只写进 `trace.warnings`，题照发。`.jpp` 的 `diagnose` 内置把同一结果交回程序，
//! 由作者或库（derive 的候选挑选，B45）按自己的策略用。

/// 交给闸门的一道题：只带字符串，不带值类型（检查器不依赖 `jpp-value`，`20` §2.2 第 4 条）。
#[derive(Clone, Copy, Debug)]
pub struct GateQuestion<'a> {
    /// `test` / `select` / `measure`
    pub op: &'a str,
    /// 实际发出的题面
    pub text: &'a str,
    /// 由题式填出时的模板（带 `{槽}`）
    pub template: Option<&'a str>,
    /// 由题式填出时的填法：槽名 → 实际填入的文字
    pub fill: Option<&'a [(String, String)]>,
}

/// 一条诊断：码（`W-diag-*`）、说错在哪、修法。修法是自动建议，零成本（结论 :29「大部分是自动建议」）。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GateNote {
    pub code: String,
    pub message: String,
    pub fix: String,
}

pub trait QuestionGate {
    /// 运行期闸门：只诊断检查期没见过的题（字面题检查期已经报过，不重复报）。
    fn gate(&self, q: &GateQuestion<'_>) -> Vec<GateNote>;
    /// `.jpp` 的 `diagnose` 内置：不论字面与否，按同一份规则诊断这道题。
    fn diagnose(&self, q: &GateQuestion<'_>) -> Vec<GateNote>;
}
