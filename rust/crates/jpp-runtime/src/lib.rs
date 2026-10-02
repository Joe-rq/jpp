//! 解释器：逐语句即时执行；效应即时发出（惰性融合是后续优化，未迁移）。
//! 纪律由这里与检查器把关：J-01 读数不进槽、J-02 禁自指、J-03 线来自校准记录、J-05 unsure 必消费、
//! J-06 有界循环 + 键重复即停、J-07 预算超即停（Pending）、J-12 Fail 是值、J-13 序号、J-18 账本头。

use std::collections::{BTreeSet, HashMap, HashSet};
use std::rc::Rc;

use serde_json::{Value as Json, json};

// 运行时读 IR（步 12c；步 12d 删除核心语法树后只剩 IR）
use jpp_effects::port::{CallInput, EffectError, EffectOut, Ports};
use jpp_effects::view::{self, Callee, K, kind};
use jpp_effects::views::{CalibView, Lookup};
use jpp_effects::views::{FitRecord, FitView};
use jpp_ir::ir::{Block, Budget, Expr, Function, Program, Span, Stmt};
use jpp_ledger::{
    AttemptRef, CalibRef, Durability, EffectKey, Entry, Header, HeaderCompare, JudgeKey,
    LedgerError, LedgerPort, MatMeta, RENDER_VERSION, SourceEdge, StopCause, Trace,
};
use jpp_ir::key::SiteRef;
use jpp_value::value::*;

pub const HANDLER_VERSION: &str = "h0.1-rs";
pub const DEFAULT_DEPTH: u32 = 256;

#[derive(Clone, Debug, PartialEq)]
pub struct RtError {
    pub rule: Option<String>,
    pub message: String,
    pub span: Span,
}

impl RtError {
    pub fn new(rule: Option<&str>, msg: impl Into<String>, span: Span) -> RtError {
        RtError {
            rule: rule.map(|s| s.to_string()),
            message: msg.into(),
            span,
        }
    }
    pub fn render(&self) -> String {
        match &self.rule {
            Some(r) => format!(
                "[{r}] {} @{}..{}",
                self.message, self.span.start, self.span.end
            ),
            None => format!("{} @{}..{}", self.message, self.span.start, self.span.end),
        }
    }
}

/// 控制流信号：错误，或程序级挂起（预算、ask 未答、显式 pending）。
#[derive(Debug)]
pub enum Fault {
    Error(RtError),
    Halt(Pending),
}

impl From<RtError> for Fault {
    fn from(e: RtError) -> Fault {
        Fault::Error(e)
    }
}

type R<T> = Result<T, Fault>;

fn err<T>(rule: Option<&str>, msg: impl Into<String>, span: Span) -> R<T> {
    Err(Fault::Error(RtError::new(rule, msg, span)))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TaintOut {
    Trusted,
    Untrusted,
    Inherit,
}

/// 登记的动作（`do` 只能触发登记过的动作；§2.5）。
#[derive(Clone)]
pub struct Action {
    pub name: String,
    pub cost: f64,
    pub reversible: bool,
    pub taint_out: TaintOut,
    pub f: Rc<dyn Fn(&[Value]) -> Result<Value, String>>,
    /// 声明的输出形状（B51-R2 候选字段，步 15d）：`None` = 未声明，运行期不核
    pub mat_shape: Option<jpp_effects::MatShape>,
    /// 如实的可撤回性事实（C-7，Z0173）：三值、理由、成立条件。`None` = 登记方没给（`action_fact` 只能
    /// 从 `reversible` 布尔推出两值、理由写「登记时未附」）。这一份只是事实与记录，J-08、放行门不读它。
    pub undo: Option<ActionUndo>,
}

/// 动作可撤回性的如实标注（C-7）：`reversibility` 取 `"reversible" | "irreversible" | "depends_on_args"`。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ActionUndo {
    pub reversibility: String,
    pub reason: String,
    pub conditions: Vec<String>,
}

#[derive(Default)]
pub struct ActionRegistry {
    pub actions: HashMap<String, Rc<Action>>,
}

impl ActionRegistry {
    pub fn new() -> ActionRegistry {
        ActionRegistry::default()
    }
    pub fn register(
        &mut self,
        name: &str,
        cost: f64,
        reversible: bool,
        taint_out: TaintOut,
        f: impl Fn(&[Value]) -> Result<Value, String> + 'static,
    ) {
        self.actions.insert(
            name.to_string(),
            Rc::new(Action {
                name: name.to_string(),
                cost,
                reversible,
                taint_out,
                f: Rc::new(f),
                mat_shape: None,
                undo: None,
            }),
        );
    }

    /// 给已登记的动作附上可撤回性事实（C-7）：`.jpp` 里 `action_fact(name)` 与报告读它。动作没登记时返回 `false`。
    pub fn describe_undo(&mut self, name: &str, undo: ActionUndo) -> bool {
        match self.actions.get_mut(name) {
            Some(a) => {
                Rc::make_mut(a).undo = Some(undo);
                true
            }
            None => false,
        }
    }

