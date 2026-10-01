//! Z0411 / Z0412：缺 mid 报错补齐与「画像没有 δ」的处理。
//!
//! - Z0411 (ii)：`Profile::delta_mid_missing()` = 任一列有 δ（尾段或中段）而中段三列不齐，与 `jpp profile check` 同口径；
//!   只有部分列的画像也算缺，`cut` 兜底报 `E-delta-mid`。（Z0411 (i) 裂变缺 mid 报错的测试在 `fission_bu.rs`。）
//! - Z0412（裁定五十六，读法乙）：画像完全没有 δ（如 `profiles/judge-claude-p.json`，或没加载画像）时不编 0 去加宽带：
//!   `cut` 照线切、不加迁移带，出口带 `delta_unknown`，`releases` 为假，`--guard` 下不放行不可逆 do、报 `W-delta-unknown`。
//!
//! 预注册：`地基/过程记录/工程-Z0334-δ分层.md` §十七（`93e8d41b2`）与 §17.5 修订（`0c7526575`）。

// 伴随题「最缺哪类」是 K 选一：替身经 common::伴随中性judge 给伴随元题中性读数（Z0398 返修，过程记录 5.23）
mod common;
use std::cell::Cell;
use std::path::PathBuf;

use jpp::effects::{CalibStore, FnPort, JudgeResult, Ports, Profile};
use jpp::interp::{ActionRegistry, TaintOut};
use jpp::ledger::Ledger;
use jpp::truth::{CertifyMethod, ImportOptions, LabelRow, import_labels};
use jpp::value::{Answer, Question, State, Taint, Value};
use jpp::{lower, run, syntax::parse};
use serde_json::{Value as Json, json};

fn 画像文件(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(format!("../../profiles/{name}"))
}

/// 读数恒为 `p` 的判断端口
fn 端口(p: f64, calls: &Cell<u64>) -> Ports<'_> {
    Ports::new().with(common::伴随中性judge(
        "m",
        move |_s: &State, qs: &[&Question]| {
            calls.set(calls.get() + 1);
            Ok(JudgeResult {
                answers: qs.iter().map(|_| Answer::Noul(p)).collect(),
                tokens: 0,
                cost: 0.0,
                mode_share: vec![],
                perms: vec![],
                confidence: vec![],
            })
        },
    ))
}

fn 动作() -> ActionRegistry {
    let mut a = ActionRegistry::new();
    // 第三个参数 false = 不可逆
    a.register("退款", 0.0, false, TaintOut::Trusted, |_| {
        Ok(Value::Text("已退".into(), Taint::Trusted.into()))
    });
    a
}

const 路由: &str = r#"
budget {calls: 4, cost: 1, depth: 8};
let e = cut(judge(state(mat("顾客说要退款")), test("该退吗", "k")));
handle(e, {
    act: fn() { "act" },
    ignore: fn() { "ignore" },
    unsure: fn(u) { consume(u, "drop"); unsure_cause(u) }})
"#;

const 放行: &str = r#"
budget {calls: 4, cost: 1, depth: 8};
handle(cut(judge(state(mat("顾客说要退款")), test("该退吗", "k"))), {
    act: fn() { (do("退款", [], 0)) },
    ignore: fn() { "没退" },
    unsure: fn(u) { consume(u, "drop"); "没退" }})
"#;

/// 校准库：键 `k` 一条上岗记录、线 hi 0.60 / lo 0.10、**没有 δ**，画像给定
fn 库(profile: Profile) -> CalibStore {
    let mut s = CalibStore::new();
    s.profile = profile;
    s.put("k", 0.60, 0.10, 60, "上岗", None).unwrap();
    s
}

fn 跑(src: &str, calib: &CalibStore, p: f64, guard: bool) -> Result<jpp::Outcome, String> {
    let mut program = lower(&parse(src).expect("解析")).expect("lower");
    program.entry.guard = guard;
    let calls = Cell::new(0u64);
    run(
        &program,
        端口(p, &calls),
        calib,
        &动作(),
        &mut Ledger::new(),
    )
    .map_err(|e| e.render())
}

