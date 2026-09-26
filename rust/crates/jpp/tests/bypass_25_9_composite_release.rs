//! 步 25-9：合成出口的放行取全部分量放行之合取，谱系穿过 `parts`；`E-tally-no-exit`（B140）；`first_k`
//! 吸收全部未决分量（B141）。末两条是比赛现场那条链：判出来的图、搜索的产物在每个分量都放行时守不可逆动作。
//!
//! 依据：`12` §2.3 B131、B140、B141；B72-4；B128/20j-2；预注册 `地基/过程记录/工程-步25-9.md` §一·2。

mod common;

use jpp::effects::{CalibStore, CertGrade, FnPort, JudgeResult, Ports};
use jpp::interp::{ActionRegistry, TaintOut};
use jpp::ledger::Ledger;
use jpp::value::{Answer, Question, State, Taint, Value};
use jpp::{lower, syntax::parse};
use serde_json::Value as Json;

/// 题面含「未决」给 0.5（落带内），含「不」给 0.05，其余 0.95；`ask` 恒答 0.95
fn 端口<'a>() -> Ports<'a> {
    Ports::new()
        .with(FnPort::judge("m", |_s: &State, qs: &[&Question]| {
            Ok(JudgeResult {
                answers: qs
                    .iter()
                    .map(|q| {
                        Answer::Noul(if q.text.contains("未决") {
                            0.5
                        } else if q.text.contains("不") {
                            0.05
                        } else {
                            0.95
                        })
                    })
                    .collect(),
                tokens: 0,
                cost: 0.0,
                mode_share: vec![None; qs.len()],
                perms: vec![0; qs.len()],
                confidence: vec![],
            })
        }))
        .with(FnPort::ask("m", |_s, _q| Ok(Some(Answer::Noul(0.95)))))
}

/// 正式线 k、试用线 t、夹具线 fx
fn 库() -> CalibStore {
    let mut c = CalibStore::new();
    common::certified(&mut c, "k", 0.8, 0.2, 50);
    common::certified(&mut c, "t", 0.8, 0.2, 50);
    for cert in c.records.get_mut("t").unwrap().certs.values_mut() {
        cert.grade = CertGrade::Trial;
    }
    c.put("fx", 0.8, 0.2, 50, "上岗", Some(0.05)).unwrap();
    c
}

fn 动作表() -> ActionRegistry {
    let mut a = ActionRegistry::new();
    jpp::actions::register_all(&mut a, &jpp::actions::Ctx::default(), false);
    a.register("发邮件", 0.0, false, TaintOut::Trusted, |_| {
        Ok(Value::Text("已发".into(), Taint::Trusted.into()))
    });
    a
}

fn 跑(src: &str) -> Result<Json, String> {
    let program = lower(&parse(src).unwrap_or_else(|e| panic!("解析：{e:?}"))).expect("lower");
    let mut l = Ledger::new();
    jpp::run(&program, 端口(), &库(), &动作表(), &mut l)
        .map(|o| o.value_json())
        .map_err(|e| e.render())
}

fn 会话跑(src: &str, 接受: bool) -> Result<Json, String> {
    let entry = jpp::EntryArgs {
        accept: jpp::HostAccept {
            declared_lines: 接受,
        },
        ..Default::default()
    };
    let program = jpp::Session::compile(&parse(src).expect("解析"), &entry.decl())
        .map_err(|e| format!("{e:?}"))?;
    let (c, a) = (库(), 动作表());
    let mut l = Ledger::new();
    jpp::Session::new(端口(), &c, &a)
        .run(&program, &entry, &mut l)
        .map(|o| o.value_json())
        .map_err(|e| e.render())
}

const 发: &str = r#"{act: fn() { content(do("发邮件", [], 0)) }, ignore: fn() { "不发" }, unsure: fn(u) { consume(u, "drop"); "不发" }}"#;

#[test]
fn a_没有分量的合成出口不放行() {
    let e = 跑(&format!(
        "budget {{calls: 4, cost: 1, depth: 8}};\nhandle(compose([], \"all\"), {发})\n"
    ))
    .expect_err("空分量集合不取真（B140）");
    assert!(e.contains("J-08"), "{e}");
}

#[test]
fn b_含_ask_分量的合成出口放行() {
    let v = 跑(&format!(
        r#"budget {{calls: 4, cost: 1, depth: 8, escalate: 2}};
let s = state(mat("一段程序自己写的材料"));
let a = cut(judge(s, test("该发吗", "k")));
let h = ask(s, test("该发吗", "k"));
handle(compose([a, h], "all"), {发})
"#
    ))
    .expect("正式线与人答都放行");
    assert_eq!(v, Json::from("已发"));
}