    /// 给已登记的动作声明输出形状（B51-R2，步 15d）：`do` 返回后运行期核基数与单项尺寸，违反出
    /// `Fail(ShapeMismatch)`。动作没登记时返回 `false`。依据：B51-R2（20 附录 A）
    pub fn shape(&mut self, name: &str, shape: jpp_effects::MatShape) -> bool {
        match self.actions.get_mut(name) {
            Some(a) => {
                Rc::make_mut(a).mat_shape = Some(shape);
                true
            }
            None => false,
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct Cost {
    pub calls: u64,
    pub replayed: u64,
    pub tokens: u64,
    pub usd: f64,
    pub asks: u64,
}

/// 审计重放的记账（B35）：账本里记过的调用在重放时计入预算，但不算新调用、不进报告的 `cost`。
#[derive(Clone, Debug, Default)]
struct ReplayAudit {
    on: bool,
    /// 按账本记录折算的调用次数与费用（融合后多道题同属一次调用，按 `call` 去重）
    calls: u64,
    usd: f64,
    seen_calls: HashSet<u64>,
    /// 最近一次从账本取答的站点：费用超预算时停在这里（首跑的调用后核预算停在同一站点）
    last_site: Option<Span>,
}

/// 这条线**凭什么**：`手填` 还是某一张证书。与「哪一层」正交。
fn 凭据(rec: &Lookup) -> String {
    match &rec.selected {
        // **第三轴：这个数是怎么算出来的。**「哪一层」「凭什么」「怎么算的」是三件事——
        // 把代价折进「凭什么」那一格，就是把一小时前刚红过的那次合并再做一遍。
        // **一条由代价矩阵算出来的线，和一条人拍脑袋写的线，不是一回事。**
        Some(c) => match c.cost {
            // B72：试用证书在凭据里说出来（handler 经 `line_source` 看得见）
            Some((fp, fn_)) => format!(
                "{}证书:α={:.2}·代价(fp={fp},fn={fn_})",
                试用前缀(c),
                c.alpha
            ),
            None => format!("{}证书:α={:.2}", 试用前缀(c), c.alpha),
        },
        None => "手填".to_string(),
    }
}

fn 试用前缀(c: &jpp_effects::views::CertView) -> &'static str {
    if c.trial { "试用" } else { "" }
}

fn e_line_empty(s: &str) -> bool {
    s.is_empty()
}

/// 取前 n 个**字符**做诊断摘要。
///
/// **不能按字节切**：`q_hash` 平时是十六进制，但 `fit` 的结果把 `fit:{名}` 放在那一位，
/// 名字是中文时按字节切会切在字符中间——**`&s[..8]` 直接 panic**。
/// 一句告警文字把整个程序打崩，是比它想报的问题严重得多的事。
///
/// **同族全部走这里**，包括那些「现在一定是十六进制所以按字节切也安全」的地方
/// （账本键、`state.hash`）。安全靠的是一条没写下来的不变量，
/// **而下一个人得重新验一遍才知道安全**——统一成一种形状就不用验。
fn 头(s: &str, n: usize) -> String {
    s.chars().take(n).collect()
}

#[derive(Debug)]
pub struct Outcome {
    /// 程序值；挂起时为 None
    pub value: Option<Value>,
    pub pending: Vec<Pending>,
    pub trace: Trace,
    pub cost: Cost,
    /// 最外层带出的未消费 Unsure（v0.1.1：允许并记）
    pub returned_unsure: Vec<String>,
    /// B162（步 25d）：带回的未决责任按账本键列持有者 `{key, exit, holders: [路径…]}`；只在有键被两个及以上
    /// 持有者带回时非空（单一持有者由 `returned_unsure` 说清）
    pub duties: Vec<serde_json::Value>,
    /// 每次刷新发出的一层（12 §2.2 的分层结果）；层数是 lift / fuse 的量具
    pub layers: Vec<Layer>,
    /// **这一趟跑出来的证据**（`12`:347 的「运行期写入口」）：`(校准键, 观察)`。
    ///
    /// **它是缓冲区，不是写库。** 程序发出证据，宿主决定折不折进 `CalibStore`——
    /// 这样 I4「程序里不可写线」在字面上仍然成立：程序连库的可变引用都拿不到。
    /// **重放不进这里**：重放读的是既有事实，不是新观察。
    pub evidence: Vec<(String, jpp_effects::views::Sample)>,
    /// **停岗候选**（B25）：本趟自动标记的校准键（漂移信号超线）。CLI 的 `--calib-out`
    /// 把它们写成「停岗候选」；正式停岗由人确认（`jpp calib-confirm`）。
    pub suspend_candidates: Vec<String>,
    /// **逐 `cut` 出口的线等级**（步 20f，总账待补「逐出口记线等级」）：每条
    /// `{site, exit, grade, releases, key?, scope_out?, suspend_candidate?}`，`grade` 用 `20` §3.4 的
    /// `LineGrade` 变体名（`Certified` `Form` `Trial` `Provisional` `Class` `Fixture` `Cold`）。
    /// 出口不进账本（`20` §3.7(1)）：等级随校准记录的证书进账本的 `CalibUsed` 条目（账本 v3；v2 在头行 `calib_used`），只凭账本重放时重算出同一张表。
    pub exits: Vec<Json>,
    /// **本趟问过的题**（B107、B120 (a)，步 20h-2）：每个不同的题哈希一行 `{q, form_hash?, template?, fill?, kind}`，
    /// `kind` 是登记读数时算出的精化类（B76）；不带读数与出口。`calib-import --list-out --report` 据此给清单行附题面，
    /// 回填时据此给标注行附题类。
    pub questions: Vec<Json>,
    /// 预算停机（B93，步 22-0）：预算耗尽后未发的判断与效应数、首个未发站点。没耗尽为 `None`
    /// （报告不出 `budget` 段，默认输出逐字节不变）。
    pub budget: Option<BudgetStop>,
    /// 按缓存键复用的计数（步 19，B40、B151）：给了跨运行缓存或本趟有命中时才有（报告 `cache` 一节）。
    pub cache: Option<CacheStats>,
    /// 交给下一轮的整场余额（C-3）：进门余额扣去本轮实际花费、深度 +1。宿主没给上游余额（重放时账本头没有）为 `None`
    pub carry: Option<BudgetCarry>,
    /// 层内挑选的决定（步 30 / B0488）：本趟每次层内挑选一行（层、剩余额度、各组的类别、下游层数、估计费用、
    /// 各校准键本趟计数、是否在钩子给的发出段）。没有挑选时为空（报告不出这一段）。不进账本，审计重放不重算
    pub selections: Vec<Json>,
    /// J-05 默认链（B0492 S2）：每个进过默认链的出口一行 `{key, cause, site, asked, fetched, missed, why, end}`；
    /// 没有时为空（报告不出 `unsure_default` 段，默认输出逐字节不变）
    pub unsure_default: Vec<Json>,
    /// 伴随题（B0492 S5）：报告 `improve` 段，每道带了伴随题的原题一行 `{key, q, companions: [{kind, key, p}]}`；
    /// 没有时为空（报告不出这一段）
    pub improve: Vec<Json>,
    /// `order` / 候选分档用的并档容差（Z0425）：每行 `site`、`tol`、`tol_source`（record | profile | unknown）；
    /// 并档是否可信看这里，下游出口不继承
    pub orders: Vec<Json>,
    /// 超窗次数（Z0918）：`{text, ctx, group}`——对象槽单段超窗、语境槽超窗、同材料一次请求（语境 + 候选）超窗。
    /// 画像没测窗口时为 `Null`（不核窗口，计数无意义），报告不出这一段
    pub window_over: Json,
    /// 点名类别无取法（Jpp 2026-10-02）：`{类别: 次数}`——伴随题点名（「最缺哪类」选出，或「参照与语境够吗」带外判否映射）
    /// 的类别、默认链取不到的次数。为空时 `Null`，报告不出这一段
    pub named_unfetchable: Json,
    /// 查不到结构化站点、回退成 `@<偏移>` 的次数（B0630）。为 0 时报告不出这一项；不为 0 说明运行时进键的 span 有站点表外的来源
    pub site_key_fallback: u64,
    /// 违规的单次形态（G2，`12` R9）：程序结束时还欠着的未决，每笔一项；值照带，这次结论为「未决（violation）」。
    /// 为空即没有违规
    pub violations: Vec<ViolationReport>,
    /// 守卫下推迟、结算时才执行失败的不可逆 `do`（Z0565）：每条 `{site, action, detail}`；不算 J-12，退出码不变
    pub settle_failed: Vec<Json>,
    /// 单元图的只读统计（C2b，步 41）：单元图关着为 `None`。不进报告（报告由宿主按字段拼，不含它），给测试用
    pub cells: Option<单元统计>,
}

/// 预算停机的记账（B93）：停发，不停程序。
#[derive(Clone, Debug, serde::Serialize)]
pub struct BudgetStop {
    pub exhausted: bool,
    /// 预算耗尽后没发出的判断（按账本键计）与效应（`gen`、`do`）个数
    pub unsent: u64,
    /// 首个未发站点的起始字节偏移
    pub first_site: usize,
    /// 停的原因：`None` = 预算耗尽（B93，报告逐字节不变）；`"depth"` = 跨程序触发链已到深度上限，这一趟只停发、照常求值（G4）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cause: Option<String>,
}

impl Outcome {
    pub fn value_json(&self) -> Json {
        self.value
            .as_ref()
            .map(|v| v.to_json())
            .unwrap_or(Json::Null)
    }
}

struct Frame {
    name: String,
    exits: Vec<Rc<Exit>>,
    /// 本帧登记、尚未解析的惰性出口（B94，步 23c）：帧返回前全部解析，出口挂回本帧
    cuts: Vec<Rc<PendingCut>>,
    /// 返回类型提到 Exit：未消费的 Unsure 由调用者接手
    returns_exit: bool,
    /// 本帧过桥的次数（G2：欠账记号的「第几次」）
    过桥: u32,
    /// 帧的主人（G2 附录三，Z0564、R9）：函数帧是 `<函数名>#<实参哈希>`；程序顶层帧不用（取程序单元身份）。
    /// Z0882：只在本帧真的建出口时才算（大多数调用不过桥，算了也用不上）
    主人: 帧主人,
}

/// 帧主人：已算好的，或等到第一次要用时再算的（函数名加实参；值不可变，什么时候算结果都一样）。
pub(crate) enum 帧主人 {
    已算(String),
    待算 { 名: String, 实参: Vec<Value> },
}

impl 帧主人 {
    pub(crate) fn 取(&mut self) -> String {
        if let 帧主人::待算 { 名, 实参 } = self {
            let s = format!("{名}#{}", crate::violation::实参哈希(实参));
            *self = 帧主人::已算(s);
        }
        match self {
            帧主人::已算(s) => s.clone(),
            帧主人::待算 { .. } => unreachable!(),
        }
    }
}

struct LoopCtx {
    seen_keys: HashSet<String>,
    repeated: Option<String>,
}

pub struct Interp<'a> {
    /// 按效应实例索引的端口表（步 15b，`20` §2.3 `Ports.effects`）：判断、生成、问人只经它发调用
    ports: Ports<'a>,
    ledger: &'a mut dyn LedgerPort,
    /// 校准只经只读视图读（步 11b，`20` §2.3：运行时不依赖 `CalibStore` 具体类型）
    calib: &'a dyn CalibView,
    /// 私有读数表：读数是句柄，答案只在这里（步 11b-3）。写只经 `flush.rs::fill_answer`，
    /// 读只经 `readings.rs::answer_of`。
    answers: std::cell::RefCell<AnswerTable>,
    /// 读数 id → 判断器随答案报的自报置信度（B154，步 20j-3）。与答案同处写（`flush.rs::fill_from_record`），
    /// 没报的读数不在表里。`Reading` 不带它：读数是句柄，读数的内容只在持有者的表里。
    置信表: std::cell::RefCell<HashMap<u64, f64>>,
    /// 声明式拟合的数（B153 (2)，步 20j-4）：`Score` 的 id → 拟合出的数。只由拟合分支写（经 `IssueReading`），
    /// 只由 `cut`、`order` 读（经 `ReadAnswer` / 桥）；程序读不出它（J-01 型面不变）。
    拟合表: std::cell::RefCell<HashMap<u64, f64>>,
    next_score: std::cell::Cell<u64>,
    /// 正在求值声明式拟合的闭包（>0）：此时效应、内核构造、出口与责任形式、读答案的刷新点一律拒绝（B153 (2)）
    pub(crate) 拟合中: u32,
    /// 谱系放行（B72-4，步 20j-4）：`Score` 出口的合成账本键 → 各输入读数的账本键
    拟合谱系: std::cell::RefCell<HashMap<String, Vec<String>>>,
    next_reading: std::cell::Cell<u64>,
    actions: &'a ActionRegistry,
    fits: Fits<'a>,
    budget: Budget,
    pub trace: Trace,
    pub cost: Cost,
    frames: Vec<Frame>,
    loops: Vec<LoopCtx>,
    next_exit: usize,
    depth: u32,
    /// 本段递归深度的峰值（C2c 复核 K1）：代码单元记下首次求值往下走了几层，取记忆时按它核 J-06
    pub(crate) 深度峰: u32,
    run_seq: u64,
    model_id: String,
    /// 已登记但还没发出的判断（`12` §2.2「登记后不发」）
    pending: Vec<PendingJudge>,
    /// 交出去还没取回的生成与生成缓存（B149，步 15h-2：`gen_pending.rs`）
    生成: gen_pending::GenState,
    /// 调用号 → 这次调用的费用（账本里同调用号判断条目的最大 `cost`）。账本只增，按已扫到的条目数增量补
    /// （L7 2026-09-28：合批只在首条记费，读者按调用号取最大，见 `gen_pending.rs::调用费`）
    调用费表: std::cell::RefCell<(usize, HashMap<u64, f64>)>,
    /// **审计重放**（B35；21 步 3）：只凭账本重现首跑。账本里记过的调用照记录计入预算，
    /// 使首跑在哪里预算停机，重放就在哪里停；账本缺的记录报 `E-replay`（致命，不进 cause）。
    /// 续跑（`--resume`）不开：已记录的不付费、继续往下。依据：12 §2.3 B35 注；21 §三·2 步 3。
    audit: ReplayAudit,
    /// 宿主入口参数（B105；步 14b-0 起 `--input`）：运行入口绑定进环境，哈希进账本头 `entry_hash`
    entry: EntryArgs,
    /// 入口材料的哈希 → 入口名（J-08 报文「该材料是宿主入口 <名>」用；B105）
    entry_mat_names: HashMap<String, String>,
    /// 每次刷新发出的一层，供测试与 Trace 看分层结果
    pub layers: Vec<Layer>,
    /// 本次运行的计划（`jpp-plan` 写，这里只读）。步 14a 起由宿主算好经 [`Interp::run`] 交进来
    /// （`20` §2.2 第 3 条：运行时不依赖 `jpp-plan`；pass 开关随之留在宿主，`jpp::interp::Interp`）
    plan: jpp_ir::plan::Plan,
    /// 规划的运行期钩子（`20` §2.3 `Ports.hooks`）。步 14a 起由宿主经 [`Interp::run`] 注入
    hooks: &'a dyn jpp_ir::plan::PlanHooks,
    /// 当前所处的条件链：每层是「这个条件里有没有一个 **trusted 来源的合取项**」。
    ///
    /// J-08（`12`:265）说的「放行不可逆 `do` 的**守卫表达式**」，在一个有 `if` 的语言里
    /// 就是**包着这个 `do` 的那些条件**——不必是 `do` 的一个参数。条件求值成 `Bool` 之后
    /// taint 就没了，所以在**求值条件的那一刻**记下来。
    guards: Vec<GuardEv>,
    /// 推测登记且自付一次调用发出了的跨状态键（步 22 / B0487 起由 `flush.rs` 维护：登记时先记，刷新时拿掉本层
    /// 待发的，只含推测的组发出时加回）。程序真走到时命中账本，没走到的留 `W-spec-unused`（B51-C1）
    speculated: HashSet<String>,
    /// 推测的键里，真被程序用上的
    speculation_used: HashSet<String>,
    /// 读数站点的 `span.start → SiteId`（步 30 / B0488）：`run` 入口按 `Program.sites` 建，只收产出读数的站点种类
    /// （判断效应、`literalize`、构造）；同一起点有多个候选的不收。判断键本来就以 `span.start` 作站点，同口径
    站点表: HashMap<usize, jpp_ir::key::SiteId>,
    /// 本趟逐校准键计数（步 30 / B0488；读数进「接下来先问什么」，B22 第 5 类消费者）：键 → (过桥且有答案的读数条数,
    /// 其中出口为 `Unsure` 的条数)。同一读数切两次只计一次（`键计数已记`）
    键计数: HashMap<String, (u64, u64)>,
    键计数已记: HashSet<u64>,
    /// 层内挑选的决定（步 30 / B0488，主控 Q7/Q9）：每次钩子被调用记一行，进报告 `selections`，不进账本
    挑选记录: Vec<Json>,
    /// `order` 并档容差的报告行（Z0425），进报告 `orders`
    并档记录: Vec<Json>,
    /// 超窗次数（Z0918，裁定七十三 (3)）：`[对象槽单段超窗, 语境槽超窗, 同材料一次请求超窗]`，与 `W-window` 告警同一判据、
    /// 每次告警计一次；进报告 `window_over`，摘要从这里取，不从日志手数
    超窗计数: [u64; 3],
    /// 点名类别无取法（裁定七十七之后，Jpp 2026-10-02）：伴随题点名的类别、默认链取不到的次数，按类别计；进报告
    /// `named_unfetchable`。点名了却取不到本身是给作者的改进信息（意图汇编 30），不算失败
    点名无取法: std::collections::BTreeMap<String, u64>,
    /// 最近一次默认链末端转交时缺的类别（`unsure_default` 回给调用方，写进去向：「缺<类别>、无取法」）
    末次缺: Vec<String>,
    /// 裁定七十八：过程入口的动作题由 `drive.jpp` 打标记（`unsure_default(出口, {end: "top"})`），这一次走链的末端
    /// 判过而仍拿不准时取最大项；`末次最大项` 回（最大项下标, 候选数）。只在 `b_unsure_default` 里置、用完即清
    末端取最大项: bool,
    末次最大项: Option<(usize, usize)>,
    /// 最大项取自哪一次读数（账本键）：缩小后再问时它的候选是缩小集，按值映回原候选
    末次最大项键: Option<String>,
    /// Z0913：中间判断没走成的原因（子题拿不准、子题未发出、缩小后为空、缩小未发出），调用方写进 pending 的去向
    末次题树败因: Option<&'static str>,
    /// Z0913（D1）：题树层数计入运行时深度，只下一层（≤ 1）
    题树深: u32,
    /// Z0913：过程入口的动作题交来的题树规格（子题 S、缩小题式、开关），只在 `b_unsure_default` 里置、用完即清
    题树: Option<crate::unsure_default::题树规格>,
    /// 首个停发站点的延后记法（步 30，B93 第 6 条；复核 B0488-A 缺口 1）：本次刷新重排过、且本趟此前没有停发时为
    /// `Some`，各停发组的（原登记下标, 条数, 站点, 报文）先攒在这里，刷新结束取原登记下标最小的一组写 `W-budget` 与
    /// `first_site`——与审计重放按登记顺序停发的第一组相同。挑选只改发出顺序，不改首个停发站点的记法
    首停延后: Option<Vec<(usize, u64, jpp_ir::ir::Span, String)>>,
    /// 当前处理的组在本次刷新里的原登记下标（配合 `首停延后`）
    当前组位: usize,
    /// 布尔绑定的来源：`let x = …` 求值过程中产生的出口带着状态 taint，据此答
    /// 「这个布尔是不是由**可信状态上的判断**决定的」（J-08）。名字被重新绑定时覆盖。
    /// 来源通道**的作用域由环境给**：`let x = …` 时把来源连同值一起绑进 `env`，
    /// 名字叫 `x\u{1f}prov`。于是它天然随块退出而消失、被重新绑定而覆盖、
    /// 在 helper 的帧里查不到外层的——**和它描述的那个值同一个作用域**。
    ///
    /// 此前这里是一张按名字索引的 `HashMap` 旁路表，**没有作用域**：
    /// 一次「返回值不是 Bool/Record、内部走过 ask」的调用会把「经过 ask」留在传送带上，
    /// 被之后**任意一条不相关的 `let`** 继承——连 `let ok = true;` 都会被污染。
    /// 而它**同时**会漏（`lifecycle.jpp` 那次假拒绝）：**漏和串是同一个病的两种表现**。
    ///
    /// 判别法（`12` §2.11 第五条）：**这条来源通道，它的作用域是谁给的？**
    /// 账本里已有的 `ask` 条数：只参与核 `budget.escalate` 总上限，**不进 `Cost.asks`**
    /// （那个报的是「这次运行实际问了几次人」，重放时该是 0）。
    asks_in_ledger: u64,
    /// **缺席 / 超时标记**（B32）：账本键 → `absent` / `latency`。`cut` 据此给 `Unsure(原因)`。
    absent_marks: HashMap<String, String>,
    /// 按读数记的缺席原因（G5，裁定五十九第 16 条）：同一内容键之后可以再问，已经缺席的那个读数不被后来的答案改写
    读数缺席: HashMap<u64, String>,
    /// 审计重放（G5 附录二）：最后一条趟标记与那一趟的缺席记录（首次用到时扫账本建表；没有趟标记为 `None`）
    重放趟: std::cell::OnceCell<Option<readings::重放趟表>>,
    /// 本趟每道题（账本键）由真站点登记了几次（G5 附录二：缺席记录的读数身份）
    真登记次数: HashMap<String, u32>,
    /// 读数 → 它是这道题本趟第几次真登记（推测、提升登记的读数不在表里）
    真登记序: HashMap<u64, u32>,
    /// 正要发出的组里，每道题的真登记序号（同键有多条真登记取最小；只有推测、提升登记的不在表里）
    待发真序: HashMap<String, u32>,
    /// 审计重放里按缺席复现的读数 → 取哪一条缺席记录（G5 附录一）
    重放缺席键: HashMap<u64, String>,
    /// 连续缺席次数（熔断用）
    consecutive_absent: u32,
    /// 本趟算出的判断键：账本键 → 结构化键（账本 v2 条目记结构化键，步 7）
    judge_keys: HashMap<String, JudgeKey>,
    /// 判断键的渲染分量（B155，步 15i）：缺省 `RENDER_VERSION`；只凭账本重放时取账本头记的版本，
    /// 旧渲染的账本照样命中（重放不发请求，不违反 B48）。续接遇到旧渲染在 `run` 入口拒绝。
    render: String,
    /// 本趟的键法（B0630）：`run` 入口按账本头的 `key_version` 选；新跑取 `KEY_VERSION_CURRENT`。
    /// 写进新头；只凭账本重放旧键法账本时取账本的键法，头里照写它（与键一致，能再次重放）。
    key_version: jpp_ledger::key_version::KeyVersion,
    /// 结构化站点表（B0630）：`Program.site_keys`，宿主没写（不经 `Session`）时在 `run` 入口按 IR 现算（全部算非 lib）
    站点键: jpp_ir::site_key::SiteKeys,
    /// 查不到结构化站点、回退成 `@<偏移>` 的次数（B0630），进报告 `site_key_fallback`，为 0 不出
    站点回退: u64,
    /// 本趟算出的效应键：账本键 → 结构化键（步 7）
    effect_keys: HashMap<String, EffectKey>,
    /// 本趟判断调用累计耗时（秒，B32 时延预算）
    latency_spent: f64,
    /// 宿主交进来的上游余额（C-3）：`run` 入口据此收紧 `budget`、定起算深度；审计重放改取账本头里的
    上游: Option<BudgetCarry>,
    /// 带上游余额时的 (程序声明的深度, 上游深度上限, 起算深度)：J-06 报文据此区分「程序声明」与「上游收紧」（C-3）
    深度上游: Option<(u32, u32, u32)>,
    /// 本趟开跑时账本里已答的问人次数（C-3）：`Spent.asks` 记本趟新增的得到回答的次数
    本趟起答: u64,
    /// 宿主显式重新授权（C-3 R1）
    重新授权: bool,
    /// 不带余额的一趟在账本头有余额、前一趟被杀时，开跑前要补的结清（C-3 Z0384 B1）
    无余额结清: Option<Entry>,
    /// 程序声明的 `budget.depth`（G4b）：进门收紧之前记下（外层 `None` = 还没记）；J-06 的递归上限只取它（没声明取引擎默认），不被余额收紧
    声明深度: Option<Option<u32>>,
    /// 引擎默认深度上限（G4b，裁定六十四）：宿主配置，缺省 [`DEFAULT_DEPTH`]；审计重放取账本头的 `depth_cap_default`
    深度默认: u32,
    /// 跨程序触发链到限（G4：`hop ≥ depth_cap`）：这一趟判断一律不发（`Unsure(depth)`、`Unasked`），效应产出失败值
    深度停: bool,
    /// 本趟的续跑计数（G4 一·5）：带余额时取余额的 `round`，否则取开跑前账本里的段数；`attempt.n = 本趟轮 + 1`
    本趟轮: u32,
    /// 账本里（开跑前）记为未问的题：键 → 原因（G4 一·4：审计重放在同一站点照样停发、续跑照样重发）；首次用到时建
    未问表: std::cell::OnceCell<HashMap<String, String>>,
    /// 停发说明（G4 一·4）：停发那一刻由预算核对算出的文字，按键留在内存（`W-sieve-budget` 与契约值的 `resume.detail`
    /// 用；原先取自账本 `Absent.detail`）
    停发说明: HashMap<String, String>,
    /// 出口的欠账记号成分（G2）：出口号 → 帧种类、主人、过桥种类、帧内第几次
    记号表: HashMap<usize, violation::记号成分>,
    /// 函数返回前丢了、被挂到调用者的未决（G2）：出口号 → 最初所在的函数名
    丢失处: HashMap<usize, String>,
    /// 程序结束时记下的违规（G2）：（报告, 出口号）
    违规: Vec<violation::违规笔>,
    /// 结算时才失败的推迟动作（Z0565，报告 `settle_failed`）
    结算失败: Vec<Json>,
    /// 守卫下推迟的不可逆 `do`（G2，B200）
    推迟: Vec<violation::推迟动作>,
    /// 当前建出口的桥（G2：`cut` 或 `cut_score`）
    当前桥: jpp_ledger::Via,
    /// 本趟已写过「停下」（G4：一趟一条）
    停下已写: bool,
    /// 本趟已记「未问」的键（G4：同一键一趟一条）
    未问已记: HashSet<String>,
    /// 本趟见过的单次判断调用最大费用（步 15e：cost 预算按它限窗；`None` = 还没观察到）
    c_max: Option<f64>,
    /// 预算停机（B93，步 22-0）：首次停发时置上；此后登记的站点同样停发、费用为零
    预算停: Option<BudgetStop>,
    /// 解析惰性出口时借用的出口号与所属帧（B94，步 23c）：`new_exit_from` 取用一次即清
    出口预定: Option<(usize, usize)>,
    /// 已停发过的判断键（`unsent` 按键去重：同一键经提前登记与真站点各进一次刷新时只计一次）
    停发键: HashSet<String>,
    /// 这一轮里被 `content()` 从 **untrusted 材料**拆出来的内容（规范 JSON）。
    /// `mat()` 拿到其中之一时不能当字面量洗成 trusted——见 `as_mat` 的兜底臂。
    /// 从 untrusted 来源拆出的**文本叶子**（K-182 / K-203，2026-09-23）：派生出的新字符串
    /// （拼接、join、slice、text）按子串关系认回来，不再只做整值精确匹配。
    /// 含有「成分不可信的计算值」材料的状态哈希（J-08 诊断用，B33 第 8 点）
    computed_untrusted_states: std::cell::RefCell<HashSet<String>>,
    /// 含「宿主入口、未声明可信」材料的状态哈希 → 入口名（J-08 诊断用，B105）
    input_untrusted_states: std::cell::RefCell<HashMap<String, String>>,
    /// 本趟已记下命中的校准键（`note_calib` 去重；账本 v3 起 `calib_used` 是账本条目的派生视图，
    /// 跨趟保留，不再在入口清空）
    本趟已记校准: HashSet<String>,
    /// C-1：本趟每条去向事件（按序列化文本）是第几次出现，及账本里这趟之前已有几条相同的（续跑、重放不重复写）
    去向计数: HashMap<String, (usize, usize)>,
    /// J-05 默认链（B0492 S2；Z0398 起两项都可省）：程序声明的取材料来源 `unsure_source({need?, fetch?})`：
    /// 类别清单（候选第一级，空 = 没给）与取材料函数（取法第一级）
    未决来源: Option<(Vec<String>, Option<Rc<Closure>>)>,
    /// 默认链再判要用的来历：读数账本键 → 发这道题时的状态与题（`judge` 登记时记）
    判断来历: HashMap<String, (Rc<State>, Rc<Question>)>,
    /// 读数账本键 → `cut` 的校准键与切法（`cut` 时记；再判用同一条线切）
    切法来历: HashMap<String, (Option<String>, bridge::CutOpts)>,
    /// 报告 `unsure_default` 段：每个进过默认链的出口一行
    默认链记录: Vec<Json>,
    /// 「无作者去向」站点（B0492 S2c）：运行入口从 `Program.unsure_default_sites` 取；这些站点切出未决时当场走默认链
    默认链站点: BTreeSet<usize>,
    /// 默认链再判要用的读数：读数账本键 → 读数（读数触发看它离边界多远）
    读数表: HashMap<String, Rc<Reading>>,
    /// 正在补信息（>0）：这期间的再判不再触发读数触发（B0492 S2b）
    链中: u32,
    /// 审计重放里，这趟开始前账本按次数找不到的去向事件（W-replay-duty，主控 2026-09-29）
    本趟去向: Vec<Entry>,
    /// 伴随题（B0492 S5）：`unsure_source` 给的题式；每道原题带的伴随题；并行发法下伴随题的读数键；登记伴随题中
    伴随题式: Option<Vec<Value>>,
    伴随: Vec<companions::伴随组>,
    pub(crate) 伴随键: HashSet<String>,
    pub(crate) 伴随中: bool,
    /// 正在登记裂变的块（>0）：块不是新题，不带伴随题（主控 2026-09-30）
    pub(crate) 裂变块中: u32,
    /// 程序顶层环境（运行入口记下；伴随题取 `unsure_companions` 用）
    pub(crate) 顶层环境: Option<Env>,
    /// 伴随题序言（B0492 S5，主控 2026-09-30 路 A）：宿主交来的 `lib/unsure.jpp` 降级结果；运行入口先求值它，
    /// 取其中的 `unsure_companions` 作标准题式（`序言伴随`）
    序言: Option<Program>,
    序言伴随: Option<Vec<Value>>,
    /// 序言里的通用类别表 `unsure_lacks`（裁定五十一；默认链候选的末级）
    序言类别: Option<Vec<String>>,
    /// Z0556：语言自己发的元题（伴随题、默认链「为什么拿不准」）的读数号；刷新时答案形状不符就降级（丢读数、报
    /// `W-companion-shape`），不中止。`元题登记` > 0 时 `judge` 登记的读数都记进来
    pub(crate) 元题: HashSet<u64>,
    pub(crate) 元题登记: u32,
    /// 默认链取材料的第二、三级（裁定五十五）：程序料库与宿主取材料端口，宿主经 `Session` 交来
    料库: Option<Rc<dyn jpp_effects::CategoryStore>>,
    宿主取材料: Option<Rc<dyn jpp_effects::MaterialSource>>,
    /// 本趟已记的声明线记录（键 + 记录哈希；B175 (3)，步 20j-3 追加 (8)）：同一站点每个不同的线各记一条
    本趟已记声明: HashSet<(String, String)>,
    /// 层末条目追加时账本端口报的错（B55，步 18b）：先记下，下一次层末落盘时报 `E-ledger-io`
    账本错: Option<LedgerError>,
    /// 按缓存键复用（步 19）：本运行的缓存键表、跨运行缓存、生成器身份、计数
    复用: reuse::ReuseState<'a>,
    /// 谱系放行（B72-4，步 17b）：本趟切过的出口，按账本键记「是否全部已决且放行」与第一个不放行者的说明。
    /// 同一键切多次时须全部放行（17b 解释登记 (c)）。
    出口放行表: HashMap<String, (bool, String)>,
    /// 谱系断的出口：出口 id → 说明（J-08 报文补「该材料由 … 的出口选出」用）
    谱系断: std::cell::RefCell<HashMap<usize, String>>,
    /// B52（步 21）：判为 Fn¹ 的责任 → 那个闭包的名字（J-05 报文说明「唯一路径是哪个闭包」用）
    fn1_of: HashMap<usize, String>,
    /// B95（步 21）：本趟显式 drop 过的未决出口（返回前核 `W-drop-then-return` 用）
    dropped: Vec<Rc<Exit>>,
    /// B162（步 25d）：已解除的未决责任，按账本键：键 → 首次解除的方式与站点。同一键的多个持有者是一份
    /// 责任的多个视图，任一处消费即解除；再次消费报 `W-duty-twice`
    解除: HashMap<String, String>,
    /// 已报过 `W-lineage-unknown` 的祖先键（每键报一次）
    谱系缺键已报: HashSet<String>,
    /// 这次运行已经报过漂移的键：**一条天天响的告警等于没有告警**
    drift_reported: HashSet<String>,
    /// B104：`W-delta-unknown` / `W-scope-unknown` 每键每趟只报一次（值为「告警码\u{1f}键」）
    unknown_reported: HashSet<String>,
    /// 本趟各校准键切过的读数的统计量值（B128 `near_line` 的分母，步 20j-1）。按（键，统计量）分组：
    /// `max` 组的键就是校准键（与 20j-1 同），其他统计量的组键为「校准键\u{1f}统计量规范 JSON」（步 20j-3）
    键读数: HashMap<String, Vec<f64>>,
    /// 声明线出口的说明（出口 id → 「@站点 hi lo，标注 n 条」），J-08 拒绝报文用（B128）
    声明出口: HashMap<usize, String>,
    /// 宿主开启放行把关（意图汇编 11a；`Program.entry.guard`，在 [`Interp::run`] 入口取）。默认关：`release`
    /// 不拦任何 `do`，`W-lineage-unknown` 不报，`W-declared-line` 不说放行
    pub(crate) guard: bool,
    /// 运行期诊断闸门（步 26，B47）：由宿主注入，实现在 `jpp-check::diag::gate`（运行时不依赖检查器）。
    /// 未注入时不诊断。只提示、不改走向（批 9 裁定 :42、:136）。
    pub(crate) gate: Option<&'a dyn jpp_ir::diag_gate::QuestionGate>,
    /// 本趟已过闸门的题哈希：每道题每趟只诊断一次
    pub(crate) gated: HashSet<String>,
    /// 宿主变换表（步 26，`12` §2.8 `transform(f: HostFn, …)`、`20` §2.3 `Ports.transform`）：由宿主注入，
    /// 登记者是 `jpp-lib::s_library()`。未注入时 `transform("名", …)` 报 `E-rt-arg`。
    pub(crate) transforms: Option<&'a jpp_effects::TransformTable>,
    evidence: Vec<(String, jpp_effects::views::Sample)>,
    /// 逐 `cut` 出口的线等级（步 20f，报告 `exits`；出口不进账本，重放时重算）
    exit_grades: Vec<Json>,
    /// 出口 id → 它在 `exit_grades` 里的行（步 25-2b，B133）：`cut` 写完一行时记；元素构造 `element` 按它找行写
    /// `index`/`pos`（B120 (b)），不依赖「最后一行」。
    exit_rows: HashMap<usize, usize>,
    /// 读数的精化题类（B76，步 12e-2；`21` 写作 `ReadingMeta.kind`）：按读数句柄记，登记读数时算。
    /// 放在这里而不放 `Reading` 上，是为了不改 `Reading` 的字面量构造（宿主与测试各处）。
    reading_kinds: HashMap<u64, QuestionKind>,
    /// 报告的 `questions` 表（B107、B120 (a)，步 20h-2）：每个不同的题哈希一行，按首次登记的顺序；不带读数
    questions: Vec<Json>,
    /// 超窗裂变的合成读数（步 23b）：合成读数的句柄 → 它的块读数与合回方式。`judge` 在声明了
    /// `fission: "approx"` 的超窗站点返回合成读数，`cut` 据此逐块过桥再合回（`fission.rs`）。放在这里而不放
    /// `Reading` 上，理由同 `reading_kinds`
    裂变表: HashMap<u64, Rc<fission::合成读数>>,
    /// 裂变出的块站点的账本键（步 23b；`20` v2 §4.5 第 3 条末句「超窗裂变出的调用按跨状态推测计」）：层内挑选给组定类别时
    /// 不把它们算作真站点，只含这些键的组归跨状态推测、预算不够时先让出（`budget.rs::层内挑选`）
    pub(crate) 裂变块键: HashSet<String>,
    /// 窗口未测的读数句柄（Z0364，裁定四十九 (c)；设计写作 `ReadingMeta.window_untested`）：开关开、画像没测窗口、
    /// 题声明了 `fission: "approx"` 的读数（本想按窗切、因窗口未测没切）。`cut` 出口据此置 `Exit.window_untested`。
    /// 放在这里而不放 `Reading` 上，理由同 `reading_kinds`
    pub(crate) 窗口未测读数: HashSet<u64>,
    /// 单元图（C2b，步 41；`cells.rs`）：缺省开，`set_cells(false)` 关（H1a 对照臂），关着为 `None`
    单元: Option<cells::单元图>,
}

/// 一次 `judge` 登记：一个状态 + 它那几道题
struct PendingJudge {
    state: Rc<State>,
    items: Vec<(Rc<Question>, Rc<Reading>, String)>,
    site: Span,
    /// 推测登记的（不是程序真走到的站点）。超预算时**先丢它**，再动真站点。
    speculative: bool,
    /// 直线段提升登记的（B94 下半，审查修复 3b）：刷新分组按组内第一条非提升登记的位置排序，
    /// 只有提升登记的组排在最后，不挤掉程序序更靠前的真站点
    lifted: bool,
    /// 登记处的站点号（步 30 / B0488）：按 `span.start → SiteId` 表查（`Interp::站点表`），查不到为 `None`。
    /// 层内挑选按它取计划里的下游层数
    site_id: Option<jpp_ir::key::SiteId>,
}

/// 一次刷新发出的一层
#[derive(Clone, Debug, PartialEq)]
pub struct Layer {
    /// 触发这次刷新的刷新点：`cut` / `if` / `end` / `content` …
    pub reason: String,
    pub calls: u64,
    pub questions: usize,
}

pub const BUILTINS: &[&str] = &[
    "state",
    "test",
    "select",
    "measure",
    "form",
    "fill",
    "judge",
    "sieve",
    "pair",
    "tally",
    "first_k",
    "iterate",
    "outcome",
    "key_of",
    "action_fact",
    "element",
    "cut",
    "handle",
    "consume",
    "gen",
    "do",
    "ask",
    "transform",
    "mat",
    "content",
    "unsure",
    "pending",
    "fail",
    "is_fail",
    "loop",
    "stop",
    "unsure_cause",
    "untested",
    "near_boundary",
    "unsure_default",
    "line_source",
    "cert",
    "compose",
    "taint",
    "escalate",
    "literalize",
    // J-05 默认链（B0492 S2）：取材料函数的声明、库代码记细化
    "refine",
    "unsure_source",
    "unsure_fetch",
    "allocate",
    "unsure_bound",
    "agg",
    "repeat",
    "order",
    "fit",
    "gate_info",
    "split_point",
    "known_answers",
    "state_within",
    "len",
    "map",
    "filter",
    "fold",
    "range",
    "append",
    "concat",
    "slice",
    "contains",
    "sum",
    "reverse",
    "keys",
    "with",
    "has",
    "text",
    "join",
    "print",
    "min",
    "max",
    "abs",
    "floor",
    "exit_kind",
    // 文本与数据内置（B157，步 7t）
    "split",
    "lower",
    "upper",
    "trim",
    "replace",
    "starts_with",
    "ends_with",
    "index_of",
    "chars",
    "regex_match",
    "regex_find",
    "sort",
    "sort_by",
    "parse_json",
    "to_json",
    "hash",
    "date_parse",
    "date_format",
    "date_add",
    // 带种子伪随机（B158，步 7t）
    "rand",
    "rand_int",
    "shuffle",
];

pub fn root_env() -> Env {
    let env = env_root();
    for b in BUILTINS {
        env_define(&env, b, Value::Builtin(b));
    }
    env
}

/// 空的 fit 表：不用 fit 的程序共用这一个（步 14a 前是线程局部 leak 的一份空 `FitRegistry`；
/// 运行时不依赖 `jpp-calib`，改为恒空的 `FitView` 实现，查什么都查不到，与空注册表同义）。
pub struct NoFits;
impl FitView for NoFits {
    type Record = Rc<FitRecord>;
    fn get(&self, _fit_ref: &str) -> Option<&Rc<FitRecord>> {
        None
    }
}

/// `fit` 注册表的只读视图（`20` §2.3 `Ports.fits`；`jpp-calib::FitRegistry` 实现它）。
pub type Fits<'a> = &'a dyn FitView<Record = Rc<FitRecord>>;

/// 构造到 [`Interp::run`] 之间的占位钩子：计划与钩子只在 `run` 入口注入，此前运行时不求值，
/// 所以它不会被调用（步 14a）。
struct Unplanned;
impl jpp_ir::plan::PlanHooks for Unplanned {
    fn instantiate(
        &self,
        _: &jpp_ir::plan::Plan,
        _: &Function,
        _: &dyn jpp_ir::plan::EnvView,
    ) -> Vec<jpp_ir::plan::Target> {
        unreachable!("计划与钩子在 Interp::run 入口注入")
    }
    fn speculate<'b>(
        &self,
        _: &jpp_ir::plan::Plan,
        _: jpp_ir::key::NodeId,
        _: &'b Block,
        _: &dyn jpp_ir::plan::EnvView,
    ) -> Vec<&'b Expr> {
        unreachable!("计划与钩子在 Interp::run 入口注入")
    }
    fn segment(
        &self,
        _: &jpp_ir::plan::Plan,
        _: jpp_ir::key::NodeId,
        _: &Block,
        _: &dyn jpp_ir::plan::EnvView,
    ) -> Vec<jpp_ir::plan::Target> {
        unreachable!("计划与钩子在 Interp::run 入口注入")
    }
    fn may_effect(&self, _: &Expr, _: &dyn jpp_ir::plan::EnvView, _: jpp_ir::plan::Reach) -> bool {
        unreachable!("计划与钩子在 Interp::run 入口注入")
    }
}

