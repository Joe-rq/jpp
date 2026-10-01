//! 程序用了哪些校准键（K-088 静态列键那一半；L7 2026-09-28）。只读语法与字面量，不执行、不解析名字，
//! 口径同 [`crate::questions`]。
//!
//! 运行期 `cut` 查线的链是 题键 → 题式键 → 类键（B44、B34）：
//! - `test(题面, 标签[, 记录])`、`select(题面, 标签[, 记录])`、`measure(题面, [档位…], 标签)` 的题键与类键
//!   都是作者写的标签；这类题没有题式哈希（`Question.form_hash` 为空），不走题式级；
//! - `form(题型, 模板, {calib: 标签, …})` 填出的题多一级题式键：题式哈希由模板与声明字段算，宿主一侧
//!   用 `Form::new` 算（本 crate 不依赖值模型，`20` §2.2）；
//! - `cut(…, {cost: [fp, fn]})` 只选按这个代价矩阵认证的证书（B29），作者要知道该跑哪几次
//!   `calib-import --cost fp,fn`，所以代价矩阵一并列出。
//!
//! 标签、模板不是字面量的，列进跳过项并给原因（不猜）。

use crate::analysis::view::ExprKind;
use crate::*;
use jpp_ir::ir::{Host, Node};

/// 模板题式的声明字段（算题式哈希用）。
#[derive(Clone, Debug, PartialEq)]
pub struct LiteralForm {
    pub template: String,
    pub scale: Vec<String>,
    pub evidence: Vec<String>,
    pub presupposition: Option<String>,
    pub request: Option<String>,
    /// `labels` 进题式哈希（B155）；写了就不在这里算哈希，宿主报「未算」
    pub has_labels: bool,
}

/// 一处用到校准键的题（题或题式）。
#[derive(Clone, Debug, PartialEq)]
pub struct KeyUse {
    /// 作者写的校准标签：题键与类键
    pub label: String,
    /// `test` / `select` / `measure`
    pub op: &'static str,
    /// 字面题面（模板题式为模板）
    pub text: Option<String>,
    /// 由 `form(…)` 构造时的声明（有题式级键）
    pub form: Option<LiteralForm>,
    pub span: Span,
}

/// `cut` 上写的字面代价矩阵。
#[derive(Clone, Debug, PartialEq)]
pub struct CostUse {
    pub fp: f64,
    pub fn_: f64,
    pub span: Span,
}

/// 跳过的一处与原因。
#[derive(Clone, Debug, PartialEq)]
pub struct KeySkip {
    pub call: &'static str,
    pub reason: String,
    pub span: Span,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct CalibKeys {
    pub uses: Vec<KeyUse>,
    pub costs: Vec<CostUse>,
    pub skipped: Vec<KeySkip>,
}

fn 字面文本(e: &Expr) -> Option<String> {
    match e.kind() {
        ExprKind::Text(t) => Some(t.clone()),
        _ => None,
    }
}

fn 字面文本列表(e: &Expr) -> Option<Vec<String>> {
    match e.kind() {
        ExprKind::List(items) => items.iter().map(字面文本).collect(),
        _ => None,
    }
}

fn 字面数(e: &Expr) -> Option<f64> {
    match &e.node {
        Node::Host(Host::Integer(v)) => Some(*v as f64),
        Node::Host(Host::Decimal(v)) => Some(*v),
        _ => None,
    }
}

fn 记录字段<'a>(e: &'a Expr, k: &str) -> Option<&'a Expr> {
    match e.kind() {
        ExprKind::Record(fields) => fields.iter().find(|(n, _)| n == k).map(|(_, v)| v),
        _ => None,
    }
}

