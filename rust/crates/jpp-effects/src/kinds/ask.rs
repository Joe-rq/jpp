//! `ask`：问人；未答即挂起（`12` §2.6）。

use jpp_ir::key::EffectId;

use crate::spec::{
    EffectSpec, KeyPart as K, OutputShape, ProfileSchema, SchedClass, SlotDecl, SlotKind, TaintRule,
};

pub(super) const SPEC: EffectSpec = EffectSpec {
    id: EffectId::Ask,
    name: "ask",
    produces_reading: false,
    side_effecting: false,
    in_effect_row: true,
    sched: SchedClass::Immediate,
    input_schema: &[
        SlotDecl {
            name: "state",
            kind: SlotKind::State,
        },
        SlotDecl {
            name: "question",
            kind: SlotKind::Question,
        },
    ],
    output_shape: OutputShape::Answer,
    // 现行 `effect_key_of("ask", [state_hash, q_hash])`
    key_parts: &[K::State, K::Question],
    // 人的回答可信（`12` §2.11）
    taint_rule: TaintRule::Trusted,
    batchable: false,
    profile_schema: ProfileSchema::Scheduling,
};
