//! B0478：作者在 .jpp 里填十个模块，语言按守则组装成题和输入；与 purpose_run 共用核心（purpose_core）。
//! 预注册：地基/过程记录/工程-B0478-作者填模块.md 第七节。闭包端口，不发请求；任务是虚构的「租房」，与线 A 的靶子无关。
//! 数题一律按题面全等（伴随题开时每道题另带元题，按子串数会把它们算进去）。

mod derive_support;
use derive_support::*;
use jpp::value::{Answer, Question, State};
use serde_json::{Value as Json, json};
use std::cell::RefCell;
use std::sync::atomic::{AtomicUsize, Ordering};

const 有序题: &str = "就这份材料来说，这套房满足租客需求的程度落在哪一档？";
const 选一题: &str = "就这份材料来说，这套房对这位租客最大的不合适之处是下列哪一项？";
const 是非题: &str = "就这份材料来说，是否这套房的月租在租客预算以内？";

const 房源: &str = "[{on: mat(\"一室一厅，月租四千，离地铁五百米，朝北\")}, {on: mat(\"两室，月租六千，离地铁两公里，朝南\")}, {on: mat(\"开间，月租三千，离地铁一百米，无窗\")}]";

fn 程序(modules: &str) -> String {
    format!(
        "import \"../../lib/derive/modules.jpp\";\nbudget {{calls: 2000, cost: 0, depth: 8192}};\nlet items = {房源};\nlet r = modules_run({modules}, {{}});\n\
         {{value: r.value, pending: map(r.pending, fn(p) {{ {{cause: p.cause, exit: p.exit}} }}), detail: r.detail, fill: derive_rules().purpose_fill}}"
    )
}

