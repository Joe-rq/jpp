//! 计划的数据类型与运行期钩子（步 13a，`20` §2.3「L0 · `jpp-ir`」`plan` 子模块、「L3 · `jpp-plan`」）。
//!
//! 规划（`jpp-plan`）写 [`Plan`]，运行时只读它；运行期才算得出的那一半经 [`PlanHooks`] 注入，
//! trait 在这里定义、实现在 `jpp-plan`（`20` §2.2 第 3 条、T3）。运行时因此不自己做推测与向量化的分析：
//! 它拿到的是「哪些站点可以提前登记」，自己只求值与登记。
//!
//! 与 `20` §2.3 字面的差异（步 13a 过程记录与 `过程记录/总账待补条目.md` 各记一行，主会话 2026-09-24 认可）：
//! - **触发点以节点号为键**（`BTreeMap<NodeId, _>`），不以 `SiteId`：块内位置的触发点是 `let` 语句，
//!   `let` 没有站点；取被绑定的值表达式的节点号。
//! - **`instantiate` 带环境视图**（[`EnvView`]）而不是只带实参摘要：HEAD 判断「会不会产生效应」时按
//!   运行期环境解析名字（名字绑定的是哪个方法值、方法体里又调了谁），换成纯静态摘要会让现有测试的结果变，
//!   步 13a 就不再是 R。形参已由运行时绑进环境，实参摘要经视图查得。
//! - 高阶触发点（`map`/`filter`）在 HEAD 按内置身份触发（任何一次 `map`/`filter` 调用），不按站点；
//!   静态一半是每个函数体的候选目标（[`Plan::bodies`]），动态一半是 `instantiate`。
//! - `SitePlan`（`phys`、`fission`、`sched_class`、`concurrency`）等有消费者的 pass 落地时再加（步 23；
//!   阶段评估① §五·9「有消费者才建」）。步 22 加估计、`per_site`、告警与拒绝；`select_within` 随 B0487。

use crate::ir::{Expr, Function, IrDiag};
use crate::key::{NodeId, SiteId};
use std::collections::{BTreeMap, BTreeSet};
use std::rc::Rc;

/// 函数的身份：函数节点的节点号（`Function.id`）。
pub type FnId = NodeId;

/// 估计值：未测即未知，不写 0（`20` §2.3 `jpp-plan`，`14` 附录）。
///
/// `AtLeast` 是步 22 加的第三个变体（主控 2026-09-29 裁定，`过程记录/工程-步22.md` §二 (B)）：下界已知、
/// 上界未知是最常见的情形（费用没有 token 上界；循环 bound 非字面；有形参传进来的方法值）。
#[derive(Clone, Debug, PartialEq)]
pub enum Estimate<T> {
    Known { lo: T, hi: T },
    AtLeast { lo: T },
    Unknown(UnknownReason),
}

impl<T: Copy> Estimate<T> {
    /// 下界（未知时 `None`）
    pub fn lo(&self) -> Option<T> {
        match self {
            Estimate::Known { lo, .. } | Estimate::AtLeast { lo } => Some(*lo),
            Estimate::Unknown(_) => None,
        }
    }
    /// 上界（只有 `Known` 有）
    pub fn hi(&self) -> Option<T> {
        match self {
            Estimate::Known { hi, .. } => Some(*hi),
            _ => None,
        }
    }
}

/// 一次判断调用的费用模型（步 30 / B0488）：`单价 × (截距 + 状态系数 × 状态字符 + 题系数 × 题字符)`。
/// 系数与单价全从画像读（`cost.regression`、`cost.price_usd_per_input_token`），不写死。
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CostModel {
    /// 美元每 input token
    pub price: f64,
    pub intercept_tokens: f64,
    pub state_char_coef: f64,
    pub question_char_coef: f64,
}

impl CostModel {
    /// 一次调用的估计费用（美元）
    pub fn est_usd(&self, state_chars: u64, question_chars: u64) -> f64 {
        self.price
            * (self.intercept_tokens
                + self.state_char_coef * state_chars as f64
                + self.question_char_coef * question_chars as f64)
    }
}

/// 估计为什么未知。
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum UnknownReason {
    /// `plan` pass 关着（消融臂），没有算
    NotComputed,
    /// 画像字段未测（`20` §3.9）：值为字段名
    Untested(String),
    /// 宿主没有给判断器的实际单价（库用法缺省；CLI 总会按后端给）
    PriceNotGiven,
    /// 程序太大，分析在访问上限处放弃
    TooComplex,
}

