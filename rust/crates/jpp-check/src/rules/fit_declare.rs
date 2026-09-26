//! `E-fit-declare-effect`：声明式拟合 `fit({declare: f, …}, …)` 的闭包只做计算（B153 (2)，步 20j-4）。
//!
//! 闭包在桥内对统计量求值，只见参数与内置。检查期对两种写法核它的体：`declare` 是函数字面量，或是全程序只绑定过
//! 一次、绑定到函数字面量的名字（`let f = fn …` 或 `fn f(…)`）。报两类：
//! - 体内出现效应、内核构造、`cut` / `handle` / `consume` / `escalate` / `literalize` / `state`、`fit`，或调用
//!   `content`、`mat`、`unsure`、`pending`（运行期 `host_builtins.rs::builtin` 入口同一张清单）；
//! - 体内引用外层名字（不是参数、不是体内绑定、不是内置）：运行期看不见它，报文说「经 extra 传入」。
//!
//! 其余写法（`declare` 是别的表达式、名字绑定不止一次）交运行期守卫。`if`、`loop` 与纯内置放行
//! （步 20j-4 预注册第 0 节第 4 条）。
//!
//! 依据：B153 (2)（地基/附注/2026-09-26-批6裁定.md §一：体内不得出现效应、构造或刷新点，静态 `E-fit-declare-effect`）；
//! 过程记录 `地基/过程记录/工程-步20j-4.md`。

use std::collections::HashMap;

use super::{Cx, Hooks, Rule};
use crate::*;
use jpp_ir::ir::{Host, Node};

/// 依据：B153 (2)（声明式拟合的闭包只做计算）
pub(crate) const RULE: Rule = Rule {
    code: "E-fit-declare-effect",
    requires: &[],
    hooks: Hooks {
        after: Some(after),
        ..Hooks::NONE
    },
};

/// 闭包体内不许调用的内置（效应与构造在 IR 里另有节点）
const 禁用内置: &[&str] = &["content", "mat", "unsure", "pending"];

fn after(cx: &Cx) -> Vec<Diagnostic> {
    let 函数 = 唯一函数绑定(&cx.p.body);
    let mut out = vec![];
    jpp_ir::ir::walk(&cx.p.body, &mut |e| {
        let Node::Fit { args, .. } = &e.node else {
            return;
        };
        let Some(Node::Host(Host::Record(fs))) = args.first().map(|a| &a.node) else {
            return;
        };
        let Some((_, d)) = fs.iter().find(|(k, _)| k == "declare") else {
            return;
        };
        let f = match &d.node {
            Node::Host(Host::Function(f)) => f,
            Node::Host(Host::Name(n)) => match 函数.get(n.as_str()) {
                Some(f) => *f,
                None => return,
            },
            _ => return,
        };
        let mut w = 扫 {
            scopes: vec![f.parameters.iter().map(|p| p.name.clone()).collect()],
            out: &mut out,
        };
        w.block(&f.body);
    });
    out
}

/// 全程序只绑定过一次、绑定到函数字面量的名字 → 那个函数
fn 唯一函数绑定(b: &Block) -> HashMap<&str, &Function> {
    let mut 次数: HashMap<&str, usize> = HashMap::new();
    let mut 函数: HashMap<&str, &Function> = HashMap::new();
    fn 数<'a>(
        b: &'a Block,
        次数: &mut HashMap<&'a str, usize>,
        函数: &mut HashMap<&'a str, &'a Function>,
    ) {
        for s in &b.statements {
            match s {
                Statement::Let { name, value, .. } => {
                    *次数.entry(name.as_str()).or_default() += 1;
                    if let Node::Host(Host::Function(f)) = &value.node {
                        函数.insert(name.as_str(), f);
                    }
                    数_expr(value, 次数, 函数);
                }
                Statement::Function { name, function, .. } => {
                    *次数.entry(name.as_str()).or_default() += 1;
                    函数.insert(name.as_str(), function);
                    数(&function.body, 次数, 函数);
                }
                Statement::Expr(e) => 数_expr(e, 次数, 函数),
            }
        }
        if let Some(r) = &b.result {
            数_expr(r, 次数, 函数);
        }
    }
    fn 数_expr<'a>(
        e: &'a Expr,
        次数: &mut HashMap<&'a str, usize>,
        函数: &mut HashMap<&'a str, &'a Function>,
    ) {
        for c in e.children() {
            数_expr(c, 次数, 函数);
        }
        if let Node::Host(Host::Function(f)) = &e.node {
            for p in &f.parameters {
                *次数.entry(p.name.as_str()).or_default() += 1;
            }
        }
        for b in e.blocks() {
            数(b, 次数, 函数);
        }
    }
    数(b, &mut 次数, &mut 函数);
    函数.retain(|n, _| 次数.get(n) == Some(&1));
    函数
}