fn 告警数(o: &jpp::Outcome, 码: &str) -> usize {
    o.trace
        .warnings
        .iter()
        .filter(|w| w.starts_with(&format!("{码}:")))
        .count()
}

/// 画像 JSON：`cols` 里的列给尾段 / 中段
fn 画像(tail: &[&str], mid: &[&str]) -> Profile {
    let mut d = serde_json::Map::new();
    for c in ["noul", "choice_prob_chosen", "score"] {
        let mut o = serde_json::Map::new();
        if tail.contains(&c) {
            o.insert("immediate".into(), json!({"p99": 0.04}));
        }
        if mid.contains(&c) {
            o.insert("mid".into(), json!({"immediate": {"p99": 0.1281}}));
        }
        if !o.is_empty() {
            d.insert(c.into(), Json::Object(o));
        }
    }
    let mut j = json!({"model_version": "fixed-0"});
    if !d.is_empty() {
        j["delta"] = Json::Object(d);
    }
    Profile::from_json(&j).unwrap()
}

const 三列: [&str; 3] = ["noul", "choice_prob_chosen", "score"];

/// Z0411 (ii)：任一列有 δ 而中段三列不齐就算缺 mid
#[test]
fn 部分列画像判为缺mid() {
    assert!(
        画像(&["noul"], &[]).delta_mid_missing(),
        "只有 noul 尾段一列"
    );
    assert!(
        画像(&[], &["score"]).delta_mid_missing(),
        "只有 score 中段一列"
    );
    assert!(
        画像(&三列, &["noul", "score"]).delta_mid_missing(),
        "尾段齐、中段缺一列"
    );
    assert!(
        画像(&三列, &[]).delta_mid_missing(),
        "只有尾段（旧画像的形状）"
    );
    assert!(!画像(&[], &三列).delta_mid_missing(), "中段三列齐");
    assert!(
        !画像(&[], &[]).delta_mid_missing(),
        "两段都没有：不算缺（按裁定五十六另行处理）"
    );
    assert!(!Profile::untested().delta_mid_missing(), "没加载画像同上");
}

/// Z0411 (ii)：部分列画像下 `cut` 兜底（记录没有 δ）报 `E-delta-mid`
#[test]
fn 部分列画像cut兜底报缺mid() {
    let e = 跑(路由, &库(画像(&["noul"], &[])), 0.7, false).expect_err("部分列画像缺 mid 要报错");
    assert!(e.contains("E-delta-mid"), "{e}");
    let e = 跑(路由, &库(画像(&[], &["score"])), 0.7, false).expect_err("只有一列中段也算缺");
    assert!(e.contains("E-delta-mid"), "{e}");
}

/// Z0412（裁定五十六，读法乙）：`judge-claude-p` 画像完全没有 δ，照样能跑；照线切、不加迁移带（0.62 过 0.60 出 act），
/// 出口带 `delta_unknown`、`releases` 为假；不开 `--guard` 不报告警，开了报 `W-delta-unknown` 一次
#[test]
fn judge_claude_p没有delta照线切带位不放行() {
    let p = Profile::load(&画像文件("judge-claude-p.json")).expect("judge-claude-p 能加载");
    assert!(p.delta.get().is_none() && p.delta_tail.get().is_none());
    assert!(
        !p.delta_mid_missing(),
        "两段都没有不算缺 mid，不报 E-delta-mid"
    );
    let c = 库(p);
    // 读数 0.62：带宽若按某个 δ>0.02 加宽会落进带内出 band；照线切（不加带）就是 act
    let o = 跑(路由, &c, 0.62, false).expect("能跑");
    assert_eq!(o.value_json(), json!("act"));
    assert_eq!(o.exits.len(), 1);
    assert_eq!(o.exits[0]["delta_unknown"], json!(true), "{}", o.exits[0]);
    assert_eq!(o.exits[0]["releases"], json!(false), "{}", o.exits[0]);
    assert_eq!(告警数(&o, "W-delta-unknown"), 0, "{:?}", o.trace.warnings);
    let o = 跑(路由, &c, 0.62, true).expect("--guard 下也能跑（路由不受影响）");
    assert_eq!(o.value_json(), json!("act"));
    assert_eq!(告警数(&o, "W-delta-unknown"), 1, "{:?}", o.trace.warnings);
    assert!(
        o.trace
            .warnings
            .iter()
            .any(|w| w.starts_with("W-delta-unknown") && w.contains("画像也没有 δ")),
        "{:?}",
        o.trace.warnings
    );
}

