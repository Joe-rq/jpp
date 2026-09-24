//! pass `vectorize`（宪法登记表第 47 行）的静态一半：每个函数体一轮里的候选站点。
//!
//! `map`/`filter` 的后续各轮在第 0 轮到达刷新点之前提前登记，体内有 `cut` 时就不会一轮一层。
//! 方法值只有运行期才知道是谁，所以这里按函数节点号给出每个函数体的候选（[`jpp_ir::plan::Plan::bodies`]），
//! 运行时拿到方法值、绑好形参后经 [`crate::Hooks`] 的 `instantiate` 按环境筛。
//!
//! 它没有「白花」那一栏：`map` 对每个元素都会调方法，提前登记的站点没有一个是猜的。
//! 条件（登记表原文）：体内无 `do`/`ask`、无 loop-carried 名字。`do`/`ask` 由候选的剪枝拦
//! （只登记 `judge`，遇副作用即停）；loop-carried 在 `map`/`filter` 上由构造排除（体只收一个元素）；
//! `fold`/`loop` 有累加器，不在这条路上。

use jpp_ir::ir::Function;
use jpp_ir::plan::TargetSite;

pub fn body(f: &Function) -> Vec<TargetSite> {
    super::speculate::body(&f.body)
}
