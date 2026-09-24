//! 刷新：按状态分组成层、同键只问一次、调用客户端、写账本、填答案（20 §2.3 `flush.rs`）。
//! 步 4a 从 `interp.rs` 机械拆出，只搬代码、不改逻辑（21 §三·3）。

use super::*;

impl<'a> Interp<'a> {
    /// **写答案的唯一入口**（`20` §2.3「只有 flush 能填答案」，登记为 CI grep：`scripts/grep_fill.py`）。
    /// 刷新发出的一层与登记时的账本命中都经这里；读答案的唯一入口是 `readings.rs::answer_of`。
    pub(in crate::interp) fn fill_answer(&self, r: &Reading, a: Answer) {
        self.answers.borrow_mut().insert(r, a);
    }

    /// 从一条账本判断记录填读数：答案与置换测量一起填。
    ///
    /// 置换测量（`perms`, `mode_share`）是读数的一部分，出口由它决定 `Pick` / `tie` / `untested`（J-15）。
    /// 只填答案、不填测量，同一条读数在首发处是 `pick`/`band`，在账本命中处（重放、续跑、推测后真站点）
    /// 就成了 `untested`。所以凡是从账本或从同键首发取答的地方，都经这里。
    pub(in crate::interp) fn fill_from_record(
        &self,
        r: &Reading,
        a: Answer,
        perm: Option<crate::ledger::PermMeasure>,
    ) {
        if let Some(pm) = perm {
            r.set_mode_share(pm.mode_share, pm.perms);
        }
        self.fill_answer(r, a);
    }

