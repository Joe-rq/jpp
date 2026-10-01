//! Z0511（线 A N1）：从一句目的出第一批题。目的 → 抽模块 → 补模块 → 查题库 → 唤出 → 组装 → 每项同一批题 → 字段。
//! 预注册：地基/过程记录/工程-Z0511-目的出题.md 第四节。闭包端口，不发请求；任务是虚构的「挑供应商」，与线 A 的靶子无关。

mod derive_support;
use derive_support::*;
use jpp::ledger::Entry;
use jpp::value::{Answer, Question, State};
use serde_json::{Value as Json, json};

const 目的甲乙丙: &str =
    "在这些供应商里按低、中、高排出可靠程度，并说出每家最主要的短板是甲、乙、丙里的哪一类。";
const 目的: &str =
    "在这些供应商里找出最可能按期交付我们这份订单的，排好先后，并说出每家最大的风险。";

fn 条目(n: usize) -> String {
    let xs: Vec<String> = (0..n)
        .map(|i| format!("{{on: mat({{name: \"供应商{i}\", note: \"第 {i} 家，产能与账期各不相同\"}}), ref: mat(\"订单：三千件，六周内交付\")}}"))
        .collect();
    format!("[{}]", xs.join(", "))
}

fn 程序(n: usize, purpose: &str) -> String {
    format!(
        "import \"../../lib/derive/purpose.jpp\";\nbudget {{calls: 2000, cost: 0, depth: 8192}};\nlet r = purpose_run(\"{purpose}\", {}, {{}});\n{}",
        条目(n),
        "{value: r.value, pending: map(r.pending, fn(p) { {cause: p.cause, exit: p.exit} }), detail: r.detail}"
    )
}

/// 抽模块的生成：两个谓词（有序 + K 选一），参照与完成条件都由目的说了；前提、语境、预算、拿不准去向没说
fn 模块() -> Json {
    json!({"material": "供应商；参照是我们的订单",
           "predicates": [
             {"text": "这家供应商能按期交付我们这份订单", "cut": "ordered", "scale": ["不能", "难说", "能"], "request": "all", "field": "rank",
              "cut_from": "purpose", "scale_from": "purpose"},
             {"text": "这家供应商交付这份订单最大的风险", "cut": "k_ary", "over": ["产能", "资金", "质量", "没有明显风险"], "request": "one", "field": "risk",
              "cut_from": "purpose", "over_from": "purpose"}],
           "request": "每家一个名次与一个原因", "presupposition": null, "context": null, "reference": "我们的订单",
           "budget": null, "unsure": null, "done": ["rank", "risk"]})
}

fn 生成(p: &str) -> Vec<Json> {
    if p.starts_with("下面是一段目的") {
        vec![模块()]
    } else if p.contains("按期交付我们这份订单」") {
        vec![
            json!({"op": "measure", "text": "这家供应商按期交付这份订单的把握有多大？", "scale": ["不能", "难说", "能"], "evidence": ["ref"]}),
            json!({"op": "test", "text": "这家供应商能按期交付，并且质量合格吗？"}),
            json!({"op": "measure", "text": "这家供应商交付能力如何？"}),
        ]
    } else if p.contains("最大的风险」") {
        vec![
            json!({"op": "select", "text": "这家供应商交付这份订单最大的风险在哪一方面？", "over": ["产能", "资金", "质量", "没有明显风险"], "evidence": ["ref"]}),
        ]
    } else {
        vec![]
    }
}

fn 名次(s: &State) -> usize {
    s.on.first()
        .and_then(|m| m.content.get("name"))
        .and_then(|v| v.as_str())
        .and_then(|t| t.trim_start_matches("供应商").parse::<usize>().ok())
        .unwrap_or(0)
}

/// 查题库一律判「都不是」（最后一个候选）；诊断放行；有序题按条目给不同的档；风险题选第 (i mod 4) 个
fn 读数(t: &str, _q: &Question, s: &State) -> Answer {
    if t.contains("需要分别回答的判断") {
        Answer::Noul(if t.contains("并且") { 0.9 } else { 0.1 })
    } else if t.contains("这段材料里有没有") {
        Answer::Noul(0.9)
    } else if t.contains("已认证的题问的是同一件事") || t.contains("说的是同一件事")
    {
        // 查题库、同事异名（Z0559）一律判「都不是」
        let k = s.over.len();
        let mut v = vec![0.0; k];
        v[k - 1] = 1.0;
        Answer::Choice(v)
    } else if t.contains("把握有多大") || t.contains("交付能力") {
        let i = 名次(s);
        Answer::Score(match i % 3 {
            0 => vec![0.1, 0.2, 0.7],
            1 => vec![0.6, 0.3, 0.1],
            _ => vec![0.2, 0.6, 0.2],
        })
    } else if t.contains("最大的风险") {
        let mut v = vec![0.1; 4];
        v[名次(s) % 4] = 0.7;
        Answer::Choice(v)
    } else {
        Answer::Noul(0.5)
    }
}

fn 跑目的(
    n: usize,
    purpose: &str,
    answer: fn(&str, &Question, &State) -> Answer,
) -> (Json, 跑出) {
    let r = 跑_按提示(&程序(n, purpose), answer, 生成).unwrap();
    (r.out.value_json(), r)
}

#[test]
fn 生成器调用数与材料项个数无关() {
    let (_, a) = 跑目的(3, 目的, 读数);
    let (_, b) = 跑目的(30, 目的, 读数);
    let (vc, c) = 跑目的(400, 目的, 读数);
    // 1 次抽模块 + 1 次前提派生（B0470）+ 2 个谓词各唤出 1 次
    assert_eq!(a.gens, 4);
    assert_eq!(b.gens, 4);
    assert_eq!(c.gens, 4);
    assert_eq!(vc["value"].as_array().unwrap().len(), 400);
}

