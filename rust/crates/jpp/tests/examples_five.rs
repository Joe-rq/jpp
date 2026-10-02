//! 五个随仓示例（批量、过程、作者填模块、补信息、重放）：逐条执行 `examples/README.md` 里写的命令，
//! 输出与金样核对，不只看退出码。
//!
//! 金样是 `tests/golden/<用例>/projection.json`（`golden.rs` 的语义投影，不含路径，所以在仓库根用相对路径
//! 跑出来的与金样测试在临时目录用绝对路径跑出来的相同）；再加每个示例自己要演示的那一点。
//! 五条命令的文本只在本文件的表里写一份，README 必须逐字包含它们（改一边忘了另一边会失败）。
use serde_json::{Value, json};
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};

const 供应商目的: &str =
    "在这些供应商里找出最可能按期交付我们这份订单的，排好先后，并说出每家最大的风险。";
const 数轴目的: &str = "控制数轴上的一个点，尽快走到目标位置。";

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap()
}

/// 五条命令：（名字，金样用例，命令全文）
fn 命令表() -> Vec<(&'static str, &'static str, String)> {
    vec![
        (
            "批量",
            "purpose-run",
            format!(
                "jpp run examples/purpose-run.jpp --purpose \"{供应商目的}\" --fixtures examples/fixtures/purpose-run.json"
            ),
        ),
        (
            "过程",
            "purpose-drive",
            format!(
                "jpp run examples/purpose-drive.jpp --purpose \"{数轴目的}\" --fixtures examples/fixtures/purpose-drive.json --env 'walk=python3 worlds/walk.py'"
            ),
        ),
        (
            "作者填模块",
            "modules-fill",
            "jpp run examples/modules-fill.jpp --fixtures examples/fixtures/modules-fill.json"
                .into(),
        ),
        (
            "补信息",
            "unsure-default",
            "jpp run examples/unsure-default.jpp --fixtures examples/fixtures/unsure-default.json"
                .into(),
        ),
        (
            "重放",
            "purpose-run",
            format!(
                "jpp run examples/purpose-run.jpp --purpose \"{供应商目的}\" --replay tests/golden/purpose-run/ledger.json"
            ),
        ),
    ]
}

/// 只处理单引号、双引号与空白：够拆 README 里的命令
fn 拆词(s: &str) -> Vec<String> {
    let (mut out, mut cur, mut q, mut has) = (vec![], String::new(), None::<char>, false);
    for c in s.chars() {
        match (q, c) {
            (Some(x), c) if c == x => q = None,
            (Some(_), c) => cur.push(c),
            (None, '\'' | '"') => {
                q = Some(c);
                has = true;
            }
            (None, c) if c.is_whitespace() => {
                if has || !cur.is_empty() {
                    out.push(std::mem::take(&mut cur));
                    has = false;
                }
            }
            (None, c) => cur.push(c),
        }
    }
    if has || !cur.is_empty() {
        out.push(cur);
    }
    out
}

fn 跑(命令: &str) -> Value {
    let mut words = 拆词(命令);
    assert_eq!(words.remove(0), "jpp", "命令要以 jpp 开头：{命令}");
    let o = Command::new(env!("CARGO_BIN_EXE_jpp"))
        .current_dir(root())
        .args(&words)
        .output()
        .unwrap();
    assert!(
        o.status.success(),
        "{命令}\n退出码 {:?}\n{}",
        o.status.code(),
        String::from_utf8_lossy(&o.stderr)
    );
    serde_json::from_slice(&o.stdout).unwrap_or_else(|e| {
        panic!(
            "{命令}\nstdout 不是 JSON：{e}\n{}",
            String::from_utf8_lossy(&o.stdout)
        )
    })
}

