//! 未决与出口读出：`unsure`、`taint`、`line_source`、`untested`、`unsure_cause`、`escalate`、`literalize`、`pending`、`exit_kind`。步 36 G3 从 `host_builtins.rs` 原样搬出（只搬不改）。

use super::*;

impl<'a> Interp<'a> {
    #[allow(unused_variables)]
    pub(crate) fn b_unsure(&mut self, name: &'static str, args: Vec<Value>, sp: Span) -> R<Value> {
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
        match &args[0] {
            // 重新包装：责任继续由这个出口带着，交给调用者
            Value::Duty(e) => Ok(Value::Exit(e.clone())),
            Value::Text(c, _) => {
                // B197（步 36 G3）：原因须为十六种之一；字面量由检查器先报，这里接非字面量
                let Some(c) = UnsureCause::parse(c) else {
                    return err(
                        Some("E-unsure-cause"),
                        format!(
                            "unsure 的原因「{c}」不是未决原因之一。可用：{}（B197）",
                            UnsureCause::ALL.map(|c| c.name()).join("、")
                        ),
                        sp,
                    );
                };
                Ok(self.new_exit(
                    ExitKind::Unsure(Why::of(c)),
                    None,
                    Op::Test,
                    "explicit",
                    "",
                    Taint::Trusted,
                    sp,
                ))
            }
            other => err(
                Some("E-rt-arg"),
                format!(
                    "unsure(未决责任) 重新包装，或 unsure(原因: Text) 新造一个；收到 {}",
                    other.type_name()
                ),
                sp,
            ),
        }
    }

    #[allow(unused_variables)]
    pub(crate) fn b_taint(&mut self, name: &'static str, args: Vec<Value>, sp: Span) -> R<Value> {
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
        if args.len() != 1 {
            return err(Some("E-rt-arg"), "taint(出口)", sp);
        }
        match &args[0] {
            // **宪法第 44 行唯一那条 IFC 纪律建在 taint 上，而 `taint` 从 `cause`
            // 删掉之后，`.jpp` 作者再没有任何东西能说出「这个判断站在不可信材料上」**
            // ——J-08 只会在 `do` 那里**拒绝**，作者拿不到任何**在被拒绝之前**读得到的东西。
            // 对照：「测没测过」被判为必须是一位正交的、对 handler 可见的东西。
            // **重的那条待遇更弱**，这里把它补齐。
            Value::Duty(e) | Value::Exit(e) => Ok(Value::text(match e.taint {
                Taint::Trusted => "trusted",
                Taint::Untrusted => "untrusted",
            })),
            other => err(
                Some("E-rt-arg"),
                format!("taint 只收未决责任或出口，收到 {}", other.type_name()),
                sp,
            ),
        }
    }

    #[allow(unused_variables)]
    pub(crate) fn b_line_source(
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
        if args.len() != 1 {
            return err(Some("E-rt-arg"), "line_source(出口)", sp);
        }
        match &args[0] {
            Value::Duty(e) | Value::Exit(e) => Ok(Value::text(&e.line_source)),
            other => err(
                Some("E-rt-arg"),
                format!("line_source 只收未决责任或出口，收到 {}", other.type_name()),
                sp,
            ),
        }
    }

    #[allow(unused_variables)]
    /// `near_boundary(出口)`（Z0497）：读数是否落在画像边界带内（只读快照，不转移责任，同 `untested`）
    pub(crate) fn b_near_boundary(
        &mut self,
        _name: &'static str,
        args: Vec<Value>,
        sp: Span,
    ) -> R<Value> {
        if args.len() != 1 {
            return err(Some("E-rt-arity"), "near_boundary 需要 1 个参数", sp);
        }
        match &args[0] {
            Value::Duty(e) | Value::Exit(e) => Ok(Value::Bool(
                e.near_boundary.get(),
                Taint::Trusted.into(),
                GuardEv::EMPTY,
            )),
            other => err(
                Some("E-rt-arg"),
                format!(
                    "near_boundary 只收未决责任或出口，收到 {}",
                    other.type_name()
                ),
                sp,
            ),
        }
    }

    pub(crate) fn b_untested(
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
        arity(1)?;
        match &args[0] {
            Value::Duty(e) | Value::Exit(e) => Ok(Value::text(e.untested().unwrap_or(""))),
            other => err(
                Some("E-rt-arg"),
                format!("untested 只收未决责任或出口，收到 {}", other.type_name()),
                sp,
            ),
        }
    }

