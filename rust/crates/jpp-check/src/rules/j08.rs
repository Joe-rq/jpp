//! J-08 的静态子面「守卫全不可信」（B108，`21` 步 24-0）：包住不可逆 `do` 的每一层守卫都**确定**
//! 来自不可信状态上的判断、且路径上没有 `ask` 时，检查期就报，不等运行期花完调用再拦。
//!
//! 判据是运行期 J-08（`jpp-runtime` 的值级守卫证据 `GuardEv`：`handle` 分派产证据、`if` 与臂压守卫栈、
//! `guard.rs::release` 放行核；B121）的
//! 子集：这里报出的 `do`，只要走到，运行期必拒。做法是**只认确定的**：
//! - `do` 从顶层到它只穿过直接写在 `handle` 臂记录里的 `fn` 字面量，不穿过别的函数边界（运行期
//!   守卫栈跨帧，调用者外层的可信守卫同样放行，函数体里的 `do` 看不全守卫栈）；
//! - 守卫栈至少一层，**每一层**都确定不可信（运行期任一层可信或经 `ask` 即放行）；
//! - `if` 守卫按 `&&` 拆合取项，每项都是绑到「确定不可信的出口 / 守卫布尔」的名字；
//!   `handle` 臂守卫的出口确定不可信；
//! - 其余一切（经函数调用、容器、闭包、构造、`gen`、比较式……）按可信计、不报（零假拒绝），
//!   运行期面兜底。
//!
//! 等级：拿到宿主动作表且动作名是字面量、登记为不可逆时报 `J-08`（error）；可逆不报；没有动作表时
//! 报 `W-guard-untrusted`（warn）。
//!
//! 意图汇编 11a（2026-09-26）起整条规则只在宿主开放行把关（`Program.entry.guard`，CLI `--guard`）时运行。
//!
//! 依据：B108（地基/附注/2026-09-25-批量裁定.md §二）；`12` §3 J-08 行静态子面；B33 第 3、4、8 条与
//! B105 补（报文按来源分情形）；过程记录 `地基/过程记录/工程-步24-0.md`。
//!
//! 步 20j-2（B128）加一种根：`cut` 的实参里有**字面记录且带 `declare` 字段**（作者声明线，含 20j-3 的 `stat` 线），
//! 而宿主没有接受作者线放行（`Program.entry.accept_declared` 为假，CLI `--release-on-declared`）——这个出口的
//! `releases()` 恒假，运行期不作可信合取项。读数确定不可信时仍记不可信根（开关救不了材料）。传播与上面同一套；
//! 声明记录不是字面量的一律按可放行计。过程记录 `地基/过程记录/工程-步20j-2.md`。
//!
//! 2026-09-26（B128 补齐）：`sieve(…, {line: {declare: …}})` 的选项是字面记录时，契约值记同一种声明线根（材料
//! 确定不可信时记材料根），沿 `o.value` → `o.value[i]` → `.exit` 传到出口；经函数、闭包、非字面选项一律看不透。
//! 没写字面声明线的 `sieve` 不进这条路。过程记录 `地基/过程记录/工程-sieve声明线.md`。

use std::collections::HashMap;

use super::{Cx, Hooks, Rule};
use crate::*;
use jpp_effects::kinds::spec;
use jpp_effects::spec::{ProfileSchema, SlotKind, TaintRule};
use jpp_ir::ir::{EntryKind, EntryTaint, Host, Node};

/// 依据：B108（J-08 静态子面「守卫全不可信」）。
pub(crate) const RULE: Rule = Rule {
    code: "J-08",
    requires: &[],
    hooks: Hooks {
        after: Some(after),
        ..Hooks::NONE
    },
};

/// 守卫布尔里允许出现的宿主内置调用（不产出口、不问人）。
const GUARD_CALLS: &[&str] = &["content", "text", "mat", "len"];

/// 只造题值的构造（降级为 `Node::Construct`）：不产出口、不问人，守卫布尔与不带判断的值里都允许。
const QUESTION_CONSTRUCTS: &[&str] = &["test", "select", "measure", "form", "fill"];

/// 不可信的根：宿主入口（值条目 / 材料条目）或登记为输出不可信的动作。
#[derive(Clone, Debug, PartialEq)]
enum Root {
    EntryValue(String),
    EntryMat(String),
    Action(String),
    /// 宿主未接受的作者声明线（B128，步 20j-2）：站点与字面线，如 `@812 hi=0.7 lo=0.3`
    Declared(String),
}

