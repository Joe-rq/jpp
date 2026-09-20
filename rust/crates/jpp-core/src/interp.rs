//! 解释器：逐语句即时执行；效应即时发出（惰性融合是后续优化，未迁移）。
//! 纪律由这里与检查器把关：J-01 读数不进槽、J-02 禁自指、J-03 线来自校准记录、J-05 unsure 必消费、
//! J-06 有界循环 + 键重复即停、J-07 预算超即停（Pending）、J-12 Fail 是值、J-13 序号、J-18 账本头。

use std::collections::{BTreeSet, HashMap, HashSet};
use std::rc::Rc;

use serde_json::{Value as Json, json};

use crate::ast::*;
use crate::effects::{CalibStore, Client};
use crate::ledger::{Entry, Header, Ledger, RENDER_VERSION, Trace, effect_key, judge_key};
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

#[derive(Debug)]
pub struct Outcome {
    /// 程序值；挂起时为 None
    pub value: Option<Value>,
    pub pending: Vec<Pending>,
    pub trace: Trace,
    pub cost: Cost,
    /// 最外层带出的未消费 Unsure（v0.1.1：允许并记）
    pub returned_unsure: Vec<String>,
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
    calib: &'a CalibStore,
    actions: &'a ActionRegistry,
    budget: Budget,
    pub trace: Trace,
    pub cost: Cost,
    frames: Vec<Frame>,
    loops: Vec<LoopCtx>,
    next_exit: usize,
    depth: u32,
    run_seq: u64,
    model_id: String,
}

