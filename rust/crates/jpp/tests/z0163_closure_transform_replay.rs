//! Z0163：闭包形式的 `transform` 在只凭账本重放时的缺记录处置。
//!
//! 面一：能算出键的闭包变换，账本里查不到记录时报 `E-replay`，报文写明是哪个变换（名字、站点起点、方法哈希）。
//! 面二：捕获环境指纹化不了的那一路（`W-no-cache`，不进账本）照旧现算，值与首跑相同，
//! 并在 `trace.warnings` 里逐站点标明这一项没有经账本核对（`W-replay-unchecked`，一站一条）。
//!
//! 依据：B35 (1)；`12` §2.4 订正注、§2.8「重放取账本」；主会话裁定 2026-09-29 第二十一、二十二条；
//! 预注册 `地基/过程记录/工程-Z0163-闭包变换重放.md` §三。

// 伴随题「最缺哪类」是 K 选一：替身经 common::伴随中性judge 给伴随元题中性读数（Z0398 返修，过程记录 5.23）
mod common;
use jpp::effects::{CalibStore, EffectError, FnPort, GenResult, JudgeResult, Ports, ReplayPorts};
use jpp::interp::ActionRegistry;
use jpp::ledger::{Entry, Ledger};
use jpp::value::Answer;
use jpp::{Outcome, lower, run, run_replay, syntax::parse};
use serde_json::json;

fn 端口<'a>() -> Ports<'a> {
    Ports::new()
        .with(common::伴随中性judge("fixed-0", |_s, qs| {
            Ok::<_, EffectError>(JudgeResult {
                answers: qs.iter().map(|_| Answer::Noul(0.9)).collect(),
                tokens: 0,
                cost: 0.0,
                mode_share: vec![],
                perms: vec![],
                confidence: vec![],
            })
        }))
        .with(FnPort::generate("fixed-0", |_p, _c, _n, _r| {
            Ok(GenResult {
                outputs: vec![json!("候选")],
                ..Default::default()
            })
        }))
        .with(FnPort::ask("fixed-0", |_s, _q| Ok(Some(Answer::Noul(0.9)))))
}

fn 首跑(src: &str, ledger: &mut Ledger) -> Outcome {
    let program = lower(&parse(src).expect("parse")).expect("lower");
    let o = run(
        &program,
        端口(),
        &CalibStore::new(),
        &ActionRegistry::new(),
        ledger,
    )
    .unwrap_or_else(|e| panic!("{}", e.render()));
    ledger.rebuild_index();
    o
}

fn 重放(src: &str, ledger: &mut Ledger) -> Result<Outcome, String> {
    let program = lower(&parse(src).expect("parse")).expect("lower");
    run_replay(
        &program,
        ReplayPorts::ports("fixed-0"),
        &CalibStore::new(),
        &ActionRegistry::new(),
        ledger,
    )
    .map_err(|e| e.render())
}

fn 账本里的变换条数(ledger: &Ledger) -> usize {
    ledger
        .entries
        .iter()
        .filter(|e| matches!(e, Entry::Effect { kind, .. } if kind == "transform"))
        .count()
}

/// 两份程序长度、站点位置全同，只有闭包正文里的一个字不同：方法哈希不同 → 键不同 → 账本里没有。
fn 闭包程序(字: &str) -> String {
    format!(
        "budget {{calls: 2, cost: 0, depth: 8}};\nfn 变(m) {{ \"{字} \" + content(m) }}\nlet t = transform(变, mat(\"丙\"));\n{{t: content(t)}}"
    )
}

/// 面一：账本是别的程序（闭包正文不同）记的，重放这份程序时缺记录，报 `E-replay`，报文写明是闭包变换。
#[test]
fn 面一_闭包变换缺记录报_e_replay() {
    let mut ledger = Ledger::new();
    首跑(&闭包程序("旧"), &mut ledger);
    assert_eq!(账本里的变换条数(&ledger), 1, "首跑写进一条变换记录");
    let 条数 = ledger.entries.len();
    let e = 重放(&闭包程序("新"), &mut ledger).expect_err("缺记录应报错，不该现算");
    assert!(e.contains("E-replay"), "{e}");
    assert!(e.contains("闭包变换 变"), "报文要写明是哪个变换：{e}");
    assert!(e.contains("方法哈希"), "报文要带方法哈希：{e}");
    assert!(e.contains("站点起点"), "报文要带站点：{e}");
    assert_eq!(
        ledger.entries.len(),
        条数,
        "重放不往账本里补记录（没有现算）"
    );
}

