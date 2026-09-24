//! `do`：执行登记过的动作，触世界（`12` §2.5）。

use jpp_ir::key::EffectId;

use crate::spec::{
    EffectSpec, KeyPart as K, OutputShape, ProfileSchema, SchedClass, SlotDecl, SlotKind, TaintRule,
};

pub(super) const SPEC: EffectSpec = EffectSpec {
    id: EffectId::Do,
    name: "do",
    produces_reading: false,
    side_effecting: true,
    in_effect_row: true,
    sched: SchedClass::Immediate,
    // 现行调用约定 `do(action, args, iter_seq)`（`interp/host_builtins.rs` 的 `b_do`：第 3 实参是
    // `Int` 的轮次序号，进键；动作费用由动作登记处声明，不是实参）
    input_schema: &[
        SlotDecl {
            name: "action",
            kind: SlotKind::Name,
        },
        SlotDecl {
            name: "args",
            kind: SlotKind::List,
        },
        SlotDecl {
            name: "iter_seq",
            kind: SlotKind::Int,
        },
    ],
    output_shape: OutputShape::Mats,
    // 现行 `effect_key_of("do", [site, name, args, iter_seq])`
    key_parts: &[K::Site, K::Action, K::Args, K::IterSeq],
    // 由动作声明（`12` §2.11）
    taint_rule: TaintRule::Declared,
    batchable: false,
    profile_schema: ProfileSchema::Action,
};
