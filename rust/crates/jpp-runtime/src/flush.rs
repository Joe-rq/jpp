//! 刷新：按材料分组成层（B155 前按状态）、同键只问一次、调用客户端、写账本、填答案（20 §2.3 `flush.rs`）。
//! 步 4a 从 `interp.rs` 机械拆出，只搬代码、不改逻辑（21 §三·3）。

use super::*;

/// 一组题发出前的结论（步 15e）
pub(crate) enum 发出前 {
    /// 不发：重放照记、时延预算用完、熔断、审计下只剩推测登记
    跳过,
    /// 排进本窗
    发出(待发),
    /// 缺席处置已处理完这组，本次刷新到此结束（与拆出前的 `return` 同）
    结束,
    /// 预算发不起（B93，步 22-0）：本窗不再往里排；已排的组发完、落完账之后再停发这一组，
    /// 账本顺序因此与串行相同（先是发出的判断，再是停发的缺席条目）
    停发(Vec<(Rc<Question>, Rc<Reading>, String)>, Span, String),
}

/// 一道待发的题：题、读数句柄、账本键
type 题项 = (Rc<Question>, Rc<Reading>, String);

/// 排进窗口、等发出的一组题（步 15e）。`states[i]` 是 `items[i]` 的状态：一组按材料哈希分（B155，
/// 步 15i），组内各题 `on`/`ctx`/`ref` 相同、`over` 可不同
pub(crate) struct 待发 {
    pub(crate) states: Vec<Rc<State>>,
    site: Span,
    pub(crate) items: Vec<(Rc<Question>, Rc<Reading>, String)>,
    同键: Vec<Vec<Rc<Reading>>>,
    /// 只含推测登记（步 13a-1，B0270）：发出后端口报错就放弃这一组，不中止程序
    speculative: bool,
}

/// 推测组被放弃时账本 `Absent` 条目的原因（步 13a-1）。只对只含推测登记的组生效：真站点遇到它当作没有记录
const 推测放弃: &str = "spec_miss";

impl<'a> Interp<'a> {
    /// **写答案的唯一入口**（`20` §2.3「只有 flush 能填答案」，登记为 CI grep：`scripts/grep_fill.py`）。
    /// 刷新发出的一层与登记时的账本命中都经这里；读答案的唯一入口是 `readings.rs::answer_of`。
    pub(crate) fn fill_answer(&self, r: &Reading, a: Answer) {
        self.answers.borrow_mut().insert(r, a);
    }

    /// 从一条账本判断记录填读数：答案与置换测量一起填。
    ///
    /// 置换测量（`perms`, `mode_share`）是读数的一部分，出口由它决定 `Pick` / `tie` / `untested`（J-15）。
    /// 只填答案、不填测量，同一条读数在首发处是 `pick`/`band`，在账本命中处（重放、续跑、推测后真站点）
    /// 就成了 `untested`。所以凡是从账本或从同键首发取答的地方，都经这里。
    ///
    /// 自报置信度（B154，步 20j-3）同理：它是判断器随这条答案给的第二个输出，与答案同处取回，
    /// 否则 `stat: "confidence"` 的出口在首发处与重放处不同。
    pub(crate) fn fill_from_record(
        &self,
        r: &Reading,
        a: Answer,
        perm: Option<jpp_ledger::PermMeasure>,
        confidence: Option<f64>,
    ) {
        if let Some(pm) = perm {
            r.set_mode_share(pm.mode_share, pm.perms);
        }
        if let Some(c) = confidence {
            self.置信表.borrow_mut().insert(r.id, c);
        }
        self.fill_answer(r, a);
    }

    /// 刷新点（`12` §2.2:129）：「被 `cut`、`fit`、`match`/`if`、或宿主读内容时刷新。刷新时把所有
    /// 已登记且输入就绪的 `judge` **按状态分组**、按依赖分层，**一层一次发出**」。
    ///
    /// 这里的分层是天然的：登记发生在求值途中，依赖前一条出口的判断只可能在前一次刷新**之后**
    /// 才登记得上（要拿到出口就得先 `cut`，而 `cut` 本身就是刷新点）。所以「一次刷新 = 一层」，
    /// 层内按状态哈希分组融合——与 Python `_calls_from_plans` 的「同状态哈希」同一条规则。
    /// B155（步 15i）起改按材料哈希分组：`over` 随题走，同材料上候选集不同的题也在一次调用里。
    pub(crate) fn flush(&mut self, reason: &str) -> R<()> {
        let r = self.flush_inner(reason);
        // C2b：单元图的刷新纪元随解释器的刷新点推进
        self.单元刷新();
        // 步 30（B93 第 6 条）：重排过的刷新攒下的停发，不论本次刷新从哪条路返回都在这里落定
        self.落定首停();
        // 层末落盘（B55，步 18b）：刷新即一层；这一层与此前登记的条目按序写出
        if r.is_ok() {
            self.层末落盘()?;
        }
        r
    }

