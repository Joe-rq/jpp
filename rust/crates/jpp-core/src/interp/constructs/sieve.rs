//! 三路过滤 `sieve`（施工件 c）。
//! 步 4a 从 `interp.rs` 机械拆出，只搬代码、不改逻辑（21 §三·3）。

use crate::interp::*;

impl<'a> Interp<'a> {
    pub(in crate::interp) fn sieve(
        &mut self,
        items: &[Value],
        qs: &[Rc<Question>],
        carried_pending: &[Value],
        carried_evidence: &[Value],
        sp: Span,
    ) -> R<Vec<Value>> {
        let m0 = self.mark();
        for q in qs {
            if q.op != Op::Test {
                return err(
                    Some("E-rt-arg"),
                    format!(
                        "sieve 只收是非题（test）：题「{}」是 {}。K 选一与打分的分流待后续件",
                        q.text,
                        q.op.fixture_name()
                    ),
                    sp,
                );
            }
        }
        // 先把此前登记的判断发出去：下面要接住预算停机，不能连带吞掉别人的登记
        self.flush("sieve-before")?;
        let mut prepared: Vec<(Value, Value, Vec<Rc<Reading>>, Value)> = vec![];
        for it in items {
            let (material, trail) = element_parts(it);
            let state = match &material {
                Value::State(s) => s.clone(),
                other => match self.make_state(&[other.clone()], sp)? {
                    Value::State(s) => s,
                    _ => return err(Some("E-rt-arg"), "sieve 无法把元素变成状态", sp),
                },
            };
            // B59（步 17a）：由元素记录构造的状态，来源读数是元素的出口（结构通道）
            let lineage = element_lineage(it);
            let state = if lineage.is_empty() {
                state
            } else {
                Rc::new((*state).clone().with_parents(lineage))
            };
            let rs = self.judge(&state, qs, sp)?;
            let rs: Vec<Rc<Reading>> = rs
                .into_iter()
                .map(|v| match v {
                    Value::Reading(r) => r,
                    _ => unreachable!("judge 只回读数"),
                })
                .collect();
            prepared.push((material, trail, rs, it.clone()));
        }
        let stopped = match self.flush("sieve") {
            Ok(()) => None,
            Err(Fault::Halt(p)) if p.cause == "budget" => Some(p),
            Err(e) => return Err(e),
        };
        let (_, carried_spent) = self.since(m0);
        let mut out = vec![];
        for (j, q) in qs.iter().enumerate() {
            let (mut act, mut ignore, mut pending, mut evidence, mut n_unobserved) =
                (vec![], vec![], vec![], vec![], 0usize);
            for (i, (material, trail, rs, source)) in prepared.iter().enumerate() {
                let r = &rs[j];
                // `source` = 调用者交进来的原元素（配对产物、上一次过滤的产物……原样保留），
                // `item` 只是交给判断器的那一份材料。两者分开，来源与关系不会在过滤中丢失。
                let base = vec![
                    ("item".to_string(), material.clone()),
                    (
                        "index".to_string(),
                        Value::Int(i as i64, Taint::Trusted.into()),
                    ),
                    ("trail".to_string(), trail.clone()),
                    ("source".to_string(), source.clone()),
                ];
                if self.answer_of(r).is_none() && r.fail.is_none() {
                    // 预算停机没问到：记为 Unsure(budget) 进未决清单（B17 取舍），不混进 ignore
                    n_unobserved += 1;
                    let ex = self.new_exit(
                        ExitKind::Unsure("budget".into()),
                        None,
                        Op::Test,
                        &q.hash,
                        "",
                        crate::value::Taint::Trusted,
                        sp,
                    );
                    let mut rec = base;
                    rec.push(("exit".to_string(), ex.clone()));
                    rec.push(("cause".to_string(), Value::text("budget")));
                    pending.push(Self::pending_entry(Value::record(rec), &ex));
                    continue;
                }
                if !r.ledger_key.is_empty()
                    && !evidence.iter().any(
                        |k: &Value| matches!(k, Value::Text(t, _) if t.as_ref() == r.ledger_key),
                    )
                {
                    evidence.push(Value::text(&r.ledger_key));
                }
                let exit = self.cut(r, None, None, sp)?;
                let Value::Exit(e) = &exit else {
                    return err(Some("E-rt-arg"), "cut 没有给出出口", sp);
                };
                // B84：元素的 item 带选中它的出口键（sources 并入，taint 不动）；元素记录的 exit 照 17a 保留
                let 选键 = Provenance::sources_only(Sources::from_key(&e.ledger_key.borrow()));
                let mut rec: Vec<(String, Value)> = base
                    .into_iter()
                    .map(|(k, v)| {
                        if k == "item" {
                            (k, v.with_prov(&选键))
                        } else {
                            (k, v)
                        }
                    })
                    .collect();
                rec.push(("exit".to_string(), exit.clone()));
                match &e.kind {
                    ExitKind::Act => {
                        e.consumed.set(true);
                        *e.consumed_by.borrow_mut() = "sieve:act".into();
                        rec.push(("cause".into(), Value::Unit));
                        act.push(Value::record(rec));
                    }
                    ExitKind::Ignore => {
                        e.consumed.set(true);
                        *e.consumed_by.borrow_mut() = "sieve:ignore".into();
                        rec.push(("cause".into(), Value::Unit));
                        ignore.push(Value::record(rec));
                    }
                    ExitKind::Unsure(_) => {
                        rec.push(("cause".into(), Value::text(&e.cause())));
                        pending.push(Self::pending_entry(Value::record(rec), &exit));
                    }
                    _ => return err(Some("E-rt-arg"), "是非题给出了非是非出口", sp),
                }
            }
            if let Some(p) = &stopped {
                if j == 0 && n_unobserved > 0 {
                    self.trace.warn(format!("W-sieve-budget: 三路过滤在预算处停止，{} 个元素未观察，记为 Unsure(budget) 进未决清单（未计入 ignore）：{}", n_unobserved, p.detail));
                }
            }
            let resume = match &stopped {
                Some(p) => Value::record(vec![
                    ("reason".into(), Value::text("budget")),
                    ("detail".into(), Value::text(&p.detail)),
                    (
                        "unobserved".into(),
                        Value::Int(n_unobserved as i64, Taint::Trusted.into()),
                    ),
                ]),
                None => Value::Unit,
            };
            let detail = Value::record(vec![
                ("question".into(), Value::Question(q.clone())),
                ("ignore".into(), Value::list(ignore)),
            ]);
            out.push((Value::list(act), pending, evidence, resume, detail));
        }
        let mut outs = vec![];
        let n_out = out.len();
        for (j, (value, pending, evidence, resume, detail)) in out.into_iter().enumerate() {
            // 多道题一次过滤：调用与费用、以及从输入带进来的未决与证据，只记在第一份契约上，
            // 其余为零——融合后的调用分不到每道题，重复记会让求和翻倍。
            let (mut pending, mut evidence) = (pending, evidence);
            let spent = if j == 0 { carried_spent } else { (0, 0.0) };
            if j == 0 {
                pending.extend(carried_pending.iter().cloned());
                for k in carried_evidence.iter() {
                    push_key(&mut evidence, k.clone());
                }
            }
            let _ = n_out;
            outs.push(Self::outcome_value(
                "sieve",
                value,
                pending,
                evidence,
                resume,
                spent,
                detail,
                Value::Unit,
                sp,
            )?);
        }
        Ok(outs)
    }
    #[allow(unused_variables)]
    pub(in crate::interp) fn b_sieve(
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
        // 三路过滤（05 §1 `filter(S, q)`，施工件 c）：一组材料 × 一道题（或题列表 / 题式 + 填法）
        // → 三条流 act / ignore / unsure，外加 unobserved（预算提前停止时没问到的）。
        // 直接吃题：全部登记完再一次刷新，同状态的题由融合合成一次调用——
        // 「14 倍」那种绕过批处理的写法在这里没有可写的位置。
        if n != 2 && n != 3 {
            return err(
                Some("E-rt-arg"),
                "sieve(材料列表, 题 | [题…]) 或 sieve(材料列表, 题式, [填法…])",
                sp,
            );
        }
        // 输入可以是列表，也可以是上一个构造的契约值（取它的产出；它的未决与证据带进新契约）
        let (items, carried_pending, carried_evidence) = self.unpack(&args[0], "sieve", sp)?;
        let (qs, many) = if n == 3 {
            let Value::List(fills) = &args[2] else {
                return err(
                    Some("E-rt-arg"),
                    "sieve(材料, 题式, [填法…]) 的第三个参数要是填法记录的列表",
                    sp,
                );
            };
            let mut qs = vec![];
            for f in fills.iter() {
                match self.builtin("fill", vec![args[1].clone(), f.clone()], sp)? {
                    Value::Question(q) => qs.push(q),
                    _ => return err(Some("E-rt-question"), "fill 没有给出题", sp),
                }
            }
            (qs, true)
        } else {
            match &args[1] {
                Value::Question(q) => (vec![q.clone()], false),
                Value::List(l) => {
                    let mut qs = vec![];
                    for q in l.iter() {
                        match q {
                            Value::Question(q) => qs.push(q.clone()),
                            other => {
                                return err(
                                    Some("E-rt-arg"),
                                    format!("sieve 的题列表里有 {}", other.type_name()),
                                    sp,
                                );
                            }
                        }
                    }
                    (qs, true)
                }
                other => {
                    return err(
                        Some("E-rt-arg"),
                        format!(
                            "sieve 的第二个参数要是题或题列表，收到 {}",
                            other.type_name()
                        ),
                        sp,
                    );
                }
            }
        };
        let mut out = self.sieve(&items, &qs, &carried_pending, &carried_evidence, sp)?;
        if many {
            Ok(Value::list(out))
        } else {
            Ok(out.remove(0))
        }
    }
}
