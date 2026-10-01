//! Z0860：派生前提三层——①代码能定的代码定（不进判断器，账本记 by: code）、②生成器看样本取值、
//! ③判断类前提先在抽样上检验区分度，全同一侧弃掉记 no_split 再派生。
//! 预注册：地基/过程记录/工程-Z0860-前提三层.md 第四节与第六节附录。闭包端口，不发请求；任务是虚构的「挑供应商」。
//! 数题按题面全等（伴随题开时每道题另带元题）。

mod derive_support;
use derive_support::*;
use jpp::ledger::Entry;
use jpp::value::{Answer, Question, State};
use serde_json::{Value as Json, json};
use std::cell::RefCell;

const 目的: &str = "在这些供应商里找出能按期交付我们这份订单的。";
const 深判题: &str = "这家供应商能在六周内交付吗？";
const 全满足题: &str = "材料里写明了这家供应商的名字吗？";
const 分裂题: &str = "材料里写明了这家供应商做过汽车零件吗？";

/// n 家：kind 按 i mod 3 取 汽车 / 家电 / 玩具；weeks = 4 + (i mod 5)
fn 条目(n: usize) -> String {
    let kinds = ["汽车", "家电", "玩具"];
    let xs: Vec<String> = (0..n)
        .map(|i| {
            format!(
                "{{on: mat({{name: \"供应商{i}\", kind: \"{}\", weeks: {}}})}}",
                kinds[i % 3],
                4 + i % 5
            )
        })
        .collect();
    format!("[{}]", xs.join(", "))
}

fn 程序(n: usize) -> String {
    程序_预算(n, 4000)
}

fn 程序_预算(n: usize, calls: usize) -> String {
    format!(
        "import \"../../lib/derive/purpose.jpp\";\nbudget {{calls: {calls}, cost: 0, depth: 8192}};\nlet r = purpose_run(\"{目的}\", {}, {{}});\n{}",
        条目(n),
        "{value: r.value, pending: map(r.pending, fn(p) { {cause: p.cause, via: p.via, exit: p.exit} }), evidence: r.evidence, detail: r.detail}"
    )
}

fn 模块() -> Json {
    json!({"material": "供应商", "predicates": [{"text": "这家供应商能按期交付", "cut": "binary", "cut_from": "purpose", "request": "one", "field": "ontime"}],
           "request": null, "presupposition": null, "context": null, "reference": null, "budget": null, "unsure": null, "done": ["ontime"]})
}

fn kind(s: &State) -> String {
    s.on.first()
        .and_then(|m| m.content.get("kind"))
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string()
}

fn 读数(t: &str, _q: &Question, s: &State) -> Answer {
    if t.contains("需要分别回答的判断") {
        Answer::Noul(0.1)
    } else if t.contains("这段材料里有没有") {
        Answer::Noul(0.9)
    } else if t.contains("已认证的题问的是同一件事") {
        let k = s.over.len();
        let mut v = vec![0.0; k];
        v[k - 1] = 1.0;
        Answer::Choice(v)
    } else if t == 全满足题 {
        Answer::Noul(0.9)
    } else if t == 分裂题 {
        Answer::Noul(if kind(s) == "汽车" { 0.9 } else { 0.1 })
    } else {
        Answer::Noul(0.8)
    }
}

struct 跑记 {
    r: 跑出,
    v: Json,
    prompts: Vec<String>,
}

/// 前提派生每一轮依次给 rounds[i]（超出的轮给最后一个）
fn 跑(n: usize, rounds: Vec<Vec<Json>>) -> 跑记 {
    跑_全(&程序(n), 读数, rounds)
}