    fn flush_inner(&mut self, reason: &str) -> R<()> {
        debug_assert!(
            refresh_point(reason).is_some(),
            "刷新点 {reason} 未登记在 readings.rs::REFRESH_POINTS"
        );
        // B93（步 22-0）：预算停机在刷新点降级。首跑发不起的组记缺席账（首因 `budget`），
        // 审计重放照账本在同一站点停发，所以这里不再补核「最近一次取答」处的预算。
        // B160（步 15h-2）：这一层要发的还有登记以来的生成；新的一层开始前先收上一层（至多一个开着的层），
        // 生成先交出（非阻塞），本层的判断条目记进层里，层收齐时按登记序入账
        let 有生成 = self.有未交生成();
        if self.pending.is_empty() && !有生成 {
            return Ok(());
        }
        self.收层()?;
        if 有生成 {
            self.交出生成()?;
        }
        if self.pending.is_empty() {
            return Ok(());
        }
        let pending = std::mem::take(&mut self.pending);
        // W-spec-unused 只报「发出、跨状态、没用上」（B51-C1，步 22 / B0487；主控 Z0209 Q4）：本层待发的推测键先从
        // `speculated` 拿掉，只含推测的组真发出时再加回（`flush_after_send`）。并进含真站点那次调用的（同状态推测）
        // 和因预算、中止没发出的不加回。`speculated` 因此是「推测登记且发出了的跨状态键」
        for p in pending.iter().filter(|p| p.speculative) {
            for (_, _, k) in &p.items {
                self.speculated.remove(k);
            }
        }
        // 融合 pass（12 §4 序 2）：同材料、同层的题合成一次调用（P5 / 12 §10 G2；B155 起按材料哈希
        // `hash(on, ctx, ref)`，同材料上候选集不同的 `select` 也合成一次）。
        // **关掉就逐题发**——这正是 §4 表里「不做会坏什么：E8 成本 +45%」那一栏要量的东西。
        let mut order: Vec<String> = vec![];
        let mut groups: HashMap<String, Vec<PendingJudge>> = HashMap::new();
        let fuse = self.plan.fuse;
        // 审查修复 3b（B94 下半）：组的先后按组内第一条非提升登记的位置；只有提升登记的组排在最后。
        // 没有提升登记时与改前的「首次出现」逐字相同
        let mut 首个非提升: HashMap<String, usize> = HashMap::new();
        let 有提升 = pending.iter().any(|p| p.lifted);
        // 复核修复 8：不融合时分组前按题拆开，每题继承原登记的状态、站点、推测与提升标记和登记位置
        // （改前在 `flush_before_send` 里拆，余项丢了标记、排到最后）
        let 逐条: Vec<(usize, PendingJudge)> = if fuse {
            pending.into_iter().enumerate().collect()
        } else {
            pending
                .into_iter()
                .enumerate()
                .flat_map(|(idx, p)| {
                    let PendingJudge {
                        state,
                        items,
                        site,
                        speculative,
                        lifted,
                        site_id,
                    } = p;
                    items.into_iter().map(move |it| {
                        let p = PendingJudge {
                            state: state.clone(),
                            items: vec![it],
                            site,
                            speculative,
                            lifted,
                            site_id,
                        };
                        (idx, p)
                    })
                })
                .collect()
        };
        for (idx, p) in 逐条 {
            // 不融合时逐题一组，键取账本键：同一个键的多条登记（真站点与提升、推测）合成一组、只问一次，
            // 与融合开时组内去重同口径（复核修复 8）
            // B155（步 15i）：分组键是材料哈希，不是 `StateHash`（`over` 随题走，不分组）。
            // 依据：B155（地基/附注/2026-09-26-批6裁定.md §三）
            let h = if fuse {
                // 伴随题并行发法（B0492 S5）：伴随题另成一组，同一刷新时刻另发一次调用
                if self.伴随键.contains(&p.items[0].2) {
                    format!("{}#伴随", p.state.mat_hash())
                } else {
                    p.state.mat_hash()
                }
            } else {
                p.items[0].2.clone()
            };
            if !groups.contains_key(&h) {
                order.push(h.clone());
            }
            if !p.lifted {
                首个非提升.entry(h.clone()).or_insert(idx);
            }
            groups.entry(h).or_default().push(p);
        }
        if 有提升 {
            // 稳定排序：有非提升登记的组按其位置，只有提升登记的组保持原相对次序排在后面
            order.sort_by_key(|h| 首个非提升.get(h).copied().unwrap_or(usize::MAX));
        }
        // 超预算先丢推测的，再动真站点（推测本来就是可放弃的）
        if self.cost.calls + self.audit.calls >= self.budget.calls {
            groups
                .values_mut()
                .for_each(|g| g.retain(|p| !p.speculative));
            // 只含推测的组整组拿空了：不再排进本层（改前空组会走到 `flush_before_send` 的 `group[0]` 越界；
            // 推测在预算用完后被重新登记即会触发，步 22 的层内挑选测试首次撞上）
            order.retain(|h| !groups[h].is_empty());
        }
        // 层内挑选（B43、B51-C1，步 22 / B0487）：本层会真发的组超出剩余调用数时，按钩子给的顺序发——真站点先、
        // 跨状态推测后；不超时顺序不动。停发点仍由下面逐组的 `charge_after` 定
        // 原登记下标（步 30：重排后首个停发站点仍按它记，B93 第 6 条）
        let 原位: HashMap<String, usize> = order
            .iter()
            .enumerate()
            .map(|(i, h)| (h.clone(), i))
            .collect();
        let order = {
            let gs: Vec<&[PendingJudge]> = order.iter().map(|h| groups[h].as_slice()).collect();
            match self.层内挑选(&gs) {
                Some(新序) => {
                    if self.预算停.is_none() && self.首停延后.is_none() {
                        self.首停延后 = Some(vec![]);
                    }
                    新序.into_iter().map(|i| order[i].clone()).collect()
                }
                None => order,
            }
        };
        let mut layer_calls = 0u64;
        let mut layer_questions = 0usize;
        // **按窗口发出**（步 15e）：窗口大小取画像 `concurrency`（未测 1），cost 预算再按已观察的
        // 单次最大费用限窗（`窗口大小`）。W = 1 时每窗一组，与逐组串行逐行相同。
        let 配置窗 = if self.audit.on {
            1
        } else {
            self.calib.profile().concurrency().unwrap_or(1).max(1) as usize
        };
        let mut 队列: std::collections::VecDeque<String> = order.into();
        while !队列.is_empty() {
            let w = self.窗口大小(配置窗);
            let mut 窗: Vec<待发> = vec![];
            // 发出前的停：审计缺记录、缺席处置报错（`Err`），或缺席处置结束本次刷新（`结束`）
            let mut 前停: Option<R<()>> = None;
            // 预算发不起的那一组（B93）：本窗发完再停发它，队列里余下的组接着逐组核（同样停发）
            let mut 停发组: Option<(Vec<题项>, Span, String, usize)> = None;
            // 窗内各组的原登记下标（步 30：发出后重试中途停发时，首个停发站点按它记）
            let mut 窗位: Vec<usize> = vec![];
            while 窗.len() < w {
                let Some(h) = 队列.pop_front() else {
                    break;
                };
                let group = groups.remove(&h).expect("刚放进去的");
                self.当前组位 = 原位.get(&h).copied().unwrap_or(usize::MAX);
                match self.flush_before_send(group, 窗.len() as u64) {
                    Ok(发出前::跳过) => {}
                    Ok(发出前::发出(g)) => {
                        窗.push(g);
                        窗位.push(self.当前组位);
                    }
                    Ok(发出前::结束) => {
                        前停 = Some(Ok(()));
                        break;
                    }
                    Ok(发出前::停发(items, site, detail)) => {
                        停发组 = Some((items, site, detail, self.当前组位));
                        break;
                    }
                    Err(f) => {
                        前停 = Some(Err(f));
                        break;
                    }
                }
            }
            let 批起 = std::time::Instant::now();
            let 首发: Vec<Result<jpp_effects::JudgeResult, EffectError>> = if 窗.len() == 1 {
                let ask: Vec<&Question> = 窗[0].items.iter().map(|(q, _, _)| q.as_ref()).collect();
                let states = 窗[0].states.clone();
                vec![self.call_judge(&states, &ask)]
            } else if 窗.is_empty() {
                vec![]
            } else {
                // 墙钟按窗口计（B32「预计层数 × p95」口径），之后各组只再计自己的重试
                let r = self.call_judge_many(&窗);
                self.latency_spent += 批起.elapsed().as_secs_f64();
                r
            };
            let n = 窗.len();
            // 发出后按登记顺序处理。预算停机（`Halt`）不中断本窗：同窗其余调用已经发出、钱已花，
            // 先把事实记完再停（`13` §5）；W = 1 时本窗只有一组，与串行相同
            let mut 后停: Option<Fault> = None;
            for (k, (g, r)) in 窗.into_iter().zip(首发).enumerate() {
                self.当前组位 = 窗位[k];
                let 起 = if n == 1 {
                    批起
                } else {
                    std::time::Instant::now()
                };
                let 在途 = (n - 1 - k) as u64;
                match self.flush_after_send(g, r, 起, 在途, &mut layer_calls, &mut layer_questions)
                {
                    Ok(()) => {}
                    Err(f @ Fault::Halt(_)) => {
                        if 后停.is_none() {
                            后停 = Some(f);
                        }
                    }
                    Err(f) => return Err(f),
                }
            }
            if let Some(f) = 后停 {
                return Err(f);
            }
            if let Some((items, site, detail, 位)) = 停发组 {
                self.当前组位 = 位;
                self.预算停发(&items, site, &detail, 0);
            }
            if let Some(r) = 前停 {
                return r;
            }
        }
        if layer_calls > 0 {
            self.layers.push(Layer {
                reason: reason.to_string(),
                calls: layer_calls,
                questions: layer_questions,
            });
        }
        // 关融合时拆出来的余项，接着发（它们同属这一层，只是各自一次调用）。先落定本次攒下的停发：
        // 下一次刷新的登记下标另起，不能和这一次的比
        if !self.pending.is_empty() {
            self.落定首停();
            return self.flush(reason);
        }
        Ok(())
    }
    /// 本窗发几组（步 15e，主会话 2026-09-25 裁定）：`min(W, floor((budget.cost − 已花) / c_max))`；
    /// 还没观察到单次费用时 1（第一窗串行，用来学 `c_max`）；`c_max = 0`（没给价格、固定观察）时不限；
    /// `floor` 为 0 时 1（与串行相同：发出前按费用 0 核，停的是下一步）。
    fn 窗口大小(&self, w: usize) -> usize {
        if w <= 1 {
            return 1;
        }
        match self.c_max {
            None => 1,
            Some(c) if c <= 0.0 => w,
            Some(c) => {
                let 余 = self.budget.cost - (self.cost.usd + self.audit.usd);
                let k = (余 / c).floor();
                if k >= 1.0 { (k as usize).min(w) } else { 1 }
            }
        }
    }