/// 同 `golden.rs::project`：跨路径、跨账本格式仍应相等的那部分行为
fn 投影(report: &Value) -> Value {
    let events = report["trace"]["events"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    let effects: Vec<Value> = events
        .iter()
        .filter(|e| matches!(e["kind"].as_str(), Some("do" | "ask" | "gen")))
        .map(|e| json!([e["kind"], e["note"], e["key"], e["replayed"]]))
        .collect();
    let judges = events.iter().filter(|e| e["kind"] == "judge").count();
    let pending: Vec<Value> = report["pending"]
        .as_array()
        .cloned()
        .unwrap_or_default()
        .iter()
        .map(|p| json!([p["cause"], p["site"]]))
        .collect();
    json!({
        "status": report["status"],
        "value": report["value"],
        "returned_unsure": report["returned_unsure"],
        "pending": pending,
        "effects": effects,
        "judge_registrations": judges,
        "calls": report["cost"]["calls"],
        "asks": report["cost"]["asks"],
        "usd": report["cost"]["usd"],
    })
}

fn 金样投影(用例: &str) -> Value {
    let p = root()
        .join("tests/golden")
        .join(用例)
        .join("projection.json");
    serde_json::from_slice(&fs::read(&p).unwrap_or_else(|e| panic!("读不到 {}：{e}", p.display())))
        .unwrap()
}

#[test]
fn readme_逐字包含五条命令() {
    let readme = fs::read_to_string(root().join("examples/README.md")).unwrap();
    for (名字, _, 命令) in 命令表() {
        assert!(
            readme.contains(&命令),
            "examples/README.md 里没有「{名字}」示例的命令原文：\n{命令}"
        );
    }
}

#[test]
fn 五条命令的输出与金样一致() {
    let mut 首跑值 = Value::Null;
    for (名字, 用例, 命令) in 命令表() {
        let report = 跑(&命令);
        assert_eq!(report["status"], "returned", "{名字}");
        if 名字 == "重放" {
            // 重放：不带夹具，新增调用 0、新增提问 0，返回值与示例 1 的首跑逐字相同
            assert_eq!(report["replay"], true, "{名字}");
            assert_eq!(report["cost"]["calls"], 0, "{名字}");
            assert_eq!(report["cost"]["asks"], 0, "{名字}");
            assert!(report["cost"]["replayed"].as_u64().unwrap() > 0, "{名字}");
            assert_eq!(report["value"], 首跑值, "{名字}：重放的值与首跑不同");
            continue;
        }
        // 重录（与 golden.rs 同一个开关，B0630 起账本键换了要重录）：`JPP_GOLDEN_UPDATE=1 cargo test -p jpp --test examples_five`
        if std::env::var("JPP_GOLDEN_UPDATE").is_ok_and(|v| v == "1") {
            let p = root().join("tests/golden").join(用例).join("projection.json");
            fs::write(&p, serde_json::to_string_pretty(&投影(&report)).unwrap() + "\n").unwrap();
        }
        assert_eq!(
            投影(&report),
            金样投影(用例),
            "{名字}（{用例}）：输出与金样不一致"
        );
        match 名字 {
            "批量" => {
                首跑值 = report["value"].clone();
                let v = report["value"]["value"].as_array().unwrap();
                assert_eq!(v.len(), 3);
                assert!(
                    v.iter()
                        .all(|x| x["fields"]["rank"].is_i64() && x["fields"]["risk"].is_string()),
                    "{v:?}"
                );
                assert_eq!(report["value"]["pending"], json!([]));
            }
            "过程" => {
                let res = report["value"]["results"].as_array().unwrap();
                assert_eq!(res.len(), 2, "两局");
                assert!(
                    res.iter().all(|r| r["result"]["reached"] == true),
                    "{res:?}"
                );
                let acts: Vec<&str> = report["value"]["rows"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|r| r["action"].as_str().unwrap())
                    .collect();
                assert_eq!(acts, ["right", "right", "right", "left", "left"]);
            }
            "作者填模块" => {
                let v = report["value"]["value"].as_array().unwrap();
                assert_eq!(v.len(), 3);
                assert!(
                    v.iter()
                        .all(|x| x["fields"]["fit"].is_i64() && x["fields"]["flaw"].is_string()),
                    "{v:?}"
                );
                assert_eq!(report["value"]["questions"].as_array().unwrap().len(), 2);
            }
            "补信息" => {
                let ud = report["unsure_default"].as_array().unwrap();
                assert_eq!(ud.len(), 2);
                assert_eq!(
                    (ud[0]["end"].as_str(), ud[0]["fetched"].clone()),
                    (Some("decided"), json!(["过往项目"]))
                );
                assert_eq!(ud[1]["end"], "drop");
            }
            _ => unreachable!(),
        }
    }
    // 没有操作系统沙箱的机器上，带撤不回动作的过程示例会把账本写到源文件旁（INTERFACE §〇），README 的命令没给 --ledger-out
    let _ = fs::remove_file(root().join("examples/purpose-drive.ledger.jsonl"));
}

/// 过程示例不进 `golden.rs` 的清单：它的账本条目带世界调用的墙钟（`wall`），逐字节金样比不了。
/// 这里补它的重放：首跑留账本，再凭账本重放，不给 `--env` 也成功（不再启动世界进程），值逐字相同。
#[test]
fn 过程示例凭账本重放不再启动世界() {
    let (_, _, 命令) = 命令表().into_iter().find(|(n, _, _)| *n == "过程").unwrap();
    let dir = root().join(format!("target/examples-five-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    let ledger = dir.join("drive.ledger.json");
    let first = 跑(&format!("{命令} --ledger-out {}", ledger.display()));
    let replay_cmd = 命令
        .split(" --env ")
        .next()
        .unwrap()
        .replace(" --fixtures examples/fixtures/purpose-drive.json", "");
    let replay = 跑(&format!("{replay_cmd} --replay {}", ledger.display()));
    let _ = fs::remove_dir_all(&dir);
    assert_eq!(replay["replay"], true);
    assert_eq!(replay["cost"]["calls"], 0);
    assert!(replay["cost"]["replayed"].as_u64().unwrap() > 0);
    assert_eq!(replay["value"], first["value"], "重放的值与首跑不同");
}
