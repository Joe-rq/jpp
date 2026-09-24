//! 站点表给出的事实（B71 第 6 条）：规则从 IR 与站点表取「所属函数、最近外层站点、方法位」，
//! 不再从表达式形状重算（原 `analysis/spans.rs` 的重复跨度与 `analysis/syntax.rs` 的 `Ctx` 撤）。
//!
//! 站点表（步 12a）每个站点记所属函数与最近外层 `Loop` / 高阶 / 构造站点；这里只补两项 IR 上本来
//! 就有、站点表没直接存的对照：某个函数字面量是不是某个站点的方法位实参，以及函数的外层函数。
//! 都在检查开始时从 IR 读一遍，规则只读。

use crate::*;
use jpp_ir::ir::{Host, Node, SiteKind, SiteTable};
use jpp_ir::key::{NodeId, SiteId};

/// 从 IR 读出的站点事实。
pub(crate) struct SiteFacts<'a> {
    sites: &'a SiteTable,
    /// 方法位上的函数字面量 → (它所属的站点, 该站点是不是 `map` / `filter`)
    method_of: HashMap<NodeId, (SiteId, bool)>,
    /// 函数 → 外层函数（顶层为 `None`）
    fn_parent: HashMap<NodeId, Option<NodeId>>,
    /// 程序顶层块里的函数定义
    top_fns: HashSet<NodeId>,
    /// `loop` 的 bound 表达式里的站点。**口径差（交步 24）**：bound 只求值一次，按树的包含它不在
    /// 循环体里；拆分前按源码跨度算，它落在 `loop(…)` 的跨度里，被算作「可能不止一遍」。这里保留
    /// 旧口径，保住诊断逐字节不变（B71 推翻条件 (3)）。
    loop_bound: HashSet<SiteId>,
}

/// 站点的种类是不是「体会跑不止一遍」的容器：`loop`、`map` / `filter` / `fold`、`iterate`、`pair`。
fn repeats(kind: &SiteKind) -> bool {
    match kind {
        SiteKind::Loop => true,
        SiteKind::HigherOrder(n) => matches!(n.as_str(), "map" | "filter" | "fold"),
        SiteKind::Construct(n) => matches!(n.as_str(), "iterate" | "pair"),
        _ => false,
    }
}

/// 表达式节点的站点（语言形式、效应、构造、高阶宿主调用、条件有站点；其余没有）。
pub(crate) fn site_of(e: &Expr) -> Option<SiteId> {
    match &e.node {
        Node::State { site, .. }
        | Node::Effect { site, .. }
        | Node::Cut { site, .. }
        | Node::Fit { site, .. }
        | Node::Loop { site, .. }
        | Node::Handle { site, .. }
        | Node::Consume { site, .. }
        | Node::Construct { site, .. } => Some(*site),
        Node::Host(Host::Call { site, .. }) => *site,
        Node::Host(Host::If { site, .. }) => Some(*site),
        _ => None,
    }
}

