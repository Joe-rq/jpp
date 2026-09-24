//! 语法趟：走一遍程序，在内置调用、`let`、语句表达式、函数体结束四处调规则钩子
//! （J-03 / J-05 / J-06 / E7 / J-13 / J-14 的判定在 `rules/`）。
//!
//! 走的时候只带两样：当前所在的最内层函数（IR 节点，与站点表 `SiteInfo.function` 同口径），和
//! 「这一轮里会变的名字」（名字分析的一部分，J-13 读）。是否在迭代体里、是否在 `map` / `filter`
//! 体内，规则从站点表与方法位对照取（B71；原先在这里用 `Ctx` 从表达式形状重算）。

#![allow(unused_imports)]
use crate::rules::CallSite;
use crate::*;
use jpp_ir::key::NodeId;

impl Checker<'_> {
    pub(crate) fn syntax(&mut self, p: &Program) {
        self.syntax_block(&p.body, None, &[]);
    }

    pub(crate) fn syntax_block(&mut self, b: &Block, func: Option<NodeId>, iter_params: &[String]) {
        // **`iter_params` 的含义是「这一轮里会变的名字」，不是「循环的形参」。**
        // 块里 `let x = f(i)` 之后，`x` 也是每轮会变的——不把它算进来，
        // J-13 的 `seq_const` 会把它当成常量而误报。
        let in_iteration = self.cx.sites.in_iteration(func);
        let mut varying: Vec<String> = iter_params.to_vec();
        for s in &b.statements {
            match s {
                Statement::Let {
                    value, name, span, ..
                } => {
                    self.syntax_expr(value, func, &varying);
                    self.on_bind(value, name, *span, b);
                    if in_iteration && varying.iter().any(|p| mentions(value, p)) {
                        varying.push(name.clone());
                    }
                }
                // 函数定义是新的词法环境：这一轮会变的名字不穿过它
                Statement::Function { function, .. } => self.syntax_function(function),
                Statement::Expr(e) => {
                    self.syntax_expr(e, func, &varying);
                    self.on_stmt(e);
                }
            }
        }
        if let Some(r) = &b.result {
            self.syntax_expr(r, func, &varying);
        }
    }

    pub(crate) fn syntax_function(&mut self, f: &Function) {
        self.syntax_block(&f.body, Some(f.id), &[]);
        self.on_function(f);
    }

    pub(crate) fn syntax_expr(&mut self, e: &Expr, func: Option<NodeId>, iter_params: &[String]) {
        if let Some(name) = call_name(e) {
            let args = call_args(e);
            let name = name.to_string();
            self.on_call(&CallSite {
                name: &name,
                args: &args,
                span: e.span,
                function: func,
                iter_params,
            });
            for a in args.iter() {
                match a.kind() {
                    // 方法位上的函数字面量：体里这一轮会变的名字从它的形参起算
                    ExprKind::Function(f) if self.cx.sites.in_iteration(Some(f.id)) => {
                        let params: Vec<String> =
                            f.parameters.iter().map(|p| p.name.clone()).collect();
                        self.syntax_block(&f.body, Some(f.id), &params);
                        self.on_function(f);
                    }
                    _ => self.syntax_expr(a, func, iter_params),
                }
            }
            return;
        }
        match e.kind() {
            ExprKind::Function(f) => self.syntax_function(f),
            ExprKind::List(items) => items
                .iter()
                .for_each(|x| self.syntax_expr(x, func, iter_params)),
            ExprKind::Record(fields) => fields
                .iter()
                .for_each(|(_, x)| self.syntax_expr(x, func, iter_params)),
            ExprKind::Call {
                function,
                arguments,
            } => {
                if let Some(fe) = function.expr() {
                    self.syntax_expr(fe, func, iter_params);
                }
                arguments
                    .iter()
                    .for_each(|x| self.syntax_expr(x, func, iter_params));
            }
            ExprKind::Field { value, .. } => self.syntax_expr(value, func, iter_params),
            ExprKind::Index { value, index } => {
                self.syntax_expr(value, func, iter_params);
                self.syntax_expr(index, func, iter_params);
            }
            ExprKind::Unary { value, .. } => self.syntax_expr(value, func, iter_params),
            ExprKind::Binary { left, right, .. } => {
                self.syntax_expr(left, func, iter_params);
                self.syntax_expr(right, func, iter_params);
            }
            ExprKind::If { condition, yes, no } => {
                self.syntax_expr(condition, func, iter_params);
                self.syntax_block(yes, func, iter_params);
                self.syntax_block(no, func, iter_params);
            }
            ExprKind::Block(b) => self.syntax_block(b, func, iter_params),
            _ => {}
        }
    }
}
