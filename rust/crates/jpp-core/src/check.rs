//! 静态检查器：能在不执行的情况下判定的纪律，报文带 `Span` 与规则号。
//!
//! 分工：`interp.rs` 已经在运行期精确把关 J-02（禁自指，要实际材料）、J-05（出口 `consumed` 标记，
//! 返回前核）、J-06 键重复即停、J-07 预算、J-12 Fail、J-18 账本头。这里只做**静态确定**的那一半，
//! 不重复运行期已经判准的东西；宁可漏报也不误报——误报会卡住已经写好的源码。
//!
//! 依据：`11-语言规范-v1.md` §诊断（E1…E16）与 `12-IR与类契约-v0.1.md` §5（J-01…J-18）。
//! 报文格式沿用 Python 检查器：`规则: 一句话。修法：…`。

use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

use crate::ast::*;
use crate::interp::BUILTINS;

/// 效应名（函数 `!{…}` 标注里能出现的）。`transform` 是记账变换不是效应形式，不在此列。
pub const EFFECT_NAMES: &[&str] = &["judge", "gen", "do", "ask"];

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Severity {
    Error,
    Warning,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Diagnostic {
    /// 规则号：`J-xx` / `Exx`（依据文本编号）或 `E-xxx` / `W-xxx`（core 本地码，见 INTERFACE.md）
    pub rule: String,
    pub severity: Severity,
    pub message: String,
    pub span: Span,
}

impl Diagnostic {
    pub fn error(rule: &str, msg: impl Into<String>, span: Span) -> Diagnostic {
        Diagnostic {
            rule: rule.into(),
            severity: Severity::Error,
            message: msg.into(),
            span,
        }
    }
    pub fn warning(rule: &str, msg: impl Into<String>, span: Span) -> Diagnostic {
        Diagnostic {
            rule: rule.into(),
            severity: Severity::Warning,
            message: msg.into(),
            span,
        }
    }
    pub fn render(&self) -> String {
        format!(
            "{}: {} @{}..{}",
            self.rule, self.message, self.span.start, self.span.end
        )
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Report {
    pub diagnostics: Vec<Diagnostic>,
}

impl Report {
    pub fn errors(&self) -> Vec<&Diagnostic> {
        self.diagnostics
            .iter()
            .filter(|d| d.severity == Severity::Error)
            .collect()
    }
    pub fn warnings(&self) -> Vec<&Diagnostic> {
        self.diagnostics
            .iter()
            .filter(|d| d.severity == Severity::Warning)
            .collect()
    }
    /// 没有错（warning 不阻塞运行）
    pub fn is_ok(&self) -> bool {
        self.errors().is_empty()
    }
    /// 第一条给定规则号的诊断
    pub fn find(&self, rule: &str) -> Option<&Diagnostic> {
        self.diagnostics.iter().find(|d| d.rule == rule)
    }
    pub fn render(&self) -> String {
        self.diagnostics
            .iter()
            .map(|d| format!("{}\n", d.render()))
            .collect()
    }
}

/// 静态检查一个程序。`errors()` 非空即不应运行。
pub fn check(program: &Program) -> Report {
    let mut c = Checker::default();
    c.budget(program);
    c.names_and_readings(program);
    c.syntax(program);
    c.effects(program);
    c.out.sort_by_key(|d| (d.span.start, d.span.end));
    Report { diagnostics: c.out }
}

// ---------------------------------------------------------------- 抽象值类别

/// 只区分「读数 / 出口 / 其它」——J-01 与 J-05 的静态面要的就这三档。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kind {
    Reading,
    Exit,
    Other,
}

struct Scope {
    /// 块内所有绑定名。函数体里全可见：解释器的环境节点是共享的，递归与互相引用都成立。
    declared: HashMap<String, Kind>,
    /// 绑定到具名方法的，留下参数表：调用点据此核参数个数与字面量类型
    signatures: HashMap<String, Vec<Option<Type>>>,
    /// 已经走过的绑定。语句位置的直接引用只能看见这些。
    defined: HashSet<String>,
}

impl Scope {
    fn new() -> Scope {
        Scope {
            declared: HashMap::new(),
            signatures: HashMap::new(),
            defined: HashSet::new(),
        }
    }
}

/// 字面量与类型标注各自落在哪一档。只有两边都认得、且不同，才判不符。
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Shape {
    Int,
    Decimal,
    Bool,
    Text,
    List,
    Record,
    Method,
    Unit,
}

impl Shape {
    fn of_literal(e: &Expr) -> Option<Shape> {
        Some(match &e.kind {
            ExprKind::Integer(_) => Shape::Int,
            ExprKind::Decimal(_) => Shape::Decimal,
            ExprKind::Bool(_) => Shape::Bool,
            ExprKind::Text(_) => Shape::Text,
            ExprKind::List(_) => Shape::List,
            ExprKind::Record(_) => Shape::Record,
            ExprKind::Function(_) => Shape::Method,
            ExprKind::Unit => Shape::Unit,
            _ => return None,
        })
    }
    fn of_type(t: &Type) -> Option<Shape> {
        Some(match t {
            Type::Function(_, _) => Shape::Method,
            Type::Applied(n, _) | Type::Named(n) => match n.as_str() {
                "Int" => Shape::Int,
                "Decimal" | "Float" => Shape::Decimal,
                "Bool" => Shape::Bool,
                "Text" => Shape::Text,
                "List" => Shape::List,
                "Record" => Shape::Record,
                "Fn" | "Method" => Shape::Method,
                "Unit" => Shape::Unit,
                // Mat / State / Question / Reading / Exit 等由运行期把关
                _ => return None,
            },
        })
    }
    fn name(&self) -> &'static str {
        match self {
            Shape::Int => "Int",
            Shape::Decimal => "Decimal",
            Shape::Bool => "Bool",
            Shape::Text => "Text",
            Shape::List => "List",
            Shape::Record => "Record",
            Shape::Method => "方法",
            Shape::Unit => "Unit",
        }
    }
    /// 整数字面量可以当小数用；其余不互通
    fn fits(arg: Shape, want: Shape) -> bool {
        arg == want || (arg == Shape::Int && want == Shape::Decimal)
    }
}

struct FuncInfo {
    name: String,
    declared: Option<Vec<String>>,
    /// 实际可见效应；None = ⊤（体内有静态解析不了的被调者，不检查）
    inferred: Option<HashSet<String>>,
    span: Span,
    body: Block,
}

#[derive(Default)]
struct Checker {
    out: Vec<Diagnostic>,
    functions: Vec<FuncInfo>,
    by_name: HashMap<String, Vec<usize>>,
    /// 判定为读数的表达式节点（按地址标记，不依赖 Span 唯一）
    readings: HashSet<*const Expr>,
}

fn is_builtin(n: &str) -> bool {
    BUILTINS.contains(&n)
}

/// 高阶内置里哪一位收方法
fn method_positions(builtin: &str) -> &'static [usize] {
    match builtin {
        "map" | "filter" => &[1],
        "fold" | "loop" => &[2],
        "transform" => &[0],
        _ => &[],
    }
}

