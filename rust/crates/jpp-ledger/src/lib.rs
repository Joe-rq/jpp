//! 只增账本（`12` §2.10）：判断读数、效应记录、成本。重放时同键零调用。
//!
//! **格式 v3（步 18a，格式步；B124）**：JSONL 链式。首行 `{version, header}`，在第 1 条条目之前定稿；
//! 之后每条一行 `{seq, prev, entry}`，`prev` 是上一行文本的哈希（首条对头行）。实际命中的校准记录不在头行，
//! 而是条目 [`Entry::CalibUsed`]（每键首次命中追加一条，按键取最后一条）；效应输出的材料元数据在
//! `Effect.output_mat`（带来源边与种类，`derived_from` 不写，读回由值依赖边重算，B84、B92）。
//! 末行半写（无换行且解析不了）即截断到最后一条完整条目并报告；链断、未知字段、完整行解析失败即拒绝。
//! v2（步 7 至 18c）报 `E-ledger-v2`，经 `jpp::store::migrations::ledger_v2` 迁移（`jpp ledger-migrate`，
//! 或 CLI 读入时在内存里迁移），旧二进制在标签 `ledger-v2-archive`；v1 报 `E-ledger-archived`，
//! 用标签 `ledger-v1-archive` 的二进制重放。
//!
//! **格式 v5（步 34 V5，B196）**：一次声明骨架 v1 的全部事件（`12` §2.13 R13，冻结清单 §4.1）：不定输入
//! `Attempt`、`Flush`、`HostEvent`、`Merge`、`Opaque`、`Transparent`；契约性结果 `Publish`、`Duty`、`Violation`、
//! `Unasked`、`Stop`、`Withheld`（B200）。结构在 [`skeleton`] 模块。本步只声明，运行时照旧写 v4 的条目，
//! 新事件由 C3、G2、G4 开始写。删 `Halt`（从未构造，由 `Stop` 取代）。账本头的 schema 就是头行 `version`；
//! `HeaderCompared` 加 `trace`、`segments`、`key_version`（为空不写）。`CarryRecord` 的 `depth_at` 改名 `hop`
//! （旧名照读）、加 `round`。v3、v4 照读不迁移，v4 旧二进制在标签 `ledger-v4-archive`。
//! 过程记录 `地基/过程记录/工程-V5-账本格式.md`。
//! 依据：`20` §2.3 `jpp-ledger`、§3.7、§九 账本行；`21` §三·4 步 7、E5；B40、B59、B61。
//!
//! **步 10（R）**：自 `jpp-core::ledger` 原样搬成 crate `jpp-ledger`（只依赖 `jpp-ir`、`jpp-value`），
//! `jpp-core` 原路径重导出。编解码只对字节与文本，落盘归宿主（`jpp-cli` 今天、`jpp-store` 步 18）。
//! 新增两个只读聚合：[`observations`]（从账本派生校准观察）与 [`depth_profile`]（验收 2 的取数函数）。

use std::collections::{BTreeMap, HashMap};

use serde::{Deserialize, Serialize};
use serde_json::Value as Json;

use jpp_ir::ir::Span;
use jpp_value::prov::EdgeKind;
use jpp_value::value::{Answer, Taint, hash_of};

mod aggregate;
pub use aggregate::{Observation, depth_profile, observations};
mod port;
pub use port::{Durability, LedgerError, LedgerPort};

pub use jpp_ir::key::{
    CacheKey, CalibRef, EffectKey, JudgeKey, RENDER_VERSION, SpanId, TraceCtx, TraceId, effect_key,
    judge_key,
};
mod tree;
pub use tree::{SpanNode, TraceGroup, TraceTree, Untraced};
pub mod skeleton;
pub use skeleton::{
    AttemptRef, DebtMark, Duty, DutyClass, DutyForm, FlushCall, FlushQuestion, FrameKind,
    MergeKind, OpaqueDecl, Peek, PortRead, PubState, Segment, Snapshot, StopCause, VersionAt, Via,
    WithheldCause,
};

/// 账本格式版本（步 18a 起 3；C-1 起 4：加未决去向事件的六个变体；C-2 的追踪字段并进同一个 v4：
/// 行外壳 `{seq, prev, entry, trace}` 的 `trace` 可缺省，没有它的 v4 账本照读，见 [`EntryLine`]；
/// 步 34 V5 起 5：骨架事件一次声明，B196）。头行的 `version` 就是冻结清单 §4.1 账本头的 `schema`。
pub const LEDGER_VERSION: u32 = 5;
/// 本二进制照读的旧版本：v3、v4 的条目集合是 v5 的子集（`Halt` 从未构造），不迁移（C-1 Z0207 第 1 条；V5）。
pub const LEDGER_READS_AS_IS: [u32; 2] = [3, 4];
/// v1 账本的归档标签：只由这个标签处的二进制重放（`21` E5）。
pub const V1_ARCHIVE_TAG: &str = "ledger-v1-archive";
/// v2 账本的归档标签（步 18a 格式变更前，`21` E5）。v2 另有迁移（`jpp::store::migrations::ledger_v2`）。
pub const V2_ARCHIVE_TAG: &str = "ledger-v2-archive";
/// v4 二进制的归档标签（步 34 V5 格式变更前，`21` E5）。v4 账本本二进制照读，标签只为留住 v4 二进制。
pub const V4_ARCHIVE_TAG: &str = "ledger-v4-archive";

/// 一条来源边（账本 v3 的 `output_mat.sources`，B84、B92）：直接来源读数的账本键、边的种类、题哈希。
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourceEdge {
    pub key: String,
    pub kind: EdgeKind,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub q: String,
}

/// 效应输出材料的元数据（账本 v3，步 18a）：内容在 `Effect.output`，这里是地址、来源链、taint 与来源边。
/// `taint` 必填：坏值或缺失即解码拒绝（不兜底，`12` §2.11「无声吞掉一个字段」通则）。
/// `derived_from` 不写：它是值依赖边的题哈希投影，读回时由 `sources` 重算（B84、B92）。
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MatMeta {
    pub addr: String,
    pub origin: Vec<String>,
    pub taint: Taint,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub sources: Vec<SourceEdge>,
}

/// 一次置换测量：用了几个置换（K），众数占比多少。K 是这个测量身份的一部分，二者不拆开记。
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PermMeasure {
    pub perms: usize,
    pub mode_share: f64,
}

