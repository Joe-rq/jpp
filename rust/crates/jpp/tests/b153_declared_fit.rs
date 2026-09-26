//! 步 20j-4：声明式拟合 `fit({declare: f, tie?}, rs, extra?)`（B153 (2)(3)）。
//!
//! 第一段（运行期）：(a) 加权例、(b) 取大例、(c) `extra` 带外部数、(d)(d′) 闭包纯度的运行期半、(e) `order`、
//! (g) 守卫放行、(i) 谱系、(j) 输入不可用、(k) `confidence`、(l) 冷与选项、(m) `cuts`。检查期的半（(d)(d′)(f)(g)）
//! 随第二段。依据：`地基/附注/2026-09-26-批6裁定.md` §一；过程记录 `地基/过程记录/工程-步20j-4.md`。

mod common;

use jpp::effects::{CalibStore, FixedPorts, FnPort, JudgeResult, Ports};
use jpp::interp::{ActionRegistry, TaintOut};
use jpp::ledger::Ledger;
use jpp::value::{Answer, Mat, Op, Question, State, Taint, Value};
use jpp::{lower, syntax::parse};
use serde_json::{Value as Json, json};

/// 判断器（模型 `m`）：按「材料原文 + 题面」查答案，未列出的回 0.5；置换测过且一致；不报自报置信度
fn 端口<'a>(表: Vec<(&'static str, &'static str, Answer)>) -> Ports<'a> {
    Ports::new().with(FnPort::judge("m", move |s: &State, qs: &[&Question]| {
        let 原文 =
            s.on.first()
                .and_then(|m| m.content.as_str().map(String::from))
                .unwrap_or_default();
        Ok(JudgeResult {
            answers: qs
                .iter()
                .map(|q| {
                    表.iter()
                        .find(|(料, 题, _)| *料 == 原文 && *题 == q.text)
                        .map(|(_, _, a)| a.clone())
                        .unwrap_or(Answer::Noul(0.5))
                })
                .collect(),
            tokens: 0,
            cost: 0.0,
            mode_share: qs.iter().map(|_| Some(1.0)).collect(),
            perms: qs.iter().map(|_| 2).collect(),
            confidence: vec![],
        })
    }))
}

fn 动作表() -> ActionRegistry {
    let mut a = ActionRegistry::new();
    a.register("发退款", 0.0, false, TaintOut::Trusted, |_| {
        Ok(Value::Text("已退".into(), Taint::Trusted.into()))
    });
    a
}

fn 入口(接受: bool) -> jpp::EntryArgs {
    jpp::EntryArgs {
        accept: jpp::HostAccept {
            declared_lines: 接受,
        },
        // 本文件测声明式拟合的放行与告警：开 --guard（意图汇编 11a；W-declared-line 与放行只在把关下出现）
        guard: true,
        ..Default::default()
    }
}

fn 编译(src: &str, entry: &jpp::EntryArgs) -> jpp::Program {
    jpp::Session::compile(&parse(src).expect("解析"), &entry.decl()).expect("compile")
}

#[derive(Debug)]
struct 跑出 {
    value: Json,
    exits: Vec<Json>,
    warnings: Vec<String>,
    ledger: Ledger,
}

/// 经 `Session::run`（先检查、再执行）
fn 跑(
    src: &str,
    表: Vec<(&'static str, &'static str, Answer)>,
    calib: &CalibStore,
    接受: bool,
) -> Result<跑出, String> {
    let entry = 入口(接受);
    let a = 动作表();
    let mut l = Ledger::new();
    let o = jpp::Session::new(端口(表), calib, &a)
        .run(&编译(src, &entry), &entry, &mut l)
        .map_err(|e| e.render())?;
    Ok(跑出 {
        value: o.value_json(),
        exits: o.exits.clone(),
        warnings: o.trace.warnings.clone(),
        ledger: l,
    })
}