fn 跑_全(
    src: &str,
    answer: fn(&str, &Question, &State) -> Answer,
    rounds: Vec<Vec<Json>>,
) -> 跑记 {
    let prompts = RefCell::new(Vec::<String>::new());
    let k = RefCell::new(0usize);
    let r = 跑_按提示(src, answer, |p: &str| {
        if p.contains("字面前提题") {
            prompts.borrow_mut().push(p.to_string());
            let i = *k.borrow();
            *k.borrow_mut() += 1;
            rounds[i.min(rounds.len() - 1)].clone()
        } else if p.starts_with("下面是一段目的") {
            vec![模块()]
        } else {
            vec![json!({"op": "test", "text": 深判题})]
        }
    })
    .unwrap_or_else(|e| panic!("{e}"));
    let v = r.out.value_json();
    跑记 {
        r,
        v,
        prompts: prompts.into_inner(),
    }
}

fn 次数(r: &跑出, t: &str) -> usize {
    r.asked.iter().filter(|x| x.as_str() == t).count()
}

fn 代码变换(r: &跑出) -> Vec<(String, Json)> {
    r.ledger
        .entries
        .iter()
        .filter_map(|e| match e {
            Entry::Effect {
                key, kind, output, ..
            } if kind == "transform" && output.get("by") == Some(&json!("code")) => {
                Some((key.clone(), output.clone()))
            }
            _ => None,
        })
        .collect()
}

/// 预测 4.3-1：代码谓词零判断，账本一条变换带 by: code，淘汰行追得到那条变换
#[test]
fn 代码谓词_零判断_账本记by_code_淘汰行可追() {
    let x = 跑(
        6,
        vec![vec![
            json!({"op": "code", "field": "kind", "check": "eq", "value": "汽车"}),
        ]],
    );
    let (r, v) = (&x.r, &x.v);
    // 前提层没有一道判断：判断端口只收到深判题（纳入的 2 家）与闸门、查题库那些
    assert!(
        r.asked.iter().all(|t| !t.starts_with("材料里写明")),
        "{:?}",
        r.asked
    );
    assert_eq!(次数(r, 深判题), 2, "{:?}", r.asked);
    let tf = 代码变换(r);
    assert_eq!(tf.len(), 1, "{tf:?}");
    assert_eq!(
        tf[0].1["exits"],
        json!(["act", "ignore", "ignore", "act", "ignore", "ignore"]),
        "{tf:?}"
    );
    let rows = v["value"].as_array().unwrap();
    assert_eq!(rows.len(), 6);
    for i in [1, 2, 4, 5] {
        assert_eq!(rows[i]["excluded"], json!(true), "{v}");
        let m = rows[i]["premise"]
            .as_array()
            .unwrap()
            .last()
            .unwrap()
            .clone();
        assert_eq!(m["by"], "code", "{v}");
        assert_eq!(m["exit"], "ignore", "{v}");
        assert_eq!(m["source_key"], json!(tf[0].0), "淘汰行追到那条变换：{v}");
    }
    for i in [0, 3] {
        assert_eq!(rows[i]["fields"]["ontime"], json!(true), "{v}");
        assert!(rows[i].get("excluded").is_none(), "{v}");
    }
    let d = &v["detail"]["premise"];
    assert_eq!(d["excluded"], json!(4), "{d}");
    assert_eq!(d["code"][0]["key"], json!(tf[0].0), "{d}");
    assert_eq!(d["layers"][0]["by"], "code", "{d}");
    assert_eq!(d["layers"][0]["asked"], json!(0), "{d}");
    assert_eq!(d["reason"], "derived", "{d}");
}

