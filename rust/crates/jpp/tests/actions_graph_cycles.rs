//! `graph:cycles`（T0 步 39，Z0454）的对拍：随机有向小图上与本文件另写的暴力枚举结果逐项相同。暴力是「全部含
//! `through` 的 k 元子集 × 其余节点全排列逐个试环」，与共用夹具 `reference::closed_in` 同一定义，但不调夹具
//! （`rust-jpp` 不依赖夹具）、不与被测共用代码。环长取 `[2, 3]`、`[3, 4]` 与不连续的 `[2, 4]`，后者区分集合式与
//! 区间式判据（复核 `3d22eab5a` 的变异检验里只有区间式没被抓住）。另核报错与 `among` 限制。
//! 预注册：`地基/过程记录/工程-T0-夹具端口.md` §五末行、第十二节 Q3。

use jpp::interp::{ActionRegistry, json_to_value};
use serde_json::{Value as Json, json};
use std::collections::BTreeSet;

fn call(input: Json) -> Result<Json, String> {
    let mut a = ActionRegistry::new();
    jpp::actions::register_all(&mut a, &jpp::actions::Ctx::default(), false);
    let f = &a
        .actions
        .get("graph:cycles")
        .expect("graph:cycles 已登记")
        .f;
    f(&[json_to_value(&input)]).map(|v| v.to_json())
}

struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }
}

fn perms(xs: &[usize]) -> Vec<Vec<usize>> {
    if xs.is_empty() {
        return vec![vec![]];
    }
    let mut out = vec![];
    for i in 0..xs.len() {
        let mut r = xs.to_vec();
        let x = r.remove(i);
        for mut p in perms(&r) {
            p.insert(0, x);
            out.push(p);
        }
    }
    out
}

/// 暴力：含 0 号节点、其余取自 among 的 k 元子集，按 id 升序，第一个固定，其余全排列试环。
fn brute(
    n: usize,
    e: &BTreeSet<(usize, usize)>,
    among: &[usize],
    sizes: &[usize],
) -> BTreeSet<Vec<String>> {
    let name = |i: usize| format!("n{i:02}");
    let mut out = BTreeSet::new();
    let m = among.len();
    for mask in 0u32..(1 << m) {
        let mut ms: Vec<usize> = (0..m)
            .filter(|b| mask & (1 << b) != 0)
            .map(|b| among[b])
            .collect();
        ms.push(0);
        if !sizes.contains(&ms.len()) {
            continue;
        }
        let mut ids: Vec<String> = ms.iter().map(|&i| name(i)).collect();
        ids.sort();
        let order: Vec<usize> = ids.iter().map(|s| s[1..].parse().unwrap()).collect();
        let rest: Vec<usize> = (1..order.len()).collect();
        let ok = perms(&rest).into_iter().any(|p| {
            let mut o = vec![0];
            o.extend(p);
            (0..o.len()).all(|i| e.contains(&(order[o[i]], order[o[(i + 1) % o.len()]])))
        });
        if ok {
            out.insert(ids);
        }
    }
    let _ = n;
    out
}

#[test]
fn 随机小图上与暴力枚举相同() {
    let mut rng = Rng(0x9e37_79b9_7f4a_7c15);
    for round in 0..200 {
        let n = 5 + (rng.next() % 6) as usize;
        let mut e = BTreeSet::new();
        let mut edges = vec![];
        for u in 0..n {
            for v in 0..n {
                if u != v && rng.next() % 100 < 35 {
                    e.insert((u, v));
                    edges.push(json!({"u": format!("n{u:02}"), "v": format!("n{v:02}")}));
                }
            }
        }
        // among 是 1..n 的一个随机子集（至少两个）
        let among: Vec<usize> = (1..n).filter(|_| rng.next() % 100 < 75).collect();
        let sizes = match round % 3 {
            0 => vec![2, 3],
            1 => vec![3, 4],
            _ => vec![2, 4],
        };
        let got = call(json!({
            "edges": edges,
            "through": "n00",
            "among": among.iter().map(|i| format!("n{i:02}")).collect::<Vec<_>>(),
            "sizes": sizes,
        }))
        .unwrap();
        let got: BTreeSet<Vec<String>> = got["sets"]
            .as_array()
            .unwrap()
            .iter()
            .map(|s| {
                s.as_array()
                    .unwrap()
                    .iter()
                    .map(|x| x.as_str().unwrap().to_string())
                    .collect()
            })
            .collect();
        assert_eq!(got, brute(n, &e, &among, &sizes), "第 {round} 轮");
    }
}

#[test]
fn 输出按字典序并带个数() {
    let r = call(json!({
        "edges": [{"u": "a", "v": "b"}, {"u": "b", "v": "c"}, {"u": "c", "v": "a"},
                  {"u": "b", "v": "a"}, {"u": "c", "v": "d"}, {"u": "d", "v": "a"}],
        "through": "a", "sizes": [2, 3, 4]
    }))
    .unwrap();
    assert_eq!(
        r["sets"],
        json!([["a", "b"], ["a", "b", "c"], ["a", "b", "c", "d"]])
    );
    assert_eq!(r["count"], json!(3));
    // 不连续的环长 [2, 4]：三方环 a→b→c→a 不算（集合式判据，不是区间 2..=4）
    let r = call(json!({
        "edges": [{"u": "a", "v": "b"}, {"u": "b", "v": "c"}, {"u": "c", "v": "a"},
                  {"u": "b", "v": "a"}, {"u": "c", "v": "d"}, {"u": "d", "v": "a"}],
        "through": "a", "sizes": [2, 4]
    }))
    .unwrap();
    assert_eq!(r["sets"], json!([["a", "b"], ["a", "b", "c", "d"]]));
    // among 不含 d：四方环不出现
    let r = call(json!({
        "edges": [{"u": "a", "v": "b"}, {"u": "b", "v": "c"}, {"u": "c", "v": "d"}, {"u": "d", "v": "a"}],
        "through": "a", "among": ["b", "c"], "sizes": [3, 4]
    }))
    .unwrap();
    assert_eq!(r["count"], json!(0));
}

#[test]
fn 自环与越界的环长报错() {
    assert!(
        call(json!({"edges": [{"u": "a", "v": "a"}], "through": "a", "sizes": [3]}))
            .unwrap_err()
            .contains("self-loop")
    );
    assert!(
        call(json!({"edges": [], "through": "a", "sizes": [7]}))
            .unwrap_err()
            .contains("2..=6")
    );
    assert!(
        call(json!({"edges": [], "through": "a", "sizes": []}))
            .unwrap_err()
            .contains("empty")
    );
}