/// 跳过静态检查，只看运行期
fn 直跑(src: &str, 表: Vec<(&'static str, &'static str, Answer)>) -> Result<Json, String> {
    let mut program = lower(&parse(src).expect("解析")).expect("lower");
    program.entry.guard = true; // 测放行把关本身：开 --guard（意图汇编 11a）
    let c = CalibStore::new();
    let a = 动作表();
    let mut l = Ledger::new();
    jpp::run_unchecked(&program, 端口(表), &c, &a, &mut l)
        .map(|o| o.value_json())
        .map_err(|e| format!("[{}] {}", e.rule.clone().unwrap_or_default(), e.message))
}

fn 档(expect: f64) -> Answer {
    // 四档，期望档位 = expect：在相邻两档上分配概率
    let lo = expect.floor() as usize;
    let w = expect - lo as f64;
    let mut v = vec![0.0; 4];
    v[lo] = 1.0 - w;
    if w > 0.0 {
        v[lo + 1] = w;
    }
    Answer::Score(v)
}

const 加权: &str = r#"
budget {calls: 8, cost: 0, depth: 16};
let L = ["无", "低", "中", "高"];
fn 评(料) !{judge} {
    let s = state(mat(料));
    fit({declare: fn(a, b, c, d) { (0.4 * a.expect + 0.35 * b.expect + 0.25 * c.expect) / 3.0 * d.p }},
        [judge(s, measure("技能", L, "k1")), judge(s, measure("经历", L, "k2")),
         judge(s, measure("文化", L, "k3")), judge(s, test("在招", "k4"))])
}
let 分 = map(["甲", "乙"], 评);
{结论: map(分, fn(s) { exit_kind(cut(s, 线)) }), 排序: order(分)}
"#;

fn 加权表() -> Vec<(&'static str, &'static str, Answer)> {
    vec![
        ("甲", "技能", 档(2.4)),
        ("甲", "经历", 档(1.8)),
        ("甲", "文化", 档(2.0)),
        ("甲", "在招", Answer::Noul(0.9)),
        ("乙", "技能", 档(1.0)),
        ("乙", "经历", 档(1.2)),
        ("乙", "文化", 档(0.8)),
        ("乙", "在招", Answer::Noul(0.7)),
    ]
}

/// (a) 加权例：0.627 → act，0.238 → ignore；`order` 为 [[0], [1]]；报告行带 `fit`、`inputs`，等级 Declared
#[test]
fn a_加权例按手算() {
    let src = 加权.replace("线", "{declare: {hi: 0.6}}");
    let o = 跑(&src, 加权表(), &CalibStore::new(), false).unwrap();
    assert_eq!(o.value["结论"], json!(["act", "ignore"]));
    assert_eq!(o.value["排序"], json!([[0], [1]]));
    let row = &o.exits[0];
    assert_eq!(row["grade"], "Declared");
    assert_eq!(row["releases"], false);
    assert_eq!(row["inputs"], json!(["k1", "k2", "k3", "k4"]));
    assert!(row["fit"].as_str().is_some_and(|f| !f.is_empty()));
    assert_eq!(row["key"], format!("fit:{}", row["fit"].as_str().unwrap()));
    // 声明记录：同站点同拟合一条，带 fit 与 inputs
    let 记: Vec<(&String, &Json)> = o
        .ledger
        .calib_used
        .iter()
        .filter(|(k, _)| k.starts_with("declared:fit:"))
        .collect();
    assert_eq!(
        记.len(),
        1,
        "{:?}",
        o.ledger.calib_used.keys().collect::<Vec<_>>()
    );
    assert_eq!(记[0].1["record"]["inputs"], json!(["k1", "k2", "k3", "k4"]));
    assert_eq!(记[0].1["record"]["line"], "declared");
    let w: Vec<&String> = o
        .warnings
        .iter()
        .filter(|w| w.starts_with("W-declared-line"))
        .collect();
    assert_eq!(w.len(), 1, "{:?}", o.warnings);
    assert!(w[0].contains("fit="), "{}", w[0]);
}

