//! 超窗裂变 `fission` 的执行一半（步 23b；`12` §4 pass 3、`11` §5.3、L-035、`20` 第 827 行「在刷新点看 K 与尺寸」）。
//!
//! **近似档**：只在题或题式声明 `fission: "approx"` 时切（V8 实测整篇与两块合回的出口一致率 0.897，`21` 步 23b 注）。
//! 四个条件同时成立才切：计划开关 `Plan.fission`；画像测过窗口（没测照报 `W-window-untested`、不切，主控 Q-F6）；
//! 题声明了；材料超窗（对象槽单段超 `window.text`，或语境槽 `ctx` + `ref` 超 `window.json_ctx`）。否则原路径逐字节不变。
//!
//! - **切**（登记处，[`Interp::裂变判断`]）：`on` 里超窗的每一段按 `jpp_ir::fission::split` 切（文本材料切文本；
//!   记录材料只切最长的文本字段、其余字段每块照带，主控 Q-F4）；语境超窗时 `ref` 每块照带、`ctx` 按先后贪心分组；
//!   `over` 不切。块状态是普通状态、普通站点：照常登记、合批、记账、可重放。声明的题返回**合成读数**（句柄进
//!   `Interp.裂变表`），只供 `cut` 消费。
//! - **合回**（`cut` 入口，[`Interp::裂变合回`]）：每块按同一 `cut` 选项各自过桥，再按题的操作合回——test → exists
//!   （`compose` 的 `any`），声明 `merge: "all"` 走 `all`；measure → 按出口计数（强 Kleene，`jpp_ir::fission::measure_count`）；
//!   select → 分块选出胜者（按无线默认），各胜者在它胜出的块上问一道派生是非题（B156 的 K-noul 形式），取读数最大者。
//!
//! 已知差距见过程记录 `地基/过程记录/工程-步23b.md` §六·5。
//! 依据：`11` §5.3；`12` §4 pass 3、H6、B156；L-035；主控 Z0208（Q-F1–Q-F8）与 2026-09-29 越界项答复

use super::*;
use crate::bridge::CutOpts;
use crate::constructs::compose::{self, 规则};
use jpp_effects::Window;
use jpp_ir::fission::{measure_count, split};
use std::cell::RefCell;

/// 一道声明了裂变的题在一个超窗站点上的合成读数：整篇状态、各块状态与块读数。
#[derive(Debug)]
pub(crate) struct 合成读数 {
    pub q: Rc<Question>,
    pub whole: Rc<State>,
    pub blocks: Vec<(Rc<State>, Rc<Reading>)>,
    /// 登记它的 `judge` 站点（select 第二层的派生题用同一站点记键）
    pub site: Span,
    /// select 的第二层：（各胜者的派生题读数（候选, 读数），没有胜者的块的未决原因）。同一刷新点里各 select 站点的第二层
    /// 一起登记、一次刷新（`11` §5.3「再一层」是一层，不是每站点一层；复核 B0476 缺口 3），登记后记在这里
    pub 第二层: RefCell<Option<第二层读数>>,
    /// select 第二层登记时各块的第一层胜者（候选, 块），只作报告行 `fission.winners`（裁定五十）
    pub 胜者: RefCell<Vec<(usize, usize)>>,
}

/// select 第二层：各胜者的派生题读数（候选, 读数）与没有胜者的块的未决原因
pub(crate) type 第二层读数 = (Vec<(usize, Rc<Reading>)>, Vec<String>);

/// 把一份超窗材料切成块；切不开时给原因（照整篇判断并告警）
fn 切材料(m: &Mat, window: usize) -> Result<Vec<Mat>, String> {
    match &m.content {
        Json::String(t) => {
            // 块的估计直接用 `Mat::tokens()`：切块判据与窗口检查永远是同一个数
            let est = |s: &str| m.with_content(Json::String(s.to_string())).tokens();
            let bs = split(t, window, &est);
            if bs.iter().any(|b| est(b) > window) {
                return Err("文本切到单个字符仍超窗".into());
            }
            Ok(bs
                .into_iter()
                .map(|b| m.with_content(Json::String(b)))
                .collect())
        }
        Json::Object(o) => {
            // 只切最长的文本字段（按字符数；同长取键序在前的），其余字段每块照带
            let Some((k, t)) = o
                .iter()
                .filter_map(|(k, v)| v.as_str().map(|s| (k.clone(), s.to_string())))
                .max_by(|a, b| {
                    a.1.chars()
                        .count()
                        .cmp(&b.1.chars().count())
                        .then(b.0.cmp(&a.0))
                })
            else {
                return Err("记录里没有文本字段可切".into());
            };
            let with = |s: &str| {
                let mut o2 = o.clone();
                o2.insert(k.clone(), Json::String(s.to_string()));
                Json::Object(o2)
            };
            let est = |s: &str| m.with_content(with(s)).tokens();
            if est("") > window {
                return Err(format!("记录除字段 {k} 以外的部分已超窗"));
            }
            let bs = split(&t, window, &est);
            if bs.iter().any(|b| est(b) > window) {
                return Err(format!("字段 {k} 切到单个字符仍超窗"));
            }
            Ok(bs.into_iter().map(|b| m.with_content(with(&b))).collect())
        }
        other => Err(format!(
            "材料是 {}，不是文本或记录（列表按对象各建状态，B62）",
            match other {
                Json::Array(_) => "列表",
                Json::Number(_) => "数",
                Json::Bool(_) => "布尔",
                _ => "空值",
            }
        )),
    }
}

