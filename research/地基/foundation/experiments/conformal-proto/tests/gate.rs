//! **证书门的回归保护**（原为「展示缺口」的测试，缺口已堵，2026-09-21 改写）。
//!
//! **这条测试记录的缺口**：`待真值 → 上岗` 这一步曾经没有任何代码在做，唯一能上岗的
//! 入口 `put` 只核 `n > 0`——**73 条带标注的证据在库里，一条手写的 `hi=0.78` 就能让
//! 这个键上岗，没有任何东西核过它凭什么。**
//!
//! **什么时候被什么堵上的**：`rust-core-3` 2026-09-21 在 `CalibStore::put` 里加了证书门——
//! 记录上有**带标注**的证据时，手写的线不是凭据，要走 `commission`。
//! 门没有做成墙：**收紧永远放行**（`hi` 不降、`lo` 不升 = 带更宽 = `Unsure` 更多 =
//! 往拒绝那边倒），不需要任何凭据。
//!
//! **所以现在断言的是反过来的**：手写上岗**必须失败**，而收紧**必须成功**。
//! 缺口回来了这两条都会红。**「展示缺口」是一次性的，「保护缺口不再出现」是长期的。**

use conformal_probe::*;
use jpp_core::effects::{CalibStore, LiteralMode, Sample};
use serde_json::Value as Json;

fn 装样本(store: &mut CalibStore, key: &str) {
    let j: Json = serde_json::from_str(include_str!("ecal_fixture.json")).unwrap();
    for r in j["noul"].as_array().unwrap() {
        store
            .absorb(key, Sample {
                p: Some(r[0].as_f64().unwrap()),
                label: Some(r[1].as_i64().unwrap() as u8),
                perms: 0,
                mode_share: None,
                mode: LiteralMode::default(),
                phys: "noul".into(),
                cluster: Some(r[2].as_str().unwrap().to_string()),
            })
            .unwrap();
    }
}

fn 取(t: &str) -> Vec<(f64, bool)> {
    let j: Json = serde_json::from_str(include_str!("ecal_fixture.json")).unwrap();
    j[t].as_array().unwrap().iter().map(|r| (r[0].as_f64().unwrap(), r[1].as_i64().unwrap() == 1)).collect()
}

#[test]
fn 证书门回归_手写上岗被挡_收紧放行_放宽要凭据() {
    let mut store = CalibStore::new();
    let s = 取("noul");
    // 生产端：把标注样本折进记录（absorb 是 324786d 给的入口）
    for (p, l) in &s {
        store
            .absorb("e_cal.noul", Sample {
                p: Some(*p),
                label: Some(if *l { 1 } else { 0 }),
                perms: 0,
                mode_share: None,
                mode: LiteralMode::default(),
                phys: "noul".into(), cluster: None,
            })
            .unwrap();
    }
    let rec = store.get("e_cal.noul");
    println!("absorb 之后：status={} n={} samples={}", rec.status, rec.n, rec.samples.len());
    // **absorb 自己不上岗**：73 条带标注的样本进去，状态停在「待真值」
    assert_eq!(rec.status, "待真值", "absorb 不让记录上岗——这是它的设计，不是缺陷");
    assert_eq!(rec.n, 73);

    // 证书门：认证不过 → **不许上岗**，留在待真值
    let c = certify(&s, 0.10, 0.10);
    println!("证书：{c:?}");
    assert!(c.is_refused());
    assert_eq!(c.untested_carrier(), Some("conformal_line"));

    // **回归一：手写上岗必须失败。** 缺口回来了这一条会红。
    let e = store.put("e_cal.noul", 0.78, 0.22, 73, "上岗").expect_err("手写上岗必须被证书门挡住");
    println!("手写上岗被挡：{e}");
    assert!(e.contains("commission"), "报错要指出走哪条路，而不是只说不行：{e}");
    assert_eq!(store.get("e_cal.noul").status, "待真值", "挡住之后记录保持原状，不因为被挡就停岗");

    // **回归二：门不是墙——收紧不需要凭据。** 少了这一条，门会变成失败开放的反面：
    // 旧线继续放行，而人已经不能收紧它了。
    let mut b = CalibStore::new();
    装样本(&mut b, "k");
    // 用 α=0.60：它给出 hi=0.195 的线，才做得了「放宽」那一条的对照（α=0.80 给 hi=0.000，没得再低）
    let c = b.commission("k", 0.60, 0.10, "对象段").expect("松 α 上认得住");
    let (h0, l0) = { let r = b.get("k"); (r.hi, r.lo) };
    println!("commission 之后：status={} hi={h0:.3} lo={l0:.3}（证书 α={:.2}）", b.get("k").status, c.alpha);
    assert_eq!(b.get("k").status, "上岗");
    b.put("k", h0 + 0.10, l0, 73, "上岗").expect("收紧（hi 更高）不需要凭据");
    // **判据是相对当前的线，不是相对 commission 那一刻的线**——
    // 上一行已经把 hi 抬到 h0+0.10，这里再写 h0 就是**放宽**，会被正确挡住。
    // （我第一版正是这么写的，当场红；红的是测试不是内核。）
    let (h_now, l_now) = { let r = b.get("k"); (r.hi, r.lo) };
    b.put("k", h_now, (l_now - 0.10).max(0.0), 73, "上岗").expect("lo 更低也是带更宽，同样不需要凭据");

    // **回归三：放宽才要凭据。** hi 更低 = 放行更多 = 这一步必须被挡。
    let (h1, l1) = { let r = b.get("k"); (r.hi, r.lo) };
    assert!(h1 > 0.05, "这一条要 hi 有下降空间才测得了，实际 hi={h1}");
    let e2 = b.put("k", h1 - 0.05, l1, 73, "上岗").expect_err("放宽 hi 必须要凭据");
    println!("放宽被挡：{e2}");
}
