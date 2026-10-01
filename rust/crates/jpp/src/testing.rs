//! 测试替身共用的约定（B0492 S5，主控 2026-09-30）：伴随元题的中性读数只在这一处定义。集成测试（`tests/common`）
//! 与命令行的单元测试（`cli/run_io.rs` 的 Jev 传输桩）都引用它。

use crate::value::{Answer, Question, State};

/// 题面是不是伴随题式的形状（正式定义在 [`crate::session::是伴随题面`]）。Jev 线上请求不带校准键，只能按题面认
pub fn 伴随题面(text: &str) -> bool {
    crate::session::是伴随题面(text)
}

/// 替身判断器碰到伴随元题给中性读数：是非题 `Noul(0.5)`，K 选一（「最缺哪类」，过程记录 5.23）在 `s.over` 上均匀。
/// 认法：校准键是伴随题键，并且题面是伴随题式的形状（免得误伤 `lib/diag.jpp` 自己的两道诊断题）
pub fn 伴随中性(q: &Question, s: &State) -> Option<Answer> {
    let 键 = q.calib.starts_with("unsure-companion-")
        || q.calib == "diag-in-material"
        || q.calib == "diag-two-judgments";
    if !(键 && 伴随题面(&q.text)) {
        return None;
    }
    Some(match q.op {
        crate::value::Op::Select => {
            Answer::Choice(vec![1.0 / s.over.len().max(1) as f64; s.over.len()])
        }
        _ => Answer::Noul(0.5),
    })
}