/// 块状态与说明（说明写进告警）
type 块与说明 = (Vec<Rc<State>>, String);

/// 块状态；不超窗为 `Ok(None)`，切不开为 `Err(原因)`。
fn 切状态(state: &State, w: &Window) -> Result<Option<块与说明>, String> {
    let mut 说明: Vec<String> = vec![];
    // 对象槽：每一段各自比（单段）
    let mut on_choices: Vec<Vec<Mat>> = vec![];
    for m in &state.on {
        let t = m.tokens();
        if t > w.text {
            let bs = 切材料(m, w.text)?;
            说明.push(format!(
                "对象槽单段 {t} token 超已测窗口 {}，切成 {} 块",
                w.text,
                bs.len()
            ));
            on_choices.push(bs);
        } else {
            on_choices.push(vec![m.clone()]);
        }
    }
    // 语境槽：ctx 与 ref 求和
    let 语境: usize = state
        .ctx
        .iter()
        .chain(&state.r#ref)
        .map(|m| m.tokens())
        .sum();
    let ctx_groups: Vec<Vec<Mat>> = if 语境 > w.json_ctx {
        let ref_t: usize = state.r#ref.iter().map(|m| m.tokens()).sum();
        if ref_t >= w.json_ctx {
            return Err(format!(
                "参照槽 {ref_t} token 已超 JSON 槽窗口 {}",
                w.json_ctx
            ));
        }
        let 余 = w.json_ctx - ref_t;
        let mut gs: Vec<Vec<Mat>> = vec![];
        let mut cur: Vec<Mat> = vec![];
        let mut n = 0usize;
        for m in &state.ctx {
            let t = m.tokens();
            if t > 余 {
                return Err(format!("单个语境材料 {t} token 超出可用的 {余}"));
            }
            if n + t > 余 && !cur.is_empty() {
                gs.push(std::mem::take(&mut cur));
                n = 0;
            }
            n += t;
            cur.push(m.clone());
        }
        if !cur.is_empty() {
            gs.push(cur);
        }
        说明.push(format!(
            "语境槽 {语境} token 超 JSON 槽窗口 {}，语境分成 {} 组",
            w.json_ctx,
            gs.len()
        ));
        gs
    } else {
        vec![state.ctx.clone()]
    };
    if 说明.is_empty() {
        return Ok(None);
    }
    // 各段的块与语境组取笛卡尔积（`on` 是一对且两段都超窗时各块两两配）
    let mut ons: Vec<Vec<Mat>> = vec![vec![]];
    for choices in &on_choices {
        let mut next = vec![];
        for prefix in &ons {
            for c in choices {
                let mut p = prefix.clone();
                p.push(c.clone());
                next.push(p);
            }
        }
        ons = next;
    }
    let mut out = vec![];
    for on in &ons {
        for ctx in &ctx_groups {
            let st = State::new(
                on.clone(),
                ctx.clone(),
                state.r#ref.clone(),
                state.over.clone(),
                false,
            )
            .with_parents(state.parents.iter().cloned());
            out.push(Rc::new(st));
        }
    }
    Ok(Some((out, 说明.join("；"))))
}

/// 合回方式的名字（告警与报告行用）
fn 合回名(q: &Question) -> &'static str {
    match (q.op, q.fission.map(|d| d.all).unwrap_or(false)) {
        (Op::Test, false) => "exists（任一块 act 即 act）",
        (Op::Test, true) => "all（任一块 ignore 即 ignore）",
        (Op::Select, _) => "分块选后再一层 noul-argmax",
        (Op::Measure, _) => "按出口计数",
    }
}

