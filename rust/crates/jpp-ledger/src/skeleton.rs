//! 账本 v5 的骨架事件用到的结构（步 34 V5，B196；`12` §2.13 R13；冻结清单 §4.1）。
//!
//! 本步只声明，不写入：运行时照旧写 v4 的条目，新事件由 C3、G2、G4 开始写。英文名取冻结清单 §4.1；§4.1 没给的
//! （[`PubState`]、[`FrameKind`]、[`Via`] 等）是本步取的，登在 `地基/过程记录/工程-V5-账本格式.md` §二·1。
//! 所有结构 `deny_unknown_fields`，集合一律 `Vec`（有序），编码因此确定。

use serde::{Deserialize, Serialize};

use jpp_ir::key::SpanId;

use crate::{Entry, Skipped};

/// 尝试引用 `(program, n)`：契约性结果都带它（B196）。`program` 是程序单元身份（B193 第 4 条），`n` 从 1 起。
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AttemptRef {
    pub program: String,
    pub n: u64,
}

/// 帧的种类：程序单元的帧、代码单元的帧（R9、R4）。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FrameKind {
    Program,
    Code,
}

/// 过桥种类（R9：`cut`、`fit` 及分数过线）。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Via {
    Cut,
    Fit,
    CutScore,
}

impl FrameKind {
    pub fn name(self) -> &'static str {
        match self {
            FrameKind::Program => "program",
            FrameKind::Code => "code",
        }
    }
}

impl Via {
    pub fn name(self) -> &'static str {
        match self {
            Via::Cut => "cut",
            Via::Fit => "fit",
            Via::CutScore => "cut_score",
        }
    }
}

/// 欠账记号（R9、R4；冻结清单 1.4；原型乙 Z0419(1) `owe` 的六项构成）：帧种类、主人、过桥种类、第几次过桥、
/// 题内容键、原因。六项全上账本，`Violation` 直接读得出主人、第几次、原因、题；任一项不同即不同记号。
/// `cause` 用 B197 十六个名字的字符串，封闭枚举归 G3，这里不另造。
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DebtMark {
    pub frame: FrameKind,
    pub owner: String,
    pub via: Via,
    pub nth: u32,
    pub key: String,
    pub cause: String,
}

impl DebtMark {
    /// 记号摘要：C1 的指纹（R4 第二层）与按记号销账用。不上账本。
    /// 公式在 `jpp_ir::key::debt_mark_token`，`jpp-cell` 用同一个函数（主控 Z0519）。
    pub fn token(&self) -> String {
        jpp_ir::key::debt_mark_token(
            self.frame.name(),
            &self.owner,
            self.via.name(),
            self.nth,
            &self.key,
            &self.cause,
        )
    }
}

/// `Unasked` 与 `Stop` 的原因（R13、R15：`budget`、`depth`、`deadline`）。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StopCause {
    Budget,
    Depth,
    Deadline,
}

/// 读到某单元的第几版。
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VersionAt {
    pub unit: String,
    pub version: u64,
}

/// 尝试的版本快照（B196 `snapshot`；字段名取 §4.1）：等到读的程序版本、多写者单元版本。
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Snapshot {
    #[serde(default)]
    pub settled_reads: Vec<VersionAt>,
    #[serde(default)]
    pub shared: Vec<VersionAt>,
}

/// 已发布版本的状态（R2）：进行中、已定、已修正、未决。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PubState {
    InProgress,
    Settled,
    Revised,
    Unsure,
}

/// 立刻读到的版本（R5；§4.1 `peeks`）。
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Peek {
    pub prog: String,
    pub version: u64,
    pub state: PubState,
    pub hash: String,
}

/// 多写者合并的结果种类（R7；§4.1 `type`）。§4.1 的 `rejected:<原因>` 拆成 `rejected` 加 `Merge.cause`。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MergeKind {
    Merge,
    Conflict,
    Overwritten,
    ClaimGranted,
    ClaimRejected,
    Rejected,
}