fn call_name(e: &Expr) -> Option<&str> {
    match &e.kind {
        ExprKind::Call { function, .. } => match &function.kind {
            ExprKind::Name(n) => Some(n.as_str()),
            _ => None,
        },
        _ => None,
    }
}

fn call_args(e: &Expr) -> &[Expr] {
    match &e.kind {
        ExprKind::Call { arguments, .. } => arguments,
        _ => &[],
    }
}

fn mentions(e: &Expr, name: &str) -> bool {
    let mut found = false;
    walk_expr(e, &mut |x| {
        if let ExprKind::Name(n) = &x.kind {
            if n == name {
                found = true;
            }
        }
    });
    found
}

fn walk_expr(e: &Expr, f: &mut impl FnMut(&Expr)) {
    f(e);
    match &e.kind {
        ExprKind::List(items) => items.iter().for_each(|x| walk_expr(x, f)),
        ExprKind::Record(fields) => fields.iter().for_each(|(_, x)| walk_expr(x, f)),
        ExprKind::Function(func) => walk_block(&func.body, f),
        ExprKind::Call {
            function,
            arguments,
        } => {
            walk_expr(function, f);
            arguments.iter().for_each(|x| walk_expr(x, f));
        }
        ExprKind::Field { value, .. } => walk_expr(value, f),
        ExprKind::Index { value, index } => {
            walk_expr(value, f);
            walk_expr(index, f);
        }
        ExprKind::Unary { value, .. } => walk_expr(value, f),
        ExprKind::Binary { left, right, .. } => {
            walk_expr(left, f);
            walk_expr(right, f);
        }
        ExprKind::If { condition, yes, no } => {
            walk_expr(condition, f);
            walk_block(yes, f);
            walk_block(no, f);
        }
        ExprKind::Block(b) => walk_block(b, f),
        _ => {}
    }
}

fn walk_block(b: &Block, f: &mut impl FnMut(&Expr)) {
    for s in &b.statements {
        match s {
            Statement::Let { value, .. } => walk_expr(value, f),
            Statement::Function { function, .. } => walk_block(&function.body, f),
            Statement::Expression(e) => walk_expr(e, f),
        }
    }
    if let Some(r) = &b.result {
        walk_expr(r, f);
    }
}

// ---------------------------------------------------------------- 预算（E12 / E10）