/// 判断器每个 input token 的实际单价，由宿主按实际后端给（固定观察为 0；真机为画像价格）。
/// 不直接读画像：固定观察运行也可以装一份带价格的画像（`过程记录/工程-步22.md` §二 2.1 第 2 条）。
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum JudgePrice {
    /// 已知单价（美元每 input token）
    Known(f64),
    /// 画像没有价格（`W-cost-unknown`）
    Untested,
    /// 宿主没说
    #[default]
    NotGiven,
}

/// 规划的运行语境（`20` §2.3 `PlanCtx`、B42）：只有宿主知道的三件事。
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PlanCtx {
    /// 账本没有任何条目（首跑）；续接与审计重放为假
    pub ledger_empty: bool,
    /// 没开跨运行缓存
    pub cache_off: bool,
    pub judge_price: JudgePrice,
}

impl PlanCtx {
    /// 什么都不知道：不算可靠下界、不拒（旧入口 `jpp_plan::plan` 用它）
    pub fn unknown() -> PlanCtx {
        PlanCtx {
            ledger_empty: false,
            cache_off: false,
            judge_price: JudgePrice::NotGiven,
        }
    }
}

/// 运行期没声明 `budget.absent` 时网络类错误的隐含重试次数（现场稳定性三修 (1)）。运行时（`budget.rs`）
/// 与规划（调用数上界：每次发出都计入 `cost.calls`）共用这一处。
pub const IMPLICIT_RETRY: u32 = 2;

/// 每个效应站点的计划（`20` §2.3 `SitePlan` 的步 22 部分；`phys`、`fission` 等随步 23b）。
/// B0487 的 `select_within` 按 `order` 与估计挑本层子集，B0488 往这里加价值密度。
#[derive(Clone, Debug, PartialEq)]
pub struct SitePlan {
    /// 登记顺序（规划遍历到的先后，步 22 价值密度的占位即按它）
    pub order: u32,
    /// 每条路径都会执行（可靠下界只数这些）
    pub must: bool,
    /// 这个站点的调用在第几层之后（必经站点的层；非必经为 `None`）
    pub layer_lo: Option<u32>,
    /// 这个站点最多执行几次（未知为 `None`）
    pub execs_hi: Option<u64>,
    /// 下游层数（步 30 / B0488，K-135「层数」目标）：从这个读数站点的输出出发，沿数据依赖与控制依赖往后最多还有
    /// 几层判断。静态下界（循环只展开到第二轮），只作层内挑选的排序提示，不进任何语义
    pub downstream: u32,
}

/// 伴随题的发法（B0492 S5，J-05 草案第三稿 (0a)，主会话裁定四十一及补充 B0639）：没有可信记录的题带五道伴随题，
/// 与原题同一状态、同一刷新时刻发。`Same` 并进原题那一次请求；`Parallel` 另发一次并行请求（批数 +1、时延不变、
/// 同请求串扰归零）；`Off` 不带。`Passes` 的默认发法是 `Same`（H2 串扰实验定，裁定四十一）；`Plan::empty()` 取 `Off`。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum CompanionMode {
    #[default]
    Off,
    Same,
    Parallel,
}

