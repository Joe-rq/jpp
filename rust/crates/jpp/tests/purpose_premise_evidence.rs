//! 前提≠证据（derive-15，主会话 2026-10-02；预注册：地基/规划/第二靶子-预注册-草稿.md 8.6）：
//! ① 守则写明前提与结果无关；② 样本上有候选标签可比时，前提的分法与某个 K 选一候选的分法一致即 evidence_not_premise 弃掉。
//! ③（derive-16、17，预注册 8.7、8.8）生成器给每道前提自报 if_false（结构 {falls_to, value?}）：指向候选的按证据弃掉，
//! unanswerable 保留（不看措辞；derive-18 由 none 改名，旧值按不合形拒），不合形在闸门第⓪段拒；每轮原文与处理结果记进账本（by: gen，带 taint）。
//! 闭包端口，不发请求；任务是虚构的「挑供应商并归类」，标签取自记录里与候选原文相同的字段取值。
//! 只测①②的用例给候选补 if_false: none（补if_false）；测③的用例原样给。

mod derive_support;
use derive_support::*;
use jpp::value::{Answer, Question, State};
use serde_json::{Value as Json, json};
use std::cell::RefCell;

const 目的: &str = "在这些供应商里找出能按期交付我们这份订单的，并把每家归为汽车、家电或玩具。";
const 深判题: &str = "这家供应商能在六周内交付吗？";
const 证据题: &str = "材料里写明了这家供应商做过汽车零件吗？";
const 交期题: &str = "材料里写明了这家供应商的交期不短于六周吗？";

/// n 家：kind 按 i mod 3 取 汽车 / 家电 / 玩具（与候选原文相同，可比）；weeks = 4 + (i mod 5)
fn 条目(n: usize, kinds: [&str; 3]) -> String {
    let xs: Vec<String> = (0..n)
        .map(|i| {
            format!(
                "{{on: mat({{name: \"供应商{i}\", kind: \"{}\", weeks: {}}})}}",
                kinds[i % 3],
                4 + i % 5
            )
        })
        .collect();
    format!("[{}]", xs.join(", "))
}

fn 程序(n: usize, kinds: [&str; 3]) -> String {
    format!(
        "import \"../../lib/derive/purpose.jpp\";\nbudget {{calls: 4000, cost: 0, depth: 8192}};\nlet r = purpose_run(\"{目的}\", {}, {{}});\n{}",
        条目(n, kinds),
        "{value: r.value, pending: map(r.pending, fn(p) { {cause: p.cause, via: p.via, exit: p.exit} }), evidence: r.evidence, detail: r.detail}"
    )
}

fn 模块(k_ary: bool) -> Json {
    let mut ps = vec![
        json!({"text": "这家供应商能按期交付", "cut": "binary", "cut_from": "purpose", "request": "one", "field": "ontime"}),
    ];
    if k_ary {
        ps.push(json!({"text": "这家供应商属于哪一类", "cut": "k_ary", "cut_from": "purpose", "over": ["汽车", "家电", "玩具"],
                       "over_from": "purpose", "request": "one", "field": "class"}));
    }
    let done: Vec<&str> = if k_ary {
        vec!["ontime", "class"]
    } else {
        vec!["ontime"]
    };
    json!({"material": "供应商", "predicates": ps, "request": null, "presupposition": null, "context": null, "reference": null,
           "budget": null, "unsure": null, "done": done})
}

fn 字段(s: &State, k: &str) -> Json {
    s.on.first()
        .and_then(|m| m.content.get(k))
        .cloned()
        .unwrap_or(Json::Null)
}

fn 读数(t: &str, _q: &Question, s: &State) -> Answer {
    if t.contains("需要分别回答的判断") {
        Answer::Noul(0.1)
    } else if t.contains("这段材料里有没有") {
        Answer::Noul(0.9)
    } else if t.contains("已认证的题问的是同一件事") || t.contains("说的是同一件事")
    {
        let k = s.over.len();
        let mut v = vec![0.0; k];
        v[k - 1] = 1.0;
        Answer::Choice(v)
    } else if t == 证据题 {
        Answer::Noul(
            if 字段(s, "kind").as_str().unwrap_or("").starts_with("汽车") {
                0.9
            } else {
                0.1
            },
        )
    } else if t == 交期题 {
        Answer::Noul(if 字段(s, "weeks").as_i64().unwrap_or(0) >= 6 {
            0.9
        } else {
            0.1
        })
    } else if !s.over.is_empty() {
        let mut v = vec![0.0; s.over.len()];
        v[0] = 1.0;
        Answer::Choice(v)
    } else {
        Answer::Noul(0.8)
    }
}

struct 跑记 {
    v: Json,
    prompts: Vec<String>,
    asked: Vec<String>,
}

