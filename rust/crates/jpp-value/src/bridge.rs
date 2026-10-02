//! 桥的判序（`12` §2.3；`20` §2.3 L1 `jpp-value` 的 `bridge`）：读数 + 线 → 出口种类。
//!
//! 步 8a-2（R）：从 `jpp-core` 运行时 `cut_inner` 的判序部分原样抽出，成纯函数。
//! 运行时只保留查找（线从哪一级来）、登记与留痕；判序在这里，一处。
//! 依据：`12` §2.3 判序（insufficient 在运行时查线之前已处理）、J-15（未测取保守并带修法）、B32、B29。

use std::cell::{Cell, RefCell};

use jpp_ir::ir::Span;

use crate::value::{Answer, Exit, ExitKind, Op, Taint, UnsureCause, Why};

/// 出口的各部分（运行时查好的事实）。
pub struct ExitParts {
    pub id: usize,
    pub kind: ExitKind,
    pub untested: Option<String>,
    pub op: Op,
    pub q_hash: String,
    pub state_hash: String,
    pub taint: Taint,
    pub line_source: String,
    pub site: Span,
}

/// **出口的唯一构造处**（`20` A3、§2.4「出口只由桥产生」）。`Exit` 标了 `#[non_exhaustive]`，
/// 本 crate 以外写不出 `Exit { .. }`；运行时的所有出口（`cut`、`ask`、构造里的派生出口）都经这里。
///
/// ```compile_fail
/// // 依据：20 §2.4 `bypass/exit_ctor`：外部 crate 不能用结构体字面量造出口
/// use jpp_value::value::{Exit, ExitKind, Op, Taint};
/// let _ = Exit { id: 0, op: Op::Test, kind: ExitKind::Act, q_hash: String::new(), state_hash: String::new(),
///     taint: Taint::Trusted, site: Default::default(), from_ask: Default::default(), consumed: Default::default(),
///     consumed_by: Default::default(), line_source: String::new(), untested: None, ledger_key: Default::default(),
///     grade: Default::default(), suspend_candidate: Default::default(), scope_out: Default::default(),
///     delta_unknown: Default::default(), scope_unknown: Default::default(), parts: Default::default() };
/// ```
pub fn issue(p: ExitParts) -> Exit {
    Exit {
        id: p.id,
        op: p.op,
        kind: p.kind,
        q_hash: p.q_hash,
        state_hash: p.state_hash,
        taint: p.taint,
        site: p.site,
        from_ask: Cell::new(false),
        consumed: Cell::new(false),
        consumed_by: RefCell::new(String::new()),
        untested: p.untested,
        line_source: p.line_source,
        ledger_key: RefCell::new(String::new()),
        grade: Cell::new(None),
        suspend_candidate: Cell::new(false),
        scope_out: Cell::new(false),
        delta_unknown: Cell::new(false),
        scope_unknown: Cell::new(false),
        window_untested: Cell::new(false),
        near_boundary: Cell::new(false),
        parts: RefCell::new(Vec::new()),
        host_accepts_declared: Cell::new(false),
        alpha: Cell::new(None),
        bound: Cell::new(None),
    }
}

/// 判序的输入：运行时查好线之后的全部事实。
pub struct CutInput<'a> {
    /// 读数本身是 Fail（状态含 Fail 材料）
    pub fail: Option<&'a str>,
    /// 判断器缺席或超时的标记（B32）
    pub absent: Option<&'a str>,
    /// 查到的线 `(hi, lo)`；`None` = 没有线（没有上岗记录、记录停岗、作者要的证书没有），按判断器的回答走（B187）
    pub line: Option<(f64, f64)>,
    /// 调用者给了代价矩阵而没有同代价的证书：出口照回答走，另带 J-15 载体 `cost_line`（记录，B187）
    pub cost_requested: bool,
    /// 调用者给了 `alpha`（B129）而没有 α ≤ a 的证书：出口照回答走，另带 J-15 载体（运行时改写为 `alpha_line`）
    pub alpha_requested: bool,
    /// 刷新之后的答案（只在需要比线时读）
    pub answer: Option<Answer>,
    /// 这条线的 δ（线附近 ±δ 为 band）。`None` = 记录没有 δ：照线切、不加迁移带（裁定五十六、五十七 (3)、
    /// 六十六；步 36 G3 前这里出 `Unsure(untested)`、载体 `Delta`）
    pub delta: Option<f64>,
    /// 置换众数占比（`None` = 本次路径上没测过置换）
    pub mode_share: Option<f64>,
}