/// 一次运行的计划：规划写、运行时读。
#[derive(Clone, Debug, PartialEq)]
pub struct Plan {
    /// 同状态、同层的题合成一次调用（`fuse`）
    pub fuse: bool,
    /// 高阶调用（`map`/`filter`）后续各轮的目标站点提前登记（`vectorize`）
    pub vectorize: bool,
    /// 惰性过桥（B94，步 23c，`lazy_cut`）：`cut` 返回未解析出口，第一次被检视时才刷新、解析
    pub lazy_cut: bool,
    /// 层内挑选（B43、B51-C1，步 22 / B0487）：本层超出剩余预算时经 [`PlanHooks::select_within`] 按类别与
    /// 价值密度排发出顺序。关掉即消融矩阵第 ⑧ 行的对照臂「降为登记顺序」（`21` §六·5）
    pub select_within: bool,
    /// 超窗裂变（步 23b，`fission`，`12` §4 pass 3）：题声明 `fission: "approx"`、画像测过窗口且材料超窗时，运行时按窗切块、
    /// `cut` 处按题的操作合回。全程序一位；逐站点的 `SitePlan.fission` 未接（过程记录 工程-步23b.md 已知差距）
    pub fission: bool,
    /// 层内挑选按关键路径排（步 30 / B0488，K-135 层数目标）：真站点组里下游层数大的先发。关掉即步 22 的登记顺序；
    /// 价值密度另有开关（B 段），两者分开，消融三臂分得清变量（复核 B-1，主控 2026-09-29）
    pub critical_path: bool,
    /// 层内挑选按价值密度排（步 30 B 段 / B0488，B7、B43；裁定三十八、四十三的 `planner_value`）：同下游层数（或关键路径
    /// 关时同类别）内价值大的组先发。关掉即不看价值；与 `critical_path` 分开，消融四臂分得清变量
    pub value_density: bool,
    /// 逐组费用估计的模型（步 30，给费用约束下的挑选用）：画像单价与 token 回归系数，任一未测即 `None`
    pub cost_model: Option<CostModel>,
    /// 伴随题的发法（B0492 S5）
    pub companions: CompanionMode,
    /// 推测：`let` 值表达式的节点号 → 从这条语句起、`if` 两侧分支体里的候选站点（`speculate`）
    pub triggers: BTreeMap<NodeId, TriggerPlan>,
    /// 提升：`let` 值表达式（本身是一次 `judge`）的节点号 → 后续同状态语句的提升步（`lift`）
    pub lifts: BTreeMap<NodeId, LiftPlan>,
    /// 每个函数体的候选目标站点（静态一半；向量化时经 [`PlanHooks::instantiate`] 按运行期环境筛）
    pub bodies: BTreeMap<FnId, Vec<TargetSite>>,
    /// 直线段提升穿过函数调用（B94，步 23c，随 `lift`）：`let` 值表达式的节点号 → 从这条语句起的
    /// 直线段里的候选站点与调用（经 [`PlanHooks::segment`] 按运行期环境筛）
    pub segments: BTreeMap<NodeId, Vec<TargetSite>>,
    /// 调用数估计（`budget.calls` 口径：`judge`、`gen`、`do`、`ask` 的每次发出，含重试；步 22 `plan` pass）
    pub calls_est: Estimate<u64>,
    /// 判断层数估计
    pub layers_est: Estimate<u64>,
    /// 费用估计（美元；下界按每次判断调用 1 个 token）
    pub cost_est: Estimate<f64>,
    /// 时延估计（秒；判断层数 × 画像 p95）
    pub latency_est: Estimate<f64>,
    /// 各效应站点的计划（`plan` pass 写）
    pub per_site: BTreeMap<SiteId, SitePlan>,
    /// 计划期告警（`W-cost`、`W-cost-unknown`）
    pub warnings: Vec<IrDiag>,
    /// 计划期拒绝（B42：只在可靠下界超预算或时延下界超时延预算，`E-budget-plan`）
    pub rejected: Option<IrDiag>,
}

impl Plan {
    /// 什么都不提前的计划（全部 pass 关）
    pub fn empty() -> Plan {
        Plan {
            fuse: false,
            vectorize: false,
            lazy_cut: false,
            select_within: false,
            fission: false,
            critical_path: false,
            value_density: false,
            cost_model: None,
            companions: CompanionMode::Off,
            triggers: BTreeMap::new(),
            lifts: BTreeMap::new(),
            bodies: BTreeMap::new(),
            segments: BTreeMap::new(),
            calls_est: Estimate::Unknown(UnknownReason::NotComputed),
            layers_est: Estimate::Unknown(UnknownReason::NotComputed),
            cost_est: Estimate::Unknown(UnknownReason::NotComputed),
            latency_est: Estimate::Unknown(UnknownReason::NotComputed),
            per_site: BTreeMap::new(),
            warnings: vec![],
            rejected: None,
        }
    }
}

/// 触发点的种类。
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TriggerKind {
    /// 块内第 `n` 条语句求值之前（推测 `if` 两侧）
    BlockFrom(usize),
}

/// 一个触发点：从这里出发可提前登记的目标站点，按遍历顺序（登记顺序即层内题序，决定账本字节）。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TriggerPlan {
    pub kind: TriggerKind,
    pub targets: Vec<TargetSite>,
}

/// 目标站点：一个 `judge` 节点。能否提前登记还要看运行期环境（钩子按 [`EnvView`] 核）。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TargetSite {
    /// `judge` 节点的节点号
    pub node: NodeId,
    /// 状态与题两段表达式里用到的名字（运行时按它们求值）
    pub needs_bound: BTreeSet<String>,
    /// 步 13b：这是一次对用户函数的调用（向量化时穿进被调函数体），不是 `judge` 节点
    pub call: bool,
}