impl<'a> Interp<'a> {
    /// 登记处的分流（`register.rs::judge` 调）：四个条件同时成立时切块登记、返回（与题同序的）读数；
    /// 否则 `None`，调用者走原路径。
    pub(crate) fn 裂变判断(
        &mut self,
        state: &Rc<State>,
        qs: &[Rc<Question>],
        sp: Span,
    ) -> R<Option<Vec<Value>>> {
        if !self.plan.fission || state.has_fail || !qs.iter().any(|q| q.fission.is_some()) {
            return Ok(None);
        }
        // 画像没测窗口：不切，原路径照报 W-window-untested（主控 Q-F6）
        let Some(w) = self.calib.profile().window() else {
            return Ok(None);
        };
        let (blocks, 说明) = match 切状态(state, &w) {
            Ok(Some(x)) => x,
            Ok(None) => return Ok(None),
            Err(原因) => {
                if self.unknown_reported.insert(format!(
                    "fission-no@{}:{}",
                    sp.start,
                    state.mat_hash()
                )) {
                    self.trace.warn(format!(
                        "W-fission-approx: @{} 题声明了 fission: \"approx\"，但这份超窗材料切不开（{原因}），照整篇判断",
                        sp.start
                    ));
                }
                return Ok(None);
            }
        };
        let (声明, 未声明): (Vec<usize>, Vec<usize>) =
            (0..qs.len()).partition(|i| qs[*i].fission.is_some());
        let mut out: Vec<Option<Value>> = vec![None; qs.len()];
        // 没声明的题照整篇判断（原路径，超窗照报 W-window）
        if !未声明.is_empty() {
            let uq: Vec<Rc<Question>> = 未声明.iter().map(|i| qs[*i].clone()).collect();
            for (i, v) in 未声明.iter().zip(self.judge(state, &uq, sp)?) {
                out[*i] = Some(v);
            }
        }
        let dq: Vec<Rc<Question>> = 声明.iter().map(|i| qs[*i].clone()).collect();
        let mut 块读数: Vec<Vec<(Rc<State>, Rc<Reading>)>> = vec![vec![]; dq.len()];
        // 伴随题跟原题走，不跟块走（B0492 S5，主控 2026-09-30）：登记块时不带，下面对原题各登记一次
        self.裂变块中 += 1;
        let 块结果: R<Vec<Vec<Value>>> = blocks.iter().map(|b| self.judge(b, &dq, sp)).collect();
        self.裂变块中 -= 1;
        let 块结果 = 块结果?;
        for (b, rs) in blocks.iter().zip(块结果) {
            for (j, v) in rs.into_iter().enumerate() {
                if let Value::Reading(r) = v {
                    // 裂变出的调用按跨状态推测计（`20` v2 §4.5 第 3 条）：层内挑选里不算真站点
                    self.裂变块键.insert(r.ledger_key.clone());
                    块读数[j].push((b.clone(), r));
                }
            }
        }
        // 每道原题登记一次伴随题：挂在第一块（窗内）上，校准记录按原题查（主控 2026-09-30；过程记录 5.15）
        for (j, q) in dq.iter().enumerate() {
            if let Some((b0, r0)) = 块读数[j].first().cloned() {
                self.登记伴随(&b0, std::slice::from_ref(q), &[Value::Reading(r0)], sp)?;
            }
        }
        let mut 合回们: Vec<&'static str> = vec![];
        for (j, i) in 声明.iter().enumerate() {
            let q = &qs[*i];
            if !合回们.contains(&合回名(q)) {
                合回们.push(合回名(q));
            }
            let id = self.new_reading_id();
            let r = Rc::new(Reading {
                q_hash: q.hash.clone(),
                state_hash: state.hash.clone(),
                op: q.op,
                calib: q.calib.clone(),
                id,
                fail: None,
                model_id: self.model_id.clone(),
                // 合成读数本身没有账本条目：证据在各块读数的键上
                ledger_key: String::new(),
                over_len: state.over.len(),
                scale: q.scale.clone(),
                perms: std::cell::Cell::new(0),
                mode_share: std::cell::Cell::new(None),
                missing_evidence: missing_evidence(state, q),
                state_taint: Taint::join(state.taint, q.taint),
                form_hash: q.form_hash.clone(),
                fp: Some(jpp_value::stat::material_fingerprint(&state.on_text())),
            });
            self.裂变表.insert(
                id,
                Rc::new(合成读数 {
                    q: q.clone(),
                    whole: state.clone(),
                    blocks: std::mem::take(&mut 块读数[j]),
                    site: sp,
                    第二层: RefCell::new(None),
                    胜者: RefCell::new(vec![]),
                }),
            );
            out[*i] = Some(Value::Reading(r));
        }
        if self
            .unknown_reported
            .insert(format!("fission@{}:{}", sp.start, state.mat_hash()))
        {
            let select注 = if dq.iter().any(|q| q.op == Op::Select) {
                "；select 的第二层是由题式派生的是非题，合回按无线默认或作者声明线（题的认证线在 K 选一 p_max 上，不套到第二层读数）"
            } else {
                ""
            };
            // 几处都切（成对材料两段都超窗、或对象槽与语境槽都超）时块数是乘积，另写合计（复核 B0476 缺口 6）
            let 合计 = if 说明.contains('；') {
                format!("，合计 {} 块", blocks.len())
            } else {
                String::new()
            };
            self.trace.warn(format!(
                "W-fission-approx: @{} {说明}{合计}，按声明 fission: \"approx\" 逐块判断，在 cut 处按 {} 合回{select注}。这是近似策略：V8 实测两块时整篇与合回的出口一致率 0.897；多块切分、all 合回、measure 计数、语境切分未经实测（依据：11 §5.3；21 步 23b）",
                sp.start,
                合回们.join("、")
            ));
        }
        Ok(Some(
            out.into_iter()
                .map(|v| v.expect("每道题都有读数"))
                .collect(),
        ))
    }

