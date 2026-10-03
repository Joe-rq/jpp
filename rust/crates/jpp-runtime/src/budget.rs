//! 预算核、审计重放折算、判断力缺席的处置（20 §2.3 `budget.rs`；B32、B35）。
//! 步 4a 从 `interp.rs` 机械拆出，只搬代码、不改逻辑（21 §三·3）。

use super::*;

/// 网络类错误的隐含缺席策略（现场稳定性三修 (1)，`地基/过程记录/工程-现场稳定性三修.md`）：重试次数与首次退避秒数。
/// 重试次数与规划的调用数上界共用 `jpp_ir::plan::IMPLICIT_RETRY`（步 22）
const 隐含重试: u32 = jpp_ir::plan::IMPLICIT_RETRY;
const 隐含退避秒: u32 = 1;

impl<'a> Interp<'a> {
    /// 计划期要的运行语境（步 22，B42 `PlanCtx`）：账本是否为空、是否开了跨运行缓存、判断器画像。
    /// 宿主（`jpp::interp::Interp::run`）在运行前据此算计划；运行时自己不判拒绝。
    pub fn plan_inputs(&self) -> (bool, bool, &jpp_effects::Profile) {
        (
            self.ledger.view().entries.is_empty(),
            self.复用.cross.is_none(),
            self.calib.profile(),
        )
    }

    /// 一组判断首发之后用哪条缺席策略：程序声明了 `budget.absent` 用声明的；没声明而首发报的是网络类错误
    /// （[`EffectError::is_network`]），用隐含策略——重试 2 次、退避 1 秒起翻倍、用尽转 `Unsure(absent)`、程序
    /// 照常，不熔断（熔断只看声明的策略）；其余返回 `None`，客户端错误照旧是运行期错误。
    pub(crate) fn 缺席策略(
        &self,
        首发错: Option<&EffectError>,
    ) -> Option<jpp_ir::ir::AbsentPolicy> {
        self.budget.absent.clone().or_else(|| {
            首发错
                .filter(|e| e.is_network())
                .map(|_| jpp_ir::ir::AbsentPolicy {
                    retry: 隐含重试,
                    backoff: f64::from(隐含退避秒),
                    then: "conservative".into(),
                    breaker: u32::MAX,
                })
        })
    }

    /// 开启审计重放（B35）：只凭账本重现首跑，缺记录即 `E-replay`。CLI 的 `--replay` 用它；`--resume` 不用。
    pub fn audit_replay(mut self) -> Self {
        self.audit.on = true;
        self
    }

    /// 设宿主入口参数（B105；步 14b 起取代 `with_host_input`）：`run` 入口把条目绑定进程序环境、
    /// 整份入口的哈希写进账本头 `entry_hash`。依据：B105（地基/附注/2026-09-25-B105-B106裁定.md）
    pub fn with_entry(mut self, entry: EntryArgs) -> Self {
        self.entry = entry;
        self
    }

    /// 审计重放：从账本取到一次记过的调用，按记录计入预算（融合的多道题按 `call` 只计一次；
    /// 老账本 `call == 0` 时按条计，是上界）。
    pub(crate) fn audit_account(&mut self, call: u64, usd: f64, sp: Span) {
        if !self.audit.on {
            return;
        }
        self.audit.last_site = Some(sp);
        // 合批的费用只记在首条（L7 2026-09-28）：同一调用号第一次读到时按整次调用的费用（`调用费`，同调用号取最大）
        // 计入，与读到的是哪一条无关；旧账本每条记整次费用，取最大也是同一个数
        if call == 0 {
            self.audit.calls += 1;
            self.audit.usd += usd;
        } else if self.audit.seen_calls.insert(call) {
            self.audit.calls += 1;
            self.audit.usd += self.调用费(call).max(usd);
        }
    }

    /// 审计重放遇到账本缺的记录：致命，不进 cause，不挂起（B35 第 1 条）。
    // 依据：B35（审计重放缺记录即 E-replay）
    pub(crate) fn replay_missing(&self, what: String, sp: Span) -> Fault {
        Fault::Error(RtError::new(
            Some("E-replay"),
            format!("重放缺账本记录：{what}。只凭账本重放不发新调用；要继续往下请用 --resume"),
            sp,
        ))
    }

