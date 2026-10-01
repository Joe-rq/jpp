//! 料库标记的宿主侧（步 29，B20 第 1 种；主控:B0460）：`.jpp` 经两个宿主变换读、写材料上的标记。
//!
//! **标记**是「某份材料在某道字面前提题上的答案」：键 = （材料哈希 `Mat.hash`，前提题哈希），与目的无关，
//! 所以别的目的的程序问到同一道字面前提题时读得到。内容有出口（`act`、`ignore`、`unsure:<原因>`）、
//! 来源出口键（产生这个答案的判断的账本键）、前提题字面、第一次写它的目的。
//!
//! **临时口子**：料库的最终形态等骨架裁定；这里只定义 [`MarkStore`] 这一个窄接口，具体存储由宿主给
//! （`jpp::store::FileMatStore` 是最简单的文件实现）。B65 限制的是按哈希取回材料内容，读写标记是 B20 第 1 种
//! 「每层答案留在材料上、别的目的读得到」必须有的出口（主控裁定 2026-09-29）。
//!
//! 两个变换（`s_library_with` 在装了料库时注册；每层批量调一次）：
//! - `transform("mat_marks", mat({premise, mats: [哈希…]}))` → `[{hash, found, exit?, p?, source_key?, purpose?}]`，
//!   与 `mats` 等长同序；`found` 为假表示该材料在这道前提题上没有标记。**只读**，不区分已决与未决，复用与否由
//!   `select.jpp` 定（未决不复用，意图汇编 7c）。
//! - `transform("mat_mark", mat({purpose, premise, marks: [{mat, exit, p?, source_key}…]}))` → `{written: n}`。
//!   同键重写幂等（后写覆盖先写，`first_purpose` 保留最先写的目的）。
//!
//! 宿主没装料库时两个变换仍在（读一律 `found: false`，写不写、返回 `stored: false`），所以 `select.jpp` 不用探测
//! 有没有料库，没料库时自然退化为每层照发；报告的 `detail.stored` 说明这次有没有存。
//!
//! 变换版本固定：审计重放按账本键（含版本）取账本记录，版本随料库内容变会让旧账本重放失败。
//! 缓存边界（Z0206 已处理，Z0222 改按位）：两个变换注册时带 `reads_external_state`，`jpp::store::CacheIndex` 建索引时
//! 按这一位跳过它们的条目，标记读写不跨运行缓存；本运行内的复用与账本重放不经索引，照旧（`crates/jpp/tests/mat_marks_cache_boundary.rs`）。

use std::rc::Rc;

use jpp_effects::{HostTaint, HostTransform};
use jpp_ir::key::hash_of;
use serde_json::{Value as Json, json};

/// 前提题的哈希：只看字面，不看校准标签与线（同一道字面题不论被哪个目的、哪条线问，标记都通用）。
pub fn premise_hash(premise: &str) -> String {
    hash_of(&["premise", premise])
}

/// 一条标记。
#[derive(Clone, Debug, PartialEq)]
pub struct Mark {
    /// 材料哈希（`Mat.hash`，也是 `MatStorePort` 的地址）
    pub mat: String,
    /// 前提题字面
    pub premise: String,
    /// 前提题哈希（[`premise_hash`]）
    pub q: String,
    /// `act` | `ignore` | `unsure:<原因>`
    pub exit: String,
    /// 读数（记账用，不参与复用判断）
    pub p: Option<f64>,
    /// 产生这个答案的判断的账本键（来源出口键）
    pub source_key: String,
    /// 第一次写这条标记的目的
    pub first_purpose: String,
    /// 最近一次写它的目的
    pub purpose: String,
    /// 命中数（为 B0463 按最近被命中的运行数淘汰预留；本批只写 0）
    pub hits: u64,
}

