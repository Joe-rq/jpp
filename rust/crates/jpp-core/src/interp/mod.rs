//! 解释器：逐语句即时执行；效应即时发出（惰性融合是后续优化，未迁移）。
//! 纪律由这里与检查器把关：J-01 读数不进槽、J-02 禁自指、J-03 线来自校准记录、J-05 unsure 必消费、
//! J-06 有界循环 + 键重复即停、J-07 预算超即停（Pending）、J-12 Fail 是值、J-13 序号、J-18 账本头。

use std::collections::{BTreeSet, HashMap, HashSet};
use std::rc::Rc;

use serde_json::{Value as Json, json};

// 运行时读 IR（步 12c；步 12d 删除核心语法树后只剩 IR）
use crate::effects::{Client, FitRegistry};
use crate::ledger::{
    CalibRef, EffectKey, Entry, Header, HeaderCompare, JudgeKey, Ledger, RENDER_VERSION, Trace,
};
use crate::value::*;
use jpp_effects::view::{self, Callee, K, kind};
use jpp_effects::views::{CalibView, Lookup};
use jpp_ir::ir::{Block, Budget, Expr, Function, Program, Span, Stmt};
/// pass 开关（步 13a 搬到 `jpp-plan`；原路径保留，测试与宿主不改）
pub use jpp_plan::Passes;

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
pub struct Action {
    pub name: String,
    pub cost: f64,
    pub reversible: bool,
    pub taint_out: TaintOut,
    pub f: Rc<dyn Fn(&[Value]) -> Result<Value, String>>,
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
            }),
        );
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

/// 出口作为守卫：J-08:265 的两个析取项各自独立取值。
///
/// **`asked` 不从 taint 推**——`trusted`（状态可信）与 `asked`（经人拍板）
/// 是那条规则亲口并列的两件事，人答是 trusted 走的是另一条路（§2.11）。
fn 守卫(e: &Rc<Exit>) -> GuardInfo {
    GuardInfo {
        trusted: e.guard_trusted(),
        asked: e.from_ask.get(),
    }
}

