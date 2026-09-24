//! 真值通道（件 a，B19）：标注导入 → 题式级线 → 只凭账本重放出口一致。
use serde_json::{Value, json};
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn jpp(args: &[&str]) -> (Value, String) {
    let out = Command::new(env!("CARGO_BIN_EXE_jpp")).current_dir(root()).args(args).output().unwrap();
    let stderr = String::from_utf8_lossy(&out.stderr).to_string();
    assert!(out.status.success(), "{stderr}");
    (serde_json::from_slice(&out.stdout).unwrap_or(Value::Null), stderr)
}

fn scratch(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("jpp-truth-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&d);
    fs::create_dir_all(&d).unwrap();
    d
}

fn write_rows(path: &Path, rows: &[Value]) {
    fs::write(path, rows.iter().map(|r| r.to_string()).collect::<Vec<_>>().join("\n")).unwrap();
}

/// 题式 `这段话是否提到了{city}？`（与 examples/sieve.jpp 同一规格，所以落在同一个题式键上）
fn mention() -> Value {
    json!({"op": "test", "template": "这段话是否提到了{city}？"})
}

#[test]
fn computed_truth_certifies_both_sides_and_replay_needs_only_the_ledger() {
    let d = scratch("replay");
    let mut rows = vec![];
    // B24 拆分认证：认证半每侧要有零错误所需的条数（α=δ=0.1 时约 22），所以每侧 100 条
    for i in 0..100 {
        rows.push(json!({"form": mention(), "item": format!("pos{i}"), "p": 0.97 + (i % 3) as f64 * 0.01, "label": true, "source": "computed"}));
        rows.push(json!({"form": mention(), "item": format!("neg{i}"), "p": 0.01 + (i % 3) as f64 * 0.01, "label": false, "source": "computed"}));
    }
    let labels = d.join("labels.jsonl");
    write_rows(&labels, &rows);
    let calib = d.join("calib");
    let (report, _) = jpp(&["calib-import", labels.to_str().unwrap(), "--calib-out", calib.to_str().unwrap()]);
    assert_eq!(report[0]["status"], "上岗");
    assert!(report[0]["lo"].as_f64().unwrap() > 0.0, "两侧认证：Ignore 出口可达");
    assert!(report[0]["lo"].as_f64().unwrap() < report[0]["hi"].as_f64().unwrap());

    // 夹具里没有 form-mention 的线：出口只能来自题式级回退，且要留痕
    let ledger = d.join("ledger.json");
    let first = d.join("first.json");
    jpp(&["run", "examples/sieve.jpp", "--fixtures", "examples/fixtures/sieve-truth.json", "--calib", calib.to_str().unwrap(),
          "--ledger-out", ledger.to_str().unwrap(), "--output", first.to_str().unwrap()]);
    let first: Value = serde_json::from_slice(&fs::read(&first).unwrap()).unwrap();
    let warnings = first["trace"]["warnings"].to_string();
    assert!(warnings.contains("W-form-line"), "用了题式级线就要说出来：{warnings}");
    assert_eq!(first["value"]["streams"][0]["act"], json!([0, 3, 8, 9]));

    // 只凭账本重放：不给 --calib、不给 --fixtures，出口与首跑逐字节一致
    let again = d.join("again.json");
    let (_, stderr) = jpp(&["run", "examples/sieve.jpp", "--replay", ledger.to_str().unwrap(), "--output", again.to_str().unwrap()]);
    assert!(stderr.contains("从账本补回"), "{stderr}");
    let again: Value = serde_json::from_slice(&fs::read(&again).unwrap()).unwrap();
    assert_eq!(again["cost"]["calls"], 0);
    assert_eq!(again["value"], first["value"]);
}

#[test]
fn model_truth_waits_for_a_spot_check_above_the_gate() {
    let d = scratch("gate");
    let mut rows = vec![];
    for i in 0..40 {
        let truth = i % 2 == 0;
        rows.push(json!({"form": mention(), "item": format!("m{i}"), "p": if truth { 0.95 } else { 0.05 }, "label": truth, "source": "model:test"}));
    }
    // 人工抽检 10 条，其中 3 条与模型相反：一致率 0.7 < 0.9
    for i in 0..10 {
        let truth = i % 2 == 0;
        rows.push(json!({"form": mention(), "item": format!("m{i}"), "p": 0.5, "label": if i < 3 { !truth } else { truth }, "source": "human", "spot_check": "b1"}));
    }
    rows.push(json!({"form": mention(), "item": "amb", "p": 0.5, "label": "ambiguous", "source": "model:test"}));
    let labels = d.join("labels.jsonl");
    write_rows(&labels, &rows);
    let calib = d.join("calib");
    let (report, _) = jpp(&["calib-import", labels.to_str().unwrap(), "--calib-out", calib.to_str().unwrap()]);
    assert_eq!(report[0]["status"], "待真值");
    let gate = report[0]["truth"]["gate"].as_str().unwrap();
    assert!(gate.starts_with("待核") && gate.contains("0.70"), "{gate}");
    assert_eq!(report[0]["truth"]["spot_check"]["n"], 10);
    assert_eq!(report[0]["truth"]["ambiguous"], 1);
}