impl<'a> SiteFacts<'a> {
    pub(crate) fn of(body: &Block, sites: &'a SiteTable) -> SiteFacts<'a> {
        let mut f = SiteFacts {
            sites,
            method_of: HashMap::new(),
            fn_parent: HashMap::new(),
            top_fns: HashSet::new(),
            loop_bound: HashSet::new(),
        };
        for s in &body.statements {
            if let Statement::Function { function, .. } = s {
                f.top_fns.insert(function.id);
            }
        }
        f.block(body, None);
        f
    }

    fn block(&mut self, b: &Block, func: Option<NodeId>) {
        for s in &b.statements {
            match s {
                Statement::Let { value, .. } => self.expr(value, func),
                Statement::Function { function, .. } => self.function(function, func),
                Statement::Expr(e) => self.expr(e, func),
            }
        }
        if let Some(r) = &b.result {
            self.expr(r, func);
        }
    }

    fn function(&mut self, f: &Function, parent: Option<NodeId>) {
        self.fn_parent.insert(f.id, parent);
        self.block(&f.body, Some(f.id));
    }

    fn expr(&mut self, e: &Expr, func: Option<NodeId>) {
        if let (Some(site), ExprKind::Call { arguments, .. }) = (site_of(e), e.kind()) {
            let kind = self.sites.get(site).map(|i| i.kind.clone());
            // 方法位：map / filter 的第 2 个实参，fold / loop / iterate 的第 3 个
            let method = match &kind {
                Some(SiteKind::HigherOrder(n)) if n == "map" || n == "filter" => Some((1, true)),
                Some(SiteKind::HigherOrder(n)) if n == "fold" => Some((2, false)),
                Some(SiteKind::Loop) => Some((2, false)),
                Some(SiteKind::Construct(n)) if n == "iterate" => Some((2, false)),
                _ => None,
            };
            if let Some((idx, yields)) = method
                && let Some(ExprKind::Function(m)) = arguments.get(idx).map(|a| a.kind())
            {
                self.method_of.insert(m.id, (site, yields));
            }
            if matches!(kind, Some(SiteKind::Loop))
                && let Some(bound) = arguments.first()
            {
                walk_expr(bound, &mut |x| {
                    if let Some(s) = site_of(x) {
                        self.loop_bound.insert(s);
                    }
                });
            }
        }
        match e.kind() {
            ExprKind::Function(f) => self.function(f, func),
            ExprKind::Call {
                function,
                arguments,
            } => {
                if let Some(fe) = function.expr() {
                    self.expr(fe, func);
                }
                arguments.iter().for_each(|a| self.expr(a, func));
            }
            ExprKind::List(items) => items.iter().for_each(|x| self.expr(x, func)),
            ExprKind::Record(fields) => fields.iter().for_each(|(_, x)| self.expr(x, func)),
            ExprKind::Field { value, .. } | ExprKind::Unary { value, .. } => self.expr(value, func),
            ExprKind::Index { value, index } => {
                self.expr(value, func);
                self.expr(index, func);
            }
            ExprKind::Binary { left, right, .. } => {
                self.expr(left, func);
                self.expr(right, func);
            }
            ExprKind::If { condition, yes, no } => {
                self.expr(condition, func);
                self.block(yes, func);
                self.block(no, func);
            }
            ExprKind::Block(b) => self.block(b, func),
            _ => {}
        }
    }

    /// 在这个函数体里：它是 `loop` / `fold` / `map` / `filter` / `iterate` 方法位上的函数字面量
    /// （J-13 序号要随轮次变的位置）。函数定义是新的词法环境，外层的迭代不穿过它。
    pub(crate) fn in_iteration(&self, func: Option<NodeId>) -> bool {
        func.is_some_and(|f| self.method_of.contains_key(&f))
    }

    /// 在 `map` / `filter`（`for … yield`）的体内（E7）：沿「方法位函数 → 它所属站点 → 该站点所在的
    /// 函数」往外走，只穿过方法位函数，遇到其它函数即停。
    pub(crate) fn in_yield(&self, mut func: Option<NodeId>) -> bool {
        while let Some(f) = func {
            let Some((site, yields)) = self.method_of.get(&f) else {
                return false;
            };
            if *yields {
                return true;
            }
            func = self.sites.get(*site).and_then(|i| i.function);
        }
        false
    }

    /// 这个站点在运行期会不会跑不止一遍（J-10、B32）：外层有循环、高阶或 `iterate` / `pair`，或
    /// 所在函数处在顶层定义的函数里（静态判不了调用图，函数体一律按可能不止一遍计，往拒绝那边倒）。
    /// 无站点的节点按一遍计。
    pub(crate) fn repeated(&self, site: Option<SiteId>) -> bool {
        let Some(site) = site else { return false };
        if self.loop_bound.contains(&site) {
            return true;
        }
        let Some(info) = self.sites.get(site) else {
            return false;
        };
        let mut up = info.enclosing;
        while let Some(s) = up {
            let Some(outer) = self.sites.get(s) else {
                break;
            };
            if repeats(&outer.kind) {
                return true;
            }
            up = outer.enclosing;
        }
        let mut f = info.function;
        while let Some(ff) = f {
            if self.top_fns.contains(&ff) {
                return true;
            }
            f = self.fn_parent.get(&ff).copied().flatten();
        }
        false
    }
}
