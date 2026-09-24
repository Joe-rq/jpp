//! B29：J-03 同时约束程序与宿主。宿主 `put` 只写夹具记录；夹具线给出的出口带
//! `W-fixture-line`，不算放行不可逆 `do` 的可信合取项。`cut(…, {cost: [fp, fn]})`
//! 只取按该代价矩阵认证过的证书的线，没有就是冷。

mod common;
use std::cell::RefCell;

use jpp_core::effects::{CalibStore, Cert, Client, EffectError, GenResult, JudgeResult, LabelSource};
use jpp_core::ledger::Ledger;
use jpp_core::value::{Answer, Question, State};
use jpp_core::{ActionRegistry, TaintOut, run};
use jpp_frontend::{lower, parse};
use serde_json::Value as Json;

struct 定值(f64, RefCell<u64>);
impl Client for 定值 {
    fn model_id(&self) -> String { "fixed-0".into() }
    fn judge(&mut self, _s: &State, qs: &[&Question]) -> Result<JudgeResult, EffectError> {
        *self.1.borrow_mut() += 1;
        Ok(JudgeResult { answers: qs.iter().map(|_| Answer::Noul(self.0)).collect(), tokens: 0, cost: 0.0, mode_share: vec![], perms: vec![] })
    }
    fn generate(&mut self, _p: &str, _c: &[Json], _n: usize, _r: u64) -> Result<GenResult, EffectError> { Err(EffectError("不该 gen".into())) }
    fn ask(&mut self, _s: &State, _q: &Question) -> Result<Option<Answer>, EffectError> { Ok(None) }
    fn calls(&self) -> u64 { *self.1.borrow() }
}

const 发信: &str = r#"
budget {calls: 2, cost: 0, depth: 8};
let e = cut(judge(state(mat("内部材料")), test("可以发吗", "k")));
handle(e, {act: fn() { content(do("发邮件", [], 0)) },
           ignore: fn() { "没发" },
           unsure: fn(u) { consume(u, "drop"); "没发" }})
"#;

fn 跑(src: &str, calib: &CalibStore, p: f64) -> Result<(Json, Vec<String>), String> {
    let program = lower(&parse(src).expect("解析")).expect("lower");
    let mut a = ActionRegistry::new();
    a.register("发邮件", 0.0, false, TaintOut::Trusted, |_| Ok(jpp_core::value::Value::text("已发")));
    let mut c = 定值(p, RefCell::new(0));
    let mut l = Ledger::new();
    run(&program, &mut c, calib, &a, &mut l).map(|o| (o.value_json(), o.trace.warnings.clone())).map_err(|e| e.render())
}

#[test]
fn 夹具线不放行不可逆do() {
    let mut 夹具 = CalibStore::new();
    夹具.put("k", 0.8, 0.2, 50, "上岗").unwrap();
    assert!(夹具.records["k"].fixture, "put 写的是夹具记录");
    let e = 跑(发信, &夹具, 0.95).expect_err("夹具线的 Act 不能单独放行不可逆 do");
    assert!(e.contains("J-08") && e.contains("夹具线"), "{e}");

    let mut 认证 = CalibStore::new();
    common::certified(&mut 认证, "k", 0.8, 0.2, 50);
    let (v, w) = 跑(发信, &认证, 0.95).expect("认证线的 Act 放行");
    assert_eq!(v, Json::from("已发"));
    assert!(!w.iter().any(|x| x.starts_with("W-fixture-line")), "{w:?}");
}

#[test]
fn 夹具线出口照常路由并告警() {
    let mut 夹具 = CalibStore::new();
    夹具.put("k", 0.8, 0.2, 50, "上岗").unwrap();
    let src = r#"
budget {calls: 2, cost: 0, depth: 8};
let e = cut(judge(state(mat("材料")), test("行吗", "k")));
handle(e, {act: fn() { "act" }, ignore: fn() { "ig" }, unsure: fn(u) { consume(u, "drop"); "un" }})
"#;
    let (v, w) = 跑(src, &夹具, 0.95).expect("可逆路径照常");
    assert_eq!(v, Json::from("act"));
    assert!(w.iter().any(|x| x.starts_with("W-fixture-line")), "{w:?}");
}