#[test]
fn 每项两个字段_名次与原因() {
    let (v, _) = 跑目的(6, 目的, 读数);
    let vs = v["value"].as_array().unwrap();
    assert_eq!(vs.len(), 6, "{v}");
    for x in vs {
        let f = &x["fields"];
        assert!(f["rank"].as_i64().is_some_and(|r| r >= 1), "{v}");
        assert!(
            ["产能", "资金", "质量", "没有明显风险"].contains(&f["risk"].as_str().unwrap_or("")),
            "{v}"
        );
    }
    // 名次：第 0、3 家（把握高）排第 1 档，并列同档
    assert_eq!(vs[0]["fields"]["rank"], vs[3]["fields"]["rank"], "{v}");
    assert_eq!(vs[0]["fields"]["rank"], json!(1), "{v}");
    assert_eq!(v["pending"], json!([]), "{v}");
    let d = &v["detail"];
    assert_eq!(d["fields"], json!(["rank", "risk"]), "{d}");
    assert_eq!(d["fields_missing"], json!([]), "{d}");
    // 接 Z0398：唤出路的题式带 lacks（守则 R.lacks.elicit）
    assert_eq!(
        d["questions"][0]["lacks"],
        json!(["材料", "语境", "参照"]),
        "{d}"
    );
    assert_eq!(
        d["questions"][1]["lacks"],
        json!(["材料", "语境", "参照"]),
        "{d}"
    );
    // 一题两问的候选被闸门拒；有序谓词取的是 measure 那道
    assert_eq!(d["questions"][0]["op"], "measure", "{d}");
    assert_eq!(d["questions"][0]["from"], "elicit", "{d}");
    // 模块来源：目的说了的记 purpose，前提补不上记 missing（B0470），参照由系统指向 ref 之外目的也说了
    assert_eq!(d["sources"]["predicate"], "purpose", "{d}");
    assert_eq!(d["sources"]["cut"], "purpose", "{d}");
    assert_eq!(d["sources"]["presupposition"], "system", "{d}");
    assert_eq!(d["sources"]["unsure"], "system", "{d}");
    assert_eq!(d["sources"]["done"], "purpose", "{d}");
}

#[test]
fn 账本依次见抽模块_查题库_唤出_组装() {
    let (v, r) = 跑目的(3, 目的, 读数);
    // 顺序：第一条生成是抽模块；之后有查题库的判断；再是唤出的生成；闸门的 diagnose 变换；最后每项的判断
    let mut seq: Vec<&str> = vec![];
    for e in &r.ledger.entries {
        match e {
            Entry::Effect { ekey, .. } => {
                let k = serde_json::to_value(ekey).unwrap();
                let kind = k["kind"].as_str().unwrap_or("").to_string();
                let parts = k["parts"].to_string();
                if kind == "gen" && parts.contains("下面是一段目的") {
                    seq.push("modules")
                } else if kind == "gen" {
                    seq.push("elicit")
                } else {
                    seq.push("transform")
                }
            }
            Entry::Judge { calib_ref, .. } => {
                let c = serde_json::to_value(calib_ref).unwrap().to_string();
                if c.contains("bank-match") {
                    seq.push("bank")
                } else {
                    seq.push("judge")
                }
            }
            _ => {}
        }
    }
    let first = |x: &str| {
        seq.iter()
            .position(|s| *s == x)
            .unwrap_or_else(|| panic!("缺 {x}：{seq:?} {v}"))
    };
    assert!(first("modules") < first("bank"), "{seq:?}");
    assert!(first("bank") < first("elicit"), "{seq:?}");
    assert!(first("elicit") < first("transform"), "{seq:?}");
    let bank = v["detail"]["bank"].as_array().unwrap();
    assert!(
        bank.iter()
            .all(|b| b["how"] == "none" || b["how"] == "no-same-kind"),
        "{v}"
    );
}

/// 查题库命中零槽题式：直接用它（有线），不唤出
fn 读数_命中(t: &str, q: &Question, s: &State) -> Answer {
    if t.contains("已认证的题问的是同一件事") {
        let k = s.over.len();
        let i = s
            .over
            .iter()
            .position(|m| {
                m.content
                    .as_str()
                    .is_some_and(|x| x.contains("很难或无法完全撤销"))
            })
            .unwrap_or(k - 1);
        let mut v = vec![0.0; k];
        v[i] = 1.0;
        Answer::Choice(v)
    } else {
        读数(t, q, s)
    }
}

#[test]
fn 命中零槽题式_直接用_少一次唤出() {
    let src = 程序(3, "这些操作里哪些一旦做了就撤不回来，逐个说是不是。");
    let r = 跑_按提示(&src, 读数_命中, |p| {
        if p.starts_with("下面是一段目的") {
            vec![json!({"material": "操作", "predicates": [{"text": "这项操作执行以后难以撤销", "cut": "binary", "request": "one", "field": "irreversible"}],
                        "request": null, "presupposition": null, "context": null, "reference": null, "budget": null, "unsure": null, "done": ["irreversible"]})]
        } else {
            vec![json!({"op": "test", "text": "这项操作能撤回吗？"})]
        }
    })
    .unwrap();
    let v = r.out.value_json();
    assert_eq!(r.gens, 2, "{v}"); // 抽模块 + 前提派生（B0470），命中题库不唤出
    let d = &v["detail"];
    assert_eq!(d["bank"][0]["how"], "bank", "{d}");
    assert_eq!(d["bank"][0]["hit"], "hard_to_undo", "{d}");
    assert_eq!(d["questions"][0]["from"], "bank", "{d}");
    assert!(
        r.asked.iter().any(|x| x.contains("很难或无法完全撤销")),
        "{:?}",
        r.asked
    );
}

