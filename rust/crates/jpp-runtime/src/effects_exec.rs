//! 效应执行：`do`、`gen`、`ask`、`transform` 的键、账本与 taint（20 §2.3 `effects_exec.rs`）。
//! 步 4a 从 `interp.rs` 机械拆出，只搬代码、不改逻辑（21 §三·3）。

use super::*;
use jpp_effects::{EffectSpec, TaintRule};

/// 效应输出的 taint 按 `EffectSpec.taint_rule` 定（步 15a；规则表在注册表，格运算在 `jpp-value`，
/// `20` §2.3）。`inherited` 是输入 taint 的 ∨；`declared` 是声明位（`do` 的动作声明）。记账变换
/// 今天没有声明位，按 `inherit` 处理，与步 15a 之前逐字节相同。
fn out_taint(rule: TaintRule, inherited: Taint, declared: Option<TaintOut>) -> Taint {
    match rule {
        TaintRule::Trusted => Taint::Trusted,
        TaintRule::Inherit => inherited,
        TaintRule::Declared => match declared {
            Some(TaintOut::Trusted) => Taint::Trusted,
            Some(TaintOut::Untrusted) => Taint::Untrusted,
            Some(TaintOut::Inherit) | None => inherited,
        },
    }
}

/// 动作输出与声明形状不符时给出报文（B51-R2）：列表按项数计，非列表算 1 项；单项尺寸按规范 JSON
/// 文本（字符串取原文）的字符数计。`settled` 本版只记录，不核（静态面在步 24）。
fn shape_violation(shape: &jpp_effects::MatShape, out: &Json) -> Option<String> {
    use jpp_effects::ShapeItems;
    let items: Vec<&Json> = match out {
        Json::Array(a) => a.iter().collect(),
        other => vec![other],
    };
    let bound = match shape.items {
        ShapeItems::One => Some(1),
        ShapeItems::AtMost(k) => Some(k),
        ShapeItems::Unbounded => None,
    };
    if let Some(k) = bound
        && items.len() > k
    {
        // 依据：B51-R2（20 附录 A；步 15d 运行期核对）
        return Some(format!(
            "ShapeMismatch: 声明至多 {k} 项，实际 {} 项（B51-R2）",
            items.len()
        ));
    }
    if let Some(limit) = shape.item_size {
        for (i, it) in items.iter().enumerate() {
            let n = match it {
                Json::String(t) => t.chars().count(),
                other => canon(other).chars().count(),
            };
            if n > limit {
                // 依据：B51-R2（20 附录 A；步 15d 运行期核对）
                return Some(format!(
                    "ShapeMismatch: 第 {i} 项 {n} 字符，超声明的单项上界 {limit}（B51-R2）"
                ));
            }
        }
    }
    None
}

/// 一组题的端口输入（B155，步 15i）：全组同一 `StateHash` 发 `StateQuestions`（与步 15i 前逐字节相同，
/// 只认它的端口不受影响）；跨多个状态（同材料、`over` 不同）发 `MaterialQuestions`，逐题带状态。
/// 依据：B155（地基/附注/2026-09-26-批6裁定.md §三）
fn judge_input(states: &[Rc<State>], questions: Vec<Question>) -> CallInput {
    if states.iter().all(|s| s.hash == states[0].hash) {
        CallInput::StateQuestions {
            state: (*states[0]).clone(),
            questions,
        }
    } else {
        CallInput::MaterialQuestions {
            states: states.iter().map(|s| (**s).clone()).collect(),
            questions,
        }
    }
}

impl<'a> Interp<'a> {
    /// 判断调用经端口发出（步 15b）：产出读数的效应的端口。`states[i]` 是 `questions[i]` 的状态，
    /// 同一组题材料相同（B155，步 15i）；全组同一 `StateHash` 时输入仍是一个状态加一组题。
    pub(crate) fn call_judge(
        &mut self,
        states: &[Rc<State>],
        questions: &[&Question],
    ) -> Result<jpp_effects::JudgeResult, EffectError> {
        let effect = jpp_effects::find(|s| s.produces_reading).expect("注册表里有判断");
        let input = judge_input(states, questions.iter().map(|q| (*q).clone()).collect());
        match self.ports.call(effect, input)? {
            EffectOut::Readings(r) => Ok(r),
            _ => Err(EffectError("判断端口返回的不是读数".into())),
        }
    }