    /// 提前登记（推测、向量化、提升）用的块：与 [`Self::裂变判断`] 同一组条件，成立时返回块状态，否则 `None`。
    /// 提前登记处据此把声明了裂变的题登记在块上：不这样做，提前登记会按整篇发出（真站点走到时又去切块），
    /// 整篇那次调用白花、还占预算（window-over-approx 替身首跑：calls 20+1 / 20，W-spec-unused 36 个站点）。
    pub(crate) fn 裂变推测块(
        &self,
        state: &State,
        qs: &[Rc<Question>],
    ) -> Option<Vec<Rc<State>>> {
        if !self.plan.fission || state.has_fail || !qs.iter().any(|q| q.fission.is_some()) {
            return None;
        }
        let w = self.calib.profile().window()?;
        match 切状态(state, &w) {
            Ok(Some((bs, _))) => Some(bs),
            _ => None,
        }
    }

    /// 窗口未测的读数记进旁表（`register.rs::judge` 在登记后调；裁定四十九 (c)）：开关开、画像没测窗口、题声明了裂变。
    /// 这些读数本想按窗切、因窗口未测没切（`裂变判断` 的早退），出口据此置 `window_untested`
    pub(crate) fn 记窗口未测(&mut self, qs: &[Rc<Question>], rs: &[Rc<Reading>]) {
        if !self.plan.fission || self.calib.profile().window().is_some() {
            return;
        }
        for (q, r) in qs.iter().zip(rs) {
            if q.fission.is_some() {
                self.窗口未测读数.insert(r.id);
            }
        }
    }

    /// 合回已决时被吸收的块未决入账（裁定四十九 (b)：块出口连 cause 照记账本）：写一条 `Refine{how: "fission-merge"}`
    /// （块的未决被合回出口取代，最接近细化；主控 Z0364 答复第 4 条，不用 `Drop`、不改 `jpp-ledger`）。
    /// 键已由别处解除的不重复写，免得报 `W-duty-twice`
    fn 记块未决被吸收(&mut self, p: &Rc<Exit>, sp: Span) {
        let 键 = crate::duty::责任键(p);
        if !键.is_empty() && 键.iter().all(|k| self.解除.contains_key(k)) {
            return;
        }
        self.记去向(
            p,
            crate::duty::去向::Refine {
                how: "fission-merge",
                to: None,
            },
            "fission 合回吸收（块的未决被合回出口取代）",
            sp,
        );
    }

    /// 合回出口的报告行（设计名 `ReadingMeta.fission`；合回出口没有读数句柄，报告 `exits` 表的行就是它的元信息所在）：
    /// `fission = {blocks, unsure_blocks, causes}`，select 另带 `winners`、`second`、`delta`（裁定五十）。
    /// 行登记进 `exit_rows`，`sieve` 的元素构造按出口 id 找得到它。
    fn 记合回行(&mut self, e: &Rc<Exit>, f: &合成读数, 汇总: Json) {
        let 行 = self.exit_grades.len();
        self.exit_grades.push(json!({
            "site": f.site.start,
            "exit": e.label(),
            "grade": "无等级",
            "releases": e.releases(),
            "item": f.whole.hash.chars().take(12).collect::<String>(),
            "fission": 汇总,
        }));
        // 与 `cut` 的报告行同一个字段（裁定五十六：画像没有 δ 时裂变合回也带这一位）
        if e.delta_unknown.get() {
            self.exit_grades[行]["delta_unknown"] = Json::Bool(true);
            // Z0425：裂变合回只在画像没有 δ 时置这一位，没加带
            self.exit_grades[行]["delta_source"] = Json::from("none");
        }
        self.exit_rows.insert(e.id, 行);
    }