/// 一条确定不可信的来源。`direct`：材料就是根本身（入口材料条目、动作输出），没有经过读出再造。
#[derive(Clone, Debug, PartialEq)]
struct Src {
    root: Root,
    direct: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Class {
    Data,
    State,
    Reading,
    Exit,
    Guard,
    /// `sieve(…, {line: {declare: …}})` 的契约值（B128 补齐）：`.value` 是元素列表
    Outcome,
    /// 契约值的 `.value`：下标取元素
    Elems,
    /// 一个元素：`.exit` 是出口
    Elem,
}

#[derive(Clone, Debug)]
enum Binding {
    /// 确定不可信的值
    Known(Class, Src),
    /// 不带判断的值（字面量、题、数据、`handle` 臂的形参）
    Plain,
    /// 看不透：可能带判断，来源不定
    Opaque,
}

fn after(cx: &Cx) -> Vec<Diagnostic> {
    // 意图汇编 11a（2026-09-26）：放行把关是宿主可选开启的工具（`Program.entry.guard`，CLI `--guard`），
    // 默认不开，静态子面（`J-08` 与 `W-guard-untrusted`）一条都不报
    if !cx.p.entry.guard {
        return vec![];
    }
    let mut counts: HashMap<String, usize> = HashMap::new();
    count_block(&cx.p.body, &mut counts);
    let mut w = W {
        cx,
        counts,
        scopes: vec![],
        guards: vec![],
        out: vec![],
    };
    w.block(&cx.p.body);
    w.out
}

// ---------------------------------------------------------------- 绑定计数

/// 全程序每个名字被绑定的次数（`let`、函数名、形参）。作用域里查不到而别处绑定过的名字看不透；
/// 程序里绑定过的入口名不再当入口（遮蔽）。
fn count_block(b: &Block, c: &mut HashMap<String, usize>) {
    for s in &b.statements {
        match s {
            Statement::Let { name, value, .. } => {
                *c.entry(name.clone()).or_default() += 1;
                count_expr(value, c);
            }
            Statement::Function { name, function, .. } => {
                *c.entry(name.clone()).or_default() += 1;
                count_function(function, c);
            }
            Statement::Expr(e) => count_expr(e, c),
        }
    }
    if let Some(r) = &b.result {
        count_expr(r, c);
    }
}

fn count_function(f: &Function, c: &mut HashMap<String, usize>) {
    for p in &f.parameters {
        *c.entry(p.name.clone()).or_default() += 1;
    }
    count_block(&f.body, c);
}

fn count_expr(e: &Expr, c: &mut HashMap<String, usize>) {
    match &e.node {
        Node::Host(Host::Function(f)) => count_function(f, c),
        Node::Host(Host::If {
            condition, yes, no, ..
        }) => {
            count_expr(condition, c);
            count_block(yes, c);
            count_block(no, c);
        }
        Node::Host(Host::Block(b)) => count_block(b, c),
        _ => e.children().into_iter().for_each(|x| count_expr(x, c)),
    }
}

// ---------------------------------------------------------------- 遍历

struct W<'a, 'c> {
    cx: &'c Cx<'a>,
    counts: HashMap<String, usize>,
    scopes: Vec<HashMap<String, Binding>>,
    /// 包住当前位置的守卫：`Some` = 确定不可信
    guards: Vec<Option<Src>>,
    out: Vec<Diagnostic>,
}

impl W<'_, '_> {
    fn bound_elsewhere(&self, n: &str) -> bool {
        self.counts.get(n).copied().unwrap_or(0) > 0
    }

    /// 词法作用域先查（走过的每个块与 `handle` 臂都压了作用域，找到的就是最近的那个绑定）；
    /// 查不到而程序别处绑定过（函数体里、没走过的块里）的名字看不透。
    fn lookup(&self, n: &str) -> Binding {
        for s in self.scopes.iter().rev() {
            if let Some(b) = s.get(n) {
                return b.clone();
            }
        }
        if self.bound_elsewhere(n) {
            return Binding::Opaque;
        }
        // 没被程序绑定过的名字：入口条目，或内置 / 全局名
        match self.cx.p.entry.params.iter().find(|p| p.name == n) {
            Some(p) if p.taint == EntryTaint::Untrusted => {
                let (root, direct) = match p.kind {
                    EntryKind::Value => (Root::EntryValue(n.to_string()), false),
                    EntryKind::Mat => (Root::EntryMat(n.to_string()), true),
                };
                Binding::Known(Class::Data, Src { root, direct })
            }
            _ => Binding::Plain,
        }
    }

    fn define(&mut self, n: &str, b: Binding) {
        if let Some(s) = self.scopes.last_mut() {
            s.insert(n.to_string(), b);
        }
    }