/// 未测载体与修法提示（J-15）：由判序产生，运行时统一出告警。
pub type Untested = Option<(String, String)>;

/// 取最大分量：返回 `(下标, 值)`。
pub fn argmax(v: &[f64]) -> (usize, f64) {
    let mut best = (0usize, f64::MIN);
    for (i, p) in v.iter().enumerate() {
        if *p > best.1 {
            best = (i, *p);
        }
    }
    best
}

/// 概率最大的下标，只在它唯一时给出（恰好并列、没有单元时为 `None`）。浮点精确相等：没有记录就没有 δ。
pub fn unique_argmax(v: &[f64]) -> Option<usize> {
    let (k, m) = argmax(v);
    (!v.is_empty() && v.iter().filter(|p| **p == m).count() == 1).then_some(k)
}

/// **按判断器的回答走**（意图汇编 11a，2026-09-26）：没有记录的线、作者也没写线时的出口。是非题 p > 0.5 为
/// act、p < 0.5 为 ignore；select 取概率最大的候选，measure 取概率最大的档位；恰好并列（p = 0.5，或最大值
/// 不唯一）时判断器没有给出回答，出 `Unsure(tie)`。作者在 select 上声明了置换而正逆两序众数不一致
/// （`mode_share < 1`）同样是 tie（与有线时同口径）；没声明置换不要求置换。等级由运行时记 `Answer`。
pub fn follow_answer(a: &Answer, mode_share: Option<f64>) -> ExitKind {
    follow_answer_delta(a, mode_share, None)
}

/// [`follow_answer`] 加上画像中段 δ（Z0912，主会话裁定七十二 (2)）：K 选一读数的前两项相差不超过 δ 即并列
/// （`Unsure(tie)`，走默认链）——与 `order` 按中段 δ 并档同一规则，δ 是画像实测（`delta.<题型>.mid` p99），不是手写阈值。
/// `delta` 为 `None` 或 0（画像没测中段 δ）时与改前相同：只有最大值恰好不唯一才并列。是非题与程度题不变
/// （是非题带内由运行时的读数触发走默认链，裁定五十二 (a)）。
/// 替代与放弃：「连续 k 拍拿不准才算」——k 是手写数，放弃（裁定七十二）。推翻条件：真机上并列拍占比远超 δ 的含义
/// （画像 δ 测的是同一读数重问的抖动；若带内读数重问后多数稳定在同一候选，说明 δ 取大了，回到画像测法）
pub fn follow_answer_delta(a: &Answer, mode_share: Option<f64>, delta: Option<f64>) -> ExitKind {
    let tie = || ExitKind::Unsure(Why::of(UnsureCause::Tie));
    match a {
        Answer::Noul(p) if *p > 0.5 => ExitKind::Act,
        Answer::Noul(p) if *p < 0.5 => ExitKind::Ignore,
        Answer::Noul(_) => tie(),
        Answer::Choice(_) if mode_share.is_some_and(|ms| ms < 1.0) => tie(),
        Answer::Choice(v) => match (unique_argmax(v), delta.filter(|d| *d > 0.0)) {
            (Some(k), Some(d)) if v.len() >= 2 => {
                let mut s = v.clone();
                s.sort_by(|x, y| y.partial_cmp(x).unwrap_or(std::cmp::Ordering::Equal));
                // 与 order 并档同一容差比较（BOUNDARY_EPS 吸收两位小数读数的浮点误差）
                if s[0] - s[1] <= d + crate::stat::BOUNDARY_EPS { tie() } else { ExitKind::Pick(k) }
            }
            (k, _) => k.map_or_else(tie, ExitKind::Pick),
        },
        Answer::Score(v) => unique_argmax(v).map_or_else(tie, ExitKind::At),
    }
}

