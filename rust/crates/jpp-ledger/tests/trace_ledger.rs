//! C-2：账本行外壳的 `trace` 字段与按追踪编号拼树（预注册 `地基/过程记录/工程-C2-追踪编号.md` §十，
//! 预测 C6–C7、D9–D13）。
use jpp_ledger::{Entry, Ledger, TraceCtx, TraceTree};
use jpp_value::value::Answer;

fn judge(key: &str, cost: f64, call: u64) -> Entry {
    Entry::judge(key, Answer::Noul(0.9), 10, cost, "m", call)
}

/// 一段：`n` 条判断条目，键带来源标签以免同键不写。
fn 一段(l: &mut Ledger, ctx: &TraceCtx, tag: &str, n: usize, cost: f64) {
    l.set_trace(Some(ctx.clone()));
    for i in 0..n {
        l.put(judge(&format!("{tag}{i}"), cost, i as u64 + 1));
    }
}

fn 行(text: &str) -> Vec<String> {
    text.lines().map(str::to_string).collect()
}

#[test]
fn 盖章_编码_解码往返() {
    let root = TraceCtx::start("s", "a");
    let child = root.enter("b");
    let mut l = Ledger::new();
    l.put(judge("先", 0.0, 1)); // 没设上下文：不盖章
    一段(&mut l, &root, "r", 2, 0.5);
    一段(&mut l, &child, "c", 1, 0.25);
    assert_eq!(l.trace_at(0), None);
    assert_eq!(l.trace_at(1), Some(&root));
    assert_eq!(l.trace_at(3), Some(&child));
    assert_eq!(l.last_trace(), Some(&child));
    assert_eq!(l.span_count(), 2);
    let text = l.encode();
    let ls = 行(&text);
    assert!(
        !ls[1].contains("\"trace\""),
        "没盖章的行不写 trace：{}",
        ls[1]
    );
    assert!(ls[2].contains("\"trace\":{\"trace\":\""), "{}", ls[2]);
    // 预注册 A2：根段每行 +79，带父段 +107
    let bare = |s: &str| {
        let mut j: serde_json::Value = serde_json::from_str(s).unwrap();
        j.as_object_mut().unwrap().remove("trace");
        serde_json::to_string(&j).unwrap().len()
    };
    assert_eq!(ls[2].len() - bare(&ls[2]), 79);
    assert_eq!(ls[4].len() - bare(&ls[4]), 107);
    let (back, trunc) = Ledger::decode(&text).unwrap();
    assert!(trunc.is_none());
    assert_eq!(back.len(), 4);
    for i in 0..4 {
        assert_eq!(back.trace_at(i), l.trace_at(i), "第 {i} 条");
    }
    assert_eq!(back.encode(), text, "编解码往返逐字节相同");
}

#[test]
fn 没设上下文时账本与改前逐字节相同() {
    let mut l = Ledger::new();
    l.put(judge("k", 0.1, 1));
    let text = l.encode();
    assert!(!text.contains("trace"), "{text}");
}

#[test]
fn 缺省字段_今天的v4账本照读() {
    let mut l = Ledger::new();
    l.put(judge("k1", 0.1, 1));
    l.put(judge("k2", 0.1, 2));
    let (back, _) = Ledger::decode(&l.encode()).unwrap();
    assert_eq!(back.len(), 2);
    assert_eq!(back.trace_at(0), None);
    assert_eq!(back.last_trace(), None);
    assert_eq!(back.span_count(), 0);
}

#[test]
fn trace进prev链_改非末行被发现_改末行与改entry同一边界() {
    let root = TraceCtx::start("s", "a");
    let other = TraceCtx::start("t", "z");
    let mut l = Ledger::new();
    一段(&mut l, &root, "r", 3, 0.0);
    let ls = 行(&l.encode());
    // 改第 2 行（条目 1，非末行）的 trace
    let mut bad = ls.clone();
    bad[1] = bad[1].replace(root.span.as_str(), other.span.as_str());
    assert_ne!(bad[1], ls[1]);
    let err = Ledger::decode(&(bad.join("\n") + "\n")).unwrap_err();
    assert!(
        err.starts_with("E-ledger-corrupt") && err.contains("第 3 行"),
        "{err}"
    );
    // 改末行的 trace：没有后继行，发现不了（与改末行的 entry 同）
    let mut last = ls.clone();
    let n = last.len() - 1;
    last[n] = last[n].replace(root.span.as_str(), other.span.as_str());
    let (l2, _) = Ledger::decode(&(last.join("\n") + "\n")).unwrap();
    assert_eq!(l2.trace_at(2).unwrap().span, other.span);
    // trace 值不合格（长度不对）：解码拒收
    let mut short = ls.clone();
    short[1] = short[1].replace(root.trace.as_str(), "abc");
    assert!(Ledger::decode(&(short.join("\n") + "\n")).is_err());
}

