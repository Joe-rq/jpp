//! 超窗裂变 pass `fission` 的计划期一半（`12` §4 pass 3；`20` §2.3「`passes/fission.rs`（窗口切分）」；步 23b）。
//!
//! **近似档（V8 实测 0.897，`21` 步 23b 设计收口盘点注）**：只在题或题式声明 `fission: "approx"` 时起作用。
//! 三个条件同时成立才切：开关开（[`crate::Passes::fission`]，写进 `Plan.fission`）；画像测过窗口；材料确实超窗
//! （对象槽单段超 `window.text_slots.usable_lower`，或语境槽超 `json_slots`）。画像没测窗口时不切，照报
//! `W-window-untested`（主控 Q-F6）。
//!
//! 计划期今天只决定这一位：切多少块要看实际材料，在运行时登记处决定（`20` 第 827 行「在刷新点看 K 与尺寸」；
//! `jpp-runtime/src/fission.rs`）。切点规则只有一份，在 L0（[`jpp_ir::fission`]），这里转出供计划与测试引用。
//!
//! 已知差距（过程记录 `工程-步23b.md` §六·5）：逐站点 `SitePlan.fission` 未接；`plan` pass 的调用估计不数裂变出的
//! 调用；`12` pass 8「超窗裂变出的调用按跨状态推测计」未接。
//!
//! 依据：`11` §5.3；`12` §4 pass 3、H6；L-035；`21` 步 23b；主控答复 Z0208 与 2026-09-29 越界项答复

pub use jpp_ir::fission::{CutKind, cut_once, measure_count, split};

/// 一道题声明的合回方式（`11` §5.3：test → exists，或按声明 all；select → 分块选后再一层 noul-argmax；
/// measure → 按出口计数）。运行时按题的操作与声明取；这里只给名字，报告与告警用。
pub fn merge_name(op: &str, all: bool) -> &'static str {
    match (op, all) {
        ("test", false) => "exists",
        ("test", true) => "all",
        ("select", _) => "noul-argmax",
        ("measure", _) => "count",
        _ => "exists",
    }
}
