//! C-8（主控 Z0174）：`lib/compose/graph.jpp` 的 `interval` 认并列解——并列的最优解（同样大的另一个团、另一条
//! 同长最短路、另一组等权匹配）用到的未决边也进 `differs`，`same` 不再误报为真；新增 `unique`、`tie_count_lo`、
//! `tie_count_hi`、`ties_complete`。案例 01 缺口第 3 条的原文要求（`地基/案例/01-谁在说谎/缺口.md` 第 39、82 行）。
//!
//! 预注册：`地基/过程记录/工程-C7C8-动作表.md` §5.3 P8-6。装配与 `compose_graph.rs` 相同（固定判断端口按材料给读数，
//! 真动作 `graph:*`，库文本接在 budget 行后）。

mod common;

use jpp::effects::{CalibStore, FnPort, JudgeResult, Ports};
use jpp::interp::{ActionRegistry, Passes};
use jpp::ledger::Ledger;
use jpp::value::{Answer, Question, State};
use jpp::{lower, syntax::parse};

const 库: &str = include_str!("../../../lib/compose/graph.jpp");

fn 端口<'a>(边: &'a [(&'a str, &'a str, f64)]) -> Ports<'a> {
    Ports::new().with(FnPort::judge("m", move |s: &State, qs: &[&Question]| {
        let t = s.on_text();
        let p = 边
            .iter()
            .find(|(a, b, _)| t.contains(a) && t.contains(b))
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

fn 跑(body: &str, 边: &[(&str, &str, f64)]) -> serde_json::Value {
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
    let o = it
        .run(&program)
        .unwrap_or_else(|e| panic!("程序应当跑完：{}", e.render()));
    o.value_json()
}

fn j(s: &str) -> serde_json::Value {
    serde_json::from_str(s).unwrap()
}

const 团程序: &str = r#"budget {calls: 20, cost: 1, depth: 64};
let ns = map(range(0, 6), fn(i) { mat(join(["N", text(i)], "")) });
let g = judged_graph(ns, test("这两方能合作吗？", "edge"), {prune: fn(ns) { CANDS }});
let t = interval(g, "max_clique", {});
{differs: map(t.differs, fn(e) { e.ends }), same: t.same, unique: t.unique,
 lo: t.tie_count_lo, hi: t.tie_count_hi, full: t.ties_complete,
 members: map(t.hi.value, fn(p) { p.members }), pending: concat(t.hi.pending, g.pending)}
"#;

fn 团(cands: &str, 边: &[(&str, &str, f64)]) -> serde_json::Value {
    跑(&团程序.replace("CANDS", cands), 边)
}

/// 案例 01 的形状：{N0,N1} 是已决团，{N2,N3} 同样大、但它的边是未决的。改前 `differs` 只看主解（搜索顺序里第一个，
/// 已决团），为空、`same` 误报为真；改后并列团用到的未决边进 `differs`，`same` 为假。P8-6。
#[test]
fn 并列团的未决边进differs() {
    let v = 团("[[0, 1], [2, 3]]", &[("N0", "N1", 0.95), ("N2", "N3", 0.5)]);
    // 主解是已决团 {N0,N1}（搜索顺序第一个）：改前 `differs` 只收主解用到的未决边，为空、`same` 为真
    assert_eq!(v["members"], j("[[0, 1]]"), "{v}");
    assert_eq!(v["differs"], j("[[2, 3]]"), "{v}");
    assert_eq!(
        v["same"],
        j("false"),
        "并列团依赖未决边，结论不能报「不依赖」"
    );
    assert_eq!(v["unique"], j("false"));
    assert_eq!(v["lo"], j("1"), "只信已决边时只有 {{N0,N1}} 一个最大团");
    assert_eq!(v["hi"], j("2"), "乐观图里两个团并列");
    assert_eq!(v["full"], j("true"));
}

/// 两个并列团的边都已决：不依赖未决边（`same` 为真），但结论不唯一（`unique` 为假）——读者不会再误以为唯一。
#[test]
fn 已决并列团不依赖未决边但不唯一() {
    let v = 团(
        "[[0, 1], [2, 3]]",
        &[("N0", "N1", 0.95), ("N2", "N3", 0.95)],
    );
    assert_eq!(v["differs"], j("[]"));
    assert_eq!(v["same"], j("true"));
    assert_eq!(v["unique"], j("false"), "{v}");
    assert_eq!((v["lo"].clone(), v["hi"].clone()), (j("2"), j("2")));
}

/// 唯一最大团（三角形已决）：`unique` 为真，与改前一致。
#[test]
fn 唯一最大团unique为真() {
    let v = 团(
        "[[0, 1], [1, 2], [0, 2]]",
        &[("N0", "N1", 0.95), ("N1", "N2", 0.95), ("N0", "N2", 0.95)],
    );
    assert_eq!(v["unique"], j("true"), "{v}");
    assert_eq!(v["same"], j("true"));
    assert_eq!((v["lo"].clone(), v["hi"].clone()), (j("1"), j("1")));
}

/// 二部图匹配：L0 配 R0（已决）或 R1（未决）同权，并列匹配用到的未决边进 `differs`。
#[test]
fn 并列匹配的未决边进differs() {
    let 程序 = r#"budget {calls: 20, cost: 1, depth: 64};
let left = [mat("L0")];
let right = [mat("R0"), mat("R1")];
let g = judged_bipartite(left, right, test("这两方能合作吗？", "edge"), {prune: fn(l, r) { [[0, 0], [0, 1]] }});
let t = interval(g, "matching", {});
{differs: map(t.differs, fn(e) { e.ends }), same: t.same, unique: t.unique,
 lo: t.tie_count_lo, hi: t.tie_count_hi, pending: concat(t.hi.pending, g.pending)}
"#;
    let v = 跑(程序, &[("L0", "R0", 0.95), ("L0", "R1", 0.5)]);
    assert_eq!(v["differs"], j("[[0, 2]]"), "{v}");
    assert_eq!(v["same"], j("false"));
    assert_eq!(v["unique"], j("false"));
    assert_eq!((v["lo"].clone(), v["hi"].clone()), (j("1"), j("2")));
}

/// 最短路：经 a 的路已决，经 b 的路最后一条边未决、同长。并列路径用到的未决边进 `differs`。
#[test]
fn 并列最短路的未决边进differs() {
    let 程序 = r#"budget {calls: 20, cost: 1, depth: 64};
let ns = map(range(0, 4), fn(i) { mat(join(["N", text(i)], "")) });
let g = judged_graph(ns, test("这两方能合作吗？", "edge"), {prune: fn(ns) { [[0, 1], [1, 3], [0, 2], [2, 3]] }});
let t = interval(g, "shortest_path", {source: 0, target: 3});
{differs: map(t.differs, fn(e) { e.ends }), same: t.same, unique: t.unique,
 lo: t.tie_count_lo, hi: t.tie_count_hi, pending: concat(t.hi.pending, g.pending)}
"#;
    let v = 跑(
        程序,
        &[
            ("N0", "N1", 0.95),
            ("N1", "N3", 0.95),
            ("N0", "N2", 0.95),
            ("N2", "N3", 0.5),
        ],
    );
    assert_eq!(v["differs"], j("[[2, 3]]"), "{v}");
    assert_eq!(v["same"], j("false"));
    assert_eq!(v["unique"], j("false"));
    assert_eq!((v["lo"].clone(), v["hi"].clone()), (j("1"), j("2")));
}

/// 不可达（没跑成或没有解）时不出错：`tie_count` 为 0，`unique` 为假（没有解谈不上唯一）。
#[test]
fn 无解时不出错() {
    let 程序 = r#"budget {calls: 20, cost: 1, depth: 64};
let ns = map(range(0, 3), fn(i) { mat(join(["N", text(i)], "")) });
let g = judged_graph(ns, test("这两方能合作吗？", "edge"), {prune: fn(ns) { [[0, 1]] }});
let t = interval(g, "shortest_path", {source: 0, target: 2});
{same: t.same, unique: t.unique, lo: t.tie_count_lo, hi: t.tie_count_hi, complete: t.complete}
"#;
    let v = 跑(程序, &[("N0", "N1", 0.95)]);
    assert_eq!((v["lo"].clone(), v["hi"].clone()), (j("0"), j("0")), "{v}");
    assert_eq!(v["complete"], j("true"));
    assert_eq!(v["unique"], j("true"), "两次都没有并列可漏：{v}");
}
