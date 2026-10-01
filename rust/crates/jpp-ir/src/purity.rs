//! 代码单元的纯性判定（B193 第 1、2、5 条；`12` §2.13 R1；步 41 C2c 附录二 A2.1）。
//!
//! 一次对闭包的调用成为代码单元，当且仅当函数体纯：不出现 `do`、`gen`、`ask`、`transform` 与内含 `ask` 的
//! `escalate`，不读世界与解释器的表（校准、计划、动作表、答案表、程序级声明、宿主输出），调用的名字都能在
//! 定义处解析到白名单内置或同样纯的闭包。`judge`、`cut`、`handle`、`consume` 等判断与责任形式**不**使函数
//! 不纯（B193 只列四种效应；T1 的过桥都在代码单元里）——它们同一趟内留下的痕迹由运行时的足迹规则处理。
//!
//! 只看 IR 与名字解析（[`EnvView`]），运行时与检查器都调它；表外的内置缺省按不纯。函数体里的局部名（形参、
//! `let`、局部 `fn`）被调用时不在这里判：局部函数字面量本身在体里、照样被遍历；从实参传进来的函数由调用方
//! 按 [`fn_args_pure`] 逐个核（运行时手里有实参）。

use crate::ir::{Block, ConsumeHow, Expr, Function, Host, Node, Stmt};
use crate::key::EffectId;
use crate::plan::{EnvView, ValueSummary};
use std::collections::{HashMap, HashSet};

/// 一次判定的状态：正在看的方法实例（递归护栏）、本次判定内已有的结论、这一段求值有没有碰到护栏。
#[derive(Default)]
struct 判定 {
    在看: HashSet<String>,
    结论: HashMap<String, bool>,
    碰到护栏: bool,
}

/// 只读实参的值内置与判断、责任形式（作为宿主调用的名字出现时）：调用它们不使函数不纯。
pub const PURE_BUILTINS: &[&str] = &[
    // 只读实参的值内置
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
    "min",
    "max",
    "abs",
    "floor",
    "split",
    "lower",
    "upper",
    "trim",
    "replace",
    "starts_with",
    "ends_with",
    "index_of",
    "chars",
    "regex_match",
    "regex_find",
    "sort",
    "sort_by",
    "parse_json",
    "to_json",
    "hash",
    "date_parse",
    "date_format",
    "date_add",
    "rand",
    "rand_int",
    "shuffle",
    "fail",
    "is_fail",
    "stop",
    "loop",
    "exit_kind",
    "unsure_cause",
    "untested",
    "taint",
    "key_of",
    // 判断与责任（同一趟内有足迹，由运行时按足迹规则重新求值）
    "state",
    "test",
    "select",
    "measure",
    "form",
    "fill",
    "judge",
    "sieve",
    "pair",
    "tally",
    "first_k",
    "iterate",
    "outcome",
    "element",
    "cut",
    "handle",
    "consume",
    "mat",
    "content",
    "unsure",
    "pending",
    "literalize",
    "refine",
    "compose",
    "allocate",
    "unsure_bound",
    "agg",
    "repeat",
    "order",
];

/// 不纯的内核构造（作为 `Construct` 节点出现时）：读拟合注册表的 `fit`。其余构造按判断与责任处理。
const IMPURE_CONSTRUCTS: &[&str] = &["fit"];

/// 函数体纯不纯（在它的捕获环境 `env` 里解析名字）。
pub fn cell_pure(f: &Function, env: &dyn EnvView) -> bool {
    let mut st = 判定::default();
    let mut w = Walker {
        st: &mut st,
        locals: vec![f.parameters.iter().map(|p| p.name.clone()).collect()],
    };
    w.block(&f.body, env)
}

/// 实参里（递归到列表与记录）的每个函数值都纯：调用方在调用时核，因为函数体里调形参、或把形参交给
/// `map` 之类时，静态上看不透它是什么。
pub fn fn_args_pure(args: &[ValueSummary]) -> bool {
    let mut st = 判定::default();
    args.iter().all(|a| value_pure(a, &mut st))
}

