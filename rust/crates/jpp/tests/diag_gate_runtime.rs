//! 步 26（B47）诊断闸门的静态路：运行期闸门（`register.rs` 两个登记点，只提示、不记账）与宿主变换
//! `diagnose`（`transform("diagnose", …)`，记账、重放取账本）。
//!
//! 依据：`21` 步 26；`20-v2` B47；批 9 裁定 :42、:136（只提示，不改走向）；主会话裁定 2026-09-29 第十四、二十条；
//! 预注册 `地基/过程记录/工程-步26.md` §一。

mod common;
use std::cell::Cell;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::atomic::{AtomicUsize, Ordering};

use jpp::effects::{CalibStore, EffectError, FnPort, GenResult, JudgeResult, Ports};
use jpp::interp::{ActionRegistry, Passes};
use jpp::ledger::{Entry, Ledger};
use jpp::value::{Answer, Question, State};
use jpp::{EntryArgs, Outcome, Program, Session};
use jpp_effects::{HostTaint, HostTransform, TransformTable};
use serde_json::{Value as Json, json};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

static NEXT: AtomicUsize = AtomicUsize::new(0);

/// 源码写进 `target/` 下的临时目录再装载（`import "../../lib/…"` 相对它解析）
fn 编译(src: &str) -> Program {
    let dir = root().join(format!(
        "target/diag-gate-{}-{}",
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

fn 装载(rel: &str) -> Program {
    let loaded = jpp::syntax::loader::load(&root().join(rel)).expect("装载");
    jpp::lower(&loaded.program).expect("lower")
}

/// 夹具：观察按（题面, on, ctx）取读数；生成按提示词取输出；校准记录照写
struct 夹具 {
    obs: HashMap<(String, String, String), Answer>,
    gens: HashMap<String, Vec<Json>>,
    calib: Vec<Json>,
}

fn 文(v: &Json) -> String {
    match v {
        Json::String(s) => s.clone(),
        o => o.to_string(),
    }
}

fn 读夹具(rel: &str) -> 夹具 {
    let f: Json =
        serde_json::from_str(&std::fs::read_to_string(root().join(rel)).unwrap()).unwrap();
    let 连 = |v: &Json| -> String {
        v.as_array()
            .map(|a| a.iter().map(文).collect::<Vec<_>>().join("\u{1f}"))
            .unwrap_or_default()
    };
    let mut obs = HashMap::new();
    for o in f["observations"].as_array().cloned().unwrap_or_default() {
        obs.insert(
            (
                o["text"].as_str().unwrap().to_string(),
                连(&o["on"]),
                连(&o["ctx"]),
            ),
            serde_json::from_value(o["answer"].clone()).unwrap(),
        );
    }
    let mut gens = HashMap::new();
    for g in f["generations"].as_array().cloned().unwrap_or_default() {
        gens.insert(
            g["prompt"].as_str().unwrap().to_string(),
            g["output"].as_array().unwrap().clone(),
        );
    }
    夹具 {
        obs,
        gens,
        calib: f["calibrations"].as_array().cloned().unwrap_or_default(),
    }
}

fn 端口<'a>(fx: &'a 夹具, 调用: &'a Cell<usize>) -> Ports<'a> {
    Ports::new()
        .with(common::伴随中性judge(
            "fixed-0",
            move |s: &State, qs: &[&Question]| {
                调用.set(调用.get() + 1);
                let on =
                    s.on.iter()
                        .map(|m| 文(&m.content))
                        .collect::<Vec<_>>()
                        .join("\u{1f}");
                let ctx = s
                    .ctx
                    .iter()
                    .map(|m| 文(&m.content))
                    .collect::<Vec<_>>()
                    .join("\u{1f}");
                let answers = qs
                    .iter()
                    .map(|q| {
                        fx.obs
                            .get(&(q.text.clone(), on.clone(), ctx.clone()))
                            .cloned()
                            .ok_or_else(|| {
                                EffectError(format!("夹具没有「{}」@{on}|{ctx}", q.text))
                            })
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
        .with(FnPort::generate("fixed-0", move |p, _c, _n, _r| {
            Ok(GenResult {
                outputs: fx.gens.get(p).cloned().unwrap_or_default(),
                ..Default::default()
            })
        }))
}

fn 库(fx: &夹具) -> CalibStore {
    let mut c = CalibStore::new();
    for r in &fx.calib {
        c.put(
            r["key"].as_str().unwrap(),
            r["hi"].as_f64().unwrap(),
            r["lo"].as_f64().unwrap(),
            r["n"].as_u64().unwrap(),
            r["status"].as_str().unwrap(),
            r["delta"].as_f64(),
        )
        .unwrap();
    }
    c
}

/// 闸门开（`Session`：`jpp_lib::s_library` 注册闸门与宿主变换）
fn 开(p: &Program, fx: &夹具, ledger: &mut Ledger) -> (Outcome, usize) {
    let 调用 = Cell::new(0);
    let calib = 库(fx);
    let acts = ActionRegistry::new();
    let o = Session::new(端口(fx, &调用), &calib, &acts)
        .with_companions(common::伴随())
        .run(p, &EntryArgs::default(), ledger)
        .unwrap_or_else(|e| panic!("{}", e.render()));
    (o, 调用.get())
}

/// 闸门关（直接用解释器，不注入闸门）
fn 关(p: &Program, fx: &夹具) -> (Outcome, usize) {
    let 调用 = Cell::new(0);
    let calib = 库(fx);
    let acts = ActionRegistry::new();
    let mut ledger = Ledger::new();
    let o = {
        let mut it = jpp::interp::Interp::new(
            端口(fx, &调用),
            &mut ledger,
            &calib,
            &acts,
            p.budget.clone(),
        );
        it.passes = Passes::default();
        it.run(p).unwrap_or_else(|e| panic!("{}", e.render()))
    };
    (o, 调用.get())
}

fn 运行期(o: &Outcome) -> Vec<String> {
    o.trace
        .warnings
        .iter()
        .filter(|w| w.starts_with("W-diag-") && w.contains("运行期诊断"))
        .cloned()
        .collect()
}

fn 码(ws: &[String]) -> Vec<String> {
    ws.iter()
        .map(|w| w.split(':').next().unwrap().to_string())
        .collect()
}

fn 空夹具() -> 夹具 {
    夹具 {
        obs: HashMap::new(),
        gens: HashMap::new(),
        calib: vec![],
    }
}

fn 一条(fx: &mut 夹具, text: &str, on: &str, p: f64) {
    fx.obs
        .insert((text.into(), on.into(), String::new()), Answer::Noul(p));
}

const 拼: &str = r#"budget {calls: 5, cost: 0.01, depth: 16};
let a = "这段话是否提到了成都";
let q = test(a + "并且是否提到了出差？", "k");
let e = cut(judge(state(mat("我去成都出差")), q));
handle(e, {act: fn() { 1 }, ignore: fn() { 0 }, unsure: fn(u) { {p: u} }})"#;

#[test]
fn 拼出的题发出前过诊断且不改走向() {
    let p = 编译(拼);
    let mut fx = 空夹具();
    一条(
        &mut fx,
        "这段话是否提到了成都并且是否提到了出差？",
        "我去成都出差",
        0.9,
    );
    let (on, n_on) = 开(&p, &fx, &mut Ledger::new());
    let (off, n_off) = 关(&p, &fx);
    assert_eq!(on.value_json(), off.value_json());
    assert_eq!((n_on, on.cost.calls), (n_off, off.cost.calls));
    assert_eq!(
        码(&运行期(&on)),
        ["W-diag-two-judgments", "W-diag-mention-scope"],
        "{:?}",
        on.trace.warnings
    );
    assert!(运行期(&off).is_empty());
    assert!(运行期(&on)[0].contains("修法："));
}

#[test]
fn 字面题检查期报过运行期不重复报() {
    let p = 编译(
        r#"budget {calls: 5, cost: 0.01, depth: 16};
let e = cut(judge(state(mat("我去成都出差")), test("这段话是否提到了成都并且是否提到了出差？", "k")));
handle(e, {act: fn() { 1 }, ignore: fn() { 0 }, unsure: fn(u) { {p: u} }})"#,
    );
    let mut fx = 空夹具();
    一条(
        &mut fx,
        "这段话是否提到了成都并且是否提到了出差？",
        "我去成都出差",
        0.9,
    );
    let (o, _) = 开(&p, &fx, &mut Ledger::new());
    assert!(运行期(&o).is_empty(), "{:?}", o.trace.warnings);
}

#[test]
fn 同一道题每趟只诊断一次() {
    let p = 编译(
        r#"budget {calls: 5, cost: 0.01, depth: 16};
let a = "这段话是否提到了成都";
let q = test(a + "并且是否提到了出差？", "k");
let e1 = cut(judge(state(mat("甲")), q));
let e2 = cut(judge(state(mat("乙")), q));
[handle(e1, {act: fn() { 1 }, ignore: fn() { 0 }, unsure: fn(u) { {p: u} }}),
 handle(e2, {act: fn() { 1 }, ignore: fn() { 0 }, unsure: fn(u) { {p: u} }})]"#,
    );
    let mut fx = 空夹具();
    一条(
        &mut fx,
        "这段话是否提到了成都并且是否提到了出差？",
        "甲",
        0.9,
    );
    一条(
        &mut fx,
        "这段话是否提到了成都并且是否提到了出差？",
        "乙",
        0.1,
    );
    let (o, n) = 开(&p, &fx, &mut Ledger::new());
    assert_eq!(n, 2);
    assert_eq!(
        码(&运行期(&o)),
        ["W-diag-two-judgments", "W-diag-mention-scope"],
        "{:?}",
        o.trace.warnings
    );
}

fn 填法程序(值: &str) -> String {
    format!(
        r#"budget {{calls: 5, cost: 0.01, depth: 16}};
let f = form("test", "这段话是否直接写出了{{x}}？", {{calib: "k"}});
let q = fill(f, {{x: {值}}});
let e = cut(judge(state(mat("材料")), q));
handle(e, {{act: fn() {{ 1 }}, ignore: fn() {{ 0 }}, unsure: fn(u) {{ {{p: u}} }}}})"#
    )
}

#[test]
fn 非字面填法用真实填入值诊断() {
    let p = 编译(&填法程序(r#""焦" + "虑感""#));
    let mut fx = 空夹具();
    一条(&mut fx, "这段话是否直接写出了焦虑感？", "材料", 0.9);
    let (o, _) = 开(&p, &fx, &mut Ledger::new());
    assert_eq!(
        码(&运行期(&o)),
        ["W-diag-abstract-direct"],
        "{:?}",
        o.trace.warnings
    );
    // 具体值不命中
    let p = 编译(&填法程序(r#""成" + "都""#));
    let mut fx = 空夹具();
    一条(&mut fx, "这段话是否直接写出了成都？", "材料", 0.9);
    let (o, _) = 开(&p, &fx, &mut Ledger::new());
    assert!(运行期(&o).is_empty(), "{:?}", o.trace.warnings);
}

/// 预注册 §一·1、§一·2：现有示例接上闸门前后值与调用数相同，运行期告警数如预注册
#[test]
fn 现有示例接上闸门不改走向() {
    for (ex, fx, 预期) in [
        (
            "examples/seq-wrapped-new.jpp",
            "examples/fixtures/seq-wrapped.json",
            4,
        ),
        (
            "examples/seq-wrapped-old.jpp",
            "examples/fixtures/seq-wrapped.json",
            4,
        ),
        (
            "examples/question-as-data.jpp",
            "examples/fixtures/question-as-data.json",
            0,
        ),
        (
            "examples/question-forms.jpp",
            "examples/fixtures/question-forms.json",
            0,
        ),
        // tree-collab 的夹具按 FixedPorts 的观察键配（本文件的简易夹具端口配不上），改由 CLI 核：
        // 过程记录 工程-步26.md §二
    ] {
        let p = 装载(ex);
        let fx = 读夹具(fx);
        let (on, n_on) = 开(&p, &fx, &mut Ledger::new());
        let (off, n_off) = 关(&p, &fx);
        assert_eq!(on.value_json(), off.value_json(), "{ex}");
        assert_eq!((n_on, on.cost.calls), (n_off, off.cost.calls), "{ex}");
        let w = 运行期(&on);
        eprintln!("{ex}: 调用 {n_on}，运行期告警 {}：{:?}", w.len(), 码(&w));
        assert_eq!(w.len(), 预期, "{ex}: {w:?}");
        if 预期 > 0 {
            assert!(
                码(&w).iter().all(|c| c == "W-diag-mention-scope"),
                "{ex}: {w:?}"
            );
        }
    }
}

const 宿主诊断: &str = r#"import "../../lib/diag.jpp";
budget {calls: 5, cost: 0.01, depth: 16};
let a = "这段话是否提到了成都";
let bad = test(a + "并且是否提到了出差？", "k");
let good = test("这段话" + "是否直接写出了成都这个名字？", "k");
{bad: diag_codes(bad), good: diag_notes(good), fixes: map(diag_notes(bad), fn(n) { n.fix != "" })}"#;

#[test]
fn 宿主变换diagnose返回码且记账() {
    let p = 编译(宿主诊断);
    let fx = 空夹具();
    let mut ledger = Ledger::new();
    let (o, n) = 开(&p, &fx, &mut ledger);
    assert_eq!(n, 0, "诊断不发判断调用");
    assert_eq!(
        o.value_json(),
        json!({"bad": ["W-diag-two-judgments", "W-diag-mention-scope"], "good": [], "fixes": [true, true]})
    );
    // 每个（站点，输入）一条 transform 条目，输出就是卡住的码。三次调用都在 diag_notes 体内同一站点：
    // bad 两次、good 一次；bad 第二次同站点同输入，命中账本、不新增条目 → 共 2 条，trace 里 3 次、其中 1 次取自账本
    let 条目: Vec<&Json> = ledger
        .entries
        .iter()
        .filter_map(|e| match e {
            Entry::Effect { kind, output, .. } if kind == "transform" => Some(output),
            _ => None,
        })
        .collect();
    assert_eq!(条目.len(), 2, "{条目:?}");
    let 次: Vec<bool> = o
        .trace
        .events
        .iter()
        .filter(|e| e.kind == "transform")
        .map(|e| e.replayed)
        .collect();
    assert_eq!(次, [false, false, true], "第三次（bad 再诊断）取自账本");
    assert!(
        条目
            .iter()
            .any(|x| x.to_string().contains("W-diag-two-judgments"))
    );
    assert!(
        条目
            .iter()
            .any(|x| x.as_array().is_some_and(|a| a.is_empty()))
    );
}

/// 数宿主函数被调了几次的 diagnose（版本可换）
fn 计数表(版本: &str, 次: Rc<Cell<usize>>) -> TransformTable {
    let inner = jpp_lib::diagnose_transform();
    let mut t = TransformTable::new();
    t.register(HostTransform {
        name: "diagnose".into(),
        version: 版本.into(),
        taint_out: HostTaint::Inherit,
        doc: inner.doc.clone(),
        reads_external_state: inner.reads_external_state,
        run: Box::new(move |ins: &[Json]| {
            次.set(次.get() + 1);
            (inner.run)(ins)
        }),
    });
    t
}

fn 直跑(
    p: &Program,
    t: &TransformTable,
    ledger: &mut Ledger,
    重放: bool,
) -> Result<Outcome, String> {
    let fx = 空夹具();
    let 调用 = Cell::new(0);
    let calib = 库(&fx);
    let acts = ActionRegistry::new();
    let mut it =
        jpp::interp::Interp::new(端口(&fx, &调用), ledger, &calib, &acts, p.budget.clone());
    if 重放 {
        it = it.audit_replay();
    }
    it.passes = Passes::default();
    it.set_transforms(t);
    it.run(p).map_err(|e| e.render())
}

#[test]
fn 同一候选重放时诊断结果从账本取不重算() {
    let p = 编译(宿主诊断);
    let 次 = Rc::new(Cell::new(0));
    let t = 计数表(jpp_lib::RULES_VERSION, 次.clone());
    let mut ledger = Ledger::new();
    let first = 直跑(&p, &t, &mut ledger, false).unwrap();
    let 首跑次 = 次.get();
    assert!(首跑次 >= 2, "首跑至少算两道题：{首跑次}");
    let again = 直跑(&p, &t, &mut ledger, true).unwrap();
    assert_eq!(次.get(), 首跑次, "重放不重算");
    assert_eq!(again.value_json(), first.value_json());
    assert!(again.cost.replayed >= 2);
}

/// 规则版本进键；只凭账本重放时宿主变换缺记录即 `E-replay`，报文写明变换名与版本（主会话裁定 2026-09-29
/// 第二十一条；B35 (1)、`12` §2.8）。旧规则的账本在新规则下重放：键不同、账本里没有 → `E-replay`，
/// 新规则一次都不算，旧产物也不被当成新规则的产物。
/// 账本头 `W-header: lib_version` 那一面：运行时填头归步 27（题库代理），填好后补成端到端测试；
/// 现在账本口那一面在 `lib_version_header.rs`。
#[test]
fn 规则版本不同的旧账本重放报缺记录() {
    let p = 编译(宿主诊断);
    let 次 = Rc::new(Cell::new(0));
    let 旧表 = 计数表("b13-0", 次.clone());
    let mut ledger = Ledger::new();
    直跑(&p, &旧表, &mut ledger, false).unwrap();
    let 新次 = Rc::new(Cell::new(0));
    let 新表 = 计数表(jpp_lib::RULES_VERSION, 新次.clone());
    let r = 直跑(&p, &新表, &mut ledger, true);
    let e = r.expect_err("应报 E-replay");
    assert!(e.contains("E-replay"), "{e}");
    assert!(e.contains("宿主变换 diagnose"), "{e}");
    assert!(
        e.contains(&format!("版本 {}", jpp_lib::RULES_VERSION)),
        "{e}"
    );
    assert_eq!(新次.get(), 0, "新规则一次都不算");
}