    /// 发一次之前核预算，把同窗已发出或已排队、尚未计数的 `前面` 次调用当作已花（步 15e：calls 预算
    /// 硬截断）。先加后减，报文里的「已用 + 本次」与串行在同一处停时逐字相同；`前面 = 0` 即 `charge(1, 0)`。
    fn charge_after(&mut self, 前面: u64) -> Result<(), String> {
        self.cost.calls += 前面;
        let r = self.charge(1, 0.0);
        self.cost.calls -= 前面;
        r
    }

    /// 一组题发出前的处理（步 15e 从 `flush` 的组循环原样搬出）：同键去重、重放已记缺席、时延预算、
    /// 熔断、审计、预算核对。`已排` 是同窗此前已排队未发的调用数（串行为 0）。
    fn flush_before_send(&mut self, group: Vec<PendingJudge>, 已排: u64) -> R<发出前> {
        let site = group[0].site;
        let only_speculative = group.iter().all(|p| p.speculative);
        // 审查修复 3b：真站点登记的键（预算停发只标它们；只按推测进组的键没走到，不记缺席账）
        let 真键: HashSet<String> = group
            .iter()
            .filter(|p| !p.speculative)
            .flat_map(|p| p.items.iter().map(|(_, _, k)| k.clone()))
            .collect();
        // 逐题带自己的状态（B155：一组按材料分，组内 `over` 可不同）
        let (各态, items): (Vec<Rc<State>>, Vec<题项>) = group
            .into_iter()
            .flat_map(|p| {
                let st = p.state;
                p.items.into_iter().map(move |it| (st.clone(), it))
            })
            .unzip();
        if items.is_empty() {
            return Ok(发出前::跳过);
        }
        // 关掉融合时，同一次 judge 登记的多道题也要逐题发：`flush` 分组前已按题拆开（复核修复 8），
        // 这里每组只剩同一个账本键
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
        // 同一个账本键即同一个状态（键含 `StateHash`），留首条的状态
        let mut states: Vec<Rc<State>> = vec![];
        for (st, (q, r, k)) in 各态.into_iter().zip(items) {
            match 首见.get(&k) {
                Some(i) => 同键[*i].push(r),
                None => {
                    首见.insert(k.clone(), 去重.len());
                    同键.push(vec![r.clone()]);
                    去重.push((q, r, k));
                    states.push(st);
                }
            }
        }
        let items = 去重;
        // G5 附录二：这一组每道题所属的真登记（同键有多条取最小；只有推测、提升登记的不记），缺席入账时写进 `nth`
        for ((_, _, k), rs) in items.iter().zip(&同键) {
            match rs
                .iter()
                .filter_map(|r| self.真登记序.get(&r.id).copied())
                .min()
            {
                Some(m) => {
                    self.待发真序.insert(k.clone(), m);
                }
                None => {
                    self.待发真序.remove(k);
                }
            }
        }
        // G4（裁定五十九第 7、17 条）：跨程序触发链到限，这一趟一道题都不发，逐题 `Unsure(depth)` 并记「未问」
        if self.深度停 {
            self.深度停发(&items, site);
            return Ok(发出前::跳过);
        }
        // **缺席 / 超时的重放**（B32、B35）：账本里记过这一组题的缺席事件，照记的给出，不再发。
        // 首跑的失败尝试按记的次数计入审计重放的预算（逐次计费，B32 裁定选项 A）。
        let 已记: Vec<Option<(String, String, u64)>> = items
            .iter()
            .map(|(_, r, k)| {
                // G5 附录一：审计重放按缺席复现的读数取登记时对上的那一条记录（带序号），其余取 `absent:<k>`
                let 记录键 = self
                    .重放缺席键
                    .get(&r.id)
                    .cloned()
                    .unwrap_or_else(|| format!("absent:{k}"));
                match self.账本查(&记录键) {
                    // 步 13a-1：推测组被放弃的记录只对只含推测的组生效；真站点走到同一个键（续跑）照常发问
                    Some(Entry::Absent { cause, .. })
                        if cause.as_str() == 推测放弃 && !only_speculative =>
                    {
                        None
                    }
                    Some(Entry::Absent {
                        cause,
                        detail,
                        attempts,
                        ..
                    }) => Some((cause.clone(), detail.clone(), *attempts)),
                    // G4 一·4：预算停发记为「未问」的题（原先记 `Absent(budget)`）；说明文字到用时再算
                    _ if self.未问(k) == Some("budget") => {
                        Some(("budget".into(), String::new(), 0))
                    }
                    _ => None,
                }
            })
            .collect();
        if 已记.iter().all(|x| x.is_some()) {
            let 记: Vec<(String, String, u64)> = 已记.into_iter().flatten().collect();
            let (首因, 首详, _) = 记[0].clone();
            let 尝试: u64 = 记.iter().map(|x| x.2).sum();
            let 策略 = self.budget.absent.clone();
            // 没声明策略时账本里的缺席只能来自隐含策略（网络类错误用尽重试），隐含策略的处置就是 conservative
            // （`budget.rs::缺席策略`）。G5（推翻 PR #48）：续跑不论处置都重发，处置只在审计重放复现时用
            let 处置 = 策略
                .as_ref()
                .map(|p| p.then.clone())
                .unwrap_or_else(|| "conservative".into());
            // 续跑（非审计）时，因预算停机、挂起或报错的站点要重新发：它们当时没有得到答案（与层内挑选的预判共用）
            let 重发 = self.缺席记录重发(&首因);
            if !重发 {
                if self.audit.on {
                    self.audit.calls += 尝试;
                    self.audit.last_site = Some(site);
                }
                if 首因 == "budget" {
                    // B93：首跑在这里停发（发出前预算不够，或重试中途用完），重放在同一站点同样停发。
                    // G4：「未问」不带说明文字，由同一站点的预算核对重算（审计重放计数与首跑相同，文字相同）
                    let 首详 = if 首详.is_empty() {
                        self.charge(1, 0.0).err().unwrap_or_default()
                    } else {
                        首详
                    };
                    self.预算停发(&items, site, &首详, 0);
                    return Ok(发出前::跳过);
                }
                // 没声明策略的缺席（现场稳定性三修：网络类错误的隐含策略）审计重放照记录给 Unsure(absent)，走下面的循环
                if 首因 == "absent"
                    && 处置 != "conservative"
                    && let Some(pol) = 策略.clone()
                {
                    return self
                        .缺席处置(&items, &pol, site, 首详, 0)
                        .map(|_| 发出前::结束);
                }
                if 首因 == 推测放弃 {
                    // 步 13a-1：首跑放弃的推测组，重放按记录跳过（调用已照记录计入），补报同一告警
                    self.推测组放弃告警(&items, site, "重放：首跑时端口报错", &首详);
                    // 首跑发出过（付了一次调用）：照首跑记进白花账（步 22，B35 重放同告警）
                    for (_, _, k) in &items {
                        self.speculated.insert(k.clone());
                    }
                    return Ok(发出前::跳过);
                }
                for ((_, r, k), (c, _, _)) in items.iter().zip(记) {
                    self.标缺席(r, k, &c);
                    self.cost.replayed += 1;
                }
                return Ok(发出前::跳过);
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
                return Ok(发出前::跳过);
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
                return Ok(发出前::跳过);
            }
        }
        // 审计重放：首跑没发过的推测登记照样不发（首跑可能因预算丢掉了它们）
        if self.audit.on && only_speculative {
            return Ok(发出前::跳过);
        }
        // 窗口内已排队未发的调用也算进 calls 预算（步 15e，硬截断；已排 = 0 即串行）。
        // B93：发不起就停发这一组（按缺席处置），不停程序；此后的组同样停发、费用为零
        if let Err(detail) = self.charge_after(已排) {
            // 只有推测登记的组发不起就放弃，不记缺席账、不计停发：推测本来就可放弃，站点若真走到，
            // 真站点登记时再按停发处置（审计重放同样跳过只剩推测的组）
            if only_speculative {
                return Ok(发出前::跳过);
            }
            // 审查修复 3b：组里只按推测进来的键同理不标、不记（没走到）
            let items: Vec<_> = items
                .into_iter()
                .filter(|(_, _, k)| 真键.contains(k))
                .collect();
            if items.is_empty() {
                return Ok(发出前::跳过);
            }
            return Ok(发出前::停发(items, site, detail));
        }
        if self.audit.on {
            let texts: Vec<&str> = items.iter().map(|(q, _, _)| q.text.as_str()).collect();
            return Err(self.replay_missing(format!("judge「{}」", texts.join("|")), site));
        }
        // 同材料合批后的窗口核对（B155 (5)：按材料加各题 criteria 总量；画像测过窗口才核）
        self.check_window_group(&states, &items, site);
        Ok(发出前::发出(待发 {
            states,
            site,
            items,
            同键,
            speculative: only_speculative,
        }))
    }