/// 这条线**凭什么**：`手填` 还是某一张证书。与「哪一层」正交。
fn 凭据(rec: &Lookup) -> String {
    match &rec.selected {
        // **第三轴：这个数是怎么算出来的。**「哪一层」「凭什么」「怎么算的」是三件事——
        // 把代价折进「凭什么」那一格，就是把一小时前刚红过的那次合并再做一遍。
        // **一条由代价矩阵算出来的线，和一条人拍脑袋写的线，不是一回事。**
        Some(c) => match c.cost {
            // B72：试用证书在凭据里说出来（handler 经 `line_source` 看得见）
            Some((fp, fn_)) => format!("{}证书:α={:.2}·代价(fp={fp},fn={fn_})", 试用前缀(c), c.alpha),
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
    /// 每次刷新发出的一层（12 §2.2 的分层结果）；层数是 lift / fuse 的量具
    pub layers: Vec<Layer>,
    /// **这一趟跑出来的证据**（`12`:347 的「运行期写入口」）：`(校准键, 观察)`。
    ///
    /// **它是缓冲区，不是写库。** 程序发出证据，宿主决定折不折进 `CalibStore`——
    /// 这样 I4「程序里不可写线」在字面上仍然成立：程序连库的可变引用都拿不到。
    /// **重放不进这里**：重放读的是既有事实，不是新观察。
    pub evidence: Vec<(String, crate::effects::Sample)>,
    /// **停岗候选**（B25）：本趟自动标记的校准键（漂移信号超线）。CLI 的 `--calib-out`
    /// 把它们写成「停岗候选」；正式停岗由人确认（`jpp calib-confirm`）。
    pub suspend_candidates: Vec<String>,
    /// **逐 `cut` 出口的线等级**（步 20f，总账待补「逐出口记线等级」）：每条
    /// `{site, exit, grade, releases, key?, scope_out?, suspend_candidate?}`，`grade` 用 `20` §3.4 的
    /// `LineGrade` 变体名（`Certified` `Form` `Trial` `Provisional` `Class` `Fixture` `Cold`）。
    /// 出口不进账本（`20` §3.7(1)）：等级随校准记录的证书进账本头 `calib_used`，只凭账本重放时重算出同一张表。
    pub exits: Vec<Json>,
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
    /// 返回类型提到 Exit：未消费的 Unsure 由调用者接手
    returns_exit: bool,
}

struct LoopCtx {
    seen_keys: HashSet<String>,
    repeated: Option<String>,
}

pub struct Interp<'a> {
    client: &'a mut dyn Client,
    ledger: &'a mut Ledger,
    /// 校准只经只读视图读（步 11b，`20` §2.3：运行时不依赖 `CalibStore` 具体类型）
    calib: &'a dyn CalibView,
    /// 私有读数表：读数是句柄，答案只在这里（步 11b-3）。写只经 `flush.rs::fill_answer`，
    /// 读只经 `readings.rs::answer_of`。
    answers: std::cell::RefCell<AnswerTable>,
    next_reading: std::cell::Cell<u64>,
    actions: &'a ActionRegistry,
    fits: &'a FitRegistry,
    budget: Budget,
    pub trace: Trace,
    pub cost: Cost,
    frames: Vec<Frame>,
    loops: Vec<LoopCtx>,
    next_exit: usize,
    depth: u32,
    run_seq: u64,
    model_id: String,
    /// 已登记但还没发出的判断（`12` §2.2「登记后不发」）
    pending: Vec<PendingJudge>,
    /// **审计重放**（B35；21 步 3）：只凭账本重现首跑。账本里记过的调用照记录计入预算，
    /// 使首跑在哪里预算停机，重放就在哪里停；账本缺的记录报 `E-replay`（致命，不进 cause）。
    /// 续跑（`--resume`）不开：已记录的不付费、继续往下。依据：12 §2.3 B35 注；21 §三·2 步 3。
    audit: ReplayAudit,
    /// 每次刷新发出的一层，供测试与 Trace 看分层结果
    pub layers: Vec<Layer>,
    /// 编译 pass 开关（12 §4）。步 13a 起定义在 `jpp-plan`；`run` 入口据它算出 [`Interp::plan`]
    pub passes: Passes,
    /// 本次运行的计划（`jpp-plan` 写，这里只读；`run` 入口由 `passes` 算出，步 13a）
    plan: jpp_ir::plan::Plan,
    /// 规划的运行期钩子（`20` §2.3 `Ports.hooks`；步 14a 前默认 `jpp_plan::Hooks`）
    hooks: Box<dyn jpp_ir::plan::PlanHooks>,
    /// 当前所处的条件链：每层是「这个条件里有没有一个 **trusted 来源的合取项**」。
    ///
    /// J-08（`12`:265）说的「放行不可逆 `do` 的**守卫表达式**」，在一个有 `if` 的语言里
    /// 就是**包着这个 `do` 的那些条件**——不必是 `do` 的一个参数。条件求值成 `Bool` 之后
    /// taint 就没了，所以在**求值条件的那一刻**记下来。
    guards: Vec<GuardInfo>,
    /// 推测登记过的账本键：程序真走到时会命中账本，没走到的留 `W-spec-unused`
    speculated: HashSet<String>,
    /// 推测的键里，真被程序用上的
    speculation_used: HashSet<String>,
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
    /// 刚结束的那次求值（含 helper 调用）里出口的来源，供 `let` 绑定收走
    /// 刚结束的那次求值（含 helper 调用）里出口的来源。**只在一条 `let` 的求值期间有效**：
    /// 求值前清空、求值后立刻被那条 `let` 取走——不跨语句、不跨帧留存。
    last_eval_provenance: Option<(bool, bool)>,
    /// 刚求值的那个记录字面量里，**逐字段**的来源。与 `last_eval_provenance` 一样只活一条 `let`。
    pending_field_prov: Option<Vec<(String, (bool, bool))>>,
    /// 账本里已有的 `ask` 条数：只参与核 `budget.escalate` 总上限，**不进 `Cost.asks`**
    /// （那个报的是「这次运行实际问了几次人」，重放时该是 0）。
    asks_in_ledger: u64,
    /// **缺席 / 超时标记**（B32）：账本键 → `absent` / `latency`。`cut` 据此给 `Unsure(原因)`。
    absent_marks: HashMap<String, String>,
    /// 连续缺席次数（熔断用）
    consecutive_absent: u32,
    /// 本趟算出的判断键：账本键 → 结构化键（账本 v2 条目记结构化键，步 7）
    judge_keys: HashMap<String, JudgeKey>,
    /// 本趟算出的效应键：账本键 → 结构化键（步 7）
    effect_keys: HashMap<String, EffectKey>,
    /// 本趟判断调用累计耗时（秒，B32 时延预算）
    latency_spent: f64,
    /// 这一轮里被 `content()` 从 **untrusted 材料**拆出来的内容（规范 JSON）。
    /// `mat()` 拿到其中之一时不能当字面量洗成 trusted——见 `as_mat` 的兜底臂。
    /// 从 untrusted 来源拆出的**文本叶子**（K-182 / K-203，2026-09-23）：派生出的新字符串
    /// （拼接、join、slice、text）按子串关系认回来，不再只做整值精确匹配。
    /// 含有「成分不可信的计算值」材料的状态哈希（J-08 诊断用，B33 第 8 点）
    computed_untrusted_states: std::cell::RefCell<HashSet<String>>,
    /// 这次运行已经报过漂移的键：**一条天天响的告警等于没有告警**
    drift_reported: HashSet<String>,
    evidence: Vec<(String, crate::effects::Sample)>,
    /// 逐 `cut` 出口的线等级（步 20f，报告 `exits`；出口不进账本，重放时重算）
    exit_grades: Vec<Json>,
    /// 读数的精化题类（B76，步 12e-2；`21` 写作 `ReadingMeta.kind`）：按读数句柄记，登记读数时算。
    /// 放在这里而不放 `Reading` 上，是为了不改 `Reading` 的字面量构造（宿主与测试各处）。
    reading_kinds: HashMap<u64, QuestionKind>,
}

/// 一次 `judge` 登记：一个状态 + 它那几道题
struct PendingJudge {
    state: Rc<State>,
    items: Vec<(Rc<Question>, Rc<Reading>, String)>,
    site: Span,
    /// 推测登记的（不是程序真走到的站点）。超预算时**先丢它**，再动真站点。
    speculative: bool,
}

/// 一层条件（`if` 的条件）里，J-08 关心的那两件事
#[derive(Clone, Copy, Debug, Default)]
struct GuardInfo {
    /// 有没有一个合取项来自 **trusted 状态的判断**（`12`:265「至少一个」）
    trusted: bool,
    /// 有没有经 `ask`（`12`:265「或经 `ask`」；人答是 trusted，§2.11）
    asked: bool,
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
    "line_source",
    "taint",
    "escalate",
    "literalize",
    "allocate",
    "unsure_bound",
    "agg",
    "repeat",
    "order",
    "fit",
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
];

pub fn root_env() -> Env {
    let env = env_root();
    for b in BUILTINS {
        env_define(&env, b, Value::Builtin(b));
    }
    env
}

/// 空的 fit 注册表：不用 fit 的程序共用这一个。
///
/// `Box::leak` 而不是 `unsafe` 的裸指针——两者都是「建一次不释放」，但前者是安全代码，
/// 而这里没有任何需要 `unsafe` 的理由。`FitRegistry` 含 `Rc` 不是 `Sync`，所以用
/// 线程局部：每个线程各一份空表，各自 leak 一次。
fn empty_fits() -> &'static FitRegistry {
    thread_local! {
        static EMPTY: &'static FitRegistry = Box::leak(Box::new(FitRegistry::new()));
    }
    EMPTY.with(|f| *f)
}