    fn block(&mut self, b: &Block) {
        self.scopes.push(HashMap::new());
        for s in &b.statements {
            match s {
                Statement::Let { name, value, .. } => {
                    self.expr(value);
                    let b = self.classify_let(value);
                    self.define(name, b);
                }
                // 函数边界：体内的 `do` 不在本子面管辖内；名字看不透
                Statement::Function { name, .. } => self.define(name, Binding::Opaque),
                Statement::Expr(e) => self.expr(e),
            }
        }
        if let Some(r) = &b.result {
            self.expr(r);
        }
        self.scopes.pop();
    }

    fn expr(&mut self, e: &Expr) {
        match &e.node {
            // 函数边界（`handle` 臂除外，见下）
            Node::Host(Host::Function(_)) => {}
            Node::Host(Host::If {
                condition, yes, no, ..
            }) => {
                self.expr(condition);
                let g = self.if_guard(condition);
                self.guards.push(g);
                self.block(yes);
                self.block(no);
                self.guards.pop();
            }
            Node::Host(Host::Block(b)) => self.block(b),
            Node::Handle {
                exit, arms, rest, ..
            } => {
                self.expr(exit);
                rest.iter().for_each(|x| self.expr(x));
                let g = match self.val(exit) {
                    Some((Class::Exit, s)) => Some(s),
                    _ => None,
                };
                match &arms.node {
                    Node::Host(Host::Record(fields)) => {
                        for (_, f) in fields {
                            match &f.node {
                                // 运行期：这一臂执行前压入该出口作守卫（`duty.rs`）
                                Node::Host(Host::Function(func)) => {
                                    self.guards.push(g.clone());
                                    self.arm_body(func);
                                    self.guards.pop();
                                }
                                _ => self.expr(f),
                            }
                        }
                    }
                    _ => self.expr(arms),
                }
            }
            Node::Effect { effect, inputs, .. } => {
                inputs.iter().for_each(|(_, x)| self.expr(x));
                if spec(*effect).profile_schema == ProfileSchema::Action {
                    self.check_do(e, inputs);
                }
            }
            _ => e.children().into_iter().for_each(|x| self.expr(x)),
        }
    }

    fn arm_body(&mut self, f: &Function) {
        let mut s = HashMap::new();
        for p in &f.parameters {
            s.insert(p.name.clone(), Binding::Plain);
        }
        self.scopes.push(s);
        self.block(&f.body);
        self.scopes.pop();
    }

    // ------------------------------------------------------------ 值的分类

