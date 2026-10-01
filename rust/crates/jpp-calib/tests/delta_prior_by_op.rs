//! Z0238：`calib-import`（`import_labels`）在记录没有 δ 时取画像的 δ 先验，要按行的题型取对的那一列：
//! test → noul、select → choice、measure → score。
//!
//! 缺陷：`truth.rs` 把语义操作名（`select` / `measure`）交给只认物理名（`noul` / `choice` / `score`）的
//! `反查题型`，查不到就静默落到 test，select、measure 都拿到 noul 的 δ（`db7000356` 起，步 15d-2）。
//! 画像三列取互不相同、也不等于发行画像的数，任何两列混用都会被断言抓到。
//! 同时断言记录的 `delta`、选中证书的 `selection.delta` 与 `unsure_rate_delta`：三处都来自同一个 δ。
//!
//! 依据：`21` 步 15d-2（δ 只从画像取、按题型）；B73「数字只住画像」；
//! `地基/过程记录/工程-Z0238-认证δ映射.md`。

use jpp_calib::CalibStore;
use jpp_calib::truth::{CertifyMethod, ImportOptions, LabelRow, import_labels};

const 先验: (f64, f64, f64) = (0.03, 0.07, 0.11);

fn 选项() -> ImportOptions {
    ImportOptions {
        alpha: 0.1,
        conf_delta: 0.1,
        spot_check_min: 0.9,
        spot_check_conf: 0.95,
        abstain_warn: 0.1,
        batch: "z0238".into(),
        seed: 1,
        extent_min_disagree: 3,
        extent_same_dir: 0.8,
        extent_same_tier: 2.0 / 3.0,
        scope_quantiles: (0.01, 0.99),
        scope_margins: Default::default(),
        class_min_sources: 2,
        alpha_trial: None,
        certify: CertifyMethod::FixedSequence,
        step: None,
        sequential: None,
    }
}

fn 库() -> CalibStore {
    let mut s = CalibStore::new();
    s.profile.delta = jpp_effects::Field::known(先验, "测试：三列互不相同");
    s
}

/// 60 条计算真值的行；K 元行高 p_max 且 argmax 全对，test 行两极全对。
fn 行(op: &str) -> Vec<LabelRow> {
    (0..60)
        .map(|i| {
            let v = match op {
                "test" => {
                    let yes = i % 2 == 0;
                    let p = if yes { 0.9 } else { 0.02 } + (i % 10) as f64 * 0.008;
                    serde_json::json!({"key": "k", "item": format!("m{i}"), "p": p,
                                       "label": yes, "source": "computed"})
                }
                _ => {
                    let truth = i % 3;
                    serde_json::json!({"key": "k", "op": op, "item": format!("m{i}"),
                                       "p": 0.9 + (i % 10) as f64 * 0.009, "pick": truth,
                                       "label": truth, "source": "computed"})
                }
            };
            serde_json::from_value(v).unwrap()
        })
        .collect()
}

/// 导入一种题型，返回（记录 δ、选中证书的 `selection.delta`、`unsure_rate_delta`）。
fn 导入(op: &str) -> (Option<f64>, Option<f64>, Option<f64>) {
    let mut s = 库();
    let rep = import_labels(&mut s, &行(op), &选项()).unwrap();
    assert_eq!(rep[0].status, "上岗", "{op}: {}", rep[0].truth.gate);
    let r = s.get("k");
    let c = r.选中的证书().expect("上岗记录有证书");
    let sel = c
        .selection
        .as_ref()
        .expect("固定序是平移生产者，写 selection");
    (r.delta, sel.delta, r.unsure_rate_delta)
}

#[test]
fn select_import_takes_choice_prior() {
    let d = Some(先验.1);
    assert_eq!(导入("select"), (d, d, d), "select 行应取画像 choice 列的 δ");
}

#[test]
fn measure_import_takes_score_prior() {
    let d = Some(先验.2);
    assert_eq!(
        导入("measure"),
        (d, d, d),
        "measure 行应取画像 score 列的 δ"
    );
}

/// 对照：test 行取 noul 列（修前修后都对；证明本文件的装置本身没坏）。
#[test]
fn test_import_takes_noul_prior() {
    let mut s = 库();
    let rep = import_labels(&mut s, &行("test"), &选项()).unwrap();
    assert_eq!(rep[0].status, "上岗", "{}", rep[0].truth.gate);
    let r = s.get("k");
    assert_eq!(r.delta, Some(先验.0));
    let c = r.选中的证书().expect("上岗记录有证书");
    assert_eq!(c.selection.as_ref().unwrap().delta, Some(先验.0));
}