/// 命中带槽题式：填参数要 B0479，今天做不到，退回唤出并照实记
#[test]
fn 命中带槽题式_记缺参数_退回唤出() {
    let src = 程序(2, "这些 JSON 里哪些有我们要的字段，逐个说。");
    let r = 跑_按提示(
        &src,
        |t, q, s| {
            if t.contains("已认证的题问的是同一件事") {
                let k = s.over.len();
                let i = s
                    .over
                    .iter()
                    .position(|m| m.content.as_str().is_some_and(|x| x.contains("JSON 是否有字段")))
                    .unwrap_or(k - 1);
                let mut v = vec![0.0; k];
                v[i] = 1.0;
                Answer::Choice(v)
            } else {
                读数(t, q, s)
            }
        },
        |p| {
            if p.starts_with("下面是一段目的") {
                vec![json!({"material": "JSON", "predicates": [{"text": "这个 JSON 有要的字段", "cut": "binary", "field": "has_field"}], "done": ["has_field"]})]
            } else {
                vec![json!({"op": "test", "text": "这个 JSON 里有名为 id 的字段吗？"})]
            }
        },
    )
    .unwrap();
    let v = r.out.value_json();
    let d = &v["detail"];
    assert_eq!(d["bank"][0]["how"], "needs-params:B0479", "{d}");
    assert_eq!(d["questions"][0]["from"], "elicit", "{d}");
    assert_eq!(r.gens, 3, "{v}"); // 抽模块 + 前提派生（B0470）+ 唤出
}

/// 模块解析不了：返回空题集并说明，不报错
#[test]
fn 模块解析不了_空题集并说明() {
    let src = 程序(2, 目的);
    let r = 跑_按提示(&src, 读数, |_p| vec![json!("不是 JSON")]).unwrap();
    let v = r.out.value_json();
    assert_eq!(v["value"], json!([]), "{v}");
    assert_eq!(v["detail"]["error"], "modules:parse", "{v}");
}

// ———— Z0511 复核返修（附录第 1–4 条）————

fn 程序_关键(n: usize, purpose: &str, proj: &str) -> String {
    format!(
        "import \"../../lib/derive/purpose.jpp\";\nbudget {{calls: 2000, cost: 0, depth: 8192}};\nlet r = purpose_run(\"{purpose}\", {}, {{}});\n{proj}",
        条目(n)
    )
}

/// 反例 D：目的列了候选甲乙丙、说了排先后；唤出给别的候选与是非题，被第⓪段拒；给对的才收
#[test]
fn 目的明说的切法与候选约束唤出() {
    let m = json!({"material": "供应商", "predicates": [
        {"text": "这家供应商最主要的短板", "cut": "k_ary", "over": ["甲", "乙", "丙"], "cut_from": "purpose", "over_from": "purpose", "field": "weak"},
        {"text": "这家供应商有多可靠", "cut": "ordered", "scale": ["低", "中", "高"], "cut_from": "purpose", "scale_from": "purpose", "field": "rank"}],
        "done": ["weak", "rank"]});
    // Z0558 起标 purpose 的候选与档位要在目的原文里逐字出现，这里的目的把「甲、乙、丙」与「低、中、高」写进去
    let src = 程序_关键(
        3,
        目的甲乙丙,
        "{value: r.value, pending: map(r.pending, fn(p) { {cause: p.cause, exit: p.exit} }), detail: r.detail}",
    );
    let r = 跑_按提示(&src, |t, _q, s| {
        if t.contains("需要分别回答的判断") { Answer::Noul(0.1) }
        else if t.contains("这段材料里有没有") { Answer::Noul(0.9) }
        else if t.contains("已认证的题问的是同一件事") { let k = s.over.len(); let mut v = vec![0.0; k]; v[k - 1] = 1.0; Answer::Choice(v) }
        else if t.contains("短板") { let mut v = vec![0.1; s.over.len()]; v[0] = 0.8; Answer::Choice(v) }
        else { Answer::Noul(0.7) }
    }, move |p| {
        if p.starts_with("下面是一段目的") { vec![m.clone()] }
        else if p.contains("短板」") {
            vec![json!({"op": "select", "text": "这家供应商最主要的短板在哪？", "over": ["X", "Y"]}),
                 json!({"op": "test", "text": "这家供应商有短板吗？"}),
                 json!({"op": "select", "text": "这家供应商最主要的短板是哪一类？", "over": ["丙", "甲", "乙"]})]
        } else { vec![json!({"op": "test", "text": "这家供应商可靠吗？"})] }
    }).unwrap();
    let v = r.out.value_json();
    let d = &v["detail"];
    let codes: Vec<String> = d["elicit"]["rejected"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|x| {
            x["codes"]
                .as_array()
                .unwrap()
                .iter()
                .map(|c| c.as_str().unwrap().to_string())
        })
        .collect();
    assert!(codes.contains(&"over-mismatch".to_string()), "{d}");
    assert!(codes.contains(&"cut-mismatch".to_string()), "{d}");
    for x in v["value"].as_array().unwrap() {
        assert!(
            ["甲", "乙", "丙"].contains(&x["fields"]["weak"].as_str().unwrap_or("")),
            "{v}"
        );
    }
    assert_eq!(d["fields_missing"], json!(["rank"]), "{d}");
}

/// 反例 D 另一面：候选标 system（目的没列）时，唤出给的候选照收
#[test]
fn 候选标系统补时照收唤出的候选() {
    let m = json!({"material": "供应商", "predicates": [
        {"text": "这家供应商最主要的短板", "cut": "k_ary", "over": ["甲", "乙", "丙"], "cut_from": "purpose", "over_from": "system", "field": "weak"}],
        "done": ["weak"]});
    let src = 程序_关键(
        2,
        目的,
        "{value: r.value, pending: map(r.pending, fn(p) { {cause: p.cause, exit: p.exit} }), detail: r.detail}",
    );
    let r = 跑_按提示(&src, |t, _q, s| {
        if t.contains("需要分别回答的判断") { Answer::Noul(0.1) }
        else if t.contains("这段材料里有没有") { Answer::Noul(0.9) }
        else if t.contains("已认证的题问的是同一件事") { let k = s.over.len(); let mut v = vec![0.0; k]; v[k - 1] = 1.0; Answer::Choice(v) }
        else { let mut v = vec![0.1; s.over.len()]; v[0] = 0.8; Answer::Choice(v) }
    }, move |p| {
        if p.starts_with("下面是一段目的") { vec![m.clone()] }
        else { vec![json!({"op": "select", "text": "这家供应商最主要的短板在哪？", "over": ["X", "Y"]})] }
    }).unwrap();
    let v = r.out.value_json();
    assert_eq!(v["value"][0]["fields"]["weak"], "X", "{v}");
}

