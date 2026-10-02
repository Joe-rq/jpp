//! N-T6（主控板 Z0867，推进线 A E5）：`purpose_run` 的字段未决先走默认链（Z0514 的 `unsure_default`）——
//! 并列没配取法照旧转交并带缺的类别；配了取法就取、再判、落字段；缺参照槽取来补进 ref 再判；开 `--guard` 照旧。
//! 预注册：`地基/过程记录/工程-Z0514-链内未决接默认链.md` 附录一。闭包端口，不发请求；任务是虚构的「挑供应商」。

mod derive_support;
use derive_support::*;
use jpp::ledger::Entry;
use jpp::value::{Answer, Question, State};
use serde_json::{Value as Json, json};

const 目的: &str = "说出每家供应商最主要的短板。";
const 题: &str = "这家供应商最主要的短板在哪一方面？";

/// `前` 放在 purpose_run 之前（unsure_source 等）
fn 程序(前: &str) -> String {
    format!(
        "import \"../../lib/derive/purpose.jpp\";\nbudget {{calls: 2000, cost: 0, depth: 8192}};\n{前}\nlet r = purpose_run(\"{目的}\", [{{on: mat({{name: \"供应商0\", note: \"产能一般\"}})}}], {{}});\n{}",
        "{value: map(r.value, fn(v) { v.fields }), pending: map(r.pending, fn(p) { {cause: p.cause, via: p.via, pos: p.pos} }), dropped: r.detail.dropped}"
    )
}

fn 生成(证据: bool) -> impl Fn(&str) -> Vec<Json> {
    move |p: &str| {
        if p.contains("字面前提题") {
            vec![]
        } else if p.starts_with("下面是一段目的") {
            vec![
                json!({"material": "供应商", "predicates": [{"text": "这家供应商最主要的短板", "cut": "k_ary", "cut_from": "purpose",
                        "over": ["产能", "资金", "质量"], "over_from": "system", "request": "one", "field": "weak"}],
                        "request": null, "presupposition": null, "context": null, "reference": null, "budget": null, "unsure": null, "done": ["weak"]}),
            ]
        } else if 证据 {
            vec![
                json!({"op": "select", "text": 题, "over": ["产能", "资金", "质量"], "evidence": ["ref"]}),
            ]
        } else {
            vec![json!({"op": "select", "text": 题, "over": ["产能", "资金", "质量"]})]
        }
    }
}

/// 主判题：状态里补进了材料（ctx 或 ref）就判得出（选第二项），没补是并列；「为什么拿不准」选第一类；诊断放行
fn 读数(t: &str, _q: &Question, s: &State) -> Answer {
    if t.contains("需要分别回答的判断") {
        return Answer::Noul(0.1);
    }
    if t.contains("这段材料里有没有") {
        return Answer::Noul(0.9);
    }
    if t.contains("已认证的题问的是同一件事") {
        let k = s.over.len();
        let mut v = vec![0.0; k];
        v[k - 1] = 1.0;
        return Answer::Choice(v);
    }
    if t.contains("为什么拿不准") {
        let mut v = vec![0.02; s.over.len()];
        v[0] = 0.9;
        return Answer::Choice(v);
    }
    let k = s.over.len().max(3);
    let mut v = vec![0.0; k];
    if s.ctx.is_empty() && s.r#ref.is_empty() {
        v[0] = 0.5;
        v[1] = 0.5;
    } else {
        v[1] = 0.9;
        v[0] = 0.1;
    }
    Answer::Choice(v)
}

fn 计(r: &跑出, f: impl Fn(&Entry) -> bool) -> usize {
    r.ledger.entries.iter().filter(|e| f(e)).count()
}

#[test]
fn 并列没配取法_照旧转交并带缺的类别() {
    let r = 跑_按提示_关(&程序(""), 读数, 生成(false)).unwrap_or_else(|e| panic!("{e}"));
    let v = r.out.value_json();
    assert_eq!(v["value"], json!([{"weak": null}]), "{v}");
    assert_eq!(
        v["pending"],
        json!([{"cause": "tie", "via": ["缺材料、无取法", "purpose:weak"], "pos": 0}]),
        "去向写明缺哪类、无取法（Jpp 2026-10-02）：{v}"
    );
    assert_eq!(v["dropped"], json!([]), "{v}");
    let ud: Vec<&Json> = r
        .out
        .unsure_default
        .iter()
        .filter(|x| x["cause"] == json!("tie"))
        .collect();
    assert_eq!(ud.len(), 1, "{:?}", r.out.unsure_default);
    assert_eq!(ud[0]["end"], json!("handoff"));
    assert_eq!(
        ud[0]["needed"],
        json!(["材料"]),
        "取题 lacks 的第一类：{:?}",
        ud[0]
    );
}

