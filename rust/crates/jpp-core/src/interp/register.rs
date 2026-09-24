//! 效应登记：`judge` 登记、提前登记（lift、speculate、vectorize）、窗口检查（20 §2.3 `register.rs`）。
//! 步 4a 从 `interp.rs` 机械拆出，只搬代码、不改逻辑（21 §三·3）。

use super::plan_view::RtEnv;
use super::*;
use jpp_ir::plan::Reach;

impl<'a> Interp<'a> {
    /// 判断键：算出账本键并记下结构化键，写账本时附上（账本 v2，步 7）。键值与 `judge_key` 相同。
    pub(in crate::interp) fn judge_key_of(
        &mut self,
        state_hash: &str,
        q_hash: &str,
        phys: &str,
        site: usize,
    ) -> String {
        let k = JudgeKey::new(
            &self.model_id,
            state_hash,
            q_hash,
            phys,
            0,
            self.run_seq,
            site,
        );
        let d = k.digest();
        self.judge_keys.insert(d.clone(), k);
        d
    }

    /// 效应键：算出账本键并记下结构化键（账本 v2，步 7）。键值与 `effect_key` 相同。
    pub(in crate::interp) fn effect_key_of(&mut self, kind: &str, parts: &[&str]) -> String {
        let k = EffectKey::new(kind, parts);
        let d = k.digest();
        self.effect_keys.insert(d.clone(), k);
        d
    }

    pub(in crate::interp) fn judge(
        &mut self,
        state: &Rc<State>,
        qs: &[Rc<Question>],
        sp: Span,
    ) -> R<Vec<Value>> {
        for q in qs {
            if state.derived_from.contains(&q.hash) {
                return err(
                    Some("J-02"),
                    format!("禁自指：状态含由题「{}」派生的材料，不能再问同一题", q.text),
                    sp,
                );
            }
        }
        if state.has_fail {
            let rs: Vec<Rc<Reading>> = qs
                .iter()
                .map(|q| {
                    Rc::new(Reading {
                        q_hash: q.hash.clone(),
                        state_hash: state.hash.clone(),
                        op: q.op,
                        calib: q.calib.clone(),
                        id: self.new_reading_id(),
                        fail: Some("状态含 Fail 材料".into()),
                        model_id: self.model_id.clone(),
                        ledger_key: String::new(),
                        over_len: state.over.len(),
                        scale: q.scale.clone(),
                        perms: std::cell::Cell::new(0),
                        mode_share: std::cell::Cell::new(None),
                        missing_evidence: missing_evidence(state, q),
                        state_taint: state.taint,
                        form_hash: q.form_hash.clone(),
                        fp: Some(jpp_value::stat::material_fingerprint(&state.on_text())),
                    })
                })
                .collect();
            self.note_kinds(state, qs, &rs);
            return Ok(rs.into_iter().map(Value::Reading).collect());
        }
        let keys: Vec<String> = qs
            .iter()
            .map(|q| self.judge_key_of(&state.hash, &q.hash, q.op.phys(), sp.start))
            .collect();
        // 循环内键重复即停（J-06）
        if let Some(lc) = self.loops.last_mut() {
            for k in &keys {
                if !lc.seen_keys.insert(k.clone()) && lc.repeated.is_none() {
                    lc.repeated = Some(k.clone());
                }
            }
        }
        // 12 §2.2「**惰性**：登记后不发」。账本命中的当场填上（重放不花钱、也不必推迟）；
        // 缺的登记进 `pending`，等一个**刷新点**（`cut` / `if` / 程序结束）按状态分组一层发出。
        let readings: Vec<Rc<Reading>> = qs
            .iter()
            .zip(&keys)
            .map(|(q, k)| {
                Rc::new(Reading {
                    q_hash: q.hash.clone(),
                    state_hash: state.hash.clone(),
                    op: q.op,
                    calib: q.calib.clone(),
                    id: self.new_reading_id(),
                    fail: None,
                    model_id: self.model_id.clone(),
                    ledger_key: k.clone(),
                    over_len: state.over.len(),
                    scale: q.scale.clone(),
                    perms: std::cell::Cell::new(0),
                    mode_share: std::cell::Cell::new(None),
                    missing_evidence: missing_evidence(state, q),
                    state_taint: state.taint,
                    form_hash: q.form_hash.clone(),
                    fp: Some(jpp_value::stat::material_fingerprint(&state.on_text())),
                })
            })
            .collect();
        self.note_kinds(state, qs, &readings);
        let mut missing = vec![];
        for (i, k) in keys.iter().enumerate() {
            // 真站点走到了一个推测过的键：这次推测用上了
            if self.speculated.contains(k) {
                self.speculation_used.insert(k.clone());
            }
            if let Some(Entry::Judge {
                answer,
                cost,
                call,
                perm,
                ..
            }) = self.ledger.get(k)
            {
                let (answer, cost, call, perm) = (answer.clone(), *cost, *call, *perm);
                // 账本命中当场填：写答案只经 flush.rs 的 fill_answer（grep_fill 核）；
                // 置换测量随答案一起取回，否则 K 选一出口在重放处变成 `untested`
                self.fill_from_record(&readings[i], answer, perm);
                self.audit_account(call, cost, sp);
                self.cost.replayed += 1;
                self.trace
                    .push("judge", k, true, 0.0, sp, format!("「{}」", qs[i].text));
            } else if qs[i].op == Op::Select && state.over.is_empty() {
                // **没有候选**（B3）：K 选一的 over 槽是空的，问了也没有可选的——不发，出口 Unsure(no_candidate)，
                // 去向是调生成器补候选，不同于 tie / insufficient。
                self.absent_marks.insert(k.clone(), "no_candidate".into());
                self.trace.warn(format!("W-no-candidate: @{} 选择题「{}」没有候选（over 为空），出口 Unsure(no_candidate)", sp.start, qs[i].text));
            } else {
                missing.push(i);
            }
        }
        // 窗口检查（J-14 / 12:117）：对象槽内单段按 text_slots，槽间按 json_slots。
        // 超窗不报错只留痕——它不是算错，是**读数被语境接管而无人察觉**
        // （档案：「≈1,000 token 带主张语境下翻转 60.7%，读数被语境接管」）。
        // 与 Python `_check_window` 同为 warn。
        self.check_window(state, sp);
        if !missing.is_empty() {
            self.pending.push(PendingJudge {
                state: state.clone(),
                items: missing
                    .iter()
                    .map(|i| (qs[*i].clone(), readings[*i].clone(), keys[*i].clone()))
                    .collect(),
                site: sp,
                speculative: false,
            });
        }
        Ok(readings.into_iter().map(Value::Reading).collect())
    }

