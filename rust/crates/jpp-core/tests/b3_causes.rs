//! B3：未决原因 `rejected_all`（候选全被否决）与 `no_candidate`（没有候选）。

mod common;
use std::cell::RefCell;

use jpp_core::effects::{CalibStore, Client, EffectError, GenResult, JudgeResult};
use jpp_core::ledger::Ledger;
use jpp_core::value::{Answer, Question, State};
use jpp_core::{ActionRegistry, run};
use jpp_core::{lower, syntax::parse};
use serde_json::Value as Json;

struct 定值(f64, RefCell<u64>);
impl Client for 定值 {
    fn model_id(&self) -> String { "fixed-0".into() }
    fn judge(&mut self, s: &State, qs: &[&Question]) -> Result<JudgeResult, EffectError> {
        *self.1.borrow_mut() += 1;
        Ok(JudgeResult { answers: qs.iter().map(|q| if q.op == jpp_core::value::Op::Select { Answer::Choice(vec![1.0 / s.over.len() as f64; s.over.len()]) } else { Answer::Noul(self.0) }).collect(), tokens: 0, cost: 0.0, mode_share: vec![], perms: vec![] })
    }
    fn generate(&mut self, _p: &str, _c: &[Json], _n: usize, _r: u64) -> Result<GenResult, EffectError> { Err(EffectError("不该 gen".into())) }
    fn ask(&mut self, _s: &State, _q: &Question) -> Result<Option<Answer>, EffectError> { Ok(None) }
    fn calls(&self) -> u64 { *self.1.borrow() }
}

fn 跑(src: &str, p: f64) -> (Json, u64) {
    let program = lower(&parse(src).expect("解析")).expect("lower");
    let mut calib = CalibStore::new();
    calib.put("k", 0.75, 0.25, 50, "上岗").unwrap();
    let mut c = 定值(p, RefCell::new(0));
    let mut l = Ledger::new();
    let o = run(&program, &mut c, &calib, &ActionRegistry::new(), &mut l).unwrap_or_else(|e| panic!("{}", e.render()));
    (o.value_json(), *c.1.borrow())
}

const 找第一个: &str = r#"
budget {calls: 8, cost: 0, depth: 16};
fn 第一个(pool) {
    let r = first_k(sieve(pool, test("行吗", "k")), 1);
    let c = handle(r.value.exit, {act: fn() { "act" }, ignore: fn() { "ignore" },
                   unsure: fn(u) { let c = unsure_cause(u); consume(u, "drop"); c }});
    consume(r, "drop");
    c
}
POOL
"#;

#[test]
fn 全被否决与没有候选分开() {
    let (v, _) = 跑(&找第一个.replace("POOL", r#"第一个(["甲", "乙"])"#), 0.05);
    assert_eq!(v, Json::from("rejected_all"));
    let (v, calls) = 跑(&找第一个.replace("POOL", "第一个([])"), 0.05);
    assert_eq!(v, Json::from("no_candidate"));
    assert_eq!(calls, 0);
    let (v, _) = 跑(&找第一个.replace("POOL", r#"第一个(["甲", "乙"])"#), 0.95);
    assert_eq!(v, Json::from("act"));
}

#[test]
fn 选择题没有候选不发并给no_candidate() {
    let src = r#"
budget {calls: 4, cost: 0, depth: 8};
let e = cut(judge(state(mat("哪个好"), {over: []}), select("选一个", "k")));
let c = exit_kind(e);
consume(e, "drop");
c
"#;
    let (v, calls) = 跑(src, 0.5);
    assert!(v.as_str().unwrap().contains("no_candidate"), "{v}");
    assert_eq!(calls, 0, "没有候选不发");
}