    /// 这个读数是不是裂变合成读数（`readings_of` 用）
    pub(crate) fn 是合成读数(&self, r: &Reading) -> bool {
        self.裂变表.contains_key(&r.id)
    }

    /// `cut` 入口的合回（`bridge.rs::cut` 调）：合成读数逐块过桥，再按题的操作合回。
    pub(crate) fn 裂变合回(
        &mut self,
        f: &Rc<合成读数>,
        calib_key: Option<&str>,
        opts: CutOpts,
        sp: Span,
    ) -> R<Value> {
        // 惰性出口（B94）借来的出口号与帧留给合回出口；各块出口另取新号
        let 预定 = self.出口预定.take();
        let 族 = crate::bridge::出口族(f.q.op, &opts.stat, opts.declare.as_ref());
        if f.q.op == Op::Select && 族 == Op::Select {
            let r = self.裂变合回_选(f, calib_key, opts, 预定, sp);
            self.出口预定 = None;
            return r;
        }
        let n = f.blocks.len();
        let mut 块出口: Vec<Rc<Exit>> = vec![];
        for (i, (_, br)) in f.blocks.iter().enumerate() {
            let v = self.cut(br, calib_key, opts.clone(), sp)?;
            let Value::Exit(e) = v else {
                return err(Some("E-rt-arg"), "cut 没有给出出口", sp);
            };
            if let Some(&row) = self.exit_rows.get(&e.id)
                && let Some(x) = self.exit_grades.get_mut(row)
            {
                x["fission"] = json!({"block": i, "of": n});
            }
            块出口.push(e);
        }
        let 种类: Vec<ExitKind> = 块出口.iter().map(|e| e.kind.clone()).collect();
        let kind = if 族 == Op::Measure {
            let 档: Vec<usize> = 种类
                .iter()
                .filter_map(|k| match k {
                    ExitKind::At(l) => Some(*l),
                    _ => None,
                })
                .collect();
            let 未决: Vec<&Why> = 种类
                .iter()
                .filter_map(|k| match k {
                    ExitKind::Unsure(w) => Some(w),
                    _ => None,
                })
                .collect();
            match measure_count(&档, 未决.len()) {
                Ok(l) => ExitKind::At(l),
                Err(true) => ExitKind::Unsure(Why::of(UnsureCause::Tie)),
                Err(false) => ExitKind::Unsure(
                    未决
                        .iter()
                        .find(|w| w.cause.is_absent_class())
                        .or(未决.first())
                        .map(|w| (*w).clone())
                        .unwrap_or_else(|| Why::of(UnsureCause::Tie)),
                ),
            }
        } else {
            let 规则 = if f.q.fission.is_some_and(|d| d.all) {
                规则::All
            } else {
                规则::Any
            };
            compose::合成种类(&规则, &种类)
                .map_err(|m| Fault::Error(RtError::new(Some("E-rt-arg"), m, sp)))?
        };
        self.出口预定 = 预定;
        let taint = 块出口
            .iter()
            .fold(Taint::Trusted, |t, e| Taint::join(t, e.taint));
        let x = self.new_exit(kind, None, 族, &f.q.hash, &f.whole.hash, taint, sp);
        self.出口预定 = None;
        if let Value::Exit(e) = &x {
            // 合回出口的分量 = 块出口：放行 = 全部分量放行之合取（B131），联合界照 `compose`（B161）
            *e.parts.borrow_mut() = 块出口.clone();
            e.bound.set(Some(compose::联合界(&规则::All, &块出口)));
            let 已决 = !e.is_unsure();
            let 标签 = if !已决 {
                // 未决：块的未决责任并入合回出口（与 `compose` 同一标签，B162 的解除照常）
                "compose"
            } else {
                // 已决：块的未决在合回里已经有了去向
                "fission"
            };
            let 未决块因: Vec<String> = 块出口
                .iter()
                .filter(|p| p.is_unsure())
                .map(|p| p.cause())
                .collect();
            for p in 块出口.iter().filter(|p| p.is_unsure() && !p.consumed.get()) {
                p.consumed.set(true);
                *p.consumed_by.borrow_mut() = 标签.into();
                if 已决 {
                    self.记块未决被吸收(p, sp);
                }
            }
            let 汇总 = json!({"blocks": n, "unsure_blocks": 未决块因.len(), "causes": 未决块因});
            self.记合回行(e, f, 汇总);
        }
        Ok(x)
    }