    /// 表达式是否确定不可信，以及它是哪一类值。
    fn val(&self, e: &Expr) -> Option<(Class, Src)> {
        match &e.node {
            Node::Host(Host::Name(n)) => match self.lookup(n) {
                Binding::Known(c, s) => Some((c, s)),
                _ => None,
            },
            // `taint`、`hash` 返回可信文本（材料自带的位与哈希），不传
            Node::Host(Host::Field { value, field }) => {
                if field == "taint" || field == "hash" {
                    return None;
                }
                // `sieve` 声明线的契约值（B128 补齐）：`o.value` → 元素列表，元素的 `.exit` → 出口
                match (self.val(value), field.as_str()) {
                    (Some((Class::Outcome, s)), "value") => Some((Class::Elems, s)),
                    (Some((Class::Elem, s)), "exit") => Some((Class::Exit, s)),
                    (Some((Class::Data, s)), _) => Some((Class::Data, Src { direct: false, ..s })),
                    _ => None,
                }
            }
            Node::Host(Host::Index { value, .. }) => match self.val(value) {
                Some((Class::Elems, s)) => Some((Class::Elem, s)),
                Some((Class::Data, s)) => Some((Class::Data, Src { direct: false, ..s })),
                _ => None,
            },
            // `sieve` 的字面声明线（B128 补齐，过程记录 工程-sieve声明线.md）：宿主没接受时，这个契约值里选出的出口
            // 确定不作放行证据；材料确定不可信时根记材料（与 `cut` 同一优先）。没写字面声明线的 `sieve` 不进这一支
            Node::Construct { name, args, .. } if name == "sieve" => {
                let 线 = sieve_字面声明(args)?;
                // `sieve` 每个元素一个状态（不同于 `state([…])` 把列表并成一个状态）：列表字面量要**每个**元素都确定
                // 不可信才记材料根，否则可信的那个元素在宿主接受时照样放行，按材料根报就是假拒绝
                let 材料 = args.first().and_then(|m| match &m.node {
                    Node::Host(Host::List(xs)) => {
                        let srcs: Vec<Src> = xs.iter().filter_map(|x| self.data(x)).collect();
                        (!xs.is_empty() && srcs.len() == xs.len())
                            .then(|| srcs.into_iter().next())
                            .flatten()
                    }
                    _ => self.data(m),
                });
                if let Some(s) = 材料 {
                    return Some((Class::Outcome, s));
                }
                if self.cx.p.entry.accept_declared {
                    return None;
                }
                Some((
                    Class::Outcome,
                    Src {
                        root: Root::Declared(format!("@{} {线}", e.span.start)),
                        direct: false,
                    },
                ))
            }
            Node::Host(Host::Call {
                callee,
                args,
                site: None,
            }) => {
                let n = builtin_callee(self, callee)?;
                let a = self.data(args.first()?)?;
                match n {
                    "content" | "text" => Some((Class::Data, Src { direct: false, ..a })),
                    "mat" => Some((Class::Data, a)),
                    _ => None,
                }
            }
            Node::Effect { effect, inputs, .. } => {
                let sp = spec(*effect);
                if sp.profile_schema == ProfileSchema::Action {
                    let name = action_name(inputs)?;
                    let facts = self.cx.actions?.actions.get(name)?;
                    if facts.output_untrusted {
                        return Some((
                            Class::Data,
                            Src {
                                root: Root::Action(name.to_string()),
                                direct: true,
                            },
                        ));
                    }
                    return None;
                }
                if sp.produces_reading {
                    let pos = sp
                        .input_schema
                        .iter()
                        .position(|d| d.kind == SlotKind::State)?;
                    let (_, s) = inputs.get(pos)?;
                    return match self.val(s)? {
                        (Class::State, src) => Some((Class::Reading, src)),
                        _ => None,
                    };
                }
                // 步 24c 补齐「暂不认的根」（24-0 遗留）：`taint_rule == Inherit` 且不产出读数的
                // 效应（现行注册表里只有 `gen`，不按名字写死）——输出 taint = ∨ ctx.槽，
                // ctx 里有确定不可信的材料，输出就确定不可信。不按效应名分支（`grep_effect_names`
                // 纪律，20 A2），按 `taint_rule`/`produces_reading` 两个字段泛化匹配。
                if sp.taint_rule == TaintRule::Inherit && !sp.produces_reading {
                    let pos = sp.input_schema.iter().position(|d| d.name == "ctx")?;
                    let (_, ctx) = inputs.get(pos)?;
                    return self
                        .data_or_list(ctx)
                        .map(|s| (Class::Data, Src { direct: false, ..s }));
                }
                None
            }
            Node::State { on, opts, .. } => {
                let mut found = self.data_or_list(on);
                if found.is_none()
                    && let Some(o) = opts
                    && let Node::Host(Host::Record(fields)) = &o.node
                {
                    found = fields
                        .iter()
                        .filter(|(k, _)| k == "ctx" || k == "ref" || k == "over")
                        .find_map(|(_, x)| self.data_or_list(x));
                }
                found.map(|s| (Class::State, s))
            }
            // 声明式拟合（B153 (3)，步 20j-4）：`Score` 的来源穿过它取各输入读数的状态——输入里有确定不可信的读数，
            // 结果就按带该来源的读数计；否则看不透（之后的 `cut` 按声明线根处理）
            Node::Fit { args, .. } => {
                let 声明式 = matches!(
                    args.first().map(|a| &a.node),
                    Some(Node::Host(Host::Record(fs))) if fs.iter().any(|(k, _)| k == "declare")
                );
                let Some(Node::Host(Host::List(xs))) = args.get(1).map(|a| &a.node) else {
                    return None;
                };
                if !声明式 {
                    return None;
                }
                xs.iter().find_map(|x| match self.val(x) {
                    Some((Class::Reading, s)) => Some((Class::Reading, s)),
                    _ => None,
                })
            }
            // `literalize(u, state, 题, {line: {declare: …}})` 的重问出口（B128 补齐，过程记录 工程-sieve声明线.md §四）：
            // 字面声明线、宿主未接受时记声明线根；状态确定不可信时记材料根。没写字面声明线不进这一支
            Node::Consume {
                how: jpp_ir::ir::ConsumeHow::Literalize,
                args,
                ..
            } if args.len() == 4 => {
                let 线 = sieve_字面声明(args)?;
                if let Some((Class::State, s)) = args.get(1).and_then(|x| self.val(x)) {
                    return Some((Class::Exit, s));
                }
                if self.cx.p.entry.accept_declared {
                    return None;
                }
                Some((
                    Class::Exit,
                    Src {
                        root: Root::Declared(format!("@{} {线}", e.span.start)),
                        direct: false,
                    },
                ))
            }
            Node::Cut { reading, rest, .. } => match self.val(reading) {
                Some((Class::Reading, s)) => Some((Class::Exit, s)),
                // 依据：B128（宿主未接受的作者声明线不放行；步 20j-2）
                _ if !self.cx.p.entry.accept_declared => 字面声明(rest).map(|线| {
                    (
                        Class::Exit,
                        Src {
                            root: Root::Declared(format!("@{} {线}", e.span.start)),
                            direct: false,
                        },
                    )
                }),
                _ => None,
            },
            _ => None,
        }
    }

