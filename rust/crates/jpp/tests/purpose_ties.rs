//! 并列读数（真机概率保留两位小数，常出并列；臂 0′ 真机踩过「同一出口 handle 两次、第一次就丢了未决」）：
//! purpose_run 与 select_material 在并列读数下每个出口只处理一次（不报 J-05、程序跑完），未决都进 pending。
//! 覆盖：前提题恰好 0.5、查题库 K 选一并列、选择题 argmax 并列、是非题恰好 0.5、程度题档并列、有序名次全并列。
//! 闭包端口，不发请求；任务是虚构的「挑供应商」。

mod derive_support;
use derive_support::*;
use jpp::value::{Answer, Question, State};
use serde_json::{Value as Json, json};

const 目的: &str =
    "在这些供应商里找出最可能按期交付我们这份订单的，排好先后，并说出每家最大的风险。";

fn 程序(n: usize) -> String {
    let xs: Vec<String> = (0..n)
        .map(|i| format!("{{on: mat({{name: \"供应商{i}\", note: \"交付周期六周\"}})}}"))
        .collect();
    format!(
        "import \"../../lib/derive/purpose.jpp\";\nbudget {{calls: 2000, cost: 0, depth: 8192}};\nlet r = purpose_run(\"{目的}\", [{}], {{}});\n{}",
        xs.join(", "),
        "{value: map(r.value, fn(v) { v.fields }), pending: map(r.pending, fn(p) { {cause: p.cause, via: p.via, pos: p.pos} }), detail: r.detail}"
    )
}

fn 模块() -> Json {
    json!({"material": "供应商",
           "predicates": [
             {"text": "这家供应商能按期交付我们这份订单", "cut": "ordered", "scale": ["不能", "难说", "能"], "request": "all", "field": "rank",
              "cut_from": "purpose", "scale_from": "purpose"},
             {"text": "这家供应商交付这份订单最大的风险", "cut": "k_ary", "over": ["产能", "资金", "质量", "没有明显风险"], "request": "one", "field": "risk",
              "cut_from": "purpose", "over_from": "purpose"},
             {"text": "这家供应商写明了交付周期", "cut": "binary", "cut_from": "purpose", "request": "one", "field": "stated"}],
           "request": null, "presupposition": null, "context": null, "reference": null, "budget": null, "unsure": null,
           "done": ["rank", "risk", "stated"]})
}

fn 生成(p: &str) -> Vec<Json> {
    if p.contains("字面前提题") {
        补if_false(vec![
            json!({"op": "test", "text": "材料里写明了交付周期吗？"}),
        ])
    } else if p.starts_with("下面是一段目的") {
        vec![模块()]
    } else if p.contains("按期交付我们这份订单」") {
        vec![
            json!({"op": "measure", "text": "这家供应商按期交付这份订单的把握有多大？", "scale": ["不能", "难说", "能"]}),
        ]
    } else if p.contains("最大的风险」") {
        vec![
            json!({"op": "select", "text": "这家供应商交付这份订单最大的风险在哪一方面？", "over": ["产能", "资金", "质量", "没有明显风险"]}),
        ]
    } else {
        vec![json!({"op": "test", "text": "这家供应商写明了交付周期吗？"})]
    }
}

/// 全是并列：是非题 0.5；K 选一前两项并列（查题库那次也并列）；程度题前两档并列。诊断照常放行，否则闸门把题全拒了
fn 并列(t: &str, q: &Question, s: &State) -> Answer {
    if t.contains("需要分别回答的判断") {
        return Answer::Noul(0.1);
    }
    if t.contains("这段材料里有没有") {
        return Answer::Noul(0.9);
    }
    match q.op {
        jpp::value::Op::Test => Answer::Noul(0.5),
        jpp::value::Op::Select => {
            let k = s.over.len();
            let mut v = vec![0.0; k];
            v[0] = 0.5;
            v[1] = 0.5;
            Answer::Choice(v)
        }
        jpp::value::Op::Measure => {
            let k = q.scale.len();
            let mut v = vec![0.0; k];
            v[0] = 0.5;
            v[1] = 0.5;
            Answer::Score(v)
        }
    }
}

#[test]
fn 并列读数_程序跑完_空字段都有去向() {
    for n in [1, 4] {
        let r = 跑_按提示(&程序(n), 并列, 生成)
            .unwrap_or_else(|e| panic!("并列读数下程序没跑完（{n} 项）：{e}"));
        let v = r.out.value_json();
        let rows = v["value"].as_array().unwrap();
        assert_eq!(rows.len(), n, "{v}");
        let pend = v["pending"].as_array().unwrap();
        // 前提题恰好 0.5：每项都拿不准、都存活进深判，前提题出口各进 pending 一条
        assert_eq!(
            pend.iter()
                .filter(|p| p["via"]
                    .as_array()
                    .is_some_and(|a| a.iter().any(|x| x == "select#0")))
                .count(),
            n,
            "{v}"
        );
        // 三个谓词都成了题；K 选一 argmax 并列、是非题恰好 0.5 判不出，字段为空（不是被丢掉的空：下面核去向）
        assert_eq!(
            v["detail"]["fields"],
            json!(["rank", "risk", "stated"]),
            "{v}"
        );
        assert!(
            rows.iter()
                .all(|x| x["risk"].is_null() && x["stated"].is_null()),
            "{v}"
        );
        // 名次全并列：同一档，名次都是 1
        assert!(rows.iter().all(|x| x["rank"] == json!(1)), "{v}");
        // 字段为空的每一格，pending 里有同一项同一字段的去向，或默认链放弃、记在 detail.dropped 里（N-T6：并列先走默认链，
        // 「为什么拿不准」那道在这个夹具下也并列，语言自己的诊断题拿不准就按末端放弃）
        let dropped = v["detail"]["dropped"].as_array().unwrap();
        for (i, row) in rows.iter().enumerate() {
            for f in v["detail"]["fields"].as_array().unwrap() {
                let f = f.as_str().unwrap();
                if row[f].is_null() {
                    let via = format!("purpose:{f}");
                    let 转交 = pend.iter().any(|p| {
                        p["pos"] == json!(i)
                            && p["via"]
                                .as_array()
                                .is_some_and(|a| a.iter().any(|x| x == via.as_str()))
                    });
                    let 放弃 = dropped
                        .iter()
                        .any(|d| d["pos"] == json!(i) && d["field"] == json!(f));
                    assert!(
                        转交 != 放弃,
                        "第 {i} 项字段 {f} 为空，去向应恰好一个（转交或放弃）：{v}"
                    );
                }
            }
        }
    }
}