/// 账本条目。每个变体的字段集合是封闭的：解码遇未知字段报错并指名（`12` §2.11「无声吞掉一个字段」通则）。
/// `reused_from`、`Intent` 先存在、恒为空或不产生，由步 19、18b 填；`calib_ref` 的
/// `key`/`kind`/`fill` 由 20a-2 填（B124，账本格式不再变，`21` ET1）。`Halt` 在 V5 删去（从未构造，B93；由 `Stop` 取代）。
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum Entry {
    Judge {
        /// 账本键（`JudgeKey::digest()`），索引与重放按它查。
        key: String,
        /// 结构化键；宿主手写的条目可以没有。
        jkey: Option<JudgeKey>,
        answer: Answer,
        /// 这次调用的 token 与费用。**合批（一次调用多道题，`merged_by: fuse`）只在同调用号的第一条记整次调用的
        /// tokens/cost，其余记 0**（L7 2026-09-28）：按条目相加即实际花费；按调用号去重的读者取同调用号的最大值
        /// ——2026-09-28 之前的账本每条都记整次费用，取最大对新旧账本都对。
        tokens: u64,
        cost: f64,
        model_id: String,
        /// 这条答案来自本次运行的第几次模型调用（融合后多道题同属一次调用）。
        /// 契约值的 `spent` 按它数调用、按调用计费，重放时从账本读出同一个数（B17）。
        call: u64,
        /// 题声明的校准键；实际命中的记录在 `CalibUsed` 条目（出口 = f(读数, 线)，出口不进账本）。
        calib_ref: Option<Box<CalibRef>>,
        /// 第几层发出（D8.2 从账本派生分层）；0 = 宿主手写的条目。
        layer: u32,
        /// 这条读数与同状态的其他题合并在一次调用里发出时，记合并它的 pass（`fuse`）。
        merged_by: Option<String>,
        /// 状态材料的来源读数（B59：跳含派生链）。步 17 填。
        parents: Vec<String>,
        /// 1 + max(parents.hop)。步 17 填。
        hop: u32,
        /// 复用来源（B40）。步 19 填。
        reused_from: Option<String>,
        /// 置换测量（K 选一，`12`:151、J-15）：发出时测了置换才有，`perms` 与 `mode_share` 成对。
        /// 它是这条读数的一部分：重放与同键复用只从这里取回，缺了就是「没测过」（`untested`）。
        /// 依据：`jpp-core/INTERFACE.md` §四·二·七·五「`mode_share` 连同 `perms` 进账本」；
        /// 修复记录 `过程记录/工程-修复-folio重放.md`。没测时不序列化，旧账本与金样逐字节不变。
        #[serde(default, skip_serializing_if = "Option::is_none")]
        perm: Option<PermMeasure>,
        /// 判断器随答案给的自报置信度（B154；`cut` 的 `stat: "confidence"` 读它）。与 `perm` 同理，它是这条读数的
        /// 一部分：重放与同键复用只从这里取回。判断器没报时不序列化，旧账本与金样逐字节不变。
        /// 依据：B154 (1)（地基/附注/2026-09-26-批6裁定.md §二；`21` 18a 追加项，账本字段提前到步 20j-3）
        #[serde(default, skip_serializing_if = "Option::is_none")]
        confidence: Option<f64>,
    },
    Effect {
        key: String,
        ekey: Option<EffectKey>,
        kind: String,
        /// 输出内容（材料输出时是材料内容，其余是值本身）。
        output: Json,
        /// 输出是材料时的元数据（步 18a 起由 `do` 填；`gen`、`transform` 的材料元数据由实参重算，为空）。
        output_mat: Option<Box<MatMeta>>,
        cost: f64,
        /// 复用来源（步 19，B40、B151）：同一运行里第一次那条的账本键，或跨运行的 `ext:<来源>#<序号>`。
        /// 复用条目 `cost` 为 0。为空不写，没有复用的账本逐字节不变。
        #[serde(default, skip_serializing_if = "Option::is_none")]
        reused_from: Option<String>,
    },
    /// 问人。`answer: None` = 已问未答：重放照样以 `Pending` 结束；续跑时再问，答案另起一条。
    Ask {
        key: String,
        ekey: Option<EffectKey>,
        answer: Option<Answer>,
    },
    /// 不可逆 `do` 的写前意向（B55，`20` §4.3）。步 18 启用，v2 不产生。B200：`--guard` 下不可逆 `do` 推迟到尝试结算后，
    /// 到达执行点只记它；`attempt` 是那次尝试（V5 留位，G2 起填；为空不写）。
    Intent {
        key: String,
        at: u64,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        attempt: Option<AttemptRef>,
    },
    /// 判断器缺席或超时（B32）：重放据此照记的原因给出，不再发。原因以 `fail` 开头（如 `fail:shape`，Z0556）表示
    /// 端口答了但答案不可用（形状不符），不是判断器缺席：它不属缺席类，可以放弃（Z0594）。
    Absent {
        key: String,
        jkey: Option<JudgeKey>,
        cause: String,
        detail: String,
        /// 这一组题实际向后端发出的尝试次数（B32 重试逐次计费；审计重放据此计入预算，B35）。
        /// 只记在一组的第一题上，其余为 0。
        #[serde(default)]
        attempts: u64,
        /// 这次缺席所属的读数：这道题本趟第几次由真站点登记（从 1 起；G5 附录二，格式上的加法）。只有原因为 `absent`
        /// 的记录写；推测、提升登记的缺席与旧账本为空。审计重放按它配读数，不按登记顺序
        #[serde(default, skip_serializing_if = "Option::is_none")]
        nth: Option<u32>,
    },
    /// 实际命中的校准记录（B83 第 6 条、B124；账本 v3 起离开头行）：`cut` 查到某键的记录时，
    /// 该键在账本里最后一条的哈希与本次不同（或没有）才追加一条。重放与续接按键取最后一条。
    /// 不进键索引（`key()` 为空）。
    CalibUsed {
        key: String,
        hash: String,
        record: Json,
    },
    /// 以下六种是未决去向事件（C-1，账本 v4；裁定纸面阶段第五节「共 2」：每种去向在账本里是不同类型的事件）。
    /// 都不进键索引（`key()` 为空）。字段一律可缺省（之后 C-2 往 v4 里加字段，旧条目照读）。
    /// 前五种（`Drop`、`Refine`、`Enrich`、`Handoff`、`Escalate`）是责任去向：`of` 是这次新解除（`Handoff`：新交出）
    /// 的**责任键**，一趟运行里每个责任键至多出现在这五种的一条里。`Reselect` 不在此列：它的 `of` 是**读数键**，
    /// 记的是已决 pick 被代码谓词改选；没有可行候选时同一个键之后还会有一条责任去向（`Unsure(infeasible)` 的去处）。
    /// `site` 是去向动作发生处的源码偏移，逐变体见各自的注释（不是 `JudgeKey.site` 的判断站点；判断由 `of` 指出）。
    /// 显式丢弃：`consume(u, "drop")`；`site` 是 `consume` 调用点。
    Drop {
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        of: Vec<String>,
        #[serde(default)]
        cause: String,
        #[serde(default)]
        site: usize,
    },
    /// 细化：`how` 是 `branch`（`consume(u, "branch")`）、`literalize`（重问），或默认链里被再判取代（`default`）；
    /// `to` 是接替它的判断的账本键（有才写）。`site` 是 `consume` / `literalize` 调用点。
    Refine {
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        of: Vec<String>,
        #[serde(default)]
        cause: String,
        #[serde(default)]
        site: usize,
        #[serde(default)]
        how: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        to: Option<String>,
    },
    /// 补信息：默认链的一轮（`need` 类别、第几轮、取到没有、问类别那道 select 的账本键）。`site` 在 S2 定。
    Enrich {
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        of: Vec<String>,
        #[serde(default)]
        cause: String,
        #[serde(default)]
        site: usize,
        #[serde(default)]
        need: String,
        #[serde(default)]
        round: u32,
        #[serde(default)]
        got: bool,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        asked_by: Option<String>,
    },
    /// 转交：未决随返回值交到 `to`（今天只有 `program`：程序结果）。中途经函数返回不写。`site` 是出口的站点。
    /// 预算停机没观察到的项没有账本键，`of` 为空。
    Handoff {
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        of: Vec<String>,
        #[serde(default)]
        cause: String,
        #[serde(default)]
        site: usize,
        #[serde(default)]
        to: String,
    },
    /// 升级：`escalate(u, …)` 交给人；`ask` 是问人那条 `Ask` 的账本键。`site` 是 `escalate` 调用点。
    /// 它写在问人之前：问人因 `budget.escalate` 挂起时，`ask` 指的 `Ask` 条目暂时还不在账本里，续跑问到时补上。
    Escalate {
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        of: Vec<String>,
        #[serde(default)]
        cause: String,
        #[serde(default)]
        site: usize,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        ask: Option<String>,
    },
    /// 改选／跳过（C-4 `feasible`）：`of` 是读数的账本键；`from` 原 pick，`chosen` 改选到的候选
    /// （没有可行候选时空，出口 `Unsure(infeasible)`），`skipped` 是被否决的候选与概率。`site` 是 `cut` 站点。
    Reselect {
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        of: Vec<String>,
        #[serde(default)]
        site: usize,
        #[serde(default)]
        from: usize,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        chosen: Option<usize>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        skipped: Vec<Skipped>,
    },
    /// 本轮实际花费（C-3，主控 Z0171 第二轮答复）：带上游余额时，或账本头记着本段余额而这一趟没带余额时（Z0384 R11），
    /// 由运行时写，一轮结束时一条（正常返回、挂起、运行期错误三条路都写；审计重放不写）。交回余额按它算（`jpp_runtime::BudgetCarry::after_round`），所以首跑、
    /// 失败后与只凭账本重放给出同一份余额。`secs` 是本轮判断调用的累计时延（账本里记的判断时延，不是重放的实际耗时）；
    /// `started: false` = 上游深度已到上限、整轮没开跑。不进键索引（`key()` 为空）。
    Spent {
        calls: u64,
        usd: f64,
        secs: f64,
        asks: u64,
        started: bool,
    },
    /// 效应因参数里有未决值没发出（B0492 S3，J-05 草案 (5a)）：`kind` 是效应名，`of` 是那份未决的责任键，
    /// `site` 是效应调用点。与 `Reselect` 一样不是责任去向，不计入「每键至多一条」。
    Skip {
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        of: Vec<String>,
        #[serde(default)]
        kind: String,
        #[serde(default)]
        site: usize,
        /// 所在尝试（V5 留位，C3 起填；为空不写）
        #[serde(default, skip_serializing_if = "Option::is_none")]
        attempt: Option<AttemptRef>,
    },
    /// 只收紧的余额记录（C-3 Z0384）：续跑时宿主给的余额（没开重新授权）在某一项上小于账本剩余，开跑前写这一条，
    /// 本段剩余从这里起逐项不超过它（`secs` 为空 = 时延不设限）。它只会收紧，不触「新段只靠显式开关」；交回余额按
    /// 本段依次扣 `Spent`、遇它取小算出，重放照账本复现。不进键索引（`key()` 为空）。
    CarryCap {
        calls: u64,
        usd: f64,
        secs: Option<f64>,
        asks: u64,
        /// 深度上限（Z0384 复核 R2-a：宿主给的深度上限更小时，交回也取小）
        depth_cap: u32,
    },

    // ── 账本 v5（步 34 V5，B196）：骨架事件。本步只声明，运行时不写；C3、G2、G4 起写。都不进键索引。 ──
    // 不定输入（R6 (a)、R13、冻结清单 §4.1 (a)）
    /// 一次尝试的不定输入（B196）：纪元、版本快照、立刻读的版本、过时依赖。
    Attempt {
        program: String,
        n: u64,
        host_epoch: u64,
        flush_epoch: u64,
        #[serde(default)]
        snapshot: Snapshot,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        peeks: Vec<Peek>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        stale: Vec<String>,
    },
    /// 一个刷新点发出的全部调用（R11 第 3、7 条）；读数在同键的 `Judge` 条目里。`flushed_at` 为 Z0403 留位，
    /// 是内容，不进重放比对。
    Flush {
        f: u64,
        calls: Vec<FlushCall>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        flushed_at: Option<String>,
    },
    /// 宿主事件：宿主纪元加一，定时的拍也是它（R12）。
    HostEvent { epoch: u64, event: Json },
    /// 多写者合并顺序（R7）：线上字段名 `type`。
    Merge {
        attempt: AttemptRef,
        cell: String,
        key: String,
        hash: String,
        version: u64,
        #[serde(rename = "type")]
        kind: MergeKind,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        cause: Option<String>,
    },
    /// 不透明（或未声明）宿主动作在某个世界纪元上的结果，重放从这里取（R8）。
    Opaque {
        key: String,
        world_epoch: u64,
        decl: OpaqueDecl,
        value: Json,
    },
    /// 透明宿主动作的结果哈希，重放照算比对（R8）。
    Transparent {
        key: String,
        port_reads: Vec<PortRead>,
        hash: String,
    },
    // 契约性结果（R6 (b)、R13、§4.1 (b)），都带 `attempt`
    /// 程序单元的新一版（R2）；`cause` 只在 `unsure` 时有（如 `violation`、`deadline`）。
    Publish {
        attempt: AttemptRef,
        version: u64,
        state: PubState,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        cause: Option<String>,
        hash: String,
    },
    /// 去向的规范形式（B196，四类六形式加记号）。v4 的六种去向事件是旧形式，照读照写，互转见 [`Duty`]。
    Duty(Duty),
    /// 有结论时还欠着的一笔未决（R9），一笔一条。同一判断（同键）的几个视图没人接只记这一笔（B162；Z0593）：
    /// 主记号在 `mark`，其余视图的记号在 `also`（格式上的加法，没有合并时不写）。
    Violation {
        attempt: AttemptRef,
        mark: DebtMark,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        also: Vec<DebtMark>,
    },
    /// 题没有发出（R13）：不是缺席，也不是读数。`key` 是题内容键；字段名 `reason` 取冻结清单 §4.1（V5 复核 A1）。
    Unasked {
        attempt: AttemptRef,
        key: String,
        payer: String,
        reason: StopCause,
    },
    /// 预算、深度或截止到限：只停发，程序不停（R11 第 4 条、R16）。
    Stop {
        attempt: AttemptRef,
        cause: StopCause,
    },
    /// B200：`--guard` 下不可逆 `do` 这一趟没有执行（有违规、挂起或出错，见 [`WithheldCause`]）；`key` 是那条
    /// `Intent` 的键。同键 `Intent` 可以在它之后再出现一次（下一趟再到达执行点，G2 附录三）。
    Withheld {
        attempt: AttemptRef,
        key: String,
        cause: WithheldCause,
    },
}