    // ---------- 效应 ----------

    /// 预算核（B93，步 22-0）：只报超额，不停程序。超额时返回报文，由调用处决定怎么停发：
    /// 判断站点按缺席处置（`预算停发`），`gen`/`do` 产出失败值，`ask` 挂起。
    /// 审计重放时，账本里记过的调用照记录算进已花（B35）：首跑在哪里停发，重放就在哪里停发。
    pub(crate) fn charge(&self, calls: u64, usd: f64) -> Result<(), String> {
        // Z0565：守卫下推迟、还没结算的不可逆 `do` 同样算已用（否则几个推迟动作各自过核对、结算时一起执行超预算）
        let (推迟次, 推迟费) = self.推迟预留();
        let (used_calls, used_usd) = (
            // 步 15h-2（B160）：登记了还没交出的生成也算已用（交出时才计入 cost.calls）
            self.cost.calls + self.audit.calls + self.生成预留() + 推迟次,
            self.cost.usd + self.audit.usd + 推迟费,
        );
        if used_calls + calls > self.budget.calls || used_usd + usd > self.budget.cost {
            return Err(format!(
                "预算耗尽：calls {}+{} / {}，cost {:.6}+{:.6} / {:.6}",
                used_calls, calls, self.budget.calls, used_usd, usd, self.budget.cost
            ));
        }
        Ok(())
    }

    /// 账本里已记缺席的一组，本趟要不要重发（B32、B35、B93）：续跑（非审计）时，因预算停机、挂起或报错的站点
    /// 当时没有得到答案，要重发；审计重放照记录停在同一处。G5（裁定五十九第 16 条、六十一主控暂定 (b)，推翻 PR #48）：
    /// 缺席不论处置（含 conservative 与没声明策略时的隐含处置）都重发；`latency` 照旧不重发。
    /// `flush_before_send` 与层内挑选的预判共用这一条（预判少算会让续跑时推测抢在重发的真站点前面）。
    pub(crate) fn 缺席记录重发(&self, 首因: &str) -> bool {
        !self.audit.on && (首因 == "budget" || 首因 == "absent")
    }

    /// 这一组本层会不会真的发出去、付一次调用（层内挑选的预判，步 22 / B0487）。与 `flush_before_send` 的跳过
    /// 条件同口径：审计重放下只含推测的组不发；账本已记缺席且不重发的组不发。时延用完、熔断整层都不发，
    /// 由 [`Self::层内挑选`] 先判。
    fn 组会发出(&self, group: &[PendingJudge]) -> bool {
        let only_speculative = group.iter().all(|p| p.speculative);
        if self.audit.on && only_speculative {
            return false;
        }
        let mut 首因: Option<String> = None;
        for p in group {
            for (_, _, k) in &p.items {
                match self.账本查(&format!("absent:{k}")) {
                    // 推测组被放弃的记录只对只含推测的组生效（步 13a-1）
                    Some(Entry::Absent { cause, .. })
                        if cause.as_str() != "spec_miss" || only_speculative =>
                    {
                        首因.get_or_insert_with(|| cause.clone());
                    }
                    // G4 一·4：预算停发记为「未问」的题，与原先的 `Absent(budget)` 同口径
                    _ if self.未问(k) == Some("budget") => {
                        首因.get_or_insert_with(|| "budget".to_string());
                    }
                    // 有一道题没有缺席记录，这组就要发
                    _ => return true,
                }
            }
        }
        match 首因 {
            Some(c) => self.缺席记录重发(&c),
            None => false,
        }
    }

