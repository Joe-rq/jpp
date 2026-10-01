//! 校准记录、样本、证书与 `CalibStore`（线只从记录来）。（步 4c 自 `effects.rs` 原样搬出）
//!
//! 2026-09-24 按职责拆成四个文件，只搬不改（`20` 每文件不超过 800 行）：`record`（记录、样本、
//! 证书、标注来源）、`store`（`CalibStore` 本体与写入口）、`commission`（认证与选线）、
//! `access`（漂移、查询、持久化、键构造、读线）。公共路径不变。

mod access;
mod commission;
mod extend;
mod fixed_sequence;
mod record;
mod rerun;
mod sequential;
mod store;

pub use extend::{ExtendOptions, ExtendRow};
pub use fixed_sequence::{FIXED_SEQUENCE_RULE, fixed_sequence_candidates, fixed_sequence_step};
pub use record::*;
use record::{单侧声明, 反查题型, 标注集指纹, 经验unsure率};
pub use rerun::RerunOutcome;
/// `phys` 反查题型（给 `lib.rs` 的有效 α 补算用）。**只收物理名** `noul` / `choice` / `score`（样本的 `phys`）。
pub(crate) fn 反查题型_pub(phys: &str) -> Option<jpp_value::value::Op> {
    反查题型(phys)
}
/// **语义操作名**反查题型：只收 `test` / `select` / `measure`（标注行与题式的 `op`，`Op::fixture_name`）。
/// 与 [`反查题型_pub`] 是两套名字，不能互换：Z0238 就是把 `select` / `measure` 交给了物理名的反查，
/// 查不到被静默当成 test，K 元记录拿到 noul 的 δ。名字只来自 `fixture_name`，不另写一张表。
pub(crate) fn 操作名反查题型(name: &str) -> Option<jpp_value::value::Op> {
    use jpp_value::value::Op;
    [Op::Test, Op::Select, Op::Measure]
        .into_iter()
        .find(|o| o.fixture_name() == name)
}
pub use sequential::{SEQUENTIAL_RULE, SeqSpec, random_arrival, seq_first, two_ends_order};
pub use store::*;
