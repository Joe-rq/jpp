//! Z0535（线 A N9 的一半）：无作者线时已决出口成叶（缺省口径）；elicit:"always" 只作调试开关，行为照旧。
//! 预注册：地基/过程记录/工程-Z0535-派生何时停.md。闭包端口，不发请求。

mod derive_support;
use derive_support::*;
use jpp::value::Answer;
use serde_json::json;

fn 程序(elicit: &str) -> String {
    format!(
        "budget {{calls: 400, cost: 0, depth: 8192}};
let first = fn(item) {{ {{q: test(\"根题：这个方案可行吗？\", \"t-root\")}} }};
let r = chain([{{on: mat(\"方案甲：三个月，两人\")}}, {{on: mat(\"方案乙：半年，五人\")}}], first, 1, {{elicit: {elicit}}});
{{n: len(r.value), pending: map(r.pending, fn(p) {{ {{cause: p.cause, exit: p.exit}} }}), unasked: len(r.detail.unasked)}}"
    )
}

fn 读数(t: &str, _q: &jpp::value::Question, s: &jpp::value::State) -> Answer {
    if t.contains("需要分别回答的判断") {
        Answer::Noul(0.1)
    } else if t.contains("这段材料里有没有") {
        Answer::Noul(0.9)
    } else if s
        .on
        .first()
        .is_some_and(|m| m.content.to_string().contains("方案甲"))
    {
        Answer::Noul(0.8)
    } else {
        Answer::Noul(0.2)
    }
}

fn 候选(_p: &str) -> Vec<serde_json::Value> {
    vec![json!({"op": "test", "text": "方案的预算写清楚了吗？"})]
}

#[test]
fn 缺省口径_已决出口成叶_不唤出() {
    let r = 跑_按提示(&程序("true"), 读数, 候选).unwrap();
    let v = r.out.value_json();
    assert_eq!(r.gens, 0, "{v}");
    assert_eq!(v["n"], json!(2), "{v}");
}

#[test]
fn 调试开关_always_每个出口都唤出() {
    let r = 跑_按提示(&程序("\"always\""), 读数, 候选).unwrap();
    let v = r.out.value_json();
    assert!(r.gens >= 1, "{v}");
    assert_eq!(v["n"], json!(0), "{v}");
    assert!(v["unasked"].as_i64().unwrap() > 0, "{v}");
}