mod bridge;
mod budget;
mod constructs;
mod duty;
mod effects_exec;
mod eval;
mod flush;
mod guard;
mod host_builtins;
mod outcome;
mod plan_view;
mod readings;
mod register;
mod schedule;

use readings::refresh_point;

impl<'a> Interp<'a> {
    /// 不带 fit 注册表的入口（绝大多数程序不用 fit）。要用 fit 走 `with_fits`。
    pub fn new(
        client: &'a mut dyn Client,
        ledger: &'a mut Ledger,
        calib: &'a dyn CalibView,
        actions: &'a ActionRegistry,
        budget: Budget,
    ) -> Interp<'a> {
        Interp::with_fits(client, ledger, calib, actions, empty_fits(), budget)
    }

    pub fn with_fits(
        client: &'a mut dyn Client,
        ledger: &'a mut Ledger,
        calib: &'a dyn CalibView,
        actions: &'a ActionRegistry,
        fits: &'a FitRegistry,
        budget: Budget,
    ) -> Interp<'a> {
        let model_id = client.model_id();
        Interp {
            client,
            ledger,
            calib,
            answers: Default::default(),
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
            run_seq: 0,
            model_id,
            pending: vec![],
            layers: vec![],
            passes: Passes::default(),
            plan: jpp_ir::plan::Plan::empty(),
            hooks: Box::new(jpp_plan::Hooks),
            guards: vec![],
            speculated: HashSet::new(),
            speculation_used: HashSet::new(),
            last_eval_provenance: None,
            pending_field_prov: None,
            asks_in_ledger: 0,
            computed_untrusted_states: std::cell::RefCell::new(HashSet::new()),
            drift_reported: HashSet::new(),
            exit_grades: vec![],
            reading_kinds: HashMap::new(),
            evidence: vec![],
            absent_marks: HashMap::new(),
            consecutive_absent: 0,
            judge_keys: HashMap::new(),
            effect_keys: HashMap::new(),
            latency_spent: 0.0,
            audit: ReplayAudit::default(),
        }
    }
}

