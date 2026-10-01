//! Z0223：二分图匹配并列枚举在 162x163 规模上的耗时与结果指纹；随机中等规模图对拍暴力枚举。
//!
//! 预注册：`地基/过程记录/工程-Z0223-图匹配耗时.md`「预注册」一节。
//! 输入与 `actions_graph.rs::matching_bipartite_325_timing` 逐字相同（种子 325、162×163、密度 0.15、权 1..20），
//! 这里多做一件事：把 `tie_count`、`ties_complete` 与全部并列解的指纹打出来并钉住，保证「改快了」不改并列语义。
//! 随机数用手写 xorshift64，固定种子。
#![allow(clippy::needless_range_loop)]

use std::collections::BTreeSet;
use std::time::Instant;

use jpp::interp::{ActionRegistry, json_to_value};
use serde_json::{Value as Json, json};

fn call(name: &str, input: Json) -> Json {
    let mut a = ActionRegistry::new();
    jpp::actions::register_all(&mut a, &jpp::actions::Ctx::default(), false);
    let action = a
        .actions
        .get(name)
        .unwrap_or_else(|| panic!("{name} 未登记"));
    (action.f)(&[json_to_value(&input)])
        .unwrap_or_else(|e| panic!("{name}: {e}"))
        .to_json()
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
    fn chance(&mut self, p: f64) -> bool {
        (self.next_u64() % 1_000_000) as f64 / 1_000_000.0 < p
    }
}

fn pair_set(t: &Json) -> BTreeSet<(String, String)> {
    t["pairs"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| {
            (
                p["u"].as_str().unwrap().to_string(),
                p["v"].as_str().unwrap().to_string(),
            )
        })
        .collect()
}

