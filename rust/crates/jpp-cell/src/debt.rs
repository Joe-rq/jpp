//! 欠账、记号与去向（`12` §2.13 R9、R10、R13；原型乙 Z0419(1)、裁定五十九第 3 条）。
//!
//! 经过桥（`cut`、`fit`、分数过线）或占用被拒（主控 2026-09-30 Q2）得到的每个未决记一笔欠账，带一个记号。
//! 记号 = `jpp_ir::key::debt_mark_token(帧种类, 帧的主人, 过桥种类, 第几次过桥, 题的内容键, 原因)`，与账本 v5
//! 的 `jpp_ledger::DebtMark::token` 是同一个函数（主控 Z0519）：帧种类 `program`、`code`，过桥种类 `cut`、`fit`、
//! `cut_score` 与线上 `Via` 同名；`claim` 线上还没有，留在单元图内部（C6 时走「改」进线上名）。

use jpp_ir::cause::UnsureCause;
use jpp_ir::key::{debt_mark_token, hash_of};
use std::collections::BTreeMap;
use std::sync::Arc;

/// 欠账记号：摘要本身（全长），比对、并进指纹、销账都用它。
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct DebtTok(pub Arc<str>);

impl DebtTok {
    pub fn as_str(&self) -> &str {
        &self.0
    }
    /// 不对应欠账的合成键（不带记号的单元去向、单元报错）：前缀与记号摘要不同源。
    pub(crate) fn synthetic(parts: &[&str]) -> DebtTok {
        DebtTok(Arc::from(format!("~{}", hash_of(parts))))
    }
}

impl std::fmt::Display for DebtTok {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "#{}", &self.0[..self.0.len().min(16)])
    }
}

/// 帧的种类：程序段本体，或正在求值的代码单元。
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum FrameKind {
    Program,
    Cell,
}

impl FrameKind {
    pub fn tag(self) -> &'static str {
        match self {
            FrameKind::Program => "program",
            FrameKind::Cell => "code",
        }
    }
}

/// 产生欠账的引擎出口种类。
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum BridgeKind {
    Cut,
    Fit,
    CutScore,
    /// 占用被拒（Q2：R9 的「过桥」读作「程序拿到一个由引擎产出的未决出口」）
    Claim,
}

impl BridgeKind {
    pub fn tag(self) -> &'static str {
        match self {
            BridgeKind::Cut => "cut",
            BridgeKind::Fit => "fit",
            BridgeKind::CutScore => "cut_score",
            BridgeKind::Claim => "claim",
        }
    }
}

/// 一笔欠账。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Debt {
    pub tok: DebtTok,
    /// 帧的主人：程序名或代码单元键
    pub owner: String,
    /// 过桥种类；承接来的为 `None`
    pub bridge: Option<BridgeKind>,
    /// 第几次过桥；承接来的为 0
    pub nth: u32,
    pub cause: UnsureCause,
    /// 题的内容键（占用被拒时是单元加组的哈希）；承接来的为空
    pub question_key: String,
    /// 承接来的：从哪里读到（程序与版本、代码单元、多写者单元）
    pub from: Option<String>,
}

/// 一笔欠账的记号（公开给宿主与跨 crate 对照用；与 `jpp_ledger::DebtMark::token` 同一个函数）。
pub fn mark_token(
    kind: FrameKind,
    owner: &str,
    bridge: BridgeKind,
    nth: u32,
    question_key: &str,
    cause: UnsureCause,
) -> DebtTok {
    debt_token(kind, owner, bridge, nth, question_key, cause, true)
}

/// 记号的算法（Z0419(1)）。`with_question_and_cause = false` 只给反证测试用（旧记号不含题与原因）。
pub(crate) fn debt_token(
    kind: FrameKind,
    owner: &str,
    bridge: BridgeKind,
    nth: u32,
    question_key: &str,
    cause: UnsureCause,
    with_question_and_cause: bool,
) -> DebtTok {
    let (q, c) = if with_question_and_cause {
        (question_key, cause.name())
    } else {
        ("", "")
    };
    DebtTok(Arc::from(debt_mark_token(
        kind.tag(),
        owner,
        bridge.tag(),
        nth,
        q,
        c,
    )))
}

/// 去向：四类六形式（R13）。第一类交给另一道判断或计算（细化、补信息、改选）；第二类交给人或请求处理过程（升级）；
/// 第三类显式放弃并记账；第四类随返回值或单元值转交。没有「什么都不做」这个成员。
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum DutyKind {
    Refine,
    Enrich,
    Reselect,
    Escalate,
    DropAccounted,
    Handoff,
}