fn 导入选项() -> ImportOptions {
    ImportOptions {
        alpha: 0.1,
        conf_delta: 0.1,
        spot_check_min: 0.9,
        spot_check_conf: 0.95,
        abstain_warn: 0.1,
        batch: "z0425".into(),
        seed: 20260930,
        extent_min_disagree: 3,
        extent_same_dir: 0.8,
        extent_same_tier: 2.0 / 3.0,
        scope_quantiles: (0.01, 0.99),
        scope_margins: Default::default(),
        class_min_sources: 2,
        alpha_trial: Some(0.25),
        certify: CertifyMethod::Split,
        step: None,
        sequential: None,
    }
}

/// 发行画像下用 240 条两极构造真值认证出键 `k` 的正式证书（与 `bypass_b72_trial.rs` 同一做法）
fn 正式库() -> CalibStore {
    let mut s = CalibStore::new();
    s.profile = Profile::load(&画像文件("jev-1.13.0.json")).unwrap();
    let rows: Vec<LabelRow> = (0..240)
        .map(|i| {
            let yes = i % 2 == 0;
            serde_json::from_value(json!({
                "key": "k", "item": format!("m{i}"), "p": if yes { 0.97 } else { 0.03 },
                "label": yes, "source": "computed", "text": "顾客说要退款"
            }))
            .unwrap()
        })
        .collect();
    import_labels(&mut s, &rows, &导入选项()).unwrap();
    assert_eq!(s.get("k").status, "上岗");
    s
}

/// Z0412 / Z0425：`--guard` 下，δ 未知的出口不放行不可逆 do。两份库用同一张正式证书，差在 δ 从哪都取不到：
/// 一份带 δ（记录与证书都有，画像是发行画像）；另一份把记录与证书的 δ 都去掉，**并且把画像换成没有 δ 的
/// `judge-claude-p`**（只去掉记录的 δ、画像仍是发行画像时，会取画像中段、来源 `profile_mid`，同样带位不放行）。
/// 前者放行、执行 `do`；后者 `J-08` 拦下，出口等级仍是 Certified、带 `delta_unknown`、来源 `none`，
/// 且没有 `scope_unknown`、`untested` 这些别的不放行位——所以拦下是因为 δ 未知，不是因为等级或别的位
#[test]
fn 正式证书去掉delta后守卫下拦住不可逆do() {
    let 有 = 正式库();
    let o = 跑(放行, &有, 0.97, true).expect("正式证书、δ 已知：放行");
    assert_eq!(o.value_json()["content"], json!("已退"));
    assert_eq!(o.exits[0]["grade"], json!("Certified"), "{}", o.exits[0]);
    assert_eq!(o.exits[0]["releases"], json!(true), "{}", o.exits[0]);
    assert!(o.exits[0].get("delta_unknown").is_none(), "{}", o.exits[0]);

    let mut 无 = 有.clone();
    无.profile = Profile::load(&画像文件("judge-claude-p.json")).unwrap();
    for rec in 无.records.values_mut() {
        rec.delta = None;
        for c in rec.certs.values_mut() {
            if let Some(sel) = c.selection.as_mut() {
                sel.delta = None;
            }
        }
    }
    let e = 跑(放行, &无, 0.97, true).expect_err("δ 未知的出口不该放行不可逆 do");
    assert!(e.contains("J-08"), "{e}");
    let o = 跑(路由, &无, 0.97, true).expect("路由照常");
    assert_eq!(o.value_json(), json!("act"));
    assert_eq!(
        o.exits[0]["grade"],
        json!("Certified"),
        "等级没变：{}",
        o.exits[0]
    );
    assert_eq!(o.exits[0]["delta_unknown"], json!(true), "{}", o.exits[0]);
    assert_eq!(o.exits[0]["delta_source"], json!("none"), "{}", o.exits[0]);
    assert_eq!(o.exits[0]["releases"], json!(false), "{}", o.exits[0]);
    assert!(o.exits[0].get("scope_unknown").is_none(), "{}", o.exits[0]);
    assert!(o.exits[0].get("untested").is_none(), "{}", o.exits[0]);
}

