//! Z0517（线 A N10）：派生题去重。同一个 loop 里判断键重复会被 J-06 当成没有进展而停下整条 chain；
//! chain 跨跳带去重与诊断记忆后，重复的候选并入先出现的节点、诊断不再重发。预注册：地基/过程记录/工程-Z0517-派生去重.md。
//! 闭包端口，不发请求。数「发出的题」时按题面全等计，伴随元题（开 JPP_TEST_COMPANIONS=on 时每道题带的前提、
//! 两判、细分、参照、最缺哪类）题面不同，不计入。

mod derive_support;
use derive_support::*;
use jpp::value::Answer;
use serde_json::{Value as Json, json};
use std::cell::RefCell;

fn noul(p: f64) -> Answer {
    Answer::Noul(p)
}

/// 诊断题都放行（藏两判 ignore、在材料里 act）；其余题按 f
fn 读数<'a>(
    f: impl Fn(&str) -> f64 + 'a,
) -> impl Fn(&str, &jpp::value::Question, &jpp::value::State) -> Answer + 'a {
    move |t, _q, _s| {
        if t.contains("需要分别回答的判断") {
            noul(0.1)
        } else if t.contains("这段材料里有没有") {
            noul(0.9)
        } else {
            noul(f(t))
        }
    }
}

/// 唤出提示里被判的那道题（「…」里第一段）
fn 父题(p: &str) -> String {
    let a = p.find('「').map(|i| i + '「'.len_utf8()).unwrap_or(0);
    let b = p[a..].find('」').map(|j| a + j).unwrap_or(p.len());
    p[a..b].to_string()
}

fn t(x: &str) -> Json {
    json!({"op": "test", "text": x})
}