impl Checker {
    fn budget(&mut self, p: &Program) {
        let Some(b) = &p.budget else {
            self.out.push(Diagnostic::error(
                "E12",
                "程序缺 budget：预算必填，没有它就没有「超即停」的依据。修法：源码首行写 `budget {calls: N, cost: X, depth: D};`",
                p.span,
            ));
            return;
        };
        if b.escalate.unwrap_or(0) == 0 {
            let mut site = None;
            walk_block(&p.body, &mut |e| {
                if site.is_none() && call_name(e) == Some("ask") {
                    site = Some(e.span);
                }
            });
            if let Some(span) = site {
                self.out.push(Diagnostic::error(
                    "E10",
                    "程序里有 ask 而 budget 的 escalate 是 0：问人的次数上限没给，ask 只会直接挂起。修法：budget 里写 `escalate: N`",
                    span,
                ));
            }
        }
    }
}

// ---------------------------------------------------------------- 名字与读数（E-name / J-01 / W-shadow）

impl Checker {
    fn names_and_readings(&mut self, p: &Program) {
        let mut scopes = vec![];
        self.scan_block(&p.body, &mut scopes, 0);
    }

    /// `fn_depth > 0` 表示正在某个函数体内：该位置能看见所属块的全部绑定。
    fn scan_block(&mut self, b: &Block, scopes: &mut Vec<Scope>, fn_depth: usize) {
        let mut scope = Scope::new();
        for s in &b.statements {
            match s {
                Statement::Let {
                    name, span, value, ..
                } => {
                    scope.declared.insert(name.clone(), Kind::Other);
                    if let ExprKind::Function(f) = &value.kind {
                        scope.signatures.insert(
                            name.clone(),
                            f.parameters.iter().map(|p| p.annotation.clone()).collect(),
                        );
                    }
                    self.shadow(name, *span);
                }
                Statement::Function {
                    name,
                    span,
                    function,
                } => {
                    scope.declared.insert(name.clone(), Kind::Other);
                    scope.signatures.insert(
                        name.clone(),
                        function
                            .parameters
                            .iter()
                            .map(|p| p.annotation.clone())
                            .collect(),
                    );
                    self.shadow(name, *span);
                }
                Statement::Expression(_) => {}
            }
        }
        scopes.push(scope);
        for s in &b.statements {
            match s {
                Statement::Let { name, value, .. } => {
                    let k = self.scan_expr(value, scopes, fn_depth);
                    let top = scopes.last_mut().unwrap();
                    top.declared.insert(name.clone(), k);
                    top.defined.insert(name.clone());
                }
                Statement::Function { name, function, .. } => {
                    self.scan_function(function, scopes, fn_depth);
                    scopes.last_mut().unwrap().defined.insert(name.clone());
                }
                Statement::Expression(e) => {
                    self.scan_expr(e, scopes, fn_depth);
                }
            }
        }
        if let Some(r) = &b.result {
            self.scan_expr(r, scopes, fn_depth);
        }
        scopes.pop();
    }

    fn scan_function(&mut self, f: &Function, scopes: &mut Vec<Scope>, fn_depth: usize) {
        let mut scope = Scope::new();
        for p in &f.parameters {
            scope.declared.insert(p.name.clone(), Kind::Other);
            scope.defined.insert(p.name.clone());
            self.shadow(&p.name, p.span);
        }
        scopes.push(scope);
        self.scan_block(&f.body, scopes, fn_depth + 1);
        scopes.pop();
    }

    fn shadow(&mut self, name: &str, span: Span) {
        if is_builtin(name) {
            self.out.push(Diagnostic::warning(
                "W-shadow",
                format!("名字 {name} 盖住了同名内置操作：这个块里 {name}(…) 调的是你的定义，内置版本取不到了。修法：换个名字"),
                span,
            ));
        }
    }

    /// 名字在当前位置的类别；None = 没有这个绑定
    fn lookup(&self, scopes: &[Scope], name: &str) -> Option<Kind> {
        scopes
            .iter()
            .rev()
            .find_map(|s| s.declared.get(name).copied())
    }

    fn defined_yet(&self, scopes: &[Scope], name: &str, fn_depth: usize) -> bool {
        for s in scopes.iter().rev() {
            if s.declared.contains_key(name) {
                return fn_depth > 0 || s.defined.contains(name);
            }
        }
        false
    }

    /// 这个名字在此处绑定到的具名方法的参数表（被更近的绑定盖住时取不到）
    fn signature(&self, scopes: &[Scope], name: &str) -> Option<Vec<Option<Type>>> {
        for s in scopes.iter().rev() {
            if s.declared.contains_key(name) {
                return s.signatures.get(name).cloned();
            }
        }
        None
    }