/// Z0425：报告行分开 `delta_unknown` 的两种来源——画像没有 δ（照线切、没加带）为 `none`；
/// 记录没有 δ、已按画像中段加带为 `profile_mid`
#[test]
fn 报告行分开delta_unknown的来源() {
    let o = 跑(
        路由,
        &库(Profile::load(&画像文件("judge-claude-p.json")).unwrap()),
        0.62,
        false,
    )
    .unwrap();
    assert_eq!(o.exits[0]["delta_source"], json!("none"), "{}", o.exits[0]);
    let o = 跑(
        路由,
        &库(Profile::load(&画像文件("jev-1.13.0.json")).unwrap()),
        0.62,
        false,
    )
    .unwrap();
    assert_eq!(o.exits[0]["delta_unknown"], json!(true), "{}", o.exits[0]);
    assert_eq!(
        o.exits[0]["delta_source"],
        json!("profile_mid"),
        "{}",
        o.exits[0]
    );
}

/// Z0412：没加载画像（`Profile::untested()`）同样算「完全没有 δ」：照线切、带 `delta_unknown`、`--guard` 下报告警
#[test]
fn 没加载画像同样带位() {
    let c = 库(Profile::untested());
    let o = 跑(路由, &c, 0.62, true).expect("能跑");
    assert_eq!(o.value_json(), json!("act"));
    assert_eq!(o.exits[0]["delta_unknown"], json!(true), "{}", o.exits[0]);
    assert_eq!(o.exits[0]["releases"], json!(false));
    assert_eq!(告警数(&o, "W-delta-unknown"), 1, "{:?}", o.trace.warnings);
}

/// 对照：发行画像三列中段齐，记录没有 δ 时取中段 δ 0.1281 加带，0.62 落在 0.60 + δ 以内出 band（不是 act），
/// 仍带 `delta_unknown`（B187：记录没有 δ 本来就置这一位），但不报「画像也没有 δ」的告警
#[test]
fn 对照发行画像取中段加带() {
    let p = Profile::load(&画像文件("jev-1.13.0.json")).unwrap();
    let o = 跑(路由, &库(p), 0.62, true).expect("能跑");
    assert_eq!(o.value_json(), json!("band"));
    assert!(
        !o.trace
            .warnings
            .iter()
            .any(|w| w.starts_with("W-delta-unknown") && w.contains("画像也没有 δ")),
        "{:?}",
        o.trace.warnings
    );
}

// ---------------------------------------------------------------- Z0425：order 并档容差

