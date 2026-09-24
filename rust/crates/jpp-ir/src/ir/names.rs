//! 名字表（步 12d，`20` §2.3 `lower`「表层名到节点的三条规则」）：降级按它把调用位置上的名字
//! 解析成语言形式、效应、内核构造或宿主调用。
//!
//! 表本身不在本 crate：效应来自 `jpp-effects` 的注册表，构造与宿主内置来自运行时，由外观层组装后
//! 传入（本 crate 与 `jpp-syntax` 都不依赖它们，`20` §2.2 第 5 条）。

use super::*;

/// 一个名字在调用位置上是什么。
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NameClass {
    State,
    Effect(EffectId),
    Cut,
    Fit,
    Loop,
    Handle,
    Consume(ConsumeHow),
    /// 内核构造（`sieve`、`outcome`、`repeat` …）
    Construct,
    /// 高阶宿主内置（`map`/`filter`/`fold`）：带站点
    HigherOrder,
    /// 其他宿主内置或用户名字
    Plain,
}

pub trait NameTable {
    fn classify(&self, name: &str) -> NameClass;
    /// 效应的输入槽名（`EffectSpec.input_schema` 顺序）
    fn slots(&self, effect: EffectId) -> Vec<&'static str>;
}