    /// 一窗判断一次交端口（步 15e）：各组一个调用，端口内并发；结果按组的顺序。端口拒收整批时每组同错。
    pub(crate) fn call_judge_many(
        &mut self,
        窗: &[super::flush::待发],
    ) -> Vec<Result<jpp_effects::JudgeResult, EffectError>> {
        let effect = jpp_effects::find(|s| s.produces_reading).expect("注册表里有判断");
        let inputs = 窗
            .iter()
            .map(|g| {
                judge_input(
                    &g.states,
                    g.items.iter().map(|(q, _, _)| (**q).clone()).collect(),
                )
            })
            .collect();
        match self.ports.call_many(effect, inputs) {
            Ok(rs) => rs
                .into_iter()
                .map(|r| match r {
                    Ok(EffectOut::Readings(r)) => Ok(r),
                    Ok(_) => Err(EffectError("判断端口返回的不是读数".into())),
                    Err(e) => Err(e),
                })
                .collect(),
            Err(e) => 窗.iter().map(|_| Err(e.clone())).collect(),
        }
    }

    /// 效应内置的分派（步 15a，`20` A2 与 §2.3 `effects_exec.rs`）：只读 `EffectSpec` 的字段。
    /// 产出读数的效应走分层登记（刷新点成批发出）；其余是 `Immediate`，当场执行：触世界的走动作
    /// 登记处，输出是人的回答的走问人，不进效应行的是宿主记账变换，余下输出材料的走生成端口。
    pub(crate) fn effect_builtin(
        &mut self,
        s: &'static jpp_effects::EffectSpec,
        name: &'static str,
        args: Vec<Value>,
        sp: Span,
    ) -> R<Value> {
        use jpp_effects::{OutputShape, SchedClass};
        if s.produces_reading {
            debug_assert_eq!(s.sched, SchedClass::Layered);
            return self.b_judge(name, args, sp);
        }
        debug_assert_eq!(s.sched, SchedClass::Immediate);
        if s.side_effecting {
            self.b_do(s, name, args, sp)
        } else if s.output_shape == OutputShape::Answer {
            self.b_ask(s, name, args, sp)
        } else if !s.in_effect_row {
            self.b_transform(s, name, args, sp)
        } else {
            self.b_gen(s, name, args, sp)
        }
    }

