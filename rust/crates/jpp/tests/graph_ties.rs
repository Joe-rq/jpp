//! C-8（主控 Z0174）：`graph:*` 动作报并列解——构造并列的图，逐算法测试；随机小图对拍暴力枚举。
//!
//! 预注册（代码之前登记的预测）：`地基/过程记录/工程-C7C8-动作表.md` §5.3（P8-1 至 P8-6）。
//! 形态：原有字段不变，新增 `tie_count`、`ties`（主解第一）、`ties_complete`；`max_flow` 只加 `flow_unique`；
//! 枚举上限 64，超过如实报 `ties_complete=false`。随机数用手写 xorshift64，固定种子，失败可复现。
#![allow(clippy::needless_range_loop)]

use std::collections::BTreeSet;

use jpp::interp::{ActionRegistry, json_to_value};
use serde_json::{Value as Json, json};

fn registry() -> ActionRegistry {
    let mut a = ActionRegistry::new();
    jpp::actions::register_all(&mut a, &jpp::actions::Ctx::default(), false);
    a
}

fn call(name: &str, input: Json) -> Json {
    let a = registry();
    let action = a
        .actions
        .get(name)
        .unwrap_or_else(|| panic!("{name} 未登记"));
    let out = (action.f)(&[json_to_value(&input)]).unwrap_or_else(|e| panic!("{name}: {e}"));
    out.to_json()
}

struct Rng(u64);
impl Rng {
    fn new(seed: u64) -> Self {
        Rng(seed | 1)
    }
    fn next_u64(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }
    fn range(&mut self, n: usize) -> usize {
        (self.next_u64() as usize) % n
    }
    fn chance(&mut self, num: u64, den: u64) -> bool {
        self.next_u64() % den < num
    }
}

fn tc(out: &Json) -> u64 {
    out["tie_count"].as_u64().expect("有 tie_count")
}
fn complete(out: &Json) -> bool {
    out["ties_complete"].as_bool().expect("有 ties_complete")
}
fn ties(out: &Json) -> &Vec<Json> {
    out["ties"].as_array().expect("有 ties")
}

/// 通用形状断言：`tie_count == ties.len()`，主解（原有字段）等于 `ties[0]` 的同名字段。
fn check_shape(out: &Json) {
    assert_eq!(tc(out) as usize, ties(out).len(), "{out}");
    for (k, v) in ties(out)[0].as_object().unwrap() {
        assert_eq!(&out[k], v, "主解字段 {k} 应等于 ties[0] 的同名字段：{out}");
    }
}

// ---------- max_clique ----------

/// 案例 02 的最小复现：四个人，只有「甲—乙」「丙—丁」两条边，两个同样大的最大团。P8-2。
#[test]
fn max_clique_两个并列团() {
    let out = call(
        "graph:max_clique",
        json!({"nodes": ["甲", "乙", "丙", "丁"],
               "edges": [{"u": "甲", "v": "乙"}, {"u": "丙", "v": "丁"}]}),
    );
    assert_eq!(tc(&out), 2, "{out}");
    assert!(complete(&out));
    check_shape(&out);
    let cliques: BTreeSet<Vec<String>> = ties(&out)
        .iter()
        .map(|t| {
            t["clique"]
                .as_array()
                .unwrap()
                .iter()
                .map(|x| x.as_str().unwrap().to_string())
                .collect()
        })
        .collect();
    assert_eq!(cliques.len(), 2);
    // 每个并列团都带自己用到的边下标：甲乙 → [0]，丙丁 → [1]
    let idx: BTreeSet<Vec<u64>> = ties(&out)
        .iter()
        .map(|t| {
            t["edge_indices"]
                .as_array()
                .unwrap()
                .iter()
                .map(|x| x.as_u64().unwrap())
                .collect()
        })
        .collect();
    assert_eq!(idx, BTreeSet::from([vec![0u64], vec![1u64]]));
}

/// 唯一最大团：`tie_count=1`，字段齐全。
#[test]
fn max_clique_唯一最大团() {
    let out = call(
        "graph:max_clique",
        json!({"edges": [{"u": "a", "v": "b"}, {"u": "b", "v": "c"}, {"u": "a", "v": "c"},
                          {"u": "c", "v": "d"}, {"u": "d", "v": "e"}]}),
    );
    assert_eq!(tc(&out), 1);
    assert!(complete(&out));
    assert_eq!(out["size"], 3);
    check_shape(&out);
}