/// 不透明结果的声明来源（R8：未声明按不透明处理，账本记下是哪一种）。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OpaqueDecl {
    Opaque,
    Undeclared,
}

/// 透明动作经端口读过的世界键与版本（R8）。
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PortRead {
    pub key: String,
    pub version: u64,
}

/// 刷新里的一次调用（§4.1 `calls`）。读数不重复存：在同键的 `Judge` 条目里。
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FlushCall {
    pub call: u64,
    pub material: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub merged_by: Option<String>,
    pub questions: Vec<FlushQuestion>,
}

/// 刷新里的一道题：要求者（按登记顺序）与付钱方（R11 第 3 条）。
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FlushQuestion {
    pub key: String,
    pub requesters: Vec<String>,
    pub payer: String,
    /// 跨变更对齐同一个判断的身份（原型 `identity()`：题号加各槽材料的主人加候选），C6 只凭账本统计重发时用；
    /// 留位，为空不写（V5 复核）。原型的 `qid` 由 `Judge.jkey.q` 推出，`state_chars` 由材料内容重算，不存。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub identity: Option<String>,
}

/// 账本头 `segments` 的一项（冻结清单 §4.1 账本头）：本账本里的段与父段。
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Segment {
    pub seg: SpanId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent: Option<SpanId>,
}

/// `Withheld` 的原因（B200；G2 附录三加挂起与出错，主控 Z0564）。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WithheldCause {
    /// 这次尝试有违规
    Violation,
    /// 这一趟挂起等人回答（`ask` 未答），没走到结算
    Suspended,
    /// 这一趟运行期出错，没走到结算
    Error,
}

/// 去向的类（R13、`13` §3、`12` J-05 B31：四类封闭）。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DutyClass {
    /// (一) 交给另一道判断或计算
    Another,
    /// (二) 交给人或请求处理过程
    Escalated,
    /// (三) 显式放弃并记账
    Dropped,
    /// (四) 随返回值或单元值转交
    HandedOff,
}

/// 去向的形式与各自的附加载荷（B196「四类六形式」，主控定）：形式就是标签，一个字段，形式与载荷对不上的状态
/// 表示不出来。类由形式推出（[`DutyForm::class`]）。
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum DutyForm {
    Refine {
        #[serde(default)]
        how: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        to: Option<String>,
    },
    Enrich {
        #[serde(default)]
        need: String,
        #[serde(default)]
        round: u32,
        #[serde(default)]
        got: bool,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        asked_by: Option<String>,
    },
    Reselect {
        #[serde(default)]
        from: usize,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        chosen: Option<usize>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        skipped: Vec<Skipped>,
    },
    Escalate {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        ask: Option<String>,
    },
    DropAccounted {},
    Handoff {
        #[serde(default)]
        to: String,
    },
}

impl DutyForm {
    /// 线上名（`refine`、`enrich`、`reselect`、`escalate`、`drop_accounted`、`handoff`）。
    pub fn name(&self) -> &'static str {
        match self {
            DutyForm::Refine { .. } => "refine",
            DutyForm::Enrich { .. } => "enrich",
            DutyForm::Reselect { .. } => "reselect",
            DutyForm::Escalate { .. } => "escalate",
            DutyForm::DropAccounted {} => "drop_accounted",
            DutyForm::Handoff { .. } => "handoff",
        }
    }
    pub fn class(&self) -> DutyClass {
        match self {
            DutyForm::Refine { .. } | DutyForm::Enrich { .. } | DutyForm::Reselect { .. } => {
                DutyClass::Another
            }
            DutyForm::Escalate { .. } => DutyClass::Escalated,
            DutyForm::DropAccounted {} => DutyClass::Dropped,
            DutyForm::Handoff { .. } => DutyClass::HandedOff,
        }
    }
}

