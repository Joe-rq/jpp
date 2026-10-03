//! 结构化站点标识（B0630，裁定三十七；预注册 `地基/规划/B0630-键与格式稳定-预注册.md` §2.1）。
//!
//! 判断键与效应键里的站点从源码字节偏移换成 `<定义路径>:<标签>#<序号>`：改注释、空行、别的定义、定义在文件里
//! 挪位置都不变；只有同一定义里、同一标签的站点前面插入同标签站点，后面的序号才变。
//!
//! - 定义路径：顶层 `fn 名` / `let 名 = …` 开一段 `名`；函数体内的 `fn 名` 与值直接是函数字面量的 `let 名 = fn(…)`
//!   开一段 `名`；同一父段里同名的第 k 次写 `名~k`；其余函数字面量开一段 `λk`（直接外层定义里第几个匿名函数）；
//!   入口顶层的表达式语句与结果、`budget` 归 `<main>`。文件路径不进键（loader 已保证顶层名全程序唯一）。
//! - 标签：效应名；被调者是名字的调用取该名字，否则 `call`；`consume`/`escalate`/`literalize`；构造名；`cut`、`fit`、
//!   `handle`、`loop`、`state`、`if`。
//! - 序号：同一定义（不含嵌套定义）里同标签节点按前序遍历的第几个，从 1 起。
//!
//! 表按完整 Span（起、止）查：运行时进键的 span 来自效应节点与内置调用点（`sieve`、`unsure_default`、默认链、
//! 裂变、`cut` 上的声明线），`f(x)(y)` 这类起点相同的嵌套调用靠止点区分；同一 Span 有多个节点时取前序第一个。
//! 纯函数，只读 IR 与宿主交来的 lib 区间，放 L0。

use crate::ir::{Block, Expr, Host, Node, Program, Stmt};
use std::collections::HashMap;

/// 一个站点的结构化标识与它是否在标准库定义里。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SiteKeyEntry {
    pub key: String,
    /// 所在顶层定义来自 lib 文件（`lib/` 之下、不在 `lib/bank/` 下，与 `versions_of` 同一判据）
    pub lib: bool,
}

/// 站点表：完整 Span → 结构化标识。
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SiteKeys {
    by_span: HashMap<(usize, usize), SiteKeyEntry>,
}

impl SiteKeys {
    pub fn get(&self, start: usize, end: usize) -> Option<&SiteKeyEntry> {
        self.by_span.get(&(start, end))
    }
    pub fn is_empty(&self) -> bool {
        self.by_span.is_empty()
    }
    pub fn len(&self) -> usize {
        self.by_span.len()
    }
    /// 全部条目（Span 起、止，标识），次序不定。
    pub fn iter(&self) -> impl Iterator<Item = ((usize, usize), &SiteKeyEntry)> {
        self.by_span.iter().map(|(k, v)| (*k, v))
    }
}

/// 按 IR 建站点表。`lib_ranges`：lib 文件在全程序源码里的字节区间 `[起, 止)`；顶层定义的 Span 起点落在其中即算 lib。
pub fn site_keys(p: &Program, lib_ranges: &[(usize, usize)]) -> SiteKeys {
    let mut w = Walker {
        out: SiteKeys::default(),
        lib: false,
    };
    let in_lib = |start: usize| lib_ranges.iter().any(|(a, b)| start >= *a && start < *b);
    let mut top = Scope::new("<main>".into());
    let mut names: HashMap<String, u32> = HashMap::new();
    for s in &p.body.statements {
        match s {
            Stmt::Let {
                name, value, span, ..
            } => {
                w.lib = in_lib(span.start);
                let mut sc = Scope::new(segment(&mut names, name));
                match &value.node {
                    Node::Host(Host::Function(f)) => w.block(&f.body, &mut sc),
                    _ => w.expr(value, &mut sc),
                }
            }
            Stmt::Function {
                name,
                function,
                span,
            } => {
                w.lib = in_lib(span.start);
                let mut sc = Scope::new(segment(&mut names, name));
                w.block(&function.body, &mut sc);
            }
            Stmt::Expr(e) => {
                w.lib = in_lib(e.span.start);
                w.expr(e, &mut top);
            }
        }
    }
    if let Some(r) = &p.body.result {
        w.lib = in_lib(r.span.start);
        w.expr(r, &mut top);
    }
    w.out
}

/// 同一父段里第 k 次出现的名字写 `名~k`（k≥2）。
fn segment(names: &mut HashMap<String, u32>, name: &str) -> String {
    let n = names.entry(name.to_string()).or_insert(0);
    *n += 1;
    if *n == 1 {
        name.to_string()
    } else {
        format!("{name}~{n}")
    }
}

/// 一个定义：路径、各标签计数、子段名计数、匿名函数计数。
struct Scope {
    path: String,
    labels: HashMap<String, u32>,
    names: HashMap<String, u32>,
    lambdas: u32,
}

