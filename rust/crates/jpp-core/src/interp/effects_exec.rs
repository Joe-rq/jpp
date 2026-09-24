//! 效应执行：`do`、`gen`、`ask`、`transform` 的键、账本与 taint（20 §2.3 `effects_exec.rs`）。
//! 步 4a 从 `interp.rs` 机械拆出，只搬代码、不改逻辑（21 §三·3）。

use super::*;

impl<'a> Interp<'a> {
    pub(in crate::interp) fn do_(
        &mut self,
        name: &str,
        args: &[Value],
        iter_seq: i64,
        sp: Span,
    ) -> R<Value> {
        let action = self.actions.actions.get(name).cloned().ok_or_else(|| {
            Fault::Error(RtError::new(
                Some("J-11"),
                {
                    let mut 表: Vec<String> = self
                        .actions
                        .actions
                        .iter()
                        .map(|(n, a)| {
                            format!(
                                "{n}（{}）",
                                if a.reversible {
                                    "可逆"
                                } else {
                                    "**不可逆**"
                                }
                            )
                        })
                        .collect();
                    表.sort();
                    // **J-08 保护的是不可逆动作，而作者此前没有任何办法知道哪些动作不可逆。**
                    // 一个作者无法查询的安全边界，等于没有边界。这里顺手把它变成可查的。
                    format!(
                        "动作 {name} 未登记：do 只能触发登记过的动作（register）。本次登记了：{}",
                        表.join("、")
                    )
                },
                sp,
            ))
        })?;
        let args_canon: Vec<String> = args.iter().map(|a| canon(&a.to_json())).collect();
        let key = self.effect_key_of(
            "do",
            &[
                &sp.start.to_string(),
                name,
                &args_canon.join("\u{1f}"),
                &iter_seq.to_string(),
            ],
        );
        if let Some(Entry::Effect { output, cost, .. }) = self.ledger.get(&key) {
            // B59（步 17a）：来源键不进账本，重放时与首跑一样由实参重算
            let v = match json_to_effect_value(output) {
                Value::Mat(m) => {
                    Value::Mat(Rc::new((*m).clone().with_from_key(from_keys_of(args))))
                }
                other => other,
            };
            let cost = *cost;
            if self.audit.on {
                // do 只计费用、不计调用（首跑 charge(0, action.cost)，在执行前核，不留停机站点）
                self.audit.usd += cost;
            }
            self.cost.replayed += 1;
            self.trace.push("do", &key, true, 0.0, sp, name.into());
            return Ok(v);
        }
        // J-08（12:265）：放行**不可逆** do 的守卫表达式中，至少一个合取项来自 taint=trusted
        // 的状态；untrusted 项的数量不改变这一要求；或经 ask。
        //
        // 「守卫表达式」在一个有 if 的语言里就是**包着这个 do 的那些条件**——不必是 do 的一个参数。
        // `00-宪法.md:44` 说 IFC 的纪律只有这一条：不可信材料上的判断不得单独放行不可逆 do。
        //
        // 不查的：**这个 trusted 是不是真的可信**（12:649 Nature 裁定：taint_out="trusted" 是作者的
        // 显式标记，语言保证它可见可追，**不设审核方**）。所以这里问的是「守卫里有没有一个 trusted
        // 合取项」，不是「那个 trusted 配不配」。
        //
        // 无条件执行的 do 不受管：它没有守卫可查，作者直接写 do 是他自己的决定。
        if !action.reversible && !self.guards.is_empty() {
            let 放行 = self.guards.iter().any(|g| g.trusted || g.asked);
            if !放行 {
                // B33 第 8 点：守卫出口所在状态含成分不可信的计算值材料时，说明原因。
                // 近似：看当前各帧产生过的出口（守卫就是由它们折出来的）。
                let 计算值 = {
                    let set = self.computed_untrusted_states.borrow();
                    self.frames
                        .iter()
                        .flat_map(|f| f.exits.iter())
                        .any(|e| set.contains(&e.state_hash))
                };
                let 补充 = if 计算值 {
                    "。该材料由计算值构成，成分含不可信内容"
                } else {
                    ""
                };
                return err(
                    Some("J-08"),
                    format!(
                        "不可逆动作 {name} 的守卫里没有一个来自可信状态的合取项：不可信材料上的判断不得**单独**放行不可逆动作（宪法 IFC / 12 §5 J-08）。修法：在条件里再合取一个来自 trusted 状态的判断，或改走 ask 让人拍板，或把这个动作登记成可逆。注意：凭夹具线（W-fixture-line，B29）或停岗候选线（W-suspend-candidate，B25）得到的出口不算可信合取项{补充}"
                    ),
                    sp,
                );
            }
        }
        self.charge(0, action.cost, sp)?;
        if self.audit.on {
            return Err(self.replay_missing(format!("do「{name}」"), sp));
        }
        // 入参 taint 要**递归看容器**：材料嵌在记录字段或嵌套列表里时，顶层 match 看不见它，
        // 以前落进 `_ => Trusted`，于是 Inherit 的动作拿到「入参全可信」——脏材料喂进 do 出来就干净了。
        // 与 J-01 的 `unwrap_or(false)`、taint 反序列化兜底、`mat(content(脏))` 同一形状。
        let taint_in = args
            .iter()
            .fold(Taint::Trusted, |t, a| Taint::join(t, taint_of(a)));
        let taint = match action.taint_out {
            TaintOut::Trusted => Taint::Trusted,
            TaintOut::Untrusted => Taint::Untrusted,
            TaintOut::Inherit => taint_in,
        };
        let out = match (action.f)(args) {
            // `derived_from` 与 taint 一样要**折算**，不是恒清零：`do(…, [m])` 的产物
            // 当然仍派生自 m 那道题。**这是保持同一跳，不是增加一跳**（`12` J-02 范围裁定）。
            // 闭包在 `as_mat(exit)` 那里自然截断——做成传递闭包会重演「逐字传播让几乎所有
            // 输出不可用」（宪法第 44 行），材料越传越「派生自所有题」，J-02 最后拦住一切。
            Ok(v) => Value::Mat(Rc::new(
                Mat::new(
                    v.to_json(),
                    &format!("do:{name}"),
                    vec![format!("do:{key}")],
                    taint,
                    derived_of(args),
                )
                .with_from_key(from_keys_of(args)),
            )),
            Err(msg) => {
                // 失败信息同样来自外面：Fail 带动作的输出位，`text(f)` 经 ∨ 输入带出去
                let m = format!("{name}: {msg}");
                // B84：失败值的来源 = 实参的来源（与输出材料同一条边）
                Value::Fail(
                    Rc::from(m.as_str()),
                    Provenance::new(taint, Sources::from_set(from_keys_of(args))),
                )
            }
        };
        self.cost.usd += action.cost;
        self.ledger.put(Entry::Effect {
            key: key.clone(),
            ekey: self.effect_keys.get(&key).cloned(),
            output_mat: None,
            kind: "do".into(),
            output: effect_value_to_json(&out),
            cost: action.cost,
        });
        self.trace
            .push("do", &key, false, action.cost, sp, name.into());
        Ok(out)
    }

