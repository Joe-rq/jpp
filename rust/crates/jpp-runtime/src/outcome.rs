//! 运行入口与 `Outcome` 组装：`run`、契约值的构造与记账位置（20 §2.3 `outcome.rs`）。
//! 步 4a 从 `interp.rs` 机械拆出，只搬代码、不改逻辑（21 §三·3）。

use super::*;

impl<'a> Interp<'a> {
    /// 一次运行。计划（`jpp-plan` 按 pass 开关算出）与钩子由宿主交进来（步 14a；此前在这里由
    /// `self.passes` 现算，算法与时点不变：宿主在调用 `run` 前一刻算）。运行时只读计划、问钩子。
    pub fn run(
        mut self,
        program: &Program,
        plan: jpp_ir::plan::Plan,
        hooks: &'a dyn jpp_ir::plan::PlanHooks,
    ) -> Result<Outcome, RtError> {
        self.plan = plan;
        self.hooks = hooks;
        // 步 30 / B0488：读数站点的 `span.start → SiteId` 表（层内挑选取下游层数用）；同一起点多个候选的不收
        {
            use jpp_ir::ir::{ConsumeHow, SiteKind};
            let mut 重: HashSet<usize> = HashSet::new();
            for s in &program.sites.sites {
                let 读数 = match &s.kind {
                    SiteKind::Effect(e) => jpp_effects::spec(*e).produces_reading,
                    SiteKind::Consume(ConsumeHow::Literalize) | SiteKind::Construct(_) => true,
                    _ => false,
                };
                if !读数 {
                    continue;
                }
                if self.站点表.insert(s.span.start, s.id).is_some() {
                    重.insert(s.span.start);
                }
            }
            for k in 重 {
                self.站点表.remove(&k);
            }
        }
        // B0630：结构化站点表。`Session::compile` 写好的直接用；没写（不经 `Session` 的宿主）按 IR 现算，全部算非 lib
        self.站点键 = if program.site_keys.is_empty() {
            jpp_ir::site_key::site_keys(program, &[])
        } else {
            program.site_keys.clone()
        };
        // 放行把关只有一个来源：`Program.entry.guard`（意图汇编 11a），检查器读同一位
        self.guard = program.entry.guard;
        // 「无作者去向」站点（B0492 S2c）：检查器算好、编译时写进 IR
        self.默认链站点 = program.unsure_default_sites.clone();
        // B155（步 15i）：账本头记的渲染版本与本二进制不同时——只凭账本重放按旧版本算判断键（不发请求，
        // 旧账本照样命中），新头也写旧版本（重放写出的账本头与键一致，能再次重放），另报 `W-header` 说明
        // 本二进制的渲染版本不同；续接会发新请求，同一账本混两种渲染违反 B48，拒绝。依据：B155、B48、B30
        let mut 渲染差: Option<String> = None;
        if let Some(旧) = self
            .ledger
            .view()
            .header
            .as_ref()
            .map(|h| h.compared.render_version.clone())
            .filter(|r| r != RENDER_VERSION)
        {
            if self.audit.on {
                渲染差 = Some(format!(
                    "render_version 旧 {旧} 新 {RENDER_VERSION}（只凭账本重放按账本的渲染版本算键，读数来自旧线上形状）"
                ));
                self.render = 旧;
            } else {
                return Err(RtError::new(
                    Some("E-render-version"),
                    format!(
                        "账本的渲染版本是 {旧}，本二进制是 {RENDER_VERSION}：续接会用新渲染发请求，与账本里的旧读数混在一本账里（B48）。修法：用 --replay 只凭账本重放（按账本的渲染版本算键，不发请求），或不带账本重跑（依据：B155）"
                    ),
                    jpp_ir::ir::Span::default(),
                ));
            }
        }
        // B0630：按账本头的 `key_version` 选键法（预注册 §2.3）。不认识的值与续接旧键法账本报 `E-key-version`，
        // 在发任何请求之前；只凭账本重放旧键法账本按账本的键法算，头里照写它
        {
            let 视图 = self.ledger.view();
            let 头值 = 视图
                .header
                .as_ref()
                .and_then(|h| h.compared.key_version.clone());
            match jpp_ledger::key_version::choose(
                头值.as_deref(),
                !视图.entries.is_empty(),
                self.audit.on,
            ) {
                Ok(k) => self.key_version = k,
                Err(m) => {
                    return Err(RtError::new(
                        Some("E-key-version"),
                        m.trim_start_matches("E-key-version: ").to_string(),
                        program.span,
                    ));
                }
            }
        }
        // 依据：B77、J-18（只凭账本重放不比 `calib_hash`，续接两者都比）
        let 场合 = if self.audit.on {
            HeaderCompare::Replay
        } else {
            HeaderCompare::Resume
        };
        // 账本 v3（步 18a，B124）：头在 run() 入口定稿、之后不再改；命中的校准记录在 `CalibUsed` 条目，
        // 入口与当前校准视图逐键比（比对函数在 `jpp-ledger`，B77 的两种场合、B83 的报文都在那里）。
        let calib = self.calib;
        // C-3：进门收紧（审计重放取账本头里的余额，其余取宿主给的）；账本头的 `budget` 仍记程序声明，余额另记 `carry`
        let 声明 = self.budget.clone();
        // G4b 附录一：审计重放写回头时照抄原账本的 `depth_cap_default`（在换头之前取）
        let 旧头深度默认 = self
            .ledger
            .view()
            .header
            .as_ref()
            .and_then(|h| h.depth_cap_default);
        let 本趟 = self.进门收紧();
        // C-3 D1：这一趟没带余额、账本头记着本段余额时，新头照抄旧头的余额与段起点（不清掉）。这一趟不写 `Spent`，
        // 它的花费在下一趟被认成「未记」：下一趟带余额时按旧文件处理、补结清，整场上限不会因为漏传一趟而丢掉
        let 旧头余额 = if 本趟.is_none() {
            self.ledger
                .view()
                .header
                .as_ref()
                .and_then(|h| h.carry.clone().map(|c| (c, h.carry_from)))
        } else {
            None
        };
        // 本段的进门余额：交回余额按它从账本算（续跑时它是这一轮首趟的余额，不是本趟的剩余）
        let 进门余额 = 本趟.as_ref().map(|t| t.segment.clone());
        let 头告警 = self.ledger.open_run(
            Header::new(
                声明.calls,
                声明.cost,
                &self.model_id,
                // B155：只凭账本重放旧渲染的账本时写账本的版本（与键一致），其余是 `RENDER_VERSION`
                &self.render,
                HANDLER_VERSION,
            )
            .with_key_version(self.key_version.header_tag().map(String::from))
            // 头在 run() 入口定稿：档案是运行前就定下的输入，不该等跑完再补
            .with_profile_hash(self.calib.profile().hash.clone())
            .with_behavior_hash(self.calib.profile().behavior_hash.clone())
            // **运行开始那一刻的校准库**。这里没有「哪一刻」的选择余地：
            // `Interp` 拿的是 `&dyn CalibView`（只读），**一次运行之内它长不了**；
            // 增长只发生在两次运行之间（宿主拿 `Outcome.evidence` 去 `absorb`）。
            .with_calib_hash(Some(self.calib.hash()))
            // 宿主入口参数的哈希（B105-2，整份入口）；无入口时仍为 None（金样不变）
            .with_entry_hash(self.entry.hash())
            // 生成器模型与画像哈希（步 19）：宿主给了真实生成器才有，为空不写（金样不变）
            .with_gen(
                self.复用.gen_model.clone(),
                self.复用.gen_profile_hash.clone(),
            )
            // 标准库与题库版本（步 27，B48）：没装载时都为 None，账本逐字节不变
            .with_versions(
                self.复用.lib_version.clone(),
                self.复用.bank_version.clone(),
            )
            .with_carry(
                本趟
                    .as_ref()
                    .map(|t| t.segment.record().clone())
                    .or_else(|| 旧头余额.as_ref().map(|(c, _)| c.clone())),
                本趟
                    .as_ref()
                    .map(|t| t.from)
                    .or_else(|| 旧头余额.as_ref().and_then(|(_, f)| *f)),
            )
            // G4b（裁定六十四）：本趟用到引擎默认（带余额，或程序没声明 budget.depth）时记下它，重放以它为准；
            // 附录一：审计重放写回的头照抄原账本这一项（原来没有就不补写）
            .with_depth_cap_default(if self.audit.on {
                旧头深度默认
            } else {
                (本趟.is_some() || 声明.depth.is_none()).then_some(self.深度默认())
            }),
            场合,
            &|k: &str| calib.record_json(k),
        );
        // 头定稿即写出（文件后端：新头与已有条目整份原子写，B55，步 18b）；写不进去即停，一个效应都不发。
        // 渲染版本的差并进同一条 `W-header`（B155：只凭账本重放旧渲染时头里写的是账本的版本，比对不出它）
        match 头告警 {
            Ok(w) => {
                let w = match (w, 渲染差) {
                    (Some(w), Some(d)) => Some(format!("{w}；{d}")),
                    // 依据：B155、J-18
                    (None, Some(d)) => Some(format!("W-header: 账本头不同，不承诺重放一致：{d}")),
                    (w, None) => w,
                };
                if let Some(w) = w {
                    self.trace.warn(w);
                }
            }
            Err(e) => return Err(self.账本写不进(&e, program.span)),
        }
        // C-3：前一趟被杀、花费没被记上时，开跑前补一条结清（交回余额不会在后面的续跑里长回来）
        if let Err(Fault::Error(e)) = self.记结清(&本趟) {
            return Err(e);
        }
        // G4（裁定五十九第 7、17 条，裁定六十二第 1 条）：上游传来的深度已到上限时不再「整轮不开跑」——`进门收紧` 置
        // `深度停`，这一趟照常求值，判断一律不发（`Unsure(depth)`、`Unasked`、`Stop`），效应产出失败值，欠账照常结算
        // `budget.escalate` 是**问人的总次数上限**，不是「每次运行 k 次」（12:177、:180
        // 「恢复 = 从头重跑…ask 的答案作为账本条目参与重放」）。核上限时要把账本里**已经问过**的
        // 那些算进去——否则上限 2 在三轮恢复里能问到 6 次人。问人是最贵的效应，这个方向是多花钱。
        //
        // 但**不能把它们计进 `cost.asks`**：那个字段报的是「这次运行实际问了几次人」，
        // 重放时本来就该是 0（CLI 的 library_lifecycle 正是这么断言的，它是对的）。
        // 所以另存一个「账本里已有多少次」，只参与核上限，不进 Cost。
        self.asks_in_ledger = self
            .ledger
            .view()
            .entries
            .iter()
            // 已问未答的不算（步 7 起它们也入账；上限数的是得到回答的问人次数，与入账前一致）
            .filter(|e| {
                matches!(
                    e,
                    Entry::Ask {
                        answer: Some(_),
                        ..
                    }
                )
            })
            .count() as u64;
        self.frames.push(Frame {
            name: "<program>".into(),
            exits: vec![],
            cuts: vec![],
            returns_exit: true,
            过桥: 0,
            主人: crate::帧主人::已算(String::new()),
        });
        let env = env_child(&root_env());
        // 宿主入口参数（B105）：绑定在程序最外层之外，程序自己的同名 `let` 照常遮蔽它。
        // 值条目按读出规则（叶子 taint = 宿主声明）；材料条目盖 origin = input。
        // `purpose` 以名字 `purpose` 绑定为不可信 Text（B105-3），先于值条目。B105-3 的放行论证以题面
        // taint（B58）为前提：目的文本填进题面，切出的出口不可信，不能单独放行不可逆 `do`。步 17b 两者同落。
        // 依据：B105（地基/附注/2026-09-25-B105-B106裁定.md §一、§二）、B58
        if let Some(p) = &self.entry.purpose {
            env_define(
                &env,
                "purpose",
                Value::Text(Rc::from(p.as_str()), Taint::Untrusted.into()),
            );
        }
        for v in &self.entry.values {
            env_define(
                &env,
                &v.name,
                json_to_value(&v.value).with_prov(&jpp_value::prov::Provenance::from(v.taint)),
            );
        }
        for m in &self.entry.materials {
            let bound = m.bound_mat();
            self.entry_mat_names
                .insert(bound.hash.clone(), m.name.clone());
            env_define(&env, &m.name, Value::Mat(Rc::new(bound)));
        }
        // 伴随题序言（B0492 S5，主控 2026-09-30 路 A）：程序体之前求值，取标准题式
        // C2b：序言求值之后开程序单元的这一次尝试（序言不进单元图，3.8；每趟开新宿主纪元，C2a 复核第 1 条）
        let result = self
            .求值序言()
            .map(|_| self.单元开尝试(program))
            .and_then(|_| self.eval_block(&program.body, &env))
            .and_then(|v| {
            // 刷新点：程序结束（登记了却没人读的判断，到这里也要发出并记账）
            self.flush("end")?;
            // 程序返回前解析顶层帧的惰性出口（B94：返回值离开程序是检视点）
            self.解析本帧()?;
            // B160：程序结束收齐在飞的生成（结束的刷新已把登记着的交出），层内条目按登记序入账
            self.收层()?;
            // G2（B200）：按有无违规结算推迟的不可逆 `do`。随返回值交出它的值不算「同一次运行里读它」——那是把要做的事
            // 写进结论；结算后句柄填上动作的产出（扣下的填失败值），返回值里看得到
            let 有违规 = self.预判违规(&v);
            self.结算推迟(有违规)?;
            // 推错的那些：推测花了调用、花了预算，**花掉的必须留痕**。只报「发出、跨状态、没用上」三者同时成立的
            // （B51-C1、`21`:291，步 22 / B0487）：`speculated` 由刷新维护为发出了的跨状态推测键（`flush.rs`），
            // 同状态并进真站点调用的、因预算没发出的不在里面。例子按键排序，报文不随哈希集迭代序变
            let mut 没用上: Vec<&String> = self.speculated.iter().filter(|k| !self.speculation_used.contains(*k)).collect();
            没用上.sort();
            if !没用上.is_empty() {
                let n = 没用上.len();
                let 例 = 没用上.iter().take(3).map(|k| 头(k, 8)).collect::<Vec<_>>().join(", ");
                self.trace.warn(format!("W-spec-unused: 推测了 {n} 个站点没被走到（{例}…）：这些调用花掉了，结果留在账本里但程序没用上"));
            }
            Ok(v)
        });
        // 声明线证据定稿与 W-declared-line（B128，步 20j-1）：成功与挂起两条路都要
        self.声明证据定稿();
        // 账本里的声明线站点在程序里已无声明（B142，步 20j-1）
        self.声明站点核对(program);
        // 运行结束也落盘一次（B55，步 18b）：正常返回与挂起时写不进去即 `E-ledger-io`；出错时照报原错
        // （宿主收尾时会再写一次，那次写不进去由宿主报）。挂起与出错两条路先收齐在飞的生成（B160，
        // 与下面两臂原有的收层同一件事，提前到落盘之前），这一趟的条目才都在这次落盘里
        let result = match result {
            Err(Fault::Error(e)) => {
                let _ = self.收层();
                // G2 附录三（Z0564）：没走到结算的推迟动作写 Withheld(error)；写不进去不盖过原错
                let _ = self.扣下推迟(jpp_ledger::WithheldCause::Error);
                // G5 附录二：趟标记（三条路都写）
                self.写趟标记();
                let _ = self.层末落盘();
                Err(Fault::Error(e))
            }
            r @ Err(Fault::Halt(_)) => {
                let _ = self.收层();
                // G2 附录三（Z0564）：挂起时没走到结算的推迟动作写 Withheld(suspended)，续接时照常再推迟
                self.扣下推迟(jpp_ledger::WithheldCause::Suspended)
                    .and_then(|_| {
                        self.写趟标记();
                        self.层末落盘()
                    })
                    .and(r)
            }
            r => {
                self.写趟标记();
                self.层末落盘().and(r)
            }
        };
        // 步 19：复用计数先取出（下面两臂会把 self 的字段移走）
        let 复用 = self.复用统计();
        // C-3：交给下一轮的余额（下面两臂会把 self 的字段移走，先算）
        // 本轮花费进账本（三条路都写；出错这条路写不进也照报原错），交回余额从账本算——与失败后、重放时同一个算法
        // Z0384 R11：账本头记着本段余额、这一趟没带余额时也写（照抄了旧头），这一趟的花费照记进本段
        if (进门余额.is_some() || 旧头余额.is_some())
            && let Err(Fault::Error(e)) = self.记花费(true)
            && !matches!(result, Err(Fault::Error(_)))
        {
            return Err(e);
        }
        let 交回 = 进门余额.as_ref().map(|c| c.after_round(self.ledger.view()));
        // 伴随题的报告段（B0492 S5）：下面两臂会把 self 的字段移走，先算
        let 伴随报告 = self.伴随报告();
        let 点名报告 = if self.点名无取法.is_empty() {
            Json::Null
        } else {
            serde_json::to_value(&self.点名无取法).unwrap_or(Json::Null)
        };
        // Z0918：超窗次数；画像没测窗口时不核窗口，计数无意义，报告不出
        let 超窗报告 = if self.calib.profile().window().is_some() {
            serde_json::json!({"text": self.超窗计数[0], "ctx": self.超窗计数[1], "group": self.超窗计数[2]})
        } else {
            Json::Null
        };
        match result {
            Ok(v) => {
                let frame = self.frames.pop().unwrap();
                let mut in_value = HashSet::new();
                collect_exit_ids(&v, &mut in_value);
                // B162：责任按账本键计，同键的另一个持有者在返回值里或键已解除即有去向
                let 值键 = crate::duty::值里的键(&v);
                let mut returned = vec![];
                // C-1：随返回值交到程序结果的未决，按 frame.exits 的顺序记 Handoff
                let mut 交出: Vec<Rc<Exit>> = vec![];
                for e in frame
                    .exits
                    .iter()
                    .filter(|e| e.is_unsure() && !e.consumed.get())
                {
                    if !in_value.contains(&e.id) {
                        match self.键的去向(e, &值键) {
                            Some(true) => {
                                e.consumed.set(true);
                                *e.consumed_by.borrow_mut() = "view:已解除".into();
                                continue;
                            }
                            // 同键的持有者在返回值里：这份责任随返回值交出（B162）
                            Some(false) => {
                                e.consumed.set(true);
                                *e.consumed_by.borrow_mut() = "returned:view".into();
                                returned.push(e.label());
                                交出.push(e.clone());
                                continue;
                            }
                            None => {}
                        }
                    }
                    if in_value.contains(&e.id) {
                        e.consumed.set(true);
                        *e.consumed_by.borrow_mut() = "returned".into();
                        returned.push(e.label());
                        交出.push(e.clone());
                    } else {
                        // G2（`12` R9 单次形态，主控定第 1、2 条）：不再报运行期 J-05，记一笔违规，值照带
                        let e = e.clone();
                        self.记一笔违规(&e);
                    }
                }
                // C-1：Handoff 写在最后一次落盘之后，这里补落一次；G2：违规写在转交之后，扣下的推迟动作写在违规之后
                // （附录三：Violation 在引用它的 Withheld 之前）
                if !交出.is_empty() || !self.违规.is_empty() || !self.推迟.is_empty() {
                    self.记转交(&交出);
                    self.写违规账();
                    if let Err(Fault::Error(e)) =
                        self.扣下推迟(jpp_ledger::WithheldCause::Violation)
                    {
                        return Err(e);
                    }
                    match self.层末落盘() {
                        Err(Fault::Error(e)) => return Err(e),
                        Err(Fault::Halt(_)) | Ok(()) => {}
                    }
                }
                self.drop_then_return(&v, &in_value);
                // C2b：程序单元这一次尝试有结论，按 R2 发布（只在内存里；违规已在上面记下，这一版落「未决（violation）」）
                self.单元结束尝试(Some(&v));
                let 单元 = self.单元统计();
                // 审计重放核去向事件（主控 2026-09-29，W-replay-duty）：程序结束的转交已写完之后核
                self.核重放去向();
                // J-05 默认链（B0492 S2）：仍未决的（记账放弃或转交）一趟报一次，带条数
                let 未决 = self
                    .默认链记录
                    .iter()
                    .filter(|r| r["end"] != "decided")
                    .count();
                if 未决 > 0 {
                    self.trace.warn(format!(
                        "W-unsure-default: {未决} 个未决没写去向，走了语言的默认链（问缺哪类信息、取来再判）仍拿不准：判过而拿不准的已记账放弃，没观察到的随值交出（值被丢掉则记违规）。逐条见报告 unsure_default 段。要自己定去向，给 handle 写 unsure 臂"
                    ));
                }
                if !returned.is_empty() {
                    self.trace
                        .warn(format!("returned_unsure: {}", returned.join(", ")));
                }
                // B162：带回的未决按键列持有者；只在有键被两个及以上持有者带回时列（单一持有者 returned_unsure 已说清）
                let 持有 = crate::duty::键的持有者(&v);
                let duties: Vec<serde_json::Value> = if 持有.values().any(|(_, h)| h.len() > 1) {
                    持有
                        .into_iter()
                        .map(|(k, (label, holders))| {
                            serde_json::json!({"key": k, "exit": label, "holders": holders})
                        })
                        .collect()
                } else {
                    vec![]
                };
                Ok(Outcome {
                    value: Some(v),
                    pending: vec![],
                    trace: self.trace,
                    cost: self.cost,
                    returned_unsure: returned,
                    duties,
                    layers: self.layers,
                    evidence: self.evidence,
                    exits: self.exit_grades,
                    questions: self.questions,
                    suspend_candidates: {
                        let mut v: Vec<String> = self.drift_reported.iter().cloned().collect();
                        v.sort();
                        v
                    },
                    budget: self.预算停.clone(),
                    cache: 复用,
                    carry: 交回,
                    selections: self.挑选记录,
                    unsure_default: std::mem::take(&mut self.默认链记录),
                    improve: 伴随报告,
                    window_over: 超窗报告.clone(),
                    named_unfetchable: 点名报告.clone(),
                    site_key_fallback: self.站点回退,
                    orders: std::mem::take(&mut self.并档记录),
                    violations: std::mem::take(&mut self.违规)
                        .into_iter()
                        .map(|v| v.report)
                        .collect(),
                    settle_failed: std::mem::take(&mut self.结算失败),
                    cells: 单元,
                })
            }
            Err(Fault::Halt(p)) => Ok({
                self.核重放去向();
                // C2b：挂起的这一次尝试没有结论，发布「进行中」
                self.单元结束尝试(None);
                let 单元 = self.单元统计();
                Outcome {
                    // 挂起等人工回答（ask）：已交出的生成先取回记账（步 15h-2），续接时从账本取
                    value: {
                        let _ = self.收层();
                        None
                    },
                    pending: vec![p],
                    trace: self.trace,
                    cost: self.cost,
                    returned_unsure: vec![],
                    duties: vec![],
                    layers: self.layers,
                    evidence: self.evidence,
                    exits: self.exit_grades,
                    questions: self.questions,
                    suspend_candidates: {
                        let mut v: Vec<String> = self.drift_reported.iter().cloned().collect();
                        v.sort();
                        v
                    },
                    budget: self.预算停.clone(),
                    cache: 复用,
                    carry: 交回,
                    selections: self.挑选记录,
                    unsure_default: std::mem::take(&mut self.默认链记录),
                    improve: 伴随报告,
                    window_over: 超窗报告.clone(),
                    named_unfetchable: 点名报告.clone(),
                    site_key_fallback: self.站点回退,
                    orders: self.并档记录,
                    violations: vec![],
                    settle_failed: vec![],
                    cells: 单元,
                }
            }),
            Err(Fault::Error(e)) => {
                // 运行期出错：已交出的生成照样收齐入账（钱已经花了，`13` §5；B160），没交出的不交；收层本身的错不盖过原错
                let _ = self.收层();
                Err(e)
            }
        }
    }