/// 反例 E：生成器输出 untrusted 时，题库命中路与唤出路的首批出口都不可信
#[test]
fn 题库命中路随抽模块带污() {
    // N-T6 起字段未决先走默认链：读数 0.5 判不出时，伴随题关着走路 C 转交、开着按中性读数「两可」放弃，出口不一定在 pending；
    // 首批出口的可信标记改从每项的 trust 读（落字段时取那次出口的 taint，未决也记）
    let proj = "{taints: map(r.value, fn(v) { v.trust.irreversible }), from: map(r.detail.questions, fn(q) { q.from })}";
    let src = 程序_关键(3, "这些操作里哪些一旦做了就撤不回来，逐个说是不是。", proj);
    let r = 跑_按提示_带污(&src, |t, _q, s| {
        if t.contains("已认证的题问的是同一件事") {
            let k = s.over.len();
            let i = s.over.iter().position(|m| m.content.as_str().is_some_and(|x| x.contains("很难或无法完全撤销"))).unwrap_or(k - 1);
            let mut v = vec![0.0; k]; v[i] = 1.0; Answer::Choice(v)
        } else { Answer::Noul(0.5) }
    }, |p| {
        if p.starts_with("下面是一段目的") {
            vec![json!({"material": "操作", "predicates": [{"text": "这项操作执行以后难以撤销", "cut": "binary", "cut_from": "purpose", "field": "irreversible"}], "done": ["irreversible"]})]
        } else { vec![] }
    }, jpp::value::Taint::Untrusted).unwrap();
    let v = r.out.value_json();
    assert_eq!(v["from"], json!(["bank"]), "{v}");
    assert_eq!(
        v["taints"],
        json!(["untrusted", "untrusted", "untrusted"]),
        "{v}"
    );
}

/// 反例 F：候选在第 0 项上「在材料里」判没有、在第 1、2 项上判有：收下，三项都有字段；三项都没有才拒
#[test]
fn 样本闸门_在材料里全部没有才拒() {
    let m = json!({"material": "供应商", "predicates": [{"text": "这家供应商写明交付周期", "cut": "binary", "cut_from": "purpose", "field": "has_lead"}], "done": ["has_lead"]});
    let 跑一次 = |全无: bool| {
        let m = m.clone();
        let src = 程序_关键(
            3,
            目的,
            "{value: r.value, pending: map(r.pending, fn(p) { {cause: p.cause, exit: p.exit} }), detail: r.detail}",
        );
        let r = 跑_按提示(
            &src,
            move |t, _q, s| {
                if t.contains("需要分别回答的判断") {
                    Answer::Noul(0.1)
                } else if t.contains("这段材料里有没有") {
                    let 第0 =
                        s.on.first()
                            .is_some_and(|x| x.content.to_string().contains("供应商0"));
                    Answer::Noul(if 全无 || 第0 { 0.1 } else { 0.9 })
                } else if t.contains("已认证的题问的是同一件事") {
                    let k = s.over.len();
                    let mut v = vec![0.0; k];
                    v[k - 1] = 1.0;
                    Answer::Choice(v)
                } else {
                    Answer::Noul(0.8)
                }
            },
            move |p| {
                if p.starts_with("下面是一段目的") {
                    vec![m.clone()]
                } else {
                    vec![json!({"op": "test", "text": "材料里写明交付周期了吗？"})]
                }
            },
        )
        .unwrap();
        r.out.value_json()
    };
    let v = 跑一次(false);
    assert_eq!(v["detail"]["fields_missing"], json!([]), "{v}");
    assert_eq!(v["detail"]["gate_items"], json!([0, 1, 2]), "{v}");
    for x in v["value"].as_array().unwrap() {
        assert_eq!(x["fields"]["has_lead"], json!(true), "{v}");
    }
    let w = 跑一次(true);
    assert_eq!(w["detail"]["fields_missing"], json!(["has_lead"]), "{w}");
}

/// 反例 G：两个谓词唤出同一道题：两个字段都落值，这道题每项只判一次
#[test]
fn 两谓词同题_两个字段都落值() {
    let m = json!({"material": "供应商", "predicates": [
        {"text": "这家供应商写明付款方式", "cut": "binary", "cut_from": "purpose", "field": "a"},
        {"text": "这家供应商的付款方式写清楚了", "cut": "binary", "cut_from": "purpose", "field": "b"}], "done": ["a", "b"]});
    let src = 程序_关键(
        3,
        目的,
        "{value: r.value, pending: map(r.pending, fn(p) { {cause: p.cause, exit: p.exit} }), detail: r.detail}",
    );
    let r = 跑_按提示(
        &src,
        |t, _q, s| {
            if t.contains("需要分别回答的判断") {
                Answer::Noul(0.1)
            } else if t.contains("这段材料里有没有") {
                Answer::Noul(0.9)
            } else if t.contains("已认证的题问的是同一件事") {
                let k = s.over.len();
                let mut v = vec![0.0; k];
                v[k - 1] = 1.0;
                Answer::Choice(v)
            } else {
                Answer::Noul(0.8)
            }
        },
        move |p| {
            if p.starts_with("下面是一段目的") {
                vec![m.clone()]
            } else {
                vec![json!({"op": "test", "text": "材料里写明付款方式了吗？"})]
            }
        },
    )
    .unwrap();
    let v = r.out.value_json();
    assert_eq!(v["detail"]["fields_missing"], json!([]), "{v}");
    for x in v["value"].as_array().unwrap() {
        assert_eq!(x["fields"]["a"], json!(true), "{v}");
        assert_eq!(x["fields"]["b"], json!(true), "{v}");
    }
    assert_eq!(
        r.asked
            .iter()
            .filter(|x| x.as_str() == "材料里写明付款方式了吗？")
            .count(),
        3,
        "{:?}",
        r.asked
    );
}