/// 面一之二：空账本重放，同样报 `E-replay`（匿名方法写「匿名方法」）。
#[test]
fn 面一_空账本重放匿名闭包() {
    let src = "budget {calls: 2, cost: 0, depth: 8};\nlet t = transform(fn(m) { content(m) }, mat(\"丙\"));\n{t: content(t)}";
    let mut 空 = Ledger::new();
    let e = 重放(src, &mut 空).expect_err("空账本缺记录应报错");
    assert!(e.contains("E-replay") && e.contains("匿名方法"), "{e}");
}

/// 对照：同一程序首跑再只凭账本重放，键命中，不报错、值相同、变换记为复用。
#[test]
fn 对照_同一程序一致重放不报() {
    let mut ledger = Ledger::new();
    let first = 首跑(&闭包程序("变"), &mut ledger);
    let again = 重放(&闭包程序("变"), &mut ledger).expect("一致重放");
    assert_eq!(again.value_json(), first.value_json());
    assert_eq!(again.cost.calls, 0);
    assert!(again.cost.replayed >= 1);
    assert!(
        !again
            .trace
            .warnings
            .iter()
            .any(|w| w.starts_with("W-replay-unchecked")),
        "键能算出的变换不该报未核对：{:?}",
        again.trace.warnings
    );
}

/// 面二的程序：闭包捕获一个读数（指纹取不到，`W-no-cache`）；同一站点在 `map` 里执行三次。
const 不进账本的程序: &str = r#"
budget {calls: 2, cost: 0, depth: 8};
let r = judge(state(mat("被判的")), test("行吗", "k"));
fn g(m) { let _ = r; content(m) }
{a: map(["甲", "乙", "丙"], fn(x) { content(transform(g, mat(x))) })}
"#;

/// 面二：`W-no-cache` 那一路只凭账本重放时照旧现算，值与首跑相同、不报 `E-replay`，
/// 报告里逐站点标明没有经账本核对（同站点多次执行只一条，首跑不发）。
#[test]
fn 面二_w_no_cache_照旧现算并标明未核对() {
    let mut ledger = Ledger::new();
    let first = 首跑(不进账本的程序, &mut ledger);
    assert_eq!(账本里的变换条数(&ledger), 0, "这一路不进账本");
    assert!(
        first
            .trace
            .warnings
            .iter()
            .any(|w| w.starts_with("W-no-cache")),
        "首跑要走到 W-no-cache：{:?}",
        first.trace.warnings
    );
    assert!(
        !first
            .trace
            .warnings
            .iter()
            .any(|w| w.starts_with("W-replay-unchecked")),
        "首跑不发未核对警告"
    );

    let again =
        重放(不进账本的程序, &mut ledger).expect("W-no-cache 一路重放照旧现算，不报 E-replay");
    assert_eq!(again.value_json(), first.value_json());
    assert_eq!(again.cost.calls, 0);
    let 标 = again
        .trace
        .warnings
        .iter()
        .filter(|w| w.starts_with("W-replay-unchecked"))
        .collect::<Vec<_>>();
    assert_eq!(标.len(), 1, "一站一条：{:?}", again.trace.warnings);
    assert!(标[0].contains("闭包变换 g"), "写明是哪个变换：{}", 标[0]);
    assert!(
        标[0].contains("@") && 标[0].contains("方法哈希") && 标[0].contains("没有经账本核对"),
        "{}",
        标[0]
    );
    assert_eq!(
        账本里的变换条数(&ledger),
        0,
        "重放后账本里仍没有这一路的记录"
    );
}