mod bridge;
mod budget;
mod builtins_text;
mod caps;
mod carry;
mod cells;
mod companions;
mod constructs;
mod duty;
mod effects_exec;
mod entry;
mod eval;
mod fission;
mod flush;
mod gen_pending;
mod guard;
mod host_builtins;
mod outcome;
mod plan_view;
mod readings;
mod register;
mod reuse;
mod schedule;
mod undecided;
mod unsure_default;
mod violation;
pub use cells::单元统计;
pub use violation::{ViolationReport, ViolationView};
pub mod strength;
pub use builtins_text::RAND_VERSION;
use caps::Caps;
pub use caps::{ConstructSpec, Privilege, construct_specs};
pub use carry::{BudgetCarry, TripCarry, rewrite_plan_rejection};
pub use entry::{EntryArgs, EntryMat, EntryValue, HostAccept};
pub use reuse::CacheStats;

use readings::refresh_point;

impl<'a> Interp<'a> {
    /// 注入运行期诊断闸门（步 26，B47）。宿主构造闸门（`jpp_check::diag::RuntimeGate::new(&program)`）后调用；
    /// 不调用则运行期不诊断。
    pub fn set_gate(&mut self, gate: &'a dyn jpp_ir::diag_gate::QuestionGate) {
        self.gate = Some(gate);
    }

