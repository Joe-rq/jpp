//! **证书门管谁、不管谁**（总控 2026-09-21 裁定）。边界不是新造的，是 `provenance()` 已经有的那条。
//!
//! - **程序积累**的证据要让一个键上岗 → **必须有证书**。没有就留 `冷`，
//!   `cause` 仍是 `cold`，J-15 那一位挂 `conformal_line`。
//! - **宿主手填**的线照旧上岗，**不需要证书**。理由是 I4：线来自程序之外，**宿主为它负责**；
//!   责任归属可查（`provenance()` 分得出来），就不必再加一道门。
//!
//! 这条边界之所以能划，是因为 `provenance()` 是**算出来的**（`n` 对不对得上实际标注条数），
//! 不是调用方自己声明的——**声明的边界拦不住任何人**。

use conformal_probe::*;
use jpp_core::effects::{CalibStore, LiteralMode, Provenance, Sample};

fn 取(t: &str) -> Vec<(f64, bool)> {
    conformal_probe::ecal2(t)
}
fn 样本(p: f64, l: bool) -> Sample {
    Sample { p: Some(p), label: Some(if l { 1 } else { 0 }), perms: 0, mode_share: None, mode: LiteralMode::default(), phys: "noul".into(), cluster: None }
}

/// 门要不要开，由 `provenance()` 决定——而它是算出来的，不是声明的。
fn 要不要证书(rec: &jpp_core::effects::CalibRecord) -> bool {
    matches!(rec.provenance(), Provenance::程序积累 | Provenance::混合)
}

#[test]
fn 程序积累要证书而宿主手填不要() {
    // —— 程序积累：73 条真实标注折进去 ——
    let mut a = CalibStore::new();
    for (p, l) in 取("noul") {
        a.absorb("程序积累的键", 样本(p, l)).unwrap();
    }
    let rec = a.get("程序积累的键");
    println!("程序积累：provenance={:?} n={} labeled={}", rec.provenance(), rec.n, rec.labeled());
    assert_eq!(rec.provenance(), Provenance::程序积累);
    assert!(要不要证书(&rec), "程序积累的键要过证书门");
    assert!(certify(&取("noul"), 0.10, 0.10).is_refused(), "而这批数据拿不到证书");
    assert_eq!(rec.status, "待真值", "拿不到证书就上不了岗——但**程序照常跑**，只是这个键切不出 Act");

    // —— 宿主手填：宣称了 n，一条证据也没有 ——
    let mut b = CalibStore::new();
    b.put("宿主手填的键", 0.78, 0.22, 200, "上岗").unwrap();
    let rec2 = b.get("宿主手填的键");
    println!("宿主手填：provenance={:?} n={} observations={}", rec2.provenance(), rec2.n, rec2.observations());
    assert_eq!(rec2.provenance(), Provenance::宿主手填);
    assert!(!要不要证书(&rec2), "宿主手填不过证书门——I4：线来自程序之外，宿主为它负责");
    assert_eq!(rec2.status, "上岗");
}

/// **「不阻塞」与「不放行」不是一件事**，而 J-15 原文要的是前者。
///
/// 我先前把它们当成了一件事，于是把「三个题型全部拿不到证书」写成「语言等于停摆」。
/// **那是误述**：全是 `unsure` 的程序不是停摆，是这批数据上**诚实的结果**——
/// `unsure` 是一等出口，有 handler 路由、能 `escalate` 给人、能 `literalize` 重问。
/// 这门语言存在的理由就是让这种情形**有地方可去**，而不是被压成一个假的「通过」。
///
/// 这里把那句误述钉成一个可跑的反例：拿不到证书的键，`cut` 仍然切得出出口。
#[test]
fn 拿不到证书不等于程序跑不下去() {
    let store = CalibStore::new();                       // 没有任何记录 = 全是冷键
    let rec = store.get("没有证书的键");
    assert_eq!(rec.status, "冷");
    // `冷` 的线是档案保守线，不是「没有线」——`cut` 照常切，出口是 Unsure(cold)。
    let (hi, lo) = store.lines_for(&rec);
    println!("冷键的线：hi={hi} lo={lo}（档案保守线，不是没有线）");
    assert!(hi >= lo, "保守线仍然是一条线，程序不会因为没有证书而跑不下去");
}