/// 判序：Fail → 缺席 → 没有线（按判断器的回答走，B187）→ 按题型过线（是非题带 ±δ 的 band；选择题要求置换众数一致）。
/// B187（批 9）：`cause` 删 `cold` 与 `drift`——停岗的记录不供线，与没有记录一样按回答走；作者要的 `cost` / `alpha`
/// 证书没有时也按回答走，另带 J-15 载体（只是记录，告诉作者没找到他要的证书）。
pub fn decide(i: &CutInput) -> (ExitKind, Untested) {
    if let Some(f) = i.fail {
        return (ExitKind::Unsure(Why::with(UnsureCause::Fail, f)), None);
    }
    if let Some(c) = i.absent {
        // B32：判断器缺席或超时，出口按 J-05 四条去向路由，不加新去向
        return (ExitKind::Unsure(Why::from_record(c)), None);
    }
    let Some((hi, lo)) = i.line else {
        let a = i.answer.as_ref().expect("回答路径：刷新之后答案必然在");
        let 载体 = if i.cost_requested {
            Some((
                "cost_line".into(),
                "修法【需接线人】：用 commission_costed（或 calib-import --cost）为这个代价矩阵从带真值样本认证一条线；没有这条线时出口按判断器的回答走".into(),
            ))
        } else if i.alpha_requested {
            Some((
                "calib_line".into(),
                "修法【作者可改】：这道题没有认证过的线；认证一条（calib-import），或去掉 alpha 让出口按判断器的回答走".into(),
            ))
        } else {
            None
        };
        return (follow_answer_delta(a, i.mode_share, i.delta), 载体);
    };
    // 有线没 δ 不是未决（裁定五十六、五十七 (3)、六十六；步 36 G3）：照线切、不加迁移带（按 δ = 0 比较）；
    // 读数与出口上的 `delta_unknown` 由调用方置（运行时 `cut` 本来如此，这一支只有题库统计与测试走到）
    let delta = i.delta.unwrap_or(0.0);
    match i.answer.as_ref().expect("刷新之后答案必然在") {
        // `12`:167「再过线，再 band（线附近 ±δ）」。裸的 `p >= hi` 是失败开放（跨内核对照照出，E-JPP-LIVE）。
        Answer::Noul(p) => {
            // 边界按容差比较，与认证同一已决集合（步 15d-2，`stat::decided_up/down`）
            if crate::stat::decided_up(*p, hi, delta) {
                (ExitKind::Act, None)
            } else if crate::stat::decided_down(*p, lo, delta) {
                (ExitKind::Ignore, None)
            } else {
                (ExitKind::Unsure(Why::of(UnsureCause::Band)), None)
            }
        }
        // 12:151：Pick 要求置换众数一致；没测过（J-15）不是 tie（测了、不一致）。
        Answer::Choice(v) => {
            let (k, p) = argmax(v);
            match i.mode_share {
                // 裁定六十六（步 36 G3）：原因 `cold`（线对这个站点不可用：保守线加标记），正交位 `untested:permutation`
                None => (
                    ExitKind::Unsure(Why::of(UnsureCause::Cold)),
                    Some((
                        "permutation".into(),
                        // 依据：B64（步 15f：置换是 select 站点的测量声明）
                        "修法【作者可改】：在这道 select 题或它的题式上声明 {permute: true}（正逆两序，select 的调用数 ×2；B64）".into(),
                    )),
                ),
                Some(ms) if ms < 1.0 => (ExitKind::Unsure(Why::of(UnsureCause::Tie)), None),
                // 依据：B63（K 元划分的单侧线带 δ 迟滞：p_max ≥ hi + δ 才出 Pick）
                Some(_) => {
                    if crate::stat::decided_up(p, hi, delta) {
                        (ExitKind::Pick(k), None)
                    } else {
                        (ExitKind::Unsure(Why::of(UnsureCause::Band)), None)
                    }
                }
            }
        }
        Answer::Score(v) => {
            let (l, p) = argmax(v);
            // 依据：B63（同上；档位即动作不是免线的理由）
            if crate::stat::decided_up(p, hi, delta) {
                (ExitKind::At(l), None)
            } else {
                (ExitKind::Unsure(Why::of(UnsureCause::Band)), None)
            }
        }
    }
}

