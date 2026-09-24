//! `judge`：对一个状态问一组题，产出读数（`12` §2.1）。

use jpp_ir::key::EffectId;

use crate::spec::{
    EffectSpec, KeyPart as K, OutputShape, ProfileSchema, SchedClass, SlotDecl, SlotKind, TaintRule,
};

pub(super) const SPEC: EffectSpec = EffectSpec {
    id: EffectId::Judge,
    name: "judge",
    produces_reading: true,
    side_effecting: false,
    in_effect_row: true,
    sched: SchedClass::Layered,
    input_schema: &[
        SlotDecl {
            name: "state",
            kind: SlotKind::State,
        },
        SlotDecl {
            name: "questions",
            kind: SlotKind::Questions,
        },
    ],
    output_shape: OutputShape::Readings,
    // 与 `jpp_ir::key::JudgeKey` 字段同序
    key_parts: &[
        K::Model,
        K::State,
        K::Question,
        K::Phys,
        K::Render,
        K::PermSeed,
        K::RunSeq,
        K::Site,
    ],
    // 出口 taint 继承读数 taint（状态 ∨ 题，`12` §2.11、B58）
    taint_rule: TaintRule::Inherit,
    batchable: true,
    profile_schema: ProfileSchema::Reading,
};