struct 扫<'o> {
    scopes: Vec<Vec<String>>,
    out: &'o mut Vec<Diagnostic>,
}

impl 扫<'_> {
    fn 已绑定(&self, n: &str) -> bool {
        self.scopes.iter().any(|s| s.iter().any(|x| x == n))
    }

    fn block(&mut self, b: &Block) {
        self.scopes.push(vec![]);
        for s in &b.statements {
            match s {
                Statement::Let { name, value, .. } => {
                    self.expr(value);
                    self.scopes.last_mut().unwrap().push(name.clone());
                }
                Statement::Function { name, function, .. } => {
                    self.scopes.last_mut().unwrap().push(name.clone());
                    self.function(function);
                }
                Statement::Expr(e) => self.expr(e),
            }
        }
        if let Some(r) = &b.result {
            self.expr(r);
        }
        self.scopes.pop();
    }

    fn function(&mut self, f: &Function) {
        self.scopes
            .push(f.parameters.iter().map(|p| p.name.clone()).collect());
        self.block(&f.body);
        self.scopes.pop();
    }

    fn 报(&mut self, 什么: &str, e: &Expr) {
        // 依据：B153 (2)（闭包体内不得出现效应、构造或刷新点）
        self.out.push(Diagnostic::error(
            "E-fit-declare-effect",
            format!(
                "声明式拟合的闭包里不能有 {什么}：闭包在桥内对统计量求值，只做计算（算术、比较、if、max / min 等纯内置），不发判断、不调效应、不跑构造、不造出口（B153）。修法：把它移到 fit 之外，结果经 extra 传入"
            ),
            e.span,
        ));
    }

    fn expr(&mut self, e: &Expr) {
        match &e.node {
            Node::Effect { .. } => self.报("效应", e),
            Node::Construct { name, .. } => self.报(&format!("构造 {name}"), e),
            Node::Fit { .. } => self.报("fit", e),
            Node::Cut { .. } => self.报("cut", e),
            Node::Handle { .. } => self.报("handle", e),
            Node::Consume { how, .. } => self.报(how.name(), e),
            Node::State { .. } => self.报("state", e),
            Node::Host(Host::Function(f)) => self.function(f),
            Node::Host(Host::Name(n)) => {
                if !self.已绑定(n) && !is_builtin(n) {
                    // 依据：B153 (2)（闭包在桥内求值，环境只含参数；外层的值经 extra 传入）
                    self.out.push(Diagnostic::error(
                        "E-fit-declare-effect",
                        format!(
                            "声明式拟合的闭包引用了外层的 {n}：闭包在桥内求值，只见参数与内置，看不见外层的名字（B153）。修法：把 {n} 放进 fit 的第三个参数 extra，闭包按位多收一个参数，例如 fit({{declare: fn(a, {n}) {{ … }}}}, [r], [{n}])"
                        ),
                        e.span,
                    ));
                }
            }
            Node::Host(Host::Call { callee, args, .. }) => {
                if let Node::Host(Host::Name(n)) = &callee.node
                    && !self.已绑定(n)
                    && 禁用内置.contains(&n.as_str())
                {
                    self.报(&format!("{n}(…)"), e);
                }
                self.expr(callee);
                for a in args {
                    self.expr(a);
                }
            }
            _ => {
                for c in e.children() {
                    self.expr(c);
                }
                for b in e.blocks() {
                    self.block(b);
                }
            }
        }
    }
}
