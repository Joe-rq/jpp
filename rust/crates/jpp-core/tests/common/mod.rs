//! 手工构造 core AST 的小建造器。每个节点拿一个唯一的 `Span`——测试要断言诊断落在哪个节点上，
//! 账本里 `do` 的键也含 `span.start`，所以不能让它们都是 0。

#![allow(dead_code)]

use std::cell::Cell;

use jpp_core::ast::*;

thread_local! {
    static NEXT: Cell<usize> = const { Cell::new(0) };
}

/// 取一个此后不会重复的位置
pub fn sp() -> Span {
    NEXT.with(|n| {
        let v = n.get() + 2;
        n.set(v);
        Span::new(v, v + 1)
    })
}

pub fn int(v: i64) -> Expr {
    Expr::new(ExprKind::Integer(v), sp())
}
pub fn dec(v: f64) -> Expr {
    Expr::new(ExprKind::Decimal(v), sp())
}
pub fn boolean(v: bool) -> Expr {
    Expr::new(ExprKind::Bool(v), sp())
}
pub fn text(s: &str) -> Expr {
    Expr::new(ExprKind::Text(s.to_string()), sp())
}
pub fn name(n: &str) -> Expr {
    Expr::new(ExprKind::Name(n.to_string()), sp())
}
pub fn unit() -> Expr {
    Expr::new(ExprKind::Unit, sp())
}
pub fn list(items: Vec<Expr>) -> Expr {
    Expr::new(ExprKind::List(items), sp())
}
pub fn rec(fields: Vec<(&str, Expr)>) -> Expr {
    Expr::new(
        ExprKind::Record(
            fields
                .into_iter()
                .map(|(k, v)| (k.to_string(), v))
                .collect(),
        ),
        sp(),
    )
}
pub fn call(f: &str, args: Vec<Expr>) -> Expr {
    Expr::new(
        ExprKind::Call {
            function: Box::new(name(f)),
            arguments: args,
        },
        sp(),
    )
}
pub fn call_of(f: Expr, args: Vec<Expr>) -> Expr {
    Expr::new(
        ExprKind::Call {
            function: Box::new(f),
            arguments: args,
        },
        sp(),
    )
}
pub fn field(v: Expr, f: &str) -> Expr {
    Expr::new(
        ExprKind::Field {
            value: Box::new(v),
            field: f.to_string(),
        },
        sp(),
    )
}
pub fn index(v: Expr, i: Expr) -> Expr {
    Expr::new(
        ExprKind::Index {
            value: Box::new(v),
            index: Box::new(i),
        },
        sp(),
    )
}
pub fn bin(op: &str, l: Expr, r: Expr) -> Expr {
    Expr::new(
        ExprKind::Binary {
            op: op.to_string(),
            left: Box::new(l),
            right: Box::new(r),
        },
        sp(),
    )
}
pub fn not(v: Expr) -> Expr {
    Expr::new(
        ExprKind::Unary {
            op: "!".to_string(),
            value: Box::new(v),
        },
        sp(),
    )
}
pub fn if_(c: Expr, yes: Expr, no: Expr) -> Expr {
    Expr::new(
        ExprKind::If {
            condition: Box::new(c),
            yes: body(vec![], yes),
            no: body(vec![], no),
        },
        sp(),
    )
}

pub fn body(statements: Vec<Statement>, result: Expr) -> Block {
    Block {
        statements,
        result: Some(Box::new(result)),
        span: sp(),
    }
}

pub fn bind(n: &str, v: Expr) -> Statement {
    Statement::Let {
        name: n.to_string(),
        annotation: None,
        value: v,
        span: sp(),
    }
}

fn make(
    params: &[&str],
    effects: Option<&[&str]>,
    result_type: Option<Type>,
    b: Block,
) -> Function {
    Function {
        parameters: params
            .iter()
            .map(|p| Parameter {
                name: p.to_string(),
                annotation: None,
                span: sp(),
            })
            .collect(),
        result_type,
        effects: effects.map(|e| e.iter().map(|x| x.to_string()).collect()),
        body: b,
    }
}

/// 匿名方法值（可传入、可返回、可再组合）
pub fn lambda(params: &[&str], b: Block) -> Expr {
    Expr::new(ExprKind::Function(make(params, None, None, b)), sp())
}
pub fn lambda_eff(params: &[&str], effects: &[&str], b: Block) -> Expr {
    Expr::new(
        ExprKind::Function(make(params, Some(effects), None, b)),
        sp(),
    )
}
/// 具名方法
pub fn func(n: &str, params: &[&str], b: Block) -> Statement {
    Statement::Function {
        name: n.to_string(),
        function: make(params, None, None, b),
        span: sp(),
    }
}
pub fn func_eff(n: &str, params: &[&str], effects: &[&str], b: Block) -> Statement {
    Statement::Function {
        name: n.to_string(),
        function: make(params, Some(effects), None, b),
        span: sp(),
    }
}
/// 带返回类型标注（J-05：标注含 Exit 才许把出口带出）
pub fn func_ret(
    n: &str,
    params: &[&str],
    effects: Option<&[&str]>,
    ret: Type,
    b: Block,
) -> Statement {
    Statement::Function {
        name: n.to_string(),
        function: make(params, effects, Some(ret), b),
        span: sp(),
    }
}

pub fn budget(calls: u64, depth: u32) -> Budget {
    Budget {
        calls,
        cost: 0.0,
        depth: Some(depth),
        escalate: None,
    }
}

pub fn program(b: Option<Budget>, statements: Vec<Statement>, result: Expr) -> Program {
    Program {
        budget: b,
        body: body(statements, result),
        span: sp(),
    }
}

/// 断言静态检查一条诊断都没有；有就把它们打出来（报文本身是交付物的一部分）
pub fn assert_clean(p: &Program) {
    let report = jpp_core::check(p);
    assert!(
        report.diagnostics.is_empty(),
        "静态检查本不该有话说，却报了：\n{}",
        report.render()
    );
}