    /// 参数个数不对（E-arity）、字面量与标注不符（E-type）。两者都只在静态确定时才报。
    fn named_call(&mut self, name: &str, sig: &[Option<Type>], args: &[Expr], span: Span) {
        if args.len() != sig.len() {
            self.out.push(Diagnostic::error(
                "E-arity",
                format!("{name} 要 {} 个参数，这里给了 {}", sig.len(), args.len()),
                span,
            ));
            return;
        }
        for (annotation, arg) in sig.iter().zip(args) {
            let (Some(t), Some(got)) = (annotation.as_ref(), Shape::of_literal(arg)) else {
                continue;
            };
            let Some(want) = Shape::of_type(t) else {
                continue;
            };
            if !Shape::fits(got, want) {
                self.out.push(Diagnostic::error(
                    "E-type",
                    format!(
                        "{name} 的这个参数标的是 {}，这里给的是 {} 字面量",
                        want.name(),
                        got.name()
                    ),
                    arg.span,
                ));
            }
        }
    }

    /// 这个名字在此处是否解析成内置（没有被用户绑定盖住）
    fn resolves_to_builtin(&self, scopes: &[Scope], name: &str) -> bool {
        is_builtin(name) && self.lookup(scopes, name).is_none()
    }

    fn scan_expr(&mut self, e: &Expr, scopes: &mut Vec<Scope>, fn_depth: usize) -> Kind {
        let kind = self.scan_expr_inner(e, scopes, fn_depth);
        if kind == Kind::Reading {
            self.readings.insert(e as *const Expr);
        }
        kind
    }

    fn scan_expr_inner(&mut self, e: &Expr, scopes: &mut Vec<Scope>, fn_depth: usize) -> Kind {
        match &e.kind {
            ExprKind::Name(n) => {
                if self.resolves_to_builtin(scopes, n) {
                    return Kind::Other;
                }
                match self.lookup(scopes, n) {
                    Some(k) => {
                        if !self.defined_yet(scopes, n, fn_depth) {
                            self.out.push(Diagnostic::error(
                                "E-name",
                                format!(
                                    "名字 {n} 在定义之前被用。修法：把 {n} 的定义移到这一行之前"
                                ),
                                e.span,
                            ));
                        }
                        k
                    }
                    None => {
                        self.out.push(Diagnostic::error(
                            "E-name",
                            format!("未定义的名字 {n}：既不是这个块里的绑定，也不是内置操作"),
                            e.span,
                        ));
                        Kind::Other
                    }
                }
            }
            ExprKind::List(items) => {
                for x in items {
                    self.scan_expr(x, scopes, fn_depth);
                }
                Kind::Other
            }
            ExprKind::Record(fields) => {
                for (_, x) in fields {
                    self.scan_expr(x, scopes, fn_depth);
                }
                Kind::Other
            }
            ExprKind::Function(f) => {
                self.scan_function(f, scopes, fn_depth);
                Kind::Other
            }
            ExprKind::Block(b) => {
                self.scan_block(b, scopes, fn_depth);
                Kind::Other
            }
            ExprKind::If { condition, yes, no } => {
                if self.scan_expr(condition, scopes, fn_depth) == Kind::Reading {
                    self.reading_err(condition.span, "读数不能当 if 的条件");
                }
                self.scan_block(yes, scopes, fn_depth);
                self.scan_block(no, scopes, fn_depth);
                Kind::Other
            }
            ExprKind::Field { value, field } => {
                if self.scan_expr(value, scopes, fn_depth) == Kind::Reading {
                    self.reading_err(value.span, format!("读数没有字段 {field} 可读"));
                }
                Kind::Other
            }
            ExprKind::Index { value, index } => {
                let k = self.scan_expr(value, scopes, fn_depth);
                self.scan_expr(index, scopes, fn_depth);
                // judge(state, [题…]) 给的是读数列表，取下标仍是读数
                k
            }
            ExprKind::Unary { op, value } => {
                if self.scan_expr(value, scopes, fn_depth) == Kind::Reading {
                    self.reading_err(value.span, format!("读数不能做一元 {op}"));
                }
                Kind::Other
            }
            ExprKind::Binary { op, left, right } => {
                let a = self.scan_expr(left, scopes, fn_depth);
                let b = self.scan_expr(right, scopes, fn_depth);
                if a == Kind::Reading {
                    self.reading_err(left.span, format!("读数不能做 {op}：读数不可比、不可算"));
                }
                if b == Kind::Reading {
                    self.reading_err(right.span, format!("读数不能做 {op}：读数不可比、不可算"));
                }
                Kind::Other
            }
            ExprKind::Call {
                function,
                arguments,
            } => {
                self.scan_expr(function, scopes, fn_depth);
                for a in arguments {
                    self.scan_expr(a, scopes, fn_depth);
                }
                let builtin = match &function.kind {
                    ExprKind::Name(n) if self.resolves_to_builtin(scopes, n) => Some(n.clone()),
                    _ => None,
                };
                if let ExprKind::Name(n) = &function.kind {
                    if let Some(sig) = self.signature(scopes, n) {
                        self.named_call(n, &sig, arguments, e.span);
                    }
                }
                match builtin {
                    Some(n) => {
                        self.builtin_call(&n, arguments);
                        match n.as_str() {
                            "judge" => Kind::Reading,
                            "cut" | "unsure" | "ask" => Kind::Exit,
                            _ => Kind::Other,
                        }
                    }
                    None => Kind::Other,
                }
            }
            _ => Kind::Other,
        }
    }