    /// 注入宿主变换表（步 26）：`transform("名", 材料…)` 按名字取宿主函数，记账同闭包形式（`12` §2.8）。
    pub fn set_transforms(&mut self, t: &'a jpp_effects::TransformTable) {
        self.transforms = Some(t);
    }

    /// 不带 fit 注册表的入口（绝大多数程序不用 fit）。要用 fit 走 `with_fits`。
    /// 按端口表构造（步 15b；步 15c 起唯一入口）：运行时只经端口发调用。不带 fit 注册表（绝大多数
    /// 程序不用 fit），要用 fit 走 `with_fits`。
    pub fn new(
        ports: Ports<'a>,
        ledger: &'a mut dyn LedgerPort,
        calib: &'a dyn CalibView,
        actions: &'a ActionRegistry,
        budget: Budget,
    ) -> Interp<'a> {
        Interp::with_fits(ports, ledger, calib, actions, &NoFits, budget)
    }

    /// 账本头与判断键的模型 id 取服务「产出读数的效应」的那个实例的模型（B60；I5）；没有注册判断端口时
    /// 取第一个已注册实例的模型（这时任何判断调用都会报没有端口）。
    pub fn with_fits(
        ports: Ports<'a>,
        ledger: &'a mut dyn LedgerPort,
        calib: &'a dyn CalibView,
        actions: &'a ActionRegistry,
        fits: Fits<'a>,
        budget: Budget,
    ) -> Interp<'a> {
        let model_id = jpp_effects::find(|s| s.produces_reading)
            .and_then(|e| ports.instance_of(e))
            .or_else(|| ports.instances().into_iter().next())
            .map(|i| i.model)
            .unwrap_or_default();
        Interp {
            ports,
            ledger,
            calib,
            answers: Default::default(),
            置信表: Default::default(),
            拟合表: Default::default(),
            next_score: Default::default(),
            拟合中: 0,
            拟合谱系: Default::default(),
            next_reading: Default::default(),
            actions,
            fits,
            budget,
            trace: Trace::default(),
            cost: Cost::default(),
            frames: vec![],
            loops: vec![],
            next_exit: 0,
            depth: 0,
            深度峰: 0,
            run_seq: 0,
            model_id,
            pending: vec![],
            生成: Default::default(),
            调用费表: Default::default(),
            layers: vec![],
            plan: jpp_ir::plan::Plan::empty(),
            hooks: &Unplanned,
            guards: vec![],
            speculated: HashSet::new(),
            speculation_used: HashSet::new(),
            站点表: HashMap::new(),
            键计数: HashMap::new(),
            键计数已记: HashSet::new(),
            挑选记录: vec![],
            并档记录: vec![],
            超窗计数: [0; 3],
            点名无取法: Default::default(),
            末次缺: vec![],
            末端取最大项: false,
            末次最大项: None,
            末次最大项键: None,
            末次题树败因: None,
            题树深: 0,
            题树: None,
            首停延后: None,
            当前组位: 0,
            asks_in_ledger: 0,
            computed_untrusted_states: std::cell::RefCell::new(HashSet::new()),
            input_untrusted_states: std::cell::RefCell::new(HashMap::new()),
            本趟已记校准: HashSet::new(),
            去向计数: HashMap::new(),
            未决来源: None,
            判断来历: HashMap::new(),
            切法来历: HashMap::new(),
            默认链记录: vec![],
            默认链站点: BTreeSet::new(),
            读数表: HashMap::new(),
            链中: 0,
            本趟去向: vec![],
            伴随题式: None,
            伴随: vec![],
            伴随键: HashSet::new(),
            伴随中: false,
            裂变块中: 0,
            顶层环境: None,
            序言: None,
            序言伴随: None,
            序言类别: None,
            元题: HashSet::new(),
            元题登记: 0,
            料库: None,
            宿主取材料: None,
            本趟已记声明: HashSet::new(),
            账本错: None,
            复用: Default::default(),
            出口放行表: HashMap::new(),
            谱系断: std::cell::RefCell::new(HashMap::new()),
            fn1_of: HashMap::new(),
            dropped: vec![],
            解除: HashMap::new(),
            谱系缺键已报: HashSet::new(),
            drift_reported: HashSet::new(),
            unknown_reported: HashSet::new(),
            键读数: HashMap::new(),
            声明出口: HashMap::new(),
            exit_grades: vec![],
            exit_rows: HashMap::new(),
            reading_kinds: HashMap::new(),
            questions: vec![],
            裂变表: HashMap::new(),
            裂变块键: HashSet::new(),
            窗口未测读数: HashSet::new(),
            单元: Some(cells::单元图::new()),
            evidence: vec![],
            absent_marks: HashMap::new(),
            读数缺席: HashMap::new(),
            重放趟: std::cell::OnceCell::new(),
            真登记次数: HashMap::new(),
            真登记序: HashMap::new(),
            待发真序: HashMap::new(),
            重放缺席键: HashMap::new(),
            consecutive_absent: 0,
            judge_keys: HashMap::new(),
            render: RENDER_VERSION.to_string(),
            key_version: jpp_ledger::key_version::KEY_VERSION_CURRENT,
            站点键: Default::default(),
            站点回退: 0,
            effect_keys: HashMap::new(),
            latency_spent: 0.0,
            上游: None,
            深度上游: None,
            本趟起答: 0,
            重新授权: false,
            无余额结清: None,
            声明深度: None,
            深度默认: DEFAULT_DEPTH,
            深度停: false,
            本趟轮: 0,
            未问表: std::cell::OnceCell::new(),
            停发说明: HashMap::new(),
            记号表: HashMap::new(),
            丢失处: HashMap::new(),
            违规: vec![],
            结算失败: vec![],
            推迟: vec![],
            当前桥: jpp_ledger::Via::Cut,
            停下已写: false,
            未问已记: HashSet::new(),
            c_max: None,
            预算停: None,
            出口预定: None,
            停发键: HashSet::new(),
            audit: ReplayAudit::default(),
            entry: EntryArgs::default(),
            entry_mat_names: HashMap::new(),
            guard: false,
            gate: None,
            gated: HashSet::new(),
            transforms: None,
        }
    }
}

// 自由函数在 `support.rs`（C2a 拆出）；对外的常量与函数在这里按原路径导出
mod support;
use jpp_value::bridge::argmax;
use support::*;
pub use support::{FORM_FIELDS, OUTCOME_FIELDS, QUESTION_FIELDS};
pub use support::{effect_value_to_entry, entry_to_effect_value, is_outcome, json_to_value};
