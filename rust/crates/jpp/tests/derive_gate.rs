//! 步 28（B0468）：验题闸门。被拒的候选不发出（端口没收到、账本没有它的判断条目）；确定性派生与唤出走同一个
//! 闸门；过闸门的按信息值取前 width 个，其余记「过闸门未入选」也不发出；第③段可关。闭包端口。
//!
//! 第①段（静态诊断，真 `diagnose`）命中阻断码即拒、两路候选同批进同一个闸门的用例在 `derive_gate_batch.rs`；
//! 本文件 (b) 只有唤出一路（坏候选与未入选不发出），两路同批见那边。

mod derive_support;
use derive_support::*;
use jpp::value::Answer;
use serde_json::{Value as Json, json};

fn noul(p: f64) -> Answer {
    Answer::Noul(p)
}

/// 诊断题（lib/diag.jpp）：「藏两判」问在题面材料上，题面提到「乙」时判 act
fn 夹具(t: &str, _q: &jpp::value::Question, s: &jpp::value::State) -> Answer {
    if t.contains("需要分别回答的判断") {
        noul(if s.on_text().contains("「乙」") {
            0.9
        } else {
            0.1
        })
    } else if t.contains("这段材料里有没有") {
        noul(0.9)
    } else if t.contains("的答案是否是") {
        noul(0.8)
    } else if t.contains("谁最合适") {
        Answer::Choice(vec![0.4, 0.4, 0.2])
    } else {
        panic!("夹具没有：{t}")
    }
}

const 三选一: &str = r#"budget {calls: 20, cost: 0, depth: 512};
let first = fn(item) { {q: select("这件事谁最合适牵头？", "t-root"), over: ["甲", "乙", "丙"]} };
"#;

#[test]
fn a_判断器诊断拒掉的候选不发出() {
    let src =
        三选一.to_string() + "let r = chain([{on: mat(\"三人的履历\")}], first, 3, {});\n" + 读出;
    let r = 跑(&src, 夹具, vec![]).unwrap();
    let v: Json = r.out.value_json();
    let rej = v["rejected"].as_array().unwrap();
    assert_eq!(rej.len(), 1, "{v}");
    assert_eq!(rej[0]["stage"], json!("judge-diag"));
    assert_eq!(rej[0]["codes"], json!(["two-judgments"]));
    let text = rej[0]["q"].as_str().unwrap();
    assert!(text.contains("「乙」"));
    assert!(!r.asked.iter().any(|t| t == text), "被拒的题端口没收到");
    let qh = rej[0]["qh"].as_str().unwrap();
    assert!(
        !判断条目(&r.ledger).iter().any(|j| j.1 == qh),
        "账本没有被拒候选的判断条目"
    );
    // 其余两道照常发出并合成
    assert_eq!(v["value"][0]["refined"], json!(["甲", "丙"]));
    // 闸门第①段每个候选一条 transform 条目（被拒的也有，裁定二十）
    assert_eq!(transform_条目数(&r.ledger), 3);
}

#[test]
fn b_唤出与确定性派生过同一个闸门_坏候选与未入选的都不发出() {
    let src = r#"budget {calls: 20, cost: 0, depth: 512};
let first = fn(item) { {q: test("这份提案可行吗？", "t-root"), line: {declare: {hi: 0.7, lo: 0.3}}} };
let r = chain([{on: mat("一份提案")}], first, 3, {elicit: true, width: 1});
"#
    .to_string()
        + 读出;
    let 生成 = vec![
        json!("这不是 JSON"),
        json!({"op": "test", "text": "提案里写明了资金来源吗？"}),
        json!({"op": "select", "text": "提案最大的风险来自哪一方面？", "over": ["资金", "技术", "市场"]}),
    ];
    let r = 跑(
        &src,
        |t, _q, s| {
            if t.contains("需要分别回答的判断") {
                noul(0.1)
            } else if t.contains("这段材料里有没有") {
                let _ = s;
                noul(0.9)
            } else if t.contains("可行吗") {
                noul(0.5)
            } else if t.contains("风险来自哪一方面") {
                Answer::Choice(vec![0.2, 0.7, 0.1])
            } else {
                panic!("夹具没有：{t}")
            }
        },
        生成,
    )
    .unwrap();
    let v: Json = r.out.value_json();
    assert_eq!(r.gens, 1);
    let rej = v["rejected"].as_array().unwrap();
    assert_eq!(rej.len(), 1, "{v}");
    assert_eq!(rej[0]["stage"], json!("shape"));
    assert_eq!(rej[0]["by"], json!("elicit"));
    let un = v["unchosen"].as_array().unwrap();
    assert_eq!(un.len(), 1);
    assert_eq!(
        un[0]["q"],
        json!("提案里写明了资金来源吗？"),
        "K 选一（三块）胜过是非题（两块）"
    );
    assert!(
        !r.asked
            .iter()
            .any(|t| t.contains("资金来源") || t.contains("坏候选"))
    );
    assert!(r.asked.iter().any(|t| t == "提案最大的风险来自哪一方面？"));
    assert_eq!(
        v["value"][0]["path"],
        json!(["author:unsure(band)", "elicit:pick(1)"])
    );
    // 原题 1 +「藏两判」2 +「在不在」2（两个过了第⓪①②段的候选，每个两道诊断）+ 发出 1
    assert_eq!(r.calls, 6);
    // 唤出的题 hop 2
    assert_eq!(判断条目(&r.ledger).iter().map(|j| j.3).max(), Some(2));
}

#[test]
fn c_第三段可关_不发诊断题() {
    let src = 三选一.to_string()
        + "let r = chain([{on: mat(\"三人的履历\")}], first, 3, {judge_diag: false});\n"
        + 读出;
    let r = 跑_关(&src, 夹具, vec![]).unwrap();
    assert!(!r.asked.iter().any(|t| t.contains("需要分别回答的判断")));
    assert_eq!(r.calls, 2, "原题 1 + 三道子题同一状态 1");
}

#[test]
fn d_作者手写的第一题不过闸门() {
    // 作者的题照旧只得到提示，不被派生题的阻断表拦下（意图汇编 11a）；诊断接入后本条才有牙
    let src = r#"budget {calls: 20, cost: 0, depth: 512};
let first = fn(item) { {q: test("这份材料有助于判断提案是否可行吗？", "t-root")} };
let r = chain([{on: mat("一份提案")}], first, 3, {});
"#
    .to_string()
        + 读出;
    let r = 跑_关(&src, |_t, _q, _s| noul(0.8), vec![]).unwrap();
    assert_eq!(
        r.asked,
        vec!["这份材料有助于判断提案是否可行吗？".to_string()]
    );
    assert_eq!(transform_条目数(&r.ledger), 0);
}
