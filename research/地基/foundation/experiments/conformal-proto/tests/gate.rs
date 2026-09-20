//! **证书门的位置**：`待真值 → 上岗` 这一步今天没有任何代码在做，而那正是证书该站的地方。
//!
//! `rust-core-3` 的 `324786d` 补上了生产端：`CalibStore::absorb` 收样本，
//! 无标注的观察只把 `冷` 推到 `待真值`、`n` 不动（它自己堵掉了「无标注也算进 n」那个洗白口）。
//! 但 `absorb` **不让记录上岗**，而全库唯一能上岗的入口 `put` 要调用方自己写死 `hi/lo`——
//! **也就是说「这条线凭什么能上岗」今天由调用方手写，没有任何东西核过它。**

use conformal_probe::*;
use jpp_core::effects::{CalibStore, LiteralMode, Sample};
use serde_json::Value as Json;

fn 取(t: &str) -> Vec<(f64, bool)> {
    let j: Json = serde_json::from_str(include_str!("ecal_fixture.json")).unwrap();
    j[t].as_array().unwrap().iter().map(|r| (r[0].as_f64().unwrap(), r[1].as_i64().unwrap() == 1)).collect()
}

#[test]
fn 证书门站在待真值到上岗这一步() {
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

    // 反面：今天**没有任何东西拦得住**手写一条上岗记录。
    // `put` 只核 n > 0，不核这条线凭什么。73 > 0，于是随手写的 0.78 就上岗了。
    assert!(store.put("e_cal.noul", 0.78, 0.22, 73, "上岗").is_ok(), "这一行能过，正是证书门要补的那个缺口");
    assert_eq!(store.get("e_cal.noul").status, "上岗");
    println!("**缺口**：手写 hi=0.78 就上岗了，而同一批数据的证书是 {:?}", certify(&s, 0.10, 0.10).is_refused());
}
