//! 运行期值。`Mat`（材料）与 `Reading`（读数）是不同变体：读数只能经 `cut` 离开（J-01）。
//! 函数值带显式环境链 `Env`，没有 Rust 闭包，可打印、可序列化成名字→值。

use std::cell::{Cell, RefCell};
use std::collections::BTreeSet;
use std::rc::Rc;

use serde::{Deserialize, Serialize};
use serde_json::{Value as Json, json};
use sha2::{Digest, Sha256};

use crate::ast::{Function, Span};

pub fn hash_of(parts: &[&str]) -> String {
    let mut h = Sha256::new();
    for p in parts {
        h.update(p.as_bytes());
        h.update(b"\x1f");
    }
    hex::encode(&h.finalize()[..12])
}

mod hex {
    pub fn encode(b: &[u8]) -> String {
        b.iter().map(|x| format!("{x:02x}")).collect()
    }
}

/// 规范化 JSON（键排序）——账本键与缓存键都建在它上面（§2.10）。
pub fn canon(j: &Json) -> String {
    match j {
        Json::Object(m) => {
            let mut keys: Vec<_> = m.keys().collect();
            keys.sort();
            let inner: Vec<String> = keys.iter().map(|k| format!("{}:{}", serde_json::to_string(k).unwrap(), canon(&m[*k]))).collect();
            format!("{{{}}}", inner.join(","))
        }
        Json::Array(a) => format!("[{}]", a.iter().map(canon).collect::<Vec<_>>().join(",")),
        // 指数写法要与 Python 的 `json.dumps` 一致：它按 repr 出 `4.2e-08`（两位指数），
        // serde_json 出 `4.2e-8`。这个差别会让同一份档案在两边算出不同的哈希，
        // 而 profile_hash 进账本头——两边对不上，跨内核的重放判定就废了。
        Json::Number(n) => {
            let s = n.to_string();
            match s.split_once('e') {
                Some((mant, exp)) => {
                    let (sign, digits) = match exp.strip_prefix('-') {
                        Some(d) => ("-", d),
                        None => ("+", exp.strip_prefix('+').unwrap_or(exp)),
                    };
                    format!("{mant}e{sign}{:0>2}", digits)
                }
                None => s,
            }
        }
        other => other.to_string(),
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum Taint {
    Trusted,
    // 反序列化缺字段时按保守那边兜底（与 json_to_effect_value 同一条判据）
    #[default]
    Untrusted,
}

impl Taint {
    pub fn join(a: Taint, b: Taint) -> Taint {
        if a == Taint::Untrusted || b == Taint::Untrusted { Taint::Untrusted } else { Taint::Trusted }
    }
}

/// 唯一材料类型（§2）。
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Mat {
    pub content: Json,
    pub addr: String,
    pub modality: String,
    /// 来源链：literal / gen:<key> / do:<key> / ask:<key> / transform:<key> / exit:<q>
    pub origin: Vec<String>,
    pub taint: Taint,
    /// 由哪些题派生（J-02 禁自指）
    pub derived_from: BTreeSet<String>,
    pub hash: String,
}

impl Mat {
    pub fn new(content: Json, addr: &str, origin: Vec<String>, taint: Taint, derived_from: BTreeSet<String>) -> Mat {
        let hash = hash_of(&["mat", &canon(&content), addr]);
        Mat { content, addr: addr.to_string(), modality: "text".into(), origin, taint, derived_from, hash }
    }
    pub fn literal(content: Json) -> Mat {
        Mat::new(content, "", vec!["literal".into()], Taint::Trusted, BTreeSet::new())
    }
    /// 估算 token。与 Python 的 `ir.py:84` **同一个估法**（`int(len(canon)/1.3) + 1`，
    /// 依据是档案 `cost.regression` 的 state_char_coef≈1、1.3 字符/token）——
    /// 两边估法不同，窗口检查就会在不同的地方触发。
    ///
    /// 窗口检查（J-14 / `12`:117）要它：对象槽内单段按 `window.text_slots…usable_lower`，
    /// 槽间按 `window.json_slots`。超窗的后果不是算错，是**读数被语境接管而无人察觉**
    /// ——档案原话「≈1,000 token 带主张语境下翻转 60.7%，读数被语境接管」。
    pub fn tokens(&self) -> usize {
        (canon(&self.content).chars().count() as f64 / 1.3) as usize + 1
    }
    pub fn text(&self) -> String {
        match &self.content {
            Json::String(s) => s.clone(),
            other => other.to_string(),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Op {
    Test,
    Select,
    Measure,
}

impl Op {
    /// 夹具 JSON 里认的名字（`test` / `select` / `measure`）。
    ///
    /// **与 `phys()`（`noul`/`choice`/`score`）不是一回事**——前者是题式、后者是物理形式。
    /// 未命中报文以前打的是 `phys()`，**于是那句「照抄这两份 JSON」是假的**：
    /// 夹具只认 `test`。
    pub fn fixture_name(&self) -> &'static str {
        match self {
            Op::Test => "test",
            Op::Select => "select",
            Op::Measure => "measure",
        }
    }
    pub fn phys(&self) -> &'static str {
        match self {
            Op::Test => "noul",
            Op::Select => "choice",
            Op::Measure => "score",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Question {
    pub op: Op,
    pub text: String,
    /// 校准键；线只从校准记录来（I4 / J-03）
    pub calib: String,
    /// measure 的档位
    pub scale: Vec<String>,
    /// J-09 的**决定性证据槽**名（`on` / `ctx` / `ref` / `over`）。
    /// `cut` 判序第一步：这些槽不在状态里就**不信任 p**（`12`:148、:244）。
    #[serde(default)]
    pub evidence: Vec<String>,
    pub hash: String,
}

impl Question {
    pub fn new(op: Op, text: &str, calib: &str, scale: Vec<String>) -> Question {
        Question::with_evidence(op, text, calib, scale, vec![])
    }
    /// 带决定性证据槽的题（J-09）。`evidence` 进题的哈希——**声明了证据的题和没声明的
    /// 不是同一道题**，账本键按题哈希走，不能让它们共用一条记录。
    pub fn with_evidence(op: Op, text: &str, calib: &str, scale: Vec<String>, evidence: Vec<String>) -> Question {
        let hash = hash_of(&["q", op.phys(), text, &scale.join("\u{1e}"), &evidence.join("\u{1e}")]);
        Question { op, text: text.to_string(), calib: calib.to_string(), scale, evidence, hash }
    }
}

/// 状态 = 具名槽（§2.1）。`on` 恰一个对象（或一对），`over` 是候选。
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct State {
    pub on: Vec<Mat>,
    pub ctx: Vec<Mat>,
    pub r#ref: Vec<Mat>,
    pub over: Vec<Mat>,
    pub taint: Taint,
    pub derived_from: BTreeSet<String>,
    /// 含槽结构的规范化 JSON 的哈希（§2.10）
    pub hash: String,
    pub has_fail: bool,
}

impl State {
    pub fn new(on: Vec<Mat>, ctx: Vec<Mat>, r#ref: Vec<Mat>, over: Vec<Mat>, has_fail: bool) -> State {
        let all = on.iter().chain(&ctx).chain(&r#ref).chain(&over);
        let mut taint = Taint::Trusted;
        let mut derived = BTreeSet::new();
        for m in all {
            taint = Taint::join(taint, m.taint);
            derived.extend(m.derived_from.iter().cloned());
        }
        let mut s = State { on, ctx, r#ref, over, taint, derived_from: derived, hash: String::new(), has_fail };
        s.hash = hash_of(&["state", &canon(&s.to_json())]);
        s
    }
    /// **夹具形状**的状态 JSON：`on` 恒为列表。
    ///
    /// 与 [`State::to_json`] 的差别只有一处：那个在 `on` 只有一个对象时**摊成对象**
    /// （因为送给模型时那样更自然），**而夹具只收列表**
    /// （`invalid type: map, expected a sequence`）。
    /// 未命中报文说「照抄这两份 JSON」，**打 `to_json` 那句话就是假的**。
    pub fn as_fixture_json(&self) -> Json {
        let m = |v: &Vec<Mat>| Json::Array(v.iter().map(|x| x.content.clone()).collect());
        let mut o = serde_json::Map::new();
        o.insert("on".into(), m(&self.on));
        for (k, v) in [("ctx", &self.ctx), ("ref", &self.r#ref), ("over", &self.over)] {
            if !v.is_empty() {
                o.insert(k.into(), m(v));
            }
        }
        Json::Object(o)
    }

    /// 送给模型的状态 JSON（H1：题面按路径引用槽）。
    pub fn to_json(&self) -> Json {
        let m = |v: &Vec<Mat>| Json::Array(v.iter().map(|x| x.content.clone()).collect());
        let mut o = serde_json::Map::new();
        o.insert("on".into(), if self.on.len() == 1 { self.on[0].content.clone() } else { m(&self.on) });
        if !self.ctx.is_empty() {
            o.insert("ctx".into(), m(&self.ctx));
        }
        if !self.r#ref.is_empty() {
            o.insert("ref".into(), m(&self.r#ref));
        }
        if !self.over.is_empty() {
            o.insert("over".into(), m(&self.over));
        }
        Json::Object(o)
    }
}

/// 模型对一题的回答（校验后的规范形）。
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Answer {
    Noul(f64),
    /// 按 over 下标的概率
    Choice(Vec<f64>),
    /// 按档位下标的概率
    Score(Vec<f64>),
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Reading {
    pub q_hash: String,
    pub state_hash: String,
    pub op: Op,
    pub calib: String,
    /// 惰性：登记时是 `None`，到刷新点一层发出后才填上（`12` §2.2「登记后不发」）
    pub answer: RefCell<Option<Answer>>,
    /// 状态含 Fail 材料时读数为空（J-12 → Unsure(fail)）
    pub fail: Option<String>,
    pub model_id: String,
    pub ledger_key: String,
    pub over_len: usize,
    pub scale: Vec<String>,
    /// 用了几个置换。**与 `mode_share` 成对**——K 是那个测量身份的一部分。
    #[serde(default)]
    pub perms: std::cell::Cell<usize>,
    /// 置换众数占比（`12`:151）。`None` = 这条路上没测过置换 → `cut` 不给 `Pick`。
    /// 与 `answer` 同样是刷新时才填上的，所以是 `Cell`。
    #[serde(default, skip)]
    pub mode_share: std::cell::Cell<Option<f64>>,
    /// 这道题声明了、而状态里没有的决定性证据槽（J-09）。`cut` 判序第一步据此给
    /// `Unsure(insufficient)`——**在看 p 之前**。登记时就算好，因为那时状态还在手上。
    #[serde(default)]
    pub missing_evidence: Vec<String>,
    /// 被判断的那个状态的 taint。`cut` 据此给出口定 taint
    /// （`12`:150「出口 taint 继承状态 taint」、§2.11「cut 继承」）。
    /// 以前没有这个字段，`cut` 只好一律给 `Trusted`——**状态算好的 taint 被丢掉了**。
    #[serde(default)]
    pub state_taint: Taint,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum ExitKind {
    Act,
    Ignore,
    Pick(usize),
    At(usize),
    Unsure(String),
}

#[derive(Debug)]
pub struct Exit {
    pub id: usize,
    /// 出口来自哪种题（handle 的穷尽分支按它定）
    pub op: Op,
    pub kind: ExitKind,
    pub q_hash: String,
    pub state_hash: String,
    pub taint: Taint,
    pub site: Span,
    /// 这个出口是不是经 `ask` 来的（J-08「或经 `ask`」；人答是 trusted，§2.11）
    pub from_ask: Cell<bool>,
    pub consumed: Cell<bool>,
    pub consumed_by: RefCell<String>,
    /// **这条线是哪一级的**：`""` = 没用上线（冷 / 停岗 / fail）、`"题级"` = 这道题自己的、
    /// `"模式级"` = 借的同类先验（`12`:136）。
    ///
    /// **它必须对 handler 可见**，理由与 J-15 那一位同：**模式级的线不能冒充题级的线**。
    /// 看不见来源，handler 就只能把「这道题测过 200 条」和「这类题测过 200 条、这道题
    /// 一条没有」当同一件事办——**那是把两种证据强度压平**。
    pub line_source: String,
    /// **J-15 的那一位**：本次路径上有没有一个**被声明为判据、却没有被测量的量**。
    /// `None` = 该出口引用的判据都测过；`Some(载体)` = 那个量没测过（载体只作诊断）。
    ///
    /// **它不是 `cause`，这是要点。** `cause` 的本分是**路由键**（§5 handler 库按它分流）。
    /// 把「测没测过」塞进 `cause` 会得到 `cold` / `no_perm` / `no_ece` / `no_klimit`……
    /// 而 **`cold` 的存在正是证据：那条路已经走过一次，然后停了**。每多一个 `cause`，
    /// handler 库就多一条要记得加的路由，**而漏加路由的失效方式是静默的**——`otherwise`
    /// 兜住，没人知道。所以它是**正交的一位**：跟着出口走，`cause` 不动。
    ///
    /// 判别法（`12` §2.11）：这个区别是「这一格特有的」还是「会在很多格上重复出现的」？
    /// 今天已经有五个载体同处「没测过」：线未测（`cold`）、置换未测（`mode_share == None`）、
    /// 档案字段未测（`choice_same_call_perm_crosstalk`）、`k_limit` 120–250 档未测、
    /// ECE 未过检。重复出现 → 加维度，不加 `cause`。
    pub untested: Option<String>,
}

impl Exit {
    pub fn is_unsure(&self) -> bool {
        matches!(self.kind, ExitKind::Unsure(_))
    }
    /// 这个出口引用的判据里没被测量的那个量（J-15 的那一位）。**读取不转移责任。**
    /// 与 `cause()` 正交：`cause()` 回答「往哪条路由走」，这个回答「那条路上的判据测没测过」。
    pub fn untested(&self) -> Option<&str> {
        self.untested.as_deref()
    }
    /// 未决的原因（`band` / `cold` / `tie` / `untested` / `fail:…`）。**它是路由键**
    /// （§5 handler 库按它分流），不承载「测没测过」——那一位见 [`Exit::untested`]。
    /// 读取不转移责任。
    pub fn cause(&self) -> String {
        match &self.kind {
            ExitKind::Unsure(c) => c.clone(),
            other => format!("{other:?}"),
        }
    }
    pub fn label(&self) -> String {
        match &self.kind {
            ExitKind::Act => "act".into(),
            ExitKind::Ignore => "ignore".into(),
            ExitKind::Pick(k) => format!("pick({k})"),
            ExitKind::At(l) => format!("at({l})"),
            // 那一位进 label，因为 `exit_kind(e)` 是程序**自己带进返回值**的审计面
            // （出口不进 `Ledger`——那里只有 Judge/Effect/Ask 三种条目）。
            ExitKind::Unsure(c) => match &self.untested {
                // 通用 cause `untested`（没有既有路由可骑的那一类）：载体直接跟在后面
                Some(carrier) if c == "untested" => format!("unsure(untested:{carrier})"),
                // 骑既有路由的那一类（如 `cold`）：**路由键不动**，那一位挂在后面
                Some(carrier) => format!("unsure({c}|untested:{carrier})"),
                None => format!("unsure({c})"),
            },
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Pending {
    pub cause: String,
    pub key: String,
    pub site: Span,
    pub detail: String,
}

/// 显式环境链：名字→值，可打印。
#[derive(Debug)]
pub struct EnvNode {
    pub vars: RefCell<Vec<(String, Value)>>,
    pub parent: Option<Env>,
}
pub type Env = Rc<EnvNode>;

pub fn env_root() -> Env {
    Rc::new(EnvNode { vars: RefCell::new(vec![]), parent: None })
}
pub fn env_child(parent: &Env) -> Env {
    Rc::new(EnvNode { vars: RefCell::new(vec![]), parent: Some(parent.clone()) })
}
impl Reading {
    /// 已经填上的答案；`None` = **还没刷新**。
    ///
    /// **判据（比清单更可靠）：凡结果依赖于答案的操作，都是刷新点**——调这个方法之前必须先
    /// `flush`。`12` §2.2:129 列了 `cut`/`fit`/`match`/`if`/读内容五项，但清单形式会漏：
    /// 每加一个读答案的操作就要记得补一项，而**漏补的失效方式是静默给出错误结果**
    /// （`allocate` 漏了刷新时选出 `[0,1,2,3]` 而不是 `[2,4,6,8]`，不报错也不告警）。
    ///
    /// 所以这个名字带着 `_after_flush`：它在**每个调用点**把这条判据摆在眼前，
    /// 而不是指望作者记得去查清单。新增读答案的操作时，先问「它的结果依赖答案吗」，
    /// 依赖就先 `flush`。
    pub fn answer_after_flush(&self) -> Option<Answer> {
        self.answer.borrow().clone()
    }
    pub fn set_mode_share(&self, ms: f64, perms: usize) {
        self.mode_share.set(Some(ms));
        self.perms.set(perms);
    }
    pub fn fill(&self, a: Answer) {
        *self.answer.borrow_mut() = Some(a);
    }
}

pub fn env_lookup(env: &Env, name: &str) -> Option<Value> {
    let mut cur = Some(env.clone());
    while let Some(e) = cur {
        if let Some((_, v)) = e.vars.borrow().iter().rev().find(|(n, _)| n == name) {
            return Some(v.clone());
        }
        cur = e.parent.clone();
    }
    None
}
pub fn env_define(env: &Env, name: &str, v: Value) {
    env.vars.borrow_mut().push((name.to_string(), v));
}
/// 环境的可读形式（只列名字，避免打印巨大值；给 INTERFACE 的「函数值环境表示」）
pub fn env_names(env: &Env) -> Vec<Vec<String>> {
    let mut out = vec![];
    let mut cur = Some(env.clone());
    while let Some(e) = cur {
        out.push(e.vars.borrow().iter().map(|(n, _)| n.clone()).collect());
        cur = e.parent.clone();
    }
    out
}

#[derive(Debug)]
pub struct Closure {
    pub function: Function,
    pub env: Env,
    pub name: Option<String>,
    pub span: Span,
    /// 函数体的结构哈希（transform 的键用）
    pub hash: String,
}

#[derive(Clone, Debug)]
pub enum Value {
    Unit,
    Int(i64),
    Float(f64),
    Bool(bool),
    Text(Rc<str>),
    List(Rc<Vec<Value>>),
    Record(Rc<Vec<(String, Value)>>),
    Fn(Rc<Closure>),
    Builtin(&'static str),
    Mat(Rc<Mat>),
    State(Rc<State>),
    Question(Rc<Question>),
    Reading(Rc<Reading>),
    Exit(Rc<Exit>),
    /// 未决责任 `U(q)`：`handle` 的 unsure 臂收到的就是它。不可伪造（只能由 handle 交付）、
    /// 不能默默变成材料或 JSON 就算销账。与出口共享同一个 `Rc<Exit>`，销账记录是同一份。
    Duty(Rc<Exit>),
    /// `do` 的失败值（J-12）
    Fail(Rc<str>),
    /// `stop(v)`：有界循环的显式停止
    Stop(Rc<Value>),
}

impl Value {
    pub fn text(s: &str) -> Value {
        Value::Text(Rc::from(s))
    }
    pub fn list(v: Vec<Value>) -> Value {
        Value::List(Rc::new(v))
    }
    pub fn record(v: Vec<(String, Value)>) -> Value {
        Value::Record(Rc::new(v))
    }
    pub fn type_name(&self) -> &'static str {
        match self {
            Value::Unit => "Unit",
            Value::Int(_) => "Int",
            Value::Float(_) => "Float",
            Value::Bool(_) => "Bool",
            Value::Text(_) => "Text",
            Value::List(_) => "List",
            Value::Record(_) => "Record",
            Value::Fn(_) => "Fn",
            Value::Builtin(_) => "Builtin",
            Value::Mat(_) => "Mat",
            Value::State(_) => "State",
            Value::Question(_) => "Question",
            Value::Reading(_) => "Reading",
            Value::Exit(_) => "Exit",
            Value::Duty(_) => "Unsure",
            Value::Fail(_) => "Fail",
            Value::Stop(_) => "Stop",
        }
    }
    pub fn get(&self, key: &str) -> Option<Value> {
        match self {
            Value::Record(r) => r.iter().find(|(k, _)| k == key).map(|(_, v)| v.clone()),
            _ => None,
        }
    }
    /// 可打印 / 可序列化形式。读数只露元数据不露概率（读数不能被宿主当数用，J-01）。
    pub fn to_json(&self) -> Json {
        match self {
            Value::Unit => Json::Null,
            Value::Int(i) => json!(i),
            Value::Float(f) => json!(f),
            Value::Bool(b) => json!(b),
            Value::Text(s) => json!(s.as_ref()),
            Value::List(l) => Json::Array(l.iter().map(|v| v.to_json()).collect()),
            Value::Record(r) => {
                let mut m = serde_json::Map::new();
                for (k, v) in r.iter() {
                    m.insert(k.clone(), v.to_json());
                }
                Json::Object(m)
            }
            Value::Fn(c) => json!({"fn": c.name, "params": c.function.parameters.iter().map(|p| p.name.clone()).collect::<Vec<_>>(), "env": env_names(&c.env), "hash": c.hash}),
            Value::Builtin(n) => json!({"builtin": n}),
            Value::Mat(m) => json!({"mat": m.hash, "content": m.content, "taint": m.taint, "origin": m.origin}),
            Value::State(s) => json!({"state": s.hash, "slots": s.to_json(), "taint": s.taint}),
            Value::Question(q) => json!({"question": q.hash, "op": q.op.phys(), "text": q.text, "calib": q.calib}),
            Value::Reading(r) => json!({"reading": r.ledger_key, "q": r.q_hash, "state": r.state_hash, "op": r.op.phys()}),
            Value::Exit(e) => json!({"exit": e.label(), "id": e.id, "consumed": e.consumed.get(), "q": e.q_hash}),
            Value::Duty(e) => json!({"unsure": e.cause(), "duty": e.id, "q": e.q_hash}),
            Value::Fail(s) => json!({"fail": s.as_ref()}),
            Value::Stop(v) => json!({"stop": v.to_json()}),
        }
    }
    /// 这个值可比吗；`None` = 里面有读数（J-01）。容器要递归看，装进列表或记录不改变这件事。
    pub fn comparable(&self) -> Option<()> {
        match self {
            Value::Reading(_) => None,
            Value::List(l) => l.iter().try_for_each(|x| x.comparable()),
            Value::Record(fs) => fs.iter().try_for_each(|(_, v)| v.comparable()),
            _ => Some(()),
        }
    }

    /// 结构相等（函数按哈希）；读数不可比（返回 None，调用方报 J-01）
    pub fn equals(&self, other: &Value) -> Option<bool> {
        Some(match (self, other) {
            (Value::Reading(_), _) | (_, Value::Reading(_)) => return None,
            (Value::Unit, Value::Unit) => true,
            (Value::Int(a), Value::Int(b)) => a == b,
            (Value::Float(a), Value::Float(b)) => a == b,
            (Value::Int(a), Value::Float(b)) | (Value::Float(b), Value::Int(a)) => (*a as f64) == *b,
            (Value::Bool(a), Value::Bool(b)) => a == b,
            (Value::Text(a), Value::Text(b)) => a == b,
            // 容器里的「不可比」要传上来，不能被 `== Some(true)` 悄悄吃成 false：
            // 读数装进列表或记录还是读数，没有可读的值（J-01）。
            (Value::List(a), Value::List(b)) => {
                if a.iter().chain(b.iter()).any(|x| x.comparable().is_none()) {
                    return None;
                }
                a.len() == b.len() && a.iter().zip(b.iter()).all(|(x, y)| x.equals(y) == Some(true))
            }
            (Value::Record(a), Value::Record(b)) => {
                if a.iter().chain(b.iter()).any(|(_, v)| v.comparable().is_none()) {
                    return None;
                }
                a.len() == b.len() && a.iter().all(|(k, v)| b.iter().any(|(k2, v2)| k == k2 && v.equals(v2) == Some(true)))
            }
            (Value::Mat(a), Value::Mat(b)) => a.hash == b.hash,
            (Value::State(a), Value::State(b)) => a.hash == b.hash,
            (Value::Question(a), Value::Question(b)) => a.hash == b.hash,
            (Value::Exit(a), Value::Exit(b)) => a.kind == b.kind,
            // 责任按身份比：同一道题的两个未决是两份责任
            (Value::Duty(a), Value::Duty(b)) => a.id == b.id,
            (Value::Fn(a), Value::Fn(b)) => a.hash == b.hash,
            (Value::Builtin(a), Value::Builtin(b)) => a == b,
            (Value::Fail(a), Value::Fail(b)) => a == b,
            _ => false,
        })
    }
}
