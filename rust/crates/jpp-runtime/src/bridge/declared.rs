//! 桥的声明线、冷读、打分与声明证据（`cut_declared`、`cut_stat_cold`、`cut_score`、`note_declared`）。
//! 步 41（C2a）从 `bridge.rs` 机械拆出，只搬代码、不改逻辑（`bridge.rs` 超 A8 的 1,500 行）。

use super::*;

/// 作者声明线（B128，步 20j-1）：`cut(r, {declare: {hi, lo?}})`。
impl<'a> Interp<'a> {
    /// 声明分支：按作者写的数切，不查记录定线、不看停岗、不平移 δ、不取严（B128）。判序：J-09 证据不足
    /// → Fail → 缺席 → 过线（`test`：p ≥ hi 为 act、p ≤ lo 为 ignore，其间 band；K 元：p_max ≥ hi 出
    /// pick / at，否则 band；`select` 的置换众数规则照旧）。等级 `Declared`；声明线以 `CalibUsed` 入账（B142）。
    /// 步 20j-3：`stat` 不是 `max` 时先经 `stat_of` 取统计量再过线（`{hi, lo}` 出 test 型出口，`cuts` 出
    /// `At(ℓ)`，出口的臂族随之，B153）；两端开闭按 `closed`（B165）。
    pub(crate) fn cut_declared(
        &mut self,
        r: &Reading,
        calib_key: Option<&str>,
        line: &DeclaredLine,
        stat: &Stat,
        feasible: Option<&Rc<Closure>>,
        sp: Span,
    ) -> R<Value> {
        self.flush("cut")?;
        let key = calib_key.unwrap_or(&r.calib).to_string();
        let 族 = 出口族(r.op, stat, Some(line));
        if let Some(missing) = r.missing_evidence.first() {
            return Ok(self.new_exit(
                ExitKind::Unsure(Why::with(UnsureCause::Insufficient, missing.clone())),
                None,
                族,
                &r.q_hash,
                &r.state_hash,
                r.state_taint,
                sp,
            ));
        }
        let absent = self.缺席因(r);
        let 用线 = r.fail.is_none() && absent.is_none();
        let 判序 = |answer: Option<Answer>| {
            jpp_value::bridge::decide(&jpp_value::bridge::CutInput {
                fail: r.fail.as_deref(),
                absent: absent.as_deref(),
                line: Some((line.hi, line.lo)),
                cost_requested: false,
                alpha_requested: false,
                answer,
                // 声明线按写的数用：不平移 δ（B128）
                delta: Some(jpp_value::stat::DECLARED_DELTA),
                mode_share: r.mode_share.get(),
            })
        };
        let (kind, untested) = if !用线 {
            判序(None)
        } else {
            let a = self.answer_of(r).expect("刷新之后答案必然在");
            if stat.is_max() && r.op != Op::Test {
                // K 元单侧线：置换众数规则照旧；上端开（B165）时 p_max 须严格越过 hi
                let p = jpp_value::stat::stat_of(&a, &Stat::Max, None).unwrap_or(f64::MIN);
                match 判序(Some(a)) {
                    (ExitKind::Pick(_) | ExitKind::At(_), _)
                        if !line.closed_hi && !jpp_value::stat::beyond_up(p, line.hi) =>
                    {
                        (ExitKind::Unsure(Why::of(UnsureCause::Band)), None)
                    }
                    k => k,
                }
            } else {
                let c = self.置信(r, &a);
                let s = match jpp_value::stat::stat_of(&a, stat, c) {
                    Ok(s) => s,
                    Err(jpp_value::stat::StatError::Options(m)) => {
                        return err(Some("E-cut-options"), format!("cut 的 stat：{m}"), sp);
                    }
                    Err(jpp_value::stat::StatError::Unavailable(m)) => {
                        // 依据：B154 (3)（不静默退回 p_max）
                        return err(
                            Some("E-stat-unavailable"),
                            format!(
                                "@{} cut 的 stat: \"confidence\" 取不到数：{m}。修法：换一个随答案报 confidence 的判断器（画像 H9 reports_confidence: true），或在夹具观察里给 confidence（B154）",
                                sp.start
                            ),
                            sp,
                        );
                    }
                };
                (jpp_value::bridge::past_declared(s, line), None)
            }
        };
        // C-4：已决 pick 之后按代码谓词改选；改选后的出口线不再担保，等级降为 `Answer`
        let (kind, 改选) = self.可行改选(r, feasible, kind, sp)?;
        if let Some((carrier, 修法)) = &untested {
            self.trace.warn(format!(
                "W-untested: @{} {carrier} 在本次路径上没有被测量，按 J-15 取保守项（出口 {}）。{修法}",
                sp.start,
                match &kind {
                    ExitKind::Unsure(w) => w.text(),
                    other => format!("{other:?}"),
                }
            ));
        }
        // C-4：改选后的出口线不再担保，不留线来源（与 `cut_inner` 的回答路径一致）
        let 线源 = if 用线 && 改选.is_none() {
            let mut s = if line.is_cuts() {
                format!("作者声明·cuts={:?}", line.cuts)
            } else {
                format!("作者声明·hi={}·lo={}", line.hi, line.lo)
            };
            if let Some(c) = line.closed_json() {
                s += &format!("·closed={c}");
            }
            if !stat.is_max() {
                s += &format!("·stat={}", stat.to_json());
            }
            s
        } else {
            String::new()
        };
        let 出口 = self.new_exit_from(
            kind,
            untested.map(|(carrier, _)| carrier),
            族,
            &r.q_hash,
            &r.state_hash,
            r.state_taint,
            线源,
            sp,
        );
        if let Value::Exit(e) = &出口
            && 用线
        {
            let site = self.站点(sp);
            self.note_declared(&key, site, line, stat);
            let 等级 = if 改选.is_some() {
                LineGrade::Answer
            } else {
                LineGrade::Declared
            };
            e.grade.set(Some(等级));
            // 宿主接受作者声明线放行（B128，步 20j-2）：在写报告行、登记谱系之前置位（两处都读 `releases()`）
            let 接受 = self.entry.accept.declared_lines;
            e.host_accepts_declared.set(接受);
            let evidence = self.声明证据(&key, r, line, stat);
            let n = evidence["labelled"].as_u64().unwrap_or(0);
            // J-08 拒绝报文的「宿主未声明接受作者线」补句只对未接受的出口成立
            if !接受 && 改选.is_none() {
                self.声明出口.insert(
                    e.id,
                    format!("@{} {}，标注 {n} 条", sp.start, 声明说明(line, stat)),
                );
            }
            // 报告的 `declared`：线的数、站点与非缺省端位（缺省值不写，20j-1 的行逐字节不变）
            let mut declared = line.numbers_json();
            declared.insert("site".into(), json!(sp.start));
            if let Some(c) = line.closed_json() {
                declared.insert("closed".into(), c);
            }
            let mut row = json!({
                "site": sp.start, "exit": e.label(), "grade": 等级.name(), "releases": e.releases(),
                "item": 材料摘要(&r.state_hash), "key": key,
                "declared": declared,
                "evidence": evidence,
            });
            if let Some(f) = &改选 {
                self.记改选(&mut row, f, true, sp);
            }
            self.exit_grades.push(row);
        }
        Ok(出口)
    }