/// 作者声明线（B128；`cuts` 为 B153 的分桶，`closed` 为 B165 的端位；步 20j-1、20j-3）。
/// `cuts` 非空时是分桶线（出 `At(ℓ)`），`hi`/`lo` 不用；否则是 `{hi, lo}` 线，只给 `hi` 时 `lo = hi`。
#[derive(Clone, Debug, PartialEq)]
pub struct DeclaredLine {
    pub hi: f64,
    pub lo: f64,
    pub cuts: Vec<f64>,
    /// 作者写了 `lo`（单线时为假）
    pub lo_given: bool,
    pub closed_hi: bool,
    pub closed_lo: bool,
    /// 各切点的开闭（B176，步 20j-3 追加 (7)）：长度 = `cuts`，真为闭（`E ≥ cᵢ` 算过该切点）；`{hi, lo}` 线为空
    pub closed_cuts: Vec<bool>,
}

/// 缺省两端（与切点）全闭（B165 (2)：与认证线同约定）。手写而不派生：派生的 `bool` 缺省为假，会把端位静默变开。
impl Default for DeclaredLine {
    fn default() -> DeclaredLine {
        DeclaredLine {
            hi: Default::default(),
            lo: Default::default(),
            cuts: vec![],
            lo_given: false,
            closed_hi: true,
            closed_lo: true,
            closed_cuts: vec![],
        }
    }
}

impl DeclaredLine {
    /// `{hi, lo?}` 线，两端闭（B128 原样）
    pub fn two_sided(hi: f64, lo: f64, lo_given: bool) -> DeclaredLine {
        DeclaredLine {
            hi,
            lo,
            lo_given,
            ..Default::default()
        }
    }
    /// `{cuts: [c₁ < c₂ < …]}` 分桶线（B153），切点闭（B165 缺省）；`hi`/`lo` 不用
    pub fn with_cuts(cuts: Vec<f64>) -> DeclaredLine {
        DeclaredLine {
            closed_cuts: vec![true; cuts.len()],
            cuts,
            ..Default::default()
        }
    }
    pub fn is_cuts(&self) -> bool {
        !self.cuts.is_empty()
    }
    /// 端位不是缺省全闭时的 `closed` 记录（B165 (3)）；全闭为 `None`，不写进记录与报告
    pub fn closed_json(&self) -> Option<serde_json::Value> {
        if self.is_cuts() {
            // B176：全同写单值（全闭不写，20j-3 的记录哈希不变），不全同写数组
            let c = &self.closed_cuts;
            if c.iter().all(|x| *x) {
                None
            } else if c.iter().all(|x| !*x) {
                Some(serde_json::json!({"cuts": false}))
            } else {
                Some(serde_json::json!({ "cuts": c }))
            }
        } else if self.closed_hi && self.closed_lo {
            None
        } else {
            Some(serde_json::json!({"hi": self.closed_hi, "lo": self.closed_lo}))
        }
    }
    /// 线的数：`{hi, lo}` 或 `{cuts}`（B142 记录与报告 `declared` 共用）
    pub fn numbers_json(&self) -> serde_json::Map<String, serde_json::Value> {
        let mut m = serde_json::Map::new();
        if self.is_cuts() {
            m.insert("cuts".into(), serde_json::json!(self.cuts));
        } else {
            m.insert("hi".into(), serde_json::json!(self.hi));
            m.insert("lo".into(), serde_json::json!(self.lo));
        }
        m
    }
    /// 告警与线源里的一段文字：`hi=0.7 lo=0.3`、`cuts=[0.5, 1.5]`，非缺省端位附在后面
    pub fn describe(&self) -> String {
        let 数 = if self.is_cuts() {
            format!("cuts={:?}", self.cuts)
        } else {
            format!("hi={} lo={}", self.hi, self.lo)
        };
        match self.closed_json() {
            Some(c) => format!("{数} closed={c}"),
            None => 数,
        }
    }
}