impl DutyKind {
    pub const ALL: [DutyKind; 6] = [
        DutyKind::Refine,
        DutyKind::Enrich,
        DutyKind::Reselect,
        DutyKind::Escalate,
        DutyKind::DropAccounted,
        DutyKind::Handoff,
    ];
    /// 事件类型名（与账本 v4 去向变体同名）。
    pub fn event_type(self) -> &'static str {
        match self {
            DutyKind::Refine => "refine",
            DutyKind::Enrich => "enrich",
            DutyKind::Reselect => "reselect",
            DutyKind::Escalate => "escalate",
            DutyKind::DropAccounted => "drop_accounted",
            DutyKind::Handoff => "handoff",
        }
    }
    /// 所属的类（1–4）。
    pub fn class(self) -> u8 {
        match self {
            DutyKind::Refine | DutyKind::Enrich | DutyKind::Reselect => 1,
            DutyKind::Escalate => 2,
            DutyKind::DropAccounted => 3,
            DutyKind::Handoff => 4,
        }
    }
}

/// 缺席类原因的出口被放弃时记的事件（R9、B95），不销账。
pub const E_DROP_UNOBSERVED: &str = "E-drop-unobserved";

/// 程序级的去向事件（一次尝试里记下的；账本编码归 V5）。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DutyEvent {
    /// `DutyKind::event_type()` 之一，或 [`E_DROP_UNOBSERVED`]
    pub kind: &'static str,
    pub tok: Option<DebtTok>,
    pub detail: String,
}

/// 违规（B196 `Violation`）：有结论时还欠着的一笔。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Violation {
    pub program: String,
    /// 第几次尝试
    pub attempt: u32,
    pub debt: Debt,
}

/// 记忆哈希 = 值哈希并上欠账记号、单元去向（记号、种类）与单元里记下的报错的摘要（R4 第二层）；都空时原样返回。
pub(crate) fn with_debt_digest(
    value_hash: String,
    debts: &BTreeMap<DebtTok, Debt>,
    duties: &BTreeMap<DebtTok, DutyKind>,
    errors: &BTreeMap<DebtTok, &'static str>,
) -> String {
    if debts.is_empty() && duties.is_empty() && errors.is_empty() {
        return value_hash;
    }
    let parts: Vec<String> = debts
        .keys()
        .map(|k| format!("d{}", k.0))
        .chain(
            duties
                .iter()
                .map(|(k, t)| format!("u{}:{}", k.0, t.event_type())),
        )
        .chain(errors.iter().map(|(k, e)| format!("e{}:{e}", k.0)))
        .collect();
    let refs: Vec<&str> = parts.iter().map(|s| s.as_str()).collect();
    format!("{value_hash}#{}", hash_of(&refs))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 记号按六个成分区分() {
        let base = |k, o: &str, b, n, q: &str, c| debt_token(k, o, b, n, q, c, true);
        let t = base(
            FrameKind::Cell,
            "X",
            BridgeKind::Cut,
            1,
            "q",
            UnsureCause::Tie,
        );
        assert_ne!(
            t,
            base(
                FrameKind::Program,
                "X",
                BridgeKind::Cut,
                1,
                "q",
                UnsureCause::Tie
            ),
            "帧种类"
        );
        assert_ne!(
            t,
            base(
                FrameKind::Cell,
                "Y",
                BridgeKind::Cut,
                1,
                "q",
                UnsureCause::Tie
            ),
            "主人"
        );
        assert_ne!(
            t,
            base(
                FrameKind::Cell,
                "X",
                BridgeKind::Fit,
                1,
                "q",
                UnsureCause::Tie
            ),
            "过桥种类"
        );
        assert_ne!(
            t,
            base(
                FrameKind::Cell,
                "X",
                BridgeKind::Cut,
                2,
                "q",
                UnsureCause::Tie
            ),
            "第几次"
        );
        assert_ne!(
            t,
            base(
                FrameKind::Cell,
                "X",
                BridgeKind::Cut,
                1,
                "r",
                UnsureCause::Tie
            ),
            "题"
        );
        assert_ne!(
            t,
            base(
                FrameKind::Cell,
                "X",
                BridgeKind::Cut,
                1,
                "q",
                UnsureCause::Absent
            ),
            "原因"
        );
        assert_eq!(t.0.len(), 24, "全长摘要");
    }

    #[test]
    fn 去向四类六形式() {
        let classes: Vec<u8> = DutyKind::ALL.iter().map(|d| d.class()).collect();
        assert_eq!(classes, [1, 1, 1, 2, 3, 4]);
    }
}