/// Z0535：完成条件列了、谓词没覆盖的字段，补一道题（查题库 → 唤出），仍与材料项个数无关
#[test]
fn 完成条件缺字段_补题() {
    let gen3 = |p: &str| -> Vec<Json> {
        if p.starts_with("下面是一段目的") {
            let mut m = 模块();
            m["done"] = json!(["rank", "risk", "note"]);
            vec![m]
        } else if p.contains("「note」") {
            vec![json!({"op": "test", "text": "这家供应商有没有写明付款方式？"})]
        } else {
            生成(p)
        }
    };
    let a = 跑_按提示(&程序(3, 目的), 读数, gen3).unwrap();
    let b = 跑_按提示(&程序(30, 目的), 读数, gen3).unwrap();
    let v = a.out.value_json();
    let d = &v["detail"];
    assert_eq!(a.gens, 5, "{d}"); // 含前提派生一次（B0470）
    assert_eq!(b.gens, 5);
    assert_eq!(d["fields"], json!(["rank", "risk", "note"]), "{d}");
    assert_eq!(d["fields_missing"], json!([]), "{d}");
    for x in v["value"].as_array().unwrap() {
        assert!(x["fields"].get("note").is_some(), "{v}");
    }
}

// ———— Z0558 ————

fn 读数_常(t: &str, _q: &Question, s: &State) -> Answer {
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
    } else if !s.over.is_empty() {
        let mut v = vec![0.1; s.over.len()];
        v[0] = 0.8;
        Answer::Choice(v)
    } else {
        Answer::Noul(0.8)
    }
}

/// 标 purpose 的候选由代码核：目的里有 → 保持并约束唤出；目的里缺一项 → 改记 system、给 W-purpose-label、唤出的候选照收；
/// 缺切法标记 → unlabeled
#[test]
fn 来源标记由代码核() {
    let 跑一次 = |purpose: &'static str, cut_label: bool| {
        let mut p = json!({"text": "这家供应商最主要的短板", "cut": "k_ary", "over": ["甲", "乙", "丙"], "over_from": "purpose", "field": "weak"});
        if cut_label {
            p["cut_from"] = json!("purpose");
        }
        let m = json!({"material": "供应商", "predicates": [p], "done": ["weak"]});
        let src = 程序_关键(
            2,
            purpose,
            "{value: r.value, pending: map(r.pending, fn(p) { {cause: p.cause, exit: p.exit} }), detail: r.detail}",
        );
        let r = 跑_按提示(&src, 读数_常, move |p| {
            if p.starts_with("下面是一段目的") { vec![m.clone()] }
            else { vec![json!({"op": "select", "text": "这家供应商最主要的短板在哪？", "over": ["X", "Y"]})] }
        }).unwrap();
        r.out.value_json()
    };
    let v = 跑一次("说出每家供应商最主要的短板是甲、乙、丙里的哪一类。", true);
    assert_eq!(v["detail"]["preds"][0]["over_from"], "purpose", "{v}");
    assert_eq!(v["detail"]["warnings"], json!([]), "{v}");
    assert_eq!(v["detail"]["fields_missing"], json!(["weak"]), "{v}"); // X、Y 被 over-mismatch 拒
    let w = 跑一次("说出每家供应商最主要的短板是甲还是乙。", true);
    assert_eq!(w["detail"]["preds"][0]["over_from"], "system", "{w}");
    assert_eq!(w["detail"]["warnings"][0]["code"], "W-purpose-label", "{w}");
    // 「甲还是乙」里的「甲」挨着连词「还是」，算出现（Z0634）；只列出「丙」
    assert_eq!(
        w["detail"]["warnings"][0]["not_in_purpose"],
        json!(["丙"]),
        "{w}"
    );
    assert_eq!(w["value"][0]["fields"]["weak"], "X", "{w}");
    let u = 跑一次("说出每家供应商最主要的短板是甲、乙、丙里的哪一类。", false);
    assert_eq!(u["detail"]["preds"][0]["cut_from"], "unlabeled", "{u}");
    assert_eq!(u["detail"]["sources"]["cut"], "unlabeled", "{u}");
}

/// 样本没覆盖全部项时，样本内都判「材料里没有」也不拒；覆盖全部且都没有才拒
#[test]
fn 样本闸门_样本未覆盖全部时不拒() {
    let m = json!({"material": "供应商", "predicates": [{"text": "这家供应商写明交付周期", "cut": "binary", "cut_from": "purpose", "field": "has_lead"}], "done": ["has_lead"]});
    let 跑一次 = |sample: usize, 全无: bool| {
        let m = m.clone();
        let src = format!(
            "import \"../../lib/derive/purpose.jpp\";\nbudget {{calls: 2000, cost: 0, depth: 8192}};\nlet r = purpose_run(\"{目的}\", {}, {{sample: {sample}}});\n{{value: r.value, pending: map(r.pending, fn(p) {{ {{cause: p.cause, exit: p.exit}} }}), detail: r.detail}}",
            条目(5)
        );
        let r = 跑_按提示(
            &src,
            move |t, q, s| {
                if t.contains("这段材料里有没有") {
                    let i =
                        s.on.first()
                            .map(|x| x.content.to_string())
                            .unwrap_or_default();
                    let 前三 = ["供应商0", "供应商1", "供应商2"]
                        .iter()
                        .any(|k| i.contains(k));
                    Answer::Noul(if 全无 || 前三 { 0.1 } else { 0.9 })
                } else {
                    读数_常(t, q, s)
                }
            },
            move |p| {
                if p.starts_with("下面是一段目的") {
                    vec![m.clone()]
                } else {
                    vec![json!({"op": "test", "text": "材料里写明交付周期了吗？"})]
                }
            },
        )
        .unwrap();
        r.out.value_json()
    };
    let v = 跑一次(3, false);
    assert_eq!(v["detail"]["fields_missing"], json!([]), "{v}");
    assert_eq!(v["value"].as_array().unwrap().len(), 5);
    assert_eq!(
        v["detail"]["gate_notes"].as_array().unwrap().len(),
        1,
        "{v}"
    );
    let w = 跑一次(5, true);
    assert_eq!(w["detail"]["fields_missing"], json!(["has_lead"]), "{w}");
}

