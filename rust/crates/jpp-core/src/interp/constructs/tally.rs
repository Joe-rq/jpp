//! 集合聚合 `tally` 与输入顺序中的前 k 个 `first_k`（施工件 f）。
//! 步 4a 从 `interp.rs` 机械拆出，只搬代码、不改逻辑（21 §三·3）。

use crate::interp::*;

impl<'a> Interp<'a> {
    #[allow(unused_variables)]
    pub(in crate::interp) fn b_tally(
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
        // 集合聚合（05 §1 `agg(S, op)` 的存在 / 全部 / 计数，施工件 f）：吃一个契约值（通常是 sieve 的）。
        // 精确计算，不是概率：计数给区间 [act, act + 未决]，未决里 cause=budget 的是未观察项；
        // 存在、全部是三值出口——结论取决于未决元素时给 unsure。
        // 输入的未决：结论被它们挡住时并入聚合出口（记为已消费），聚合出口进新的未决清单；
        // 没挡住结论时原样带进新契约（13 §3：不许无声消失）。
        arity(1)?;
        let r = &args[0];
        if !is_outcome(r) {
            return err(
                Some("E-rt-arg"),
                format!(
                    "tally 收一个契约值（sieve / pair / outcome 的结果），收到 {}",
                    r.type_name()
                ),
                sp,
            );
        }
        // 步 1（K1，放行方向）：调用者用 outcome 重包契约值时若漏写 detail.ignore，
        // 已决否定会被当成「没有」，all 从 ignore 静默翻成 act。算不出来的必须有人明确说
        // 调用者构造的契约值没有 ignore 键即报错；内置构造各有定义，不受影响。
        // 依据：20 §11.6 第 19 条；21 §三·2 步 1；12 §2.12（B17 契约）。
        if matches!(r.get("kind"), Some(Value::Text(k, _)) if k.as_ref() == "outcome")
            && r.get("detail").and_then(|d| d.get("ignore")).is_none()
        {
            return err(
                Some("E-tally-missing-rejected"),
                "tally 收到调用者构造的契约值，但 detail 里没有 ignore（已决否定）：不写会把「全部」算成成立。修法：抄入原构造的 detail.ignore；确实没有已决否定就显式写 detail: {ignore: []}",
                sp,
            );
        }
        let act = list_of(r.get("value"));
        let ignore = list_of(r.get("detail").and_then(|d| d.get("ignore")));
        let pend = list_of(r.get("pending"));
        let is_budget =
            |e: &Value| matches!(e.get("cause"), Some(Value::Text(t, _)) if t.as_ref() == "budget");
        let no = pend.iter().filter(|e| is_budget(e)).count() as i64;
        let nu = pend.len() as i64 - no;
        let (na, ni) = (act.len() as i64, ignore.len() as i64);
        let mut taint = Taint::Trusted;
        for e in act.iter().chain(&ignore).chain(&pend) {
            if let Some(Value::Exit(x)) = e.get("exit") {
                if x.taint != Taint::Trusted {
                    taint = x.taint;
                }
            }
        }
        // 结论被谁挡住：未观察优先（原因 budget），否则取第一个未决元素的原因
        let blocker = if no > 0 {
            Some("budget".to_string())
        } else {
            pend.first().and_then(|e| e.get("cause")).map(|c| match c {
                Value::Text(t, _) => t.to_string(),
                _ => "band".into(),
            })
        };
        let exists = if na > 0 {
            ExitKind::Act
        } else if let Some(c) = &blocker {
            ExitKind::Unsure(c.clone())
        } else {
            ExitKind::Ignore
        };
        let all = if ni > 0 {
            ExitKind::Ignore
        } else if let Some(c) = &blocker {
            ExitKind::Unsure(c.clone())
        } else {
            ExitKind::Act
        };
        let absorbed = matches!(exists, ExitKind::Unsure(_)) || matches!(all, ExitKind::Unsure(_));
        let ex = self.new_exit(exists, None, Op::Test, "tally:exists", "", taint, sp);
        let al = self.new_exit(all, None, Op::Test, "tally:all", "", taint, sp);
        let mut pending = vec![];
        if absorbed {
            // 元素的未决责任并入聚合出口；聚合出口自己仍要被消费
            for e in &pend {
                if let Some(Value::Exit(x)) = e.get("exit") {
                    x.consumed.set(true);
                    *x.consumed_by.borrow_mut() = "tally".into();
                }
            }
            for x in [&ex, &al] {
                if let Value::Exit(e) = x {
                    if e.is_unsure() {
                        pending.push(Self::pending_entry(Value::Unit, x));
                    }
                }
            }
        } else {
            pending = pend.clone();
            // 已决的聚合出口不带责任
            for x in [&ex, &al] {
                if let Value::Exit(e) = x {
                    if !e.is_unsure() {
                        e.consumed.set(true);
                        *e.consumed_by.borrow_mut() = "tally:decided".into();
                    }
                }
            }
        }
        if absorbed {
            for x in [&ex, &al] {
                if let Value::Exit(e) = x {
                    if !e.is_unsure() {
                        e.consumed.set(true);
                        *e.consumed_by.borrow_mut() = "tally:decided".into();
                    }
                }
            }
        }
        let value = Value::record(vec![
            (
                "n".into(),
                Value::Int(na + ni + nu + no, Taint::Trusted.into()),
            ),
            ("act".into(), Value::Int(na, Taint::Trusted.into())),
            ("ignore".into(), Value::Int(ni, Taint::Trusted.into())),
            ("unsure".into(), Value::Int(nu, Taint::Trusted.into())),
            ("unobserved".into(), Value::Int(no, Taint::Trusted.into())),
            (
                "count".into(),
                Value::list(vec![
                    Value::Int(na, Taint::Trusted.into()),
                    Value::Int(na + nu + no, Taint::Trusted.into()),
                ]),
            ),
            (
                "complete".into(),
                Value::Bool(nu == 0 && no == 0, Taint::Trusted.into()),
            ),
            ("exists".into(), ex),
            ("all".into(), al),
        ]);
        Self::outcome_value(
            "tally",
            value,
            pending,
            list_of(r.get("evidence")),
            r.get("resume").unwrap_or(Value::Unit),
            (0, 0.0),
            Value::record(vec![]),
            Value::Unit,
            sp,
        )
    }
    #[allow(unused_variables)]
    pub(in crate::interp) fn b_first_k(
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
        // 输入顺序中的前 k 个接受项（交接首包「第一个」语义；05 §1 前 k）：
        // 按原顺序走，遇到未决（含 cause=budget 的未观察项）而还没凑够 k 个，就不能宣称后面的接受项是「前 k 个」。
        // 产出 `{items, exit}`：act = 凑够了 k 个且之前没有挡路的；ignore = 全部观察完、确定不足 k 个；
        // unsure = 被挡住。被挡的位置与原因进续接 `resume`（B17 取舍）。
        // 输入的未决原样带进新契约；被挡住时的 unsure 出口也进未决清单。
        arity(2)?;
        let (r, Value::Int(k, _)) = (&args[0], &args[1]) else {
            return err(Some("E-rt-arg"), "first_k(契约值, k: Int)", sp);
        };
        let k = *k;
        if k <= 0 {
            return err(Some("E-rt-arg"), "first_k 的 k 要是正整数", sp);
        }
        if !is_outcome(r) {
            return err(
                Some("E-rt-arg"),
                format!(
                    "first_k 收一个契约值（sieve 的结果），收到 {}",
                    r.type_name()
                ),
                sp,
            );
        }
        let pend = list_of(r.get("pending"));
        let mut all: Vec<(i64, String, Value)> = vec![];
        for e in list_of(r.get("value")) {
            all.push((0, "act".into(), e));
        }
        for e in list_of(r.get("detail").and_then(|d| d.get("ignore"))) {
            all.push((0, "ignore".into(), e));
        }
        for p in &pend {
            let c = match p.get("cause") {
                Some(Value::Text(t, _)) => t.to_string(),
                _ => "band".into(),
            };
            all.push((
                0,
                format!("pending:{c}"),
                p.get("element").unwrap_or(Value::Unit),
            ));
        }
        for x in all.iter_mut() {
            x.0 = match x.2.get("index") {
                Some(Value::Int(i, _)) => i,
                _ => {
                    return err(
                        Some("E-rt-arg"),
                        "first_k：元素缺 index（只收 sieve 产物的元素）",
                        sp,
                    );
                }
            };
        }
        all.sort_by_key(|x| x.0);
        let mut items = vec![];
        // B3：没有候选 → no_candidate（调生成器）；有候选、全部观察完、一个都没接受 → rejected_all（换材料或换前提）；
        // 接受了一些但不足 k 个 → Ignore（确定不足）。
        let mut kind = if all.is_empty() {
            ExitKind::Unsure("no_candidate".into())
        } else if all.iter().all(|x| x.1 == "ignore") {
            ExitKind::Unsure("rejected_all".into())
        } else {
            ExitKind::Ignore
        };
        let mut resume = Value::Unit;
        let mut taint = Taint::Trusted;
        for (idx, tag, e) in &all {
            if let Some(Value::Exit(x)) = e.get("exit") {
                if x.taint != Taint::Trusted {
                    taint = x.taint;
                }
            }
            match tag.as_str() {
                "act" => {
                    items.push(e.clone());
                    if items.len() as i64 == k {
                        kind = ExitKind::Act;
                        break;
                    }
                }
                "ignore" => {}
                t => {
                    let c = t.trim_start_matches("pending:").to_string();
                    kind = ExitKind::Unsure(c.clone());
                    resume = Value::record(vec![
                        ("reason".into(), Value::text("blocked")),
                        ("at".into(), Value::Int(*idx, Taint::Trusted.into())),
                        ("cause".into(), Value::text(&c)),
                    ]);
                    break;
                }
            }
        }
        let ex = self.new_exit(kind, None, Op::Test, "first_k", "", taint, sp);
        let mut pending = pend.clone();
        if let Value::Exit(e) = &ex {
            if e.is_unsure() {
                pending.push(Self::pending_entry(Value::Unit, &ex));
            } else {
                e.consumed.set(true);
                *e.consumed_by.borrow_mut() = "first_k:decided".into();
            }
        }
        let value = Value::record(vec![
            ("items".into(), Value::list(items)),
            ("exit".into(), ex),
            ("k".into(), Value::Int(k, Taint::Trusted.into())),
        ]);
        Self::outcome_value(
            "first_k",
            value,
            pending,
            list_of(r.get("evidence")),
            resume,
            (0, 0.0),
            Value::record(vec![]),
            Value::Unit,
            sp,
        )
    }
}
