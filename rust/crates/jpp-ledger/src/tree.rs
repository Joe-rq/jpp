//! 把多份账本按追踪编号拼成调用树（C-2，`地基/规划/骨架候选.md` 2.0.2 C-2 与研究 15 的 OpenTelemetry）。
//!
//! 只读聚合，不写账本。一趟程序运行是一段（span）：账本行外壳的 `trace` 给出追踪编号、段编号与父段编号。
//! 按追踪编号分组，再按父段编号把段连成树。树上放不进去的不吞掉、不报错：
//! - 父段不在这批账本里的段（少给了一份账本）进 [`TraceGroup::orphans`]，连同它自己的子树；
//! - 没有追踪字段的条目（C-2 之前的账本）按来源单列在 [`TraceTree::untraced`]。
//!
//! 结果只取决于账本的内容，不取决于送入的顺序：入参先按来源名排序，子节点按段编号排序。
//! 同一段出现在多份账本里（续跑的文件带着旧条目）时，节点的数取该段条目最多的那一份，不叠加。

use std::collections::{BTreeMap, BTreeSet};

use serde::Serialize;

use crate::{Entry, Ledger, SpanId, TraceId};

/// 树上的一个节点：一段（一趟程序运行）。
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct SpanNode {
    pub span: SpanId,
    pub parent: Option<SpanId>,
    /// 这一段的条目出现在哪些账本里（来源名，去重、有序）。
    pub sources: Vec<String>,
    /// 这一段的条目数（各种条目都算）。
    pub entries: usize,
    /// 其中判断条目（`Judge`）数。
    pub judges: usize,
    /// 判断条目里不同的调用号个数（调用号 0 是宿主手写的，不计）。
    pub calls: usize,
    /// 判断与效应条目的 `cost` 之和（美元）。
    pub cost: f64,
    pub children: Vec<SpanNode>,
}

impl SpanNode {
    /// 以本节点为根的子树节点数。
    pub fn size(&self) -> usize {
        1 + self.children.iter().map(SpanNode::size).sum::<usize>()
    }
    /// 以本节点为根的子树深度（只有自己为 1）。
    pub fn depth(&self) -> usize {
        1 + self.children.iter().map(SpanNode::depth).max().unwrap_or(0)
    }
    fn walk<'a>(&'a self, f: &mut dyn FnMut(&'a SpanNode)) {
        f(self);
        for c in &self.children {
            c.walk(f);
        }
    }
}

/// 一个追踪编号下的所有段。
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct TraceGroup {
    pub trace: TraceId,
    /// 没有父段的段（链的起点）及其子树。
    pub roots: Vec<SpanNode>,
    /// 父段不在这批账本里的段及其子树（缺了账本）。
    pub orphans: Vec<SpanNode>,
}

impl TraceGroup {
    pub fn nodes(&self) -> Vec<&SpanNode> {
        let mut v = vec![];
        for r in self.roots.iter().chain(&self.orphans) {
            r.walk(&mut |n| v.push(n));
        }
        v
    }
}

/// 没有追踪字段的条目：来源与条数。
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Untraced {
    pub source: String,
    pub entries: usize,
}

/// 一批账本拼出的调用树。
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct TraceTree {
    pub traces: Vec<TraceGroup>,
    pub untraced: Vec<Untraced>,
}

#[derive(Default)]
struct Slice {
    entries: usize,
    judges: usize,
    calls: BTreeSet<u64>,
    cost: f64,
}

impl TraceTree {
    /// 从（来源名，账本）拼树。来源名只用来标注，同名的先后按送入顺序。
    pub fn build(ledgers: &[(String, &Ledger)]) -> TraceTree {
        let mut order: Vec<usize> = (0..ledgers.len()).collect();
        order.sort_by(|a, b| ledgers[*a].0.cmp(&ledgers[*b].0));
        // (追踪, 段) -> 父段、各来源的切片
        type Key = (TraceId, SpanId);
        let mut spans: BTreeMap<Key, (Option<SpanId>, BTreeMap<String, Slice>)> = BTreeMap::new();
        let mut untraced = vec![];
        for i in order {
            let (name, l) = &ledgers[i];
            let mut none = 0usize;
            for (k, e) in l.entries.iter().enumerate() {
                let Some(t) = l.trace_at(k) else {
                    none += 1;
                    continue;
                };
                let (parent, per) = spans
                    .entry((t.trace.clone(), t.span.clone()))
                    .or_insert_with(|| (t.parent.clone(), BTreeMap::new()));
                if parent.is_none() {
                    *parent = t.parent.clone();
                }
                let s = per.entry(name.clone()).or_default();
                s.entries += 1;
                match e {
                    Entry::Judge { cost, call, .. } => {
                        s.judges += 1;
                        if *call > 0 {
                            s.calls.insert(*call);
                        }
                        s.cost += cost;
                    }
                    Entry::Effect { cost, .. } => s.cost += cost,
                    _ => {}
                }
            }
            if none > 0 {
                untraced.push(Untraced {
                    source: name.clone(),
                    entries: none,
                });
            }
        }
        // 每段一个节点（数取条目最多的那份切片，并列取来源名靠前的）
        let mut by_trace: BTreeMap<TraceId, BTreeMap<SpanId, SpanNode>> = BTreeMap::new();
        for ((trace, span), (parent, per)) in spans {
            let best = per
                .iter()
                .max_by(|a, b| a.1.entries.cmp(&b.1.entries).then_with(|| b.0.cmp(a.0)))
                .map(|(_, s)| s)
                .expect("每段至少一份切片");
            by_trace.entry(trace).or_default().insert(
                span.clone(),
                SpanNode {
                    span,
                    parent,
                    sources: per.keys().cloned().collect(),
                    entries: best.entries,
                    judges: best.judges,
                    calls: best.calls.len(),
                    cost: best.cost,
                    children: vec![],
                },
            );
        }
        let traces = by_trace
            .into_iter()
            .map(|(trace, nodes)| group(trace, nodes))
            .collect();
        TraceTree { traces, untraced }
    }