    fn data(&self, e: &Expr) -> Option<Src> {
        match self.val(e)? {
            (Class::Data, s) => Some(s),
            _ => None,
        }
    }

    /// 状态槽：一份材料，或列表字面量里任一份（状态 taint = 各材料之 ∨）
    fn data_or_list(&self, e: &Expr) -> Option<Src> {
        match &e.node {
            Node::Host(Host::List(xs)) => xs.iter().find_map(|x| self.data(x)),
            _ => self.data(e),
        }
    }

    // ------------------------------------------------------------ let 与守卫

    fn classify_let(&mut self, value: &Expr) -> Binding {
        if let Some((c, s)) = self.val(value) {
            return Binding::Known(c, s);
        }
        let mut srcs = vec![];
        if self.scan_guard(value, &mut srcs)
            && let Some(s) = srcs.into_iter().next()
        {
            return Binding::Known(Class::Guard, s);
        }
        if self.plain(value) {
            Binding::Plain
        } else {
            Binding::Opaque
        }
    }

    /// 不带判断的值：字面量、容器、取字段与下标、运算，以及实参都不带判断的一阶内置调用与状态。
    fn plain(&self, e: &Expr) -> bool {
        match &e.node {
            Node::Host(h) => match h {
                Host::Integer(_)
                | Host::Decimal(_)
                | Host::Bool(_)
                | Host::Text(_)
                | Host::Unit => true,
                Host::Name(n) => matches!(
                    self.lookup(n),
                    Binding::Plain | Binding::Known(Class::Data, _)
                ),
                Host::List(_)
                | Host::Record(_)
                | Host::Field { .. }
                | Host::Index { .. }
                | Host::Unary { .. }
                | Host::Binary { .. } => e.children().into_iter().all(|x| self.plain(x)),
                Host::Call {
                    callee,
                    args,
                    site: None,
                } => builtin_callee(self, callee).is_some() && args.iter().all(|x| self.plain(x)),
                _ => false,
            },
            Node::State { .. } => e.children().into_iter().all(|x| self.plain(x)),
            Node::Construct { name, args, .. } if QUESTION_CONSTRUCTS.contains(&name.as_str()) => {
                args.iter().all(|x| self.plain(x))
            }
            _ => false,
        }
    }