#[test]
fn 末行半写仍能截断_追踪字段不影响截断规则() {
    let root = TraceCtx::start("s", "a");
    let mut l = Ledger::new();
    一段(&mut l, &root, "r", 3, 0.0);
    let text = l.encode();
    let cut = &text[..text.len() - 30]; // 砍掉末行的尾巴和换行
    let (back, trunc) = Ledger::decode(cut).unwrap();
    assert_eq!(back.len(), 2);
    assert_eq!(trunc.unwrap().kept, 2);
    assert_eq!(back.trace_at(1), Some(&root));
}

/// 预注册 D9：ping(S1)→pong(S2)→ping(S3)，ping 另调一次 pong(S4)。
fn 四段() -> (Vec<(String, Ledger)>, [TraceCtx; 4]) {
    let s1 = TraceCtx::start("链", "ping");
    let s2 = s1.enter("pong");
    let s3 = s2.enter("ping");
    let s4 = s1.enter("pong-2");
    let mut ls = vec![];
    for (name, ctx, n, cost) in [
        ("L1", &s1, 2, 0.5),
        ("L2", &s2, 1, 0.25),
        ("L3", &s3, 3, 0.125),
        ("L4", &s4, 1, 1.0),
    ] {
        let mut l = Ledger::new();
        一段(&mut l, ctx, name, n, cost);
        ls.push((name.to_string(), l));
    }
    (ls, [s1, s2, s3, s4])
}

fn 借(ls: &[(String, Ledger)]) -> Vec<(String, &Ledger)> {
    ls.iter().map(|(n, l)| (n.clone(), l)).collect()
}

#[test]
fn 四份账本拼成一棵树() {
    let (ls, [s1, s2, s3, s4]) = 四段();
    let t = TraceTree::build(&借(&ls));
    assert_eq!(t.traces.len(), 1);
    assert!(t.untraced.is_empty());
    let g = &t.traces[0];
    assert_eq!(g.trace, s1.trace);
    assert!(g.orphans.is_empty());
    assert_eq!(g.roots.len(), 1);
    let r = &g.roots[0];
    assert_eq!(r.span, s1.span);
    assert_eq!(t.node_count(), 4);
    assert_eq!(r.size(), 4);
    assert_eq!(r.depth(), 3);
    assert_eq!(r.children.len(), 2, "S1 有 S2、S4 两个子节点");
    let kids: Vec<_> = r.children.iter().map(|c| c.span.clone()).collect();
    assert!(kids.contains(&s2.span) && kids.contains(&s4.span));
    let n2 = r.children.iter().find(|c| c.span == s2.span).unwrap();
    assert_eq!(n2.children.len(), 1);
    assert_eq!(n2.children[0].span, s3.span);
    assert_eq!(n2.children[0].parent.as_ref(), Some(&s2.span));
    // D11：节点的数等于账本里 Judge 条目的数与费用和；四个节点之和等于四份账本之和
    let mut 总条目 = 0;
    let mut 总费用 = 0.0;
    for n in g.nodes() {
        let 源 = &ls.iter().find(|(name, _)| *name == n.sources[0]).unwrap().1;
        let 判断 = 源
            .entries
            .iter()
            .filter(|e| matches!(e, Entry::Judge { .. }))
            .count();
        let 费用: f64 = 源
            .entries
            .iter()
            .map(|e| match e {
                Entry::Judge { cost, .. } => *cost,
                _ => 0.0,
            })
            .sum();
        assert_eq!(n.judges, 判断);
        assert_eq!(n.entries, 源.len());
        assert_eq!(n.calls, 判断, "调用号各不相同");
        assert!((n.cost - 费用).abs() < 1e-12);
        总条目 += n.entries;
        总费用 += n.cost;
    }
    assert_eq!(总条目, ls.iter().map(|(_, l)| l.len()).sum::<usize>());
    assert!((总费用 - (0.5 * 2.0 + 0.25 + 0.125 * 3.0 + 1.0)).abs() < 1e-12);
    // 文本形式：一个追踪一块、按深度缩进
    let text = t.render();
    assert!(text.starts_with("追踪 "), "{text}");
    assert_eq!(text.lines().count(), 5, "一行追踪头加四段：{text}");
}