/// 统计量 `s` 过声明线（B128、B153、B165）：`{hi, lo}` 线出 test 型出口（act 当且仅当 s 在上侧已决区，ignore
/// 当且仅当在下侧已决区，其间 `Unsure(band)`）；`cuts` 线出 `At(ℓ)`，ℓ = 越过的切点数。闭端按现行容差
/// （`stat::decided_up/down`，δ = 0，与认证同一已决集合），开端按「严格越过线 ± ε」（`stat::beyond_up/down`）。
/// 依据：B153 (1)、B165 (2)(4)（地基/附注/2026-09-26-批6裁定.md §一、§十三）
pub fn past_declared(s: f64, l: &DeclaredLine) -> ExitKind {
    use crate::stat::{beyond_down, beyond_up, decided_down, decided_up};
    if l.is_cuts() {
        // B176：逐切点取开闭（缺省全闭）
        let 档 = l
            .cuts
            .iter()
            .enumerate()
            .filter(|(i, c)| {
                if l.closed_cuts.get(*i).copied().unwrap_or(true) {
                    decided_up(s, **c, crate::stat::DECLARED_DELTA)
                } else {
                    beyond_up(s, **c)
                }
            })
            .count();
        return ExitKind::At(档);
    }
    let 上 = if l.closed_hi {
        decided_up(s, l.hi, crate::stat::DECLARED_DELTA)
    } else {
        beyond_up(s, l.hi)
    };
    let 下 = if l.closed_lo {
        decided_down(s, l.lo, crate::stat::DECLARED_DELTA)
    } else {
        beyond_down(s, l.lo)
    };
    if 上 {
        ExitKind::Act
    } else if 下 {
        ExitKind::Ignore
    } else {
        ExitKind::Unsure(Why::of(UnsureCause::Band))
    }
}

/// `stat` 不是 `max` 而没有 `declare` 的旧冷出口（B153 (1)）。B187 起运行时不再产生：`mass` 按概率和的多数块走，
/// 其他统计量在 `cut` 解析时报 `E-cut-options`；保留给旧测试与账本说明用。
pub fn cold_for_stat(stat: &crate::stat::Stat) -> (ExitKind, Untested) {
    (
        ExitKind::Unsure(Why::of(UnsureCause::Cold)),
        Some((
            "calib_line".into(),
            format!(
                "修法【作者可改】：认证线在 p_max 上，对 stat: {} 无效（同键记录不借）；要按这个统计量切，写作者声明线 cut(r, {{stat: {}, declare: {{hi: …, lo: …}}}})，按你写的数切，不作错误率保证（B153、B128）；不写 stat 的 cut 在没有线时按判断器的回答走",
                stat.name(),
                stat.to_json()
            ),
        )),
    )
}

#[cfg(test)]
mod answer_route_tests {
    //! 意图汇编 11a：没有线、作者也没要求证书线时按判断器的回答走；要求了证书线（cost / alpha）照旧冷。
    use super::*;