    pub(in crate::interp) fn generate(
        &mut self,
        prompt: &str,
        ctx: &[Mat],
        n: usize,
        retry_seq: i64,
        sp: Span,
    ) -> R<Value> {
        let ctx_hash: Vec<&str> = ctx.iter().map(|m| m.hash.as_str()).collect();
        // 12:158「键：(site, prompt_hash, ctx_hash, n, retry_seq)」——site 排第一位。
        // 缺了它，同一段 prompt 在两个站点生成会撞键，第二个站点命中第一个的输出。
        // gen 比 judge 更容易撞：prompt 常是字面量，两处写同一句话很正常。
        let key = self.effect_key_of(
            "gen",
            &[
                &sp.start.to_string(),
                prompt,
                &ctx_hash.join(","),
                &n.to_string(),
                &retry_seq.to_string(),
            ],
        );
        let taint = ctx
            .iter()
            .fold(Taint::Trusted, |t, m| Taint::join(t, m.taint));
        // 与 do 同：并入 ctx 各材料的 derived_from（保一跳）
        let derived: BTreeSet<String> = ctx
            .iter()
            .flat_map(|m| m.derived_from.iter().cloned())
            .collect();
        // B59（步 17a）：来源出口键同样承接 ctx
        let from: BTreeSet<String> = ctx
            .iter()
            .flat_map(|m| m.from_key.iter().cloned())
            .collect();
        let wrap = |outs: &[Json]| {
            Value::list(
                outs.iter()
                    .map(|o| {
                        Value::Mat(Rc::new(
                            Mat::new(
                                o.clone(),
                                &format!("gen:{prompt}"),
                                vec![format!("gen:{key}")],
                                taint,
                                derived.clone(),
                            )
                            .with_from_key(from.clone()),
                        ))
                    })
                    .collect(),
            )
        };
        if let Some(Entry::Effect { output, cost, .. }) = self.ledger.get(&key) {
            let outs: Vec<Json> = output.as_array().cloned().unwrap_or_default();
            let cost = *cost;
            self.audit_account(0, cost, sp);
            self.cost.replayed += 1;
            self.trace.push("gen", &key, true, 0.0, sp, prompt.into());
            return Ok(wrap(&outs));
        }
        self.charge(1, 0.0, sp)?;
        if self.audit.on {
            return Err(self.replay_missing(format!("gen「{prompt}」"), sp));
        }
        let ctx_json: Vec<Json> = ctx.iter().map(|m| m.content.clone()).collect();
        let res = self
            .client
            .generate(prompt, &ctx_json, n, retry_seq as u64)
            .map_err(|e| {
                Fault::Error(RtError::new(
                    Some("E-rt-client"),
                    format!("gen 失败：{}", e.0),
                    sp,
                ))
            })?;
        // 13 §5：后端已经返回 = 钱已经花了。先记事实（费用、token、账本），再核预算决定下一步。
        self.cost.calls += 1;
        self.cost.tokens += res.tokens;
        self.cost.usd += res.cost;
        self.ledger.put(Entry::Effect {
            key: key.clone(),
            ekey: self.effect_keys.get(&key).cloned(),
            output_mat: None,
            kind: "gen".into(),
            output: Json::Array(res.outputs.clone()),
            cost: res.cost,
        });
        self.trace
            .push("gen", &key, false, res.cost, sp, prompt.into());
        self.charge(0, 0.0, sp)?;
        Ok(wrap(&res.outputs))
    }

