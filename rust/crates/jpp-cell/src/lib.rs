//! J++ 的单元图（L3b，B198；`12` §2.13 骨架 v1）。
//!
//! 程序里值得留下的中间结果都是单元：源单元（宿主写）、代码单元（纯函数调用的结果，按需求值、记忆）、判断单元
//! （按题与材料的内容键）、程序单元（一段程序对外有版本的值）、多写者单元（声明了归约器）。依赖边在读的那一刻记下；
//! 材料一变沿反向边标脏，有人要时才修复；修复时两层截断：核输入（依赖都没变不重算）、核出口（记忆哈希没变上层
//! 不重算），记忆哈希里并着欠账与去向的摘要。欠账（R9）、读者承接与转交（R10）、多写者与占用（R7）都在这一层。
//!
//! 本 crate 在 C1（步 40）只做单线程语义，不接解释器（C2）、不写账本（C3）、不做多程序调度与预算（C4）、不做并发
//! 尝试（C10）。单元身份与键在 `jpp_ir::cell_key`（B193），未决原因在 `jpp_ir::cause`（B197）。
//! 预注册与结果：`地基/过程记录/工程-C1-单元图.md`；参照实现：`地基/骨架原型/乙/src/engine.rs`。

pub mod ctx;
pub mod debt;
pub mod graph;
pub mod reducer;
pub mod value;

pub use ctx::{Attempt, ClaimResult, CodeStep, Ctx, JudgeRead};
pub use debt::{BridgeKind, Debt, DebtTok, DutyEvent, DutyKind, FrameKind, Violation};
pub use graph::{
    AttemptRec, CellGraph, Dep, FlushAnswer, FlushRec, JudgeSeen, PState, Pend, Program, Published,
    Stats,
};
pub use jpp_ir::cause::UnsureCause;
pub use reducer::{ClaimState, MergeEvent, MergeKind, Reducer, SharedRead, Writers};
pub use value::CellValue;

#[cfg(test)]
mod tests;
