//! J++ 的 L0 层（`20` §2.1、§2.3「L0 · `jpp-ir`」）。
//!
//! 步 6 建 [`key`]：跨 crate 传递的键、id 与它们的纯函数。步 12a 加 [`ir`]：IR 节点、站点表、标注表、
//! `wellformed`、`print`；步 13a 加 [`plan`]：计划的数据类型与运行期钩子 trait；步 12e-1 加 [`question_kind`]：题类推断（B76）；步 26 加 [`diag_gate`]：运行期诊断闸门的 trait（B47）；步 23b 加 [`fission`]：超窗裂变的切点与计数纯函数；步 40（C1）加 [`cause`]：十六种未决原因（B197）与 [`cell_key`]：单元身份与键（B193）。本 crate 不依赖任何内部 crate（`20` §2.2 第 5 条）。
//!
//! 步 8a 从 `jpp-core` 搬来的核心语法树在步 12d 删除：表层 AST 由 `jpp-syntax` 直接降到 IR，
//! 源位置、预算、形参与类型标注这些叶子类型并入 [`ir`]。

pub mod cause;
pub mod cell_key;
pub mod diag_gate;
pub mod fission;
pub mod ir;
pub mod key;
pub mod plan;
pub mod purity;
pub mod question_kind;
pub mod site_key;
