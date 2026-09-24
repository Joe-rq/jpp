//! J++ 的 L0 层（`20` §2.1、§2.3「L0 · `jpp-ir`」）。
//!
//! 步 6 建 [`key`]：跨 crate 传递的键、id 与它们的纯函数。步 12a 加 [`ir`]：IR 节点、站点表、标注表、
//! `wellformed`、`print`；步 13a 加 [`plan`]：计划的数据类型与运行期钩子 trait；步 12e-1 加 [`question_kind`]：题类推断（B76）。本 crate 不依赖任何内部 crate（`20` §2.2 第 5 条）。
//!
//! 步 8a 从 `jpp-core` 搬来的核心语法树在步 12d 删除：表层 AST 由 `jpp-syntax` 直接降到 IR，
//! 源位置、预算、形参与类型标注这些叶子类型并入 [`ir`]。

pub mod ir;
pub mod key;
pub mod plan;
pub mod question_kind;