    /// 全部追踪下的节点总数。
    pub fn node_count(&self) -> usize {
        self.traces.iter().map(|g| g.nodes().len()).sum()
    }

    /// 缩进文本：一个追踪一块，子节点缩进两格；缺父段的段与没有追踪字段的账本单列。
    pub fn render(&self) -> String {
        let mut out = String::new();
        for g in &self.traces {
            out.push_str(&format!(
                "追踪 {}（{} 段）\n",
                g.trace.as_str().get(..8).unwrap_or(""),
                g.nodes().len()
            ));
            for r in &g.roots {
                render_node(r, 1, &mut out);
            }
            if !g.orphans.is_empty() {
                out.push_str("  缺父段（父段不在这批账本里）：\n");
                for r in &g.orphans {
                    render_node(r, 2, &mut out);
                }
            }
        }
        for u in &self.untraced {
            out.push_str(&format!("无追踪字段：{}（{} 条）\n", u.source, u.entries));
        }
        out
    }
}

fn short(s: &SpanId) -> &str {
    s.as_str().get(..8).unwrap_or("")
}

fn render_node(n: &SpanNode, depth: usize, out: &mut String) {
    let parent = match &n.parent {
        Some(p) => format!(" 父 {}", short(p)),
        None => String::new(),
    };
    out.push_str(&format!(
        "{}{} [{}]{} 条目 {} 判断 {} 调用 {} 费用 {:.6}\n",
        "  ".repeat(depth),
        short(&n.span),
        n.sources.join(","),
        parent,
        n.entries,
        n.judges,
        n.calls,
        n.cost
    ));
    for c in &n.children {
        render_node(c, depth + 1, out);
    }
}

/// 把一个追踪下的节点按父段连成树。没有父段的是根；父段不在集合里的是缺父段的根；
/// 剩下没有被任何根走到的（父子成环，坏数据）也按缺父段处理，从段编号最小的起打断。
fn group(trace: TraceId, mut nodes: BTreeMap<SpanId, SpanNode>) -> TraceGroup {
    let mut kids: BTreeMap<SpanId, Vec<SpanId>> = BTreeMap::new();
    let (mut roots, mut orphans) = (vec![], vec![]);
    for (id, n) in &nodes {
        match &n.parent {
            None => roots.push(id.clone()),
            Some(p) if p != id && nodes.contains_key(p) => {
                kids.entry(p.clone()).or_default().push(id.clone())
            }
            Some(_) => orphans.push(id.clone()),
        }
    }
    let mut taken: BTreeSet<SpanId> = BTreeSet::new();
    fn build(
        id: &SpanId,
        nodes: &mut BTreeMap<SpanId, SpanNode>,
        kids: &BTreeMap<SpanId, Vec<SpanId>>,
        taken: &mut BTreeSet<SpanId>,
    ) -> Option<SpanNode> {
        if !taken.insert(id.clone()) {
            return None;
        }
        let mut n = nodes.remove(id)?;
        for k in kids.get(id).map(|v| v.as_slice()).unwrap_or(&[]) {
            if let Some(c) = build(k, nodes, kids, taken) {
                n.children.push(c);
            }
        }
        Some(n)
    }
    let roots: Vec<SpanNode> = roots
        .iter()
        .filter_map(|id| build(id, &mut nodes, &kids, &mut taken))
        .collect();
    let mut orphan_nodes: Vec<SpanNode> = orphans
        .iter()
        .filter_map(|id| build(id, &mut nodes, &kids, &mut taken))
        .collect();
    // 成环的余下部分
    let rest: Vec<SpanId> = nodes.keys().cloned().collect();
    for id in rest {
        if let Some(n) = build(&id, &mut nodes, &kids, &mut taken) {
            orphan_nodes.push(n);
        }
    }
    orphan_nodes.sort_by(|a, b| a.span.cmp(&b.span));
    TraceGroup {
        trace,
        roots,
        orphans: orphan_nodes,
    }
}