    /// 层内挑选（B43 的运行期落点，`20` §4.1 第 7 步、§4.3；B51-C1）：本层会真发的组超出剩余调用数时，经钩子
    /// `select_within` 排出发出顺序（真站点先、跨状态推测后），返回 `groups` 的新下标顺序；不超时返回 `None`，
    /// 发出集合与顺序与改前逐字节相同。停发点仍由逐组 `charge_after` 定（主控 Z0209 Q2 (b)）。
    ///
    /// `groups[i]` 是按登记位置排好的第 `i` 组。剩余额度与 `charge` 同口径（含审计计入与生成预留）。
    pub(crate) fn 层内挑选(&mut self, groups: &[&[PendingJudge]]) -> Option<Vec<usize>> {
        // 时延预算已用完、已熔断：整层都不发（缺席处置按原顺序入账），不重排
        if self
            .budget
            .latency_p95
            .is_some_and(|lim| self.latency_spent > lim)
        {
            return None;
        }
        if let Some(pol) = &self.budget.absent
            && self.consecutive_absent >= pol.breaker
        {
            return None;
        }
        let layer: Vec<jpp_ir::plan::PendingSite> = groups
            .iter()
            .enumerate()
            .map(|(pos, g)| {
                // 超窗裂变出的块站点按跨状态推测计（`20` v2 §4.5 第 3 条末句，步 23b）：不算真站点
                let 真键: HashSet<&String> = g
                    .iter()
                    .filter(|p| !p.speculative)
                    .flat_map(|p| p.items.iter().map(|(_, _, k)| k))
                    .filter(|k| !self.裂变块键.contains(*k))
                    .collect();
                let class = if 真键.is_empty() {
                    jpp_ir::plan::SiteClass::CrossStateSpec
                } else {
                    jpp_ir::plan::SiteClass::Real
                };
                // 同一次调用里捎带的推测题（同状态推测，B51-C1）：只按推测进组、不与真站点同键的
                let 捎带: HashSet<&String> = g
                    .iter()
                    .filter(|p| p.speculative)
                    .flat_map(|p| p.items.iter().map(|(_, _, k)| k))
                    .filter(|k| !真键.is_empty() && !真键.contains(k))
                    .collect();
                // 步 30 / B0488：规划目标要的输入。真站点组只看真站点成员，只含推测的组看全部成员
                let 成员: Vec<&PendingJudge> = g
                    .iter()
                    .filter(|p| class == jpp_ir::plan::SiteClass::CrossStateSpec || !p.speculative)
                    .collect();
                let downstream = 成员
                    .iter()
                    .filter_map(|p| p.site_id)
                    .filter_map(|s| self.plan.per_site.get(&s).map(|x| x.downstream))
                    .max()
                    .unwrap_or(0);
                // 每道题的键：`价值记录` 返回的记录键（线实际会命中的那条，计数也记在它上面，主控 B0488 缺口 12）；
                // 没有可用记录时取题自己的 `calib`（报告照列，价值为 0）
                let mut 键序: Vec<String> = vec![];
                let mut 键记录: Vec<Option<jpp_ir::plan::Channel>> = vec![];
                let mut 题键: HashMap<String, String> = HashMap::new();
                let mut 成员键: HashSet<&String> = HashSet::new();
                for p in &成员 {
                    for (q, _, k) in &p.items {
                        if !成员键.insert(k) {
                            continue;
                        }
                        let (key, rec) = match self.价值记录(q) {
                            Some((key, ch)) => (key, Some(ch)),
                            None => (q.calib.clone(), None),
                        };
                        题键.insert(k.clone(), key.clone());
                        if !键序.contains(&key) {
                            键序.push(key);
                            键记录.push(rec);
                        }
                    }
                }
                let keys: Vec<jpp_ir::plan::KeyCount> = 键序
                    .iter()
                    .zip(&键记录)
                    .map(|(key, rec)| {
                        let (seen, unsure) = self.键计数.get(key).copied().unwrap_or((0, 0));
                        jpp_ir::plan::KeyCount {
                            key: key.clone(),
                            seen,
                            unsure,
                            record: *rec,
                        }
                    })
                    .collect();
                // 各题的价值输入（按账本键去重）
                let mut 已: HashSet<&String> = HashSet::new();
                let values: Vec<jpp_ir::plan::ValueInput> = 成员
                    .iter()
                    .flat_map(|p| p.items.iter().map(move |it| (p, it)))
                    .filter(|(_, (_, _, k))| 已.insert(k))
                    .map(|(p, (q, _, lk))| {
                        let (request, k) = 请求与规模(q, &p.state);
                        jpp_ir::plan::ValueInput {
                            request,
                            k,
                            key: 键序
                                .iter()
                                .position(|x| Some(x) == 题键.get(lk))
                                .unwrap_or(0),
                        }
                    })
                    .collect();
                // 费用估计的字符数：一次调用发出的全部材料与全部（去重的）题
                let st = &g[0].state;
                let state_chars: usize = st
                    .on
                    .iter()
                    .chain(&st.ctx)
                    .chain(&st.r#ref)
                    .chain(&st.over)
                    .map(|m| m.text().chars().count())
                    .sum();
                let mut 全键: HashSet<&String> = HashSet::new();
                let question_chars: usize = g
                    .iter()
                    .flat_map(|p| p.items.iter())
                    .filter(|(_, _, k)| 全键.insert(k))
                    .map(|(q, _, _)| q.text.chars().count())
                    .sum();
                jpp_ir::plan::PendingSite {
                    pos,
                    class,
                    calls: u64::from(self.组会发出(g)),
                    same_state_spec: 捎带.len() as u32,
                    site: 成员.first().and_then(|p| p.site_id),
                    downstream,
                    questions: 成员键.len() as u32,
                    state_chars: state_chars as u64,
                    question_chars: question_chars as u64,
                    keys,
                    values,
                }
            })
            .collect();
        let 需: u64 = layer.iter().map(|p| p.calls).sum();
        let 已用 = self.cost.calls + self.audit.calls + self.生成预留();
        let left = jpp_ir::plan::BudgetLeft {
            calls: self.budget.calls.saturating_sub(已用),
            usd: (self.budget.cost - (self.cost.usd + self.audit.usd)).max(0.0),
        };
        if 需 <= left.calls {
            return None;
        }
        let sel = self.hooks.select_within(&self.plan, &layer, left);
        self.记挑选(groups, &layer, left, 需, &sel);
        Some(sel.order().collect())
    }

    /// 层内挑选的决定进报告（步 30 / B0488，主控 Q7/Q9）：组按钩子给的顺序列出；`send` 是钩子按剩余调用数切的，
    /// 实际停发点仍以逐组预算核为准。不进账本
    fn 记挑选(
        &mut self,
        groups: &[&[PendingJudge]],
        layer: &[jpp_ir::plan::PendingSite],
        left: jpp_ir::plan::BudgetLeft,
        需: u64,
        sel: &jpp_ir::plan::Selection,
    ) {
        let arm = match (
            self.plan.select_within,
            self.plan.critical_path,
            self.plan.value_density,
        ) {
            (false, _, _) => "registration",
            (true, false, false) => "order",
            (true, true, false) => "critical_path",
            (true, false, true) => "value_density",
            (true, true, true) => "critical_path+value_density",
        };
        let 发: HashSet<usize> = sel.send.iter().copied().collect();
        let rows: Vec<Json> = sel
            .order()
            .map(|i| {
                let p = &layer[i];
                let est = self
                    .plan
                    .cost_model
                    .map(|m| m.est_usd(p.state_chars, p.question_chars));
                serde_json::json!({
                    "pos": p.pos,
                    "class": match p.class {
                        jpp_ir::plan::SiteClass::Real => "real",
                        jpp_ir::plan::SiteClass::CrossStateSpec => "cross_state_spec",
                    },
                    // 与 `PendingSite.site` 同一条成员：真站点组取第一条真站点成员（不取可能是捎带推测的 `g[0]`）；
                    // `site_id` 与 EXPLAIN `per_site` 的键同一个数（复核 B0488-A 缺口 4）
                    "site": 代表(groups[i]).site.start,
                    "site_id": p.site.map(|s| s.0),
                    "questions": p.questions,
                    "downstream": p.downstream,
                    "same_state_spec": p.same_state_spec,
                    "est_usd": est,
                    // B 段：组的价值（价值密度开关关时为 null）；每个键的记录（u、ε、n）与本趟更新后的 u_run
                    "value": sel.value.get(i),
                    // 每个键：本趟计数、记录平滑后的未决比例 u 与总数 n、本趟更新后的 u_run（裁定四十六；无记录为 null）
                    "keys": p.keys.iter().enumerate().map(|(j, k)| {
                        let st = sel.key_stats.get(i).and_then(|v| v.get(j).copied().flatten());
                        serde_json::json!({
                            "key": k.key, "seen": k.seen, "unsure": k.unsure,
                            "u": st.map(|x| x.0), "n": st.map(|x| x.1), "u_run": st.map(|x| x.2),
                        })
                    }).collect::<Vec<_>>(),
                    "send": 发.contains(&i),
                })
            })
            .collect();
        self.挑选记录.push(serde_json::json!({
            "layer": self.layers.len() + 1,
            "left": {"calls": left.calls, "usd": left.usd},
            "need": 需,
            "arm": arm,
            "groups": rows,
        }));
    }

    /// 记一次停发（B93）：首次停发时出一条 `W-budget`（列首个未发站点），此后只计数。深度到限（G4）的停发同一条路，
    /// 报文与报告的 `cause` 按 `深度停` 分开。
    pub(crate) fn 记停发(&mut self, n: u64, site: Span, detail: &str) {
        // 本次刷新重排过：先攒，刷新结束按原登记下标定首个停发站点（`落定首停`）
        if self.预算停.is_none()
            && let Some(v) = &mut self.首停延后
        {
            v.push((self.当前组位, n, site, detail.to_string()));
            return;
        }
        match &mut self.预算停 {
            Some(b) => b.unsent += n,
            None => {
                // 依据：B93（`12` §3 J-07「停」指停发，不是停程序）；G4（裁定五十九第 7、17 条）深度到限同理
                let cause = if self.深度停 {
                    self.trace.warn(format!(
                        "W-budget: @{} 深度已到上限，此后未发的判断记 Unsure(depth)、效应产出失败值，程序照常返回：{detail}",
                        site.start
                    ));
                    Some("depth".to_string())
                } else {
                    self.trace.warn(format!(
                        "W-budget: @{} 预算耗尽，此后未发的判断记 Unsure(budget)、效应产出失败值，程序照常返回：{detail}",
                        site.start
                    ));
                    None
                };
                self.预算停 = Some(BudgetStop {
                    exhausted: true,
                    unsent: n,
                    first_site: site.start,
                    cause,
                });
            }
        }
    }

    /// 一道题的价值记录（步 30；裁定三十八、四十三、四十六；Z0385）：按 `cut_inner` 的判序（[`线所在级`]）停在有线的那一级；
    /// 那一级不是「上岗」、是夹具线（降为夹具或没有选中证书）、或建不出混淆矩阵，返回 `None`（价值 0），不再往下找。返回
    /// （键, 混淆矩阵）。只读，不记账（挑选在审计重放时不调，见过程记录 工程-步30 §一 (d)）；闸门与切分点的内置用到时由调用处
    /// `note_calib`。`cut(r, key)` 换键仍是已知限制：挑选时只知道题自己的 `calib`
    pub(crate) fn 价值记录(&self, q: &Question) -> Option<(String, jpp_ir::plan::Channel)> {
        let 链 = self.calib.chain(&q.calib, q.form_hash.as_deref());
        let l = 线所在级(&链, &q.calib)?;
        if l.rec.status != "上岗" || l.rec.fixture_line() {
            return None;
        }
        Some((l.key.clone(), self.记录信道(&l.key, &l.rec)?))
    }

    /// 一条记录的混淆矩阵（原始计数；裁定四十六）：带标注样本按记录自己的线与 δ（`line_delta`）经 `stat::decided_up/down`
    /// （与 `cut` 同一判据）落到出口。是非记录（样本 (p, 真值)）→ 真是 / 真否 × act、ignore、unsure；K 元划分记录（样本
    /// (p_max, 对错)）→ 已决且对、已决且错、未决。没有样本或 δ 取不到为 `None`
    pub(crate) fn 记录信道(&self, key: &str, rec: &Lookup) -> Option<jpp_ir::plan::Channel> {
        let 样本 = self.calib.labelled(key);
        if 样本.is_empty() {
            return None;
        }
        let d = self.calib.line_delta(rec)?;
        if self.calib.binary(key) {
            let mut n = [[0f64; 3]; 2];
            for (p, 真) in &样本 {
                let 列 = if jpp_value::stat::decided_up(*p, rec.hi, d) {
                    0
                } else if jpp_value::stat::decided_down(*p, rec.lo, d) {
                    1
                } else {
                    2
                };
                n[usize::from(!*真)][列] += 1.0;
            }
            Some(jpp_ir::plan::Channel::Binary { n })
        } else {
            let (mut c, mut w, mut u) = (0.0, 0.0, 0.0);
            for (p, 对) in &样本 {
                if jpp_value::stat::decided_up(*p, rec.hi, d) {
                    if *对 {
                        c += 1.0;
                    } else {
                        w += 1.0;
                    }
                } else {
                    u += 1.0;
                }
            }
            Some(jpp_ir::plan::Channel::Symmetric { c, w, u })
        }
    }

    /// 本次刷新攒下的停发落定（步 30，B93 第 6 条）：原登记下标最小的那一组作首个停发站点，写 `W-budget`；
    /// 其余组只计数。没有攒下任何停发时什么都不做
    pub(crate) fn 落定首停(&mut self) {
        let Some(mut v) = self.首停延后.take() else {
            return;
        };
        if v.is_empty() {
            return;
        }
        // 稳定排序：同一原登记下标（关融合时同组拆开的题）保持停发先后
        v.sort_by_key(|x| x.0);
        let 合计: u64 = v.iter().map(|x| x.1).sum();
        let (_, n0, site, detail) = v.remove(0);
        self.记停发(n0, site, &detail);
        if let Some(b) = &mut self.预算停 {
            b.unsent += 合计 - n0;
        }
    }

    /// 一组判断预算停发（B93）：按缺席处置——标 `budget` 给 `cut`、记缺席账（首因 `budget`）、不发。
    /// `cut` 遇到这类读数给 `Unsure(budget)`；续跑（非审计）时这些站点重发，审计重放在同一站点同样停发。
    pub(crate) fn 预算停发(
        &mut self,
        items: &[(Rc<Question>, Rc<Reading>, String)],
        site: Span,
        detail: &str,
        attempts: u64,
    ) {
        let mut 新 = 0u64;
        for (_, r, k) in items {
            self.标缺席(r, k, "budget");
            self.停发说明.insert(k.clone(), detail.to_string());
            if self.停发键.insert(k.clone()) {
                新 += 1;
            }
        }
        // G4 一·4（主控定；裁定五十九第 17 条、B196）：从未发出的题记「未问」，第一次停发写「停下」；
        // 发出后重试中途付不起的（`attempts > 0`，调用已花）照旧是缺席
        self.写停下(StopCause::Budget);
        if attempts == 0 {
            self.记未问(items, StopCause::Budget);
        } else {
            self.记缺席账(items, "budget", detail, attempts);
        }
        self.记停发(新, site, detail);
    }

    /// 深度到限的一组判断（G4，一·3）：不发，标 `depth` 给 `cut`（出口 `Unsure(depth)`），每题记「未问」，
    /// 第一次停发写「停下」并出 `W-budget`。
    pub(crate) fn 深度停发(
        &mut self,
        items: &[(Rc<Question>, Rc<Reading>, String)],
        site: Span,
    ) {
        let detail = self.深度停说明();
        let mut 新 = 0u64;
        for (_, r, k) in items {
            self.标缺席(r, k, "depth");
            self.停发说明.insert(k.clone(), detail.clone());
            if self.停发键.insert(k.clone()) {
                新 += 1;
            }
        }
        self.写停下(StopCause::Depth);
        self.记未问(items, StopCause::Depth);
        self.记停发(新, site, &detail);
    }

    /// 深度到限的说明文字（`W-budget` 报文尾与契约值的 `resume.detail`）
    pub(crate) fn 深度停说明(&self) -> String {
        match self.深度上游 {
            Some((_, 上限, hop)) => format!("跨程序触发链已到第 {hop} 层，上限 {上限}"),
            None => "跨程序触发链已到深度上限".to_string(),
        }
    }

    /// 这一趟的尝试引用（G4 一·5）：`program` 取本段的段编号（没有追踪上下文时 `main`），`n = 本趟轮 + 1`
    pub(crate) fn 尝试(&self) -> AttemptRef {
        AttemptRef {
            program: self
                .ledger
                .view()
                .current_trace()
                .map(|t| t.span.as_str().to_string())
                .unwrap_or_else(|| "main".to_string()),
            n: self.本趟轮 as u64 + 1,
        }
    }

    /// 第一次停发时写一条「停下」（G4；R13）。审计重放不写（没有账本键，重放若写会重复）
    pub(crate) fn 写停下(&mut self, cause: StopCause) {
        if self.audit.on || self.停下已写 {
            return;
        }
        self.停下已写 = true;
        let attempt = self.尝试();
        self.登记记账(Entry::Stop { attempt, cause });
    }

    /// 一组题记「未问」（G4；R13）：每个账本键一条，按读数的登记序入账。审计重放不写
    pub(crate) fn 记未问(
        &mut self,
        items: &[(Rc<Question>, Rc<Reading>, String)],
        reason: StopCause,
    ) {
        if self.audit.on {
            return;
        }
        let attempt = self.尝试();
        for (_, r, k) in items {
            if self.未问已记.insert(k.clone()) {
                self.记账(
                    r.id,
                    Entry::Unasked {
                        attempt: attempt.clone(),
                        key: k.clone(),
                        payer: attempt.program.clone(),
                        reason,
                    },
                );
            }
        }
    }

    /// 开跑前的账本里这道题是否记为「未问」、原因是什么（G4 一·4）
    pub(crate) fn 未问(&self, k: &str) -> Option<&str> {
        self.未问表
            .get_or_init(|| {
                self.ledger
                    .view()
                    .entries
                    .iter()
                    .filter_map(|e| match e {
                        Entry::Unasked { key, reason, .. } => Some((
                            key.clone(),
                            match reason {
                                StopCause::Budget => "budget",
                                StopCause::Depth => "depth",
                                StopCause::Deadline => "deadline",
                            }
                            .to_string(),
                        )),
                        _ => None,
                    })
                    .collect()
            })
            .get(k)
            .map(String::as_str)
    }

    /// 缺席事件入账（只增）：尝试次数记在一组的第一题上（B32 逐次计费、B35 重放按它计入预算）。
    pub(crate) fn 记缺席账(
        &mut self,
        items: &[(Rc<Question>, Rc<Reading>, String)],
        cause: &str,
        detail: &str,
        attempts: u64,
    ) {
        for (i, (_, r, k)) in items.iter().enumerate() {
            // G5 附录一：同一内容键再次缺席时记下一个序号（`absent:<k>#<n>`），审计重放据此逐次复现
            if let Some(mk) = self.缺席记录键(k, cause) {
                self.记账(
                    r.id,
                    Entry::Absent {
                        key: mk,
                        jkey: self.judge_keys.get(k).cloned(),
                        cause: cause.to_string(),
                        detail: detail.to_string(),
                        attempts: if i == 0 { attempts } else { 0 },
                        // G5 附录二：判断器缺席记下所属的真登记
                        nth: if cause == "absent" {
                            self.待发真序.get(k).copied()
                        } else {
                            None
                        },
                    },
                );
            }
        }
    }

    /// 记一组题的缺席或超时（B32）：标记给 `cut`，账本记事件（重放据此复现），trace 留痕。
    pub(crate) fn 记缺席(
        &mut self,
        items: &[(Rc<Question>, Rc<Reading>, String)],
        cause: &str,
        site: Span,
        detail: String,
        attempts: u64,
    ) {
        for (_, r, k) in items {
            self.标缺席(r, k, cause);
        }
        self.记缺席账(items, cause, &detail, attempts);
        for (_, _, k) in items {
            let mk = format!("absent:{k}");
            self.trace.push(
                "absent",
                &mk,
                false,
                0.0,
                site,
                format!("{cause}：{detail}"),
            );
        }
        self.trace.warn(format!(
            "W-{cause}: @{} {} 道题转 Unsure({cause})：{detail}",
            site.start,
            items.len()
        ));
    }

    /// 缺席策略的 `then`（B32）：escalate → 程序挂起待续跑；conservative → 出口 Unsure(absent) 继续；fail → 运行期错误。
    pub(crate) fn 缺席处置(
        &mut self,
        items: &[(Rc<Question>, Rc<Reading>, String)],
        pol: &jpp_ir::ir::AbsentPolicy,
        site: Span,
        detail: String,
        attempts: u64,
    ) -> R<()> {
        // 挂起与报错也入账：只凭账本重放时照记的处置重现（B35「首跑在哪里停，重放就在哪里停」）
        if pol.then != "conservative" {
            self.记缺席账(items, "absent", &detail, attempts);
        }
        match pol.then.as_str() {
            "fail" => err(
                Some("E-rt-absent"),
                format!("判断器缺席（absent.then=fail）：{detail}"),
                site,
            ),
            "conservative" => {
                self.记缺席(items, "absent", site, detail, attempts);
                Ok(())
            }
            _ => {
                self.trace.warn(format!(
                    "W-absent: @{} 判断器缺席，按 absent.then=escalate 挂起：{detail}",
                    site.start
                ));
                Err(Fault::Halt(Pending {
                    cause: "absent".into(),
                    key: items.first().map(|x| x.2.clone()).unwrap_or_default(),
                    site,
                    detail: format!(
                        "判断器缺席：{detail}。恢复后用 --resume 续跑，已完成的判断不再付费"
                    ),
                }))
            }
        }
    }
}

/// 一组的代表成员（步 30）：有真站点成员取第一条真站点成员，只含推测的组取第一条成员
fn 代表(g: &[PendingJudge]) -> &PendingJudge {
    g.iter().find(|p| !p.speculative).unwrap_or(&g[0])
}

/// 一道题的请求与选择规模（步 28 起 `info_values`，步 30 起层内挑选与 `gate_info` 共用）：请求取题的 `request`，缺省按 op；
/// 选择规模：K 选一取状态 `over` 数，打分取档数，是非题 0
pub(crate) fn 请求与规模(q: &Question, st: &State) -> (jpp_ir::question_kind::Request, usize) {
    use jpp_ir::question_kind::Request;
    let request = Request::parse(&q.request()).unwrap_or(match q.op {
        Op::Test => Request::Whether,
        Op::Select => Request::One,
        Op::Measure => Request::Degree,
    });
    let k = match q.op {
        Op::Select => st.over.len(),
        Op::Measure => q.scale.len(),
        Op::Test => 0,
    };
    (request, k)
}

/// `cut_inner`（不带 `cost`/`alpha` 时）用哪一级的线：题级记录「上岗」或「停岗候选」即题级；题级「停岗」即冷；`fit:` 键不借；
/// 否则题式级同样判；再否则类级，但题式「停岗」时不借类（B34）；都没有即冷（`None`）。与 `bridge.rs::cut_inner` 的分支同序，
/// 改一处须改两处（步 30 Z0385：价值取的记录要是切出口的那条线所在的记录）
pub(crate) fn 线所在级<'c>(
    链: &'c jpp_effects::views::Chain,
    key: &str,
) -> Option<&'c jpp_effects::views::Link> {
    let 有线 = |l: &jpp_effects::views::Link| matches!(l.rec.status.as_str(), "上岗" | "停岗候选");
    if 有线(&链.question) {
        return Some(&链.question);
    }
    if 链.question.rec.status == "停岗" || key.starts_with("fit:") {
        return None;
    }
    if let Some(f) = 链.form.as_ref().filter(|f| 有线(f)) {
        return Some(f);
    }
    let 题式停岗 = 链.form.as_ref().is_some_and(|f| f.rec.status == "停岗");
    if 题式停岗 {
        return None;
    }
    链.class.as_ref().filter(|c| 有线(c))
}