    /// 条目进账本、层末落盘（B55，步 18b）。端口报错先记下，下一次层末落盘时报 `E-ledger-io`。
    pub(crate) fn 账本追加(&mut self, e: Entry) {
        if let Err(err) = self.ledger.append(e, Durability::Layer) {
            self.账本错.get_or_insert(err);
        }
    }

    /// 即刻落盘的条目（B55：不可逆 `do` 的意向与结果）：先写出缓着的条目，再写这一条，落盘后才返回；
    /// 写不进去即 `E-ledger-io`。
    pub(crate) fn 即刻记账(&mut self, e: Entry, sp: Span) -> R<()> {
        self.ledger
            .append(e, Durability::Now)
            .map_err(|err| Fault::Error(self.账本写不进(&err, sp)))
    }

    /// 层末：缓着的条目落盘（每次刷新结束与运行结束各一次）。
    pub(crate) fn 层末落盘(&mut self) -> R<()> {
        let r = match self.账本错.take() {
            Some(e) => Err(e),
            None => self.ledger.end_layer(),
        };
        r.map_err(|e| Fault::Error(self.账本写不进(&e, Span::default())))
    }

    pub(crate) fn 账本写不进(&self, e: &LedgerError, sp: Span) -> RtError {
        // 依据：B55（20 v2 附录 B55 条：意向与不可逆动作的结果逐行落盘，先于动作）
        RtError::new(
            Some("E-ledger-io"),
            format!(
                "账本写不进存储：{e}。不可逆动作的写前意向要先落盘（B55），本次运行停在这里；已落盘的部分可以续接"
            ),
            sp,
        )
    }

