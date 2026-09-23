//! Explicit source-AST to shared-core translation. No execution lives here.
use crate::{Diagnostic, ast as a};
use jpp_core::ast as c;

pub fn lower(program: &a::Program) -> Result<c::Program, Diagnostic> {
    Ok(c::Program {
        budget: program.budget.as_ref().map(budget).transpose()?,
        body: block(&program.body),
        span: span(program.body.span),
    })
}

fn budget(expr: &a::Expr) -> Result<c::Budget, Diagnostic> {
    let a::ExprKind::Record(fields) = &expr.kind else {
        return Err(Diagnostic::new(
            "budget must be a record of literal limits",
            expr.span,
        ));
    };
    let mut calls = None;
    let mut cost = None;
    let mut depth = None;
    let mut escalate = None;
    let mut unsure = None;
    let mut latency_p95 = None;
    let mut absent = None;
    let num = |value: &a::Expr| -> Option<f64> {
        match value.kind {
            a::ExprKind::Integer(n) => Some(n as f64),
            a::ExprKind::Decimal(n) => Some(n),
            _ => None,
        }
    };
    for (name, value) in fields {
        let invalid = || Diagnostic::new(format!("invalid budget limit '{name}'"), value.span);
        match name.as_str() {
            "calls" | "depth" | "escalate" => {
                let a::ExprKind::Integer(n) = value.kind else {
                    return Err(invalid());
                };
                let n = u64::try_from(n).map_err(|_| invalid())?;
                match name.as_str() {
                    "calls" => calls = Some(n),
                    "depth" => depth = Some(u32::try_from(n).map_err(|_| invalid())?),
                    _ => escalate = Some(n),
                }
            }
            "cost" => {
                let n = match value.kind {
                    a::ExprKind::Integer(n) => n as f64,
                    a::ExprKind::Decimal(n) => n,
                    _ => return Err(invalid()),
                };
                if !n.is_finite() || n < 0.0 {
                    return Err(invalid());
                }
                cost = Some(n);
            }
            // J-10 的 unsure 预算（只报不停）
            "unsure" => {
                let n = num(value).filter(|n| n.is_finite() && *n >= 0.0).ok_or_else(invalid)?;
                unsure = Some(n);
            }
            // B32 时延预算（秒）
            "latency_p95" => {
                let n = num(value).filter(|n| n.is_finite() && *n > 0.0).ok_or_else(invalid)?;
                latency_p95 = Some(n);
            }
            // B32 判断力缺席策略 {retry, backoff, then, breaker}
            "absent" => {
                let a::ExprKind::Record(fs) = &value.kind else { return Err(invalid()) };
                let mut retry = 2u32;
                let mut backoff = 1.0f64;
                let mut then = "escalate".to_string();
                let mut breaker = 3u32;
                for (k, v) in fs {
                    let bad = || Diagnostic::new(format!("invalid absent field '{k}'"), v.span);
                    match k.as_str() {
                        "retry" => retry = match v.kind { a::ExprKind::Integer(n) if n >= 0 => n as u32, _ => return Err(bad()) },
                        "breaker" => breaker = match v.kind { a::ExprKind::Integer(n) if n >= 1 => n as u32, _ => return Err(bad()) },
                        "backoff" => backoff = num(v).filter(|n| n.is_finite() && *n >= 0.0).ok_or_else(bad)?,
                        "then" => then = match &v.kind {
                            a::ExprKind::Text(t) if matches!(t.as_str(), "escalate" | "conservative" | "fail") => t.clone(),
                            _ => return Err(Diagnostic::new("absent.then must be \"escalate\", \"conservative\" or \"fail\"", v.span)),
                        },
                        _ => return Err(Diagnostic::new(format!("unknown absent field '{k}'"), v.span)),
                    }
                }
                absent = Some(c::AbsentPolicy { retry, backoff, then, breaker });
            }
            _ => {
                return Err(Diagnostic::new(
                    format!("unknown budget field '{name}'"),
                    value.span,
                ));
            }
        }
    }
    Ok(c::Budget {
        calls: calls.ok_or_else(|| Diagnostic::new("budget requires 'calls'", expr.span))?,
        cost: cost.ok_or_else(|| Diagnostic::new("budget requires 'cost'", expr.span))?,
        depth,
        escalate,
        unsure,
        absent,
        latency_p95,
    })
}

