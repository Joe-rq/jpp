//! `EffectSpec` 的字段类型（`20` §2.3、A2）。规划、调度、账本键、taint、登记、执行六处将只读这些字段，
//! 不按效应名分支（步 15a）。本步字段已按现行代码的行为填好，但还没有读者。

use jpp_ir::key::EffectId;

/// 调度类别：`Layered` 在刷新点成批发出（判断的层）；`Immediate` 在登记处当场执行。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SchedClass {
    Layered,
    Immediate,
}

/// 效应输出的种类，按效应固定（与 B51-R2 动作级的 `mat_shape` 不是一回事）。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OutputShape {
    /// 读数（经 `cut` 成出口）
    Readings,
    /// 材料
    Mats,
    /// 普通值
    Value,
    /// 人的回答（可能未答，`Pending`）
    Answer,
}

/// taint 规则（`12` §2.11 表的唯一实现处；格运算在 `jpp-value`）。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TaintRule {
    /// 输出 taint = 输入 taint 的 ∨（`cut` 继承读数；`gen` = ∨ ctx.taint）
    Inherit,
    /// 由声明给出（`do` 的动作声明、`transform` 的 `taint_out`）
    Declared,
    /// 恒可信（`ask`：人的回答）
    Trusted,
}

/// 输入槽的种类。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SlotKind {
    State,
    Questions,
    Question,
    Text,
    List,
    Int,
    Num,
    Name,
    Fn,
}

/// 一个输入槽：名字与种类。`wellformed` 与检查器校验效应节点的唯一来源（步 12a/12b 起读）。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SlotDecl {
    pub name: &'static str,
    pub kind: SlotKind,
}

/// 账本键的一个分量。判断的键按 `jpp_ir::key::JudgeKey` 的字段逐项列出；
/// 其他效应按现行 `effect_key_of(kind, parts)` 的 `parts` 顺序列出。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KeyPart {
    Model,
    State,
    Question,
    Phys,
    Render,
    PermSeed,
    RunSeq,
    Site,
    Action,
    Args,
    IterSeq,
    Prompt,
    CtxHash,
    N,
    RetrySeq,
    Fn,
    Captured,
    InputHashes,
}

/// 这种效应的画像分表有哪些字段（B37）：只有产出读数的效应带类假设。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProfileSchema {
    /// 调度与预算输入 + 类假设 H1–H8（`judge`）
    Reading,
    /// 只有调度与预算输入（`gen`、`ask`、`transform`）
    Scheduling,
    /// 动作画像：调度与预算输入 + 可逆性、`taint_out`（`do`）
    Action,
}

/// 一种效应「是什么」。字段缺值是编译错（S2 的漏改检测）。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EffectSpec {
    pub id: EffectId,
    /// 源码里的名字（`judge`、`gen`、`do`、`ask`、`transform`）
    pub name: &'static str,
    pub produces_reading: bool,
    pub side_effecting: bool,
    /// 能否写进函数的效应行 `!{…}`。记账变换 `transform` 为假：它由宿主纯函数完成、纯性由账本核，
    /// 不是作者声明的效应形式（`12` §2.8）。检查器的效应名表只从这一位推出。
    pub in_effect_row: bool,
    pub sched: SchedClass,
    pub input_schema: &'static [SlotDecl],
    pub output_shape: OutputShape,
    pub key_parts: &'static [KeyPart],
    pub taint_rule: TaintRule,
    /// 同一刷新点的多个调用能否并成一次（P5：一状态多题一次问完）
    pub batchable: bool,
    pub profile_schema: ProfileSchema,
}

impl EffectSpec {
    /// 轮次序号槽：键含 `IterSeq` 或 `RetrySeq` 时，返回同名输入槽的位置与名字（J-13 读它）。
    pub fn seq_slot(&self) -> Option<(usize, &'static str)> {
        [
            (KeyPart::IterSeq, "iter_seq"),
            (KeyPart::RetrySeq, "retry_seq"),
        ]
        .into_iter()
        .filter(|(p, _)| self.key_parts.contains(p))
        .find_map(|(_, slot)| {
            self.input_schema
                .iter()
                .position(|d| d.name == slot)
                .map(|i| (i, slot))
        })
    }
}
