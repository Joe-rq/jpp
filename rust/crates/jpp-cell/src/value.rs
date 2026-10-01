//! 单元图对值的最小要求。
//!
//! 单元图不绑定 `jpp_value::Value`：那是 `Rc`/`RefCell` 表示，出口结构体对外不可构造，绑上它测试就只能等解释器。
//! 单元图只要两件事：值的内容哈希（早截断、发布「值变了没有」都看它），与值里带着的未决出口（读者承接、
//! 转交成立与否、发布状态「未决」都看它）。C2 给 `jpp_value::Value` 实现本 trait；为此 `jpp-value` 的出口要能
//! 带欠账记号（今天只有运行内的 `id`），这是留给 C2 / G3 的接口要求。

use crate::debt::DebtTok;
use jpp_ir::cause::UnsureCause;

/// 单元里存的值。
pub trait CellValue: Clone + 'static {
    /// 值的内容哈希。taint 进值哈希、不进单元键（R1），由实现方保证；欠账记号**不**进值哈希（记号只用来认是哪一笔
    /// 欠账，欠账另在记忆哈希的摘要里）。
    fn content_hash(&self) -> String;

    /// 值里（递归地）带着的每个未决出口：记号（引擎发的；`None` 表示不对应任何欠账）与原因。
    fn unsure_exits(&self, out: &mut Vec<(Option<DebtTok>, UnsureCause)>);
}

/// 便利：值里带着的未决出口。
pub(crate) fn exits_of<V: CellValue>(v: &V) -> Vec<(Option<DebtTok>, UnsureCause)> {
    let mut out = vec![];
    v.unsure_exits(&mut out);
    out
}