/// (b) 取大例（snake 形）：`max(a.p, b.p)` 配 `{hi: 0.8}`
#[test]
fn b_取大例() {
    let src = r#"
budget {calls: 4, cost: 0, depth: 16};
let s = state(mat("甲"));
let f = fit({declare: fn(a, b) { max(a.p, b.p) }}, [judge(s, test("撞墙", "w")), judge(s, test("撞身", "b"))]);
exit_kind(cut(f, {declare: {hi: 0.8}}))
"#;
    let 表 = |x: f64| {
        vec![
            ("甲", "撞墙", Answer::Noul(0.3)),
            ("甲", "撞身", Answer::Noul(x)),
        ]
    };
    let c = CalibStore::new();
    assert_eq!(跑(src, 表(0.85), &c, false).unwrap().value, "act");
    assert_eq!(跑(src, 表(0.5), &c, false).unwrap().value, "ignore");
}

/// (c) `extra` 带外部数（jevymarket 形 `a.p - price`）；`extra` 带读数报 E-rt-arg
#[test]
fn c_extra带外部数() {
    let src = |价: &str| {
        format!(
            r#"
budget {{calls: 4, cost: 0, depth: 16}};
let r = judge(state(mat("甲")), test("会涨", "k"));
let f = fit({{declare: fn(a, price) {{ a.p - price }}}}, [r], [{价}]);
exit_kind(cut(f, {{declare: {{hi: 0.05}}}}))
"#
        )
    };
    let 表 = || vec![("甲", "会涨", Answer::Noul(0.72))];
    let c = CalibStore::new();
    assert_eq!(跑(&src("0.65"), 表(), &c, false).unwrap().value, "act");
    assert_eq!(跑(&src("0.7"), 表(), &c, false).unwrap().value, "ignore");
    let e = 跑(&src("r"), 表(), &c, false).expect_err("extra 带读数");
    assert!(
        e.contains("E-rt-arg") && e.contains("extra 只收数据"),
        "{e}"
    );
}

/// (d)(d′) 闭包纯度的运行期半：闭包里发判断报 E-fit-declare-effect；闭包引用外层名字报 E-rt-name（看不见）
#[test]
fn d_闭包纯度运行期() {
    let 发判断 = r#"
budget {calls: 4, cost: 0, depth: 16};
let s = state(mat("甲"));
let f = fit({declare: fn(a) { judge(state(mat("乙")), test("再问", "k")); a.p }}, [judge(s, test("好", "k"))]);
exit_kind(cut(f, {declare: {hi: 0.5}}))
"#;
    let e = 直跑(发判断, vec![]).expect_err("闭包里发判断");
    assert!(e.contains("E-fit-declare-effect"), "{e}");
    let 外层 = r#"
budget {calls: 4, cost: 0, depth: 16};
let s = state(mat("甲"));
let 外 = judge(s, test("外层", "k"));
let w = 0.4;
let f = fit({declare: fn(a) { a.p * w }}, [judge(s, test("好", "k"))]);
consume(cut(外, {declare: {hi: 0.5}}), "drop");
exit_kind(cut(f, {declare: {hi: 0.1}}))
"#;
    let e = 直跑(外层, vec![]).expect_err("闭包看不见外层名字");
    assert!(e.contains("E-rt-name") && e.contains("w"), "{e}");
    // 纯计算照常：if、max、下标、列表内置都可用
    let 纯 = r#"
budget {calls: 4, cost: 0, depth: 16};
let s = state(mat("甲"));
let f = fit({declare: fn(a, ws) { if a.max > 0.5 { a.probs[0] * ws[0] } else { sum(a.probs) } }},
            [judge(state(mat("甲"), {over: [mat("x"), mat("y")]}), select("哪个", "k"))], [[2.0]]);
exit_kind(cut(f, {declare: {hi: 1.0}}))
"#;
    let v = 直跑(纯, vec![("甲", "哪个", Answer::Choice(vec![0.7, 0.3]))]).unwrap();
    assert_eq!(v, "act");
}

