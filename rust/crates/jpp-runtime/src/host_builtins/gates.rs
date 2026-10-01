//! 闸与补信息：`known_answers`、`state_within`、`gate_info`、`split_point`。步 36 G3 从 `host_builtins.rs` 原样搬出（只搬不改）。

use super::*;

impl<'a> Interp<'a> {
    /// `known_answers(题, 读数列表, 期望列表[, {gap?, safety?, tolerance?}]) -> {pass: Bool, code: Text}`：
    /// 验题闸门的已知答案一半（步 28，B0468）。原定义 `04-语言规范-v0.md` §5.4「≥3 act + ≥3 ignore；
    /// gap = min(p|act) − max(p|ignore) ≥ 0.20（safety 0.30）」；程度题按原实现（`地基/foundation/core/probe.py`
    /// `_counts_ok`、`_judge`；`params.py` `score_tolerance = 0.5`）：每档 ≥ 2 道验题，每道读数的期望档
    /// Σ i·p_i 与期望档相差 ≤ 0.5。读数的数不回 `.jpp`，只回通过与否和码：`ok`、`known-count`、`known-gap`、
    /// `known-level`、`known-unread`（有读数不可用）；K 选一的验题归 B0485，这里回 `pass: true`、码
    /// `known-select-b0485`。读答案前刷新（与构造内部 `cut` 当场读答案过判据同类）。
    pub(crate) fn b_known_answers(
        &mut self,
        name: &'static str,
        args: Vec<Value>,
        sp: Span,
    ) -> R<Value> {
        if args.len() != 3 && args.len() != 4 {
            return err(
                Some("E-rt-arity"),
                format!(
                    "{name} 需要 3 或 4 个参数（题, 读数列表, 期望列表[, 选项]），收到 {}",
                    args.len()
                ),
                sp,
            );
        }
        let Value::Question(q) = &args[0] else {
            return err(Some("E-rt-arg"), "known_answers 的第一个参数要是题", sp);
        };
        let (Value::List(rs), Value::List(xs)) = (&args[1], &args[2]) else {
            return err(
                Some("E-rt-arg"),
                "known_answers(题, 读数列表, 期望列表)：后两个参数要是列表",
                sp,
            );
        };
        if rs.len() != xs.len() {
            return err(
                Some("E-rt-arg"),
                format!(
                    "known_answers：读数 {} 条、期望 {} 条，要一一对应",
                    rs.len(),
                    xs.len()
                ),
                sp,
            );
        }
        let num = |k: &str, d: f64| -> R<f64> {
            match args.get(3).and_then(|o| o.get(k)) {
                None | Some(Value::Unit) => Ok(d),
                Some(Value::Float(f, _)) => Ok(f),
                Some(Value::Int(i, _)) => Ok(i as f64),
                Some(other) => err(
                    Some("E-rt-arg"),
                    format!("known_answers 的 {k} 要是数，收到 {}", other.type_name()),
                    sp,
                ),
            }
        };
        let safety = matches!(
            args.get(3).and_then(|o| o.get("safety")),
            Some(Value::Bool(true, _, _))
        );
        // 04-v0 §5.4 的两个门槛（safety 用后者）与原实现的档位容差
        let gap_min = num("gap", if safety { 0.30 } else { 0.20 })?;
        let tolerance = num("tolerance", 0.5)?;
        let out = |pass: bool, code: &str| {
            Ok(Value::record(vec![
                ("pass".into(), Value::bool(pass)),
                ("code".into(), Value::text(code)),
            ]))
        };
        if q.op == Op::Select {
            return out(true, "known-select-b0485");
        }
        let mut readings = vec![];
        for r in rs.iter() {
            let Value::Reading(r) = r else {
                return err(
                    Some("E-rt-arg"),
                    "known_answers 的读数列表里要是读数（judge 的结果）",
                    sp,
                );
            };
            readings.push(r.clone());
        }
        self.flush("cut")?;
        let answers: Vec<Option<Answer>> = readings
            .iter()
            .map(|r| {
                if r.fail.is_some() {
                    None
                } else {
                    self.answer_of(r)
                }
            })
            .collect();
        if answers.iter().any(Option::is_none) {
            return out(false, "known-unread");
        }
        match q.op {
            Op::Test => {
                let (mut acts, mut igns) = (vec![], vec![]);
                for (a, x) in answers.iter().zip(xs.iter()) {
                    let Some(Answer::Noul(p)) = a else {
                        return err(
                            Some("E-rt-arg"),
                            "known_answers：是非题的读数要是是非读数",
                            sp,
                        );
                    };
                    match x {
                        Value::Text(t, _) if t.as_ref() == "act" => acts.push(*p),
                        Value::Text(t, _) if t.as_ref() == "ignore" => igns.push(*p),
                        _ => {
                            return err(
                                Some("E-rt-arg"),
                                "known_answers：是非题的期望写 \"act\" 或 \"ignore\"",
                                sp,
                            );
                        }
                    }
                }
                if acts.len() < 3 || igns.len() < 3 {
                    return out(false, "known-count");
                }
                let lo_act = acts.iter().cloned().fold(f64::INFINITY, f64::min);
                let hi_ign = igns.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
                if lo_act - hi_ign >= gap_min {
                    out(true, "ok")
                } else {
                    out(false, "known-gap")
                }
            }
            Op::Measure => {
                let m = q.scale.len();
                let mut per = vec![0usize; m];
                let mut level_ok = true;
                for (a, x) in answers.iter().zip(xs.iter()) {
                    let Some(Answer::Score(ps)) = a else {
                        return err(
                            Some("E-rt-arg"),
                            "known_answers：程度题的读数要是分档读数",
                            sp,
                        );
                    };
                    let exp = match x {
                        Value::Int(i, _) if (*i as usize) < m && *i >= 0 => *i as usize,
                        _ => {
                            return err(
                                Some("E-rt-arg"),
                                format!(
                                    "known_answers：程度题的期望写档下标 0..{}",
                                    m.saturating_sub(1)
                                ),
                                sp,
                            );
                        }
                    };
                    per[exp] += 1;
                    let e: f64 = ps.iter().enumerate().map(|(i, p)| i as f64 * p).sum();
                    if (e - exp as f64).abs() > tolerance {
                        level_ok = false;
                    }
                }
                if per.iter().any(|n| *n < 2) {
                    out(false, "known-count")
                } else if level_ok {
                    out(true, "ok")
                } else {
                    out(false, "known-level")
                }
            }
            Op::Select => unreachable!(),
        }
    }