/// 前提派生每一轮依次给 rounds[i]（超出的轮给最后一个），缺 if_false 的补 none
fn 跑(n: usize, kinds: [&str; 3], k_ary: bool, rounds: Vec<Vec<Json>>) -> 跑记 {
    跑_原样(
        n,
        kinds,
        k_ary,
        rounds.into_iter().map(补if_false).collect(),
    )
}

/// 前提派生每一轮依次给 rounds[i]，原样（测 if_false 用）
fn 跑_原样(n: usize, kinds: [&str; 3], k_ary: bool, rounds: Vec<Vec<Json>>) -> 跑记 {
    let prompts = RefCell::new(Vec::<String>::new());
    let k = RefCell::new(0usize);
    let r = 跑_按提示(&程序(n, kinds), 读数, |p: &str| {
        if p.contains("字面前提题") {
            prompts.borrow_mut().push(p.to_string());
            let i = *k.borrow();
            *k.borrow_mut() += 1;
            rounds[i.min(rounds.len() - 1)].clone()
        } else if p.starts_with("下面是一段目的") {
            vec![模块(k_ary)]
        } else if p.contains("属于哪一类") {
            vec![json!({"op": "select", "text": "这家供应商属于哪一类？", "over": ["汽车", "家电", "玩具"]})]
        } else {
            vec![json!({"op": "test", "text": 深判题})]
        }
    })
    .unwrap_or_else(|e| panic!("{e}"));
    跑记 {
        v: r.out.value_json(),
        prompts: prompts.into_inner(),
        asked: r.asked.clone(),
    }
}

const 可比: [&str; 3] = ["汽车", "家电", "玩具"];

/// 判断类前提的样本分法与候选「汽车」一致：evidence_not_premise 弃掉，记下谓词、候选、比了几项；另一道照常入选
#[test]
fn 判断类前提_与候选分法一致_按证据弃掉() {
    let x = 跑(
        30,
        可比,
        true,
        vec![vec![
            json!({"op": "test", "text": 证据题}),
            json!({"op": "test", "text": 交期题}),
        ]],
    );
    let d = &x.v["detail"]["premise"];
    assert_eq!(d["passed"], json!([交期题]), "{d}");
    let dr = d["dropped"].as_array().unwrap();
    assert_eq!(dr.len(), 1, "{d}");
    assert_eq!(dr[0]["q"], 证据题, "{d}");
    assert_eq!(dr[0]["reason"], "evidence_not_premise", "{d}");
    assert_eq!(dr[0]["by"], "judge", "{d}");
    assert_eq!(dr[0]["field"], "class", "{d}");
    assert_eq!(dr[0]["cand"], "汽车", "{d}");
    assert_eq!(dr[0]["side"], "act", "{d}");
    assert_eq!(
        dr[0]["labelled"],
        json!(20),
        "样本 20 项都有标签、都已决：{d}"
    );
    // 守则那一句在提示里
    assert!(
        x.prompts[0].contains("前提必须与结果无关"),
        "{}",
        x.prompts[0]
    );
}

/// 代码类前提在全部项上比：kind eq 汽车 与候选「汽车」一致，按证据弃掉（by: code）
#[test]
fn 代码类前提_与候选分法一致_按证据弃掉() {
    let x = 跑(
        12,
        可比,
        true,
        vec![vec![
            json!({"op": "code", "field": "kind", "check": "eq", "value": "汽车"}),
            json!({"op": "test", "text": 交期题}),
        ]],
    );
    let d = &x.v["detail"]["premise"];
    let dr = d["dropped"].as_array().unwrap();
    assert_eq!(dr.len(), 1, "{d}");
    assert_eq!(dr[0]["reason"], "evidence_not_premise", "{d}");
    assert_eq!(dr[0]["by"], "code", "{d}");
    assert_eq!(dr[0]["cand"], "汽车", "{d}");
    assert_eq!(dr[0]["labelled"], json!(12), "{d}");
    assert_eq!(d["code"], json!([]), "{d}");
    assert_eq!(d["passed"], json!([交期题]), "{d}");
}

/// 一道都没留下：带着按证据弃掉的再派生一次，第二轮提示写明它们是证据
#[test]
fn 全按证据弃掉_再派生_提示写明是证据() {
    let x = 跑(
        30,
        可比,
        true,
        vec![
            vec![json!({"op": "test", "text": 证据题})],
            vec![json!({"op": "test", "text": 交期题})],
        ],
    );
    assert_eq!(x.prompts.len(), 2, "{:?}", x.prompts);
    assert!(
        x.prompts[1].contains("是那个答案的证据") && x.prompts[1].contains(证据题),
        "{}",
        x.prompts[1]
    );
    assert!(!x.prompts[1].contains("没有区分度"), "{}", x.prompts[1]);
    let d = &x.v["detail"]["premise"];
    assert_eq!(d["rounds"], json!(2), "{d}");
    assert_eq!(d["passed"], json!([交期题]), "{d}");
}