fn value_pure(v: &ValueSummary, st: &mut 判定) -> bool {
    match v {
        ValueSummary::Data => true,
        ValueSummary::Builtin(b) => PURE_BUILTINS.contains(&b.as_str()),
        ValueSummary::Fn(c) => {
            // 递归护栏按实例（结构哈希加捕获，C2c 复核 K2）：正在看的同一实例再次出现（递归）先按纯处理，由外层
            // 那一次的结论决定；结构相同、捕获不同的两个闭包是两个实例，各看一遍
            let id = c.instance();
            if let Some(r) = st.结论.get(&id) {
                return *r;
            }
            if st.在看.contains(&id) {
                st.碰到护栏 = true;
                return true;
            }
            st.在看.insert(id.clone());
            let 外层碰到 = std::mem::take(&mut st.碰到护栏);
            let env = c.env();
            let r = {
                let mut w = Walker {
                    st: &mut *st,
                    locals: vec![
                        c.function()
                            .parameters
                            .iter()
                            .map(|p| p.name.clone())
                            .collect(),
                    ],
                };
                w.block(&c.function().body, &*env)
            };
            st.在看.remove(&id);
            // 缓存只存不依赖假设的结论：不纯（怎么假设都不纯），或求值中没有碰到护栏的纯
            if !r || !st.碰到护栏 {
                st.结论.insert(id, r);
            }
            st.碰到护栏 |= 外层碰到;
            r
        }
        ValueSummary::Container(items) => items().iter().all(|x| value_pure(x, st)),
    }
}

struct Walker<'s> {
    st: &'s mut 判定,
    /// 作用域栈：形参、`let`、局部 `fn` 的名字
    locals: Vec<HashSet<String>>,
}