/// `instantiate` 的结果（步 13b）：可提前登记的站点，或穿进一次用户函数调用后的站点。
/// 运行时按顺序执行：`Site` 在当前环境里求状态与题并登记；`Enter` 在当前环境里求被调者与实参
/// （实参只有名字或字面量），在被调函数的捕获环境上绑好形参，再对 `inner` 同样执行。
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Target {
    Site(NodeId),
    Enter { call: NodeId, inner: Vec<Target> },
}

/// 提升计划：头一条 `let` 是一次 `judge`，其后各条语句按顺序的处置。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LiftPlan {
    /// 头之后逐条语句（静态上遇到必停处即截断）
    pub steps: Vec<LiftStep>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LiftStep {
    /// 该语句在块内的下标
    pub index: usize,
    /// 该语句的值表达式的节点号
    pub node: NodeId,
    /// 绑定的名字
    pub name: String,
    /// 同状态的 `judge`：提前求值；否则只是越过它
    pub lift: bool,
    /// 这一句重新绑定了头状态里用到的名字：处理完即停
    pub stop_after: bool,
}

/// 「会不会产生效应」问的是哪一种：严格（任何不在可提前求值表上的调用）或只问触世界。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Reach {
    Strict,
    World,
}

/// 运行期环境的只读视图：规划的钩子经它按名字查值的摘要，运行时实现。
pub trait EnvView {
    fn lookup(&self, name: &str) -> Option<ValueSummary>;
}

/// 方法值的视图：函数体、身份（结构哈希）与捕获环境。
pub trait FnView {
    fn function(&self) -> &Function;
    fn identity(&self) -> &str;
    fn env(&self) -> Rc<dyn EnvView>;
    /// 这个方法值实例的身份：同一实例即同一份捕获（C2c 复核 K2：纯性判定的递归护栏与缓存按它，不按结构哈希）。
    /// 缺省取结构哈希；运行时的实现加上闭包实例地址。
    fn instance(&self) -> String {
        self.identity().to_string()
    }
}

/// 值的摘要：钩子判断效应只需要这些。
#[derive(Clone)]
pub enum ValueSummary {
    /// 不含方法的数据
    Data,
    /// 内置（按名字）
    Builtin(String),
    /// 方法值
    Fn(Rc<dyn FnView>),
    /// 列表或记录：元素的摘要，按需展开
    Container(Rc<dyn Fn() -> Vec<ValueSummary>>),
}

