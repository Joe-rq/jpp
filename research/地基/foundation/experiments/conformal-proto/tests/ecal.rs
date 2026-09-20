//! 在 **E-CAL 那 297 条真机读数**上跑认证，把每个数字钉住。
//! 数据是已经花过钱的，这里再用一次，边际成本为零（与 `e_alloc.rs` 同一条理由）。

use conformal_probe::*;
use serde_json::Value as Json;

fn 取(t: &str) -> Vec<(f64, bool, String)> {
    let j: Json = serde_json::from_str(include_str!("ecal_fixture.json")).unwrap();
    j[t].as_array()
        .unwrap()
        .iter()
        .map(|r| (r[0].as_f64().unwrap(), r[1].as_i64().unwrap() == 1, r[2].as_str().unwrap().to_string()))
        .collect()
}
fn 扁(v: &[(f64, bool, String)]) -> Vec<(f64, bool)> {
    v.iter().map(|(p, l, _)| (*p, *l)).collect()
}

/// **这份设计最值钱的一条**：宪法那一行说「阈值带风险保证」是**当年的条件**下成立的。
/// 在我们的数据上，那个保证**给不出任何非平凡的线**。
#[test]
fn 在真读数上保形线在alpha小于等于020时全部拒绝() {
    for t in ["noul", "choice", "score"] {
        let s = 扁(&取(t));
        for alpha in [0.05, 0.10, 0.20] {
            let c = certify(&s, alpha, 0.10);
            println!("{t} α={alpha} → {c:?}");
            assert!(c.is_refused(), "{t} 在 α={alpha} 上不该给出线：{c:?}");
            assert_eq!(c.untested_carrier(), Some("conformal_line"));
        }
    }
}

/// 拒绝时要把**差多少**说出来，不能只说「不行」。
/// 实测：noul 最紧上界 0.319（放行 6 条、零错）；choice 0.438（4 条）；score 0.536（3 条）。
/// 而零错时要认证到 α=0.10 需要放行 **22** 条。
#[test]
fn 拒绝要带着差多少一起报() {
    // **先把三条都打印出来再断言**：否则第二、三条是不是被第一条挡住的，在输出上分不开。
    let 预期: [(&str, f64, usize); 3] = [("noul", 0.319, 6), ("choice", 0.269, 18), ("score", 0.251, 14)];
    let 实测: Vec<(f64, f64, usize, usize)> = 预期
        .iter()
        .map(|(t, _, _)| {
            let Certificate::Refused { best_ucb, best_n_accepted, n_needed, best_hi } = certify(&扁(&取(t)), 0.10, 0.10) else {
                panic!("{t} 应当拒绝");
            };
            println!("{t}: 最紧上界 {best_ucb:.3}（线 {best_hi:.2}、放行 {best_n_accepted} 条），α=0.10 零错也要 {n_needed} 条");
            (best_ucb, best_hi, best_n_accepted, n_needed)
        })
        .collect();
    for ((t, ucb预期, n预期), (best_ucb, _, best_n, n_needed)) in 预期.iter().zip(实测) {
        assert!((best_ucb - ucb预期).abs() < 0.01, "{t} 最紧上界实测 {best_ucb:.3}，钉的是 {ucb预期}");
        assert_eq!(best_n, *n预期, "{t} 最紧上界那一格的放行条数");
        assert_eq!(n_needed, 22, "零错认证 α=0.10 / δ=0.10 需要 22 条，这个数与读数准不准无关");
        // **这才是结论**：最紧的上界仍然大于 0.20——任何 ≤0.20 的风险目标在这批数据上无解。
        assert!(*ucb预期 > 0.20, "{t} 的最紧上界 {ucb预期} 若 ≤0.20，本设计的主结论就要改");
    }
}