    /// 这个位置上的值是读数吗？只穿过列表 / 记录字面量——它们原样装着值；
    /// 调用、字段等节点的类别是它们自己的结果（`cut(judge(…))` 是出口，不是读数）。
    fn is_reading(&self, e: &Expr) -> bool {
        if self.readings.contains(&(e as *const Expr)) {
            return true;
        }
        match &e.kind {
            ExprKind::List(items) => items.iter().any(|x| self.is_reading(x)),
            ExprKind::Record(fields) => fields.iter().any(|(_, x)| self.is_reading(x)),
            _ => false,
        }
    }

    /// J-01：读数只经 cut 离开。这里查它被放到了要材料 / 要出口 / 要数的位置。
    fn builtin_call(&mut self, name: &str, args: &[Expr]) {
        let inside = |c: &Checker, e: &Expr| c.is_reading(e);
        match name {
            "state" | "mat" | "transform" => {
                for a in args {
                    if inside(self, a) {
                        self.reading_err(a.span, format!("读数不能放进 {name} 的槽：读数不是材料"));
                    }
                }
            }
            "content" | "text" | "len" | "sum" => {
                if args.first().map(|a| inside(self, a)).unwrap_or(false) {
                    self.reading_err(args[0].span, format!("读数不能进 {name}：读数没有可读的值"));
                }
            }
            "handle" | "consume" | "exit_kind" => {
                if args.first().map(|a| inside(self, a)).unwrap_or(false) {
                    self.reading_err(
                        args[0].span,
                        format!("{name} 的第一个参数要是出口，这里是读数"),
                    );
                }
            }
            "contains" | "append" => {
                if args.get(1).map(|a| inside(self, a)).unwrap_or(false) {
                    self.reading_err(args[1].span, "读数不可比、不可存");
                }
            }
            _ => {}
        }
    }

    fn reading_err(&mut self, span: Span, msg: impl Into<String>) {
        self.out.push(Diagnostic::error(
            "J-01",
            format!("{}。修法：cut(读数) 得到出口，再 handle 它", msg.into()),
            span,
        ));
    }
}

// ---------------------------------------------------------------- 语法面（E5 / E7 / J-03 / J-05 / J-13 / J-14）

/// 走到当前节点时所处的上下文
#[derive(Clone, Copy, Default)]
struct Ctx {
    /// 在 loop / fold / map / filter 的体内（J-13 序号）
    in_iteration: bool,
    /// 在 map / filter 的体内（E7：`for … yield` 是纯映射）
    in_yield: bool,
}

impl Checker {
    fn syntax(&mut self, p: &Program) {
        self.syntax_block(&p.body, Ctx::default(), &[]);
    }

    fn syntax_block(&mut self, b: &Block, ctx: Ctx, iter_params: &[String]) {
        for s in &b.statements {
            match s {
                Statement::Let {
                    value, name, span, ..
                } => {
                    self.syntax_expr(value, ctx, iter_params);
                    self.exit_binding(value, name, *span, b);
                }
                // 函数定义是新的词法环境：迭代上下文不穿过它
                Statement::Function { function, .. } => self.syntax_function(function),
                Statement::Expression(e) => self.syntax_expr(e, ctx, iter_params),
            }
        }
        if let Some(r) = &b.result {
            self.syntax_expr(r, ctx, iter_params);
        }
    }

    fn syntax_function(&mut self, f: &Function) {
        self.syntax_block(&f.body, Ctx::default(), &[]);
        self.exit_returned(f);
    }

