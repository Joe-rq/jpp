//! Core AST：前端 lower 的目标。每个节点带字节偏移 `Span`，与前端一致；`serde` 可往返。
//! 只有宿主表达式语言 + 名字。效应与内核操作都是普通调用，名字在根环境里解析成 `Value::Builtin`，
//! 所以前端不需要为每种效应造节点；检查器按名字识别它们。

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Span {
    pub start: usize,
    pub end: usize,
}

impl Span {
    pub fn new(start: usize, end: usize) -> Self {
        Self { start, end }
    }
}

/// 程序 = 预算声明 + 一个块。预算必填（E12）；检查器报错，运行时拒绝无预算程序。
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Program {
    pub budget: Option<Budget>,
    pub body: Block,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Budget {
    /// 模型调用次数上限
    pub calls: u64,
    /// 美元上限
    pub cost: f64,
    /// 嵌套调用深度上限（递归也算），None 取默认 256
    #[serde(default)]
    pub depth: Option<u32>,
    /// `ask` 次数上限，None 取 0
    #[serde(default)]
    pub escalate: Option<u64>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Block {
    pub statements: Vec<Statement>,
    pub result: Option<Box<Expr>>,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Statement {
    Let { name: String, annotation: Option<Type>, value: Expr, span: Span },
    Function { name: String, function: Function, span: Span },
    Expression(Expr),
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Expr {
    pub kind: ExprKind,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum ExprKind {
    Integer(i64),
    Decimal(f64),
    Bool(bool),
    Text(String),
    Unit,
    Name(String),
    List(Vec<Expr>),
    Record(Vec<(String, Expr)>),
    Function(Function),
    Call { function: Box<Expr>, arguments: Vec<Expr> },
    Field { value: Box<Expr>, field: String },
    Index { value: Box<Expr>, index: Box<Expr> },
    Unary { op: String, value: Box<Expr> },
    Binary { op: String, left: Box<Expr>, right: Box<Expr> },
    If { condition: Box<Expr>, yes: Block, no: Block },
    Block(Block),
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Function {
    pub parameters: Vec<Parameter>,
    pub result_type: Option<Type>,
    /// None = 未声明；Some([]) = 显式纯。效应名：judge / gen / do / ask。
    pub effects: Option<Vec<String>>,
    pub body: Block,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Parameter {
    pub name: String,
    pub annotation: Option<Type>,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Type {
    Named(String),
    Applied(String, Vec<Type>),
    /// 只有参数与结果的旧形式：效应行未知。前端目前 lower 出的就是这个。
    Function(Vec<Type>, Box<Type>),
    /// 方法类型：效应行与责任捕获跟着**类型**走，所以方法经参数、记录字段、返回值传递时契约不丢。
    Method(MethodType),
}

/// `Fn^κ(params ⊸ε ret)`。`effects` 是效应行（None = 未知/待推断，Some 是上界）；
/// `captures_responsibility` 区分 `Fn¹`（捕获了未决责任，不可随意丢弃或重复调用）与 `Fnω`。
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MethodType {
    pub params: Vec<Type>,
    pub ret: Box<Type>,
    #[serde(default)]
    pub effects: Option<Vec<String>>,
    #[serde(default)]
    pub captures_responsibility: bool,
}

impl MethodType {
    /// `Fnω`：可重复调用、可丢弃；高阶操作（map / filter）只收这种
    pub fn omega(params: Vec<Type>, ret: Type, effects: &[&str]) -> MethodType {
        MethodType {
            params,
            ret: Box::new(ret),
            effects: Some(effects.iter().map(|e| e.to_string()).collect()),
            captures_responsibility: false,
        }
    }
    /// `Fn¹`：捕获了未决责任
    pub fn linear(params: Vec<Type>, ret: Type, effects: &[&str]) -> MethodType {
        MethodType { captures_responsibility: true, ..MethodType::omega(params, ret, effects) }
    }
}

impl Type {
    /// 类型是否提到 `Exit`（J-05 的返回类型消费）
    pub fn mentions(&self, name: &str) -> bool {
        match self {
            Type::Named(n) => n == name,
            Type::Applied(n, args) => n == name || args.iter().any(|a| a.mentions(name)),
            Type::Function(ps, r) => ps.iter().any(|p| p.mentions(name)) || r.mentions(name),
            Type::Method(m) => m.params.iter().any(|p| p.mentions(name)) || m.ret.mentions(name),
        }
    }

    /// 这个类型是方法吗？是的话给出它的效应行（None = 未知）与是否捕获责任。
    pub fn as_method(&self) -> Option<(Option<&[String]>, bool)> {
        match self {
            Type::Method(m) => Some((m.effects.as_deref(), m.captures_responsibility)),
            Type::Function(_, _) => Some((None, false)),
            Type::Named(n) | Type::Applied(n, _) if n == "Fn" || n == "Method" => Some((None, false)),
            _ => None,
        }
    }
}

impl Expr {
    pub fn new(kind: ExprKind, span: Span) -> Self {
        Self { kind, span }
    }
    pub fn name(n: &str, span: Span) -> Self {
        Self::new(ExprKind::Name(n.to_string()), span)
    }
    pub fn int(v: i64, span: Span) -> Self {
        Self::new(ExprKind::Integer(v), span)
    }
    pub fn text(s: &str, span: Span) -> Self {
        Self::new(ExprKind::Text(s.to_string()), span)
    }
    pub fn call(f: Expr, args: Vec<Expr>, span: Span) -> Self {
        Self::new(ExprKind::Call { function: Box::new(f), arguments: args }, span)
    }
    /// `name(args…)` 的简写
    pub fn call_name(n: &str, args: Vec<Expr>, span: Span) -> Self {
        Self::call(Self::name(n, span), args, span)
    }
    pub fn field(v: Expr, f: &str, span: Span) -> Self {
        Self::new(ExprKind::Field { value: Box::new(v), field: f.to_string() }, span)
    }
    pub fn record(fields: Vec<(&str, Expr)>, span: Span) -> Self {
        Self::new(ExprKind::Record(fields.into_iter().map(|(k, v)| (k.to_string(), v)).collect()), span)
    }
    pub fn list(items: Vec<Expr>, span: Span) -> Self {
        Self::new(ExprKind::List(items), span)
    }
    pub fn binary(op: &str, l: Expr, r: Expr, span: Span) -> Self {
        Self::new(ExprKind::Binary { op: op.to_string(), left: Box::new(l), right: Box::new(r) }, span)
    }
    pub fn if_(c: Expr, yes: Block, no: Block, span: Span) -> Self {
        Self::new(ExprKind::If { condition: Box::new(c), yes, no }, span)
    }
    pub fn func(params: &[&str], ret: Option<Type>, body: Block, span: Span) -> Self {
        Self::new(
            ExprKind::Function(Function {
                parameters: params.iter().map(|p| Parameter { name: p.to_string(), annotation: None, span }).collect(),
                result_type: ret,
                effects: None,
                body,
            }),
            span,
        )
    }
}

impl Block {
    pub fn new(statements: Vec<Statement>, result: Option<Expr>, span: Span) -> Self {
        Self { statements, result: result.map(Box::new), span }
    }
    pub fn expr(e: Expr) -> Self {
        let span = e.span;
        Self { statements: vec![], result: Some(Box::new(e)), span }
    }
}

pub fn let_(name: &str, value: Expr, span: Span) -> Statement {
    Statement::Let { name: name.to_string(), annotation: None, value, span }
}