    /// 守卫布尔：值里只有白名单节点，没有 `ask`、构造、用户函数；收集判断来源（`cut` 与引用的出口 /
    /// 守卫布尔）。返回 `false` 表示看不透。
    fn scan_guard(&mut self, e: &Expr, srcs: &mut Vec<Src>) -> bool {
        // `sieve` 声明线契约值里取出的出口（`o.value[i].exit`，B128 补齐）是判断来源
        if matches!(e.node, Node::Host(Host::Field { .. }))
            && let Some((Class::Exit, s)) = self.val(e)
        {
            srcs.push(s);
            return true;
        }
        match &e.node {
            Node::Host(h) => match h {
                Host::Integer(_)
                | Host::Decimal(_)
                | Host::Bool(_)
                | Host::Text(_)
                | Host::Unit => true,
                Host::Name(n) => match self.lookup(n) {
                    Binding::Known(Class::Exit | Class::Guard | Class::Reading, s) => {
                        srcs.push(s);
                        true
                    }
                    Binding::Known(_, _) | Binding::Plain => true,
                    Binding::Opaque => false,
                },
                Host::List(_)
                | Host::Record(_)
                | Host::Field { .. }
                | Host::Index { .. }
                | Host::Unary { .. }
                | Host::Binary { .. } => e.children().into_iter().all(|x| self.scan_guard(x, srcs)),
                Host::If {
                    condition, yes, no, ..
                } => {
                    self.scan_guard(condition, srcs)
                        && self.scan_block(yes, srcs)
                        && self.scan_block(no, srcs)
                }
                Host::Block(b) => self.scan_block(b, srcs),
                Host::Call {
                    callee,
                    args,
                    site: None,
                } => match builtin_callee(self, callee) {
                    Some(n) if GUARD_CALLS.contains(&n) => {
                        args.iter().all(|x| self.scan_guard(x, srcs))
                    }
                    _ => false,
                },
                _ => false,
            },
            Node::State { .. } => e.children().into_iter().all(|x| self.scan_guard(x, srcs)),
            Node::Construct { name, args, .. } if QUESTION_CONSTRUCTS.contains(&name.as_str()) => {
                args.iter().all(|x| self.scan_guard(x, srcs))
            }
            Node::Effect { effect, inputs, .. } => {
                let sp = spec(*effect);
                sp.produces_reading
                    && sp.taint_rule != TaintRule::Trusted
                    && inputs.iter().all(|(_, x)| self.scan_guard(x, srcs))
            }
            Node::Cut { reading, rest, .. } => {
                let Some((Class::Exit, s)) = self.val(e) else {
                    return false;
                };
                srcs.push(s);
                self.scan_guard(reading, srcs) && rest.iter().all(|x| self.scan_guard(x, srcs))
            }
            Node::Handle {
                exit, arms, rest, ..
            } => {
                if !self.scan_guard(exit, srcs) || !rest.iter().all(|x| self.scan_guard(x, srcs)) {
                    return false;
                }
                let Node::Host(Host::Record(fields)) = &arms.node else {
                    return false;
                };
                for (_, f) in fields {
                    let ok = match &f.node {
                        Node::Host(Host::Function(func)) => {
                            let mut s = HashMap::new();
                            for p in &func.parameters {
                                s.insert(p.name.clone(), Binding::Plain);
                            }
                            self.scopes.push(s);
                            let ok = self.scan_block(&func.body, srcs);
                            self.scopes.pop();
                            ok
                        }
                        _ => self.scan_guard(f, srcs),
                    };
                    if !ok {
                        return false;
                    }
                }
                true
            }
            Node::Consume { how, args, .. } => {
                *how == jpp_ir::ir::ConsumeHow::Consume
                    && args.iter().all(|x| self.scan_guard(x, srcs))
            }
            _ => false,
        }
    }

    fn scan_block(&mut self, b: &Block, srcs: &mut Vec<Src>) -> bool {
        self.scopes.push(HashMap::new());
        let mut ok = true;
        for s in &b.statements {
            ok = ok
                && match s {
                    Statement::Let { name, value, .. } => {
                        let r = self.scan_guard(value, srcs);
                        self.define(name, Binding::Opaque);
                        r
                    }
                    Statement::Function { .. } => false,
                    Statement::Expr(e) => self.scan_guard(e, srcs),
                };
        }
        if let Some(r) = &b.result {
            ok = ok && self.scan_guard(r, srcs);
        }
        self.scopes.pop();
        ok
    }

    /// 一层 `if` 守卫：按 `&&` 拆合取项，每项都是绑到确定不可信的出口或守卫布尔的名字。
    fn if_guard(&self, cond: &Expr) -> Option<Src> {
        let mut conj = vec![];
        conjuncts(cond, &mut conj);
        let mut first = None;
        for c in conj {
            let Node::Host(Host::Name(n)) = &c.node else {
                return None;
            };
            match self.lookup(n) {
                Binding::Known(Class::Exit | Class::Guard, s) => {
                    first.get_or_insert(s);
                }
                _ => return None,
            }
        }
        first
    }

    // ------------------------------------------------------------ `do` 站点

    fn check_do(&mut self, e: &Expr, inputs: &[(String, Expr)]) {
        if self.guards.is_empty() || self.guards.iter().any(|g| g.is_none()) {
            return;
        }
        // 报文：不可信材料的来源取最内层一个不可信根；声明线另列（步 20j-2，全部层的站点）
        let 层: Vec<Src> = self.guards.iter().flatten().cloned().collect();
        let src = Why {
            untrusted: 层
                .iter()
                .rev()
                .find(|s| !matches!(s.root, Root::Declared(_)))
                .cloned(),
            declared: 层
                .iter()
                .filter_map(|s| match &s.root {
                    Root::Declared(d) => Some(d.clone()),
                    _ => None,
                })
                .collect(),
        };
        let name = action_name(inputs);
        // 动作名不是字面量时，查是不是「形参经全程序调用点都同一字面量」（B179 (b)，步 24e-4）；
        // 追到就把诊断落在各个用户调用点，不是 do 自己在库文件里的位置
        let via = name
            .is_none()
            .then(|| action_name_expr(inputs))
            .flatten()
            .and_then(|arg| self.cx.via_param.get(&(arg as *const Expr)));
        match via {
            Some(v) => {
                for span in &v.call_sites {
                    self.report_do(Some(v.action.as_str()), &src, *span);
                }
            }
            None => self.report_do(name, &src, e.span),
        }
    }