    #[allow(unused_variables)]
    pub(crate) fn b_unsure_cause(
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
        arity(1)?;
        match &args[0] {
            Value::Duty(e) | Value::Exit(e) => Ok(Value::text(&e.cause())),
            other => err(
                Some("E-rt-arg"),
                format!(
                    "unsure_cause 只收未决责任或出口，收到 {}",
                    other.type_name()
                ),
                sp,
            ),
        }
    }

    #[allow(unused_variables)]
    pub(crate) fn b_escalate(
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
        let (Value::Duty(u), Value::State(s), Value::Question(q)) = (&args[0], &args[1], &args[2])
        else {
            return err(
                Some("J-05"),
                "escalate(未决责任, state, 题)：第一个参数要是 unsure 臂收到的那份责任",
                sp,
            );
        };
        let (u, s, q) = (u.clone(), s.clone(), q.clone());
        // escalate 隐含的效应是「输出为人的回答」的那一个（按注册表字段取，步 15a）
        let spec = jpp_effects::ALL
            .into_iter()
            .map(jpp_effects::spec)
            .find(|e| e.output_shape == jpp_effects::OutputShape::Answer)
            .expect("注册表里有问人");
        // C-1：升级记 Escalate，带问人那条 Ask 的账本键（与 `ask` 里算键同一个式子），写在 Ask 之前
        let ask = self.effect_key_of(spec.name, &[&s.hash, &q.hash]);
        self.记去向(
            &u,
            crate::duty::去向::Escalate { ask: Some(ask) },
            "escalate",
            sp,
        );
        u.consumed.set(true);
        *u.consumed_by.borrow_mut() = "escalate".into();
        self.ask(spec, &s, &q, sp)
    }

    #[allow(unused_variables)]
    pub(crate) fn b_literalize(
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
        // 第四参可选：与 `sieve` 同形的选项 `{line}`，原样交给重问那一次的 `cut`（B128 补齐，过程记录
        // 工程-sieve声明线.md §四）。先解析选项，再销旧责任：选项错时旧责任不被动过
        if n != 3 {
            arity(4)?;
        }
        let line = match args.get(3) {
            Some(o) => crate::constructs::sieve::解析筛选项(name, o, sp)?,
            None => super::bridge::CutOpts::default(),
        };
        let (Value::Duty(u), Value::State(s), Value::Question(q)) = (&args[0], &args[1], &args[2])
        else {
            return err(
                Some("J-05"),
                "literalize(未决责任, state, 更字面的题, {line?})：第一个参数要是 unsure 臂收到的那份责任",
                sp,
            );
        };
        let (u, s, mut q) = (u.clone(), s.clone(), q.clone());
        // B84：重问的题来源 = 那个未决出口
        let 未决键 = u.ledger_key.borrow().clone();
        if !未决键.is_empty() {
            Rc::make_mut(&mut q).from_key.insert(未决键);
        }
        u.consumed.set(true);
        *u.consumed_by.borrow_mut() = "literalize".into();
        let reading = self.judge(&s, &[q], sp)?.remove(0);
        // C-1：重问是细化，账本记 Refine{how: literalize, to: 重问那道题的账本键}
        let to = match &reading {
            Value::Reading(r) if !r.ledger_key.is_empty() => Some(r.ledger_key.clone()),
            _ => None,
        };
        self.记去向(
            &u,
            crate::duty::去向::Refine {
                how: "literalize",
                to,
            },
            "literalize",
            sp,
        );
        match reading {
            Value::Reading(r) => {
                // 依据：B128（逐读数核选项与 cut 同一套，B153 (1)）
                if let Err(m) = 核选项(&r, &line) {
                    return err(Some("E-cut-options"), m, sp);
                }
                self.cut(&r, None, line, sp)
            }
            other => Ok(other),
        }
    }

    #[allow(unused_variables)]
    pub(crate) fn b_pending(&mut self, name: &'static str, args: Vec<Value>, sp: Span) -> R<Value> {
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
        let Value::Text(c, _) = &args[0] else {
            return err(Some("E-rt-arg"), "pending(reason: Text)", sp);
        };
        Err(Fault::Halt(Pending {
            cause: "explicit".into(),
            key: String::new(),
            site: sp,
            detail: c.to_string(),
        }))
    }

    #[allow(unused_variables)]
    pub(crate) fn b_exit_kind(
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
        arity(1)?;
        match &args[0] {
            Value::Exit(e) => Ok(Value::text(&e.label())),
            _ => err(Some("E-rt-arg"), "exit_kind 只收出口", sp),
        }
    }
}
