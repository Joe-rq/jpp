//! 函数的结构哈希（`ir::Function.source_hash`）：账本键口径，不是第二棵语法树。
//!
//! 运行时的闭包身份、`transform` 的键与捕获指纹都取这个哈希。步 12c 起它的口径是
//! `hash_of(["fn", 函数的 JSON])`，JSON 是删除前的核心语法树 `ast::Function` 经 serde 的输出。
//! 核心语法树在步 12d 删除，这里用只派生 `Serialize` 的影子结构按同一字段名、同一变体名、同一
//! 顺序重现那份 JSON，账本键因此逐字节不变。只用于算哈希，不被任何人读取。

use crate::ast as a;
use serde::Serialize;

pub(super) fn source_hash(f: &a::Function) -> String {
    jpp_ir::key::hash_of(&[
        "fn",
        &serde_json::to_string(&Function::of(f)).unwrap_or_default(),
    ])
}

#[derive(Serialize)]
struct Span {
    start: usize,
    end: usize,
}

impl Span {
    fn of(s: a::Span) -> Span {
        Span {
            start: s.start,
            end: s.end,
        }
    }
}

#[derive(Serialize)]
struct Function {
    parameters: Vec<Parameter>,
    result_type: Option<Type>,
    effects: Option<Vec<String>>,
    body: Block,
}

impl Function {
    fn of(f: &a::Function) -> Function {
        Function {
            parameters: f
                .parameters
                .iter()
                .map(|p| Parameter {
                    name: p.name.clone(),
                    annotation: p.annotation.as_ref().map(Type::of),
                    span: Span::of(p.span),
                })
                .collect(),
            result_type: f.result_type.as_ref().map(Type::of),
            effects: f.effects.clone(),
            body: Block::of(&f.body),
        }
    }
}

#[derive(Serialize)]
struct Parameter {
    name: String,
    annotation: Option<Type>,
    span: Span,
}

#[derive(Serialize)]
enum Type {
    Named(String),
    Applied(String, Vec<Type>),
    Function(Vec<Type>, Box<Type>),
    Method(MethodType),
}

#[derive(Serialize)]
struct MethodType {
    params: Vec<Type>,
    ret: Box<Type>,
    effects: Option<Vec<String>>,
    captures_responsibility: bool,
}

impl Type {
    fn of(t: &a::Type) -> Type {
        match t {
            a::Type::Named(n) => Type::Named(n.clone()),
            a::Type::Applied(n, args) => {
                Type::Applied(n.clone(), args.iter().map(Type::of).collect())
            }
            a::Type::Function(args, r) => {
                Type::Function(args.iter().map(Type::of).collect(), Box::new(Type::of(r)))
            }
            a::Type::Method {
                parameters,
                result,
                effects,
                captures_responsibility,
            } => Type::Method(MethodType {
                params: parameters.iter().map(Type::of).collect(),
                ret: Box::new(Type::of(result)),
                effects: effects.clone(),
                captures_responsibility: *captures_responsibility,
            }),
        }
    }
}

#[derive(Serialize)]
struct Block {
    statements: Vec<Statement>,
    result: Option<Box<Expr>>,
    span: Span,
}

impl Block {
    fn of(b: &a::Block) -> Block {
        Block {
            statements: b
                .statements
                .iter()
                .map(|s| match s {
                    a::Statement::Let {
                        name,
                        annotation,
                        value,
                        span,
                    } => Statement::Let {
                        name: name.clone(),
                        annotation: annotation.as_ref().map(Type::of),
                        value: Expr::of(value),
                        span: Span::of(*span),
                    },
                    a::Statement::Function {
                        name,
                        function,
                        span,
                    } => Statement::Function {
                        name: name.clone(),
                        function: Function::of(function),
                        span: Span::of(*span),
                    },
                    a::Statement::Expression(e) => Statement::Expression(Expr::of(e)),
                })
                .collect(),
            result: b.result.as_ref().map(|e| Box::new(Expr::of(e))),
            span: Span::of(b.span),
        }
    }
}

#[derive(Serialize)]
enum Statement {
    Let {
        name: String,
        annotation: Option<Type>,
        value: Expr,
        span: Span,
    },
    Function {
        name: String,
        function: Function,
        span: Span,
    },
    Expression(Expr),
}

#[derive(Serialize)]
struct Expr {
    kind: ExprKind,
    span: Span,
}

#[derive(Serialize)]
enum ExprKind {
    Integer(i64),
    Decimal(f64),
    Bool(bool),
    Text(String),
    Unit,
    Name(String),
    List(Vec<Expr>),
    Record(Vec<(String, Expr)>),
    Function(Function),
    Call {
        function: Box<Expr>,
        arguments: Vec<Expr>,
    },
    Field {
        value: Box<Expr>,
        field: String,
    },
    Index {
        value: Box<Expr>,
        index: Box<Expr>,
    },
    Unary {
        op: String,
        value: Box<Expr>,
    },
    Binary {
        op: String,
        left: Box<Expr>,
        right: Box<Expr>,
    },
    If {
        condition: Box<Expr>,
        yes: Block,
        no: Block,
    },
    Block(Block),
}

impl Expr {
    fn of(e: &a::Expr) -> Expr {
        use a::ExprKind as A;
        let b = |e: &a::Expr| Box::new(Expr::of(e));
        let kind = match &e.kind {
            A::Integer(n) => ExprKind::Integer(*n),
            A::Decimal(n) => ExprKind::Decimal(*n),
            A::Bool(v) => ExprKind::Bool(*v),
            A::Text(t) => ExprKind::Text(t.clone()),
            A::Unit => ExprKind::Unit,
            A::Name(n) => ExprKind::Name(n.clone()),
            A::List(xs) => ExprKind::List(xs.iter().map(Expr::of).collect()),
            A::Record(fs) => {
                ExprKind::Record(fs.iter().map(|(k, v)| (k.clone(), Expr::of(v))).collect())
            }
            A::Function(f) => ExprKind::Function(Function::of(f)),
            A::Call {
                function,
                arguments,
            } => ExprKind::Call {
                function: b(function),
                arguments: arguments.iter().map(Expr::of).collect(),
            },
            A::Field { value, field } => ExprKind::Field {
                value: b(value),
                field: field.clone(),
            },
            A::Index { value, index } => ExprKind::Index {
                value: b(value),
                index: b(index),
            },
            A::Unary { op, value } => ExprKind::Unary {
                op: op.clone(),
                value: b(value),
            },
            A::Binary { op, left, right } => ExprKind::Binary {
                op: op.clone(),
                left: b(left),
                right: b(right),
            },
            A::If { condition, yes, no } => ExprKind::If {
                condition: b(condition),
                yes: Block::of(yes),
                no: Block::of(no),
            },
            A::Block(bl) => ExprKind::Block(Block::of(bl)),
        };
        Expr {
            kind,
            span: Span::of(e.span),
        }
    }
}