    fn 输入(answer: Answer, cost: bool, alpha: bool) -> (ExitKind, Untested) {
        decide(&CutInput {
            fail: None,
            absent: None,
            line: None,
            cost_requested: cost,
            alpha_requested: alpha,
            answer: Some(answer),
            delta: None,
            mode_share: None,
        })
    }

    /// Z0912（裁定七十二 (2)）：没有线的 K 选一，前两项差不超过画像中段 δ 即并列；δ 未知或为 0 时照改前（恰好相等才并列）
    #[test]
    fn 选择题没线_前两项差在中段δ内为并列() {
        let tie = ExitKind::Unsure(Why::of(UnsureCause::Tie));
        let v = Answer::Choice(vec![0.30, 0.22, 0.48]);
        // 0.48 − 0.30 = 0.18 > 0.0971：照最大项
        assert_eq!(follow_answer_delta(&v, None, Some(0.0971)), ExitKind::Pick(2));
        let w = Answer::Choice(vec![0.30, 0.22, 0.38]);
        // 0.38 − 0.30 = 0.08 ≤ 0.0971：并列
        assert_eq!(follow_answer_delta(&w, None, Some(0.0971)), tie);
        // 恰在 δ 上：并列（与 order 并档同一容差比较）
        let x = Answer::Choice(vec![0.25, 0.35]);
        assert_eq!(follow_answer_delta(&x, None, Some(0.10)), tie);
        // δ 未知或为 0：照改前
        assert_eq!(follow_answer_delta(&w, None, None), ExitKind::Pick(2));
        assert_eq!(follow_answer_delta(&w, None, Some(0.0)), ExitKind::Pick(2));
        // 是非题、程度题不受影响
        assert_eq!(follow_answer_delta(&Answer::Noul(0.55), None, Some(0.2)), ExitKind::Act);
        assert_eq!(follow_answer_delta(&Answer::Score(vec![0.3, 0.36, 0.34]), None, Some(0.2)), ExitKind::At(1));
        // decide 的没线分支用上 δ
        let i = CutInput { fail: None, absent: None, line: None, cost_requested: false, alpha_requested: false,
                           answer: Some(w.clone()), delta: Some(0.0971), mode_share: None };
        assert_eq!(decide(&i).0, tie);
    }

    #[test]
    fn 是非题按_0_5_切_恰好一半为并列() {
        assert_eq!(输入(Answer::Noul(0.51), false, false).0, ExitKind::Act);
        assert_eq!(输入(Answer::Noul(0.49), false, false).0, ExitKind::Ignore);
        assert_eq!(
            输入(Answer::Noul(0.5), false, false),
            (ExitKind::Unsure(Why::of(UnsureCause::Tie)), None)
        );
        // 没有 untested：回答路径不是「判据未测」
        assert_eq!(输入(Answer::Noul(0.9), false, false).1, None);
    }

    #[test]
    fn select与measure取最大_并列为tie() {
        assert_eq!(
            输入(Answer::Choice(vec![0.2, 0.5, 0.3]), false, false).0,
            ExitKind::Pick(1)
        );
        assert_eq!(
            输入(Answer::Choice(vec![0.4, 0.4, 0.2]), false, false).0,
            ExitKind::Unsure(Why::of(UnsureCause::Tie))
        );
        assert_eq!(
            输入(Answer::Score(vec![0.1, 0.2, 0.7]), false, false).0,
            ExitKind::At(2)
        );
        assert_eq!(
            输入(Answer::Score(vec![0.5, 0.5]), false, false).0,
            ExitKind::Unsure(Why::of(UnsureCause::Tie))
        );
        // 声明了置换而两序众数不一致：tie；没测置换不要求
        assert_eq!(
            follow_answer(&Answer::Choice(vec![0.2, 0.8]), Some(0.5)),
            ExitKind::Unsure(Why::of(UnsureCause::Tie))
        );
        assert_eq!(
            follow_answer(&Answer::Choice(vec![0.2, 0.8]), Some(1.0)),
            ExitKind::Pick(1)
        );
        assert_eq!(
            follow_answer(&Answer::Choice(vec![]), None),
            ExitKind::Unsure(Why::of(UnsureCause::Tie))
        );
    }

