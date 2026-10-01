//! `jpp bank-stats`：从账本聚合每条题式的使用统计（步 27；B48、规范 §1.5）。
//!
//! 证明项：跑两个用题库的示例（`bank-json_field`、`bank-answers_question`，固定后端，0 美元），
//! `bank-stats` 输出非零统计，且出口分布与对应金样报告 `exits` 表一致（重算与运行时同一个判序函数）。

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use jpp::ledger::{Entry, Ledger};
use jpp::store::bank_stats::aggregate;
use serde_json::Value as Json;

const F1: &str = "ceb097176786d327fe1a0ba3"; // json_field
const F2: &str = "1d77f7a203b4258048dfefea"; // answers_question

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn scratch(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("jpp-bank-stats-{}-{tag}", std::process::id()));
    let _ = fs::remove_dir_all(&d);
    fs::create_dir_all(&d).unwrap();
    d
}

fn run_example(slug: &str, hash: &str, dir: &Path) -> PathBuf {
    let led = dir.join(format!("{slug}.jsonl"));
    let o = Command::new(env!("CARGO_BIN_EXE_jpp"))
        .current_dir(dir)
        .args([
            "run".into(),
            root()
                .join(format!("examples/bank-{slug}.jpp"))
                .display()
                .to_string(),
            "--fixtures".into(),
            root()
                .join(format!("examples/fixtures/bank-{slug}.json"))
                .display()
                .to_string(),
            "--calib".into(),
            root()
                .join(format!("bank/entries/{hash}/calib"))
                .display()
                .to_string(),
            "--ledger-out".into(),
            led.display().to_string(),
        ])
        .output()
        .unwrap();
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    led
}

fn decode(p: &Path) -> Ledger {
    Ledger::decode(&fs::read_to_string(p).unwrap()).unwrap().0
}

fn roster() -> Vec<String> {
    let b: Json =
        serde_json::from_slice(&fs::read(root().join("bank/bank.json")).unwrap()).unwrap();
    b["entries"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["form_hash"].as_str().unwrap().to_string())
        .collect()
}

fn golden_exit_counts(case: &str) -> (usize, usize, usize) {
    let r: Json = serde_json::from_slice(
        &fs::read(root().join(format!("tests/golden/{case}/report.json"))).unwrap(),
    )
    .unwrap();
    let ex = r["exits"].as_array().unwrap();
    let n = |k: &str| ex.iter().filter(|e| e["exit"] == k).count();
    let unsure = ex
        .iter()
        .filter(|e| e["exit"].as_str().is_some_and(|s| s.starts_with("unsure")))
        .count();
    (n("act"), n("ignore"), unsure)
}

#[test]
fn two_bank_examples_give_nonzero_stats_matching_the_golden_exits() {
    let d = scratch("two");
    let l1 = decode(&run_example("json_field", F1, &d));
    let l2 = decode(&run_example("answers_question", F2, &d));
    let st = aggregate(&[l1, l2], &roster());
    let a = &st.forms[F1];
    assert_eq!((a.calls, a.reused, a.runs), (4, 0, 1));
    assert_eq!(a.cost, 0.0);
    assert_eq!(a.in_band, 0);
    let b = &st.forms[F2];
    assert_eq!((b.calls, b.reused, b.runs), (4, 0, 1));
    assert_eq!(b.in_band, 1, "读数 0.54 那一对落在带内");
    assert_eq!(b.in_band_share(), Some(0.25));
    // 出口分布与金样报告 exits 表一致
    let (act, ign, uns) = golden_exit_counts("bank-json_field");
    assert_eq!(
        (
            a.exits.get("act"),
            a.exits.get("ignore"),
            a.exits.get("unsure")
        ),
        (Some(&(act as u64)), Some(&(ign as u64)), None),
        "json_field 金样：act {act} ignore {ign} unsure {uns}"
    );
    let (act, ign, uns) = golden_exit_counts("bank-answers_question");
    let g = |k: &str| b.exits.get(k).copied().unwrap_or(0) as usize;
    assert_eq!((g("act"), g("ignore"), g("unsure")), (act, ign, uns));
    assert_eq!(b.unsure_causes.get("band"), Some(&1));
    // 读数只有 4 条：样本不足，不出漂移信号
    assert_eq!(b.drift(), "insufficient_sample");
    // 其余名单内题式在列、全 0；没有无归属条目、没有名单外题式
    let zero = st
        .forms
        .iter()
        .filter(|(h, _)| h.as_str() != F1 && h.as_str() != F2)
        .all(|(_, s)| s.calls == 0 && s.reused == 0 && s.cost == 0.0 && s.exits.is_empty());
    assert!(zero);
    // 名单内题式数 = bank.json 的条目数（不写死，题库会增条目）
    assert_eq!(st.forms.len(), roster().len());
    assert_eq!((st.unattributed_calls, st.unattributed_reused), (0, 0));
    assert!(st.outside.is_empty());
    let _ = fs::remove_dir_all(&d);
}