    pub(crate) fn do_(
        &mut self,
        s: &'static EffectSpec,
        name: &str,
        args: &[Value],
        iter_seq: i64,
        sp: Span,
    ) -> R<Value> {
        // 惰性过桥（B94，审查修复 3a）：此前切出、还没检视的出口先解析，判断在这次效应之前计费，
        // 与改前 `cut` 当场刷新的先后相同（预算紧时出口种类不变）
        self.解析全部帧()?;
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
        let site = self.站点(sp).to_string();
        let key = self.effect_key_of(
            s.name,
            &[
                &site,
                name,
                &args_canon.join("\u{1f}"),
                &iter_seq.to_string(),
            ],
        );
        // 步 15h-3：先查开着的层（层里已有的不重复执行），再查账本
        if let Some(Entry::Effect {
            output,
            output_mat,
            cost,
            ..
        }) = self.账本查(&key)
        {
            // 账本 v3 记下了首跑的来源边（B84、B92）；重放时再并上由实参重算的边（同一程序同一结果）
            let v = match entry_to_effect_value(output, output_mat.as_deref()) {
                Value::Mat(m) => {
                    Value::Mat(Rc::new((*m).clone().with_sources(&from_keys_of(args))))
                }
                other => other,
            };
            let cost = *cost;
            if self.audit.on {
                // B38（步 15d）：do 计一次调用与它的费用（首跑 charge(1, action.cost)，在执行前核）
                self.audit.usd += cost;
                self.audit.calls += 1;
            }
            self.cost.replayed += 1;
            self.trace_不付费(s, &key, sp, name);
            // Z0565：守卫下复用了一条失败的不可逆记录就列出（首跑在结算处列出；审计重放与非审计再跑都在这里照记录
            // 列出，复核 Z0565 小项）
            if self.guard && !action.reversible {
                self.记结算失败(sp, name, &v);
            }
            return Ok(v);
        }
        // 入参 taint 要**递归看容器**：材料嵌在记录字段或嵌套列表里时，顶层 match 看不见它，
        // 以前落进 `_ => Trusted`，于是 Inherit 的动作拿到「入参全可信」——脏材料喂进 do 出来就干净了。
        // 与 J-01 的 `unwrap_or(false)`、taint 反序列化兜底、`mat(content(脏))` 同一形状。
        let taint_in = args
            .iter()
            .fold(Taint::Trusted, |t, a| Taint::join(t, taint_of(a)));
        let taint = out_taint(s.taint_rule, taint_in, Some(action.taint_out));
        // B55（步 18b）：上一趟执行前写了意向、账本里没有结果的不可逆动作，远端可能已经完成。
        // 续接与审计重放都不重执行（画像声明 `idempotent: true` 的除外），给结果未知的失败值
        // （J-12 走 `Unsure(fail)`）；不补写 `Effect`，账本里「有 `Effect` 即动作返回过」保持成立。
        // 依据：B55（20 v2 附录 B55 条、§4.3）；主会话 2026-09-25（审计重放同样给失败值、idempotent 缺失按不幂等）
        let 意向键 = format!("intent:{key}");
        let 要意向 = !action.reversible;
        let 幂等 = self
            .calib
            .profile()
            .actions
            .get(name)
            .and_then(|a| a.idempotent)
            .unwrap_or(false);
        // G2（B200；附录三按位置判）：最后一条意向之后有 `Withheld`＝那一趟确知没执行，不是结果未知
        let 扣下过 = self.ledger.view().intent_withheld(&意向键);
        if 要意向 && !幂等 && !扣下过 && self.ledger.view().get(&意向键).is_some() {
            // 依据：B55（20 v2 附录 B55 条：有意向无结果的不可逆动作不重执行，标明未知）
            self.trace.warn(format!(
                "W-unknown-outcome: @{} do「{name}」上一趟执行前写了意向、账本里没有结果：动作可能已经完成，也可能没有。不重执行，产出结果未知的失败值（B55）；要确认请去查动作的目标",
                sp.start
            ));
            self.trace_不付费(s, &key, sp, name);
            return Ok(Value::Fail(
                Rc::from(
                    format!("unknown_outcome: do「{name}」上一趟写了意向、没有结果，远端可能已经完成；不重执行（B55）")
                        .as_str(),
                ),
                Provenance::new(taint, from_keys_of(args)),
            ));
        }
        // J-08：不可逆 `do` 的唯一放行点（`20` v2 §2.3 `guard.rs::release`；步 16）
        self.release(name, action.reversible, sp)?;
        // B38（步 15d）：budget.calls 计所有效应调用，do 每次执行计一次。依据：B38（20 附录 A）
        // G4：跨程序触发链到限，不执行、不写意向，产出失败值
        if self.深度停 {
            return Ok(self.深度停效应(sp));
        }
        // B93（步 22-0）：超预算不执行、不写意向，产出失败值（J-12 走 Unsure(fail)），程序照常往下
        if let Err(detail) = self.charge(1, action.cost) {
            self.写停下(StopCause::Budget);
            self.记停发(1, sp, &detail);
            return Ok(预算失败值(&detail));
        }
        // G2（B200，主控定第 4 条）：`--guard` 下不可逆 `do` 推迟到这次运行有结论之后——到达这里只写意向、不执行，
        // 给一个「已推迟」的值；程序结束时没有违规才按序执行，有违规就扣下（`Withheld`）
        if self.guard && 要意向 {
            return self.推迟do(s, name, args.to_vec(), key, 意向键, taint, sp, action.cost);
        }
        if self.audit.on {
            return Err(self.replay_missing(format!("do「{name}」"), sp));
        }
        // B55（步 18b）：不可逆动作执行前写意向并落盘，写不进去即停（动作不执行）。键加 `intent:` 前缀：
        // 与结果同键的话，`put` 会因同键已有丢掉后来的 `Effect`。`at` = 追加时账本已有的条目数。
        // 依据：B55（20 v2 附录 B55 条）
        // 步 15h-3：层开着时先收层——写前意向仍先于动作落盘（B55），并排在它之前登记的条目之后
        // 依据：B55、B160（过程记录 工程-步15h-3.md 二·3）
        if 要意向 {
            self.收层()?;
        }
        // G2 附录三：上一条意向已被扣下的，这一趟另写一条（执行后被杀仍能认出结果未知）
        if 要意向 && (self.ledger.view().get(&意向键).is_none() || 扣下过) {
            let at = self.ledger.view().len() as u64;
            self.即刻记账(
                Entry::Intent {
                    key: 意向键,
                    at,
                    attempt: None,
                },
                sp,
            )?;
        }
        // B51-R2（步 15d）：声明了输出形状的动作，返回后核基数与单项尺寸；违反即失败值
        // Z0885：世界动作（`env:*`，B159 写法二）记这次执行的墙钟，供时延实测（记而不比，不进值）
        let 墙钟起 = name.starts_with("env:").then(unix_seconds);
        let result = (action.f)(args).and_then(|v| match &action.mat_shape {
            Some(shape) => 形状核对(shape, &v.to_json()).map(|_| v),
            None => Ok(v),
        });
        let out = 动作产出(name, &key, args, taint, result);
        self.cost.usd += action.cost;
        self.cost.calls += 1;
        self.记请求(s);
        let (output, output_mat) = effect_value_to_entry(&out);
        let 结果 = Entry::Effect {
            key: key.clone(),
            ekey: self.effect_keys.get(&key).cloned(),
            output_mat: output_mat.map(Box::new),
            kind: s.name.into(),
            output,
            cost: action.cost,
            reused_from: None,
            wall: 墙钟起.map(|t0| [t0, unix_seconds()]),
        };
        // B55：不可逆动作的结果即刻落盘；可逆动作层末落盘
        if 要意向 {
            self.即刻记账(结果, sp)?;
        } else {
            // 步 15h-3：层开着时进层，按登记序入账（B160）
            self.登记记账(结果);
        }
        self.trace
            .push(s.name, &key, false, action.cost, sp, name.into());
        Ok(out)
    }