#[test]
fn 并列配了取法_取来再判落字段() {
    let 前 = "unsure_source({need: [\"材料\"], fetch: fn(q, need, m) { mat(\"去年三次交付延误都因资金周转\") }});";
    let r = 跑_按提示_关(&程序(前), 读数, 生成(false)).unwrap_or_else(|e| panic!("{e}"));
    let v = r.out.value_json();
    assert_eq!(v["value"], json!([{"weak": "资金"}]), "{v}");
    assert_eq!(v["pending"], json!([]), "{v}");
    let ud: Vec<&Json> = r
        .out
        .unsure_default
        .iter()
        .filter(|x| x["cause"] == json!("tie"))
        .collect();
    assert_eq!(ud.len(), 1, "{:?}", r.out.unsure_default);
    assert_eq!(ud[0]["end"], json!("decided"));
    assert_eq!(计(&r, |e| matches!(e, Entry::Enrich { .. })), 1);
    assert_eq!(
        计(&r, |e| matches!(
            e,
            Entry::Handoff { .. } | Entry::Drop { .. }
        )),
        0
    );
}

#[test]
fn 缺参照槽配了取法_补进ref再判() {
    let 前 =
        "unsure_source({need: [\"参照\"], fetch: fn(q, need, m) { mat(\"同行的交付记录\") }});";
    let r = 跑_按提示_关(&程序(前), 读数, 生成(true)).unwrap_or_else(|e| panic!("{e}"));
    let v = r.out.value_json();
    assert_eq!(v["value"], json!([{"weak": "资金"}]), "{v}");
    assert_eq!(v["pending"], json!([]), "{v}");
    let ud: Vec<&Json> = r
        .out
        .unsure_default
        .iter()
        .filter(|x| x["cause"] == json!("insufficient"))
        .collect();
    assert_eq!(ud.len(), 1, "{:?}", r.out.unsure_default);
    assert_eq!(ud[0]["end"], json!("decided"));
}

#[test]
fn 开guard_照旧转交() {
    let 前 = "unsure_source({need: [\"材料\"], fetch: fn(q, need, m) { mat(\"去年三次交付延误都因资金周转\") }});";
    let r = 跑_按提示_把关(&程序(前), 读数, 生成(false)).unwrap_or_else(|e| panic!("{e}"));
    let v = r.out.value_json();
    assert_eq!(
        v["pending"],
        json!([{"cause": "tie", "via": ["purpose:weak"], "pos": 0}]),
        "{v}"
    );
    assert!(
        r.out
            .unsure_default
            .iter()
            .all(|x| x["cause"] != json!("tie")),
        "{:?}",
        r.out.unsure_default
    );
}

/// 作者填模块入口（modules_run，B0478）与目的入口共用 purpose_core：K 选一并列同样先走默认链，配了取法就取、再判、落字段
#[test]
fn 作者填模块入口_并列也走默认链() {
    let src = "import \"../../lib/derive/modules.jpp\";\nbudget {calls: 2000, cost: 0, depth: 8192};\n\
               unsure_source({need: [\"材料\"], fetch: fn(q, need, m) { mat(\"去年三次交付延误都因资金周转\") }});\n\
               let r = modules_run({material: [{on: mat({name: \"供应商0\", note: \"产能一般\"})}],\n\
                 predicates: [{text: \"这家供应商最主要的短板\", cut: \"k_ary\", over: [\"产能\", \"资金\", \"质量\"], field: \"weak\"}],\n\
                 done: [\"weak\"]}, {});\n\
               {value: map(r.value, fn(v) { v.fields }), pending: map(r.pending, fn(p) { {cause: p.cause, via: p.via, pos: p.pos} }), dropped: r.detail.dropped}";
    let r = 跑_按提示_关(src, 读数, |_p: &str| vec![]).unwrap_or_else(|e| panic!("{e}"));
    let v = r.out.value_json();
    assert_eq!(v["value"], json!([{"weak": "资金"}]), "{v}");
    assert_eq!(v["pending"], json!([]), "{v}");
    let ud: Vec<&Json> = r
        .out
        .unsure_default
        .iter()
        .filter(|x| x["cause"] == json!("tie"))
        .collect();
    assert_eq!(ud.len(), 1, "{:?}", r.out.unsure_default);
    assert_eq!(ud[0]["end"], json!("decided"));
}