/// 代码谓词的几种检查：数值比较、列表/文字包含、字段缺而要取值记 unknown（存活）
#[test]
fn 代码谓词_比较与包含() {
    // weeks = 4 + i mod 5：le 5 → 0、1、5 满足
    let x = 跑(
        6,
        vec![vec![
            json!({"op": "code", "field": "weeks", "check": "le", "value": 5}),
        ]],
    );
    assert_eq!(
        代码变换(&x.r)[0].1["exits"],
        json!(["act", "act", "ignore", "ignore", "ignore", "act"])
    );
    let x = 跑(
        6,
        vec![vec![
            json!({"op": "code", "field": "name", "check": "contains", "value": "商1"}),
        ]],
    );
    assert_eq!(
        代码变换(&x.r)[0].1["exits"],
        json!(["ignore", "act", "ignore", "ignore", "ignore", "ignore"])
    );
    // 数值比较遇到文字：unknown，项存活，不算一侧；全 unknown 的没有一项已决，不弃
    let x = 跑(
        6,
        vec![vec![
            json!({"op": "code", "field": "kind", "check": "gt", "value": 3}),
        ]],
    );
    assert_eq!(代码变换(&x.r)[0].1["exits"], json!(vec!["unknown"; 6]));
    let rows = x.v["value"].as_array().unwrap();
    assert!(
        rows.iter()
            .all(|r| r.get("excluded").is_none() && r["fields"]["ontime"] == json!(true)),
        "{}",
        x.v
    );
}

/// 预测 4.3-2：核不过的代码谓词记 code-bad，不执行
#[test]
fn 代码谓词核不过_记code_bad_不执行() {
    for bad in [
        json!({"op": "code", "field": "city", "check": "has"}),
        json!({"op": "code", "field": "kind", "check": "like", "value": "汽"}),
        json!({"op": "code", "field": "kind", "check": "eq"}),
    ] {
        let x = 跑(6, vec![vec![bad.clone()]]);
        let d = &x.v["detail"]["premise"];
        assert_eq!(d["rejected"][0]["codes"], json!(["code-bad"]), "{bad} {d}");
        assert_eq!(d["reason"], "none-passed", "{d}");
        assert!(代码变换(&x.r).is_empty(), "{bad}");
        // 全被拒不再派生
        assert_eq!(x.prompts.len(), 1, "{bad}");
    }
}

/// 预测 4.3-3：样本取值进提示（N = 12：k = 5，步长 2，取第 0、2、4、6、8 项）
#[test]
fn 样本取值进提示_等距() {
    let x = 跑(
        12,
        vec![vec![
            json!({"op": "code", "field": "kind", "check": "eq", "value": "汽车"}),
        ]],
    );
    let p = &x.prompts[0];
    assert!(
        p.contains("字段：") && p.contains("kind") && p.contains("weeks"),
        "{p}"
    );
    for i in [0, 2, 4, 6, 8] {
        assert!(
            p.contains(&format!("\"供应商{i}\"")),
            "第 {i} 项应在样本里：{p}"
        );
    }
    for i in [1, 3, 10, 11] {
        assert!(
            !p.contains(&format!("\"供应商{i}\"")),
            "第 {i} 项不该在样本里：{p}"
        );
    }
}

/// 预测 4.3-4：N = 40、n = 20。全满足题在样本上全 act → no_split 弃掉、再派生；分裂题进全量层，样本 20 次被复用
#[test]
fn 抽样检验_no_split弃掉再派生_全量层复用样本() {
    let x = 跑(
        40,
        vec![
            vec![json!({"op": "test", "text": 全满足题})],
            vec![json!({"op": "test", "text": 分裂题})],
        ],
    );
    let (r, v) = (&x.r, &x.v);
    assert_eq!(x.prompts.len(), 2, "再派生一次");
    assert!(
        x.prompts[1].contains(全满足题),
        "第二轮提示带着被弃的前提：{}",
        x.prompts[1]
    );
    assert_eq!(次数(r, 全满足题), 20, "全满足题只在 20 条样本上判");
    assert_eq!(
        次数(r, 分裂题),
        40,
        "分裂题全量 40 次，样本那 20 次被复用，不是 60"
    );
    let d = &v["detail"]["premise"];
    assert_eq!(
        d["dropped"][0],
        json!({"q": 全满足题, "reason": "no_split", "by": "judge", "sample": 20, "act": 20, "ignore": 0, "unsure": 0}),
        "{d}"
    );
    assert_eq!(d["rounds"], json!(2), "{d}");
    assert_eq!(d["passed"], json!([分裂题]), "{d}");
    // kind 按 i mod 3：汽车 14 家（0、3、…、39）
    assert_eq!(d["excluded"], json!(26), "{d}");
    assert_eq!(次数(r, 深判题), 14, "{:?}", r.asked.len());
    // 生成器：抽模块 1 + 前提 2 + 唤出 1
    assert_eq!(r.gens, 4);
}

