//! 调用者构造契约值 `outcome` 与账本键取用 `key_of`（B17）。
//! 步 4a 从 `interp.rs` 机械拆出，只搬代码、不改逻辑（21 §三·3）。

use crate::interp::*;

impl<'a> Interp<'a> {
    #[allow(unused_variables)]
    pub(in crate::interp) fn b_outcome(
        &mut self,
        name: &'static str,
        args: Vec<Value>,
        sp: Span,
    ) -> R<Value> {
        let n = args.len();
        let arity = |k: usize| -> R<()> {
            if n == k {
                Ok(())
            } else {
                err(
                    Some("E-rt-arity"),
                    format!("{name} 需要 {k} 个参数，收到 {n}"),
                    sp,
                )
            }
        };
        // 调用者自己构造一个契约值（B17）：`outcome({value, pending?, evidence?, resume?, purpose?, detail?})`。
        // 与内置构造返回同一类型，可再交给 sieve / pair / tally 等。
        // - pending：出口，或 `{element?, exit, cause?}` 记录；每项必须带出口（责任载体）；
        // - evidence：只收账本键（Text，由 key_of 取得），不收读数或材料副本（不变量 3）；
        // - resume：记录；或一个方法，记为 `{reason: "continue", next: 方法}`。
        arity(1)?;
        let r = &args[0];
        if !matches!(r, Value::Record(_)) {
            return err(
                Some("E-rt-arg"),
                "outcome({value, pending?, evidence?, resume?, purpose?, detail?})",
                sp,
            );
        }
        if let Value::Record(fs) = r {
            for (k, _) in fs.iter() {
                if ![
                    "value", "pending", "evidence", "resume", "purpose", "detail", "spent",
                ]
                .contains(&k.as_str())
                {
                    return err(
                        Some("E-rt-arg"),
                        format!(
                            "outcome 不认得字段 {k}：可给 value、pending、evidence、resume、purpose、detail、spent"
                        ),
                        sp,
                    );
                }
            }
        }
        let Some(value) = r.get("value") else {
            return err(Some("E-rt-arg"), "outcome 必须给 value（产出）", sp);
        };
        let mut pending = vec![];
        for (i, p) in list_of(r.get("pending")).into_iter().enumerate() {
            match &p {
                Value::Exit(_) | Value::Duty(_) => {
                    pending.push(Self::pending_entry(Value::Unit, &p))
                }
                Value::Record(_) => match p.get("exit") {
                    Some(x @ (Value::Exit(_) | Value::Duty(_))) => pending.push(
                        Self::pending_entry(p.get("element").unwrap_or(Value::Unit), &x),
                    ),
                    _ => {
                        return err(
                            Some("J-05"),
                            format!(
                                "outcome 的 pending 第 {i} 项没有出口：未决清单的每一项都要带承担责任的出口（exit）。修法：把 cut / handle 前的出口放进来"
                            ),
                            sp,
                        );
                    }
                },
                other => {
                    return err(
                        Some("J-05"),
                        format!(
                            "outcome 的 pending 第 {i} 项是 {}：只收出口或带 exit 的记录",
                            other.type_name()
                        ),
                        sp,
                    );
                }
            }
        }
        let mut evidence = vec![];
        for (i, k) in list_of(r.get("evidence")).into_iter().enumerate() {
            match &k {
                Value::Text(t, _) if !t.is_empty() => push_key(&mut evidence, k.clone()),
                other => {
                    return err(
                        Some("E-rt-arg"),
                        format!(
                            "E-evidence: outcome 的 evidence 第 {i} 项是 {}：证据只存账本键（Text），不存读数、观察或材料的副本。修法：用 key_of(出口) 取键",
                            other.type_name()
                        ),
                        sp,
                    );
                }
            }
        }
        let resume = match r.get("resume") {
            None | Some(Value::Unit) => Value::Unit,
            Some(f @ (Value::Fn(_) | Value::Builtin(_))) => Value::record(vec![
                ("reason".into(), Value::text("continue")),
                ("next".into(), f),
            ]),
            Some(rec @ Value::Record(_)) => rec,
            Some(other) => {
                return err(
                    Some("E-rt-arg"),
                    format!(
                        "outcome 的 resume 要是记录或方法，收到 {}",
                        other.type_name()
                    ),
                    sp,
                );
            }
        };
        let spent = match r.get("spent") {
            Some(sv) => (
                match sv.get("calls") {
                    Some(Value::Int(c, _)) => c,
                    _ => 0,
                },
                match sv.get("usd") {
                    Some(Value::Float(u, _)) => u,
                    Some(Value::Int(u, _)) => u as f64,
                    _ => 0.0,
                },
            ),
            None => (0, 0.0),
        };
        Self::outcome_value(
            "outcome",
            value,
            pending,
            evidence,
            resume,
            spent,
            r.get("detail").unwrap_or(Value::record(vec![])),
            r.get("purpose").unwrap_or(Value::Unit),
            sp,
        )
    }
    #[allow(unused_variables)]
    pub(in crate::interp) fn b_key_of(
        &mut self,
        name: &'static str,
        args: Vec<Value>,
        sp: Span,
    ) -> R<Value> {
        let n = args.len();
        let arity = |k: usize| -> R<()> {
            if n == k {
                Ok(())
            } else {
                err(
                    Some("E-rt-arity"),
                    format!("{name} 需要 {k} 个参数，收到 {n}"),
                    sp,
                )
            }
        };
        // 账本键：出口取它来自的那条账本记录；读数取自己的键；契约值取它的证据列表
        arity(1)?;
        match &args[0] {
            Value::Exit(e) | Value::Duty(e) => Ok(Value::text(&e.ledger_key.borrow())),
            Value::Reading(r) => Ok(Value::text(&r.ledger_key)),
            v if is_outcome(v) => Ok(v.get("evidence").unwrap_or(Value::list(vec![]))),
            other => err(
                Some("E-rt-arg"),
                format!("key_of 收出口、读数或契约值，收到 {}", other.type_name()),
                sp,
            ),
        }
    }
}