const 读出: &str = "{hops: r.detail.hops, per_hop: r.detail.per_hop, pending: len(r.pending),
  rejected: map(r.detail.rejected, fn(x) { x.stage + \":\" + join(x.codes, \",\") }),
  merged: if has(r.detail, \"merged\") { r.detail.merged } else { unit },
  value: map(r.value, fn(v) { {q: v.path[len(v.path) - 1].q, exit: exit_kind(v.exit),
                               also: if has(v, \"also\") { v.also } else { unit },
                               merged_into: if has(v, \"merged_into\") { v.merged_into } else { unit },
                               path_also: map(filter(v.path, fn(e) { has(e, \"also\") }), fn(e) { e.q })} })}";

fn 停了(w: &[String]) -> bool {
    w.iter().any(|x| x.contains("W-noprogress"))
}

/// 预测 1：两个条目、每跳唤出固定的 A、B 加一道随父题变化的新题；改前第 2 轮闸门对 A、B 再判「藏两判」→ J-06 停
#[test]
fn 跨跳同题面_不再被_j06_停下() {
    let src = r#"budget {calls: 400, cost: 0, depth: 8192};
let first = fn(item) { {q: test("根题：这个方案可行吗？", "t-root")} };
let r = chain([{on: mat("方案甲：三个月，两人")}, {on: mat("方案乙：半年，五人")}], first, 3, {elicit: true});
"#
    .to_string()
        + 读出;
    let 藏两判材料 = RefCell::new(Vec::<String>::new());
    let r = 跑_按提示(
        &src,
        |t, q, s| {
            if t.contains("需要分别回答的判断") {
                藏两判材料
                    .borrow_mut()
                    .push(serde_json::to_string(&s.to_json()).unwrap());
            }
            读数(|_| 0.5)(t, q, s)
        },
        |p| {
            vec![
                t("方案的预算写清楚了吗？"),
                t("方案的负责人写明了吗？"),
                t(&format!("{}（细一层）", 父题(p))),
            ]
        },
    )
    .unwrap();
    let v: Json = r.out.value_json();
    assert!(!停了(&r.out.trace.warnings), "{:?}", r.out.trace.warnings);
    assert_eq!(v["hops"], json!(3), "{v}");
    // 「藏两判」同一题面在一条 chain 里只判一次
    let mut 见 = 藏两判材料.borrow().clone();
    let n = 见.len();
    见.sort();
    见.dedup();
    assert_eq!(n, 见.len(), "藏两判有重发：{:?}", 藏两判材料.borrow());
    // A、B 在第 2 跳起对各自条目的已问节点记为并入
    let merged = v["merged"].as_array().unwrap();
    // 复核返修：into 写节点 id，并入已问节点的另标 asked
    assert!(
        merged
            .iter()
            .any(|m| m["asked"] == json!(true)
                || m["into"].as_str().is_some_and(|x| x.starts_with('x'))),
        "{v}"
    );
    // 与父节点同题的照旧按 same-as-source 拒
    let rej: Vec<String> = serde_json::from_value(v["rejected"].clone()).unwrap();
    assert!(rej.iter().any(|x| x == "structure:same-as-source"), "{v}");
}

/// 预测 1 的变体：只给固定候选，第 2 跳全部被并入或按 same-as-source 拒，没有新题可问而停（不是 J-06）
#[test]
fn 固定候选_无新题而停_不是_j06() {
    let src = r#"budget {calls: 400, cost: 0, depth: 8192};
let first = fn(item) { {q: test("根题：这个方案可行吗？", "t-root")} };
let r = chain([{on: mat("方案甲：三个月，两人")}, {on: mat("方案乙：半年，五人")}], first, 3, {elicit: true});
"#
    .to_string()
        + 读出;
    let r = 跑_按提示(&src, 读数(|_| 0.5), |_p| {
        vec![
            t("方案的预算写清楚了吗？"),
            t("方案的负责人写明了吗？"),
            t("方案的周期写明了吗？"),
        ]
    })
    .unwrap();
    let v: Json = r.out.value_json();
    assert!(!停了(&r.out.trace.warnings), "{:?}", r.out.trace.warnings);
    assert_eq!(v["hops"], json!(2), "{v}");
    let merged = v["merged"].as_array().unwrap();
    assert_eq!(merged.len(), 12, "{v}");
    let rej: Vec<String> = serde_json::from_value(v["rejected"].clone()).unwrap();
    assert_eq!(
        rej.iter()
            .filter(|x| *x == "structure:same-as-source")
            .count(),
        6,
        "{v}"
    );
}

/// 预测 2：同一跳两个父节点对同一条目唤出同一道新题 e：只发一次，并入记一条，两个父节点的未决都按细化消费
#[test]
fn 同跳两父同题_只发一次() {
    let src = r#"budget {calls: 200, cost: 0, depth: 8192};
let first = fn(item) { {q: test("根题：这个方案可行吗？", "t-root")} };
let r = chain([{on: mat("方案甲：三个月，两人")}], first, 3, {elicit: true});
"#
    .to_string()
        + 读出;
    let r = 跑_按提示(
        &src,
        读数(|t| if t.contains("交付日期") { 0.8 } else { 0.5 }),
        |p| {
            let f = 父题(p);
            if f.starts_with("根题") {
                vec![t("方案的预算写清楚了吗？"), t("方案的负责人写明了吗？")]
            } else {
                vec![t("方案写明交付日期了吗？")]
            }
        },
    )
    .unwrap();
    let v: Json = r.out.value_json();
    assert!(!停了(&r.out.trace.warnings), "{:?}", r.out.trace.warnings);
    assert_eq!(
        r.asked
            .iter()
            .filter(|x| x.as_str() == "方案写明交付日期了吗？")
            .count(),
        1,
        "{:?}",
        r.asked
    );
    let merged = v["merged"].as_array().unwrap();
    assert_eq!(merged.len(), 1, "{v}");
    assert_ne!(merged[0]["into"], json!("asked"), "{v}");
    assert_eq!(v["pending"], json!(0), "{v}");
    let e = v["value"]
        .as_array()
        .unwrap()
        .iter()
        .find(|x| x["q"] == "方案写明交付日期了吗？")
        .unwrap();
    assert_eq!(e["exit"], "act", "{v}");
    assert_eq!(e["also"].as_array().map(|a| a.len()), Some(1), "{v}");
}

/// 预测 3：第 2 跳派生出第 1 跳已问过的题（同一条目）：不再发，记并入已问节点
#[test]
fn 并入已问节点_不再发() {
    let src = r#"budget {calls: 200, cost: 0, depth: 8192};
let first = fn(item) { {q: test("根题：这个方案可行吗？", "t-root")} };
let r = chain([{on: mat("方案甲：三个月，两人")}], first, 3, {elicit: true});
"#
    .to_string()
        + 读出;
    let r = 跑_按提示(
        &src,
        读数(|t| if t.contains("负责人") { 0.8 } else { 0.5 }),
        |p| {
            let f = 父题(p);
            if f.starts_with("根题") {
                vec![t("方案的预算写清楚了吗？"), t("方案的负责人写明了吗？")]
            } else {
                vec![t("方案的负责人写明了吗？")]
            }
        },
    )
    .unwrap();
    let v: Json = r.out.value_json();
    assert!(!停了(&r.out.trace.warnings), "{:?}", r.out.trace.warnings);
    assert_eq!(
        r.asked
            .iter()
            .filter(|x| x.as_str() == "方案的负责人写明了吗？")
            .count(),
        1,
        "{:?}",
        r.asked
    );
    let merged = v["merged"].as_array().unwrap();
    assert_eq!(merged.len(), 1, "{v}");
    // 复核返修（B1）：into 是已问节点的 id；被并入方的父节点在 value 里留一个元素写明并进了哪里
    let into = merged[0]["into"].as_str().unwrap().to_string();
    assert!(into.starts_with("x0."), "{v}");
    assert_eq!(v["pending"], json!(0), "{v}");
    let back = v["value"]
        .as_array()
        .unwrap()
        .iter()
        .find(|x| x["q"] == "方案的预算写清楚了吗？")
        .unwrap_or_else(|| panic!("{v}"));
    assert_eq!(back["merged_into"], json!([into]), "{v}");
}

/// 复核返修（B2）：两父并入同一子题 e，e 拿不准继续细化；e 的子孙叶子的 path 里 e 那一条带 also
#[test]
fn 并入的节点继续细化_also_不丢() {
    let src = r#"budget {calls: 300, cost: 0, depth: 8192};
let first = fn(item) { {q: test("根题：这个方案可行吗？", "t-root")} };
let r = chain([{on: mat("方案甲：三个月，两人")}], first, 4, {elicit: true});
"#
    .to_string()
        + 读出;
    let r = 跑_按提示(
        &src,
        读数(|t| if t.contains("里程碑") { 0.8 } else { 0.5 }),
        |p| {
            let f = 父题(p);
            if f.starts_with("根题") {
                vec![t("方案的预算写清楚了吗？"), t("方案的负责人写明了吗？")]
            } else if f.contains("交付日期") {
                vec![t("方案列出了里程碑吗？")]
            } else {
                vec![t("方案写明交付日期了吗？")]
            }
        },
    )
    .unwrap();
    let v: Json = r.out.value_json();
    let leaf = v["value"]
        .as_array()
        .unwrap()
        .iter()
        .find(|x| x["q"] == "方案列出了里程碑吗？")
        .unwrap_or_else(|| panic!("{v}"));
    assert_eq!(leaf["path_also"], json!(["方案写明交付日期了吗？"]), "{v}");
}

/// 复核返修（C1）：③b 验题随条目带不同 ref，第二项在它自己的验题上 gap 为负，应拒；验题各判一次（12 次）
#[test]
fn 已知答案_ref_不同_各自判() {
    let ks = |r: &str| -> String {
        let a: Vec<String> = [0.8, 0.85, 0.9]
            .iter()
            .map(|p| format!("{{on: mat(\"p={p}\"), ref: mat(\"{r}\"), expect: \"act\"}}"))
            .collect();
        let b: Vec<String> = [0.1, 0.2, 0.3]
            .iter()
            .map(|p| format!("{{on: mat(\"p={p}\"), ref: mat(\"{r}\"), expect: \"ignore\"}}"))
            .collect();
        [a, b].concat().join(", ")
    };
    let src = format!(
        r#"budget {{calls: 60, cost: 0, depth: 512}};
let n1 = derive_node("x0", {{on: mat("甲的合同记录")}}, test("会续约吗？", "t0"), {{}});
let n2 = derive_node("x1", {{on: mat("乙的合同记录")}}, test("会续约吗？", "t0"), {{}});
let c1 = derive_child(n1, unit, 0, test("按时交付了吗？", "t1"), {{by: "elicit"}});
let c2 = derive_child(n2, unit, 0, test("按时交付了吗？", "t1"), {{by: "elicit"}});
let g = derive_gate([c1, c2], {{judge_diag: false, known: fn(x) {{ if x.id == "x0.0" {{ [{}] }} else {{ [{}] }} }}}});
{{pass: map(g.pass, fn(c) {{ c.id }}), rejected: map(g.rejected, fn(x) {{ x.id }})}}
"#,
        ks("参照：正"),
        ks("参照：反")
    );
    let r = 跑_关(
        &src,
        |_t, _q, s| {
            let p: f64 = s.on_text().strip_prefix("p=").unwrap().parse().unwrap();
            let 反 = s.r#ref.iter().any(|m| m.content.to_string().contains("反"));
            Answer::Noul(if 反 { 1.0 - p } else { p })
        },
        vec![],
    )
    .unwrap();
    let v = r.out.value_json();
    assert_eq!(r.calls, 12, "{v}");
    assert_eq!(v, json!({"pass": ["x0.0"], "rejected": ["x1.0"]}));
}

/// 复核返修（C2）：验题材料与 ref 相同、第二项期望反转：读数只判 6 次，第二项按自己的期望被拒
#[test]
fn 已知答案_期望不同_按各自期望重算() {
    let ks = |flip: bool| -> String {
        let (ea, eb) = if flip {
            ("ignore", "act")
        } else {
            ("act", "ignore")
        };
        let a: Vec<String> = [0.8, 0.85, 0.9]
            .iter()
            .map(|p| format!("{{on: mat(\"p={p}\"), expect: \"{ea}\"}}"))
            .collect();
        let b: Vec<String> = [0.1, 0.2, 0.3]
            .iter()
            .map(|p| format!("{{on: mat(\"p={p}\"), expect: \"{eb}\"}}"))
            .collect();
        [a, b].concat().join(", ")
    };
    let src = format!(
        r#"budget {{calls: 60, cost: 0, depth: 512}};
let n1 = derive_node("x0", {{on: mat("甲的合同记录")}}, test("会续约吗？", "t0"), {{}});
let n2 = derive_node("x1", {{on: mat("乙的合同记录")}}, test("会续约吗？", "t0"), {{}});
let c1 = derive_child(n1, unit, 0, test("按时交付了吗？", "t1"), {{by: "elicit"}});
let c2 = derive_child(n2, unit, 0, test("按时交付了吗？", "t1"), {{by: "elicit"}});
let g = derive_gate([c1, c2], {{judge_diag: false, known: fn(x) {{ if x.id == "x0.0" {{ [{}] }} else {{ [{}] }} }}}});
{{pass: map(g.pass, fn(c) {{ c.id }}), rejected: map(g.rejected, fn(x) {{ x.id }})}}
"#,
        ks(false),
        ks(true)
    );
    let r = 跑_关(
        &src,
        |_t, _q, s| Answer::Noul(s.on_text().strip_prefix("p=").unwrap().parse().unwrap()),
        vec![],
    )
    .unwrap();
    let v = r.out.value_json();
    assert_eq!(r.calls, 6, "{v}");
    assert_eq!(v, json!({"pass": ["x0.0"], "rejected": ["x1.0"]}));
    assert!(
        !r.out
            .trace
            .warnings
            .iter()
            .any(|w| w.contains("W-noprogress"))
    );
}