    /// `do` 没有执行、不付费的一行轨迹（账本里已有结果，或 B55 的未知结果）
    fn trace_不付费(&mut self, s: &'static EffectSpec, key: &str, sp: Span, name: &str) {
        self.trace.push(s.name, key, true, 0.0, sp, name.into());
    }

    pub(crate) fn generate(
        &mut self,
        s: &'static EffectSpec,
        prompt: &str,
        ctx: &[Mat],
        n: usize,
        retry_seq: i64,
        sp: Span,
    ) -> R<Value> {
        // 审查修复 3a：同 `do_`
        self.解析全部帧()?;
        let ctx_hash: Vec<&str> = ctx.iter().map(|m| m.hash.as_str()).collect();
        // 12:158「键：(site, prompt_hash, ctx_hash, n, retry_seq)」——site 排第一位。
        // 缺了它，同一段 prompt 在两个站点生成会撞键，第二个站点命中第一个的输出。
        // gen 比 judge 更容易撞：prompt 常是字面量，两处写同一句话很正常。
        let site = self.站点(sp).to_string();
        let key = self.effect_key_of(
            s.name,
            &[
                &site,
                prompt,
                &ctx_hash.join(","),
                &n.to_string(),
                &retry_seq.to_string(),
            ],
        );
        let taint = ctx
            .iter()
            .fold(Taint::Trusted, |t, m| Taint::join(t, m.taint));
        let taint = out_taint(s.taint_rule, taint, None);
        // 与 do 同：并入 ctx 各材料的 derived_from（保一跳）
        let derived: BTreeSet<String> = ctx
            .iter()
            .flat_map(|m| m.derived_from.iter().cloned())
            .collect();
        // B59（步 17a）：来源出口键同样承接 ctx
        // B92（步 18c）：承接 ctx 的来源边，种类不变
        let from = ctx
            .iter()
            .fold(Sources::empty(), |s, m| s.union(&m.prov().sources));
        // 输出材料的 taint：端口声明了就取声明（B149：生成器端口缺省 `untrusted`），否则 ∨ ctx（B37）
        let wrap = |outs: &[Json], taint: Taint| {
            crate::gen_pending::包材料(outs, prompt, &key, taint, &derived, &from)
        };
        let model = self.生成模型();
        // 步 15h-3：先查开着的层——复用命中的条目层开着时进层（下面），同一站点再调一次要在层里查到它，
        // 否则会再命中一次复用、层里出现两条同键条目
        if let Some(Entry::Effect {
            output,
            output_mat,
            cost,
            reused_from,
            ..
        }) = self.账本查(&key)
        {
            // 步 15h-1：失败照记录给回 `Fail`；声明的 taint 记在 `output_mat`（旧条目为空，照旧 ∨ ctx）
            let 失败 = output.get("__fail").is_some();
            let v = if 失败 {
                entry_to_effect_value(output, None)
            } else {
                let outs: Vec<Json> = output.as_array().cloned().unwrap_or_default();
                wrap(&outs, output_mat.as_ref().map_or(taint, |m| m.taint))
            };
            let (cost, 复用来的) = (*cost, reused_from.is_some());
            // 步 19：复用条目本来没有调用，不计入审计重放；成功的记进本运行表
            if !复用来的 {
                self.audit_account(0, cost, sp);
            }
            if !失败 {
                self.记可复用效应(&key, &model);
            }
            self.cost.replayed += 1;
            self.trace.push(s.name, &key, true, 0.0, sp, prompt.into());
            return Ok(v);
        }
        // 公开 PR #37 评审 P1：同键的生成已登记、还没收回——共享它，不再登记、不交出、不计调用与预算
        // （账本按键只留一条，只凭账本重放时这一位置走上面的账本命中，拿到同一输出）
        if let Some(h) = self.在飞同键(&key) {
            self.cost.replayed += 1;
            self.trace.push(s.name, &key, true, 0.0, sp, prompt.into());
            return Ok(Value::Gen(h));
        }
        // 步 19（B151 两段式，取代 15h-2 的 `--gen-cache`）：按不含调用位置、带生成器模型的缓存键复用本运行
        // 已取回的生成或跨运行缓存里的产物；命中不登记、不交端口、不计调用，照写一条复用条目（费用 0），
        // 这一趟的账本仍可只凭账本重放。审计重放只凭账本，不查缓存。依据：B151（`21` 步 19 追加项）
        if let Some(hit) = self.效应复用(&key, &model) {
            let v = if hit.output.get("__fail").is_some() {
                entry_to_effect_value(&hit.output, None)
            } else {
                let outs: Vec<Json> = hit.output.as_array().cloned().unwrap_or_default();
                wrap(&outs, hit.output_mat.as_ref().map_or(taint, |m| m.taint))
            };
            // 步 15h-3（Q3，B171）：层开着时复用条目进层，按登记序入账（B160）
            self.登记记账(Entry::Effect {
                key: key.clone(),
                ekey: self.effect_keys.get(&key).cloned(),
                output_mat: hit.output_mat,
                kind: s.name.into(),
                output: hit.output,
                cost: reuse::零费用,
                reused_from: Some(hit.reused_from),
                wall: None,
            });
            self.trace
                .push(s.name, &key, true, reuse::零费用, sp, prompt.into());
            return Ok(v);
        }
        // G4：跨程序触发链到限，不发，产出失败值
        if self.深度停 {
            return Ok(self.深度停效应(sp));
        }
        // B93（步 22-0）：超预算不发，产出失败值（J-12），程序照常往下
        if let Err(detail) = self.charge(1, 0.0) {
            self.写停下(StopCause::Budget);
            self.记停发(1, sp, &detail);
            return Ok(预算失败值(&detail));
        }
        if self.audit.on {
            return Err(self.replay_missing(format!("gen「{prompt}」"), sp));
        }
        let ctx_json: Vec<Json> = ctx.iter().map(|m| m.content.clone()).collect();
        let input = CallInput::Prompt {
            prompt: prompt.to_string(),
            ctx: ctx_json,
            n,
            retry_seq: retry_seq as u64,
        };
        // 步 15h-2（B149）：只交端口、不在调用点等；结果在第一次被检视时取回、记账（`gen_pending.rs`）。
        // 实际费用超出时停的是下一步（下一次发出前的核对，B93）
        self.登记生成(s, key, prompt, input, taint, derived, from, sp)
    }