    /// 记下每个读数的精化题类（B76，步 12e-2）：题 × 状态槽形。
    pub(in crate::interp) fn note_kinds(
        &mut self,
        state: &State,
        qs: &[Rc<Question>],
        readings: &[Rc<Reading>],
    ) {
        let shape = state.slot_shape();
        for (q, r) in qs.iter().zip(readings) {
            self.reading_kinds.insert(r.id, q.kind_on(&shape));
        }
    }

    /// 读数的精化题类（B76）；合成读数（`fit`、`repeat`）没有记录，为 `None`。
    #[allow(dead_code)]
    pub(in crate::interp) fn reading_kind(&self, r: &Reading) -> Option<QuestionKind> {
        self.reading_kinds.get(&r.id).copied()
    }

    /// 提升（pass `lift`）的执行一半：把后面同状态、可安全提前登记的 `judge` 一起登记上来。
    ///
    /// 哪些语句可提、停在哪里由 `jpp-plan` 判（步 13a 从这里搬走，`jpp_plan::passes::lift`）：
    /// 计划给出逐句的提升步，按环境才判得出的两问（越过的这一句会不会触世界，K-075；被提的这一句
    /// 能不能提前求值，K-069）逐句问钩子，因为前面被提的句子会改变环境。这里只求值与绑定。
    pub(in crate::interp) fn lift_followers(
        &mut self,
        b: &Block,
        at: jpp_ir::key::NodeId,
        env: &Env,
        lifted: &mut HashSet<usize>,
    ) -> R<()> {
        let Some(lp) = self.plan.lifts.get(&at).cloned() else {
            return Ok(());
        };
        for step in &lp.steps {
            let Stmt::Let { name, value, .. } = &b.statements[step.index] else {
                break;
            };
            let view = RtEnv(env.clone());
            if self.hooks.may_effect(value, &view, Reach::World) {
                break;
            }
            if step.lift {
                if self.hooks.may_effect(value, &view, Reach::Strict) {
                    break;
                }
                let v = self.eval(value, env)?;
                env_define(env, name, v);
                lifted.insert(step.index);
            }
            if step.stop_after {
                break;
            }
        }
        Ok(())
    }

