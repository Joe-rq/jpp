//! 题与题式：`test`、`select`、`measure`、`form`、`fill`。
//! 步 4a 从 `interp.rs` 机械拆出，只搬代码、不改逻辑（21 §三·3）。

use crate::interp::*;

impl<'a> Interp<'a> {
    #[allow(unused_variables)]
    pub(in crate::interp) fn b_test(
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
        if n != 2 && n != 3 {
            return err(
                Some("E-rt-arity"),
                format!(
                    "{name} 需要 2 或 3 个参数（题面, calib[, {{evidence: [槽名…]}}]），收到 {n}"
                ),
                sp,
            );
        }
        let (Value::Text(t, _), Value::Text(c, _)) = (&args[0], &args[1]) else {
            return err(
                Some("J-03"),
                format!("{name}(题面: Text, calib: Text) — calib 是校准记录的键，不是线"),
                sp,
            );
        };
        let op = if name == "test" { Op::Test } else { Op::Select };
        let evidence = evidence_of(args.get(2), sp)?;
        let mut q = Question::with_evidence(op, t, c, vec![], evidence);
        let (presupposition, request) = question_decl_of(args.get(2), op, sp)?;
        q.presupposition = presupposition;
        q.request = request;
        Ok(Value::Question(Rc::new(q)))
    }
    #[allow(unused_variables)]
    pub(in crate::interp) fn b_measure(
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
        arity(3)?;
        let (Value::Text(t, _), Value::List(scale), Value::Text(c, _)) =
            (&args[0], &args[1], &args[2])
        else {
            return err(Some("E-rt-question"), "measure(题面, [档位…], calib)", sp);
        };
        let mut sc = vec![];
        for s in scale.iter() {
            match s {
                Value::Text(x, _) => sc.push(x.to_string()),
                _ => return err(Some("E-rt-question"), "档位要是 Text", sp),
            }
        }
        if sc.len() < 2 {
            return err(Some("E-rt-question"), "measure 至少两档", sp);
        }
        Ok(Value::Question(Rc::new(Question::new(
            Op::Measure,
            t,
            c,
            sc,
        ))))
    }
    #[allow(unused_variables)]
    pub(in crate::interp) fn b_form(
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
        // form(题型, 模板题面, {calib, scale?, evidence?, presupposition?, request?}) → 题式
        arity(3)?;
        let (Value::Text(opname, _), Value::Text(template, _)) = (&args[0], &args[1]) else {
            return err(
                Some("E-rt-question"),
                "form(题型: \"test\" | \"select\" | \"measure\", 模板题面: Text, {calib: \"校准键\", …})",
                sp,
            );
        };
        let op = match opname.as_ref() {
            "test" => Op::Test,
            "select" => Op::Select,
            "measure" => Op::Measure,
            other => {
                return err(
                    Some("E-rt-question"),
                    format!("form 的题型要是 test / select / measure，收到 {other}"),
                    sp,
                );
            }
        };
        let Value::Record(_) = &args[2] else {
            return err(
                Some("E-rt-question"),
                "form 的第三个参数要是记录：{calib: \"校准键\", …}",
                sp,
            );
        };
        let calib = match args[2].get("calib") {
            Some(Value::Text(c, _)) => c.to_string(),
            Some(Value::Int(_, _)) | Some(Value::Float(_, _)) => {
                return err(
                    Some("J-03"),
                    "form 的 calib 是数字：线不可字面，这一位只收校准记录的键（Text）",
                    sp,
                );
            }
            _ => {
                return err(
                    Some("J-03"),
                    "form 需要 calib：{calib: \"校准键\"}。线只从校准记录来",
                    sp,
                );
            }
        };
        let mut scale = vec![];
        if let Some(v) = args[2].get("scale") {
            let Value::List(l) = v else {
                return err(Some("E-rt-question"), "scale 要是档位列表", sp);
            };
            for x in l.iter() {
                match x {
                    Value::Text(t, _) => scale.push(t.to_string()),
                    _ => return err(Some("E-rt-question"), "档位要是 Text", sp),
                }
            }
        }
        match (op, scale.len()) {
            (Op::Measure, n) if n < 2 => {
                return err(
                    Some("E-rt-question"),
                    "measure 题式至少两档：{scale: [\"低\", \"高\"]}",
                    sp,
                );
            }
            (Op::Test | Op::Select, n) if n > 0 => {
                return err(Some("E-rt-question"), "只有 measure 题式带 scale", sp);
            }
            _ => {}
        }
        let evidence = evidence_of(Some(&args[2]), sp)?;
        let (presupposition, request) = question_decl_of(Some(&args[2]), op, sp)?;
        let mut f = crate::value::Form::new(
            op,
            template,
            &calib,
            scale,
            evidence,
            presupposition,
            request,
        )
        .map_err(|m| Fault::Error(RtError::new(Some("E-rt-question"), m, sp)))?;
        // B76（步 12e-2）：题式的 `over` 声明，只决定题类，不进 form_hash
        f.over_kind = match args[2].get("over_kind") {
            None | Some(Value::Unit) => None,
            Some(Value::Text(t, _)) => Some(crate::value::OverKind::parse(&t).ok_or_else(|| {
                Fault::Error(RtError::new(
                    Some("E-rt-question"),
                    format!("over_kind 「{t}」不认识：只收 labels、candidates、questions、actions（B76）"),
                    sp,
                ))
            })?),
            Some(other) => {
                return err(
                    Some("E-rt-question"),
                    format!("over_kind 要是文本，收到 {}", other.type_name()),
                    sp,
                );
            }
        };
        Ok(Value::Form(Rc::new(f)))
    }
    #[allow(unused_variables)]
    pub(in crate::interp) fn b_fill(
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
        // fill(题式, {槽: 值, …}) → 题。值按 text() 渲染；Int/Float/Bool/Text 以外的值不能填进题面。
        arity(2)?;
        let Value::Form(f) = &args[0] else {
            return err(
                Some("E-rt-question"),
                format!(
                    "fill 的第一个参数要是题式（form(…) 的结果），收到 {}",
                    args[0].type_name()
                ),
                sp,
            );
        };
        let Value::Record(fields) = &args[1] else {
            return err(
                Some("E-rt-question"),
                "fill 的第二个参数要是记录：{槽名: 值}",
                sp,
            );
        };
        let mut fill = vec![];
        for (k, v) in fields.iter() {
            let t = match v {
                Value::Text(t, _) => t.to_string(),
                Value::Int(i, _) => i.to_string(),
                Value::Float(x, _) => x.to_string(),
                Value::Bool(b, _) => b.to_string(),
                Value::Reading(_) => {
                    return err(
                        Some("J-01"),
                        format!("槽 {k} 填的是读数：读数不能进题面（它不是材料，也不可渲染）"),
                        sp,
                    );
                }
                other => {
                    return err(
                        Some("E-rt-question"),
                        format!(
                            "槽 {k} 要填 Text / Int / Float / Bool，收到 {}",
                            other.type_name()
                        ),
                        sp,
                    );
                }
            };
            fill.push((k.clone(), t));
        }
        let q = f
            .fill(&fill)
            .map_err(|m| Fault::Error(RtError::new(Some("E-rt-question"), m, sp)))?;
        Ok(Value::Question(Rc::new(q)))
    }
}