/// `form` 的第三个实参：取 `calib` 与算题式哈希要的字段。`Err` 是跳过原因。
fn 题式(op: &'static str, args: &[&Expr]) -> Result<KeyUse, String> {
    let template = args
        .get(1)
        .and_then(|a| 字面文本(a))
        .ok_or("模板题面不是字面文本")?;
    let rec = args.get(2).ok_or("缺第三个实参 {calib: …}")?;
    if !matches!(rec.kind(), ExprKind::Record(_)) {
        return Err("第三个实参不是字面记录".into());
    }
    let label = 记录字段(rec, "calib")
        .and_then(字面文本)
        .ok_or("calib 不是字面文本")?;
    let 列表 = |k: &str| -> Result<Vec<String>, String> {
        match 记录字段(rec, k) {
            None => Ok(vec![]),
            Some(v) => 字面文本列表(v).ok_or_else(|| format!("{k} 不是字面文本列表")),
        }
    };
    let 文本 = |k: &str| -> Result<Option<String>, String> {
        match 记录字段(rec, k) {
            None => Ok(None),
            Some(v) => 字面文本(v)
                .map(Some)
                .ok_or_else(|| format!("{k} 不是字面文本")),
        }
    };
    Ok(KeyUse {
        label,
        op,
        text: Some(template.clone()),
        form: Some(LiteralForm {
            template,
            scale: 列表("scale")?,
            evidence: 列表("evidence")?,
            presupposition: 文本("presupposition")?,
            request: 文本("request")?,
            has_labels: 记录字段(rec, "labels").is_some(),
        }),
        span: Span::default(),
    })
}

/// 全程序用到的校准键、代价矩阵与跳过项，都按源码顺序。
pub fn calib_keys(p: &Program) -> CalibKeys {
    let mut out = CalibKeys::default();
    walk_block(&p.body, &mut |e| {
        let Some(name) = call_name(e) else { return };
        let args = call_args(e);
        match name {
            "test" | "select" | "measure" => {
                let op: &'static str = match name {
                    "test" => "test",
                    "select" => "select",
                    _ => "measure",
                };
                let 标签位 = if op == "measure" { 2 } else { 1 };
                match args.get(标签位).map(|a| 字面文本(a)) {
                    Some(Some(label)) => out.uses.push(KeyUse {
                        label,
                        op,
                        text: args.first().and_then(|a| 字面文本(a)),
                        form: None,
                        span: e.span,
                    }),
                    Some(None) => out.skipped.push(KeySkip {
                        call: op,
                        reason: "校准标签不是字面文本（运行期才知道）".into(),
                        span: e.span,
                    }),
                    None => out.skipped.push(KeySkip {
                        call: op,
                        reason: "没写校准标签".into(),
                        span: e.span,
                    }),
                }
            }
            "form" => {
                let op: Option<&'static str> =
                    match args.first().and_then(|a| 字面文本(a)).as_deref() {
                        Some("test") => Some("test"),
                        Some("select") => Some("select"),
                        Some("measure") => Some("measure"),
                        _ => None,
                    };
                match op
                    .ok_or_else(|| "题型不是字面的 test / select / measure".to_string())
                    .and_then(|op| 题式(op, &args))
                {
                    Ok(mut u) => {
                        u.span = e.span;
                        out.uses.push(u);
                    }
                    Err(reason) => out.skipped.push(KeySkip {
                        call: "form",
                        reason,
                        span: e.span,
                    }),
                }
            }
            "cut" => {
                // 代价记录在第二或第三个实参位（同 `rules/j03.rs::cost_shape`）；形状不对由 J-03 报，这里只收两数
                let Some(v) = args.iter().skip(1).find_map(|a| 记录字段(a, "cost")) else {
                    return;
                };
                if let ExprKind::List(items) = v.kind()
                    && let [a, b] = items
                    && let (Some(fp), Some(fn_)) = (字面数(a), 字面数(b))
                {
                    out.costs.push(CostUse {
                        fp,
                        fn_,
                        span: e.span,
                    });
                }
            }
            _ => {}
        }
    });
    out.uses.sort_by_key(|u| u.span.start);
    out.costs.sort_by_key(|c| c.span.start);
    out.skipped.sort_by_key(|s| s.span.start);
    out
}
