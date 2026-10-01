//! Z0205（复核 Z0173-Z0174 缺口 1）：并列最优解超过动作的枚举上限（64）、`ties_complete` 为假时，
//! `interval.same` 不得判真——此时 `differs` 只看得到被列出的并列解，可能漏掉只在被截掉的并列解里出现的未决边。
//! 预注册：`地基/过程记录/工程-小缺陷-0929.md` §4.3。装配与 `interval_ties.rs` 相同（固定判断端口按材料给读数，
//! 真动作 `graph:*`，库文本接在 budget 行后）。

mod common;

use jpp::effects::{CalibStore, FnPort, JudgeResult, Ports};
use jpp::interp::{ActionRegistry, Passes};
use jpp::ledger::Ledger;
use jpp::value::{Answer, Question, State};
use jpp::{lower, syntax::parse};
use serde_json::Value as Json;

const 库: &str = include_str!("../../../lib/compose/graph.jpp");

type 边 = (String, String, f64);

fn 端口(边: Vec<边>) -> Ports<'static> {
    Ports::new().with(FnPort::judge("m", move |s: &State, qs: &[&Question]| {
        let t = s.on_text();
        let p = 边
            .iter()
            .find(|(a, b, _)| t.contains(a.as_str()) && t.contains(b.as_str()))
            .map(|x| x.2);
        Ok(JudgeResult {
            answers: qs
                .iter()
                .map(|q| {
                    Answer::Noul(match p {
                        Some(p) if q.text.contains("合作") => p,
                        _ if q.text.contains("不") => 0.05,
                        _ => 0.95,
                    })
                })
                .collect(),
            tokens: 0,
            cost: 0.0,
            mode_share: vec![None; qs.len()],
            perms: vec![0; qs.len()],
            confidence: vec![],
        })
    }))
}

fn 跑(body: &str, 边: Vec<边>) -> Json {
    let (budget, rest) = body.split_once('\n').expect("第一行是 budget");
    let src = format!("{budget}\n{库}\n{rest}");
    let program = lower(&parse(&src).unwrap_or_else(|e| panic!("解析：{e:?}"))).expect("lower");
    let mut calib = CalibStore::new();
    common::certified(&mut calib, "edge", 0.8, 0.2, 50);
    let mut actions = ActionRegistry::new();
    jpp::actions::register_all(&mut actions, &jpp::actions::Ctx::default(), false);
    let mut ledger = Ledger::new();
    let mut it = jpp::interp::Interp::new(
        端口(边),
        &mut ledger,
        &calib,
        &actions,
        program.budget.clone(),
    );
    it.passes = Passes::default();
    it.run(&program)
        .unwrap_or_else(|e| panic!("程序应当跑完：{}", e.render()))
        .value_json()
}

/// 两位数编号，避免「N1」是「N10」的子串（端口按子串认边）
fn 名(前缀: &str, i: usize) -> String {
    format!("{前缀}{}", 10 + i)
}

/// k 组、每组两个互不相连的点、组间全连：2^k 个最大团。`pending` 给一对点号时，这条边判出未决（读数 0.5），其余已决。
fn 团边(k: usize, pending: Option<(usize, usize)>) -> (Vec<(usize, usize)>, Vec<边>) {
    let mut ij = vec![];
    let mut es = vec![];
    for a in 0..2 * k {
        for b in (a + 1)..2 * k {
            if a / 2 != b / 2 {
                ij.push((a, b));
                let p = if pending == Some((a, b)) { 0.5 } else { 0.95 };
                es.push((名("N", a), 名("N", b), p));
            }
        }
    }
    (ij, es)
}

fn 团程序(n: usize, ij: &[(usize, usize)]) -> String {
    let cands: Vec<String> = ij.iter().map(|(a, b)| format!("[{a}, {b}]")).collect();
    format!(
        r#"budget {{calls: 2000, cost: 10, depth: 64}};
let ns = map(range(0, {n}), fn(i) {{ mat(join(["N", text(10 + i)], "")) }});
let g = judged_graph(ns, test("这两方能合作吗？", "edge"), {{prune: fn(ns) {{ [{}] }}}});
let t = interval(g, "max_clique", {{}});
{{differs: map(t.differs, fn(e) {{ e.ends }}), same: t.same, unique: t.unique,
 lo: t.tie_count_lo, hi: t.tie_count_hi, full: t.ties_complete, complete: t.complete,
 pending: concat(t.hi.pending, g.pending)}}
"#,
        cands.join(", ")
    )
}

