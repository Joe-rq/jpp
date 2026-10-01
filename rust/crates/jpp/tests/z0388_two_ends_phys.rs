//! Z0388：两端先标（`calib-import --from-ledger … --list-out`）按读数实际的物理题型取画像 δ 的列。
//! 打分读数建框时也带 argmax 作 pick，原来按「有没有 pick」判成 K 选一，取到 choice 列（与 Z0238 同一类错）。
//! 清单头行写这次排序用的题型与 δ。预注册：`地基/过程记录/工程-Z0334-δ分层.md` §十九。

use serde_json::{Value, json};
use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Output},
};

fn 跑(dir: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_jpp"))
        .current_dir(dir)
        .args(args)
        .output()
        .unwrap()
}

fn 目录(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("jpp-z0388-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&d);
    fs::create_dir_all(&d).unwrap();
    d
}

fn 发行画像() -> String {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../profiles/jev-1.13.0.json")
        .display()
        .to_string()
}

/// 固定观察造一份 20 条读数的账本（一种题型），导出两端先标清单，返回头行
fn 清单头(tag: &str, 状态: &str, 题: &str, obs: impl Fn(usize, &str) -> Value) -> Value {
    let d = 目录(tag);
    let mats: Vec<String> = (0..20).map(|i| format!("第{i}号材料。")).collect();
    let fx: Vec<Value> = mats.iter().enumerate().map(|(i, m)| obs(i, m)).collect();
    fs::write(d.join("fx.json"), json!({"observations": fx}).to_string()).unwrap();
    let list = mats
        .iter()
        .map(|m| format!("{m:?}"))
        .collect::<Vec<_>>()
        .join(", ");
    fs::write(
        d.join("p.jpp"),
        format!(
            "budget {{calls: 40, cost: 1, depth: 64}};\nlet q = {题};\nmap([{list}], fn(m) {{ judge({状态}, q) }});\n0\n"
        ),
    )
    .unwrap();
    let o = 跑(
        &d,
        &[
            "run",
            "p.jpp",
            "--fixtures",
            "fx.json",
            "--ledger-out",
            "led.jsonl",
        ],
    );
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    let 画像 = 发行画像();
    let o = 跑(
        &d,
        &[
            "calib-import",
            "--from-ledger",
            "led.jsonl",
            "--key",
            "k",
            "--list-out",
            "l.jsonl",
            "--profile",
            &画像,
        ],
    );
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    let head: Value = serde_json::from_str(
        fs::read_to_string(d.join("l.jsonl"))
            .unwrap()
            .lines()
            .next()
            .unwrap(),
    )
    .unwrap();
    let _ = fs::remove_dir_all(&d);
    head["list"].clone()
}

const 档: &str = r#"["低", "中", "高"]"#;

/// 打分读数取打分列 δ（发行画像中段 0.0821），不是 K 选一列（0.0971）
#[test]
fn 打分读数取打分列delta() {
    let h = 清单头(
        "score",
        "state(mat(m))",
        &format!("measure(\"这件事有多严重？\", {档}, \"k\")"),
        |i, m| {
            let p = if i % 2 == 0 { 0.9 } else { 0.1 };
            json!({"on": [m], "op": "measure", "text": "这件事有多严重？", "calib": "k",
                   "scale": ["低", "中", "高"], "answer": {"Score": [1.0 - p, 0.0, p]}})
        },
    );
    assert_eq!(h["phys"], json!("score"), "{h}");
    assert_eq!(h["delta"], json!(0.0821), "{h}");
}

/// 对照：K 选一读数取 choice 列（0.0971），是非题取 noul 列（0.1281）
#[test]
fn k选一与是非题各取本列delta() {
    let h = 清单头(
        "select",
        "state(mat(m), {over: [mat(\"甲\"), mat(\"乙\")]})",
        "select(\"说的是哪一个？\", \"k\")",
        |i, m| {
            let p = if i % 2 == 0 { 0.9 } else { 0.1 };
            json!({"on": [m], "over": ["甲", "乙"], "op": "select", "text": "说的是哪一个？", "calib": "k",
                   "answer": {"Choice": [p, 1.0 - p]}})
        },
    );
    assert_eq!(h["phys"], json!("choice"), "{h}");
    assert_eq!(h["delta"], json!(0.0971), "{h}");
    let h = 清单头(
        "test",
        "state(mat(m))",
        "test(\"行吗？\", \"k\")",
        |i, m| {
            json!({"on": [m], "op": "test", "text": "行吗？", "calib": "k",
               "answer": {"Noul": if i % 2 == 0 { 0.9 } else { 0.1 }}})
        },
    );
    assert_eq!(h["phys"], json!("noul"), "{h}");
    assert_eq!(h["delta"], json!(0.1281), "{h}");
}

/// 跑一段程序留账本（伴随题关，账本里只有程序自己的读数），返回目录
fn 首跑(tag: &str, 程序: &str, fx: Value) -> PathBuf {
    let d = 目录(tag);
    fs::write(d.join("fx.json"), fx.to_string()).unwrap();
    fs::write(d.join("p.jpp"), 程序).unwrap();
    let o = 跑(
        &d,
        &[
            "run",
            "p.jpp",
            "--fixtures",
            "fx.json",
            "--ledger-out",
            "led.jsonl",
            "--companions",
            "off",
        ],
    );
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    d
}

fn 导出(d: &Path) -> Output {
    let 画像 = 发行画像();
    跑(
        d,
        &[
            "calib-import",
            "--from-ledger",
            "led.jsonl",
            "--key",
            "k",
            "--list-out",
            "l.jsonl",
            "--profile",
            &画像,
        ],
    )
}

/// 复核可后补（Z0388 退回路径）：建框前账本先经 `read_any` 解码（PR35 评审修复），`jkey.phys` 是必有字段，
/// 去掉它的账本在读入时就报 `E-ledger-corrupt`，走不到「账本没写 phys」的退回分支。所以退回分支（按答案种类
/// 判题型，不再按有没有 pick 猜）只是防御，CLI 上够不着；这里钉住「够不着」这件事本身
#[test]
fn 账本没有phys读入即报错_退回分支够不着() {
    let d = 首跑(
        "nophys",
        "budget {calls: 4, cost: 1, depth: 16};\nlet a = judge(state(mat(\"甲\")), measure(\"这件事有多严重？\", [\"低\", \"中\", \"高\"], \"k\"));\n0\n",
        json!({"observations": [
            {"on": ["甲"], "op": "measure", "text": "这件事有多严重？", "calib": "k",
             "scale": ["低", "中", "高"], "answer": {"Score": [0.1, 0.0, 0.9]}}
        ]}),
    );
    let 账本 = fs::read_to_string(d.join("led.jsonl")).unwrap();
    let mut 去掉 = 0;
    let 改: Vec<String> = 账本
        .lines()
        .map(|l| {
            let mut v: Value = serde_json::from_str(l).unwrap();
            if let Some(k) = v
                .pointer_mut("/entry/Judge/jkey")
                .and_then(Value::as_object_mut)
                && k.remove("phys").is_some()
            {
                去掉 += 1;
            }
            v.to_string()
        })
        .collect();
    assert_eq!(去掉, 1);
    fs::write(d.join("led.jsonl"), 改.join("\n") + "\n").unwrap();
    let o = 导出(&d);
    assert!(!o.status.success());
    let err = String::from_utf8_lossy(&o.stderr);
    assert!(
        err.contains("E-ledger-corrupt") && err.contains("phys"),
        "{err}"
    );
    let _ = fs::remove_dir_all(&d);
}

/// 同一键下混了两种题型（打分与是非）→ 不知道取哪一列，报错
#[test]
fn 同一键题型不一致报错() {
    let d = 首跑(
        "mixed",
        "budget {calls: 4, cost: 1, depth: 16};\nlet a = judge(state(mat(\"甲\")), measure(\"这件事有多严重？\", [\"低\", \"中\", \"高\"], \"k\"));\nlet b = judge(state(mat(\"乙\")), test(\"行吗？\", \"k\"));\n0\n",
        json!({"observations": [
            {"on": ["甲"], "op": "measure", "text": "这件事有多严重？", "calib": "k",
             "scale": ["低", "中", "高"], "answer": {"Score": [0.1, 0.0, 0.9]}},
            {"on": ["乙"], "op": "test", "text": "行吗？", "calib": "k", "answer": {"Noul": 0.9}}
        ]}),
    );
    let o = 导出(&d);
    assert!(!o.status.success(), "题型不一致要报错");
    let err = String::from_utf8_lossy(&o.stderr);
    assert!(err.contains("题型不一致"), "{err}");
    let _ = fs::remove_dir_all(&d);
}
