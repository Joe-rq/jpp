//! Z0512（线 A N6）：唤出不带材料，同一提示（父题题面 + 出口原因）在不同材料项上命中同一次生成；
//! 生成器调用数随不同提示的个数增长，不随材料项数增长。预注册：地基/过程记录/工程-Z0512-唤出按题记忆.md。
//! 闭包端口，不发请求。数题按题面全等计，伴随元题不计入。

mod derive_support;
use derive_support::*;
use jpp::value::Answer;
use serde_json::{Value as Json, json};

fn 程序(n: usize) -> String {
    let xs: Vec<String> = (0..n)
        .map(|i| format!("{{on: mat(\"方案{i}：周期与人手各不相同\")}}"))
        .collect();
    format!(
        "budget {{calls: 4000, cost: 0, depth: 8192}};
let first = fn(item) {{ {{q: test(\"根题：这个方案可行吗？\", \"t-root\")}} }};
let r = chain([{}], first, 2, {{elicit: true}});
{{n: len(r.value), pending: map(r.pending, fn(p) {{ {{cause: p.cause, exit: p.exit}} }}), hops: r.detail.hops}}",
        xs.join(", ")
    )
}

/// 根题一律并列（走唤出后备）；诊断放行；唤出的子题判出
fn 读数(t: &str, _q: &jpp::value::Question, _s: &jpp::value::State) -> Answer {
    if t.contains("需要分别回答的判断") {
        Answer::Noul(0.1)
    } else if t.contains("这段材料里有没有") {
        Answer::Noul(0.9)
    } else if t.starts_with("根题") {
        Answer::Noul(0.5)
    } else {
        Answer::Noul(0.8)
    }
}

fn 候选(_p: &str) -> Vec<Json> {
    vec![
        json!({"op": "test", "text": "方案的预算写清楚了吗？"}),
        json!({"op": "test", "text": "方案的负责人写明了吗？"}),
    ]
}

#[test]
fn 生成器调用数与材料项数无关() {
    let a = 跑_按提示(&程序(3), 读数, 候选).unwrap();
    let b = 跑_按提示(&程序(30), 读数, 候选).unwrap();
    // 只有一个不同的提示（根题 + unsure(tie)）：一次生成
    assert_eq!(a.gens, 1, "{}", a.out.value_json());
    assert_eq!(b.gens, 1, "{}", b.out.value_json());
    // 每项仍各问一遍唤出的两道子题
    assert_eq!(
        a.asked
            .iter()
            .filter(|x| x.as_str() == "方案的预算写清楚了吗？")
            .count(),
        3
    );
    assert_eq!(
        b.asked
            .iter()
            .filter(|x| x.as_str() == "方案的预算写清楚了吗？")
            .count(),
        30
    );
    assert_eq!(b.out.value_json()["n"], json!(60));
}
