//! `judge`、`cut`、`handle`、`consume`。步 36 G3 从 `host_builtins.rs` 原样搬出（只搬不改）。

use super::*;

impl<'a> Interp<'a> {
    #[allow(unused_variables)]
    pub(crate) fn b_judge(&mut self, name: &'static str, args: Vec<Value>, sp: Span) -> R<Value> {
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
                self.记判断来历(st, &qs, &rs);
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
            Value::Question(q) => {
                let rs = self.judge(s, &[q.clone()], sp)?;
                // J-05 默认链再判要用（B0492 S2）
                self.记判断来历(s, &[q.clone()], &rs);
                Ok(rs.into_iter().next().expect("单题一条"))
            }
            Value::List(l) => {
                let mut qs = vec![];
                for q in l.iter() {
                    match q {
                        Value::Question(q) => qs.push(q.clone()),
                        _ => return err(Some("E-rt-arg"), "judge 的题列表里有非题", sp),
                    }
                }
                let rs = self.judge(s, &qs, sp)?;
                self.记判断来历(s, &qs, &rs);
                Ok(Value::list(rs))
            }
            _ => err(Some("E-rt-arg"), "judge 的第二个参数要是题或题列表", sp),
        }
    }

    #[allow(unused_variables)]
    pub(crate) fn b_cut(&mut self, name: &'static str, args: Vec<Value>, sp: Span) -> R<Value> {
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
                "cut(reading)、cut(reading, calib_key)、cut(reading, {declare | cost | alpha | stat}) 或 cut(reading, calib_key, {…})",
                sp,
            );
        }
        // 第二、三位：校准键（Text）与策略记录（B129 三式：`declare` 作者声明线、`cost` 代价、`alpha` 可接受
        // 假放行率；步 20j-3 加 `stat` 统计量，B153、B154）。`cost`/`alpha` 的线仍只来自记录；`declare` 按作者
        // 写的数切（B128）。策略记录的解析在 `解析策略`（`sieve` 的 `{line}` 同用）。依据：B128、B129
        let mut calib: Option<String> = None;
        let mut opts = super::bridge::CutOpts::default();
        let mut 有记录 = false;
        let 选项错 = |msg: String| -> R<Value> { err(Some("E-cut-options"), msg, sp) };
        for a in args.iter().skip(1) {
            match a {
                Value::Text(k, _) if calib.is_none() && !有记录 => calib = Some(k.to_string()),
                Value::Record(_) if !有记录 => {
                    有记录 = true;
                    opts = 解析策略(a, sp)?;
                }
                // 裸数字读作作者声明线 `{declare: {hi: 数}}`（B188 第 1 条、B129 推翻条件 (2) 提前触发；批 9）
                Value::Float(..) | Value::Int(..) if !有记录 => {
                    有记录 = true;
                    let hi = 数值(a).unwrap_or_default();
                    opts.declare = Some(jpp_value::bridge::DeclaredLine::two_sided(hi, hi, false));
                }
                other => {
                    return err(
                        Some("J-03"),
                        format!(
                            "cut 的校准参数是校准记录的键（Text）、策略记录 {{declare | cost | alpha | stat}} 或一个数（读作声明线 {{declare: {{hi: 数}}}}）；收到 {}",
                            other.type_name()
                        ),
                        sp,
                    );
                }
            }
        }
        // `select`/`measure` 只有单侧线（B63 同形）：声明线给了 lo 即错
        let 读数们: Vec<&Rc<Reading>> = match &args[0] {
            Value::Reading(r) => vec![r],
            Value::List(l) => l
                .iter()
                .filter_map(|x| match x {
                    Value::Reading(r) => Some(r),
                    _ => None,
                })
                .collect(),
            _ => vec![],
        };
        for r in &读数们 {
            if let Err(m) = 核选项(r, &opts) {
                return 选项错(m);
            }
        }
        // 声明式拟合的结果（B153 (2)，步 20j-4）：只走声明分支
        let 有拟合 = match &args[0] {
            Value::Score(_) => true,
            Value::List(l) => l.iter().any(|x| matches!(x, Value::Score(_))),
            _ => false,
        };
        if 有拟合 && let Err(m) = 核拟合选项(&opts) {
            return 选项错(m);
        }
        // J-05 默认链再判用同一条线（B0492 S2）
        self.记切法来历(&args[0], calib.as_deref(), &opts);
        let v = match &args[0] {
            Value::Reading(r) => self.过桥(r, calib.as_deref(), opts, sp),
            Value::Score(s) => self.cut_score(s, opts, sp),
            Value::List(l) => {
                let mut out = vec![];
                for r in l.iter() {
                    match r {
                        Value::Reading(r) => {
                            out.push(self.过桥(r, calib.as_deref(), opts.clone(), sp)?)
                        }
                        Value::Score(s) => out.push(self.cut_score(s, opts.clone(), sp)?),
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
        }?;
        // 「无作者去向」站点（B0492 S2c）：切出未决当场走默认链
        self.站点走链(v, sp)
    }

    #[allow(unused_variables)]
    pub(crate) fn b_handle(&mut self, name: &'static str, args: Vec<Value>, sp: Span) -> R<Value> {
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
        // 未决值传播（B0492 S3）：交给 handle 照常，按未决出口走作者的臂
        let (Value::Exit(e) | Value::Duty(e)) = &args[0] else {
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
    pub(crate) fn b_consume(&mut self, name: &'static str, args: Vec<Value>, sp: Span) -> R<Value> {
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
            return err(
                Some("E-rt-arg"),
                "consume(exit | [exits], \"drop\" | \"branch\")",
                sp,
            );
        };
        // "branch"（issue #56）：程序没有丢掉这次未决，而是把候选都留下、各自跟进（J-05 的细化去向）。
        // 与 drop 一样算消费、一样按账本键解除，但不进丢弃表：不报 W-drop-vs-escalate，跟进的项
        // 出现在返回值里也不报 W-drop-then-return
        let branch = match how.as_ref() {
            "drop" => false,
            "branch" => true,
            _ => {
                return err(
                    Some("E-rt-arg"),
                    "consume 只支持 \"drop\"（显式丢弃）与 \"branch\"（留下候选各自跟进）；升级用 ask",
                    sp,
                );
            }
        };
        // B95（步 21）：契约值不能整份 drop——一行丢掉整份未决清单（含预算未观察项）比转交还短，
        // 正是非设计者程序绕过 J-05 的写法。依据：B95、`12` §3 J-05 注
        if is_outcome(&args[0]) {
            return err(
                Some("J-05"),
                "consume 不收契约值：整份丢掉会把它的未决清单（含预算停机没观察到的项）一起从输出里抹掉。修法：转交——把 undecided(o) 与 unobserved(o)（或 o.pending）放进返回值，元素投影保留 exit 字段；判过而拿不准的项（原因不是 budget、absent、latency）确实不进入任何输出、不参与路由，才逐项丢：map(filter(undecided(o), fn(p) { p.cause != \"absent\" && p.cause != \"latency\" }), fn(p) { consume(p.exit, \"drop\") })，其余转交",
                sp,
            );
        }
        let list: Vec<Value> = match &args[0] {
            Value::List(l) => l.iter().cloned().collect(),
            v => vec![v.clone()],
        };
        for v in &list {
            match v {
                Value::Exit(e) | Value::Duty(e) => {
                    // B95：缺席类原因不是「判过而拿不准」，是没观察到；丢掉等于把预算停机或判断器缺席
                    // 从输出里抹掉。去向只剩转交或 escalate。依据：B95、B93（缺席原因集合）
                    let c = e.cause();
                    if e.is_unsure()
                        && (!e.consumed.get() || crate::duty::已被吸收(e))
                        && e.why().is_some_and(|w| w.cause.is_absent_class())
                    {
                        // 依据：B95；被合成吸收过的分量同样核（B162）
                        return err(
                            Some("E-drop-unobserved"),
                            format!(
                                "{} 是没观察到的项（原因 {c}），不能 drop：它不是判过而拿不准，丢掉就把{}从输出里抹掉了。修法：转交——放进返回值（契约值的写 unobserved(o) 或 o.pending，元素投影保留 exit 字段），或 escalate(u, …) 交给人",
                                e.label(),
                                match c.as_str() {
                                    "budget" => "预算停机",
                                    // G4：跨程序触发链到限停发
                                    "depth" => "深度到限停发",
                                    _ => "判断器缺席",
                                }
                            ),
                            sp,
                        );
                    }
                }
                _ => return err(Some("E-rt-arg"), "consume 只收出口或未决责任", sp),
            }
        }
        for v in &list {
            if let Value::Exit(e) | Value::Duty(e) = v {
                // 被 compose、tally 吸收过的分量仍可按边处理：责任在合成出口里，这里按账本键解除（B162）
                if e.is_unsure() && (!e.consumed.get() || crate::duty::已被吸收(e)) {
                    if branch {
                        // C-1：留下候选各自跟进是细化，账本记 Refine{how: branch}
                        self.记去向(
                            &e.clone(),
                            crate::duty::去向::Refine {
                                how: "branch",
                                to: None,
                            },
                            "consume(…, \"branch\")",
                            sp,
                        );
                    } else {
                        // drop 是合法去向（12 §6「unsure 显式丢弃并记账」），但要留痕
                        self.trace.warn(format!(
                            "W-drop-vs-escalate: 显式丢弃了未决责任 {}（题 {}）；drop 合法且已记账，但只有 escalate 会把它交给人",
                            e.label(),
                            头(&e.q_hash, 8)
                        ));
                        self.dropped.push(e.clone());
                        // C-1：显式丢弃，账本记 Drop
                        self.记去向(
                            &e.clone(),
                            crate::duty::去向::Drop,
                            "consume(…, \"drop\")",
                            sp,
                        );
                    }
                }
                e.consumed.set(true);
                *e.consumed_by.borrow_mut() = if branch {
                    "consume:branch"
                } else {
                    "consume:drop"
                }
                .into();
            }
        }
        Ok(Value::Unit)
    }
}
