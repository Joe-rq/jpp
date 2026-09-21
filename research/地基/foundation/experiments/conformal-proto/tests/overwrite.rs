//! **证书寻址的回归保护**（原为「展示缺口」的测试，缺口已堵，2026-09-21 改写）。
//!
//! **这条测试记录的缺口**：证书曾经是记录上的**一个字段** `cert: Option<Cert>`。
//! 于是同一个键上认两次，后者覆盖前者——实测 α=0.60 给线 0.195，再认一次 α=0.80
//! 给线 **0.000**（从「过线才放行」变成「全放行」），**而覆盖过的记录与只认证过一次的
//! 记录逐字段相同**。覆盖不可恢复：旧证书没了，源头也没了。
//!
//! **这个缺口是我自己设计里的**：我写的是「证书**记进记录**的最小字段」，
//! 而「记进记录」是**字段**，不是**键**。
//!
//! **什么时候被什么堵上的**：`rust-core-3` 2026-09-21 把它改成
//! `certs: BTreeMap<地址, Cert>`，地址 = `(α, conf_delta, 簇单位[, 代价])`——
//! **限定进了地址，取值不同就是不同的键，于是并存而不是覆盖**；取用时
//! `选中的证书()` 取 **α 最小**的那张（同 α 取线更高的），
//! **所以后认一个更松的 α 永远不会把线放宽**。
//!
//! **所以现在断言的是反过来的**：两张都还在、线不被放宽、而且**与认证顺序无关**。
//! 缺口回来了这三条都会红。

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

/// 两个都认得住的 α：严的那个给更高的线，松的那个给更低的线。
const 严: f64 = 0.60;
const 松: f64 = 0.80;

#[test]
fn 后认一张更松的证书不会把线放宽_而且两张都还在() {
    let mut s = CalibStore::new();
    装样本(&mut s, "k");
    let c严 = s.commission("k", 严, 0.10, "对象段").expect("严的那个认得住");
    let hi严 = s.get("k").hi;
    let c松 = s.commission("k", 松, 0.10, "对象段").expect("松的那个也认得住");
    let rec = s.get("k");
    println!("α={严:.2} → 线 {:.3}（ucb {:.3}）", c严.hi, c严.ucb);
    println!("α={松:.2} → 线 {:.3}（ucb {:.3}）", c松.hi, c松.ucb);
    println!("记录里：hi={:.3}，证书 {} 张：{:?}", rec.hi, rec.certs.len(), rec.certs.keys().collect::<Vec<_>>());

    // 前提：这两个 α 真的给出不同的线，否则这条测试什么也没测
    assert!(c松.hi < c严.hi, "松的 α 要给出更低的线才做得了这个试验：{} vs {}", c松.hi, c严.hi);

    // **回归一：两张都还在。** 覆盖回来了这一条会红。
    assert_eq!(rec.certs.len(), 2, "两个 α 是两个地址，必须并存：{:?}", rec.certs.keys().collect::<Vec<_>>());

    // **回归二：线没有被放宽。** 这才是覆盖真正的危害——不是少了一条记录，是线松了。
    assert_eq!(rec.hi, hi严, "后认一张更松的证书把线从 {hi严:.3} 放宽到了 {:.3}", rec.hi);
    assert_eq!(rec.选中的证书().unwrap().alpha, 严, "取用的必须是 α 最小（最严）的那张");

    // **回归三：与认证顺序无关。** 顺序相关的话，「先认哪个」就成了一个没人记录的输入。
    let mut t = CalibStore::new();
    装样本(&mut t, "k");
    t.commission("k", 松, 0.10, "对象段").expect("先松");
    t.commission("k", 严, 0.10, "对象段").expect("后严");
    println!("反序之后：hi={:.3}，证书 {} 张", t.get("k").hi, t.get("k").certs.len());
    assert_eq!(t.get("k").certs, rec.certs, "两种顺序得到同一组证书");
    assert_eq!(t.get("k").hi, rec.hi, "两种顺序得到同一条线");
}