    /// select 的合回：分块选后再一层 noul-argmax（`11` §5.3），第二层题按 B156 的 K-noul 形式由 select 题派生。
    fn 裂变合回_选(
        &mut self,
        f: &Rc<合成读数>,
        calib_key: Option<&str>,
        opts: CutOpts,
        预定: Option<(usize, usize)>,
        sp: Span,
    ) -> R<Value> {
        if calib_key.is_some() || opts.cost.is_some() || opts.alpha.is_some() {
            return err(
                Some("E-cut-options"),
                "裂变合成的 select 读数只按无线默认或作者声明线合回：认证线、cost、alpha 都是 K 选一 p_max 上的证书，套不到第二层的是非题读数上（步 23b）。修法：去掉校准键 / cost / alpha，或写 {declare: {hi: …}}",
                sp,
            );
        }
        // 第一层已随登记发出；刷新后，把本趟全部还没登记第二层的 select 合成读数的第二层一起登记、只刷新一次。
        // 不能只看未解析的惰性出口：`map` 体里的 `cut` 在函数返回时就逐个解析了（V8b 就是这样）。
        // 同层融合：派生题按块状态合成一次调用（复核 B0476 缺口 3：改前每个站点自己刷新，V8b 层数 1 → 36）。
        // 正在 `cut` 的本站点排最前，其余按读数登记先后：这一层只有派生题、都归跨状态推测，层内挑选同类按登记位置发，
        // 预算不够时先推迟的是别的站点的（复查 B0476 x12：改前本站点可能排在从不 `cut` 的站点后面而拿不到预算）
        if f.第二层.borrow().is_none() {
            self.flush("cut")?;
            let mut 别的: Vec<(u64, Rc<合成读数>)> = self
                .裂变表
                .iter()
                .filter(|(_, g)| {
                    g.q.op == Op::Select && g.第二层.borrow().is_none() && !Rc::ptr_eq(g, f)
                })
                .map(|(id, g)| (*id, g.clone()))
                .collect();
            别的.sort_by_key(|(id, _)| *id);
            let mut 登了 = self.登记第二层(f)?;
            for (_, g) in &别的 {
                登了 |= self.登记第二层(g)?;
            }
            if 登了 {
                self.flush("cut")?;
            }
            // 别的站点的派生题是运行时替它们提前问的：发出了就记为跨状态推测，那个站点后来被 `cut` 时记为用上，
            // 到程序结束仍没用上的报 `W-spec-unused`（`20` §4.5 第 3 条；复查 B0476 条件 3）
            for (_, g) in &别的 {
                let 键: Vec<String> = g
                    .第二层
                    .borrow()
                    .iter()
                    .flat_map(|(问, _)| 问.iter())
                    .filter(|(_, r)| self.answer_of(r).is_some())
                    .map(|(_, r)| r.ledger_key.clone())
                    .collect();
                self.speculated.extend(键);
            }
        }
        // 本站点的派生题用上了（它可能是别的站点合回时替它提前问的）
        let 本键: Vec<String> = f
            .第二层
            .borrow()
            .iter()
            .flat_map(|(问, _)| 问.iter())
            .map(|(_, r)| r.ledger_key.clone())
            .collect();
        self.speculation_used.extend(本键);
        let (问, 块因) = f.第二层.borrow().clone().expect("本站点的第二层刚登记过");
        let mut 候选: Vec<(usize, f64, String)> = vec![]; // (候选, 读数, 账本键)
        let mut 二因: Vec<String> = vec![];
        for (k, r) in &问 {
            let absent = self.缺席因(r);
            match (&r.fail, absent, self.answer_of(r)) {
                (Some(m), _, _) => 二因.push(format!("fail:{m}")),
                (None, Some(c), _) => 二因.push(c),
                (None, None, Some(Answer::Noul(p))) => match 候选.iter_mut().find(|x| x.0 == *k) {
                    Some(x) if p > x.1 => {
                        x.1 = p;
                        x.2 = r.ledger_key.clone();
                    }
                    Some(_) => {}
                    None => 候选.push((*k, p, r.ledger_key.clone())),
                },
                _ => 二因.push("absent".into()),
            }
        }
        候选.sort_by(|a, b| {
            b.1.partial_cmp(&a.1)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then(a.0.cmp(&b.0))
        });
        let 首键 = f
            .blocks
            .first()
            .map(|(_, r)| r.ledger_key.clone())
            .unwrap_or_default();
        // 步 36 G3：块与第二层的原因仍按标签文字收（报告 `causes` 段照旧），进出口时读回封闭原因
        let 未决因 = |因: &[String]| {
            因.iter()
                .find(|c| Why::from_record(c).cause.is_absent_class())
                .or(因.first())
                .map(|c| Why::from_record(c))
                .unwrap_or_else(|| Why::of(UnsureCause::Tie))
        };
        // 第二层读数来自不同块，跨状态比较是近似（B0476 近似档），不套认证线；δ 取画像中段（Z0334）。
        // 画像缺 mid 报错，与 `cut` 兜底同一口径：只在真要用 δ 时报（第二层有两个以上候选），裁定四十五、Z0411 (i)。
        // 两段都没测的画像（或没加载画像）δ 未知：不编 0，不判 tie，按最大读数出、出口带 `delta_unknown`（不放行），
        // 报 `W-delta-unknown`（裁定五十六，主控板 Z0412）
        if 候选.len() >= 2 && self.calib.profile().delta_mid_missing() {
            return err(
                Some("E-delta-mid"),
                format!(
                    "@{} 裂变 select 合回要比第二层读数的差与画像 δ，而画像测了 δ、中段 δ（delta.<题型>.mid）却不全。\
                     尾段 δ 用满信心材料测得，线附近偏小，不能代替中段（裁定四十五）。修法【需接线人】：用覆盖中段读数的材料重测 δ，\
                     写进画像 delta.<题型>.mid（jpp profile check 列缺项）",
                    sp.start
                ),
                sp,
            );
        }
        let 画像阈值 = self.calib.profile().delta_prior(Op::Test);
        let 带宽未知 = 候选.len() >= 2 && 画像阈值.is_none();
        let (mut kind, mut 键) = match (候选.as_slice(), 画像阈值) {
            ([], _) if !二因.is_empty() => (ExitKind::Unsure(未决因(&二因)), 首键.clone()),
            ([], _) => (ExitKind::Unsure(未决因(&块因)), 首键.clone()),
            // 裁定五十：最大两个第二层读数相差不超过画像 δ 出 tie（B166 `order` 并档口径，边界带项目的 `BOUNDARY_EPS`：两位小数读数上「差恰为 δ」不是零测度事件）
            ([a, b, ..], Some(阈)) if a.1 - b.1 <= 阈 + jpp_value::stat::BOUNDARY_EPS => {
                (ExitKind::Unsure(Why::of(UnsureCause::Tie)), a.2.clone())
            }
            // δ 未知时不判 tie（裁定五十六）：按最大读数出，出口带 `delta_unknown`
            ([a, ..], _) => (ExitKind::Pick(a.0), a.2.clone()),
        };
        // C-4：已决 pick 之后按作者的代码谓词在第二层候选里按读数降序改选
        if let (ExitKind::Pick(_), Some(fp)) = (&kind, opts.feasible.clone()) {
            let mut 选 = None;
            for (k, _, kk) in &候选 {
                if self.调可行谓词(&fp, *k, sp)? {
                    选 = Some((*k, kk.clone()));
                    break;
                }
            }
            match 选 {
                Some((k, kk)) => {
                    kind = ExitKind::Pick(k);
                    键 = kk;
                }
                None => kind = ExitKind::Unsure(Why::of(UnsureCause::Infeasible)),
            }
        }
        // 作者声明线只作用在最终的最大读数上（V8 declared 口径）
        if let (ExitKind::Pick(k), Some(line)) = (&kind, &opts.declare) {
            let 过 = 候选.iter().find(|x| x.0 == *k).is_some_and(|x| {
                if line.closed_hi {
                    x.1 >= line.hi
                } else {
                    x.1 > line.hi
                }
            });
            if !过 {
                kind = ExitKind::Unsure(Why::of(UnsureCause::Band));
            }
        }
        let taint = f
            .blocks
            .iter()
            .fold(Taint::join(f.whole.taint, f.q.taint), |t, (s, _)| {
                Taint::join(t, s.taint)
            });
        self.出口预定 = 预定;
        let x = self.new_exit(kind, None, Op::Select, &f.q.hash, &f.whole.hash, taint, sp);
        if let Value::Exit(e) = &x {
            *e.ledger_key.borrow_mut() = 键;
            if 带宽未知 {
                e.delta_unknown.set(true);
                if self
                    .unknown_reported
                    .insert(format!("W-delta-unknown\u{1f}fission:{}", f.q.hash))
                {
                    self.线等级告警(format!(
                        "W-delta-unknown: @{} 裂变 select 合回要比第二层读数的差与画像 δ，而画像没有 δ（两段都没测或没加载画像）：不判 tie，按最大读数出，出口不放行不可逆 do（裁定五十六）。修法【需接线人】：给画像测 delta.<题型>.mid",
                        sp.start
                    ));
                }
            }
            let 标 = |k: usize| f.whole.over.get(k).map(|m| m.text()).unwrap_or_default();
            let 汇总 = json!({
                "blocks": f.blocks.len(),
                "unsure_blocks": 块因.len(),
                "causes": 块因,
                "winners": f.胜者.borrow().iter().map(|(k, i)| json!({"block": i, "candidate": k, "label": 标(*k)})).collect::<Vec<_>>(),
                "second": 候选.iter().map(|(k, p, _)| json!({"candidate": k, "label": 标(*k), "p": p})).collect::<Vec<_>>(),
                "delta": 画像阈值,
            });
            self.记合回行(e, f, 汇总);
        }
        Ok(x)
    }