pub const BUILTINS: &[&str] = &[
    "state",
    "test",
    "select",
    "measure",
    "judge",
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

impl<'a> Interp<'a> {
    pub fn new(
        client: &'a mut dyn Client,
        ledger: &'a mut Ledger,
        calib: &'a CalibStore,
        actions: &'a ActionRegistry,
        budget: Budget,
    ) -> Interp<'a> {
        let model_id = client.model_id();
        Interp {
            client,
            ledger,
            calib,
            actions,
            budget,
            trace: Trace::default(),
            cost: Cost::default(),
            frames: vec![],
            loops: vec![],
            next_exit: 0,
            depth: 0,
            run_seq: 0,
            model_id,
        }
    }

    pub fn run(mut self, program: &Program) -> Result<Outcome, RtError> {
        self.ledger.set_header(Header {
            budget_calls: self.budget.calls,
            budget_cost: self.budget.cost,
            model_id: self.model_id.clone(),
            render_version: RENDER_VERSION.into(),
            handler_version: HANDLER_VERSION.into(),
        });
        if let Some(w) = self.ledger.header_warning.take() {
            self.trace.warn(w);
        }
        self.frames.push(Frame {
            name: "<program>".into(),
            exits: vec![],
            returns_exit: true,
        });
        let env = env_child(&root_env());
        let result = self.eval_block(&program.body, &env);
        match result {
            Ok(v) => {
                let frame = self.frames.pop().unwrap();
                let mut in_value = HashSet::new();
                collect_exit_ids(&v, &mut in_value);
                let mut returned = vec![];
                for e in frame
                    .exits
                    .iter()
                    .filter(|e| e.is_unsure() && !e.consumed.get())
                {
                    if in_value.contains(&e.id) {
                        e.consumed.set(true);
                        *e.consumed_by.borrow_mut() = "returned".into();
                        returned.push(e.label());
                    } else {
                        return Err(RtError::new(
                            Some("J-05"),
                            format!(
                                "程序结束时有未消费的 {}（题 {}）。修法：用 handle(e, {{…, unsure: …}}) 或 consume(e, \"drop\") 处理，或把它带在返回值里",
                                e.label(),
                                &e.q_hash[..8]
                            ),
                            e.site,
                        ));
                    }
                }
                if !returned.is_empty() {
                    self.trace
                        .warn(format!("returned_unsure: {}", returned.join(", ")));
                }
                Ok(Outcome {
                    value: Some(v),
                    pending: vec![],
                    trace: self.trace,
                    cost: self.cost,
                    returned_unsure: returned,
                })
            }
            Err(Fault::Halt(p)) => Ok(Outcome {
                value: None,
                pending: vec![p],
                trace: self.trace,
                cost: self.cost,
                returned_unsure: vec![],
            }),
            Err(Fault::Error(e)) => Err(e),
        }
    }

    fn frame(&mut self) -> &mut Frame {
        self.frames.last_mut().unwrap()
    }

    // ---------- 求值 ----------

    fn eval_block(&mut self, b: &Block, env: &Env) -> R<Value> {
        let env = env_child(env);
        for s in &b.statements {
            match s {
                Statement::Let { name, value, .. } => {
                    let v = self.eval(value, &env)?;
                    env_define(&env, name, v);
                }
                Statement::Function {
                    name,
                    function,
                    span,
                } => {
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
        Value::Fn(Rc::new(Closure {
            function: f.clone(),
            env: env.clone(),
            name,
            span,
            hash,
        }))
    }

    fn eval(&mut self, e: &Expr, env: &Env) -> R<Value> {
        let sp = e.span;
        match &e.kind {
            ExprKind::Integer(i) => Ok(Value::Int(*i)),
            ExprKind::Decimal(d) => Ok(Value::Float(*d)),
            ExprKind::Bool(b) => Ok(Value::Bool(*b)),
            ExprKind::Text(t) => Ok(Value::text(t)),
            ExprKind::Unit => Ok(Value::Unit),
            ExprKind::Name(n) => env_lookup(env, n)
                .ok_or_else(|| Fault::Error(RtError::new(None, format!("未定义的名字 {n}"), sp))),
            ExprKind::List(items) => {
                let mut v = Vec::with_capacity(items.len());
                for it in items {
                    v.push(self.eval(it, env)?);
                }
                Ok(Value::list(v))
            }
            ExprKind::Record(fields) => {
                let mut v = Vec::with_capacity(fields.len());
                for (k, it) in fields {
                    v.push((k.clone(), self.eval(it, env)?));
                }
                Ok(Value::record(v))
            }
            ExprKind::Function(f) => Ok(self.closure(f, env, None, sp)),
            ExprKind::Block(b) => self.eval_block(b, env),
            ExprKind::If { condition, yes, no } => match self.eval(condition, env)? {
                Value::Bool(true) => self.eval_block(yes, env),
                Value::Bool(false) => self.eval_block(no, env),
                Value::Reading(_) => err(
                    Some("J-01"),
                    "读数不能当条件；先 cut 成出口再 handle",
                    condition.span,
                ),
                other => err(
                    None,
                    format!("if 的条件要 Bool，收到 {}", other.type_name()),
                    condition.span,
                ),
            },
            ExprKind::Field { value, field } => {
                let v = self.eval(value, env)?;
                match &v {
                    Value::Record(_) => v.get(field).ok_or_else(|| {
                        Fault::Error(RtError::new(None, format!("记录没有字段 {field}"), sp))
                    }),
                    Value::Mat(m) => match field.as_str() {
                        "content" => Ok(json_to_value(&m.content)),
                        "taint" => Ok(Value::text(if m.taint == Taint::Trusted {
                            "trusted"
                        } else {
                            "untrusted"
                        })),
                        "hash" => Ok(Value::text(&m.hash)),
                        _ => err(None, format!("Mat 没有字段 {field}"), sp),
                    },
                    Value::Exit(x) => match field.as_str() {
                        "kind" => Ok(Value::text(&x.label())),
                        _ => err(None, format!("Exit 没有字段 {field}（用 handle 消费）"), sp),
                    },
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
                    (Value::Record(_), Value::Text(k)) => v.get(k).ok_or_else(|| {
                        Fault::Error(RtError::new(None, format!("记录没有字段 {k}"), sp))
                    }),
                    _ => err(
                        None,
                        format!("{}[{}] 不可索引", v.type_name(), i.type_name()),
                        sp,
                    ),
                }
            }
            ExprKind::Unary { op, value } => {
                let v = self.eval(value, env)?;
                match (op.as_str(), &v) {
                    ("!", Value::Bool(b)) => Ok(Value::Bool(!b)),
                    ("-", Value::Int(i)) => Ok(Value::Int(-i)),
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
            ExprKind::Call {
                function,
                arguments,
            } => {
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
            return err(
                Some("J-01"),
                format!("读数不能做 {op}：读数不可比、不可算，只能经 cut 离开"),
                sp,
            );
        }
        use Value::*;
        Ok(match (op, &l, &r) {
            ("+", Int(a), Int(b)) => Int(a + b),
            ("-", Int(a), Int(b)) => Int(a - b),
            ("*", Int(a), Int(b)) => Int(a * b),
            ("/", Int(a), Int(b)) => {
                if *b == 0 {
                    return err(None, "除以零", sp);
                }
                Int(a / b)
            }
            ("%", Int(a), Int(b)) => {
                if *b == 0 {
                    return err(None, "模零", sp);
                }
                Int(a % b)
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
            ("==", _, _) => Bool(l.equals(&r).unwrap_or(false)),
            ("!=", _, _) => Bool(!l.equals(&r).unwrap_or(false)),
            _ => {
                return err(
                    None,
                    format!("二元 {op} 不适用于 {} 与 {}", l.type_name(), r.type_name()),
                    sp,
                );
            }
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
            return err(
                None,
                format!(
                    "{} 需要 {} 个参数，收到 {}",
                    c.name.as_deref().unwrap_or("函数"),
                    f.parameters.len(),
                    args.len()
                ),
                sp,
            );
        }
        let max_depth = self.budget.depth.unwrap_or(DEFAULT_DEPTH);
        if self.depth >= max_depth {
            return err(
                Some("E5"),
                format!(
                    "调用深度超过 {max_depth}（递归无界）。修法：用 loop(bound, …) 或提高 budget.depth"
                ),
                sp,
            );
        }
        self.depth += 1;
        let env = env_child(&c.env);
        for (p, a) in f.parameters.iter().zip(args) {
            env_define(&env, &p.name, a);
        }
        let returns_exit = f
            .result_type
            .as_ref()
            .map(|t| t.mentions("Exit"))
            .unwrap_or(false);
        self.frames.push(Frame {
            name: c.name.clone().unwrap_or_else(|| "<fn>".into()),
            exits: vec![],
            returns_exit,
        });
        let result = self.eval_block(&f.body, &env);
        let frame = self.frames.pop().unwrap();
        self.depth -= 1;
        let v = result?;
        let mut in_value = HashSet::new();
        collect_exit_ids(&v, &mut in_value);
        for e in frame
            .exits
            .into_iter()
            .filter(|e| e.is_unsure() && !e.consumed.get())
        {
            if frame.returns_exit && in_value.contains(&e.id) {
                *e.consumed_by.borrow_mut() = format!("return_type:{}", frame.name);
                self.frame().exits.push(e);
            } else if frame.returns_exit {
                return err(
                    Some("J-05"),
                    format!("{} 里的 {} 既没消费也没返回", frame.name, e.label()),
                    e.site,
                );
            } else {
                return err(
                    Some("J-05"),
                    format!(
                        "{} 返回前有未消费的 {}。修法：在函数内 handle/consume，或把返回类型标为含 Exit（如 `-> Exit`、`-> Record<Exit>`）并在调用者消费",
                        frame.name,
                        e.label()
                    ),
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
            Value::Reading(_) => err(
                Some("J-01"),
                format!("读数不能放进 {slot} 槽：读数只能经 cut 离开，不是材料"),
                sp,
            ),
            Value::Exit(e) => {
                let mut d = BTreeSet::new();
                d.insert(e.q_hash.clone());
                Ok(Mat::new(
                    json!({"exit": e.label()}),
                    "",
                    vec![format!("exit:{}", e.q_hash)],
                    e.taint,
                    d,
                ))
            }
            Value::State(_)
            | Value::Question(_)
            | Value::Fn(_)
            | Value::Builtin(_)
            | Value::Stop(_) => err(None, format!("{} 不能作材料", v.type_name()), sp),
            Value::Fail(s) => Ok(Mat::new(
                json!({"fail": s.as_ref()}),
                "",
                vec!["fail".into()],
                Taint::Trusted,
                BTreeSet::new(),
            )),
            other => Ok(Mat::literal(other.to_json())),
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
            return err(
                None,
                "state(on) 或 state(on, {ctx: […], ref: […], over: […]})",
                sp,
            );
        }
        let (on, f1) = self.as_mats(&args[0], "on", sp)?;
        if on.is_empty() || on.len() > 2 {
            return err(
                Some("J-14"),
                format!("on 恰一个判断对象（或一对），收到 {}", on.len()),
                sp,
            );
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
        Ok(Value::State(Rc::new(State::new(
            on, ctx, r#ref, over, fail,
        ))))
    }

    // ---------- 效应 ----------

    fn charge(&mut self, calls: u64, usd: f64, sp: Span) -> R<()> {
        if self.cost.calls + calls > self.budget.calls || self.cost.usd + usd > self.budget.cost {
            return Err(Fault::Halt(Pending {
                cause: "budget".into(),
                key: String::new(),
                site: sp,
                detail: format!(
                    "预算耗尽：calls {}+{} / {}，cost {:.6}+{:.6} / {:.6}",
                    self.cost.calls, calls, self.budget.calls, self.cost.usd, usd, self.budget.cost
                ),
            }));
        }
        Ok(())
    }

    fn judge(&mut self, state: &Rc<State>, qs: &[Rc<Question>], sp: Span) -> R<Vec<Value>> {
        for q in qs {
            if state.derived_from.contains(&q.hash) {
                return err(
                    Some("J-02"),
                    format!("禁自指：状态含由题「{}」派生的材料，不能再问同一题", q.text),
                    sp,
                );
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
                        answer: None,
                        fail: Some("状态含 Fail 材料".into()),
                        model_id: self.model_id.clone(),
                        ledger_key: String::new(),
                        over_len: state.over.len(),
                        scale: q.scale.clone(),
                    }))
                })
                .collect());
        }
        let keys: Vec<String> = qs
            .iter()
            .map(|q| {
                judge_key(
                    &self.model_id,
                    &state.hash,
                    &q.hash,
                    q.op.phys(),
                    0,
                    self.run_seq,
                )
            })
            .collect();
        // 循环内键重复即停（J-06）
        if let Some(lc) = self.loops.last_mut() {
            for k in &keys {
                if !lc.seen_keys.insert(k.clone()) && lc.repeated.is_none() {
                    lc.repeated = Some(k.clone());
                }
            }
        }
        let mut answers: Vec<Option<Answer>> = vec![None; qs.len()];
        let mut missing = vec![];
        for (i, k) in keys.iter().enumerate() {
            if let Some(Entry::Judge { answer, .. }) = self.ledger.get(k) {
                answers[i] = Some(answer.clone());
                self.cost.replayed += 1;
                self.trace
                    .push("judge", k, true, 0.0, sp, format!("「{}」", qs[i].text));
            } else {
                missing.push(i);
            }
        }
        if !missing.is_empty() {
            self.charge(1, 0.0, sp)?;
            let ask: Vec<&Question> = missing.iter().map(|i| qs[*i].as_ref()).collect();
            let res = self.client.judge(state, &ask).map_err(|e| {
                Fault::Error(RtError::new(None, format!("客户端错误：{}", e.0), sp))
            })?;
            if res.answers.len() != ask.len() {
                return err(None, "客户端返回的答案数与题数不符", sp);
            }
            self.charge(0, res.cost, sp)?;
            self.cost.calls += 1;
            self.cost.tokens += res.tokens;
            self.cost.usd += res.cost;
            for (j, i) in missing.iter().enumerate() {
                let a = res.answers[j].clone();
                self.validate_answer(&a, &qs[*i], state, sp)?;
                self.ledger.put(Entry::Judge {
                    key: keys[*i].clone(),
                    answer: a.clone(),
                    tokens: res.tokens,
                    cost: res.cost,
                    model_id: self.model_id.clone(),
                });
                self.trace.push(
                    "judge",
                    &keys[*i],
                    false,
                    res.cost,
                    sp,
                    format!("「{}」", qs[*i].text),
                );
                answers[*i] = Some(a);
            }
        }
        Ok(qs
            .iter()
            .zip(keys)
            .zip(answers)
            .map(|((q, k), a)| {
                Value::Reading(Rc::new(Reading {
                    q_hash: q.hash.clone(),
                    state_hash: state.hash.clone(),
                    op: q.op,
                    calib: q.calib.clone(),
                    answer: a,
                    fail: None,
                    model_id: self.model_id.clone(),
                    ledger_key: k,
                    over_len: state.over.len(),
                    scale: q.scale.clone(),
                }))
            })
            .collect())
    }

    fn validate_answer(&self, a: &Answer, q: &Question, s: &State, sp: Span) -> R<()> {
        match (a, q.op) {
            (Answer::Noul(p), Op::Test) if (0.0..=1.0).contains(p) => Ok(()),
            (Answer::Choice(v), Op::Select) if v.len() == s.over.len() => Ok(()),
            (Answer::Score(v), Op::Measure) if v.len() == q.scale.len() => Ok(()),
            _ => err(
                None,
                format!(
                    "答案形状与题不符：{:?} vs {}（over {} / 档位 {}）",
                    a,
                    q.op.phys(),
                    s.over.len(),
                    q.scale.len()
                ),
                sp,
            ),
        }
    }

    fn new_exit(
        &mut self,
        kind: ExitKind,
        op: Op,
        q_hash: &str,
        state_hash: &str,
        taint: Taint,
        sp: Span,
    ) -> Value {
        let id = self.next_exit;
        self.next_exit += 1;
        let e = Rc::new(Exit {
            id,
            op,
            kind,
            q_hash: q_hash.into(),
            state_hash: state_hash.into(),
            taint,
            site: sp,
            consumed: std::cell::Cell::new(false),
            consumed_by: std::cell::RefCell::new(String::new()),
        });
        self.frame().exits.push(e.clone());
        Value::Exit(e)
    }

    fn cut(&mut self, r: &Reading, calib_key: Option<&str>, sp: Span) -> R<Value> {
        let key = calib_key.unwrap_or(&r.calib);
        let rec = self.calib.get(key);
        let taint = Taint::Trusted; // 状态 taint 在读数里未带；首包按 trusted，见 INTERFACE 待定
        let kind = if let Some(f) = &r.fail {
            ExitKind::Unsure(format!("fail:{f}"))
        } else if rec.status == "冷" {
            ExitKind::Unsure("cold".into())
        } else if rec.status == "停岗" {
            ExitKind::Unsure("drift".into())
        } else {
            match r.answer.as_ref().unwrap() {
                Answer::Noul(p) => {
                    if *p >= rec.hi {
                        ExitKind::Act
                    } else if *p <= rec.lo {
                        ExitKind::Ignore
                    } else {
                        ExitKind::Unsure("band".into())
                    }
                }
                Answer::Choice(v) => {
                    let (k, p) = argmax(v);
                    if p >= rec.hi {
                        ExitKind::Pick(k)
                    } else {
                        ExitKind::Unsure("tie".into())
                    }
                }
                Answer::Score(v) => {
                    let (l, p) = argmax(v);
                    if p >= rec.hi {
                        ExitKind::At(l)
                    } else {
                        ExitKind::Unsure("band".into())
                    }
                }
            }
        };
        Ok(self.new_exit(kind, r.op, &r.q_hash, &r.state_hash, taint, sp))
    }

    fn handle(&mut self, e: &Rc<Exit>, arms: &Value, sp: Span) -> R<Value> {
        let Value::Record(_) = arms else {
            return err(
                Some("J-05"),
                "handle 的第二个参数是记录 {act, ignore, pick, at, unsure, otherwise}",
                sp,
            );
        };
        let required: &[&str] = match e.op {
            Op::Test => &["act", "ignore", "unsure"],
            Op::Select => &["pick", "unsure"],
            Op::Measure => &["at", "unsure"],
        };
        let has_other = arms.get("otherwise").is_some();
        let missing: Vec<&str> = required
            .iter()
            .copied()
            .filter(|k| arms.get(k).is_none())
            .collect();
        if !missing.is_empty() && !has_other {
            return err(
                Some("J-05"),
                format!(
                    "handle 不穷尽：{} 题的出口缺分支 {}。修法：补上或给 otherwise",
                    e.op.phys(),
                    missing.join(", ")
                ),
                sp,
            );
        }
        let (name, arg): (&str, Value) = match &e.kind {
            ExitKind::Act => ("act", Value::Unit),
            ExitKind::Ignore => ("ignore", Value::Unit),
            ExitKind::Pick(k) => ("pick", Value::Int(*k as i64)),
            ExitKind::At(l) => ("at", Value::Int(*l as i64)),
            ExitKind::Unsure(c) => ("unsure", Value::text(c)),
        };
        let arm = arms.get(name).or_else(|| arms.get("otherwise")).unwrap();
        e.consumed.set(true);
        *e.consumed_by.borrow_mut() = format!("handle:{name}");
        match arm {
            Value::Fn(c) => {
                let n = c.function.parameters.len();
                let args = if n == 0 { vec![] } else { vec![arg] };
                self.call_closure(&c, args, sp)
            }
            other => Ok(other),
        }
    }

    fn do_(&mut self, name: &str, args: &[Value], iter_seq: i64, sp: Span) -> R<Value> {
        let action = self.actions.actions.get(name).cloned().ok_or_else(|| {
            Fault::Error(RtError::new(
                Some("J-11"),
                format!("动作 {name} 未登记：do 只能触发登记过的动作（register）"),
                sp,
            ))
        })?;
        let args_canon: Vec<String> = args.iter().map(|a| canon(&a.to_json())).collect();
        let key = effect_key(
            "do",
            &[
                &sp.start.to_string(),
                name,
                &args_canon.join("\u{1f}"),
                &iter_seq.to_string(),
            ],
        );
        if let Some(Entry::Effect { output, .. }) = self.ledger.get(&key) {
            self.cost.replayed += 1;
            self.trace.push("do", &key, true, 0.0, sp, name.into());
            return Ok(json_to_effect_value(output));
        }
        self.charge(0, action.cost, sp)?;
        let taint_in = args.iter().fold(Taint::Trusted, |t, a| {
            Taint::join(
                t,
                match a {
                    Value::Mat(m) => m.taint,
                    _ => Taint::Trusted,
                },
            )
        });
        let taint = match action.taint_out {
            TaintOut::Trusted => Taint::Trusted,
            TaintOut::Untrusted => Taint::Untrusted,
            TaintOut::Inherit => taint_in,
        };
        let out = match (action.f)(args) {
            Ok(v) => Value::Mat(Rc::new(Mat::new(
                v.to_json(),
                &format!("do:{name}"),
                vec![format!("do:{key}")],
                taint,
                BTreeSet::new(),
            ))),
            Err(msg) => Value::Fail(Rc::from(format!("{name}: {msg}").as_str())),
        };
        self.cost.usd += action.cost;
        self.ledger.put(Entry::Effect {
            key: key.clone(),
            kind: "do".into(),
            output: effect_value_to_json(&out),
            cost: action.cost,
        });
        self.trace
            .push("do", &key, false, action.cost, sp, name.into());
        Ok(out)
    }

    fn generate(
        &mut self,
        prompt: &str,
        ctx: &[Mat],
        n: usize,
        retry_seq: i64,
        sp: Span,
    ) -> R<Value> {
        let ctx_hash: Vec<&str> = ctx.iter().map(|m| m.hash.as_str()).collect();
        let key = effect_key(
            "gen",
            &[
                prompt,
                &ctx_hash.join(","),
                &n.to_string(),
                &retry_seq.to_string(),
            ],
        );
        let taint = ctx
            .iter()
            .fold(Taint::Trusted, |t, m| Taint::join(t, m.taint));
        let wrap = |outs: &[Json]| {
            Value::list(
                outs.iter()
                    .map(|o| {
                        Value::Mat(Rc::new(Mat::new(
                            o.clone(),
                            &format!("gen:{prompt}"),
                            vec![format!("gen:{key}")],
                            taint,
                            BTreeSet::new(),
                        )))
                    })
                    .collect(),
            )
        };
        if let Some(Entry::Effect { output, .. }) = self.ledger.get(&key) {
            let outs: Vec<Json> = output.as_array().cloned().unwrap_or_default();
            self.cost.replayed += 1;
            self.trace.push("gen", &key, true, 0.0, sp, prompt.into());
            return Ok(wrap(&outs));
        }
        self.charge(1, 0.0, sp)?;
        let ctx_json: Vec<Json> = ctx.iter().map(|m| m.content.clone()).collect();
        let outs = self
            .client
            .generate(prompt, &ctx_json, n, retry_seq as u64)
            .map_err(|e| Fault::Error(RtError::new(None, format!("gen 失败：{}", e.0), sp)))?;
        self.cost.calls += 1;
        self.ledger.put(Entry::Effect {
            key: key.clone(),
            kind: "gen".into(),
            output: Json::Array(outs.clone()),
            cost: 0.0,
        });
        self.trace.push("gen", &key, false, 0.0, sp, prompt.into());
        Ok(wrap(&outs))
    }

    fn ask(&mut self, state: &Rc<State>, q: &Rc<Question>, sp: Span) -> R<Value> {
        let key = effect_key("ask", &[&state.hash, &q.hash]);
        let answer = if let Some(Entry::Ask { answer, .. }) = self.ledger.get(&key) {
            answer.clone()
        } else {
            let limit = self.budget.escalate.unwrap_or(0);
            if self.cost.asks >= limit {
                return Err(Fault::Halt(Pending {
                    cause: "budget.escalate".into(),
                    key,
                    site: sp,
                    detail: format!("ask 次数已到上限 {limit}"),
                }));
            }
            self.cost.asks += 1;
            let a = self
                .client
                .ask(state, q)
                .map_err(|e| Fault::Error(RtError::new(None, format!("ask 失败：{}", e.0), sp)))?;
            if a.is_some() {
                self.ledger.put(Entry::Ask {
                    key: key.clone(),
                    answer: a.clone(),
                });
            }
            a
        };
        match answer {
            Some(a) => {
                self.trace
                    .push("ask", &key, false, 0.0, sp, format!("「{}」已答", q.text));
                let kind = match a {
                    Answer::Noul(p) => {
                        if p >= 0.5 {
                            ExitKind::Act
                        } else {
                            ExitKind::Ignore
                        }
                    }
                    Answer::Choice(v) => ExitKind::Pick(argmax(&v).0),
                    Answer::Score(v) => ExitKind::At(argmax(&v).0),
                };
                Ok(self.new_exit(kind, q.op, &q.hash, &state.hash, Taint::Trusted, sp))
            }
            None => {
                self.trace.push(
                    "ask",
                    &key,
                    false,
                    0.0,
                    sp,
                    format!("「{}」未答 → Pending", q.text),
                );
                Err(Fault::Halt(Pending {
                    cause: "ask".into(),
                    key,
                    site: sp,
                    detail: format!("等人回答「{}」", q.text),
                }))
            }
        }
    }

    fn transform(&mut self, f: &Rc<Closure>, args: &[Value], sp: Span) -> R<Value> {
        let mut mats = vec![];
        for a in args {
            mats.push(self.as_mat(a, "transform", sp)?);
        }
        let hashes: Vec<&str> = mats.iter().map(|m| m.hash.as_str()).collect();
        let key = effect_key("transform", &[&f.hash, &hashes.join(",")]);
        let taint = mats
            .iter()
            .fold(Taint::Trusted, |t, m| Taint::join(t, m.taint));
        if let Some(Entry::Effect { output, .. }) = self.ledger.get(&key) {
            self.cost.replayed += 1;
            self.trace
                .push("transform", &key, true, 0.0, sp, String::new());
            return Ok(Value::Mat(Rc::new(Mat::new(
                output.clone(),
                "transform",
                vec![format!("transform:{key}")],
                taint,
                BTreeSet::new(),
            ))));
        }
        let v = self.call_closure(
            f,
            mats.iter()
                .map(|m| Value::Mat(Rc::new(m.clone())))
                .collect(),
            sp,
        )?;
        if matches!(
            v,
            Value::Reading(_) | Value::Exit(_) | Value::Fn(_) | Value::State(_)
        ) {
            return err(
                Some("J-11"),
                format!("transform 的输出要能成材料，收到 {}", v.type_name()),
                sp,
            );
        }
        let content = match &v {
            Value::Mat(m) => m.content.clone(),
            other => other.to_json(),
        };
        self.ledger.put(Entry::Effect {
            key: key.clone(),
            kind: "transform".into(),
            output: content.clone(),
            cost: 0.0,
        });
        self.trace
            .push("transform", &key, false, 0.0, sp, String::new());
        Ok(Value::Mat(Rc::new(Mat::new(
            content,
            "transform",
            vec![format!("transform:{key}")],
            taint,
            BTreeSet::new(),
        ))))
    }

    fn loop_(&mut self, bound: i64, init: Value, step: &Value, sp: Span) -> R<Value> {
        if bound <= 0 {
            return err(
                Some("E5"),
                format!("loop 的 bound 必须是正整数，收到 {bound}"),
                sp,
            );
        }
        let Value::Fn(step) = step else {
            return err(
                None,
                "loop(bound, init, step) 的 step 要是函数 fn(acc, i)",
                sp,
            );
        };
        self.loops.push(LoopCtx {
            seen_keys: HashSet::new(),
            repeated: None,
        });
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
                self.trace.warn(format!(
                    "W-noprogress: 第 {} 轮重复了账本键 {}，循环停止（J-06 键重复即停）",
                    i + 1,
                    &k[..8]
                ));
                break;
            }
        }
        self.loops.pop();
        if result.is_none() {
            self.trace
                .warn(format!("W-bound: loop 到 bound={bound} 仍未 stop"));
        }
        Ok(result.unwrap_or(acc))
    }

    // ---------- 内置 ----------

    fn builtin(&mut self, name: &'static str, args: Vec<Value>, sp: Span) -> R<Value> {
        let n = args.len();
        let arity = |k: usize| -> R<()> {
            if n == k {
                Ok(())
            } else {
                err(None, format!("{name} 需要 {k} 个参数，收到 {n}"), sp)
            }
        };
        match name {
            "state" => self.make_state(&args, sp),
            "test" | "select" => {
                arity(2)?;
                let (Value::Text(t), Value::Text(c)) = (&args[0], &args[1]) else {
                    return err(
                        Some("J-03"),
                        format!("{name}(题面: Text, calib: Text) — calib 是校准记录的键，不是线"),
                        sp,
                    );
                };
                let op = if name == "test" { Op::Test } else { Op::Select };
                Ok(Value::Question(Rc::new(Question::new(op, t, c, vec![]))))
            }
            "measure" => {
                arity(3)?;
                let (Value::Text(t), Value::List(scale), Value::Text(c)) =
                    (&args[0], &args[1], &args[2])
                else {
                    return err(None, "measure(题面, [档位…], calib)", sp);
                };
                let mut sc = vec![];
                for s in scale.iter() {
                    match s {
                        Value::Text(x) => sc.push(x.to_string()),
                        _ => return err(None, "档位要是 Text", sp),
                    }
                }
                if sc.len() < 2 {
                    return err(None, "measure 至少两档", sp);
                }
                Ok(Value::Question(Rc::new(Question::new(
                    Op::Measure,
                    t,
                    c,
                    sc,
                ))))
            }
            "judge" => {
                arity(2)?;
                let Value::State(s) = &args[0] else {
                    return err(None, "judge(state, question | [questions])", sp);
                };
                match &args[1] {
                    Value::Question(q) => Ok(self.judge(s, &[q.clone()], sp)?.remove(0)),
                    Value::List(l) => {
                        let mut qs = vec![];
                        for q in l.iter() {
                            match q {
                                Value::Question(q) => qs.push(q.clone()),
                                _ => return err(None, "judge 的题列表里有非题", sp),
                            }
                        }
                        Ok(Value::list(self.judge(s, &qs, sp)?))
                    }
                    _ => err(None, "judge 的第二个参数要是题或题列表", sp),
                }
            }
            "cut" => {
                if n == 0 || n > 2 {
                    return err(None, "cut(reading) 或 cut(reading, calib_key)", sp);
                }
                let calib = match args.get(1) {
                    None => None,
                    Some(Value::Text(k)) => Some(k.to_string()),
                    Some(other) => {
                        return err(
                            Some("J-03"),
                            format!(
                                "cut 的校准参数必须是校准记录的键（Text），不能是字面量线；收到 {}",
                                other.type_name()
                            ),
                            sp,
                        );
                    }
                };
                match &args[0] {
                    Value::Reading(r) => self.cut(r, calib.as_deref(), sp),
                    Value::List(l) => {
                        let mut out = vec![];
                        for r in l.iter() {
                            match r {
                                Value::Reading(r) => out.push(self.cut(r, calib.as_deref(), sp)?),
                                _ => return err(None, "cut 的列表里有非读数", sp),
                            }
                        }
                        Ok(Value::list(out))
                    }
                    other => err(
                        None,
                        format!("cut 只收读数，收到 {}", other.type_name()),
                        sp,
                    ),
                }
            }
            "handle" => {
                arity(2)?;
                let Value::Exit(e) = &args[0] else {
                    return err(
                        None,
                        format!("handle 的第一个参数要是出口，收到 {}", args[0].type_name()),
                        sp,
                    );
                };
                let e = e.clone();
                self.handle(&e, &args[1], sp)
            }
            "consume" => {
                arity(2)?;
                let Value::Text(how) = &args[1] else {
                    return err(None, "consume(exit | [exits], \"drop\")", sp);
                };
                if how.as_ref() != "drop" {
                    return err(None, "consume 目前只支持 \"drop\"；升级用 ask", sp);
                }
                let list: Vec<Value> = match &args[0] {
                    Value::List(l) => l.iter().cloned().collect(),
                    v => vec![v.clone()],
                };
                for v in &list {
                    match v {
                        Value::Exit(e) => {
                            e.consumed.set(true);
                            *e.consumed_by.borrow_mut() = "consume:drop".into();
                        }
                        _ => return err(None, "consume 只收出口", sp),
                    }
                }
                Ok(Value::Unit)
            }
            "gen" => {
                arity(4)?;
                let (Value::Text(p), ctx, Value::Int(k), Value::Int(r)) =
                    (&args[0], &args[1], &args[2], &args[3])
                else {
                    return err(None, "gen(prompt, [ctx], n, retry_seq)", sp);
                };
                let (ctx, _) = self.as_mats(ctx, "ctx", sp)?;
                let p = p.to_string();
                self.generate(&p, &ctx, *k as usize, *r, sp)
            }
            "do" => {
                arity(3)?;
                let (Value::Text(a), Value::List(l), Value::Int(i)) =
                    (&args[0], &args[1], &args[2])
                else {
                    return err(None, "do(action, [args], iter_seq)", sp);
                };
                let a = a.to_string();
                let l: Vec<Value> = l.iter().cloned().collect();
                self.do_(&a, &l, *i, sp)
            }
            "ask" => {
                arity(2)?;
                let (Value::State(s), Value::Question(q)) = (&args[0], &args[1]) else {
                    return err(None, "ask(state, question)", sp);
                };
                let (s, q) = (s.clone(), q.clone());
                self.ask(&s, &q, sp)
            }
            "transform" => {
                if n < 1 {
                    return err(None, "transform(f, mats…)", sp);
                }
                let Value::Fn(f) = &args[0] else {
                    return err(None, "transform 的第一个参数要是函数", sp);
                };
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
                    Value::Mat(m) => Ok(json_to_value(&m.content)),
                    Value::Reading(_) => err(Some("J-01"), "读数没有内容可读；只能经 cut 离开", sp),
                    other => err(
                        None,
                        format!("content 只收材料，收到 {}", other.type_name()),
                        sp,
                    ),
                }
            }
            "unsure" => {
                arity(1)?;
                let Value::Text(c) = &args[0] else {
                    return err(None, "unsure(cause: Text)", sp);
                };
                let c = c.to_string();
                Ok(self.new_exit(
                    ExitKind::Unsure(c),
                    Op::Test,
                    "explicit",
                    "",
                    Taint::Trusted,
                    sp,
                ))
            }
            "pending" => {
                arity(1)?;
                let Value::Text(c) = &args[0] else {
                    return err(None, "pending(reason: Text)", sp);
                };
                Err(Fault::Halt(Pending {
                    cause: "explicit".into(),
                    key: String::new(),
                    site: sp,
                    detail: c.to_string(),
                }))
            }
            "fail" => {
                arity(1)?;
                let Value::Text(c) = &args[0] else {
                    return err(None, "fail(reason: Text)", sp);
                };
                Ok(Value::Fail(Rc::from(c.as_ref())))
            }
            "is_fail" => {
                arity(1)?;
                Ok(Value::Bool(matches!(args[0], Value::Fail(_))))
            }
            "exit_kind" => {
                arity(1)?;
                match &args[0] {
                    Value::Exit(e) => Ok(Value::text(&e.label())),
                    _ => err(None, "exit_kind 只收出口", sp),
                }
            }
            "loop" => {
                arity(3)?;
                let Value::Int(b) = &args[0] else {
                    return err(Some("E5"), "loop 的 bound 必须是整数字面量或整数值", sp);
                };
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
                let (Value::List(l), f) = (&args[0], &args[1]) else {
                    return err(None, format!("{name}(list, fn)"), sp);
                };
                let mut out = vec![];
                for it in l.iter() {
                    let r = self.apply(f.clone(), vec![it.clone()], sp)?;
                    if name == "map" {
                        out.push(r);
                    } else if matches!(r, Value::Bool(true)) {
                        out.push(it.clone());
                    }
                }
                Ok(Value::list(out))
            }
            "fold" => {
                arity(3)?;
                let (Value::List(l), init, f) = (&args[0], &args[1], &args[2]) else {
                    return err(None, "fold(list, init, fn(acc, x))", sp);
                };
                let mut acc = init.clone();
                for it in l.iter() {
                    acc = self.apply(f.clone(), vec![acc, it.clone()], sp)?;
                }
                Ok(acc)
            }
            "range" => {
                arity(2)?;
                let (Value::Int(a), Value::Int(b)) = (&args[0], &args[1]) else {
                    return err(None, "range(a, b)", sp);
                };
                Ok(Value::list((*a..*b).map(Value::Int).collect()))
            }
            "append" => {
                arity(2)?;
                let Value::List(l) = &args[0] else {
                    return err(None, "append(list, v)", sp);
                };
                let mut v: Vec<Value> = l.iter().cloned().collect();
                v.push(args[1].clone());
                Ok(Value::list(v))
            }
            "concat" => {
                arity(2)?;
                let (Value::List(a), Value::List(b)) = (&args[0], &args[1]) else {
                    return err(None, "concat(a, b)", sp);
                };
                Ok(Value::list(a.iter().chain(b.iter()).cloned().collect()))
            }
            "slice" => {
                arity(3)?;
                let (Value::List(l), Value::Int(a), Value::Int(b)) = (&args[0], &args[1], &args[2])
                else {
                    return err(None, "slice(list, a, b)", sp);
                };
                let a = (*a).clamp(0, l.len() as i64) as usize;
                let b = (*b).clamp(a as i64, l.len() as i64) as usize;
                Ok(Value::list(l[a..b].to_vec()))
            }
            "contains" => {
                arity(2)?;
                let Value::List(l) = &args[0] else {
                    return err(None, "contains(list, v)", sp);
                };
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
                let Value::List(l) = &args[0] else {
                    return err(None, "sum(list)", sp);
                };
                let mut acc = Value::Int(0);
                for it in l.iter() {
                    acc = self.binop("+", acc, it.clone(), sp)?;
                }
                Ok(acc)
            }
            "min" | "max" => {
                arity(2)?;
                let (Value::Int(a), Value::Int(b)) = (&args[0], &args[1]) else {
                    return err(None, format!("{name}(Int, Int)"), sp);
                };
                Ok(Value::Int(if name == "min" {
                    *a.min(b)
                } else {
                    *a.max(b)
                }))
            }
            "abs" => {
                arity(1)?;
                match &args[0] {
                    Value::Int(a) => Ok(Value::Int(a.abs())),
                    Value::Float(a) => Ok(Value::Float(a.abs())),
                    _ => err(None, "abs(number)", sp),
                }
            }
            "floor" => {
                arity(1)?;
                match &args[0] {
                    Value::Float(a) => Ok(Value::Int(a.floor() as i64)),
                    Value::Int(a) => Ok(Value::Int(*a)),
                    _ => err(None, "floor(number)", sp),
                }
            }
            "reverse" => {
                arity(1)?;
                let Value::List(l) = &args[0] else {
                    return err(None, "reverse(list)", sp);
                };
                Ok(Value::list(l.iter().rev().cloned().collect()))
            }
            "keys" => {
                arity(1)?;
                let Value::Record(r) = &args[0] else {
                    return err(None, "keys(record)", sp);
                };
                Ok(Value::list(r.iter().map(|(k, _)| Value::text(k)).collect()))
            }
            "has" => {
                arity(2)?;
                let (Value::Record(_), Value::Text(k)) = (&args[0], &args[1]) else {
                    return err(None, "has(record, key)", sp);
                };
                Ok(Value::Bool(args[0].get(k).is_some()))
            }
            "with" => {
                arity(3)?;
                let (Value::Record(r), Value::Text(k)) = (&args[0], &args[1]) else {
                    return err(None, "with(record, key, value)", sp);
                };
                let mut v: Vec<(String, Value)> = r
                    .iter()
                    .filter(|(kk, _)| kk.as_str() != k.as_ref())
                    .cloned()
                    .collect();
                v.push((k.to_string(), args[2].clone()));
                Ok(Value::record(v))
            }
            "text" => {
                arity(1)?;
                match &args[0] {
                    Value::Text(t) => Ok(Value::text(t)),
                    Value::Reading(_) => err(Some("J-01"), "读数不能转文字", sp),
                    Value::Mat(m) => Ok(Value::text(&m.text())),
                    other => Ok(Value::text(
                        &other.to_json().to_string().trim_matches('"').to_string(),
                    )),
                }
            }
            "join" => {
                arity(2)?;
                let (Value::List(l), Value::Text(sep)) = (&args[0], &args[1]) else {
                    return err(None, "join([Text], sep)", sp);
                };
                let parts: Vec<String> = l
                    .iter()
                    .map(|v| match v {
                        Value::Text(t) => t.to_string(),
                        o => o.to_json().to_string(),
                    })
                    .collect();
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
        Json::Number(n) => n
            .as_i64()
            .map(Value::Int)
            .unwrap_or_else(|| Value::Float(n.as_f64().unwrap_or(0.0))),
        Json::String(s) => Value::text(s),
        Json::Array(a) => Value::list(a.iter().map(json_to_value).collect()),
        Json::Object(o) => Value::record(
            o.iter()
                .map(|(k, v)| (k.clone(), json_to_value(v)))
                .collect(),
        ),
    }
}

fn effect_value_to_json(v: &Value) -> Json {
    match v {
        Value::Fail(s) => json!({"__fail": s.as_ref()}),
        Value::Mat(m) => {
            json!({"__mat": m.content, "taint": m.taint, "addr": m.addr, "origin": m.origin})
        }
        other => other.to_json(),
    }
}

fn json_to_effect_value(j: &Json) -> Value {
    if let Some(f) = j.get("__fail").and_then(|x| x.as_str()) {
        return Value::Fail(Rc::from(f));
    }
    if let Some(c) = j.get("__mat") {
        let taint: Taint =
            serde_json::from_value(j.get("taint").cloned().unwrap_or(json!("Trusted")))
                .unwrap_or(Taint::Trusted);
        let addr = j.get("addr").and_then(|a| a.as_str()).unwrap_or("");
        let origin: Vec<String> =
            serde_json::from_value(j.get("origin").cloned().unwrap_or(json!([])))
                .unwrap_or_default();
        return Value::Mat(Rc::new(Mat::new(
            c.clone(),
            addr,
            origin,
            taint,
            BTreeSet::new(),
        )));
    }
    json_to_value(j)
}

fn collect_exit_ids(v: &Value, out: &mut HashSet<usize>) {
    match v {
        Value::Exit(e) => {
            out.insert(e.id);
        }
        Value::List(l) => l.iter().for_each(|x| collect_exit_ids(x, out)),
        Value::Record(r) => r.iter().for_each(|(_, x)| collect_exit_ids(x, out)),
        Value::Stop(x) => collect_exit_ids(x, out),
        _ => {}
    }
}