    /// B187：作者要的证书线没有时也按回答走，出口另带 J-15 载体（记录）
    #[test]
    fn 要求证书线而没有证书按回答走并带载体() {
        let (k, u) = 输入(Answer::Noul(0.9), true, false);
        assert_eq!(k, ExitKind::Act);
        assert_eq!(u.map(|x| x.0), Some("cost_line".into()));
        let (k, u) = 输入(Answer::Noul(0.1), false, true);
        assert_eq!(k, ExitKind::Ignore);
        assert_eq!(u.map(|x| x.0), Some("calib_line".into()));
    }

    #[test]
    fn 失败_缺席先于回答() {
        let base = |fail: Option<&'static str>, absent: Option<&'static str>| {
            decide(&CutInput {
                fail,
                absent,
                line: None,
                cost_requested: false,
                alpha_requested: false,
                answer: None,
                delta: None,
                mode_share: None,
            })
            .0
        };
        assert_eq!(
            base(Some("x"), None),
            ExitKind::Unsure(Why::with(UnsureCause::Fail, "x"))
        );
        assert_eq!(
            base(None, Some("absent")),
            ExitKind::Unsure(Why::of(UnsureCause::Absent))
        );
    }
}

#[cfg(test)]
mod line_grade_tests {
    //! 步 20a-1：`Exit::releases` 是唯一放行判定点（`附注/2026-09-24-评估①裁定.md` §十第 12(a) 条）。
    use super::*;
    use crate::value::LineGrade;

    fn 部件(untested: Option<&str>, taint: Taint) -> ExitParts {
        ExitParts {
            id: 0,
            kind: ExitKind::Act,
            untested: untested.map(str::to_string),
            op: Op::Test,
            q_hash: String::new(),
            state_hash: String::new(),
            taint,
            line_source: "题级·证书:α=0.10".into(),
            site: Span::default(),
        }
    }

    fn 出口(g: Option<LineGrade>, untested: Option<&str>) -> Exit {
        let e = issue(部件(untested, Taint::Trusted));
        e.grade.set(g);
        e
    }

    /// 等级一项：只有 `Certified`、`Form` 放行；变体名与报告 `exits` 表的字符串一致。
    #[test]
    fn grade_releases_only_certified_and_form() {
        let all = [
            (LineGrade::Cold, "Cold", false),
            (LineGrade::Fixture, "Fixture", false),
            (LineGrade::Class, "Class", false),
            (LineGrade::Trial, "Trial", false),
            (LineGrade::Provisional, "Provisional", false),
            (LineGrade::Form, "Form", true),
            (LineGrade::Certified, "Certified", true),
            // 意图汇编 11a：判断器自己的回答，不作放行证据（只在开 --guard 时有意义）
            (LineGrade::Answer, "Answer", false),
        ];
        for (g, name, rel) in all {
            assert_eq!(g.name(), name);
            assert_eq!(g.releases(), rel, "{name}");
            let e = 出口(Some(g), None);
            assert_eq!(e.releases(), rel, "{name}");
            assert_eq!(e.guard_trusted(), rel, "{name}");
        }
    }

    /// 步 20j-2（B128）：`Declared` 未经宿主接受不放行，接受后放行；taint 仍由 `guard_trusted` 合取
    #[test]
    fn declared_releases_only_when_host_accepts() {
        let d = 出口(Some(LineGrade::Declared), None);
        assert!(!d.releases() && !d.guard_trusted());
        d.host_accepts_declared.set(true);
        assert!(d.releases() && d.guard_trusted());
        // 接受位对其他等级无作用
        let t = 出口(Some(LineGrade::Trial), None);
        t.host_accepts_declared.set(true);
        assert!(!t.releases());
        // 正交位照样否决
        let u = 出口(Some(LineGrade::Declared), Some("permutation"));
        u.host_accepts_declared.set(true);
        assert!(!u.releases());
        let x = issue(部件(None, Taint::Untrusted));
        x.grade.set(Some(LineGrade::Declared));
        x.host_accepts_declared.set(true);
        assert!(x.releases() && !x.guard_trusted());
    }