impl Mark {
    /// 已决的答案（`act`、`ignore`）才可复用；未决重问（意图汇编 7c）。
    pub fn decided(&self) -> bool {
        self.exit == "act" || self.exit == "ignore"
    }
    pub fn to_json(&self) -> Json {
        json!({
            "v": 1, "mat": self.mat, "premise": self.premise, "q": self.q, "exit": self.exit,
            "p": self.p, "source_key": self.source_key,
            "first_purpose": self.first_purpose, "purpose": self.purpose, "hits": self.hits,
        })
    }
    pub fn from_json(j: &Json) -> Result<Mark, String> {
        let s = |k: &str| -> Result<String, String> {
            j.get(k)
                .and_then(Json::as_str)
                .map(String::from)
                .ok_or_else(|| format!("标记缺字段 {k}"))
        };
        Ok(Mark {
            mat: s("mat")?,
            premise: s("premise")?,
            q: s("q")?,
            exit: s("exit")?,
            p: j.get("p").and_then(Json::as_f64),
            source_key: s("source_key")?,
            first_purpose: s("first_purpose")?,
            purpose: s("purpose")?,
            hits: j.get("hits").and_then(Json::as_u64).unwrap_or(0),
        })
    }
}

/// 标记存储：宿主给的窄接口（`&self`，实现自己管内部可变）。
pub trait MarkStore {
    /// 材料 `mat` 在前提题 `q`（[`premise_hash`]）上的标记
    fn read(&self, mat: &str, q: &str) -> Option<Mark>;
    /// 写一条标记（同键覆盖，`first_purpose` 沿用旧的）
    fn write(&self, mark: Mark) -> Result<(), String>;
}

fn text_field<'a>(j: &'a Json, k: &str) -> Result<&'a str, String> {
    j.get(k)
        .and_then(Json::as_str)
        .ok_or_else(|| format!("缺文字字段 {k}"))
}

/// 宿主变换 `mat_marks`（读）。
pub fn mat_marks_transform(store: Option<Rc<dyn MarkStore>>) -> HostTransform {
    HostTransform {
        name: "mat_marks".into(),
        version: "1".into(),
        taint_out: HostTaint::Trusted,
        reads_external_state: true,
        doc: "transform(\"mat_marks\", mat({premise, mats: [材料哈希…]}))：返回与 mats 等长的 [{hash, found, exit?, p?, source_key?, purpose?}]".into(),
        run: Box::new(move |ins: &[Json]| {
            let [r] = ins else {
                return Err(format!("要一份记录，收到 {} 份", ins.len()));
            };
            let premise = text_field(r, "premise")?;
            let q = premise_hash(premise);
            let mats = r
                .get("mats")
                .and_then(Json::as_array)
                .ok_or("缺 mats（材料哈希列表）")?;
            let mut out = vec![];
            for m in mats {
                let h = m.as_str().ok_or("mats 里要是文字（材料哈希）")?;
                out.push(match store.as_ref().and_then(|s| s.read(h, &q)) {
                    Some(k) => json!({"hash": h, "found": true, "exit": k.exit, "p": k.p,
                                       "source_key": k.source_key, "purpose": k.first_purpose}),
                    None => json!({"hash": h, "found": false}),
                });
            }
            Ok(Json::Array(out))
        }),
    }
}

/// 宿主变换 `mat_mark`（写）。
pub fn mat_mark_transform(store: Option<Rc<dyn MarkStore>>) -> HostTransform {
    HostTransform {
        name: "mat_mark".into(),
        version: "1".into(),
        taint_out: HostTaint::Trusted,
        reads_external_state: true,
        doc: "transform(\"mat_mark\", mat({purpose, premise, marks: [{mat, exit, p?, source_key}…]}))：写标记，返回 {written: n, stored: 装了料库与否}".into(),
        run: Box::new(move |ins: &[Json]| {
            let [r] = ins else {
                return Err(format!("要一份记录，收到 {} 份", ins.len()));
            };
            let purpose = text_field(r, "purpose")?;
            let premise = text_field(r, "premise")?;
            let marks = r
                .get("marks")
                .and_then(Json::as_array)
                .ok_or("缺 marks（标记列表）")?;
            let Some(store) = store.as_ref() else {
                // 宿主没装料库：不写，照实说明（`select.jpp` 据此退化为每层照发）
                return Ok(json!({"written": 0, "stored": false}));
            };
            let mut n = 0;
            for m in marks {
                store.write(Mark {
                    mat: text_field(m, "mat")?.into(),
                    premise: premise.into(),
                    q: premise_hash(premise),
                    exit: text_field(m, "exit")?.into(),
                    p: m.get("p").and_then(Json::as_f64),
                    source_key: text_field(m, "source_key")?.into(),
                    first_purpose: purpose.into(),
                    purpose: purpose.into(),
                    hits: 0,
                })?;
                n += 1;
            }
            Ok(json!({"written": n, "stored": true}))
        }),
    }
}