    /// C-4：已决 `pick(k)` 之后按作者的代码谓词 `feasible(k)` 改选。
    ///
    /// 只在出口已经是 `Pick` 时动：`unsure`、`act`、`ignore`、`at` 原样返回，谓词零调用（未决没有可改的选择，
    /// 替它选是把未决二值化，J-05 要禁的）。检验顺序：判断器已决的 k 先验；不可行则其余候选按概率降序
    /// （同概率按下标升序）逐个验，第一个可行的成为新的 `Pick`；全不可行出 `Unsure("infeasible")`。
    /// 判断器的读数不改，只改出口。原 pick 可行时返回原出口、不留记录。
    /// 依据：`规划/骨架候选.md` 2.0.2 C-4；`附注/2026-09-27-world港城线-回报.md` 第二节第 2 条；意图汇编 7a。
    pub(super) fn 可行改选(
        &mut self,
        r: &Reading,
        feasible: Option<&Rc<Closure>>,
        kind: ExitKind,
        sp: Span,
    ) -> R<(ExitKind, Option<改选记录>)> {
        let (Some(f), ExitKind::Pick(k0)) = (feasible, &kind) else {
            return Ok((kind, None));
        };
        let k0 = *k0;
        let Some(Answer::Choice(v)) = self.answer_of(r) else {
            return Ok((kind, None));
        };
        let mut 序: Vec<usize> = (0..v.len()).filter(|i| *i != k0).collect();
        序.sort_by(|a, b| {
            v[*b]
                .partial_cmp(&v[*a])
                .unwrap_or(std::cmp::Ordering::Equal)
                .then(a.cmp(b))
        });
        序.insert(0, k0);
        let mut skipped: Vec<(usize, f64)> = vec![];
        for k in 序 {
            if self.调可行谓词(f, k, sp)? {
                if skipped.is_empty() {
                    return Ok((kind, None));
                }
                return Ok((
                    ExitKind::Pick(k),
                    Some(改选记录 {
                        from: k0,
                        chosen: Some(k),
                        skipped,
                        of: r.ledger_key.clone(),
                    }),
                ));
            }
            skipped.push((k, v[k]));
        }
        Ok((
            ExitKind::Unsure(Why::of(UnsureCause::Infeasible)),
            Some(改选记录 {
                from: k0,
                chosen: None,
                skipped,
                of: r.ledger_key.clone(),
            }),
        ))
    }