const 声明: &str = r#"budget {calls: 4, cost: 1, depth: 8};
let s = state(mat("一段程序自己写的材料"));
let a = cut(judge(s, test("该发吗", "k")));
let d = cut(judge(s, test("该发吗二", "k")), {declare: {hi: 0.7}});
let x = compose([a, d], "all");
{sent: handle(x, 发臂), unknown: cert(x).n_unknown}
"#;

#[test]
fn c_声明线分量随宿主接受放行() {
    let src = 声明.replace("发臂", 发);
    let e = 会话跑(&src, false).expect_err("不带 --release-on-declared 不放行");
    assert!(e.contains("J-08"), "{e}");
    let v = 会话跑(&src, true).expect("宿主接受声明线：两个分量都放行");
    assert_eq!(v["sent"], Json::from("已发"));
    // 放行与误差界分开：声明线分量仍按 1 计入 n_unknown
    assert_eq!(v["unknown"], Json::from(1));
}

#[test]
fn d_tally_收无出口成员报错() {
    let e = 跑(r#"budget {calls: 4, cost: 1, depth: 8};
tally(outcome({value: ["甲", "乙"], detail: {ignore: []}}))
"#)
    .expect_err("B140");
    assert!(e.contains("E-tally-no-exit"), "{e}");
}

#[test]
fn e_first_k_被挡时吸收未决_只交出自己的出口() {
    let v = 跑(r#"budget {calls: 4, cost: 1, depth: 8};
let r = sieve(["未决的一段", "确定的一段"], test("这一段未决吗", "k"));
let f = first_k(r, 1);
{n: len(f.pending), exit: exit_kind(f.value.exit), pending: f.pending}
"#)
    .expect("元素的未决并入 first_k 的出口，只交出这一条也有去向");
    assert_eq!(v["n"], Json::from(1));
    assert_eq!(v["exit"], Json::from("unsure(band)"));
}

fn 去import(s: &str) -> String {
    s.lines()
        .filter(|l| !l.starts_with("import "))
        .collect::<Vec<_>>()
        .join("\n")
}

/// 判出来的图：2 × 2 二部图，边题用 `边键`；已决匹配的第一对守一个不可逆动作
fn 图程序(边键: &str) -> String {
    format!(
        r#"budget {{calls: 20, cost: 1, depth: 64}};
{lib}
let left = ["任务：写登录页", "任务：写接口文档"];
let right = ["小林：前端三年", "小吴：技术写作"];
let g = judged_bipartite(left, right, test("b 这个人能接下 a 这项任务吗？", "{边键}"), {{prune: fn(l, r) {{ [[0, 0], [1, 1]] }}}});
let t = interval(g, "matching", {{}});
{{sent: handle(t.lo.value[0].exit, {发}), pending: g.pending}}
"#,
        lib = 去import(include_str!("../../../lib/compose/graph.jpp"))
    )
}

#[test]
fn f_判出来的图_分量都放行时产物守不可逆动作() {
    let v = 跑(&图程序("k")).expect("两条边都是正式线：分工产物放行");
    assert_eq!(v["sent"], Json::from("已发"));
    let e = 跑(&图程序("t")).expect_err("边是试用线：产物不放行");
    assert!(e.contains("J-08"), "{e}");
}

#[test]
fn g_搜索产物_分量都放行时守不可逆动作() {
    let lib = format!(
        "{}\n{}\n{}",
        去import(include_str!("../../../lib/outcome.jpp")),
        去import(include_str!("../../../lib/compose/carry.jpp")),
        去import(include_str!("../../../lib/compose/search.jpp"))
    );
    let 程序 = |键: &str| {
        format!(
            r#"budget {{calls: 20, cost: 1, depth: 64}};
{lib}
let propose = fn(front, i) {{ map(["方案甲", "方案乙"], fn(k) {{ mat(join([content(front[0]), "：", k], "")) }}) }};
let o = search([mat("给登录页选一个实现")], propose, test("这个方案可行吗？", "{键}"), unit, 2, {{width: 1}});
{{sent: handle(o.value[0].exit, {发}), pending: o.pending}}
"#
        )
    };
    let v = 跑(&程序("k")).expect("搜出来、判过了（正式线）、就执行");
    assert_eq!(v["sent"], Json::from("已发"));
    let e = 跑(&程序("t")).expect_err("试用线判的：不放行");
    assert!(e.contains("J-08"), "{e}");
}