/// 函数体里引用到的名字（含嵌套 lambda 与参数名）。**宁可多收**——多收只是少复用一点缓存，
/// 少收会把别人的结果当成自己的。语言形式与效应节点的名字也收（与步 12c 前按源码树收集同口径）。
fn referenced_names(f: &Function) -> BTreeSet<String> {
    fn go_block(b: &Block, out: &mut BTreeSet<String>) {
        for s in &b.statements {
            match s {
                Stmt::Let { value, .. } => go(value, out),
                Stmt::Function { function, .. } => go_block(&function.body, out),
                Stmt::Expr(e) => go(e, out),
            }
        }
        if let Some(r) = &b.result {
            go(r, out);
        }
    }
    fn go(e: &Expr, out: &mut BTreeSet<String>) {
        match kind(e) {
            K::Name(n) => {
                out.insert(n.to_string());
            }
            K::List(items) => items.iter().for_each(|x| go(x, out)),
            K::Record(fields) => fields.iter().for_each(|(_, x)| go(x, out)),
            K::Function(inner) => go_block(&inner.body, out),
            K::Call { callee, args } => {
                match callee {
                    Callee::Name(n) => {
                        out.insert(n.to_string());
                    }
                    Callee::Expr(c) => go(c, out),
                }
                args.iter().for_each(|x| go(x, out));
            }
            K::Field { value, .. } => go(value, out),
            K::Index { value, index } => {
                go(value, out);
                go(index, out);
            }
            K::Unary { value, .. } => go(value, out),
            K::Binary { left, right, .. } => {
                go(left, out);
                go(right, out);
            }
            K::If { condition, yes, no } => {
                go(condition, out);
                go_block(yes, out);
                go_block(no, out);
            }
            K::Block(b) => go_block(b, out),
            _ => {}
        }
    }
    let mut out = BTreeSet::new();
    go_block(&f.body, &mut out);
    out
}

/// 读数用来排序的那个值；`None` = 失败或没答（J-12，排最后）
fn rank_value(ans: &dyn Answers, r: &Reading) -> Option<f64> {
    if r.fail.is_some() {
        return None;
    }
    match ans.answer_of(r)? {
        Answer::Noul(p) => Some(p),
        Answer::Choice(v) | Answer::Score(v) => Some(argmax(&v).1),
    }
}

/// 同题跨运行合并：noul / score 取均值，choice 取众数（`12`:134）
fn merge_runs(ans: &dyn Answers, rs: &[Rc<Reading>], method: &str, sp: Span) -> R<Answer> {
    let answers: Vec<Answer> = rs.iter().filter_map(|r| ans.answer_of(r)).collect();
    if answers.is_empty() {
        return err(
            Some("J-12"),
            "repeat 收到的读数全是失败或未答，没有可合并的",
            sp,
        );
    }
    // 逐分量取均值或中位数（B28）。choice / score 都是概率向量，逐分量合并，不投票。
    let 合 = |xs: &mut Vec<f64>| -> f64 {
        if xs.is_empty() {
            return 0.0;
        }
        if method == "median" {
            xs.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
            let m = xs.len() / 2;
            if xs.len() % 2 == 1 {
                xs[m]
            } else {
                (xs[m - 1] + xs[m]) / 2.0
            }
        } else {
            xs.iter().sum::<f64>() / xs.len() as f64
        }
    };
    let 向量 = |pick: &dyn Fn(&Answer) -> Option<Vec<f64>>, len: usize| -> Vec<f64> {
        (0..len)
            .map(|i| {
                let mut xs: Vec<f64> = answers
                    .iter()
                    .filter_map(|a| pick(a).and_then(|v| v.get(i).copied()))
                    .collect();
                合(&mut xs)
            })
            .collect()
    };
    Ok(match &answers[0] {
        Answer::Noul(_) => {
            let mut ps: Vec<f64> = answers
                .iter()
                .filter_map(|a| {
                    if let Answer::Noul(p) = a {
                        Some(*p)
                    } else {
                        None
                    }
                })
                .collect();
            Answer::Noul(合(&mut ps))
        }
        Answer::Score(v0) => Answer::Score(向量(
            &|a| {
                if let Answer::Score(v) = a {
                    Some(v.clone())
                } else {
                    None
                }
            },
            v0.len(),
        )),
        Answer::Choice(v0) => Answer::Choice(向量(
            &|a| {
                if let Answer::Choice(v) = a {
                    Some(v.clone())
                } else {
                    None
                }
            },
            v0.len(),
        )),
    })
}

/// 这道题声明了、而状态里空着的证据槽（J-09）。与 Python `runtime.py:1133` 同口径：
/// 只查「槽不存在或为空」，不查内容。
fn missing_evidence(state: &State, q: &Question) -> Vec<String> {
    q.evidence
        .iter()
        .filter(|slot| {
            let v = match slot.as_str() {
                "on" => &state.on,
                "ctx" => &state.ctx,
                "ref" => &state.r#ref,
                "over" => &state.over,
                _ => return false,
            };
            v.is_empty()
        })
        .cloned()
        .collect()
}