/// 比不了只靠守则：记录取值与候选原文不同（无标签），或目的没有 K 选一谓词，同一道题照常入选
#[test]
fn 无可比标签_不按证据弃() {
    let x = 跑(
        30,
        ["汽车零件", "家用电器", "儿童玩具"],
        true,
        vec![vec![json!({"op": "test", "text": 证据题})]],
    );
    let d = &x.v["detail"]["premise"];
    assert_eq!(d["passed"], json!([证据题]), "{d}");
    assert_eq!(d["dropped"], json!([]), "{d}");
    let x = 跑(
        30,
        可比,
        false,
        vec![vec![json!({"op": "test", "text": 证据题})]],
    );
    let d = &x.v["detail"]["premise"];
    assert_eq!(d["passed"], json!([证据题]), "{d}");
    assert_eq!(d["dropped"], json!([]), "{d}");
}

/// ③ if_false 指向候选：按证据弃掉，不过闸门、不抽样（判断端口没收到这道题）；none 的照常入选；提示列出候选原文
#[test]
fn if_false指向候选_按证据弃掉_不判() {
    let x = 跑_原样(
        30,
        ["汽车零件", "家用电器", "儿童玩具"],
        true,
        vec![vec![
            json!({"op": "test", "text": 证据题, "if_false": {"falls_to": "candidate", "value": "家电"}}),
            json!({"op": "test", "text": 交期题, "if_false": {"falls_to": "unanswerable"}}),
        ]],
    );
    let d = &x.v["detail"]["premise"];
    assert_eq!(d["passed"], json!([交期题]), "{d}");
    let dr = d["dropped"].as_array().unwrap();
    assert_eq!(dr.len(), 1, "{d}");
    assert_eq!(dr[0]["q"], 证据题, "{d}");
    assert_eq!(dr[0]["reason"], "evidence_not_premise", "{d}");
    assert_eq!(dr[0]["by"], "if_false", "{d}");
    assert_eq!(dr[0]["field"], "class", "{d}");
    assert_eq!(dr[0]["cand"], "家电", "{d}");
    assert!(
        x.asked.iter().all(|t| t != 证据题),
        "按证据弃掉的不判：{:?}",
        x.asked
    );
    assert!(
        x.prompts[0].contains("if_false") && x.prompts[0].contains("「汽车」「家电」「玩具」"),
        "{}",
        x.prompts[0]
    );
}

/// ③ 代码类前提 if_false 指向候选：按证据弃掉，不执行（没有代码变换）
#[test]
fn if_false指向候选_代码类不执行() {
    let x = 跑_原样(
        12,
        ["汽车零件", "家用电器", "儿童玩具"],
        true,
        vec![vec![
            json!({"op": "code", "field": "weeks", "check": "ge", "value": 6, "if_false": {"falls_to": "candidate", "value": "玩具"}}),
            json!({"op": "test", "text": 交期题, "if_false": {"falls_to": "unanswerable"}}),
        ]],
    );
    let d = &x.v["detail"]["premise"];
    let dr = d["dropped"].as_array().unwrap();
    assert_eq!(dr.len(), 1, "{d}");
    assert_eq!(dr[0]["by"], "if_false", "{d}");
    assert_eq!(dr[0]["q"], "代码：weeks ge 6", "{d}");
    assert_eq!(d["code"], json!([]), "{d}");
}

/// ③ 缺 if_false、或既不是候选也不是 none：闸门第⓪段拒（if-false-bad）；一道不剩时全部纳入
#[test]
fn if_false不合形_闸门拒() {
    let x = 跑_原样(
        12,
        ["汽车零件", "家用电器", "儿童玩具"],
        true,
        vec![vec![
            json!({"op": "test", "text": 证据题}),
            json!({"op": "code", "field": "weeks", "check": "ge", "value": 6, "if_false": 3}),
        ]],
    );
    let d = &x.v["detail"]["premise"];
    let rj = d["rejected"].as_array().unwrap();
    assert_eq!(rj.len(), 2, "{d}");
    assert!(
        rj.iter()
            .all(|r| r["stage"] == "shape" && r["codes"] == json!(["if-false-bad"])),
        "{d}"
    );
    assert_eq!(d["passed"], json!([]), "{d}");
    assert_eq!(d["dropped"], json!([]), "{d}");
    assert_eq!(d["excluded"], json!(0), "{d}");
    // 对不上候选也不是 none
    let x = 跑_原样(
        12,
        ["汽车零件", "家用电器", "儿童玩具"],
        true,
        vec![vec![
            json!({"op": "test", "text": 交期题, "if_false": {"falls_to": "candidate", "value": "都不是"}}),
        ]],
    );
    let d = &x.v["detail"]["premise"];
    assert_eq!(d["rejected"][0]["codes"], json!(["if-false-bad"]), "{d}");
    assert_eq!(d["passed"], json!([]), "{d}");
}