/// 字段带可信标记：生成器 untrusted 时两条路每个字段都是 untrusted；夹具不声明时 trusted
#[test]
fn 字段带可信标记() {
    let 跑一次 = |污: bool| {
        let src = 程序_关键(
            3,
            目的,
            "{value: map(r.value, fn(v) { v.trust }), pending: map(r.pending, fn(p) { {cause: p.cause, exit: p.exit} })}",
        );
        let 生 = |p: &str| -> Vec<Json> {
            if p.starts_with("下面是一段目的") {
                vec![模块()]
            } else {
                生成(p)
            }
        };
        let r = if 污 {
            跑_按提示_带污(&src, 读数, 生, jpp::value::Taint::Untrusted).unwrap()
        } else {
            跑_按提示(&src, 读数, 生).unwrap()
        };
        r.out.value_json()
    };
    let v = 跑一次(true);
    for t in v["value"].as_array().unwrap() {
        assert_eq!(t, &json!({"rank": "untrusted", "risk": "untrusted"}), "{v}");
    }
    let w = 跑一次(false);
    for t in w["value"].as_array().unwrap() {
        assert_eq!(t, &json!({"rank": "trusted", "risk": "trusted"}), "{w}");
    }
}

// ———— Z0559 ————

/// 补出的谓词切法未知：唤出提示写「切法：未定」，查题库按三种题型都查；唤出给 K 选一就按候选落字段
#[test]
fn 补谓词切法未知_不填binary() {
    let 提示 = std::cell::RefCell::new(Vec::<String>::new());
    let 题库候选 = std::cell::RefCell::new(Vec::<String>::new());
    let gen3 = |p: &str| -> Vec<Json> {
        提示.borrow_mut().push(p.to_string());
        if p.starts_with("下面是一段目的") {
            let mut m = 模块();
            m["done"] = json!(["rank", "risk", "note"]);
            vec![m]
        } else if p.contains("「note」") {
            vec![
                json!({"op": "select", "text": "这家供应商的付款方式是哪一种？", "over": ["预付", "月结", "货到付款"]}),
            ]
        } else {
            生成(p)
        }
    };
    let r = 跑_按提示(
        &程序(3, 目的),
        |t, q, s| {
            if t.contains("已认证的题问的是同一件事")
                && s.on
                    .first()
                    .is_some_and(|m| m.content.to_string().contains("note"))
            {
                题库候选
                    .borrow_mut()
                    .extend(s.over.iter().map(|m| m.content.to_string()));
            }
            if t.contains("付款方式是哪一种") {
                let mut v = vec![0.1; s.over.len()];
                v[1] = 0.8;
                return Answer::Choice(v);
            }
            读数(t, q, s)
        },
        gen3,
    )
    .unwrap();
    let v = r.out.value_json();
    for x in v["value"].as_array().unwrap() {
        assert_eq!(x["fields"]["note"], "月结", "{v}");
    }
    // 唤出那次的提示（前提派生的提示也列了谓词、含「note」，B0470；它以「下面是一段目的」开头）
    let p = 提示
        .borrow()
        .iter()
        .find(|p| p.contains("「note」") && !p.starts_with("下面是一段目的"))
        .cloned()
        .unwrap();
    assert!(p.contains("切法：未定"), "{p}");
    let c = 题库候选.borrow().join("|");
    assert!(
        c.contains("很难或无法完全撤销") && c.contains("下列哪一个"),
        "三种题型都查：{c}"
    );
}

/// 同事异名：判断器判 biggest_risk 与 risk 是同一件事 → 不补、同值；判都不是 → 照旧补一道
#[test]
fn 同事异名_先判再补() {
    let 跑一次 = |同名: bool| {
        let gen3 = |p: &str| -> Vec<Json> {
            if p.starts_with("下面是一段目的") {
                let mut m = 模块();
                m["done"] = json!(["rank", "risk", "biggest_risk"]);
                vec![m]
            } else if p.contains("「biggest_risk」") {
                vec![json!({"op": "test", "text": "这家供应商有明显的风险吗？"})]
            } else {
                生成(p)
            }
        };
        let r = 跑_按提示(
            &程序(3, 目的),
            move |t, q, s| {
                if t.contains("说的是同一件事") && !t.contains("已认证") {
                    let k = s.over.len();
                    let mut v = vec![0.0; k];
                    v[if 同名 { 1 } else { k - 1 }] = 1.0;
                    return Answer::Choice(v);
                }
                读数(t, q, s)
            },
            gen3,
        )
        .unwrap();
        (r.out.value_json(), r.gens)
    };
    let (v, g) = 跑一次(true);
    assert_eq!(g, 4, "{v}"); // 含前提派生一次（B0470）
    assert_eq!(
        v["detail"]["aliases"],
        json!([{"field": "biggest_risk", "to": "risk"}]),
        "{v}"
    );
    for x in v["value"].as_array().unwrap() {
        assert_eq!(x["fields"]["biggest_risk"], x["fields"]["risk"], "{v}");
        assert_eq!(x["trust"]["biggest_risk"], x["trust"]["risk"], "{v}");
    }
    assert_eq!(v["detail"]["fields_missing"], json!([]), "{v}");
    let (w, g2) = 跑一次(false);
    assert_eq!(g2, 5, "{w}");
    assert_eq!(w["detail"]["aliases"], json!([]), "{w}");
}