/// `test`/`select`/`measure` 的可选第三参：`{evidence: [槽名…]}`（J-09）
fn evidence_of(v: Option<&Value>, sp: Span) -> R<Vec<String>> {
    let Some(v) = v else { return Ok(vec![]) };
    let Value::Record(fields) = v else {
        return err(
            Some("E-rt-question"),
            "题的第三个参数要是记录：{evidence: [\"ctx\", …]}",
            sp,
        );
    };
    let Some((_, slots)) = fields.iter().find(|(k, _)| k == "evidence") else {
        return Ok(vec![]);
    };
    let Value::List(l) = slots else {
        return err(Some("E-rt-question"), "evidence 要是槽名的列表", sp);
    };
    let mut out = vec![];
    for s in l.iter() {
        let Value::Text(t, _) = s else {
            return err(Some("E-rt-question"), "evidence 里要是槽名（文本）", sp);
        };
        if !matches!(t.as_ref(), "on" | "ctx" | "ref" | "over") {
            return err(
                Some("E-rt-question"),
                format!("evidence 里的 {t} 不是槽名；状态只有 on / ctx / ref / over 四个槽"),
                sp,
            );
        }
        out.push(t.to_string());
    }
    Ok(out)
}

/// 题上可读的字段（只读）。静态检查（check.rs）用同一张表核字段名。
pub const QUESTION_FIELDS: &[&str] = &[
    "text",
    "op",
    "calib",
    "scale",
    "evidence",
    "hash",
    "subject",
    "predicate",
    "partition",
    "request",
    "presupposition",
    "form",
    "template",
    "fill",
];
/// 题式上可读的字段（只读）。
pub const FORM_FIELDS: &[&str] = &[
    "template",
    "op",
    "slots",
    "calib",
    "scale",
    "evidence",
    "presupposition",
    "request",
    "partition",
    "subject",
    "hash",
];

/// 组合封闭性契约（B17，施工件 i）的字段。每个构造（`sieve` / `pair` / `tally` / `first_k` /
/// `iterate` / `outcome`）返回同一形状的记录，检查器据此核字段名。
/// - `kind`：产生它的构造；
/// - `value`：产出；
/// - `pending`：未决清单，每项 `{element, exit, cause}`，`exit` 承担责任（J-05 / 13 §3）；
/// - `evidence`：账本键（Text），不存读数或材料的副本；
/// - `resume`：续接——停在哪里、为什么、可选的继续方法 `next`；
/// - `spent`：本构造新增的调用与费用 `{calls, usd}`；
/// - `detail`：构造特有的已决信息（例如 sieve 的 `question`、`ignore`）；
/// - `purpose`：可选的可读目的，供诊断。
pub const OUTCOME_FIELDS: &[&str] = &[
    "kind", "value", "pending", "evidence", "resume", "spent", "detail", "purpose",
];

/// 这个值是不是一个契约值（字段集合与 `OUTCOME_FIELDS` 一致）
pub fn is_outcome(v: &Value) -> bool {
    match v {
        Value::Record(r) => {
            r.len() == OUTCOME_FIELDS.len()
                && OUTCOME_FIELDS.iter().all(|f| r.iter().any(|(k, _)| k == f))
        }
        _ => false,
    }
}

/// 证据列表去重追加（证据都是账本键 Text）
fn push_key(v: &mut Vec<Value>, k: Value) {
    let same = |a: &Value| matches!((a, &k), (Value::Text(x, _), Value::Text(y, _)) if x == y);
    if !v.iter().any(same) {
        v.push(k);
    }
}

fn list_of(v: Option<Value>) -> Vec<Value> {
    match v {
        Some(Value::List(l)) => l.iter().cloned().collect(),
        _ => vec![],
    }
}

fn texts(v: &[String]) -> Value {
    Value::list(v.iter().map(|x| Value::text(x)).collect())
}
fn opt_text(v: &Option<String>) -> Value {
    v.as_deref().map(Value::text).unwrap_or(Value::Unit)
}

/// B1 五件与题的元数据。`predicate` 就是题面：主体（被判断的对象）在状态里，不在题面里，
/// 题面说的是对它判断什么。由题式填出的题另有 `template`（带槽的谓词）与 `fill`（填法）。
fn question_field(q: &Question, field: &str) -> Option<Value> {
    Some(match field {
        "text" | "predicate" => Value::text(&q.text),
        "op" => Value::text(q.op.fixture_name()),
        "calib" => Value::text(&q.calib),
        "scale" => texts(&q.scale),
        "evidence" => texts(&q.evidence),
        "hash" => Value::text(&q.hash),
        "subject" => Value::text(q.subject()),
        "partition" => Value::text(q.partition()),
        "request" => Value::text(&q.request()),
        "presupposition" => opt_text(&q.presupposition),
        "form" => opt_text(&q.form_hash),
        "template" => opt_text(&q.template),
        "fill" => match &q.fill {
            Some(f) => Value::Record(Rc::new(
                f.iter().map(|(k, v)| (k.clone(), Value::text(v))).collect(),
            )),
            None => Value::Unit,
        },
        _ => return None,
    })
}