/// (e) `order`：两个不同闭包的 Score 混排 → J-04；同一闭包按数分档、`tie` 并档；与读数混排 → J-04
#[test]
fn e_同拟合才可排序() {
    let 不同 = r#"
budget {calls: 4, cost: 0, depth: 16};
let s = state(mat("甲"));
let r = judge(s, test("好", "k"));
order([fit({declare: fn(a) { a.p }}, [r]), fit({declare: fn(a) { 1 - a.p }}, [r])])
"#;
    let e = 跑(不同, vec![], &CalibStore::new(), false).expect_err("不同拟合");
    assert!(e.contains("J-04"), "{e}");
    let 同 = |tie: &str| {
        format!(
            r#"
budget {{calls: 6, cost: 0, depth: 16}};
let f = fn(a) {{ a.p }};
fn 分(料) !{{judge}} {{ fit({{declare: f{tie}}}, [judge(state(mat(料)), test("好", "k"))]) }}
order(map(["甲", "乙", "丙"], 分))
"#
        )
    };
    let 表 = || {
        vec![
            ("甲", "好", Answer::Noul(0.5)),
            ("乙", "好", Answer::Noul(0.9)),
            ("丙", "好", Answer::Noul(0.52)),
        ]
    };
    let c = CalibStore::new();
    assert_eq!(
        跑(&同(""), 表(), &c, false).unwrap().value,
        json!([[1], [2], [0]])
    );
    assert_eq!(
        跑(&同(", tie: 0.05"), 表(), &c, false).unwrap().value,
        json!([[1], [2, 0]])
    );
    let 混 = r#"
budget {calls: 4, cost: 0, depth: 16};
let r = judge(state(mat("甲")), test("好", "k"));
order([fit({declare: fn(a) { a.p }}, [r]), r])
"#;
    let e = 跑(混, vec![], &c, false).expect_err("与读数混排");
    assert!(e.contains("J-04"), "{e}");
}

const 拟合守不可逆: &str = r#"
budget {calls: 4, cost: 0, depth: 16};
let s = state(mat("甲"));
let f = fit({declare: fn(a, b) { (a.p + b.p) / 2.0 }}, [judge(s, test("要退", "k1")), judge(s, test("合规", "k2"))]);
handle(cut(f, {declare: {hi: 0.7}}), {
    act: fn() { do("发退款", [], 0) },
    ignore: fn() { "不退" },
    unsure: fn(u) { consume(u, "drop"); "转人工" }})
"#;

/// (g) 守卫来自声明式拟合出口、不带开关守不可逆 `do` → J-08；带开关放行
#[test]
fn g_声明式拟合出口放行要宿主接受() {
    let 表 = || {
        vec![
            ("甲", "要退", Answer::Noul(0.9)),
            ("甲", "合规", Answer::Noul(0.8)),
        ]
    };
    let c = CalibStore::new();
    // 检查期：守卫全来自未接受的声明线（20j-2 的静态根对拟合出口同样成立）
    let e = 跑(拟合守不可逆, 表(), &c, false).expect_err("不带开关");
    assert!(
        e.contains("J-08") && e.contains("--release-on-declared"),
        "{e}"
    );
    // 运行期：报文说出拟合与开关
    let e = 直跑(拟合守不可逆, 表()).expect_err("运行期也拒");
    assert!(
        e.contains("J-08") && e.contains("fit=") && e.contains("宿主未声明接受作者线放行"),
        "{e}"
    );
    let o = 跑(拟合守不可逆, 表(), &c, true).expect("带开关放行");
    assert_eq!(o.value["content"], "已退", "{}", o.value);
    assert_eq!(o.exits[0]["releases"], true);
}

/// (i) 谱系：由（未接受的）声明式拟合出口选出的材料，其上认证线的判断守不可逆 `do` → J-08（谱系）；带开关放行
#[test]
fn i_谱系穿过拟合出口() {
    let src = r#"
budget {calls: 4, cost: 0, depth: 16};
let f = fit({declare: fn(a) { a.p }}, [judge(state(mat("甲")), test("好", "k0"))]);
let e = cut(f, {declare: {hi: 0.5}});
let r = judge(state(e), test("该发吗", "k2"));
handle(cut(r), {
    act: fn() { do("发退款", [], 0) },
    ignore: fn() { "不发" },
    unsure: fn(u) { consume(u, "drop"); "不发" }})
"#;
    let mut c = CalibStore::new();
    common::certified(&mut c, "k2", 0.8, 0.2, 50);
    // 出口转成的材料原文不是文本，端口按题面给数
    let 端 = |接受: bool| {
        let entry = 入口(接受);
        let a = 动作表();
        let mut l = Ledger::new();
        let ports = Ports::new().with(FnPort::judge("m", |_s: &State, qs: &[&Question]| {
            Ok(JudgeResult {
                answers: qs
                    .iter()
                    .map(|q| Answer::Noul(if q.text == "好" { 0.9 } else { 0.95 }))
                    .collect(),
                tokens: 0,
                cost: 0.0,
                mode_share: vec![None; qs.len()],
                perms: vec![0; qs.len()],
                confidence: vec![],
            })
        }));
        jpp::Session::new(ports, &c, &a)
            .run(&编译(src, &entry), &entry, &mut l)
            .map(|o| o.value_json())
            .map_err(|e| e.render())
    };
    let e = 端(false).expect_err("谱系里有未接受的声明式拟合出口");
    assert!(e.contains("J-08") && e.contains("谱系"), "{e}");
    let v = 端(true).expect("带开关，谱系放行");
    assert_eq!(v["content"], "已退", "{v}");
}