/// Z0558 复核：预算停发让一部分项的读数缺席——名次探测不 drop 缺席类未决，名次记 unit、随 pending 转交，程序跑完
#[test]
fn 名次探测遇缺席读数_转交不停() {
    let src = format!(
        "import \"../../lib/derive/purpose.jpp\";\nbudget {{calls: 16, cost: 0, depth: 8192}};\nlet r = purpose_run(\"{目的}\", {}, {{}});\n{{value: map(r.value, fn(v) {{ v.fields }}), pending: map(r.pending, fn(p) {{ {{cause: p.cause, via: p.via, exit: p.exit}} }})}}",
        条目(6)
    );
    let r = 跑_按提示(&src, 读数, 生成).unwrap_or_else(|e| panic!("{e}"));
    let v = r.out.value_json();
    let ranks: Vec<&Json> = v["value"]
        .as_array()
        .unwrap()
        .iter()
        .map(|x| &x["rank"])
        .collect();
    let 缺 = ranks.iter().filter(|x| x.is_null()).count();
    assert!(缺 > 0 && 缺 < 6, "应有一部分项读数缺席：{v}");
    // 只数名次字段转交的未决（via purpose:rank），条数等于名次为 unit 的项数
    let 名次转交 = v["pending"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|p| {
            p["via"]
                .as_array()
                .is_some_and(|a| a.iter().any(|x| x == "purpose:rank"))
        })
        .count();
    assert_eq!(名次转交, 缺, "缺席的名次要随 pending 转交：{v}");
    // 读到的项名次是 ≥ 1 的整数
    for x in ranks.iter().filter(|x| !x.is_null()) {
        assert!(x.as_i64().is_some_and(|n| n >= 1), "{v}");
    }
}

// ———— Z0596 ————

fn 跑标记(purpose: &'static str, over: Json, from: &'static str, cut: &'static str) -> Json {
    let key = if cut == "k_ary" { "over" } else { "scale" };
    let mut p =
        json!({"text": "这家供应商的某一项", "cut": cut, "cut_from": "purpose", "field": "x"});
    p[key] = over;
    p[format!("{key}_from")] = json!(from);
    let m = json!({"material": "供应商", "predicates": [p], "done": ["x"]});
    let src = 程序_关键(
        2,
        purpose,
        "{detail: r.detail, pending: map(r.pending, fn(p) { {cause: p.cause, exit: p.exit} })}",
    );
    let r = 跑_按提示(&src, 读数_常, move |p| {
        if p.starts_with("下面是一段目的") {
            vec![m.clone()]
        } else {
            vec![]
        }
    })
    .unwrap();
    r.out.value_json()
}

/// 归一：全角「ＡＢＣ」与候选 abc 按半角小写比较；空候选拿掉并记一条
#[test]
fn 来源核对_归一与空候选() {
    let v = 跑标记(
        "按ＡＢＣ三类说出每家的短板。",
        json!(["abc", "其他"]),
        "purpose",
        "k_ary",
    );
    // 「其他」不在原文里 → 仍会降级；只看 abc 被认出（not_in_purpose 里没有 abc）
    let miss = &v["detail"]["warnings"][0]["not_in_purpose"];
    assert!(!miss.as_array().unwrap().iter().any(|x| x == "abc"), "{v}");
    let w = 跑标记(
        "按ＡＢＣ和其他两类说出每家的短板。",
        json!(["abc", "其他"]),
        "purpose",
        "k_ary",
    );
    assert_eq!(w["detail"]["preds"][0]["over_from"], "purpose", "{w}");
    let e = 跑标记("说出每家的风险。", json!(["", "风险"]), "purpose", "k_ary");
    assert_eq!(
        e["detail"]["preds"][0]["over_from"], "none",
        "空候选拿掉后不足两项当没给：{e}"
    );
    assert!(
        e["detail"]["warnings"]
            .as_array()
            .unwrap()
            .iter()
            .any(|x| x["code"] == "W-purpose-empty-option"),
        "{e}"
    );
}

/// 单字按词界：「低、中、高」算出现；「中高层」里的「中」不挨同表另一项，不算
#[test]
fn 来源核对_单字按词界() {
    let v = 跑标记(
        "按低、中、高排出每家的可靠程度。",
        json!(["低", "中", "高"]),
        "purpose",
        "ordered",
    );
    assert_eq!(v["detail"]["preds"][0]["scale_from"], "purpose", "{v}");
    let w = 跑标记(
        "中高层员工的满意度，按低到高排。",
        json!(["低", "中", "高"]),
        "purpose",
        "ordered",
    );
    // 「低到高」两项隔着连词「到」，算出现；「中」只在「中高层」里，文本开头不算界（Z0634 复核收紧）
    assert_eq!(w["detail"]["preds"][0]["scale_from"], "system", "{w}");
    assert_eq!(
        w["detail"]["warnings"][0]["not_in_purpose"],
        json!(["中"]),
        "{w}"
    );
    // 冒号后同样不算
    let x = 跑标记(
        "目的：中高层员工的满意度，按低到高排。",
        json!(["低", "中", "高"]),
        "purpose",
        "ordered",
    );
    assert_eq!(
        x["detail"]["warnings"][0]["not_in_purpose"],
        json!(["中"]),
        "{x}"
    );
}

/// gate_notes 写明候选与样本结论
#[test]
fn 闸门备注写明候选() {
    let m = json!({"material": "供应商", "predicates": [{"text": "这家供应商写明交付周期", "cut": "binary", "cut_from": "purpose", "field": "has_lead"}], "done": ["has_lead"]});
    let src = format!(
        "import \"../../lib/derive/purpose.jpp\";\nbudget {{calls: 2000, cost: 0, depth: 8192}};\nlet r = purpose_run(\"{目的}\", {}, {{sample: 3}});\n{{detail: r.detail, pending: map(r.pending, fn(p) {{ {{cause: p.cause, exit: p.exit}} }})}}",
        条目(5)
    );
    let r = 跑_按提示(
        &src,
        |t, q, s| {
            if t.contains("这段材料里有没有") {
                Answer::Noul(0.1)
            } else {
                读数_常(t, q, s)
            }
        },
        move |p| {
            if p.starts_with("下面是一段目的") {
                vec![m.clone()]
            } else {
                vec![json!({"op": "test", "text": "材料里写明交付周期了吗？"})]
            }
        },
    )
    .unwrap();
    let v = r.out.value_json();
    assert_eq!(
        v["detail"]["gate_notes"],
        json!([{"field": "has_lead", "q": "材料里写明交付周期了吗？", "samples": 3, "absent": 3}]),
        "{v}"
    );
}