    /// 步 13a-1（B0270）：只含推测登记的组发出后端口报错——放弃这一组，账本记 `spec_miss`（重放据此跳过），
    /// 告警写明站点、键与错误种类。推测本来就可放弃；真站点走到同一个键时照常发问。
    fn 放弃推测组(
        &mut self,
        items: &[(Rc<Question>, Rc<Reading>, String)],
        site: Span,
        种类: &str,
        原文: &str,
    ) {
        self.记缺席账(items, 推测放弃, &format!("{种类}：{原文}"), 1);
        self.推测组放弃告警(items, site, 种类, 原文);
    }

    fn 推测组放弃告警(
        &mut self,
        items: &[(Rc<Question>, Rc<Reading>, String)],
        site: Span,
        种类: &str,
        原文: &str,
    ) {
        let 键: Vec<String> = items.iter().take(3).map(|(_, _, k)| 头(k, 8)).collect();
        // 依据：B0270（主会话 2026-09-26 定，待 Fable 批 7 追认）；12 §5 推测可放弃
        self.trace.warn(format!(
            "W-spec-fixture-miss: @{} 推测登记的 {} 道题（键 {}{}）发出后端口报错（{种类}：{}），放弃这一组，程序照常；真站点走到时照常发问",
            site.start,
            items.len(),
            键.join(", "),
            if items.len() > 3 { "…" } else { "" },
            原文.chars().take(120).collect::<String>()
        ));
    }

