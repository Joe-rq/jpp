//! `do` 的动作名实参是形参名时，沿词法作用域链追一层函数调用，把它解析成字面动作名（B179 (b)，
//! 步 24e-4）：若该形参所属的**具名**函数在全程序所有调用点上、该形参位置的实参都是同一个字面
//! 文本，把这个 `do` 站点当作那个字面动作名处理；`E-action-no-sandbox`、J-11（未登记）、J-08
//! 静态子面、`do_sites`（CLI `irreversible_action_in` 据此判定）共用这一份解析。
//!
//! **词法链，不是直接外层。** `lib/compose/ground.jpp` 的 `do(action, args_of(cand), 0)` 直接
//! 所在的函数是 `ground` 返回的匿名闭包 `fn(cand) {...}`（形参只有 `cand`），`action` 是再往外
//! 一层、`ground` 自己的形参——沿「函数 → 词法外层函数」链一路网上找，直到找到哪一层把这个名字
//! 声明为形参，不是只看 `do` 直接所在的那个函数。这条链穿过的是**同一段源码里的闭包嵌套**，不是
//! 另一次函数调用，所以不算「多追一层」。
//!
//! **一层，不递归：管的是「函数调用转发」。** 找到形参所属的具名函数后，看它在全程序的每个调用
//! 点在这个形参位置给的实参——只要有一个不是字面文本（哪怕它本身恰好又是别的函数的形参），
//! 当场停，不再往上追。这与 13b「实参只有名字或字面量」同一口径，这里进一步要求必须是字面量
//! 才算追到。找不到具名函数（形参属于一个从未被单独命名、只能作为值传递的匿名闭包）同样不追——
//! 没有名字就没有「全部调用点」可查。
//!
//! 依据：B179 (b)（`地基/附注/2026-09-26-批7裁定.md` §十二）；`地基/过程记录/工程-步24e-4.md`。

use crate::*;
use jpp_ir::ir::{Host, Node};
use jpp_ir::key::NodeId;

/// 一次成功的「经形参转发」解析：字面动作名、经过的函数与形参名、全部调用点（诊断落在这里，
/// 不是 `do` 自己的位置——`ground.jpp` 这类库函数的用户读不到库文件里的 span）。
#[derive(Clone, Debug)]
pub(crate) struct ViaParam {
    pub action: String,
    pub fn_name: String,
    pub param: String,
    pub call_sites: Vec<Span>,
}

/// 每个函数字面量的登记（具名与否都收，词法链要能穿过匿名闭包）。
struct FnMeta {
    /// 具名时是 `Statement::Function`/`let 名 = fn(...) {...}` 绑定的名字；匿名闭包为 `None`。
    name: Option<String>,
    params: Vec<String>,
    /// 词法外层函数（顶层为 `None`）。
    parent: Option<NodeId>,
}

/// 一条待解析项：`do` 的动作名实参是 `Host::Name`，登记它所在的函数与这个名字。
struct Pending<'a> {
    name_arg: &'a Expr,
    fn_at: Option<NodeId>,
    param: &'a str,
}

struct Collector<'a> {
    fn_meta: HashMap<NodeId, FnMeta>,
    /// 被调者是裸名字的调用点：函数名 → [(调用点 span, 实参表)]。
    calls_by_name: HashMap<&'a str, Vec<(Span, &'a [Expr])>>,
    pending: Vec<Pending<'a>>,
}

impl<'a> Collector<'a> {
    fn register_fn(&mut self, f: &'a Function, name: Option<&str>, parent: Option<NodeId>) {
        self.fn_meta.entry(f.id).or_insert_with(|| FnMeta {
            name: name.map(str::to_string),
            params: f.parameters.iter().map(|p| p.name.clone()).collect(),
            parent,
        });
    }

    fn block(&mut self, b: &'a Block, current_fn: Option<NodeId>) {
        for s in &b.statements {
            match s {
                Statement::Let { name, value, .. } => {
                    if let Node::Host(Host::Function(f)) = &value.node {
                        self.register_fn(f, Some(name.as_str()), current_fn);
                    }
                    self.expr(value, current_fn);
                }
                Statement::Function { name, function, .. } => {
                    self.register_fn(function, Some(name.as_str()), current_fn);
                    self.block(&function.body, Some(function.id));
                }
                Statement::Expr(e) => self.expr(e, current_fn),
            }
        }
        if let Some(r) = &b.result {
            self.expr(r, current_fn);
        }
    }