/// `Reselect` 里一个被否决的候选：下标与判断器给它的概率。
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Skipped {
    pub k: usize,
    pub p: f64,
}

impl Entry {
    /// 一条判断记录（其余字段取缺省）。测试与宿主经它构造，不写结构体字面量。
    pub fn judge(
        key: impl Into<String>,
        answer: Answer,
        tokens: u64,
        cost: f64,
        model_id: impl Into<String>,
        call: u64,
    ) -> Entry {
        Entry::Judge {
            key: key.into(),
            jkey: None,
            answer,
            tokens,
            cost,
            model_id: model_id.into(),
            call,
            calib_ref: None,
            layer: 0,
            merged_by: None,
            parents: vec![],
            hop: 0,
            reused_from: None,
            perm: None,
            confidence: None,
        }
    }
    /// 一条效应记录（`output_mat` 取缺省）。
    pub fn effect(ekey: EffectKey, kind: &str, output: Json, cost: f64) -> Entry {
        Entry::Effect {
            key: ekey.digest(),
            ekey: Some(ekey),
            kind: kind.into(),
            output,
            output_mat: None,
            cost,
            reused_from: None,
        }
    }
    /// 账本键另有来历（`repeat`、`absent` 的派生键）的效应记录。
    pub fn effect_keyed(key: String, kind: &str, output: Json, cost: f64) -> Entry {
        Entry::Effect {
            key,
            ekey: None,
            kind: kind.into(),
            output,
            output_mat: None,
            cost,
            reused_from: None,
        }
    }
    pub fn key(&self) -> &str {
        match self {
            Entry::Judge { key, .. }
            | Entry::Effect { key, .. }
            | Entry::Ask { key, .. }
            | Entry::Intent { key, .. }
            | Entry::Absent { key, .. } => key,
            Entry::CalibUsed { .. }
            | Entry::Drop { .. }
            | Entry::Refine { .. }
            | Entry::Enrich { .. }
            | Entry::Handoff { .. }
            | Entry::Escalate { .. }
            | Entry::Reselect { .. }
            | Entry::Spent { .. }
            | Entry::CarryCap { .. }
            | Entry::Skip { .. }
            | Entry::Attempt { .. }
            | Entry::Flush { .. }
            | Entry::HostEvent { .. }
            | Entry::Merge { .. }
            | Entry::Opaque { .. }
            | Entry::Transparent { .. }
            | Entry::Publish { .. }
            | Entry::Duty(_)
            | Entry::Violation { .. }
            | Entry::Unasked { .. }
            | Entry::Stop { .. }
            | Entry::Withheld { .. } => "",
        }
    }
    /// 是不是未决去向事件（C-1 的六种、`Skip`，与 V5 的规范形式 `Duty`）。
    pub fn is_duty_event(&self) -> bool {
        matches!(
            self,
            Entry::Drop { .. }
                | Entry::Refine { .. }
                | Entry::Enrich { .. }
                | Entry::Handoff { .. }
                | Entry::Escalate { .. }
                | Entry::Reselect { .. }
                | Entry::Skip { .. }
                | Entry::Duty(_)
        )
    }
}

/// 作者声明线（B128）在 `CalibUsed` 里的键前缀（B142，步 20j-1）：`declared:<校准键>@<站点>`，记录
/// `{line: "declared", hi, lo, site}`。它不是校准记录：比对、补回校准库、导出夹具都跳过它，由运行时在站点比。
pub const DECLARED_PREFIX: &str = "declared:";

/// 账本头里记录但不比对的预算（B61：续接时预算变大不报 `W-header`）。
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BudgetRecord {
    pub calls: u64,
    pub cost: f64,
}

