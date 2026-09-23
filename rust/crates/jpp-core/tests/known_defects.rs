//! PR #20 originally reproduced three defects. The fixes and their behavioral
//! regressions now live in v13_rules.rs (PR #25). Keep the additional source-span
//! assertion requested in review: a checker failure cannot pass as an overflow.
//! 原三条缺陷已修复；这里补验运行错误类型和准确源码区间，不再忽略旧复现。

use jpp_core::effects::{CalibStore, FixedClient};
use jpp_core::ledger::Ledger;
use jpp_core::{run, ActionRegistry, Error};

#[test]
fn overflow_is_a_runtime_error_at_the_expression() {
    let src = "budget {calls: 0, cost: 0, depth: 8};\n9223372036854775807 + 1";
    let program = jpp_frontend::lower(&jpp_frontend::parse(src).unwrap()).unwrap();
    let mut client = FixedClient::new();
    let mut ledger = Ledger::new();
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        run(&program, &mut client, &CalibStore::default(),
            &ActionRegistry::default(), &mut ledger)
    }));
    match result {
        Ok(Err(Error::Runtime(error))) => {
            assert!(error.render().contains("溢出"));
            assert_eq!(error.span.start, src.find("9223372036854775807").unwrap());
            assert_eq!(error.span.end, src.len());
            assert_eq!(&src[error.span.start..error.span.end], "9223372036854775807 + 1");
        }
        other => panic!("expected a source-located overflow runtime error, got {other:?}"),
    }
}