#[test]
fn 送入顺序不影响树() {
    let (ls, _) = 四段();
    let 正 = TraceTree::build(&借(&ls));
    let mut rev = 借(&ls);
    rev.reverse();
    assert_eq!(TraceTree::build(&rev), 正);
    let mut mix = 借(&ls);
    mix.swap(0, 2);
    mix.swap(1, 3);
    assert_eq!(TraceTree::build(&mix), 正);
    assert_eq!(TraceTree::build(&mix).render(), 正.render());
}

#[test]
fn 缺一份账本_下游进orphans_不报错() {
    let (ls, [s1, _s2, s3, s4]) = 四段();
    let 缺: Vec<(String, &Ledger)> = ls
        .iter()
        .filter(|(n, _)| n != "L2")
        .map(|(n, l)| (n.clone(), l))
        .collect();
    let t = TraceTree::build(&缺);
    let g = &t.traces[0];
    assert_eq!(g.roots.len(), 1);
    assert_eq!(g.roots[0].span, s1.span);
    assert_eq!(g.roots[0].children.len(), 1);
    assert_eq!(g.roots[0].children[0].span, s4.span);
    assert_eq!(g.orphans.len(), 1);
    assert_eq!(g.orphans[0].span, s3.span);
    assert_eq!(t.node_count(), 3);
    assert!(t.render().contains("缺父段"));
}

#[test]
fn 没有追踪字段的账本进untraced_不影响别的树() {
    let (mut ls, _) = 四段();
    let mut 旧 = Ledger::new();
    旧.put(judge("旧1", 0.1, 1));
    旧.put(judge("旧2", 0.1, 2));
    ls.push(("旧".to_string(), 旧));
    let t = TraceTree::build(&借(&ls));
    assert_eq!(t.untraced.len(), 1);
    assert_eq!(t.untraced[0].source, "旧");
    assert_eq!(t.untraced[0].entries, 2);
    assert_eq!(t.node_count(), 4);
    assert!(t.render().contains("无追踪字段：旧（2 条）"));
}

#[test]
fn 两个追踪编号各成一棵树() {
    let a = TraceCtx::start("甲", "p");
    let b = TraceCtx::start("乙", "p");
    let (mut la, mut lb) = (Ledger::new(), Ledger::new());
    一段(&mut la, &a, "a", 1, 0.0);
    一段(&mut lb, &b, "b", 1, 0.0);
    let t = TraceTree::build(&[("A".into(), &la), ("B".into(), &lb)]);
    assert_eq!(t.traces.len(), 2);
    assert!(t.traces.iter().all(|g| g.roots.len() == 1));
}

#[test]
fn 续跑的账本带旧条目_同一段不叠加() {
    let s1 = TraceCtx::start("链", "ping");
    let mut l1 = Ledger::new();
    一段(&mut l1, &s1, "x", 2, 0.5);
    // 续跑：账本 = 旧条目 + 新段的新条目
    let text = l1.encode();
    let (mut l2, _) = Ledger::decode(&text).unwrap();
    let s1b = s1.enter("resume#1");
    l2.set_trace(Some(s1b.clone()));
    l2.put(judge("y0", 0.75, 3));
    assert_eq!(l2.span_count(), 2);
    assert_eq!(l2.last_trace(), Some(&s1b));
    let t = TraceTree::build(&[("L1".into(), &l1), ("L2".into(), &l2)]);
    let g = &t.traces[0];
    assert_eq!(t.node_count(), 2);
    let root = &g.roots[0];
    assert_eq!(root.span, s1.span);
    assert_eq!(
        root.entries, 2,
        "旧条目在两份账本里各有一份，取多的那份，不叠加"
    );
    assert_eq!(root.sources, vec!["L1".to_string(), "L2".to_string()]);
    assert_eq!(root.children.len(), 1);
    assert_eq!(root.children[0].span, s1b.span);
    assert_eq!(root.children[0].entries, 1);
}

#[test]
fn 父子成环的坏数据不死循环_进orphans() {
    // 手写一份成环的账本行：S_a 的父是 S_b，S_b 的父是 S_a
    let a = TraceCtx::start("环", "a");
    let b = a.enter("b");
    let mut ca = b.clone();
    ca.span = a.span.clone();
    ca.parent = Some(b.span.clone());
    let mut l = Ledger::new();
    l.set_trace(Some(ca));
    l.put(judge("k1", 0.0, 1));
    l.set_trace(Some(b));
    l.put(judge("k2", 0.0, 2));
    let t = TraceTree::build(&[("环".into(), &l)]);
    assert_eq!(t.node_count(), 2);
    assert_eq!(t.traces[0].roots.len(), 0);
    assert_eq!(t.traces[0].orphans.len(), 1);
}