    /// 登记一个 select 合成读数的第二层（第一层须已刷新）：每块取胜者（无线默认：唯一最大、声明置换时正逆两序一致；
    /// B187），每个胜者在它胜出的块上问一道派生是非题。已登记过返回 `false`。
    fn 登记第二层(&mut self, f: &Rc<合成读数>) -> R<bool> {
        if f.第二层.borrow().is_some() {
            return Ok(false);
        }
        let mut 胜者: Vec<(usize, usize)> = vec![]; // (候选, 块)
        let mut 块因: Vec<String> = vec![];
        for (i, (_, br)) in f.blocks.iter().enumerate() {
            let absent = self.缺席因(br);
            let answer = if br.fail.is_none() && absent.is_none() {
                self.answer_of(br)
            } else {
                None
            };
            let kind = if br.fail.is_none() && absent.is_none() && answer.is_none() {
                ExitKind::Unsure(Why::of(UnsureCause::Absent))
            } else {
                jpp_value::bridge::decide(&jpp_value::bridge::CutInput {
                    fail: br.fail.as_deref(),
                    absent: absent.as_deref(),
                    line: None,
                    cost_requested: false,
                    alpha_requested: false,
                    answer,
                    delta: None,
                    mode_share: br.mode_share.get(),
                })
                .0
            };
            match kind {
                ExitKind::Pick(k) => 胜者.push((k, i)),
                ExitKind::Unsure(w) => 块因.push(w.text()),
                _ => {}
            }
        }
        let mut 问: Vec<(usize, Rc<Reading>)> = vec![];
        for (k, i) in &胜者 {
            let (st, _) = &f.blocks[*i];
            let q = self.派生是非题(&f.q, &f.whole, *k);
            // 第二层问在胜者所在的块上（不带候选槽），不是整篇
            let st2 = Rc::new(
                State::new(
                    st.on.clone(),
                    st.ctx.clone(),
                    st.r#ref.clone(),
                    vec![],
                    false,
                )
                .with_parents(st.parents.iter().cloned()),
            );
            // 第二层派生题也不带伴随题：原题的伴随题在第一块已登记一次（复查 2026-09-30 小项 3）
            self.裂变块中 += 1;
            let 读 = self.judge(&st2, &[q], f.site);
            self.裂变块中 -= 1;
            if let Some(Value::Reading(r)) = 读?.into_iter().next() {
                self.裂变块键.insert(r.ledger_key.clone());
                问.push((*k, r));
            }
        }
        *f.胜者.borrow_mut() = 胜者;
        *f.第二层.borrow_mut() = Some((问, 块因));
        Ok(true)
    }