    pub(crate) fn ask(
        &mut self,
        s: &'static EffectSpec,
        state: &Rc<State>,
        q: &Rc<Question>,
        sp: Span,
    ) -> R<Value> {
        // 审查修复 3a：同 `do_`
        self.解析全部帧()?;
        let key = self.effect_key_of(s.name, &[&state.hash, &q.hash]);
        // 已答的照答；已问未答的：重放照记的给出（Pending），续跑再问一次
        // 步 15h-3：先查开着的层
        let recorded = match self.账本查(&key) {
            Some(Entry::Ask {
                answer: Some(a), ..
            }) => Some(Some(a.clone())),
            Some(Entry::Ask { answer: None, .. }) if self.audit.on => Some(None),
            _ => None,
        };
        // B38（步 15d）：只凭账本重放时，记过的 ask 照记录计入调用（首跑在哪里停，重放就在哪里停）
        if recorded.is_some() && self.audit.on {
            self.audit.calls += 1;
        }
        let answer = if let Some(answer) = recorded {
            answer
        } else {
            // G4（主控定第 3 条）：跨程序触发链到限，不问人，直接给出口 `Unsure(depth)`，不挂起
            if self.深度停 {
                let d = self.深度停说明();
                self.写停下(StopCause::Depth);
                self.记停发(1, sp, &d);
                self.trace.push(
                    s.name,
                    &key,
                    false,
                    0.0,
                    sp,
                    format!("「{}」深度到限未问 → Unsure(depth)", q.text),
                );
                return Ok(self.new_exit(
                    ExitKind::Unsure(Why::of(UnsureCause::Depth)),
                    None,
                    q.op,
                    &q.hash,
                    &state.hash,
                    out_taint(s.taint_rule, Taint::Trusted, None),
                    sp,
                ));
            }
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
            // B38（步 15d）：ask 也计入 budget.calls；它同时受 budget.escalate 约束，两道限制并存
            // `ask` 超 calls 预算仍挂起（B93：Pending 只由 ask 产生）
            if let Err(detail) = self.charge(1, 0.0) {
                return Err(Fault::Halt(Pending {
                    cause: "budget".into(),
                    key,
                    site: sp,
                    detail,
                }));
            }
            self.cost.asks += 1;
            self.cost.calls += 1;
            self.记请求(s);
            let input = CallInput::StateQuestion {
                state: (**state).clone(),
                question: (**q).clone(),
            };
            let a = match self.ports.call(s.id, input) {
                Ok(EffectOut::Answer(a)) => Ok(a),
                Ok(_) => Err(EffectError("问人端口返回的不是回答".into())),
                Err(e) => Err(e),
            }
            .map_err(|e| {
                Fault::Error(RtError::new(
                    Some("E-rt-client"),
                    format!("ask 失败：{}", e.0),
                    sp,
                ))
            })?;
            // 已问未答也入账（步 7）：重放照样以 Pending 结束；续跑得到答案时另起一条（只增）
            // 步 15h-3：层开着时进层，按登记序入账（B160）
            self.登记记账(Entry::Ask {
                key: key.clone(),
                ekey: self.effect_keys.get(&key).cloned(),
                answer: a.clone(),
            });
            a
        };
        match answer {
            Some(a) => {
                self.trace
                    .push(s.name, &key, false, 0.0, sp, format!("「{}」已答", q.text));
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
                let e = self.new_exit(
                    kind,
                    None,
                    q.op,
                    &q.hash,
                    &state.hash,
                    out_taint(s.taint_rule, Taint::Trusted, None),
                    sp,
                );
                if let Value::Exit(x) = &e {
                    x.from_ask.set(true); // 12:265「或经 ask」；人答是 trusted（§2.11）
                    x.alpha.set(Some(jpp_value::stat::ALPHA_HUMAN)); // B161：人答即真值（B31），联合界计 0
                }
                Ok(e)
            }
            None => {
                self.trace.push(
                    s.name,
                    &key,
                    false,
                    0.0,
                    sp,
                    format!("「{}」未答 → Pending", q.text),
                );
                Err(Fault::Halt(Pending {
                    cause: s.name.into(),
                    key,
                    site: sp,
                    detail: format!("等人回答「{}」", q.text),
                }))
            }
        }
    }

