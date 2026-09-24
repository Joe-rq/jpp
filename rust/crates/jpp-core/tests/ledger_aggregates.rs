//! 步 10：`jpp-ledger` 的两个只读聚合。`observations` 与运行时交出的 `Outcome.evidence` 逐条一致；
//! `depth_profile` 按跳数数判断条目与缺席。

use std::cell::RefCell;

use jpp_core::effects::{CalibStore, Client, EffectError, GenResult, JudgeResult};
use jpp_core::ledger::{Entry, Ledger, depth_profile, observations};
use jpp_core::value::{Answer, Question, State};
use jpp_core::{ActionRegistry, run};
use jpp_core::{lower, syntax::parse};
use serde_json::Value as Json;

/// 第 `坏` 次及以后的调用失败；成功时是非题给 0.9、选择题给 [0.7, 0.3]
struct 桩 {
    坏: u64,
    calls: RefCell<u64>,
}
impl Client for 桩 {
    fn model_id(&self) -> String {
        "fixed-0".into()
    }
    fn judge(&mut self, _s: &State, qs: &[&Question]) -> Result<JudgeResult, EffectError> {
        *self.calls.borrow_mut() += 1;
        if *self.calls.borrow() >= self.坏 {
            return Err(EffectError("连接中断".into()));
        }
        let answers = qs
            .iter()
            .map(|q| {
                if q.scale.is_empty() {
                    Answer::Noul(0.9)
                } else {
                    Answer::Choice(vec![0.7, 0.3])
                }
            })
            .collect();
        Ok(JudgeResult {
            answers,
            tokens: 0,
            cost: 0.0,
            mode_share: vec![],
            perms: vec![],
        })
    }
    fn generate(
        &mut self,
        _p: &str,
        _c: &[Json],
        _n: usize,
        _r: u64,
    ) -> Result<GenResult, EffectError> {
        Err(EffectError("不该 gen".into()))
    }
    fn ask(&mut self, _s: &State, _q: &Question) -> Result<Option<Answer>, EffectError> {
        Ok(None)
    }
    fn calls(&self) -> u64 {
        *self.calls.borrow()
    }
}

fn calib() -> CalibStore {
    let mut c = CalibStore::new();
    c.put("k", 0.65, 0.35, 50, "上岗").unwrap();
    c.put("s", 0.65, 0.35, 50, "上岗").unwrap();
    c
}

const 两题: &str = r#"
budget {calls: 10, cost: 0, depth: 16};
let a = cut(judge(state(mat("甲")), test("行吗", "k")));
let ka = exit_kind(a);
consume(a, "drop");
let b = cut(judge(state(mat("乙")), test("好吗", "s")));
let kb = exit_kind(b);
consume(b, "drop");
[ka, kb]
"#;

#[test]
fn observations_match_runtime_evidence() {
    let program = lower(&parse(两题).expect("解析")).expect("lower");
    let mut l = Ledger::new();
    // 宿主手写的条目不是本次运行的观察
    l.put(Entry::judge(
        "宿主写的",
        Answer::Noul(0.5),
        0,
        0.0,
        "fixed-0",
        0,
    ));
    let mut c = 桩 {
        坏: u64::MAX,
        calls: RefCell::new(0),
    };
    let o = run(&program, &mut c, &calib(), &ActionRegistry::new(), &mut l).expect("跑完");
    let obs = observations(&l);
    let from_ledger: Vec<_> = obs
        .iter()
        .map(|x| (x.calib.clone(), x.p, x.phys.clone()))
        .collect();
    let from_run: Vec<_> = o
        .evidence
        .iter()
        .map(|(k, s)| (k.clone(), s.p, s.phys.clone()))
        .collect();
    assert_eq!(from_run.len(), 2);
    assert_eq!(from_ledger, from_run);
    assert!(obs.iter().all(|x| x.key != "宿主写的"));

    // 只凭账本重放：不发调用，派生出的观察不变
    let mut c2 = 桩 {
        坏: 1,
        calls: RefCell::new(0),
    };
    jpp_core::run_replay(&program, &mut c2, &calib(), &ActionRegistry::new(), &mut l)
        .expect("重放");
    assert_eq!(observations(&l), obs);
}

#[test]
fn depth_profile_counts_judges_and_absences() {
    let src = 两题.replace(
        "depth: 16}",
        r#"depth: 16, absent: {retry: 0, backoff: 0, then: "conservative"}}"#,
    );
    let program = lower(&parse(&src).expect("解析")).expect("lower");
    let mut l = Ledger::new();
    // 第一题成功，第二题缺席
    let mut c = 桩 {
        坏: 2,
        calls: RefCell::new(0),
    };
    let o = run(&program, &mut c, &calib(), &ActionRegistry::new(), &mut l).expect("跑完");
    assert_eq!(
        o.value_json()[1],
        Json::from("unsure(absent)"),
        "{}",
        o.value_json()
    );
    // 步 17c 起缺席按「同一状态、同一站点」的判断条目的跳归，没有记 1（B59 无父为 1）：
    // 1 条判断、1 条缺席都在 1 跳（17a 时缺席错位在 0 跳，见 `地基/过程记录/工程-步17a.md` §二）。
    assert_eq!(depth_profile(&l), vec![(1, 1, 1)]);
    assert_eq!(observations(&l).len(), 1, "缺席没有读数，不是观察");
}
