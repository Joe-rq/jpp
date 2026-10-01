//! 同题重复读数 `repeat`（旧名 `agg`）与判断向量 `order`（B28）。
//! 步 4a 从 `interp.rs` 机械拆出，只搬代码、不改逻辑（21 §三·3）。

use crate::*;

impl<'a> Interp<'a> {
    #[allow(unused_variables)]
    pub(crate) fn b_agg(
        &mut self,
        caps: &Caps,
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
        if n == 0 || n > 2 {
            return err(
                Some("E-rt-arg"),
                format!("{name}(读数列表[, \"mean\" | \"median\"])"),
                sp,
            );
        }
        if name == "agg" {
            self.trace.warn(format!("W-deprecated: @{} agg 已改名 repeat（B28），语义改为只取均值 / 中位数、禁众数；下一版移除 agg", sp.start));
        }
        let method = match args.get(1) {
            None => "mean".to_string(),
            Some(Value::Text(t, _)) if t.as_ref() == "mean" || t.as_ref() == "median" => {
                t.to_string()
            }
            Some(Value::Text(t, _)) if t.as_ref() == "mode" => {
                return err(
                    Some("B28"),
                    "repeat 不许取众数：对出口或选项投票就是多数表决，错误持久时它不降错（B9 / B28）。修法：用 mean 或 median 压抖动",
                    sp,
                );
            }
            Some(other) => {
                return err(
                    Some("E-rt-arg"),
                    format!(
                        "repeat 的方式只收 \"mean\" 或 \"median\"，收到 {}",
                        other.type_name()
                    ),
                    sp,
                );
            }
        };
        self.flush("repeat")?;
        let rs = self.readings_of(&args[0], "repeat", sp)?;
        if rs.is_empty() {
            return err(Some("E-rt-arg"), "repeat 要至少一条读数", sp);
        }
        // 合并后**仍是读数**——所以还能 cut。拿 fold 求平均得到的是裸数，进不了 cut、也不带校准键。
        let first = &rs[0];
        if rs.iter().any(|r| r.q_hash != first.q_hash) {
            return err(
                Some("J-01"),
                "repeat 只合并**同一道题**跨运行的读数：收到的读数不是同一道题",
                sp,
            );
        }
        let merged = merge_runs(&*caps.read_answer().answers(self), &rs, &method, sp)?;
        let n_runs = rs.len();
        let 键 = format!("{}\u{1f}repeat(n={n_runs})", first.calib);
        let lk = format!("repeat(n={n_runs},{method}):{}", first.ledger_key);
        if !caps.ledger_write().has_key(self, &lk) {
            caps.ledger_write().ledger_put(self, Entry::effect_keyed(lk.clone(), "repeat", serde_json::json!({"n": n_runs, "method": method, "calib": 键.replace('\u{1f}', ":")}), 0.0));
        }
        caps.ledger_write().trace_event(
            self,
            "repeat",
            &lk,
            false,
            0.0,
            sp,
            format!("n={n_runs} {method}"),
        );
        if !self
            .calib
            .line(&first.calib)
            .rerun_independent
            .unwrap_or(false)
        {
            self.trace.warn(format!(
                    "W-repeat-persistent: @{} 键 {} 未通过重跑分歧检验：重复读数只压抖动、不降错（错误持久，B9）。合并结果用独立键 {}，没有它的认证记录就是冷",
                    sp.start, first.calib, 键.replace('\u{1f}', ":")
                ));
        }
        let r = Rc::new(Reading {
            q_hash: first.q_hash.clone(),
            state_hash: first.state_hash.clone(),
            op: first.op,
            calib: 键,
            id: caps.issue_reading().new_reading_id(self),
            fail: None,
            model_id: first.model_id.clone(),
            ledger_key: lk,
            over_len: first.over_len,
            scale: first.scale.clone(),
            perms: std::cell::Cell::new(first.perms.get()),
            mode_share: std::cell::Cell::new(first.mode_share.get()),
            missing_evidence: first.missing_evidence.clone(),
            state_taint: rs
                .iter()
                .fold(Taint::Trusted, |t, r| Taint::join(t, r.state_taint)),
            // 合并结果不借题式线：它有自己的含 n 的键（B28）
            form_hash: None,
            fp: None,
        });
        caps.issue_reading().fill_answer(self, &r, merged);
        Ok(Value::Reading(r))
    }
    #[allow(unused_variables)]
    pub(crate) fn b_order(
        &mut self,
        caps: &Caps,
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
        if n != 2 {
            arity(1)?;
        }
        // 步 15k（B167 (1)）：第二个参数 `{stat?, tie?}`，`stat` 与 `cut` 同一枚举、同一解析
        let 选项错 = |msg: String| -> R<Value> { err(Some("E-order-options"), msg, sp) };
        let mut stat: Option<jpp_value::stat::Stat> = None;
        let mut tie: Option<f64> = None;
        if let Some(opt) = args.get(1) {
            // 依据：B167 (1)、(3)（地基/附注/2026-09-26-批6裁定.md §十五）
            let Value::Record(fields) = opt else {
                return 选项错(format!(
                    "order 的第二个参数要是记录 {{stat?, tie?}}，收到 {}（B167）",
                    opt.type_name()
                ));
            };
            for (k, v) in fields.iter() {
                match k.as_str() {
                    "stat" => match crate::host_builtins::解析统计量(v) {
                        Ok(s) => stat = Some(s),
                        Err(m) => return 选项错(format!("order 的 {m}")),
                    },
                    "tie" => match v {
                        Value::Int(i, _) if *i >= 0 => tie = Some(*i as f64),
                        Value::Float(f, _) if *f >= f64::default() => tie = Some(*f),
                        _ => {
                            return 选项错(
                                "order 的 tie 要是非负数：期望档位相差不超过它就并成一档（B167 (3)）".into(),
                            );
                        }
                    },
                    other => {
                        return 选项错(format!(
                            "order 的选项只认 stat、tie：{{stat: \"expect\", tie: 0.1}}；收到字段 {other}（B167）"
                        ));
                    }
                }
            }
            if tie.is_some() && stat != Some(jpp_value::stat::Stat::Expect) {
                return 选项错(
                    "order 的 tie 只配 stat: \"expect\"：概率型统计量按画像 δ 并档，argmax 按档位相等并档（B167 (3)）".into(),
                );
            }
        }
        self.flush("order")?;
        // 同一声明式拟合的 Score（B153 (2)，步 20j-4）：按数分档，并档带宽取 fit 的 tie；不收 stat / tie（B167 (4)）
        if let Value::List(l) = &args[0]
            && l.iter().any(|x| matches!(x, Value::Score(_)))
        {
            if args.len() == 2 {
                // 依据：B167 (4)（同拟合 Score 序已是标量）
                return 选项错(
                    "order 对声明式拟合的 Score 按数分档，不收 stat / tie：并档带宽写在 fit({declare, tie}) 上（B153、B167 (4)）".into(),
                );
            }
            return self.order_scores(caps, l, sp);
        }
        // 步 15k（B166）：一条 `select` 读数（不是列表）→ 候选下标按概率分档；不产生出口
        if let Value::Reading(r) = &args[0]
            && r.op == Op::Select
        {
            // 依据：B166 (1)、B167 (4)（候选序是单元，不收 stat）
            if args.len() == 2 {
                return 选项错(
                    "order 对一条 select 读数按候选概率分档，不收 stat / tie：候选序就是单元（B166、B167 (4)）".into(),
                );
            }
            if r.mode_share.get().is_none()
                && self
                    .unknown_reported
                    .insert(format!("order-unpermuted@{}", sp.start))
            {
                // 依据：B166 (2)、B64（位置偏差；只告警不阻止）
                self.trace.warn(format!(
                    "W-order-unpermuted: @{} order 按一条 select 读数的候选概率分档，这条读数没有置换测量（没声明 {{permute: true}}，或候选是 {{label, text}} 记录）：候选的位置可能改变分档（B64、B166）",
                    sp.start
                ));
            }
            let tol = self.并档容差(std::slice::from_ref(r), sp)?;
            return Ok(Self::下标分档(self.candidate_tiers(
                caps.read_answer(),
                r,
                tol,
            )));
        }
        let rs = self.readings_of(&args[0], "order", sp)?;
        // J-04（12:255）：跨题、跨候选集、跨刻度或异锚的读数**不可比**。
        // order 是排序，排序就是比——两道题各有各的校准线，p 不在同一把尺子上，
        // 「问题一 0.9 高于问题二 0.5」这个比较本身不成立。
        if let Some(first) = rs.first() {
            for r in &rs {
                if r.q_hash != first.q_hash {
                    return err(
                        Some("J-04"),
                        "order 只排**同一道题**跨对象的读数：跨题的读数不可比（各有各的校准线，p 不在同一把尺子上）",
                        sp,
                    );
                }
                if r.over_len != first.over_len || r.scale != first.scale {
                    return err(
                        Some("J-04"),
                        "order 的读数候选集或刻度不同：指纹不同即不可比",
                        sp,
                    );
                }
            }
        }
        // 缺省统计量（B167 (2)）：`measure` 按档位，`test`、`select` 按 p / p_max（现状）
        let stat = stat.unwrap_or(match rs.first().map(|r| r.op) {
            Some(Op::Measure) => jpp_value::stat::Stat::Argmax,
            _ => jpp_value::stat::Stat::Max,
        });
        // 并档容差（B167 (3)；Z0425）：`argmax` 按档位相等、`expect` 按作者的 `tie`，概率型取记录 δ → 画像中段 → 不并档
        let tol = match stat {
            jpp_value::stat::Stat::Argmax => 0.0,
            jpp_value::stat::Stat::Expect => tie.unwrap_or_default(),
            _ => self.并档容差(&rs, sp)?,
        };
        match self.order_tiers(caps.read_answer(), &rs, &stat, tol) {
            Ok(t) => Ok(Self::下标分档(t)),
            // 依据：B167 (1)（统计量与题型不配）
            Err(jpp_value::stat::StatError::Options(m)) => err(
                Some("E-order-options"),
                format!("order 的 stat: {}：{m}（B167）", stat.to_json()),
                sp,
            ),
            // 依据：B154 (3)（取不到 confidence 不退回 p_max）
            Err(jpp_value::stat::StatError::Unavailable(m)) => err(
                Some("E-stat-unavailable"),
                format!(
                    "@{} order 的 stat: \"confidence\" 取不到数：{m}。修法：换一个随答案报 confidence 的判断器（画像 H9 reports_confidence: true），或在夹具观察里给 confidence（B154）",
                    sp.start
                ),
                sp,
            ),
        }
    }