    /// 刷新点（`12` §2.2:129）：「被 `cut`、`fit`、`match`/`if`、或宿主读内容时刷新。刷新时把所有
    /// 已登记且输入就绪的 `judge` **按状态分组**、按依赖分层，**一层一次发出**」。
    ///
    /// 这里的分层是天然的：登记发生在求值途中，依赖前一条出口的判断只可能在前一次刷新**之后**
    /// 才登记得上（要拿到出口就得先 `cut`，而 `cut` 本身就是刷新点）。所以「一次刷新 = 一层」，
    /// 层内按状态哈希分组融合——与 Python `_calls_from_plans` 的「同状态哈希」同一条规则。
    pub(in crate::interp) fn flush(&mut self, reason: &str) -> R<()> {
        debug_assert!(
            refresh_point(reason).is_some(),
            "刷新点 {reason} 未登记在 readings.rs::REFRESH_POINTS"
        );
        // 审计重放（B35）：首跑在调用之后核预算（「停的是下一步」）；重放从账本取答不经过那一处，
        // 在这里补核——记过的费用已超预算，就在最近一次取答的站点停，与首跑同一处。
        if self.audit.on && self.cost.usd + self.audit.usd > self.budget.cost {
            if let Some(site) = self.audit.last_site {
                self.charge(0, f64::default(), site)?;
            }
        }
        if self.pending.is_empty() {
            return Ok(());
        }
        let pending = std::mem::take(&mut self.pending);
        // 融合 pass（12 §4 序 2）：同状态、同层的题合成一次调用（P5 / 12 §10 G2）。
        // **关掉就逐题发**——这正是 §4 表里「不做会坏什么：E8 成本 +45%」那一栏要量的东西。
        let mut order: Vec<String> = vec![];
        let mut groups: HashMap<String, Vec<PendingJudge>> = HashMap::new();
        let fuse = self.plan.fuse;
        for (idx, p) in pending.into_iter().enumerate() {
            // 不融合时每道题自成一组：键上带题序，保证互不合并
            let h = if fuse {
                p.state.hash.clone()
            } else {
                format!("{}#{idx}", p.state.hash)
            };
            if !groups.contains_key(&h) {
                order.push(h.clone());
            }
            groups.entry(h).or_default().push(p);
        }
        // 超预算先丢推测的，再动真站点（推测本来就是可放弃的）
        if self.cost.calls + self.audit.calls >= self.budget.calls {
            groups
                .values_mut()
                .for_each(|g| g.retain(|p| !p.speculative));
        }
        let mut layer_calls = 0u64;
        let mut layer_questions = 0usize;
        for h in &order {
            let group = groups.remove(h).expect("刚放进去的");
            let state = group[0].state.clone();
            let site = group[0].site;
            let only_speculative = group.iter().all(|p| p.speculative);
            let mut items: Vec<(Rc<Question>, Rc<Reading>, String)> =
                group.into_iter().flat_map(|p| p.items).collect();
            if items.is_empty() {
                continue;
            }
            // 关掉融合时，同一次 judge 登记的多道题也要逐题发——否则「一状态多题」这一条
            // 仍然在融合，关掉的只是「跨登记合并」，量出来的省钱会偏小
            if !fuse && items.len() > 1 {
                let rest = items.split_off(1);
                for (q, r, k) in rest {
                    self.pending.push(PendingJudge {
                        state: state.clone(),
                        items: vec![(q, r, k)],
                        site,
                        speculative: false,
                    });
                }
            }
            // **同一个账本键只问一次。**
            //
            // 提前登记（`speculate` / `vectorize`）与真站点会登记同一个键：实测
            // `vectorize` 接上之后，`[1,1,1]` 变成 `[2,2,1]`——**同一道题付了两次钱**。
            // 这不是 `vectorize` 独有的，`speculate` 也走同一条路，只是以前没量到。
            //
            // 去重后**每个读数都要填上答案**：真站点那份和提前登记那份是两个 `Reading`
            // 对象，只填一个，另一个会停在「没有答案」上。
            let mut 首见: HashMap<String, usize> = HashMap::new();
            let mut 同键: Vec<Vec<Rc<Reading>>> = vec![];
            let mut 去重: Vec<(Rc<Question>, Rc<Reading>, String)> = vec![];
            for (q, r, k) in items.into_iter() {
                match 首见.get(&k) {
                    Some(i) => 同键[*i].push(r),
                    None => {
                        首见.insert(k.clone(), 去重.len());
                        同键.push(vec![r.clone()]);
                        去重.push((q, r, k));
                    }
                }
            }
            let items = 去重;
            // **缺席 / 超时的重放**（B32、B35）：账本里记过这一组题的缺席事件，照记的给出，不再发。
            // 首跑的失败尝试按记的次数计入审计重放的预算（逐次计费，B32 裁定选项 A）。
            let 已记: Vec<Option<(String, String, u64)>> = items
                .iter()
                .map(|(_, _, k)| match self.ledger.get(&format!("absent:{k}")) {
                    Some(Entry::Absent {
                        cause,
                        detail,
                        attempts,
                        ..
                    }) => Some((cause.clone(), detail.clone(), *attempts)),
                    _ => None,
                })
                .collect();
            if 已记.iter().all(|x| x.is_some()) {
                let 记: Vec<(String, String, u64)> = 已记.into_iter().flatten().collect();
                let (首因, 首详, _) = 记[0].clone();
                let 尝试: u64 = 记.iter().map(|x| x.2).sum();
                let 策略 = self.budget.absent.clone();
                let 处置 = 策略.as_ref().map(|p| p.then.clone()).unwrap_or_default();
                // 续跑（非审计）时，因预算停机、挂起或报错的站点要重新发：它们当时没有得到答案
                let 重发 = !self.audit.on
                    && (首因 == "budget" || (首因 == "absent" && 处置 != "conservative"));
                if !重发 {
                    if self.audit.on {
                        self.audit.calls += 尝试;
                        self.audit.last_site = Some(site);
                    }
                    if 首因 == "budget" {
                        // 首跑在重试中途因预算停机：重放在同一站点同样停机
                        self.charge(1, 0.0, site)?;
                        return Err(self.replay_missing(format!("预算停机站点 {首详}"), site));
                    }
                    if 首因 == "absent" && 处置 != "conservative" {
                        let pol = 策略.expect("刚判过");
                        return self.缺席处置(&items, &pol, site, 首详, 0).map(|_| ());
                    }
                    for ((_, _, k), (c, _, _)) in items.iter().zip(记) {
                        self.absent_marks.insert(k.clone(), c);
                        self.cost.replayed += 1;
                    }
                    continue;
                }
            }
            // **时延预算已用完**（B32）：之后的判断站点转 `Unsure(latency)`，不静默继续，也不再发
            if let Some(lim) = self.budget.latency_p95 {
                if self.latency_spent > lim {
                    self.记缺席(
                        &items,
                        "latency",
                        site,
                        format!("时延预算 {lim}s 已用完（已用 {:.2}s）", self.latency_spent),
                        0,
                    );
                    continue;
                }
            }
            // **熔断**（B32）：连续缺席到上限后不再发
            if let Some(pol) = self.budget.absent.clone() {
                if self.consecutive_absent >= pol.breaker {
                    self.缺席处置(
                        &items,
                        &pol,
                        site,
                        format!("熔断：连续缺席 {} 次", self.consecutive_absent),
                        0,
                    )?;
                    continue;
                }
            }
            // 审计重放：首跑没发过的推测登记照样不发（首跑可能因预算丢掉了它们）
            if self.audit.on && only_speculative {
                continue;
            }
            self.charge(1, 0.0, site)?;
            if self.audit.on {
                let texts: Vec<&str> = items.iter().map(|(q, _, _)| q.text.as_str()).collect();
                return Err(self.replay_missing(format!("judge「{}」", texts.join("|")), site));
            }
            let ask: Vec<&Question> = items.iter().map(|(q, _, _)| q.as_ref()).collect();
            let 起 = std::time::Instant::now();
            // **每次发出都计费**（B32，主会话裁定选项 A）：首发已在上面核过预算，这里记一次；
            // 重试前各核一次预算，每次发出（无论成败）都计入 `cost.calls`。
            let mut 结果 = self.client.judge(&state, &ask);
            self.cost.calls += 1;
            let mut 尝试 = 1u64;
            // **重试与退避**（B32）：只有声明了 absent 策略才重试；没声明沿用旧行为（客户端错误即运行期错误）
            if let Some(pol) = self.budget.absent.clone() {
                let mut 等 = pol.backoff;
                let mut 次 = 0;
                while 结果.is_err() && 次 < pol.retry {
                    if 等 > 0.0 {
                        std::thread::sleep(std::time::Duration::from_secs_f64(等));
                    }
                    等 *= 2.0;
                    次 += 1;
                    if let Err(f) = self.charge(1, 0.0, site) {
                        // 重试中途预算用完：已花的时延与尝试入账，停机（重放在同一站点停，B35）
                        self.latency_spent += 起.elapsed().as_secs_f64();
                        self.记缺席账(&items, "budget", &format!("@{}", site.start), 尝试);
                        return Err(f);
                    }
                    结果 = self.client.judge(&state, &ask);
                    self.cost.calls += 1;
                    尝试 += 1;
                }
                // 失败路径也计时延：退避睡眠与各次失败请求都占时延预算
                let 用时 = 起.elapsed().as_secs_f64();
                self.latency_spent += 用时;
                if let Err(e) = &结果 {
                    self.consecutive_absent += 1;
                    self.缺席处置(
                        &items,
                        &pol,
                        site,
                        format!("判断器不可用（重试 {次} 次）：{}", e.0),
                        尝试,
                    )?;
                    continue;
                }
                self.consecutive_absent = 0;
            } else {
                let 用时 = 起.elapsed().as_secs_f64();
                self.latency_spent += 用时;
            }
            let res = 结果.map_err(|e| {
                Fault::Error(RtError::new(
                    Some("E-rt-client"),
                    format!("客户端错误：{}", e.0),
                    site,
                ))
            })?;
            if res.answers.len() != ask.len() {
                return err(Some("E-rt-answer"), "客户端返回的答案数与题数不符", site);
            }
            // 13 §5：后端已经返回 = 调用已经发生、钱已经花了。先把事实记下来，再决定要不要继续。
            // （调用次数在发出时已计，含失败的重试）
            self.cost.tokens += res.tokens;
            self.cost.usd += res.cost;
            layer_calls += 1;
            layer_questions += items.len();
            let shares = res.mode_share;
            let res_perms = res.perms;
            // 一次调用里不止一道题 = 同状态合并发出（融合），账本条目记下是谁合并的（D8.2）
            let merged = items.len() > 1;
            for (idx, ((q, r, key), a)) in items.iter().zip(res.answers.into_iter()).enumerate() {
                // perms 跟着读数走：改 K 产生**新键**而不是覆盖旧值，两边并存
                // ——与「线重算之后已经发出的出口不改」是同一条纪律。
                // 测量与答案一起进账本（INTERFACE §四·二·七·五），重放与同键复用从账本取回。
                let perm = match shares.get(idx) {
                    Some(Some(ms)) => Some(crate::ledger::PermMeasure {
                        perms: res_perms.get(idx).copied().unwrap_or(0),
                        mode_share: *ms,
                    }),
                    _ => None,
                };
                // 测量先落到读数上：下面的运行期证据（`evidence`）从读数取 `perms`/`mode_share`
                if let Some(pm) = perm {
                    r.set_mode_share(pm.mode_share, pm.perms);
                }
                self.validate_answer(&a, q, &state, site)?;
                // B59（步 17a）：跳按依赖计。parents = 状态的来源读数 ∪ 题的来源出口（排序去重），
                // hop = 1 + max(父条目的 hop)，无父为 1；父条目不在账本或不是判断条目按 0 计。
                // 只数结构通道（下界），经普通值的依赖待候选 B84。
                let parents: Vec<String> = state
                    .parents
                    .iter()
                    .chain(q.from_key.iter())
                    .cloned()
                    .collect::<BTreeSet<String>>()
                    .into_iter()
                    .collect();
                let hop = 1 + parents
                    .iter()
                    .map(|p| match self.ledger.get(p) {
                        Some(Entry::Judge { hop, .. }) => *hop,
                        _ => 0,
                    })
                    .max()
                    .unwrap_or(0);
                self.ledger.put(Entry::Judge {
                    key: key.clone(),
                    jkey: self.judge_keys.get(key).cloned(),
                    answer: a.clone(),
                    tokens: res.tokens,
                    cost: res.cost,
                    model_id: self.model_id.clone(),
                    call: self.cost.calls,
                    calib_ref: Some(CalibRef {
                        declared: r.calib.clone(),
                    }),
                    layer: self.layers.len() as u32 + 1,
                    merged_by: if merged { Some("fuse".into()) } else { None },
                    parents,
                    hop,
                    reused_from: None,
                    perm,
                });
                self.trace.push(
                    "judge",
                    key,
                    false,
                    res.cost,
                    site,
                    format!("「{}」", q.text),
                );
                // **运行期写入口的产出端**（`12`:347）。挂在这里而不是挂在「有读数产生」上，
                // 是因为重放路径（`interp.rs` 的 `ledger.get` 分支）根本不经过这里——
                // **重放于是天然不重复计数**，不需要再加一个「是不是重放」的开关。
                self.evidence.push((
                    r.calib.clone(),
                    crate::effects::Sample {
                        p: match &a {
                            Answer::Noul(p) => Some(*p),
                            _ => None,
                        },
                        // 读数本身没有真值：真值通道是 `12`:347 未定的另一样
                        label: None,
                        perms: r.perms.get(),
                        mode_share: r.mode_share.get(),
                        mode: crate::effects::LiteralMode::default(),
                        phys: q.op.phys().to_string(),
                        // 运行期这条路上没有簇 id：**读数不知道自己属于哪个对象段**。
                        // 留 `None`，于是它只能参与「按条」的认证——而「按条」会被如实写进证书。
                        cluster: None,
                        stratum: None,
                    },
                ));
                self.fill_from_record(r, a.clone(), perm);
                // 同键的其余读数（提前登记那些）也要填上，否则它们停在「没有答案」；
                // 置换测量一起填，否则它们的出口停在 `untested`
                for other in 同键[idx].iter().skip(1) {
                    self.fill_from_record(other, a.clone(), perm);
                }
            }
            // **超时站点**（B32）：这一次调用把累计时延推过预算，本组题转 `Unsure(latency)`（答案已记账，但不采信）
            if let Some(lim) = self.budget.latency_p95 {
                if self.latency_spent > lim {
                    self.记缺席(
                        &items,
                        "latency",
                        site,
                        format!(
                            "本次调用后累计时延 {:.2}s 超过预算 {lim}s",
                            self.latency_spent
                        ),
                        0,
                    );
                }
            }
            // 事实记完了再核预算：实际费用高于调用前的估计时，停的是**下一步**，不是这一步
            self.charge(0, 0.0, site)?;
        }
        if layer_calls > 0 {
            self.layers.push(Layer {
                reason: reason.to_string(),
                calls: layer_calls,
                questions: layer_questions,
            });
        }
        // 关融合时拆出来的余项，接着发（它们同属这一层，只是各自一次调用）
        if !self.pending.is_empty() {
            return self.flush(reason);
        }
        Ok(())
    }

    pub(in crate::interp) fn validate_answer(
        &self,
        a: &Answer,
        q: &Question,
        s: &State,
        sp: Span,
    ) -> R<()> {
        match (a, q.op) {
            (Answer::Noul(p), Op::Test) if (0.0..=1.0).contains(p) => Ok(()),
            (Answer::Choice(v), Op::Select) if v.len() == s.over.len() => Ok(()),
            (Answer::Score(v), Op::Measure) if v.len() == q.scale.len() => Ok(()),
            _ => err(
                Some("E-rt-answer"),
                format!(
                    "答案形状与题不符：{:?} vs {}（over {} / 档位 {}）",
                    a,
                    q.op.phys(),
                    s.over.len(),
                    q.scale.len()
                ),
                sp,
            ),
        }
    }
}
