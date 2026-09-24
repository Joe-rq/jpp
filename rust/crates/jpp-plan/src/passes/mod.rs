//! 已落地的四个 pass 的计划期一半，各一个文件（`20` §2.3「L3 · `jpp-plan`」）。
//! `fuse` 的决定只有一位（同状态同层合成一次调用），分组在运行时的刷新里执行；`ledger` 是账本键与重放，
//! 无计划期部分。其余四个（`fission`、`lower`、`schedule`、`plan`）未落地，有消费者时再建（步 23）。

pub mod lift;
pub mod speculate;
pub mod vectorize;
