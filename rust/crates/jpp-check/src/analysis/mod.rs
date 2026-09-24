//! 检查器的分析部分：名字与读数、语法遍历、效应行推断（`20` §2.3 分析器那一半），以及遍历
//! 助手与跨度收集。分析器自己报名字、字段、参数与效应标注的错；J/B 规则单元在 `rules/`。

pub(crate) mod annot;
pub(crate) mod effects;
pub(crate) mod hooks;
pub(crate) mod names;
pub(crate) mod rows;
pub(crate) mod syntax;
pub(crate) mod view;
pub(crate) mod walk;