/// 面一之三（守卫顺序）：闭包一被调用就会出一个另外的可观察错误（`1 / 0`）。只凭账本重放缺记录时，
/// 收到的必须是 `E-replay`，不是闭包自己的错——证明守卫在闭包执行之前返回，闭包没被调用。
#[test]
fn 面一_守卫先于闭包执行() {
    let 会出错 = "budget {calls: 2, cost: 0, depth: 8};\nfn 炸(m) { 1 / 0 }\nlet t = transform(炸, mat(\"丙\"));\n{t: content(t)}";
    // 前提：这份程序真跑（首跑）时闭包自己的错确实会出来，且不是 E-replay
    let mut 首 = Ledger::new();
    let program = lower(&parse(会出错).expect("parse")).expect("lower");
    let 自己的错 = run(
        &program,
        端口(),
        &CalibStore::new(),
        &ActionRegistry::new(),
        &mut 首,
    )
    .err()
    .map(|e| e.render())
    .expect("闭包体一被调用就该出错");
    assert!(
        自己的错.contains("E-rt-int"),
        "前提：闭包自己的错是除以零：{自己的错}"
    );
    // 账本是别的程序（同一个变换位置、正文不同）记的：只凭账本重放缺记录
    let mut ledger = Ledger::new();
    首跑(&闭包程序("旧"), &mut ledger);
    let e = 重放(会出错, &mut ledger).expect_err("缺记录应报错");
    assert!(e.contains("E-replay"), "守卫要先于闭包执行：{e}");
    assert!(e.contains("闭包变换 炸"), "{e}");
    assert!(!e.contains("E-rt-int"), "收到的不该是闭包自己的错：{e}");
}

/// 续接（`--resume`，即带非空账本的 `run`，不是审计重放）：闭包缺记录照常现算，不报 `E-replay`，
/// 现算的结果写进账本。抓「`audit.on || 账本非空`」这类错实现。
#[test]
fn 续接_闭包缺记录照常现算() {
    let mut ledger = Ledger::new();
    首跑(&闭包程序("旧"), &mut ledger);
    assert_eq!(账本里的变换条数(&ledger), 1);
    // 同一个账本、换了正文的程序：续接（run）查不到，照常现算
    let o = 首跑(&闭包程序("新"), &mut ledger);
    assert_eq!(o.value_json()["t"], json!("新 丙"));
    assert_eq!(o.cost.calls, 0);
    assert_eq!(账本里的变换条数(&ledger), 2, "续接现算的结果记进账本");
    // 续接得来的账本随后能只凭账本重放这份新程序
    let again = 重放(&闭包程序("新"), &mut ledger).expect("续接后的账本可重放");
    assert_eq!(again.value_json()["t"], json!("新 丙"));
}

/// 命令行：`run --replay --json` 里 `W-replay-unchecked` 带 span（报文用 `@偏移`），作者在编辑器里点得到那个变换的站点。
/// 程序捕获一道题（`Question` 指纹化不了 → `W-no-cache`），不需要判断器与夹具。
#[test]
fn 命令行_未核对警告带_span() {
    use std::process::Command;
    let d = std::env::temp_dir().join(format!("jpp-z0163-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    std::fs::write(
        d.join("p.jpp"),
        "budget {calls: 2, cost: 0, depth: 8};\nlet q = test(\"行吗\", \"k\");\nfn g(m) { let _ = q; content(m) }\n{a: content(transform(g, mat(\"材料\")))}\n",
    )
    .unwrap();
    let jpp = |args: &[&str]| {
        Command::new(env!("CARGO_BIN_EXE_jpp"))
            .current_dir(&d)
            .args(args)
            .output()
            .unwrap()
    };
    let 首 = jpp(&[
        "run",
        "p.jpp",
        "--ledger-out",
        "l.jsonl",
        "--output",
        "r1.json",
    ]);
    assert!(
        首.status.success(),
        "{}",
        String::from_utf8_lossy(&首.stderr)
    );
    let 重 = jpp(&[
        "run", "p.jpp", "--replay", "l.jsonl", "--json", "--output", "r2.json",
    ]);
    let err = String::from_utf8_lossy(&重.stderr).to_string();
    assert!(重.status.success(), "{err}");
    let 诊断: Vec<serde_json::Value> = err
        .lines()
        .filter(|l| l.starts_with('{'))
        .filter_map(|l| serde_json::from_str(l).ok())
        .collect();
    let 条 = 诊断
        .iter()
        .find(|j| j["code"] == "W-replay-unchecked")
        .unwrap_or_else(|| panic!("--json 里没有 W-replay-unchecked：{err}"));
    assert!(!条["span"].is_null(), "span 应非空：{条}");
    assert!(条["span"]["line"].as_u64().is_some(), "{条}");
    let 首报: serde_json::Value =
        serde_json::from_slice(&std::fs::read(d.join("r1.json")).unwrap()).unwrap();
    let 重报: serde_json::Value =
        serde_json::from_slice(&std::fs::read(d.join("r2.json")).unwrap()).unwrap();
    assert_eq!(重报["value"], 首报["value"]);
    let _ = std::fs::remove_dir_all(&d);
}
