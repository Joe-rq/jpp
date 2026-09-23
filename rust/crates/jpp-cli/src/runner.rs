//! Host wiring only: fixed observations, local action registration and report I/O.
use jpp_core::{
    ast::Program,
    effects::{CalibStore, Client},
    interp::{ActionRegistry, TaintOut, json_to_value},
    ledger::Ledger,
};
use serde_json::{Value, json};
use std::{cell::RefCell, rc::Rc};

pub fn execute(
    program: &Program,
    client: &mut dyn Client,
    calibrations: &CalibStore,
    ledger: &mut Ledger,
    replay_only: bool,
    // 越界接线：把这一趟的证据带出去，供 `--calib-out` 折进记录。
    // **J-03 决定了程序永远写不了线**，所以「跑程序 → 积累证据 → 认证 → 用上」这条环
    // **只能靠宿主/CLI 闭合**——而这是出料那一半。
    evidence_out: &mut Vec<(String, jpp_core::effects::Sample)>,
) -> Result<Value, jpp_core::Error> {
    let checks = Rc::new(RefCell::new(Vec::new()));
    let log = checks.clone();
    let mut actions = ActionRegistry::new();
    // The source computes validity. This action only records and returns its value.
    actions.register("record_check", 0.0, true, TaintOut::Inherit, move |args| {
        if replay_only {
            return Err("replay has no completed record for record_check".into());
        }
        if args.len() != 1 {
            return Err("record_check expects one source-computed record".into());
        }
        log.borrow_mut().push(args[0].to_json());
        Ok(args[0].clone())
    });
    actions.register("read_json", 0.0, true, TaintOut::Untrusted, move |args| {
        if replay_only {
            return Err("replay has no completed record for read_json".into());
        }
        let [jpp_core::value::Value::Text(path, _)] = args else {
            return Err("read_json expects one file path".into());
        };
        let bytes = std::fs::read(path.as_ref()).map_err(|e| format!("{path}: {e}"))?;
        let value: Value = serde_json::from_slice(&bytes).map_err(|e| format!("{path}: {e}"))?;
        // Reject unsigned integers that the core's JSON adapter cannot represent as Int.
        validate_numbers(&value)?;
        Ok(json_to_value(&value))
    });
    actions.register("write_json", 0.0, false, TaintOut::Inherit, move |args| {
        if replay_only {
            return Err("replay has no completed record for write_json".into());
        }
        let [jpp_core::value::Value::Text(path, _), value] = args else {
            return Err("write_json expects a file path and a value".into());
        };
        let bytes = serde_json::to_vec_pretty(&value.to_json()).map_err(|e| e.to_string())?;
        std::fs::write(path.as_ref(), bytes).map_err(|e| format!("{path}: {e}"))?;
        Ok(value.clone())
    });
    let outcome = jpp_core::run(program, client, calibrations, &actions, ledger)?;
    evidence_out.extend(outcome.evidence.iter().cloned());
    // J-10 的静态告警只有带校准记录的那次检查报得出来（在 `jpp_core::run` 里），
    // CLI 执行前那次检查没有记录、报不出它；这里打到 stderr，`--output` 时终端也看得见。
    for w in outcome.trace.warnings.iter().filter(|w| w.starts_with("J-10")) {
        eprintln!("warning: {w}");
    }
    let mut report = json!({
        "mode": "fixed observations; no model API requests",
        "status": if outcome.pending.is_empty() { "returned" } else { "pending" },
        "value": outcome.value_json(),
        "pending": outcome.pending,
        "returned_unsure": outcome.returned_unsure,
        "cost": {"calls": outcome.cost.calls, "replayed": outcome.cost.replayed,
                 "tokens": outcome.cost.tokens, "usd": outcome.cost.usd, "asks": outcome.cost.asks},
        "trace": outcome.trace,
        "local_checks": *checks.borrow(),
    });
    // 停岗候选（B25）只在有时出现，默认输出逐字节不变
    if !outcome.suspend_candidates.is_empty() {
        report["suspend_candidates"] = json!(outcome.suspend_candidates);
    }
    Ok(report)
}

fn validate_numbers(value: &Value) -> Result<(), String> {
    match value {
        Value::Number(n) if n.is_u64() && n.as_i64().is_none() => {
            Err("JSON integer exceeds J++ Int range".into())
        }
        Value::Array(items) => items.iter().try_for_each(validate_numbers),
        Value::Object(items) => items.values().try_for_each(validate_numbers),
        _ => Ok(()),
    }
}
