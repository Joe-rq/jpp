//! C-2 证明示例：两个程序互调，账本可按追踪编号拼成一棵树（主控板 Z0170）。
//! 预注册：`地基/过程记录/工程-C2-追踪编号.md` §十，预测 D9–D15（另 C6、A2、E18）。
//!
//! 「调用」由宿主串联做——语言里今天没有 `.jpp` 调用 `.jpp` 的构造（记入「语言缺什么」）：宿主起 ping，
//! 把它的返回值当 pong 的入口值，再把 pong 的返回值当 ping 的入口值（A→B→A），ping 的第一段另调一次 pong
//! （分叉）。追踪上下文经 `--trace-seed`、`--trace-parent`（W3C traceparent）传递，宿主从报告的 `span.traceparent` 取。
use jpp::ledger::{Ledger, TraceTree};
use serde_json::{Value, json};
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};

fn scratch(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("jpp-trace-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&d);
    fs::create_dir_all(&d).unwrap();
    d
}

fn fx(f: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/trace_fixtures")
        .join(f)
}

fn jpp(dir: &Path, args: &[&str]) -> (bool, String, String) {
    let o = Command::new(env!("CARGO_BIN_EXE_jpp"))
        .current_dir(dir)
        .env("HOME", dir)
        .args(args)
        .output()
        .unwrap();
    (
        o.status.success(),
        String::from_utf8_lossy(&o.stdout).into(),
        String::from_utf8_lossy(&o.stderr).into(),
    )
}

/// 跑一段：`prog` 是 ping 或 pong，入口值 `{"a": 值}`，账本落在 `dir/<账本>`；返回报告。
fn 跑一段(
    dir: &Path, prog: &str, 值: &str, 账本: &str, 追踪: &[&str], 另: &[&str]
) -> Value {
    let inp = format!("in-{账本}.json");
    fs::write(dir.join(&inp), json!({"a": 值}).to_string()).unwrap();
    let src = fx(&format!("{prog}.jpp"));
    let fixtures = fx("fx.json");
    let out = format!("r-{账本}.json");
    let mut args = vec![
        "run",
        src.to_str().unwrap(),
        "--input",
        &inp,
        "--fixtures",
        fixtures.to_str().unwrap(),
        "--ledger-out",
        账本,
        "--output",
        &out,
    ];
    args.extend_from_slice(追踪);
    args.extend_from_slice(另);
    let (ok, _, err) = jpp(dir, &args);
    assert!(ok, "{prog} {账本}: {err}");
    serde_json::from_str(&fs::read_to_string(dir.join(&out)).unwrap()).unwrap()
}

struct 链 {
    dir: PathBuf,
    /// L1..L4 的报告
    r: Vec<Value>,
}

/// ping(S1) → pong(S2) → ping(S3)，另 ping(S1) → pong(S4，标签 pong-2)。
fn 跑链(dir: &Path) -> 链 {
    let r1 = 跑一段(
        dir,
        "ping",
        "start",
        "L1.jsonl",
        &["--trace-seed", "链"],
        &[],
    );
    let tp1 = r1["span"]["traceparent"].as_str().unwrap().to_string();
    let v1 = r1["value"].as_str().unwrap().to_string();
    let r2 = 跑一段(dir, "pong", &v1, "L2.jsonl", &["--trace-parent", &tp1], &[]);
    let tp2 = r2["span"]["traceparent"].as_str().unwrap().to_string();
    let v2 = r2["value"].as_str().unwrap().to_string();
    let r3 = 跑一段(dir, "ping", &v2, "L3.jsonl", &["--trace-parent", &tp2], &[]);
    let r4 = 跑一段(
        dir,
        "pong",
        &v1,
        "L4.jsonl",
        &["--trace-parent", &tp1],
        &["--trace-label", "pong-2"],
    );
    assert_eq!(v1, "ping-是");
    assert_eq!(v2, "pong-是");
    链 {
        dir: dir.to_path_buf(),
        r: vec![r1, r2, r3, r4],
    }
}

fn 读账本(dir: &Path, f: &str) -> (String, Ledger) {
    let text = fs::read_to_string(dir.join(f)).unwrap();
    let (l, trunc) = Ledger::decode(&text).unwrap();
    assert!(trunc.is_none());
    (text, l)
}

fn 四份(c: &链) -> Vec<(String, Ledger)> {
    ["L1.jsonl", "L2.jsonl", "L3.jsonl", "L4.jsonl"]
        .iter()
        .map(|f| (f.to_string(), 读账本(&c.dir, f).1))
        .collect()
}

fn 借(ls: &[(String, Ledger)]) -> Vec<(String, &Ledger)> {
    ls.iter().map(|(n, l)| (n.clone(), l)).collect()
}

/// 去掉行外壳的 `trace` 与 `prev` 后的各行（seq、entry 不变）。
fn 去追踪与链(text: &str) -> Vec<Value> {
    text.lines()
        .skip(1)
        .map(|l| {
            let mut j: Value = serde_json::from_str(l).unwrap();
            let o = j.as_object_mut().unwrap();
            o.remove("trace");
            o.remove("prev");
            j
        })
        .collect()
}

/// D9–D11：四份账本、四段，拼成一棵树。
#[test]
fn 两个程序互调_账本拼成一棵树() {
    let d = scratch("tree");
    let c = 跑链(&d);
    let ls = 四份(&c);
    // 每份账本每条条目都带追踪，且同一份账本里只有一段（预测 D9 的前提）
    for (n, l) in &ls {
        assert!(l.len() >= 1, "{n}");
        let t0 = l
            .trace_at(0)
            .unwrap_or_else(|| panic!("{n} 第 1 条没有 trace"));
        for i in 0..l.len() {
            assert_eq!(l.trace_at(i), Some(t0), "{n} 第 {i} 条");
        }
    }
    let ctx: Vec<_> = ls
        .iter()
        .map(|(_, l)| l.trace_at(0).unwrap().clone())
        .collect();
    assert!(
        ctx.iter().all(|t| t.trace == ctx[0].trace),
        "同一个追踪编号"
    );
    assert_eq!(ctx[0].parent, None, "L1 是根段");
    assert_eq!(ctx[1].parent.as_ref(), Some(&ctx[0].span));
    assert_eq!(ctx[2].parent.as_ref(), Some(&ctx[1].span));
    assert_eq!(ctx[3].parent.as_ref(), Some(&ctx[0].span));
    // 报告里的 span 栏与账本一致
    for (i, r) in c.r.iter().enumerate() {
        assert_eq!(r["span"]["span"], json!(ctx[i].span.as_str()), "报告 {i}");
        assert_eq!(r["span"]["trace"], json!(ctx[i].trace.as_str()));
    }

    let t = TraceTree::build(&借(&ls));
    assert_eq!(t.traces.len(), 1);
    assert!(t.untraced.is_empty());
    let g = &t.traces[0];
    assert!(g.orphans.is_empty());
    assert_eq!(g.roots.len(), 1);
    assert_eq!(t.node_count(), 4, "预测 D9：4 个节点");
    let root = &g.roots[0];
    assert_eq!(root.span, ctx[0].span);
    assert_eq!(root.depth(), 3, "最大深度 3");
    assert_eq!(root.children.len(), 2, "S1 有 S2、S4 两个子节点");
    let s2 = root
        .children
        .iter()
        .find(|n| n.span == ctx[1].span)
        .unwrap();
    assert_eq!(s2.children.len(), 1);
    assert_eq!(s2.children[0].span, ctx[2].span);
    assert!(root.children.iter().any(|n| n.span == ctx[3].span));
    // D11：节点的数与对应账本里 Judge 条目一致，四个节点之和等于四份账本之和
    let mut 总条目 = 0;
    let mut 总判断 = 0;
    for (i, n) in g.nodes().into_iter().enumerate() {
        let _ = i;
        let (name, l) = ls.iter().find(|(name, _)| *name == n.sources[0]).unwrap();
        let 判断 = l
            .entries
            .iter()
            .filter(|e| matches!(e, jpp::ledger::Entry::Judge { .. }))
            .count();
        assert_eq!(n.judges, 判断, "{name}");
        assert_eq!(n.judges, 1, "每段一次判断");
        assert_eq!(n.entries, l.len(), "{name}");
        总条目 += n.entries;
        总判断 += n.judges;
    }
    assert_eq!(总条目, ls.iter().map(|(_, l)| l.len()).sum::<usize>());
    assert_eq!(总判断, 4);

    // D10：乱序、倒序送入，树完全相同
    let mut rev = 借(&ls);
    rev.reverse();
    assert_eq!(TraceTree::build(&rev), t);
    let mut mix = 借(&ls);
    mix.swap(0, 3);
    mix.swap(1, 2);
    assert_eq!(TraceTree::build(&mix), t);

    // CLI：`jpp ledger-tree` 的文本与 API 的 render 逐字相同（来源名是命令行上给的路径）
    let (ok, out, err) = jpp(
        &d,
        &[
            "ledger-tree",
            "L3.jsonl",
            "L1.jsonl",
            "L4.jsonl",
            "L2.jsonl",
        ],
    );
    assert!(ok, "{err}");
    assert_eq!(out, t.render(), "CLI 输出与 API 一致");
    assert!(out.starts_with("追踪 "));
    let (ok, js, err) = jpp(
        &d,
        &[
            "ledger-tree",
            "L1.jsonl",
            "L2.jsonl",
            "L3.jsonl",
            "L4.jsonl",
            "--json",
        ],
    );
    assert!(ok, "{err}");
    let j: Value = serde_json::from_str(js.trim()).unwrap();
    assert_eq!(j["traces"].as_array().unwrap().len(), 1);
    assert_eq!(
        j["traces"][0]["roots"][0]["span"],
        json!(ctx[0].span.as_str())
    );
    let _ = fs::remove_dir_all(&d);
}

/// D12：同一条链整体重跑，四份账本逐字节相同；对其中一份审计重放，新增调用 0、账本不变（预测 C6）。
#[test]
fn 重跑与重放逐字节一致() {
    let (d1, d2) = (scratch("rerun-a"), scratch("rerun-b"));
    let (a, b) = (跑链(&d1), 跑链(&d2));
    for f in ["L1.jsonl", "L2.jsonl", "L3.jsonl", "L4.jsonl"] {
        assert_eq!(
            fs::read_to_string(a.dir.join(f)).unwrap(),
            fs::read_to_string(b.dir.join(f)).unwrap(),
            "{f} 两次整链重跑逐字节相同"
        );
    }
    // 审计重放 L2：给同样的入口值与同样的追踪上下文，账本不变
    let tp1 = a.r[0]["span"]["traceparent"].as_str().unwrap().to_string();
    let src = fx("pong.jpp");
    let (ok, _, err) = jpp(
        &d1,
        &[
            "run",
            src.to_str().unwrap(),
            "--input",
            "in-L2.jsonl.json",
            "--replay",
            "L2.jsonl",
            "--trace-parent",
            &tp1,
            "--ledger-out",
            "L2-replay.jsonl",
            "--output",
            "r-replay.json",
        ],
    );
    assert!(ok, "{err}");
    let r: Value =
        serde_json::from_str(&fs::read_to_string(d1.join("r-replay.json")).unwrap()).unwrap();
    assert_eq!(r["cost"]["calls"], json!(0), "重放不发调用");
    assert_eq!(r["value"], a.r[1]["value"]);
    assert_eq!(
        fs::read_to_string(d1.join("L2-replay.jsonl")).unwrap(),
        fs::read_to_string(d1.join("L2.jsonl")).unwrap(),
        "重放写出的账本与原账本逐字节相同"
    );
    let _ = fs::remove_dir_all(&d1);
    let _ = fs::remove_dir_all(&d2);
}

/// D13：缺一份账本，下游进 orphans；混入没有追踪字段的账本，进 untraced，其余树不变。
#[test]
fn 缺账本与旧账本各自单列() {
    let d = scratch("gaps");
    let c = 跑链(&d);
    let ls = 四份(&c);
    let ctx: Vec<_> = ls
        .iter()
        .map(|(_, l)| l.trace_at(0).unwrap().clone())
        .collect();
    let 缺: Vec<(String, &Ledger)> = ls
        .iter()
        .filter(|(n, _)| n != "L2.jsonl")
        .map(|(n, l)| (n.clone(), l))
        .collect();
    let t = TraceTree::build(&缺);
    let g = &t.traces[0];
    assert_eq!(g.roots.len(), 1);
    assert_eq!(g.roots[0].children.len(), 1, "只剩 S4");
    assert_eq!(g.orphans.len(), 1);
    assert_eq!(g.orphans[0].span, ctx[2].span, "S3 的父段 S2 不在这批里");
    assert!(t.render().contains("缺父段"));

    // 今天的旧 v4 账本：不设上下文写出的账本就是它的形状（没有 trace 键）
    let mut 旧 = Ledger::new();
    旧.put(jpp::ledger::Entry::judge(
        "旧键",
        jpp::value::Answer::Noul(0.7),
        1,
        0.0,
        "fixed-0",
        1,
    ));
    let 旧文 = 旧.encode();
    assert!(!旧文.contains("\"trace\""));
    fs::write(d.join("old.jsonl"), &旧文).unwrap();
    let mut 全 = 借(&ls);
    let (_, 旧读) = 读账本(&d, "old.jsonl");
    全.push(("old.jsonl".to_string(), &旧读));
    let t2 = TraceTree::build(&全);
    assert_eq!(t2.node_count(), 4);
    assert_eq!(t2.untraced.len(), 1);
    assert_eq!(t2.untraced[0].source, "old.jsonl");
    assert_eq!(t2.untraced[0].entries, 1);
    assert_eq!(
        t2.traces,
        TraceTree::build(&借(&ls)).traces,
        "旧账本不影响别的树"
    );
    let (ok, out, err) = jpp(&d, &["ledger-tree", "L1.jsonl", "old.jsonl"]);
    assert!(ok, "{err}");
    assert!(out.contains("无追踪字段：old.jsonl（1 条）"), "{out}");
    let _ = fs::remove_dir_all(&d);
}

/// D14：追踪身份不影响账本本体——换一个种子，去掉 `trace` 与 `prev` 后账本逐字节相同。
/// 另（预测 18）：不给任何追踪参数的运行也带根上下文，报告没有 `span` 栏。
#[test]
fn 追踪身份不影响账本本体_默认也带根上下文() {
    let d = scratch("body");
    let a = 跑一段(&d, "ping", "start", "A.jsonl", &["--trace-seed", "甲"], &[]);
    let b = 跑一段(&d, "ping", "start", "B.jsonl", &["--trace-seed", "乙"], &[]);
    let (ta, la) = 读账本(&d, "A.jsonl");
    let (tb, lb) = 读账本(&d, "B.jsonl");
    assert_ne!(la.trace_at(0), lb.trace_at(0), "种子不同，上下文不同");
    assert_ne!(ta, tb);
    assert_eq!(去追踪与链(&ta), 去追踪与链(&tb));
    assert_eq!(a["value"], b["value"]);

    // 默认：不给任何追踪参数
    let r = 跑一段(&d, "ping", "start", "D1.jsonl", &[], &[]);
    let r2 = 跑一段(&d, "ping", "start", "D2.jsonl", &[], &[]);
    assert!(
        r.get("span").is_none(),
        "默认的报告没有 span 栏（报告金样不变）"
    );
    let (t1, l1) = 读账本(&d, "D1.jsonl");
    let (t2, _) = 读账本(&d, "D2.jsonl");
    assert_eq!(
        t1, t2,
        "默认根上下文是确定的：同程序同入口两次运行逐字节相同"
    );
    assert_eq!(r["value"], r2["value"]);
    for i in 0..l1.len() {
        let t = l1.trace_at(i).expect("默认也每条都带 trace");
        assert_eq!(t.parent, None, "默认是根段");
    }
    assert_eq!(
        去追踪与链(&t1),
        去追踪与链(&ta),
        "默认运行与带种子运行的账本本体相同"
    );
    let _ = fs::remove_dir_all(&d);
}

/// D15：续跑——同一追踪编号、新一段，父段是账本里上一段；没有新条目时账本逐字节不变。
#[test]
fn 续跑是同一追踪的新一段() {
    let d = scratch("resume");
    let r1 = 跑一段(
        &d,
        "ping",
        "start",
        "L1.jsonl",
        &["--trace-seed", "链"],
        &[],
    );
    let (全文, l1) = 读账本(&d, "L1.jsonl");
    let s1 = l1.trace_at(0).unwrap().clone();
    assert!(l1.len() >= 2, "账本至少两条才能截出「半途」：{}", l1.len());
    // 完整账本续跑：没有新条目，账本逐字节不变
    let 续 = 跑一段(
        &d,
        "ping",
        "start",
        "L1-resume.jsonl",
        &[],
        &["--resume", "L1.jsonl"],
    );
    assert_eq!(fs::read_to_string(d.join("L1-resume.jsonl")).unwrap(), 全文);
    assert_eq!(续["value"], r1["value"]);
    // 半途账本（头行加第一条）续跑：补上的条目属于新的一段，追踪编号不变，父段是 S1
    let 行: Vec<&str> = 全文.lines().collect();
    fs::write(d.join("cut.jsonl"), format!("{}\n{}\n", 行[0], 行[1])).unwrap();
    跑一段(
        &d,
        "ping",
        "start",
        "cut-resume.jsonl",
        &[],
        &["--resume", "cut.jsonl"],
    );
    let (_, l) = 读账本(&d, "cut-resume.jsonl");
    assert_eq!(l.len(), l1.len(), "补全了缺的条目");
    assert_eq!(l.trace_at(0), Some(&s1), "旧条目的上下文不改");
    let 新 = l.trace_at(l.len() - 1).unwrap();
    assert_eq!(新.trace, s1.trace, "同一追踪编号");
    assert_ne!(新.span, s1.span, "新的一段");
    assert_eq!(新.parent.as_ref(), Some(&s1.span), "父段 = 账本里的上一段");
    assert_eq!(l.span_count(), 2);
    // 树上：一个追踪、两个节点、S1 之下挂新一段；节点数取旧条目多的那份（不叠加）
    let t = TraceTree::build(&[("cut-resume.jsonl".to_string(), &l)]);
    assert_eq!(t.node_count(), 2);
    assert_eq!(t.traces[0].roots[0].span, s1.span);
    assert_eq!(t.traces[0].roots[0].children[0].span, 新.span);
    // 半途账本无追踪时（截到只剩头行）续跑等于首跑：根上下文相同，账本与不截时逐字节相同
    fs::write(d.join("head.jsonl"), format!("{}\n", 行[0])).unwrap();
    跑一段(
        &d,
        "ping",
        "start",
        "head-resume.jsonl",
        &[],
        &["--resume", "head.jsonl", "--trace-seed", "链"],
    );
    assert_eq!(
        fs::read_to_string(d.join("head-resume.jsonl")).unwrap(),
        全文
    );
    let _ = fs::remove_dir_all(&d);
}

/// 复核 G1（主控 Z0249）：续跑一律开新段——宿主给了 `--trace-seed` / `--trace-parent` 也一样。追踪编号取账本里的，
/// 新段的父段是账本里的上一段；宿主给的追踪编号与账本的不一致时报 `W-trace-mismatch`，以账本为准，不静默混段。
/// 有意的代价：崩溃重跑的账本与一次跑完的账本不再逐字节相同（这里断言它们不同）。
#[test]
fn 续跑一律新段_宿主给了追踪参数也一样() {
    let d = scratch("resume-flags");
    跑一段(
        &d,
        "ping",
        "start",
        "L1.jsonl",
        &["--trace-seed", "链"],
        &[],
    );
    let (全文, l1) = 读账本(&d, "L1.jsonl");
    let s1 = l1.trace_at(0).unwrap().clone();
    let 行: Vec<&str> = 全文.lines().collect();
    fs::write(d.join("cut.jsonl"), format!("{}\n{}\n", 行[0], 行[1])).unwrap();
    let 警告 = |r: &Value| -> Vec<String> {
        r["trace"]["warnings"]
            .as_array()
            .unwrap()
            .iter()
            .map(|w| w.as_str().unwrap().to_string())
            .filter(|w| w.contains("W-trace-mismatch"))
            .collect()
    };
    let 检 = |名: &str, r: &Value| -> String {
        let (文, l) = 读账本(&d, 名);
        assert_eq!(l.len(), l1.len(), "{名} 补全了缺的条目");
        assert_eq!(l.trace_at(0), Some(&s1), "{名} 旧条目的上下文不改");
        let 新 = l.trace_at(l.len() - 1).unwrap();
        assert_eq!(新.trace, s1.trace, "{名} 追踪编号取账本里的");
        assert_ne!(新.span, s1.span, "{名} 开了新段");
        assert_eq!(
            新.parent.as_ref(),
            Some(&s1.span),
            "{名} 父段 = 账本里的上一段"
        );
        assert_eq!(l.span_count(), 2, "{名}");
        assert_eq!(
            r["span"]["span"],
            json!(新.span.as_str()),
            "{名} 报告的 span 是实际落账的新段"
        );
        assert_eq!(r["span"]["parent"], json!(s1.span.as_str()), "{名}");
        assert_ne!(
            文, 全文,
            "{名} 崩溃重跑的账本与一次跑完的不再逐字节相同（有意）"
        );
        文
    };
    // 格 1：续跑 + 同一 --trace-seed
    let r1 = 跑一段(
        &d,
        "ping",
        "start",
        "c1.jsonl",
        &["--trace-seed", "链"],
        &["--resume", "cut.jsonl"],
    );
    let c1 = 检("c1.jsonl", &r1);
    assert!(警告(&r1).is_empty(), "追踪编号一致，没有告警");
    // 格 2：续跑 + 同一条链的 --trace-parent（追踪编号一致）
    let r2 = 跑一段(
        &d,
        "ping",
        "start",
        "c2.jsonl",
        &["--trace-parent", &s1.to_traceparent()],
        &["--resume", "cut.jsonl"],
    );
    assert_eq!(
        检("c2.jsonl", &r2),
        c1,
        "新段只由账本推导，与宿主怎么给的一致性参数无关"
    );
    assert!(警告(&r2).is_empty());
    // 格 3：续跑 + 另一条链的 --trace-parent：告警，以账本为准，账本与格 1 逐字节相同
    let 别 = jpp::ledger::TraceCtx::start("别的链", "x");
    let r3 = 跑一段(
        &d,
        "ping",
        "start",
        "c3.jsonl",
        &["--trace-parent", &别.to_traceparent()],
        &["--resume", "cut.jsonl"],
    );
    assert_eq!(检("c3.jsonl", &r3), c1);
    let w = 警告(&r3);
    assert_eq!(w.len(), 1, "{w:?}");
    assert!(
        w[0].contains("以账本为准") && w[0].contains(别.trace.as_str()),
        "{}",
        w[0]
    );
    // 格 4：续跑 + 另一个 --trace-seed：同上
    let r4 = 跑一段(
        &d,
        "ping",
        "start",
        "c4.jsonl",
        &["--trace-seed", "别的种子"],
        &["--resume", "cut.jsonl"],
    );
    assert_eq!(检("c4.jsonl", &r4), c1);
    assert_eq!(警告(&r4).len(), 1);
    let _ = fs::remove_dir_all(&d);
}

/// 命令行的用法错误：`--trace-parent` 与 `--trace-seed` 互斥，traceparent 不合格被拒，`--trace-label` 要有伴。
#[test]
fn 追踪参数的用法错误() {
    let d = scratch("usage");
    let src = fx("ping.jpp");
    let s = src.to_str().unwrap();
    let tp = "00-0123456789abcdef0123456789abcdef-0123456789abcdef-01";
    for (args, 含) in [
        (
            vec!["run", s, "--trace-parent", tp, "--trace-seed", "x"],
            "either --trace-parent",
        ),
        (
            vec!["run", s, "--trace-parent", "00-abc-def-01"],
            "E-trace-id",
        ),
        (
            vec!["run", s, "--trace-label", "x"],
            "requires --trace-parent or --trace-seed",
        ),
        (vec!["run", s, "--trace-seed"], "requires a value"),
        (vec!["ledger-tree"], "requires one or more"),
    ] {
        let (ok, _, err) = jpp(&d, &args);
        assert!(!ok && err.contains(含), "{args:?}: {err}");
    }
    let _ = fs::remove_dir_all(&d);
}