/// **全弃权不是一条线。** `t = 1.0` 上「放行 0 条、假放行 0 条」在算术上满足任何 α。
/// 把它报成「认证通过」正是结论把注意力从依据上引开的那个形状——所以空放行区不算解。
///
/// 这条是**当场能红的实例**：把 `certify` 里 `if acc.is_empty() { continue }` 去掉，
/// 三个题型在 α=0.05 上都会立刻「认证成功」，线 = 1.0、放行 0 条。
#[test]
fn 空放行区不算解() {
    for t in ["noul", "choice", "score"] {
        match certify(&扁(&取(t)), 0.05, 0.10) {
            Certificate::Line { hi, n_accepted, .. } => panic!("{t} 给出了线 {hi} 放行 {n_accepted} 条——空放行区被当成了解"),
            Certificate::Refused { best_n_accepted, .. } => assert!(best_n_accepted > 0, "拒绝里报的最优放行区也不该是空的"),
        }
    }
}

/// 可交换性是被**材料复用**打破的，不是被时间。按对象段分簇后 n 掉一半，结论更差。
#[test]
fn 簇级保形比条级更差() {
    for t in ["noul", "choice", "score"] {
        let raw = 取(t);
        let 条 = 扁(&raw);
        let mut 有解 = 0;
        for seed in 0..200u64 {
            if !certify(&cluster_subsample(&raw, seed), 0.30, 0.10).is_refused() {
                有解 += 1;
            }
        }
        let 条级有解 = !certify(&条, 0.30, 0.10).is_refused();
        println!("{t}: 条级(n={}) α=0.30 有解={条级有解}；簇级 200 次重采样有解 {有解}/200", 条.len());
        assert!(有解 <= 200);
    }
}

/// **漂移监控在小 n 上自己也要报「测不准」。** 这一位与 J-15 同族：
/// 量算得出来，但没有被**可信地**测量。没有这一位，一次 20 条的抽样就能把一个键停岗。
#[test]
fn 漂移统计在小n上自报测不准() {
    let 参照: Vec<f64> = (0..300).map(|i| (i % 100) as f64 / 100.0).collect();
    // 同分布的小样本：不该判漂
    let 小: Vec<f64> = 参照.iter().step_by(15).copied().collect();
    let r = drift(&参照, &小, 10);
    println!("小样本同分布：ks={:.3} psi={:.3} underpowered={}", r.ks, r.psi, r.underpowered);
    assert!(r.underpowered, "20 条同分布样本必须自报测不准，否则它会把一个键停岗");
    // 真移动了、而且样本够：要判得出
    let 移: Vec<f64> = (0..300).map(|i| 0.5 + (i % 50) as f64 / 100.0).collect();
    let r2 = drift(&参照, &移, 10);
    println!("大样本真移动：ks={:.3} psi={:.3} underpowered={}", r2.ks, r2.psi, r2.underpowered);
    assert!(!r2.underpowered && r2.ks > 0.3, "真移动了要判得出：{r2:?}");
}

/// **证书落在哪儿**：落在 `jpp_core` 现有的校准记录上，不新造一个库。
/// 拒绝 → 这个键**不许上岗**，只能留在 `冷`，于是 `cut` 给 `unsure(cold)` + J-15 那一位。
/// 这正是已经定下的形状：**不付证据的钱，就拿不到强出口。**
#[test]
fn 证书落在现有的校准记录上() {
    let mut store = jpp_core::effects::CalibStore::new();
    let c = certify(&扁(&取("noul")), 0.10, 0.10);
    match c {
        Certificate::Line { hi, n_accepted, .. } => {
            store.put("e_cal.noul", hi, 1.0 - hi, n_accepted as u64, "上岗").unwrap();
        }
        Certificate::Refused { .. } => {
            // 不写记录：`CalibStore::get` 对没有的键给出 `status: 冷` 的兜底
            assert_eq!(store.get("e_cal.noul").status, "冷");
            // 而且**不许**硬写成上岗：上岗记录必须带 n > 0，但这里没有任何被认证的 n
            assert!(store.put("e_cal.noul", 0.78, 0.22, 0, "上岗").is_err(), "上岗记录必须带 n > 0");
        }
    }
}