/// `Entry::Duty` 的内容（B196 的规范去向事件）。与 v4 六种去向事件（旧形式，照读照写）的互转见
/// [`Duty::from_legacy`] / [`Duty::to_legacy`]：C3 起改写 `Duty` 时只换写入的变体，不再动格式。
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Duty {
    pub attempt: AttemptRef,
    /// 与 v4 六种事件的 `of` 同义：责任键（`reselect` 是读数键）
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub of: Vec<String>,
    /// 原因；为空不写（`reselect` 在 v4 没有原因，V5 复核）。转写时旧事件的空串记为 `None`，转回时还原为空串
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cause: Option<String>,
    #[serde(default)]
    pub site: usize,
    /// 销掉的欠账记号（C1 有记号后填）
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub marks: Vec<DebtMark>,
    /// 说明（§4.1「类型、说明、销掉的记号」）
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub detail: String,
    pub form: DutyForm,
}

impl Duty {
    /// v4 的六种去向事件转写成 `Duty`（给 C3 用的投影）；不是去向事件返回 `None`。
    pub fn from_legacy(e: &Entry, attempt: AttemptRef, marks: Vec<DebtMark>) -> Option<Duty> {
        let d = |of: &Vec<String>, cause: &str, site: usize, form: DutyForm| Duty {
            attempt: attempt.clone(),
            of: of.clone(),
            cause: (!cause.is_empty()).then(|| cause.to_string()),
            site,
            marks: marks.clone(),
            detail: String::new(),
            form,
        };
        Some(match e {
            Entry::Drop { of, cause, site } => d(of, cause, *site, DutyForm::DropAccounted {}),
            Entry::Refine {
                of,
                cause,
                site,
                how,
                to,
            } => d(
                of,
                cause,
                *site,
                DutyForm::Refine {
                    how: how.clone(),
                    to: to.clone(),
                },
            ),
            Entry::Enrich {
                of,
                cause,
                site,
                need,
                round,
                got,
                asked_by,
            } => d(
                of,
                cause,
                *site,
                DutyForm::Enrich {
                    need: need.clone(),
                    round: *round,
                    got: *got,
                    asked_by: asked_by.clone(),
                },
            ),
            Entry::Handoff {
                of,
                cause,
                site,
                to,
            } => d(of, cause, *site, DutyForm::Handoff { to: to.clone() }),
            Entry::Escalate {
                of,
                cause,
                site,
                ask,
            } => d(of, cause, *site, DutyForm::Escalate { ask: ask.clone() }),
            // `Reselect` 在 v4 没有 `cause`
            Entry::Reselect {
                of,
                site,
                from,
                chosen,
                skipped,
            } => d(
                of,
                "",
                *site,
                DutyForm::Reselect {
                    from: *from,
                    chosen: *chosen,
                    skipped: skipped.clone(),
                },
            ),
            _ => return None,
        })
    }

    /// 转回 v4 的旧形式（`attempt`、`marks`、`detail` 在旧形式里没有位置，丢掉；`reselect` 的 `cause` 同理）。
    pub fn to_legacy(&self) -> Entry {
        let (of, cause, site) = (
            self.of.clone(),
            self.cause.clone().unwrap_or_default(),
            self.site,
        );
        match &self.form {
            DutyForm::DropAccounted {} => Entry::Drop { of, cause, site },
            DutyForm::Refine { how, to } => Entry::Refine {
                of,
                cause,
                site,
                how: how.clone(),
                to: to.clone(),
            },
            DutyForm::Enrich {
                need,
                round,
                got,
                asked_by,
            } => Entry::Enrich {
                of,
                cause,
                site,
                need: need.clone(),
                round: *round,
                got: *got,
                asked_by: asked_by.clone(),
            },
            DutyForm::Handoff { to } => Entry::Handoff {
                of,
                cause,
                site,
                to: to.clone(),
            },
            DutyForm::Escalate { ask } => Entry::Escalate {
                of,
                cause,
                site,
                ask: ask.clone(),
            },
            DutyForm::Reselect {
                from,
                chosen,
                skipped,
            } => Entry::Reselect {
                of,
                site,
                from: *from,
                chosen: *chosen,
                skipped: skipped.clone(),
            },
        }
    }
}