#[test]
fn cut代价参数只取同代价的证书() {
    let src = r#"
budget {calls: 2, cost: 0, depth: 8};
let e = cut(judge(state(mat("材料")), test("行吗", "k")), {cost: [10, 1]});
handle(e, {act: fn() { "act" }, ignore: fn() { "ig" }, unsure: fn(u) { let c = unsure_cause(u); consume(u, "drop"); c }})
"#;
    // 没有这个代价矩阵的证书 → 冷（不借无代价证书的线）
    let mut c = CalibStore::new();
    common::certified(&mut c, "k", 0.5, 0.2, 50);
    let (v, w) = 跑(src, &c, 0.6).expect("跑得完");
    assert_eq!(v, Json::from("cold"), "{w:?}");
    assert!(w.iter().any(|x| x.contains("cost_line")), "{w:?}");
    // 有 cost(10,1) 的证书：线 0.7，读数 0.6 → 未过线
    let cert = Cert { alpha: 0.10, conf_delta: 0.10, hi: 0.7, n_accepted: 40, n_errors: 0, ucb: 0.06,
        cluster_unit: "测试合成证书".into(), resample: None, cost: Some((10.0, 1.0)), bounded_side: "单侧".into(),
        label_fp: String::new(), selection: None, label_source: LabelSource::全体 };
    c.records.get_mut("k").unwrap().certs.insert(cert.addr(), cert);
    let (v, _) = 跑(src, &c, 0.6).expect("跑得完");
    assert_ne!(v, Json::from("act"), "代价线 0.7 高于读数 0.6");
    let (v, _) = 跑(src, &c, 0.9).expect("跑得完");
    assert_eq!(v, Json::from("act"));
}

// ---- PR #30 评审 4082390428、4082390437：代价分支看**所选证书自己所在记录**的状态；
// 题式级记录被读到时要入账，只凭账本重放才能补回同一张代价证书。----

const 题式头: &str = r#"let f = form("test", "这段话是否提到了{city}？", {calib: "k"});"#;

const 代价题式程序: &str = r#"
budget {calls: 2, cost: 0, depth: 8};
let f = form("test", "这段话是否提到了{city}？", {calib: "k"});
let q = fill(f, {city: "上海"});
let e = cut(judge(state(mat("我去了上海")), q), {cost: [1, 5]});
handle(e, {act: fn() { "act" }, ignore: fn() { "ig" }, unsure: fn(u) { let c = unsure_cause(u); consume(u, "drop"); c }})
"#;

/// 题式键：跑一段只取 `f.hash` 的程序
fn 题式键() -> String {
    let (v, _) = 跑(&format!("budget {{calls: 0, cost: 0, depth: 8}};\n{题式头}\nf.hash"), &CalibStore::new(), 0.5).expect("跑得完");
    CalibStore::form_key(v.as_str().expect("hash 是文本"))
}

/// 在 `key` 上写一条认证记录，挂一张 cost(1,5)、线 0.7 的证书，再把状态改成 `status`
fn 带代价证书(c: &mut CalibStore, key: &str, status: &str) {
    common::certified(c, key, 0.8, 0.2, 50);
    let cert = Cert { alpha: 0.10, conf_delta: 0.10, hi: 0.7, n_accepted: 40, n_errors: 0, ucb: 0.06,
        cluster_unit: "测试合成证书".into(), resample: None, cost: Some((1.0, 5.0)), bounded_side: "单侧".into(),
        label_fp: String::new(), selection: None, label_source: LabelSource::全体 };
    let r = c.records.get_mut(key).unwrap();
    r.certs.insert(cert.addr(), cert);
    r.status = status.into();
}

#[test]
fn 代价线的题式级记录停岗与题级停岗同路由() {
    let mut c = CalibStore::new();
    let fk = 题式键();
    带代价证书(&mut c, &fk, "上岗");
    let (v, w) = 跑(代价题式程序, &c, 0.9).expect("跑得完");
    assert_eq!(v, Json::from("act"), "对照：题式级上岗时代价线放行 {w:?}");
    c.records.get_mut(&fk).unwrap().status = "停岗".into();
    let (v, w) = 跑(代价题式程序, &c, 0.9).expect("跑得完");
    assert_eq!(v, Json::from("drift"), "题式级停岗与题级停岗同路由 {w:?}");
}

#[test]
fn 代价线非上岗记录带证书仍是冷() {
    let mut c = CalibStore::new();
    带代价证书(&mut c, "k", "冷");
    let (v, w) = 跑(代价题式程序, &c, 0.9).expect("跑得完");
    assert_eq!(v, Json::from("cold"), "{w:?}");
    assert!(w.iter().any(|x| x.contains("cost_line")), "{w:?}");
}

#[test]
fn 代价线停岗候选照常路由并告警() {
    let mut c = CalibStore::new();
    let fk = 题式键();
    带代价证书(&mut c, &fk, "停岗候选");
    let (v, w) = 跑(代价题式程序, &c, 0.9).expect("跑得完");
    assert_eq!(v, Json::from("act"), "候选线照常路由 {w:?}");
    assert!(w.iter().any(|x| x.starts_with("W-suspend-candidate")), "候选要标出来 {w:?}");
}
