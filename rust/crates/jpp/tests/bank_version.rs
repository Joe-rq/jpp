//! 账本头 `lib_version`、`bank_version` 真正写值，以及判断条目的 `calib_ref` 归属（步 27；B48、B116 一部分）。
//!
//! 端到端：真跑 `jpp run`，读回账本看头。依据：`20` B48（版本进账本头，`lib_version` 与 `bank_version` 分开）、
//! 主会话裁定十五（`lib_version` 由 `jpp_lib::lib_version` 算，没装载文件也有值）、主控批复 C（`bank_version`
//! 直接取 `bank.json` 的 `version`）。

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use jpp::effects::CalibStore;
use jpp::ledger::{Entry, Ledger};
use serde_json::Value as Json;

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn scratch(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("jpp-bank-version-{}-{tag}", std::process::id()));
    let _ = fs::remove_dir_all(&d);
    fs::create_dir_all(&d).unwrap();
    d
}

fn run(args: &[String], dir: &Path) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_jpp"))
        .current_dir(dir)
        .args(args)
        .output()
        .unwrap()
}

fn ledger_of(ex: &str, fixtures: &str, calib: Option<&str>, dir: &Path) -> (Ledger, PathBuf) {
    let led = dir.join(format!("{ex}.jsonl"));
    let mut a: Vec<String> = vec![
        "run".into(),
        root()
            .join(format!("examples/{ex}.jpp"))
            .display()
            .to_string(),
        "--fixtures".into(),
        root()
            .join(format!("examples/fixtures/{fixtures}.json"))
            .display()
            .to_string(),
        "--ledger-out".into(),
        led.display().to_string(),
    ];
    if let Some(c) = calib {
        a.push("--calib".into());
        a.push(root().join(c).display().to_string());
    }
    let o = run(&a, dir);
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    let (l, _) = Ledger::decode(&fs::read_to_string(&led).unwrap()).unwrap();
    (l, led)
}

fn bank_json() -> Json {
    serde_json::from_slice(&fs::read(root().join("bank/bank.json")).unwrap()).unwrap()
}

#[test]
fn header_has_lib_and_bank_version_after_a_real_run() {
    let d = scratch("real");
    let h = "ceb097176786d327fe1a0ba3";
    let (l, _) = ledger_of(
        "bank-json_field",
        "bank-json_field",
        Some(&format!("bank/entries/{h}/calib")),
        &d,
    );
    let c = l.header.expect("账本有头").compared;
    // bank_version：直接取 bank.json 的 version，不加索引哈希
    assert_eq!(c.bank_version, Some(bank_json()["version"].to_string()));
    // lib_version：`<文件哈希>+diag:<规则版本>`，这个示例 import 了 lib/outcome.jpp 与 lib/bank/*.jpp
    let lib = c.lib_version.expect("lib_version 有值");
    assert!(lib.contains("+diag:"), "{lib}");
    let _ = fs::remove_dir_all(&d);
}

#[test]
fn program_without_library_import_still_has_lib_version_but_no_bank_version() {
    let d = scratch("nolib");
    let (l, _) = ledger_of("adaptive", "adaptive", None, &d);
    let c = l.header.unwrap().compared;
    // 裁定十五：什么都没装载也有值（只含规则版本）；没装载题库则 bank_version 为空
    assert!(
        c.lib_version
            .as_deref()
            .is_some_and(|v| v.contains("+diag:"))
    );
    assert_eq!(c.bank_version, None);
    let _ = fs::remove_dir_all(&d);
}

