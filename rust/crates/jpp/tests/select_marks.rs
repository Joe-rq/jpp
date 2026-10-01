//! 步 29（主控:B0460，B20 第 1 种）：`lib/skeletons/select.jpp` 递归筛 + `FileMatStore` 标记存储。
//! 预注册见 `地基/过程记录/工程-步29-选材料.md` 第八节（夹具：6 份材料、目的 A 题链 [P1, P2]、目的 B 题链 [P2, P3]）。
//!
//! **临时存储，骨架最终裁定后换**：本测试钉的是接口行为（标记内容、来源出口键、别的目的复用、少发的题），
//! 不钉文件格式的细节以外的东西。

mod common;
use std::cell::Cell;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::atomic::{AtomicUsize, Ordering};

use jpp::effects::{CalibStore, EffectError, FnPort, JudgeResult, NoCallPorts, Ports};
use jpp::interp::ActionRegistry;
use jpp::ledger::{Entry, Ledger};
use jpp::store::{DirBlob, FileMatStore, MemBlob};
use jpp::value::{Answer, Question, State};
use jpp::{EntryArgs, Program, Session};
use jpp_effects::views::MatStorePort;
use serde_json::{Value as Json, json};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

static NEXT: AtomicUsize = AtomicUsize::new(0);

fn 编译(src: &str) -> Program {
    let dir = root().join(format!(
        "target/select-marks-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("p.jpp");
    std::fs::write(&path, src).unwrap();
    let loaded = jpp::syntax::loader::load(&path);
    let _ = std::fs::remove_dir_all(&dir);
    jpp::lower(&loaded.expect("装载").program).expect("lower")
}

const P1: &str = "这段材料是否提到了具体金额？";
const P2: &str = "这段材料是否出自合同文本？";
const P3: &str = "这段材料是否涉及付款期限？";

/// 读数：`A`=留（0.99）、`I`=淘（0.01）、`U`=未决（0.5，作者声明线 0.1..0.9 内是 band）
fn 读数(id: i64, q: &str, 第二次: bool) -> f64 {
    let 表: [&str; 6] = match (q, 第二次) {
        (P1, _) => ["A", "A", "A", "I", "I", "A"],
        (P2, false) => ["A", "I", "U", "I", "A", "A"],
        (P2, true) => ["A", "I", "A", "I", "A", "A"],
        (P3, _) => ["A", "I", "I", "I", "A", "A"],
        _ => panic!("没有这道题：{q}"),
    };
    match 表[(id - 1) as usize] {
        "A" => 0.99,
        "I" => 0.01,
        _ => 0.5,
    }
}

const 头: &str = r#"import "../../lib/skeletons/select.jpp";
budget {calls: 40, cost: 0.01, depth: 256};
let pool = [
    mat({id: 1, text: "甲方应于签约日支付定金三万元。"}),
    mat({id: 2, text: "会议纪要：下周二讨论排期。"}),
    mat({id: 3, text: "补充条款：逾期按日千分之一计违约金。"}),
    mat({id: 4, text: "午饭吃什么。"}),
    mat({id: 5, text: "本合同自双方签字之日起生效。"}),
    mat({id: 6, text: "合同总价为人民币十二万元，分三期支付。"})
];
let L = {declare: {hi: 0.9, lo: 0.1}};
"#;

fn 程序(目的: &str, 前提: &[&str], 选项: &str) -> String {
    let 链: Vec<String> = 前提
        .iter()
        .map(|p| format!("{{q: \"{p}\", line: L}}"))
        .collect();
    format!(
        "{头}let r = select(\"{目的}\", pool, [{}], {选项});\n\
         {{ids: map(r.value, fn(e) {{ content(e.item).id }}),\n\
          marks: map(r.value, fn(e) {{ e.marks }}),\n\
          pool_hashes: map(pool, fn(m) {{ m.hash }}),\n\
          pending: map(r.pending, fn(p) {{ {{id: content(p.item).id, cause: p.cause, exit: p.exit}} }}),\n\
          per_layer: r.detail.per_layer, reason: r.detail.reason, layers: r.detail.layers,\n\
          stored: r.detail.stored, evidence: r.evidence}}",
        链.join(", ")
    )
}

struct 一趟 {
    value: Json,
    ledger: Ledger,
    端口调用: usize,
}

fn 跑(src: &str, 存: Option<&Rc<FileMatStore>>, 第二次: bool, ledger: Ledger) -> 一趟 {
    let 调用 = Cell::new(0usize);
    let 结果 = {
        let ports = Ports::new().with(common::伴随中性judge(
            "fixed-0",
            |s: &State, qs: &[&Question]| {
                let id = s.on[0].content["id"].as_i64().expect("材料带 id");
                let answers = qs
                    .iter()
                    .map(|q| {
                        调用.set(调用.get() + 1);
                        Answer::Noul(读数(id, &q.text, 第二次))
                    })
                    .collect::<Vec<_>>();
                Ok::<_, EffectError>(JudgeResult {
                    tokens: 0,
                    cost: 0.0,
                    mode_share: vec![None; answers.len()],
                    perms: vec![0; answers.len()],
                    confidence: vec![],
                    answers,
                })
            },
        ));
        let calib = CalibStore::new();
        let acts = ActionRegistry::new();
        let mut s =
            Session::new(ports, &calib, &acts).with_companions(jpp::interp::CompanionMode::Off);
        if let Some(st) = 存 {
            s = s.with_mat_store(st.clone());
        }
        let mut l = ledger;
        let o = s
            .run(&编译(src), &EntryArgs::default(), &mut l)
            .unwrap_or_else(|e| panic!("{}", e.render()));
        (o.value_json(), l)
    };
    一趟 {
        value: 结果.0,
        ledger: 结果.1,
        端口调用: 调用.get(),
    }
}

fn 新库() -> Rc<FileMatStore> {
    Rc::new(FileMatStore::new(Box::new(MemBlob::new())))
}

fn 判断条目数(l: &Ledger) -> usize {
    l.entries
        .iter()
        .filter(|e| {
            matches!(
                e,
                Entry::Judge {
                    reused_from: None,
                    ..
                }
            )
        })
        .count()
}

fn 判断键(l: &Ledger) -> Vec<String> {
    l.entries
        .iter()
        .filter_map(|e| match e {
            Entry::Judge { key, .. } => Some(key.clone()),
            _ => None,
        })
        .collect()
}

fn 变换条目数(l: &Ledger) -> usize {
    l.entries
        .iter()
        .filter(|e| matches!(e, Entry::Effect { kind, .. } if kind == "transform"))
        .count()
}

fn 待定(v: &Json) -> Vec<(i64, String)> {
    v["pending"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| {
            (
                p["id"].as_i64().unwrap(),
                p["cause"].as_str().unwrap().to_string(),
            )
        })
        .collect()
}

fn 层(v: &Json) -> Vec<(u64, u64, u64, u64, u64, u64)> {
    v["per_layer"]
        .as_array()
        .unwrap()
        .iter()
        .map(|l| {
            let g = |k: &str| l[k].as_u64().unwrap();
            (
                g("alive"),
                g("asked"),
                g("reused"),
                g("act"),
                g("ignore"),
                g("unsure"),
            )
        })
        .collect()
}

#[test]
fn 第一次运行_目的a_空料库() {
    let 库 = 新库();
    let r = 跑(&程序("A", &[P1, P2], "{}"), Some(&库), false, Ledger::new());
    let v = &r.value;
    // 预测 1：两层，(存活, 问, 复用, act, ignore, unsure)
    assert_eq!(层(v), vec![(6, 6, 0, 4, 2, 0), (4, 4, 0, 2, 1, 1)], "{v}");
    // 预测 2
    assert_eq!(r.端口调用, 10);
    assert_eq!(判断条目数(&r.ledger), 10);
    // 预测 3
    assert_eq!(v["ids"], json!([1, 3, 6]));
    assert_eq!(待定(v), vec![(3, "band".to_string())]);
    assert_eq!(v["reason"], "chain_done");
    assert_eq!(v["stored"], true);
    // 预测 4：料库写入 10 条标记；账本 transform 条目 4
    assert_eq!(库.mark_count(), 10);
    assert_eq!(变换条目数(&r.ledger), 4);
    let 判键 = 判断键(&r.ledger);
    let hs: Vec<String> = v["pool_hashes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|h| h.as_str().unwrap().to_string())
        .collect();
    let mut 见过 = std::collections::BTreeSet::new();
    for h in &hs {
        for m in 库.marks_full(h) {
            assert!(判键.contains(&m.source_key), "来源出口键不在账本里：{m:?}");
            assert!(见过.insert(m.source_key.clone()), "来源出口键应各不相同");
            assert_eq!(m.first_purpose, "A");
            assert!([P1, P2].contains(&m.premise.as_str()));
        }
    }
    // M3（下标 2）的 P2 标记是未决
    let m3 = 库.marks_full(&hs[2]);
    let p2 = m3.iter().find(|m| m.premise == P2).unwrap();
    assert_eq!(p2.exit, "unsure:band");
    let p1 = m3.iter().find(|m| m.premise == P1).unwrap();
    assert_eq!(p1.exit, "act");
    // 返回值里每份存活材料带着标记
    assert_eq!(v["marks"][1].as_array().unwrap().len(), 2);
    assert_eq!(v["marks"][1][1]["exit"], "unsure:band");
}

#[test]
fn 第二次运行_目的b_复用目的a留下的标记() {
    let 库 = 新库();
    let 一 = 跑(&程序("A", &[P1, P2], "{}"), Some(&库), false, Ledger::new());
    let 一键 = 判断键(&一.ledger);
    let hs: Vec<String> = 一.value["pool_hashes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|h| h.as_str().unwrap().to_string())
        .collect();
    // 第二次运行之前，第一次运行留在 M1、M2、M6 上的 P2 标记（已决，会被复用）的来源出口键
    let 待复用: Vec<String> = [0usize, 1, 5]
        .iter()
        .map(|&i| {
            let m = 库.marks_full(&hs[i]);
            let p2 = m.iter().find(|m| m.premise == P2).unwrap();
            assert!(p2.decided());
            p2.source_key.clone()
        })
        .collect();
    let 二 = 跑(&程序("B", &[P2, P3], "{}"), Some(&库), true, Ledger::new());
    let v = &二.value;
    // 预测 5
    assert_eq!(层(v), vec![(6, 3, 3, 4, 2, 0), (4, 4, 0, 3, 1, 0)], "{v}");
    // 预测 6：端口只收到 7 次；复用的 3 条来源键是第一次运行的键，不在第二次账本里
    assert_eq!(二.端口调用, 7);
    assert_eq!(判断条目数(&二.ledger), 7);
    let 二键 = 判断键(&二.ledger);
    let 复用键: Vec<String> = v["marks"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|ms| ms.as_array().unwrap().iter())
        .filter(|m| m["reused"] == true)
        .map(|m| m["source_key"].as_str().unwrap().to_string())
        .collect();
    // 返回值只带存活材料的标记，M2 被复用的 ignore 淘掉了，所以这里只看得到 M1、M6 两条复用
    assert_eq!(复用键.len(), 2, "{v}");
    for k in &复用键 {
        assert!(
            待复用.contains(k) && 一键.contains(k),
            "复用的来源键应是第一次运行的：{k}"
        );
    }
    for k in &待复用 {
        assert!(!二键.contains(k), "三条复用的问都没在第二次账本里重发");
    }
    // 预测 7
    assert_eq!(v["ids"], json!([1, 5, 6]));
    assert_eq!(v["pending"], json!([]));
    assert_eq!(v["reason"], "chain_done");
    assert_eq!(库.mark_count(), 16);
    let m3 = 库.marks_full(&hs[2]);
    let p2 = m3.iter().find(|m| m.premise == P2).unwrap();
    assert_eq!(p2.exit, "act", "M3 的 P2 标记被重问后重写为已决");
    assert_eq!(p2.first_purpose, "A", "first_purpose 保留最先写的目的");
    assert_eq!(p2.purpose, "B");
}

#[test]
fn 对照臂_目的b_空料库要多发三问() {
    let 库 = 新库();
    let r = 跑(&程序("B", &[P2, P3], "{}"), Some(&库), true, Ledger::new());
    assert_eq!(r.端口调用, 10, "第 1 层 6 + 第 2 层 4");
    assert_eq!(层(&r.value), vec![(6, 6, 0, 4, 2, 0), (4, 4, 0, 3, 1, 0)]);
}

#[test]
fn 未装料库_退化为每层照发() {
    let r = 跑(&程序("A", &[P1, P2], "{}"), None, false, Ledger::new());
    assert_eq!(r.端口调用, 10);
    assert_eq!(r.value["ids"], json!([1, 3, 6]));
    assert_eq!(待定(&r.value), vec![(3, "band".to_string())]);
    assert!(层(&r.value).iter().all(|l| l.2 == 0), "复用全为 0");
    assert_eq!(r.value["stored"], false);
}

#[test]
fn 重放_零调用() {
    let 库 = 新库();
    let 一 = 跑(&程序("A", &[P1, P2], "{}"), Some(&库), false, Ledger::new());
    let 二 = 跑(&程序("B", &[P2, P3], "{}"), Some(&库), true, Ledger::new());
    for (src, r) in [
        (程序("A", &[P1, P2], "{}"), 一),
        (程序("B", &[P2, P3], "{}"), 二),
    ] {
        let calib = CalibStore::new();
        let acts = ActionRegistry::new();
        let mut l = r.ledger;
        let o = Session::new(NoCallPorts::ports(), &calib, &acts)
            .with_companions(jpp::interp::CompanionMode::Off)
            .replay(&编译(&src), &EntryArgs::default(), &mut l)
            .unwrap_or_else(|e| panic!("{}", e.render()));
        assert_eq!(o.value_json(), r.value, "重放的值与运行时相同");
    }
}

#[test]
fn 范围与层数上限() {
    // max_layers = 1：只走 P1 一层，reason 为 bound
    let r = 跑(
        &程序("A", &[P1, P2], "{max_layers: 1}"),
        Some(&新库()),
        false,
        Ledger::new(),
    );
    assert_eq!(r.value["layers"], 1);
    assert_eq!(r.value["reason"], "bound");
    assert_eq!(r.端口调用, 6);
    // 存活为空：范围只给 M4、M5，P1 把它们淘光，P2 层不发，reason 为 empty
    let src = 程序("A", &[P1, P2], "{}").replace(
        "select(\"A\", pool,",
        "select(\"A\", filter(pool, fn(m) { content(m).id == 4 || content(m).id == 5 }),",
    );
    let r = 跑(&src, Some(&新库()), false, Ledger::new());
    assert_eq!(r.value["reason"], "empty");
    assert_eq!(r.value["layers"], 1);
    assert_eq!(r.端口调用, 2);
    assert_eq!(r.value["ids"], json!([]));
    // 对照：reuse、mark 都关
    let r = 跑(
        &程序("A", &[P1, P2], "{reuse: false, mark: false}"),
        Some(&新库()),
        false,
        Ledger::new(),
    );
    assert_eq!(r.端口调用, 10);
    assert_eq!(r.value["stored"], false, "mark: false 不写标记");
}

// ---- FileMatStore 本身 ----

#[test]
fn 文件料库_存取与内容寻址() {
    use jpp::value::{Mat, Taint};
    let mut 库 = FileMatStore::new(Box::new(MemBlob::new()));
    let mut m = Mat::new(
        json!({"id": 1}),
        "gen:x",
        vec!["gen:k".into()],
        Taint::Untrusted,
        Default::default(),
    );
    m.from_key.insert("judge#7".into());
    let a1 = 库.put(&m);
    let a2 = 库.put(&m);
    assert_eq!(a1, m.hash);
    assert_eq!(a1, a2, "同内容同地址");
    assert_eq!(
        库.get(&a1),
        Some(m),
        "取回与原材料相等，含来源出口键与 taint"
    );
    assert_eq!(库.get("不存在"), None);
    assert!(库.marks(&a1).is_empty());
}

#[test]
fn 文件料库_标记落盘后新库读得到并带临时存储声明() {
    let dir = root().join(format!("target/select-marks-dir-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let r = {
        let 库 = Rc::new(FileMatStore::new(Box::new(DirBlob::new(&dir))));
        跑(&程序("A", &[P1, P2], "{}"), Some(&库), false, Ledger::new())
    };
    let 新 = FileMatStore::open_dir(&dir);
    assert_eq!(新.mark_count(), 10, "新建的库读得到同样的标记");
    let h = r.value["pool_hashes"][0].as_str().unwrap();
    let ks = 新.marks(h);
    assert_eq!(ks.len(), 2, "M1 有 P1、P2 两条标记");
    assert!(ks.iter().all(|k| k.model_id == "mark" && k.state == h));
    let meta = std::fs::read_to_string(dir.join("mat-store.json")).unwrap();
    assert!(meta.contains("临时存储，骨架最终裁定后换"), "{meta}");
    let _ = std::fs::remove_dir_all(&dir);
}

// ---- 示例 examples/select-reuse.jpp：一次运行里先后两个目的（预注册补充（九）） ----

fn 跑示例(存: Option<&Rc<FileMatStore>>) -> 一趟 {
    let src = std::fs::read_to_string(root().join("examples/select-reuse.jpp")).unwrap();
    // 示例里的 import 是相对 examples/ 的；编译函数把源码放在 target/<子目录>/ 下，所以换成相对那里的路径
    let src = src.replace(
        "\"../lib/skeletons/select.jpp\"",
        "\"../../lib/skeletons/select.jpp\"",
    );
    跑(&src, 存, false, Ledger::new())
}

#[test]
fn 示例_一次运行两个目的_装了料库() {
    let 库 = 新库();
    let r = 跑示例(Some(&库));
    let v = &r.value;
    // 预注册补充（九）第 1 条预测 17，没中：同一次运行里运行时自己的判断复用（同状态同题）又省了 M3 的 P2 重问
    // （`asked` 记的是发给运行时的问数 3，运行时对 M3 命中本次运行内已有的读数），实际 16
    assert_eq!(r.端口调用, 16);
    assert_eq!(判断条目数(&r.ledger), 16);
    assert_eq!(
        层(&v["a"]),
        vec![(6, 6, 0, 4, 2, 0), (4, 4, 0, 2, 1, 1)],
        "{v}"
    );
    assert_eq!(
        层(&v["b"]),
        vec![(6, 3, 3, 3, 2, 1), (4, 4, 0, 3, 1, 0)],
        "{v}"
    );
    assert_eq!(v["a"]["ids"], json!([1, 3, 6]));
    assert_eq!(v["b"]["ids"], json!([1, 5, 6]));
    assert_eq!(待定(&v["a"]), vec![(3, "band".to_string())]);
    assert_eq!(待定(&v["b"]), vec![(3, "band".to_string())]);
    assert_eq!(库.mark_count(), 16);
}

#[test]
fn 示例_未装料库_结果相同_复用数为零() {
    let 有 = 跑示例(Some(&新库()));
    let 无 = 跑示例(None);
    // 预注册补充（九）第 5 条预测 20，没中：一次运行里运行时自己的判断复用（同状态同题，不需要料库）已经让
    // 目的 B 少发 M1 M2 M3 M6 的 P2，实际 16。标记的价值在跨运行、跨程序、没有账本时（见「第二次运行」那条测试）
    assert_eq!(无.端口调用, 16);
    assert_eq!(无.value["b"]["per_layer"][0]["reused"], 0);
    assert_eq!(有.value["b"]["per_layer"][0]["reused"], 3);
    assert_eq!(无.value["a"]["stored"], false);
    assert_eq!(无.value["a"]["ids"], 有.value["a"]["ids"]);
    assert_eq!(无.value["b"]["ids"], 有.value["b"]["ids"]);
}