    pub(crate) fn transform(
        &mut self,
        s: &'static EffectSpec,
        f: 变换方法<'_>,
        args: &[Value],
        sp: Span,
    ) -> R<Value> {
        let mut mats = vec![];
        for a in args {
            mats.push(self.as_mat(a, s.name, sp)?);
        }
        let hashes: Vec<&str> = mats.iter().map(|m| m.hash.as_str()).collect();
        // 13 §4：可复用结果的身份不能只取代码正文——工厂造出来的两个方法正文相同、捕获不同时，
        // 只按正文就会把前一个的结果复用给后一个（**算错**）。身份 = 代码哈希 + 实际捕获状态的指纹。
        // 指纹取不到（捕获里有读数/出口/嵌套太深）时**禁用这一项的跨运行缓存**，照常执行；
        // 宁可不缓存，也不返回另一个方法的结果。
        // 宿主变换（步 26，`12` §2.8 `f: HostFn`）：身份 = `host:<名>@<版本>`，没有捕获环境，指纹恒为空串
        let (captured, f_hash, 声明) = match f {
            变换方法::闭包(c) => (
                self.env_fingerprint(&c.env, &referenced_names(&c.function), 3),
                c.hash.clone(),
                None,
            ),
            变换方法::宿主(h) => (
                Some(String::new()),
                h.identity(),
                Some(match h.taint_out {
                    jpp_effects::HostTaint::Inherit => TaintOut::Inherit,
                    jpp_effects::HostTaint::Trusted => TaintOut::Trusted,
                    jpp_effects::HostTaint::Untrusted => TaintOut::Untrusted,
                }),
            ),
        };
        let taint = mats
            .iter()
            .fold(Taint::Trusted, |t, m| Taint::join(t, m.taint));
        let taint = out_taint(s.taint_rule, taint, 声明);
        // 保一跳：`transform(f, m)` 的产物仍派生自 m 那道题。此前恒清零——
        // 一次恒等变换 `transform(fn(x){content(x)}, m)` 就洗掉 J-02 的禁自指。
        let derived: BTreeSet<String> = mats
            .iter()
            .flat_map(|m| m.derived_from.iter().cloned())
            .collect();
        // B59（步 17a）：变换的产物承接输入材料的来源出口键
        // B92（步 18c）：承接输入材料的来源边，种类不变
        let from = mats
            .iter()
            .fold(Sources::empty(), |s, m| s.union(&m.prov().sources));
        let Some(captured) = captured else {
            let 变换方法::闭包(f) = f else {
                unreachable!("宿主变换的捕获指纹恒为 Some")
            };
            // **这句原来只说「不进跨运行缓存」，而它少说了一半**：这条路
            // **在 `ledger.put` 之前就返回了**，于是这份材料**根本不进账本**——
            // 而账本是今天唯一存着效应输出内容的地方。`12`:182 说「`gen`/`do`/`transform`
            // 的输出**默认入库**」，**对这条路是假的**，跨会话也就取不回来。
            self.trace.warn(format!(
                "W-no-cache: transform 的方法捕获环境指纹化不了（{}），这一项不进跨运行缓存；照常执行，不复用别人的结果。**它也不进账本**——账本是今天唯一存着效应输出内容的地方，所以这份材料跨会话取不回来（12:182「输出默认入库」对这条路不成立）",
                f.name.clone().unwrap_or_else(|| "匿名方法".into())
            ));
            // 依据：B35 (1)、12 §2.4 订正注、主会话裁定 2026-09-29 第二十二条：这一路没有键、不进账本，
            // 只凭账本重放时无处可查，照旧现算；报告里逐站点标明这一项没有经账本核对（一站一条）。
            if self.audit.on {
                let 未核对 = format!(
                    "W-replay-unchecked: @{} 闭包变换 {}（方法哈希 {f_hash}）的捕获环境指纹化不了，不进账本；只凭账本重放时无记录可核，照旧现算，这一项没有经账本核对",
                    sp.start,
                    f.name.clone().unwrap_or_else(|| "匿名方法".into()),
                );
                if !self.trace.warnings.contains(&未核对) {
                    self.trace.warn(未核对);
                }
            }
            let v = self.call_closure(
                f,
                mats.iter()
                    .map(|m| Value::Mat(Rc::new(m.clone())))
                    .collect(),
                sp,
            )?;
            // 惰性出口先解析（B94，审查修复 2）：闭包返回的出口要照改前报 J-11
            let v = self.检视(v)?;
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
                    s.name,
                    vec!["transform:uncached".to_string()],
                    taint,
                    derived,
                )
                .with_sources(&from),
            )));
        };
        // 12:189「键 (site, f_hash, args_hash)」。`captured` 是 13 §4 另加的（身份含捕获状态）。
        let site = self.站点(sp).to_string();
        let key = self.effect_key_of(s.name, &[&site, &f_hash, &captured, &hashes.join(",")]);
        // 步 15h-3：先查开着的层，再查账本
        // 变换的缓存键不含模型（方法身份在方法哈希与捕获指纹里）
        let 已有 = match self.账本查(&key) {
            Some(Entry::Effect { output, .. }) => Some(output.clone()),
            _ => None,
        };
        if let Some(output) = 已有 {
            self.记可复用效应(&key, "");
            self.cost.replayed += 1;
            self.trace.push(s.name, &key, true, 0.0, sp, String::new());
            return Ok(Value::Mat(Rc::new(
                Mat::new(
                    output,
                    s.name,
                    vec![format!("transform:{key}")],
                    taint,
                    derived,
                )
                .with_sources(&from),
            )));
        }
        // 步 19（B20、jev-ca 提醒 1）：按不含调用位置的缓存键复用本运行或跨运行的变换结果
        if let Some(hit) = self.效应复用(&key, "") {
            // 步 15h-3（Q3，B171）：层开着时复用条目进层，按登记序入账（B160）
            self.登记记账(Entry::Effect {
                key: key.clone(),
                ekey: self.effect_keys.get(&key).cloned(),
                output_mat: None,
                kind: s.name.into(),
                output: hit.output.clone(),
                cost: reuse::零费用,
                reused_from: Some(hit.reused_from),
                wall: None,
            });
            self.trace
                .push(s.name, &key, true, reuse::零费用, sp, String::new());
            return Ok(Value::Mat(Rc::new(
                Mat::new(
                    hit.output,
                    s.name,
                    vec![format!("transform:{key}")],
                    taint,
                    derived,
                )
                .with_sources(&from),
            )));
        }
        // 只凭账本重放时变换缺记录即 `E-replay`，报文写明是哪个变换、哪个版本（宿主变换：名与版本，主会话裁定
        // 2026-09-29 第二十一条；闭包变换：名、站点起点与方法哈希，第二十二条）。依据：B35 (1)、`12` §2.8「重放取账本」，
        // 与 `do`、`gen`、`ask` 缺记录同一处置；早于变换本身执行。键算不出来的闭包（`W-no-cache`）在上面已另行处置。
        if self.audit.on {
            let 身份 = match f {
                变换方法::宿主(h) => format!(
                    "宿主变换 {}（版本 {}，键 {key}）；版本不同说明账本是旧规则记的，按新规则重跑或续跑",
                    h.name, h.version
                ),
                变换方法::闭包(c) => format!(
                    "闭包变换 {}（站点起点 {}，方法哈希 {f_hash}，键 {key}）；账本里没有这次变换的记录，方法正文、捕获的值、入参或所在位置与记账时不同，按当前程序重跑或续跑",
                    c.name.clone().unwrap_or_else(|| "匿名方法".into()),
                    sp.start
                ),
            };
            return Err(self.replay_missing(身份, sp));
        }
        let content = match f {
            变换方法::宿主(h) => {
                let ins: Vec<Json> = mats.iter().map(|m| m.content.clone()).collect();
                (h.run)(&ins).or_else(|e| {
                    err(
                        Some("E-rt-arg"),
                        format!("宿主变换 {} 报错：{e}。用法：{}", h.name, h.doc),
                        sp,
                    )
                })?
            }
            变换方法::闭包(f) => {
                let v = self.call_closure(
                    f,
                    mats.iter()
                        .map(|m| Value::Mat(Rc::new(m.clone())))
                        .collect(),
                    sp,
                )?;
                // 惰性出口先解析（B94，审查修复 2）：闭包返回的出口要照改前报 J-11
                let v = self.检视(v)?;
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
                match &v {
                    Value::Mat(m) => m.content.clone(),
                    other => other.to_json(),
                }
            }
        };
        // 步 15h-3：层开着时进层，按登记序入账（B160）
        self.登记记账(Entry::Effect {
            key: key.clone(),
            ekey: self.effect_keys.get(&key).cloned(),
            output_mat: None,
            kind: s.name.into(),
            output: content.clone(),
            cost: 0.0,
            reused_from: None,
            wall: None,
        });
        self.记可复用效应(&key, "");
        self.trace.push(s.name, &key, false, 0.0, sp, String::new());
        Ok(Value::Mat(Rc::new(
            Mat::new(
                content,
                s.name,
                vec![format!("transform:{key}")],
                taint,
                derived,
            )
            .with_sources(&from),
        )))
    }
}