    fn expr(&mut self, e: &'a Expr, current_fn: Option<NodeId>) {
        if let Node::Host(Host::Function(f)) = &e.node {
            // 到这里还没登记过的，就是没被 let/具名函数语句直接绑定的匿名闭包（例如内联实参）。
            self.register_fn(f, None, current_fn);
        }
        if let Node::Host(Host::Call { callee, args, .. }) = &e.node
            && let Node::Host(Host::Name(cn)) = &callee.node
        {
            // 内置/效应在降级时已经变成各自的专门节点（Node::Effect、Node::Cut……），走不到这里；
            // 留在 Host::Call 上的裸名字调用天然只剩用户函数。
            self.calls_by_name
                .entry(cn.as_str())
                .or_default()
                .push((e.span, args.as_slice()));
        }
        if let Node::Effect { effect, inputs, .. } = &e.node
            && jpp_effects::kinds::spec(*effect).profile_schema
                == jpp_effects::spec::ProfileSchema::Action
            && let Some(arg) = crate::rules::j08_action_name_expr(inputs)
            && let Node::Host(Host::Name(p)) = &arg.node
        {
            self.pending.push(Pending {
                name_arg: arg,
                fn_at: current_fn,
                param: p.as_str(),
            });
        }
        for c in e.children() {
            self.expr(c, current_fn);
        }
        for b in e.blocks() {
            let next = match &e.node {
                Node::Host(Host::Function(f)) => Some(f.id),
                _ => current_fn,
            };
            self.block(b, next);
        }
    }
}

/// 从 `fn_at` 出发，沿「函数 → 词法外层函数」链找最近一个把 `param` 声明为形参的函数；查它在
/// 全程序的调用点，要求都给了同一个字面文本才算追到。
fn resolve_one<'a>(
    meta: &HashMap<NodeId, FnMeta>,
    calls: &HashMap<&'a str, Vec<(Span, &'a [Expr])>>,
    mut fn_at: Option<NodeId>,
    param: &str,
) -> Option<(String, String, Vec<Span>)> {
    while let Some(id) = fn_at {
        let m = meta.get(&id)?;
        if let Some(idx) = m.params.iter().position(|p| p == param) {
            let name = m.name.as_ref()?; // 匿名闭包：没有名字，找不到调用点，不追
            let sites = calls.get(name.as_str())?;
            if sites.is_empty() {
                return None;
            }
            let mut lit: Option<&str> = None;
            let mut spans = vec![];
            for (span, args) in sites {
                let a = args.get(idx)?;
                let Node::Host(Host::Text(t)) = &a.node else {
                    return None; // 调用点实参不是字面量：当场停，不递归
                };
                match lit {
                    None => lit = Some(t.as_str()),
                    Some(l) if l == t.as_str() => {}
                    Some(_) => return None, // 不同调用点给了不同字面量
                }
                spans.push(*span);
            }
            return Some((lit?.to_string(), name.clone(), spans));
        }
        fn_at = m.parent;
    }
    None
}

/// 全程序一遍：`do` 的动作名实参是形参名时，尝试解析成字面动作名。返回值以「动作名实参表达式」
/// 的指针为键——`CallSite.args`、`action_name_expr` 拿到的 `&Expr` 与这里遍历用的是同一棵树，
/// 两遍遍历的引用指向同一块内存，可以直接用指针对上，不用另建 `NodeId` 对照表。
pub(crate) fn resolve(program: &Program) -> HashMap<*const Expr, ViaParam> {
    let mut c = Collector {
        fn_meta: HashMap::new(),
        calls_by_name: HashMap::new(),
        pending: vec![],
    };
    c.block(&program.body, None);
    let mut out = HashMap::new();
    for p in &c.pending {
        if let Some((action, fn_name, call_sites)) =
            resolve_one(&c.fn_meta, &c.calls_by_name, p.fn_at, p.param)
        {
            out.insert(
                p.name_arg as *const Expr,
                ViaParam {
                    action,
                    fn_name,
                    param: p.param.to_string(),
                    call_sites,
                },
            );
        }
    }
    out
}
