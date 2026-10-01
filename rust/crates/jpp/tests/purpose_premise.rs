//! B0470：前提题由目的派生，判过线的材料才纳入深判（库层读法，运行时 J-09 不动）。
//! 预注册：地基/过程记录/工程-B0470-前提由目的派生.md 第四节。闭包端口，不发请求；任务是虚构的「挑供应商」。

mod derive_support;
use derive_support::*;
use jpp::value::{Answer, Question, State};
use serde_json::{Value as Json, json};

const 目的: &str = "在这些供应商里找出能按期交付我们这份订单的。";
const 前提题: &str = "材料里写明了交付周期吗？";
const 深判题: &str = "这家供应商能在六周内交付吗？";

/// 6 家：0–2 写明交付周期，3、4 没写，5 写了「交付周期待定」（前提题拿不准）
fn 条目() -> String {
    let notes = [
        "交付周期六周",
        "交付周期五周",
        "交付周期四周",
        "只写了产能",
        "只写了账期",
        "交付周期待定",
    ];
    let xs: Vec<String> = notes
        .iter()
        .enumerate()
        .map(|(i, n)| format!("{{on: mat({{name: \"供应商{i}\", note: \"{n}\"}})}}"))
        .collect();
    format!("[{}]", xs.join(", "))
}

fn 程序() -> String {
    format!(
        "import \"../../lib/derive/purpose.jpp\";\nbudget {{calls: 2000, cost: 0, depth: 8192}};\nlet r = purpose_run(\"{目的}\", {}, {{}});\n{}",
        条目(),
        "{value: r.value, pending: map(r.pending, fn(p) { {cause: p.cause, via: p.via} }), evidence: r.evidence, detail: r.detail}"
    )
}

fn 模块() -> Json {
    json!({"material": "供应商", "predicates": [{"text": "这家供应商能按期交付", "cut": "binary", "cut_from": "purpose", "request": "one", "field": "ontime"}],
           "request": null, "presupposition": null, "context": null, "reference": null, "budget": null, "unsure": null, "done": ["ontime"]})
}

fn 生成(前提: Json) -> impl Fn(&str) -> Vec<Json> {
    move |p: &str| {
        if p.contains("字面前提题") {
            vec![前提.clone()]
        } else if p.starts_with("下面是一段目的") {
            vec![模块()]
        } else {
            vec![json!({"op": "test", "text": 深判题})]
        }
    }
}

fn 读数(t: &str, _q: &Question, s: &State) -> Answer {
    let note =
        s.on.first()
            .map(|m| m.content.to_string())
            .unwrap_or_default();
    if t.contains("需要分别回答的判断") {
        Answer::Noul(0.1)
    } else if t.contains("这段材料里有没有") {
        Answer::Noul(0.9)
    } else if t.contains("已认证的题问的是同一件事") {
        let k = s.over.len();
        let mut v = vec![0.0; k];
        v[k - 1] = 1.0;
        Answer::Choice(v)
    } else if t == 前提题 {
        Answer::Noul(if note.contains("待定") {
            0.5
        } else if note.contains("交付周期") {
            0.9
        } else {
            0.1
        })
    } else {
        Answer::Noul(0.8)
    }
}

#[test]
fn 派生前提筛材料_淘汰的有一行并能追到淘汰它的判断() {
    let r = 跑_按提示(&程序(), 读数, 生成(json!({"op": "test", "text": 前提题})))
        .unwrap_or_else(|e| panic!("{e}"));
    let v = r.out.value_json();
    // 前提题对 6 项各判一次（源码里没有前提题字面，它来自生成器）
    assert_eq!(
        r.asked.iter().filter(|t| t.as_str() == 前提题).count(),
        6,
        "{:?}",
        r.asked
    );
    assert!(!程序().contains(前提题));
    let rows = v["value"].as_array().unwrap();
    assert_eq!(rows.len(), 6, "每项一行：{v}");
    let ev: Vec<&str> = v["evidence"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|x| x.as_str())
        .collect();
    for i in [0, 1, 2] {
        assert_eq!(rows[i]["fields"]["ontime"], json!(true), "{v}");
        assert_eq!(rows[i]["premise"][0]["exit"], json!("act"), "{v}");
        assert!(rows[i].get("excluded").is_none(), "{v}");
    }
    for i in [3, 4] {
        assert_eq!(rows[i]["excluded"], json!(true), "{v}");
        assert_eq!(rows[i]["fields"], json!({}), "{v}");
        let last = &rows[i]["premise"][0];
        assert_eq!(last["exit"], json!("ignore"), "{v}");
        assert_eq!(last["q"], json!(前提题), "{v}");
        let k = last["source_key"].as_str().unwrap();
        assert!(ev.contains(&k), "淘汰它的判断键在 evidence 里：{k} {v}");
        // 账本里有这次前提判断（B0478 rebase：前提接线挪进 purpose_core 之后核实际落地值），校准标签是前提题的
        let hit: Vec<String> = r
            .ledger
            .entries
            .iter()
            .filter_map(|e| match e {
                jpp::ledger::Entry::Judge { key, calib_ref, .. } if key == k => {
                    Some(serde_json::to_string(calib_ref).unwrap())
                }
                _ => None,
            })
            .collect();
        assert_eq!(hit.len(), 1, "淘汰它的判断在账本里恰一条：{k}");
        assert!(hit[0].contains("purpose-premise"), "{hit:?}");
    }
    // 前提拿不准的存活、进深判，前提题出口随 pending 转交
    assert_eq!(rows[5]["fields"]["ontime"], json!(true), "{v}");
    assert!(
        rows[5]["premise"][0]["exit"]
            .as_str()
            .unwrap()
            .starts_with("unsure"),
        "{v}"
    );
    assert!(
        v["pending"].as_array().unwrap().iter().any(|p| p["via"]
            .as_array()
            .is_some_and(|a| a.iter().any(|x| x == "select#0"))),
        "{v}"
    );
    // 深判的题只对纳入的 4 项发
    assert_eq!(
        r.asked.iter().filter(|t| t.as_str() == 深判题).count(),
        4,
        "{:?}",
        r.asked
    );
    let d = &v["detail"];
    assert_eq!(d["premise"]["passed"], json!([前提题]), "{d}");
    assert_eq!(d["premise"]["excluded"], json!(2), "{d}");
    assert_eq!(d["premise"]["reason"], json!("derived"), "{d}");
    assert_eq!(d["sources"]["presupposition"], json!("system"), "{d}");
    // 抽模块、前提派生、唤出各一次
    assert_eq!(r.gens, 3);
}