    /// 推测（pass `speculate`）的执行一半：从触发点 `at`（本块一条 `let` 的值表达式）起，
    /// 把 `if` 两侧分支体里此刻已能求值的站点推测登记。候选与许可由 `jpp-plan` 给（计划 + 钩子）。
    pub(in crate::interp) fn speculate_ahead(
        &mut self,
        b: &Block,
        at: jpp_ir::key::NodeId,
        env: &Env,
    ) {
        let view = RtEnv(env.clone());
        let judges = self.hooks.speculate(&self.plan, at, b, &view);
        for j in judges {
            self.speculate_judge(j, env);
        }
    }

    /// **循环向量化**（宪法登记表第 47 行，pass `vectorize`）的执行一半：把 `map`/`filter`
    /// **后续各轮**的 `judge` 站点提前登记进本层，这样体内有 `cut` 时不会一轮一层。
    ///
    /// **实测它只在一个形状上有余量**（四个形状各跑一遍）：
    /// | 形状 | 接之前 | 可省 |
    /// |---|---|---|
    /// | 异状态·无 `cut` | calls 3 / layers 1 | **无**——惰性已经把三轮并进一层 |
    /// | 同状态·无 `cut` | calls 1 / layers 1 | **无**——`fuse` 已经合成一次调用 |
    /// | **异状态·带 `cut`** | **calls 3 / layers 3** | **layers 3 → 1** |
    /// | 同状态·带 `cut` | calls 1 / layers 1 | **无**——后两轮同键，账本直接重放 |
    ///
    /// 每一轮：绑好形参，经钩子 `instantiate` 取这一轮可提前登记的站点（候选按函数体静态给出，
    /// 许可按这一轮的环境判，`jpp_plan::passes::vectorize`），再逐个求值登记。
    pub(in crate::interp) fn vectorize_ahead(&mut self, f: &Value, items: &[Value], _sp: Span) {
        if !self.plan.vectorize {
            return;
        }
        let Value::Fn(c) = f else { return };
        if c.function.parameters.len() != 1 {
            return;
        }
        // 第 0 轮马上就要真跑，不用提前登记；提前的是其余各轮
        for it in items.iter().skip(1) {
            let env = env_child(&c.env);
            env_define(&env, &c.function.parameters[0].name, it.clone());
            let view = RtEnv(env.clone());
            let targets = self.hooks.instantiate(&self.plan, &c.function, &view);
            self.run_targets(&c.function, &targets, &env);
        }
    }

    /// 执行钩子给的目标（步 13b）：`Site` 在当前环境里求状态与题并登记；`Enter` 求被调者与实参
    /// （钩子已核：实参只有名字或字面量、按环境不会产生效应），在被调者的捕获环境上绑好形参再往里执行。
    /// 这里只求值与登记，不判许可（`20` T3）；求不出来就放弃这一支，不报错（与推测同一口径）。
    fn run_targets(&mut self, f: &Function, targets: &[jpp_ir::plan::Target], env: &Env) {
        use jpp_ir::plan::Target;
        for t in targets {
            match t {
                Target::Site(id) => {
                    if let Some(e) = jpp_ir::ir::find_expr(&f.body, *id) {
                        self.speculate_judge(e, env);
                    }
                }
                Target::Enter { call, inner } => {
                    let Some(e) = jpp_ir::ir::find_expr(&f.body, *call) else {
                        continue;
                    };
                    let K::Call { callee, args } = kind(e) else {
                        continue;
                    };
                    let Some(Value::Fn(c2)) = callee.name().and_then(|n| env_lookup(env, n)) else {
                        continue;
                    };
                    let mut vals = vec![];
                    for a in &args {
                        match self.eval(a, env) {
                            Ok(v) => vals.push(v),
                            Err(_) => break,
                        }
                    }
                    if vals.len() != c2.function.parameters.len() {
                        continue;
                    }
                    let env2 = env_child(&c2.env);
                    for (p, v) in c2.function.parameters.iter().zip(vals) {
                        env_define(&env2, &p.name, v);
                    }
                    let c2 = c2.clone();
                    self.run_targets(&c2.function, inner, &env2);
                }
            }
        }
    }

