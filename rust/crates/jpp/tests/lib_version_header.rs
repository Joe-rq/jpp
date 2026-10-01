//! 改一次诊断规则版本后重放旧账本，报 `W-header: … lib_version`（步 26；主会话裁定 2026-09-29 第十五条）。
//!
//! 账本头的 `lib_version` 由运行时填（题库代理，步 27）；本测试直接在账本口上比：旧账本头记的是旧规则版本
//! 算出的 `lib_version`，本二进制按 `jpp_lib::lib_version` 算出新值，按只凭账本重放的场合换头。

use jpp::ledger::{Header, HeaderCompare, Ledger, LedgerPort};

fn 头(lib: String) -> Header {
    let mut h = Header::new(10, 1.0, "m", "r1", "h");
    h.compared.lib_version = Some(lib);
    h
}

#[test]
fn 规则版本改过重放报lib_version() {
    let files = vec![("diag.jpp".to_string(), b"fn x() { 1 }".to_vec())];
    let 旧 = jpp_lib::lib_version_with("b13-0", &files);
    let 新 = jpp_lib::lib_version(&files);
    let mut ledger = Ledger::new();
    assert_eq!(
        ledger
            .open_run(头(旧.clone()), HeaderCompare::Replay, &|_| None)
            .unwrap(),
        None
    );
    let w = ledger
        .open_run(头(新.clone()), HeaderCompare::Replay, &|_| None)
        .unwrap()
        .expect("应报 W-header");
    assert!(w.starts_with("W-header"), "{w}");
    assert!(w.contains(&format!("lib_version 旧 {旧} 新 {新}")), "{w}");
}

#[test]
fn 规则与文件都没变重放不报() {
    let files = vec![("diag.jpp".to_string(), b"fn x() { 1 }".to_vec())];
    let mut ledger = Ledger::new();
    ledger
        .open_run(
            头(jpp_lib::lib_version(&files)),
            HeaderCompare::Replay,
            &|_| None,
        )
        .unwrap();
    let w = ledger
        .open_run(
            头(jpp_lib::lib_version(&files)),
            HeaderCompare::Replay,
            &|_| None,
        )
        .unwrap();
    assert_eq!(w, None);
}