    /// 求一次可行性谓词：普通 `.jpp` 函数（可捕获词法环境），返回 `Bool`；体内禁效应与内核构造（与声明式拟合同一口径，
    /// 计数 `拟合中`）。k 是候选在 `over` 里的原序下标（与 `pick(k)` 同一个 k）。
    pub(crate) fn 调可行谓词(&mut self, f: &Rc<Closure>, k: usize, sp: Span) -> R<bool> {
        self.拟合中 += 1;
        let r = self.call_closure(f, vec![Value::Int(k as i64, Provenance::trusted())], sp);
        self.拟合中 -= 1;
        match r? {
            Value::Bool(b, ..) => Ok(b),
            other => err(
                Some("E-rt-type"),
                format!(
                    "cut 的 feasible 要返回布尔，收到 {}（{}）",
                    other.type_name(),
                    other.to_json()
                ),
                sp,
            ),
        }
    }

    /// **改选记账的唯一写入点**（C-4 路 A）：报告 `exits` 行的 `feasible` 字段加一条 trace 提示。
    /// 账本事件分类型（C-1）落地后，跳过项改写进账本只改这一个函数。`降级` 为真表示原来有线担保、改选后降为 `Answer`。
    pub(super) fn 记改选(&mut self, row: &mut Json, f: &改选记录, 降级: bool, sp: Span) {
        self.记改选入账(&f.of, f.from, f.chosen, &f.skipped, sp); // C-1：账本记 Reselect
        let 跳过: Vec<Json> = f
            .skipped
            .iter()
            .map(|(k, p)| json!({"k": k, "p": p}))
            .collect();
        let mut rec = json!({"from": f.from, "chosen": f.chosen, "skipped": 跳过});
        if 降级 {
            rec["downgraded"] = json!("因改选降级");
        }
        row["feasible"] = rec;
        let 去向 = match f.chosen {
            Some(k) => format!("改选 pick({k})"),
            None => "没有可行候选，出口 Unsure(infeasible)".to_string(),
        };
        self.trace.warn(format!(
            "N-feasible-skip: @{} 判断器已决 pick({})，代码谓词否决了 {} 个候选（{}），{去向}",
            sp.start,
            f.from,
            f.skipped.len(),
            f.skipped
                .iter()
                .map(|(k, p)| format!("k={k} p={p:.3}"))
                .collect::<Vec<_>>()
                .join("，")
        ));
    }

    /// `stat` 不是 `max` 而没有 `declare`（B153 (1)、B154 (2)）：认证在 p_max 上的线对别的统计量无效，不借，不查记录
    /// （不调 `note_calib`）。B187（批 9 第 2 格）：`mass` 按概率和的多数块走（> 0.5 act、< 0.5 ignore、恰等于 0.5 为
    /// `Unsure(tie)`），等级 `Answer`；别的统计量不写线在 `cut` 解析时已报 `E-cut-options`，走不到这里。
    /// 判序：J-09 证据不足 → Fail → 缺席 → 按回答。
    pub(crate) fn cut_stat_cold(
        &mut self,
        r: &Reading,
        calib_key: Option<&str>,
        stat: &Stat,
        sp: Span,
    ) -> R<Value> {
        self.flush("cut")?;
        let key = calib_key.unwrap_or(&r.calib).to_string();
        let 族 = 出口族(r.op, stat, None);
        if let Some(missing) = r.missing_evidence.first() {
            return Ok(self.new_exit(
                ExitKind::Unsure(Why::with(UnsureCause::Insufficient, missing.clone())),
                None,
                族,
                &r.q_hash,
                &r.state_hash,
                r.state_taint,
                sp,
            ));
        }
        let absent = self.缺席因(r);
        let 回答路径 = r.fail.is_none() && absent.is_none();
        let (kind, untested) = if !回答路径 {
            jpp_value::bridge::decide(&jpp_value::bridge::CutInput {
                fail: r.fail.as_deref(),
                absent: absent.as_deref(),
                line: None,
                cost_requested: false,
                alpha_requested: false,
                answer: None,
                delta: None,
                mode_share: None,
            })
        } else if matches!(stat, Stat::Mass(_)) {
            let a = self.answer_of(r).expect("刷新之后答案必然在");
            let s = match jpp_value::stat::stat_of(&a, stat, None) {
                Ok(s) => s,
                Err(jpp_value::stat::StatError::Options(m))
                | Err(jpp_value::stat::StatError::Unavailable(m)) => {
                    // 依据：B153（mass 的块须是合法选项下标）；B187（没有线的 mass 按回答走）
                    return err(Some("E-cut-options"), format!("cut 的 stat：{m}"), sp);
                }
            };
            // 概率和是「答案落在这几块里」的概率：按它的多数块走（B187）
            (
                jpp_value::bridge::follow_answer(&Answer::Noul(s), None),
                None,
            )
        } else {
            jpp_value::bridge::cold_for_stat(stat)
        };
        // 依据：B153 (1)（认证线在 p_max 上，对别的统计量无效，不借）、B130（冷出口告警一趟一键一条）
        let 报 = untested.is_some()
            && self.unknown_reported.insert(format!(
                "W-untested-calib_line\u{1f}{key}\u{1f}{}",
                stat.to_json()
            ));
        if let Some((carrier, 修法)) = untested.as_ref().filter(|_| 报) {
            // 依据：J-15（未测取保守并带修法）；B153 (1)
            self.trace.warn(format!(
                "W-untested: @{} {carrier} 在本次路径上没有被测量，按 J-15 取保守项（出口 {}）。{修法}",
                sp.start,
                match &kind {
                    ExitKind::Unsure(w) => w.text(),
                    other => format!("{other:?}"),
                }
            ));
        }
        let 出口 = self.new_exit_from(
            kind,
            untested.map(|(carrier, _)| carrier),
            族,
            &r.q_hash,
            &r.state_hash,
            r.state_taint,
            String::new(),
            sp,
        );
        if let Value::Exit(e) = &出口 {
            let 等级 = if 回答路径 {
                LineGrade::Answer
            } else {
                LineGrade::Cold
            };
            e.grade.set(Some(等级));
            self.exit_grades.push(json!({
                "site": sp.start, "exit": e.label(), "grade": 等级.name(), "releases": e.releases(),
                "item": 材料摘要(&r.state_hash), "key": key,
            }));
        }
        Ok(出口)
    }

