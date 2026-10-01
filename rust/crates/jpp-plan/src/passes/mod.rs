//! 已落地的四个 pass 的计划期一半，各一个文件（`20` §2.3「L3 · `jpp-plan`」）。
//! `fuse` 的决定只有一位（同状态同层合成一次调用），分组在运行时的刷新里执行；`ledger` 是账本键与重放，
//! 无计划期部分。`plan`（估计与计划期拒绝）随步 22 落地，层内挑选（`select`，B43 的运行期落点）随 B0487；`fission` 随步 23b 落地（开关语义与切点规则在 `passes/fission.rs`，执行在运行时）；`lower`、`schedule` 未落地，有消费者时再建（步 23）。

pub mod fission;
pub mod lift;
pub mod plan;
pub mod select;
pub mod speculate;
pub mod vectorize;