    /// 三路过滤的求值（`05` §1）。返回每道题一份 `{question, act, ignore, unsure, unobserved, stopped}`。
    ///
    /// - 每个元素保留原值（`item`）、输入位置（`index`）、出口（`exit`）、未决原因（`cause`）、
    ///   以及经过的前几次过滤（`trail`，由上一次过滤的出口组成）。
    /// - **完整输入时 act / ignore / unsure 不漏、互斥**；同一元素出现两次就在流里出现两次（各带自己的 index），
    ///   同状态同题只问一次（账本同键去重）。
    /// - **预算提前停止**：没问到的元素进 `unobserved`，不混进 ignore 或 unsure；`stopped` 写明原因。
    ///   这里接住 budget 停机是因为「部分观察 + 未观察范围」本身就是这个算子的一个合法结果；
    ///   之后的判断照常受预算约束。
    /// - 输入元素若本身是一次过滤的产物（带 `item` / `exit` / `trail` 的记录），取它的 `item` 当材料，
    ///   `trail` 接上：**产物与输入同形，可再过滤**（组合封闭）。
    /// - act / ignore 出口在这里被路由，记为已消费；unsure 出口放进 unsure 流，责任随返回值转交（J-05）。
    /// 构造一个契约值：经 `jpp_value::contract::build`（唯一构造，核 B17 两条不变量）。
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn outcome_value(
        kind: &str,
        value: Value,
        pending: Vec<Value>,
        evidence: Vec<Value>,
        resume: Value,
        spent: (i64, f64),
        detail: Value,
        purpose: Value,
        sp: Span,
    ) -> R<Value> {
        jpp_value::contract::build(jpp_value::contract::Parts {
            kind: kind.to_string(),
            value,
            pending,
            evidence,
            resume,
            spent,
            detail,
            purpose,
        })
        // 依据：B17（内置构造交出的契约值不变量不成立即运行时缺陷，按规则编号报出，不静默放过）
        .map_err(|r| Fault::Error(RtError::new(Some(r.rule), r.message, sp)))
    }