    /// select 第二层的派生是非题（B156 的 K-noul 形式：第 k 道把 `over[k]` 内容字面渲染进题面）。
    /// 题面 `「{select 题面}」——答案是「{候选内容}」吗？`；`q_hash = hash("fission-noul", select 的题哈希, 候选)`（B156 的
    /// `hash(form_hash, k)` 形；用题哈希不用题式哈希，因为同一题式的不同填法是不同的题）；
    /// 校准键另起（`{select 键}/fission-noul`），不借 select 的线。依据：`12` B156；主控 Z0208 Q-F2
    fn 派生是非题(&self, sq: &Question, whole: &State, k: usize) -> Rc<Question> {
        let 候选 = whole.over.get(k).map(|m| m.text()).unwrap_or_default();
        let text = format!("「{}」——答案是「{}」吗？", sq.text, 候选);
        let taint = Taint::join(
            sq.taint,
            whole.over.get(k).map(|m| m.taint).unwrap_or(Taint::Trusted),
        );
        // B138：题值只经 `IssueQuestion` 签发（复核 B0476 第 5 条）
        crate::caps::裂变派生题(
            &text,
            &format!("{}/fission-noul", sq.calib),
            hash_of(&["fission-noul", &sq.hash, &候选]),
            taint,
        )
    }
}
