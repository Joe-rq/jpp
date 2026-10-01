//! 步 28（B0468）：唤出（过程记录 §16.1 第 2、3 项）。守则要模型写的前提与证据槽解析时保留、接得上前提反面；
//! 请求非缺省记坏候选；先选后填没有可选值时唤出；`{elicit: "always"}` 两路候选同批过闸门。闭包端口。

mod derive_support;
use derive_support::*;
use jpp::value::Answer;
use serde_json::{Value as Json, json};

fn noul(p: f64) -> Answer {
    Answer::Noul(p)
}

fn 诊断放行(t: &str) -> Option<Answer> {
    if t.contains("需要分别回答的判断") {
        Some(noul(0.1))
    } else if t.contains("这段材料里有没有") {
        Some(noul(0.9))
    } else {
        None
    }
}

const 属性题: &str = r#"budget {calls: 30, cost: 0, depth: 512};
let first = fn(item) { {q: test("这份提案可行吗？", "t-root"), line: {declare: {hi: 0.7, lo: 0.3}}} };
"#;

#[test]
fn a_唤出题的前提与证据槽保留_缺参照时走前提反面() {
    let src = 属性题.to_string()
        + "let r = chain([{on: mat(\"一份提案\")}], first, 4, {elicit: true});\n"
        + 读出;
    let 生成 = vec![json!({"op": "test", "text": "提案的资金来源可靠吗？",
                           "presupposition": "提案写了资金来源", "evidence": ["ref"]})];
    let r = 跑(
        &src,
        |t, _q, _s| {
            诊断放行(t).unwrap_or_else(|| {
                if t.contains("可行吗") {
                    noul(0.5)
                } else if t == "是否提案写了资金来源？" {
                    noul(0.2)
                } else if t == "提案的资金来源可靠吗？" {
                    noul(0.8)
                } else {
                    panic!("夹具没有：{t}")
                }
            })
        },
        生成,
    )
    .unwrap();
    let v: Json = r.out.value_json();
    // 缺 ref（证据槽）：唤出题判完，出口是 insufficient（J-09），前提反面接手
    assert!(
        r.asked.iter().any(|t| t == "是否提案写了资金来源？"),
        "按唤出题的前提出前提题：{:?}",
        r.asked
    );
    let paths: Vec<Json> = v["value"]
        .as_array()
        .unwrap()
        .iter()
        .map(|x| x["path"].clone())
        .collect();
    assert!(
        paths.contains(&json!([
            "author:unsure(band)",
            "elicit:unsure(insufficient:ref)",
            "premise:ignore"
        ])),
        "{v}"
    );
    assert!(
        v["value"]
            .as_array()
            .unwrap()
            .iter()
            .any(|x| x["conclusion"] == json!("前提不成立")),
        "{v}"
    );
}

#[test]
fn b_请求不是缺省_记坏候选() {
    let src = 属性题.to_string()
        + "let r = chain([{on: mat(\"一份提案\")}], first, 2, {elicit: true});\n"
        + 读出;
    let 生成 = vec![
        json!({"op": "select", "text": "提案里有哪些风险？", "over": ["资金", "技术"], "request": "all"}),
        json!({"op": "test", "text": "提案里写明了资金来源吗？", "request": "whether"}),
    ];
    let r = 跑(
        &src,
        |t, _q, _s| {
            诊断放行(t).unwrap_or_else(|| {
                if t.contains("可行吗") {
                    noul(0.5)
                } else {
                    noul(0.8)
                }
            })
        },
        生成,
    )
    .unwrap();
    let v: Json = r.out.value_json();
    let rej = v["rejected"].as_array().unwrap();
    assert_eq!(rej.len(), 1, "{v}");
    assert_eq!(
        (rej[0]["stage"].clone(), rej[0]["codes"].clone()),
        (json!("shape"), json!(["request-unsupported"]))
    );
    assert!(
        r.asked.iter().any(|t| t == "提案里写明了资金来源吗？"),
        "缺省请求照常"
    );
}

#[test]
fn c_先选后填没有可选值_开着唤出就唤出() {
    let src = r#"budget {calls: 30, cost: 0, depth: 512};
let why = {params: [{slot: "主要原因", choices: ["价格"]}]};
let root = form("test", "这位客户会续约吗？", {calib: "t-root", on: {act: why}});
let first = fn(item) { {form: root} };
let r = chain([{on: mat("客户的沟通记录")}], first, 2, {elicit: true});
"#
    .to_string()
        + 读出;
    let r = 跑(
        &src,
        |t, _q, _s| {
            诊断放行(t).unwrap_or_else(|| {
                if t.contains("续约吗") {
                    noul(0.8)
                } else {
                    noul(0.7)
                }
            })
        },
        vec![json!({"op": "test", "text": "客户说过价格太高吗？"})],
    )
    .unwrap();
    assert_eq!(r.gens, 1, "可选值只有一项，确定性派生出不了题，唤出");
    assert!(
        r.asked.iter().any(|t| t == "客户说过价格太高吗？"),
        "{:?}",
        r.asked
    );
}