fn 团(k: usize, pending: Option<(usize, usize)>) -> Json {
    let (ij, es) = 团边(k, pending);
    跑(&团程序(2 * k, &ij), es)
}

/// 匹配：k 个左点各有两个等权右点（2^k 个最大权匹配）。`pending` 给（左点, 右点号）时这条边未决。
fn 匹配(k: usize, pending: Option<(usize, usize)>) -> Json {
    let mut cands = vec![];
    let mut es = vec![];
    for i in 0..k {
        for j in [2 * i, 2 * i + 1] {
            cands.push(format!("[{i}, {j}]"));
            let p = if pending == Some((i, j)) { 0.5 } else { 0.95 };
            es.push((名("L", i), 名("R", j), p));
        }
    }
    let 程序 = format!(
        r#"budget {{calls: 2000, cost: 10, depth: 64}};
let left = map(range(0, {k}), fn(i) {{ mat(join(["L", text(10 + i)], "")) }});
let right = map(range(0, {}), fn(i) {{ mat(join(["R", text(10 + i)], "")) }});
let g = judged_bipartite(left, right, test("这两方能合作吗？", "edge"), {{prune: fn(l, r) {{ [{}] }}}});
let t = interval(g, "matching", {{}});
{{differs: map(t.differs, fn(e) {{ e.ends }}), same: t.same, unique: t.unique,
 lo: t.tie_count_lo, hi: t.tie_count_hi, full: t.ties_complete, complete: t.complete,
 pending: concat(t.hi.pending, g.pending)}}
"#,
        2 * k,
        cands.join(", ")
    );
    跑(&程序, es)
}

fn j(s: &str) -> Json {
    serde_json::from_str(s).unwrap()
}

/// 团：7 组（128 个最大团，超上限 64），一条未决边（点 1 与点 2，跨组）。动作只列出前 64 个并列团，其中没有一个
/// 用到这条边（用探针在 98 个位置试过，见过程记录 §5.3），`differs` 因此为空；但乐观图里有并列团用到它。
/// 改前 `same` 为真（本缺陷）；改后 `ties_complete` 为假，`same` 不得判真。
#[test]
fn max_clique超上限时same不判真() {
    let v = 团(7, Some((1, 2)));
    assert_eq!(v["complete"], j("true"), "{v}");
    assert_eq!(
        v["differs"],
        j("[]"),
        "被列出的并列团没有一个用到这条未决边：{v}"
    );
    assert_eq!(
        v["full"],
        j("false"),
        "并列 128 > 64，动作如实报没列全：{v}"
    );
    assert_eq!(v["hi"], j("64"), "上限本身：至少这么多");
    assert_eq!(
        v["same"],
        j("false"),
        "并列没列全，无法证明结论不依赖未决边：{v}"
    );
    assert_eq!(v["unique"], j("false"));
}

/// 匹配：7 个左点各有两个等权右点（128 个最大权匹配），未决边（左点 0 与右点 1）同理落在被截掉的并列里。
#[test]
fn matching超上限时same不判真() {
    let v = 匹配(7, Some((0, 1)));
    assert_eq!(v["complete"], j("true"), "{v}");
    assert_eq!(v["differs"], j("[]"), "{v}");
    assert_eq!(v["full"], j("false"), "{v}");
    assert_eq!(v["hi"], j("64"));
    assert_eq!(v["same"], j("false"), "{v}");
    assert_eq!(v["unique"], j("false"));
}

/// 对照：并列在上限内（k=5 共 32 个；k=6 恰好 64 个，不算溢出），全部已决：列全了、`same` 照旧为真。
#[test]
fn 并列在上限内时same照旧() {
    for (k, n) in [(5, "32"), (6, "64")] {
        let v = 团(k, None);
        assert_eq!(v["hi"], j(n), "{v}");
        assert_eq!(v["full"], j("true"), "k={k}：{v}");
        assert_eq!(v["differs"], j("[]"));
        assert_eq!(
            v["same"],
            j("true"),
            "k={k}：列全了且不依赖未决边，same 仍为真：{v}"
        );
        assert_eq!(v["unique"], j("false"), "有并列，不唯一");
    }
}
