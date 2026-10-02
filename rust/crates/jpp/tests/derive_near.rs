//! Z0497（线 A N2）：没有作者线时，读数落在画像边界带（delta.noul.mid，与 Z0398 默认链同一个带）内才派生。
//! 出口上的正交位 near_boundary 由运行时置、只读内置 near_boundary(e) 读；chain 在作者没给 near 时按它决定已决出口
//! 要不要再派生。预注册：地基/过程记录/工程-Z0497-边界带触发派生.md。闭包端口，不发请求。

mod derive_support;
use derive_support::*;
use jpp::effects::Profile;
use jpp::value::Answer;
use serde_json::{Value as Json, json};

/// `jev-1.13.0` 的副本，delta.noul.mid 设成 band（同 unsure_default.rs 的做法）；None = 画像没有 δ
fn 画像(band: Option<f64>) -> Profile {
    let Some(b) = band else {
        return Profile {
            hash: Some("测试".into()),
            ..Profile::untested()
        };
    };
    let p = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../profiles/jev-1.13.0.json");
    let mut j: Json = serde_json::from_str(&std::fs::read_to_string(p).unwrap()).unwrap();
    j["delta"]["noul"]["mid"]["immediate"]["p99"] = json!(b);
    Profile::from_json(&j).unwrap()
}

fn 程序(来源: &str, guard_read: bool) -> String {
    let _ = guard_read;
    format!(
        r#"budget {{calls: 60, cost: 0, depth: 4096}};
{来源}
let first = fn(item) {{ {{q: test("根题：这个方案可行吗？", "t-root")}} }};
let r = chain([{{on: mat("方案甲：三个月，两人")}}], first, 2, {{elicit: true}});
{{n: len(r.value), per_hop: r.detail.per_hop, pending: map(r.pending, fn(p) {{ {{cause: p.cause, exit: p.exit}} }}),
  root: map(r.value, fn(v) {{ {{exit: exit_kind(v.path[0].exit), near: near_boundary(v.path[0].exit)}} }})}}"#
    )
}

fn 读数(
    根: f64,
    补后: f64,
) -> impl Fn(&str, &jpp::value::Question, &jpp::value::State) -> Answer {
    move |t, _q, s| {
        if t.contains("需要分别回答的判断") {
            Answer::Noul(0.1)
        } else if t.contains("这段材料里有没有") {
            Answer::Noul(0.9)
        } else if t.starts_with("根题") {
            // 补过信息（状态里多了一份 ctx 材料）就给补后的读数
            Answer::Noul(if s.ctx.is_empty() { 根 } else { 补后 })
        } else if t.contains("为什么拿不准") || t.contains("最缺哪一类") {
            let k = s.over.len();
            let mut v = vec![0.0; k];
            v[0] = 1.0;
            Answer::Choice(v)
        } else {
            Answer::Noul(0.8)
        }
    }
}

fn 候选(_p: &str) -> Vec<Json> {
    vec![json!({"op": "test", "text": "方案的预算写清楚了吗？"})]
}

fn 派生了(v: &Json) -> bool {
    v["per_hop"][0]["derived"].as_i64().unwrap_or(0) > 0
}

#[test]
fn 带内派生_带外成叶() {
    let r = 跑_按提示_画像(&程序("", false), 读数(0.55, 0.55), 候选, 画像(Some(0.1))).unwrap();
    let v = r.out.value_json();
    assert!(派生了(&v), "0.55 在带内应派生：{v}");
    assert_eq!(r.gens, 1, "{v}");
    let r = 跑_按提示_画像(&程序("", false), 读数(0.8, 0.8), 候选, 画像(Some(0.1))).unwrap();
    let v = r.out.value_json();
    assert!(!派生了(&v), "0.8 在带外应成叶：{v}");
    assert_eq!(v["root"], json!([{"exit": "act", "near": false}]), "{v}");
    assert_eq!(r.gens, 0, "{v}");
}

#[test]
fn 画像没有delta_不置位不派生() {
    let r = 跑_按提示_画像(&程序("", false), 读数(0.55, 0.55), 候选, 画像(None)).unwrap();
    let v = r.out.value_json();
    assert!(!派生了(&v), "{v}");
    assert_eq!(v["root"], json!([{"exit": "act", "near": false}]), "{v}");
}

/// 固定关伴随题：开着时伴随题第一轮按中性读数可能路由到「题不清 / 两可」而不补，测不到「补后」的带
#[test]
fn 补到带外不派生_补后仍近才派生() {
    let 来源 = r#"unsure_source({need: ["材料"], fetch: fn(q, need, m) { "补来的材料" }});"#;
    let r = 跑_按提示_画像_关(&程序(来源, false), 读数(0.55, 0.9), 候选, 画像(Some(0.1))).unwrap();
    let v = r.out.value_json();
    assert!(!派生了(&v), "补后 0.9 在带外：{v}");
    assert_eq!(r.gens, 0, "{v}");
    let r = 跑_按提示_画像_关(&程序(来源, false), 读数(0.55, 0.56), 候选, 画像(Some(0.1))).unwrap();
    let v = r.out.value_json();
    assert!(派生了(&v), "补后 0.56 仍在带内：{v}");
}

