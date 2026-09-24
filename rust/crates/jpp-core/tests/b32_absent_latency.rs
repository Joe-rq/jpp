//! B32：判断力缺席与时延。判断器调用失败时按 `budget.absent` 重试、退避，然后
//! escalate（挂起）/ conservative（出口 Unsure(absent)）/ fail（运行期错误）；连续缺席熔断；
//! 时延预算用完后的判断站点转 Unsure(latency)；静态估计超预算的计划在检查阶段拒绝。

mod common;
use std::cell::RefCell;

use jpp_core::effects::{CalibStore, Client, EffectError, GenResult, JudgeResult, Profile};
use jpp_core::ledger::Ledger;
use jpp_core::value::{Answer, Question, State};
use jpp_core::{ActionRegistry, run};
use jpp_core::{lower, syntax::parse};
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
    assert_eq!(o.cost.calls, 6, "逐次计费（选项 A）：3 站点 ×（首发 + 重试 1）");
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
    let o = o.expect("跑完");
    assert_eq!(o.value_json(), serde_json::json!(["act", "act", "act"]));
    assert_eq!(o.cost.calls, 4, "第 1 站点失败 1 次后成功（2），其余各 1");
}

#[test]
fn 升级策略挂起待续跑() {
    let src = 源(r#"{calls: 20, cost: 0, depth: 16, absent: {retry: 0, backoff: 0}}"#);
    let (o, _) = 跑(&src, 100, 0, &mut Ledger::new());
    let o = o.expect("挂起不是错误");
    assert_eq!(o.pending.first().map(|p| p.cause.as_str()), Some("absent"), "默认 then = escalate");
    assert_eq!(o.cost.calls, 1, "失败的那一次也计");
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
    let o = o.expect("跑完");
    assert_eq!(calls, 2, "第三个站点熔断，不再发");
    assert_eq!(o.cost.calls, 2, "熔断不发、不计费");
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

/// 失败策略报错时，失败的那一次也在账面上（选项 A）：用账本看（运行期错误没有 Outcome）
#[test]
fn 失败策略的尝试入账() {
    let src = 源(r#"{calls: 20, cost: 0, depth: 16, absent: {retry: 0, backoff: 0, then: "fail"}}"#);
    let mut l = Ledger::new();
    let (o, calls) = 跑(&src, 100, 0, &mut l);
    assert!(o.is_err());
    assert_eq!(calls, 1);
    let n: u64 = l.entries.iter().map(|e| match e { jpp_core::ledger::Entry::Absent { attempts, .. } => *attempts, _ => 0 }).sum();
    assert_eq!(n, 1, "账本记下 1 次尝试");
}

fn 单站点(budget: &str) -> String {
    format!(r#"
budget {budget};
let e = cut(judge(state(mat("甲")), test("行吗", "k")));
let k = exit_kind(e);
consume(e, "drop");
k
"#)
}

/// 补丁请求 1：`calls: 1, retry: 5` 且始终失败 → 第 2 次尝试前预算停机，后端只收到 1 次
#[test]
fn retry_charges_each_attempt() {
    let src = 单站点(r#"{calls: 1, cost: 0, depth: 16, absent: {retry: 5, backoff: 0.01, then: "conservative"}}"#);
    let (o, calls) = 跑(&src, 100, 0, &mut Ledger::new());
    let o = o.expect("预算停机是挂起");
    assert_eq!(o.pending.first().map(|p| p.cause.as_str()), Some("budget"));
    assert_eq!(calls, 1, "后端恰好收到 1 次");
    assert_eq!(o.cost.calls, 1);
}

/// 前两次失败、第三次成功 → 该站点计 3 次
#[test]
fn retry_success_counts_all_attempts() {
    let src = 单站点(r#"{calls: 10, cost: 0, depth: 16, absent: {retry: 2, backoff: 0, then: "conservative"}}"#);
    let (o, calls) = 跑(&src, 2, 0, &mut Ledger::new());
    let o = o.expect("跑完");
    assert_eq!(o.value_json(), serde_json::json!("act"));
    assert_eq!(calls, 3);
    assert_eq!(o.cost.calls, 3);
}

/// 全失败也计时延：退避 0.01 + 0.02 = 0.03s 进时延预算，于是下一个站点转 Unsure(latency)
#[test]
fn terminal_failure_counts_latency() {
    // 第 1 站点 3 次全失败（首发 + 重试 2），之后成功；预算 0.025s 被第 1 站点的退避用完
    let src = 源(r#"{calls: 20, cost: 0, depth: 16, latency_p95: 0.025, absent: {retry: 2, backoff: 0.01, then: "conservative", breaker: 5}}"#);
    let (o, calls) = 跑(&src, 3, 0, &mut Ledger::new());
    let v = o.expect("跑完").value_json();
    let a = v.as_array().unwrap();
    assert!(a[0].as_str().unwrap().contains("absent"), "{v}");
    assert!(a[1].as_str().unwrap().contains("latency"), "失败路径的退避计入时延：{v}");
    assert_eq!(calls, 3, "时延用完后不再发");
}

/// 首跑在重试中途因预算停机；只凭账本审计重放停在同一站点、同样的原因
#[test]
fn absent_replay_stops_where_first_run_stopped() {
    let src = 源(r#"{calls: 3, cost: 0, depth: 16, absent: {retry: 2, backoff: 0, then: "conservative", breaker: 5}}"#);
    let mut l = Ledger::new();
    // 第 1 站点首发成功；第 2 站点首发失败，重试 1 次失败，重试 2 时预算不够（1+2 已花）
    let program = lower(&parse(&src).expect("解析")).expect("lower");
    let mut calib = CalibStore::new();
    calib.put("k", 0.65, 0.35, 50, "上岗").unwrap();
    struct 第二站点坏 { calls: RefCell<u64> }
    impl Client for 第二站点坏 {
        fn model_id(&self) -> String { "fixed-0".into() }
        fn judge(&mut self, _s: &State, qs: &[&Question]) -> Result<JudgeResult, EffectError> {
            *self.calls.borrow_mut() += 1;
            if *self.calls.borrow() >= 2 { return Err(EffectError("连接中断".into())); }
            Ok(JudgeResult { answers: qs.iter().map(|_| Answer::Noul(0.9)).collect(), tokens: 0, cost: 0.0, mode_share: vec![], perms: vec![] })
        }
        fn generate(&mut self, _p: &str, _c: &[Json], _n: usize, _r: u64) -> Result<GenResult, EffectError> { Err(EffectError("不该 gen".into())) }
        fn ask(&mut self, _s: &State, _q: &Question) -> Result<Option<Answer>, EffectError> { Ok(None) }
        fn calls(&self) -> u64 { *self.calls.borrow() }
    }
    let mut c = 第二站点坏 { calls: RefCell::new(0) };
    let o1 = run(&program, &mut c, &calib, &ActionRegistry::new(), &mut l).expect("首跑挂起");
    let p1 = o1.pending.first().expect("首跑预算停机").clone();
    assert_eq!(p1.cause, "budget");
    assert_eq!(o1.cost.calls, 3);
    let mut c2 = 时好时坏 { 坏: 0, 睡: 0, calls: RefCell::new(0) };
    let o2 = jpp_core::run_replay(&program, &mut c2, &calib, &ActionRegistry::new(), &mut l).expect("重放挂起");
    let p2 = o2.pending.first().expect("重放同样停机");
    assert_eq!((p2.cause.as_str(), p2.site), (p1.cause.as_str(), p1.site), "停在同一站点");
    assert_eq!(*c2.calls.borrow(), 0, "重放不发");
}
