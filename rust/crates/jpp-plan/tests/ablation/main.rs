//! 消融测试（`21` 步 23 原文：每个 pass 配 `tests/ablation/<名>.rs`，断言写死值）。
//! 步 22：`plan_estimate`（估计与计划期拒绝）。B0487 的 `select_within` 另加文件。

mod fission;
mod plan_estimate;

// 步 30（B0488）：下游层数、层内挑选两臂、逐组费用模型
mod plan;