impl Walker<'_> {
    fn is_local(&self, n: &str) -> bool {
        self.locals.iter().any(|s| s.contains(n))
    }

    fn block(&mut self, b: &Block, env: &dyn EnvView) -> bool {
        self.locals.push(HashSet::new());
        let mut ok = true;
        for st in &b.statements {
            ok = ok
                && match st {
                    Stmt::Let { name, value, .. } => {
                        let r = self.expr(value, env);
                        self.locals.last_mut().unwrap().insert(name.clone());
                        r
                    }
                    Stmt::Function { name, function, .. } => {
                        self.locals.last_mut().unwrap().insert(name.clone());
                        self.function(function, env)
                    }
                    Stmt::Expr(e) => self.expr(e, env),
                };
            if !ok {
                break;
            }
        }
        ok = ok && b.result.as_ref().is_none_or(|r| self.expr(r, env));
        self.locals.pop();
        ok
    }

    fn function(&mut self, f: &Function, env: &dyn EnvView) -> bool {
        self.locals
            .push(f.parameters.iter().map(|p| p.name.clone()).collect());
        let r = self.block(&f.body, env);
        self.locals.pop();
        r
    }

    fn all(&mut self, es: &[Expr], env: &dyn EnvView) -> bool {
        es.iter().all(|e| self.expr(e, env))
    }

    /// 一个名字被调用：局部名放行（局部函数字面量照样被遍历，实参里的函数由调用方核）；自由名按环境解析。
    fn callee_name(&mut self, n: &str, env: &dyn EnvView) -> bool {
        if self.is_local(n) {
            return true;
        }
        match env.lookup(n) {
            Some(v) => value_pure(&v, self.st),
            None => false,
        }
    }

    fn expr(&mut self, e: &Expr, env: &dyn EnvView) -> bool {
        match &e.node {
            Node::State { on, opts, rest, .. } => {
                self.expr(on, env)
                    && opts.as_ref().is_none_or(|o| self.expr(o, env))
                    && self.all(rest, env)
            }
            Node::Effect { effect, inputs, .. } => {
                matches!(effect, EffectId::Judge) && inputs.iter().all(|(_, x)| self.expr(x, env))
            }
            Node::Cut { reading, rest, .. } => self.expr(reading, env) && self.all(rest, env),
            Node::Fit { .. } => false,
            Node::Loop { bound, rest, .. } => self.expr(bound, env) && self.all(rest, env),
            Node::Handle {
                exit, arms, rest, ..
            } => self.expr(exit, env) && self.expr(arms, env) && self.all(rest, env),
            Node::Consume { how, args, .. } => {
                !matches!(how, ConsumeHow::Escalate) && self.all(args, env)
            }
            Node::Construct { name, args, .. } => {
                !IMPURE_CONSTRUCTS.contains(&name.as_str()) && self.all(args, env)
            }
            Node::Host(h) => self.host(h, env),
        }
    }

    fn host(&mut self, h: &Host, env: &dyn EnvView) -> bool {
        match h {
            Host::Integer(_) | Host::Decimal(_) | Host::Bool(_) | Host::Text(_) | Host::Unit => {
                true
            }
            // 名字当值用：函数值之后可能被调用（`map(xs, f)`），按同一规则核；局部名由调用方核实参
            Host::Name(n) => {
                if self.is_local(n) {
                    return true;
                }
                match env.lookup(n) {
                    Some(v @ (ValueSummary::Fn(_) | ValueSummary::Builtin(_))) => {
                        value_pure(&v, self.st)
                    }
                    _ => true,
                }
            }
            Host::List(xs) => self.all(xs, env),
            Host::Record(fs) => fs.iter().all(|(_, x)| self.expr(x, env)),
            Host::Function(f) => self.function(f, env),
            Host::Call { callee, args, .. } => {
                let c = match &callee.node {
                    Node::Host(Host::Name(n)) => self.callee_name(n, env),
                    // 调用一个表达式的结果、字段里的方法：看不透
                    _ => false,
                };
                c && self.all(args, env)
            }
            Host::Field { value, .. } | Host::Unary { value, .. } => self.expr(value, env),
            Host::Index { value, index } => self.expr(value, env) && self.expr(index, env),
            Host::Binary { left, right, .. } => self.expr(left, env) && self.expr(right, env),
            Host::If {
                condition, yes, no, ..
            } => self.expr(condition, env) && self.block(yes, env) && self.block(no, env),
            Host::Block(b) => self.block(b, env),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plan::FnView;
    use std::rc::Rc;

    /// 测试用环境：内置名一律给内置。
    struct Builtins;
    impl EnvView for Builtins {
        fn lookup(&self, n: &str) -> Option<ValueSummary> {
            Some(ValueSummary::Builtin(n.to_string()))
        }
    }

    fn sp() -> crate::ir::Span {
        crate::ir::Span { start: 0, end: 0 }
    }
    fn ex(node: Node) -> Expr {
        Expr {
            id: crate::key::NodeId(0),
            node,
            span: sp(),
        }
    }
    fn name(n: &str) -> Expr {
        ex(Node::Host(Host::Name(n.into())))
    }
    fn call(f: &str, args: Vec<Expr>) -> Expr {
        ex(Node::Host(Host::Call {
            callee: Box::new(name(f)),
            args,
            site: None,
        }))
    }
    fn func(params: &[&str], body: Expr) -> Function {
        Function {
            id: crate::key::NodeId(0),
            parameters: params
                .iter()
                .map(|p| crate::ir::Parameter {
                    name: (*p).into(),
                    annotation: None,
                    span: sp(),
                })
                .collect(),
            result_type: None,
            effects: None,
            body: Block {
                statements: vec![],
                result: Some(Box::new(body)),
                span: sp(),
            },
            source_hash: String::new(),
        }
    }
    fn effect(e: EffectId) -> Expr {
        ex(Node::Effect {
            effect: e,
            inputs: vec![],
            site: crate::key::SiteId(0),
        })
    }

    #[test]
    fn 值内置与判断形式纯_效应与读表的不纯() {
        assert!(cell_pure(
            &func(&["x"], call("len", vec![name("x")])),
            &Builtins
        ));
        assert!(
            cell_pure(&func(&["x"], effect(EffectId::Judge)), &Builtins),
            "judge 不使函数不纯（B193）"
        );
        for e in [
            EffectId::Do,
            EffectId::Gen,
            EffectId::Ask,
            EffectId::Transform,
        ] {
            assert!(!cell_pure(&func(&[], effect(e)), &Builtins), "{e:?}");
        }
        for b in [
            "print",
            "known_answers",
            "gate_info",
            "cert",
            "action_fact",
            "unsure_source",
            "未登记的新内置",
        ] {
            assert!(
                !cell_pure(&func(&["x"], call(b, vec![name("x")])), &Builtins),
                "{b}"
            );
        }
        let esc = ex(Node::Consume {
            how: ConsumeHow::Escalate,
            args: vec![],
            site: crate::key::SiteId(0),
        });
        assert!(!cell_pure(&func(&[], esc), &Builtins), "escalate 内含 ask");
    }

    #[test]
    fn 形参调用放行_自由名解析不到不纯() {
        assert!(
            cell_pure(&func(&["g", "x"], call("g", vec![name("x")])), &Builtins),
            "形参由调用方核实参"
        );
        struct Empty;
        impl EnvView for Empty {
            fn lookup(&self, _: &str) -> Option<ValueSummary> {
                None
            }
        }
        assert!(!cell_pure(
            &func(&["x"], call("没定义", vec![name("x")])),
            &Empty
        ));
        let 调表达式 = ex(Node::Host(Host::Call {
            callee: Box::new(ex(Node::Host(Host::Field {
                value: Box::new(name("x")),
                field: "f".into(),
            }))),
            args: vec![],
            site: None,
        }));
        assert!(
            !cell_pure(&func(&["x"], 调表达式), &Builtins),
            "字段里的方法看不透"
        );
    }

    /// 测试用方法值：函数、捕获环境、实例名（结构哈希一律取 "f"：不同实例结构哈希相同）。
    struct F(Function, Rc<dyn EnvView>, &'static str);
    impl FnView for F {
        fn function(&self) -> &Function {
            &self.0
        }
        fn identity(&self) -> &str {
            "f"
        }
        fn env(&self) -> Rc<dyn EnvView> {
            self.1.clone()
        }
        fn instance(&self) -> String {
            self.2.to_string()
        }
    }

    #[test]
    fn 实参里的函数逐个核() {
        let 纯 = ValueSummary::Fn(Rc::new(F(
            func(&["x"], call("len", vec![name("x")])),
            Rc::new(Builtins),
            "纯",
        )));
        let 脏 = ValueSummary::Fn(Rc::new(F(
            func(&[], effect(EffectId::Do)),
            Rc::new(Builtins),
            "脏",
        )));
        assert!(fn_args_pure(&[ValueSummary::Data, 纯.clone()]));
        assert!(!fn_args_pure(&[纯, 脏.clone()]));
        let 脏c = 脏.clone();
        assert!(
            !fn_args_pure(&[ValueSummary::Container(Rc::new(move || vec![脏c.clone()]))]),
            "列表里的也核"
        );
    }

    /// 捕获环境：名字 `g` 绑一个方法值，其余按内置。
    struct 捕获(ValueSummary);
    impl EnvView for 捕获 {
        fn lookup(&self, n: &str) -> Option<ValueSummary> {
            Some(if n == "g" {
                self.0.clone()
            } else {
                ValueSummary::Builtin(n.to_string())
            })
        }
    }

    /// C2c 复核 K2：`mk(g) = fn(x) { g(x) }`；`f2 = mk(bad)`、`f1 = mk(f2)`，`bad` 调 `print`。f1 与 f2 结构相同、
    /// 捕获不同：护栏只按结构哈希时，看 f1 会把 f2 当成「正在看」而判纯；按实例才判对。
    #[test]
    fn k2_结构同捕获不同的闭包各看一遍() {
        let 内层 = func(&["x"], call("g", vec![name("x")]));
        let bad = ValueSummary::Fn(Rc::new(F(
            func(&["x"], call("print", vec![name("x")])),
            Rc::new(Builtins),
            "bad",
        )));
        let f2 = ValueSummary::Fn(Rc::new(F(内层.clone(), Rc::new(捕获(bad)), "f2")));
        let f1 = ValueSummary::Fn(Rc::new(F(内层.clone(), Rc::new(捕获(f2.clone())), "f1")));
        assert!(!fn_args_pure(&[f1]), "f1 经 f2 调到 print");
        assert!(!cell_pure(&内层, &捕获(f2)), "在 f1 的捕获环境里判函数体");
        // 对照：链尾换成纯函数，整条链纯
        let ok = ValueSummary::Fn(Rc::new(F(
            func(&["x"], call("len", vec![name("x")])),
            Rc::new(Builtins),
            "ok",
        )));
        let g2 = ValueSummary::Fn(Rc::new(F(内层.clone(), Rc::new(捕获(ok)), "g2")));
        let g1 = ValueSummary::Fn(Rc::new(F(内层, Rc::new(捕获(g2)), "g1")));
        assert!(fn_args_pure(&[g1]));
    }

    /// 递归：同一实例经自己的捕获再次出现，先按纯处理，由外层的结论决定（体里有 print 就不纯）。
    #[test]
    fn 递归实例按外层结论() {
        struct 自指(std::cell::RefCell<Option<ValueSummary>>);
        impl EnvView for 自指 {
            fn lookup(&self, n: &str) -> Option<ValueSummary> {
                Some(if n == "g" {
                    self.0.borrow().clone().unwrap()
                } else {
                    ValueSummary::Builtin(n.to_string())
                })
            }
        }
        for (内置, 纯) in [("len", true), ("print", false)] {
            let env = Rc::new(自指(std::cell::RefCell::new(None)));
            let body = ex(Node::Host(Host::Block(Block {
                statements: vec![Stmt::Expr(call("g", vec![name("x")]))],
                result: Some(Box::new(call(内置, vec![name("x")]))),
                span: sp(),
            })));
            let f = ValueSummary::Fn(Rc::new(F(func(&["x"], body), env.clone(), "rec")));
            *env.0.borrow_mut() = Some(f.clone());
            assert_eq!(fn_args_pure(&[f]), 纯, "{内置}");
        }
    }
}
