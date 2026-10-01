//! `lib_version` 的性质（步 26；主会话裁定 2026-09-29 第十五条）。

use jpp_lib::{RULES_VERSION, lib_version, lib_version_with};

fn f(p: &str, c: &str) -> (String, Vec<u8>) {
    (p.to_string(), c.as_bytes().to_vec())
}

#[test]
fn 什么都没装载也有版本且带规则版本() {
    let v = lib_version(&[]);
    assert!(v.ends_with(&format!("+diag:{RULES_VERSION}")), "{v}");
    assert_eq!(v, lib_version(&[]));
}

#[test]
fn 与交来的顺序无关() {
    let a = [f("diag.jpp", "x"), f("compose/tree.jpp", "y")];
    let b = [f("compose/tree.jpp", "y"), f("diag.jpp", "x")];
    assert_eq!(lib_version(&a), lib_version(&b));
}

#[test]
fn 内容或路径一变版本就变() {
    let base = lib_version(&[f("diag.jpp", "x")]);
    assert_ne!(base, lib_version(&[f("diag.jpp", "x2")]));
    assert_ne!(base, lib_version(&[f("diag2.jpp", "x")]));
    assert_ne!(base, lib_version(&[]));
}

#[test]
fn 题库文件不算() {
    let a = [f("diag.jpp", "x")];
    let b = [f("diag.jpp", "x"), f("bank/same_name.jpp", "z")];
    assert_eq!(lib_version(&a), lib_version(&b));
}

#[test]
fn 规则版本一变版本就变() {
    let files = [f("diag.jpp", "x")];
    assert_ne!(lib_version_with("旧", &files), lib_version(&files));
    assert_eq!(lib_version_with(RULES_VERSION, &files), lib_version(&files));
}