    fn syntax_expr(&mut self, e: &Expr, ctx: Ctx, iter_params: &[String]) {
        if let Some(name) = call_name(e) {
            let args = call_args(e);
            let name = name.to_string();
            self.builtin_shape(&name, args, e.span, ctx, iter_params);
            // 高阶内置：函数参数的体带上迭代上下文
            let (fn_idx, yields) = match name.as_str() {
                "map" | "filter" => (Some(1usize), true),
                "fold" | "loop" => (Some(2usize), false),
                _ => (None, false),
            };
            for (i, a) in args.iter().enumerate() {
                match (&a.kind, Some(i) == fn_idx) {
                    (ExprKind::Function(f), true) => {
                        let inner = Ctx {
                            in_iteration: true,
                            in_yield: yields || ctx.in_yield,
                        };
                        let params: Vec<String> =
                            f.parameters.iter().map(|p| p.name.clone()).collect();
                        self.syntax_block(&f.body, inner, &params);
                        self.exit_returned(f);
                    }
                    _ => self.syntax_expr(a, ctx, iter_params),
                }
            }
            return;
        }
        match &e.kind {
            ExprKind::Function(f) => self.syntax_function(f),
            ExprKind::List(items) => items
                .iter()
                .for_each(|x| self.syntax_expr(x, ctx, iter_params)),
            ExprKind::Record(fields) => fields
                .iter()
                .for_each(|(_, x)| self.syntax_expr(x, ctx, iter_params)),
            ExprKind::Call {
                function,
                arguments,
            } => {
                self.syntax_expr(function, ctx, iter_params);
                arguments
                    .iter()
                    .for_each(|x| self.syntax_expr(x, ctx, iter_params));
            }
            ExprKind::Field { value, .. } => self.syntax_expr(value, ctx, iter_params),
            ExprKind::Index { value, index } => {
                self.syntax_expr(value, ctx, iter_params);
                self.syntax_expr(index, ctx, iter_params);
            }
            ExprKind::Unary { value, .. } => self.syntax_expr(value, ctx, iter_params),
            ExprKind::Binary { left, right, .. } => {
                self.syntax_expr(left, ctx, iter_params);
                self.syntax_expr(right, ctx, iter_params);
            }
            ExprKind::If { condition, yes, no } => {
                self.syntax_expr(condition, ctx, iter_params);
                self.syntax_block(yes, ctx, iter_params);
                self.syntax_block(no, ctx, iter_params);
            }
            ExprKind::Block(b) => self.syntax_block(b, ctx, iter_params),
            _ => {}
        }
    }

    fn builtin_shape(
        &mut self,
        name: &str,
        args: &[Expr],
        span: Span,
        ctx: Ctx,
        iter_params: &[String],
    ) {
        match name {
            // E5：有界循环必带 bound
            "loop" => {
                if args.len() != 3 {
                    self.out.push(Diagnostic::error(
                        "E5",
                        format!("loop 缺 bound：要写成 loop(bound, 初值, fn(acc, i))，这里给了 {} 个参数", args.len()),
                        span,
                    ));
                } else {
                    match &args[0].kind {
                        ExprKind::Integer(n) if *n > 0 => {}
                        ExprKind::Integer(n) => self.out.push(Diagnostic::error(
                            "E5",
                            format!("loop 的 bound 是 {n}：bound 必须是正整数，否则循环体一次都不跑"),
                            args[0].span,
                        )),
                        _ => self.out.push(Diagnostic::warning(
                            "W-bound",
                            "loop 的 bound 不是字面量：静态估不出上界，只有运行期能核。修法：写成整数字面量",
                            args[0].span,
                        )),
                    }
                }
                if ctx.in_yield {
                    self.out.push(Diagnostic::error(
                        "E7",
                        "map / filter（for…yield）的体内不能含 loop：它是纯映射，元素之间必须独立。修法：整段改用 loop 或 fold",
                        span,
                    ));
                }
            }
            "stop" if ctx.in_yield => self.out.push(Diagnostic::error(
                "E7",
                "map / filter（for…yield）的体内不能 stop：stop 是 loop 的控制。修法：整段改用 loop 或 fold",
                span,
            )),
            // J-13：循环里的序号必须随轮次变
            "do" if ctx.in_iteration => self.seq_const(name, args, 2, "iter_seq", span, iter_params),
            "gen" if ctx.in_iteration => self.seq_const(name, args, 3, "retry_seq", span, iter_params),
            // J-03：线不可字面；calib 位只收校准记录的键
            "cut" => self.calib_literal(name, args, 1),
            "test" | "select" => self.calib_literal(name, args, 1),
            "measure" => self.calib_literal(name, args, 2),
            // J-14：on 恰一个判断对象（关系用一对）
            "state" => {
                if let Some(ExprKind::List(items)) = args.first().map(|a| &a.kind) {
                    if items.len() > 2 {
                        self.out.push(Diagnostic::error(
                            "J-14",
                            format!(
                                "state 的 on 槽放了 {} 个对象：一题一对象（关系用一对）。修法：逐个对象建状态，或把它们放进 over 槽当候选",
                                items.len()
                            ),
                            args[0].span,
                        ));
                    }
                }
            }
            // J-05 静态面：字面臂表必须有 unsure 去向
            "handle" => {
                if let Some(a) = args.get(1) {
                    if let ExprKind::Record(fields) = &a.kind {
                        let has = |k: &str| fields.iter().any(|(n, _)| n == k);
                        if !has("unsure") && !has("otherwise") {
                            self.out.push(Diagnostic::error(
                                "J-05",
                                "handle 的臂表没有 unsure 去向：三种题的出口都可能是 unsure，缺这一臂就是静默丢弃。修法：补 unsure: fn(cause) {…}，或给 otherwise",
                                a.span,
                            ));
                        }
                    }
                }
            }
            _ => {}
        }
    }

