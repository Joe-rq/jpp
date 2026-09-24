//! IR 的求值视图（步 12c）：运行时读 IR，但语言形式、效应与构造在求值上仍是「调用一个内置」。
//! 这里把各类节点统一成求值者需要的形状：宿主节点原样，语言形式与效应节点呈现为
//! 「以名字调用、实参按源码顺序」。求值与运行期分析（推测、提升、守卫、捕获指纹）只经这一处读节点。
//!
//! 步 13a 从 `jpp-core/src/interp/view.rs` 原样搬来：运行时（求值）与 `jpp-plan`（推测、提升、向量化的
//! 分析）都要按同一口径读节点，而效应节点的名字取自效应表（[`crate::spec`]），所以放在效应表旁边；
//! `jpp-runtime` 与 `jpp-plan` 都可依赖 `jpp-effects`（`20` §2.2 第 1 条），两边不必各留一份。

use jpp_ir::ir::Span;
use jpp_ir::ir::{Block, Expr, Function, Host, Node};

/// 被调用者：语言形式与效应节点只有名字（按效应表或形式名），宿主调用是一个表达式。
#[derive(Clone, Copy)]
pub enum Callee<'a> {
    Name(&'a str),
    Expr(&'a Expr),
}

impl<'a> Callee<'a> {
    /// 被调用者是一个名字吗（语言形式，或宿主调用里直接写的名字）
    pub fn name(&self) -> Option<&'a str> {
        match self {
            Callee::Name(n) => Some(n),
            Callee::Expr(e) => match &e.node {
                Node::Host(Host::Name(n)) => Some(n.as_str()),
                _ => None,
            },
        }
    }
}

pub enum K<'a> {
    Integer(i64),
    Decimal(f64),
    Bool(bool),
    Text(&'a str),
    Unit,
    Name(&'a str),
    List(&'a [Expr]),
    Record(&'a [(String, Expr)]),
    Function(&'a Function),
    Call {
        callee: Callee<'a>,
        args: Vec<&'a Expr>,
    },
    Field {
        value: &'a Expr,
        field: &'a str,
    },
    Index {
        value: &'a Expr,
        index: &'a Expr,
    },
    Unary {
        op: &'a str,
        value: &'a Expr,
    },
    Binary {
        op: &'a str,
        left: &'a Expr,
        right: &'a Expr,
    },
    If {
        condition: &'a Expr,
        yes: &'a Block,
        no: &'a Block,
    },
    Block(&'a Block),
}

fn call<'a>(name: &'a str, args: Vec<&'a Expr>) -> K<'a> {
    K::Call {
        callee: Callee::Name(name),
        args,
    }
}

pub fn kind(e: &Expr) -> K<'_> {
    match &e.node {
        Node::State { on, opts, rest, .. } => {
            let mut a: Vec<&Expr> = vec![on];
            if let Some(o) = opts {
                a.push(o);
            }
            a.extend(rest.iter());
            call("state", a)
        }
        Node::Effect { effect, inputs, .. } => call(
            crate::spec(*effect).name,
            inputs.iter().map(|(_, x)| x).collect(),
        ),
        Node::Cut { reading, rest, .. } => {
            let mut a: Vec<&Expr> = vec![reading];
            a.extend(rest.iter());
            call("cut", a)
        }
        Node::Fit { args, .. } => call("fit", args.iter().collect()),
        Node::Loop { bound, rest, .. } => {
            let mut a: Vec<&Expr> = vec![bound];
            a.extend(rest.iter());
            call("loop", a)
        }
        Node::Handle {
            exit, arms, rest, ..
        } => {
            let mut a: Vec<&Expr> = vec![exit, arms];
            a.extend(rest.iter());
            call("handle", a)
        }
        Node::Consume { how, args, .. } => call(how.name(), args.iter().collect()),
        Node::Construct { name, args, .. } => call(name.as_str(), args.iter().collect()),
        Node::Host(h) => match h {
            Host::Integer(v) => K::Integer(*v),
            Host::Decimal(v) => K::Decimal(*v),
            Host::Bool(v) => K::Bool(*v),
            Host::Text(v) => K::Text(v),
            Host::Unit => K::Unit,
            Host::Name(n) => K::Name(n),
            Host::List(xs) => K::List(xs),
            Host::Record(fs) => K::Record(fs),
            Host::Function(f) => K::Function(f),
            Host::Call { callee, args, .. } => K::Call {
                callee: Callee::Expr(callee),
                args: args.iter().collect(),
            },
            Host::Field { value, field } => K::Field { value, field },
            Host::Index { value, index } => K::Index { value, index },
            Host::Unary { op, value } => K::Unary { op, value },
            Host::Binary { op, left, right } => K::Binary { op, left, right },
            Host::If {
                condition, yes, no, ..
            } => K::If { condition, yes, no },
            Host::Block(b) => K::Block(b),
        },
    }
}

/// 名字节点的位置（报错用）：语言形式与效应节点的名字没有单独的节点，取调用节点的位置。
pub fn callee_span(c: Callee<'_>, call: &Expr) -> Span {
    match c {
        Callee::Name(_) => call.span,
        Callee::Expr(e) => e.span,
    }
}

/// 结构相同吗（忽略位置、节点号、站点号）：`lift` 判「同状态」用它。
pub fn same_shape(a: &Expr, b: &Expr) -> bool {
    fn strip(e: &Expr) -> serde_json::Value {
        let mut j = serde_json::to_value(e).unwrap_or(serde_json::Value::Null);
        fn drop_meta(j: &mut serde_json::Value) {
            match j {
                serde_json::Value::Object(m) => {
                    m.remove("span");
                    m.remove("id");
                    m.remove("site");
                    m.values_mut().for_each(drop_meta);
                }
                serde_json::Value::Array(a) => a.iter_mut().for_each(drop_meta),
                _ => {}
            }
        }
        drop_meta(&mut j);
        j
    }
    strip(a) == strip(b)
}