const 全模块: &str = "{material: items,
    predicates: [{text: \"这套房满足租客需求的程度\", cut: \"ordered\", scale: [\"不满足\", \"部分满足\", \"满足\"], field: \"fit\"},
                 {text: \"这套房对这位租客最大的不合适之处\", cut: \"k_ary\", over: [\"价格\", \"通勤\", \"采光\", \"没有明显不合适\"], field: \"flaw\"}],
    reference: mat(\"租客：预算五千以内，每天地铁通勤，喜欢朝南\"),
    context: \"北京，十月看房\",
    presupposition: \"房源信息是真实的\",
    done: [\"fit\", \"flaw\"]}";

fn 第几套(s: &State) -> usize {
    let t = s.on.first().and_then(|m| m.content.as_str()).unwrap_or("");
    if t.starts_with("一室") {
        0
    } else if t.starts_with("两室") {
        1
    } else {
        2
    }
}

/// 查题库一律判「都不是」；有序题按房源给档；选一题第 0 套选通勤、第 1 套选价格、第 2 套选采光
fn 读数(t: &str, _q: &Question, s: &State) -> Answer {
    if t.contains("需要分别回答的判断") {
        // 目的入口闸门第③段的诊断（作者入口不问）：放行
        Answer::Noul(0.1)
    } else if t.contains("这段材料里有没有") {
        Answer::Noul(0.9)
    } else if t.contains("已认证的题问的是同一件事") {
        let k = s.over.len();
        let mut v = vec![0.0; k];
        v[k - 1] = 1.0;
        Answer::Choice(v)
    } else if t == 有序题 {
        Answer::Score(match 第几套(s) {
            0 => vec![0.2, 0.6, 0.2],
            1 => vec![0.1, 0.2, 0.7],
            _ => vec![0.7, 0.2, 0.1],
        })
    } else if t == 选一题 {
        let mut v = vec![0.1; 4];
        v[[1, 0, 2][第几套(s)]] = 0.7;
        Answer::Choice(v)
    } else if t == 是非题 {
        Answer::Noul(if 第几套(s) == 1 { 0.1 } else { 0.9 })
    } else {
        Answer::Noul(0.5)
    }
}

fn 无生成(_p: &str) -> Vec<Json> {
    vec![]
}

#[test]
fn 只填模块_零生成_组装出题与输入() {
    type 见 = (
        String,
        Vec<String>,
        Vec<String>,
        Option<String>,
        Vec<String>,
    );
    let seen: RefCell<Vec<见>> = RefCell::new(vec![]);
    let r = 跑_按提示(
        &程序(全模块),
        |t, q, s| {
            if t == 有序题 || t == 选一题 {
                let cs = |ms: &Vec<jpp::value::Mat>| {
                    ms.iter()
                        .map(|m| m.content.as_str().unwrap_or("").to_string())
                        .collect::<Vec<_>>()
                };
                seen.borrow_mut().push((
                    t.to_string(),
                    cs(&s.ctx),
                    cs(&s.r#ref),
                    q.presupposition.clone(),
                    q.evidence.clone(),
                ));
            }
            读数(t, q, s)
        },
        无生成,
    )
    .unwrap();
    let v = r.out.value_json();
    // 预测 1：零生成；判断调用 = 查题库 2 次 + 每项 2 次
    assert_eq!(r.gens, 0, "{v}");
    assert_eq!(r.calls, 8, "{v}");
    let d = &v["detail"];
    assert_eq!(d["entry"], "author", "{d}");
    // 预测 2：组装的题
    assert_eq!(d["questions"][0]["q"], 有序题, "{d}");
    assert_eq!(d["questions"][0]["op"], "measure", "{d}");
    assert_eq!(d["questions"][0]["from"], "assemble", "{d}");
    assert_eq!(d["questions"][1]["q"], 选一题, "{d}");
    assert_eq!(d["questions"][1]["op"], "select", "{d}");
    assert_eq!(d["questions"][1]["from"], "assemble", "{d}");
    for k in 0..2 {
        assert_eq!(
            d["questions"][k]["lacks"],
            json!(["材料", "语境", "参照"]),
            "{d}"
        );
    }
    assert_eq!(d["elicit"]["calls"], 0, "{d}");
    assert_eq!(d["gate_items"], json!([]), "{d}");
    // 输入：语境进 ctx、参照进 ref、候选进 over
    assert_eq!(
        d["inputs"][0],
        json!({"field": "fit", "slots": ["on", "ref", "ctx"], "ctx": "北京，十月看房"}),
        "{d}"
    );
    assert_eq!(
        d["inputs"][1],
        json!({"field": "flaw", "slots": ["on", "ref", "ctx", "over"], "ctx": "北京，十月看房", "over": ["价格", "通勤", "采光", "没有明显不合适"]}),
        "{d}"
    );
    // 判断器真正看到的状态与题式声明
    let seen = seen.borrow();
    assert_eq!(seen.iter().filter(|x| x.0 == 有序题).count(), 3, "{seen:?}");
    assert_eq!(seen.iter().filter(|x| x.0 == 选一题).count(), 3, "{seen:?}");
    for (_, ctx, rf, pre, ev) in seen.iter() {
        assert_eq!(ctx, &vec!["北京，十月看房".to_string()]);
        assert_eq!(
            rf,
            &vec!["租客：预算五千以内，每天地铁通勤，喜欢朝南".to_string()]
        );
        assert_eq!(pre.as_deref(), Some("房源信息是真实的"));
        assert_eq!(ev, &vec!["ref".to_string()]);
    }
    // 字段：名次（第 1 套档最高）与不合适之处；预测 3：全部可信
    let vs = v["value"].as_array().unwrap();
    assert_eq!(vs.len(), 3, "{v}");
    assert_eq!(vs[1]["fields"]["fit"], json!(1), "{v}");
    assert_eq!(vs[0]["fields"]["flaw"], "通勤", "{v}");
    assert_eq!(vs[1]["fields"]["flaw"], "价格", "{v}");
    assert_eq!(vs[2]["fields"]["flaw"], "采光", "{v}");
    for x in vs {
        assert_eq!(
            x["trust"],
            json!({"fit": "trusted", "flaw": "trusted"}),
            "{v}"
        );
    }
    assert_eq!(v["pending"], json!([]), "{v}");
    assert_eq!(d["fields"], json!(["fit", "flaw"]), "{d}");
    assert_eq!(d["fields_missing"], json!([]), "{d}");
    // 作者填了的模块都记 author
    for k in [
        "material",
        "predicate",
        "cut",
        "presupposition",
        "context",
        "reference",
        "done",
    ] {
        assert_eq!(d["sources"][k], "author", "{k} {d}");
    }
}

#[test]
fn 只填材料与谓词_其余由系统补() {
    let m = "{material: map(items, fn(x) { {on: x.on, ref: mat(\"租客：预算五千以内\")} }),
              predicates: [{text: \"这套房的月租在租客预算以内\", cut: \"binary\"}]}";
    let r = 跑_按提示(&程序(m), 读数, 无生成).unwrap();
    let v = r.out.value_json();
    let d = &v["detail"];
    assert_eq!(r.gens, 0, "{v}");
    assert_eq!(d["questions"][0]["q"], 是非题, "{d}");
    assert_eq!(d["questions"][0]["op"], "test", "{d}");
    // 预测 5：系统补的模块与来源
    let vs = v["value"].as_array().unwrap();
    let s = &d["sources"];
    for k in ["material", "predicate", "cut"] {
        assert_eq!(s[k], "author", "{k} {s}");
    }
    for k in ["request", "budget", "unsure", "done"] {
        assert_eq!(s[k], "system", "{k} {s}");
    }
    assert_eq!(s["context"], "system:none", "{s}");
    // 项都带参照：系统把参照模块指向 ref 槽
    assert_eq!(s["reference"], "system", "{s}");
    // 前提：作者入口没有目的原文，不派生（B0478 rebase 到 B0470 之上，复核要求按实际落地值核）——
    // 来源记 not-derived；题式不带前提；生成器没被调（没有前提派生那一次）；判断端口没收到前提题；没有 premise 明细与每行标记
    assert_eq!(s["presupposition"], "not-derived", "{s}");
    assert!(d.get("premise").is_none(), "{d}");
    assert!(
        vs.iter()
            .all(|x| x.get("premise").is_none() && x.get("excluded").is_none()),
        "{v}"
    );
    assert!(
        !r.asked.iter().any(|t| t.contains("材料里写明")),
        "{:?}",
        r.asked
    );
    assert!(
        r.ledger.entries.iter().all(|e| !serde_json::to_string(e)
            .unwrap()
            .contains("purpose-premise")),
        "账本里不该有前提题（前提题的校准标签是 purpose-premise）"
    );
    // 完成条件由系统补成全部谓词的字段（字段名缺省 p0）
    assert_eq!(d["fields"], json!(["p0"]), "{d}");
    assert_eq!(d["inputs"][0]["slots"], json!(["on", "ref"]), "{d}");
    assert_eq!(vs[0]["fields"]["p0"], json!(true), "{v}");
    assert_eq!(vs[1]["fields"]["p0"], json!(false), "{v}");
}

/// 预测 4：缺必填模块即报错，报文带码；判断端口与生成器端口都没被调用
#[test]
fn 缺模块报错_先于任何效应() {
    let p1 = "{text: \"这套房满足租客需求的程度\", cut: \"ordered\", scale: [\"低\", \"高\"], field: \"fit\"}";
    let cases: Vec<(String, &str)> = vec![
        (format!("{{predicates: [{p1}]}}"), "E-module-missing"),
        (format!("{{material: [], predicates: [{p1}]}}"), "E-module-missing"),
        ("{material: items}".into(), "E-module-missing"),
        ("{material: items, predicates: []}".into(), "E-module-missing"),
        ("{material: items, predicates: [{cut: \"binary\"}]}".into(), "E-module-missing"),
        ("{material: items, predicates: [{text: \"月租在预算以内\"}]}".into(), "E-module-missing"),
        ("{material: items, predicates: [{text: \"月租在预算以内\", cut: \"yes_no\"}]}".into(), "E-module-missing"),
        ("{material: items, predicates: [{text: \"最大的不合适\", cut: \"k_ary\", over: [\"价格\"]}]}".into(), "E-module-missing"),
        ("{material: items, predicates: [{text: \"满足程度\", cut: \"ordered\"}]}".into(), "E-module-missing"),
        (format!("{{material: items, predicates: [{p1}], done: [\"fit\", \"why\"]}}"), "E-module-done"),
        (format!("{{material: items, predicates: [{p1}], contxt: \"北京\"}}"), "E-module-unknown"),
        ("{material: items, predicates: [{text: \"月租在预算以内\", cut: \"binary\", lines: 1}]}".into(), "E-module-unknown"),
        (format!("{{material: [{{on: mat(\"x\"), note: 1}}], predicates: [{p1}]}}"), "E-module-bad"),
        (format!("{{material: items, predicates: [{p1}, {p1}]}}"), "E-module-bad"),
        (format!("{{material: items, predicates: [{p1}], request: \"some\"}}"), "E-module-bad"),
    ];
    for (m, code) in cases {
        let calls = AtomicUsize::new(0);
        let gens = AtomicUsize::new(0);
        let e = 跑_按提示(
            &程序(&m),
            |t, q, s| {
                calls.fetch_add(1, Ordering::Relaxed);
                读数(t, q, s)
            },
            |_p| {
                gens.fetch_add(1, Ordering::Relaxed);
                vec![]
            },
        )
        .err()
        .unwrap_or_else(|| panic!("应报错：{m}"));
        assert!(e.contains(code), "{m} → {e}");
        assert_eq!(calls.load(Ordering::Relaxed), 0, "{m}");
        assert_eq!(gens.load(Ordering::Relaxed), 0, "{m}");
    }
}

/// 查题库命中：answer 选中题面含 needle 的已认证题式
fn 命中(needle: &'static str) -> impl Fn(&str, &Question, &State) -> Answer {
    move |t, q, s| {
        if t.contains("已认证的题问的是同一件事") {
            let k = s.over.len();
            let i = s
                .over
                .iter()
                .position(|m| m.content.as_str().is_some_and(|x| x.contains(needle)))
                .unwrap_or(k - 1);
            let mut v = vec![0.0; k];
            v[i] = 1.0;
            Answer::Choice(v)
        } else if t.contains("很难或无法完全撤销") {
            Answer::Noul(0.9)
        } else {
            读数(t, q, s)
        }
    }
}

#[test]
fn 命中零槽题式_用题库的题_不组装() {
    let m = "{material: items, predicates: [{text: \"这项操作执行以后难以撤销\", cut: \"binary\", field: \"irreversible\"}]}";
    let r = 跑_按提示(&程序(m), 命中("很难或无法完全撤销"), 无生成).unwrap();
    let v = r.out.value_json();
    let d = &v["detail"];
    assert_eq!(r.gens, 0, "{v}");
    assert_eq!(d["bank"][0]["how"], "bank", "{d}");
    assert_eq!(d["bank"][0]["hit"], "hard_to_undo", "{d}");
    assert_eq!(d["questions"][0]["from"], "bank", "{d}");
    assert!(
        d["questions"][0]["q"]
            .as_str()
            .unwrap()
            .contains("很难或无法完全撤销"),
        "{d}"
    );
    // 作者的文字判出的命中，题可信
    for x in v["value"].as_array().unwrap() {
        assert_eq!(x["trust"]["irreversible"], "trusted", "{v}");
        assert_eq!(x["fields"]["irreversible"], json!(true), "{v}");
    }
}

#[test]
fn 命中带槽题式_记缺参数_退回组装() {
    let m =
        "{material: items, predicates: [{text: \"这套房的月租在租客预算以内\", cut: \"binary\"}]}";
    let r = 跑_按提示(&程序(m), 命中("是否有字段"), 无生成).unwrap();
    let v = r.out.value_json();
    let d = &v["detail"];
    assert_eq!(r.gens, 0, "{v}");
    assert_eq!(d["bank"][0]["how"], "needs-params:B0479", "{d}");
    assert_eq!(d["bank"][0]["hit"], "json_field", "{d}");
    assert_eq!(d["questions"][0]["from"], "assemble", "{d}");
    assert_eq!(d["questions"][0]["q"], 是非题, "{d}");
}

/// 同一条路：同一份模块记录由生成器从目的里抽出（purpose_run）与由作者填（modules_run），契约值与 detail 的形状相同，
/// 差别只在入口、来源标记与题从哪来（唤出 / 组装）
#[test]
fn 两个入口同一个核心() {
    let 模块 = json!({"material": "房源；参照是租客的需求",
        "predicates": [{"text": "这套房对这位租客最大的不合适之处", "cut": "k_ary", "over": ["价格", "通勤", "采光", "没有明显不合适"],
                        "field": "flaw", "cut_from": "purpose", "over_from": "purpose"}],
        "request": null, "presupposition": null, "context": null, "reference": null, "budget": null, "unsure": null, "done": ["flaw"]});
    let 生成 = move |p: &str| {
        if p.contains("字面前提题") {
            vec![]
        } else if p.starts_with("下面是一段目的") {
            vec![模块.clone()]
        } else {
            vec![
                json!({"op": "select", "text": 选一题, "over": ["价格", "通勤", "采光", "没有明显不合适"]}),
            ]
        }
    };
    let src_p = format!(
        "import \"../../lib/derive/purpose.jpp\";\nbudget {{calls: 2000, cost: 0, depth: 8192}};\nlet items = {房源};\n\
         let r = purpose_run(\"给这些房源说出每套对租客最大的不合适之处，是价格、通勤、采光还是没有明显不合适\", items, {{}});\n\
         {{value: r.value, detail: r.detail}}"
    );
    let a = 跑_按提示(&src_p, 读数, 生成).unwrap().out.value_json();
    let m = "{material: items, predicates: [{text: \"这套房对这位租客最大的不合适之处\", cut: \"k_ary\", over: [\"价格\", \"通勤\", \"采光\", \"没有明显不合适\"], field: \"flaw\"}]}";
    let b = 跑_按提示(&程序(m), 读数, 无生成).unwrap().out.value_json();
    let ks = |x: &Json| {
        // premise 明细只有目的入口有（前提派生与筛选只接目的入口，B0470）
        let mut k: Vec<String> = x["detail"]
            .as_object()
            .unwrap()
            .keys()
            .filter(|k| *k != "premise")
            .cloned()
            .collect();
        k.sort();
        k
    };
    assert_eq!(ks(&a), ks(&b), "{a}\n{b}");
    assert_eq!(a["detail"]["entry"], "purpose");
    assert_eq!(b["detail"]["entry"], "author");
    assert_eq!(a["detail"]["fields"], b["detail"]["fields"]);
    assert_eq!(a["detail"]["questions"][0]["from"], "elicit", "{a}");
    assert_eq!(b["detail"]["questions"][0]["from"], "assemble", "{b}");
    let rk = |x: &Json| {
        x["value"]
            .as_array()
            .unwrap()
            .iter()
            .map(|r| {
                r["fields"]
                    .as_object()
                    .unwrap()
                    .keys()
                    .cloned()
                    .collect::<Vec<_>>()
            })
            .collect::<Vec<_>>()
    };
    assert_eq!(rk(&a), rk(&b));
}
