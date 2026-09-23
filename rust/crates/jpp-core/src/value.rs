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
        other => other.to_string(),
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Taint {
    Trusted,
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
    pub hash: String,
}

impl Question {
    pub fn new(op: Op, text: &str, calib: &str, scale: Vec<String>) -> Question {
        let hash = hash_of(&["q", op.phys(), text, &scale.join("\u{1e}")]);
        Question { op, text: text.to_string(), calib: calib.to_string(), scale, hash }
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
    pub answer: Option<Answer>,
    /// 状态含 Fail 材料时读数为空（J-12 → Unsure(fail)）
    pub fail: Option<String>,
    pub model_id: String,
    pub ledger_key: String,
    pub over_len: usize,
    pub scale: Vec<String>,
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
    pub consumed: Cell<bool>,
    pub consumed_by: RefCell<String>,
}

impl Exit {
    pub fn is_unsure(&self) -> bool {
        matches!(self.kind, ExitKind::Unsure(_))
    }
    /// 未决的原因（`band` / `cold` / `tie` / `fail:…`）。读取不转移责任。
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
            ExitKind::Unsure(c) => format!("unsure({c})"),
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
            (Value::List(a), Value::List(b)) => a.len() == b.len() && a.iter().zip(b.iter()).all(|(x, y)| x.equals(y) == Some(true)),
            (Value::Record(a), Value::Record(b)) => {
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