/// 预测 4.3-5：两轮都全 no_split，全部纳入，reason no-split，前提生成恰 2 次
#[test]
fn 两轮都没有区分度_全部纳入() {
    let x = 跑(
        40,
        vec![vec![
            json!({"op": "test", "text": 全满足题}),
            json!({"op": "code", "field": "name", "check": "has"}),
        ]],
    );
    let v = &x.v;
    assert_eq!(x.prompts.len(), 2);
    let d = &v["detail"]["premise"];
    assert_eq!(d["reason"], "no-split", "{d}");
    assert_eq!(d["excluded"], json!(0), "{d}");
    // 每轮各弃两道（代码一道、判断一道）
    assert_eq!(d["dropped"].as_array().unwrap().len(), 4, "{d}");
    assert!(
        d["dropped"]
            .as_array()
            .unwrap()
            .iter()
            .any(|x| x["by"] == "code" && x["sample"] == json!(40) && x["act"] == json!(40)),
        "{d}"
    );
    // 第二轮同题同样本同键复用：全满足题只付 20 次
    assert_eq!(次数(&x.r, 全满足题), 20);
    let rows = v["value"].as_array().unwrap();
    assert!(
        rows.iter()
            .all(|r| r.get("premise").is_none() && r["fields"]["ontime"] == json!(true)),
        "{v}"
    );
    assert_eq!(次数(&x.r, 深判题), 40);
}

/// 预注册 4.1 的离线回放形状（线 A 臂 3）：两道「写明某字段」前提，记录都有该字段。
/// 写成代码谓词时前提层判断 0 次；写成是非题时只在样本上各判 20 次（原做法 2 × N）
#[test]
fn 臂3形状回放_字段齐全的前提() {
    let x = 跑(
        400,
        vec![vec![
            json!({"op": "code", "field": "kind", "check": "has"}),
            json!({"op": "code", "field": "weeks", "check": "has"}),
        ]],
    );
    assert!(x.r.asked.iter().all(|t| !t.starts_with("材料里写明")));
    assert_eq!(x.v["detail"]["premise"]["reason"], "no-split");
    let y = 跑(
        400,
        vec![vec![
            json!({"op": "test", "text": 全满足题}),
            json!({"op": "test", "text": "材料里写明了交付周数吗？"}),
        ]],
    );
    let 前提次数: usize =
        y.r.asked
            .iter()
            .filter(|t| t.starts_with("材料里写明"))
            .count();
    assert_eq!(前提次数, 40, "两道各在 20 条样本上判一次，第二轮同键复用");
    assert_eq!(y.v["detail"]["premise"]["reason"], "no-split");
}

/// 复核返修：抽样时预算停发，缺席类探测未决不能 drop（E-drop-unobserved），随返回值转交；运行跑完
#[test]
fn 抽样时预算停发_不中止_缺席类转交() {
    for calls in [3usize, 10, 25] {
        let x = 跑_全(
            &程序_预算(40, calls),
            读数,
            vec![vec![json!({"op": "test", "text": 分裂题})]],
        );
        let p = x.v["pending"].as_array().unwrap();
        assert!(
            p.iter().any(|e| e["cause"] == "budget"),
            "calls {calls}：{}",
            x.v["pending"]
        );
    }
    // 对照面：预算充足时没有探测转交
    let x = 跑(40, vec![vec![json!({"op": "test", "text": 分裂题})]]);
    let p = x.v["pending"].as_array().unwrap();
    assert!(
        !p.iter().any(|e| e["via"]
            .as_array()
            .is_some_and(|a| a.iter().any(|v| v == "premise-probe"))),
        "{}",
        x.v["pending"]
    );
}