    fn calib_literal(&mut self, name: &str, args: &[Expr], idx: usize) {
        let Some(a) = args.get(idx) else { return };
        if matches!(
            a.kind,
            ExprKind::Decimal(_) | ExprKind::Integer(_) | ExprKind::Bool(_)
        ) {
            self.out.push(Diagnostic::error(
                "J-03",
                format!("{name} 的第 {} 个参数是数字字面量：线不可字面，这一位只收校准记录的键（Text）。修法：{name}(…, \"校准键\")", idx + 1),
                a.span,
            ));
        }
    }

    fn seq_const(
        &mut self,
        name: &str,
        args: &[Expr],
        idx: usize,
        field: &str,
        span: Span,
        iter_params: &[String],
    ) {
        let Some(a) = args.get(idx) else { return };
        if !matches!(a.kind, ExprKind::Integer(_)) {
            return;
        }
        // 其它参数随轮次变时键不会碰撞，降为提示（与 Python 检查器同口径）
        let varies = args
            .iter()
            .enumerate()
            .any(|(i, x)| i != idx && iter_params.iter().any(|p| mentions(x, p)));
        if varies {
            self.out.push(Diagnostic::warning(
                "W-seq-const",
                format!("循环里的 {name} 用了常量 {field}；这一轮参数随循环变量变，键还不碰撞，但改个写法就会。修法：把轮次 i 传给 {field}"),
                span,
            ));
        } else {
            self.out.push(Diagnostic::error(
                "J-13",
                format!("循环里的 {name} 用常量 {field}：每轮的键都一样，第二轮起会被当成重放，效应不再发生。修法：把 loop 的轮次 i 传给 {field}"),
                span,
            ));
        }
    }

    // --------- J-05 静态面

    /// 出口绑定之后在本块里再没被提到 = 静默丢弃
    fn exit_binding(&mut self, value: &Expr, name: &str, span: Span, block: &Block) {
        if !matches!(call_name(value), Some("cut") | Some("unsure") | Some("ask")) {
            return;
        }
        let mut used = false;
        let mut note = |e: &Expr| {
            if let ExprKind::Name(n) = &e.kind {
                if n == name {
                    used = true;
                }
            }
        };
        for s in &block.statements {
            match s {
                Statement::Let {
                    value: v, name: n, ..
                } if n == name && std::ptr::eq(v, value) => {}
                Statement::Let { value: v, .. } => walk_expr(v, &mut note),
                Statement::Function { function, .. } => walk_block(&function.body, &mut note),
                Statement::Expression(e) => walk_expr(e, &mut note),
            }
        }
        if let Some(r) = &block.result {
            walk_expr(r, &mut note);
        }
        if !used {
            self.out.push(Diagnostic::error(
                "J-05",
                format!("出口 {name} 绑定之后再没被提到：未消费的 unsure 就是静默丢弃。修法：handle({name}, {{…, unsure: …}})，或 consume({name}, \"drop\") 显式丢并记账"),
                span,
            ));
        }
    }

    /// 函数的结果就是出口名字本身，而返回类型没提 Exit
    fn exit_returned(&mut self, f: &Function) {
        let Some(result) = &f.body.result else { return };
        let ExprKind::Name(n) = &result.kind else {
            return;
        };
        let bound = f.body.statements.iter().any(|s| match s {
            Statement::Let { name, value, .. } => {
                name == n && matches!(call_name(value), Some("cut") | Some("unsure") | Some("ask"))
            }
            _ => false,
        });
        if !bound {
            return;
        }
        if !f
            .result_type
            .as_ref()
            .map(|t| t.mentions("Exit"))
            .unwrap_or(false)
        {
            self.out.push(Diagnostic::error(
                "J-05",
                format!("函数把出口 {n} 直接带出，返回类型却没提 Exit：调用者不知道自己要消费它。修法：标注 -> Exit（或含 Exit 的类型），或在函数里 handle / consume 掉"),
                result.span,
            ));
        }
    }
}

// ---------------------------------------------------------------- 效应标注（E-effect）

impl Checker {
    fn effects(&mut self, p: &Program) {
        self.collect_functions(&p.body);
        // 不动点：⊤ 吸收
        for _ in 0..=self.functions.len() {
            let mut changed = false;
            for i in 0..self.functions.len() {
                let body = self.functions[i].body.clone();
                let next = self.infer(&body);
                if next != self.functions[i].inferred {
                    self.functions[i].inferred = next;
                    changed = true;
                }
            }
            if !changed {
                break;
            }
        }
        for i in 0..self.functions.len() {
            let (Some(declared), Some(inferred)) = (
                self.functions[i].declared.clone(),
                self.functions[i].inferred.clone(),
            ) else {
                continue;
            };
            let declared: HashSet<String> = declared.into_iter().collect();
            let name = self.functions[i].name.clone();
            let span = self.functions[i].span;
            let mut missing: Vec<String> = inferred.difference(&declared).cloned().collect();
            missing.sort();
            if !missing.is_empty() {
                let mut all: Vec<String> = declared.union(&inferred).cloned().collect();
                all.sort();
                self.out.push(Diagnostic::error(
                    "E-effect",
                    format!(
                        "{name} 的效应标注少了 {}：函数体里这些效应实际会发生，标注是调用者估预算的依据。修法：写成 !{{{}}}",
                        missing.join(", "),
                        all.join(", ")
                    ),
                    span,
                ));
            }
            let mut unused: Vec<String> = declared.difference(&inferred).cloned().collect();
            unused.sort();
            if !unused.is_empty() {
                self.out.push(Diagnostic::warning(
                    "W-effect",
                    format!(
                        "{name} 标了 {} 但函数体里看不到：多标不出错，只是预算会估高",
                        unused.join(", ")
                    ),
                    span,
                ));
            }
        }
    }

