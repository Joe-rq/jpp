//! `transform`：宿主纯函数把材料变成材料，纯性由账本核（`12` §2.8）。

use jpp_ir::key::EffectId;

use crate::spec::{
    EffectSpec, KeyPart as K, OutputShape, ProfileSchema, SchedClass, SlotDecl, SlotKind, TaintRule,
};

pub(super) const SPEC: EffectSpec = EffectSpec {
    id: EffectId::Transform,
    name: "transform",
    produces_reading: false,
    side_effecting: false,
    // 记账变换，不是作者声明的效应形式，不进效应行（`12` §2.8）
    in_effect_row: false,
    sched: SchedClass::Immediate,
    input_schema: &[
        SlotDecl {
            name: "f",
            kind: SlotKind::Fn,
        },
        SlotDecl {
            name: "args",
            kind: SlotKind::List,
        },
    ],
    // 现行实现返回 `Value::Mat`
    output_shape: OutputShape::Mats,
    // 现行 `effect_key_of("transform", [site, f.hash, captured, input_hashes])`
    key_parts: &[K::Site, K::Fn, K::Captured, K::InputHashes],
    // 由 `taint_out` 声明给出（`12` §2.8、§2.11）
    taint_rule: TaintRule::Declared,
    batchable: false,
    profile_schema: ProfileSchema::Scheduling,
};