#[test]
fn 把关下不补_带内照样置位派生() {
    let 来源 = r#"unsure_source({need: ["材料"], fetch: fn(q, need, m) { "补来的材料" }});"#;
    let r = 跑_按提示_画像_把关(&程序(来源, true), 读数(0.55, 0.9), 候选, 画像(Some(0.1))).unwrap();
    let v = r.out.value_json();
    // --guard 下默认链不补（补后 0.9 用不上），根题读数 0.55 在带内：置位、派生
    assert!(派生了(&v), "{v}");
    assert!(
        r.asked.iter().filter(|x| x.starts_with("根题")).count() == 1,
        "不补就只判一次：{:?}",
        r.asked
    );
}

// ———— Z0497 复核返修 (a) ————

fn 程序_跳(来源: &str, hops: usize, near: &str) -> String {
    format!(
        r#"budget {{calls: 60, cost: 0, depth: 4096}};
{来源}
let first = fn(item) {{ {{q: test("根题：这个方案可行吗？", "t-root")}} }};
let r = chain([{{on: mat("方案甲：三个月，两人")}}, {{on: mat("方案乙：半年，五人")}}], first, {hops}, {{elicit: true{near}}});
{{value: map(r.value, fn(v) {{ {{q: v.path[len(v.path) - 1].q, exit: exit_kind(v.exit),
  near: if has(v, "near") {{ v.near }} else {{ false }}, refined: if has(v, "refined") {{ v.refined }} else {{ [] }},
  unasked: if has(v, "unasked") {{ len(v.unasked) }} else {{ 0 }}}} }}),
 pending: map(r.pending, fn(p) {{ {{cause: p.cause, exit: p.exit}} }})}}"#
    )
}

fn 根题元素(v: &Json) -> Vec<Json> {
    v["value"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|x| x["q"] == "根题：这个方案可行吗？")
        .cloned()
        .collect()
}

/// hops = 1、带内读数：派生的子题落在最后一跳没问到，根题的出口仍在 value（改前 value 为空）
#[test]
fn 跳数1_带内_根题出口仍在() {
    let r = 跑_按提示_画像(&程序_跳("", 1, ""), 读数(0.55, 0.55), 候选, 画像(Some(0.1))).unwrap();
    let v = r.out.value_json();
    let roots = 根题元素(&v);
    assert_eq!(roots.len(), 2, "{v}");
    for x in &roots {
        assert_eq!(x["exit"], "act", "{v}");
        assert_eq!(x["near"], json!(true), "{v}");
        assert_eq!(x["unasked"], json!(1), "{v}");
    }
    // 作者给 near 的同形程序（不加载画像）同样留下根题元素
    let r = 跑_按提示(
        &程序_跳("", 1, ", near: {declare: {hi: 0.6, lo: 0.4}}"),
        读数(0.55, 0.55),
        候选,
    )
    .unwrap();
    let w = r.out.value_json();
    assert_eq!(根题元素(&w).len(), 2, "{w}");
}

/// hops = 2：子题问到了，挂进根题元素的 refined，unasked 为空
#[test]
fn 跳数2_子题挂作细化() {
    let r = 跑_按提示_画像(&程序_跳("", 2, ""), 读数(0.55, 0.55), 候选, 画像(Some(0.1))).unwrap();
    let v = r.out.value_json();
    assert_eq!(根题元素(&v).len(), 2, "{v}");
    for x in 根题元素(&v) {
        assert_eq!(x["unasked"], json!(0), "{v}");
        assert_eq!(
            x["refined"],
            json!([{"q": "方案的预算写清楚了吗？", "exit": "act"}]),
            "{v}"
        );
    }
}

/// 伴随题开着（强制开）：替身给伴随题中性读数（0.5）。Z0912（裁定七十三 (1)）起伴随题只在带外算数，带内没有信号，
/// 默认链照常往下走：补材料、再判 0.9，已决、不在带内 → 不置位、不派生，根题元素是叶（refined 空）。
/// 改前（按 0.5 分）中性读数判「两可」停下、不补，读数仍 0.55 → 置位派生；这条测试在 Z0912 之前的代码上失败
#[test]
fn 伴随题开着_带内中性读数_照常补_补后成叶() {
    let 来源 = r#"unsure_source({need: ["材料"], fetch: fn(q, need, m) { "补来的材料" }});"#;
    let r = 跑_按提示_画像_伴随(
        &程序_跳(来源, 2, ""),
        读数(0.55, 0.9),
        候选,
        画像(Some(0.1)),
    )
    .unwrap();
    let v = r.out.value_json();
    let roots = 根题元素(&v);
    assert_eq!(roots.len(), 2, "{v}");
    for x in &roots {
        assert_eq!(x["near"], json!(false), "带内中性读数不停，补后 0.9：{v}");
        assert_eq!(x["refined"], json!([]), "{v}");
    }
    assert!(
        r.out.unsure_default.iter().all(|u| u["fetched"] == json!(["材料"]) && u["end"] == "decided"),
        "{:?}",
        r.out.unsure_default
    );
}