    fn collect_functions(&mut self, b: &Block) {
        for s in &b.statements {
            match s {
                Statement::Function {
                    name,
                    function,
                    span,
                } => {
                    self.push_function(name.clone(), function, *span);
                    self.collect_functions(&function.body);
                }
                Statement::Let {
                    name, value, span, ..
                } => {
                    if let ExprKind::Function(f) = &value.kind {
                        self.push_function(name.clone(), f, *span);
                    }
                    self.collect_anon(value);
                }
                Statement::Expression(e) => self.collect_anon(e),
            }
        }
        if let Some(r) = &b.result {
            self.collect_anon(r);
        }
    }

    fn push_function(&mut self, name: String, f: &Function, span: Span) {
        let id = self.functions.len();
        self.functions.push(FuncInfo {
            name: name.clone(),
            declared: f.effects.clone(),
            inferred: Some(HashSet::new()),
            span,
            body: f.body.clone(),
        });
        self.by_name.entry(name).or_default().push(id);
    }

    /// 带效应标注的匿名方法也要核（`map(xs, fn(x) !{do} { … })`）
    fn collect_anon(&mut self, e: &Expr) {
        let mut found: Vec<(Function, Span)> = vec![];
        walk_expr(e, &mut |x| {
            if let ExprKind::Function(f) = &x.kind {
                if f.effects.is_some() {
                    found.push((f.clone(), x.span));
                }
            }
        });
        for (f, span) in found {
            self.functions.push(FuncInfo {
                name: "匿名方法".into(),
                declared: f.effects.clone(),
                inferred: Some(HashSet::new()),
                span,
                body: f.body.clone(),
            });
        }
    }

    /// 函数体的实际可见效应。None = ⊤（体内有静态解析不了的被调者）
    fn infer(&self, b: &Block) -> Option<HashSet<String>> {
        let mut set = HashSet::new();
        let mut unknown = false;
        walk_block(b, &mut |e| {
            let ExprKind::Call {
                function,
                arguments,
            } = &e.kind
            else {
                return;
            };
            match &function.kind {
                ExprKind::Name(n) => {
                    if EFFECT_NAMES.contains(&n.as_str()) {
                        set.insert(n.clone());
                    } else if !is_builtin(n) {
                        match self.effect_of_name(n) {
                            // 已知函数
                            Some(Some(s)) => set.extend(s),
                            // 已知函数但 ⊤ / 名字不是已知函数（可能是参数里的方法值）
                            Some(None) | None => unknown = true,
                        }
                    }
                }
                // 被调者是参数、字段或别的表达式：静态判不了
                _ => unknown = true,
            }
            // 传到高阶操作的方法位上的具名方法，其效应也会发生。只认已知的方法位：
            // 把方法值存进列表、传给 text 之类不是调用，不该并进来。
            let mut absorb = |a: &Expr| {
                if let ExprKind::Name(n) = &a.kind {
                    match self.effect_of_name(n) {
                        Some(Some(s)) => set.extend(s),
                        Some(None) => unknown = true,
                        None => {}
                    }
                }
            };
            if let ExprKind::Name(callee) = &function.kind {
                for i in method_positions(callee) {
                    if let Some(a) = arguments.get(*i) {
                        absorb(a);
                    }
                }
                // handle 的臂可以是具名方法
                if callee == "handle" {
                    if let Some(Expr {
                        kind: ExprKind::Record(arms),
                        ..
                    }) = arguments.get(1)
                    {
                        for (_, v) in arms {
                            absorb(v);
                        }
                    }
                }
            }
        });
        if unknown { None } else { Some(set) }
    }

    /// `Some(Some(集))` = 已知函数；`Some(None)` = 已知函数但 ⊤；`None` = 不是已知函数
    fn effect_of_name(&self, n: &str) -> Option<Option<HashSet<String>>> {
        let ids = self.by_name.get(n)?;
        if ids.len() != 1 {
            return Some(None);
        }
        let f = &self.functions[ids[0]];
        match &f.declared {
            Some(d) => Some(Some(d.iter().cloned().collect())),
            None => Some(f.inferred.clone()),
        }
    }
}