    /// `cut(Score, …)`（B153 (2)，步 20j-4）：声明式拟合的结果只走声明分支，按作者写的数切，等级 `Declared`
    /// （放行经 `--release-on-declared`）；不带 `declare` 即冷（本版不查记录）。输入不可用时出对应的未决。
    /// 出口的账本键是合成键 `fit:<fit_hash>#<id>`，谱系放行经旁表追到各输入读数（B72-4）。
    /// 依据：B153 (2)（地基/附注/2026-09-26-批6裁定.md §一）；B128；B142
    pub(crate) fn cut_score(&mut self, s: &Rc<Score>, opts: CutOpts, sp: Span) -> R<Value> {
        // G2：分数过线建的出口，欠账记号的过桥种类是 `cut_score`
        let 旧 = std::mem::replace(&mut self.当前桥, jpp_ledger::Via::CutScore);
        let r = self.cut_score_body(s, opts, sp);
        self.当前桥 = 旧;
        r
    }

    fn cut_score_body(&mut self, s: &Rc<Score>, opts: CutOpts, sp: Span) -> R<Value> {
        let key = format!("fit:{}", s.fit_hash);
        let 合成键 = format!("fit:{}#{}", s.fit_hash, s.id);
        self.拟合谱系
            .borrow_mut()
            .insert(合成键.clone(), s.input_keys.clone());
        let 值 = self.拟合表.borrow().get(&s.id).copied();
        let line = opts.declare.clone();
        let 族 = if line.as_ref().is_some_and(|l| l.is_cuts()) {
            Op::Measure
        } else {
            Op::Test
        };
        let (kind, untested) = match (&s.fail, &line, 值) {
            (Some(f), _, _) => (ExitKind::Unsure(f.clone()), None),
            (None, Some(l), Some(v)) => (jpp_value::bridge::past_declared(v, l), None),
            _ => (
                ExitKind::Unsure(Why::of(UnsureCause::Cold)),
                Some((
                    "calib_line".to_string(),
                    "修法【作者可改】：声明式拟合没有认证通道（本版不查记录，B153），拟合分数也不是判断器的回答；按你的数切写 cut(s, {declare: {hi: …, lo: …}})（B128）".to_string(),
                )),
            ),
        };
        // 依据：J-15（未测取保守并带修法）；B130（冷出口告警一趟一键一条）
        let 报 = untested.is_some()
            && self
                .unknown_reported
                .insert(format!("W-untested-calib_line\u{1f}{key}"));
        if let Some((carrier, 修法)) = untested.as_ref().filter(|_| 报) {
            // 依据：J-15；B153 (2)（声明式拟合不查记录）
            self.trace.warn(format!(
                "W-untested: @{} {carrier} 在本次路径上没有被测量，按 J-15 取保守项（出口 cold）。{修法}",
                sp.start
            ));
        }
        let 用线 = s.fail.is_none() && line.is_some() && 值.is_some();
        let 线源 = match (&line, 用线) {
            (Some(l), true) => format!("作者声明·fit={}·{}", s.fit_hash, l.describe()),
            _ => String::new(),
        };
        let 出口 = self.new_exit_from(
            kind,
            untested.map(|(carrier, _)| carrier),
            族,
            &format!("fit:{}", s.fit_hash),
            &s.state_hash,
            s.taint,
            线源,
            sp,
        );
        let Value::Exit(e) = &出口 else {
            return Ok(出口);
        };
        *e.ledger_key.borrow_mut() = 合成键;
        let 已记 = self.exit_grades.len();
        let 基本 = json!({
            "site": sp.start, "item": 材料摘要(&s.state_hash), "key": key,
            "fit": s.fit_hash, "inputs": s.inputs,
        });
        let mut row = 基本;
        if let (true, Some(l), Some(v)) = (用线, &line, 值) {
            let site = self.站点(sp);
            self.note_declared_with(&key, site, l, &Stat::Max, Some((&s.fit_hash, &s.inputs)));
            e.grade.set(Some(LineGrade::Declared));
            let 接受 = self.entry.accept.declared_lines;
            e.host_accepts_declared.set(接受);
            if !接受 {
                self.声明出口.insert(
                    e.id,
                    format!("@{} fit={} {}", sp.start, s.fit_hash, l.describe()),
                );
            }
            let mut declared = l.numbers_json();
            declared.insert("site".into(), json!(sp.start));
            if let Some(c) = l.closed_json() {
                declared.insert("closed".into(), c);
            }
            row["declared"] = Json::Object(declared);
            // 声明式拟合没有标注与认证线：`near_line` 在运行结束时按本趟同拟合的数算
            row["evidence"] = json!({
                "labelled": 0, "errors_at_line": Json::Null,
                "near_line": {"window": 声明窗口, "count": 0, "share": 0.0},
                "certified": Json::Null,
            });
            self.键读数.entry(key.clone()).or_default().push(v);
        } else {
            e.grade.set(Some(LineGrade::Cold));
        }
        row["exit"] = json!(e.label());
        row["grade"] = json!(e.grade.get().map(|g| g.name()).unwrap_or("Cold"));
        row["releases"] = json!(e.releases());
        self.exit_grades.push(row);
        self.exit_rows.insert(e.id, 已记);
        self.登记出口放行(e);
        Ok(出口)
    }