#[test]
fn judge_entries_carry_form_key_kind_and_fill() {
    let d = scratch("ref");
    let h = "ceb097176786d327fe1a0ba3";
    let (l, _) = ledger_of(
        "bank-json_field",
        "bank-json_field",
        Some(&format!("bank/entries/{h}/calib")),
        &d,
    );
    let refs: Vec<_> = l
        .entries
        .iter()
        .filter_map(|e| match e {
            Entry::Judge { calib_ref, .. } => calib_ref.as_deref().cloned(),
            _ => None,
        })
        .collect();
    assert_eq!(refs.len(), 4);
    for r in &refs {
        // 与 CalibStore::form_key 同格式：两处各自拼字符串，格式漂移就在这里红
        assert_eq!(r.key.as_deref(), Some(CalibStore::form_key(h).as_str()));
        assert_eq!(r.kind.as_deref(), Some("attr"));
        assert_eq!(
            r.fill,
            Some(vec![("F".to_string(), "order_no".to_string())])
        );
    }
    // 手写题（没有题式）不填：三项为空，账本与旧格式相同
    let (l2, _) = ledger_of("adaptive", "adaptive", None, &d);
    for e in &l2.entries {
        if let Entry::Judge { calib_ref, .. } = e {
            let r = calib_ref.as_deref().unwrap();
            assert!(r.key.is_none() && r.kind.is_none() && r.fill.is_none());
        }
    }
    let _ = fs::remove_dir_all(&d);
}

#[test]
fn bank_version_follows_bank_json_and_lib_version_follows_lib_files() {
    let d = scratch("unit");
    fs::create_dir_all(d.join("lib/bank")).unwrap();
    fs::create_dir_all(d.join("bank")).unwrap();
    fs::write(d.join("lib/a.jpp"), "let a = 1;").unwrap();
    fs::write(
        d.join("lib/bank/f.jpp"),
        "let f = form(\"test\", \"是否 {x}？\");",
    )
    .unwrap();
    let bank = |v: u64| {
        fs::write(
            d.join("bank/bank.json"),
            format!("{{\"version\": {v}, \"entries\": []}}"),
        )
        .unwrap();
    };
    let files = |a_text: &str| -> (String, Option<String>) {
        let a = d.join("lib/a.jpp");
        let f = d.join("lib/bank/f.jpp");
        let entry = d.join("prog.jpp");
        jpp::store::bank::versions_of([
            (entry.as_path(), b"x".as_slice()),
            (a.as_path(), a_text.as_bytes()),
            (f.as_path(), b"f".as_slice()),
        ])
    };
    bank(7);
    let (lib1, b1) = files("let a = 1;");
    assert_eq!(b1.as_deref(), Some("7"));
    bank(8);
    let (lib2, b2) = files("let a = 1;");
    assert_eq!(
        b2.as_deref(),
        Some("8"),
        "bank.json 升版本，头里的 bank_version 跟着变"
    );
    assert_eq!(lib1, lib2, "题库变动不改 lib_version（两件事分开）");
    let (lib3, _) = files("let a = 2;");
    assert_ne!(lib1, lib3, "标准库文件内容变，lib_version 变");
    // 只装载入口程序：没有 bank_version
    let entry = d.join("prog.jpp");
    let (_, b) = jpp::store::bank::versions_of([(entry.as_path(), b"x".as_slice())]);
    assert_eq!(b, None);
    let _ = fs::remove_dir_all(&d);
}

#[test]
fn replaying_a_ledger_from_an_older_binary_reports_w_header_lib_version() {
    let d = scratch("old");
    let (mut l, _) = ledger_of("adaptive", "adaptive", None, &d);
    // 旧二进制写的账本：头里没有 lib_version
    l.header.as_mut().unwrap().compared.lib_version = None;
    let old = d.join("old.jsonl");
    fs::write(&old, l.encode()).unwrap();
    let out = d.join("r.json");
    let o = run(
        &[
            "run".into(),
            root().join("examples/adaptive.jpp").display().to_string(),
            "--replay".into(),
            old.display().to_string(),
            "--output".into(),
            out.display().to_string(),
        ],
        &d,
    );
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    let r: Json = serde_json::from_slice(&fs::read(&out).unwrap()).unwrap();
    let warn = r["trace"]["warnings"]
        .as_array()
        .or_else(|| r["warnings"].as_array())
        .map(|a| {
            a.iter()
                .filter_map(|w| w.as_str())
                .collect::<Vec<_>>()
                .join("\n")
        })
        .unwrap_or_else(|| r.to_string());
    assert!(
        warn.contains("W-header") && warn.contains("lib_version"),
        "{warn}"
    );
    let _ = fs::remove_dir_all(&d);
}