fn form_field(f: &crate::value::Form, field: &str) -> Option<Value> {
    Some(match field {
        "template" => Value::text(&f.template),
        "op" => Value::text(f.op.fixture_name()),
        "slots" => texts(&f.slots),
        "calib" => Value::text(&f.calib),
        "scale" => texts(&f.scale),
        "evidence" => texts(&f.evidence),
        "presupposition" => opt_text(&f.presupposition),
        "request" => Value::text(
            f.request
                .as_deref()
                .unwrap_or(crate::value::default_request(f.op)),
        ),
        "partition" => Value::text(match f.op {
            Op::Test => "binary",
            Op::Select => "k_ary",
            Op::Measure => "ordered",
        }),
        "subject" => Value::text(match f.op {
            Op::Select => "over",
            _ => "on",
        }),
        "hash" => Value::text(&f.hash),
        _ => return None,
    })
}

/// 题的声明项：前提（可选文本）与请求（本版只接受各题型的缺省请求，见下）。
fn question_decl_of(v: Option<&Value>, op: Op, sp: Span) -> R<(Option<String>, Option<String>)> {
    let Some(v) = v else { return Ok((None, None)) };
    let presupposition = match v.get("presupposition") {
        None | Some(Value::Unit) => None,
        Some(Value::Text(t, _)) => Some(t.to_string()),
        Some(other) => {
            return err(
                Some("E-rt-question"),
                format!("presupposition 要是文本，收到 {}", other.type_name()),
                sp,
            );
        }
    };
    let request = match v.get("request") {
        None | Some(Value::Unit) => None,
        Some(Value::Text(t, _)) => {
            // 本版 `cut` 只实现每个题型的缺省请求。「K 选一、选出全部」（all）要由三路过滤
            // 与子集判断承担（施工件 c），在那之前声明它只会被静默当成 one——所以拒绝，而不是收下不管。
            if t.as_ref() != crate::value::default_request(op) {
                let hint = if op == Op::Select && t.as_ref() == "all" {
                    "；「选出全部」待三路过滤（施工件 c）实现后可用，现在用 map + test 逐个判"
                } else {
                    ""
                };
                return err(
                    Some("E-rt-question"),
                    format!(
                        "request 「{t}」不适用于 {} 题：本版只支持缺省请求 {}{hint}",
                        op.fixture_name(),
                        crate::value::default_request(op)
                    ),
                    sp,
                );
            }
            Some(t.to_string())
        }
        Some(other) => {
            return err(
                Some("E-rt-question"),
                format!("request 要是文本，收到 {}", other.type_name()),
                sp,
            );
        }
    };
    Ok((presupposition, request))
}

/// 一组实参里各材料的来源出口键（`Mat.from_key`）的并（B59，步 17a；容器递归看，与 `derived_of` 同）。
/// 步 17c（B84）起取值级标签的 sources：标量叶子、材料、出口、题都算。
fn from_keys_of(args: &[Value]) -> BTreeSet<String> {
    args.iter()
        .fold(Provenance::trusted(), |p, a| prov_join(&p, &a.prov()))
        .sources
        .to_set()
}

/// 出口交给 `pick`/`at` 臂的标签（B84）：出口 taint，sources = {出口键}。
fn 出口标签(e: &Exit) -> Provenance {
    Provenance::new(e.taint, Sources::from_key(&e.ledger_key.borrow()))
}

/// 元素记录的直接来源（B59，步 17a，结构通道）：`sieve` 元素有 `exit` 且账本键非空 → 该键；
/// `pair` 元素（无 `exit`、有 `left`/`right`）→ 两侧来源的并；其余为空。
/// 只取直接来源，更早的祖先经它们自己的 `hop` 计入。经普通值（`e.item` 取出）的依赖不在此列（候选 B84）。
fn element_lineage(it: &Value) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    if !matches!(it, Value::Record(_)) {
        return out;
    }
    match it.get("exit") {
        Some(Value::Exit(e)) | Some(Value::Duty(e)) => {
            let k = e.ledger_key.borrow().clone();
            if !k.is_empty() {
                out.insert(k);
            }
        }
        _ => {
            for side in ["left", "right"] {
                if let Some(v) = it.get(side) {
                    out.extend(element_lineage(&v));
                }
            }
        }
    }
    out
}