/// 完全多部图：k 组各两个互不相连的点、组间全连，最大团每组取一个，共 2^k 个。
fn multipartite(k: usize) -> Json {
    let mut edges = vec![];
    for a in 0..2 * k {
        for b in (a + 1)..2 * k {
            if a / 2 != b / 2 {
                edges.push(json!({"u": format!("n{a}"), "v": format!("n{b}")}));
            }
        }
    }
    json!({"nodes": (0..2 * k).map(|i| format!("n{i}")).collect::<Vec<_>>(), "edges": edges})
}

#[test]
fn max_clique_并列数在上限内与超过上限() {
    // k=5：32 个，全部列出
    let out = call("graph:max_clique", multipartite(5));
    assert_eq!(tc(&out), 32);
    assert!(complete(&out));
    check_shape(&out);
    // k=7：128 个 > 64，只列 64 个，如实报不完整（不静默截断）
    let out = call("graph:max_clique", multipartite(7));
    assert_eq!(tc(&out), 64, "上限 64");
    assert!(!complete(&out), "超过上限必须报 ties_complete=false");
    assert_eq!(ties(&out).len(), 64);
    check_shape(&out);
    // 64 个互不相同
    let all: BTreeSet<String> = ties(&out).iter().map(|t| t["clique"].to_string()).collect();
    assert_eq!(all.len(), 64);
}

/// 恰好 64 个（k=6）：全列，`ties_complete=true`（边界：上限本身不算溢出）。
#[test]
fn max_clique_恰好64个不算溢出() {
    let out = call("graph:max_clique", multipartite(6));
    assert_eq!(tc(&out), 64);
    assert!(
        complete(&out),
        "恰好 64 个是列全了：{}",
        out["ties_complete"]
    );
}

// ---------- set_cover ----------