    /// 依据：B108（有动作表且不可逆 → J-08；没有动作表 → W-guard-untrusted）
    fn report_do(&mut self, name: Option<&str>, src: &Why, span: Span) {
        match self.cx.actions {
            Some(table) => {
                let Some(n) = name else { return };
                let Some(f) = table.actions.get(n) else {
                    return;
                };
                if f.reversible {
                    return;
                }
                // 依据：B108（知道动作表且动作不可逆）
                self.out
                    .push(Diagnostic::error("J-08", message(n, src, true), span));
            }
            // 依据：B108（检查时不知动作表，可逆动作不受 J-08，所以只报 warn）
            None => self.out.push(Diagnostic::warning(
                "W-guard-untrusted",
                message(name.unwrap_or("?"), src, false),
                span,
            )),
        }
    }
}

/// 被调者是没被程序绑定过的内置名时，返回它。
fn builtin_callee<'e>(w: &W, callee: &'e Expr) -> Option<&'e str> {
    match &callee.node {
        Node::Host(Host::Name(n)) if !w.bound_elsewhere(n) && is_builtin(n) => Some(n.as_str()),
        _ => None,
    }
}

/// 动作名：动作效应的名字槽（`SlotKind::Name`）上的字面文本
pub(crate) fn action_name(inputs: &[(String, Expr)]) -> Option<&str> {
    let slot = jpp_effects::ALL
        .iter()
        .map(|id| spec(*id))
        .find(|s| s.profile_schema == ProfileSchema::Action)?
        .input_schema
        .iter()
        .find(|d| d.kind == SlotKind::Name)?
        .name;
    inputs.iter().find_map(|(k, x)| match &x.node {
        Node::Host(Host::Text(t)) if k == slot => Some(t.as_str()),
        _ => None,
    })
}

/// `action_name` 的姊妹函数（B179 (b)，步 24e-4）：不要求字面量，返回名字槽位置上的原始表达式——
/// `do_sites`、检查器与 CLI 用它定位「动作名实参」，再查 [`crate::analysis::action_via_param`]
/// 的解析表（键是这个表达式的指针，`&Expr` 与 `CallSite.args` 里的引用来自同一棵树，两遍遍历
/// 能用指针对上，不用另建 `NodeId` 对照表）。
pub(crate) fn action_name_expr(inputs: &[(String, Expr)]) -> Option<&Expr> {
    let slot = jpp_effects::ALL
        .iter()
        .map(|id| spec(*id))
        .find(|s| s.profile_schema == ProfileSchema::Action)?
        .input_schema
        .iter()
        .find(|d| d.kind == SlotKind::Name)?
        .name;
    inputs.iter().find_map(|(k, x)| (k == slot).then_some(x))
}

/// `cut` 的实参里有字面记录且带 `declare` 字段（作者声明线，B128；`stat` 线同样要 `declare`，B153）时，给出线的
/// 文字（`hi=0.7 lo=0.3`、`cuts=[…]`，数不是字面量的写 `…`；带字面 `stat` 附上），与运行期拒绝报文同形。
fn 字面声明(rest: &[Expr]) -> Option<String> {
    let 数 = |e: Option<&Expr>| -> Option<String> {
        match &e?.node {
            Node::Host(Host::Decimal(x)) => Some(format!("{x}")),
            Node::Host(Host::Integer(i)) => Some(format!("{i}")),
            _ => None,
        }
    };
    rest.iter().find_map(|a| {
        let Node::Host(Host::Record(fs)) = &a.node else {
            return None;
        };
        let (_, d) = fs.iter().find(|(k, _)| k == "declare")?;
        let 取 = |名: &str| match &d.node {
            Node::Host(Host::Record(df)) => df.iter().find(|(k, _)| k == 名).map(|(_, v)| v),
            _ => None,
        };
        let mut 线 = match (取("hi"), 取("cuts")) {
            (_, Some(_)) => "cuts=[…]".to_string(),
            (hi, None) => {
                let h = 数(hi).unwrap_or_else(|| "…".into());
                let l = match 取("lo") {
                    Some(v) => 数(Some(v)).unwrap_or_else(|| "…".into()),
                    None => h.clone(),
                };
                format!("hi={h} lo={l}")
            }
        };
        if let Some((_, s)) = fs.iter().find(|(k, _)| k == "stat")
            && let Node::Host(Host::Text(t)) = &s.node
        {
            线 += &format!(" stat={t}");
        }
        Some(线)
    })
}

