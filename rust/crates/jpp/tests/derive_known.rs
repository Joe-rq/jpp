//! 步 28（B0468）：验题闸门的已知答案一半（过程记录 §16.1 第 4 项）。原定义 `04-语言规范-v0.md` §5.4：
//! 「≥3 act + ≥3 ignore；gap = min(p|act) − max(p|ignore) ≥ 0.20（safety 0.30）」；程度题按原实现
//! （`地基/foundation/core/probe.py`）：每档 ≥ 2 道验题、期望档偏差 ≤ 0.5。候选直接交给 `derive_gate`，
//! `opts.known` 给验题材料与期望。闭包端口：验题材料的文字就是读数（「p=0.85」），不发请求。

mod derive_support;
use derive_support::*;
use jpp::value::Answer;
use serde_json::{Value as Json, json};

/// 验题材料写成「p=0.85」（是非）或「s=0,0.9,0.1」（程度，各档概率）
fn 按材料(_t: &str, _q: &jpp::value::Question, s: &jpp::value::State) -> Answer {
    let m = s.on_text();
    if let Some(x) = m.strip_prefix("p=") {
        Answer::Noul(x.parse().unwrap())
    } else if let Some(x) = m.strip_prefix("s=") {
        Answer::Score(x.split(',').map(|v| v.parse().unwrap()).collect())
    } else {
        panic!("夹具没有：{m}")
    }
}

/// 一个是非候选、一组验题：acts / igns 是读数
fn 是非(acts: &[f64], igns: &[f64], gate_opts: &str) -> Json {
    let ks: Vec<String> = acts
        .iter()
        .map(|p| format!("{{on: mat(\"p={p}\"), expect: \"act\"}}"))
        .chain(
            igns.iter()
                .map(|p| format!("{{on: mat(\"p={p}\"), expect: \"ignore\"}}")),
        )
        .collect();
    let src = format!(
        r#"budget {{calls: 20, cost: 0, depth: 512}};
let n = derive_node("x0", {{on: mat("甲的合同记录")}}, test("甲会续约吗？", "t0"), {{}});
let c = derive_child(n, unit, 0, test("甲按时交付了吗？", "t1"), {{by: "elicit"}});
let g = derive_gate([c], {{judge_diag: false, known: fn(x) {{ [{}] }}{gate_opts}}});
{{pass: len(g.pass), rejected: map(g.rejected, fn(x) {{ {{stage: x.stage, codes: x.codes}} }})}}
"#,
        ks.join(", ")
    );
    let r = 跑_关(&src, 按材料, vec![]).unwrap(); // 数验题调用次数，固定关伴随题（主控 2026-09-30：第二类）
    let v = r.out.value_json();
    assert_eq!(
        r.calls,
        acts.len() + igns.len(),
        "每条验题一次调用（各自的材料）"
    );
    v
}

#[test]
fn a_是非题_gap_够_过() {
    let v = 是非(&[0.8, 0.85, 0.9], &[0.1, 0.2, 0.3], "");
    assert_eq!(v, json!({"pass": 1, "rejected": []}));
}

#[test]
fn b_是非题_gap_不足_拒() {
    // min(act) − max(ignore) = 0.5 − 0.4 = 0.1 < 0.20
    let v = 是非(&[0.5, 0.85, 0.9], &[0.1, 0.2, 0.4], "");
    assert_eq!(
        v,
        json!({"pass": 0, "rejected": [{"stage": "known", "codes": ["known-gap"]}]})
    );
}

#[test]
fn c_是非题_验题不足三道_拒() {
    let v = 是非(&[0.8, 0.9], &[0.1, 0.2, 0.3], "");
    assert_eq!(
        v,
        json!({"pass": 0, "rejected": [{"stage": "known", "codes": ["known-count"]}]})
    );
}

#[test]
fn d_是非题_safety_门槛_030() {
    // gap = 0.65 − 0.4 = 0.25：缺省（0.20）过，safety（0.30）拒
    let acts = [0.65, 0.8, 0.9];
    let igns = [0.1, 0.2, 0.4];
    assert_eq!(是非(&acts, &igns, "")["pass"], json!(1));
    let v = 是非(&acts, &igns, ", known_gate: {safety: true}");
    assert_eq!(
        v,
        json!({"pass": 0, "rejected": [{"stage": "known", "codes": ["known-gap"]}]})
    );
}

/// 一个三档程度候选：每项是（期望档, 各档概率）
fn 程度(ks: &[(usize, [f64; 3])]) -> Json {
    let ks: Vec<String> = ks
        .iter()
        .map(|(e, p)| format!("{{on: mat(\"s={},{},{}\"), expect: {e}}}", p[0], p[1], p[2]))
        .collect();
    let src = format!(
        r#"budget {{calls: 20, cost: 0, depth: 512}};
let n = derive_node("x0", {{on: mat("甲的合同记录")}}, test("甲会续约吗？", "t0"), {{}});
let c = derive_child(n, unit, 0, measure("甲的交付有多准时？", ["常误期", "偶尔误期", "准时"], "t1"), {{by: "elicit"}});
let g = derive_gate([c], {{judge_diag: false, known: fn(x) {{ [{}] }}}});
{{pass: len(g.pass), rejected: map(g.rejected, fn(x) {{ {{stage: x.stage, codes: x.codes}} }})}}
"#,
        ks.join(", ")
    );
    let r = 跑(&src, 按材料, vec![]).unwrap();
    r.out.value_json()
}

#[test]
fn e_程度题_每档两道_期望档都对_过() {
    let v = 程度(&[
        (0, [0.9, 0.1, 0.0]),
        (0, [0.8, 0.2, 0.0]),
        (1, [0.1, 0.8, 0.1]),
        (1, [0.2, 0.7, 0.1]),
        (2, [0.0, 0.1, 0.9]),
        (2, [0.0, 0.2, 0.8]),
    ]);
    assert_eq!(v, json!({"pass": 1, "rejected": []}));
}

#[test]
fn f_程度题_有一道偏档_拒() {
    // 最后一道期望第 2 档，读数的期望档 = 0.1·1 + 0.0·2 ≈ 0.1，偏 1.9 > 0.5
    let v = 程度(&[
        (0, [0.9, 0.1, 0.0]),
        (0, [0.8, 0.2, 0.0]),
        (1, [0.1, 0.8, 0.1]),
        (1, [0.2, 0.7, 0.1]),
        (2, [0.0, 0.1, 0.9]),
        (2, [0.9, 0.1, 0.0]),
    ]);
    assert_eq!(
        v,
        json!({"pass": 0, "rejected": [{"stage": "known", "codes": ["known-level"]}]})
    );
}

#[test]
fn g_程度题_有一档不到两道_拒() {
    let v = 程度(&[
        (0, [0.9, 0.1, 0.0]),
        (0, [0.8, 0.2, 0.0]),
        (1, [0.1, 0.8, 0.1]),
        (2, [0.0, 0.1, 0.9]),
        (2, [0.0, 0.2, 0.8]),
    ]);
    assert_eq!(
        v,
        json!({"pass": 0, "rejected": [{"stage": "known", "codes": ["known-count"]}]})
    );
}