    /// 正交位：任一为真即不放行，可与放行等级叠加；`untested`（J-15）自步 20a-1 起计入。
    #[test]
    fn orthogonal_bits_block_release() {
        assert!(出口(Some(LineGrade::Certified), None).releases());
        let u = 出口(Some(LineGrade::Certified), Some("permutation"));
        assert!(!u.releases() && !u.guard_trusted(), "判据未测不放行");
        let sets: [fn(&Exit); 4] = [
            |e| e.scope_out.set(true),
            |e| e.suspend_candidate.set(true),
            |e| e.delta_unknown.set(true),
            |e| e.scope_unknown.set(true),
        ];
        for set in sets {
            let x = 出口(Some(LineGrade::Form), None);
            set(&x);
            assert!(!x.releases());
        }
        // 不来自 `cut` 的出口没有等级：B131（步 25-9）起只有 `ask` 出口放行（此前取真）
        let n = 出口(None, None);
        assert!(!n.releases(), "grade None 且不来自 ask：不放行");
        n.from_ask.set(true);
        assert!(n.releases(), "ask 出口：人答即真值");
        // taint 由 guard_trusted 合取，不进 releases
        let t = issue(部件(None, Taint::Untrusted));
        t.grade.set(Some(LineGrade::Certified));
        assert!(t.releases() && !t.guard_trusted());
    }
}

#[cfg(test)]
mod g3_tests {
    //! 步 36 G3（预注册附录二 U-1、U-2；裁定六十六）
    use super::*;

    fn 有线(answer: Answer, delta: Option<f64>, mode_share: Option<f64>) -> (ExitKind, Untested) {
        decide(&CutInput {
            fail: None,
            absent: None,
            line: Some((0.7, 0.3)),
            cost_requested: false,
            alpha_requested: false,
            answer: Some(answer),
            delta,
            mode_share,
        })
    }

    /// U-2：有线没 δ 不是未决，照线切、不加迁移带、不带载体
    #[test]
    fn u2_有线没delta照线切() {
        assert_eq!(有线(Answer::Noul(0.7), None, None), (ExitKind::Act, None));
        assert_eq!(
            有线(Answer::Noul(0.3), None, None),
            (ExitKind::Ignore, None)
        );
        assert_eq!(
            有线(Answer::Noul(0.5), None, None),
            (ExitKind::Unsure(Why::of(UnsureCause::Band)), None)
        );
        assert_eq!(
            有线(Answer::Choice(vec![0.1, 0.9]), None, Some(1.0)),
            (ExitKind::Pick(1), None)
        );
    }

    /// U-1：select 有线、没测置换：原因 cold，载体 permutation
    #[test]
    fn u1_select未测置换为cold带正交位() {
        let (k, u) = 有线(Answer::Choice(vec![0.1, 0.9]), Some(0.05), None);
        assert_eq!(k, ExitKind::Unsure(Why::of(UnsureCause::Cold)));
        assert_eq!(u.map(|x| x.0), Some("permutation".to_string()));
    }

    /// 细节进标签文字、不进原因名；账本读回的非成员原因按 absent（待定项 4）
    #[test]
    fn why的文字与读回() {
        assert_eq!(
            Why::with(UnsureCause::Insufficient, "ref").text(),
            "insufficient:ref"
        );
        assert_eq!(Why::from_record("budget"), Why::of(UnsureCause::Budget));
        assert_eq!(Why::from_record("spec_miss"), Why::of(UnsureCause::Absent));
        assert_eq!(
            Why::from_record("fail:x"),
            Why::with(UnsureCause::Fail, "x")
        );
    }
}
