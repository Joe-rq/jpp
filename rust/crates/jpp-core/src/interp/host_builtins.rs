//! 内置分派表与通用内置（列表、文本、出口读出等）；内核构造的臂在 `constructs/`（20 §2.3 `host_builtins.rs`）。
//! 步 4a 从 `interp.rs` 机械拆出，只搬代码、不改逻辑（21 §三·3）。

use super::*;

impl<'a> Interp<'a> {
    // ---------- 内置 ----------

    pub(in crate::interp) fn builtin(
        &mut self,
        name: &'static str,
        args: Vec<Value>,
        sp: Span,
    ) -> R<Value> {
        match name {
            "state" => self.make_state(&args, sp),
            "test" | "select" => self.b_test(name, args, sp),
            "measure" => self.b_measure(name, args, sp),
            "form" => self.b_form(name, args, sp),
            "fill" => self.b_fill(name, args, sp),
            "judge" => self.b_judge(name, args, sp),
            "sieve" => self.b_sieve(name, args, sp),
            "pair" => self.b_pair(name, args, sp),
            "tally" => self.b_tally(name, args, sp),
            "first_k" => self.b_first_k(name, args, sp),
            "iterate" => self.b_iterate(name, args, sp),
            "outcome" => self.b_outcome(name, args, sp),
            "key_of" => self.b_key_of(name, args, sp),
            "cut" => self.b_cut(name, args, sp),
            "handle" => self.b_handle(name, args, sp),
            "consume" => self.b_consume(name, args, sp),
            "gen" => self.b_gen(name, args, sp),
            "do" => self.b_do(name, args, sp),
            "ask" => self.b_ask(name, args, sp),
            "transform" => self.b_transform(name, args, sp),
            "mat" => self.b_mat(name, args, sp),
            "content" => self.b_content(name, args, sp),
            "unsure" => self.b_unsure(name, args, sp),
            // **J-15 那一位对 handler 可见**，不是只进 trace（`12` §2.11 硬要求一）。
            // 理由是路由真的不同：`tie` 的既定去向是「逐候选 noul」，而没测过的那条路
            // （K-noul）**本来就是逐候选 noul，路过去是空转**。handler 看不见那一位，
            // 就只能把两种情形当同一件事办——**那正是要消除的东西**。
            //
            // 返回 `""` 表示「都测过了」，返回载体名表示「那个量没测」。与 `unsure_cause`
            // 一样是**只读快照，不转移责任**（见 tests/duty.rs：读原因不算处理）。
            "taint" => self.b_taint(name, args, sp),
            "line_source" => self.b_line_source(name, args, sp),
            "untested" => self.b_untested(name, args, sp),
            "unsure_cause" => self.b_unsure_cause(name, args, sp),
            // 合法去向之一：把责任交给明确关联的人工请求（效应 ask）
            "escalate" => self.b_escalate(name, args, sp),
            // 合法去向之一：接走旧责任、按更字面的题重问，产生新的待处理出口（效应 judge）
            "literalize" => self.b_literalize(name, args, sp),
            "pending" => self.b_pending(name, args, sp),
            "fail" => self.b_fail(name, args, sp),
            "is_fail" => self.b_is_fail(name, args, sp),
            // ---- 长处（G4 §7「判断力花在哪」）：读数是带校准线的随机变量，不是值。
            // 这两个构件只读校准线、不做跨题算术、返回宿主值不返回读数，所以合法（Python
            // `runtime.py:1236` allocate 的 docstring 原话）。都不花钱：不发调用、不进账本、不动预算。
            "allocate" => self.b_allocate(name, args, sp),
            "unsure_bound" => self.b_unsure_bound(name, args, sp),
            // 判断向量的两法（12:134「合法操作**只有两种**…其余运算不存在（J-01）」）。
            // 它们不是「读数列表上的工具函数」——正因为只有这两种，读数才不会被当成数用。
            // **同题重复读数的合并（B28）**：`repeat`（原 `agg`）只许均值或中位数，用于压抖动、不为降错；
            // 合并结果过桥用**含 n 的独立校准键**（`键·repeat(n=…)`，不借题式或模式线），账本记 n；
            // 对出口取众数（多数表决）禁止——choice 取各候选概率的均值 / 中位数，不投票。
            // 键未通过重跑分歧检验时（记录 `rerun_independent` 不为真）照常合并但告警：错误持久时重问不降错（B9）。
            "agg" | "repeat" => self.b_agg(name, args, sp),
            "order" => self.b_order(name, args, sp),
            // fit 桥（12 §6.0:315 `fit(名, [读数…])`，**输出仍是读数、仍要 cut**）。
            // 让什么活下来：**跨题的联合判断在类型上仍是读数**，因而仍要过线、仍可能 unsure。
            // 直接产出出口就绕过了 cut 的判序（insufficient → taint → 过线 → band）。
            "fit" => self.b_fit(name, args, sp),
            "exit_kind" => self.b_exit_kind(name, args, sp),
            "loop" => self.b_loop(name, args, sp),
            "stop" => self.b_stop(name, args, sp),
            "len" => self.b_len(name, args, sp),
            "map" | "filter" => self.b_map(name, args, sp),
            "fold" => self.b_fold(name, args, sp),
            "range" => self.b_range(name, args, sp),
            "append" => self.b_append(name, args, sp),
            "concat" => self.b_concat(name, args, sp),
            "slice" => self.b_slice(name, args, sp),
            "contains" => self.b_contains(name, args, sp),
            "sum" => self.b_sum(name, args, sp),
            "min" | "max" => self.b_min(name, args, sp),
            "abs" => self.b_abs(name, args, sp),
            "floor" => self.b_floor(name, args, sp),
            "reverse" => self.b_reverse(name, args, sp),
            "keys" => self.b_keys(name, args, sp),
            "has" => self.b_has(name, args, sp),
            "with" => self.b_with(name, args, sp),
            "text" => self.b_text(name, args, sp),
            "join" => self.b_join(name, args, sp),
            "print" => self.b_print(name, args, sp),
            _ => err(Some("E-rt-name"), format!("未知内置 {name}"), sp),
        }
    }
    #[allow(unused_variables)]
    pub(in crate::interp) fn b_judge(
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
        arity(2)?;
        // 12:129 的向量化形式：`judge(ss: [State], qs) → [Readings]`「状态列表，同层并发」。
        // 同一道题问多个对象——判断向量（`order`）要的正是这个形状。
        // 它们同层登记，所以一次刷新就全发出去。
        if let Value::List(states) = &args[0] {
            let mut qs = vec![];
            match &args[1] {
                Value::Question(q) => qs.push(q.clone()),
                Value::List(l) => {
                    for q in l.iter() {
                        match q {
                            Value::Question(q) => qs.push(q.clone()),
                            _ => return err(Some("E-rt-arg"), "judge 的题列表里有非题", sp),
                        }
                    }
                }
                _ => return err(Some("E-rt-arg"), "judge 的第二个参数要是题或题列表", sp),
            }
            let mut out = vec![];
            for st in states.iter() {
                let Value::State(st) = st else {
                    return err(Some("E-rt-arg"), "judge 的状态列表里有非状态", sp);
                };
                let rs = self.judge(st, &qs, sp)?;
                // 单题时每个对象给一条读数（而不是一个只有一条的列表），`order` 才好用
                out.push(if qs.len() == 1 {
                    rs.into_iter().next().expect("单题一条")
                } else {
                    Value::list(rs)
                });
            }
            return Ok(Value::list(out));
        }
        let Value::State(s) = &args[0] else {
            return err(
                Some("E-rt-arg"),
                "judge(state | [states], question | [questions])",
                sp,
            );
        };
        match &args[1] {
            Value::Question(q) => Ok(self.judge(s, &[q.clone()], sp)?.remove(0)),
            Value::List(l) => {
                let mut qs = vec![];
                for q in l.iter() {
                    match q {
                        Value::Question(q) => qs.push(q.clone()),
                        _ => return err(Some("E-rt-arg"), "judge 的题列表里有非题", sp),
                    }
                }
                Ok(Value::list(self.judge(s, &qs, sp)?))
            }
            _ => err(Some("E-rt-arg"), "judge 的第二个参数要是题或题列表", sp),
        }
    }
    #[allow(unused_variables)]
    pub(in crate::interp) fn b_cut(
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
        if n == 0 || n > 3 {
            return err(
                Some("E-rt-arg"),
                "cut(reading)、cut(reading, calib_key)、cut(reading, {cost: [fp, fn]}) 或 cut(reading, calib_key, {cost: [fp, fn]})",
                sp,
            );
        }
        // 第二、三位：校准键（Text）与代价（记录 {cost: [fp, fn]}，B29）。线仍只来自记录：
        // 给了代价，就用该记录上按这个代价矩阵认证过的那张证书的线；没有这张证书 → 冷。
        let mut calib: Option<String> = None;
        let mut cost: Option<(f64, f64)> = None;
        for a in args.iter().skip(1) {
            match a {
                Value::Text(k, _) if calib.is_none() && cost.is_none() => {
                    calib = Some(k.to_string())
                }
                Value::Record(_) if cost.is_none() => {
                    let c = a.get("cost").ok_or_else(|| {
                        Fault::Error(RtError::new(
                            Some("E-rt-arg"),
                            "cut 的记录参数只认 {cost: [fp, fn]}",
                            sp,
                        ))
                    })?;
                    let nums: Vec<f64> = match &c {
                        Value::List(l) => l
                            .iter()
                            .filter_map(|x| match x {
                                Value::Int(i, _) => Some(*i as f64),
                                Value::Float(f, _) => Some(*f),
                                _ => None,
                            })
                            .collect(),
                        _ => vec![],
                    };
                    if nums.len() != 2 || nums.iter().any(|x| !(*x > 0.0)) {
                        return err(
                            Some("E-rt-arg"),
                            "cost 要是两个正数 [fp, fn]：放错一条（假放行）与漏掉一条（假拒绝）的代价",
                            sp,
                        );
                    }
                    cost = Some((nums[0], nums[1]));
                }
                other => {
                    return err(
                        Some("J-03"),
                        format!(
                            "cut 的校准参数必须是校准记录的键（Text）或代价记录 {{cost: [fp, fn]}}，不能是字面量线；收到 {}",
                            other.type_name()
                        ),
                        sp,
                    );
                }
            }
        }
        match &args[0] {
            Value::Reading(r) => self.cut(r, calib.as_deref(), cost, sp),
            Value::List(l) => {
                let mut out = vec![];
                for r in l.iter() {
                    match r {
                        Value::Reading(r) => out.push(self.cut(r, calib.as_deref(), cost, sp)?),
                        _ => return err(Some("E-rt-arg"), "cut 的列表里有非读数", sp),
                    }
                }
                Ok(Value::list(out))
            }
            other => err(
                Some("E-rt-arg"),
                format!("cut 只收读数，收到 {}", other.type_name()),
                sp,
            ),
        }
    }
    #[allow(unused_variables)]
    pub(in crate::interp) fn b_handle(
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
        arity(2)?;
        let Value::Exit(e) = &args[0] else {
            return err(
                Some("E-rt-arg"),
                format!("handle 的第一个参数要是出口，收到 {}", args[0].type_name()),
                sp,
            );
        };
        let e = e.clone();
        self.handle(&e, &args[1], sp)
    }
    #[allow(unused_variables)]
    pub(in crate::interp) fn b_consume(
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
        arity(2)?;
        let Value::Text(how, _) = &args[1] else {
            return err(Some("E-rt-arg"), "consume(exit | [exits], \"drop\")", sp);
        };
        if how.as_ref() != "drop" {
            return err(
                Some("E-rt-arg"),
                "consume 目前只支持 \"drop\"；升级用 ask",
                sp,
            );
        }
        // 契约值：丢它的整个未决清单（每项的 exit）
        let list: Vec<Value> = if is_outcome(&args[0]) {
            list_of(args[0].get("pending"))
                .into_iter()
                .filter_map(|e| e.get("exit"))
                .collect()
        } else {
            match &args[0] {
                Value::List(l) => l.iter().cloned().collect(),
                v => vec![v.clone()],
            }
        };
        for v in &list {
            match v {
                Value::Exit(e) | Value::Duty(e) => {
                    if e.is_unsure() && !e.consumed.get() {
                        // drop 是合法去向（12 §6「unsure 显式丢弃并记账」），但要留痕
                        self.trace.warn(format!(
                                "W-drop-vs-escalate: 显式丢弃了未决责任 {}（题 {}）；drop 合法且已记账，但只有 escalate 会把它交给人",
                                e.label(),
                                头(&e.q_hash, 8)
                            ));
                    }
                    e.consumed.set(true);
                    *e.consumed_by.borrow_mut() = "consume:drop".into();
                }
                _ => return err(Some("E-rt-arg"), "consume 只收出口或未决责任", sp),
            }
        }
        Ok(Value::Unit)
    }
    #[allow(unused_variables)]
    pub(in crate::interp) fn b_gen(
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
        arity(4)?;
        let (Value::Text(p, _), ctx, Value::Int(k, _), Value::Int(r, _)) =
            (&args[0], &args[1], &args[2], &args[3])
        else {
            return err(Some("E-rt-arg"), "gen(prompt, [ctx], n, retry_seq)", sp);
        };
        let (ctx, _) = self.as_mats(ctx, "ctx", sp)?;
        let p = p.to_string();
        self.generate(&p, &ctx, *k as usize, *r, sp)
    }
    #[allow(unused_variables)]
    pub(in crate::interp) fn b_do(
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
        let (Value::Text(a, _), Value::List(l), Value::Int(i, _)) = (&args[0], &args[1], &args[2])
        else {
            return err(Some("E-rt-arg"), "do(action, [args], iter_seq)", sp);
        };
        let a = a.to_string();
        let l: Vec<Value> = l.iter().cloned().collect();
        self.do_(&a, &l, *i, sp)
    }
    #[allow(unused_variables)]
    pub(in crate::interp) fn b_ask(
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
        arity(2)?;
        let (Value::State(s), Value::Question(q)) = (&args[0], &args[1]) else {
            return err(Some("E-rt-arg"), "ask(state, question)", sp);
        };
        let (s, q) = (s.clone(), q.clone());
        self.ask(&s, &q, sp)
    }
    #[allow(unused_variables)]
    pub(in crate::interp) fn b_transform(
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
        if n < 1 {
            return err(Some("E-rt-arg"), "transform(f, mats…)", sp);
        }
        let Value::Fn(f) = &args[0] else {
            return err(Some("E-rt-arg"), "transform 的第一个参数要是函数", sp);
        };
        let f = f.clone();
        self.transform(&f, &args[1..], sp)
    }
    #[allow(unused_variables)]
    pub(in crate::interp) fn b_mat(
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
        Ok(Value::Mat(Rc::new(self.as_mat(&args[0], "mat", sp)?)))
    }
    #[allow(unused_variables)]
    pub(in crate::interp) fn b_content(
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
            // 读出规则（B33 第 2 点）：从 untrusted 材料读出的值，所有叶子标 untrusted。
            // 值自己带着来源，拼接、join、text、取字段之后仍带着（内置输出 ∨ 输入），
            // 取代此前按值匹配的旁路表（982d7ca）：那张表没有作用域，同时漏与串。
            // B84：读出的叶子同时带材料的来源读数
            Value::Mat(m) => Ok(json_to_value(&m.content).with_prov(&m.prov())),
            Value::Reading(_) => err(Some("J-01"), "读数没有内容可读；只能经 cut 离开", sp),
            other => err(
                Some("E-rt-arg"),
                format!("content 只收材料，收到 {}", other.type_name()),
                sp,
            ),
        }
    }
    #[allow(unused_variables)]
    pub(in crate::interp) fn b_unsure(
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
            // 重新包装：责任继续由这个出口带着，交给调用者
            Value::Duty(e) => Ok(Value::Exit(e.clone())),
            Value::Text(c, _) => {
                let c = c.to_string();
                Ok(self.new_exit(
                    ExitKind::Unsure(c),
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
    pub(in crate::interp) fn b_taint(
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
    pub(in crate::interp) fn b_line_source(
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
    pub(in crate::interp) fn b_untested(
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
    pub(in crate::interp) fn b_unsure_cause(
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
    pub(in crate::interp) fn b_escalate(
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
        u.consumed.set(true);
        *u.consumed_by.borrow_mut() = "escalate".into();
        self.ask(&s, &q, sp)
    }
    #[allow(unused_variables)]
    pub(in crate::interp) fn b_literalize(
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
                "literalize(未决责任, state, 更字面的题)：第一个参数要是 unsure 臂收到的那份责任",
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
        match reading {
            Value::Reading(r) => self.cut(&r, None, None, sp),
            other => Ok(other),
        }
    }
    #[allow(unused_variables)]
    pub(in crate::interp) fn b_pending(
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
    pub(in crate::interp) fn b_fail(
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
        let Value::Text(c, t) = &args[0] else {
            return err(Some("E-rt-arg"), "fail(reason: Text)", sp);
        };
        Ok(Value::Fail(Rc::from(c.as_ref()), t.clone()))
    }
    #[allow(unused_variables)]
    pub(in crate::interp) fn b_is_fail(
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
        Ok(Value::Bool(
            matches!(args[0], Value::Fail(..)),
            Taint::Trusted.into(),
        ))
    }
    #[allow(unused_variables)]
    pub(in crate::interp) fn b_exit_kind(
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
    #[allow(unused_variables)]
    pub(in crate::interp) fn b_loop(
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
        let Value::Int(b, _) = &args[0] else {
            return err(Some("J-06"), "loop 的 bound 必须是整数字面量或整数值", sp);
        };
        let (b, init, step) = (*b, args[1].clone(), args[2].clone());
        self.loop_(b, init, &step, sp)
    }
    #[allow(unused_variables)]
    pub(in crate::interp) fn b_stop(
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
        Ok(Value::Stop(Rc::new(args[0].clone())))
    }
    #[allow(unused_variables)]
    pub(in crate::interp) fn b_len(
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
            Value::List(l) => Ok(Value::Int(l.len() as i64, Taint::Trusted.into())),
            Value::Text(t, _) => Ok(Value::Int(t.chars().count() as i64, Taint::Trusted.into())),
            Value::Record(r) => Ok(Value::Int(r.len() as i64, Taint::Trusted.into())),
            other => err(
                Some("E-rt-type"),
                format!("len 不适用于 {}", other.type_name()),
                sp,
            ),
        }
    }
    #[allow(unused_variables)]
    pub(in crate::interp) fn b_map(
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
        arity(2)?;
        let (Value::List(l), f) = (&args[0], &args[1]) else {
            return err(Some("E-rt-arg"), format!("{name}(list, fn)"), sp);
        };
        self.vectorize_ahead(f, l, sp);
        let mut out = vec![];
        for it in l.iter() {
            let r = self.apply(f.clone(), vec![it.clone()], sp)?;
            if name == "map" {
                out.push(r);
                continue;
            }
            // filter 的谓词必须返回 Bool。返回别的东西以前被**静默当假**：
            // 出口、未决责任传进来会无声消失，正是 13 §3 要堵的那类。
            match r {
                Value::Bool(true, _) => out.push(it.clone()),
                Value::Bool(false, _) => {}
                other => {
                    return err(
                        Some("E-rt-type"),
                        format!(
                            "filter 的谓词要返回 Bool（真假），收到 {}。返回别的东西以前被当成假、元素被静默丢掉。修法：让谓词自己算出真假再返回",
                            other.type_name()
                        ),
                        sp,
                    );
                }
            }
        }
        Ok(Value::list(out))
    }
    #[allow(unused_variables)]
    pub(in crate::interp) fn b_fold(
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
        let (Value::List(l), init, f) = (&args[0], &args[1], &args[2]) else {
            return err(Some("E-rt-arg"), "fold(list, init, fn(acc, x))", sp);
        };
        let mut acc = init.clone();
        for it in l.iter() {
            acc = self.apply(f.clone(), vec![acc, it.clone()], sp)?;
        }
        Ok(acc)
    }
    #[allow(unused_variables)]
    pub(in crate::interp) fn b_range(
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
        arity(2)?;
        let (Value::Int(a, _), Value::Int(b, _)) = (&args[0], &args[1]) else {
            return err(Some("E-rt-arg"), "range(a, b)", sp);
        };
        Ok(Value::list((*a..*b).map(Value::int).collect()))
    }
    #[allow(unused_variables)]
    pub(in crate::interp) fn b_append(
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
        arity(2)?;
        let Value::List(l) = &args[0] else {
            return err(Some("E-rt-arg"), "append(list, v)", sp);
        };
        let mut v: Vec<Value> = l.iter().cloned().collect();
        v.push(args[1].clone());
        Ok(Value::list(v))
    }
    #[allow(unused_variables)]
    pub(in crate::interp) fn b_concat(
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
        arity(2)?;
        let (Value::List(a), Value::List(b)) = (&args[0], &args[1]) else {
            return err(Some("E-rt-arg"), "concat(a, b)", sp);
        };
        Ok(Value::list(a.iter().chain(b.iter()).cloned().collect()))
    }
    #[allow(unused_variables)]
    pub(in crate::interp) fn b_slice(
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
        let (Value::List(l), Value::Int(a, _), Value::Int(b, _)) = (&args[0], &args[1], &args[2])
        else {
            return err(Some("E-rt-arg"), "slice(list, a, b)", sp);
        };
        let a = (*a).clamp(0, l.len() as i64) as usize;
        let b = (*b).clamp(a as i64, l.len() as i64) as usize;
        Ok(Value::list(l[a..b].to_vec()))
    }
    #[allow(unused_variables)]
    pub(in crate::interp) fn b_contains(
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
        arity(2)?;
        let Value::List(l) = &args[0] else {
            return err(Some("E-rt-arg"), "contains(list, v)", sp);
        };
        for it in l.iter() {
            match it.equals(&args[1]) {
                Some(true) => return Ok(Value::Bool(true, Taint::Trusted.into())),
                None => return err(Some("J-01"), "读数不可比", sp),
                _ => {}
            }
        }
        Ok(Value::Bool(false, Taint::Trusted.into()))
    }
    #[allow(unused_variables)]
    pub(in crate::interp) fn b_sum(
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
        let Value::List(l) = &args[0] else {
            return err(Some("E-rt-arg"), "sum(list)", sp);
        };
        let mut acc = Value::Int(0, Taint::Trusted.into());
        for it in l.iter() {
            acc = self.binop("+", acc, it.clone(), sp)?;
        }
        Ok(acc)
    }
    #[allow(unused_variables)]
    pub(in crate::interp) fn b_min(
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
        arity(2)?;
        let (Value::Int(a, _), Value::Int(b, _)) = (&args[0], &args[1]) else {
            return err(Some("E-rt-arg"), format!("{name}(Int, Int)"), sp);
        };
        Ok(Value::Int(
            if name == "min" { *a.min(b) } else { *a.max(b) },
            Taint::Trusted.into(),
        ))
    }
    #[allow(unused_variables)]
    pub(in crate::interp) fn b_abs(
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
            // 13 §6：i64::MIN 没有对应的正数，取绝对值同样越界
            Value::Int(a, _) => Ok(Value::Int(
                a.checked_abs()
                    .ok_or_else(|| overflow("取绝对值", *a, 0, sp))?,
                Taint::Trusted.into(),
            )),
            Value::Float(a, _) => Ok(Value::Float(a.abs(), Taint::Trusted.into())),
            _ => err(Some("E-rt-arg"), "abs(number)", sp),
        }
    }
    #[allow(unused_variables)]
    pub(in crate::interp) fn b_floor(
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
            Value::Float(a, _) => Ok(Value::Int(a.floor() as i64, Taint::Trusted.into())),
            Value::Int(a, _) => Ok(Value::Int(*a, Taint::Trusted.into())),
            _ => err(Some("E-rt-arg"), "floor(number)", sp),
        }
    }
    #[allow(unused_variables)]
    pub(in crate::interp) fn b_reverse(
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
        let Value::List(l) = &args[0] else {
            return err(Some("E-rt-arg"), "reverse(list)", sp);
        };
        Ok(Value::list(l.iter().rev().cloned().collect()))
    }
    #[allow(unused_variables)]
    pub(in crate::interp) fn b_keys(
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
        let Value::Record(r) = &args[0] else {
            return err(Some("E-rt-arg"), "keys(record)", sp);
        };
        Ok(Value::list(r.iter().map(|(k, _)| Value::text(k)).collect()))
    }
    #[allow(unused_variables)]
    pub(in crate::interp) fn b_has(
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
        arity(2)?;
        let (Value::Record(_), Value::Text(k, _)) = (&args[0], &args[1]) else {
            return err(Some("E-rt-arg"), "has(record, key)", sp);
        };
        Ok(Value::Bool(args[0].get(k).is_some(), Taint::Trusted.into()))
    }
    #[allow(unused_variables)]
    pub(in crate::interp) fn b_with(
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
        let (Value::Record(r), Value::Text(k, _)) = (&args[0], &args[1]) else {
            return err(Some("E-rt-arg"), "with(record, key, value)", sp);
        };
        let mut v: Vec<(String, Value)> = r
            .iter()
            .filter(|(kk, _)| kk.as_str() != k.as_ref())
            .cloned()
            .collect();
        v.push((k.to_string(), args[2].clone()));
        Ok(Value::record(v))
    }
    #[allow(unused_variables)]
    pub(in crate::interp) fn b_text(
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
            Value::Text(t, _) => Ok(Value::text(t)),
            Value::Reading(_) => err(Some("J-01"), "读数不能转文字", sp),
            // 读出规则：材料的文字带材料的位；其余由分派处的 ∨ 输入给出
            // B84：同时带材料的来源读数
            Value::Mat(m) => Ok(Value::Text(Rc::from(m.text().as_str()), m.prov())),
            other => Ok(Value::text(other.to_json().to_string().trim_matches('"'))),
        }
    }
    #[allow(unused_variables)]
    pub(in crate::interp) fn b_join(
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
        arity(2)?;
        let (Value::List(l), Value::Text(sep, _)) = (&args[0], &args[1]) else {
            return err(Some("E-rt-arg"), "join([Text], sep)", sp);
        };
        let parts: Vec<String> = l
            .iter()
            .map(|v| match v {
                Value::Text(t, _) => t.to_string(),
                o => o.to_json().to_string(),
            })
            .collect();
        Ok(Value::text(&parts.join(sep)))
    }
    #[allow(unused_variables)]
    pub(in crate::interp) fn b_print(
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
        let s = args[0].to_json().to_string();
        self.trace.push("print", "", false, 0.0, sp, s);
        Ok(Value::Unit)
    }
}
