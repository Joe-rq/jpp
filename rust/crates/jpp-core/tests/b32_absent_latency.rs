//! B32：判断力缺席与时延。判断器调用失败时按 `budget.absent` 重试、退避，然后
//! escalate（挂起）/ conservative（出口 Unsure(absent)）/ fail（运行期错误）；连续缺席熔断；
//! 时延预算用完后的判断站点转 Unsure(latency)；静态估计超预算的计划在检查阶段拒绝。

mod common;
use std::cell::RefCell;

use jpp_core::effects::{CalibStore, Client, EffectError, GenResult, JudgeResult, Profile};
use jpp_core::ledger::Ledger;
use jpp_core::value::{Answer, Question, State};
use jpp_core::{ActionRegistry, run};
use jpp_frontend::{lower, parse};
use serde_json::Value as Json;

/// 前 `坏` 次调用失败，之后返回 0.9；每次调用睡 `睡` 毫秒
struct 时好时坏 { 坏: u64, 睡: u64, calls: RefCell<u64> }
impl Client for 时好时坏 {
    fn model_id(&self) -> String { "fixed-0".into() }
    fn judge(&mut self, _s: &State, qs: &[&Question]) -> Result<JudgeResult, EffectError> {
        *self.calls.borrow_mut() += 1;
        if self.睡 > 0 { std::thread::sleep(std::time::Duration::from_millis(self.睡)); }
        if *self.calls.borrow() <= self.坏 { return Err(EffectError("连接中断".into())); }
        Ok(JudgeResult { answers: qs.iter().map(|_| Answer::Noul(0.9)).collect(), tokens: 0, cost: 0.0, mode_share: vec![], perms: vec![] })
    }
    fn generate(&mut self, _p: &str, _c: &[Json], _n: usize, _r: u64) -> Result<GenResult, EffectError> { Err(EffectError("不该 gen".into())) }
    fn ask(&mut self, _s: &State, _q: &Question) -> Result<Option<Answer>, EffectError> { Ok(None) }
    fn calls(&self) -> u64 { *self.calls.borrow() }
}

fn 源(budget: &str) -> String {
    format!(r#"
budget {budget};
fn 判(t) {{
    let e = cut(judge(state(mat(t)), test("行吗", "k")));
    let k = exit_kind(e);
    consume(e, "drop");
    k
}}
[判("甲"), 判("乙"), 判("丙")]
"#)
}

fn 跑(src: &str, 坏: u64, 睡: u64, ledger: &mut Ledger) -> (Result<jpp_core::Outcome, String>, u64) {
    let program = lower(&parse(src).expect("解析")).expect("lower");
    let mut calib = CalibStore::new();
    calib.put("k", 0.65, 0.35, 50, "上岗").unwrap();
    let mut c = 时好时坏 { 坏, 睡, calls: RefCell::new(0) };
    let o = run(&program, &mut c, &calib, &ActionRegistry::new(), ledger).map_err(|e| e.render());
    (o, *c.calls.borrow())
}

#[test]
fn 保守策略出口转未决并可重放() {
    let src = 源(r#"{calls: 20, cost: 0, depth: 16, absent: {retry: 1, backoff: 0, then: "conservative", breaker: 5}}"#);
    let mut l = Ledger::new();
    let (o, calls) = 跑(&src, 100, 0, &mut l);
    let o = o.expect("conservative 下程序继续");
    let v = o.value_json();
    assert!(v.as_array().unwrap().iter().all(|x| x.as_str().unwrap().contains("absent")), "{v}");
    assert_eq!(calls, 6, "三次调用各重试一次");
    // 只凭账本重放：不再发，出口一致
    let program = lower(&parse(&src).expect("解析")).expect("lower");
    let mut calib = CalibStore::new();
    calib.put("k", 0.65, 0.35, 50, "上岗").unwrap();
    let mut c = 时好时坏 { 坏: 0, 睡: 0, calls: RefCell::new(0) };
    let o2 = run(&program, &mut c, &calib, &ActionRegistry::new(), &mut l).expect("重放");
    assert_eq!(o2.value_json(), v);
    assert_eq!(*c.calls.borrow(), 0, "缺席事件已记账，重放不发");
}

#[test]
fn 重试成功就照常出口() {
    let src = 源(r#"{calls: 20, cost: 0, depth: 16, absent: {retry: 2, backoff: 0, then: "conservative", breaker: 5}}"#);
    let (o, _) = 跑(&src, 1, 0, &mut Ledger::new());
    assert_eq!(o.expect("跑完").value_json(), serde_json::json!(["act", "act", "act"]));
}

#[test]
fn 升级策略挂起待续跑() {
    let src = 源(r#"{calls: 20, cost: 0, depth: 16, absent: {retry: 0, backoff: 0}}"#);
    let (o, _) = 跑(&src, 100, 0, &mut Ledger::new());
    let o = o.expect("挂起不是错误");
    assert_eq!(o.pending.first().map(|p| p.cause.as_str()), Some("absent"), "默认 then = escalate");
}

#[test]
fn 失败策略是运行期错误() {
    let src = 源(r#"{calls: 20, cost: 0, depth: 16, absent: {retry: 0, backoff: 0, then: "fail"}}"#);
    let (o, _) = 跑(&src, 100, 0, &mut Ledger::new());
    assert!(o.expect_err("fail").contains("缺席"));
}

#[test]
fn 连续缺席熔断后不再发() {
    let src = 源(r#"{calls: 20, cost: 0, depth: 16, absent: {retry: 0, backoff: 0, then: "conservative", breaker: 2}}"#);
    let (o, calls) = 跑(&src, 100, 0, &mut Ledger::new());
    o.expect("跑完");
    assert_eq!(calls, 2, "第三个站点熔断，不再发");
}

#[test]
fn 时延预算用完转超时未决() {
    let src = 源(r#"{calls: 20, cost: 0, depth: 16, latency_p95: 0.001}"#);
    let (o, _) = 跑(&src, 0, 5, &mut Ledger::new());
    let v = o.expect("跑完").value_json();
    let a = v.as_array().unwrap();
    assert!(a.iter().all(|x| x.as_str().unwrap().contains("latency")), "每次调用 5ms，预算 1ms：{v}");
}

#[test]
fn 静态估计超时延预算即拒绝() {
    let src = r#"
budget {calls: 4, cost: 0, depth: 8, latency_p95: 0.5};
let e = cut(judge(state(mat("甲")), test("行吗", "k")));
consume(e, "drop");
1
"#;
    let program = lower(&parse(src).expect("解析")).expect("lower");
    let profile = Profile { latency_p95: Some(1.0), hash: Some("测试".into()), ..Profile::default() };
    let r = jpp_core::check::check_with_profile(&program, &profile);
    assert!(r.diagnostics.iter().any(|d| d.rule == "E-latency"), "{:?}", r.diagnostics);
    let r = jpp_core::check::check(&program);
    assert!(r.diagnostics.iter().any(|d| d.rule == "W-untested" && d.message.contains("latency")), "{:?}", r.diagnostics);
}
