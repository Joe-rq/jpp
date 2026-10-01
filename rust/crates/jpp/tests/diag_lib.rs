//! 步 26（B47）诊断闸门的判断器路：`lib/diag.jpp` 的两条字面题诊断（这题里有两个判断吗、题问的东西在面前的
//! 材料里吗）输出契约值；`suggest_rewrite` 只在显式调用时发一次 `gen`；示例 `examples/diag-runtime.jpp` 端到端。
//!
//! 依据：`21` 步 26；`20-v2` B47；`12` J-17 后 B13 条（不用元题）；核心讨论结论 :29、:41；
//! 预注册 `地基/过程记录/工程-步26.md` §一·4、§一·5。

mod common;
use std::cell::{Cell, RefCell};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

use jpp::effects::{CalibStore, EffectError, FnPort, GenResult, JudgeResult, Ports};
use jpp::interp::ActionRegistry;
use jpp::ledger::Ledger;
use jpp::value::{Answer, Question, State};
use jpp::{EntryArgs, Outcome, Program, Session};
use serde_json::{Value as Json, json};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

static NEXT: AtomicUsize = AtomicUsize::new(0);

fn 编译(src: &str) -> Program {
    let dir = root().join(format!(
        "target/diag-lib-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("p.jpp");
    std::fs::write(&path, src).unwrap();
    let loaded = jpp::syntax::loader::load(&path);
    let _ = std::fs::remove_dir_all(&dir);
    jpp::lower(&loaded.expect("装载").program).expect("lower")
}

const 两判: &str = "这道题里是否有两个或更多需要分别回答的判断？";
const 在料: &str = "上下文里那道题问的东西，这段材料里有没有？";

fn 文(v: &Json) -> String {
    match v {
        Json::String(s) => s.clone(),
        o => o.to_string(),
    }
}

/// 读数表：按（诊断题面, on, ctx）给读数；未列出的报错。记下每次调用的题数、生成次数
struct 表 {
    读数: Vec<(&'static str, &'static str, &'static str, f64)>,
}

fn 跑(p: &Program, t: &表, 生成输出: &str) -> (Outcome, Vec<usize>, usize) {
    let 每次 = RefCell::new(vec![]);
    let 生成 = Cell::new(0);
    let ports = Ports::new()
        .with(common::伴随中性judge(
            "fixed-0",
            |s: &State, qs: &[&Question]| {
                每次.borrow_mut().push(qs.len());
                let on =
                    s.on.iter()
                        .map(|m| 文(&m.content))
                        .collect::<Vec<_>>()
                        .join("|");
                let ctx = s
                    .ctx
                    .iter()
                    .map(|m| 文(&m.content))
                    .collect::<Vec<_>>()
                    .join("|");
                let answers = qs
                    .iter()
                    .map(|q| {
                        t.读数
                            .iter()
                            .find(|(qt, o, c, _)| *qt == q.text && *o == on && *c == ctx)
                            .map(|x| Answer::Noul(x.3))
                            .ok_or_else(|| EffectError(format!("没有「{}」@{on}|{ctx}", q.text)))
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                Ok(JudgeResult {
                    tokens: 0,
                    cost: 0.0,
                    mode_share: vec![None; answers.len()],
                    perms: vec![0; answers.len()],
                    confidence: vec![],
                    answers,
                })
            },
        ))
        .with(FnPort::generate("fixed-0", |_p, _c, _n, _r| {
            生成.set(生成.get() + 1);
            Ok(GenResult {
                outputs: vec![json!(生成输出)],
                ..Default::default()
            })
        }));
    let calib = CalibStore::new();
    let acts = ActionRegistry::new();
    let o = Session::new(ports, &calib, &acts)
        .with_companions(common::伴随())
        .run(p, &EntryArgs::default(), &mut Ledger::new())
        .unwrap_or_else(|e| panic!("{}", e.render()));
    let n = 每次.borrow().clone();
    (o, n, 生成.get())
}

const 头: &str = r#"import "../../lib/diag.jpp";
budget {calls: 10, cost: 0.01, depth: 32};
let a = "这段话是否";
let q1 = test(a + "提到了成都？", "k");
let q2 = test(a + "说他去出差并且见了朋友？", "k");
let q3 = test("他在成都" + "住了多少天？", "k");
let qs = [q1, q2, q3];
let note = mat("我上个月去了成都出差。");
"#;

fn 材料文(e: &Json) -> String {
    e.to_string()
}

#[test]
fn 两个判断分流并附修法() {
    let p = 编译(&format!(
        "{头}let o = diag_two_judgments(qs);\n{{n_act: len(o.value), n_ignore: len(o.detail.ignore), n_pending: len(o.pending), pending: o.pending, advice: o.detail.advice, which: o.detail.diagnosis, kind: o.kind, act: map(o.value, fn(e) {{ e.pos }})}}"
    ));
    let t = 表 {
        读数: vec![
            (两判, "这段话是否提到了成都？", "", 0.05),
            (两判, "这段话是否说他去出差并且见了朋友？", "", 0.95),
            (两判, "他在成都住了多少天？", "", 0.5),
        ],
    };
    let (o, 每次, 生成) = 跑(&p, &t, "");
    let v = o.value_json();
    eprintln!("{}", 材料文(&v));
    assert_eq!(v["n_act"], 1);
    assert_eq!(v["n_ignore"], 1);
    assert_eq!(v["n_pending"], 1, "并列读数未决，随契约值交出");
    assert_eq!(v["act"], json!([1]));
    assert_eq!(v["which"], "two_judgments");
    assert!(v["advice"].as_str().unwrap().contains("拆成两道题"));
    assert_eq!(每次.len(), 3, "三道题面各一个状态：{每次:?}");
    assert_eq!(生成, 0);
}

#[test]
fn 问的东西在不在材料里分流并附修法() {
    let p = 编译(&format!(
        "{头}let o = diag_in_material(qs, note);\n{{n_act: len(o.value), n_ignore: len(o.detail.ignore), ignored: map(o.detail.ignore, fn(e) {{ e.pos }}), advice: o.detail.advice, keys: len(o.evidence)}}"
    ));
    let m = "我上个月去了成都出差。";
    let t = 表 {
        读数: vec![
            (在料, m, "这段话是否提到了成都？", 0.95),
            (在料, m, "这段话是否说他去出差并且见了朋友？", 0.9),
            (在料, m, "他在成都住了多少天？", 0.1),
        ],
    };
    let (o, 每次, 生成) = 跑(&p, &t, "");
    let v = o.value_json();
    eprintln!("{}", 材料文(&v));
    assert_eq!(v["n_act"], 2);
    assert_eq!(v["n_ignore"], 1);
    assert_eq!(v["ignored"], json!([2]));
    assert_eq!(v["keys"], 3, "证据是三条账本键");
    assert!(v["advice"].as_str().unwrap().contains("补进材料"));
    assert_eq!(每次.len(), 3, "ctx 不同，各一个状态：{每次:?}");
    assert_eq!(生成, 0);
}

#[test]
fn 诊断库自身静态检查零诊断告警() {
    let p = 编译(&format!("{头}len(qs)"));
    let r = jpp::check(&p);
    let diag: Vec<String> = r
        .warnings()
        .iter()
        .filter(|d| d.rule.starts_with("W-diag"))
        .map(|d| d.render())
        .collect();
    assert!(diag.is_empty(), "{diag:?}");
    assert!(r.is_ok());
    // 两道诊断题面逐条过 B13：不含元题词、不报任何 W-diag
    for t in [两判, 在料] {
        let d = jpp::check::diag::diagnose_question(
            &jpp::check::diag::QuestionLit::new("test", t, false, Default::default()),
            &Default::default(),
        );
        assert!(d.is_empty(), "{t}: {d:?}");
    }
}

#[test]
fn 大模型建议只在显式调用时发一次生成() {
    let p = 编译(&format!(
        "{头}let r = suggest_rewrite(q2, diag_notes(q2));\n{{n: len(r), text: content(r[0])}}"
    ));
    let t = 表 { 读数: vec![] };
    let (o, 每次, 生成) = 跑(&p, &t, "这段话是否说他去出差了？");
    assert_eq!(生成, 1);
    assert!(每次.is_empty());
    assert_eq!(
        o.value_json(),
        json!({"n": 1, "text": "这段话是否说他去出差了？"})
    );
}

#[test]
fn 示例运行期生成题端到端() {
    let loaded =
        jpp::syntax::loader::load(&root().join("examples/diag-runtime.jpp")).expect("装载");
    let p = jpp::lower(&loaded.program).expect("lower");
    let fx: Json = serde_json::from_str(
        &std::fs::read_to_string(root().join("examples/fixtures/diag-runtime.json")).unwrap(),
    )
    .unwrap();
    let obs = fx["observations"].as_array().unwrap().clone();
    let gen_out = fx["generations"][0]["output"][0]
        .as_str()
        .unwrap()
        .to_string();
    let 每次 = RefCell::new(vec![]);
    let 生成 = Cell::new(0);
    let ports = Ports::new()
        .with(common::伴随中性judge(
            "fixed-0",
            |s: &State, qs: &[&Question]| {
                每次.borrow_mut().push(qs.len());
                let on: Vec<String> = s.on.iter().map(|m| 文(&m.content)).collect();
                let ctx: Vec<String> = s.ctx.iter().map(|m| 文(&m.content)).collect();
                let answers = qs
                    .iter()
                    .map(|q| {
                        obs.iter()
                            .find(|o| {
                                o["text"] == q.text.as_str()
                                    && o["on"]
                                        .as_array()
                                        .unwrap()
                                        .iter()
                                        .map(文)
                                        .collect::<Vec<_>>()
                                        == on
                                    && o["ctx"]
                                        .as_array()
                                        .map(|a| a.iter().map(文).collect::<Vec<_>>())
                                        .unwrap_or_default()
                                        == ctx
                            })
                            .map(|o| serde_json::from_value(o["answer"].clone()).unwrap())
                            .ok_or_else(|| EffectError(format!("没有「{}」", q.text)))
                    })
                    .collect::<Result<Vec<Answer>, _>>()?;
                Ok(JudgeResult {
                    tokens: 0,
                    cost: 0.0,
                    mode_share: vec![None; answers.len()],
                    perms: vec![0; answers.len()],
                    confidence: vec![],
                    answers,
                })
            },
        ))
        .with(FnPort::generate("fixed-0", |_p, _c, _n, _r| {
            生成.set(生成.get() + 1);
            Ok(GenResult {
                outputs: vec![json!(gen_out)],
                ..Default::default()
            })
        }));
    let calib = CalibStore::new();
    let acts = ActionRegistry::new();
    let o = Session::new(ports, &calib, &acts)
        .with_companions(common::伴随())
        .run(&p, &EntryArgs::default(), &mut Ledger::new())
        .unwrap_or_else(|e| panic!("{}", e.render()));
    let 运行期: Vec<String> = o
        .trace
        .warnings
        .iter()
        .filter(|w| w.contains("运行期诊断"))
        .map(|w| w.split(':').next().unwrap().to_string())
        .collect();
    assert_eq!(
        运行期,
        [
            "W-diag-mention-scope",
            "W-diag-two-judgments",
            "W-diag-open-question"
        ]
    );
    assert_eq!(生成.get(), 1);
    assert_eq!(每次.borrow().len(), 7, "{:?}", 每次.borrow());
    assert_eq!(o.cost.calls, 8);
    let v = o.value_json();
    let codes: Vec<Json> = v["auto"]
        .as_array()
        .unwrap()
        .iter()
        .map(|x| x["codes"].clone())
        .collect();
    assert_eq!(
        codes,
        [
            json!(["W-diag-mention-scope"]),
            json!(["W-diag-two-judgments"]),
            json!(["W-diag-open-question"])
        ]
    );
    let n = |k: &str| {
        (
            v[k]["value"].as_array().unwrap().len(),
            v[k]["detail"]["ignore"]
                .as_array()
                .map(|a| a.len())
                .unwrap_or(0),
            v[k]["pending"].as_array().unwrap().len(),
        )
    };
    assert_eq!(n("two_judgments"), (1, 2, 0));
    assert_eq!(n("in_material"), (2, 1, 0));
    assert_eq!((n("answered").0, n("answered").2), (2, 1));
}