/// 上游交下来的整场余额（C-3，主控 Z0171；裁定纸面阶段第五节「共 1」）：还剩多少调用、多少钱、多少时延预算
/// （判断调用的累计秒数，不是墙钟截止时间）、多少次问人，深度已到第几层、上限多少。记在账本头里，只凭账本重放
/// 从这里取、不从宿主取（B35：首跑在哪里停，重放就在哪里停）；续跑时记而不比（与 `budget` 一样）。
/// 运行时的包装是 `jpp_runtime::BudgetCarry`；命令行 `--carry-in`/`--carry-out` 的文件也是这个形状。
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CarryRecord {
    pub calls: u64,
    pub cost: f64,
    /// 时延预算余额（秒）；`None` = 不设限
    pub latency_p95: Option<f64>,
    pub escalate: u64,
    /// 深度已到第几层（V5：原名 `depth_at`，旧名照读）。G4 之前计法不变：调用栈深度与跨运行跳数是同一个计数、
    /// 一轮交回加一；G4 起只按跨程序触发链计（`12` R11 第 5 条）。深度检查读它。
    #[serde(alias = "depth_at")]
    pub hop: u32,
    /// 续跑计数（V5 留位，R11 第 5 条「续跑不计」深度，另记在这里）；G4 之前恒为 0。
    #[serde(default)]
    pub round: u32,
    pub depth_cap: u32,
}

/// 账本头的比对集合：**唯一定义处**，十字段，任一不同报 `W-header`、不承诺重放一致（J-18）。
/// 比对哪些字段随场合（[`HeaderCompare`]）：只凭账本重放不比 `calib_hash`，续接全比（B77）。
/// 命中的校准记录另由 [`Ledger::set_header_checked`] 与 `CalibUsed` 条目逐键比（B124：
/// 「十字段加 `CalibUsed` 条目与视图逐键比」；原第十一字段 `calib_used_hash` 自账本 v3 移出）。
/// `lib_version`、`bank_version`、`ir_version`、`entry_hash` 在 v2 先存在、恒为 `None`
/// （标准库、题库、IR 版本与入口参数在步 12、14b、27 接上）。
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HeaderCompared {
    pub model_id: String,
    pub render_version: String,
    pub handler_version: String,
    /// 模型档案的哈希。**`None` = 本次未加载档案**，线与 δ 是代码兜底值——
    /// 「用了兜底」与「档案恰好等于兜底」在账本上要分得开，换档案重放也要察觉得到。
    pub profile_hash: Option<String>,
    /// 行为承载子集的摘要：告诉看账本的人那次不同是线和 δ 也变了，还是只改了说明。
    pub behavior_hash: Option<String>,
    /// 校准库的哈希：出口 = f(读数, 线)，读数进了账本，线的来源靠它比对。
    /// 空库也有自己的哈希；`None` 只有「早于这个字段」一个意思。
    pub calib_hash: Option<String>,
    pub lib_version: Option<String>,
    pub bank_version: Option<String>,
    pub ir_version: Option<String>,
    pub entry_hash: Option<String>,
    /// 生成器模型（步 19）：宿主给了真实生成器（CLI `--gen-model`）时填；为空时生成物的缓存键按
    /// `model_id` 算。为空不写，旧账本与金样逐字节不变。依据：B151（生成物缓存键要带生成器）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gen_model: Option<String>,
    /// 生成器画像的哈希（步 19，主会话 2026-09-26：与判断器画像同一套，不同报 `W-header`）。为空不写。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gen_profile_hash: Option<String>,
    /// 追踪编号（V5，裁定五十九第 21a 条：比对集合并上 `trace`）。C4 起写；为空不写。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub trace: Option<TraceId>,
    /// 本账本里的段与父段（冻结清单 §4.1 账本头；V5）。续接只会加段：旧的是新的前缀即不算不同。为空不写。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub segments: Vec<Segment>,
    /// 键版本（裁定三十七，V5 留位；落地时再填）。为空不写。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub key_version: Option<String>,
}

/// 账本头比对的场合（B77，`12` J-18 行）：同一份账本被读回来时是哪种用法。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HeaderCompare {
    /// 续接（以及同一账本上再跑一趟）：十字段全比，另逐键比命中记录。续接会命中首跑没命中的键，
    /// 那些键的线来自装载库，所以 `calib_hash` 也要比。
    Resume,
    /// 只凭账本的审计重放：不比 `calib_hash`，只逐键比命中记录（`CalibUsed` 条目）。
    Replay,
}

/// 命中记录集合的哈希（B77）：`[[键, 记录全文], …]` 按键排序后取哈希。空集合也有哈希。
/// 输入是「键 → 记录全文」（`CalibUsed` 条目的 `record`，或运行时按同一批键从校准视图取出的记录）。
/// 账本 v3 起它不再存进头，只在 `W-header` 报文里给出比对双方的集合哈希（B83 报文不变，B124）。
pub fn calib_used_hash<'a>(records: impl IntoIterator<Item = (&'a str, &'a Json)>) -> String {
    let sorted: BTreeMap<&str, &Json> = records.into_iter().collect();
    let arr = Json::Array(
        sorted
            .into_iter()
            .map(|(k, r)| Json::Array(vec![Json::String(k.to_string()), r.clone()]))
            .collect(),
    );
    hash_of(&["calib-used", &jpp_ir::key::canon(&arr)])
}

impl HeaderCompared {
    /// 与另一头不同的字段（名字、旧值、新值），十字段全比（续接口径）。
    pub fn diff(&self, new: &HeaderCompared) -> Vec<(&'static str, String, String)> {
        self.diff_in(new, HeaderCompare::Resume)
    }

