//! `action_fact`、`fail`、`is_fail`、`loop`、`stop`。步 36 G3 从 `host_builtins.rs` 原样搬出（只搬不改）。

use super::*;

impl<'a> Interp<'a> {
    /// `action_fact(name)`（C-7，Z0173）：宿主动作可撤回性的如实事实，`{reversibility, reason, conditions}`。
    /// 纯读：不调效应、不花钱、不进账本。未登记的动作名返回失败值（不中止程序）。只是事实与记录，
    /// J-08 与放行门不读它，`.jpp` 作者自己决定要不要据此做什么（意图汇编 11a）。
    pub(crate) fn b_action_fact(
        &mut self,
        name: &'static str,
        args: Vec<Value>,
        sp: Span,
    ) -> R<Value> {
        if args.len() != 1 {
            return err(
                Some("E-rt-arity"),
                format!("{name} 需要 1 个参数，收到 {}", args.len()),
                sp,
            );
        }
        let Value::Text(n, _) = &args[0] else {
            return err(Some("E-rt-arg"), "action_fact(name: Text)", sp);
        };
        let Some(a) = self.actions.actions.get(n.as_ref()) else {
            return Ok(Value::Fail(
                Rc::from(format!("unknown_action: {n} 没有登记").as_str()),
                Provenance::trusted(),
            ));
        };
        let (kind, reason, conds) = match &a.undo {
            Some(u) => (
                u.reversibility.clone(),
                u.reason.clone(),
                u.conditions.clone(),
            ),
            None => (
                if a.reversible {
                    "reversible"
                } else {
                    "irreversible"
                }
                .to_string(),
                "登记时未附理由，只有可逆布尔".to_string(),
                vec![],
            ),
        };
        Ok(Value::record(vec![
            ("reversibility".into(), Value::text(&kind)),
            ("reason".into(), Value::text(&reason)),
            (
                "conditions".into(),
                Value::list(conds.iter().map(|c| Value::text(c)).collect()),
            ),
        ]))
    }

    #[allow(unused_variables)]
    pub(crate) fn b_fail(&mut self, name: &'static str, args: Vec<Value>, sp: Span) -> R<Value> {
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
        arity(1)?;
        let Value::Text(c, t) = &args[0] else {
            return err(Some("E-rt-arg"), "fail(reason: Text)", sp);
        };
        Ok(Value::Fail(Rc::from(c.as_ref()), t.clone()))
    }

    #[allow(unused_variables)]
    pub(crate) fn b_is_fail(&mut self, name: &'static str, args: Vec<Value>, sp: Span) -> R<Value> {
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
        arity(1)?;
        Ok(Value::Bool(
            matches!(args[0], Value::Fail(..)),
            Taint::Trusted.into(),
            GuardEv::EMPTY,
        ))
    }

    #[allow(unused_variables)]
    pub(crate) fn b_loop(&mut self, name: &'static str, args: Vec<Value>, sp: Span) -> R<Value> {
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
        arity(3)?;
        let Value::Int(b, _) = &args[0] else {
            return err(Some("J-06"), "loop 的 bound 必须是整数字面量或整数值", sp);
        };
        let (b, init, step) = (*b, args[1].clone(), args[2].clone());
        self.loop_(b, init, &step, sp)
    }

    #[allow(unused_variables)]
    pub(crate) fn b_stop(&mut self, name: &'static str, args: Vec<Value>, sp: Span) -> R<Value> {
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
        arity(1)?;
        Ok(Value::Stop(Rc::new(args[0].clone())))
    }
}
