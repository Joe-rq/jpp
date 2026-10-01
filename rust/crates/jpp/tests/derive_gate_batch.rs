//! 步 28（B0468）：验题闸门的三条显式用例（过程记录 §16.1 第 6、7 项）。
//! (a) 第①段命中阻断码即拒（真 `diagnose`）；(b) 确定性派生与唤出的候选同一跳同批进同一个闸门、被同一张阻断表拒；
//! (c) 第②段「状态材料不越出来源状态」。闭包端口，不发请求。

mod derive_support;
use derive_support::*;
use jpp::value::Answer;
use serde_json::{Value as Json, json};

fn noul(p: f64) -> Answer {
    Answer::Noul(p)
}

/// 诊断题都放行（藏两判 ignore、在材料里 act），只看第①段
fn 放行诊断(t: &str) -> Option<Answer> {
    if t.contains("需要分别回答的判断") {
        Some(noul(0.1))
    } else if t.contains("这段材料里有没有") {
        Some(noul(0.9))
    } else {
        None
    }
}

#[test]
fn a_第一段命中阻断码即拒_不发出() {
    let src = r#"budget {calls: 20, cost: 0, depth: 512};
let first = fn(item) { {q: test("这份提案可行吗？", "t-root"), line: {declare: {hi: 0.7, lo: 0.3}}} };
let r = chain([{on: mat("一份提案")}], first, 3, {elicit: true});
"#
    .to_string()
        + 读出;
    let 生成 = vec![
        json!({"op": "test", "text": "这份材料有助于判断提案是否可行吗？"}),
        json!({"op": "test", "text": "提案的成本是否合理，并且周期是否可行？"}),
        json!({"op": "test", "text": "提案里写明了资金来源吗？"}),
    ];
    let r = 跑(
        &src,
        |t, _q, _s| {
            放行诊断(t).unwrap_or_else(|| {
                if t.contains("可行吗") {
                    noul(0.5)
                } else if t.contains("资金来源") {
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
    let rej = v["rejected"].as_array().unwrap();
    let got: Vec<(String, Json)> = rej
        .iter()
        .map(|x| (x["stage"].as_str().unwrap().to_string(), x["codes"].clone()))
        .collect();
    assert!(
        got.contains(&("static".into(), json!(["W-diag-meta"]))),
        "{v}"
    );
    assert!(
        got.contains(&("static".into(), json!(["W-diag-two-judgments"]))),
        "{v}"
    );
    assert_eq!(rej.len(), 2, "{v}");
    for x in rej {
        let t = x["q"].as_str().unwrap();
        assert!(
            !r.asked.iter().any(|a| a == t),
            "被第①段拒的题端口没收到：{t}"
        );
    }
    assert!(
        r.asked.iter().any(|a| a == "提案里写明了资金来源吗？"),
        "没被拒的照常发出"
    );
}

#[test]
fn b_两路候选同批进同一个闸门() {
    // x1 走确定性派生：题式上 act 块的签名，填空题的模板一题两问；x2 走唤出：属性题落在带里，生成器出一道元题
    let src = r#"budget {calls: 30, cost: 0, depth: 512};
let deal = form("test", "甲是否守约，并且是否按时交付{物}？", {calib: "t-deal"});
let r1 = form("test", "甲会续约吗？", {calib: "t-r1", on: {act: {form: deal, params: [{slot: "物", choices: ["货物", "文件"]}]}}});
let r2 = form("test", "这份提案可行吗？", {calib: "t-r2"});
let first = fn(item) { if has(item, "b") { {form: r2, line: {declare: {hi: 0.7, lo: 0.3}}} } else { {form: r1} } };
let r = chain([{on: mat("甲的合同记录")}, {on: mat("一份提案"), b: true}], first, 2, {elicit: true});
"#
    .to_string()
        + 读出;
    let r = 跑(
        &src,
        |t, _q, _s| {
            放行诊断(t).unwrap_or_else(|| {
                if t.contains("续约吗") {
                    noul(0.8)
                } else if t.contains("可行吗") {
                    noul(0.5)
                } else {
                    panic!("夹具没有：{t}")
                }
            })
        },
        vec![json!({"op": "test", "text": "这份材料有助于判断提案是否可行吗？"})],
    )
    .unwrap();
    let v: Json = r.out.value_json();
    assert_eq!(r.gens, 1);
    // 同一跳（第 1 跳之后的那一次闸门）派生 2、拒 2：两路的候选在同一次 derive_gate 调用里
    assert_eq!(v["per_hop"][0]["derived"], json!(2), "{v}");
    assert_eq!(v["per_hop"][0]["rejected"], json!(2), "{v}");
    let rej = v["rejected"].as_array().unwrap();
    let by_stage: Vec<(String, String, Json)> = rej
        .iter()
        .map(|x| {
            (
                x["by"].as_str().unwrap().to_string(),
                x["stage"].as_str().unwrap().to_string(),
                x["codes"].clone(),
            )
        })
        .collect();
    assert!(
        by_stage.contains(&(
            "fill".into(),
            "static".into(),
            json!(["W-diag-two-judgments"])
        )),
        "{v}"
    );
    assert!(
        by_stage.contains(&("elicit".into(), "static".into(), json!(["W-diag-meta"]))),
        "{v}"
    );
    // 两路候选各一条闸门 transform 条目（同一个 diagnose）
    assert_eq!(
        transform_条目数(&r.ledger),
        3,
        "两个候选的 diagnose + 一次 gen"
    );
}

#[test]
fn c_第二段_状态材料不越出来源状态() {
    let src = r#"budget {calls: 10, cost: 0, depth: 512};
let n = derive_node("x0", {on: mat("甲的合同记录")}, test("甲会续约吗？", "t0"), {});
let ok = derive_child(n, unit, 0, test("甲按时交付了吗？", "t1"), {by: "elicit"});
let bad = with(derive_child(n, unit, 1, test("乙按时交付了吗？", "t2"), {by: "elicit"}), "st", state(mat("乙的合同记录")));
let g = derive_gate([ok, bad], {judge_diag: false});
{pass: map(g.pass, fn(c) { c.q.text }), rejected: map(g.rejected, fn(x) { {q: x.q, stage: x.stage, codes: x.codes} })}
"#;
    let r = 跑(src, |_t, _q, _s| panic!("不该判断"), vec![]).unwrap();
    let v: Json = r.out.value_json();
    assert_eq!(v["pass"], json!(["甲按时交付了吗？"]), "{v}");
    assert_eq!(
        v["rejected"],
        json!([{"q": "乙按时交付了吗？", "stage": "structure", "codes": ["state-beyond-source"]}])
    );
}