    /// 按场合比对。比对只在这里（J-18；B77：重放跳过 `calib_hash`）。
    pub fn diff_in(
        &self,
        new: &HeaderCompared,
        mode: HeaderCompare,
    ) -> Vec<(&'static str, String, String)> {
        let o = |x: &Option<String>| x.clone().unwrap_or_else(|| "（无）".into());
        let mut d = vec![];
        let mut s = |name: &'static str, a: String, b: String| {
            if a != b {
                d.push((name, a, b));
            }
        };
        s("model_id", self.model_id.clone(), new.model_id.clone());
        s(
            "render_version",
            self.render_version.clone(),
            new.render_version.clone(),
        );
        s(
            "handler_version",
            self.handler_version.clone(),
            new.handler_version.clone(),
        );
        s("profile_hash", o(&self.profile_hash), o(&new.profile_hash));
        s(
            "behavior_hash",
            o(&self.behavior_hash),
            o(&new.behavior_hash),
        );
        if mode == HeaderCompare::Resume {
            s("calib_hash", o(&self.calib_hash), o(&new.calib_hash));
        }
        s("lib_version", o(&self.lib_version), o(&new.lib_version));
        s("bank_version", o(&self.bank_version), o(&new.bank_version));
        s("ir_version", o(&self.ir_version), o(&new.ir_version));
        s("entry_hash", o(&self.entry_hash), o(&new.entry_hash));
        // 步 19：生成器模型与画像两种场合都比（与判断器画像同一套）
        s("gen_model", o(&self.gen_model), o(&new.gen_model));
        s(
            "gen_profile_hash",
            o(&self.gen_profile_hash),
            o(&new.gen_profile_hash),
        );
        // V5（裁定五十九第 21a 条，取并集）：`schema` 由解码的版本闸承担（读不了的版本直接拒，照读的 v3、v4
        // 与 v5 同族）；`trace`、`key_version` 两种场合都比；`segments` 旧的是新的前缀即不算不同（续接只加段）。
        let t = |x: &Option<TraceId>| {
            x.as_ref()
                .map(|t| t.as_str().to_string())
                .unwrap_or_else(|| "（无）".into())
        };
        s("trace", t(&self.trace), t(&new.trace));
        s("key_version", o(&self.key_version), o(&new.key_version));
        if !new.segments.starts_with(&self.segments) {
            let segs = |v: &[Segment]| {
                v.iter()
                    .map(|x| x.seg.as_str().to_string())
                    .collect::<Vec<_>>()
                    .join(",")
            };
            d.push(("segments", segs(&self.segments), segs(&new.segments)));
        }
        d
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Header {
    pub budget: BudgetRecord,
    pub compared: HeaderCompared,
    /// 上游余额（C-3）：宿主给了才有；为空不写，旧账本与金样逐字节不变。不进比对字段
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub carry: Option<CarryRecord>,
    /// 本轮开跑时账本已有的条目数（C-3）：交回余额只认它之后的 `Spent`（续跑账本里有前几趟的）。随 `carry` 一起写
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub carry_from: Option<u64>,
    /// 引擎默认深度上限（G4b，裁定六十四，`12` R11 第 5 条）：本趟用到引擎默认时（带余额，或程序没声明
    /// `budget.depth`）写；重放以它为准、不读引擎配置。为空不写，并进 v5（C-3 `CarryCap` 同一先例），不进比对集合
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub depth_cap_default: Option<u32>,
}

impl Header {
    /// 账本头（其余哈希字段为 `None`，用 `with_*` 设）。测试与宿主经它构造，不写结构体字面量。
    pub fn new(
        budget_calls: u64,
        budget_cost: f64,
        model_id: &str,
        render_version: &str,
        handler_version: &str,
    ) -> Header {
        Header {
            budget: BudgetRecord {
                calls: budget_calls,
                cost: budget_cost,
            },
            compared: HeaderCompared {
                model_id: model_id.into(),
                render_version: render_version.into(),
                handler_version: handler_version.into(),
                profile_hash: None,
                behavior_hash: None,
                calib_hash: None,
                lib_version: None,
                bank_version: None,
                ir_version: None,
                entry_hash: None,
                gen_model: None,
                gen_profile_hash: None,
                trace: None,
                segments: vec![],
                key_version: None,
            },
            carry: None,
            carry_from: None,
            depth_cap_default: None,
        }
    }
    /// 引擎默认深度上限（G4b）：`None` 与不调相同
    pub fn with_depth_cap_default(mut self, d: Option<u32>) -> Header {
        self.depth_cap_default = d;
        self
    }
    /// 上游余额（C-3）：`None` 与不调相同
    pub fn with_carry(mut self, c: Option<CarryRecord>, from: Option<u64>) -> Header {
        self.carry_from = c.as_ref().and(from);
        self.carry = c;
        self
    }
    pub fn with_profile_hash(mut self, h: Option<String>) -> Header {
        self.compared.profile_hash = h;
        self
    }
    pub fn with_behavior_hash(mut self, h: Option<String>) -> Header {
        self.compared.behavior_hash = h;
        self
    }
    pub fn with_calib_hash(mut self, h: Option<String>) -> Header {
        self.compared.calib_hash = h;
        self
    }
    /// 宿主入口参数的哈希（步 14b-0：`jpp run --input` 的规范化 JSON 哈希；不带入口参数时 `None`）。
    pub fn with_entry_hash(mut self, h: Option<String>) -> Header {
        self.compared.entry_hash = h;
        self
    }
    /// 生成器模型与画像哈希（步 19；宿主没给真实生成器时都为 `None`）。
    pub fn with_gen(mut self, model: Option<String>, profile_hash: Option<String>) -> Header {
        self.compared.gen_model = model;
        self.compared.gen_profile_hash = profile_hash;
        self
    }
    /// 标准库与题库的版本（步 27，B48）：`lib_version` 是本次运行装载的标准库文件的内容哈希，`bank_version`
    /// 是 `bank.json` 的 `version`；没装载对应文件时为 `None`（账本逐字节不变）。两件事不挤一个字段。
    pub fn with_versions(mut self, lib: Option<String>, bank: Option<String>) -> Header {
        self.compared.lib_version = lib;
        self.compared.bank_version = bank;
        self
    }
    pub fn model_id(&self) -> &str {
        &self.compared.model_id
    }
    /// 生成物缓存键里的生成器模型（步 19）：有 `gen_model` 取它，否则取 `model_id`。
    pub fn gen_model_or_default(&self) -> &str {
        self.compared
            .gen_model
            .as_deref()
            .unwrap_or(&self.compared.model_id)
    }
}

/// 账本（内存形态）。落盘只经 [`Ledger::encode`] / [`Ledger::decode`]：没有第二条序列化路径。
#[derive(Clone, Debug, Default)]
pub struct Ledger {
    pub header: Option<Header>,
    pub entries: Vec<Entry>,
    index: HashMap<String, usize>,
    pub header_warning: Option<String>,
    /// **实际命中的校准记录**（键 → `{hash, record}`）：`CalibUsed` 条目按键取最后一条的**派生视图**，
    /// 由 [`Ledger::put`] 与 [`Ledger::rebuild_index`] 维护，只读；写入只经 [`Ledger::note_calib_used`]。
    /// 只凭账本重放时（不给 `--calib` / `--fixtures`），宿主用这里补回当时的线，出口因此逐字节一致。
    pub calib_used: BTreeMap<String, Json>,
    /// 每条条目的追踪上下文（C-2）：与 `entries` 同序，可以比 `entries` 短（旧账本、直接往 `entries` 推的条目
    /// 没有）；缺的按无追踪算。只经 [`Ledger::put`] 系列写入，只读经 [`Ledger::trace_at`]。
    traces: Vec<Option<TraceCtx>>,
    /// 此后追加的条目盖的上下文（[`Ledger::set_trace`]）；`None` = 不盖章。
    cur_trace: Option<TraceCtx>,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct HeadLine {
    version: u32,
    header: Option<Header>,
}

/// 账本一行：`{seq, prev, entry}` 加可缺省的 `trace`（C-2：这一条属于哪条调用链、哪一段、谁调起的这一段）。
/// 追踪字段放行外壳，不放进 [`Entry`] 各变体：`Entry` 的结构体字面量遍布 `jpp-runtime`，往变体里加字段要改十个
/// 运行时文件，而追踪身份不是条目本体（主控 Z0245 定）。`trace` 是行文本的一部分，所以进 `prev` 链：改任何一行
/// （不是最后一行）的 `trace`，下一行的链就对不上，`decode` 拒收。没有 `trace` 的旧 v4 账本照读（值为空）。
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct EntryLine {
    seq: u64,
    prev: String,
    entry: Entry,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    trace: Option<TraceCtx>,
}

/// 解码时的截断报告：末行半写被丢掉。
#[derive(Clone, Debug, PartialEq)]
pub struct Truncated {
    pub kept: usize,
    pub dropped_bytes: usize,
}

impl Truncated {
    pub fn render(&self) -> String {
        // 依据：20 §九 账本行「末行半写 → 截断到最后一条完整并报 Truncated」；21 步 7
        format!(
            "W-ledger-truncated: 账本末行半写（{} 字节），已截断到第 {} 条完整条目",
            self.dropped_bytes, self.kept
        )
    }
}

/// 账本一行文本的链哈希：下一行的 `prev` 就是它。
pub fn line_hash(line: &str) -> String {
    hash_of(&["ledger-line", line])
}

/// 头行文本（不含换行）。与 [`encode_entry`] 一起是账本唯一的序列化路径：[`Ledger::encode`] 与
/// 逐行落盘的文件后端（`jpp::store::LedgerFile`，步 18b）都经这两个函数。
pub fn encode_head(header: &Option<Header>) -> String {
    serde_json::to_string(&HeadLine {
        version: LEDGER_VERSION,
        header: header.clone(),
    })
    .expect("账本头可序列化")
}

/// 第 `seq` 条（从 1 起）的行文本（不含换行）；`prev` 是上一行的 [`line_hash`]（首条对头行）。
pub fn encode_entry(seq: u64, prev: &str, e: &Entry, trace: Option<&TraceCtx>) -> String {
    serde_json::to_string(&EntryLine {
        seq,
        prev: prev.to_string(),
        entry: e.clone(),
        trace: trace.cloned(),
    })
    .expect("账本条目可序列化")
}

impl Ledger {
    pub fn new() -> Ledger {
        Ledger::default()
    }
    pub fn rebuild_index(&mut self) {
        self.index.clear();
        self.calib_used.clear();
        for (i, e) in self.entries.iter().enumerate() {
            if !e.key().is_empty() {
                // 同键多条时后写的生效：只有「已问未答 → 答」这一种（见 `put_answer`）
                self.index.insert(e.key().to_string(), i);
            }
            if let Entry::CalibUsed { key, hash, record } = e {
                self.calib_used.insert(
                    key.clone(),
                    serde_json::json!({"hash": hash, "record": record}),
                );
            }
        }
    }
    /// 记下一次校准记录命中（B83 第 6 条、B124）：该键最后一条 `CalibUsed` 的哈希与本次相同则不追加，
    /// 否则追加一条。首跑每键首次命中一条；同一账本再跑一趟命中同一记录不追加（账本逐字节不变）；
    /// 续接时记录换了才追加，重放按键取最后一条即那一趟的线。
    pub fn note_calib_used(&mut self, key: &str, hash: &str, record: Json) {
        if self.calib_used_same(key, hash) {
            return;
        }
        self.put(Entry::CalibUsed {
            key: key.to_string(),
            hash: hash.to_string(),
            record,
        });
    }
    /// 该键最后一条 `CalibUsed` 的哈希是否就是 `hash`（是则不必再追加，B124）。
    pub fn calib_used_same(&self, key: &str, hash: &str) -> bool {
        self.calib_used
            .get(key)
            .and_then(|v| v.get("hash"))
            .and_then(|h| h.as_str())
            == Some(hash)
    }
    /// 换头并比对（B77、B124；比对只在这里）：十字段按场合比，另把 `calib_used`（`CalibUsed` 条目按键取
    /// 最后一条）与当前校准视图逐键比——某键视图里的记录哈希与条目不同，或视图里没有该键，即为变化。
    /// 有差异时 `header_warning` 为 `W-header`；命中记录变化的一段写作「calib_used_hash 旧 X 新 Y」
    /// （X、Y 为比对双方这批键的记录集合哈希）并追加「变化的键：…（共 n 条）」，与账本 v2 的报文相同
    /// （B83 报文不变）。`视图` 按键返回当前视图里的记录全文。
    pub fn set_header_checked(
        &mut self,
        h: Header,
        mode: HeaderCompare,
        视图: &dyn Fn(&str) -> Option<Json>,
    ) {
        let mut parts: Vec<String> = match &self.header {
            Some(old) => old
                .compared
                .diff_in(&h.compared, mode)
                .iter()
                .map(|(n, a, b)| format!("{n} 旧 {a} 新 {b}"))
                .collect(),
            None => vec![],
        };
        // B142（步 20j-1）：`declared:` 键是作者声明线，不在校准视图里；它由运行时在 `cut` 站点比（不在这里比）
        let 记录键 = |k: &&String| !k.starts_with(DECLARED_PREFIX);
        let 现: Vec<(String, Json)> = self
            .calib_used
            .keys()
            .filter(记录键)
            .filter_map(|k| 视图(k).map(|j| (k.clone(), j)))
            .collect();
        let 变化的键: Vec<String> = self
            .calib_used
            .iter()
            .filter(|(k, _)| 记录键(k))
            .filter(|(k, v)| {
                let 现哈希 = 视图(k).map(|j| hash_of(&[&j.to_string()]));
                现哈希.as_deref() != v.get("hash").and_then(|h| h.as_str())
            })
            .map(|(k, _)| k.replace('\u{1f}', ":"))
            .collect();
        if !变化的键.is_empty() {
            let 旧 = calib_used_hash(
                self.calib_used
                    .iter()
                    .filter(|(k, _)| 记录键(k))
                    .map(|(k, v)| (k.as_str(), v.get("record").unwrap_or(&Json::Null))),
            );
            let 新 = calib_used_hash(现.iter().map(|(k, j)| (k.as_str(), j)));
            let 列: Vec<&str> = 变化的键.iter().take(3).map(|k| k.as_str()).collect();
            parts.push(format!(
                "calib_used_hash 旧 {旧} 新 {新}；变化的键：{}（共 {} 条）",
                列.join("、"),
                变化的键.len()
            ));
        }
        if !parts.is_empty() {
            // 依据：J-18（账本头不同即不承诺重放一致）；B61（预算不比对）；B77、B83、B124（命中记录逐键比）
            self.header_warning = Some(format!(
                "W-header: 账本头不同，不承诺重放一致：{}",
                parts.join("；")
            ));
        }
        self.header = Some(h);
    }
    /// 账本头（J-18）：比对集合里任一字段不同即报 `W-header`，不承诺重放一致。预算记录但不比对（B61）。
    /// 续接口径（全比）；只凭账本重放用 [`Ledger::set_header_in`] 传 [`HeaderCompare::Replay`]。
    pub fn set_header(&mut self, h: Header) {
        self.set_header_in(h, HeaderCompare::Resume)
    }
    /// 按场合比对后换头（B77）。
    pub fn set_header_in(&mut self, h: Header, mode: HeaderCompare) {
        if let Some(old) = &self.header {
            let d = old.compared.diff_in(&h.compared, mode);
            if !d.is_empty() {
                let parts: Vec<String> = d
                    .iter()
                    .map(|(n, a, b)| format!("{n} 旧 {a} 新 {b}"))
                    .collect();
                // 依据：J-18（账本头不同即不承诺重放一致）；B61（预算不比对）
                self.header_warning = Some(format!(
                    "W-header: 账本头不同，不承诺重放一致：{}",
                    parts.join("；")
                ));
            }
        }
        self.header = Some(h);
    }
    pub fn get(&self, key: &str) -> Option<&Entry> {
        self.index.get(key).map(|i| &self.entries[*i])
    }
    /// 键索引指向的条目下标（同键多条时是最后一条）
    pub fn position(&self, key: &str) -> Option<usize> {
        self.index.get(key).copied()
    }
    /// 同键最后一条 `Intent` 之后有没有同键 `Withheld`（B55 按位置判，G2 附录三）：有 = 那一趟确知没有执行；
    /// 没有、也没有结果 = 结果未知。键不是 `Intent` 时为 `false`。
    pub fn intent_withheld(&self, intent_key: &str) -> bool {
        match self.index.get(intent_key) {
            Some(&i) if matches!(self.entries[i], Entry::Intent { .. }) => self.entries[i + 1..]
                .iter()
                .any(|e| matches!(e, Entry::Withheld { key, .. } if key == intent_key)),
            _ => false,
        }
    }
    /// 唯一的追加入口：同键已有即不写（账本是审计物不是缓存）。
    pub fn put(&mut self, e: Entry) {
        let k = e.key().to_string();
        if !k.is_empty() && self.index.contains_key(&k) {
            return;
        }
        if !k.is_empty() {
            self.index.insert(k, self.entries.len());
        }
        if let Entry::CalibUsed { key, hash, record } = &e {
            self.calib_used.insert(
                key.clone(),
                serde_json::json!({"hash": hash, "record": record}),
            );
        }
        self.push_stamped(e);
    }
    /// 同键另起一条（只增，不改旧条目），索引指向新条目。只接受两种情形：
    /// - 已问未答的 `ask` 在续跑时得到了答案：旧条目是同键的未答 `Ask`、新条目是已答 `Ask`；
    /// - 被扣下的不可逆 `do` 下一趟再到达执行点（G2 附录三，主控 Z0564 后的 B55 决定）：旧条目是同键 `Intent`、
    ///   它之后有同键 `Withheld`，新条目是 `Intent`。每一趟因此有自己的意向，执行后被杀仍能认出「结果未知」。
    pub fn put_answer(&mut self, e: Entry) {
        let k = e.key().to_string();
        let replaces_unanswered = matches!(self.get(&k), Some(Entry::Ask { answer: None, .. }));
        let is_answer = matches!(
            &e,
            Entry::Ask {
                answer: Some(_),
                ..
            }
        );
        let 再意向 = matches!(&e, Entry::Intent { .. }) && self.intent_withheld(&k);
        if !(replaces_unanswered && is_answer) && !再意向 {
            return self.put(e);
        }
        self.index.insert(k, self.entries.len());
        self.push_stamped(e);
    }
    /// 追加一条并盖上当前段的追踪上下文（[`Ledger::set_trace`]）；`traces` 先补齐到 `entries` 的长度。
    fn push_stamped(&mut self, e: Entry) {
        self.traces.resize(self.entries.len(), None);
        self.entries.push(e);
        self.traces.push(self.cur_trace.clone());
    }
    /// 设此后追加的条目所属的一段（C-2）。`None` = 不盖章。已有的条目不改。
    pub fn set_trace(&mut self, ctx: Option<TraceCtx>) {
        self.cur_trace = ctx;
    }
    /// 当前段的上下文（[`Ledger::set_trace`] 设的）。
    pub fn current_trace(&self) -> Option<&TraceCtx> {
        self.cur_trace.as_ref()
    }
    /// 第 `i` 条（0 起）条目的追踪上下文；没有（旧账本、没盖章）为 `None`。
    pub fn trace_at(&self, i: usize) -> Option<&TraceCtx> {
        self.traces.get(i).and_then(|t| t.as_ref())
    }
    /// 账本里最后一条带追踪上下文的条目的上下文（续跑推导新段、审计重放沿用它）。
    pub fn last_trace(&self) -> Option<&TraceCtx> {
        self.traces.iter().rev().find_map(|t| t.as_ref())
    }
    /// 账本里出现过的不同段的个数（续跑序号：新段的推导带它，仍是确定的）。
    pub fn span_count(&self) -> usize {
        let mut seen: Vec<&SpanId> = vec![];
        for t in self.traces.iter().flatten() {
            if !seen.contains(&&t.span) {
                seen.push(&t.span);
            }
        }
        seen.len()
    }
    /// 命中记录（`CalibUsed` 条目按键取最后一条）的集合哈希（B77）。
    pub fn calib_used_hash(&self) -> String {
        calib_used_hash(
            self.calib_used
                .iter()
                .map(|(k, v)| (k.as_str(), v.get("record").unwrap_or(&Json::Null))),
        )
    }
    pub fn len(&self) -> usize {
        self.entries.len()
    }
    /// 账本里与 `e` 完全相同的条目有几条（C-1：续跑、重放同一程序时去向事件按出现次数比，不重复写）。
    pub fn count_same(&self, e: &Entry) -> usize {
        self.entries.iter().filter(|x| *x == e).count()
    }
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// v3 编码：JSONL 链式（见模块文档）。确定性：同一内容逐字节相同。
    pub fn encode(&self) -> String {
        let head = encode_head(&self.header);
        let mut out = String::new();
        let prev = line_hash(&head);
        out.push_str(&head);
        out.push('\n');
        for line in self.encode_from(0, &prev).0 {
            out.push_str(&line);
            out.push('\n');
        }
        out
    }

