//! 规划器可靠下界的固定回归线（Z0190，复核-B0466 缺口 1）：跨 `tests/golden/manifest.json` 里全部可运行示例，
//! 核报告 `plan.calls` 的下界不超过实际调用数 `cost.calls`；上界已知的，实际也不超过上界。
//!
//! 数据来源是各例金样 `report.json` 里的 `plan` 段。金样由 `golden` 测试逐字节钉住真实运行的输出，
//! 所以「金样里的估计 ≤ 金样里的实际」等于「这一趟运行的估计 ≤ 这一趟运行的实际」。
//! 首跑（账本空、缓存关）的下界是可靠下界本身；续接与重放的下界为 0，这里一并核，不豁免。
//! `replay-report.json` 也核：重放新增调用为 0，下界必须为 0。
use serde_json::Value;
use std::{fs, path::Path};

fn root() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap()
}

/// 一份报告的越界之处：下界超过实际，或已知上界小于实际。无 `plan` 段视为缺失，也算问题。
fn violations(report: &Value) -> Vec<String> {
    let actual = report["cost"]["calls"].as_u64().expect("cost.calls");
    let calls = &report["plan"]["calls"];
    let mut out = vec![];
    match calls["kind"].as_str() {
        Some("known") => {
            let (lo, hi) = (calls["lo"].as_u64().unwrap(), calls["hi"].as_u64().unwrap());
            if lo > actual {
                out.push(format!("下界 {lo} > 实际 {actual}"));
            }
            if hi < actual {
                out.push(format!("上界 {hi} < 实际 {actual}"));
            }
        }
        Some("at_least") => {
            let lo = calls["lo"].as_u64().unwrap();
            if lo > actual {
                out.push(format!("下界 {lo} > 实际 {actual}"));
            }
        }
        Some("unknown") => {} // 未知不给承诺，也就没有可违背的
        _ => out.push("报告缺 plan.calls".to_string()),
    }
    out
}

#[test]
fn 下界不超过实际调用数跨全部可运行示例() {
    let root = root();
    let manifest: Value =
        serde_json::from_str(&fs::read_to_string(root.join("tests/golden/manifest.json")).unwrap())
            .unwrap();
    let mut checked = 0;
    let mut positive_lo = 0;
    let mut bad = vec![];
    for case in manifest["cases"].as_array().unwrap() {
        if case["expect"] == "error" {
            continue;
        }
        let name = case["name"].as_str().unwrap();
        for file in ["report.json", "replay-report.json"] {
            let path = root.join("tests/golden").join(name).join(file);
            let report: Value = serde_json::from_str(
                &fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}：{e}", path.display())),
            )
            .unwrap();
            checked += 1;
            if report["plan"]["calls"]["lo"].as_u64().unwrap_or(0) > 0 {
                positive_lo += 1;
            }
            for v in violations(&report) {
                bad.push(format!("{name}/{file}：{v}"));
            }
        }
    }
    assert!(bad.is_empty(), "可靠下界被打破：\n{}", bad.join("\n"));
    // 防止空转：可运行示例至少 45 个、每个两份报告；且确有下界大于 0 的（否则「下界 ≤ 实际」恒真，什么也没验）
    assert!(checked >= 90, "只核了 {checked} 份报告");
    assert!(positive_lo >= 20, "下界大于 0 的报告只有 {positive_lo} 份");
}

#[test]
fn 合成的越界报告会被抓出() {
    let mk = |plan: Value, actual: u64| serde_json::json!({"cost": {"calls": actual}, "plan": {"calls": plan}});
    let lo_over = mk(serde_json::json!({"kind": "at_least", "lo": 4}), 3);
    assert_eq!(violations(&lo_over), vec!["下界 4 > 实际 3".to_string()]);
    let known_lo_over = mk(serde_json::json!({"kind": "known", "lo": 4, "hi": 9}), 3);
    assert_eq!(violations(&known_lo_over).len(), 1);
    let hi_under = mk(serde_json::json!({"kind": "known", "lo": 1, "hi": 2}), 3);
    assert_eq!(violations(&hi_under), vec!["上界 2 < 实际 3".to_string()]);
    let ok = mk(serde_json::json!({"kind": "known", "lo": 3, "hi": 3}), 3);
    assert!(violations(&ok).is_empty());
    let missing = serde_json::json!({"cost": {"calls": 1}});
    assert_eq!(violations(&missing).len(), 1);
}