#[test]
fn ledgers_without_form_key_are_reported_unattributed_not_guessed() {
    let d = scratch("old");
    let mut l = decode(&run_example("json_field", F1, &d));
    // 旧账本：判断条目没有 calib_ref.key
    for e in l.entries.iter_mut() {
        if let Entry::Judge { calib_ref, .. } = e {
            calib_ref.as_mut().unwrap().key = None;
        }
    }
    let st = aggregate(&[l], &roster());
    assert_eq!(st.unattributed_calls, 4);
    assert!(st.forms.values().all(|s| s.calls == 0), "不猜归属");
    let _ = fs::remove_dir_all(&d);
}

#[test]
fn drift_signal_needs_enough_readings_and_three_times_the_certified_band_rate() {
    let d = scratch("drift");
    let l = decode(&run_example("answers_question", F2, &d));
    // 把同一份账本聚合 10 次：读数 40 条、带内 10 条（0.25），认证集未决率 0.0125，超过三倍且样本够
    let ls: Vec<Ledger> = (0..10).map(|_| l.clone()).collect();
    let st = aggregate(&ls, &roster());
    let s = &st.forms[F2];
    assert_eq!((s.calls, s.in_band), (40, 10));
    assert_eq!(s.drift(), "signal");
    // json_field 带内 0，样本够也不出信号
    let l1 = decode(&run_example("json_field", F1, &d));
    let st = aggregate(&(0..10).map(|_| l1.clone()).collect::<Vec<_>>(), &roster());
    assert_eq!(st.forms[F1].drift(), "none");
    let _ = fs::remove_dir_all(&d);
}

#[test]
fn cli_bank_stats_prints_nonzero_json_for_two_ledgers() {
    let d = scratch("cli");
    let a = run_example("json_field", F1, &d);
    let b = run_example("answers_question", F2, &d);
    let o = Command::new(env!("CARGO_BIN_EXE_jpp"))
        .current_dir(root())
        .args([
            "bank-stats".to_string(),
            a.display().to_string(),
            b.display().to_string(),
            "--bank".into(),
            "bank".into(),
            "--json".into(),
        ])
        .output()
        .unwrap();
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    let j: Json = serde_json::from_slice(&o.stdout).unwrap();
    assert_eq!(j["forms"][F1]["calls"], 4);
    assert_eq!(j["forms"][F2]["calls"], 4);
    assert_eq!(j["forms"][F1]["slug"], "json_field");
    assert_eq!(j["forms"][F2]["exits"]["unsure"], 1);
    assert_eq!(j["unattributed"]["calls"], 0);
    // 文本模式也能出表
    let o = Command::new(env!("CARGO_BIN_EXE_jpp"))
        .current_dir(root())
        .args([
            "bank-stats".to_string(),
            a.display().to_string(),
            "--bank".into(),
            "bank".into(),
        ])
        .output()
        .unwrap();
    assert!(o.status.success());
    let text = String::from_utf8_lossy(&o.stdout);
    assert!(
        text.contains("json_field") && text.contains("无题式归属"),
        "{text}"
    );
    let _ = fs::remove_dir_all(&d);
}
