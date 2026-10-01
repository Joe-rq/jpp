//! 步 28（B0469）：划分细化与前提反面。
//! 细化：unsure(tie) / unsure(band) 按题类拆；是非题拆不了、不唤出时未决转交；打开 near 时，已判出而离边界近的
//! 出口在同一读数上再切一次作探测，落进带里也细化——原出口照判断器的回答走、不改判（主控答复 Q1）。
//! 前提反面：insufficient 且声明了前提时问前提（带否定标签）；ignore → 「前提不成立」；act → 未决转交；
//! 没声明前提不派生。闭包端口。

mod derive_support;
use derive_support::*;
use jpp::value::Answer;
use serde_json::{Value as Json, json};

fn noul(p: f64) -> Answer {
    Answer::Noul(p)
}

/// 默认链「为什么拿不准」那道 K 选一（候选类别加「两可」「题不清」）选第一项
fn 选第一项(k: usize) -> Answer {
    let mut v = vec![0.1 / (k - 1) as f64; k];
    v[0] = 0.9;
    Answer::Choice(v)
}

fn 诊断(text: &str) -> Option<Answer> {
    if text.contains("需要分别回答的判断") {
        Some(noul(0.1))
    } else if text.contains("这段材料里有没有") {
        Some(noul(0.9))
    } else {
        None
    }
}

fn 候选夹具(
    root: Vec<f64>,
) -> impl Fn(&str, &jpp::value::Question, &jpp::value::State) -> Answer {
    move |t, _q, _s| {
        诊断(t).unwrap_or_else(|| {
            if t.contains("的答案是否是「甲」") {
                noul(0.8)
            } else if t.contains("的答案是否是「乙」") {
                noul(0.3)
            } else if t.contains("的答案是否是「丙」") {
                noul(0.1)
            } else if t.contains("谁最合适") {
                Answer::Choice(root.clone())
            } else {
                panic!("夹具没有：{t}")
            }
        })
    }
}

fn 程度夹具(
    root: Vec<f64>,
) -> impl Fn(&str, &jpp::value::Question, &jpp::value::State) -> Answer {
    move |t, q, _s| {
        诊断(t).unwrap_or_else(|| {
            if !q.scale.is_empty() {
                Answer::Score(root.clone())
            } else if t.contains("至少达到「中」") {
                noul(0.8)
            } else if t.contains("至少达到「高」") {
                noul(0.2)
            } else {
                panic!("夹具没有：{t}")
            }
        })
    }
}

const 三选一: &str = r#"budget {calls: 20, cost: 0, depth: 512};
let first = fn(item) { {q: select("这件事谁最合适牵头？", "t-root"), over: ["甲", "乙", "丙"]} };
"#;

#[test]
fn a_select_并列_拆成逐候选是非题_合成取判_act_的候选() {
    let src =
        三选一.to_string() + "let r = chain([{on: mat(\"三人的履历\")}], first, 3, {});\n" + 读出;
    let r = 跑(&src, 候选夹具(vec![0.4, 0.4, 0.2]), vec![]).unwrap();
    let v: Json = r.out.value_json();
    assert_eq!(
        v["value"].as_array().unwrap().len(),
        1,
        "三道子题合成一个元素：{v}"
    );
    assert_eq!(v["value"][0]["refined"], json!(["甲"]));
    assert_eq!(
        v["value"][0]["path"],
        json!(["author:unsure(tie)"]),
        "原出口不改判"
    );
    assert_eq!(v["pending"], json!([]), "来源的未决按细化去向消费");
    // 原题 1 +「藏两判」3 +「在不在」3（lib/diag.jpp，每道子题两道诊断）+ 三道子题同一状态 1
    assert_eq!(r.calls, 8);
}