/// 案例 03 的最小复现：两个等价集合。P8-2。
#[test]
fn set_cover_两组等价覆盖() {
    let out = call(
        "graph:set_cover",
        json!({"universe": ["e1", "e2"],
               "sets": [{"id": "s1", "elements": ["e1", "e2"]}, {"id": "s2", "elements": ["e1", "e2"]}]}),
    );
    assert_eq!(out["chosen"], json!(["s1"]));
    assert_eq!(tc(&out), 2, "{out}");
    assert!(complete(&out));
    check_shape(&out);
    let chosen: BTreeSet<String> = ties(&out).iter().map(|t| t["chosen"].to_string()).collect();
    assert_eq!(
        chosen,
        BTreeSet::from([r#"["s1"]"#.to_string(), r#"["s2"]"#.to_string()])
    );
}

/// 更像案例 03 的：两组各 6 篇的书单并列（其中一篇二选一），总成本相同。
#[test]
fn set_cover_一篇二选一() {
    let mut sets = vec![];
    for i in 0..5 {
        sets.push(json!({"id": format!("k{i}"), "elements": [format!("c{i}")]}));
    }
    sets.push(json!({"id": "CAP", "elements": ["c5"]}));
    sets.push(json!({"id": "PACELC", "elements": ["c5"]}));
    let out = call(
        "graph:set_cover",
        json!({"universe": (0..6).map(|i| format!("c{i}")).collect::<Vec<_>>(), "sets": sets}),
    );
    assert_eq!(tc(&out), 2, "{out}");
    assert_eq!(out["total_cost"], 6.0);
}

/// 成本不同就不并列；零成本的冗余集合不算另一种覆盖。
#[test]
fn set_cover_不同成本不并列_零成本冗余不算() {
    let out = call(
        "graph:set_cover",
        json!({"universe": ["e1"], "sets": [{"id": "a", "elements": ["e1"], "cost": 1},
                                             {"id": "b", "elements": ["e1"], "cost": 2}]}),
    );
    assert_eq!(tc(&out), 1);
    let out = call(
        "graph:set_cover",
        json!({"universe": ["e1"], "sets": [{"id": "a", "elements": ["e1"], "cost": 1},
                                             {"id": "z", "elements": ["e1"], "cost": 0}]}),
    );
    assert_eq!(out["chosen"], json!(["z"]));
    assert_eq!(tc(&out), 1, "只有 z 一个最优（成本 0）：{out}");
}

#[test]
fn set_cover_覆盖不了返回零并列() {
    let out = call(
        "graph:set_cover",
        json!({"universe": ["e1", "e2"], "sets": [{"id": "a", "elements": ["e1"]}]}),
    );
    assert_eq!(tc(&out), 0);
    assert!(complete(&out));
    assert_eq!(ties(&out).len(), 0);
}

/// 贪心路径（宇宙 > 20）不是精确算法：只报它自己给的一个，如实标不完整。P8-3。
#[test]
fn set_cover_贪心路径标不完整() {
    let universe: Vec<String> = (0..21).map(|i| format!("e{i}")).collect();
    let sets = vec![
        json!({"id": "s1", "elements": universe}),
        json!({"id": "s2", "elements": universe}),
    ];
    let out = call(
        "graph:set_cover",
        json!({"universe": universe, "sets": sets}),
    );
    assert_eq!(out["exact"], false);
    assert_eq!(tc(&out), 1);
    assert!(!complete(&out), "贪心保证不了列全并列解");
}

#[test]
fn set_cover_并列数超过上限() {
    // 7 个元素，每个元素有两个等价集合：2^7 = 128 个最优覆盖
    let universe: Vec<String> = (0..7).map(|i| format!("e{i}")).collect();
    let mut sets = vec![];
    for i in 0..7 {
        for c in ["a", "b"] {
            sets.push(json!({"id": format!("{c}{i}"), "elements": [format!("e{i}")]}));
        }
    }
    let out = call(
        "graph:set_cover",
        json!({"universe": universe, "sets": sets}),
    );
    assert_eq!(tc(&out), 64);
    assert!(!complete(&out));
    check_shape(&out);
}

// ---------- matching ----------

/// k 组「左点配两个右点之一」，每组两条等权边：2^k 个最大权匹配。
fn pair_choice(k: usize, w: f64) -> Json {
    let mut edges = vec![];
    let mut left = vec![];
    let mut right = vec![];
    for i in 0..k {
        left.push(format!("a{i}"));
        right.push(format!("b{i}"));
        right.push(format!("c{i}"));
        edges.push(json!({"u": format!("a{i}"), "v": format!("b{i}"), "w": w}));
        edges.push(json!({"u": format!("a{i}"), "v": format!("c{i}"), "w": w}));
    }
    json!({"edges": edges, "bipartite": true, "parts": [left, right]})
}

#[test]
fn matching_二分图并列数() {
    // 1 组：2 个；3 组：8 个（P8-2）
    let out = call("graph:matching", pair_choice(1, 1.0));
    assert_eq!(tc(&out), 2);
    let out = call("graph:matching", pair_choice(3, 1.0));
    assert_eq!(tc(&out), 8, "{out}");
    assert!(complete(&out));
    check_shape(&out);
    // 每个并列匹配总权都是 3
    for t in ties(&out) {
        assert_eq!(t["total_weight"], 3.0);
    }
    // k=7：128 > 64
    let out = call("graph:matching", pair_choice(7, 1.0));
    assert_eq!(tc(&out), 64);
    assert!(!complete(&out));
    check_shape(&out);
}

/// 权重远小于 1e-9 时并列仍认得出（比较是相对的，没有绝对项）。
#[test]
fn matching_极小权重仍认并列() {
    let out = call("graph:matching", pair_choice(2, 1e-12));
    assert_eq!(tc(&out), 4, "{out}");
}

/// 唯一最优：零次额外求解、`tie_count=1`。
#[test]
fn matching_唯一最优() {
    let out = call(
        "graph:matching",
        json!({"edges": [{"u": "a", "v": "x", "w": 3}, {"u": "a", "v": "y", "w": 2},
                          {"u": "b", "v": "x", "w": 2}, {"u": "b", "v": "y", "w": 1}],
               "bipartite": true, "parts": [["a", "b"], ["x", "y"]]}),
    );
    // ax+by = 4；ay+bx = 4：并列！这是构造的并列例
    assert_eq!(tc(&out), 2, "{out}");
    let out = call(
        "graph:matching",
        json!({"edges": [{"u": "a", "v": "x", "w": 5}, {"u": "a", "v": "y", "w": 2},
                          {"u": "b", "v": "x", "w": 2}, {"u": "b", "v": "y", "w": 1}],
               "bipartite": true, "parts": [["a", "b"], ["x", "y"]]}),
    );
    assert_eq!(tc(&out), 1, "{out}");
    assert!(complete(&out));
}

/// 一般图：三角形三条等权边，每次只能选一条：3 个并列匹配。
#[test]
fn matching_一般图三角形() {
    let out = call(
        "graph:matching",
        json!({"edges": [{"u": "a", "v": "b", "w": 1}, {"u": "b", "v": "c", "w": 1},
                          {"u": "a", "v": "c", "w": 1}]}),
    );
    assert_eq!(out["mode"], "general_exact");
    assert_eq!(tc(&out), 3, "{out}");
    assert!(complete(&out));
    check_shape(&out);
}

/// 0.1 + 0.2 与 0.3 差一个浮点误差，按相对容差认作并列。
#[test]
fn matching_浮点误差内认并列() {
    let out = call(
        "graph:matching",
        json!({"edges": [{"u": "a", "v": "b", "w": 0.1}, {"u": "c", "v": "d", "w": 0.2},
                          {"u": "a", "v": "c", "w": 0.3}]}),
    );
    assert_eq!(tc(&out), 2, "{out}");
}

/// 有 `size` 截断时如实标不完整（边界上同权的边取谁也是并列来源）。
#[test]
fn matching_有截断时标不完整() {
    let mut inp = pair_choice(2, 1.0);
    inp["size"] = json!(1);
    let out = call("graph:matching", inp);
    assert!(!complete(&out), "{out}");
    assert_eq!(out["pairs"].as_array().unwrap().len(), 1);
}

// ---------- shortest_path ----------

#[test]
fn shortest_path_菱形两条同长路径() {
    let out = call(
        "graph:shortest_path",
        json!({"edges": [{"u": "s", "v": "a", "w": 1}, {"u": "a", "v": "t", "w": 1},
                          {"u": "s", "v": "b", "w": 1}, {"u": "b", "v": "t", "w": 1}],
               "source": "s", "target": "t", "directed": true}),
    );
    assert_eq!(out["distance"], 2.0);
    assert_eq!(tc(&out), 2, "{out}");
    assert!(complete(&out));
    check_shape(&out);
    let paths: BTreeSet<String> = ties(&out).iter().map(|t| t["path"].to_string()).collect();
    assert_eq!(paths.len(), 2);
}

/// 平行边：同一对节点间两条等权边，走哪条是不同的解（用到的边不同）。
#[test]
fn shortest_path_平行边算并列() {
    let out = call(
        "graph:shortest_path",
        json!({"edges": [{"u": "s", "v": "t", "w": 1}, {"u": "s", "v": "t", "w": 1}],
               "source": "s", "target": "t"}),
    );
    assert_eq!(tc(&out), 2, "{out}");
}

#[test]
fn shortest_path_唯一与不可达与起终点相同() {
    let out = call(
        "graph:shortest_path",
        json!({"edges": [{"u": "s", "v": "a", "w": 1}, {"u": "a", "v": "t", "w": 1},
                          {"u": "s", "v": "t", "w": 5}], "source": "s", "target": "t"}),
    );
    assert_eq!(tc(&out), 1);
    let out = call(
        "graph:shortest_path",
        json!({"nodes": ["s", "t"], "edges": [], "source": "s", "target": "t"}),
    );
    assert_eq!(out["reachable"], false);
    assert_eq!(tc(&out), 0);
    assert!(complete(&out));
    let out = call(
        "graph:shortest_path",
        json!({"nodes": ["s"], "edges": [], "source": "s", "target": "s"}),
    );
    assert_eq!(tc(&out), 1, "{out}");
}

/// 零权环：不会把同一条简单路径重复算成不同解。
#[test]
fn shortest_path_零权环不重复计数() {
    let out = call(
        "graph:shortest_path",
        json!({"edges": [{"u": "s", "v": "a", "w": 0}, {"u": "a", "v": "b", "w": 0},
                          {"u": "b", "v": "s", "w": 0}, {"u": "b", "v": "t", "w": 1}],
               "source": "s", "target": "t"}),
    );
    // s→a→b→t 与 s→b→t（b 也与 s 零权相连）：两条简单最短路
    assert_eq!(tc(&out), 2, "{out}");
}

#[test]
fn shortest_path_并列数超过上限() {
    // 7 段，每段两条平行等权边：2^7 = 128
    let mut edges = vec![];
    for i in 0..7 {
        for _ in 0..2 {
            edges.push(json!({"u": format!("n{i}"), "v": format!("n{}", i + 1), "w": 1}));
        }
    }
    let out = call(
        "graph:shortest_path",
        json!({"edges": edges, "source": "n0", "target": "n7", "directed": true}),
    );
    assert_eq!(tc(&out), 64);
    assert!(!complete(&out));
    check_shape(&out);
}

// ---------- components ----------

#[test]
fn components_恒唯一() {
    let out = call(
        "graph:components",
        json!({"nodes": ["a", "b", "c", "d"],
               "edges": [{"u": "a", "v": "b"}, {"u": "c", "v": "d"}]}),
    );
    assert_eq!(tc(&out), 1);
    assert!(complete(&out));
    check_shape(&out);
}

// ---------- max_flow ----------

#[test]
fn max_flow_分配唯一与不唯一() {
    // 单一路径：唯一（P8-4）
    let out = call(
        "graph:max_flow",
        json!({"edges": [{"u": "s", "v": "a", "cap": 5}, {"u": "a", "v": "t", "cap": 3}],
               "source": "s", "sink": "t"}),
    );
    assert_eq!(out["max_flow"], 3.0);
    assert_eq!(out["flow_unique"], true, "{out}");
    // 瓶颈边后接两条等容量并行路：流走哪条都行，分配不唯一
    let out = call(
        "graph:max_flow",
        json!({"edges": [{"u": "s", "v": "m", "cap": 1}, {"u": "m", "v": "a", "cap": 1},
                          {"u": "m", "v": "b", "cap": 1}, {"u": "a", "v": "t", "cap": 1},
                          {"u": "b", "v": "t", "cap": 1}],
               "source": "s", "sink": "t"}),
    );
    assert_eq!(out["max_flow"], 1.0);
    assert_eq!(out["flow_unique"], false, "{out}");
    // 平行边：一条边上的流可以挪到另一条平行边上
    let out = call(
        "graph:max_flow",
        json!({"edges": [{"u": "s", "v": "t", "cap": 1}, {"u": "s", "v": "t", "cap": 1}],
               "source": "s", "sink": "t"}),
    );
    assert_eq!(out["max_flow"], 2.0);
    assert_eq!(out["flow_unique"], true, "两条都满流，只有一种分配：{out}");
    let out = call(
        "graph:max_flow",
        json!({"edges": [{"u": "s", "v": "m", "cap": 1}, {"u": "m", "v": "t", "cap": 1},
                          {"u": "m", "v": "t", "cap": 1}],
               "source": "s", "sink": "t"}),
    );
    assert_eq!(out["flow_unique"], false, "流可走任一条平行边：{out}");
}

/// 规模：3 个需求 × 325 人，全部等权（并列多到爆）：应在合理时间内给出 64 个并列并报不完整。
#[test]
fn matching_3x325全等权并列不拖慢() {
    let right: Vec<String> = (0..325).map(|i| format!("p{i}")).collect();
    let mut edges = vec![];
    for a in 0..3 {
        for r in &right {
            edges.push(json!({"u": format!("q{a}"), "v": r, "w": 1}));
        }
    }
    let start = std::time::Instant::now();
    let out = call(
        "graph:matching",
        json!({"edges": edges, "bipartite": true, "parts": [["q0", "q1", "q2"], right]}),
    );
    let el = start.elapsed();
    eprintln!("3x325 全等权：{el:?}");
    assert_eq!(tc(&out), 64);
    assert!(!complete(&out));
    check_shape(&out);
    assert!(el.as_secs() < 60, "并列枚举不应拖到分钟级：{el:?}");
}

// ---------- 随机小图对拍暴力枚举（P8-5） ----------

fn canon_pairs(v: &Json) -> BTreeSet<(String, String)> {
    v["pairs"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| {
            let (u, w) = (p["u"].as_str().unwrap(), p["v"].as_str().unwrap());
            if u < w {
                (u.to_string(), w.to_string())
            } else {
                (w.to_string(), u.to_string())
            }
        })
        .collect()
}

/// 暴力：所有由正权边组成的匹配，取最大总权者。`edges` 是 (u, v, w)，节点是整数。
fn brute_matchings(edges: &[(usize, usize, f64)]) -> (f64, Vec<BTreeSet<(usize, usize)>>) {
    fn rec(
        i: usize,
        edges: &[(usize, usize, f64)],
        used: &mut Vec<bool>,
        cur: &mut Vec<(usize, usize)>,
        w: f64,
        best: &mut f64,
        all: &mut Vec<(f64, BTreeSet<(usize, usize)>)>,
    ) {
        if i == edges.len() {
            if w > *best {
                *best = w;
            }
            all.push((w, cur.iter().cloned().collect()));
            return;
        }
        rec(i + 1, edges, used, cur, w, best, all);
        let (u, v, ew) = edges[i];
        if ew > 0.0 && !used[u] && !used[v] {
            used[u] = true;
            used[v] = true;
            cur.push((u.min(v), u.max(v)));
            rec(i + 1, edges, used, cur, w + ew, best, all);
            cur.pop();
            used[u] = false;
            used[v] = false;
        }
    }
    let n = edges.iter().map(|e| e.0.max(e.1) + 1).max().unwrap_or(0);
    let mut best = 0.0;
    let mut all = vec![];
    rec(
        0,
        edges,
        &mut vec![false; n],
        &mut vec![],
        0.0,
        &mut best,
        &mut all,
    );
    let sets = all
        .into_iter()
        .filter(|(w, _)| (*w - best).abs() < 1e-9)
        .map(|(_, s)| s)
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    (best, sets)
}

#[test]
fn 随机对拍_一般图匹配() {
    let mut rng = Rng::new(0xC8_01);
    let mut with_ties = 0;
    for case in 0..150 {
        let n = 2 + rng.range(6);
        let mut edges = vec![];
        let mut seen = BTreeSet::new();
        for a in 0..n {
            for b in (a + 1)..n {
                if rng.chance(1, 2) {
                    edges.push((a, b, (1 + rng.range(3)) as f64));
                    seen.insert((a, b));
                }
            }
        }
        if edges.is_empty() {
            continue;
        }
        let inp = json!({
            "nodes": (0..n).map(|i| format!("n{i}")).collect::<Vec<_>>(),
            "edges": edges.iter().map(|(a, b, w)| json!({"u": format!("n{a}"), "v": format!("n{b}"), "w": w})).collect::<Vec<_>>(),
        });
        let out = call("graph:matching", inp);
        let (_, brute) = brute_matchings(&edges);
        assert!(complete(&out), "case {case}");
        assert_eq!(
            tc(&out) as usize,
            brute.len(),
            "case {case} {edges:?} {out}"
        );
        // 主解排第一，且每个并列都是合法最优
        check_shape(&out);
        let got: BTreeSet<BTreeSet<(String, String)>> =
            ties(&out).iter().map(canon_pairs).collect();
        let want: BTreeSet<BTreeSet<(String, String)>> = brute
            .iter()
            .map(|s| {
                s.iter()
                    .map(|(a, b)| (format!("n{a}"), format!("n{b}")))
                    .collect()
            })
            .collect();
        assert_eq!(got, want, "case {case}");
        if brute.len() > 1 {
            with_ties += 1;
        }
    }
    assert!(with_ties >= 20, "对拍集里应有足够多的并列例：{with_ties}");
}

#[test]
fn 随机对拍_二分图匹配() {
    let mut rng = Rng::new(0xC8_02);
    let mut with_ties = 0;
    for case in 0..150 {
        let (nl, nr) = (1 + rng.range(4), 1 + rng.range(4));
        let mut edges = vec![];
        for a in 0..nl {
            for b in 0..nr {
                if rng.chance(3, 5) {
                    edges.push((a, nl + b, (1 + rng.range(3)) as f64));
                }
            }
        }
        if edges.is_empty() {
            continue;
        }
        let inp = json!({
            "bipartite": true,
            "parts": [(0..nl).map(|i| format!("n{i}")).collect::<Vec<_>>(),
                      (nl..nl + nr).map(|i| format!("n{i}")).collect::<Vec<_>>()],
            "edges": edges.iter().map(|(a, b, w)| json!({"u": format!("n{a}"), "v": format!("n{b}"), "w": w})).collect::<Vec<_>>(),
        });
        let out = call("graph:matching", inp);
        let (_, brute) = brute_matchings(&edges);
        assert!(complete(&out), "case {case}");
        assert_eq!(
            tc(&out) as usize,
            brute.len(),
            "case {case} {edges:?} {out}"
        );
        check_shape(&out);
        let got: BTreeSet<BTreeSet<(String, String)>> =
            ties(&out).iter().map(canon_pairs).collect();
        let want: BTreeSet<BTreeSet<(String, String)>> = brute
            .iter()
            .map(|s| {
                s.iter()
                    .map(|(a, b)| (format!("n{a}"), format!("n{b}")))
                    .collect()
            })
            .collect();
        assert_eq!(got, want, "case {case}");
        if brute.len() > 1 {
            with_ties += 1;
        }
    }
    assert!(with_ties >= 20, "{with_ties}");
}

#[test]
fn 随机对拍_最大团() {
    let mut rng = Rng::new(0xC8_03);
    let mut with_ties = 0;
    for case in 0..150 {
        let n = 1 + rng.range(8);
        let mut adj = vec![vec![false; n]; n];
        let mut edges = vec![];
        for a in 0..n {
            for b in (a + 1)..n {
                if rng.chance(1, 2) {
                    adj[a][b] = true;
                    adj[b][a] = true;
                    edges.push(json!({"u": format!("n{a}"), "v": format!("n{b}")}));
                }
            }
        }
        // 暴力：所有最大团
        let mut best = 0;
        let mut all: Vec<Vec<usize>> = vec![];
        for mask in 1u32..(1 << n) {
            let vs: Vec<usize> = (0..n).filter(|i| mask & (1 << i) != 0).collect();
            if vs.iter().all(|&a| vs.iter().all(|&b| a == b || adj[a][b])) {
                if vs.len() > best {
                    best = vs.len();
                    all.clear();
                }
                if vs.len() == best {
                    all.push(vs);
                }
            }
        }
        let out = call(
            "graph:max_clique",
            json!({"nodes": (0..n).map(|i| format!("n{i}")).collect::<Vec<_>>(), "edges": edges}),
        );
        assert_eq!(tc(&out) as usize, all.len(), "case {case} {out}");
        assert!(complete(&out));
        check_shape(&out);
        let got: BTreeSet<Vec<String>> = ties(&out)
            .iter()
            .map(|t| {
                let mut c: Vec<String> = t["clique"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|x| x.as_str().unwrap().to_string())
                    .collect();
                c.sort();
                c
            })
            .collect();
        let want: BTreeSet<Vec<String>> = all
            .iter()
            .map(|c| {
                let mut v: Vec<String> = c.iter().map(|i| format!("n{i}")).collect();
                v.sort();
                v
            })
            .collect();
        assert_eq!(got, want, "case {case}");
        if all.len() > 1 {
            with_ties += 1;
        }
    }
    assert!(with_ties >= 20, "{with_ties}");
}

#[test]
fn 随机对拍_最短路() {
    let mut rng = Rng::new(0xC8_04);
    let mut with_ties = 0;
    for case in 0..150 {
        let n = 2 + rng.range(5);
        let directed = rng.chance(1, 2);
        let mut edges = vec![];
        for a in 0..n {
            for b in 0..n {
                if a == b || (!directed && b < a) {
                    continue;
                }
                if rng.chance(1, 2) {
                    edges.push((a, b, rng.range(3) as f64)); // 含零权
                    if rng.chance(1, 5) {
                        edges.push((a, b, rng.range(3) as f64)); // 平行边
                    }
                }
            }
        }
        let (s, t) = (0usize, n - 1);
        // 暴力：所有简单路径（按边下标序列区分）
        let mut best = f64::INFINITY;
        let mut all: Vec<(f64, Vec<usize>)> = vec![];
        fn dfs(
            u: usize,
            t: usize,
            edges: &[(usize, usize, f64)],
            directed: bool,
            on: &mut Vec<bool>,
            es: &mut Vec<usize>,
            w: f64,
            all: &mut Vec<(f64, Vec<usize>)>,
        ) {
            if u == t {
                all.push((w, es.clone()));
                return;
            }
            for (i, &(a, b, ew)) in edges.iter().enumerate() {
                let next = if a == u {
                    Some(b)
                } else if !directed && b == u {
                    Some(a)
                } else {
                    None
                };
                if let Some(v) = next
                    && !on[v]
                {
                    on[v] = true;
                    es.push(i);
                    dfs(v, t, edges, directed, on, es, w + ew, all);
                    es.pop();
                    on[v] = false;
                }
            }
        }
        let mut on = vec![false; n];
        on[s] = true;
        dfs(s, t, &edges, directed, &mut on, &mut vec![], 0.0, &mut all);
        for (w, _) in &all {
            if *w < best {
                best = *w;
            }
        }
        let want: BTreeSet<Vec<usize>> = all
            .iter()
            .filter(|(w, _)| (*w - best).abs() < 1e-9)
            .map(|(_, e)| e.clone())
            .collect();
        let inp = json!({
            "nodes": (0..n).map(|i| format!("n{i}")).collect::<Vec<_>>(),
            "edges": edges.iter().map(|(a, b, w)| json!({"u": format!("n{a}"), "v": format!("n{b}"), "w": w})).collect::<Vec<_>>(),
            "source": format!("n{s}"), "target": format!("n{t}"), "directed": directed,
        });
        let out = call("graph:shortest_path", inp);
        if all.is_empty() {
            assert_eq!(out["reachable"], false);
            assert_eq!(tc(&out), 0);
            continue;
        }
        assert!(complete(&out), "case {case}");
        assert_eq!(
            tc(&out) as usize,
            want.len(),
            "case {case} {edges:?} directed={directed} {out}"
        );
        check_shape(&out);
        let got: BTreeSet<Vec<usize>> = ties(&out)
            .iter()
            .map(|t| {
                t["path_edges"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|e| e["edge_index"].as_u64().unwrap() as usize)
                    .collect()
            })
            .collect();
        assert_eq!(got, want, "case {case}");
        if want.len() > 1 {
            with_ties += 1;
        }
    }
    assert!(with_ties >= 15, "{with_ties}");
}

#[test]
fn 随机对拍_集合覆盖() {
    let mut rng = Rng::new(0xC8_05);
    let mut with_ties = 0;
    for case in 0..150 {
        let u = 1 + rng.range(5);
        let ns = 2 + rng.range(6);
        let mut sets: Vec<(Vec<usize>, f64)> = vec![];
        for _ in 0..ns {
            let el: Vec<usize> = (0..u).filter(|_| rng.chance(1, 2)).collect();
            sets.push((el, (1 + rng.range(2)) as f64));
        }
        // 暴力：所有覆盖全集的、无冗余的集合选择，取最小成本者
        let cover = |pick: &[usize]| -> bool {
            (0..u).all(|e| pick.iter().any(|&i| sets[i].0.contains(&e)))
        };
        let mut best = f64::INFINITY;
        let mut all: Vec<(f64, Vec<usize>)> = vec![];
        for mask in 0u32..(1 << ns) {
            let pick: Vec<usize> = (0..ns).filter(|i| mask & (1 << i) != 0).collect();
            if !cover(&pick) {
                continue;
            }
            let w: f64 = pick.iter().map(|&i| sets[i].1).sum();
            if w < best {
                best = w;
            }
            all.push((w, pick));
        }
        let inp = json!({
            "universe": (0..u).map(|i| format!("e{i}")).collect::<Vec<_>>(),
            "sets": sets.iter().enumerate().map(|(i, (el, c))| json!({"id": format!("s{i}"), "elements": el.iter().map(|e| format!("e{e}")).collect::<Vec<_>>(), "cost": c})).collect::<Vec<_>>(),
        });
        let out = call("graph:set_cover", inp);
        if all.is_empty() {
            assert_eq!(tc(&out), 0, "case {case}");
            continue;
        }
        // 最优里的无冗余覆盖（成本为正时最优覆盖必然无冗余）
        let want: BTreeSet<Vec<usize>> = all
            .iter()
            .filter(|(w, _)| (*w - best).abs() < 1e-9)
            .map(|(_, p)| p.clone())
            .collect();
        assert!(complete(&out), "case {case}");
        assert_eq!(tc(&out) as usize, want.len(), "case {case} {sets:?} {out}");
        check_shape(&out);
        let got: BTreeSet<Vec<usize>> = ties(&out)
            .iter()
            .map(|t| {
                let mut v: Vec<usize> = t["chosen"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|x| x.as_str().unwrap()[1..].parse().unwrap())
                    .collect();
                v.sort_unstable();
                v
            })
            .collect();
        assert_eq!(got, want, "case {case}");
        if want.len() > 1 {
            with_ties += 1;
        }
    }
    assert!(with_ties >= 15, "{with_ties}");
}