    /// 读数的自报置信度（B154）：判断器随答案报了（本趟发出或从账本取回）就用它；没报而判断实例是固定观察
    /// 端口时取夹具缺省 p_max（B154 (1)「缺省 = p_max」；只凭账本重放用账本头的 model_id，同为固定端口）；
    /// 其他判断器没报即没有（不退回 p_max，B154 (3)）。
    pub(crate) fn 置信(&self, r: &Reading, a: &Answer) -> Option<f64> {
        if let Some(c) = self.置信表.borrow().get(&r.id) {
            return Some(*c);
        }
        if r.model_id == jpp_effects::FIXED_MODEL {
            return jpp_value::stat::stat_of(a, &Stat::Max, None).ok();
        }
        None
    }

    /// 声明线入账（B142，步 20j-1）：追加 `CalibUsed`，键 `declared:<校准键>@<站点>`，记录
    /// `{line: "declared", hi, lo, site}`；本趟同键只记一次。续接或重放时账本已有该键而数不同，报 `W-header`
    /// （并列、不拒绝，与 B83 换线同）。依据：B142（地基/附注/2026-09-25-库层批4裁定.md §五）
    /// 步 20j-3：记录加 `stat`（不是 `max` 才写）、`cuts`（代替 `hi`/`lo`）、`closed`（不是全闭才写）；缺省值不写，
    /// 20j-1 记下的普通声明线记录哈希不变（旧账本续接、重放不因本步报 `W-header`）。
    pub(crate) fn note_declared(
        &mut self,
        key: &str,
        site: SiteRef,
        line: &DeclaredLine,
        stat: &Stat,
    ) {
        self.note_declared_with(key, site, line, stat, None)
    }

