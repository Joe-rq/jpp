//! B77 旁路测试（步 7b）：账本头的两个校准哈希与按场合比对。
//!
//! `21` 写作 `tests/bypass/b77_header.rs`；本仓库旁路测试的约定是各 crate 的 `tests/bypass_*.rs`。
//! 依据：`附注/2026-09-24-评估①裁定.md` §四；`12` §2.10 账本头行、J-18 行。
//! - 只凭账本重放同一批命中记录 → 不报 `W-header`（重放不比 `calib_hash`）；
//! - 续接换了装载库（多一条没命中的记录）→ `W-header: calib_hash`，不涉 `calib_used_hash`；
//! - 重放时另给的记录换了命中键的内容 → `W-header: calib_used_hash`，不涉 `calib_hash`。
use serde_json::Value;
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn scratch(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("jpp-b77-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&d);
    fs::create_dir_all(&d).unwrap();
    d
}

/// 跑一次 `jpp run …`，返回报告。
fn run(args: &[&str], out: &Path) -> Value {
    let o = Command::new(env!("CARGO_BIN_EXE_jpp"))
        .current_dir(root())
        .arg("run")
        .args(args)
        .args(["--output", out.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    serde_json::from_slice(&fs::read(out).unwrap()).unwrap()
}

fn copy_dir(from: &Path, to: &Path) {
    fs::create_dir_all(to).unwrap();
    for e in fs::read_dir(from).unwrap() {
        let p = e.unwrap().path();
        fs::copy(&p, to.join(p.file_name().unwrap())).unwrap();
    }
}

fn headers(r: &Value) -> Vec<String> {
    r["trace"]["warnings"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|w| w.as_str())
        .filter(|w| w.starts_with("W-header"))
        .map(String::from)
        .collect()
}

/// `examples/sieve.jpp` 在 `_calib/b24` 上只命中一条题式记录（金样 `sieve@truth`）。
const 命中文件: &str = "_form_38e0207a4cbe0a56b8b472b7.json";

fn 首跑(d: &Path) -> (PathBuf, Value) {
    let lib = d.join("lib");
    copy_dir(&root().join("tests/golden/_calib/b24"), &lib);
    let ledger = d.join("ledger.jsonl");
    let first = run(
        &[
            "examples/sieve.jpp",
            "--fixtures",
            "examples/fixtures/sieve-truth.json",
            "--calib",
            lib.to_str().unwrap(),
            "--ledger-out",
            ledger.to_str().unwrap(),
        ],
        &d.join("first.json"),
    );
    // 头行两个哈希都在
    let head: Value =
        serde_json::from_str(fs::read_to_string(&ledger).unwrap().lines().next().unwrap()).unwrap();
    let c = &head["header"]["compared"];
    assert!(
        c["calib_hash"].is_string() && c["calib_used_hash"].is_string(),
        "{c}"
    );
    assert_eq!(
        head["calib_used"].as_object().unwrap().len(),
        1,
        "只命中一条题式记录"
    );
    (ledger, first)
}

#[test]
fn 只凭账本重放同一批命中记录_不报w_header() {
    let d = scratch("same");
    let (ledger, first) = 首跑(&d);
    let r = run(
        &["examples/sieve.jpp", "--replay", ledger.to_str().unwrap()],
        &d.join("r.json"),
    );
    assert_eq!(r["cost"]["calls"], 0, "重放 0 调用");
    assert_eq!(r["value"], first["value"]);
    assert!(
        headers(&r).is_empty(),
        "装载库哈希不同是常态，不报：{:?}",
        headers(&r)
    );
}

#[test]
fn 续接换库_报calib_hash() {
    let d = scratch("resume");
    let (ledger, _) = 首跑(&d);
    // 新库 = 原库 + 一条本程序不命中的记录：命中集合不变，装载库变了
    let lib2 = d.join("lib2");
    copy_dir(&d.join("lib"), &lib2);
    fs::copy(
        root().join("tests/golden/_calib/pending/_form_237425f8b6d6c6670b12706f.json"),
        lib2.join("_form_237425f8b6d6c6670b12706f.json"),
    )
    .unwrap();
    let r = run(
        &[
            "examples/sieve.jpp",
            "--fixtures",
            "examples/fixtures/sieve-truth.json",
            "--calib",
            lib2.to_str().unwrap(),
            "--resume",
            ledger.to_str().unwrap(),
        ],
        &d.join("r.json"),
    );
    let w = headers(&r);
    assert_eq!(w.len(), 1, "{w:?}");
    assert!(w[0].contains("calib_hash 旧"), "{w:?}");
    assert!(!w[0].contains("calib_used_hash"), "命中集合没变：{w:?}");
}

#[test]
fn 重放换命中记录内容_报calib_used_hash() {
    let d = scratch("changed");
    let (ledger, _) = 首跑(&d);
    // 另给的库只有命中那一条，内容改一个承载行为的字段（unsure_rate 进 J-10）
    let lib3 = d.join("lib3");
    fs::create_dir_all(&lib3).unwrap();
    let text = fs::read_to_string(d.join("lib").join(命中文件)).unwrap();
    let changed = text.replacen("\"unsure_rate\": 0.0018", "\"unsure_rate\": 0.0019", 1);
    assert_ne!(text, changed, "夹具记录的字段写法变了，改这里");
    fs::write(lib3.join(命中文件), changed).unwrap();
    let r = run(
        &[
            "examples/sieve.jpp",
            "--calib",
            lib3.to_str().unwrap(),
            "--replay",
            ledger.to_str().unwrap(),
        ],
        &d.join("r.json"),
    );
    assert_eq!(r["cost"]["calls"], 0);
    let w = headers(&r);
    assert_eq!(w.len(), 1, "{w:?}");
    assert!(w[0].contains("calib_used_hash 旧"), "{w:?}");
    assert!(!w[0].contains("calib_hash 旧"), "重放不比装载库：{w:?}");
}