#[test]
fn d_always_两路候选同批过闸门() {
    let src = r#"budget {calls: 30, cost: 0, depth: 512};
let first = fn(item) { {q: measure("这份提案的风险有多大？", ["低", "中", "高"], "t-root"), line: {declare: {hi: 0.7}}} };
let r = chain([{on: mat("一份提案")}], first, 1, {elicit: "always"});
"#
    .to_string()
        + 读出;
    let r = 跑(
        &src,
        |t, _q, _s| {
            诊断放行(t).unwrap_or_else(|| {
                if t.contains("风险有多大") && !t.contains("至少达到") {
                    Answer::Score(vec![0.3, 0.4, 0.3])
                } else {
                    noul(0.6)
                }
            })
        },
        vec![json!({"op": "test", "text": "提案写了风险应对吗？"})],
    )
    .unwrap();
    let v: Json = r.out.value_json();
    assert_eq!(r.gens, 1);
    // 程度题落在带里：细化出两道「至少达到」，另唤出一道，三道同一次闸门
    assert_eq!(v["per_hop"][0]["derived"], json!(3), "{v}");
    assert_eq!(v["per_hop"][0]["passed"], json!(3), "{v}");
    // 跳数上限 1：派生出的三道过了闸门、留在下一跳没问；闸门的「在不在材料里」按条目问过这三道
    assert_eq!(v["unasked"], json!(3), "{v}");
}

/// 唤出题面来自生成器，是不可信材料（B58）：经题式造出的题与直接 `test(文字)` 一样带 Untrusted，
/// 题式不把生成的文字洗成可信
#[test]
fn e_唤出题经题式造出_taint_仍是不可信() {
    let src = r#"budget {calls: 5, cost: 0, depth: 512};
let raw = gen("出题", [], 1, 0);
let s = derive_spec(raw[0]);
let st = state(mat("一份提案"));
let e1 = cut(judge(st, test(s.text, "k")));
let e2 = cut(judge(st, fill(derive_form("test", s.text, {}, {}), {})));
let out = {direct: taint(e1), via_form: taint(e2)};
consume(e1, "drop");
consume(e2, "drop");
out
"#;
    let r = 跑_全(
        src,
        |_t, _q, _s| noul(0.8),
        vec![json!({"op": "test", "text": "提案里写明了资金来源吗？"})],
        &jpp::effects::CalibStore::new(),
        Some(jpp::value::Taint::Untrusted),
    )
    .unwrap();
    let v: Json = r.out.value_json();
    assert_eq!(
        v["direct"],
        json!("untrusted"),
        "对照：直接用生成的文字造题，出口不可信：{v}"
    );
    assert_eq!(
        v["via_form"],
        json!("untrusted"),
        "经题式造题同样不可信：{v}"
    );
}

/// 臂 3 空跑 3.1：程度题带 "request": "one"（模块里「每个条目一个答案」的词被抄成了题级请求）曾一路进 form、
/// 整个程序报 E-rt-question 中止。现在第⓪段按题型取缺省请求（程度题 degree），不合的拒为 request-unsupported，程序跑完
#[test]
fn f_程度题请求不是degree_记坏候选不崩() {
    let src = 属性题.to_string()
        + "let r = chain([{on: mat(\"一份提案\")}], first, 2, {elicit: true});\n"
        + 读出;
    let 生成 = vec![
        json!({"op": "measure", "text": "提案的资金有多充足？", "scale": ["不足", "一般", "充足"], "request": "one"}),
        json!({"op": "measure", "text": "提案的周期有多宽裕？", "scale": ["紧", "一般", "宽裕"], "request": "degree"}),
    ];
    let r = 跑(
        &src,
        |t, _q, _s| {
            诊断放行(t).unwrap_or_else(|| {
                if t.contains("可行吗") {
                    noul(0.5)
                } else {
                    Answer::Score(vec![0.1, 0.2, 0.7])
                }
            })
        },
        生成,
    )
    .unwrap_or_else(|e| panic!("程序没跑完：{e}"));
    let v: Json = r.out.value_json();
    let rej = v["rejected"].as_array().unwrap();
    assert_eq!(rej.len(), 1, "{v}");
    assert_eq!(
        (rej[0]["stage"].clone(), rej[0]["codes"].clone()),
        (json!("shape"), json!(["request-unsupported"]))
    );
    assert!(
        r.asked.iter().any(|t| t == "提案的周期有多宽裕？"),
        "程度题带缺省请求 degree 照常：{:?}",
        r.asked
    );
}

/// 唤出提示写明题级请求按题型取，并说明与模块里的 one / all 不是一回事
#[test]
fn g_唤出提示写明题级请求按题型取() {
    let src = "import \"../../lib/derive/rules.jpp\";\nbudget {calls: 0, cost: 0, depth: 64};\n\
               let p = derive_rules().purpose_elicit_prompt(\"{}\", \"某件事\", \"ordered\");\n\
               {degree: len(split(p, \"程度题 \\\"degree\\\"\")) > 1, apart: len(split(p, \"不是一回事\")) > 1}\n";
    let r = 跑(src, |_t, _q, _s| noul(0.5), vec![]).unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(r.out.value_json(), json!({"degree": true, "apart": true}));
}
