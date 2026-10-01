//! 默认链的取材料端口（Z0398，主会话裁定五十五；过程记录 5.16 附注、5.20）。今天只留接口，没有真实现。
//!
//! 默认链问出「缺哪类」后逐级取：作者 `unsure_source.fetch` > 程序自己的料库 [`CategoryStore`]（B0593，按类别取）>
//! 宿主取材料端口 [`MaterialSource`] > 无（路 C：类别进报告 `needed`，记 Handoff）。不用生成器补（路 A 不做：让模型
//! 替材料作证，与意图汇编 7c 冲突）。闭包各有一个通用实现，测试直接拿闭包作替身。

use jpp_value::value::{Mat, Question};

/// 程序自己的料库：按信息类别取一段材料，取不到为 `None`
pub trait CategoryStore {
    fn take(&self, need: &str, m: &Mat) -> Option<Mat>;
}

/// 宿主取材料端口：按题、类别与原材料取一段材料，取不到为 `None`
pub trait MaterialSource {
    fn fetch(&self, q: &Question, need: &str, m: &Mat) -> Option<Mat>;
}

impl<F: Fn(&str, &Mat) -> Option<Mat>> CategoryStore for F {
    fn take(&self, need: &str, m: &Mat) -> Option<Mat> {
        self(need, m)
    }
}

impl<F: Fn(&Question, &str, &Mat) -> Option<Mat>> MaterialSource for F {
    fn fetch(&self, q: &Question, need: &str, m: &Mat) -> Option<Mat> {
        self(q, need, m)
    }
}