/// (j) 输入不可用：证据不足（J-09）的读数进拟合，`cut(Score)` 出 `insufficient`，不求值
#[test]
fn j_输入不可用出对应未决() {
    let src = r#"
budget {calls: 4, cost: 0, depth: 16};
let s = state(mat("甲"));
let f = fit({declare: fn(a) { a.p }}, [judge(s, test("有依据吗", "k", {evidence: ["ref"]}))]);
let e = cut(f, {declare: {hi: 0.5}});
let k = exit_kind(e);
consume(e, "drop");
k
"#;
    let o = 跑(
        src,
        vec![("甲", "有依据吗", Answer::Noul(0.9))],
        &CalibStore::new(),
        false,
    )
    .unwrap();
    assert_eq!(o.value, "unsure(insufficient:ref)");
    assert_eq!(o.exits[0]["grade"], "Cold");
}

/// (k) `confidence`：固定观察夹具给了时记录里有；非固定端口没报时，读它报错并说判断器没报
#[test]
fn k_自报置信度进记录() {
    let src = r#"
budget {calls: 4, cost: 0, depth: 16};
let f = fit({declare: fn(a) { a.confidence }}, [judge(state(mat("甲")), test("行吗", "k"))]);
exit_kind(cut(f, {declare: {hi: 0.6}}))
"#;
    let mut fp = FixedPorts::new();
    let st = State::new(
        vec![Mat::literal(json!("甲"))],
        vec![],
        vec![],
        vec![],
        false,
    );
    let key = fp.observe(
        &st,
        &Question::new(Op::Test, "行吗", "k", vec![]),
        Answer::Noul(0.8),
    );
    fp.fix_confidence(&key, 0.55);
    let mut l = Ledger::new();
    let o = jpp::run(
        &lower(&parse(src).unwrap()).unwrap(),
        fp.ports(),
        &CalibStore::new(),
        &ActionRegistry::new(),
        &mut l,
    )
    .unwrap_or_else(|e| panic!("{}", e.render()));
    assert_eq!(o.value_json(), "ignore");
    let e = 跑(
        src,
        vec![("甲", "行吗", Answer::Noul(0.8))],
        &CalibStore::new(),
        false,
    )
    .expect_err("非固定端口没报 confidence");
    assert!(
        e.contains("E-rt-field") && e.contains("没有随答案报 confidence"),
        "{e}"
    );
}