    /// 同 [`Self::note_declared`]；声明式拟合（B153 (2)，步 20j-4）另写 `fit` 与 `inputs`，键为 `declared:fit:<fit_hash>@<站点>`
    pub(crate) fn note_declared_with(
        &mut self,
        key: &str,
        site: SiteRef,
        line: &DeclaredLine,
        stat: &Stat,
        拟合: Option<(&str, &[String])>,
    ) {
        let k = format!("{}{key}@{site}", jpp_ledger::DECLARED_PREFIX);
        // 站点走到过（`声明站点核对` 据此跳过）；去重按记录哈希，见下
        self.本趟已记校准.insert(k.clone());
        let mut m = serde_json::Map::new();
        m.insert("line".into(), json!("declared"));
        if let Some((fit, inputs)) = 拟合 {
            m.insert("fit".into(), json!(fit));
            m.insert("inputs".into(), json!(inputs));
        }
        m.extend(line.numbers_json());
        m.insert("site".into(), json!(site));
        if !stat.is_max() {
            m.insert("stat".into(), stat.to_json());
        }
        if let Some(c) = line.closed_json() {
            m.insert("closed".into(), c);
        }
        let record = Json::Object(m);
        let h = jpp_value::value::hash_of(&[&record.to_string()]);
        // B175 (3)（步 20j-3 追加 (8)）：按记录哈希去重——同一站点每个不同的线（例如 `hi: rand(seed, k)`）各记一条，
        // 同一个线只记一次
        if !self.本趟已记声明.insert((k.clone(), h.clone())) {
            return;
        }
        // 续接与重放：账本里该键有同哈希的条目即一致，不再追加（`记校准` 只看该键最后一条，多线时会重复追加）。
        // 旧记录 = 该键哈希不是本趟记过的条目（本趟自己写的不算旧，首跑的第二个线不报）；没有同哈希而有旧记录才报，
        // 每键每趟一条（空哈希作「已报」标记，真哈希不会是空串）
        let (同, 旧) = {
            let 本趟: HashSet<&str> = self
                .本趟已记声明
                .iter()
                .filter(|(kk, _)| kk == &k)
                .map(|(_, hh)| hh.as_str())
                .collect();
            let mut 同 = false;
            let mut 旧 = None;
            for e in &self.ledger.view().entries {
                if let Entry::CalibUsed { key, hash, record } = e
                    && key == &k
                {
                    if hash == &h {
                        同 = true;
                    } else if !本趟.contains(hash.as_str()) {
                        旧 = Some(record.clone());
                    }
                }
            }
            (同, 旧)
        };
        if 同 {
            return;
        }
        if let Some(old) = 旧
            && self.本趟已记声明.insert((k.clone(), String::new()))
        {
            // 依据：B142 (3)；J-18（账本头不同即不承诺重放一致）
            self.trace.warn(format!(
                "W-header: 账本头不同，不承诺重放一致：declared 旧 {} 新 {} @{site}（B142）",
                记录说明(&old),
                声明说明(line, stat)
            ));
        }
        // 步 18b：经账本端口追加（与 `note_calib` 同一条路，层末落盘）；端口报错先记下，层末报 `E-ledger-io`
        self.记校准(&k, &h, record);
    }

    /// `CalibUsed` 入账（B124：账本里该键最后一条的哈希相同就不写）。步 15h-3：生成的层开着时进层，按登记序
    /// 入账（B160）；校准键本趟只到这里一次（`本趟已记校准`）；声明键按记录哈希去重，同站点多线各到一次（20j-3 (8)），层按登记序逐条落盘、不按键合并。
    pub(super) fn 记校准(&mut self, key: &str, hash: &str, record: Json) {
        if self.ledger.view().calib_used_same(key, hash) {
            return;
        }
        self.登记记账(Entry::CalibUsed {
            key: key.to_string(),
            hash: hash.to_string(),
            record,
        });
    }

    /// 运行结束时（B142 (3)）：账本里有 `declared:` 键、而程序里该站点已没有声明，报 `W-header`。
    /// 「该站点仍有声明」按静态看：该站点的 `cut` 带含 `declare` 的记录字面量，或参数不是字面量（可能仍有
    /// 声明，按有计，零误报）。本趟走到过的站点已在 [`Self::note_declared`] 比过，这里跳过。
    pub(crate) fn 声明站点核对(&mut self, program: &jpp_ir::ir::Program) {
        use jpp_ir::ir::{Host, Node};
        // B0630：站点按本趟键法比（结构化标识或旧账本的字节偏移），记录里的 `site` 数与串都认
        let mut 声明span: Vec<Span> = vec![];
        jpp_ir::ir::walk(&program.body, &mut |e| {
            if let Node::Cut { rest, .. } = &e.node {
                let 有 = rest.iter().any(|a| match &a.node {
                    Node::Host(Host::Record(fs)) => fs.iter().any(|(k, _)| k == "declare"),
                    Node::Host(Host::Text(_)) => false,
                    _ => true,
                });
                if 有 {
                    声明span.push(e.span);
                }
            }
        });
        let 可能有声明: HashSet<String> = 声明span
            .into_iter()
            .map(|sp| self.站点(sp).to_string())
            .collect();
        let 旧: Vec<(String, Json)> = self
            .ledger
            .view()
            .calib_used
            .iter()
            .filter(|(k, _)| {
                k.starts_with(jpp_ledger::DECLARED_PREFIX) && !self.本趟已记校准.contains(*k)
            })
            .map(|(k, v)| (k.clone(), v["record"].clone()))
            .collect();
        for (_, r) in 旧 {
            let site = match &r["site"] {
                Json::String(s) => s.clone(),
                other => other.as_u64().unwrap_or_default().to_string(),
            };
            if !可能有声明.contains(&site) {
                // 依据：B142 (3)
                self.trace.warn(format!(
                    "W-header: 账本头不同，不承诺重放一致：declared 旧 {} 新 无 @{site}（程序里该站点已没有声明线，B142）",
                    记录说明(&r)
                ));
            }
        }
    }