    /// 把一个许可过的 `judge` 站点提前登记：在当前环境里求出状态与题，求不出就放弃这个站点（不报错）。
    ///
    /// **搬过去的站点，来源还是原来那份**：状态由 `make_state` 在**当前环境**里算出来，
    /// taint 与 `derived_from` 都跟着材料走——推测不新建材料，所以没有新边界。
    /// **这里搬的是站点不是值，值仍在原环境里算**。
    pub(in crate::interp) fn speculate_judge(&mut self, e: &Expr, env: &Env) {
        let K::Call {
            args: arguments, ..
        } = kind(e)
        else {
            return;
        };
        if let (Ok(Value::State(st)), Ok(q)) =
            (self.eval(arguments[0], env), self.eval(arguments[1], env))
        {
            let qs: Vec<Rc<Question>> = match q {
                Value::Question(q) => vec![q],
                Value::List(l) => l
                    .iter()
                    .filter_map(|x| {
                        if let Value::Question(q) = x {
                            Some(q.clone())
                        } else {
                            None
                        }
                    })
                    .collect(),
                _ => return,
            };
            if !qs.is_empty() {
                let _ = self.register_speculative(&st, &qs, e.span);
            }
        }
    }

    /// 登记一个推测站点：与真站点同一套键（`judge_key`），所以真站点走到时直接命中账本。
    pub(in crate::interp) fn register_speculative(
        &mut self,
        state: &Rc<State>,
        qs: &[Rc<Question>],
        sp: Span,
    ) -> Option<()> {
        if state.has_fail {
            return None;
        }
        for q in qs {
            if state.derived_from.contains(&q.hash) {
                return None; // 禁自指的站点不推
            }
        }
        let keys: Vec<String> = qs
            .iter()
            .map(|q| self.judge_key_of(&state.hash, &q.hash, q.op.phys(), sp.start))
            .collect();
        let mut items = vec![];
        for (q, k) in qs.iter().zip(&keys) {
            // 账本里已经有 = 不用推
            if self.ledger.get(k).is_some() {
                continue;
            }
            // 这一层已经登记过同一个键 = 不重复推
            if self
                .pending
                .iter()
                .any(|p| p.items.iter().any(|(_, _, kk)| kk == k))
            {
                continue;
            }
            let r = Rc::new(Reading {
                q_hash: q.hash.clone(),
                state_hash: state.hash.clone(),
                op: q.op,
                calib: q.calib.clone(),
                id: self.new_reading_id(),
                fail: None,
                model_id: self.model_id.clone(),
                ledger_key: k.clone(),
                over_len: state.over.len(),
                perms: std::cell::Cell::new(0),
                mode_share: std::cell::Cell::new(None),
                missing_evidence: missing_evidence(state, q),
                scale: q.scale.clone(),
                state_taint: state.taint,
                form_hash: q.form_hash.clone(),
                fp: Some(jpp_value::stat::material_fingerprint(&state.on_text())),
            });
            items.push((q.clone(), r, k.clone()));
        }
        if items.is_empty() {
            return None;
        }
        for (_, _, k) in &items {
            self.speculated.insert(k.clone());
        }
        {
            let (qs2, rs2): (Vec<Rc<Question>>, Vec<Rc<Reading>>) =
                items.iter().map(|(q, r, _)| (q.clone(), r.clone())).unzip();
            self.note_kinds(state, &qs2, &rs2);
        }
        self.pending.push(PendingJudge {
            state: state.clone(),
            items,
            site: sp,
            speculative: true,
        });
        Some(())
    }

    /// 窗口检查（J-14 / `12`:117）。静态判不了大小，所以在**登记时**查。
    pub(in crate::interp) fn check_window(&mut self, state: &State, sp: Span) {
        let p = self.calib.profile();
        // 对象槽内单段：每一段各自比，不是求和——依据说的是「单段材料」
        for m in &state.on {
            let t = m.tokens();
            if t > p.text_window {
                self.trace.warn(format!(
                    "W-window: @{} 对象槽内单段 {t} token 超已测窗口 {}（超窗的语境会接管读数，答案可能偏而无痕）",
                    sp.start, p.text_window
                ));
            }
        }
        // 槽间干扰：ctx 与 ref 求和
        let ctx: usize = state
            .ctx
            .iter()
            .chain(&state.r#ref)
            .map(|m| m.tokens())
            .sum();
        if ctx > p.json_ctx_window {
            self.trace.warn(format!(
                "W-window: @{} 语境槽 {ctx} token 超 JSON 槽已测窗口 {}（上限未测，超出即无依据）",
                sp.start, p.json_ctx_window
            ));
        }
    }
}