/// 并列解列表的指纹：每个解排序后的 (u,v) 串，按输出顺序（顺序也是语义：主解第一、其余按枚举序）逐个 FNV-1a。
fn fingerprint(out: &Json) -> u64 {
    let mut h: u64 = 0xcbf29ce484222325;
    for t in out["ties"].as_array().unwrap() {
        for (u, v) in pair_set(t) {
            for b in u.bytes().chain([b'-']).chain(v.bytes()).chain([b';']) {
                h ^= b as u64;
                h = h.wrapping_mul(0x100000001b3);
            }
        }
        h ^= b'|' as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    h
}

#[test]
fn matching_162x163_并列结果与耗时() {
    let mut rng = Rng::new(325);
    let (n, m) = (162usize, 163usize);
    let left: Vec<String> = (0..n).map(|i| format!("L{i}")).collect();
    let right: Vec<String> = (0..m).map(|i| format!("R{i}")).collect();
    let mut edges = Vec::new();
    for i in 0..n {
        for j in 0..m {
            if rng.chance(0.15) {
                edges.push(json!({"u": left[i], "v": right[j], "w": 1 + rng.range(20)}));
            }
        }
    }
    let input = json!({"edges": edges, "bipartite": true, "parts": [left, right]});
    let start = Instant::now();
    let out = call("graph:matching", input);
    let el = start.elapsed();
    let ties = out["ties"].as_array().unwrap();
    let fp = fingerprint(&out);
    eprintln!(
        "[z0223] 162x163 elapsed={el:?} tie_count={} complete={} total_weight={} fingerprint={fp:#x}",
        out["tie_count"], out["ties_complete"], out["total_weight"]
    );
    // 形状：tie_count == ties.len()，主解排第一，各解两两不同、总权同一个数
    assert_eq!(out["tie_count"].as_u64().unwrap() as usize, ties.len());
    assert_eq!(pair_set(&ties[0]), pair_set(&out));
    let sets: BTreeSet<_> = ties.iter().map(pair_set).collect();
    assert_eq!(sets.len(), ties.len(), "并列解两两不同");
    let w0 = out["total_weight"].as_f64().unwrap();
    for t in ties {
        let w = t["total_weight"].as_f64().unwrap();
        assert!(
            (w - w0).abs() <= 1e-9 * w0.abs(),
            "并列解总权应同为 {w0}，得 {w}"
        );
    }
    assert!(
        el.as_secs_f64() < 20.0,
        "162x163 匹配（含并列枚举）耗时 {el:?}"
    );
    if let Some((want_count, want_complete, want_fp)) = EXPECTED_162X163 {
        assert_eq!(out["tie_count"].as_u64().unwrap(), want_count);
        assert_eq!(out["ties_complete"].as_bool().unwrap(), want_complete);
        assert_eq!(fp, want_fp, "并列解的内容或顺序变了");
    }
}

/// C-8 之后、Z0223 改动之前实测的（并列个数，是否列全，指纹）。改动后必须逐字相同。
const EXPECTED_162X163: Option<(u64, bool, u64)> = Some((34, true, 0xb70718231b7b15df));

/// 暴力：枚举全部正权边子集里两两不相邻的匹配，返回最大总权与全部最大权匹配。
fn brute(edges: &[(usize, usize, f64)]) -> (f64, BTreeSet<BTreeSet<(usize, usize)>>) {
    fn rec(
        i: usize,
        edges: &[(usize, usize, f64)],
        used_l: &mut BTreeSet<usize>,
        used_r: &mut BTreeSet<usize>,
        cur: &mut BTreeSet<(usize, usize)>,
        w: f64,
        best: &mut (f64, BTreeSet<BTreeSet<(usize, usize)>>),
    ) {
        if i == edges.len() {
            if w > best.0 + 1e-9 {
                best.0 = w;
                best.1.clear();
                best.1.insert(cur.clone());
            } else if (w - best.0).abs() <= 1e-9 {
                best.1.insert(cur.clone());
            }
            return;
        }
        rec(i + 1, edges, used_l, used_r, cur, w, best);
        let (a, b, ew) = edges[i];
        if ew > 0.0 && !used_l.contains(&a) && !used_r.contains(&b) {
            used_l.insert(a);
            used_r.insert(b);
            cur.insert((a, b));
            rec(i + 1, edges, used_l, used_r, cur, w + ew, best);
            cur.remove(&(a, b));
            used_l.remove(&a);
            used_r.remove(&b);
        }
    }
    let mut best = (0.0, BTreeSet::new());
    rec(
        0,
        edges,
        &mut BTreeSet::new(),
        &mut BTreeSet::new(),
        &mut BTreeSet::new(),
        0.0,
        &mut best,
    );
    (best.0, best.1)
}

fn cross_check(seed: u64, cases: usize, max_l: usize, max_r: usize, density: f64, wmax: usize) {
    let mut rng = Rng::new(seed);
    let mut with_ties = 0;
    for case in 0..cases {
        let (nl, nr) = (1 + rng.range(max_l), 1 + rng.range(max_r));
        let mut edges = vec![];
        for a in 0..nl {
            for b in 0..nr {
                if rng.chance(density) {
                    edges.push((a, b, (1 + rng.range(wmax)) as f64));
                }
            }
        }
        if edges.is_empty() || edges.len() > 22 {
            continue;
        }
        let inp = json!({
            "bipartite": true,
            "parts": [(0..nl).map(|i| format!("l{i}")).collect::<Vec<_>>(),
                      (0..nr).map(|i| format!("r{i}")).collect::<Vec<_>>()],
            "edges": edges.iter().map(|(a, b, w)| json!({"u": format!("l{a}"), "v": format!("r{b}"), "w": w})).collect::<Vec<_>>(),
        });
        let out = call("graph:matching", inp);
        let (bw, bs) = brute(&edges);
        assert_eq!(out["ties_complete"], true, "seed {seed} case {case}");
        assert!(
            (out["total_weight"].as_f64().unwrap() - bw).abs() < 1e-9,
            "seed {seed} case {case}"
        );
        let got: BTreeSet<BTreeSet<(String, String)>> = out["ties"]
            .as_array()
            .unwrap()
            .iter()
            .map(pair_set)
            .collect();
        let want: BTreeSet<BTreeSet<(String, String)>> = bs
            .iter()
            .map(|s| {
                s.iter()
                    .map(|(a, b)| (format!("l{a}"), format!("r{b}")))
                    .collect()
            })
            .collect();
        assert_eq!(
            out["tie_count"].as_u64().unwrap() as usize,
            want.len(),
            "seed {seed} case {case} edges={edges:?}"
        );
        assert_eq!(got, want, "seed {seed} case {case} edges={edges:?}");
        if want.len() > 1 {
            with_ties += 1;
        }
    }
    assert!(
        with_ties >= cases / 10,
        "seed {seed}: 有并列的例太少 {with_ties}"
    );
}

#[test]
fn 随机对拍_二分图匹配_行少于列() {
    cross_check(0x0022_2301, 300, 4, 7, 0.55, 3);
}

#[test]
fn 随机对拍_二分图匹配_行多于列() {
    cross_check(0x0022_2302, 300, 7, 4, 0.55, 3);
}

#[test]
fn 随机对拍_二分图匹配_全等权稠密() {
    cross_check(0x0022_2303, 200, 5, 5, 0.6, 1);
}

#[test]
fn 随机对拍_二分图匹配_稀疏多权() {
    cross_check(0x0022_2304, 300, 6, 6, 0.35, 5);
}