/// ③ 目的没有 K 选一谓词时只认 unanswerable：写了候选样子的值也算不合形
#[test]
fn 无候选时只认unanswerable() {
    let x = 跑_原样(
        12,
        可比,
        false,
        vec![vec![
            json!({"op": "test", "text": 交期题, "if_false": {"falls_to": "unanswerable"}}),
            json!({"op": "test", "text": 证据题, "if_false": {"falls_to": "candidate", "value": "汽车"}}),
        ]],
    );
    let d = &x.v["detail"]["premise"];
    assert_eq!(d["passed"], json!([交期题]), "{d}");
    assert_eq!(d["rejected"][0]["codes"], json!(["if-false-bad"]), "{d}");
    assert!(x.prompts[0].contains("if_false 一律写"), "{}", x.prompts[0]);
}

/// ③ unanswerable 不看措辞：falls_to 是 unanswerable 就保留，多写的说明不管；写成文字 "none"（不是结构）算不合形
#[test]
fn if_false_unanswerable不看措辞_文字none不合形() {
    let x = 跑_原样(
        12,
        ["汽车零件", "家用电器", "儿童玩具"],
        true,
        vec![vec![
            json!({"op": "test", "text": 交期题, "if_false": {"falls_to": "unanswerable", "why": "不成立时这家无从判断"}}),
            json!({"op": "test", "text": 证据题, "if_false": "none"}),
        ]],
    );
    let d = &x.v["detail"]["premise"];
    assert_eq!(d["passed"], json!([交期题]), "{d}");
    assert_eq!(d["rejected"][0]["codes"], json!(["if-false-bad"]), "{d}");
}

/// ③ 每轮的 if_false 原文与处理结果进账本：一次变换，by: gen，带 taint
#[test]
fn if_false进账本_带taint() {
    // 真生成器缺省 untrusted（B149）；这里显式声明
    let r = 跑_按提示_带污(&程序(12, ["汽车零件", "家用电器", "儿童玩具"]), 读数, |p: &str| {
        if p.contains("字面前提题") {
            vec![json!({"op": "test", "text": 证据题, "if_false": {"falls_to": "candidate", "value": "汽车"}}),
                 json!({"op": "test", "text": 交期题, "if_false": {"falls_to": "unanswerable"}})]
        } else if p.starts_with("下面是一段目的") {
            vec![模块(true)]
        } else if p.contains("属于哪一类") {
            vec![json!({"op": "select", "text": "这家供应商属于哪一类？", "over": ["汽车", "家电", "玩具"]})]
        } else {
            vec![json!({"op": "test", "text": 深判题})]
        }
    }, jpp::value::Taint::Untrusted)
    .unwrap_or_else(|e| panic!("{e}"));
    let logs: Vec<Json> = r
        .ledger
        .entries
        .iter()
        .filter_map(|e| match e {
            jpp::ledger::Entry::Effect { kind, output, .. }
                if kind == "transform" && output.get("by") == Some(&json!("gen")) =>
            {
                Some(serde_json::to_value(e).unwrap())
            }
            _ => None,
        })
        .collect();
    assert_eq!(logs.len(), 1, "{logs:?}");
    let s = logs[0].to_string();
    let out = &logs[0]["Effect"]["output"];
    assert_eq!(out["rows"][0]["result"], "evidence_not_premise", "{s}");
    assert_eq!(
        out["rows"][0]["if_false"],
        json!({"falls_to": "candidate", "value": "汽车"}),
        "{s}"
    );
    assert_eq!(out["rows"][1]["result"], "unanswerable", "{s}");
    assert_eq!(out["taint"], "Untrusted", "带 taint：{s}");
}

/// ③ derive-18：旧协议值 {falls_to: "none"} 按不合形拒，不做同义迁移（候选里的兜底可以叫 none，两者分开）；
/// 提示写明兜底候选不是落点
#[test]
fn 旧协议值none_按不合形拒() {
    let x = 跑_原样(
        12,
        ["汽车零件", "家用电器", "儿童玩具"],
        true,
        vec![vec![
            json!({"op": "test", "text": 证据题, "if_false": {"falls_to": "none"}}),
            json!({"op": "test", "text": 交期题, "if_false": {"falls_to": "unanswerable"}}),
        ]],
    );
    let d = &x.v["detail"]["premise"];
    assert_eq!(d["passed"], json!([交期题]), "{d}");
    assert_eq!(d["rejected"][0]["codes"], json!(["if-false-bad"]), "{d}");
    assert!(
        x.prompts[0].contains("unanswerable") && x.prompts[0].contains("兜底候选不是落点"),
        "{}",
        x.prompts[0]
    );
}
