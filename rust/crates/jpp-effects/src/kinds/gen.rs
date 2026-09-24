//! `gen`：按提示与上下文生成候选材料（`12` §2.4）。

use jpp_ir::key::EffectId;

use crate::spec::{
    EffectSpec, KeyPart as K, OutputShape, ProfileSchema, SchedClass, SlotDecl, SlotKind, TaintRule,
};

pub(super) const SPEC: EffectSpec = EffectSpec {
    id: EffectId::Gen,
    name: "gen",
    produces_reading: false,
    side_effecting: false,
    in_effect_row: true,
    sched: SchedClass::Immediate,
    // 现行调用约定 `gen(prompt, ctx, n, retry_seq)`（`interp/host_builtins.rs` 的 `b_gen`，四个实参）
    input_schema: &[
        SlotDecl {
            name: "prompt",
            kind: SlotKind::Text,
        },
        SlotDecl {
            name: "ctx",
            kind: SlotKind::List,
        },
        SlotDecl {
            name: "n",
            kind: SlotKind::Int,
        },
        SlotDecl {
            name: "retry_seq",
            kind: SlotKind::Int,
        },
    ],
    output_shape: OutputShape::Mats,
    // 现行 `effect_key_of("gen", [site, prompt, ctx_hash, n, retry_seq])`
    key_parts: &[K::Site, K::Prompt, K::CtxHash, K::N, K::RetrySeq],
    // 输出 = ∨ ctx.taint（`12` §2.11；B37 的 `taint_out` 声明位默认 inherit）
    taint_rule: TaintRule::Inherit,
    batchable: false,
    profile_schema: ProfileSchema::Scheduling,
};
