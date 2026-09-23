//! 解释器：逐语句即时执行；效应即时发出（惰性融合是后续优化，未迁移）。
//! 纪律由这里与检查器把关：J-01 读数不进槽、J-02 禁自指、J-03 线来自校准记录、J-05 unsure 必消费、
//! J-06 有界循环 + 键重复即停、J-07 预算超即停（Pending）、J-12 Fail 是值、J-13 序号、J-18 账本头。

use std::collections::{BTreeSet, HashMap, HashSet};
use std::rc::Rc;

use serde_json::{Value as Json, json};

use crate::ast::*;
use crate::effects::{CalibStore, Client, FitRegistry};
use crate::ledger::{Entry, Header, Ledger, Trace, effect_key, judge_key, RENDER_VERSION};
use crate::value::*;

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
        RtError { rule: rule.map(|s| s.to_string()), message: msg.into(), span }
    }
    pub fn render(&self) -> String {
        match &self.rule {
            Some(r) => format!("[{r}] {} @{}..{}", self.message, self.span.start, self.span.end),
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
    pub fn register(&mut self, name: &str, cost: f64, reversible: bool, taint_out: TaintOut, f: impl Fn(&[Value]) -> Result<Value, String> + 'static) {
        self.actions.insert(name.to_string(), Rc::new(Action { name: name.to_string(), cost, reversible, taint_out, f: Rc::new(f) }));
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

/// 出口作为守卫：J-08:265 的两个析取项各自独立取值。
///
/// **`asked` 不从 taint 推**——`trusted`（状态可信）与 `asked`（经人拍板）
/// 是那条规则亲口并列的两件事，人答是 trusted 走的是另一条路（§2.11）。
fn 守卫(e: &Rc<Exit>) -> GuardInfo {
    GuardInfo { trusted: e.taint == Taint::Trusted, asked: e.from_ask.get() }
}

/// 这条线**凭什么**：`手填` 还是某一张证书。与「哪一层」正交。
fn 凭据(rec: &crate::effects::CalibRecord) -> String {
    match rec.选中的证书() {
        // **第三轴：这个数是怎么算出来的。**「哪一层」「凭什么」「怎么算的」是三件事——
        // 把代价折进「凭什么」那一格，就是把一小时前刚红过的那次合并再做一遍。
        // **一条由代价矩阵算出来的线，和一条人拍脑袋写的线，不是一回事。**
        Some(c) => match c.cost {
            Some((fp, fn_)) => format!("证书:α={:.2}·代价(fp={fp},fn={fn_})", c.alpha),
            None => format!("证书:α={:.2}", c.alpha),
        },
        None => "手填".to_string(),
    }
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
}

impl Outcome {
    pub fn value_json(&self) -> Json {
        self.value.as_ref().map(|v| v.to_json()).unwrap_or(Json::Null)
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
    calib: &'a CalibStore,
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
    /// 每次刷新发出的一层，供测试与 Trace 看分层结果
    pub layers: Vec<Layer>,
    /// 编译 pass 开关（12 §4）
    pub passes: Passes,
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
    /// 这一轮里被 `content()` 从 **untrusted 材料**拆出来的内容（规范 JSON）。
    /// `mat()` 拿到其中之一时不能当字面量洗成 trusted——见 `as_mat` 的兜底臂。
    unwrapped_untrusted: HashSet<String>,
    /// 这次运行已经报过漂移的键：**一条天天响的告警等于没有告警**
    drift_reported: HashSet<String>,
    evidence: Vec<(String, crate::effects::Sample)>,
}

/// 一次 `judge` 登记：一个状态 + 它那几道题
struct PendingJudge {
    state: Rc<State>,
    items: Vec<(Rc<Question>, Rc<Reading>, String)>,
    site: Span,
    /// 推测登记的（不是程序真走到的站点）。超预算时**先丢它**，再动真站点。
    speculative: bool,
}

/// 编译 pass 的开关（`12` §4「每个一个开关，给消融留门」）。
///
/// **是 9 个不是 7 个。** `12` §4 的表写了 7 行，但同文件 v0.1.1 修订记录 1（:610）写着
/// 「§4 **增两个 pass**」——judge 推测提升与循环向量化——而那张表从没改过。同一过期数字
/// 在三处独立写着（依据建造顺序、依据对照表、Python 模块 docstring）。这是**文档缺陷不是
/// 设计分歧**，所以这里按 9 个算，并在 INTERFACE.md 记下这处不一致（总控另走附注提请裁定）。
///
/// **开关是消融的唯一载体**：没有它，「关掉融合成本涨多少」这种账没法再核一次。
/// 七个 pass 里 core 现在落地两个——`fuse`（同状态同层合成一次调用）与 `ledger`
/// （账本键与重放）。其余五个（`lift` / `fission` / `lower` / `schedule` / `plan`）
/// **字段先留着并默认 false**，`enabled()` 对未落地的 pass 恒返回 false，
/// 这样「开关开着但什么也没发生」不会被误读成「这个 pass 在工作」。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Passes {
    /// 提升：直线段内不依赖前面结果的 judge 提到同一层（**已落地**，有消融账）
    pub lift: bool,
    /// 融合：同状态、同层的题合成一次调用（已落地）
    pub fuse: bool,
    /// 裂变：超窗的槽按窗切开（**未落地**）
    pub fission: bool,
    /// 下沉：select → choice / K-noul，measure → score（**未落地**）
    pub lower: bool,
    /// 调度：层内并发、gen/ask 一登记就发（**未落地**）
    pub schedule: bool,
    /// 预算：计划期估算 + 层边界核（**未落地**，缺 budget.layers）
    pub plan: bool,
    /// 键与重放：账本键、重放不付费（已落地）
    pub ledger: bool,
    /// judge 推测提升：同状态、静态可达、中间无 do/gen/ask/transform 的 judge 站点
    /// 随首个站点一起发（v0.1.1 修订记录 1，**已落地**；共状态零边际、异状态多花一次调用，
    /// 分情况实测见 INTERFACE）
    pub speculate: bool,
    /// 循环向量化：无 loop-carried 依赖、体内无 do/ask 的 for 体自动成一层
    /// （同上，证明不了要报 W-serial，**未落地**）
    pub vectorize: bool,
}

impl Default for Passes {
    /// 已落地的默认开，未落地的默认关——默认值不承诺未落地的东西在工作
    fn default() -> Passes {
        Passes { lift: true, fuse: true, fission: false, lower: false, schedule: false, plan: false, ledger: true, speculate: true, vectorize: true }
    }
}

impl Passes {
    /// 全关：消融的对照臂。`ledger` 也关得掉，关了就不查账本、每次都真发
    pub fn none() -> Passes {
        Passes { lift: false, fuse: false, fission: false, lower: false, schedule: false, plan: false, ledger: false, speculate: false, vectorize: false }
    }
    /// 这个 pass 现在真的会起作用吗。**未落地的一律 false**，不管开关怎么设——
    /// 否则「开关开着」会被误读成「这个 pass 在工作」。
    pub fn enabled(&self, name: &str) -> bool {
        match name {
            "fuse" => self.fuse,
            "ledger" => self.ledger,
            // 未落地：INTERFACE §七记着它们欠什么
            "lift" => self.lift,
            "speculate" => self.speculate,
            "vectorize" => self.vectorize,
            "fission" | "lower" | "schedule" | "plan" => false,
            _ => false,
        }
    }
    /// 已落地的 pass 名字（给 CLI 与诊断用）
    pub fn landed() -> &'static [&'static str] {
        &["lift", "fuse", "ledger", "speculate", "vectorize"]
    }
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
    "state", "test", "select", "measure", "form", "fill", "judge", "sieve", "pair", "tally", "first_k", "iterate", "outcome", "key_of", "cut", "handle", "consume", "gen", "do", "ask", "transform", "mat", "content", "unsure", "pending", "fail", "is_fail", "loop", "stop",
    "unsure_cause", "untested", "line_source", "taint", "escalate", "literalize", "allocate", "unsure_bound", "agg", "order", "fit",
    "len", "map", "filter", "fold", "range", "append", "concat", "slice", "contains", "sum", "reverse", "keys", "with", "has", "text", "join", "print", "min", "max", "abs", "floor", "exit_kind",
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

impl<'a> Interp<'a> {
    /// 不带 fit 注册表的入口（绝大多数程序不用 fit）。要用 fit 走 `with_fits`。
    pub fn new(client: &'a mut dyn Client, ledger: &'a mut Ledger, calib: &'a CalibStore, actions: &'a ActionRegistry, budget: Budget) -> Interp<'a> {
        Interp::with_fits(client, ledger, calib, actions, empty_fits(), budget)
    }

    pub fn with_fits(
        client: &'a mut dyn Client,
        ledger: &'a mut Ledger,
        calib: &'a CalibStore,
        actions: &'a ActionRegistry,
        fits: &'a FitRegistry,
        budget: Budget,
    ) -> Interp<'a> {
        let model_id = client.model_id();
        Interp { client, ledger, calib, actions, fits, budget, trace: Trace::default(), cost: Cost::default(), frames: vec![], loops: vec![], next_exit: 0, depth: 0, run_seq: 0, model_id, pending: vec![], layers: vec![], passes: Passes::default(), guards: vec![], speculated: HashSet::new(), speculation_used: HashSet::new(), last_eval_provenance: None, pending_field_prov: None, asks_in_ledger: 0, unwrapped_untrusted: HashSet::new(), drift_reported: HashSet::new(), evidence: vec![] }
    }

    pub fn run(mut self, program: &Program) -> Result<Outcome, RtError> {
        self.ledger.set_header(Header {
            budget_calls: self.budget.calls,
            budget_cost: self.budget.cost,
            model_id: self.model_id.clone(),
            render_version: RENDER_VERSION.into(),
            handler_version: HANDLER_VERSION.into(),
            // 头在 run() 入口定稿：档案是运行前就定下的输入，不该等跑完再补
            profile_hash: self.calib.profile.hash.clone(),
            behavior_hash: self.calib.profile.behavior_hash.clone(),
            // **运行开始那一刻的校准库**。这里没有「哪一刻」的选择余地：
            // `Interp` 拿的是 `&CalibStore`，**一次运行之内它长不了**；
            // 增长只发生在两次运行之间（宿主拿 `Outcome.evidence` 去 `absorb`）。
            calib_hash: Some(crate::effects::calib_hash(self.calib)),
        });
        if let Some(w) = self.ledger.header_warning.take() {
            self.trace.warn(w);
        }
        // `budget.escalate` 是**问人的总次数上限**，不是「每次运行 k 次」（12:177、:180
        // 「恢复 = 从头重跑…ask 的答案作为账本条目参与重放」）。核上限时要把账本里**已经问过**的
        // 那些算进去——否则上限 2 在三轮恢复里能问到 6 次人。问人是最贵的效应，这个方向是多花钱。
        //
        // 但**不能把它们计进 `cost.asks`**：那个字段报的是「这次运行实际问了几次人」，
        // 重放时本来就该是 0（CLI 的 library_lifecycle 正是这么断言的，它是对的）。
        // 所以另存一个「账本里已有多少次」，只参与核上限，不进 Cost。
        self.asks_in_ledger = self.ledger.entries.iter().filter(|e| matches!(e, Entry::Ask { .. })).count() as u64;
        self.frames.push(Frame { name: "<program>".into(), exits: vec![], returns_exit: true });
        let env = env_child(&root_env());
        let result = self.eval_block(&program.body, &env).and_then(|v| {
            // 刷新点：程序结束（登记了却没人读的判断，到这里也要发出并记账）
            self.flush("end")?;
            // 推错的那些：推测花了调用、花了预算，**花掉的必须留痕**。
            let 没用上: Vec<&String> = self.speculated.iter().filter(|k| !self.speculation_used.contains(*k)).collect();
            if !没用上.is_empty() {
                let n = 没用上.len();
                let 例 = 没用上.iter().take(3).map(|k| 头(k, 8)).collect::<Vec<_>>().join(", ");
                self.trace.warn(format!("W-spec-unused: 推测了 {n} 个站点没被走到（{例}…）：这些调用花掉了，结果留在账本里但程序没用上"));
            }
            Ok(v)
        });
        match result {
            Ok(v) => {
                let frame = self.frames.pop().unwrap();
                let mut in_value = HashSet::new();
                collect_exit_ids(&v, &mut in_value);
                let mut returned = vec![];
                for e in frame.exits.iter().filter(|e| e.is_unsure() && !e.consumed.get()) {
                    if in_value.contains(&e.id) {
                        e.consumed.set(true);
                        *e.consumed_by.borrow_mut() = "returned".into();
                        returned.push(e.label());
                    } else {
                        return Err(RtError::new(Some("J-05"), format!("程序结束时有未消费的 {}（题 {}）。修法：用 handle(e, {{…, unsure: …}}) 或 consume(e, \"drop\") 处理，或把它带在返回值里", e.label(), 头(&e.q_hash, 8)), e.site));
                    }
                }
                if !returned.is_empty() {
                    self.trace.warn(format!("returned_unsure: {}", returned.join(", ")));
                }
                Ok(Outcome { value: Some(v), pending: vec![], trace: self.trace, cost: self.cost, returned_unsure: returned, layers: self.layers, evidence: self.evidence })
            }
            Err(Fault::Halt(p)) => Ok(Outcome { value: None, pending: vec![p], trace: self.trace, cost: self.cost, returned_unsure: vec![], layers: self.layers, evidence: self.evidence }),
            Err(Fault::Error(e)) => Err(e),
        }
    }

    fn frame(&mut self) -> &mut Frame {
        self.frames.last_mut().unwrap()
    }

    // ---------- 求值 ----------

    fn eval_block(&mut self, b: &Block, env: &Env) -> R<Value> {
        let env = env_child(env);
        // lift 提前登记过的语句下标：它们的绑定已经做好，轮到时跳过
        let mut lifted: HashSet<usize> = HashSet::new();
        for (i, s) in b.statements.iter().enumerate() {
            if lifted.contains(&i) {
                continue;
            }
            match s {
                Statement::Let { name, value, .. } => {
                    // 推测执行（`12`:610「judge 推测提升」）：**在这条语句求值之前**，
                    // 把它后面 `if` 两侧分支体里此刻已能求值的 judge 站点一起登记。
                    //
                    // 为什么在这里而不是在 `if` 那里：条件自己的 `cut` 会刷新一次，
                    // 走到 `if` 时那一层**已经发出去了**，再登记就赶不上同一层了。
                    // 推测要在**触发刷新的那条语句之前**完成——与 Python `spec.py` 的
                    // `_walk_block(start=当前语句, in_progress=True)` 同一个位置。
                    if self.passes.enabled("speculate") {
                        self.speculate_ahead(b, i, &env);
                    }
                    // J-08：记下求值这个绑定时产生了哪些出口——出口带着状态 taint，
                    // 于是「这个布尔由可信状态上的判断决定吗」答得出来。
                    let before = self.frames.last().map(|f| f.exits.len()).unwrap_or(0);
                    // **求值前清空传送带**：它只承载这一条 `let` 自己求值期间的来源，
                    // 不继承上一条语句留下的。没有这一句，任意一条不相关的 `let` 都会串味。
                    self.last_eval_provenance = None;
                    self.pending_field_prov = None;
                    let v = self.eval(value, &env)?;
                    // 记录（而不只是裸 bool）也要记来源：`request_test` 那类 helper 返回的是
                    // `{resolved, value}`，守卫写成 `包.resolved && 包.value`——来源在那个记录上。
                    if matches!(v, Value::Bool(_) | Value::Record(_)) {
                        let prov = {
                            // 出口按帧记，而 helper 函数自成一帧——`问人(m)` 里的 ask 出口落在
                            // 它自己那帧上，求值结束帧就弹掉了。所以这里看的是**这次求值总共新增了
                            // 多少出口**：内层帧退出时未消费的出口会并进外层（`call_closure`），
                            // 已消费的（`handle` 吃掉的）则由 `last_eval_provenance` 带回来。
                            let exits = &self.frames.last().expect("有帧").exits;
                            let made = &exits[before.min(exits.len())..];
                            let mut acc = self.last_eval_provenance.take();
                            for x in made {
                                let cur = acc.unwrap_or((false, false));
                                acc = Some((cur.0 || x.taint == Taint::Trusted, cur.1 || x.from_ask.get()));
                            }
                            acc
                        };
                        // 逐字段来源也绑进环境，键是 `名字\u{1f}字段\u{1f}prov`
                        if let Some(各字段) = self.pending_field_prov.take() {
                            for (f, (t, a)) in 各字段 {
                                env_define(&env, &field_prov_key(name, &f), Value::List(Rc::new(vec![Value::Bool(t), Value::Bool(a)])));
                            }
                        }
                        // 来源绑进**环境**，作用域与这个绑定完全一致
                        env_define(&env, &prov_key(name), match prov {
                            Some((t, a)) => Value::List(Rc::new(vec![Value::Bool(t), Value::Bool(a)])),
                            // 显式记「这个绑定没有来源」，盖住外层同名绑定的来源
                            None => Value::Unit,
                        });
                    }
                    env_define(&env, name, v);
                    // 提升 pass（12 §4 序 1 + :610 修订记录 1 的推测提升）：
                    // 刚登记了一个 judge，就把后面**同状态**、中间无副作用的 judge 一起登记上来，
                    // 免得它们各自等到下一个刷新点、各成一层。只推测 judge——登记零成本零副作用，
                    // 所以不需要回滚。**不跨分支**（`12`:13「§4 删『跨分支提升』，提升只在直线段内」）：
                    // 下面的前瞻只走同一个块的后续语句，遇到分支或副作用就停。
                    if self.passes.enabled("lift") && judged_state(value).is_some() {
                        self.lift_followers(b, i, &env, &mut lifted)?;
                    }
                }
                Statement::Function { name, function, span } => {
                    let c = self.closure(function, &env, Some(name.clone()), *span);
                    env_define(&env, name, c);
                }
                Statement::Expression(e) => {
                    self.eval(e, &env)?;
                }
            }
        }
        match &b.result {
            Some(e) => self.eval(e, &env),
            None => Ok(Value::Unit),
        }
    }

    fn closure(&self, f: &Function, env: &Env, name: Option<String>, span: Span) -> Value {
        let hash = hash_of(&["fn", &serde_json::to_string(f).unwrap_or_default()]);
        Value::Fn(Rc::new(Closure { function: f.clone(), env: env.clone(), name, span, hash }))
    }

    fn eval(&mut self, e: &Expr, env: &Env) -> R<Value> {
        let sp = e.span;
        match &e.kind {
            ExprKind::Integer(i) => Ok(Value::Int(*i)),
            ExprKind::Decimal(d) => Ok(Value::Float(*d)),
            ExprKind::Bool(b) => Ok(Value::Bool(*b)),
            ExprKind::Text(t) => Ok(Value::text(t)),
            ExprKind::Unit => Ok(Value::Unit),
            ExprKind::Name(n) => env_lookup(env, n).ok_or_else(|| Fault::Error(RtError::new(None, format!("未定义的名字 {n}"), sp))),
            ExprKind::List(items) => {
                let mut v = Vec::with_capacity(items.len());
                for it in items {
                    v.push(self.eval(it, env)?);
                }
                Ok(Value::list(v))
            }
            ExprKind::Record(fields) => {
                let mut v = Vec::with_capacity(fields.len());
                // **逐字段记来源**：`{脏字段: 脏判, 净字段: 净判}` 这两个字段的来源不同，
                // 折成一个就等于做了析取——而 `walk_conjuncts` 在守卫那一层**专门拒绝追析取**
                // （「里面的东西不再是这个条件成立所保证的」）。同一份谨慎不能隔一层被自己拆掉。
                //
                // `12`:265 要的是「至少一个**合取项**来自 trusted 状态」，**合取项是值级的概念**。
                let mut 各字段来源: Vec<(String, (bool, bool))> = vec![];
                for (k, it) in fields {
                    let before = self.frames.last().map(|f| f.exits.len()).unwrap_or(0);
                    let saved = self.last_eval_provenance.take();
                    let val = self.eval(it, env)?;
                    if let Some(p) = self.provenance_since(before) {
                        各字段来源.push((k.clone(), p));
                    } else if let Some(p) = self.field_provenance_of(it, env) {
                        // 字段直接引用一个已有绑定（`{脏字段: 脏判}`）：继承那个绑定的来源
                        各字段来源.push((k.clone(), p));
                    }
                    // 本字段的来源已归到本字段名下，不让它漏给整条记录
                    self.last_eval_provenance = saved;
                    v.push((k.clone(), val));
                }
                if !各字段来源.is_empty() {
                    self.pending_field_prov = Some(各字段来源);
                }
                Ok(Value::record(v))
            }
            ExprKind::Function(f) => Ok(self.closure(f, env, None, sp)),
            ExprKind::Block(b) => self.eval_block(b, env),
            ExprKind::If { condition, yes, no } => {
                let c = self.eval(condition, env)?;
                // 刷新点：分支要在已知信息上走，不能让未发出的判断跨过分支边界（12 §2.2:129）
                self.flush("if")?;
                // J-08：条件求值成 Bool 之后 taint 就没了，所以**在这一刻**记下这层守卫的来源。
                let guard = self.guard_of(condition, env);
                match c {
                    Value::Bool(b) => {
                        self.guards.push(guard);
                        let r = if b { self.eval_block(yes, env) } else { self.eval_block(no, env) };
                        self.guards.pop();
                        r
                    }
                    Value::Reading(_) => err(Some("J-01"), "读数不能当条件；先 cut 成出口再 handle", condition.span),
                    other => err(None, format!("if 的条件要是 Bool，收到 {}", other.type_name()), condition.span),
                }
            }
            ExprKind::Field { value, field } => {
                let v = self.eval(value, env)?;
                match &v {
                    Value::Record(_) => v.get(field).ok_or_else(|| Fault::Error(RtError::new(None, format!("记录没有字段 {field}"), sp))),
                    Value::Mat(m) => match field.as_str() {
                        "content" => {
                            // 刷新点：宿主读内容
                            self.flush("content")?;
                            Ok(json_to_value(&m.content))
                        }
                        "taint" => Ok(Value::text(if m.taint == Taint::Trusted { "trusted" } else { "untrusted" })),
                        "hash" => Ok(Value::text(&m.hash)),
                        _ => err(None, format!("Mat 没有字段 {field}"), sp),
                    },
                    Value::Exit(x) => match field.as_str() {
                        "kind" => Ok(Value::text(&x.label())),
                        _ => err(None, format!("Exit 没有字段 {field}（用 handle 消费）"), sp),
                    },
                    Value::Question(q) => question_field(q, field).ok_or_else(|| Fault::Error(RtError::new(None, format!("Question 没有字段 {field}；可读字段：{}", QUESTION_FIELDS.join("、")), sp))),
                    Value::Form(f) => form_field(f, field).ok_or_else(|| Fault::Error(RtError::new(None, format!("Form 没有字段 {field}；可读字段：{}", FORM_FIELDS.join("、")), sp))),
                    Value::Reading(_) => err(Some("J-01"), "读数没有可读字段；只能经 cut 离开", sp),
                    other => err(None, format!("{} 没有字段 {field}", other.type_name()), sp),
                }
            }
            ExprKind::Index { value, index } => {
                let v = self.eval(value, env)?;
                let i = self.eval(index, env)?;
                match (&v, &i) {
                    (Value::List(l), Value::Int(k)) => {
                        let k = *k;
                        if k < 0 || k as usize >= l.len() {
                            return err(None, format!("下标 {k} 越界（长度 {}）", l.len()), sp);
                        }
                        Ok(l[k as usize].clone())
                    }
                    (Value::Record(_), Value::Text(k)) => v.get(k).ok_or_else(|| Fault::Error(RtError::new(None, format!("记录没有字段 {k}"), sp))),
                    _ => err(None, format!("{}[{}] 不可索引", v.type_name(), i.type_name()), sp),
                }
            }
            ExprKind::Unary { op, value } => {
                let v = self.eval(value, env)?;
                match (op.as_str(), &v) {
                    ("!", Value::Bool(b)) => Ok(Value::Bool(!b)),
                    // 13 §6：最小整数取负也越界，同样是运行错误
                    ("-", Value::Int(i)) => Ok(Value::Int(i.checked_neg().ok_or_else(|| overflow("取负", *i, 0, sp))?)),
                    ("-", Value::Float(f)) => Ok(Value::Float(-f)),
                    (_, Value::Reading(_)) => err(Some("J-01"), "读数不能做算术", sp),
                    _ => err(None, format!("一元 {op} 不适用于 {}", v.type_name()), sp),
                }
            }
            ExprKind::Binary { op, left, right } => {
                if op == "&&" || op == "||" {
                    let l = self.eval(left, env)?;
                    return match (op.as_str(), &l) {
                        ("&&", Value::Bool(false)) => Ok(Value::Bool(false)),
                        ("||", Value::Bool(true)) => Ok(Value::Bool(true)),
                        (_, Value::Bool(_)) => {
                            let r = self.eval(right, env)?;
                            match r {
                                Value::Bool(_) => Ok(r),
                                _ => err(None, format!("{op} 右侧要 Bool"), right.span),
                            }
                        }
                        _ => err(None, format!("{op} 左侧要 Bool"), left.span),
                    };
                }
                let l = self.eval(left, env)?;
                let r = self.eval(right, env)?;
                self.binop(op, l, r, sp)
            }
            ExprKind::Call { function, arguments } => {
                let f = self.eval(function, env)?;
                let mut args = Vec::with_capacity(arguments.len());
                for a in arguments {
                    args.push(self.eval(a, env)?);
                }
                self.apply(f, args, sp)
            }
        }
    }

    fn binop(&mut self, op: &str, l: Value, r: Value, sp: Span) -> R<Value> {
        if matches!(l, Value::Reading(_)) || matches!(r, Value::Reading(_)) {
            return err(Some("J-01"), format!("读数不能做 {op}：读数不可比、不可算，只能经 cut 离开"), sp);
        }
        use Value::*;
        Ok(match (op, &l, &r) {
            // 13 §6：整数行为不随 Rust 构建模式改变。溢出与除零一律是**指向 .jpp 源码的运行错误**，
            // 不是 debug 崩溃 / release 悄悄回绕。用 checked_* 表达，两种构建下同一规则。
            ("+", Int(a), Int(b)) => Int(a.checked_add(*b).ok_or_else(|| overflow("加法", *a, *b, sp))?),
            ("-", Int(a), Int(b)) => Int(a.checked_sub(*b).ok_or_else(|| overflow("减法", *a, *b, sp))?),
            ("*", Int(a), Int(b)) => Int(a.checked_mul(*b).ok_or_else(|| overflow("乘法", *a, *b, sp))?),
            ("/", Int(a), Int(b)) => {
                if *b == 0 {
                    return err(None, "除以零：Int 除法的除数不能是 0", sp);
                }
                Int(a.checked_div(*b).ok_or_else(|| overflow("除法", *a, *b, sp))?)
            }
            ("%", Int(a), Int(b)) => {
                if *b == 0 {
                    return err(None, "取模零：Int 取模的除数不能是 0", sp);
                }
                Int(a.checked_rem(*b).ok_or_else(|| overflow("取模", *a, *b, sp))?)
            }
            ("+", Float(a), Float(b)) => Float(a + b),
            ("-", Float(a), Float(b)) => Float(a - b),
            ("*", Float(a), Float(b)) => Float(a * b),
            ("/", Float(a), Float(b)) => Float(a / b),
            ("+", Int(a), Float(b)) | ("+", Float(b), Int(a)) => Float(*a as f64 + b),
            ("*", Int(a), Float(b)) | ("*", Float(b), Int(a)) => Float(*a as f64 * b),
            ("-", Int(a), Float(b)) => Float(*a as f64 - b),
            ("-", Float(a), Int(b)) => Float(a - *b as f64),
            ("+", Text(a), Text(b)) => Value::text(&format!("{a}{b}")),
            ("+", List(a), List(b)) => Value::list(a.iter().chain(b.iter()).cloned().collect()),
            ("<", Int(a), Int(b)) => Bool(a < b),
            ("<=", Int(a), Int(b)) => Bool(a <= b),
            (">", Int(a), Int(b)) => Bool(a > b),
            (">=", Int(a), Int(b)) => Bool(a >= b),
            ("<", Float(a), Float(b)) => Bool(a < b),
            ("<=", Float(a), Float(b)) => Bool(a <= b),
            (">", Float(a), Float(b)) => Bool(a > b),
            (">=", Float(a), Float(b)) => Bool(a >= b),
            // `equals` 返回 None = 里面有读数，不可比（J-01）。这里以前是 `unwrap_or(false)`，
            // 把「不可比」这个信号吃成了「不相等」——顶上那道 J-01 只拦裸读数，
            // 装进列表或记录就从这条缝里漏过去了。
            ("==", _, _) | ("!=", _, _) => match l.equals(&r) {
                Some(eq) => Bool(if op == "==" { eq } else { !eq }),
                None => return err(Some("J-01"), format!("读数不能做 {op}：读数没有可读的值，装进列表或记录也一样。修法：先 cut 成出口再比出口"), sp),
            },
            _ => return err(None, format!("二元 {op} 不适用于 {} 与 {}", l.type_name(), r.type_name()), sp),
        })
    }

    fn apply(&mut self, f: Value, args: Vec<Value>, sp: Span) -> R<Value> {
        match f {
            Value::Fn(c) => self.call_closure(&c, args, sp),
            Value::Builtin(name) => self.builtin(name, args, sp),
            other => err(None, format!("{} 不可调用", other.type_name()), sp),
        }
    }

    fn call_closure(&mut self, c: &Rc<Closure>, args: Vec<Value>, sp: Span) -> R<Value> {
        let f = &c.function;
        if args.len() != f.parameters.len() {
            return err(None, format!("{} 需要 {} 个参数，收到 {}", c.name.as_deref().unwrap_or("函数"), f.parameters.len(), args.len()), sp);
        }
        let max_depth = self.budget.depth.unwrap_or(DEFAULT_DEPTH);
        if self.depth >= max_depth {
            return err(Some("J-06"), format!("调用深度超过 {max_depth}（递归无界）。修法：用 loop(bound, …) 或提高 budget.depth"), sp);
        }
        self.depth += 1;
        let env = env_child(&c.env);
        for (p, a) in f.parameters.iter().zip(args) {
            env_define(&env, &p.name, a);
        }
        let returns_exit = f.result_type.as_ref().map(|t| t.mentions("Exit")).unwrap_or(false);
        self.frames.push(Frame { name: c.name.clone().unwrap_or_else(|| "<fn>".into()), exits: vec![], returns_exit });
        let result = self.eval_block(&f.body, &env);
        let frame = self.frames.pop().unwrap();
        // J-08：这一帧里产生过的出口（含已被 handle 消费的）的来源，带回调用方——
        // 否则 `fn 问人(m) { handle(ask(…), …) }` 这类 helper 一返回，「经 ask」就丢了。
        if !frame.exits.is_empty() {
            let prov = frame
                .exits
                .iter()
                .fold(self.last_eval_provenance.unwrap_or((false, false)), |acc, x| (acc.0 || x.taint == Taint::Trusted, acc.1 || x.from_ask.get()));
            self.last_eval_provenance = Some(prov);
        }
        self.depth -= 1;
        let v = result?;
        let mut in_value = HashSet::new();
        collect_exit_ids(&v, &mut in_value);
        // 13 §3 的两个案例，粒度在中间——不是都放过，也不是都拦下：
        //   1. 责任**没有**出现在返回值里 = 最后一份承接信息被丢了（取字段、过滤、切片扔掉了它）→ **错**。
        //   2. 责任**如实出现在返回值里**、只是返回类型没提 Exit → **警告**，报文直接给修法。
        //      它是标注缺失，不是责任丢失；责任继续往上挂，由调用者或程序结束前的检查接着核。
        //      样例的返回类型补齐后这一条升为错（见 INTERFACE.md §七）。
        for e in frame.exits.into_iter().filter(|e| e.is_unsure() && !e.consumed.get()) {
            if in_value.contains(&e.id) {
                if !frame.returns_exit {
                    self.trace.warn(format!(
                        "W-untyped-transfer: {} 把 {} 装在返回值里交了出去，但返回类型没提 Exit，调用者从签名上看不出自己收到了一份未决。修法：把返回类型标为含 Exit（如 `-> Exit`、`-> Record<Exit>`）",
                        frame.name,
                        e.label()
                    ));
                }
                *e.consumed_by.borrow_mut() = format!("return_type:{}", frame.name);
                self.frame().exits.push(e);
            } else {
                return err(
                    Some("J-05"),
                    format!("{} 返回前有未消费的 {}，而且它没出现在返回值里——最后一份承接信息被丢掉了。修法：在函数内 handle/consume，或把它放进返回值并把返回类型标为含 Exit", frame.name, e.label()),
                    e.site,
                );
            }
        }
        Ok(v)
    }

    // ---------- 材料与状态 ----------

    fn as_mat(&self, v: &Value, slot: &str, sp: Span) -> R<Mat> {
        match v {
            Value::Mat(m) => Ok((**m).clone()),
            Value::Reading(_) => err(Some("J-01"), format!("读数不能放进 {slot} 槽：读数只能经 cut 离开，不是材料"), sp),
            Value::Exit(e) => {
                let mut d = BTreeSet::new();
                d.insert(e.q_hash.clone());
                Ok(Mat::new(json!({"exit": e.label()}), "", vec![format!("exit:{}", e.q_hash)], e.taint, d))
            }
            Value::Duty(_) => err(
                Some("J-05"),
                format!("未决责任不能直接当材料放进 {slot}：变成材料或 JSON 不消除义务。修法：先 literalize(u, …) 重问，或把 u 包进返回值"),
                sp,
            ),
            Value::State(_) | Value::Question(_) | Value::Fn(_) | Value::Builtin(_) | Value::Stop(_) => err(None, format!("{} 不能作材料", v.type_name()), sp),
            Value::Fail(s) => Ok(Mat::new(json!({"fail": s.as_ref()}), "", vec!["fail".into()], Taint::Trusted, BTreeSet::new())),
            // 字面量兜底臂。**这里是一条洗白路径**：`content(脏)` 把材料拆成裸值，
            // 再 `mat(...)` 包回去就成了 `Mat::literal` —— trusted、origin=["literal"]。
            // 按已立的判据（兜底往拒绝那边倒），拆出来过的内容包回去仍然 untrusted。
            other => {
                let j = other.to_json();
                // **递归查**：包装成 `{outer: 拆了}`、`[拆了]` 都算——此前只比顶层那一个值，
                // 多套一层容器就绕过去了（实测：`mat({outer: content(脏)})` 洗白成功）。
                if contains_untrusted_part(&self.unwrapped_untrusted, &j) {
                    Ok(Mat::new(j, "", vec!["unwrapped".into()], Taint::Untrusted, BTreeSet::new()))
                } else {
                    Ok(Mat::literal(j))
                }
            }
        }
    }

    fn as_mats(&self, v: &Value, slot: &str, sp: Span) -> R<(Vec<Mat>, bool)> {
        let items: Vec<Value> = match v {
            Value::List(l) => l.iter().cloned().collect(),
            other => vec![other.clone()],
        };
        let has_fail = items.iter().any(|x| matches!(x, Value::Fail(_)));
        let mut out = vec![];
        for it in items {
            out.push(self.as_mat(&it, slot, sp)?);
        }
        Ok((out, has_fail))
    }

    fn make_state(&self, args: &[Value], sp: Span) -> R<Value> {
        if args.is_empty() || args.len() > 2 {
            return err(None, "state(on) 或 state(on, {ctx: […], ref: […], over: […]})", sp);
        }
        let (on, f1) = self.as_mats(&args[0], "on", sp)?;
        if on.is_empty() || on.len() > 2 {
            return err(Some("J-14"), format!("on 恰一个判断对象（或一对），收到 {}", on.len()), sp);
        }
        let mut ctx = vec![];
        let mut r#ref = vec![];
        let mut over = vec![];
        let mut fail = f1;
        if let Some(opts) = args.get(1) {
            if !matches!(opts, Value::Record(_)) {
                return err(None, "state 的第二个参数是记录 {ctx, ref, over}", sp);
            }
            for (k, target) in [("ctx", &mut ctx), ("ref", &mut r#ref), ("over", &mut over)] {
                if let Some(v) = opts.get(k) {
                    let (ms, f) = self.as_mats(&v, k, sp)?;
                    fail |= f;
                    *target = ms;
                }
            }
        }
        Ok(Value::State(Rc::new(State::new(on, ctx, r#ref, over, fail))))
    }

    // ---------- 效应 ----------

    fn charge(&mut self, calls: u64, usd: f64, sp: Span) -> R<()> {
        if self.cost.calls + calls > self.budget.calls || self.cost.usd + usd > self.budget.cost {
            return Err(Fault::Halt(Pending {
                cause: "budget".into(),
                key: String::new(),
                site: sp,
                detail: format!("预算耗尽：calls {}+{} / {}，cost {:.6}+{:.6} / {:.6}", self.cost.calls, calls, self.budget.calls, self.cost.usd, usd, self.budget.cost),
            }));
        }
        Ok(())
    }

    fn judge(&mut self, state: &Rc<State>, qs: &[Rc<Question>], sp: Span) -> R<Vec<Value>> {
        for q in qs {
            if state.derived_from.contains(&q.hash) {
                return err(Some("J-02"), format!("禁自指：状态含由题「{}」派生的材料，不能再问同一题", q.text), sp);
            }
        }
        if state.has_fail {
            return Ok(qs
                .iter()
                .map(|q| {
                    Value::Reading(Rc::new(Reading {
                        q_hash: q.hash.clone(),
                        state_hash: state.hash.clone(),
                        op: q.op,
                        calib: q.calib.clone(),
                        answer: std::cell::RefCell::new(None),
                        fail: Some("状态含 Fail 材料".into()),
                        model_id: self.model_id.clone(),
                        ledger_key: String::new(),
                        over_len: state.over.len(),
                        scale: q.scale.clone(),
                        perms: std::cell::Cell::new(0),
                mode_share: std::cell::Cell::new(None),
                        missing_evidence: missing_evidence(state, q),
                        state_taint: state.taint, form_hash: q.form_hash.clone(),
                    }))
                })
                .collect());
        }
        let keys: Vec<String> = qs.iter().map(|q| judge_key(&self.model_id, &state.hash, &q.hash, q.op.phys(), 0, self.run_seq, sp.start)).collect();
        // 循环内键重复即停（J-06）
        if let Some(lc) = self.loops.last_mut() {
            for k in &keys {
                if !lc.seen_keys.insert(k.clone()) && lc.repeated.is_none() {
                    lc.repeated = Some(k.clone());
                }
            }
        }
        // 12 §2.2「**惰性**：登记后不发」。账本命中的当场填上（重放不花钱、也不必推迟）；
        // 缺的登记进 `pending`，等一个**刷新点**（`cut` / `if` / 程序结束）按状态分组一层发出。
        let readings: Vec<Rc<Reading>> = qs
            .iter()
            .zip(&keys)
            .map(|(q, k)| {
                Rc::new(Reading {
                    q_hash: q.hash.clone(),
                    state_hash: state.hash.clone(),
                    op: q.op,
                    calib: q.calib.clone(),
                    answer: std::cell::RefCell::new(None),
                    fail: None,
                    model_id: self.model_id.clone(),
                    ledger_key: k.clone(),
                    over_len: state.over.len(),
                    scale: q.scale.clone(),
                    perms: std::cell::Cell::new(0),
                mode_share: std::cell::Cell::new(None),
                    missing_evidence: missing_evidence(state, q),
                    state_taint: state.taint, form_hash: q.form_hash.clone(),
                })
            })
            .collect();
        let mut missing = vec![];
        for (i, k) in keys.iter().enumerate() {
            // 真站点走到了一个推测过的键：这次推测用上了
            if self.speculated.contains(k) {
                self.speculation_used.insert(k.clone());
            }
            if let Some(Entry::Judge { answer, .. }) = self.ledger.get(k) {
                readings[i].fill(answer.clone());
                self.cost.replayed += 1;
                self.trace.push("judge", k, true, 0.0, sp, format!("「{}」", qs[i].text));
            } else {
                missing.push(i);
            }
        }
        // 窗口检查（J-14 / 12:117）：对象槽内单段按 text_slots，槽间按 json_slots。
        // 超窗不报错只留痕——它不是算错，是**读数被语境接管而无人察觉**
        // （档案：「≈1,000 token 带主张语境下翻转 60.7%，读数被语境接管」）。
        // 与 Python `_check_window` 同为 warn。
        self.check_window(state, sp);
        if !missing.is_empty() {
            self.pending.push(PendingJudge {
                state: state.clone(),
                items: missing.iter().map(|i| (qs[*i].clone(), readings[*i].clone(), keys[*i].clone())).collect(),
                site: sp,
                speculative: false,
            });
        }
        Ok(readings.into_iter().map(Value::Reading).collect())
    }


    /// 三路过滤的求值（`05` §1）。返回每道题一份 `{question, act, ignore, unsure, unobserved, stopped}`。
    ///
    /// - 每个元素保留原值（`item`）、输入位置（`index`）、出口（`exit`）、未决原因（`cause`）、
    ///   以及经过的前几次过滤（`trail`，由上一次过滤的出口组成）。
    /// - **完整输入时 act / ignore / unsure 不漏、互斥**；同一元素出现两次就在流里出现两次（各带自己的 index），
    ///   同状态同题只问一次（账本同键去重）。
    /// - **预算提前停止**：没问到的元素进 `unobserved`，不混进 ignore 或 unsure；`stopped` 写明原因。
    ///   这里接住 budget 停机是因为「部分观察 + 未观察范围」本身就是这个算子的一个合法结果；
    ///   之后的判断照常受预算约束。
    /// - 输入元素若本身是一次过滤的产物（带 `item` / `exit` / `trail` 的记录），取它的 `item` 当材料，
    ///   `trail` 接上：**产物与输入同形，可再过滤**（组合封闭）。
    /// - act / ignore 出口在这里被路由，记为已消费；unsure 出口放进 unsure 流，责任随返回值转交（J-05）。
    /// 构造一个契约值（字段顺序固定）。
    #[allow(clippy::too_many_arguments)]
    fn outcome_value(kind: &str, value: Value, pending: Vec<Value>, evidence: Vec<Value>, resume: Value, spent: (i64, f64), detail: Value, purpose: Value) -> Value {
        // 续接总是记录（空记录 = 没有停在半路、也没有继续方法），调用者可以用 has 查
        let resume = if matches!(resume, Value::Unit) { Value::record(vec![]) } else { resume };
        Value::record(vec![
            ("kind".into(), Value::text(kind)),
            ("value".into(), value),
            ("pending".into(), Value::list(pending)),
            ("evidence".into(), Value::list(evidence)),
            ("resume".into(), resume),
            ("spent".into(), Value::record(vec![("calls".into(), Value::Int(spent.0)), ("usd".into(), Value::Float(spent.1))])),
            ("detail".into(), detail),
            ("purpose".into(), purpose),
        ])
    }

    /// 未决清单的一项：`{element, exit, cause}`
    fn pending_entry(element: Value, exit: &Value) -> Value {
        let cause = match exit {
            Value::Exit(e) | Value::Duty(e) => Value::text(&e.cause()),
            _ => Value::Unit,
        };
        Value::record(vec![("element".into(), element), ("exit".into(), exit.clone()), ("cause".into(), cause)])
    }

    /// 本次调用前的记账位置：之后据此算 `spent`，并从 trace 里取本构造触发的判断账本键
    fn mark(&self) -> (usize, u64, f64) {
        (self.trace.events.len(), self.cost.calls, self.cost.usd)
    }

    /// 这一段里判断过的账本键，以及它们花掉的调用与费用。
    /// **按账本记录算，不按本次实际发出的算**：重放时读出同样的调用号与费用，
    /// 契约值因此逐字节不变（J-18）。一次调用里融合了多道题，只计一次。
    fn since(&self, m: (usize, u64, f64)) -> (Vec<Value>, (i64, f64)) {
        let mut keys: Vec<String> = vec![];
        for e in &self.trace.events[m.0.min(self.trace.events.len())..] {
            if e.kind == "judge" && !keys.contains(&e.key) {
                keys.push(e.key.clone());
            }
        }
        let mut calls: Vec<u64> = vec![];
        let mut usd = 0.0;
        for (i, k) in keys.iter().enumerate() {
            if let Some(Entry::Judge { call, cost, .. }) = self.ledger.get(k) {
                // 老账本没有调用号：每个键算一次调用（上界）
                let id = if *call == 0 { u64::MAX - i as u64 } else { *call };
                if !calls.contains(&id) {
                    calls.push(id);
                    usd += cost;
                }
            }
        }
        let _ = m.1;
        (keys.into_iter().map(|k| Value::text(&k)).collect(), (calls.len() as i64, usd))
    }

    /// 输入若是契约值：取出 (产出列表, 未决清单, 证据)；否则 (原列表, 空, 空)。
    /// 契约值的产出不是列表时报错——只有列表产出能交给吃集合的构造。
    fn unpack(&self, v: &Value, who: &str, sp: Span) -> R<(Vec<Value>, Vec<Value>, Vec<Value>)> {
        if is_outcome(v) {
            let items = match v.get("value") {
                Some(Value::List(l)) => l.iter().cloned().collect(),
                other => return err(None, format!("{who} 收到的契约值产出不是列表（是 {}），不能按集合处理；取出其中的列表再交给 {who}", other.map(|x| x.type_name()).unwrap_or("Unit")), sp),
            };
            return Ok((items, list_of(v.get("pending")), list_of(v.get("evidence"))));
        }
        match v {
            Value::List(l) => Ok((l.iter().cloned().collect(), vec![], vec![])),
            other => err(None, format!("{who} 要列表或契约值，收到 {}", other.type_name()), sp),
        }
    }

    fn sieve(&mut self, items: &[Value], qs: &[Rc<Question>], carried_pending: &[Value], carried_evidence: &[Value], sp: Span) -> R<Vec<Value>> {
        let m0 = self.mark();
        for q in qs {
            if q.op != Op::Test {
                return err(None, format!("sieve 只收是非题（test）：题「{}」是 {}。K 选一与打分的分流待后续件", q.text, q.op.fixture_name()), sp);
            }
        }
        // 先把此前登记的判断发出去：下面要接住预算停机，不能连带吞掉别人的登记
        self.flush("sieve-before")?;
        let mut prepared: Vec<(Value, Value, Vec<Rc<Reading>>, Value)> = vec![];
        for it in items {
            let (material, trail) = element_parts(it);
            let state = match &material {
                Value::State(s) => s.clone(),
                other => match self.make_state(&[other.clone()], sp)? {
                    Value::State(s) => s,
                    _ => return err(None, "sieve 无法把元素变成状态", sp),
                },
            };
            let rs = self.judge(&state, qs, sp)?;
            let rs: Vec<Rc<Reading>> = rs.into_iter().map(|v| match v { Value::Reading(r) => r, _ => unreachable!("judge 只回读数") }).collect();
            prepared.push((material, trail, rs, it.clone()));
        }
        let stopped = match self.flush("sieve") {
            Ok(()) => None,
            Err(Fault::Halt(p)) if p.cause == "budget" => Some(p),
            Err(e) => return Err(e),
        };
        let (_, carried_spent) = self.since(m0);
        let mut out = vec![];
        for (j, q) in qs.iter().enumerate() {
            let (mut act, mut ignore, mut pending, mut evidence, mut n_unobserved) = (vec![], vec![], vec![], vec![], 0usize);
            for (i, (material, trail, rs, source)) in prepared.iter().enumerate() {
                let r = &rs[j];
                // `source` = 调用者交进来的原元素（配对产物、上一次过滤的产物……原样保留），
                // `item` 只是交给判断器的那一份材料。两者分开，来源与关系不会在过滤中丢失。
                let base = vec![("item".to_string(), material.clone()), ("index".to_string(), Value::Int(i as i64)), ("trail".to_string(), trail.clone()), ("source".to_string(), source.clone())];
                if r.answer.borrow().is_none() && r.fail.is_none() {
                    // 预算停机没问到：记为 Unsure(budget) 进未决清单（B17 取舍），不混进 ignore
                    n_unobserved += 1;
                    let ex = self.new_exit(ExitKind::Unsure("budget".into()), None, Op::Test, &q.hash, "", crate::value::Taint::Trusted, sp);
                    let mut rec = base;
                    rec.push(("exit".to_string(), ex.clone()));
                    rec.push(("cause".to_string(), Value::text("budget")));
                    pending.push(Self::pending_entry(Value::record(rec), &ex));
                    continue;
                }
                if !r.ledger_key.is_empty() && !evidence.iter().any(|k: &Value| matches!(k, Value::Text(t) if t.as_ref() == r.ledger_key)) {
                    evidence.push(Value::text(&r.ledger_key));
                }
                let exit = self.cut(r, None, sp)?;
                let Value::Exit(e) = &exit else { return err(None, "cut 没有给出出口", sp) };
                let mut rec = base;
                rec.push(("exit".to_string(), exit.clone()));
                match &e.kind {
                    ExitKind::Act => { e.consumed.set(true); *e.consumed_by.borrow_mut() = "sieve:act".into(); rec.push(("cause".into(), Value::Unit)); act.push(Value::record(rec)); }
                    ExitKind::Ignore => { e.consumed.set(true); *e.consumed_by.borrow_mut() = "sieve:ignore".into(); rec.push(("cause".into(), Value::Unit)); ignore.push(Value::record(rec)); }
                    ExitKind::Unsure(_) => { rec.push(("cause".into(), Value::text(&e.cause()))); pending.push(Self::pending_entry(Value::record(rec), &exit)); }
                    _ => return err(None, "是非题给出了非是非出口", sp),
                }
            }
            if let Some(p) = &stopped {
                if j == 0 && n_unobserved > 0 {
                    self.trace.warn(format!("W-sieve-budget: 三路过滤在预算处停止，{} 个元素未观察，记为 Unsure(budget) 进未决清单（未计入 ignore）：{}", n_unobserved, p.detail));
                }
            }
            let resume = match &stopped {
                Some(p) => Value::record(vec![("reason".into(), Value::text("budget")), ("detail".into(), Value::text(&p.detail)), ("unobserved".into(), Value::Int(n_unobserved as i64))]),
                None => Value::Unit,
            };
            let detail = Value::record(vec![("question".into(), Value::Question(q.clone())), ("ignore".into(), Value::list(ignore))]);
            out.push((Value::list(act), pending, evidence, resume, detail));
        }
        let mut outs = vec![];
        let n_out = out.len();
        for (j, (value, pending, evidence, resume, detail)) in out.into_iter().enumerate() {
            // 多道题一次过滤：调用与费用、以及从输入带进来的未决与证据，只记在第一份契约上，
            // 其余为零——融合后的调用分不到每道题，重复记会让求和翻倍。
            let (mut pending, mut evidence) = (pending, evidence);
            let spent = if j == 0 { carried_spent } else { (0, 0.0) };
            if j == 0 {
                pending.extend(carried_pending.iter().cloned());
                for k in carried_evidence.iter() { push_key(&mut evidence, k.clone()); }
            }
            let _ = n_out;
            outs.push(Self::outcome_value("sieve", value, pending, evidence, resume, spent, detail, Value::Unit));
        }
        Ok(outs)
    }

    /// 把后面同状态、可安全提前登记的 `judge` 一起登记上来。
    ///
    /// 停在：遇到分支 / 循环（不跨分支）；遇到会产生副作用或改状态的调用
    /// （`do` / `gen` / `ask` / `transform`）；遇到重新绑定了状态表达式里用到的名字。
    fn lift_followers(&mut self, b: &Block, from: usize, env: &Env, lifted: &mut HashSet<usize>) -> R<()> {
        let Some(head_state) = judged_state(match &b.statements[from] {
            Statement::Let { value, .. } => value,
            _ => return Ok(()),
        }) else {
            return Ok(());
        };
        // 状态表达式用到的名字：谁被重新绑定，就不能再提了
        let mut used = BTreeSet::new();
        names_in(head_state, &mut used);

        for (j, st) in b.statements.iter().enumerate().skip(from + 1) {
            let Statement::Let { name, value, .. } = st else { break };
            // 中间有副作用 / 改状态的调用：停
            if has_impure(value) || has_branch(value) {
                break;
            }
            match judged_state(value) {
                // 同状态（结构相同的表达式）才提；不同状态的层合并没有消费者，不做
                Some(s2) if same_shape(s2, head_state) => {
                    let v = self.eval(value, env)?;
                    env_define(env, name, v);
                    lifted.insert(j);
                }
                // 不是 judge、也不碰状态里的名字：跳过它继续往后看
                None if !used.contains(name.as_str()) => {}
                _ => break,
            }
            // 这一句重新绑定了状态里用到的名字：后面的同名状态已经不是同一个了
            if used.contains(name.as_str()) {
                break;
            }
        }
        Ok(())
    }

    /// 跨对象偏序分档（`12`:134「相邻档并列」）：按可比值降序，**相邻差 ≤ δ 的并列成一档**。
    /// 失败/停止的读数（J-12）单独一档排最后。
    ///
    /// 为什么是偏序不是全序：δ 是同一读数重测的抖动，δ 之内的差**不是真差别**。
    /// 全序会让「0.71 排在 0.70 前面」看起来像个结论。
    fn order_tiers(&self, rs: &[Rc<Reading>]) -> Vec<Vec<usize>> {
        let mut ok: Vec<(usize, f64)> = vec![];
        let mut failed: Vec<usize> = vec![];
        for (i, r) in rs.iter().enumerate() {
            match rank_value(r) {
                Some(v) => ok.push((i, v)),
                None => failed.push(i),
            }
        }
        ok.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal).then(a.0.cmp(&b.0)));
        let delta = rs.first().map(|r| self.calib.delta_for(&self.calib.get(&r.calib), r.op)).unwrap_or(0.0);
        let mut tiers: Vec<Vec<usize>> = vec![];
        for (i, v) in ok {
            match tiers.last_mut() {
                Some(last) if (rank_value(&rs[*last.last().unwrap()]).unwrap_or(v) - v).abs() <= delta => last.push(i),
                _ => tiers.push(vec![i]),
            }
        }
        if !failed.is_empty() {
            tiers.push(failed);
        }
        tiers
    }

    /// 从本块的第 `from` 条语句往后看，把 `if` 两侧分支体里此刻已能求值的站点推测登记。
    /// **只走直线段**：遇到会新绑定名字的语句就把那个名字记进 `born`（之后依赖它的站点不推）。
    fn speculate_ahead(&mut self, b: &Block, from: usize, env: &Env) {
        let mut born: HashSet<String> = HashSet::new();
        for st in b.statements.iter().skip(from) {
            match st {
                Statement::Let { name, value, .. } => {
                    self.speculate_in_ifs(value, env, &born);
                    born.insert(name.clone());
                }
                Statement::Expression(e) => self.speculate_in_ifs(e, env, &born),
                Statement::Function { name, .. } => {
                    born.insert(name.clone());
                }
            }
        }
        if let Some(r) = &b.result {
            self.speculate_in_ifs(r, env, &born);
        }
    }

    /// **循环向量化**（宪法登记表第 47 行，pass `vectorize`）：把 `map`/`filter`
    /// **后续各轮**的 `judge` 站点提前登记进本层，这样体内有 `cut` 时不会一轮一层。
    ///
    /// **实测它只在一个形状上有余量**（四个形状各跑一遍）：
    /// | 形状 | 接之前 | 可省 |
    /// |---|---|---|
    /// | 异状态·无 `cut` | calls 3 / layers 1 | **无**——惰性已经把三轮并进一层 |
    /// | 同状态·无 `cut` | calls 1 / layers 1 | **无**——`fuse` 已经合成一次调用 |
    /// | **异状态·带 `cut`** | **calls 3 / layers 3** | **layers 3 → 1** |
    /// | 同状态·带 `cut` | calls 1 / layers 1 | **无**——后两轮同键，账本直接重放 |
    ///
    /// **它没有「白花」那一栏**，与 `speculate` 不同：`map` 对**每个**元素都会调 `f`，
    /// 所以提前登记的站点**没有一个是猜的**。`speculate` 那一栏之所以存在，
    /// 是因为分支只走一侧。**所以它默认开，而 `speculate` 的账要两栏。**
    ///
    /// **条件（登记表原文）**：体内无 `do`/`ask`、无 loop-carried 名字。
    /// - `do`/`ask` 由 `speculate_expr` 本来就拦（它只登记 `judge`，遇副作用即停）。
    /// - **loop-carried 在 `map`/`filter` 上由构造排除**：这门语言没有赋值，
    ///   跨轮的唯一通道是显式累加器，而 `map`/`filter` 的体只收一个元素。
    ///   **`fold`/`loop` 有累加器，所以它们不在这条路上**——这也是只接 `map`/`filter` 的原因。
    fn vectorize_ahead(&mut self, f: &Value, items: &[Value], _sp: Span) {
        if !self.passes.enabled("vectorize") {
            return;
        }
        let Value::Fn(c) = f else { return };
        if c.function.parameters.len() != 1 {
            return;
        }
        // 第 0 轮马上就要真跑，不用提前登记；提前的是其余各轮
        for it in items.iter().skip(1) {
            let env = env_child(&c.env);
            env_define(&env, &c.function.parameters[0].name, it.clone());
            self.speculate_branch_with(&c.function.body, &env, &HashSet::new());
        }
    }

    /// 在表达式里找 `if`，推测它两侧的分支体
    fn speculate_in_ifs(&mut self, e: &Expr, env: &Env, born: &HashSet<String>) {
        match &e.kind {
            ExprKind::If { yes, no, .. } => {
                self.speculate_branch_with(yes, env, born);
                self.speculate_branch_with(no, env, born);
            }
            ExprKind::Block(inner) => {
                if let Some(r) = &inner.result {
                    self.speculate_in_ifs(r, env, born);
                }
            }
            _ => {}
        }
    }

    fn speculate_branch_with(&mut self, b: &Block, env: &Env, outer_born: &HashSet<String>) {
        let mut born = outer_born.clone();
        for st in &b.statements {
            match st {
                Statement::Let { name, value, .. } => {
                    self.speculate_expr(value, env, &born);
                    born.insert(name.clone());
                }
                Statement::Expression(e) => self.speculate_expr(e, env, &born),
                Statement::Function { name, .. } => {
                    born.insert(name.clone());
                }
            }
        }
        if let Some(r) = &b.result {
            self.speculate_expr(r, env, &born);
        }
    }

    /// 把一个分支体里**此刻已能求值**的 `judge` 站点登记进本层（推测执行）。
    ///
    /// **只推测 `judge`**：`do`/`gen`/`ask` 有代价或触世界，猜错要回滚而我们没有回滚
    /// （红队 06 A1）。**只走本块的直线段**：遇到分支、循环、含副作用的语句就停。
    ///
    /// **搬过去的站点，来源还是原来那份**：状态由 `make_state` 在**当前环境**里算出来，
    /// taint 与 `derived_from` 都跟着材料走——推测不新建材料，所以没有新边界。
    /// 这一点是总控提的「每个把值搬过去的边界，默认都会把『这个值怎么来的』留在原地」
    /// 的直接回答：**这里搬的是站点不是值，值仍在原环境里算**。
    fn speculate_branch(&mut self, b: &Block, env: &Env) {
        // 分支体里新绑定的名字：依赖它们的站点此刻算不出状态，不推
        let mut born: HashSet<String> = HashSet::new();
        for st in &b.statements {
            match st {
                Statement::Let { name, value, .. } => {
                    self.speculate_expr(value, env, &born);
                    born.insert(name.clone());
                }
                Statement::Expression(e) => self.speculate_expr(e, env, &born),
                Statement::Function { name, .. } => {
                    born.insert(name.clone());
                }
            }
        }
        if let Some(r) = &b.result {
            self.speculate_expr(r, env, &born);
        }
    }

    fn speculate_expr(&mut self, e: &Expr, env: &Env, born: &HashSet<String>) {
        // 含副作用的调用：整棵子树都不推（不跨分支推测 do/gen/ask）
        if has_impure(e) {
            return;
        }
        if let ExprKind::Call { function, arguments } = &e.kind {
            if let ExprKind::Name(n) = &function.kind {
                if n == "judge" && arguments.len() == 2 {
                    // 用到分支体内才产生的名字：此刻算不出状态
                    let mut used = BTreeSet::new();
                    names_in(e, &mut used);
                    if used.iter().any(|u| born.contains(u)) {
                        return;
                    }
                    // 试着在当前环境里求出状态与题；求不出就放弃这个站点（不报错）
                    if let (Ok(Value::State(st)), Ok(q)) = (self.eval(&arguments[0], env), self.eval(&arguments[1], env)) {
                        let qs: Vec<Rc<Question>> = match q {
                            Value::Question(q) => vec![q],
                            Value::List(l) => l.iter().filter_map(|x| if let Value::Question(q) = x { Some(q.clone()) } else { None }).collect(),
                            _ => return,
                        };
                        if !qs.is_empty() {
                            let _ = self.register_speculative(&st, &qs, e.span);
                        }
                    }
                    return;
                }
            }
        }
        // 往下走（只在纯表达式里）
        match &e.kind {
            ExprKind::Call { function, arguments } => {
                self.speculate_expr(function, env, born);
                for a in arguments {
                    self.speculate_expr(a, env, born);
                }
            }
            ExprKind::List(items) => items.iter().for_each(|x| self.speculate_expr(x, env, born)),
            ExprKind::Record(fs) => fs.iter().for_each(|(_, x)| self.speculate_expr(x, env, born)),
            ExprKind::Field { value, .. } => self.speculate_expr(value, env, born),
            ExprKind::Binary { left, right, .. } => {
                self.speculate_expr(left, env, born);
                self.speculate_expr(right, env, born);
            }
            ExprKind::Block(b) => self.speculate_branch(b, env),
            _ => {}
        }
    }

    /// 登记一个推测站点：与真站点同一套键（`judge_key`），所以真站点走到时直接命中账本。
    fn register_speculative(&mut self, state: &Rc<State>, qs: &[Rc<Question>], sp: Span) -> Option<()> {
        if state.has_fail {
            return None;
        }
        for q in qs {
            if state.derived_from.contains(&q.hash) {
                return None; // 禁自指的站点不推
            }
        }
        let keys: Vec<String> = qs.iter().map(|q| judge_key(&self.model_id, &state.hash, &q.hash, q.op.phys(), 0, self.run_seq, sp.start)).collect();
        let mut items = vec![];
        for (q, k) in qs.iter().zip(&keys) {
            // 账本里已经有 = 不用推
            if self.ledger.get(k).is_some() {
                continue;
            }
            // 这一层已经登记过同一个键 = 不重复推
            if self.pending.iter().any(|p| p.items.iter().any(|(_, _, kk)| kk == k)) {
                continue;
            }
            let r = Rc::new(Reading {
                q_hash: q.hash.clone(),
                state_hash: state.hash.clone(),
                op: q.op,
                calib: q.calib.clone(),
                answer: std::cell::RefCell::new(None),
                fail: None,
                model_id: self.model_id.clone(),
                ledger_key: k.clone(),
                over_len: state.over.len(),
                perms: std::cell::Cell::new(0),
                mode_share: std::cell::Cell::new(None),
                missing_evidence: missing_evidence(state, q),
                scale: q.scale.clone(),
                state_taint: state.taint, form_hash: q.form_hash.clone(),
            });
            items.push((q.clone(), r, k.clone()));
        }
        if items.is_empty() {
            return None;
        }
        for (_, _, k) in &items {
            self.speculated.insert(k.clone());
        }
        self.pending.push(PendingJudge { state: state.clone(), items, site: sp, speculative: true });
        Some(())
    }

    /// 这个条件表达式里，有没有**来自 trusted 状态的合取项**、有没有经 `ask`（J-08）。
    ///
    /// 「合取项」按 `&&` 拆——`12`:265 说的是**合取**，所以 `||` 的两侧不算独立合取项
    /// （`a || b` 成立时不知道是哪一侧成立，不能声称 trusted 那一侧放的行）。
    ///
    /// 追来源：一个合取项最终来自哪个状态，靠的是**出口身上带的 taint**（`cut` 继承状态 taint，
    /// 前面几包刚做的）。所以这里顺着名字回到环境里的值，看它是不是由某个出口决定的。
    /// **追不到就当不可信**——兜底往拒绝那边倒。
    fn guard_of(&self, cond: &Expr, env: &Env) -> GuardInfo {
        let mut info = GuardInfo::default();
        self.walk_conjuncts(cond, env, &mut info);
        info
    }

    fn walk_conjuncts(&self, e: &Expr, env: &Env, info: &mut GuardInfo) {
        match &e.kind {
            // 合取：两侧都是独立的合取项
            ExprKind::Binary { op, left, right } if op == "&&" => {
                self.walk_conjuncts(left, env, info);
                self.walk_conjuncts(right, env, info);
            }
            // 取反、析取：里面的东西不再是「这个条件成立所保证的」，不追
            ExprKind::Unary { .. } | ExprKind::Binary { .. } => {}
            _ => {
                if let Some(src) = self.taint_source(e, env) {
                    info.trusted |= src.0;
                    info.asked |= src.1;
                }
            }
        }
    }

    fn lookup_field_prov(&self, env: &Env, name: &str, field: &str) -> Option<(bool, bool)> {
        match env_lookup(env, &field_prov_key(name, field))? {
            Value::List(l) if l.len() == 2 => match (&l[0], &l[1]) {
                (Value::Bool(t), Value::Bool(a)) => Some((*t, *a)),
                _ => None,
            },
            _ => None,
        }
    }

    /// 从 `before` 之后本帧新增的出口里折出来源（同一个字段内部**可以**取并——
    /// 那是「这个字段由哪些判断共同决定」，不是把两个字段混在一起）。
    fn provenance_since(&self, before: usize) -> Option<(bool, bool)> {
        let exits = &self.frames.last()?.exits;
        let made = &exits[before.min(exits.len())..];
        if made.is_empty() {
            return None;
        }
        Some(made.iter().fold((false, false), |acc, x| (acc.0 || x.taint == Taint::Trusted, acc.1 || x.from_ask.get())))
    }

    /// 字段的值直接引用了某个已有绑定时，取那个绑定的来源
    fn field_provenance_of(&self, e: &Expr, env: &Env) -> Option<(bool, bool)> {
        match &e.kind {
            ExprKind::Name(_) | ExprKind::Field { .. } => self.taint_source(e, env),
            _ => None,
        }
    }

    /// 一个合取项的来源：`(来自 trusted 状态, 经过 ask)`。追不到返回 `None`。
    ///
    /// 布尔名字的来源**从环境里查**（`x\u{1f}prov`）——那是在 `let` 绑定时连同值一起绑进去的：
    /// **求值这个绑定的过程中产生了哪些出口**。出口身上带着状态的 taint（`cut` 继承，
    /// 前面几包做的），所以「这个布尔是不是由可信状态上的判断决定的」答得出来。
    fn taint_source(&self, e: &Expr, env: &Env) -> Option<(bool, bool)> {
        match &e.kind {
            ExprKind::Name(n) => {
                let v = env_lookup(env, n)?;
                match &v {
                    Value::Exit(x) | Value::Duty(x) => Some((x.taint == Taint::Trusted, x.from_ask.get())),
                    Value::Bool(_) | Value::Record(_) => match env_lookup(env, &prov_key(n)) {
                        Some(Value::List(l)) if l.len() == 2 => match (&l[0], &l[1]) {
                            (Value::Bool(t), Value::Bool(a)) => Some((*t, *a)),
                            _ => None,
                        },
                        _ => None,
                    },
                    _ => None,
                }
            }
            // **取字段先查这个字段自己的来源**；查不到才退回整条记录的。
            // 此前直接继承整条记录——于是「由可信判断决定的字段」和「由不可信判断决定的字段」
            // 是同一个来源，包一层记录再取字段就绕过了守卫那一层对析取的拒绝。
            ExprKind::Field { value, field } => {
                if let ExprKind::Name(n) = &value.kind {
                    if let Some(p) = self.lookup_field_prov(env, n, field) {
                        return Some(p);
                    }
                    // 记录里有这个字段、但它没有记到来源 = 这个字段不是判断决定的（字面量之类）
                    if let Some(Value::Record(fs)) = env_lookup(env, n) {
                        if fs.iter().any(|(k, _)| k == field) && env_lookup(env, &field_prov_key(n, field)).is_some() {
                            return None;
                        }
                    }
                }
                self.taint_source(value, env)
            }
            _ => None,
        }
    }

    /// 窗口检查（J-14 / `12`:117）。静态判不了大小，所以在**登记时**查。
    fn check_window(&mut self, state: &State, sp: Span) {
        let p = &self.calib.profile;
        // 对象槽内单段：每一段各自比，不是求和——依据说的是「单段材料」
        for m in &state.on {
            let t = m.tokens();
            if t > p.text_window {
                self.trace.warn(format!(
                    "W-window: @{} 对象槽内单段 {t} token 超已测窗口 {}（超窗的语境会接管读数，答案可能偏而无痕）",
                    sp.start, p.text_window
                ));
            }
        }
        // 槽间干扰：ctx 与 ref 求和
        let ctx: usize = state.ctx.iter().chain(&state.r#ref).map(|m| m.tokens()).sum();
        if ctx > p.json_ctx_window {
            self.trace.warn(format!(
                "W-window: @{} 语境槽 {ctx} token 超 JSON 槽已测窗口 {}（上限未测，超出即无依据）",
                sp.start, p.json_ctx_window
            ));
        }
    }

    /// 刷新点（`12` §2.2:129）：「被 `cut`、`fit`、`match`/`if`、或宿主读内容时刷新。刷新时把所有
    /// 已登记且输入就绪的 `judge` **按状态分组**、按依赖分层，**一层一次发出**」。
    ///
    /// 这里的分层是天然的：登记发生在求值途中，依赖前一条出口的判断只可能在前一次刷新**之后**
    /// 才登记得上（要拿到出口就得先 `cut`，而 `cut` 本身就是刷新点）。所以「一次刷新 = 一层」，
    /// 层内按状态哈希分组融合——与 Python `_calls_from_plans` 的「同状态哈希」同一条规则。
    fn flush(&mut self, reason: &str) -> R<()> {
        if self.pending.is_empty() {
            return Ok(());
        }
        let pending = std::mem::take(&mut self.pending);
        // 融合 pass（12 §4 序 2）：同状态、同层的题合成一次调用（P5 / 12 §10 G2）。
        // **关掉就逐题发**——这正是 §4 表里「不做会坏什么：E8 成本 +45%」那一栏要量的东西。
        let mut order: Vec<String> = vec![];
        let mut groups: HashMap<String, Vec<PendingJudge>> = HashMap::new();
        let fuse = self.passes.enabled("fuse");
        for (idx, p) in pending.into_iter().enumerate() {
            // 不融合时每道题自成一组：键上带题序，保证互不合并
            let h = if fuse { p.state.hash.clone() } else { format!("{}#{idx}", p.state.hash) };
            if !groups.contains_key(&h) {
                order.push(h.clone());
            }
            groups.entry(h).or_default().push(p);
        }
        // 超预算先丢推测的，再动真站点（推测本来就是可放弃的）
        if self.cost.calls >= self.budget.calls {
            groups.values_mut().for_each(|g| g.retain(|p| !p.speculative));
        }
        let mut layer_calls = 0u64;
        let mut layer_questions = 0usize;
        for h in &order {
            let group = groups.remove(h).expect("刚放进去的");
            let state = group[0].state.clone();
            let site = group[0].site;
            let mut items: Vec<(Rc<Question>, Rc<Reading>, String)> = group.into_iter().flat_map(|p| p.items).collect();
            if items.is_empty() {
                continue;
            }
            // 关掉融合时，同一次 judge 登记的多道题也要逐题发——否则「一状态多题」这一条
            // 仍然在融合，关掉的只是「跨登记合并」，量出来的省钱会偏小
            if !fuse && items.len() > 1 {
                let rest = items.split_off(1);
                for (q, r, k) in rest {
                    self.pending.push(PendingJudge { state: state.clone(), items: vec![(q, r, k)], site, speculative: false });
                }
            }
            // **同一个账本键只问一次。**
            //
            // 提前登记（`speculate` / `vectorize`）与真站点会登记同一个键：实测
            // `vectorize` 接上之后，`[1,1,1]` 变成 `[2,2,1]`——**同一道题付了两次钱**。
            // 这不是 `vectorize` 独有的，`speculate` 也走同一条路，只是以前没量到。
            //
            // 去重后**每个读数都要填上答案**：真站点那份和提前登记那份是两个 `Reading`
            // 对象，只填一个，另一个会停在「没有答案」上。
            let mut 首见: HashMap<String, usize> = HashMap::new();
            let mut 同键: Vec<Vec<Rc<Reading>>> = vec![];
            let mut 去重: Vec<(Rc<Question>, Rc<Reading>, String)> = vec![];
            for (q, r, k) in items.into_iter() {
                match 首见.get(&k) {
                    Some(i) => 同键[*i].push(r),
                    None => {
                        首见.insert(k.clone(), 去重.len());
                        同键.push(vec![r.clone()]);
                        去重.push((q, r, k));
                    }
                }
            }
            let items = 去重;
            self.charge(1, 0.0, site)?;
            let ask: Vec<&Question> = items.iter().map(|(q, _, _)| q.as_ref()).collect();
            let res = self.client.judge(&state, &ask).map_err(|e| Fault::Error(RtError::new(None, format!("客户端错误：{}", e.0), site)))?;
            if res.answers.len() != ask.len() {
                return err(None, "客户端返回的答案数与题数不符", site);
            }
            // 13 §5：后端已经返回 = 调用已经发生、钱已经花了。先把事实记下来，再决定要不要继续。
            self.cost.calls += 1;
            self.cost.tokens += res.tokens;
            self.cost.usd += res.cost;
            layer_calls += 1;
            layer_questions += items.len();
            let shares = res.mode_share;
            let res_perms = res.perms;
            for (idx, ((q, r, key), a)) in items.iter().zip(res.answers.into_iter()).enumerate() {
                if let Some(Some(ms)) = shares.get(idx) {
                    // perms 跟着读数走：改 K 产生**新键**而不是覆盖旧值，两边并存
                    // ——与「线重算之后已经发出的出口不改」是同一条纪律。
                    r.set_mode_share(*ms, res_perms.get(idx).copied().unwrap_or(0));
                }
                self.validate_answer(&a, q, &state, site)?;
                self.ledger.put(Entry::Judge { key: key.clone(), answer: a.clone(), tokens: res.tokens, cost: res.cost, model_id: self.model_id.clone(), call: self.cost.calls });
                self.trace.push("judge", key, false, res.cost, site, format!("「{}」", q.text));
                // **运行期写入口的产出端**（`12`:347）。挂在这里而不是挂在「有读数产生」上，
                // 是因为重放路径（`interp.rs` 的 `ledger.get` 分支）根本不经过这里——
                // **重放于是天然不重复计数**，不需要再加一个「是不是重放」的开关。
                self.evidence.push((
                    r.calib.clone(),
                    crate::effects::Sample {
                        p: match &a { Answer::Noul(p) => Some(*p), _ => None },
                        // 读数本身没有真值：真值通道是 `12`:347 未定的另一样
                        label: None,
                        perms: r.perms.get(),
                        mode_share: r.mode_share.get(),
                        mode: crate::effects::LiteralMode::default(),
                        phys: q.op.phys().to_string(),
                        // 运行期这条路上没有簇 id：**读数不知道自己属于哪个对象段**。
                        // 留 `None`，于是它只能参与「按条」的认证——而「按条」会被如实写进证书。
                        cluster: None,
                    },
                ));
                r.fill(a.clone());
                // 同键的其余读数（提前登记那些）也要填上，否则它们停在「没有答案」
                for other in 同键[idx].iter().skip(1) {
                    other.fill(a.clone());
                }
            }
            // 事实记完了再核预算：实际费用高于调用前的估计时，停的是**下一步**，不是这一步
            self.charge(0, 0.0, site)?;
        }
        if layer_calls > 0 {
            self.layers.push(Layer { reason: reason.to_string(), calls: layer_calls, questions: layer_questions });
        }
        // 关融合时拆出来的余项，接着发（它们同属这一层，只是各自一次调用）
        if !self.pending.is_empty() {
            return self.flush(reason);
        }
        Ok(())
    }

    fn validate_answer(&self, a: &Answer, q: &Question, s: &State, sp: Span) -> R<()> {
        match (a, q.op) {
            (Answer::Noul(p), Op::Test) if (0.0..=1.0).contains(p) => Ok(()),
            (Answer::Choice(v), Op::Select) if v.len() == s.over.len() => Ok(()),
            (Answer::Score(v), Op::Measure) if v.len() == q.scale.len() => Ok(()),
            _ => err(None, format!("答案形状与题不符：{:?} vs {}（over {} / 档位 {}）", a, q.op.phys(), s.over.len(), q.scale.len()), sp),
        }
    }

    /// `untested` 是 **J-15 的那一位**，与 `kind` 正交：`kind` 决定路由，它只回答
    /// 「这条路上的判据测没测过」。绝大多数出口传 `None`。
    fn new_exit(&mut self, kind: ExitKind, untested: Option<String>, op: Op, q_hash: &str, state_hash: &str, taint: Taint, sp: Span) -> Value {
        self.new_exit_from(kind, untested, op, q_hash, state_hash, taint, String::new(), sp)
    }

    /// 同上，外加「线是哪一级的」（`Exit::line_source`）。
    #[allow(clippy::too_many_arguments)]
    fn new_exit_from(&mut self, kind: ExitKind, untested: Option<String>, op: Op, q_hash: &str, state_hash: &str, taint: Taint, line_source: String, sp: Span) -> Value {
        let id = self.next_exit;
        self.next_exit += 1;
        let e = Rc::new(Exit { id, op, kind, q_hash: q_hash.into(), state_hash: state_hash.into(), taint, site: sp, from_ask: std::cell::Cell::new(false), consumed: std::cell::Cell::new(false), consumed_by: std::cell::RefCell::new(String::new()), untested, line_source, ledger_key: std::cell::RefCell::new(String::new()) });
        self.frame().exits.push(e.clone());
        Value::Exit(e)
    }

    /// `allocate` / `unsure_bound` 的入参：只收读数（J-01）。单个读数也当一条收。
    fn readings_of(&self, v: &Value, who: &str, sp: Span) -> R<Vec<Rc<Reading>>> {
        let bad = |t: &str| err(Some("J-01"), format!("{who} 只接受读数，收到 {t}：读数没有可读的值，只有它才有「离线多远」这个量"), sp);
        match v {
            Value::Reading(r) => Ok(vec![r.clone()]),
            Value::List(l) => {
                let mut out = vec![];
                for x in l.iter() {
                    match x {
                        Value::Reading(r) => out.push(r.clone()),
                        other => return bad(other.type_name()),
                    }
                }
                Ok(out)
            }
            other => bad(other.type_name()),
        }
    }

    /// **漂移告警**（`12`:649「漂移监控（无标签：读数分布偏移 + 保形覆盖跌落告警）」）。
    ///
    /// **挂在「消费这条校准记录」这个动作上，不挂在 `cut` 上。**
    /// 原来只在 `cut` 里发，而实测全集（读 `self.calib` 的位置）有三处在 `cut` 之外：
    /// `allocate`、`unsure_bound`、`delta_for`。**前两处真的在用这条线**——
    /// `uncertainty` 读 `lines_for`（`strength.rs:76`），`unsure_bound` 读
    /// **只认「上岗」记录**的 `unsure_rate`（`strength.rs:155`），而它交出去的是
    /// **J-10 的联合上界，一条语言自己承诺的保证**。读数分布移开之后那个数不再成立，
    /// 程序拿到一个**静默失效的上界**，零告警。**失败开放**，且正落在「长处」那一侧。
    ///
    /// **`delta_for` 没接进来**：它取的是档案的迟滞带宽 δ，不是线；
    /// 漂移监控管的是**这条线还成不成立**。**这是一条判断不是实测**，记在此处。
    ///
    /// **只告警，不动状态**：停岗是人下的判断（走 `put`）。一个只报不动的机制
    /// **造不出永久锁**——而复岗今天不存在，所以这一点是承重的。
    ///
    /// 每个键每次运行只报一次：**一条天天响的告警等于没有告警**。
    /// 键按「每次运行」去重，所以多个消费方共用同一个键时仍然只响一次。
    fn 报漂移(&mut self, key: &str, sp: Span) {
        if self.drift_reported.contains(key) {
            return;
        }
        if let Some(d) = self.calib.drift_of(key) {
            if d.可停岗() {
                self.drift_reported.insert(key.to_string());
                self.trace.warn(format!(
                    "W-drift: @{} 键 {key} 的近期读数分布与定线时的标注分布已经移开（KS={:.3} PSI={:.3}，参照 {} 条 / 近期 {} 条）。                         **不阻塞、也不停岗**——停岗是人下的判断（12:396 写的是告警）。修法【需接线人】：复核这条线是否还成立，要停就走 put(key, …, \"停岗\")",
                    sp.start, d.ks, d.psi, d.n_ref, d.n_recent
                ));
            }
        }
    }

    /// 这批读数各自的校准键上都查一遍漂移（`allocate` / `unsure_bound` 用）。
    fn 报漂移_批(&mut self, rs: &[Rc<Reading>], sp: Span) {
        let keys: Vec<String> = rs.iter().map(|r| r.calib.clone()).collect();
        for k in keys {
            self.报漂移(&k, sp);
        }
    }

    /// 把 `cut` 查到的一条校准记录记进账本（`Ledger::calib_used`）。库里没有的键不记：
    /// 重放时它照样查不到，照样是冷，出口一致。
    fn note_calib(&mut self, key: &str) {
        if self.ledger.calib_used.contains_key(key) {
            return;
        }
        if let Some(rec) = self.calib.records.get(key) {
            let j = serde_json::to_value(rec).unwrap_or(serde_json::Value::Null);
            let h = crate::value::hash_of(&[&j.to_string()]);
            self.ledger.calib_used.insert(key.to_string(), serde_json::json!({"hash": h, "record": j}));
        }
    }

    fn cut(&mut self, r: &Reading, calib_key: Option<&str>, sp: Span) -> R<Value> {
        let v = self.cut_inner(r, calib_key, sp)?;
        if let Value::Exit(e) = &v {
            *e.ledger_key.borrow_mut() = r.ledger_key.clone();
        }
        Ok(v)
    }

    fn cut_inner(&mut self, r: &Reading, calib_key: Option<&str>, sp: Span) -> R<Value> {
        // 刷新点（12 §2.2:129）：cut 要读答案，所以先把这一层发出去
        self.flush("cut")?;
        let key = calib_key.unwrap_or(&r.calib);
        let rec = self.calib.get(key);
        // 账本记下这次查到的记录（全文 + 哈希），只凭账本重放时据此补回当时的线
        self.note_calib(key);
        // J-16：fit 的训练集 ≠ 保形集。同源就是「拿训练数据给自己打分」，
        // 过线的那条线因此不再是独立的证据。
        if let Some(fit_name) = r.calib.strip_prefix("fit:") {
            if let Some(f) = self.fits.fits.get(fit_name) {
                if !rec.set_id.is_empty() && rec.set_id == f.trained_from {
                    return err(
                        Some("J-16"),
                        format!("fit {fit_name} 的训练集与保形集同源（{}）：不相交约束违反，过线的那条线不再是独立证据", rec.set_id),
                        sp,
                    );
                }
            }
        }
        // 12:148 判序第一步：**先 insufficient**（该题声明的决定性证据槽不在状态里 → 不信任 p，J-09）。
        // 它拦的是「模型对一道没有证据可依的题照样给出一个自信的 p」——那个 p 会照常过线变成 Act。
        // 这是唯一一处**在看 p 之前**就把它挡住的检查，所以排在 taint 与过线之前。
        if let Some(missing) = r.missing_evidence.first() {
            return Ok(self.new_exit(ExitKind::Unsure(format!("insufficient:{missing}")), None, r.op, &r.q_hash, &r.state_hash, r.state_taint, sp));
        }
        // 12:150「出口 taint 继承状态 taint」、§2.11「cut 继承」。
        // 状态的 taint 由 `State::new` 折算好（on/ctx/ref/over 取并），读数带着它过来。
        let taint = r.state_taint;
        // **两件正交的事一起算出来**：`kind` 是路由键，`untested` 是 J-15 的那一位。
        // 它们分开的理由见 `Exit::untested` 的注释——合进 `cause` 就退化成
        // `cold`/`no_perm`/`no_ece`/`no_klimit` 那条已经走过一次并且停了的路。
        //
        // `untested` 这里带的是**一对**：载体名 + 这一格的修法提示。**修法跟着载体走，
        // 不靠「记得去告警那边加一个分支」**——否则就是把「漏加路由的失效方式是静默的」
        // 在这个专为消除它而建的机制内部再造一遍（漏加只会少一句修法，不会报错）。
        // **线从哪一级来**（`12`:136「题级样本不够时用模式级校准做先验收缩」的查找那一半）。
        //
        // **只有「上岗」才有线**——这是 `lines_for` 一直以来的口径，在这里显式化。
        // 冷、待真值都算「这道题没有自己的线」：**待真值尤其要说清**，它是「证据积累中、
        // 真值还没到」，`get` 给它合成的 `0.65/0.35` **是缺省值不是线**，
        // 拿它当线用就是替不确定做了乐观的默认。
        //
        // 查不到题级，就查这一类（`phys` + `literal_mode`）。**查得到也必须留痕**：
        // 模式级的线不能冒充题级的线。
        let (线, 线源): (Option<(f64, f64)>, String) = if rec.status == "上岗" {
            // **两件正交的事，不许挤进一个字段。**
            // 「题级 / 模式级」答的是**哪一层**；「手填 / 证书」答的是**凭什么**。
            // 第一版我拿后者盖掉了前者，层级信息就没了——`题级有线时用自己的` 当场红。
            (Some((rec.hi, rec.lo)), format!("题级·{}", 凭据(&rec)))
        } else if rec.status == "停岗" {
            (None, String::new())
        } else if r.calib.starts_with("fit:") {
            // **`fit` 的结果不借模式级先验。** `cut(fit结果)` 不带第二参时 `key` 就是
            // `fit:{名}`，而那条记录几乎从不上岗（fit 的校准住在 `error_rate` 里，不是一条线）。
            // 掉到 `mode_key("noul", …)` 上就是**跨种借线**——fit 的可靠性与「裸 noul 判断
            // 这一类的可靠性」毫无关系。**这正是模式键按 `phys` 分格要避免的那件事
            // 从另一道门进来**：`fit` 的 `op.phys()` 是它输入的物理形式，不是它自己的。
            (None, String::new())
        } else if r.fail.is_some() {
            // Fail 读数根本走不到过线比较；这里先算线只会白告警一句「借用了模式级先验」，
            // 而它其实什么也没借。**与「没用上线的出口不留来源」同一条**——
            // 审计物上留一句没发生的事，和留一个没用上的来源是同一种假话。
            (None, String::new())
        } else if let Some(fk) = r.form_hash.as_ref().map(|h| crate::effects::CalibStore::form_key(h)).filter(|fk| {
            self.note_calib(fk);
            self.calib.get(fk).status == "上岗"
        }) {
            // **题式级**（B2 待裁，本版只作回退层）：题键没有上岗记录，而这道题由一个
            // 有上岗记录的题式填出。**留痕**：题式线不冒充题级线。
            let f = self.calib.get(&fk);
            self.trace.warn(format!(
                "W-form-line: @{} 题级校准键 {key} 无上岗记录，用题式级线 {}（n={}）；出口带 line_source=题式级",
                sp.start, fk.trim_start_matches('\u{1f}').replace('\u{1f}', ":"), f.n
            ));
            if let Some(t) = f.truth.as_ref().filter(|t| t.gate.starts_with("临时上岗")) {
                self.trace.warn(format!("W-provisional: @{} 这条题式级线是{}", sp.start, t.gate));
            }
            (Some((f.hi, f.lo)), format!("题式级·{}", 凭据(&f)))
        } else {
            if let Some(fk) = r.form_hash.as_ref().map(|h| crate::effects::CalibStore::form_key(h)) {
                let f = self.calib.get(&fk);
                if let Some(t) = &f.truth {
                    // 真值通道导入过、但没过上岗门的题式：**说出来**，不静默当冷键
                    self.trace.warn(format!("W-form-pending: @{} 题式级记录未上岗（{}）；本题按冷键处理", sp.start, t.gate));
                }
            }
            let mk = crate::effects::CalibStore::mode_key(r.op.phys(), crate::effects::LiteralMode::default());
            self.note_calib(&mk);
            let m = self.calib.get(&mk);
            if m.status == "上岗" {
                self.trace.warn(format!(
                    "W-mode-prior: @{} 题级校准键 {key} 无上岗记录，借用模式级先验 {mk}（n={}）；出口带 line_source=模式级",
                    sp.start, m.n
                ));
                (Some((m.hi, m.lo)), format!("模式级·{}", 凭据(&m)))
            } else {
                (None, String::new())
            }
        };
        let (kind, untested): (ExitKind, Option<(String, String)>) = if let Some(f) = &r.fail {
            (ExitKind::Unsure(format!("fail:{f}")), None)
        } else if rec.status == "停岗" {
            // `drift` **不是**未测：停岗是「测过、而且测出漂了」。两者取值相反，别顺手合并。
            //
            // **停岗在这里提前返回，模式级回退够不着它。** 停岗是人下的判断（这条线不能再用了），
            // 回退到一个类级先验把它放行，是彻头彻尾的假放行。
            (ExitKind::Unsure("drift".into()), None)
        } else if 线.is_none() {
            // **题级没上岗、模式级也没上岗** → 还是冷。回退没有把所有冷键都放行。
            (ExitKind::Unsure("cold".into()), Some(("calib_line".into(), "修法【作者可改】：给这道题的校准键写一条上岗记录，或给这一类（phys + literal_mode）写一条模式级记录".into())))
        } else {
            let (hi, lo) = 线.expect("刚判过");
            match &r.answer_after_flush().expect("刷新之后答案必然在") {
                Answer::Noul(p) => {
                    // **±δ 那条带**（`12`:167「再过线，再 `band`（**线附近 ±δ**）」）。
                    //
                    // 这里原来是裸的 `p >= hi` / `p <= lo`——**整条 δ 带缺失**，
                    // 于是线的邻域里本该 `Unsure` 的读数拿到了强出口。**那是失败开放。**
                    //
                    // 发现方式值得记：不是读代码读出来的，是**拿 Python 的 `_decide`
                    // 与这一段并排看**出来的（`runtime.py`：`p >= hi + delta` /
                    // `p <= lo - delta`）。**跨内核对照第一次真的照出东西，而且是 $0。**
                    //
                    // E-JPP-LIVE 那次真机读数 `p = 0.56`、`lo = 0.56`、`δ = 0.04`：
                    // **Rust 给 `Ignore`，Python 给 `Unsure(band)`——那一条实测数据本身就分岔。**
                    let delta = match (线源.starts_with("题式级"), r.form_hash.as_ref()) {
                        (true, Some(h)) => self.calib.delta_for(&self.calib.get(&crate::effects::CalibStore::form_key(h)), r.op),
                        _ => self.calib.delta_for(&rec, r.op),
                    };
                    if *p >= hi + delta {
                        (ExitKind::Act, None)
                    } else if *p <= lo - delta {
                        (ExitKind::Ignore, None)
                    } else {
                        (ExitKind::Unsure("band".into()), None)
                    }
                }
                Answer::Choice(v) => {
                    // 12:151：Pick **要求置换众数一致**（profile.position_bias）；不一致 → Unsure(tie)。
                    // 置换不一致 = 换一下候选的排列顺序模型就选了别的，那时的 k 是**位置效应不是答案**。
                    //
                    // `mode_share == None` 表示这条路上**没测过置换**（K-noul 就是：它把一道 select
                    // 拆成 K 道独立的 noul 再取 argmax，候选顺序根本不参与）。没测过就不能声称通过
                    // ——给 Pick 等于拿一个从未做过的检查当成过了。`12` 对这一格没写，这是 core 的判断。
                    let (k, p) = argmax(v);
                    match r.mode_share.get() {
                        // **没测过**（J-15，加宽后的措辞：「引用任何被声明为判据、但在本次路径上
                        // 没有被测量的量」）。`mode_share == None` 本来就是它的实例，只是原措辞
                        // 绑在「档案字段」上所以没人认出来。
                        //
                        // 它**不是 `tie`**：`tie` 的语义是「测了，不一致」。两者路由不同——
                        // `tie` 的既定去向是「逐候选 noul」，而没测过的那条路（K-noul）**本来就是
                        // 逐候选 noul，路过去是空转**。而且两者在**账本**里不能是同一个值：
                        // 账本是审计物，这与「兜底档案的 hash 必须是 None 而不是兜底值的哈希」同源。
                        //
                        // **载体二：置换未测。** 这一格与 `cold` 不同：§5 没有一条既有路由
                        // 可骑。`tie` 被 `12`:151 指派给「测了，不一致」，骑过去就是让 `tie`
                        // 兼职；`band` 说的是 p 落在带内，而这里 p 可能远在 `hi` 之上，写进
                        // 审计物就是一句假话。所以给**通用 cause `untested`**——它是**一条**
                        // 路由（五个载体共用），不是「每个载体一条」。
                        // **这一格 `12` 没写，是 core 的判断，已提请总控裁（见 INTERFACE §J-15）。**
                        None => (
                            ExitKind::Unsure("untested".into()),
                            Some(("permutation".into(), "修法【需接线人】：开置换要在 Rust 侧设 JevClient.permute = true（select 的调用数 ×2）——**`.jpp` 作者改不了这一项**".into())),
                        ),
                        // 测了，不一致
                        Some(ms) if ms < 1.0 => (ExitKind::Unsure("tie".into()), None),
                        // 测了，一致
                        Some(_) => {
                            if p >= hi {
                                (ExitKind::Pick(k), None)
                            } else {
                                (ExitKind::Unsure("band".into()), None)
                            }
                        }
                    }
                }
                Answer::Score(v) => {
                    let (l, p) = argmax(v);
                    if p >= hi { (ExitKind::At(l), None) } else { (ExitKind::Unsure("band".into()), None) }
                }
            }
        };
        // 每条既有路由各加一句「若该量未测，取保守项并**告警**」（`12` §2.11）。
        // `cold` 本来就取保守线，缺的是那句告警——**没有告警，「用了保守线」与「线本来就这么宽」
        // 在痕迹上分不开**，跟兜底档案 `hash` 必须是 `None` 是同一条。
        //
        // **就这一处告警**，所有载体走同一条。新增载体只要在上面的分支里带上修法提示，
        // 这里不需要动；忘了带也只是少一句提示，不会漏掉告警本身。
        if let Some((carrier, 修法)) = &untested {
            self.trace.warn(format!(
                "W-untested: @{} {carrier} 在本次路径上没有被测量，按 J-15 取保守项（出口 {}）。{修法}",
                sp.start,
                match &kind { ExitKind::Unsure(c) => c.clone(), other => format!("{other:?}") }
            ));
        }
        // **没用上线的出口不留来源**（fail / 冷 / 停岗）：留一个来源就是谎称有线。
        let 留痕 = if matches!(kind, ExitKind::Unsure(ref c) if c == "cold" || c == "drift" || c.starts_with("fail:")) { String::new() } else { 线源 };
        // **强出口建在一条未经认证的线上，要出告警。**
        //
        // 判据是 `certs.is_empty()`，**不是 `n` 的大小**：三个出货示例全部拿到强出口，
        // 用的是 `n = 1` 的手填线、零告警——**同一个字段 `n`，一条路上 1 就够，
        // 另一条路上 22 还不够，中间没有任何东西把这个差别说出来**。
        // 说出那个差别的是「有没有证书」，不是那个数。
        self.报漂移(key, sp);
        // **标签来源可疑的证书给出强出口也要留痕。**
        //
        // 与 `bounded_side` **不告警**那条的分界：`bounded_side` 今天只有一个取值，
        // **一条在每个已认证键上都响的告警承载零信息**；而 `label_source` 区分得开记录，
        // 它响的时候是在说一件**别的记录不成立**的事。
        //
        // **这条告警的正确稳态是「消失」**——声明过就不响了。一条稳态为零的告警，
        // 与一条永久噪声不是一回事。
        if matches!(kind, ExitKind::Act | ExitKind::Pick(_) | ExitKind::At(_)) {
            if let Some(c) = rec.选中的证书() {
                if c.label_source.可疑() {
                    self.trace.warn(format!(
                        "W-label-source: @{} 键 {key} 的证书没说清标签是怎么选的（{:?}），而这里给出了强出口。                         若标签集是一个被选出来的子集、而选择判据与「对不对」相关，**这张证书上的每个数都同向有偏**。                         修法【需接线人】：set_label_source 是 Rust API，`.jpp` 作者调不到",
                        sp.start, c.label_source
                    ));
                }
            }
        }
        if 留痕.ends_with("·手填") && matches!(kind, ExitKind::Act | ExitKind::Pick(_) | ExitKind::At(_)) {
            self.trace.warn(format!(
                "W-uncertified: @{} 键 {key} 的线是手填的、没有保形证书（n={}），而这里给出了强出口 {}。不阻塞。修法【需接线人】：`commission` 是 Rust API，`.jpp` 作者调不到——要凭据得由接线人跑一次认证",
                sp.start, rec.n,
                match &kind { ExitKind::Pick(k) => format!("pick({k})"), other => format!("{other:?}") }
            ));
        }
        Ok(self.new_exit_from(kind, untested.map(|(carrier, _)| carrier), r.op, &r.q_hash, &r.state_hash, taint, 留痕, sp))
    }

    fn handle(&mut self, e: &Rc<Exit>, arms: &Value, sp: Span) -> R<Value> {
        let Value::Record(_) = arms else { return err(Some("J-05"), "handle 的第二个参数是记录 {act, ignore, pick, at, unsure, otherwise}", sp) };
        let required: &[&str] = match e.op {
            Op::Test => &["act", "ignore", "unsure"],
            Op::Select => &["pick", "unsure"],
            Op::Measure => &["at", "unsure"],
        };
        let has_other = arms.get("otherwise").is_some();
        // 语法覆盖约束：Unsure 必须有显式的 unsure 臂，通配分支不能代替它。
        // 其余去向可以由 otherwise 兜底。
        let missing: Vec<&str> = required.iter().copied().filter(|k| arms.get(k).is_none() && (*k == "unsure" || !has_other)).collect();
        if !missing.is_empty() {
            return err(
                Some("J-05"),
                format!(
                    "handle 不穷尽：{} 题的出口缺分支 {}。修法：补上；注意 unsure 必须自己写一臂，otherwise 兜不住未决责任",
                    e.op.phys(),
                    missing.join(", ")
                ),
                sp,
            );
        }
        if e.is_unsure() {
            let arm = arms.get("unsure").unwrap();
            // **unsure 臂也要压守卫。** 那里拿到的是未决责任，**本来就不是一个放行判定**，
            // 所以它不提供 `trusted` 合取项。但**不压就是空栈 → 整条 J-08 跳过 →
            // 臂里的不可逆 `do` 自由执行**，与这次修的是同一个洞。
            self.guards.push(守卫(e));
            let r = self.handle_unsure(e, &arm, sp);
            self.guards.pop();
            return r;
        }
        let (name, arg): (&str, Value) = match &e.kind {
            ExitKind::Act => ("act", Value::Unit),
            ExitKind::Ignore => ("ignore", Value::Unit),
            ExitKind::Pick(k) => ("pick", Value::Int(*k as i64)),
            ExitKind::At(l) => ("at", Value::Int(*l as i64)),
            ExitKind::Unsure(_) => unreachable!("上面已经分流"),
        };
        let arm = arms.get(name).or_else(|| arms.get("otherwise")).unwrap();
        e.consumed.set(true);
        *e.consumed_by.borrow_mut() = format!("handle:{name}");
        match arm {
            Value::Fn(c) => {
                let n = c.function.parameters.len();
                let args = if n == 0 { vec![] } else { vec![arg] };
                // **这一臂之所以执行，正是因为那次 `cut` 切出了这个出口——那个出口就是守卫。**
                // 以前 `guards` 只在 `if` 处 push，于是写在 handler 臂里的不可逆 `do`
                // 在 J-08 眼里是「无条件执行」，完全不受管——**而那正是 §5 与 J-05 推荐的写法**。
                // `act`/`ignore`/`pick`/`at` 全走这里，**漏掉任何一个就是把同一个洞挪过去一个分支**。
                self.guards.push(守卫(e));
                let r = self.call_closure(&c, args, sp);
                // **出错路径也要弹**：早返回会把脏栈留给这次运行的其余部分。
                self.guards.pop();
                r
            }
            other => Ok(other),
        }
    }

    /// unsure 臂收到的是**未决责任本身**（`Value::Duty`），不是原因文本。臂体跑完再核它是不是真的
    /// 交出去了：escalate 给人、字面化重问、包装进返回值、或显式 drop。什么都不做就是静默丢弃（J-05）。
    fn handle_unsure(&mut self, e: &Rc<Exit>, arm: &Value, sp: Span) -> R<Value> {
        let Value::Fn(c) = arm else {
            return err(
                Some("J-05"),
                format!(
                    "unsure 的臂是个 {}，收不下未决责任。修法：写成 unsure: fn(u) {{ … }}，在体内 escalate(u, …) / literalize(u, …) / consume(u, \"drop\")，或把 u 包进返回值交给调用者",
                    arm.type_name()
                ),
                sp,
            );
        };
        if c.function.parameters.is_empty() {
            return err(Some("J-05"), "unsure 的臂没有参数，接不到未决责任。修法：写成 fn(u) { … }", c.span);
        }
        let site = c.span;
        let result = self.call_closure(c, vec![Value::Duty(e.clone())], sp)?;
        // escalate / literalize / consume 已经在臂体里销过账
        if e.consumed.get() {
            return Ok(result);
        }
        let mut carried = HashSet::new();
        collect_exit_ids(&result, &mut carried);
        if carried.contains(&e.id) {
            // 13 §3：包进返回值是**转交**，不是了结。这里不销账——责任继续挂着，
            // 交给函数返回检查与程序结束前检查去核。那两处把两件事分开：
            // 责任**真被丢了**是错；责任**如实交了出去、只是签名没说**是警告（见 call_closure）。
            *e.consumed_by.borrow_mut() = "handle:unsure(转交调用者)".into();
            return Ok(result);
        }
        err(
            Some("J-05"),
            format!(
                "unsure 的臂把未决责任丢了：{} 既没 escalate、没重问、没 drop，也没进返回值。进臂不等于销账。修法：escalate(u, state, 题) 交给人，literalize(u, state, 更字面的题) 重问，consume(u, \"drop\") 显式丢并记账，或把 u 放进返回值",
                e.label()
            ),
            site,
        )
    }

    fn do_(&mut self, name: &str, args: &[Value], iter_seq: i64, sp: Span) -> R<Value> {
        let action = self.actions.actions.get(name).cloned().ok_or_else(|| Fault::Error(RtError::new(Some("J-11"), {
            let mut 表: Vec<String> = self.actions.actions.iter()
                .map(|(n, a)| format!("{n}（{}）", if a.reversible { "可逆" } else { "**不可逆**" }))
                .collect();
            表.sort();
            // **J-08 保护的是不可逆动作，而作者此前没有任何办法知道哪些动作不可逆。**
            // 一个作者无法查询的安全边界，等于没有边界。这里顺手把它变成可查的。
            format!("动作 {name} 未登记：do 只能触发登记过的动作（register）。本次登记了：{}", 表.join("、"))
        }, sp)))?;
        let args_canon: Vec<String> = args.iter().map(|a| canon(&a.to_json())).collect();
        let key = effect_key("do", &[&sp.start.to_string(), name, &args_canon.join("\u{1f}"), &iter_seq.to_string()]);
        if let Some(Entry::Effect { output, .. }) = self.ledger.get(&key) {
            self.cost.replayed += 1;
            self.trace.push("do", &key, true, 0.0, sp, name.into());
            return Ok(json_to_effect_value(output));
        }
        // J-08（12:265）：放行**不可逆** do 的守卫表达式中，至少一个合取项来自 taint=trusted
        // 的状态；untrusted 项的数量不改变这一要求；或经 ask。
        //
        // 「守卫表达式」在一个有 if 的语言里就是**包着这个 do 的那些条件**——不必是 do 的一个参数。
        // `00-宪法.md:44` 说 IFC 的纪律只有这一条：不可信材料上的判断不得单独放行不可逆 do。
        //
        // 不查的：**这个 trusted 是不是真的可信**（12:649 Nature 裁定：taint_out="trusted" 是作者的
        // 显式标记，语言保证它可见可追，**不设审核方**）。所以这里问的是「守卫里有没有一个 trusted
        // 合取项」，不是「那个 trusted 配不配」。
        //
        // 无条件执行的 do 不受管：它没有守卫可查，作者直接写 do 是他自己的决定。
        if !action.reversible && !self.guards.is_empty() {
            let 放行 = self.guards.iter().any(|g| g.trusted || g.asked);
            if !放行 {
                return err(
                    Some("J-08"),
                    format!(
                        "不可逆动作 {name} 的守卫里没有一个来自可信状态的合取项：不可信材料上的判断不得**单独**放行不可逆动作（宪法 IFC / 12 §5 J-08）。修法：在条件里再合取一个来自 trusted 状态的判断，或改走 ask 让人拍板，或把这个动作登记成可逆"
                    ),
                    sp,
                );
            }
        }
        self.charge(0, action.cost, sp)?;
        // 入参 taint 要**递归看容器**：材料嵌在记录字段或嵌套列表里时，顶层 match 看不见它，
        // 以前落进 `_ => Trusted`，于是 Inherit 的动作拿到「入参全可信」——脏材料喂进 do 出来就干净了。
        // 与 J-01 的 `unwrap_or(false)`、taint 反序列化兜底、`mat(content(脏))` 同一形状。
        let taint_in = args.iter().fold(Taint::Trusted, |t, a| Taint::join(t, taint_of(a)));
        let taint = match action.taint_out {
            TaintOut::Trusted => Taint::Trusted,
            TaintOut::Untrusted => Taint::Untrusted,
            TaintOut::Inherit => taint_in,
        };
        let out = match (action.f)(args) {
            // `derived_from` 与 taint 一样要**折算**，不是恒清零：`do(…, [m])` 的产物
            // 当然仍派生自 m 那道题。**这是保持同一跳，不是增加一跳**（`12` J-02 范围裁定）。
            // 闭包在 `as_mat(exit)` 那里自然截断——做成传递闭包会重演「逐字传播让几乎所有
            // 输出不可用」（宪法第 44 行），材料越传越「派生自所有题」，J-02 最后拦住一切。
            Ok(v) => Value::Mat(Rc::new(Mat::new(v.to_json(), &format!("do:{name}"), vec![format!("do:{key}")], taint, derived_of(args)))),
            Err(msg) => Value::Fail(Rc::from(format!("{name}: {msg}").as_str())),
        };
        self.cost.usd += action.cost;
        self.ledger.put(Entry::Effect { key: key.clone(), kind: "do".into(), output: effect_value_to_json(&out), cost: action.cost });
        self.trace.push("do", &key, false, action.cost, sp, name.into());
        Ok(out)
    }

    fn generate(&mut self, prompt: &str, ctx: &[Mat], n: usize, retry_seq: i64, sp: Span) -> R<Value> {
        let ctx_hash: Vec<&str> = ctx.iter().map(|m| m.hash.as_str()).collect();
        // 12:158「键：(site, prompt_hash, ctx_hash, n, retry_seq)」——site 排第一位。
        // 缺了它，同一段 prompt 在两个站点生成会撞键，第二个站点命中第一个的输出。
        // gen 比 judge 更容易撞：prompt 常是字面量，两处写同一句话很正常。
        let key = effect_key("gen", &[&sp.start.to_string(), prompt, &ctx_hash.join(","), &n.to_string(), &retry_seq.to_string()]);
        let taint = ctx.iter().fold(Taint::Trusted, |t, m| Taint::join(t, m.taint));
        // 与 do 同：并入 ctx 各材料的 derived_from（保一跳）
        let derived: BTreeSet<String> = ctx.iter().flat_map(|m| m.derived_from.iter().cloned()).collect();
        let wrap = |outs: &[Json]| Value::list(outs.iter().map(|o| Value::Mat(Rc::new(Mat::new(o.clone(), &format!("gen:{prompt}"), vec![format!("gen:{key}")], taint, derived.clone())))).collect());
        if let Some(Entry::Effect { output, .. }) = self.ledger.get(&key) {
            let outs: Vec<Json> = output.as_array().cloned().unwrap_or_default();
            self.cost.replayed += 1;
            self.trace.push("gen", &key, true, 0.0, sp, prompt.into());
            return Ok(wrap(&outs));
        }
        self.charge(1, 0.0, sp)?;
        let ctx_json: Vec<Json> = ctx.iter().map(|m| m.content.clone()).collect();
        let res = self.client.generate(prompt, &ctx_json, n, retry_seq as u64).map_err(|e| Fault::Error(RtError::new(None, format!("gen 失败：{}", e.0), sp)))?;
        // 13 §5：后端已经返回 = 钱已经花了。先记事实（费用、token、账本），再核预算决定下一步。
        self.cost.calls += 1;
        self.cost.tokens += res.tokens;
        self.cost.usd += res.cost;
        self.ledger.put(Entry::Effect { key: key.clone(), kind: "gen".into(), output: Json::Array(res.outputs.clone()), cost: res.cost });
        self.trace.push("gen", &key, false, res.cost, sp, prompt.into());
        self.charge(0, 0.0, sp)?;
        Ok(wrap(&res.outputs))
    }

    fn ask(&mut self, state: &Rc<State>, q: &Rc<Question>, sp: Span) -> R<Value> {
        let key = effect_key("ask", &[&state.hash, &q.hash]);
        let answer = if let Some(Entry::Ask { answer, .. }) = self.ledger.get(&key) {
            answer.clone()
        } else {
            let limit = self.budget.escalate.unwrap_or(0);
            // 已问过的（账本里的）+ 这次运行新问的，一起核总上限
            if self.asks_in_ledger + self.cost.asks >= limit {
                return Err(Fault::Halt(Pending { cause: "budget.escalate".into(), key, site: sp, detail: format!("ask 次数已到上限 {limit}") }));
            }
            self.cost.asks += 1;
            let a = self.client.ask(state, q).map_err(|e| Fault::Error(RtError::new(None, format!("ask 失败：{}", e.0), sp)))?;
            if a.is_some() {
                self.ledger.put(Entry::Ask { key: key.clone(), answer: a.clone() });
            }
            a
        };
        match answer {
            Some(a) => {
                self.trace.push("ask", &key, false, 0.0, sp, format!("「{}」已答", q.text));
                let kind = match a {
                    Answer::Noul(p) => if p >= 0.5 { ExitKind::Act } else { ExitKind::Ignore },
                    Answer::Choice(v) => ExitKind::Pick(argmax(&v).0),
                    Answer::Score(v) => ExitKind::At(argmax(&v).0),
                };
                let e = self.new_exit(kind, None, q.op, &q.hash, &state.hash, Taint::Trusted, sp);
                if let Value::Exit(x) = &e {
                    x.from_ask.set(true); // 12:265「或经 ask」；人答是 trusted（§2.11）
                }
                Ok(e)
            }
            None => {
                self.trace.push("ask", &key, false, 0.0, sp, format!("「{}」未答 → Pending", q.text));
                Err(Fault::Halt(Pending { cause: "ask".into(), key, site: sp, detail: format!("等人回答「{}」", q.text) }))
            }
        }
    }

    /// 捕获状态的指纹（13 §4）。`None` = 这个环境**指纹化不了**，调用方应当禁用跨运行缓存。
    ///
    /// 不无限展开：嵌套方法到 `depth` 就返回 `None`；捕获里有读数、出口、未决责任时也返回 `None`
    /// ——它们要么没有可读的值，要么带着尚未了结的义务，不该参与「结果可复用」的判断。
    fn env_fingerprint(&self, env: &Env, names: &BTreeSet<String>, depth: u32) -> Option<String> {
        let mut parts: Vec<String> = vec![];
        for n in names {
            let Some(v) = env_lookup(env, n) else { continue };
            match &v {
                // 内置名稳定，不进指纹
                Value::Builtin(_) => continue,
                Value::Fn(c) => {
                    if depth == 0 {
                        return None;
                    }
                    let inner = self.env_fingerprint(&c.env, &referenced_names(&c.function), depth - 1)?;
                    parts.push(format!("{n}=fn:{}:{inner}", c.hash));
                }
                Value::Reading(_) | Value::Exit(_) | Value::Duty(_) | Value::State(_) | Value::Question(_) => return None,
                other => parts.push(format!("{n}={}", canon(&other.to_json()))),
            }
        }
        Some(hash_of(&parts.iter().map(|s| s.as_str()).collect::<Vec<_>>()))
    }

    fn transform(&mut self, f: &Rc<Closure>, args: &[Value], sp: Span) -> R<Value> {
        let mut mats = vec![];
        for a in args {
            mats.push(self.as_mat(a, "transform", sp)?);
        }
        let hashes: Vec<&str> = mats.iter().map(|m| m.hash.as_str()).collect();
        // 13 §4：可复用结果的身份不能只取代码正文——工厂造出来的两个方法正文相同、捕获不同时，
        // 只按正文就会把前一个的结果复用给后一个（**算错**）。身份 = 代码哈希 + 实际捕获状态的指纹。
        // 指纹取不到（捕获里有读数/出口/嵌套太深）时**禁用这一项的跨运行缓存**，照常执行；
        // 宁可不缓存，也不返回另一个方法的结果。
        let captured = self.env_fingerprint(&f.env, &referenced_names(&f.function), 3);
        let taint = mats.iter().fold(Taint::Trusted, |t, m| Taint::join(t, m.taint));
        // 保一跳：`transform(f, m)` 的产物仍派生自 m 那道题。此前恒清零——
        // 一次恒等变换 `transform(fn(x){content(x)}, m)` 就洗掉 J-02 的禁自指。
        let derived: BTreeSet<String> = mats.iter().flat_map(|m| m.derived_from.iter().cloned()).collect();
        let Some(captured) = captured else {
            // **这句原来只说「不进跨运行缓存」，而它少说了一半**：这条路
            // **在 `ledger.put` 之前就返回了**，于是这份材料**根本不进账本**——
            // 而账本是今天唯一存着效应输出内容的地方。`12`:182 说「`gen`/`do`/`transform`
            // 的输出**默认入库**」，**对这条路是假的**，跨会话也就取不回来。
            self.trace.warn(format!(
                "W-no-cache: transform 的方法捕获环境指纹化不了（{}），这一项不进跨运行缓存；照常执行，不复用别人的结果。**它也不进账本**——账本是今天唯一存着效应输出内容的地方，所以这份材料跨会话取不回来（12:182「输出默认入库」对这条路不成立）",
                f.name.clone().unwrap_or_else(|| "匿名方法".into())
            ));
            let v = self.call_closure(f, mats.iter().map(|m| Value::Mat(Rc::new(m.clone()))).collect(), sp)?;
            if matches!(v, Value::Reading(_) | Value::Exit(_) | Value::Fn(_) | Value::State(_)) {
                return err(Some("J-11"), format!("transform 的输出要能成材料，收到 {}", v.type_name()), sp);
            }
            let content = match &v { Value::Mat(m) => m.content.clone(), other => other.to_json() };
            return Ok(Value::Mat(Rc::new(Mat::new(content, "transform", vec!["transform:uncached".to_string()], taint, derived))));
        };
        // 12:189「键 (site, f_hash, args_hash)」。`captured` 是 13 §4 另加的（身份含捕获状态）。
        let key = effect_key("transform", &[&sp.start.to_string(), &f.hash, &captured, &hashes.join(",")]);
        if let Some(Entry::Effect { output, .. }) = self.ledger.get(&key) {
            self.cost.replayed += 1;
            self.trace.push("transform", &key, true, 0.0, sp, String::new());
            return Ok(Value::Mat(Rc::new(Mat::new(output.clone(), "transform", vec![format!("transform:{key}")], taint, derived))));
        }
        let v = self.call_closure(f, mats.iter().map(|m| Value::Mat(Rc::new(m.clone()))).collect(), sp)?;
        if matches!(v, Value::Reading(_) | Value::Exit(_) | Value::Fn(_) | Value::State(_)) {
            return err(Some("J-11"), format!("transform 的输出要能成材料，收到 {}", v.type_name()), sp);
        }
        let content = match &v { Value::Mat(m) => m.content.clone(), other => other.to_json() };
        self.ledger.put(Entry::Effect { key: key.clone(), kind: "transform".into(), output: content.clone(), cost: 0.0 });
        self.trace.push("transform", &key, false, 0.0, sp, String::new());
        Ok(Value::Mat(Rc::new(Mat::new(content, "transform", vec![format!("transform:{key}")], taint, derived))))
    }

    fn loop_(&mut self, bound: i64, init: Value, step: &Value, sp: Span) -> R<Value> {
        if bound <= 0 {
            return err(Some("J-06"), format!("loop 的 bound 必须是正整数，收到 {bound}"), sp);
        }
        let Value::Fn(step) = step else { return err(None, "loop(bound, init, step) 的 step 要是函数 fn(acc, i)", sp) };
        self.loops.push(LoopCtx { seen_keys: HashSet::new(), repeated: None });
        let mut acc = init;
        let mut result = None;
        for i in 0..bound {
            let out = self.call_closure(step, vec![acc.clone(), Value::Int(i)], sp);
            let out = match out {
                Ok(v) => v,
                Err(e) => {
                    self.loops.pop();
                    return Err(e);
                }
            };
            match out {
                Value::Stop(v) => {
                    result = Some((*v).clone());
                    break;
                }
                v => acc = v,
            }
            if let Some(k) = self.loops.last().and_then(|l| l.repeated.clone()) {
                self.trace.warn(format!("W-noprogress: 第 {} 轮重复了账本键 {}，循环停止（J-06 键重复即停）", i + 1, 头(&k, 8)));
                break;
            }
        }
        self.loops.pop();
        if result.is_none() {
            self.trace.warn(format!("W-bound: loop 到 bound={bound} 仍未 stop"));
        }
        Ok(result.unwrap_or(acc))
    }

    // ---------- 内置 ----------

    fn builtin(&mut self, name: &'static str, args: Vec<Value>, sp: Span) -> R<Value> {
        let n = args.len();
        let arity = |k: usize| -> R<()> { if n == k { Ok(()) } else { err(None, format!("{name} 需要 {k} 个参数，收到 {n}"), sp) } };
        match name {
            "state" => self.make_state(&args, sp),
            "test" | "select" => {
                if n != 2 && n != 3 {
                    return err(None, format!("{name} 需要 2 或 3 个参数（题面, calib[, {{evidence: [槽名…]}}]），收到 {n}"), sp);
                }
                let (Value::Text(t), Value::Text(c)) = (&args[0], &args[1]) else { return err(Some("J-03"), format!("{name}(题面: Text, calib: Text) — calib 是校准记录的键，不是线"), sp) };
                let op = if name == "test" { Op::Test } else { Op::Select };
                let evidence = evidence_of(args.get(2), sp)?;
                let mut q = Question::with_evidence(op, t, c, vec![], evidence);
                let (presupposition, request) = question_decl_of(args.get(2), op, sp)?;
                q.presupposition = presupposition;
                q.request = request;
                Ok(Value::Question(Rc::new(q)))
            }
            "measure" => {
                arity(3)?;
                let (Value::Text(t), Value::List(scale), Value::Text(c)) = (&args[0], &args[1], &args[2]) else { return err(None, "measure(题面, [档位…], calib)", sp) };
                let mut sc = vec![];
                for s in scale.iter() {
                    match s { Value::Text(x) => sc.push(x.to_string()), _ => return err(None, "档位要是 Text", sp) }
                }
                if sc.len() < 2 {
                    return err(None, "measure 至少两档", sp);
                }
                Ok(Value::Question(Rc::new(Question::new(Op::Measure, t, c, sc))))
            }
            "form" => {
                // form(题型, 模板题面, {calib, scale?, evidence?, presupposition?, request?}) → 题式
                arity(3)?;
                let (Value::Text(opname), Value::Text(template)) = (&args[0], &args[1]) else {
                    return err(None, "form(题型: \"test\" | \"select\" | \"measure\", 模板题面: Text, {calib: \"校准键\", …})", sp);
                };
                let op = match opname.as_ref() {
                    "test" => Op::Test,
                    "select" => Op::Select,
                    "measure" => Op::Measure,
                    other => return err(None, format!("form 的题型要是 test / select / measure，收到 {other}"), sp),
                };
                let Value::Record(_) = &args[2] else { return err(None, "form 的第三个参数要是记录：{calib: \"校准键\", …}", sp) };
                let calib = match args[2].get("calib") {
                    Some(Value::Text(c)) => c.to_string(),
                    Some(Value::Int(_)) | Some(Value::Float(_)) => return err(Some("J-03"), "form 的 calib 是数字：线不可字面，这一位只收校准记录的键（Text）", sp),
                    _ => return err(Some("J-03"), "form 需要 calib：{calib: \"校准键\"}。线只从校准记录来", sp),
                };
                let mut scale = vec![];
                if let Some(v) = args[2].get("scale") {
                    let Value::List(l) = v else { return err(None, "scale 要是档位列表", sp) };
                    for x in l.iter() {
                        match x { Value::Text(t) => scale.push(t.to_string()), _ => return err(None, "档位要是 Text", sp) }
                    }
                }
                match (op, scale.len()) {
                    (Op::Measure, n) if n < 2 => return err(None, "measure 题式至少两档：{scale: [\"低\", \"高\"]}", sp),
                    (Op::Test | Op::Select, n) if n > 0 => return err(None, "只有 measure 题式带 scale", sp),
                    _ => {}
                }
                let evidence = evidence_of(Some(&args[2]), sp)?;
                let (presupposition, request) = question_decl_of(Some(&args[2]), op, sp)?;
                let f = crate::value::Form::new(op, template, &calib, scale, evidence, presupposition, request).map_err(|m| Fault::Error(RtError::new(None, m, sp)))?;
                Ok(Value::Form(Rc::new(f)))
            }
            "fill" => {
                // fill(题式, {槽: 值, …}) → 题。值按 text() 渲染；Int/Float/Bool/Text 以外的值不能填进题面。
                arity(2)?;
                let Value::Form(f) = &args[0] else { return err(None, format!("fill 的第一个参数要是题式（form(…) 的结果），收到 {}", args[0].type_name()), sp) };
                let Value::Record(fields) = &args[1] else { return err(None, "fill 的第二个参数要是记录：{槽名: 值}", sp) };
                let mut fill = vec![];
                for (k, v) in fields.iter() {
                    let t = match v {
                        Value::Text(t) => t.to_string(),
                        Value::Int(i) => i.to_string(),
                        Value::Float(x) => x.to_string(),
                        Value::Bool(b) => b.to_string(),
                        Value::Reading(_) => return err(Some("J-01"), format!("槽 {k} 填的是读数：读数不能进题面（它不是材料，也不可渲染）"), sp),
                        other => return err(None, format!("槽 {k} 要填 Text / Int / Float / Bool，收到 {}", other.type_name()), sp),
                    };
                    fill.push((k.clone(), t));
                }
                let q = f.fill(&fill).map_err(|m| Fault::Error(RtError::new(None, m, sp)))?;
                Ok(Value::Question(Rc::new(q)))
            }
            "judge" => {
                arity(2)?;
                // 12:129 的向量化形式：`judge(ss: [State], qs) → [Readings]`「状态列表，同层并发」。
                // 同一道题问多个对象——判断向量（`order`）要的正是这个形状。
                // 它们同层登记，所以一次刷新就全发出去。
                if let Value::List(states) = &args[0] {
                    let mut qs = vec![];
                    match &args[1] {
                        Value::Question(q) => qs.push(q.clone()),
                        Value::List(l) => {
                            for q in l.iter() {
                                match q { Value::Question(q) => qs.push(q.clone()), _ => return err(None, "judge 的题列表里有非题", sp) }
                            }
                        }
                        _ => return err(None, "judge 的第二个参数要是题或题列表", sp),
                    }
                    let mut out = vec![];
                    for st in states.iter() {
                        let Value::State(st) = st else { return err(None, "judge 的状态列表里有非状态", sp) };
                        let rs = self.judge(st, &qs, sp)?;
                        // 单题时每个对象给一条读数（而不是一个只有一条的列表），`order` 才好用
                        out.push(if qs.len() == 1 { rs.into_iter().next().expect("单题一条") } else { Value::list(rs) });
                    }
                    return Ok(Value::list(out));
                }
                let Value::State(s) = &args[0] else { return err(None, "judge(state | [states], question | [questions])", sp) };
                match &args[1] {
                    Value::Question(q) => Ok(self.judge(s, &[q.clone()], sp)?.remove(0)),
                    Value::List(l) => {
                        let mut qs = vec![];
                        for q in l.iter() {
                            match q { Value::Question(q) => qs.push(q.clone()), _ => return err(None, "judge 的题列表里有非题", sp) }
                        }
                        Ok(Value::list(self.judge(s, &qs, sp)?))
                    }
                    _ => err(None, "judge 的第二个参数要是题或题列表", sp),
                }
            }
            "sieve" => {
                // 三路过滤（05 §1 `filter(S, q)`，施工件 c）：一组材料 × 一道题（或题列表 / 题式 + 填法）
                // → 三条流 act / ignore / unsure，外加 unobserved（预算提前停止时没问到的）。
                // 直接吃题：全部登记完再一次刷新，同状态的题由融合合成一次调用——
                // 「14 倍」那种绕过批处理的写法在这里没有可写的位置。
                if n != 2 && n != 3 {
                    return err(None, "sieve(材料列表, 题 | [题…]) 或 sieve(材料列表, 题式, [填法…])", sp);
                }
                // 输入可以是列表，也可以是上一个构造的契约值（取它的产出；它的未决与证据带进新契约）
                let (items, carried_pending, carried_evidence) = self.unpack(&args[0], "sieve", sp)?;
                let (qs, many) = if n == 3 {
                    let Value::List(fills) = &args[2] else { return err(None, "sieve(材料, 题式, [填法…]) 的第三个参数要是填法记录的列表", sp) };
                    let mut qs = vec![];
                    for f in fills.iter() {
                        match self.builtin("fill", vec![args[1].clone(), f.clone()], sp)? {
                            Value::Question(q) => qs.push(q),
                            _ => return err(None, "fill 没有给出题", sp),
                        }
                    }
                    (qs, true)
                } else {
                    match &args[1] {
                        Value::Question(q) => (vec![q.clone()], false),
                        Value::List(l) => {
                            let mut qs = vec![];
                            for q in l.iter() {
                                match q { Value::Question(q) => qs.push(q.clone()), other => return err(None, format!("sieve 的题列表里有 {}", other.type_name()), sp) }
                            }
                            (qs, true)
                        }
                        other => return err(None, format!("sieve 的第二个参数要是题或题列表，收到 {}", other.type_name()), sp),
                    }
                };
                let mut out = self.sieve(&items, &qs, &carried_pending, &carried_evidence, sp)?;
                if many { Ok(Value::list(out)) } else { Ok(out.remove(0)) }
            }
            "pair" => {
                // 配对（05 §1 `pair(S, T)`，施工件 e）：两组材料 → 关系记录，返回契约值（B17）。
                // 候选怎么枚举由调用者定：全配对 `pair(左, 右)`；按调用者的方法取舍
                // `pair(左, 右, fn(a, b) -> Bool)`；或直接给候选对 `pair([[a, b], …])`。
                // 这里只构造关系，不判断；关系交给 sieve 按关系题分流。
                // 关系记录：`item` 是交给判断器的一份状态材料，两端对象段结构化标为 a / b；
                // `left` / `right` 原样保留调用者给的元素。左右可以是契约值：取产出，未决与证据带进新契约。
                let mut cands: Vec<(Value, Value, i64, i64)> = vec![];
                let mut carried_pending = vec![];
                let mut carried_evidence: Vec<Value> = vec![];
                match n {
                    1 => {
                        let Value::List(ps) = &args[0] else { return err(None, "pair([[a, b], …]) 的参数要是候选对的列表", sp) };
                        for (k, p) in ps.iter().enumerate() {
                            match p {
                                Value::List(ab) if ab.len() == 2 => cands.push((ab[0].clone(), ab[1].clone(), k as i64, k as i64)),
                                other => return err(None, format!("pair 的候选对要是两个元素的列表，第 {k} 个是 {}", other.type_name()), sp),
                            }
                        }
                    }
                    2 | 3 => {
                        let (l, lp, le) = self.unpack(&args[0], "pair", sp)?;
                        let (r, rp, re) = self.unpack(&args[1], "pair", sp)?;
                        carried_pending.extend(lp);
                        carried_pending.extend(rp);
                        for k in le.into_iter().chain(re) { push_key(&mut carried_evidence, k); }
                        for (i, a) in l.iter().enumerate() {
                            for (j, b) in r.iter().enumerate() {
                                if n == 3 {
                                    match self.apply(args[2].clone(), vec![a.clone(), b.clone()], sp)? {
                                        Value::Bool(true) => {}
                                        Value::Bool(false) => continue,
                                        other => return err(None, format!("pair 的取舍方法要返回 Bool，收到 {}", other.type_name()), sp),
                                    }
                                }
                                cands.push((a.clone(), b.clone(), i as i64, j as i64));
                            }
                        }
                    }
                    _ => return err(None, "pair(左, 右) / pair(左, 右, fn(a, b)) / pair([[a, b], …])", sp),
                }
                let rels = cands
                    .into_iter()
                    .map(|(a, b, i, j)| {
                        let (ma, _) = element_parts(&a);
                        let (mb, _) = element_parts(&b);
                        Value::record(vec![
                            ("item".into(), Value::record(vec![("a".into(), ma), ("b".into(), mb)])),
                            ("trail".into(), Value::list(vec![])),
                            ("left".into(), a),
                            ("right".into(), b),
                            ("at".into(), Value::list(vec![Value::Int(i), Value::Int(j)])),
                        ])
                    })
                    .collect();
                Ok(Self::outcome_value("pair", Value::list(rels), carried_pending, carried_evidence, Value::Unit, (0, 0.0), Value::record(vec![]), Value::Unit))
            }
            "tally" => {
                // 集合聚合（05 §1 `agg(S, op)` 的存在 / 全部 / 计数，施工件 f）：吃一个契约值（通常是 sieve 的）。
                // 精确计算，不是概率：计数给区间 [act, act + 未决]，未决里 cause=budget 的是未观察项；
                // 存在、全部是三值出口——结论取决于未决元素时给 unsure。
                // 输入的未决：结论被它们挡住时并入聚合出口（记为已消费），聚合出口进新的未决清单；
                // 没挡住结论时原样带进新契约（13 §3：不许无声消失）。
                arity(1)?;
                let r = &args[0];
                if !is_outcome(r) {
                    return err(None, format!("tally 收一个契约值（sieve / pair / outcome 的结果），收到 {}", r.type_name()), sp);
                }
                let act = list_of(r.get("value"));
                let ignore = list_of(r.get("detail").and_then(|d| d.get("ignore")));
                let pend = list_of(r.get("pending"));
                let is_budget = |e: &Value| matches!(e.get("cause"), Some(Value::Text(t)) if t.as_ref() == "budget");
                let no = pend.iter().filter(|e| is_budget(e)).count() as i64;
                let nu = pend.len() as i64 - no;
                let (na, ni) = (act.len() as i64, ignore.len() as i64);
                let mut taint = Taint::Trusted;
                for e in act.iter().chain(&ignore).chain(&pend) {
                    if let Some(Value::Exit(x)) = e.get("exit") {
                        if x.taint != Taint::Trusted { taint = x.taint; }
                    }
                }
                // 结论被谁挡住：未观察优先（原因 budget），否则取第一个未决元素的原因
                let blocker = if no > 0 { Some("budget".to_string()) } else {
                    pend.first().and_then(|e| e.get("cause")).map(|c| match c { Value::Text(t) => t.to_string(), _ => "band".into() })
                };
                let exists = if na > 0 { ExitKind::Act } else if let Some(c) = &blocker { ExitKind::Unsure(c.clone()) } else { ExitKind::Ignore };
                let all = if ni > 0 { ExitKind::Ignore } else if let Some(c) = &blocker { ExitKind::Unsure(c.clone()) } else { ExitKind::Act };
                let absorbed = matches!(exists, ExitKind::Unsure(_)) || matches!(all, ExitKind::Unsure(_));
                let ex = self.new_exit(exists, None, Op::Test, "tally:exists", "", taint, sp);
                let al = self.new_exit(all, None, Op::Test, "tally:all", "", taint, sp);
                let mut pending = vec![];
                if absorbed {
                    // 元素的未决责任并入聚合出口；聚合出口自己仍要被消费
                    for e in &pend {
                        if let Some(Value::Exit(x)) = e.get("exit") { x.consumed.set(true); *x.consumed_by.borrow_mut() = "tally".into(); }
                    }
                    for x in [&ex, &al] {
                        if let Value::Exit(e) = x { if e.is_unsure() { pending.push(Self::pending_entry(Value::Unit, x)); } }
                    }
                } else {
                    pending = pend.clone();
                    // 已决的聚合出口不带责任
                    for x in [&ex, &al] { if let Value::Exit(e) = x { if !e.is_unsure() { e.consumed.set(true); *e.consumed_by.borrow_mut() = "tally:decided".into(); } } }
                }
                if absorbed {
                    for x in [&ex, &al] { if let Value::Exit(e) = x { if !e.is_unsure() { e.consumed.set(true); *e.consumed_by.borrow_mut() = "tally:decided".into(); } } }
                }
                let value = Value::record(vec![
                    ("n".into(), Value::Int(na + ni + nu + no)),
                    ("act".into(), Value::Int(na)),
                    ("ignore".into(), Value::Int(ni)),
                    ("unsure".into(), Value::Int(nu)),
                    ("unobserved".into(), Value::Int(no)),
                    ("count".into(), Value::list(vec![Value::Int(na), Value::Int(na + nu + no)])),
                    ("complete".into(), Value::Bool(nu == 0 && no == 0)),
                    ("exists".into(), ex),
                    ("all".into(), al),
                ]);
                Ok(Self::outcome_value("tally", value, pending, list_of(r.get("evidence")), r.get("resume").unwrap_or(Value::Unit), (0, 0.0), Value::record(vec![]), Value::Unit))
            }
            "first_k" => {
                // 输入顺序中的前 k 个接受项（交接首包「第一个」语义；05 §1 前 k）：
                // 按原顺序走，遇到未决（含 cause=budget 的未观察项）而还没凑够 k 个，就不能宣称后面的接受项是「前 k 个」。
                // 产出 `{items, exit}`：act = 凑够了 k 个且之前没有挡路的；ignore = 全部观察完、确定不足 k 个；
                // unsure = 被挡住。被挡的位置与原因进续接 `resume`（B17 取舍）。
                // 输入的未决原样带进新契约；被挡住时的 unsure 出口也进未决清单。
                arity(2)?;
                let (r, Value::Int(k)) = (&args[0], &args[1]) else { return err(None, "first_k(契约值, k: Int)", sp) };
                let k = *k;
                if k <= 0 { return err(None, "first_k 的 k 要是正整数", sp); }
                if !is_outcome(r) { return err(None, format!("first_k 收一个契约值（sieve 的结果），收到 {}", r.type_name()), sp); }
                let pend = list_of(r.get("pending"));
                let mut all: Vec<(i64, String, Value)> = vec![];
                for e in list_of(r.get("value")) { all.push((0, "act".into(), e)); }
                for e in list_of(r.get("detail").and_then(|d| d.get("ignore"))) { all.push((0, "ignore".into(), e)); }
                for p in &pend {
                    let c = match p.get("cause") { Some(Value::Text(t)) => t.to_string(), _ => "band".into() };
                    all.push((0, format!("pending:{c}"), p.get("element").unwrap_or(Value::Unit)));
                }
                for x in all.iter_mut() {
                    x.0 = match x.2.get("index") { Some(Value::Int(i)) => i, _ => return err(None, "first_k：元素缺 index（只收 sieve 产物的元素）", sp) };
                }
                all.sort_by_key(|x| x.0);
                let mut items = vec![];
                let mut kind = ExitKind::Ignore;
                let mut resume = Value::Unit;
                let mut taint = Taint::Trusted;
                for (idx, tag, e) in &all {
                    if let Some(Value::Exit(x)) = e.get("exit") { if x.taint != Taint::Trusted { taint = x.taint; } }
                    match tag.as_str() {
                        "act" => { items.push(e.clone()); if items.len() as i64 == k { kind = ExitKind::Act; break; } }
                        "ignore" => {}
                        t => {
                            let c = t.trim_start_matches("pending:").to_string();
                            kind = ExitKind::Unsure(c.clone());
                            resume = Value::record(vec![("reason".into(), Value::text("blocked")), ("at".into(), Value::Int(*idx)), ("cause".into(), Value::text(&c))]);
                            break;
                        }
                    }
                }
                let ex = self.new_exit(kind, None, Op::Test, "first_k", "", taint, sp);
                let mut pending = pend.clone();
                if let Value::Exit(e) = &ex {
                    if e.is_unsure() { pending.push(Self::pending_entry(Value::Unit, &ex)); } else { e.consumed.set(true); *e.consumed_by.borrow_mut() = "first_k:decided".into(); }
                }
                let value = Value::record(vec![("items".into(), Value::list(items)), ("exit".into(), ex), ("k".into(), Value::Int(k))]);
                Ok(Self::outcome_value("first_k", value, pending, list_of(r.get("evidence")), resume, (0, 0.0), Value::record(vec![]), Value::Unit))
            }
            "iterate" => {
                // 有界迭代（05 §1 `iterate(f, S, bound)`，施工件 g）：三条终止线并存——
                // 步数到上限（bound）、每层材料严格变少（measure 不再下降即停，noshrink）、
                // 账本键在本循环内重复即停（repeat，J-06）。step 返回 stop(v) 也停。
                // 终止原因写进结果：{value, reason, rounds, measures}。
                // measure：fn(acc) -> Int，或 "tokens"（按渲染后的 token 估算，与窗口检查同一估法）。
                arity(4)?;
                let Value::Int(b) = &args[0] else { return err(Some("J-06"), "iterate 的 bound 必须是整数", sp) };
                let bound = *b;
                if bound <= 0 { return err(Some("J-06"), format!("iterate 的 bound 必须是正整数，收到 {bound}"), sp); }
                let Value::Fn(step) = &args[2] else { return err(None, "iterate(bound, 初值, fn(acc, i), measure) 的 step 要是函数", sp) };
                let step = step.clone();
                let measure = args[3].clone();
                let measure_of = |me: &mut Self, v: &Value| -> R<i64> {
                    match &measure {
                        Value::Text(t) if t.as_ref() == "tokens" => Ok((canon(&v.to_json()).chars().count() as f64 / 1.3) as i64 + 1),
                        Value::Fn(_) | Value::Builtin(_) => match me.apply(measure.clone(), vec![v.clone()], sp)? {
                            Value::Int(i) => Ok(i),
                            other => err(None, format!("iterate 的 measure 要返回 Int，收到 {}", other.type_name()), sp),
                        },
                        other => err(None, format!("iterate 的 measure 要是 fn(acc) -> Int 或 \"tokens\"，收到 {}", other.type_name()), sp),
                    }
                };
                let m0 = self.mark();
                let mut acc = args[1].clone();
                let mut prev = measure_of(self, &acc)?;
                let mut measures = vec![Value::Int(prev)];
                let mut reason = "bound";
                let mut rounds = 0i64;
                self.loops.push(LoopCtx { seen_keys: HashSet::new(), repeated: None });
                for i in 0..bound {
                    let out = match self.call_closure(&step, vec![acc.clone(), Value::Int(i)], sp) {
                        Ok(v) => v,
                        Err(e) => { self.loops.pop(); return Err(e); }
                    };
                    rounds = i + 1;
                    if let Value::Stop(v) = out { acc = (*v).clone(); reason = "stop"; break; }
                    acc = out;
                    if self.loops.last().and_then(|l| l.repeated.clone()).is_some() { reason = "repeat"; break; }
                    let m = match measure_of(self, &acc) { Ok(m) => m, Err(e) => { self.loops.pop(); return Err(e); } };
                    measures.push(Value::Int(m));
                    if m >= prev { reason = "noshrink"; break; }
                    prev = m;
                }
                self.loops.pop();
                // 停止原因与轮次进续接（B17 取舍）；产出是最后的累积值
                let (evidence, spent) = self.since(m0);
                let resume = Value::record(vec![
                    ("reason".into(), Value::text(reason)),
                    ("rounds".into(), Value::Int(rounds)),
                    ("measures".into(), Value::list(measures)),
                ]);
                Ok(Self::outcome_value("iterate", acc, vec![], evidence, resume, spent, Value::record(vec![]), Value::Unit))
            }
            "outcome" => {
                // 调用者自己构造一个契约值（B17）：`outcome({value, pending?, evidence?, resume?, purpose?, detail?})`。
                // 与内置构造返回同一类型，可再交给 sieve / pair / tally 等。
                // - pending：出口，或 `{element?, exit, cause?}` 记录；每项必须带出口（责任载体）；
                // - evidence：只收账本键（Text，由 key_of 取得），不收读数或材料副本（不变量 3）；
                // - resume：记录；或一个方法，记为 `{reason: "continue", next: 方法}`。
                arity(1)?;
                let r = &args[0];
                if !matches!(r, Value::Record(_)) { return err(None, "outcome({value, pending?, evidence?, resume?, purpose?, detail?})", sp); }
                if let Value::Record(fs) = r {
                    for (k, _) in fs.iter() {
                        if !["value", "pending", "evidence", "resume", "purpose", "detail", "spent"].contains(&k.as_str()) {
                            return err(None, format!("outcome 不认得字段 {k}：可给 value、pending、evidence、resume、purpose、detail、spent"), sp);
                        }
                    }
                }
                let Some(value) = r.get("value") else { return err(None, "outcome 必须给 value（产出）", sp) };
                let mut pending = vec![];
                for (i, p) in list_of(r.get("pending")).into_iter().enumerate() {
                    match &p {
                        Value::Exit(_) | Value::Duty(_) => pending.push(Self::pending_entry(Value::Unit, &p)),
                        Value::Record(_) => match p.get("exit") {
                            Some(x @ (Value::Exit(_) | Value::Duty(_))) => pending.push(Self::pending_entry(p.get("element").unwrap_or(Value::Unit), &x)),
                            _ => return err(Some("J-05"), format!("outcome 的 pending 第 {i} 项没有出口：未决清单的每一项都要带承担责任的出口（exit）。修法：把 cut / handle 前的出口放进来"), sp),
                        },
                        other => return err(Some("J-05"), format!("outcome 的 pending 第 {i} 项是 {}：只收出口或带 exit 的记录", other.type_name()), sp),
                    }
                }
                let mut evidence = vec![];
                for (i, k) in list_of(r.get("evidence")).into_iter().enumerate() {
                    match &k {
                        Value::Text(t) if !t.is_empty() => push_key(&mut evidence, k.clone()),
                        other => return err(None, format!("E-evidence: outcome 的 evidence 第 {i} 项是 {}：证据只存账本键（Text），不存读数、观察或材料的副本。修法：用 key_of(出口) 取键", other.type_name()), sp),
                    }
                }
                let resume = match r.get("resume") {
                    None | Some(Value::Unit) => Value::Unit,
                    Some(f @ (Value::Fn(_) | Value::Builtin(_))) => Value::record(vec![("reason".into(), Value::text("continue")), ("next".into(), f)]),
                    Some(rec @ Value::Record(_)) => rec,
                    Some(other) => return err(None, format!("outcome 的 resume 要是记录或方法，收到 {}", other.type_name()), sp),
                };
                let spent = match r.get("spent") {
                    Some(sv) => (match sv.get("calls") { Some(Value::Int(c)) => c, _ => 0 }, match sv.get("usd") { Some(Value::Float(u)) => u, Some(Value::Int(u)) => u as f64, _ => 0.0 }),
                    None => (0, 0.0),
                };
                Ok(Self::outcome_value("outcome", value, pending, evidence, resume, spent, r.get("detail").unwrap_or(Value::record(vec![])), r.get("purpose").unwrap_or(Value::Unit)))
            }
            "key_of" => {
                // 账本键：出口取它来自的那条账本记录；读数取自己的键；契约值取它的证据列表
                arity(1)?;
                match &args[0] {
                    Value::Exit(e) | Value::Duty(e) => Ok(Value::text(&e.ledger_key.borrow())),
                    Value::Reading(r) => Ok(Value::text(&r.ledger_key)),
                    v if is_outcome(v) => Ok(v.get("evidence").unwrap_or(Value::list(vec![]))),
                    other => err(None, format!("key_of 收出口、读数或契约值，收到 {}", other.type_name()), sp),
                }
            }
            "cut" => {
                if n == 0 || n > 2 {
                    return err(None, "cut(reading) 或 cut(reading, calib_key)", sp);
                }
                let calib = match args.get(1) {
                    None => None,
                    Some(Value::Text(k)) => Some(k.to_string()),
                    Some(other) => return err(Some("J-03"), format!("cut 的校准参数必须是校准记录的键（Text），不能是字面量线；收到 {}", other.type_name()), sp),
                };
                match &args[0] {
                    Value::Reading(r) => self.cut(r, calib.as_deref(), sp),
                    Value::List(l) => {
                        let mut out = vec![];
                        for r in l.iter() {
                            match r { Value::Reading(r) => out.push(self.cut(r, calib.as_deref(), sp)?), _ => return err(None, "cut 的列表里有非读数", sp) }
                        }
                        Ok(Value::list(out))
                    }
                    other => err(None, format!("cut 只收读数，收到 {}", other.type_name()), sp),
                }
            }
            "handle" => {
                arity(2)?;
                let Value::Exit(e) = &args[0] else { return err(None, format!("handle 的第一个参数要是出口，收到 {}", args[0].type_name()), sp) };
                let e = e.clone();
                self.handle(&e, &args[1], sp)
            }
            "consume" => {
                arity(2)?;
                let Value::Text(how) = &args[1] else { return err(None, "consume(exit | [exits], \"drop\")", sp) };
                if how.as_ref() != "drop" {
                    return err(None, "consume 目前只支持 \"drop\"；升级用 ask", sp);
                }
                // 契约值：丢它的整个未决清单（每项的 exit）
                let list: Vec<Value> = if is_outcome(&args[0]) {
                    list_of(args[0].get("pending")).into_iter().filter_map(|e| e.get("exit")).collect()
                } else {
                    match &args[0] { Value::List(l) => l.iter().cloned().collect(), v => vec![v.clone()] }
                };
                for v in &list {
                    match v {
                        Value::Exit(e) | Value::Duty(e) => {
                            if e.is_unsure() && !e.consumed.get() {
                                // drop 是合法去向（12 §6「unsure 显式丢弃并记账」），但要留痕
                                self.trace.warn(format!(
                                    "W-drop-vs-escalate: 显式丢弃了未决责任 {}（题 {}）；drop 合法且已记账，但只有 escalate 会把它交给人",
                                    e.label(),
                                    头(&e.q_hash, 8)
                                ));
                            }
                            e.consumed.set(true);
                            *e.consumed_by.borrow_mut() = "consume:drop".into();
                        }
                        _ => return err(None, "consume 只收出口或未决责任", sp),
                    }
                }
                Ok(Value::Unit)
            }
            "gen" => {
                arity(4)?;
                let (Value::Text(p), ctx, Value::Int(k), Value::Int(r)) = (&args[0], &args[1], &args[2], &args[3]) else { return err(None, "gen(prompt, [ctx], n, retry_seq)", sp) };
                let (ctx, _) = self.as_mats(ctx, "ctx", sp)?;
                let p = p.to_string();
                self.generate(&p, &ctx, *k as usize, *r, sp)
            }
            "do" => {
                arity(3)?;
                let (Value::Text(a), Value::List(l), Value::Int(i)) = (&args[0], &args[1], &args[2]) else { return err(None, "do(action, [args], iter_seq)", sp) };
                let a = a.to_string();
                let l: Vec<Value> = l.iter().cloned().collect();
                self.do_(&a, &l, *i, sp)
            }
            "ask" => {
                arity(2)?;
                let (Value::State(s), Value::Question(q)) = (&args[0], &args[1]) else { return err(None, "ask(state, question)", sp) };
                let (s, q) = (s.clone(), q.clone());
                self.ask(&s, &q, sp)
            }
            "transform" => {
                if n < 1 {
                    return err(None, "transform(f, mats…)", sp);
                }
                let Value::Fn(f) = &args[0] else { return err(None, "transform 的第一个参数要是函数", sp) };
                let f = f.clone();
                self.transform(&f, &args[1..], sp)
            }
            "mat" => {
                arity(1)?;
                Ok(Value::Mat(Rc::new(self.as_mat(&args[0], "mat", sp)?)))
            }
            "content" => {
                arity(1)?;
                match &args[0] {
                    // **untrusted 材料的内容，拆出来仍是 untrusted 的材料**。
                    //
                    // 此前这里拆成裸 JSON，再靠 `unwrapped_untrusted` 那张表在 `mat()` 时按值
                    // 查回来——那条路**按值精确匹配，多套一层容器就绕过去了**（实测：
                    // `mat({outer: content(脏)})` 洗白成功，而 `mat(content(脏))` 被堵住）。
                    // **穷举包装方式永远落后一步**，所以改掉机制而不是再打第三个补丁：
                    // **让值自己带着来源**，容器由 `taint_of` 递归看（它本来就会）。
                    //
                    // 代价写明：`content(脏).字段` 这类取字段现在拿到的是材料不是裸值。
                    // `Field` 那一臂已经对材料做了处理（`content`/`taint`/`hash` 三个字段），
                    // 所以取普通字段要先 `content(...)` 一次——**这是显式的代价，不是静默的**。
                    Value::Mat(m) => {
                        if m.taint == Taint::Untrusted {
                            // 这一轮里，从 untrusted 材料拆出来的内容。`mat()` 据此不把它洗成
                            // trusted——**但不再按值精确匹配**（那条路多套一层容器就绕过去了）：
                            // 记的是**内容本身及其所有子结构**，于是 `{outer: 拆了}` 这类包装
                            // 里只要含有拆出来的那份内容，`as_mat` 就能认出来。
                            insert_with_parts(&mut self.unwrapped_untrusted, &m.content);
                        }
                        Ok(json_to_value(&m.content))
                    }
                    Value::Reading(_) => err(Some("J-01"), "读数没有内容可读；只能经 cut 离开", sp),
                    other => err(None, format!("content 只收材料，收到 {}", other.type_name()), sp),
                }
            }
            "unsure" => {
                arity(1)?;
                match &args[0] {
                    // 重新包装：责任继续由这个出口带着，交给调用者
                    Value::Duty(e) => Ok(Value::Exit(e.clone())),
                    Value::Text(c) => {
                        let c = c.to_string();
                        Ok(self.new_exit(ExitKind::Unsure(c), None, Op::Test, "explicit", "", Taint::Trusted, sp))
                    }
                    other => err(None, format!("unsure(未决责任) 重新包装，或 unsure(原因: Text) 新造一个；收到 {}", other.type_name()), sp),
                }
            }
            // **J-15 那一位对 handler 可见**，不是只进 trace（`12` §2.11 硬要求一）。
            // 理由是路由真的不同：`tie` 的既定去向是「逐候选 noul」，而没测过的那条路
            // （K-noul）**本来就是逐候选 noul，路过去是空转**。handler 看不见那一位，
            // 就只能把两种情形当同一件事办——**那正是要消除的东西**。
            //
            // 返回 `""` 表示「都测过了」，返回载体名表示「那个量没测」。与 `unsure_cause`
            // 一样是**只读快照，不转移责任**（见 tests/duty.rs：读原因不算处理）。
            "taint" => {
                if args.len() != 1 { return err(None, "taint(出口)", sp); }
                match &args[0] {
                    // **宪法第 44 行唯一那条 IFC 纪律建在 taint 上，而 `taint` 从 `cause`
                    // 删掉之后，`.jpp` 作者再没有任何东西能说出「这个判断站在不可信材料上」**
                    // ——J-08 只会在 `do` 那里**拒绝**，作者拿不到任何**在被拒绝之前**读得到的东西。
                    // 对照：「测没测过」被判为必须是一位正交的、对 handler 可见的东西。
                    // **重的那条待遇更弱**，这里把它补齐。
                    Value::Duty(e) | Value::Exit(e) => Ok(Value::text(match e.taint {
                        Taint::Trusted => "trusted",
                        Taint::Untrusted => "untrusted",
                    })),
                    other => err(None, format!("taint 只收未决责任或出口，收到 {}", other.type_name()), sp),
                }
            }
            "line_source" => {
                if args.len() != 1 { return err(None, "line_source(出口)", sp); }
                match &args[0] {
                    Value::Duty(e) | Value::Exit(e) => Ok(Value::text(&e.line_source)),
                    other => err(None, format!("line_source 只收未决责任或出口，收到 {}", other.type_name()), sp),
                }
            }
            "untested" => {
                arity(1)?;
                match &args[0] {
                    Value::Duty(e) | Value::Exit(e) => Ok(Value::text(e.untested().unwrap_or(""))),
                    other => err(None, format!("untested 只收未决责任或出口，收到 {}", other.type_name()), sp),
                }
            }
            "unsure_cause" => {
                arity(1)?;
                match &args[0] {
                    Value::Duty(e) | Value::Exit(e) => Ok(Value::text(&e.cause())),
                    other => err(None, format!("unsure_cause 只收未决责任或出口，收到 {}", other.type_name()), sp),
                }
            }
            // 合法去向之一：把责任交给明确关联的人工请求（效应 ask）
            "escalate" => {
                arity(3)?;
                let (Value::Duty(u), Value::State(s), Value::Question(q)) = (&args[0], &args[1], &args[2]) else {
                    return err(Some("J-05"), "escalate(未决责任, state, 题)：第一个参数要是 unsure 臂收到的那份责任", sp);
                };
                let (u, s, q) = (u.clone(), s.clone(), q.clone());
                u.consumed.set(true);
                *u.consumed_by.borrow_mut() = "escalate".into();
                self.ask(&s, &q, sp)
            }
            // 合法去向之一：接走旧责任、按更字面的题重问，产生新的待处理出口（效应 judge）
            "literalize" => {
                arity(3)?;
                let (Value::Duty(u), Value::State(s), Value::Question(q)) = (&args[0], &args[1], &args[2]) else {
                    return err(Some("J-05"), "literalize(未决责任, state, 更字面的题)：第一个参数要是 unsure 臂收到的那份责任", sp);
                };
                let (u, s, q) = (u.clone(), s.clone(), q.clone());
                u.consumed.set(true);
                *u.consumed_by.borrow_mut() = "literalize".into();
                let reading = self.judge(&s, &[q], sp)?.remove(0);
                match reading {
                    Value::Reading(r) => self.cut(&r, None, sp),
                    other => Ok(other),
                }
            }
            "pending" => {
                arity(1)?;
                let Value::Text(c) = &args[0] else { return err(None, "pending(reason: Text)", sp) };
                Err(Fault::Halt(Pending { cause: "explicit".into(), key: String::new(), site: sp, detail: c.to_string() }))
            }
            "fail" => {
                arity(1)?;
                let Value::Text(c) = &args[0] else { return err(None, "fail(reason: Text)", sp) };
                Ok(Value::Fail(Rc::from(c.as_ref())))
            }
            "is_fail" => {
                arity(1)?;
                Ok(Value::Bool(matches!(args[0], Value::Fail(_))))
            }
            // ---- 长处（G4 §7「判断力花在哪」）：读数是带校准线的随机变量，不是值。
            // 这两个构件只读校准线、不做跨题算术、返回宿主值不返回读数，所以合法（Python
            // `runtime.py:1236` allocate 的 docstring 原话）。都不花钱：不发调用、不进账本、不动预算。
            "allocate" => {
                arity(2)?;
                // 刷新点：它要读「离线多远」，那是答案上的量（12 §2.2 的「宿主读内容」同一类）
                self.flush("allocate")?;
                let rs = self.readings_of(&args[0], "allocate", sp)?;
                // **这里也在用线**（`uncertainty` 读 `lines_for`），所以漂移要在这里也报。
                self.报漂移_批(&rs, sp);
                let Value::Int(k) = &args[1] else { return err(None, "allocate(读数们, k: Int)：k 是复核名额，通常取 budget.escalate", sp) };
                if *k < 0 {
                    return err(None, format!("allocate: k 必须是非负整数（通常取 budget.escalate），收到 {k}"), sp);
                }
                let rep = crate::strength::allocate_report(self.calib, &rs, *k as usize);
                // **算不出的那些要对程序可见，不能只进 trace**（J-15 那条纪律）。
                if !rep.算不出.is_empty() {
                    self.trace.warn(format!(
                        "W-untested: @{} uncertainty 对 {} 条读数算不出（它们的校准键没有上岗记录，没有属于自己的线），                         这些读数不进 allocate 的榜。按 J-15 不阻塞。修法【作者可改】：按 `算不出` 那一栏另行处置（那一栏就在 allocate 的返回值里）；【需接线人】给那些键写上岗记录、或给 CLI 接上 --calib/--profile",
                        sp.start, rep.算不出.len()
                    ));
                }
                // **返回记录而不是裸表**：`算不出` 那一栏必须对程序可见。
                // 以前返回一张下标表，而冷键上那张表恰好是 `[0, 1, …]`——
                // **一个与不确定性无关、却看起来像答案的答案**。
                Ok(Value::Record(Rc::new(vec![
                    ("picked".into(), Value::List(Rc::new(rep.picked.into_iter().map(|i| Value::Int(i as i64)).collect()))),
                    ("算不出".into(), Value::List(Rc::new(rep.算不出.into_iter().map(|i| Value::Int(i as i64)).collect()))),
                ])))
            }
            "unsure_bound" => {
                arity(1)?;
                // 只读校准记录的 unsure_rate，不读答案——但读数要先就绪才谈得上「这批读数」
                self.flush("unsure_bound")?;
                let rs = self.readings_of(&args[0], "unsure_bound", sp)?;
                // **J-10 的上界建在 `unsure_rate` 上，而那是标注集上的实测值**——
                // 分布移开之后它不再成立。**漂了要报，否则上界静默失效。**
                self.报漂移_批(&rs, sp);
                let b = crate::strength::unsure_bound(self.calib, &rs);
                Ok(Value::Record(Rc::new(vec![
                    ("n".into(), Value::Int(b.n as i64)),
                    ("union_bound".into(), Value::Float(b.union_bound)),
                    // **不给裸浮点。** Rust 侧的 `仅供参考` 类型闸只拦得住 Rust 调用者，
                    // 而**这门语言唯一的用户拿到的是 `Value::Float`，可以直接当判据，没有任何东西会红**
                    // ——「替身上成立、真机上失效」，只是这次的「替身」是宿主语言。
                    // 交成 `Text`：看得见、打得出，**比不了大小、做不了算术**，
                    // 与 Rust 侧那道闸是同一条纪律在同一侧生效。
                    ("independent_any".into(), Value::text(&format!("{:.4}（仅供参考，不可作判据；判据是 union_bound）", b.independent_any.as_reference_only()))),
                    ("n_unknown".into(), Value::Int(b.n_unknown as i64)),
                ])))
            }
            // 判断向量的两法（12:134「合法操作**只有两种**…其余运算不存在（J-01）」）。
            // 它们不是「读数列表上的工具函数」——正因为只有这两种，读数才不会被当成数用。
            "agg" => {
                arity(1)?;
                self.flush("agg")?;
                let rs = self.readings_of(&args[0], "agg", sp)?;
                if rs.is_empty() {
                    return err(None, "agg 要至少一条读数", sp);
                }
                // 同题跨运行的均值（noul/score）或众数（choice）。合并后**仍是读数**——
                // 所以还能 cut。拿 fold 求平均得到的是裸数，进不了 cut、也不带校准键。
                let first = &rs[0];
                if rs.iter().any(|r| r.q_hash != first.q_hash) {
                    return err(Some("J-01"), "agg 只合并**同一道题**跨运行的读数：收到的读数不是同一道题", sp);
                }
                let merged = merge_runs(&rs, sp)?;
                Ok(Value::Reading(Rc::new(Reading {
                    q_hash: first.q_hash.clone(),
                    state_hash: first.state_hash.clone(),
                    op: first.op,
                    calib: first.calib.clone(),
                    answer: std::cell::RefCell::new(Some(merged)),
                    fail: None,
                    model_id: first.model_id.clone(),
                    ledger_key: format!("agg:{}", first.ledger_key),
                    over_len: first.over_len,
                    scale: first.scale.clone(),
                    perms: std::cell::Cell::new(first.perms.get()),
                    mode_share: std::cell::Cell::new(first.mode_share.get()),
                    missing_evidence: first.missing_evidence.clone(),
                    state_taint: rs.iter().fold(Taint::Trusted, |t, r| Taint::join(t, r.state_taint)),
                    form_hash: first.form_hash.clone(),
                })))
            }
            "order" => {
                arity(1)?;
                self.flush("order")?;
                let rs = self.readings_of(&args[0], "order", sp)?;
                // J-04（12:255）：跨题、跨候选集、跨刻度或异锚的读数**不可比**。
                // order 是排序，排序就是比——两道题各有各的校准线，p 不在同一把尺子上，
                // 「问题一 0.9 高于问题二 0.5」这个比较本身不成立。
                if let Some(first) = rs.first() {
                    for r in &rs {
                        if r.q_hash != first.q_hash {
                            return err(Some("J-04"), "order 只排**同一道题**跨对象的读数：跨题的读数不可比（各有各的校准线，p 不在同一把尺子上）", sp);
                        }
                        if r.over_len != first.over_len || r.scale != first.scale {
                            return err(Some("J-04"), "order 的读数候选集或刻度不同：指纹不同即不可比", sp);
                        }
                    }
                }
                Ok(Value::list(self.order_tiers(&rs).into_iter().map(|tier| Value::list(tier.into_iter().map(|i| Value::Int(i as i64)).collect())).collect()))
            }
            // fit 桥（12 §6.0:315 `fit(名, [读数…])`，**输出仍是读数、仍要 cut**）。
            // 让什么活下来：**跨题的联合判断在类型上仍是读数**，因而仍要过线、仍可能 unsure。
            // 直接产出出口就绕过了 cut 的判序（insufficient → taint → 过线 → band）。
            "fit" => {
                arity(2)?;
                self.flush("fit")?;
                let Value::Text(name) = &args[0] else { return err(Some("J-16"), "fit(名字: Text, [读数…])", sp) };
                let rs = self.readings_of(&args[1], "fit", sp)?;
                let Some(rec) = self.fits.fits.get(name.as_ref()).cloned() else {
                    return err(Some("J-16"), format!("fit {name} 未注册：fit 只认注册表签名（12:274）。修法：用训练过程注册，或改用 cut"), sp);
                };
                if rs.len() != rec.features.len() {
                    return err(Some("J-16"), format!("fit {name} 期望 {} 个读数，收到 {}", rec.features.len(), rs.len()), sp);
                }
                // J-04：输入指纹必须与注册特征**逐项相同**
                for (r, (ck, fk)) in rs.iter().zip(&rec.features) {
                    if &r.calib != ck || r.op.phys() != fk {
                        return err(
                            Some("J-04"),
                            format!("fit {name} 的输入指纹（{}, {}）与注册特征（{ck}, {fk}）不同：跨题读数不可比，喂错题就是错", r.calib, r.op.phys()),
                            sp,
                        );
                    }
                }
                let ps: Vec<f64> = rs.iter().map(|r| rank_value(r).unwrap_or(0.0)).collect();
                let score = (rec.f)(&ps).clamp(0.0, 1.0);
                // 结果是读数：calib 位先留 fit 的名字，真正的线由 cut 的第二参给
                Ok(Value::Reading(Rc::new(Reading {
                    q_hash: format!("fit:{name}"),
                    state_hash: rs.first().map(|r| r.state_hash.clone()).unwrap_or_default(),
                    op: Op::Test,
                    calib: format!("fit:{name}"),
                    answer: std::cell::RefCell::new(Some(Answer::Noul(score))),
                    fail: None,
                    model_id: self.model_id.clone(),
                    ledger_key: format!("fit:{name}"),
                    over_len: 0,
                    scale: vec![],
                    perms: std::cell::Cell::new(0),
                mode_share: std::cell::Cell::new(None),
                    missing_evidence: vec![],
                    state_taint: rs.iter().fold(Taint::Trusted, |t, r| Taint::join(t, r.state_taint)),
                    form_hash: None,
                })))
            }
            "exit_kind" => {
                arity(1)?;
                match &args[0] { Value::Exit(e) => Ok(Value::text(&e.label())), _ => err(None, "exit_kind 只收出口", sp) }
            }
            "loop" => {
                arity(3)?;
                let Value::Int(b) = &args[0] else { return err(Some("J-06"), "loop 的 bound 必须是整数字面量或整数值", sp) };
                let (b, init, step) = (*b, args[1].clone(), args[2].clone());
                self.loop_(b, init, &step, sp)
            }
            "stop" => {
                arity(1)?;
                Ok(Value::Stop(Rc::new(args[0].clone())))
            }
            "len" => {
                arity(1)?;
                match &args[0] {
                    Value::List(l) => Ok(Value::Int(l.len() as i64)),
                    Value::Text(t) => Ok(Value::Int(t.chars().count() as i64)),
                    Value::Record(r) => Ok(Value::Int(r.len() as i64)),
                    other => err(None, format!("len 不适用于 {}", other.type_name()), sp),
                }
            }
            "map" | "filter" => {
                arity(2)?;
                let (Value::List(l), f) = (&args[0], &args[1]) else { return err(None, format!("{name}(list, fn)"), sp) };
                self.vectorize_ahead(f, l, sp);
                let mut out = vec![];
                for it in l.iter() {
                    let r = self.apply(f.clone(), vec![it.clone()], sp)?;
                    if name == "map" {
                        out.push(r);
                        continue;
                    }
                    // filter 的谓词必须返回 Bool。返回别的东西以前被**静默当假**：
                    // 出口、未决责任传进来会无声消失，正是 13 §3 要堵的那类。
                    match r {
                        Value::Bool(true) => out.push(it.clone()),
                        Value::Bool(false) => {}
                        other => {
                            return err(
                                None,
                                format!("filter 的谓词要返回 Bool（真假），收到 {}。返回别的东西以前被当成假、元素被静默丢掉。修法：让谓词自己算出真假再返回", other.type_name()),
                                sp,
                            );
                        }
                    }
                }
                Ok(Value::list(out))
            }
            "fold" => {
                arity(3)?;
                let (Value::List(l), init, f) = (&args[0], &args[1], &args[2]) else { return err(None, "fold(list, init, fn(acc, x))", sp) };
                let mut acc = init.clone();
                for it in l.iter() {
                    acc = self.apply(f.clone(), vec![acc, it.clone()], sp)?;
                }
                Ok(acc)
            }
            "range" => {
                arity(2)?;
                let (Value::Int(a), Value::Int(b)) = (&args[0], &args[1]) else { return err(None, "range(a, b)", sp) };
                Ok(Value::list((*a..*b).map(Value::Int).collect()))
            }
            "append" => {
                arity(2)?;
                let Value::List(l) = &args[0] else { return err(None, "append(list, v)", sp) };
                let mut v: Vec<Value> = l.iter().cloned().collect();
                v.push(args[1].clone());
                Ok(Value::list(v))
            }
            "concat" => {
                arity(2)?;
                let (Value::List(a), Value::List(b)) = (&args[0], &args[1]) else { return err(None, "concat(a, b)", sp) };
                Ok(Value::list(a.iter().chain(b.iter()).cloned().collect()))
            }
            "slice" => {
                arity(3)?;
                let (Value::List(l), Value::Int(a), Value::Int(b)) = (&args[0], &args[1], &args[2]) else { return err(None, "slice(list, a, b)", sp) };
                let a = (*a).clamp(0, l.len() as i64) as usize;
                let b = (*b).clamp(a as i64, l.len() as i64) as usize;
                Ok(Value::list(l[a..b].to_vec()))
            }
            "contains" => {
                arity(2)?;
                let Value::List(l) = &args[0] else { return err(None, "contains(list, v)", sp) };
                for it in l.iter() {
                    match it.equals(&args[1]) {
                        Some(true) => return Ok(Value::Bool(true)),
                        None => return err(Some("J-01"), "读数不可比", sp),
                        _ => {}
                    }
                }
                Ok(Value::Bool(false))
            }
            "sum" => {
                arity(1)?;
                let Value::List(l) = &args[0] else { return err(None, "sum(list)", sp) };
                let mut acc = Value::Int(0);
                for it in l.iter() {
                    acc = self.binop("+", acc, it.clone(), sp)?;
                }
                Ok(acc)
            }
            "min" | "max" => {
                arity(2)?;
                let (Value::Int(a), Value::Int(b)) = (&args[0], &args[1]) else { return err(None, format!("{name}(Int, Int)"), sp) };
                Ok(Value::Int(if name == "min" { *a.min(b) } else { *a.max(b) }))
            }
            "abs" => {
                arity(1)?;
                match &args[0] {
                    // 13 §6：i64::MIN 没有对应的正数，取绝对值同样越界
                    Value::Int(a) => Ok(Value::Int(a.checked_abs().ok_or_else(|| overflow("取绝对值", *a, 0, sp))?)),
                    Value::Float(a) => Ok(Value::Float(a.abs())),
                    _ => err(None, "abs(number)", sp),
                }
            }
            "floor" => {
                arity(1)?;
                match &args[0] { Value::Float(a) => Ok(Value::Int(a.floor() as i64)), Value::Int(a) => Ok(Value::Int(*a)), _ => err(None, "floor(number)", sp) }
            }
            "reverse" => {
                arity(1)?;
                let Value::List(l) = &args[0] else { return err(None, "reverse(list)", sp) };
                Ok(Value::list(l.iter().rev().cloned().collect()))
            }
            "keys" => {
                arity(1)?;
                let Value::Record(r) = &args[0] else { return err(None, "keys(record)", sp) };
                Ok(Value::list(r.iter().map(|(k, _)| Value::text(k)).collect()))
            }
            "has" => {
                arity(2)?;
                let (Value::Record(_), Value::Text(k)) = (&args[0], &args[1]) else { return err(None, "has(record, key)", sp) };
                Ok(Value::Bool(args[0].get(k).is_some()))
            }
            "with" => {
                arity(3)?;
                let (Value::Record(r), Value::Text(k)) = (&args[0], &args[1]) else { return err(None, "with(record, key, value)", sp) };
                let mut v: Vec<(String, Value)> = r.iter().filter(|(kk, _)| kk.as_str() != k.as_ref()).cloned().collect();
                v.push((k.to_string(), args[2].clone()));
                Ok(Value::record(v))
            }
            "text" => {
                arity(1)?;
                match &args[0] {
                    Value::Text(t) => Ok(Value::text(t)),
                    Value::Reading(_) => err(Some("J-01"), "读数不能转文字", sp),
                    Value::Mat(m) => Ok(Value::text(&m.text())),
                    other => Ok(Value::text(&other.to_json().to_string().trim_matches('"').to_string())),
                }
            }
            "join" => {
                arity(2)?;
                let (Value::List(l), Value::Text(sep)) = (&args[0], &args[1]) else { return err(None, "join([Text], sep)", sp) };
                let parts: Vec<String> = l.iter().map(|v| match v { Value::Text(t) => t.to_string(), o => o.to_json().to_string() }).collect();
                Ok(Value::text(&parts.join(sep)))
            }
            "print" => {
                arity(1)?;
                let s = args[0].to_json().to_string();
                self.trace.push("print", "", false, 0.0, sp, s);
                Ok(Value::Unit)
            }
            _ => err(None, format!("未知内置 {name}"), sp),
        }
    }
}

/// 函数体里引用到的名字（含嵌套 lambda 与参数名）。**宁可多收**——多收只是少复用一点缓存，
/// 少收会把别人的结果当成自己的。
fn referenced_names(f: &Function) -> BTreeSet<String> {
    fn go_block(b: &Block, out: &mut BTreeSet<String>) {
        for s in &b.statements {
            match s {
                Statement::Let { value, .. } => go(value, out),
                Statement::Function { function, .. } => go_block(&function.body, out),
                Statement::Expression(e) => go(e, out),
            }
        }
        if let Some(r) = &b.result {
            go(r, out);
        }
    }
    fn go(e: &Expr, out: &mut BTreeSet<String>) {
        match &e.kind {
            ExprKind::Name(n) => {
                out.insert(n.clone());
            }
            ExprKind::List(items) => items.iter().for_each(|x| go(x, out)),
            ExprKind::Record(fields) => fields.iter().for_each(|(_, x)| go(x, out)),
            ExprKind::Function(inner) => go_block(&inner.body, out),
            ExprKind::Call { function, arguments } => {
                go(function, out);
                arguments.iter().for_each(|x| go(x, out));
            }
            ExprKind::Field { value, .. } => go(value, out),
            ExprKind::Index { value, index } => {
                go(value, out);
                go(index, out);
            }
            ExprKind::Unary { value, .. } => go(value, out),
            ExprKind::Binary { left, right, .. } => {
                go(left, out);
                go(right, out);
            }
            ExprKind::If { condition, yes, no } => {
                go(condition, out);
                go_block(yes, out);
                go_block(no, out);
            }
            ExprKind::Block(b) => go_block(b, out),
            _ => {}
        }
    }
    let mut out = BTreeSet::new();
    go_block(&f.body, &mut out);
    out
}

/// 两个表达式结构相同吗（**忽略 Span**）。判 `lift` 的「同状态」用它：
/// `state(m)` 在源码里出现两次是两个节点、Span 不同，但它们构造的是同一个状态。
fn same_shape(a: &Expr, b: &Expr) -> bool {
    fn strip(e: &Expr) -> Json {
        let mut j = serde_json::to_value(e).unwrap_or(Json::Null);
        fn drop_spans(j: &mut Json) {
            match j {
                Json::Object(m) => {
                    m.remove("span");
                    m.values_mut().for_each(drop_spans);
                }
                Json::Array(a) => a.iter_mut().for_each(drop_spans),
                _ => {}
            }
        }
        drop_spans(&mut j);
        j
    }
    strip(a) == strip(b)
}

/// 读数用来排序的那个值；`None` = 失败或没答（J-12，排最后）
fn rank_value(r: &Reading) -> Option<f64> {
    if r.fail.is_some() {
        return None;
    }
    match r.answer_after_flush()? {
        Answer::Noul(p) => Some(p),
        Answer::Choice(v) | Answer::Score(v) => Some(argmax(&v).1),
    }
}

/// 同题跨运行合并：noul / score 取均值，choice 取众数（`12`:134）
fn merge_runs(rs: &[Rc<Reading>], sp: Span) -> R<Answer> {
    let answers: Vec<Answer> = rs.iter().filter_map(|r| r.answer_after_flush()).collect();
    if answers.is_empty() {
        return err(Some("J-12"), "agg 收到的读数全是失败或未答，没有可合并的", sp);
    }
    Ok(match &answers[0] {
        Answer::Noul(_) => {
            let ps: Vec<f64> = answers.iter().filter_map(|a| if let Answer::Noul(p) = a { Some(*p) } else { None }).collect();
            Answer::Noul(ps.iter().sum::<f64>() / ps.len() as f64)
        }
        Answer::Score(v0) => {
            // 逐档取均值
            let mut acc = vec![0.0; v0.len()];
            let mut n = 0.0;
            for a in &answers {
                if let Answer::Score(v) = a {
                    for (i, x) in v.iter().enumerate() {
                        if i < acc.len() {
                            acc[i] += x;
                        }
                    }
                    n += 1.0;
                }
            }
            Answer::Score(acc.into_iter().map(|x| x / n).collect())
        }
        Answer::Choice(v0) => {
            // 众数：每次运行投给自己的 argmax，票数最多的候选拿 1.0
            let mut votes = vec![0usize; v0.len()];
            for a in &answers {
                if let Answer::Choice(v) = a {
                    let k = argmax(v).0;
                    if k < votes.len() {
                        votes[k] += 1;
                    }
                }
            }
            let top = votes.iter().enumerate().max_by_key(|(_, n)| **n).map(|(i, _)| i).unwrap_or(0);
            Answer::Choice((0..v0.len()).map(|i| if i == top { 1.0 } else { 0.0 }).collect())
        }
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
        return err(None, "题的第三个参数要是记录：{evidence: [\"ctx\", …]}", sp);
    };
    let Some((_, slots)) = fields.iter().find(|(k, _)| k == "evidence") else { return Ok(vec![]) };
    let Value::List(l) = slots else { return err(None, "evidence 要是槽名的列表", sp) };
    let mut out = vec![];
    for s in l.iter() {
        let Value::Text(t) = s else { return err(None, "evidence 里要是槽名（文本）", sp) };
        if !matches!(t.as_ref(), "on" | "ctx" | "ref" | "over") {
            return err(None, format!("evidence 里的 {t} 不是槽名；状态只有 on / ctx / ref / over 四个槽"), sp);
        }
        out.push(t.to_string());
    }
    Ok(out)
}

/// 题上可读的字段（只读）。静态检查（check.rs）用同一张表核字段名。
pub const QUESTION_FIELDS: &[&str] = &["text", "op", "calib", "scale", "evidence", "hash", "subject", "predicate", "partition", "request", "presupposition", "form", "template", "fill"];
/// 题式上可读的字段（只读）。
pub const FORM_FIELDS: &[&str] = &["template", "op", "slots", "calib", "scale", "evidence", "presupposition", "request", "partition", "subject", "hash"];

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
pub const OUTCOME_FIELDS: &[&str] = &["kind", "value", "pending", "evidence", "resume", "spent", "detail", "purpose"];

/// 这个值是不是一个契约值（字段集合与 `OUTCOME_FIELDS` 一致）
pub fn is_outcome(v: &Value) -> bool {
    match v {
        Value::Record(r) => r.len() == OUTCOME_FIELDS.len() && OUTCOME_FIELDS.iter().all(|f| r.iter().any(|(k, _)| k == f)),
        _ => false,
    }
}

/// 证据列表去重追加（证据都是账本键 Text）
fn push_key(v: &mut Vec<Value>, k: Value) {
    let same = |a: &Value| matches!((a, &k), (Value::Text(x), Value::Text(y)) if x == y);
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
            Some(f) => Value::Record(Rc::new(f.iter().map(|(k, v)| (k.clone(), Value::text(v))).collect())),
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
        "request" => Value::text(f.request.as_deref().unwrap_or(crate::value::default_request(f.op))),
        "partition" => Value::text(match f.op { Op::Test => "binary", Op::Select => "k_ary", Op::Measure => "ordered" }),
        "subject" => Value::text(match f.op { Op::Select => "over", _ => "on" }),
        "hash" => Value::text(&f.hash),
        _ => return None,
    })
}

/// 题的声明项：前提（可选文本）与请求（本版只接受各题型的缺省请求，见下）。
fn question_decl_of(v: Option<&Value>, op: Op, sp: Span) -> R<(Option<String>, Option<String>)> {
    let Some(v) = v else { return Ok((None, None)) };
    let presupposition = match v.get("presupposition") {
        None | Some(Value::Unit) => None,
        Some(Value::Text(t)) => Some(t.to_string()),
        Some(other) => return err(None, format!("presupposition 要是文本，收到 {}", other.type_name()), sp),
    };
    let request = match v.get("request") {
        None | Some(Value::Unit) => None,
        Some(Value::Text(t)) => {
            // 本版 `cut` 只实现每个题型的缺省请求。「K 选一、选出全部」（all）要由三路过滤
            // 与子集判断承担（施工件 c），在那之前声明它只会被静默当成 one——所以拒绝，而不是收下不管。
            if t.as_ref() != crate::value::default_request(op) {
                let hint = if op == Op::Select && t.as_ref() == "all" { "；「选出全部」待三路过滤（施工件 c）实现后可用，现在用 map + test 逐个判" } else { "" };
                return err(None, format!("request 「{t}」不适用于 {} 题：本版只支持缺省请求 {}{hint}", op.fixture_name(), crate::value::default_request(op)), sp);
            }
            Some(t.to_string())
        }
        Some(other) => return err(None, format!("request 要是文本，收到 {}", other.type_name()), sp),
    };
    Ok((presupposition, request))
}

/// 把一份内容**及其所有子结构**记进表：包装成容器后也认得出来。
///
/// 为什么不按整个值精确匹配：**包装方式是无穷的**，穷举它永远落后一步。
/// 记子结构之后，判定变成「**这个新材料里含不含从 untrusted 材料拆出来的东西**」。
fn insert_with_parts(set: &mut HashSet<String>, j: &Json) {
    // 太小的标量不记：`true` / `0` / `""` 这类会撞上无关的字面量，造成假拒绝
    let 值得记 = match j {
        Json::Null | Json::Bool(_) => false,
        Json::Number(_) => false,
        Json::String(s) => s.chars().count() >= 3,
        _ => true,
    };
    if 值得记 {
        set.insert(canon(j));
    }
    match j {
        Json::Object(m) => m.values().for_each(|v| insert_with_parts(set, v)),
        Json::Array(a) => a.iter().for_each(|v| insert_with_parts(set, v)),
        _ => {}
    }
}

/// 这份内容里含不含记过的「从 untrusted 材料拆出来的东西」
fn contains_untrusted_part(set: &HashSet<String>, j: &Json) -> bool {
    if set.contains(&canon(j)) {
        return true;
    }
    match j {
        Json::Object(m) => m.values().any(|v| contains_untrusted_part(set, v)),
        Json::Array(a) => a.iter().any(|v| contains_untrusted_part(set, v)),
        _ => false,
    }
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

/// 一个值携带的 taint：容器要递归看。**没有可信度可言的东西不叫可信**——
/// 但语言里只有材料带 taint，其余（数字、文本、方法）本来就是程序自己造的，按 Trusted。
fn taint_of(v: &Value) -> Taint {
    match v {
        Value::Mat(m) => m.taint,
        Value::List(l) => l.iter().fold(Taint::Trusted, |t, x| Taint::join(t, taint_of(x))),
        Value::Record(fs) => fs.iter().fold(Taint::Trusted, |t, (_, x)| Taint::join(t, taint_of(x))),
        Value::Exit(e) => e.taint,
        Value::Stop(x) => taint_of(x),
        _ => Taint::Trusted,
    }
}

/// 这个表达式是 `judge(状态, 题)` 吗；是的话给出**状态那一段的语法树**
fn judged_state(e: &Expr) -> Option<&Expr> {
    let ExprKind::Call { function, arguments } = &e.kind else { return None };
    let ExprKind::Name(n) = &function.kind else { return None };
    if n != "judge" {
        return None;
    }
    arguments.first()
}

/// 表达式里有没有会产生副作用或改状态的调用（提升不能跨过它们）
fn has_impure(e: &Expr) -> bool {
    let mut found = false;
    walk(e, &mut |x| {
        if let ExprKind::Call { function, .. } = &x.kind {
            if let ExprKind::Name(n) = &function.kind {
                if matches!(n.as_str(), "do" | "gen" | "ask" | "transform" | "escalate" | "literalize") {
                    found = true;
                }
            }
        }
    });
    found
}

/// 表达式里有没有分支或循环（**不跨分支**：`12`:13）
fn has_branch(e: &Expr) -> bool {
    let mut found = false;
    walk(e, &mut |x| match &x.kind {
        ExprKind::If { .. } => found = true,
        ExprKind::Call { function, .. } => {
            if let ExprKind::Name(n) = &function.kind {
                if matches!(n.as_str(), "loop" | "iterate" | "map" | "filter" | "fold" | "handle" | "pair") {
                    found = true;
                }
            }
        }
        _ => {}
    });
    found
}

fn names_in(e: &Expr, out: &mut BTreeSet<String>) {
    walk(e, &mut |x| {
        if let ExprKind::Name(n) = &x.kind {
            out.insert(n.clone());
        }
    });
}

fn walk(e: &Expr, f: &mut impl FnMut(&Expr)) {
    f(e);
    match &e.kind {
        ExprKind::List(items) => items.iter().for_each(|x| walk(x, f)),
        ExprKind::Record(fs) => fs.iter().for_each(|(_, x)| walk(x, f)),
        ExprKind::Function(_) => {}
        ExprKind::Call { function, arguments } => {
            walk(function, f);
            arguments.iter().for_each(|x| walk(x, f));
        }
        ExprKind::Field { value, .. } => walk(value, f),
        ExprKind::Index { value, index } => {
            walk(value, f);
            walk(index, f);
        }
        ExprKind::Unary { value, .. } => walk(value, f),
        ExprKind::Binary { left, right, .. } => {
            walk(left, f);
            walk(right, f);
        }
        ExprKind::If { condition, .. } => walk(condition, f),
        _ => {}
    }
}

/// 13 §6 的运行错误：说清是哪一步越界、越的是哪个界，并带 `.jpp` 的 Span
fn overflow(what: &str, a: i64, b: i64, sp: Span) -> Fault {
    Fault::Error(RtError::new(
        None,
        format!("Int {what}溢出：{a} 与 {b} 的结果超出有符号 64 位范围（{} … {}）。Int 是 64 位有符号整数，溢出是错误不是回绕", i64::MIN, i64::MAX),
        sp,
    ))
}

fn argmax(v: &[f64]) -> (usize, f64) {
    let mut best = (0usize, f64::MIN);
    for (i, p) in v.iter().enumerate() {
        if *p > best.1 {
            best = (i, *p);
        }
    }
    best
}

pub fn json_to_value(j: &Json) -> Value {
    match j {
        Json::Null => Value::Unit,
        Json::Bool(b) => Value::Bool(*b),
        Json::Number(n) => n.as_i64().map(Value::Int).unwrap_or_else(|| Value::Float(n.as_f64().unwrap_or(0.0))),
        Json::String(s) => Value::text(s),
        Json::Array(a) => Value::list(a.iter().map(json_to_value).collect()),
        Json::Object(o) => Value::record(o.iter().map(|(k, v)| (k.clone(), json_to_value(v))).collect()),
    }
}

pub fn effect_value_to_json(v: &Value) -> Json {
    match v {
        Value::Fail(s) => json!({"__fail": s.as_ref()}),
        // `derived_from` 也要写：不写的话重放出来的程序与原程序**在 J-02 上不是同一个程序**，
        // 而 J-18 的整套重放判定建立在它们是同一个上。
        Value::Mat(m) => json!({"__mat": m.content, "taint": m.taint, "addr": m.addr, "origin": m.origin, "derived_from": m.derived_from}),
        other => other.to_json(),
    }
}

pub fn json_to_effect_value(j: &Json) -> Value {
    if let Some(f) = j.get("__fail").and_then(|x| x.as_str()) {
        return Value::Fail(Rc::from(f));
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
        let origin: Vec<String> = serde_json::from_value(j.get("origin").cloned().unwrap_or(json!([]))).unwrap_or_default();
        let derived: BTreeSet<String> = serde_json::from_value(j.get("derived_from").cloned().unwrap_or(json!([]))).unwrap_or_default();
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
fn collect_captured_exits(env: &Env, names: &BTreeSet<String>, depth: u32, out: &mut HashSet<usize>) {
    if depth == 0 {
        return;
    }
    for n in names {
        let Some(v) = env_lookup(env, n) else { continue };
        match &v {
            Value::Fn(c) => collect_captured_exits(&c.env, &referenced_names(&c.function), depth - 1, out),
            other => collect_exit_ids(other, out),
        }
    }
}

/// 一个元素交给判断器的那份材料与来路：
/// 过滤或配对的产物（带 `item` 与 `trail` 的记录）取 `item` 当材料，`trail` 接上上一次的出口；
/// 其余值原样当材料、来路为空。产物与输入同形，可再过滤、再配对（组合封闭）。
fn element_parts(it: &Value) -> (Value, Value) {
    let is_elem = matches!(it, Value::Record(_)) && it.get("item").is_some() && it.get("trail").is_some();
    if !is_elem {
        return (it.clone(), Value::list(vec![]));
    }
    let mut t: Vec<Value> = match it.get("trail") { Some(Value::List(l)) => l.iter().cloned().collect(), _ => vec![] };
    if let Some(e) = it.get("exit") {
        if !matches!(e, Value::Unit) {
            t.push(e);
        }
    }
    (it.get("item").unwrap_or(Value::Unit), Value::list(t))
}
