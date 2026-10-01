//! Z0594（过程记录 5.29）：`jpp run` 文本模式也把运行期告警打到 stderr——同码同址折叠，告警带 `@偏移` 的渲染成
//! `文件:行:列`（`W-unsure-default` 这类汇总告警不带位置，照「编号: 报文」打），报告写完之后；
//! `--json` 时照旧是 JSON Lines，不另打文本行。

use std::path::PathBuf;
use std::process::Command;

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn 跑(json: bool) -> (String, serde_json::Value) {
    let d = std::env::temp_dir().join(format!("jpp-z0594-{}-{json}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    let mut args = vec![
        "run".to_string(),
        root()
            .join("examples/unsure-default.jpp")
            .display()
            .to_string(),
        "--fixtures".into(),
        root()
            .join("examples/fixtures/unsure-default.json")
            .display()
            .to_string(),
        "--output".into(),
        d.join("r.json").display().to_string(),
        "--ledger-out".into(),
        d.join("l.json").display().to_string(),
    ];
    if json {
        args.push("--json".into());
    }
    let o = Command::new(env!("CARGO_BIN_EXE_jpp"))
        .args(&args)
        .output()
        .unwrap();
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    let r: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(d.join("r.json")).unwrap()).unwrap();
    let _ = std::fs::remove_dir_all(&d);
    (String::from_utf8_lossy(&o.stderr).to_string(), r)
}

#[test]
fn 文本模式_运行期告警打到stderr() {
    let (err, r) = 跑(false);
    let 报告里 = r["trace"]["warnings"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|w| w.as_str().unwrap().starts_with("W-unsure-default"))
        .count();
    assert!(
        报告里 >= 1,
        "本例应有 W-unsure-default：{}",
        r["trace"]["warnings"]
    );
    let 行: Vec<&str> = err
        .lines()
        .filter(|l| l.contains("W-unsure-default"))
        .collect();
    assert!(!行.is_empty(), "stderr 应带 W-unsure-default：\n{err}");
    assert!(
        行.iter().all(|l| l.contains("W-unsure-default: ")),
        "编号与报文：{行:?}"
    );
    assert!(行.len() <= 报告里, "同码同址折叠，不多于报告里的条数");
}

#[test]
fn json模式_照旧是json_lines() {
    let (err, _) = 跑(true);
    let 相关: Vec<&str> = err
        .lines()
        .filter(|l| l.contains("W-unsure-default"))
        .collect();
    assert!(!相关.is_empty(), "{err}");
    assert!(
        相关
            .iter()
            .all(|l| serde_json::from_str::<serde_json::Value>(l).is_ok()),
        "--json 时是 JSON 行，不另打文本：{相关:?}"
    );
}
