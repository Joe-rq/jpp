//! 步 34 V5 预测 P5：仓库里金样之外的 v3、v4 账本在 v5 二进制上照读无损。
//!
//! 手动跑（要一份账本路径清单，一行一个，相对 `地基/` 的父目录或绝对路径）：
//! `JPP_V5_OLD_LEDGERS=<清单> scripts/cargoq --local test -p jpp-ledger --test v5_old_ledgers -- --ignored --nocapture`
//! 清单由 `地基/过程记录/工程-V5-金样核对.py list-old` 生成。
//!
//! 判据：解码成功；头行按原版本号重新编码与原头行逐字节相同；条目按原头行的链重新编码与原文逐行逐字节相同
//! （读进来的内容一字不丢，v5 只改了头行版本号，`prev` 链随之重算）。逐字节依赖浮点正确舍入：`jpp-ledger` 自己
//! 声明了 serde_json 的 `float_roundtrip`（V5 复核 S1），单独 `-p jpp-ledger` 构建也成立。
use jpp_ledger::{Ledger, encode_head, line_hash};

#[test]
#[ignore]
fn 仓库旧账本照读无损() {
    let Ok(list) = std::env::var("JPP_V5_OLD_LEDGERS") else {
        eprintln!("没给 JPP_V5_OLD_LEDGERS，跳过");
        return;
    };
    let base = std::env::var("JPP_V5_BASE").unwrap_or_default();
    let paths: Vec<String> = std::fs::read_to_string(&list)
        .unwrap()
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| {
            if base.is_empty() {
                l.to_string()
            } else {
                format!("{base}/{l}")
            }
        })
        .collect();
    let (mut ok, mut trunc) = (0, 0);
    let mut bad = vec![];
    for p in &paths {
        let text = std::fs::read_to_string(p).unwrap();
        let (l, t) = match Ledger::decode(&text) {
            Ok(x) => x,
            Err(e) => {
                bad.push(format!("{p}：解码失败 {e}"));
                continue;
            }
        };
        let lines: Vec<&str> = text.lines().collect();
        let orig_head = lines[0];
        let v: serde_json::Value = serde_json::from_str(orig_head).unwrap();
        let ver = v["version"].as_u64().unwrap();
        let head = encode_head(&l.header).replacen(
            &format!("{{\"version\":{},", jpp_ledger::LEDGER_VERSION),
            &format!("{{\"version\":{ver},"),
            1,
        );
        if head != orig_head {
            bad.push(format!("{p}：头行重编码不同"));
            continue;
        }
        let (re, _) = l.encode_from(0, &line_hash(orig_head));
        let n = re.len();
        if let Some((a, b)) = re
            .iter()
            .map(String::as_str)
            .zip(lines[1..=n].iter().copied())
            .find(|(a, b)| a != b)
        {
            if bad.len() < 3 {
                eprintln!("新 {a}\n旧 {b}");
            }
            bad.push(format!("{p}：条目重编码不同"));
            continue;
        }
        if t.is_some() {
            trunc += 1;
        }
        ok += 1;
    }
    eprintln!(
        "旧账本 {} 份：逐字节照读无损 {ok}（末行半写截断 {trunc}），问题 {}",
        paths.len(),
        bad.len()
    );
    for b in &bad {
        eprintln!("  {b}");
    }
    assert!(bad.is_empty());
}