/// 运行期钩子：规划里只有运行期才算得出的那一半（`20` §2.3 `jpp-plan`、§4.5 第 2 条）。
///
/// 返回的目标站点都是 `judge` 节点，按遍历顺序；每个都已核过「此刻能提前求值它的状态与题」
/// （两段表达式按 `env` 解析都不会产生效应）。运行时对它们只做求值与登记。
pub trait PlanHooks {
    /// 方法值 `body` 的一轮（形参已绑进 `env`）里可提前登记的目标（向量化）；节点号在 `body`
    /// 或被穿进的函数体里。步 13b 起穿过对用户函数的调用（多层包装）。
    fn instantiate(&self, plan: &Plan, body: &Function, env: &dyn EnvView) -> Vec<Target>;
    /// 触发点 `at`（`let` 值表达式的节点号，所在块 `block`）处可推测登记的目标站点。
    fn speculate<'b>(
        &self,
        plan: &Plan,
        at: NodeId,
        block: &'b crate::ir::Block,
        env: &dyn EnvView,
    ) -> Vec<&'b Expr>;
    /// 触发点 `at`（`let` 值表达式的节点号，所在块 `block`）起的直线段里可提前登记的目标（B94 下半，
    /// 步 23c）：`Site` 的节点号在 `block` 里，`Enter` 的调用节点在 `block` 里、内层在被调函数体里。
    fn segment(
        &self,
        plan: &Plan,
        at: NodeId,
        block: &crate::ir::Block,
        env: &dyn EnvView,
    ) -> Vec<Target>;
    /// 表达式求值时会不会产生效应（`Reach::Strict`）或触世界（`Reach::World`）。提升逐句问它。
    fn may_effect(&self, e: &Expr, env: &dyn EnvView, reach: Reach) -> bool;
    /// 层内挑选（B43 的运行期落点，步 22 / B0487）：本层要发的组超出剩余预算时，给出发出顺序并按剩余调用数切成
    /// 「发出」与「推迟」。运行时按 `send` 再 `defer` 的顺序逐组核预算，停发点以逐组核为准（主控 Z0209 Q2 (b)）：
    /// 推迟的真站点记 `Unsure(budget)` 进责任表、续跑重发，推迟的推测本趟不发。
    ///
    /// 缺省实现是纯登记顺序、从末尾切，即消融矩阵第 ⑧ 行的对照臂；`jpp-plan` 按丢弃顺序实现。
    fn select_within(&self, _plan: &Plan, layer: &[PendingSite], left: BudgetLeft) -> Selection {
        Selection::cut((0..layer.len()).collect(), layer, left)
    }
    /// 闸门的信息值（步 28 起 derive 验题闸门第④段；主会话裁定四十三的 `gate_info`，四十六的第二版）：有记录的候选取记录混淆
    /// 矩阵上的互信息，无记录的取期望熵降 × 闸门折扣。每项是一道候选（[`GateItem`]）；`certified` 是校准库里已认证记录的（题类、
    /// 混淆矩阵），无记录的候选取同题类最低折扣、没有同题类取全库最低、
    /// 全库为空只按熵。实现在 `jpp-plan::value`，运行时的内置 `gate_info` 经这里调（`20` §2.2 第 3 条：运行时不依赖
    /// `jpp-plan`）。返回每项的（信息值, 取了哪条已认证记录的最低折扣——`certified` 的下标，自己有记录或只按熵时 `None`），
    /// 运行时把用到的记录记进账本。缺省 `None` = 没接规划器。
    fn gate_info(
        &self,
        items: &[GateItem],
        certified: &[(Option<crate::question_kind::QuestionKind>, Channel)],
    ) -> Option<Vec<(f64, Option<usize>)>> {
        let _ = (items, certified);
        None
    }
    /// 切分点与信道容量（步 30 B 段，B7 后半、宪法登记表「噪声二十问」行）：`n` 是是非记录的原始混淆计数（真是 / 真否 ×
    /// act、ignore、unsure），平滑与价值同口径（Jeffreys，复核 B0488-B 缺口 5），返回 (u*, 容量)。实现在
    /// `jpp-plan::value::split_point`；缺省 `None` = 没接规划器。
    fn split_point(&self, n: [[f64; 3]; 2]) -> Option<(f64, f64)> {
        let _ = n;
        None
    }
}

/// 闸门的一道候选（裁定四十三）：请求、选择规模、题类、自己的记录的混淆矩阵（题键 → 题式键 → 类键取第一条）。
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GateItem {
    pub request: crate::question_kind::Request,
    pub k: usize,
    pub kind: crate::question_kind::QuestionKind,
    pub own: Option<Channel>,
}

/// 一条校准记录的混淆矩阵（原始计数，未平滑；裁定四十六的第二版价值由它算，平滑在 `jpp-plan::value`）。样本按记录自己的线与
/// δ（`stat::decided_up/decided_down`，与 `cut` 同一判据）落到出口。
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Channel {
    /// 是非题的记录（样本是 (p, 真值)）：真是 / 真否 × act、ignore、unsure
    Binary { n: [[f64; 3]; 2] },
    /// K 选一与打分的记录（样本只有 (p_max, 对错)）：已决且对、已决且错、未决（对称错误近似）
    Symmetric { c: f64, w: f64, u: f64 },
}

/// 层内挑选里一组待发题的类别（B51-C1）。「同状态推测」不单成组：它与真站点进同一次融合调用，
/// 算在真站点组里（[`PendingSite::same_state_spec`]）。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SiteClass {
    /// 组里有程序此刻真走到的站点
    Real,
    /// 只含提前登记（分支推测、向量化提前、直线段提升）的组：它自己要付一次调用
    CrossStateSpec,
}