/// `sieve` / `literalize` 的末位字面选项记录里 `line` 是带 `declare` 的字面记录（B128 补齐）时，给出线的文字（同 [`字面声明`]）。
/// 选项在第三位（题或题列表）或第四位（题式加填法）；只认字面量。
fn sieve_字面声明(args: &[Expr]) -> Option<String> {
    let opts = args.get(2..)?.last()?;
    let Node::Host(Host::Record(fs)) = &opts.node else {
        return None;
    };
    let (_, line) = fs.iter().find(|(k, _)| k == "line")?;
    字面声明(std::slice::from_ref(line))
}

/// 报文用的守卫来源：最内层一个不可信材料根（若有），与各层宿主未接受的声明线（站点与线）
struct Why {
    untrusted: Option<Src>,
    declared: Vec<String>,
}

fn conjuncts<'e>(e: &'e Expr, out: &mut Vec<&'e Expr>) {
    match &e.node {
        Node::Host(Host::Binary { op, left, right }) if op == "&&" => {
            conjuncts(left, out);
            conjuncts(right, out);
        }
        _ => out.push(e),
    }
}

fn message(action: &str, why: &Why, known_irreversible: bool) -> String {
    let 动作 = if known_irreversible {
        format!("不可逆动作 {action}")
    } else {
        format!("动作 {action}（检查时不知动作表；若它不可逆）")
    };
    let 声明 = if why.declared.is_empty() {
        String::new()
    } else {
        format!(
            "守卫出口来自作者声明线（{}），宿主未声明接受作者线放行（CLI：--release-on-declared，B128）",
            why.declared.join("、")
        )
    };
    let Some(s) = &why.untrusted else {
        // 全部守卫层都是宿主未接受的声明线（步 20j-2）
        return format!(
            "（静态子面）{动作}的每一层守卫都只来自作者声明线的出口，路径上没有 ask：声明线按你写的数切、不作错误率保证，宿主未接受时不得放行不可逆动作，运行期 J-08 走到这里必拒，之前的调用白花。修法：确认这些线由你担责后带 --release-on-declared 运行（库宿主：EntryArgs.accept.declared_lines），或在条件里再合取一个认证线上的判断，或改走 ask 让人拍板，或把这个动作登记成可逆。{声明}（依据：B128、B108）"
        );
    };
    // 步 14b-1（B108）：两处「宿主未声明可信」都点出 CLI 开关名字，不止笼统说「由宿主声明入口可信」
    let 来源 = match (&s.root, s.direct) {
        (Root::EntryMat(n), true) => {
            format!("该材料是宿主入口 {n}，宿主未声明可信（CLI：--input-trusted）")
        }
        (Root::Action(n), true) => format!("该材料来自动作 {n} 的输出（登记为不可信）"),
        (Root::EntryValue(n) | Root::EntryMat(n), _) => {
            format!(
                "该材料由计算值构成，成分含不可信内容（来自宿主入口 {n}，宿主未声明可信；CLI：--input-trusted）"
            )
        }
        (Root::Action(n), false) => {
            format!("该材料由计算值构成，成分含不可信内容（来自动作 {n} 的输出，登记为不可信）")
        }
        (Root::Declared(_), _) => unreachable!("声明线根不进这一支"),
    };
    if 声明.is_empty() {
        return format!(
            "（静态子面）{动作}的每一层守卫都只来自不可信状态上的判断，路径上没有 ask：不可信材料上的判断不得**单独**放行不可逆动作，运行期 J-08 走到这里必拒，之前的调用白花。修法：在条件里再合取一个来自 trusted 状态的判断，或改走 ask 让人拍板，或由宿主声明入口可信，或把这个动作登记成可逆。{来源}（依据：B108）"
        );
    }
    // 两种都有（步 20j-2）：有的层是不可信材料上的判断，有的层是宿主未接受的声明线
    format!(
        "（静态子面）{动作}的每一层守卫都不作放行证据，路径上没有 ask：有的来自不可信状态上的判断，有的来自宿主未接受的作者声明线，运行期 J-08 走到这里必拒，之前的调用白花。修法：在条件里再合取一个 trusted 状态上、认证线上的判断，或改走 ask 让人拍板，或把这个动作登记成可逆；只带 --release-on-declared 不够。{来源}。{声明}（依据：B108、B128）"
    )
}