impl Scope {
    fn new(path: String) -> Scope {
        Scope {
            path,
            labels: HashMap::new(),
            names: HashMap::new(),
            lambdas: 0,
        }
    }
    fn child(&mut self, seg: &str) -> Scope {
        Scope::new(format!("{}/{seg}", self.path))
    }
}

struct Walker {
    out: SiteKeys,
    lib: bool,
}

impl Walker {
    fn note(&mut self, e: &Expr, label: &str, sc: &mut Scope) {
        let n = sc.labels.entry(label.to_string()).or_insert(0);
        *n += 1;
        let key = format!("{}:{label}#{n}", sc.path);
        self.out
            .by_span
            .entry((e.span.start, e.span.end))
            .or_insert(SiteKeyEntry { key, lib: self.lib });
    }

    fn block(&mut self, b: &Block, sc: &mut Scope) {
        for s in &b.statements {
            match s {
                Stmt::Let { name, value, .. } => match &value.node {
                    Node::Host(Host::Function(f)) => {
                        let seg = segment(&mut sc.names, name);
                        let mut c = sc.child(&seg);
                        self.block(&f.body, &mut c);
                    }
                    _ => self.expr(value, sc),
                },
                Stmt::Function { name, function, .. } => {
                    let seg = segment(&mut sc.names, name);
                    let mut c = sc.child(&seg);
                    self.block(&function.body, &mut c);
                }
                Stmt::Expr(e) => self.expr(e, sc),
            }
        }
        if let Some(r) = &b.result {
            self.expr(r, sc);
        }
    }

    fn exprs(&mut self, xs: &[Expr], sc: &mut Scope) {
        for x in xs {
            self.expr(x, sc);
        }
    }

    fn expr(&mut self, e: &Expr, sc: &mut Scope) {
        match &e.node {
            Node::State { on, opts, rest, .. } => {
                self.note(e, "state", sc);
                self.expr(on, sc);
                if let Some(o) = opts {
                    self.expr(o, sc);
                }
                self.exprs(rest, sc);
            }
            Node::Effect { effect, inputs, .. } => {
                // 效应名取 serde 名（与 `ir::print` 同口径；A2：不在这里按变体名分支）
                let name = serde_json::to_value(effect)
                    .ok()
                    .and_then(|v| v.as_str().map(String::from))
                    .unwrap_or_default();
                self.note(e, &name, sc);
                for (_, x) in inputs {
                    self.expr(x, sc);
                }
            }
            Node::Cut { reading, rest, .. } => {
                self.note(e, "cut", sc);
                self.expr(reading, sc);
                self.exprs(rest, sc);
            }
            Node::Fit { args, .. } => {
                self.note(e, "fit", sc);
                self.exprs(args, sc);
            }
            Node::Loop { bound, rest, .. } => {
                self.note(e, "loop", sc);
                self.expr(bound, sc);
                self.exprs(rest, sc);
            }
            Node::Handle {
                exit, arms, rest, ..
            } => {
                self.note(e, "handle", sc);
                self.expr(exit, sc);
                self.expr(arms, sc);
                self.exprs(rest, sc);
            }
            Node::Consume { how, args, .. } => {
                self.note(e, how.name(), sc);
                self.exprs(args, sc);
            }
            Node::Construct { name, args, .. } => {
                self.note(e, name, sc);
                self.exprs(args, sc);
            }
            Node::Host(h) => self.host(e, h, sc),
        }
    }

    fn host(&mut self, e: &Expr, h: &Host, sc: &mut Scope) {
        match h {
            Host::Integer(_)
            | Host::Decimal(_)
            | Host::Bool(_)
            | Host::Text(_)
            | Host::Unit
            | Host::Name(_) => {}
            Host::List(xs) => self.exprs(xs, sc),
            Host::Record(fs) => {
                for (_, x) in fs {
                    self.expr(x, sc);
                }
            }
            Host::Function(f) => {
                sc.lambdas += 1;
                let seg = format!("λ{}", sc.lambdas);
                let mut c = sc.child(&seg);
                self.block(&f.body, &mut c);
            }
            Host::Call { callee, args, .. } => {
                let label = match &callee.node {
                    Node::Host(Host::Name(n)) => n.as_str(),
                    _ => "call",
                };
                self.note(e, label, sc);
                self.expr(callee, sc);
                self.exprs(args, sc);
            }
            Host::Field { value, .. } => self.expr(value, sc),
            Host::Index { value, index } => {
                self.expr(value, sc);
                self.expr(index, sc);
            }
            Host::Unary { value, .. } => self.expr(value, sc),
            Host::Binary { left, right, .. } => {
                self.expr(left, sc);
                self.expr(right, sc);
            }
            Host::If {
                condition, yes, no, ..
            } => {
                self.note(e, "if", sc);
                self.expr(condition, sc);
                self.block(yes, sc);
                self.block(no, sc);
            }
            Host::Block(b) => self.block(b, sc),
        }
    }
}