    /// 声明线旁的证据（B128；`20` v2 §4.4 第 9 条）：同键记录只读。证据记录取查找链上第一条有线的记录
    /// （题级 → 题式级 → 类级），都没有取题键记录。`near_line` 在运行结束时由 [`Self::声明证据定稿`] 填。
    /// 步 20j-3：同键标注样本是 p_max 上的，`stat` 不是 `max`（或 `cuts` 线）时无从核，`errors_at_line` 为 `null`
    /// （`labelled` 照数，`certified` 照给作对照）；开端（B165）按严格比较数错。
    fn 声明证据(&self, key: &str, r: &Reading, line: &DeclaredLine, stat: &Stat) -> Json {
        let 链 = self.calib.chain(key, r.form_hash.as_deref());
        let 有线 =
            |l: &jpp_effects::views::Link| matches!(l.rec.status.as_str(), "上岗" | "停岗候选");
        let 层们 = [
            (Some(&链.question), "题级"),
            (链.form.as_ref(), "题式级"),
            (链.class.as_ref(), "类级"),
        ];
        let 有线层 = 层们
            .iter()
            .find_map(|(l, 层)| l.filter(|l| 有线(l)).map(|l| (l, *层)));
        let 证据键 = 有线层.map_or(链.question.key.as_str(), |(l, _)| l.key.as_str());
        let 样本 = self.calib.labelled(证据键);
        let 单侧 = r.op != Op::Test;
        let 可核 = stat.is_max() && !line.is_cuts();
        let 上 = |p: f64| {
            if line.closed_hi {
                jpp_value::stat::decided_up(p, line.hi, jpp_value::stat::DECLARED_DELTA)
            } else {
                jpp_value::stat::beyond_up(p, line.hi)
            }
        };
        let 下 = |p: f64| {
            if line.closed_lo {
                jpp_value::stat::decided_down(p, line.lo, jpp_value::stat::DECLARED_DELTA)
            } else {
                jpp_value::stat::beyond_down(p, line.lo)
            }
        };
        let 错 = 样本
            .iter()
            .filter(|(p, 对)| {
                if 上(*p) {
                    !对
                } else {
                    !单侧 && 下(*p) && *对
                }
            })
            .count();
        let certified = match 有线层 {
            Some((l, 层)) => {
                json!({"hi": l.rec.hi, "lo": l.rec.lo, "grade": 记录等级名(&l.rec, 层), "key": l.key})
            }
            None => Json::Null,
        };
        json!({
            "labelled": 样本.len(),
            "errors_at_line": if 样本.is_empty() || !可核 { Json::Null } else { json!(错) },
            "near_line": {"window": 声明窗口, "count": 0, "share": 0.0},
            "certified": certified,
        })
    }