    /// 一组题首发返回之后的处理（步 15e 从 `flush` 的组循环原样搬出）：计数、重试与退避、缺席处置、
    /// 落账、超时站点、事后核预算。`起` 是计时起点（串行为发出前，并发窗为本组处理开始）；
    /// `在途` 是同窗后面已发出、尚未计数的调用数（串行为 0），重试核预算时算进去。
    fn flush_after_send(
        &mut self,
        g: 待发,
        首发: Result<jpp_effects::JudgeResult, EffectError>,
        起: std::time::Instant,
        在途: u64,
        layer_calls: &mut u64,
        layer_questions: &mut usize,
    ) -> R<()> {
        let 待发 {
            states,
            site,
            items,
            同键,
            speculative,
        } = g;
        // **每次发出都计费**（B32，主会话裁定选项 A）：首发已在发出前核过预算，这里记一次；
        // 重试前各核一次预算，每次发出（无论成败）都计入 `cost.calls`。
        // 步 15e：首发已由窗口发出，计数在这里按登记顺序加，账本 `call` 编号与串行相同。
        let ask: Vec<&Question> = items.iter().map(|(q, _, _)| q.as_ref()).collect();
        // 只含推测的组发出了：自付一次调用的跨状态推测，没用上要记白花（B51-C1，步 22）
        if speculative {
            for (_, _, k) in &items {
                self.speculated.insert(k.clone());
            }
        }
        let mut 结果 = 首发;
        self.cost.calls += 1;
        self.记请求(Self::读数效应());
        let mut 尝试 = 1u64;
        // 步 13a-1（B0270）：只含推测的组，端口报错、回复题数不对或答案形状不合法就放弃，不重试、不走缺席策略、
        // 不中止程序。真站点走到同一个键时照常发问，同样的错误由真站点正式报出
        if speculative {
            let 坏 = match &结果 {
                Err(e) => Some((
                    if e.0.contains("固定观察未命中") {
                        "fixture-miss"
                    } else {
                        "port-error"
                    },
                    e.0.clone(),
                )),
                Ok(r) if r.answers.len() != ask.len() => Some((
                    "bad-reply",
                    format!("答案数 {} 与题数 {} 不符", r.answers.len(), ask.len()),
                )),
                Ok(r) => r
                    .answers
                    .iter()
                    .zip(items.iter())
                    .zip(states.iter())
                    .find_map(|((a, (q, _, _)), st)| {
                        self.validate_answer(a, q, st, site).err().map(|f| match f {
                            Fault::Error(e) => e.message,
                            Fault::Halt(p) => p.detail,
                        })
                    })
                    .map(|m| ("malformed", m)),
            };
            if let Some((种类, 原文)) = 坏 {
                self.latency_spent += 起.elapsed().as_secs_f64();
                // 回复已经返回（bad-reply、malformed）：钱已经花了，照记
                if let Ok(r) = &结果 {
                    self.cost.tokens += r.tokens;
                    self.cost.usd += r.cost;
                }
                self.放弃推测组(&items, site, 种类, &原文);
                return Ok(());
            }
        }
        // **重试与退避**（B32）：声明了 absent 策略按声明的；没声明而首发报网络类错误，按隐含策略重试、用尽转
        // Unsure(absent)、程序照常（现场稳定性三修 (1)）；其余沿用旧行为（客户端错误即运行期错误）
        if let Some(pol) = self.缺席策略(结果.as_ref().err()) {
            // 隐含策略（程序没声明 `budget.absent`，首发是网络类错误）只管网络类错误：重试里一旦遇到非网络类
            // 错误（HTTP 400、回复解析失败），不再重试、不转缺席，按客户端错误报 E-rt-client（公开 PR #48 Codex
            // 意见；赛后欠账 flush.rs:571）。声明了策略的按声明的办，不变
            let 隐含 = self.budget.absent.is_none();
            let 非网络 = |r: &Result<jpp_effects::JudgeResult, EffectError>| {
                r.as_ref().err().is_some_and(|e| !e.is_network())
            };
            let mut 等 = pol.backoff;
            let mut 次 = 0;
            while 结果.is_err() && 次 < pol.retry && !(隐含 && 非网络(&结果)) {
                if 等 > 0.0 {
                    std::thread::sleep(std::time::Duration::from_secs_f64(等));
                }
                等 *= 2.0;
                次 += 1;
                // 同窗后面已发未计的调用也算进去（步 15e；串行时为 0）
                if self.charge_after(在途).is_err() {
                    // 重试中途预算用完：已花的时延与尝试入账，这一组停发（B93；重放在同一站点停发，B35）
                    self.latency_spent += 起.elapsed().as_secs_f64();
                    self.预算停发(&items, site, &format!("@{}", site.start), 尝试);
                    return Ok(());
                }
                结果 = self.call_judge(&states, &ask);
                self.cost.calls += 1;
                self.记请求(Self::读数效应());
                尝试 += 1;
            }
            // 失败路径也计时延：退避睡眠与各次失败请求都占时延预算
            let 用时 = 起.elapsed().as_secs_f64();
            self.latency_spent += 用时;
            if let Err(e) = &结果
                && !(隐含 && !e.is_network())
            {
                self.consecutive_absent += 1;
                self.缺席处置(
                    &items,
                    &pol,
                    site,
                    format!("判断器不可用（重试 {次} 次）：{}", e.0),
                    尝试,
                )?;
                return Ok(());
            }
            if 结果.is_ok() {
                self.consecutive_absent = 0;
            }
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
        // 本次运行见过的单次调用最大费用（步 15e：cost 预算按它限窗）
        self.c_max = Some(self.c_max.map_or(res.cost, |c| c.max(res.cost)));
        *layer_calls += 1;
        *layer_questions += items.len();
        let shares = res.mode_share;
        let res_perms = res.perms;
        let 置信 = res.confidence;
        // 一次调用里不止一道题 = 同材料合并发出（融合），账本条目记下是谁合并的（D8.2）
        let merged = items.len() > 1;
        // 合批的费用只记在首条（L7 2026-09-28，赛后欠账「合批花费重复记账」）：一次调用的 tokens/cost 记在本组
        // 第一条判断条目上，其余记 0，按条目相加即得实际花费；同调用号（`call`）的条目仍标 `merged_by: fuse`，
        // 按调用号去重的读者取最大值（`调用费`）。均摊会让「按调用号取首条」的读者（审计重放、契约 `spent`、
        // 案例 05 build-demo）少算，且与旧账本（每条整次费用）无法用同一条规则读，所以不均摊
        for (idx, ((q, r, key), a)) in items.iter().zip(res.answers.into_iter()).enumerate() {
            let (条目tokens, 条目费用) = if idx == 0 {
                (res.tokens, res.cost)
            } else {
                (0, 0.0)
            };
            // perms 跟着读数走：改 K 产生**新键**而不是覆盖旧值，两边并存
            // ——与「线重算之后已经发出的出口不改」是同一条纪律。
            // 测量与答案一起进账本（INTERFACE §四·二·七·五），重放与同键复用从账本取回。
            let perm = match shares.get(idx) {
                Some(Some(ms)) => Some(jpp_ledger::PermMeasure {
                    perms: res_perms.get(idx).copied().unwrap_or(0),
                    mode_share: *ms,
                }),
                _ => None,
            };
            let confidence = 置信.get(idx).copied().flatten();
            // 测量先落到读数上：下面的运行期证据（`evidence`）从读数取 `perms`/`mode_share`
            if let Some(pm) = perm {
                r.set_mode_share(pm.mode_share, pm.perms);
            }
            // 每题按自己的状态核（B155：同一次调用里各 `select` 的候选数可以不同）
            let state = &states[idx];
            if let Err(e) = self.validate_answer(&a, q, state, site) {
                // Z0556（过程记录 5.26）：语言自己发的元题形状不符，丢掉这道读数（不写 Judge、不填答案），记
                // `Absent{fail:shape}` 让重放照记录给同一个缺席标记，报 W-companion-shape，程序照常；作者自己的题照旧中止
                if !self.元题.contains(&r.id) {
                    return Err(e);
                }
                let 期望 = match q.op {
                    Op::Test => "是非读数 {noul: p}".to_string(),
                    Op::Select => format!("K 选一读数（{} 个候选的概率）", state.over.len()),
                    Op::Measure => format!("打分读数（{} 档）", q.scale.len()),
                };
                let detail = format!("期望{期望}，收到 {a:?}");
                self.记缺席账(&[(q.clone(), r.clone(), key.clone())], 形状不符, &detail, 0);
                self.absent_marks.insert(key.clone(), 形状不符.to_string());
                self.trace.warn(format!(
                    "W-companion-shape: @{} 语言自己发的元题「{}」的回答形状不符（{detail}）：丢掉这道读数，按没选出处理，程序照常（Z0556）",
                    site.start, q.text
                ));
                continue;
            }
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
                .map(|p| match self.账本查(p) {
                    Some(Entry::Judge { hop, .. }) => *hop,
                    _ => 0,
                })
                .max()
                .unwrap_or(0);
            self.记账(
                r.id,
                Entry::Judge {
                    key: key.clone(),
                    jkey: self.judge_keys.get(key).cloned(),
                    answer: a.clone(),
                    tokens: 条目tokens,
                    cost: 条目费用,
                    model_id: self.model_id.clone(),
                    call: self.cost.calls,
                    calib_ref: Some(self.judge_calib_ref(r)),
                    layer: self.layers.len() as u32 + 1,
                    merged_by: if merged { Some("fuse".into()) } else { None },
                    parents,
                    hop,
                    reused_from: None,
                    perm,
                    confidence,
                },
            );
            self.trace.push(
                "judge",
                key,
                false,
                条目费用,
                site,
                format!("「{}」", q.text),
            );
            // **运行期写入口的产出端**（`12`:347）。挂在这里而不是挂在「有读数产生」上，
            // 是因为重放路径（`interp.rs` 的 `ledger.get` 分支）根本不经过这里——
            // **重放于是天然不重复计数**，不需要再加一个「是不是重放」的开关。
            self.evidence.push((
                r.calib.clone(),
                jpp_effects::views::Sample {
                    p: match &a {
                        Answer::Noul(p) => Some(*p),
                        _ => None,
                    },
                    // 读数本身没有真值：真值通道是 `12`:347 未定的另一样
                    label: None,
                    perms: r.perms.get(),
                    mode_share: r.mode_share.get(),
                    mode: jpp_ir::key::LiteralMode::default(),
                    phys: q.op.phys().to_string(),
                    // 运行期这条路上没有簇 id：**读数不知道自己属于哪个对象段**。
                    // 留 `None`，于是它只能参与「按条」的认证——而「按条」会被如实写进证书。
                    cluster: None,
                    stratum: None,
                },
            ));
            self.fill_from_record(r, a.clone(), perm, confidence);
            // 步 19（B40）：同一运行里后来登记的同缓存键判断复用这一条
            self.记可复用判断(key, &a, perm, confidence);
            // 同键的其余读数（提前登记那些）也要填上，否则它们停在「没有答案」；
            // 置换测量一起填，否则它们的出口停在 `untested`
            for other in 同键[idx].iter().skip(1) {
                self.fill_from_record(other, a.clone(), perm, confidence);
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
        // 实际费用高于调用前的估计时，停的是**下一步**：下一组发出前的核对发不起就停发（B93）
        Ok(())
    }

    pub(crate) fn validate_answer(&self, a: &Answer, q: &Question, s: &State, sp: Span) -> R<()> {
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

/// Z0556：元题回答形状不符时的缺席标记与 `Absent` 记录的原因（`fail` 族，不是缺席类，可以放弃）
pub(crate) const 形状不符: &str = "fail:shape";

impl Interp<'_> {
    /// 判断条目的 `calib_ref`（步 27，B116 的一部分：步 20a-2 里「账本 `calib_ref` 填值」）。
    ///
    /// 题由题式填出（读数带 `form_hash`）时填 `key`（题式键 `\u{1f}form\u{1f}<form_hash>`，与 `CalibStore::form_key`
    /// 同格式，`tests/bank_version.rs` 钉住）、`kind`（登记读数时算出的精化题类，B120 (a)）、`fill`（填法，B107）；
    /// 手写题没有题式，三项为空，账本与旧格式逐字节相同。题库使用统计只读账本，靠这个键把判断归到题式。
    /// 元组序列化与迁移不在这里（归 20a-2 其余部分）。
    pub(crate) fn judge_calib_ref(&self, r: &Reading) -> Box<CalibRef> {
        let mut c = CalibRef::declared(&r.calib);
        if let Some(h) = &r.form_hash {
            c.key = Some(format!("\u{1f}form\u{1f}{h}"));
            c.kind = self
                .reading_kinds
                .get(&r.id)
                .and_then(|k| serde_json::to_value(k).ok())
                .and_then(|v| v.as_str().map(str::to_string));
            c.fill = self
                .questions
                .iter()
                .find(|x| x["q"] == r.q_hash.as_str())
                .and_then(|x| x["fill"].as_object())
                .map(|o| {
                    o.iter()
                        .map(|(k, v)| (k.clone(), v.as_str().unwrap_or_default().to_string()))
                        .collect()
                });
        }
        Box::new(c)
    }
}