fn span(s: a::Span) -> c::Span {
    c::Span {
        start: s.start,
        end: s.end,
    }
}
fn ty(t: &a::Type) -> c::Type {
    match t {
        a::Type::Named(n) => c::Type::Named(n.clone()),
        a::Type::Applied(n, args) => c::Type::Applied(n.clone(), args.iter().map(ty).collect()),
        a::Type::Function(args, result) => {
            c::Type::Function(args.iter().map(ty).collect(), Box::new(ty(result)))
        }
        a::Type::Method {
            parameters,
            result,
            effects,
            captures_responsibility,
        } => c::Type::Method(c::MethodType {
            params: parameters.iter().map(ty).collect(),
            ret: Box::new(ty(result)),
            effects: effects.clone(),
            captures_responsibility: *captures_responsibility,
        }),
    }
}
fn function(f: &a::Function) -> c::Function {
    c::Function {
        parameters: f
            .parameters
            .iter()
            .map(|p| c::Parameter {
                name: p.name.clone(),
                annotation: p.annotation.as_ref().map(ty),
                span: span(p.span),
            })
            .collect(),
        result_type: f.result_type.as_ref().map(ty),
        effects: f.effects.clone(),
        body: block(&f.body),
    }
}
fn block(b: &a::Block) -> c::Block {
    c::Block {
        statements: b
            .statements
            .iter()
            .map(|s| match s {
                a::Statement::Let {
                    name,
                    annotation,
                    value,
                    span: at,
                } => c::Statement::Let {
                    name: name.clone(),
                    annotation: annotation.as_ref().map(ty),
                    value: expr(value),
                    span: span(*at),
                },
                a::Statement::Function {
                    name,
                    function: f,
                    span: at,
                } => c::Statement::Function {
                    name: name.clone(),
                    function: function(f),
                    span: span(*at),
                },
                a::Statement::Expression(e) => c::Statement::Expression(expr(e)),
            })
            .collect(),
        result: b.result.as_ref().map(|e| Box::new(expr(e))),
        span: span(b.span),
    }
}
fn expr(e: &a::Expr) -> c::Expr {
    use a::ExprKind as A;
    use c::ExprKind as C;
    let kind = match &e.kind {
        A::Integer(n) => C::Integer(*n),
        A::Decimal(n) => C::Decimal(*n),
        A::Bool(b) => C::Bool(*b),
        A::Text(t) => C::Text(t.clone()),
        A::Unit => C::Unit,
        A::Name(n) => C::Name(n.clone()),
        A::List(items) => C::List(items.iter().map(expr).collect()),
        A::Record(fields) => C::Record(fields.iter().map(|(n, e)| (n.clone(), expr(e))).collect()),
        A::Function(f) => C::Function(function(f)),
        A::Call {
            function,
            arguments,
        } => C::Call {
            function: Box::new(expr(function)),
            arguments: arguments.iter().map(expr).collect(),
        },
        A::Field { value, field } => C::Field {
            value: Box::new(expr(value)),
            field: field.clone(),
        },
        A::Index { value, index } => C::Index {
            value: Box::new(expr(value)),
            index: Box::new(expr(index)),
        },
        A::Unary { op, value } => C::Unary {
            op: op.clone(),
            value: Box::new(expr(value)),
        },
        A::Binary { op, left, right } => C::Binary {
            op: op.clone(),
            left: Box::new(expr(left)),
            right: Box::new(expr(right)),
        },
        A::If { condition, yes, no } => C::If {
            condition: Box::new(expr(condition)),
            yes: block(yes),
            no: block(no),
        },
        A::Block(b) => C::Block(block(b)),
    };
    c::Expr {
        kind,
        span: span(e.span),
    }
}