    /// 运行结束时填 `near_line`（本趟该键全部切过的读数里，落在线 ±0.2 内的条数与占比）并发
    /// `W-declared-line`（一趟一键一条）。依据：B128；`20` v2 §4.4 第 9 条
    pub(crate) fn 声明证据定稿(&mut self) {
        let 接受 = self.entry.accept.declared_lines;
        let mut 已报 = HashSet::new();
        let mut 告警 = vec![];
        for row in self.exit_grades.iter_mut() {
            // 线的数：`{hi, lo}` 或 `cuts`（步 20j-3）；不是声明行即跳过
            let d = &row["declared"];
            let (线们, 线文) = match (d["hi"].as_f64(), d["lo"].as_f64(), d["cuts"].as_array()) {
                (Some(hi), Some(lo), _) => (vec![hi, lo], format!("hi={hi} lo={lo}")),
                (_, _, Some(c)) => (
                    c.iter().filter_map(|x| x.as_f64()).collect::<Vec<f64>>(),
                    format!("cuts={}", d["cuts"]),
                ),
                _ => continue,
            };
            let 线文 = match d.get("closed") {
                Some(c) => format!("{线文} closed={c}"),
                None => 线文,
            };
            let key = row["key"].as_str().unwrap_or_default().to_string();
            // 统计量（行上只在不是 max 时有）：`键读数` 的组与告警去重都按（键，统计量）
            let 统计量 = row.get("stat").cloned();
            let 组键 = match &统计量 {
                Some(s) => format!("{key}\u{1f}{s}"),
                None => key.clone(),
            };
            let ps = self.键读数.get(&组键).cloned().unwrap_or_default();
            let 近 = |x: f64, 线: f64| (x - 线).abs() <= 声明窗口 + jpp_value::stat::BOUNDARY_EPS;
            let count = ps
                .iter()
                .filter(|x| 线们.iter().any(|线| 近(**x, *线)))
                .count();
            let share = if ps.is_empty() {
                0.0
            } else {
                (count as f64 / ps.len() as f64 * 1e4).round() / 1e4
            };
            row["evidence"]["near_line"] =
                json!({"window": 声明窗口, "count": count, "share": share});
            if 已报.insert(组键) {
                let ev = &row["evidence"];
                // 步 20j-2：宿主接受与否写进末句。意图汇编 11a：只在开放行把关（--guard）时说放行，默认不说
                let 放行 = if !self.guard {
                    ""
                } else if 接受 {
                    "；宿主已接受作者线放行（--release-on-declared），不可逆动作的后果由宿主担责"
                } else {
                    "；放行不可逆动作须宿主接受作者线（CLI：--release-on-declared）"
                };
                let 错 = match (ev["errors_at_line"].as_u64(), &统计量) {
                    (Some(k), _) => format!("按此线切错 {k} 条"),
                    (None, Some(s)) => format!("标注在 p_max 上，对 stat={s} 无从核"),
                    (None, None) => "无标注可核".to_string(),
                };
                let 线文 = match &统计量 {
                    Some(s) => format!("{线文} stat={s}"),
                    None => 线文,
                };
                // 声明式拟合（B153 (3)，步 20j-4）：告警写拟合的身份
                let 线文 = match row.get("fit").and_then(|f| f.as_str()) {
                    Some(f) => format!("{线文} fit={f}"),
                    None => 线文,
                };
                let 认证 = match ev["certified"].as_object() {
                    Some(c) => format!(
                        "同键认证线 hi={} lo={}（{}）",
                        c["hi"],
                        c["lo"],
                        c["grade"].as_str().unwrap_or_default()
                    ),
                    None => "同键无认证线".to_string(),
                };
                // 依据：B128（地基/附注/2026-09-25-作者主权与策略表达裁定.md §一）
                告警.push(format!(
                    "W-declared-line: @{} 键 {key} 用作者声明线 {线文}（B128）：同键标注 {} 条，{错}；本趟线附近 ±{声明窗口} 的读数 {count}/{}（{share}）；{认证}。出口按你写的数路由，不作错误率保证{放行}",
                    row["declared"]["site"],
                    ev["labelled"],
                    ps.len()
                ));
            }
        }
        for w in 告警 {
            self.线等级告警(w);
        }
    }

    /// 线等级与正交位的告警（B187 批 9 第 10 格）：数据已在报告 `exits` 行，默认不作告警；宿主开 `--guard` 时照发
    /// （它们解释为什么不放行）。`W-drift`、`W-form-pending`、J-15 的 `W-untested` 不走这里。
    pub(crate) fn 线等级告警(&mut self, w: String) {
        if self.guard {
            self.trace.warn(w);
        }
    }
}

/// `near_line` 的半宽：常量在 `jpp-value::stat` 的常量表（基线刷新 2026-09-26 挪入）
use jpp_value::stat::NEAR_LINE_WINDOW as 声明窗口;

/// 声明线的一段文字（J-08 拒绝报文、`W-header`）：`hi=0.7 lo=0.3`、`cuts=[0.5, 1.5]`，非缺省端位与统计量附后
fn 声明说明(line: &DeclaredLine, stat: &Stat) -> String {
    if stat.is_max() {
        line.describe()
    } else {
        format!("{} stat={}", line.describe(), stat.to_json())
    }
}

/// 账本里一条声明记录的文字（`W-header` 的「旧」）：与 [`声明说明`] 同形
fn 记录说明(r: &Json) -> String {
    let mut s = match r.get("cuts") {
        Some(c) => format!("cuts={c}"),
        None => format!("hi={} lo={}", r["hi"], r["lo"]),
    };
    if let Some(c) = r.get("closed") {
        s += &format!(" closed={c}");
    }
    if let Some(x) = r.get("stat") {
        s += &format!(" stat={x}");
    }
    s
}

/// 记录的线等级名（`evidence.certified.grade`）：夹具 / 类级 / 试用 / 临时上岗 / 题式级 / 题级，与出口等级同名
fn 记录等级名(rec: &Lookup, 层: &str) -> &'static str {
    let g = if rec.fixture_line() {
        LineGrade::Fixture
    } else if 层 == "类级" {
        LineGrade::Class
    } else if rec.selected.as_ref().is_some_and(|c| c.trial) {
        LineGrade::Trial
    } else if rec.selected.as_ref().is_some_and(|c| c.provisional)
        || rec
            .truth_gate
            .as_ref()
            .is_some_and(|g| g.starts_with("临时上岗"))
    {
        LineGrade::Provisional
    } else if 层 == "题式级" {
        LineGrade::Form
    } else {
        LineGrade::Certified
    };
    g.name()
}
