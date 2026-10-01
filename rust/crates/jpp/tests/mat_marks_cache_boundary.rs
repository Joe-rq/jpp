//! Z0206：选材料的标记读写不跨运行缓存（`store/cache_index.rs` 按宿主变换表的 `reads_external_state` 位跳过，Z0222）。
//! 预注册见 `地基/过程记录/工程-小缺陷-0929.md` 第四节 4.1。背景：`地基/过程记录/工程-步29-选材料.md`、
//! `地基/规划/主控核查/复核-B0460-选材料前半.md`。
//!
//! 边界：`mat_marks` 读的是料库当前的标记，`mat_mark` 写料库。两者的键是 `host:mat_marks@1` / `host:mat_mark@1` 加
//! 输入哈希，不含料库内容。宿主同时挂 `CacheIndex` 时，别的账本里同输入的记录会被当成复用：读到旧标记，写被跳过。
//! 修法只动跨运行索引；本运行账本内的记录（同运行复用、账本重放）不经索引，照旧。

use jpp::effects::{CalibStore, NoCallPorts};
use jpp::interp::ActionRegistry;
use jpp::ledger::{Entry, Ledger};
use jpp::store::{CacheIndex, FileMatStore, MemBlob};
use jpp::{EntryArgs, Session, lower, syntax::parse};
use serde_json::{Value as Json, json};
use std::rc::Rc;

const 读: &str = r#"budget {calls: 1, cost: 0};
let r = transform("mat_marks", mat({premise: "这段材料是否提到了具体金额？", mats: ["H1"]}));
content(r)
"#;

/// 同一运行里对同一输入读两次（调用位置不同）
const 读两次: &str = r#"budget {calls: 1, cost: 0};
let a = transform("mat_marks", mat({premise: "这段材料是否提到了具体金额？", mats: ["H1"]}));
let b = transform("mat_marks", mat({premise: "这段材料是否提到了具体金额？", mats: ["H1"]}));
{a: content(a), b: content(b)}
"#;

fn 写(出口: &str) -> String {
    format!(
        r#"budget {{calls: 1, cost: 0}};
let w = transform("mat_mark", mat({{purpose: "T", premise: "这段材料是否提到了具体金额？",
                                  marks: [{{mat: "H1", exit: "{出口}", source_key: "k-{出口}"}}]}}));
content(w)
"#
    )
}

fn 新库() -> Rc<FileMatStore> {
    Rc::new(FileMatStore::new(Box::new(MemBlob::new())))
}

struct 一趟 {
    value: Json,
    ledger: Ledger,
    cross_run: Option<u64>,
}

fn 跑(src: &str, 库: &Rc<FileMatStore>, 缓存: Option<&CacheIndex>) -> 一趟 {
    let calib = CalibStore::new();
    let acts = ActionRegistry::new();
    let mut s = Session::new(NoCallPorts::ports(), &calib, &acts).with_mat_store(库.clone());
    if let Some(c) = 缓存 {
        s = s.with_cache(c);
    }
    let mut l = Ledger::new();
    let o = s
        .run(
            &lower(&parse(src).expect("解析")).expect("lower"),
            &EntryArgs::default(),
            &mut l,
        )
        .unwrap_or_else(|e| panic!("{}", e.render()));
    一趟 {
        value: o.value_json(),
        ledger: l,
        cross_run: o.cache.map(|c| c.cross_run),
    }
}

fn 重放(src: &str, l: &mut Ledger) -> Json {
    let calib = CalibStore::new();
    let acts = ActionRegistry::new();
    Session::new(NoCallPorts::ports(), &calib, &acts)
        .replay(
            &lower(&parse(src).expect("解析")).expect("lower"),
            &EntryArgs::default(),
            l,
        )
        .unwrap_or_else(|e| panic!("{}", e.render()))
        .value_json()
}