#[test]
fn b_是非题的_band_拆不了_不唤出时未决转交() {
    let src = r#"budget {calls: 20, cost: 0, depth: 512};
let first = fn(item) { {q: test("这份提案可行吗？", "t-root"), line: {declare: {hi: 0.7, lo: 0.3}}} };
let r = chain([{on: mat("一份提案")}], first, 3, {});
"#
    .to_string()
        + 读出;
    // Z0514：未决先走默认链；没写取法，「为什么拿不准」选出「材料」后路 C 转交，仍在 pending，多一次判断
    // Z0514：默认链的选路由「为什么拿不准」定，固定关伴随题（伴随题的中性读数会先判两可放弃）
    let r = 跑_关(
        &src,
        |t, _q, s| {
            if t.contains("为什么拿不准") {
                选第一项(s.over.len())
            } else {
                noul(0.5)
            }
        },
        vec![],
    )
    .unwrap();
    let v: Json = r.out.value_json();
    assert_eq!(v["pending"], json!(["band"]), "{v}");
    assert_eq!(r.calls, 2);
    assert_eq!(r.gens, 0);
    assert_eq!(r.out.unsure_default.len(), 1, "{:?}", r.out.unsure_default);
    assert_eq!(r.out.unsure_default[0]["end"], json!("handoff"));
    assert_eq!(r.out.unsure_default[0]["needed"], json!(["材料"]));
}

/// near 的探测是「同一读数上按声明线再切一次」：是非题与程度题可直接用；K 选一的声明线要求测过置换
/// （没测时探测出 unsure(untested)，不算落进带里），所以这里用程度题
#[test]
fn c_near_打开时_已判出而离边界近也细化_原出口不动() {
    let src = r#"budget {calls: 20, cost: 0, depth: 512};
let first = fn(item) { {q: measure("这份方案的完成度如何？", ["低", "中", "高"], "t-root")} };
let r = chain([{on: mat("一份方案")}], first, 3, {near: {declare: {hi: 0.6}}});
"#
    .to_string()
        + 读出;
    let r = 跑(&src, 程度夹具(vec![0.2, 0.5, 0.3]), vec![]).unwrap();
    let v: Json = r.out.value_json();
    assert_eq!(
        v["value"][0]["path"],
        json!(["author:at(1)"]),
        "原出口照回答走：{v}"
    );
    assert_eq!(v["value"][0]["refined"], json!("中"));
    // 原题 1 +「藏两判」2 +「在不在」2 + 两道子题同一状态 1；探测不另发判断
    assert_eq!(r.calls, 6);
}

#[test]
fn d_near_不打开时_已判出的出口成叶() {
    let src =
        三选一.to_string() + "let r = chain([{on: mat(\"三人的履历\")}], first, 3, {});\n" + 读出;
    let r = 跑(&src, 候选夹具(vec![0.5, 0.3, 0.2]), vec![]).unwrap();
    let v: Json = r.out.value_json();
    assert_eq!(v["value"][0]["path"], json!(["author:pick(0)"]));
    assert_eq!(v["value"][0]["refined"], Json::Null);
    assert_eq!(r.calls, 1);
}

#[test]
fn e_程度题的_band_拆成_m减1_道至少达到第_j_档() {
    let src = r#"budget {calls: 20, cost: 0, depth: 512};
let first = fn(item) { {q: measure("这份方案的完成度如何？", ["低", "中", "高"], "t-root"), line: {declare: {hi: 0.6}}} };
let r = chain([{on: mat("一份方案")}], first, 3, {});
"#
    .to_string()
        + 读出;
    let r = 跑_关(
        &src,
        |t, q, _s| {
            诊断(t).unwrap_or_else(|| {
                if !q.scale.is_empty() {
                    Answer::Score(vec![0.3, 0.4, 0.3])
                } else if t.contains("至少达到「中」") {
                    noul(0.8)
                } else if t.contains("至少达到「高」") {
                    noul(0.2)
                } else {
                    panic!("夹具没有：{t}")
                }
            })
        },
        vec![],
    )
    .unwrap();
    let v: Json = r.out.value_json();
    assert_eq!(r.asked.iter().filter(|t| t.contains("至少达到")).count(), 2);
    assert_eq!(v["value"][0]["refined"], json!("中"));
    assert_eq!(v["value"][0]["path"], json!(["author:unsure(band)"]));
}

const 缺证据: &str = r#"budget {calls: 20, cost: 0, depth: 512};
let q0 = form("test", "c 的加入会让{双方}的合作金额变大吗？", {calib: "t-root", evidence: ["ref"], presupposition: "a 与 b 已经在合作"});
let first = fn(item) { {q: fill(q0, {双方: "a 与 b"})} };
let r = chain([{on: mat("a、b、c 的情况")}], first, 3, {});
"#;