    fn 下标分档(tiers: Vec<Vec<usize>>) -> Value {
        Value::list(
            tiers
                .into_iter()
                .map(|tier| {
                    Value::list(
                        tier.into_iter()
                            .map(|i| Value::Int(i as i64, Taint::Trusted.into()))
                            .collect(),
                    )
                })
                .collect(),
        )
    }

    /// `order([Score…])`（B153 (2)）：全部同一 `fit_hash`、同一 `tie`（作者声明的同一把尺子）时，按数降序分档，
    /// 相邻差 ≤ `tie` 并档（`tie` 不取画像 δ）；输入不可用的排最后一档。尺子不同或与读数混排报 J-04。
    /// 依据：B153 (2)（地基/附注/2026-09-26-批6裁定.md §一）；J-04 补条（同尺）
    fn order_scores(&mut self, caps: &Caps, l: &[Value], sp: Span) -> R<Value> {
        let mut ss: Vec<&Rc<Score>> = vec![];
        for x in l {
            match x {
                Value::Score(s) => ss.push(s),
                other => {
                    // 依据：B153 (2)；J-04 补条（同尺）
                    return err(
                        Some("J-04"),
                        format!(
                            "order 里声明式拟合的结果与 {} 混排：只有同一拟合的 Score 在同一把尺子上（B153）",
                            other.type_name()
                        ),
                        sp,
                    );
                }
            }
        }
        let first = ss[0];
        for s in &ss {
            if s.fit_hash != first.fit_hash || s.tie != first.tie {
                // 依据：B153 (2)（同 fit_hash 即作者声明的同一把尺子）；J-04
                return err(
                    Some("J-04"),
                    "order 只排同一声明式拟合（同一闭包、同一组输入题、同一 tie）的 Score：不同拟合是不同的尺子，作者没有声明它们可比（B153）",
                    sp,
                );
            }
        }
        let mut ok: Vec<(usize, f64)> = vec![];
        let mut failed: Vec<usize> = vec![];
        for (i, s) in ss.iter().enumerate() {
            match caps.read_answer().score_of(self, s) {
                Some(v) => ok.push((i, v)),
                None => failed.push(i),
            }
        }
        ok.sort_by(|a, b| {
            b.1.partial_cmp(&a.1)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then(a.0.cmp(&b.0))
        });
        let mut tiers: Vec<Vec<usize>> = vec![];
        let mut 上一个: Option<f64> = None;
        for (i, v) in ok {
            match (tiers.last_mut(), 上一个) {
                (Some(last), Some(p))
                    if (p - v).abs() <= first.tie + jpp_value::stat::BOUNDARY_EPS =>
                {
                    last.push(i)
                }
                _ => tiers.push(vec![i]),
            }
            上一个 = Some(v);
        }
        if !failed.is_empty() {
            tiers.push(failed);
        }
        Ok(Value::list(
            tiers
                .into_iter()
                .map(|t| {
                    Value::list(
                        t.into_iter()
                            .map(|i| Value::Int(i as i64, Taint::Trusted.into()))
                            .collect(),
                    )
                })
                .collect(),
        ))
    }
}