    /// `state_within(状态, 来源状态) -> Bool`：状态 on / ctx / ref 槽里的每份材料都在来源状态的 on / ctx / ref 里
    /// （按材料哈希；over 是题的切法，不算材料）。验题闸门第②段的结构代理（步 28，过程记录 §1.2）：派生题只问
    /// 来源已有的材料。
    pub(crate) fn b_state_within(
        &mut self,
        name: &'static str,
        args: Vec<Value>,
        sp: Span,
    ) -> R<Value> {
        let (Some(Value::State(a)), Some(Value::State(b)), 2) =
            (args.first(), args.get(1), args.len())
        else {
            return err(
                Some("E-rt-arg"),
                format!("{name}(状态, 来源状态)：两个参数都要是状态"),
                sp,
            );
        };
        let src: std::collections::BTreeSet<&str> =
            b.on.iter()
                .chain(b.ctx.iter())
                .chain(b.r#ref.iter())
                .map(|m| m.hash.as_str())
                .collect();
        Ok(Value::bool(
            a.on.iter()
                .chain(a.ctx.iter())
                .chain(a.r#ref.iter())
                .all(|m| src.contains(m.hash.as_str())),
        ))
    }

    /// `gate_info(题列表, 状态列表) -> [Float]`：每道候选在它的状态上的闸门信息值（步 28 起 derive 验题闸门第④段；步 30 按
    /// 主会话裁定四十三改为 `gate_info`，原名 `info_values`）。请求与选择规模见 `budget.rs::请求与规模`；候选自己的记录按
    /// 题键 → 题式键 → 类键取（`价值记录`），有记录时取记录混淆矩阵上的互信息（裁定四十六，与规划器同一个值）；无记录取校准库里
    /// 同题类已认证记录的最低折扣 × 期望熵降，没有同题类取全库最低，全库为空只按熵。算法在 `jpp-plan::value`（经 `PlanHooks::gate_info`，运行时不依赖规划器）；
    /// 没接规划器报 `E-rt-plan`。用到的记录（候选自己的、取了最低折扣的那一条）经 `note_calib` 进账本：只凭账本重放时视图里
    /// 只有记过的记录，它们都不低于那个最低值、那一条也在，信息值与首跑相同。
    pub(crate) fn b_gate_info(
        &mut self,
        name: &'static str,
        args: Vec<Value>,
        sp: Span,
    ) -> R<Value> {
        if args.len() != 2 {
            return err(
                Some("E-rt-arity"),
                format!(
                    "{name} 需要 2 个参数（题列表, 状态列表），收到 {}",
                    args.len()
                ),
                sp,
            );
        }
        let (Value::List(qs), Value::List(sts)) = (&args[0], &args[1]) else {
            return err(
                Some("E-rt-arg"),
                "gate_info(题列表, 状态列表)：两个参数都要是列表",
                sp,
            );
        };
        if qs.len() != sts.len() {
            return err(
                Some("E-rt-arg"),
                format!(
                    "gate_info：题 {} 道、状态 {} 个，要一一对应",
                    qs.len(),
                    sts.len()
                ),
                sp,
            );
        }
        let mut items = vec![];
        let mut 自记 = vec![];
        for (q, st) in qs.iter().zip(sts.iter()) {
            let (Value::Question(q), Value::State(st)) = (q, st) else {
                return err(
                    Some("E-rt-arg"),
                    "gate_info：题列表里要是题，状态列表里要是状态",
                    sp,
                );
            };
            let (request, k) = crate::budget::请求与规模(q, st);
            let own = self.价值记录(q);
            items.push(jpp_ir::plan::GateItem {
                request,
                k,
                kind: q.kind_on(&st.slot_shape()),
                own: own.as_ref().map(|(_, ch)| *ch),
            });
            自记.push(own.map(|(key, _)| key));
        }
        // 已认证记录与它们的混淆矩阵（建不出矩阵的跳过）
        let certified: Vec<(
            String,
            Option<jpp_ir::question_kind::QuestionKind>,
            jpp_ir::plan::Channel,
        )> = self
            .calib
            .certified()
            .into_iter()
            .filter_map(|(key, kind)| {
                let rec = self.calib.lookup(&key)?;
                let ch = self.记录信道(&key, &rec)?;
                Some((key, kind, ch))
            })
            .collect();
        let 库: Vec<(
            Option<jpp_ir::question_kind::QuestionKind>,
            jpp_ir::plan::Channel,
        )> = certified.iter().map(|(_, k, ch)| (*k, *ch)).collect();
        match self.hooks.gate_info(&items, &库) {
            Some(vs) => {
                for (key, (_, pick)) in 自记.iter().zip(&vs) {
                    if let Some(key) = key {
                        self.note_calib(key);
                    }
                    if let Some(i) = pick {
                        self.note_calib(&certified[*i].0);
                    }
                }
                Ok(Value::list(
                    vs.into_iter().map(|(v, _)| Value::float(v)).collect(),
                ))
            }
            None => err(
                Some("E-rt-plan"),
                "gate_info 要规划器算（jpp-plan::value）：这个解释器没接规划器钩子。修法：经 jpp::run / Session 跑",
                sp,
            ),
        }
    }

    /// `split_point(题) -> {u, capacity, n} | unit`（步 30 B 段，B7 后半、宪法登记表「噪声二十问」行）：这道是非题在校准后的
    /// 信道上的切分点 u*（对一个候选集问「在不在子集 A 里」时，A 应占后验质量的 u*）与信道容量。信道由记录的带标注样本估：
    /// 记录与价值同一处取（`价值记录`：题键 → 题式键 → 类键，第一条「上岗」、不是夹具线（有选中证书）、有带标注样本的），
    /// δ 取 `line_delta`，每条样本 (p, 真值) 按 `cut` 同一判据（`stat::decided_up/decided_down`）落到 act、ignore、unsure，
    /// 混淆矩阵按 Jeffreys 平滑（与价值同口径）。不是是非题、没有这样的记录、两类真值缺一时返回 `unit`。
    /// 用到的记录经 `note_calib` 进账本。消费它的探问搜索骨架归件 j（B6）。
    pub(crate) fn b_split_point(
        &mut self,
        name: &'static str,
        args: Vec<Value>,
        sp: Span,
    ) -> R<Value> {
        if args.len() != 1 {
            return err(
                Some("E-rt-arity"),
                format!("{name} 需要 1 个参数（题），收到 {}", args.len()),
                sp,
            );
        }
        let Value::Question(q) = &args[0] else {
            return err(Some("E-rt-arg"), "split_point(题)：参数要是题", sp);
        };
        if q.op != Op::Test {
            return Ok(Value::Unit);
        }
        // 记录与混淆矩阵同规划器一处取（`价值记录`），这里要是非记录（两行三列：真值为是 / 否 × act、ignore、unsure）
        let 找 = self.价值记录(q);
        let Some((key, jpp_ir::plan::Channel::Binary { n: 计 })) = 找 else {
            return Ok(Value::Unit);
        };
        let 样本数: f64 = 计.iter().flatten().sum();
        let 行和 = |r: &[f64; 3]| r.iter().sum::<f64>();
        if 行和(&计[0]) == 0.0 || 行和(&计[1]) == 0.0 {
            return Ok(Value::Unit);
        }
        // 平滑与价值同口径，在 jpp-plan::value 里做（复核 B0488-B 缺口 5）
        match self.hooks.split_point(计) {
            Some((u, c)) => {
                self.note_calib(&key);
                Ok(Value::record(vec![
                    ("u".to_string(), Value::float(u)),
                    ("capacity".to_string(), Value::float(c)),
                    ("n".to_string(), Value::int(样本数 as i64)),
                ]))
            }
            None => err(
                Some("E-rt-plan"),
                "split_point 要规划器算（jpp-plan::value）：这个解释器没接规划器钩子。修法：经 jpp::run / Session 跑",
                sp,
            ),
        }
    }
}