fn 前提夹具(p: f64) -> impl Fn(&str, &jpp::value::Question, &jpp::value::State) -> Answer {
    move |t, q, _s| {
        诊断(t).unwrap_or_else(|| {
            if t == "是否a 与 b 已经在合作？" {
                let l = q.labels.as_ref().expect("前提题带答案标签");
                assert_eq!(l.yes, "a 与 b 已经在合作");
                assert_eq!(l.no, "并非：a 与 b 已经在合作");
                noul(p)
            } else {
                noul(0.9)
            }
        })
    }
}

#[test]
fn f_insufficient_问前提_判不成立即前提不成立() {
    let r = 跑(&(缺证据.to_string() + 读出), 前提夹具(0.2), vec![]).unwrap();
    let v: Json = r.out.value_json();
    assert_eq!(v["value"][0]["conclusion"], json!("前提不成立"), "{v}");
    assert_eq!(
        v["value"][0]["path"],
        json!(["author:unsure(insufficient:ref)", "premise:ignore"])
    );
    assert_eq!(v["pending"], json!([]));
}

#[test]
fn g_前提成立_原题的未决转交() {
    let r = 跑(&(缺证据.to_string() + 读出), 前提夹具(0.8), vec![]).unwrap();
    let v: Json = r.out.value_json();
    // 步 36 G3：原因名只写成员名（B197）
    assert_eq!(v["pending"], json!(["insufficient"]), "{v}");
}

#[test]
fn h_没声明前提_不派生_未决转交() {
    let src = r#"budget {calls: 20, cost: 0, depth: 512};
let first = fn(item) { {q: test("c 的加入会让合作金额变大吗？", "t-root", {evidence: ["ref"]})} };
let r = chain([{on: mat("a、b、c 的情况")}], first, 3, {});
"#
    .to_string()
        + 读出;
    // Z0514：默认链的选路由「为什么拿不准」定，固定关伴随题（伴随题的中性读数会先判两可放弃）
    let r = 跑_关(&src, |_t, _q, _s| noul(0.9), vec![]).unwrap();
    let v: Json = r.out.value_json();
    assert_eq!(v["pending"], json!(["insufficient"]));
    // Z0514：未决先走默认链；「为什么拿不准」回答形状不符就停，insufficient 链末转交（Z0589），多一次判断
    assert_eq!(r.calls, 2);
    assert_eq!(r.out.unsure_default.len(), 1, "{:?}", r.out.unsure_default);
    assert_eq!(r.out.unsure_default[0]["end"], json!("handoff"));
    assert_eq!(r.out.unsure_default[0]["slot"], json!("ref"));
}

/// Z0542（过程记录 5.33）：跳数上限恰在派生之后，末跳过了闸门的子题没被问——父节点的未决不按 branch 销账，
/// 随返回值转交（pending 的 via 是 derive#0:unasked），账本没有它的 Refine{branch}；多一跳时子题被问，父节点照旧
/// 按 branch 消费、不在 pending
#[test]
fn z0542_到上限没问的子题_父节点未决转交() {
    let 跑一 = |hops: u32| {
        let src = 三选一.to_string()
            + &format!("let r = chain([{{on: mat(\"三人的履历\")}}], first, {hops}, {{}});\n")
            + 读出;
        跑(&src, 候选夹具(vec![0.4, 0.4, 0.2]), vec![]).unwrap()
    };
    let 细化消费 = |l: &jpp::ledger::Ledger| {
        l.entries
            .iter()
            .filter(|e| matches!(e, jpp::ledger::Entry::Refine { how, .. } if how == "branch"))
            .count()
    };
    let r = 跑一(1);
    let v: Json = r.out.value_json();
    assert_eq!(v["unasked"], json!(3), "{v}");
    assert_eq!(
        v["pending"],
        json!(["tie"]),
        "父节点的未决随返回值转交：{v}"
    );
    let via = v["carried"][0]["via"].to_string();
    assert!(via.contains("derive#0:unasked"), "{via}");
    assert_eq!(细化消费(&r.ledger), 0, "没有按 branch 销账");
    let r = 跑一(2);
    let v: Json = r.out.value_json();
    assert_eq!(
        v["pending"],
        json!([]),
        "子题被问，来源的未决按细化去向消费：{v}"
    );
    assert_eq!(细化消费(&r.ledger), 1);
}