    /// 从第 `start` 条（0 起）编到末尾：返回各行文本与最后一行的链哈希（没有条目时即 `prev`）。
    /// 逐行落盘的文件后端用它续写；[`Ledger::encode`] 用它写全部条目。
    pub fn encode_from(&self, start: usize, prev: &str) -> (Vec<String>, String) {
        let mut prev = prev.to_string();
        let mut lines = vec![];
        for (i, e) in self.entries.iter().enumerate().skip(start) {
            let line = encode_entry(i as u64 + 1, &prev, e, self.trace_at(i));
            prev = line_hash(&line);
            lines.push(line);
        }
        (lines, prev)
    }

    /// v5 解码（v3、v4 照读）。返回账本与截断报告（末行半写时）。v1 报 `E-ledger-archived`；v2 报 `E-ledger-v2`
    /// （迁移见 `jpp::store::migrations::ledger_v2`）；链断、未知字段、完整行解析失败报 `E-ledger-corrupt`
    /// 并指出行号。
    pub fn decode(text: &str) -> Result<(Ledger, Option<Truncated>), String> {
        // 依据：20 §2.3 jpp-ledger「v1 账本不迁移，decode 报 E-ledger-archived」；21 E5
        if let Ok(Json::Object(m)) = serde_json::from_str::<Json>(text) {
            if m.contains_key("entries") {
                return Err(format!(
                    "E-ledger-archived: 这是 v1 格式的账本（整份 JSON，键只存哈希），已归档不迁移。修法：用标签 {V1_ARCHIVE_TAG} 处的二进制重放它"
                ));
            }
        }
        let mut lines: Vec<&str> = text.split('\n').collect();
        let ends_clean = text.ends_with('\n');
        if ends_clean {
            lines.pop();
        }
        let Some(first) = lines.first() else {
            return Err("E-ledger-corrupt: 账本是空的，没有头行".into());
        };
        // 依据：21 E5、B124 Q3（v2 迁移而不归档）
        if let Ok(j) = serde_json::from_str::<Json>(first)
            && j.get("version").and_then(Json::as_u64) == Some(2)
        {
            return Err(format!(
                "E-ledger-v2: 这是 v2 格式的账本（步 7 至 18c）。修法：jpp ledger-migrate <本文件> <输出> 改写为 v5（CLI 读入时也会在内存里迁移）；旧二进制在标签 {V2_ARCHIVE_TAG}"
            ));
        }
        // 依据：V5 预注册 §五（更新的二进制写的账本报清楚的错，不跳过）。在严格解析头行之前看版本：
        // 新版本的头可能多出本二进制不认识的字段，先报「更新」而不是「读不成」。
        if let Ok(j) = serde_json::from_str::<Json>(first)
            && let Some(v) = j.get("version").and_then(Json::as_u64)
            && v > LEDGER_VERSION as u64
        {
            return Err(format!(
                "E-ledger-newer: 账本版本 {v}，比本二进制（v{LEDGER_VERSION}）新，是更新的二进制写的。修法：用写它的二进制读"
            ));
        }
        // 依据：20 §九 账本行「未知字段拒绝」「链哈希断裂 → decode 拒绝」；12 §2.11 无声吞字段通则
        let head: HeadLine = serde_json::from_str(first).map_err(|e| {
            if serde_json::from_str::<Json>(first).map(|j| j.get("version").and_then(Json::as_u64) == Some(1)).unwrap_or(false) {
                format!("E-ledger-archived: v1 账本，已归档不迁移。修法：用标签 {V1_ARCHIVE_TAG} 处的二进制重放它")
            } else {
                format!("E-ledger-corrupt: 第 1 行（头行）读不成：{e}")
            }
        })?;
        if head.version != LEDGER_VERSION && !LEDGER_READS_AS_IS.contains(&head.version) {
            // 依据：21 E5（每次格式变更在上一提交打归档标签，旧格式只由对应二进制重放）；C-1（v3 照读）
            return Err(format!(
                "E-ledger-archived: 账本版本 {}，本二进制只读 v{LEDGER_VERSION}（v3、v4 照读）。修法：v1 用标签 {V1_ARCHIVE_TAG} 处的二进制重放，v2 用 jpp ledger-migrate 迁移",
                head.version
            ));
        }
        let mut l = Ledger {
            header: head.header,
            ..Ledger::default()
        };
        let mut prev = line_hash(first);
        let mut truncated = None;
        let n = lines.len();
        for (i, line) in lines.iter().enumerate().skip(1) {
            let last = i + 1 == n;
            let parsed: Result<EntryLine, _> = serde_json::from_str(line);
            let el = match parsed {
                Ok(el) => el,
                Err(_) if last && !ends_clean => {
                    truncated = Some(Truncated {
                        kept: l.entries.len(),
                        dropped_bytes: line.len(),
                    });
                    break;
                }
                // 依据：20 §九 账本行（完整行读不成即拒绝，只有末行半写截断）；V5：不认识的条目种类加一句提示
                Err(e) => {
                    let hint = if e.to_string().contains("unknown variant") {
                        "（不认识的条目种类：可能是更新的二进制写的，用写它的二进制读）"
                    } else {
                        ""
                    };
                    return Err(format!(
                        "E-ledger-corrupt: 第 {} 行读不成：{e}{hint}",
                        i + 1
                    ));
                }
            };
            if el.prev != prev || el.seq != l.entries.len() as u64 + 1 {
                return Err(format!(
                    "E-ledger-corrupt: 第 {} 行链断（seq 或 prev 不接上一行）",
                    i + 1
                ));
            }
            prev = line_hash(line);
            l.traces.resize(l.entries.len(), None);
            l.entries.push(el.entry);
            l.traces.push(el.trace);
        }
        l.rebuild_index();
        Ok((l, truncated))
    }
}

/// 执行轨迹：每个效应一行，可打印。
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TraceEvent {
    pub kind: String,
    pub key: String,
    pub replayed: bool,
    pub cost: f64,
    pub site: Span,
    pub note: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Trace {
    pub events: Vec<TraceEvent>,
    pub warnings: Vec<String>,
}

impl Trace {
    pub fn push(
        &mut self,
        kind: &str,
        key: &str,
        replayed: bool,
        cost: f64,
        site: Span,
        note: String,
    ) {
        self.events.push(TraceEvent {
            kind: kind.into(),
            key: key.into(),
            replayed,
            cost,
            site,
            note,
        });
    }
    pub fn warn(&mut self, w: String) {
        self.warnings.push(w);
    }
    pub fn count(&self, kind: &str, replayed: bool) -> usize {
        self.events
            .iter()
            .filter(|e| e.kind == kind && e.replayed == replayed)
            .count()
    }
    pub fn render(&self) -> String {
        let mut s = String::new();
        for (i, e) in self.events.iter().enumerate() {
            s.push_str(&format!(
                "{:>3} {:<9} {} {:<8} ${:.6} @{}..{} {}\n",
                i,
                e.kind,
                e.key.chars().take(12).collect::<String>(),
                if e.replayed { "replay" } else { "live" },
                e.cost,
                e.site.start,
                e.site.end,
                e.note
            ));
        }
        for w in &self.warnings {
            s.push_str(&format!("    ! {w}\n"));
        }
        s
    }
}

#[cfg(test)]
mod b77_tests {
    //! B77（步 7b）：按场合比对；B124（步 18a）：命中记录改由 `CalibUsed` 条目逐键比。
    use super::*;

    fn 头(calib: &str) -> Header {
        Header::new(1, 1.0, "m", "r1", "h").with_calib_hash(Some(calib.into()))
    }

    #[test]
    fn 重放跳过装载库哈希_续接比装载库哈希() {
        let a = 头("库甲").compared;
        let 换库 = 头("库乙").compared;
        assert!(a.diff_in(&换库, HeaderCompare::Replay).is_empty());
        assert_eq!(a.diff_in(&换库, HeaderCompare::Resume)[0].0, "calib_hash");
        assert_eq!(a.diff(&换库), a.diff_in(&换库, HeaderCompare::Resume));
    }

    #[test]
    fn 命中记录逐键比_两种场合都比() {
        let r1 = serde_json::json!({"hi": 0.5});
        let r2 = serde_json::json!({"hi": 0.6});
        let h1 = hash_of(&[&r1.to_string()]);
        for mode in [HeaderCompare::Replay, HeaderCompare::Resume] {
            let mut l = Ledger::new();
            l.set_header_checked(头("库甲"), mode, &|_| None);
            l.note_calib_used("k", &h1, r1.clone());
            // 视图同记录：不报
            let 同 = r1.clone();
            l.set_header_checked(头("库甲"), mode, &|_| Some(同.clone()));
            assert!(l.header_warning.take().is_none());
            // 视图换了记录：报，列出键
            let 换 = r2.clone();
            l.set_header_checked(头("库甲"), mode, &|_| Some(换.clone()));
            let w = l.header_warning.take().unwrap();
            assert!(
                w.contains("calib_used_hash 旧") && w.contains("变化的键：k（共 1 条）"),
                "{w}"
            );
        }
    }

    #[test]
    fn 命中哈希按键排序_空集合也有哈希() {
        let r1 = serde_json::json!({"hi": 0.5});
        let r2 = serde_json::json!({"hi": 0.6});
        let 正 = calib_used_hash([("a", &r1), ("b", &r2)]);
        let 反 = calib_used_hash([("b", &r2), ("a", &r1)]);
        assert_eq!(正, 反);
        assert_ne!(正, calib_used_hash([("a", &r2), ("b", &r1)]));
        assert_eq!(Ledger::new().calib_used_hash(), calib_used_hash([]));
    }

    #[test]
    fn 同记录不重复追加_换记录追加一条_派生视图取最后一条() {
        let r1 = serde_json::json!({"hi": 0.5});
        let r2 = serde_json::json!({"hi": 0.6});
        let mut l = Ledger::new();
        l.note_calib_used("k", "h1", r1.clone());
        l.note_calib_used("k", "h1", r1.clone());
        assert_eq!(l.entries.len(), 1);
        l.note_calib_used("k", "h2", r2.clone());
        assert_eq!(l.entries.len(), 2);
        assert_eq!(l.calib_used["k"]["record"], r2);
        let (back, _) = Ledger::decode(&l.encode()).unwrap();
        assert_eq!(back.calib_used, l.calib_used);
        assert_eq!(back.encode(), l.encode());
    }
}

#[cfg(test)]
mod b55_tests {
    //! 步 18b（B55）：逐行编码与整份编码是同一条路径；内存账本作为端口。
    use super::*;

    fn 样本() -> Ledger {
        let mut l = Ledger::new();
        l.set_header(Header::new(1, 1.0, "m", "r1", "h"));
        l.put(Entry::judge("k1", Answer::Noul(0.9), 0, 0.0, "m", 1));
        l.put(Entry::Intent {
            key: "intent:e1".into(),
            at: 1,
            attempt: None,
        });
        l.put(Entry::effect_keyed("e1".into(), "do", Json::from(1), 0.0));
        l
    }

    #[test]
    fn 分两段逐行编码与整份编码逐字节相同() {
        let l = 样本();
        let head = encode_head(&l.header);
        let (全部, 末) = l.encode_from(0, &line_hash(&head));
        // 先写前两条，再从第 3 条续写：续写接上的链与一次写完相同
        let (后段, 末2) = l.encode_from(2, &line_hash(&全部[1]));
        assert_eq!(后段, 全部[2..].to_vec());
        assert_eq!(末2, 末);
        let mut s = format!("{head}\n");
        for x in &全部 {
            s.push_str(x);
            s.push('\n');
        }
        assert_eq!(s, l.encode());
        let (d, t) = Ledger::decode(&s).unwrap();
        assert!(t.is_none());
        assert_eq!(d.len(), 3);
    }

    #[test]
    fn 内存账本作端口_答案另起一条_命中记录同哈希不追加() {
        let mut l = Ledger::new();
        let p: &mut dyn LedgerPort = &mut l;
        p.append(
            Entry::Ask {
                key: "a".into(),
                ekey: None,
                answer: None,
            },
            Durability::Layer,
        )
        .unwrap();
        p.append(
            Entry::Ask {
                key: "a".into(),
                ekey: None,
                answer: Some(Answer::Noul(1.0)),
            },
            Durability::Now,
        )
        .unwrap();
        p.append_calib_used("k", "h1", Json::Null).unwrap();
        p.append_calib_used("k", "h1", Json::Null).unwrap();
        p.append_calib_used("k", "h2", Json::Null).unwrap();
        assert!(p.end_layer().is_ok());
        assert_eq!(l.len(), 4, "未答、已答、两条命中记录");
    }

    /// G2 附录三 W-5：被扣下的意向下一趟再写一条同键 `Intent`（索引指向新条目）；没被扣下的同键 `Intent` 仍不追加；
    /// 编码—解码往返逐字节相同，重载后索引仍指向最后一条
    #[test]
    fn 扣下之后同键意向另起一条_往返索引指向最后一条() {
        let at = |n| AttemptRef {
            program: "P".into(),
            n,
        };
        let 意向 = |n| Entry::Intent {
            key: "intent:e".into(),
            at: 0,
            attempt: Some(at(n)),
        };
        let mut l = Ledger::new();
        l.set_header(Header::new(1, 1.0, "m", "r1", "h"));
        let p: &mut dyn LedgerPort = &mut l;
        p.append(意向(1), Durability::Now).unwrap();
        // 没被扣下：同键第二条不追加（有意向无结果 = 结果未知，不能被新意向盖掉）
        p.append(意向(9), Durability::Now).unwrap();
        assert_eq!(l.len(), 1);
        assert!(!l.intent_withheld("intent:e"));
        l.put_answer(Entry::Withheld {
            attempt: at(1),
            key: "intent:e".into(),
            cause: WithheldCause::Suspended,
        });
        assert!(l.intent_withheld("intent:e"));
        l.put_answer(意向(2));
        assert_eq!(l.len(), 3);
        assert_eq!(l.get("intent:e"), Some(&意向(2)));
        assert!(
            !l.intent_withheld("intent:e"),
            "新意向之后没有 Withheld：这一趟的结果未知"
        );
        let s = l.encode();
        let (d, t) = Ledger::decode(&s).unwrap();
        assert!(t.is_none());
        assert_eq!(d.encode(), s);
        assert_eq!(d.get("intent:e"), Some(&意向(2)));
        assert!(!d.intent_withheld("intent:e"));
    }
}
