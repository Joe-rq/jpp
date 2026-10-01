//! 步 28（B0469）：先选后填。`select` 的候选带签名、是非题的已决块带签名（裁定十九）时派生参数题；
//! 派生题记来源出口、hop 加一；没挂签名的块、可选值不足两项的参数不派生。闭包端口。

mod derive_support;
use derive_support::*;
use jpp::value::Answer;
use serde_json::{Value as Json, json};

fn noul(p: f64) -> Answer {
    Answer::Noul(p)
}

/// 闸门第③段两道诊断题：一律「没藏两个判断」「问的东西在材料里」
fn 诊断(text: &str) -> Option<Answer> {
    if text.contains("需要分别回答的判断") {
        Some(noul(0.1))
    } else if text.contains("这段材料里有没有") {
        Some(noul(0.9))
    } else {
        None
    }
}

#[test]
fn a_select_候选带签名_选中后填参数_再出完整的题() {
    let src = r#"budget {calls: 20, cost: 0, depth: 512};
let f = form("test", "a 与 b 在{城市}有共同客户吗？", {calib: "t-f"});
let sig = {form: f, params: [{slot: "城市", choices: ["上海", "杭州"]}]};
let first = fn(item) { {q: select("这份材料主要讲哪类合作？", "t-root"), over: ["客户合作", "技术合作"],
                        cands: [{name: "客户合作", sig: sig}, "技术合作"]} };
let r = chain([{on: mat("甲乙丙的合作记录")}], first, 4, {});
"#
    .to_string()
        + 读出;
    let r = 跑(
        &src,
        |t, _q, _s| {
            诊断(t).unwrap_or_else(|| {
                if t.contains("（城市）填哪一项") {
                    Answer::Choice(vec![0.3, 0.7])
                } else if t.contains("杭州有共同客户") {
                    noul(0.9)
                } else if t.contains("哪类合作") {
                    Answer::Choice(vec![0.8, 0.2])
                } else {
                    panic!("夹具没有：{t}")
                }
            })
        },
        vec![],
    )
    .unwrap();
    let v: Json = r.out.value_json();
    assert!(
        r.asked.iter().any(|t| t == "a 与 b 在杭州有共同客户吗？"),
        "{:?}",
        r.asked
    );
    assert_eq!(v["hops"], json!(3), "{v}");
    assert_eq!(
        v["value"][0]["path"],
        json!(["author:pick(0)", "fill:pick(1)", "fill:act"])
    );
    // 账本：第 3 跳那道题的 parents 含第 2 跳填空题的出口键，hop 3
    let js = 判断条目(&r.ledger);
    let max = js.iter().map(|j| j.3).max().unwrap();
    assert_eq!(max, 3);
    let top = js.iter().find(|j| j.3 == 3).unwrap();
    assert!(
        js.iter().any(|j| j.3 == 2 && top.2.contains(&j.0)),
        "第 3 跳的 parents 含第 2 跳的键：{js:?}"
    );
}

const 是非签名: &str = r#"budget {calls: 20, cost: 0, depth: 512};
let why = {params: [{slot: "主要原因", choices: ["价格", "服务"]}]};
let items = [{on: mat("客户甲：续约")}, {on: mat("客户乙：不续约")}];
"#;

fn 是非夹具(t: &str, _q: &jpp::value::Question, s: &jpp::value::State) -> Answer {
    诊断(t).unwrap_or_else(|| {
        if t.contains("主要原因最可能是哪一项") {
            Answer::Choice(vec![0.8, 0.2])
        } else if t.contains("会续约吗") {
            noul(if s.on_text().contains("不续约") {
                0.2
            } else {
                0.8
            })
        } else {
            panic!("夹具没有：{t}")
        }
    })
}

#[test]
fn b_是非题_act_块挂签名_派生_没挂签名的块不派生() {
    let src = 是非签名.to_string()
        + r#"let root = form("test", "这位客户会续约吗？", {calib: "t-root", on: {act: why}});
let first = fn(item) { {form: root} };
let r = chain(items, first, 4, {});
"# + 读出;
    let r = 跑_关(&src, 是非夹具, vec![]).unwrap();
    let v: Json = r.out.value_json();
    let paths: Vec<Json> = v["value"]
        .as_array()
        .unwrap()
        .iter()
        .map(|x| x["path"].clone())
        .collect();
    assert!(
        paths.contains(&json!(["author:act", "fill:pick(0)"])),
        "{v}"
    );
    assert!(
        paths.contains(&json!(["author:ignore"])),
        "ignore 块没挂签名，成叶：{v}"
    );
    assert_eq!(
        r.asked.iter().filter(|t| t.contains("主要原因")).count(),
        1,
        "只有判出 act 的那一个派生"
    );
    // 派生题面由已决块起头（裁定十九：判出哪块即选中哪块）
    assert!(
        r.asked
            .iter()
            .any(|t| t.starts_with("已判「这位客户会续约吗？」为是；")),
        "{:?}",
        r.asked
    );
}