/// 样本里第 0 家读数恰 0.5：留下的分裂题，第 0 家存活、出口随 pending 转交（select#0）；账本对这个键是 Handoff、没有 Drop。
/// 被弃的全满足题在第 0 家也是 0.5：账本对那个键是 Drop，pending 里没有它
fn 读数_第0家并列(t: &str, q: &Question, s: &State) -> Answer {
    let first =
        s.on.first()
            .and_then(|m| m.content.get("name"))
            .and_then(|v| v.as_str())
            == Some("供应商0");
    if first && (t == 分裂题 || t == 全满足题) {
        Answer::Noul(0.5)
    } else {
        读数(t, q, s)
    }
}

#[test]
fn 样本未决_账本与pending对得上() {
    let x = 跑_全(
        &程序(40),
        读数_第0家并列,
        vec![vec![
            json!({"op": "test", "text": 全满足题}),
            json!({"op": "test", "text": 分裂题}),
        ]],
    );
    let (r, v) = (&x.r, &x.v);
    assert_eq!(
        v["detail"]["premise"]["passed"],
        json!([分裂题]),
        "{}",
        v["detail"]["premise"]
    );
    let row0 = &v["value"][0];
    assert!(
        row0["premise"][0]["exit"]
            .as_str()
            .unwrap()
            .starts_with("unsure"),
        "{row0}"
    );
    // 前提题在第 0 家上的判断键：账本里 Judge 条目，状态是第 0 家、题是那道前提
    let judge_keys: Vec<String> = r
        .ledger
        .entries
        .iter()
        .filter_map(|e| match e {
            Entry::Judge { key, .. } => Some(key.clone()),
            _ => None,
        })
        .collect();
    let drops: Vec<&Vec<String>> = r
        .ledger
        .entries
        .iter()
        .filter_map(|e| match e {
            Entry::Drop { of, .. } => Some(of),
            _ => None,
        })
        .collect();
    let handoffs: Vec<&Vec<String>> = r
        .ledger
        .entries
        .iter()
        .filter_map(|e| match e {
            Entry::Handoff { of, .. } => Some(of),
            _ => None,
        })
        .collect();
    let in_drop = |k: &str| drops.iter().any(|of| of.iter().any(|x| x == k));
    let in_handoff = |k: &str| handoffs.iter().any(|of| of.iter().any(|x| x == k));
    // 留下的分裂题：第 0 家那条出口的键在 pending 的 premise 标记里，账本里是 Handoff、不是 Drop
    let k_split = row0["premise"][0]["source_key"]
        .as_str()
        .unwrap()
        .to_string();
    assert!(judge_keys.contains(&k_split));
    assert!(
        in_handoff(&k_split),
        "留下的前提的样本未决应转交：{k_split}"
    );
    assert!(
        !in_drop(&k_split),
        "留下的前提的样本未决不该 drop：{k_split}"
    );
    assert!(
        v["pending"].as_array().unwrap().iter().any(|e| e["via"]
            .as_array()
            .is_some_and(|a| a.iter().any(|x| x == "select#0"))),
        "{}",
        v["pending"]
    );
    // 被弃的全满足题：恰有一条 Drop（第 0 家那条），pending 里没有 premise-probe
    assert_eq!(
        drops.iter().map(|of| of.len()).sum::<usize>(),
        1,
        "{drops:?}"
    );
    assert!(!in_handoff(&drops[0][0]));
    assert!(!v["pending"].as_array().unwrap().iter().any(|e| {
        e["via"]
            .as_array()
            .is_some_and(|a| a.iter().any(|x| x == "premise-probe"))
    }));
}
