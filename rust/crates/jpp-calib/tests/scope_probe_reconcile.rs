//! B68 修订实现对账：用实现代码（导入后的校准记录 + `material_fingerprint` + `outside`）复算
//! `probes/scope/sets.json` 里每个材料集的判出数，与 Rust 之外算出的预期（`export_sets.py`，
//! 口径同 `rule_gradient.py` 规则 B：m = 0.10、k = 2）逐项相等。
//! 依据：`地基/附注/2026-09-24-探针首轮裁定.md` B68 修订的验证判据。

use jpp_value::stat::{ScopeRanges, material_fingerprint};
use std::path::PathBuf;

fn scope_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../probes/scope")
}

#[test]
fn b68_revision_matches_out_of_rust_expectations() {
    let rec: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(scope_dir().join("calib/_form_f84be62010fca87cacb0637e.json"))
            .unwrap(),
    )
    .unwrap();
    let fp: ScopeRanges = serde_json::from_value(rec["scope"]["fingerprint"].clone()).unwrap();
    assert!(fp.margins.is_some(), "重导入后的记录应带边距");

    let sets: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(scope_dir().join("sets.json")).unwrap())
            .unwrap();
    // 区间与 Rust 之外算出的一致
    for ((_, lo, hi), exp) in fp
        .ranges
        .iter()
        .zip(sets["ranges_expected"].as_array().unwrap())
    {
        let (elo, ehi) = (exp[0].as_f64().unwrap(), exp[1].as_f64().unwrap());
        assert!(
            (lo - elo).abs() < 1e-9 && (hi - ehi).abs() < 1e-9,
            "区间 {lo},{hi} ≠ 预期 {elo},{ehi}"
        );
    }
    let mut rows = vec![];
    for s in sets["sets"].as_array().unwrap() {
        let name = s["name"].as_str().unwrap();
        let texts = s["texts"].as_array().unwrap();
        let got = texts
            .iter()
            .filter(|t| {
                fp.outside(&material_fingerprint(t.as_str().unwrap()))
                    .is_some()
            })
            .count();
        let want = s["expected_outside"].as_u64().unwrap() as usize;
        rows.push(format!("{name}: {got}/{} (预期 {want})", texts.len()));
        assert_eq!(got, want, "{name}");
    }
    eprintln!("{}", rows.join("\n"));
}