    /// 未决清单的一项：`{element, exit, cause}`
    pub(crate) fn pending_entry(element: Value, exit: &Value) -> Value {
        jpp_value::contract::pending_entry(element, exit)
    }

    /// 本次调用前的记账位置：之后据此算 `spent`，并从 trace 里取本构造触发的判断账本键
    pub(crate) fn mark(&self) -> (usize, u64, f64) {
        (self.trace.events.len(), self.cost.calls, self.cost.usd)
    }

    /// 这一段里判断过的账本键，以及它们花掉的调用与费用。
    /// **按账本记录算，不按本次实际发出的算**：重放时读出同样的调用号与费用，
    /// 契约值因此逐字节不变（J-18）。一次调用里融合了多道题，只计一次。
    pub(crate) fn since(&self, m: (usize, u64, f64)) -> (Vec<Value>, (i64, f64)) {
        let mut keys: Vec<String> = vec![];
        for e in &self.trace.events[m.0.min(self.trace.events.len())..] {
            if jpp_effects::by_name(&e.kind).is_some_and(|s| s.produces_reading)
                && !keys.contains(&e.key)
            {
                keys.push(e.key.clone());
            }
        }
        let mut calls: Vec<u64> = vec![];
        let mut usd = 0.0;
        for (i, k) in keys.iter().enumerate() {
            // 步 15h-3：先查开着的层（层里的判断条目同样算进花费；B160）
            if let Some(Entry::Judge { call, cost, .. }) = self.账本查(k) {
                // 老账本没有调用号：每个键算一次调用（上界）
                let id = if *call == 0 {
                    u64::MAX - i as u64
                } else {
                    *call
                };
                if !calls.contains(&id) {
                    calls.push(id);
                    // 合批只在首条记费（L7 2026-09-28）：有调用号的按整次调用的费用计，不看这一条自己的 cost
                    usd += if *call == 0 {
                        *cost
                    } else {
                        self.调用费(*call).max(*cost)
                    };
                }
            }
        }
        let _ = m.1;
        (
            keys.into_iter().map(|k| Value::text(&k)).collect(),
            (calls.len() as i64, usd),
        )
    }

    /// 输入若是契约值：取出 (产出列表, 未决清单, 证据)；否则 (原列表, 空, 空)。
    /// 契约值的产出不是列表时报错——只有列表产出能交给吃集合的构造。
    pub(crate) fn unpack(
        &self,
        v: &Value,
        who: &str,
        sp: Span,
    ) -> R<(Vec<Value>, Vec<Value>, Vec<Value>)> {
        if is_outcome(v) {
            let items = match v.get("value") {
                Some(Value::List(l)) => l.iter().cloned().collect(),
                other => {
                    return err(
                        Some("E-rt-arg"),
                        format!(
                            "{who} 收到的契约值产出不是列表（是 {}），不能按集合处理；取出其中的列表再交给 {who}",
                            other.map(|x| x.type_name()).unwrap_or("Unit")
                        ),
                        sp,
                    );
                }
            };
            return Ok((items, list_of(v.get("pending")), list_of(v.get("evidence"))));
        }
        match v {
            Value::List(l) => Ok((l.iter().cloned().collect(), vec![], vec![])),
            other => err(
                Some("E-rt-arg"),
                format!("{who} 要列表或契约值，收到 {}", other.type_name()),
                sp,
            ),
        }
    }
}