/// `transform` 的方法：`.jpp` 闭包，或宿主变换表里登记的宿主函数（步 26，`12` §2.8 `f: HostFn`）。
/// 两者记账、缓存、taint、来源完全同一条路径，只有身份（键的方法位）与求值不同。
#[derive(Clone, Copy)]
pub(crate) enum 变换方法<'x> {
    闭包(&'x Rc<Closure>),
    宿主(&'x jpp_effects::HostTransform),
}

/// 预算停发的效应产出的失败值（B93，`12` J-12）：`budget: <报文>`，trusted（程序自己的记账，不来自外面）
impl<'a> Interp<'a> {
    /// 深度到限时 `gen`、`do` 的产出（G4）：写「停下」、计一次停发，给失败值
    fn 深度停效应(&mut self, sp: Span) -> Value {
        let d = self.深度停说明();
        self.写停下(StopCause::Depth);
        self.记停发(1, sp, &d);
        Value::Fail(
            Rc::from(format!("depth: {d}").as_str()),
            Provenance::trusted(),
        )
    }
}

/// 动作输出形状核对（B51-R2）：违反给报文
pub(crate) fn 形状核对(shape: &jpp_effects::MatShape, out: &Json) -> Result<(), String> {
    match shape_violation(shape, out) {
        Some(m) => Err(m),
        None => Ok(()),
    }
}

/// 动作执行结果 → 程序拿到的值（成功是材料，失败是带动作输出位的失败值）；立即执行与守卫下推迟后执行共用
/// Unix 秒（墙钟）。只给账本的 `wall` 用，不进任何程序值（B157）。
fn unix_seconds() -> f64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs_f64())
        .unwrap_or(0.0)
}

pub(crate) fn 动作产出(
    name: &str,
    key: &str,
    args: &[Value],
    taint: Taint,
    result: Result<Value, String>,
) -> Value {
    match result {
        // `derived_from` 与 taint 一样要**折算**，不是恒清零：`do(…, [m])` 的产物仍派生自 m 那道题（`12` J-02 范围裁定）
        Ok(v) => Value::Mat(Rc::new(
            Mat::new(
                v.to_json(),
                &format!("do:{name}"),
                vec![format!("do:{key}")],
                taint,
                derived_of(args),
            )
            .with_sources(&from_keys_of(args)),
        )),
        Err(msg) => {
            // 失败信息同样来自外面：Fail 带动作的输出位；来源 = 实参的来源（B84）
            let m = format!("{name}: {msg}");
            Value::Fail(
                Rc::from(m.as_str()),
                Provenance::new(taint, from_keys_of(args)),
            )
        }
    }
}

fn 预算失败值(detail: &str) -> Value {
    Value::Fail(
        Rc::from(format!("budget: {detail}").as_str()),
        Provenance::trusted(),
    )
}