/// 一组实参里各材料的 `derived_from` 的并（容器要递归看，与 `taint_of` 同）
fn derived_of(args: &[Value]) -> BTreeSet<String> {
    fn go(v: &Value, out: &mut BTreeSet<String>) {
        match v {
            Value::Mat(m) => out.extend(m.derived_from.iter().cloned()),
            Value::List(l) => l.iter().for_each(|x| go(x, out)),
            Value::Record(fs) => fs.iter().for_each(|(_, x)| go(x, out)),
            Value::Stop(x) => go(x, out),
            _ => {}
        }
    }
    let mut out = BTreeSet::new();
    args.iter().for_each(|a| go(a, &mut out));
    out
}

/// 某个绑定的**某个字段**的来源在环境里的键名
fn field_prov_key(name: &str, field: &str) -> String {
    format!("{name}\u{1f}{field}\u{1f}prov")
}

/// 来源在环境里的键名。用 `\u{1f}` 分隔，源码里写不出这个字符，所以不会与用户的名字撞。
fn prov_key(name: &str) -> String {
    format!("{name}\u{1f}prov")
}

/// 不在分派处做「输出 ∨ 输入」的内置（B33 第 3 点）。两类：
/// 1. **效应边界与自带规则**：taint 按 `12` §2.11 表在这里赋值（`state`/`mat`/`content`/`judge`/
///    `cut`/`do`/`gen`/`ask`/`transform`…），或输出本身就带着该有的位（出口、材料、契约值）；
/// 2. **只搬运元素**：输出的元素就是输入的元素（或用户函数的返回值），各带自身的位；
///    整体 ∨ 会把一个不可信元素的位抹到所有元素上（取字段 / 下标返回叶子自身的位，同一原则）。
/// 不在表上的内置（含将来新增的）一律按 ∨ 输入处理——**兜底往拒绝那边倒**。
const 不做数据流合取的内置: &[&str] = &[
    // 效应边界与自带规则
    "state",
    "test",
    "select",
    "measure",
    "form",
    "fill",
    "judge",
    "cut",
    "handle",
    "consume",
    "do",
    "gen",
    "ask",
    "transform",
    "mat",
    "content",
    "sieve",
    "pair",
    "tally",
    "first_k",
    "iterate",
    "outcome",
    "repeat",
    "agg",
    "allocate",
    "unsure_bound",
    "fit",
    "order",
    "escalate",
    "literalize",
    "unsure",
    "pending",
    "print",
    "stop",
    "fail",
    // 只搬运元素
    "map",
    "filter",
    "fold",
    "loop",
    "append",
    "concat",
    "slice",
    "reverse",
    "with",
];

/// 一个值携带的 taint（B33：标量自带位，容器递归 ∨）
fn taint_of(v: &Value) -> Taint {
    v.taint()
}

/// 13 §6 的运行错误：说清是哪一步越界、越的是哪个界，并带 `.jpp` 的 Span
fn overflow(what: &str, a: i64, b: i64, sp: Span) -> Fault {
    Fault::Error(RtError::new(
        Some("E-rt-int"),
        format!(
            "Int {what}溢出：{a} 与 {b} 的结果超出有符号 64 位范围（{} … {}）。Int 是 64 位有符号整数，溢出是错误不是回绕",
            i64::MIN,
            i64::MAX
        ),
        sp,
    ))
}

use jpp_value::bridge::argmax;

pub fn json_to_value(j: &Json) -> Value {
    match j {
        Json::Null => Value::Unit,
        Json::Bool(b) => Value::Bool(*b, Taint::Trusted.into()),
        Json::Number(n) => n
            .as_i64()
            .map(Value::int)
            .unwrap_or_else(|| Value::Float(n.as_f64().unwrap_or(0.0), Taint::Trusted.into())),
        Json::String(s) => Value::text(s),
        Json::Array(a) => Value::list(a.iter().map(json_to_value).collect()),
        Json::Object(o) => Value::record(
            o.iter()
                .map(|(k, v)| (k.clone(), json_to_value(v)))
                .collect(),
        ),
    }
}

pub fn effect_value_to_json(v: &Value) -> Json {
    match v {
        // 账本编码只写 taint 分量，逐字节不变（sources 在步 18 随 `output_mat` 持久，B84）
        Value::Fail(s, t) => json!({"__fail": s.as_ref(), "taint": t.taint}),
        // `derived_from` 也要写：不写的话重放出来的程序与原程序**在 J-02 上不是同一个程序**，
        // 而 J-18 的整套重放判定建立在它们是同一个上。
        Value::Mat(m) => {
            json!({"__mat": m.content, "taint": m.taint, "addr": m.addr, "origin": m.origin, "derived_from": m.derived_from})
        }
        other => other.to_json(),
    }
}