/// 刷新时本层的一组待发题（一组 = 一次调用；融合开时按材料分组，关时逐题）。
#[derive(Clone, Debug, PartialEq)]
pub struct PendingSite {
    /// 本层登记位置（组的发出先后）。价值密度的占位（步 22）：越靠前越先发；B0488 换成价值函数时再带 `SiteId`
    pub pos: usize,
    pub class: SiteClass,
    /// 这组要付的调用数（今天恒为 1；重试另核）
    pub calls: u64,
    /// 真站点组里同一次调用捎带的推测题数（同状态推测）。只有费用是约束且有逐题单价估计时才有可省的；
    /// 发出前没有 usd 估计，今天不剥（主控 Z0209 Q3，7c）
    pub same_state_spec: u32,
    /// 组内第一条成员的站点（步 30：运行时按 `span.start → SiteId` 表查；查不到为 `None`）
    pub site: Option<SiteId>,
    /// 下游层数（`SitePlan::downstream`）：真站点组取真站点成员的最大值，只含推测的组取全部成员的最大值；查不到为 0
    pub downstream: u32,
    /// 组内真站点题数（只含推测的组为组内题数）
    pub questions: u32,
    /// 状态字符数（`on`、`ctx`、`ref`、`over` 各材料文本的字符数之和，费用估计用）
    pub state_chars: u64,
    /// 组内各题题面字符数之和（费用估计用）
    pub question_chars: u64,
    /// 组内真站点题的校准键与本趟计数（按键去重，按首次出现排）
    pub keys: Vec<KeyCount>,
    /// 组内真站点各题（按账本键去重）的价值输入（步 30 B 段）：请求、选择规模、`keys` 的下标
    pub values: Vec<ValueInput>,
}

/// 一道待发题的价值输入（步 30 B 段）：它的记录与本趟计数在 `PendingSite.keys[key]`
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ValueInput {
    pub request: crate::question_kind::Request,
    pub k: usize,
    pub key: usize,
}

impl PendingSite {
    /// 只带步 22 的四个量、其余取空（测试与缺省实现用）
    pub fn basic(pos: usize, class: SiteClass, calls: u64, same_state_spec: u32) -> PendingSite {
        PendingSite {
            pos,
            class,
            calls,
            same_state_spec,
            site: None,
            downstream: 0,
            questions: 0,
            state_chars: 0,
            question_chars: 0,
            keys: vec![],
            values: vec![],
        }
    }
}

/// 一个校准键在本趟的计数与它的记录（步 30：读数进「接下来先问什么」，B22 第 5 类消费者）。
#[derive(Clone, Debug, PartialEq)]
pub struct KeyCount {
    pub key: String,
    /// 本趟以这个键过桥、有答案的读数条数（同一读数切两次计一次；缺席与预算推迟的不计）。键是 `cut` 实际用的键
    pub seen: u64,
    /// 其中出口为 `Unsure` 的条数
    pub unsure: u64,
    /// 记录的混淆矩阵（裁定四十六的第二版）：按 `cut` 的查找链（题键 → 题式键 → 类键）取第一条上岗、有选中证书、带标注样本
    /// 非空、`line_delta` 可得的记录；无记录为 `None`（B 段）
    pub record: Option<Channel>,
}

/// 层边界的剩余预算（与运行时 `charge` 同口径）。
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BudgetLeft {
    pub calls: u64,
    pub usd: f64,
}

/// 层内挑选的结果：`send` 与 `defer` 都是 `layer` 的下标，合起来恰好是全部下标各一次。
#[derive(Clone, Debug, PartialEq)]
pub struct Selection {
    pub send: Vec<usize>,
    pub defer: Vec<usize>,
    /// 各组的价值（`layer` 下标对齐；价值密度开关关时为空）。运行时只把它写进报告 `selections`（步 30 B 段）
    pub value: Vec<f64>,
    /// 各组 `keys` 的（平滑后的未决比例 u₀、平滑后总数 n、本趟更新后的 u_run）（`layer` 下标对齐，内层与 `PendingSite.keys`
    /// 对齐；无记录为 `None`；开关关时为空）。报告用
    pub key_stats: Vec<Vec<Option<(f64, f64, f64)>>>,
}

impl Selection {
    /// 按给定的发出顺序，从前往后累计调用数，装得进剩余调用数的归 `send`，其余归 `defer`（保持顺序）。
    pub fn cut(order: Vec<usize>, layer: &[PendingSite], left: BudgetLeft) -> Selection {
        let mut used = 0u64;
        let mut send = vec![];
        let mut defer = vec![];
        for i in order {
            let c = layer[i].calls;
            if defer.is_empty() && used + c <= left.calls {
                used += c;
                send.push(i);
            } else {
                defer.push(i);
            }
        }
        Selection {
            send,
            defer,
            value: vec![],
            key_stats: vec![],
        }
    }

    /// 发出顺序：先 `send` 再 `defer`
    pub fn order(&self) -> impl Iterator<Item = usize> + '_ {
        self.send.iter().chain(&self.defer).copied()
    }
}