/// Z0596 复核：英文单字母候选按两侧词界（前后都不是 ASCII 字母或数字）
#[test]
fn 来源核对_英文单字母按词界() {
    let 在 = |purpose: &'static str, over: Json| {
        let v = 跑标记(purpose, over, "purpose", "k_ary");
        v["detail"]["preds"][0]["over_from"] == "purpose"
    };
    assert!(在("Rate each vendor as A, B or C.", json!(["a", "c"])));
    assert!(在("Choose x or y", json!(["x", "y"])));
    assert!(在("分成 A 类和 B 类", json!(["a", "b"])));
    assert!(在("分成A类和B类", json!(["a", "b"])), "汉字算界");
    // 反面：被以该字母开头、结尾或含它的词蒙过的都不算，逐个候选断言（Z0634）
    let 缺 = |purpose: &'static str, over: Json| {
        let v = 跑标记(purpose, over, "purpose", "k_ary");
        v["detail"]["warnings"][0]["not_in_purpose"].clone()
    };
    assert_eq!(
        缺("Rank vendors by yield.", json!(["y", "v"])),
        json!(["y", "v"])
    );
    assert_eq!(
        缺("Classify as good or bad.", json!(["a", "o"])),
        json!(["a", "o"])
    );
}

/// Z0634：中文单字只认与同表另一项相邻的出现，两项之间只隔空白、列举分隔符（、,;/）或连词（或、和、与、及、到、至、还是），
/// 且至少有一个分隔符或连词；文本首尾、冒号、括号、空格本身都不算界
#[test]
fn 来源核对_中文单字连词与分隔() {
    let 缺 = |purpose: &'static str, over: Json, cut: &'static str| {
        let v = 跑标记(purpose, over, "purpose", cut);
        let key = if cut == "k_ary" {
            "over_from"
        } else {
            "scale_from"
        };
        if v["detail"]["preds"][0][key] == "purpose" {
            json!([])
        } else {
            v["detail"]["warnings"][0]["not_in_purpose"].clone()
        }
    };
    // 正面
    assert_eq!(
        缺("从甲或乙里选出每家的短板。", json!(["甲", "乙"]), "k_ary"),
        json!([])
    );
    assert_eq!(
        缺("说出短板是甲还是乙。", json!(["甲", "乙"]), "k_ary"),
        json!([])
    );
    assert_eq!(缺("甲 或 乙", json!(["甲", "乙"]), "k_ary"), json!([]));
    assert_eq!(缺("按低到高排", json!(["低", "高"]), "ordered"), json!([]));
    assert_eq!(缺("好与坏", json!(["好", "坏"]), "k_ary"), json!([]));
    assert_eq!(
        缺("优、良、差", json!(["优", "良", "差"]), "ordered"),
        json!([])
    );
    // 已知漏判（宁漏勿误认）：候选之间隔着别的字、括号或只有空白
    assert_eq!(
        缺("（甲）优先，其次乙", json!(["甲", "乙"]), "k_ary"),
        json!(["甲", "乙"])
    );
    assert_eq!(
        缺("优先甲，其次乙", json!(["甲", "乙"]), "k_ary"),
        json!(["甲", "乙"])
    );
    assert_eq!(
        缺("甲比乙重要", json!(["甲", "乙"]), "k_ary"),
        json!(["甲", "乙"])
    );
    assert_eq!(
        缺("低 中 高", json!(["低", "中", "高"]), "ordered"),
        json!(["低", "中", "高"])
    );
    // 反面，逐个候选
    assert_eq!(
        缺(
            "目的：中高层员工的满意度，说出每家情况。",
            json!(["中", "低"]),
            "k_ary"
        ),
        json!(["中", "低"])
    );
    assert_eq!(
        缺("尽量提高 交付率。", json!(["高", "低"]), "k_ary"),
        json!(["高", "低"])
    );
    // 复核 Z0634 的 13 处误认（句首、句末、连词旁的词内单字，相邻两次出现），逐个候选判不在
    let 反 = [
        ("中高层员工的满意度如何", json!(["低", "中", "高"])),
        ("高层的意见最重要", json!(["高", "低"])),
        ("低于预期的供应商", json!(["低", "高"])),
        ("中国市场的供应商", json!(["中", "外"])),
        ("乙醇供应商的交付", json!(["甲", "乙"])),
        ("甲方与乙方的合作", json!(["甲", "乙"])),
        ("目标是提高", json!(["高", "低"])),
        ("达到高标准。", json!(["高", "低"])),
        ("提高和改善交付", json!(["高", "低"])),
        ("交付周期与质量", json!(["质", "价"])),
        ("看合作到期的时间", json!(["期", "年"])),
        ("门槛高高的", json!(["高", "低"])),
    ];
    for (purpose, over) in 反 {
        assert_eq!(缺(purpose, over.clone(), "k_ary"), over, "{purpose}");
    }
    // 已知误认：只要求另一项相邻，不要求这一项自身成词（「高层」的「高」挨着「低到」，「降低」的「低」挨着「到高」）
    assert_eq!(缺("从低到高层", json!(["低", "高"]), "ordered"), json!([]));
    assert_eq!(缺("降低到高位", json!(["低", "高"]), "ordered"), json!([]));
    // 单项误认、整表降级：表里有连词邻居时「期」「质」单项会认出，但「价」不在，整表仍降为 system、只列「价」
    let v = 跑标记(
        "交付周期与质量",
        json!(["期", "质", "价"]),
        "purpose",
        "k_ary",
    );
    assert_eq!(v["detail"]["preds"][0]["over_from"], "system", "{v}");
    assert_eq!(
        v["detail"]["warnings"][0]["not_in_purpose"],
        json!(["价"]),
        "{v}"
    );
}

/// Z0634：保留下来的候选去首尾空白后再核
#[test]
fn 来源核对_保留项去空白() {
    let v = 跑标记(
        "按甲、乙两类说出每家的短板。",
        json!([" 甲 ", "乙"]),
        "purpose",
        "k_ary",
    );
    assert_eq!(v["detail"]["preds"][0]["over_from"], "purpose", "{v}");
    assert_eq!(v["detail"]["warnings"], json!([]), "{v}");
}