/// 读数按材料：「甲」0.70，「乙」0.62（差 0.08）
fn 端口_按材料(calls: &Cell<u64>) -> Ports<'_> {
    Ports::new().with(common::伴随中性judge(
        "m",
        move |s: &State, qs: &[&Question]| {
            calls.set(calls.get() + 1);
            let on: String = s.on.iter().map(|m| m.text()).collect();
            let p = if on.contains('甲') { 0.70 } else { 0.62 };
            Ok(JudgeResult {
                answers: qs.iter().map(|_| Answer::Noul(p)).collect(),
                tokens: 0,
                cost: 0.0,
                mode_share: vec![],
                perms: vec![],
                confidence: vec![],
            })
        },
    ))
}

const 排序: &str = r#"
budget {calls: 4, cost: 1, depth: 8};
let q = test("该退吗", "k");
order([judge(state(mat("甲")), q), judge(state(mat("乙")), q)])
"#;

fn 跑排序(calib: &CalibStore, guard: bool) -> Result<jpp::Outcome, String> {
    let mut program = lower(&parse(排序).expect("解析")).expect("lower");
    program.entry.guard = guard;
    let calls = Cell::new(0u64);
    run(
        &program,
        端口_按材料(&calls),
        calib,
        &动作(),
        &mut Ledger::new(),
    )
    .map_err(|e| e.render())
}

fn 空库(p: Profile) -> CalibStore {
    let mut s = CalibStore::new();
    s.profile = p;
    s
}

/// 记录没有 δ：取画像中段（0.1281），0.70 与 0.62 并成一档；报告 orders 行 `tol_source: profile`
#[test]
fn order记录没有delta取画像中段并档() {
    let o = 跑排序(
        &空库(Profile::load(&画像文件("jev-1.13.0.json")).unwrap()),
        true,
    )
    .unwrap();
    assert_eq!(o.value_json(), json!([[0, 1]]));
    assert_eq!(o.orders.len(), 1, "{:?}", o.orders);
    assert_eq!(o.orders[0]["tol"], json!(0.1281));
    assert_eq!(o.orders[0]["tol_source"], json!("profile"));
    assert!(
        !o.trace
            .warnings
            .iter()
            .any(|w| w.starts_with("W-delta-unknown"))
    );
}

/// 记录有 δ（0.05）：先取记录的，差 0.08 分两档；`tol_source: record`
#[test]
fn order先取记录delta() {
    let mut c = 空库(Profile::load(&画像文件("jev-1.13.0.json")).unwrap());
    c.put("k", 0.9, 0.1, 60, "上岗", Some(0.05)).unwrap();
    let o = 跑排序(&c, false).unwrap();
    assert_eq!(o.value_json(), json!([[0], [1]]));
    assert_eq!(o.orders[0]["tol"], json!(0.05));
    assert_eq!(o.orders[0]["tol_source"], json!("record"));
}

/// 画像没有 δ（judge-claude-p；没加载画像同样）：不并档，`tol_source: unknown`，`--guard` 下报 `W-delta-unknown` 一次
#[test]
fn order没有delta不并档并报告() {
    for p in [
        Profile::load(&画像文件("judge-claude-p.json")).unwrap(),
        Profile::untested(),
    ] {
        let o = 跑排序(&空库(p.clone()), false).unwrap();
        assert_eq!(o.value_json(), json!([[0], [1]]));
        assert_eq!(o.orders[0]["tol"], json!(0.0));
        assert_eq!(o.orders[0]["tol_source"], json!("unknown"));
        assert_eq!(告警数(&o, "W-delta-unknown"), 0, "不开 --guard 不报");
        let o = 跑排序(&空库(p), true).unwrap();
        assert_eq!(告警数(&o, "W-delta-unknown"), 1, "{:?}", o.trace.warnings);
    }
}

/// 画像只有尾段（缺 mid）且记录没有 δ：order 按裁定四十五报 `E-delta-mid`，与 cut、裂变一致
#[test]
fn order画像缺mid报错() {
    let e = 跑排序(&空库(画像(&三列, &[])), false).expect_err("缺 mid 要报错");
    assert!(e.contains("E-delta-mid"), "{e}");
}