/// (l) `cut(Score)` 不带 `declare` → 冷；带 `stat` / `cost` / `alpha` → E-cut-options；`Score` 不能算、不能读
#[test]
fn l_冷与选项() {
    let src = |选项: &str| {
        format!(
            r#"
budget {{calls: 4, cost: 0, depth: 16}};
let f = fit({{declare: fn(a) {{ a.p }}}}, [judge(state(mat("甲")), test("行吗", "k"))]);
let e = cut(f{选项});
let k = exit_kind(e);
consume(e, "drop");
k
"#
        )
    };
    let 表 = || vec![("甲", "行吗", Answer::Noul(0.8))];
    let c = CalibStore::new();
    // B187（批 9 第 3 格）：拟合分数不是判断器的回答，不写线是缺分档参数（形状错）
    let e = 跑(&src(""), 表(), &c, false).unwrap_err();
    assert!(
        e.contains("E-cut-options") && e.contains("声明式拟合的结果是一个分"),
        "{e}"
    );
    let e = 跑(&src(r#", "k""#), 表(), &c, false).unwrap_err();
    assert!(e.contains("E-cut-options"), "{e}");
    for 选项 in [
        r#", {stat: "confidence", declare: {hi: 0.5}}"#,
        r#", {cost: [1, 2]}"#,
        r#", {alpha: 0.1}"#,
        r#", {declare: {hi: 0.3, lo: 0.5}}"#,
    ] {
        let e = 跑(&src(选项), 表(), &c, false).expect_err(选项);
        assert!(e.contains("E-cut-options"), "{选项}：{e}");
    }
    let 算 = r#"
budget {calls: 4, cost: 0, depth: 16};
let f = fit({declare: fn(a) { a.p }}, [judge(state(mat("甲")), test("行吗", "k"))]);
f + 1
"#;
    let e = 直跑(算, 表()).expect_err("Score 不能算");
    assert!(e.contains("J-01"), "{e}");
    let 读 = 算.replace("f + 1", "content(f)");
    let e = 直跑(&读, 表()).expect_err("Score 不能读");
    assert!(e.contains("J-01"), "{e}");
    // 作程序返回值：报告里只有拟合的身份，没有数
    let 返回 = 算.replace("f + 1", "f");
    let v = 直跑(&返回, 表()).unwrap();
    assert!(
        v.get("score").is_some() && !v.to_string().contains("0.8"),
        "{v}"
    );
}

/// (m) `cuts` 分档：0.627 → at(2)，0.238 → at(0)；`closed: {cuts: false}` 生效
#[test]
fn m_拟合分档() {
    let src = 加权.replace("线", "{declare: {cuts: [0.3, 0.6]}}");
    let o = 跑(&src, 加权表(), &CalibStore::new(), false).unwrap();
    assert_eq!(o.value["结论"], json!(["at(2)", "at(0)"]));
    assert_eq!(o.exits[0]["declared"]["cuts"], json!([0.3, 0.6]));
    let 恰好 = r#"
budget {calls: 4, cost: 0, depth: 16};
let f = fit({declare: fn(a) { a.p }}, [judge(state(mat("甲")), test("行吗", "k"))]);
[exit_kind(cut(f, {declare: {cuts: [0.6]}})), exit_kind(cut(f, {declare: {cuts: [0.6], closed: {cuts: false}}}))]
"#;
    let v = 跑(
        恰好,
        vec![("甲", "行吗", Answer::Noul(0.6))],
        &CalibStore::new(),
        false,
    )
    .unwrap()
    .value;
    assert_eq!(v, json!(["at(1)", "at(0)"]));
}

// ---------- 第二段：检查期（B153 (2)(3)、B172；步 20j-4） ----------

fn 查(src: &str) -> Vec<(String, String)> {
    jpp::check(&lower(&parse(src).expect("解析")).expect("lower"))
        .diagnostics
        .into_iter()
        .map(|d| (d.rule, d.message))
        .collect()
}

/// (d) 检查期：闭包里有效应、构造、桥（字面闭包与唯一绑定的名字都查）
#[test]
fn d_检查期闭包纯度() {
    let 字面 = r#"
budget {calls: 4, cost: 0, depth: 16};
let s = state(mat("甲"));
let f = fit({declare: fn(a) { judge(state(mat("乙")), test("再问", "k")); a.p }}, [judge(s, test("好", "k"))]);
exit_kind(cut(f, {declare: {hi: 0.5}}))
"#;
    let d = 查(字面);
    assert!(d.iter().any(|(r, _)| r == "E-fit-declare-effect"), "{d:?}");
    let 名字 = r#"
budget {calls: 4, cost: 0, depth: 16};
let g = fn(a) { sieve([mat("x")], test("筛", "k")); a.p };
let f = fit({declare: g}, [judge(state(mat("甲")), test("好", "k"))]);
exit_kind(cut(f, {declare: {hi: 0.5}}))
"#;
    let d = 查(名字);
    assert!(
        d.iter()
            .any(|(r, m)| r == "E-fit-declare-effect" && m.contains("构造")),
        "{d:?}"
    );
    // 纯计算不报：if、max、下标、sum、内层函数
    let 纯 = r#"
budget {calls: 4, cost: 0, depth: 16};
let f = fit({declare: fn(a, ws) { let h = fn(x) { x * ws[0] }; if a.max > 0.5 { h(a.probs[0]) } else { max(sum(a.probs), 0.1) } }},
            [judge(state(mat("甲"), {over: [mat("x"), mat("y")]}), select("哪个", "k"))], [[2.0]]);
exit_kind(cut(f, {declare: {hi: 1.0}}))
"#;
    let d = 查(纯);
    assert!(!d.iter().any(|(r, _)| r == "E-fit-declare-effect"), "{d:?}");
}

/// (d′) 检查期：闭包引用外层名字，同码报，报文说经 extra 传入
#[test]
fn d2_检查期外层名字() {
    let src = r#"
budget {calls: 4, cost: 0, depth: 16};
let w = 0.4;
let f = fit({declare: fn(a) { a.p * w }}, [judge(state(mat("甲")), test("好", "k"))]);
exit_kind(cut(f, {declare: {hi: 0.1}}))
"#;
    let d = 查(src);
    assert!(
        d.iter().any(|(r, m)| r == "E-fit-declare-effect"
            && m.contains("外层的 w")
            && m.contains("extra")),
        "{d:?}"
    );
}

/// (f) 检查期：Score 做算术、`content(Score)` 报 J-01（`fit` 的结果类别是读数）
#[test]
fn f_检查期score不是数() {
    for 用法 in ["f + 1", "content(f)"] {
        let src = format!(
            r#"
budget {{calls: 4, cost: 0, depth: 16}};
let f = fit({{declare: fn(a) {{ a.p }}}}, [judge(state(mat("甲")), test("好", "k"))]);
{用法}
"#
        );
        let d = 查(&src);
        assert!(d.iter().any(|(r, _)| r == "J-01"), "{用法}：{d:?}");
    }
}

/// (g′) 检查期：输入里有确定不可信的读数时，J-08 静态子面的来源穿过 Score 说材料（带开关也拒）
#[test]
fn g2_检查期来源穿过拟合() {
    let src = r#"
budget {calls: 4, cost: 0, depth: 16};
let f = fit({declare: fn(a) { a.p }}, [judge(state(料), test("要退", "k"))]);
handle(cut(f, {declare: {hi: 0.7}}), {
    act: fn() { do("write_json", ["o.json", 1], 0) },
    ignore: fn() { 0 },
    unsure: fn(u) { consume(u, "drop"); 0 }})
"#;
    let mut entry = 入口(true);
    entry
        .materials
        .push(jpp::EntryMat::untrusted("料", json!("甲")));
    let r =
        jpp::Session::explain_with_actions(&编译(src, &entry), None, &jpp::actions::check_table());
    let m: Vec<&String> = r
        .diagnostics
        .iter()
        .filter(|d| d.rule == "J-08")
        .map(|d| &d.message)
        .collect();
    assert_eq!(m.len(), 1, "{:?}", r.diagnostics);
    assert!(m[0].contains("宿主入口 料"), "{}", m[0]);
}

/// (n) 单读数两标签相减设门（B174）：`fit({declare: fn(a) { a.probs[0] - a.probs[1] }}, [r])` 配 `{hi: 0.10}`
#[test]
fn n_单读数两标签相减() {
    let src = r#"
budget {calls: 4, cost: 0, depth: 16};
let r = judge(state(mat("甲"), {over: [mat("选中"), mat("都不是")]}), select("哪个", "k"));
exit_kind(cut(fit({declare: fn(a) { a.probs[0] - a.probs[1] }}, [r]), {declare: {hi: 0.10}}))
"#;
    let c = CalibStore::new();
    let 表 = |a: f64, b: f64| vec![("甲", "哪个", Answer::Choice(vec![a, b]))];
    assert_eq!(跑(src, 表(0.62, 0.50), &c, false).unwrap().value, "act");
    assert_eq!(跑(src, 表(0.55, 0.50), &c, false).unwrap().value, "ignore");
}