pub fn json_to_effect_value(j: &Json) -> Value {
    if let Some(f) = j.get("__fail").and_then(|x| x.as_str()) {
        // 旧账本没有 taint 位：兜底往拒绝那边倒（untrusted），与材料的反序列化同一纪律
        let t = j
            .get("taint")
            .and_then(|x| serde_json::from_value::<Taint>(x.clone()).ok())
            .unwrap_or(Taint::Untrusted);
        return Value::Fail(Rc::from(f), t.into());
    }
    if let Some(c) = j.get("__mat") {
        // 兜底往**保守**那边倒。以前是 `.unwrap_or(Taint::Trusted)`：taint 字段坏了或缺了，
        // 一份 untrusted 材料经账本往返回来就变成 trusted——那不是漏报，是**洗白**，
        // 而 `12` §2.11 的 taint 代数整个建在这个字段上。
        // 与 J-01 那条是同一个形状的缝（`00-宪法.md` 第 30 行：三值被静默打成两值）。
        let taint: Taint = match j.get("taint") {
            Some(v) => serde_json::from_value(v.clone()).unwrap_or(Taint::Untrusted),
            None => Taint::Untrusted,
        };
        let addr = j.get("addr").and_then(|a| a.as_str()).unwrap_or("");
        let origin: Vec<String> =
            serde_json::from_value(j.get("origin").cloned().unwrap_or(json!([])))
                .unwrap_or_default();
        let derived: BTreeSet<String> =
            serde_json::from_value(j.get("derived_from").cloned().unwrap_or(json!([])))
                .unwrap_or_default();
        return Value::Mat(Rc::new(Mat::new(c.clone(), addr, origin, taint, derived)));
    }
    json_to_value(j)
}

/// 返回值里带着哪些出口 / 未决责任。注意它**不进函数捕获环境**——「把责任装进续接方法返回给
/// 调用者」这条合法路径因此表达不出来，见 INTERFACE.md 待定项。
fn collect_exit_ids(v: &Value, out: &mut HashSet<usize>) {
    match v {
        Value::Exit(e) | Value::Duty(e) => {
            out.insert(e.id);
        }
        Value::List(l) => l.iter().for_each(|x| collect_exit_ids(x, out)),
        Value::Record(r) => r.iter().for_each(|(_, x)| collect_exit_ids(x, out)),
        Value::Stop(x) => collect_exit_ids(x, out),
        // 方法的**捕获环境**里也可能装着责任。`13` §3 明列「随返回值/继续方法交给调用者」
        // 是合法去向，而类型侧早就用 `captures_responsibility` 认了方法能捕获责任——
        // 扫描侧不进环境，就成了内核两半打架：合法的续接方法被判成「责任丢了」。
        //
        // 只看这个方法体**实际引用到**的名字，不把共享环境链里所有可达名字都算成捕获
        // （Codex 陷阱 5 的后半句）；限深防环境链上的递归无限展开。
        Value::Fn(c) => {
            let names = referenced_names(&c.function);
            collect_captured_exits(&c.env, &names, 3, out);
        }
        _ => {}
    }
}

/// 从捕获环境里找责任。`depth` 防递归环境链无限展开。
fn collect_captured_exits(
    env: &Env,
    names: &BTreeSet<String>,
    depth: u32,
    out: &mut HashSet<usize>,
) {
    if depth == 0 {
        return;
    }
    for n in names {
        let Some(v) = env_lookup(env, n) else {
            continue;
        };
        match &v {
            Value::Fn(c) => {
                collect_captured_exits(&c.env, &referenced_names(&c.function), depth - 1, out)
            }
            other => collect_exit_ids(other, out),
        }
    }
}

/// 一个元素交给判断器的那份材料与来路：
/// 过滤或配对的产物（带 `item` 与 `trail` 的记录）取 `item` 当材料，`trail` 接上上一次的出口；
/// 其余值原样当材料、来路为空。产物与输入同形，可再过滤、再配对（组合封闭）。
fn element_parts(it: &Value) -> (Value, Value) {
    let is_elem =
        matches!(it, Value::Record(_)) && it.get("item").is_some() && it.get("trail").is_some();
    if !is_elem {
        return (it.clone(), Value::list(vec![]));
    }
    let mut t: Vec<Value> = match it.get("trail") {
        Some(Value::List(l)) => l.iter().cloned().collect(),
        _ => vec![],
    };
    if let Some(e) = it.get("exit") {
        if !matches!(e, Value::Unit) {
            t.push(e);
        }
    }
    (it.get("item").unwrap_or(Value::Unit), Value::list(t))
}
