//! 检查器读 IR 的视图（步 12b）：把 IR 节点按「宿主表达式 + 调用」的形状呈现给规则单元。
//!
//! IR 把语言形式与效应识别成专门的节点（`20` §2.3），检查器的规则大多按「这是对哪个内置的调用、
//! 实参是什么」判定，所以这里把形式节点还原成「内置名 + 实参（源顺序）」的调用视图。形式节点
//! 没有被调者表达式（被调者就是那个内置名），视图里记作 [`Callee::Builtin`]；只在名字没被遮蔽时
//! 才会降成形式节点（降级处与检查器同一条作用域口径），所以对它不需要再做名字解析。

use jpp_effects::spec;
use jpp_ir::ir::{Block, Expr, Function, Host, Node};

/// 被调者：普通表达式，或降级时已认出的内置名。
#[derive(Clone, Copy)]
pub(crate) enum Callee<'a> {
    Expr(&'a Expr),
    Builtin(&'a str),
}

impl<'a> Callee<'a> {
    /// 被调者的名字：内置名，或名字表达式的名字。
    pub(crate) fn name(&self) -> Option<&'a str> {
        match self {
            Callee::Builtin(n) => Some(n),
            Callee::Expr(e) => match &e.node {
                Node::Host(Host::Name(n)) => Some(n.as_str()),
                _ => None,
            },
        }
    }
    pub(crate) fn expr(&self) -> Option<&'a Expr> {
        match self {
            Callee::Expr(e) => Some(e),
            Callee::Builtin(_) => None,
        }
    }
    /// 被调者的形状：内置名呈现为名字
    pub(crate) fn kind(&self) -> ExprKind<'a> {
        match self {
            Callee::Expr(e) => e.kind(),
            Callee::Builtin(n) => ExprKind::Name(n),
        }
    }
}

/// 一个节点在检查器眼里的形状（与旧核心语法树的 `ExprKind` 同构）。
pub(crate) enum ExprKind<'a> {
    Integer(i64),
    Decimal,
    Bool,
    Text(&'a String),
    Unit,
    Name(&'a str),
    List(&'a [Expr]),
    Record(&'a [(String, Expr)]),
    Function(&'a Function),
    Call {
        function: Callee<'a>,
        arguments: Vec<&'a Expr>,
    },
    Field {
        value: &'a Expr,
        field: &'a String,
    },
    Index {
        value: &'a Expr,
        index: &'a Expr,
    },
    Unary {
        op: &'a String,
        value: &'a Expr,
    },
    Binary {
        op: &'a String,
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

pub(crate) trait View {
    fn kind(&self) -> ExprKind<'_>;
}

fn call<'a>(name: &'a str, args: Vec<&'a Expr>) -> ExprKind<'a> {
    ExprKind::Call {
        function: Callee::Builtin(name),
        arguments: args,
    }
}

impl View for Expr {
    fn kind(&self) -> ExprKind<'_> {
        match &self.node {
            Node::State { .. } => call("state", self.children()),
            Node::Effect { effect, .. } => call(spec(*effect).name, self.children()),
            Node::Cut { .. } => call("cut", self.children()),
            Node::Fit { .. } => call("fit", self.children()),
            Node::Loop { .. } => call("loop", self.children()),
            Node::Handle { .. } => call("handle", self.children()),
            Node::Consume { how, .. } => call(how.name(), self.children()),
            Node::Construct { name, args, .. } => ExprKind::Call {
                function: Callee::Builtin(name.as_str()),
                arguments: args.iter().collect(),
            },
            Node::Host(h) => match h {
                Host::Integer(v) => ExprKind::Integer(*v),
                Host::Decimal(_) => ExprKind::Decimal,
                Host::Bool(_) => ExprKind::Bool,
                Host::Text(v) => ExprKind::Text(v),
                Host::Unit => ExprKind::Unit,
                Host::Name(n) => ExprKind::Name(n.as_str()),
                Host::List(xs) => ExprKind::List(xs),
                Host::Record(fs) => ExprKind::Record(fs),
                Host::Function(f) => ExprKind::Function(f),
                Host::Call { callee, args, .. } => ExprKind::Call {
                    function: Callee::Expr(callee),
                    arguments: args.iter().collect(),
                },
                Host::Field { value, field } => ExprKind::Field { value, field },
                Host::Index { value, index } => ExprKind::Index { value, index },
                Host::Unary { op, value } => ExprKind::Unary { op, value },
                Host::Binary { op, left, right } => ExprKind::Binary { op, left, right },
                Host::If {
                    condition, yes, no, ..
                } => ExprKind::If { condition, yes, no },
                Host::Block(b) => ExprKind::Block(b),
            },
        }
    }
}