    pub(in crate::interp) fn ask(
        &mut self,
        state: &Rc<State>,
        q: &Rc<Question>,
        sp: Span,
    ) -> R<Value> {
        let key = self.effect_key_of("ask", &[&state.hash, &q.hash]);
        // 已答的照答；已问未答的：重放照记的给出（Pending），续跑再问一次
        let recorded = match self.ledger.get(&key) {
            Some(Entry::Ask {
                answer: Some(a), ..
            }) => Some(Some(a.clone())),
            Some(Entry::Ask { answer: None, .. }) if self.audit.on => Some(None),
            _ => None,
        };
        let answer = if let Some(answer) = recorded {
            answer
        } else {
            let limit = self.budget.escalate.unwrap_or(0);
            // 已问过的（账本里的）+ 这次运行新问的，一起核总上限
            if self.asks_in_ledger + self.cost.asks >= limit {
                return Err(Fault::Halt(Pending {
                    cause: "budget.escalate".into(),
                    key,
                    site: sp,
                    detail: format!("ask 次数已到上限 {limit}"),
                }));
            }
            if self.audit.on {
                return Err(self.replay_missing(format!("ask「{}」", q.text), sp));
            }
            self.cost.asks += 1;
            let a = self.client.ask(state, q).map_err(|e| {
                Fault::Error(RtError::new(
                    Some("E-rt-client"),
                    format!("ask 失败：{}", e.0),
                    sp,
                ))
            })?;
            // 已问未答也入账（步 7）：重放照样以 Pending 结束；续跑得到答案时另起一条（只增）
            self.ledger.put_answer(Entry::Ask {
                key: key.clone(),
                ekey: self.effect_keys.get(&key).cloned(),
                answer: a.clone(),
            });
            a
        };
        match answer {
            Some(a) => {
                self.trace
                    .push("ask", &key, false, 0.0, sp, format!("「{}」已答", q.text));
                let kind = match a {
                    Answer::Noul(p) => {
                        if p >= 0.5 {
                            ExitKind::Act
                        } else {
                            ExitKind::Ignore
                        }
                    }
                    Answer::Choice(v) => ExitKind::Pick(argmax(&v).0),
                    Answer::Score(v) => ExitKind::At(argmax(&v).0),
                };
                let e = self.new_exit(kind, None, q.op, &q.hash, &state.hash, Taint::Trusted, sp);
                if let Value::Exit(x) = &e {
                    x.from_ask.set(true); // 12:265「或经 ask」；人答是 trusted（§2.11）
                }
                Ok(e)
            }
            None => {
                self.trace.push(
                    "ask",
                    &key,
                    false,
                    0.0,
                    sp,
                    format!("「{}」未答 → Pending", q.text),
                );
                Err(Fault::Halt(Pending {
                    cause: "ask".into(),
                    key,
                    site: sp,
                    detail: format!("等人回答「{}」", q.text),
                }))
            }
        }
    }