fn 复用来源(l: &Ledger) -> Vec<String> {
    l.entries
        .iter()
        .filter_map(|e| match e {
            Entry::Effect {
                kind, reused_from, ..
            } if kind == "transform" => reused_from.clone(),
            _ => None,
        })
        .collect()
}

fn 读到的出口(v: &Json) -> String {
    assert_eq!(v[0]["found"], true, "{v}");
    v[0]["exit"].as_str().unwrap().to_string()
}

#[test]
fn 读标记不跨运行复用_两个账本先后写不同标记() {
    let 库 = 新库();
    // 账本一：先写 act，再读
    let w1 = 跑(&写("act"), &库, None);
    let r1 = 跑(读, &库, None);
    assert_eq!(读到的出口(&r1.value), "act");
    // 账本二：改写为 ignore；前面所有账本都进缓存索引，第三趟读同一输入
    let w2 = 跑(&写("ignore"), &库, None);
    let ix = CacheIndex::build(&[
        ("w1.jsonl".to_string(), w1.ledger),
        ("r1.jsonl".to_string(), r1.ledger),
        ("w2.jsonl".to_string(), w2.ledger),
    ]);
    let r2 = 跑(读, &库, Some(&ix));
    assert_eq!(
        读到的出口(&r2.value),
        "ignore",
        "第二次运行读到的应是新标记，不是缓存里旧账本的读结果"
    );
    assert!(
        复用来源(&r2.ledger).is_empty(),
        "标记读取不进跨运行复用：{:?}",
        复用来源(&r2.ledger)
    );
    assert_eq!(r2.cross_run, Some(0));
}

#[test]
fn 写标记不跨运行复用_新料库仍被写入() {
    let w1 = 跑(&写("act"), &新库(), None);
    let ix = CacheIndex::build(&[("w1.jsonl".to_string(), w1.ledger)]);
    let 库2 = 新库();
    assert_eq!(库2.mark_count(), 0);
    let w2 = 跑(&写("act"), &库2, Some(&ix));
    assert_eq!(
        库2.mark_count(),
        1,
        "写标记若被别的账本的记录顶替，新料库就一条也没写"
    );
    assert!(
        复用来源(&w2.ledger).is_empty(),
        "{:?}",
        复用来源(&w2.ledger)
    );
    assert_eq!(w2.value["written"], 1);
}

#[test]
fn 同一运行内的标记读取仍复用() {
    let 库 = 新库();
    let _ = 跑(&写("act"), &库, None);
    let r = 跑(读两次, &库, None);
    assert_eq!(r.value["a"], r.value["b"]);
    let 来源 = 复用来源(&r.ledger);
    assert_eq!(来源.len(), 1, "第二次读是同运行复用：{来源:?}");
    assert!(!来源[0].starts_with("ext:"), "{来源:?}");
}

#[test]
fn 只凭账本重放逐字节一致() {
    let 库 = 新库();
    let w1 = 跑(&写("act"), &库, None);
    let mut r1 = 跑(读, &库, None);
    let w2 = 跑(&写("ignore"), &库, None);
    let ix = CacheIndex::build(&[
        ("w1.jsonl".to_string(), w1.ledger),
        ("r1.jsonl".to_string(), r1.ledger.clone()),
        ("w2.jsonl".to_string(), w2.ledger),
    ]);
    let mut r2 = 跑(读, &库, Some(&ix));
    // 两本账本各自重放：值与首跑逐字节相同，且各自还原当时读到的标记（互不串）
    let 重1 = 重放(读, &mut r1.ledger);
    let 重2 = 重放(读, &mut r2.ledger);
    assert_eq!(
        serde_json::to_string(&重1).unwrap(),
        serde_json::to_string(&r1.value).unwrap()
    );
    assert_eq!(
        serde_json::to_string(&重2).unwrap(),
        serde_json::to_string(&r2.value).unwrap()
    );
    assert_eq!(读到的出口(&重1), "act");
    assert_eq!(读到的出口(&重2), "ignore");
    assert_eq!(json!(重1) == json!(重2), false);
}