/// 元题被第①段拒，没有前提：全部纳入，返回行不带 premise
#[test]
fn 元题当前提被拒_全部纳入() {
    let r = 跑_按提示(
        &程序(),
        读数,
        生成(json!({"op": "test", "text": "这段材料对判断这家供应商能否按期交付有没有帮助？"})),
    )
    .unwrap_or_else(|e| panic!("{e}"));
    let v = r.out.value_json();
    let d = &v["detail"];
    assert_eq!(d["premise"]["reason"], json!("none-passed"), "{d}");
    assert_eq!(d["premise"]["rejected"][0]["stage"], json!("static"), "{d}");
    let rows = v["value"].as_array().unwrap();
    assert_eq!(rows.len(), 6);
    assert!(
        rows.iter()
            .all(|x| x.get("premise").is_none() && x["fields"]["ontime"] == json!(true)),
        "{v}"
    );
}

/// 不是是非题的前提候选在第⓪段拒
#[test]
fn 前提候选不是是非题_形状拒() {
    let r = 跑_按提示(
        &程序(),
        读数,
        生成(json!({"op": "select", "text": "交付周期写在哪一栏？", "over": ["产能", "账期"]})),
    )
    .unwrap_or_else(|e| panic!("{e}"));
    let d = &r.out.value_json()["detail"];
    assert_eq!(d["premise"]["reason"], json!("none-passed"), "{d}");
    assert_eq!(d["premise"]["rejected"][0]["stage"], json!("shape"), "{d}");
    assert_eq!(
        d["premise"]["rejected"][0]["codes"],
        json!(["premise-not-test"]),
        "{d}"
    );
}

/// 臂 3 空跑 3.2：谓词带「基于参照材料」时，生成器把「对照」写进了前提题，而前提题只在条目材料上判。
/// 前提派生的提示写明判前提时只看得到条目自己的材料（只核提示文字，不用正则拦题面）
#[test]
fn 前提派生提示写明只看得到条目材料() {
    let 提示 = std::cell::RefCell::new(Vec::<String>::new());
    let 模块带参照 = json!({"material": "供应商；参照是我们的订单",
        "predicates": [{"text": "基于这份参照材料，这家供应商能按期交付", "cut": "binary", "cut_from": "purpose", "request": "one", "field": "ontime"}],
        "request": null, "presupposition": null, "context": null, "reference": "我们的订单", "budget": null, "unsure": null, "done": ["ontime"]});
    let r = 跑_按提示(&程序(), 读数, |p: &str| {
        提示.borrow_mut().push(p.to_string());
        if p.contains("字面前提题") {
            vec![json!({"op": "test", "text": 前提题})]
        } else if p.starts_with("下面是一段目的") {
            vec![模块带参照.clone()]
        } else {
            vec![json!({"op": "test", "text": 深判题})]
        }
    })
    .unwrap_or_else(|e| panic!("{e}"));
    let _ = r;
    let p = 提示
        .borrow()
        .iter()
        .find(|p| p.contains("字面前提题"))
        .cloned()
        .expect("前提派生那次的提示");
    assert!(p.contains("基于这份参照材料"), "提示里带了谓词：{p}");
    assert!(
        p.contains("只看得到这一个条目自己的材料") && p.contains("看不到参照材料"),
        "{p}"
    );
}