    pub(in crate::interp) fn transform(
        &mut self,
        f: &Rc<Closure>,
        args: &[Value],
        sp: Span,
    ) -> R<Value> {
        let mut mats = vec![];
        for a in args {
            mats.push(self.as_mat(a, "transform", sp)?);
        }
        let hashes: Vec<&str> = mats.iter().map(|m| m.hash.as_str()).collect();
        // 13 §4：可复用结果的身份不能只取代码正文——工厂造出来的两个方法正文相同、捕获不同时，
        // 只按正文就会把前一个的结果复用给后一个（**算错**）。身份 = 代码哈希 + 实际捕获状态的指纹。
        // 指纹取不到（捕获里有读数/出口/嵌套太深）时**禁用这一项的跨运行缓存**，照常执行；
        // 宁可不缓存，也不返回另一个方法的结果。
        let captured = self.env_fingerprint(&f.env, &referenced_names(&f.function), 3);
        let taint = mats
            .iter()
            .fold(Taint::Trusted, |t, m| Taint::join(t, m.taint));
        // 保一跳：`transform(f, m)` 的产物仍派生自 m 那道题。此前恒清零——
        // 一次恒等变换 `transform(fn(x){content(x)}, m)` 就洗掉 J-02 的禁自指。
        let derived: BTreeSet<String> = mats
            .iter()
            .flat_map(|m| m.derived_from.iter().cloned())
            .collect();
        // B59（步 17a）：变换的产物承接输入材料的来源出口键
        let from: BTreeSet<String> = mats
            .iter()
            .flat_map(|m| m.from_key.iter().cloned())
            .collect();
        let Some(captured) = captured else {
            // **这句原来只说「不进跨运行缓存」，而它少说了一半**：这条路
            // **在 `ledger.put` 之前就返回了**，于是这份材料**根本不进账本**——
            // 而账本是今天唯一存着效应输出内容的地方。`12`:182 说「`gen`/`do`/`transform`
            // 的输出**默认入库**」，**对这条路是假的**，跨会话也就取不回来。
            self.trace.warn(format!(
                "W-no-cache: transform 的方法捕获环境指纹化不了（{}），这一项不进跨运行缓存；照常执行，不复用别人的结果。**它也不进账本**——账本是今天唯一存着效应输出内容的地方，所以这份材料跨会话取不回来（12:182「输出默认入库」对这条路不成立）",
                f.name.clone().unwrap_or_else(|| "匿名方法".into())
            ));
            let v = self.call_closure(
                f,
                mats.iter()
                    .map(|m| Value::Mat(Rc::new(m.clone())))
                    .collect(),
                sp,
            )?;
            if matches!(
                v,
                Value::Reading(_) | Value::Exit(_) | Value::Fn(_) | Value::State(_)
            ) {
                return err(
                    Some("J-11"),
                    format!("transform 的输出要能成材料，收到 {}", v.type_name()),
                    sp,
                );
            }
            let content = match &v {
                Value::Mat(m) => m.content.clone(),
                other => other.to_json(),
            };
            return Ok(Value::Mat(Rc::new(
                Mat::new(
                    content,
                    "transform",
                    vec!["transform:uncached".to_string()],
                    taint,
                    derived,
                )
                .with_from_key(from),
            )));
        };
        // 12:189「键 (site, f_hash, args_hash)」。`captured` 是 13 §4 另加的（身份含捕获状态）。
        let key = self.effect_key_of(
            "transform",
            &[&sp.start.to_string(), &f.hash, &captured, &hashes.join(",")],
        );
        if let Some(Entry::Effect { output, .. }) = self.ledger.get(&key) {
            self.cost.replayed += 1;
            self.trace
                .push("transform", &key, true, 0.0, sp, String::new());
            return Ok(Value::Mat(Rc::new(
                Mat::new(
                    output.clone(),
                    "transform",
                    vec![format!("transform:{key}")],
                    taint,
                    derived,
                )
                .with_from_key(from),
            )));
        }
        let v = self.call_closure(
            f,
            mats.iter()
                .map(|m| Value::Mat(Rc::new(m.clone())))
                .collect(),
            sp,
        )?;
        if matches!(
            v,
            Value::Reading(_) | Value::Exit(_) | Value::Fn(_) | Value::State(_)
        ) {
            return err(
                Some("J-11"),
                format!("transform 的输出要能成材料，收到 {}", v.type_name()),
                sp,
            );
        }
        let content = match &v {
            Value::Mat(m) => m.content.clone(),
            other => other.to_json(),
        };
        self.ledger.put(Entry::Effect {
            key: key.clone(),
            ekey: self.effect_keys.get(&key).cloned(),
            output_mat: None,
            kind: "transform".into(),
            output: content.clone(),
            cost: 0.0,
        });
        self.trace
            .push("transform", &key, false, 0.0, sp, String::new());
        Ok(Value::Mat(Rc::new(
            Mat::new(
                content,
                "transform",
                vec![format!("transform:{key}")],
                taint,
                derived,
            )
            .with_from_key(from),
        )))
    }
}