#[test]
fn c_是非题_ignore_块也可以挂签名() {
    let src = 是非签名.to_string()
        + r#"let root = form("test", "这位客户会续约吗？", {calib: "t-root", on: {ignore: why}});
let first = fn(item) { {form: root} };
let r = chain(items, first, 4, {});
"# + 读出;
    let r = 跑(&src, 是非夹具, vec![]).unwrap();
    let v: Json = r.out.value_json();
    let paths: Vec<Json> = v["value"]
        .as_array()
        .unwrap()
        .iter()
        .map(|x| x["path"].clone())
        .collect();
    assert!(
        paths.contains(&json!(["author:ignore", "fill:pick(0)"])),
        "{v}"
    );
    assert!(paths.contains(&json!(["author:act"])), "{v}");
    assert!(
        r.asked
            .iter()
            .any(|t| t.starts_with("已判「这位客户会续约吗？」为否；"))
    );
}

#[test]
fn d_可选值不足两项_不派生_选中的出口成叶() {
    let src = r#"budget {calls: 20, cost: 0, depth: 512};
let sig = {params: [{slot: "城市", choices: ["上海"]}]};
let first = fn(item) { {q: select("这份材料主要讲哪类合作？", "t-root"), over: ["客户合作", "技术合作"],
                        cands: [{name: "客户合作", sig: sig}, "技术合作"]} };
let r = chain([{on: mat("甲乙丙的合作记录")}], first, 4, {});
"#
    .to_string()
        + 读出;
    let r = 跑(&src, |_t, _q, _s| Answer::Choice(vec![0.8, 0.2]), vec![]).unwrap();
    let v: Json = r.out.value_json();
    assert_eq!(r.calls, 1, "只问了作者的题");
    assert_eq!(v["value"][0]["path"], json!(["author:pick(0)"]));
    assert_eq!(v["pending"], json!([]));
}

/// 签名写在题式上，跨程序复用（裁定十九、B192）：一个文件定义带签名的题式，两个不同的程序 import 它，
/// 各自的第一题判出 act 后都按这个签名派生同一道填空题（同一个派生题式）。
#[test]
fn e_题式上的签名_两个程序_import_都派生() {
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let shared = root.join(format!("target/derive-shared-{}.jpp", std::process::id()));
    std::fs::write(
        &shared,
        r#"let renew = form("test", "这位客户会续约吗？", {calib: "t-renew",
                  on: {act: {params: [{slot: "主要原因", choices: ["价格", "服务"]}]}}});
"#,
    )
    .unwrap();
    let name = shared.file_name().unwrap().to_str().unwrap().to_string();
    let prog = |items: &str| {
        format!(
            "import \"../{name}\";\nbudget {{calls: 20, cost: 0, depth: 512}};\nlet r = chain({items}, fn(item) {{ {{form: renew}} }}, 2, {{judge_diag: false}});\n"
        ) + 读出
    };
    let 夹具 = |t: &str, _q: &jpp::value::Question, _s: &jpp::value::State| {
        if t.contains("最可能是哪一项") {
            Answer::Choice(vec![0.7, 0.3])
        } else {
            Answer::Noul(0.8)
        }
    };
    let r1 = 跑(&prog("[{on: mat(\"客户甲的沟通记录\")}]"), 夹具, vec![]).unwrap();
    let r2 = 跑(
        &prog("[{on: mat(\"客户乙的邮件\")}, {on: mat(\"客户丙的邮件\")}]"),
        夹具,
        vec![],
    )
    .unwrap();
    let _ = std::fs::remove_file(&shared);
    let 派生键 = |r: &跑出| -> Vec<String> {
        判断引用(&r.ledger)
            .into_iter()
            .filter(|(_, c)| c.declared.starts_with("derive:"))
            .filter_map(|(_, c)| c.key)
            .collect()
    };
    let (k1, k2) = (派生键(&r1), 派生键(&r2));
    assert_eq!(k1.len(), 1, "程序一：一个条目派生一道填空题");
    assert_eq!(k2.len(), 2, "程序二：两个条目各派生一道");
    assert!(
        k2.iter().all(|k| *k == k1[0]),
        "两个程序派生出同一个题式：{k1:?} {k2:?}"
    );
    assert!(
        r2.asked
            .iter()
            .filter(|t| t.contains("主要原因最可能是哪一项"))
            .count()
            >= 1
    );
}
