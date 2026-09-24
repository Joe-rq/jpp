//! 良构检查（`20` §2.3 `jpp-ir`「不变量与强制点」，D2.3）：降级末尾与每个 pass 后跑。
//!
//! 检的是 IR 自身的结构不变量，不是语言规则（那些在检查器）：节点与站点 id 唯一且站点表与节点
//! 一一对应；效应节点的槽名集合等于效应的输入槽表；字面记录写出的 `handle` 臂含 `unsure`；
//! 预算各字段非负。槽表由调用方经 [`NameTable::slots`] 传入。

use super::*;
use std::collections::BTreeSet;

pub fn wellformed(p: &Program, names: &dyn NameTable) -> Result<(), Vec<IrDiag>> {
    let mut out = vec![];
    let whole = Span::new(p.body.span.start, p.body.span.end);

    let b = &p.budget;
    let bad = |x: f64| x.is_nan() || x.is_sign_negative();
    if bad(b.cost) {
        out.push(IrDiag::new("I-budget", "预算 cost 须非负", whole));
    }
    if b.unsure.is_some_and(bad) {
        out.push(IrDiag::new("I-budget", "预算 unsure 须非负", whole));
    }
    if b.latency_p95.is_some_and(bad) {
        out.push(IrDiag::new("I-budget", "预算 latency_p95 须非负", whole));
    }

    let mut node_ids = BTreeSet::new();
    let mut seen_sites = BTreeSet::new();
    let mut fn_ids = BTreeSet::new();
    let mut visit_fn = |f: &Function, out: &mut Vec<IrDiag>| {
        if !fn_ids.insert(f.id) {
            out.push(IrDiag::new(
                "I-node",
                format!("函数节点 id {} 重复", f.id.0),
                f.body.span,
            ));
        }
    };
    let mut all_exprs: Vec<&Expr> = vec![];
    walk(&p.body, &mut |e| all_exprs.push(e));
    let mut fns: Vec<&Function> = vec![];
    collect_fns(&p.body, &mut fns);
    for f in &fns {
        visit_fn(f, &mut out);
    }
    for e in &all_exprs {
        if !node_ids.insert(e.id) || fn_ids.contains(&e.id) {
            out.push(IrDiag::new(
                "I-node",
                format!("节点 id {} 重复", e.id.0),
                e.span,
            ));
        }
        if let Some(s) = e.site() {
            if !seen_sites.insert(s) {
                out.push(IrDiag::new(
                    "I-site",
                    format!("站点 {} 被多个节点占用", s.0),
                    e.span,
                ));
            }
            match p.sites.get(s) {
                None => out.push(IrDiag::new(
                    "I-site",
                    format!("站点 {} 不在站点表里", s.0),
                    e.span,
                )),
                Some(info) if info.node != e.id => out.push(IrDiag::new(
                    "I-site",
                    format!("站点 {} 登记的节点不是它", s.0),
                    e.span,
                )),
                Some(_) => {}
            }
        }
        match &e.node {
            Node::Effect { effect, inputs, .. } => {
                let want: Vec<&str> = names.slots(*effect);
                let got: Vec<&str> = inputs.iter().map(|(k, _)| k.as_str()).collect();
                if want != got {
                    out.push(IrDiag::new(
                        "I-slots",
                        format!("效应的输入槽应为 {want:?}，实有 {got:?}"),
                        e.span,
                    ));
                }
            }
            Node::Handle { arms, .. } => {
                if let Node::Host(Host::Record(fs)) = &arms.node
                    && !fs.iter().any(|(k, _)| k == "unsure")
                {
                    out.push(IrDiag::new("I-handle", "handle 缺 unsure 臂", e.span));
                }
            }
            _ => {}
        }
    }
    for (i, s) in p.sites.sites.iter().enumerate() {
        if s.id.0 as usize != i {
            out.push(IrDiag::new(
                "I-site",
                format!("站点表第 {i} 行的 id 是 {}", s.id.0),
                s.span,
            ));
        }
        if !seen_sites.contains(&s.id) {
            out.push(IrDiag::new(
                "I-site",
                format!("站点 {} 没有节点", s.id.0),
                s.span,
            ));
        }
    }
    if out.is_empty() { Ok(()) } else { Err(out) }
}

fn collect_fns<'a>(b: &'a Block, out: &mut Vec<&'a Function>) {
    for f in b.functions() {
        out.push(f);
        collect_fns(&f.body, out);
    }
    for e in b.exprs() {
        collect_fns_expr(e, out);
    }
}

fn collect_fns_expr<'a>(e: &'a Expr, out: &mut Vec<&'a Function>) {
    if let Node::Host(Host::Function(f)) = &e.node {
        out.push(f);
    }
    for c in e.children() {
        collect_fns_expr(c, out);
    }
    for b in e.blocks() {
        collect_fns(b, out);
    }
}
