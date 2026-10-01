//! 未决原因（`12` §2.3、§2.13 R15；B197）：十六种的封闭枚举。
//!
//! 放在 L0，因为三处都要认它：检查器（只依赖 `ir`、`effects`）按它报 `E-unsure-cause`；`jpp-value` 的
//! `ExitKind` 由 G3 迁到它；单元图（`jpp-cell`）的欠账记号与发布原因用它（主控 2026-09-30 定，C1 预注册 Q1）。
//! 本模块只定类型，不改任何构造处。
//!
//! `untested` 一族（`delta_unknown`、`window_untested` 等）不是原因，是读数上的正交位（R15、B156），不在这里。

use serde::{Deserialize, Serialize};

/// 未决原因。加原因是「加」，改含义是「改」（R15）。
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UnsureCause {
    /// 落在作者声明线的中间区间
    Band,
    /// 并列
    Tie,
    /// 纠正性回答：材料不够（B4、J-09）
    Insufficient,
    /// 生成或执行失败（J-12）
    Fail,
    /// 预算付不起，题没发出
    Budget,
    /// 深度到限，题没发出
    Depth,
    /// 时延预算用完
    Latency,
    /// 到截止时间，题没发出（R16）
    Deadline,
    /// 没有进展（B3 一族）
    Noprogress,
    /// 候选全被拒（B3）
    RejectedAll,
    /// 没有候选（B3）
    NoCandidate,
    /// 判断器缺席
    Absent,
    /// 判断后按代码谓词没有可行候选（C-4）
    Infeasible,
    /// 没有可信记录的冷题（裁定四十三）
    Cold,
    /// 占用被拒（R7）
    ClaimConflict,
    /// 有结论时还欠着未决（R9）
    Violation,
}

impl UnsureCause {
    /// 全部十六种，按声明顺序。
    pub const ALL: [UnsureCause; 16] = [
        UnsureCause::Band,
        UnsureCause::Tie,
        UnsureCause::Insufficient,
        UnsureCause::Fail,
        UnsureCause::Budget,
        UnsureCause::Depth,
        UnsureCause::Latency,
        UnsureCause::Deadline,
        UnsureCause::Noprogress,
        UnsureCause::RejectedAll,
        UnsureCause::NoCandidate,
        UnsureCause::Absent,
        UnsureCause::Infeasible,
        UnsureCause::Cold,
        UnsureCause::ClaimConflict,
        UnsureCause::Violation,
    ];

    /// `unsure(<cause>` 里的那个词（B197：这个前缀是冻结接口）。
    pub fn name(self) -> &'static str {
        match self {
            UnsureCause::Band => "band",
            UnsureCause::Tie => "tie",
            UnsureCause::Insufficient => "insufficient",
            UnsureCause::Fail => "fail",
            UnsureCause::Budget => "budget",
            UnsureCause::Depth => "depth",
            UnsureCause::Latency => "latency",
            UnsureCause::Deadline => "deadline",
            UnsureCause::Noprogress => "noprogress",
            UnsureCause::RejectedAll => "rejected_all",
            UnsureCause::NoCandidate => "no_candidate",
            UnsureCause::Absent => "absent",
            UnsureCause::Infeasible => "infeasible",
            UnsureCause::Cold => "cold",
            UnsureCause::ClaimConflict => "claim_conflict",
            UnsureCause::Violation => "violation",
        }
    }

    /// 按名字取；不是十六种之一返回 `None`（检查器据此报 `E-unsure-cause`，B197）。
    pub fn parse(s: &str) -> Option<UnsureCause> {
        UnsureCause::ALL.into_iter().find(|c| c.name() == s)
    }

    /// 缺席类：题没有被判过（B197：`absent, budget, depth, latency, deadline`）。这类出口不能放弃（R9、B95）。
    pub fn is_absent_class(self) -> bool {
        matches!(
            self,
            UnsureCause::Absent
                | UnsureCause::Budget
                | UnsureCause::Depth
                | UnsureCause::Latency
                | UnsureCause::Deadline
        )
    }
}

impl std::fmt::Display for UnsureCause {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.name())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 十六种_名字唯一_可解析回来() {
        let names: std::collections::BTreeSet<&str> =
            UnsureCause::ALL.iter().map(|c| c.name()).collect();
        assert_eq!(names.len(), 16);
        for c in UnsureCause::ALL {
            assert_eq!(UnsureCause::parse(c.name()), Some(c));
            assert_eq!(
                serde_json::to_string(&c).unwrap(),
                format!("\"{}\"", c.name())
            );
        }
        assert_eq!(
            UnsureCause::parse("untested"),
            None,
            "untested 是正交位，不是原因"
        );
        assert_eq!(UnsureCause::parse("drift"), None);
    }

    #[test]
    fn 缺席类五种() {
        let n: Vec<&str> = UnsureCause::ALL
            .iter()
            .filter(|c| c.is_absent_class())
            .map(|c| c.name())
            .collect();
        assert_eq!(n, ["budget", "depth", "latency", "deadline", "absent"]);
    }
}
