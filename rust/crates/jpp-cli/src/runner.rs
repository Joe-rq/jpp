//! Host wiring only: fixed observations, local action registration and report I/O.
use jpp_core::{
    ast::Program,
    effects::{CalibStore, Client},
    interp::{ActionRegistry, TaintOut},
    ledger::Ledger,
};
use serde_json::{Value, json};
use std::{cell::RefCell, rc::Rc};

pub fn execute(
    program: &Program,
    client: &mut dyn Client,
    calibrations: &CalibStore,
    ledger: &mut Ledger,
) -> Result<Value, jpp_core::Error> {
    let checks = Rc::new(RefCell::new(Vec::new()));
    let log = checks.clone();
    let mut actions = ActionRegistry::new();
    // The source computes validity. This action only records and returns its value.
    actions.register("record_check", 0.0, true, TaintOut::Inherit, move |args| {
        if args.len() != 1 {
            return Err("record_check expects one source-computed record".into());
        }
        log.borrow_mut().push(args[0].to_json());
        Ok(args[0].clone())
    });
    let outcome = jpp_core::run(program, client, calibrations, &actions, ledger)?;
    Ok(json!({
        "mode": "fixed observations; no model API requests",
        "status": if outcome.pending.is_empty() { "returned" } else { "pending" },
        "value": outcome.value_json(),
        "pending": outcome.pending,
        "returned_unsure": outcome.returned_unsure,
        "cost": {"calls": outcome.cost.calls, "replayed": outcome.cost.replayed,
                 "tokens": outcome.cost.tokens, "usd": outcome.cost.usd, "asks": outcome.cost.asks},
        "trace": outcome.trace,
        "local_checks": *checks.borrow(),
    }))
}
