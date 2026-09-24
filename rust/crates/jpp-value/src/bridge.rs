//! 桥的判序（`12` §2.3；`20` §2.3 L1 `jpp-value` 的 `bridge`）：读数 + 线 → 出口种类。
//!
//! 步 8a-2（R）：从 `jpp-core` 运行时 `cut_inner` 的判序部分原样抽出，成纯函数。
//! 运行时只保留查找（线从哪一级来）、登记与留痕；判序在这里，一处。
//! 依据：`12` §2.3 判序（insufficient 在运行时查线之前已处理）、J-15（未测取保守并带修法）、B32、B29。

use std::cell::{Cell, RefCell};

use jpp_ir::ir::Span;

use crate::value::{Answer, Exit, ExitKind, Op, Taint};

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
///     fixture_line: Default::default(), suspend_candidate: Default::default(), scope_out: Default::default(),
///     class_line: Default::default(), trial_line: Default::default() };
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
        fixture_line: Cell::new(false),
        suspend_candidate: Cell::new(false),
        scope_out: Cell::new(false),
        class_line: Cell::new(false),
        trial_line: Cell::new(false),
    }
}

/// 判序的输入：运行时查好线之后的全部事实。
pub struct CutInput<'a> {
    /// 读数本身是 Fail（状态含 Fail 材料）
    pub fail: Option<&'a str>,
    /// 判断器缺席或超时的标记（B32）
    pub absent: Option<&'a str>,
    /// 记录状态是停岗
    pub suspended: bool,
    /// 查到的线 `(hi, lo)`；`None` = 冷
    pub line: Option<(f64, f64)>,
    /// 调用者给了代价矩阵（找不到同代价的证书线时，冷的修法不同）
    pub cost_requested: bool,
    /// 刷新之后的答案（只在需要比线时读）
    pub answer: Option<Answer>,
    /// 这条线的 δ（线附近 ±δ 为 band）
    pub delta: f64,
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

/// 判序：Fail → 缺席 → 停岗 → 冷 → 按题型过线（是非题带 ±δ 的 band；选择题要求置换众数一致）。
pub fn decide(i: &CutInput) -> (ExitKind, Untested) {
    if let Some(f) = i.fail {
        return (ExitKind::Unsure(format!("fail:{f}")), None);
    }
    if let Some(c) = i.absent {
        // B32：判断器缺席或超时，出口按 J-05 四条去向路由，不加新去向
        return (ExitKind::Unsure(c.to_string()), None);
    }
    if i.suspended {
        // `drift` 不是未测：停岗是「测过、而且测出漂了」。停岗在回退之前返回，类级先验放行不了它。
        return (ExitKind::Unsure("drift".into()), None);
    }
    let Some((hi, lo)) = i.line else {
        // 题级没上岗、回退层也没上岗 → 冷
        return if i.cost_requested {
            (
                ExitKind::Unsure("cold".into()),
                Some((
                    "cost_line".into(),
                    "修法【需接线人】：用 commission_costed（或 calib-import）为这个代价矩阵从带真值样本认证一条线；线只来自记录".into(),
                )),
            )
        } else {
            (
                ExitKind::Unsure("cold".into()),
                Some((
                    "calib_line".into(),
                    "修法【作者可改】：用 calib-import 从带真值样本为这道题或它的题式认证一条线（模式级记录不供线，B44）".into(),
                )),
            )
        };
    };
    match i.answer.as_ref().expect("刷新之后答案必然在") {
        // `12`:167「再过线，再 band（线附近 ±δ）」。裸的 `p >= hi` 是失败开放（跨内核对照照出，E-JPP-LIVE）。
        Answer::Noul(p) => {
            if *p >= hi + i.delta {
                (ExitKind::Act, None)
            } else if *p <= lo - i.delta {
                (ExitKind::Ignore, None)
            } else {
                (ExitKind::Unsure("band".into()), None)
            }
        }
        // 12:151：Pick 要求置换众数一致；没测过（J-15）不是 tie（测了、不一致）。
        Answer::Choice(v) => {
            let (k, p) = argmax(v);
            match i.mode_share {
                None => (
                    ExitKind::Unsure("untested".into()),
                    Some((
                        "permutation".into(),
                        "修法【需接线人】：开置换要在 Rust 侧设 JevClient.permute = true（select 的调用数 ×2）——**`.jpp` 作者改不了这一项**".into(),
                    )),
                ),
                Some(ms) if ms < 1.0 => (ExitKind::Unsure("tie".into()), None),
                // 依据：B63（K 元划分的单侧线带 δ 迟滞：p_max ≥ hi + δ 才出 Pick）
                Some(_) => {
                    if p >= hi + i.delta {
                        (ExitKind::Pick(k), None)
                    } else {
                        (ExitKind::Unsure("band".into()), None)
                    }
                }
            }
        }
        Answer::Score(v) => {
            let (l, p) = argmax(v);
            // 依据：B63（同上；档位即动作不是免线的理由）
            if p >= hi + i.delta {
                (ExitKind::At(l), None)
            } else {
                (ExitKind::Unsure("band".into()), None)
            }
        }
    }
}
