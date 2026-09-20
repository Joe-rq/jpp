//! 已知缺陷的复现。三条都**现在会失败**，这是故意的：它们钉的是
//! [设计修订 v0.2](../../../../research/地基/13-Rust实践反馈设计修订-v0.2.md) 第 4、5、6 条要求的行为，
//! 而当前内核还没做到。修好一条，就把对应的 `#[ignore]` 去掉。
//!
//! 三条缺陷由 PR #12 的审查提出。全部用本地替身，零付费调用。
//!
//! ```sh
//! cargo test -p jpp-core --test known_defects -- --ignored   # 看三条怎么失败
//! ```
//!
//! Reproductions of three known defects, all currently failing on purpose. Each pins a
//! behaviour required by rules 4, 5 and 6 of the design revision; drop the `#[ignore]`
//! when the rule is implemented. No paid model calls.

mod common;

use common::*;
use jpp_core::effects::{CalibStore, Client, EffectError, FixedClient, JudgeResult};
use jpp_core::ledger::Ledger;
use jpp_core::{ActionRegistry, Answer};
use jpp_core::value::{Question, State};
use jpp_core::run;
use serde_json::Value as Json;

// ---------- 规则 4：方法身份要包含实际捕获状态 ----------

/// 工厂返回正文相同、捕获值不同的两个方法；对同一材料做 transform。
/// 期望：各自得到自己的结果（1 和 2）。当前：第二个拿到第一个的缓存输出。
#[test]
#[ignore = "规则 4 未实现：方法身份只取函数正文，不含捕获环境"]
fn 正文相同捕获不同的方法不该共用结果() {
    // fn make(n) { fn(old) { {v: n} } }
    let make = func(
        "make",
        &["n"],
        body(vec![], lambda(&["old"], body(vec![], rec(vec![("v", name("n"))])))),
    );
    let program = program(
        Some(budget(4, 16)),
        vec![
            make,
            bind("m", call("mat", vec![rec(vec![("x", int(0))])])),
            bind("f1", call("make", vec![int(1)])),
            bind("f2", call("make", vec![int(2)])),
            bind("a", call("transform", vec![name("f1"), name("m")])),
            bind("b", call("transform", vec![name("f2"), name("m")])),
        ],
        list(vec![name("a"), name("b")]),
    );
    let mut client = FixedClient::new();
    let mut ledger = Ledger::new();
    let out = run(
        &program,
        &mut client,
        &CalibStore::default(),
        &ActionRegistry::default(),
        &mut ledger,
    )
    .expect("程序本身是合法的");
    let json = out.value.expect("有值").to_json();
    let a = &json[0]["content"]["v"];
    let b = &json[1]["content"]["v"];
    assert_eq!(a, 1, "捕获 n=1 的方法应当产出 1，实际 {a}");
    assert_eq!(b, 2, "捕获 n=2 的方法应当产出 2，实际 {b}（拿到了捕获 n=1 那次的缓存输出）");
}

// ---------- 规则 5：调用后先记账，再决定是否继续 ----------

/// 后端返回的实际费用高于剩余预算。
struct CostlyClient {
    calls: u64,
    cost: f64,
}

impl Client for CostlyClient {
    fn model_id(&self) -> String {
        "costly-0".into()
    }
    fn judge(&mut self, _s: &State, qs: &[&Question]) -> Result<JudgeResult, EffectError> {
        self.calls += 1;
        Ok(JudgeResult {
            answers: qs.iter().map(|_| Answer::Noul(0.9)).collect(),
            tokens: 10,
            cost: self.cost,
        })
    }
    fn generate(
        &mut self,
        _p: &str,
        _c: &[Json],
        _n: usize,
        _r: u64,
    ) -> Result<Vec<Json>, EffectError> {
        Err(EffectError("未用到".into()))
    }
    fn ask(&mut self, _s: &State, _q: &Question) -> Result<Option<Answer>, EffectError> {
        Ok(None)
    }
    fn calls(&self) -> u64 {
        self.calls
    }
}

fn one_judge() -> jpp_core::ast::Program {
    program(
        Some(Budget {
            calls: 4,
            cost: 0.001,
            depth: Some(16),
            escalate: None,
        }),
        vec![
            bind("m", call("mat", vec![rec(vec![("x", int(1))])])),
            bind("q", call("test", vec![text("这个对吗？"), text("k")])),
            bind(
                "r",
                call("judge", vec![call("state", vec![name("m")]), name("q")]),
            ),
        ],
        call("cut", vec![name("r")]),
    )
}

use jpp_core::ast::Budget;

/// 期望：已完成的调用先入账本（费用与结果都留下），再因超预算停止；
/// 带着这份账本恢复时不再重复请求。当前：`charge` 先于 `ledger.put`，两者都丢。
#[test]
#[ignore = "规则 5 未实现：charge 排在 ledger.put 之前"]
fn 超预算不该抹掉已经发生的调用() {
    let program = one_judge();
    let calib = CalibStore::default();
    let actions = ActionRegistry::default();
    let mut ledger = Ledger::new();
    let mut client = CostlyClient { calls: 0, cost: 1.0 };

    let first = run(&program, &mut client, &calib, &actions, &mut ledger);
    assert_eq!(client.calls, 1, "替身收到了一次真实请求");
    let stopped = match &first {
        Ok(o) => !o.pending.is_empty(),
        Err(_) => true,
    };
    assert!(stopped, "费用超预算，本次应当停下");

    // 这次调用已经发生、已经花了钱，账本里必须留下它
    let entries = ledger.len();
    assert!(entries > 0, "已完成调用的结果与费用被丢掉了，账本是空的");

    // 带着记录恢复：不能再问一次
    let mut client2 = CostlyClient { calls: 0, cost: 1.0 };
    let _ = run(&program, &mut client2, &calib, &actions, &mut ledger);
    assert_eq!(client2.calls, 0, "恢复时又付了一次钱");
}

// ---------- 规则 6：整数行为不随 Rust 构建模式改变 ----------

/// 期望：溢出是一条指向 J++ 源码的运行错误。当前：debug 构建下 Rust panic，release 下回绕。
#[test]
#[ignore = "规则 6 未实现：Int 算术直接用 Rust 运算符"]
fn 整数溢出应当是运行错误而不是_panic() {
    let program = program(
        Some(budget(1, 8)),
        vec![],
        bin("+", int(i64::MAX), int(1)),
    );
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let mut client = FixedClient::new();
        let mut ledger = Ledger::new();
        run(
            &program,
            &mut client,
            &CalibStore::default(),
            &ActionRegistry::default(),
            &mut ledger,
        )
    }));
    match result {
        Err(_) => panic!("最大整数加一触发了 Rust panic；应当是带 Span 的 J++ 运行错误"),
        Ok(Ok(out)) => panic!("溢出被静默接受，得到 {}", out.value.expect("有值").to_json()),
        Ok(Err(_)) => {}
    }
}
