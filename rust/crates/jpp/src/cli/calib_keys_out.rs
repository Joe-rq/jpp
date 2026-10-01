//! `jpp check` 列出程序用到的校准键（K-088 静态列键那一半；L7 2026-09-28）。
//!
//! 收集在检查器（`jpp_check::calib_keys`，只读语法与字面量）；模板题式的题式哈希在这里用
//! `jpp::value::Form::new` 算（与运行时 `form(…)` 同一个构造函数）；每处题按运行期 `cut` 的查找链
//! （题级 → 题式级 → 类级，B44、B34，`jpp-runtime/src/bridge.rs`）在这次给的校准记录里查，报用得上哪一层。
//! 不给 `--calib` 时按空记录本查，每处都是「没有线」——按判断器的回答走（B187），不是错误。
//!
//! 文本模式打到 stderr（与诊断同处，stdout 的 `Checked …` 一行不变）；`--json` 进文档的 `calib_keys` 字段。

use jpp::effects::CalibStore;
use jpp::value::{Form, Op};
use jpp_check::calib_keys::{CalibKeys, KeyUse};
use jpp_effects::views::CalibView;
use jpp_syntax::loader::LoadedProgram;
use serde_json::{Value, json};

fn op_of(op: &str) -> Op {
    match op {
        "select" => Op::Select,
        "measure" => Op::Measure,
        _ => Op::Test,
    }
}

fn 位置(loaded: &LoadedProgram, span: jpp::ir::Span) -> (String, Value) {
    match loaded.locate(jpp_syntax::ast::Span {
        start: span.start,
        end: span.end,
    }) {
        Some(l) => (
            format!("{}:{}", l.line, l.col),
            json!({"file": l.file, "line": l.line, "col": l.col, "start": l.start}),
        ),
        None => (format!("@{}", span.start), json!({"start": span.start})),
    }
}

/// 模板题式的题式哈希；普通题没有（运行期也不走题式级）。写了 `labels` 的不算（B155 进哈希，这里不重算）。
fn 题式哈希(u: &KeyUse) -> Result<Option<String>, String> {
    let Some(f) = &u.form else { return Ok(None) };
    if f.has_labels {
        return Err("写了 labels（进题式哈希，B155），这里不算".into());
    }
    Form::new(
        op_of(u.op),
        &f.template,
        "",
        f.scale.clone(),
        f.evidence.clone(),
        f.presupposition.clone(),
        f.request.clone(),
    )
    .map(|x| Some(x.hash))
    .map_err(|e| format!("算不出题式哈希：{e}"))
}

/// 按 `bridge.rs` 的查找链判这处题用得上哪一层线：各层状态与选中的层（`None` = 没有线）。
fn 查链(
    calib: &CalibStore,
    label: &str,
    form_hash: Option<&str>,
) -> (Value, Option<&'static str>) {
    let 链 = calib.chain(label, form_hash);
    let 可用 = |s: &str| matches!(s, "上岗" | "停岗候选");
    let q = 链.question.rec.status.as_str();
    let f = 链.form.as_ref().map(|l| l.rec.status.as_str());
    let c = 链.class.as_ref().map(|l| l.rec.status.as_str());
    let 选中 = if label.starts_with("fit:") {
        可用(q).then_some("题级")
    } else if 可用(q) {
        Some("题级")
    } else if q == "停岗" {
        None
    } else if f.is_some_and(可用) {
        Some("题式级")
    } else if f != Some("停岗") && c.is_some_and(可用) {
        Some("类级")
    } else {
        None
    };
    (json!({"question": q, "form": f, "class": c}), 选中)
}

/// 算好的清单：JSON 文档一份，文本若干行。
pub fn build(keys: &CalibKeys, calib: &CalibStore, loaded: &LoadedProgram) -> (Value, Vec<String>) {
    let mut uses = vec![];
    let mut 行 = vec![];
    let mut 标签: Vec<&str> = vec![];
    for u in &keys.uses {
        if !标签.contains(&u.label.as_str()) {
            标签.push(&u.label);
        }
        let (at, site) = 位置(loaded, u.span);
        let (hash, 未算) = match 题式哈希(u) {
            Ok(h) => (h, None),
            Err(why) => (None, Some(why)),
        };
        let (levels, 选中) = 查链(calib, &u.label, hash.as_deref());
        let 题面 = u.text.as_deref().unwrap_or("（题面非字面）");
        let 题式 = match (&hash, &未算) {
            (Some(h), _) => format!(" 题式 {}", &h[..h.len().min(12)]),
            (None, Some(why)) => format!(" 题式哈希未算：{why}"),
            _ => String::new(),
        };
        行.push(format!(
            "  {} @{at}：{}「{题面}」{题式}；线：{}",
            u.label,
            u.op,
            match 选中 {
                Some(l) => format!("{l}记录可用"),
                None => "没有可用记录，按判断器的回答走（B187）".into(),
            }
        ));
        uses.push(json!({
            "label": u.label, "op": u.op, "text": u.text, "site": site,
            "form_hash": hash, "form_hash_skipped": 未算,
            "records": levels, "line": 选中,
        }));
    }
    let mut costs = vec![];
    for c in &keys.costs {
        let (at, site) = 位置(loaded, c.span);
        行.push(format!(
            "  代价 [{}, {}] @{at}：只用按这个代价认证的证书（calib-import --cost {},{}），没有则按判断器的回答走",
            c.fp, c.fn_, c.fp, c.fn_
        ));
        costs.push(json!({"fp": c.fp, "fn": c.fn_, "site": site}));
    }
    let mut skipped = vec![];
    for s in &keys.skipped {
        let (at, site) = 位置(loaded, s.span);
        行.push(format!("  跳过 {} @{at}：{}", s.call, s.reason));
        skipped.push(json!({"call": s.call, "reason": s.reason, "site": site}));
    }
    if !行.is_empty() {
        行.insert(
            0,
            format!(
                "校准键（K-088）：{} 个标签、{} 处题，代价矩阵 {} 处，跳过 {} 处",
                标签.len(),
                keys.uses.len(),
                keys.costs.len(),
                keys.skipped.len()
            ),
        );
    }
    (
        json!({"labels": 标签, "uses": uses, "costs": costs, "skipped": skipped}),
        行,
    )
}
