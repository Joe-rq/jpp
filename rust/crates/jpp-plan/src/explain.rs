//! EXPLAIN：把一份 [`Plan`] 写成作者读得懂的文本（`20` §2.3 `jpp-plan` 接口 `explain(plan) -> String`；
//! `20a` D8「`Plan`/`explain` 执行前可得」；`11` §5.5 干跑报告）。
//!
//! 依据：20 §2.3；11 §5.5；B42（可靠下界与拒绝）。
//! 隐藏的决定：估计怎样读给人看——三种估计形状各写成什么话、与 `budget` 比对分几种状态、
//! 没有估计的项怎样如实标「未估」（`14` 附录：未知显示为未知，不写 0）。
//! 绕过测试：tests/explain_text.rs。
//!
//! 这里只读 `Plan`（与可选的预算、站点表、源位置），不重新推导任何估计数。唯一自己算的是站点行的调用数
//! （最多执行次数 × 每次执行的发出数）；分类与发出数取自 `passes/plan.rs`（`site_calls_class`、`attempts_of`），
//! 这里不另写拷贝，与整程序上界的一致性由 `crates/jpp/tests/explain_cli.rs` 钉住。
//!
//! 纯函数 crate：不读源文件。源位置由宿主经 [`Explain::locate`] 给。

use crate::passes::plan::{CallsClass, attempts_of, site_calls_class};
use jpp_effects::spec;
use jpp_ir::ir::{Budget, Program, SiteKind, Span};
use jpp_ir::key::SiteId;
use jpp_ir::plan::{Estimate, JudgePrice, Plan, PlanCtx, SitePlan, UnknownReason};
use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::num::FpCategory;

/// 宿主给的一个源位置：`文件:行` 与那一行的源码（`jpp-plan` 不读文件）。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Located {
    /// 例如 `sieve.jpp:20`
    pub place: String,
    /// 该行源码（首尾空白已去）
    pub source: String,
}

/// EXPLAIN 的可选输入。全为空时等于设计签名 `explain(plan)`：只凭 `Plan` 出文本。
#[derive(Default)]
pub struct Explain<'a> {
    /// 规划时的运行语境：给了就写口径行（首跑 / 账本非空 / 开缓存；单价来源）
    pub ctx: Option<&'a PlanCtx>,
    /// 给了就写预算比对，并给每个站点写种类与调用数
    pub program: Option<&'a Program>,
    /// 给了（且有 `program`）站点行就写「文件:行」与源码
    pub locate: Option<&'a dyn Fn(Span) -> Option<Located>>,
    /// 给了就写「确认阈值」一节（Z0236，`11` §5.5）。判定由宿主做（比大小只在那一处），这里只把它读给人看
    pub confirm: Option<&'a ConfirmView>,
}

/// 确认阈值的结论（Z0236，`11` §5.5「超阈值要 `--confirm`」）。宿主判定，EXPLAIN 只标注。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConfirmVerdict {
    /// 费用上界不超阈值
    Within,
    /// 费用上界超阈值：`run` 要 `--confirm`
    Over,
    /// 会花钱但费用折不成美元（画像没有单价、没有生成器）：阈值没法核，放行并标出
    Unchecked,
    /// 重放：只凭账本，不发调用
    ExemptReplay,
    /// 没有会花钱的端口（固定观察、无生成器）
    ExemptNoPaidPort,
}

impl ConfirmVerdict {
    /// 给 JSON 用的稳定名字
    pub fn code(self) -> &'static str {
        match self {
            ConfirmVerdict::Within => "within",
            ConfirmVerdict::Over => "over",
            ConfirmVerdict::Unchecked => "unchecked",
            ConfirmVerdict::ExemptReplay => "exempt_replay",
            ConfirmVerdict::ExemptNoPaidPort => "exempt_no_paid_port",
        }
    }
}

/// 一份确认判定（宿主一处算出，`--explain` 文本、JSON、拒绝与报告 `confirm` 键四处读同一份）。
#[derive(Clone, Debug, PartialEq)]
pub struct ConfirmView {
    /// 这一趟用的阈值（美元）
    pub threshold_usd: f64,
    /// 阈值取的是默认（没给 `--confirm-above`）
    pub threshold_is_default: bool,
    /// 费用上界（美元）；没有（未核、免确认）为空
    pub upper_usd: Option<f64>,
    /// 上界来自哪：`budget.cost`（生效费用上限：程序声明的 `budget.cost`，带上游余额时与余额的 `cost` 取小（C-3）；运行时已花超过它才停，可超出最后一次调用）、`carry`（同上，但起作用的是上游余额：它比程序声明更紧，C-3 Z0384）或 `plan`（计划估计更小）；没有上界为空串
    pub upper_from: &'static str,
    pub verdict: ConfirmVerdict,
    /// 命令行给了 `--confirm`
    pub confirmed: bool,
}

/// 估计与预算比对的结论（一处定义，文本与 JSON 共用）。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Verdict {
    /// 上界在预算内
    Within,
    /// 上界超出预算，下界没超：可能超，运行时到预算即停
    UpperOver,
    /// 下界已超预算：一定超；账本为空且缓存关闭时计划期拒绝（B42）
    LowerOver,
    /// 只有下界，上界未知：不能保证在预算内
    UpperUnknown,
    /// 估计是未知：不能核
    NotEstimated,
    /// 程序没有设这项预算
    NoBudget,
}

impl Verdict {
    /// 给 JSON 用的稳定名字
    pub fn code(self) -> &'static str {
        match self {
            Verdict::Within => "within",
            Verdict::UpperOver => "upper_over",
            Verdict::LowerOver => "lower_over",
            Verdict::UpperUnknown => "upper_unknown",
            Verdict::NotEstimated => "not_estimated",
            Verdict::NoBudget => "no_budget",
        }
    }
    /// 给人读的一句话
    pub fn text(self) -> &'static str {
        match self {
            Verdict::Within => "上界在预算内",
            Verdict::UpperOver => "上界超出预算，下界没超：可能超，运行时到预算即停",
            Verdict::LowerOver => {
                "下界已超预算：一定超；账本为空且缓存关闭时计划期拒绝，一次调用都不发"
            }
            Verdict::UpperUnknown => "上界未知，不能保证在预算内",
            Verdict::NotEstimated => "未估，不能核",
            Verdict::NoBudget => "预算未设",
        }
    }
}

/// 一项估计与一个预算数比对（预算 `None` = 没设）。
pub fn compare(lo: Option<f64>, hi: Option<f64>, budget: Option<f64>) -> Verdict {
    let Some(b) = budget else {
        return Verdict::NoBudget;
    };
    let Some(lo) = lo else {
        return Verdict::NotEstimated;
    };
    if lo > b {
        return Verdict::LowerOver;
    }
    match hi {
        Some(h) if h > b => Verdict::UpperOver,
        Some(_) => Verdict::Within,
        None => Verdict::UpperUnknown,
    }
}

/// 三项有预算的估计各自的比对：调用数、费用、时延（层数没有预算）。
/// 名字取 `budget` 里的字段名。
pub fn comparisons(plan: &Plan, b: &Budget) -> [(&'static str, Option<f64>, Verdict); 3] {
    let calls = (
        "calls",
        Some(b.calls as f64),
        compare(
            plan.calls_est.lo().map(|x| x as f64),
            plan.calls_est.hi().map(|x| x as f64),
            Some(b.calls as f64),
        ),
    );
    let cost = (
        "cost",
        Some(b.cost),
        compare(plan.cost_est.lo(), plan.cost_est.hi(), Some(b.cost)),
    );
    let lat = (
        "latency_p95",
        b.latency_p95,
        compare(plan.latency_est.lo(), plan.latency_est.hi(), b.latency_p95),
    );
    [calls, cost, lat]
}

/// 运行语境的口径行：这份估计按什么前提算的。
pub fn basis(ctx: &PlanCtx) -> String {
    let ledger = if !ctx.ledger_empty {
        "账本非空（续接或重放）：站点可能命中而不发，调用数与层数的下界记 0"
    } else if !ctx.cache_off {
        "开了跨运行缓存：站点可能命中而不发，调用数与层数的下界记 0"
    } else {
        "账本为空、缓存关闭（首跑）"
    };
    let price = match ctx.judge_price {
        JudgePrice::Known(p) if p.classify() == FpCategory::Zero => {
            "判断器单价 0（固定观察，不付费）".to_string()
        }
        JudgePrice::Known(p) => format!("判断器单价 {} 美元每 input token", num(p)),
        JudgePrice::Untested => "画像没有判断器单价".to_string(),
        JudgePrice::NotGiven => "没有给判断器单价".to_string(),
    };
    format!("{ledger}；{price}")
}

/// 站点的种类名（源码里的写法）。
pub fn kind_name(k: &SiteKind) -> String {
    match k {
        SiteKind::State => "state".into(),
        SiteKind::Effect(id) => spec(*id).name.to_string(),
        SiteKind::Cut => "cut".into(),
        SiteKind::Fit => "fit".into(),
        SiteKind::Loop => "loop".into(),
        SiteKind::Handle => "handle".into(),
        SiteKind::Consume(how) => how.name().into(),
        SiteKind::Construct(n) | SiteKind::HigherOrder(n) => n.clone(),
        SiteKind::If => "if".into(),
    }
}

/// 站点的分类名（给 JSON 用的稳定名字）。分类本身在 `passes/plan.rs`，这里不另写拷贝。
pub fn class_name(c: CallsClass) -> &'static str {
    match c {
        CallsClass::Reading => "reading",
        CallsClass::Call => "call",
        CallsClass::NoCall => "no_call",
        CallsClass::Opaque => "opaque",
        CallsClass::Other => "other",
    }
}

/// 会发判断调用（占判断层，或口径未建的构造）。
pub fn is_reading(c: CallsClass) -> bool {
    matches!(c, CallsClass::Reading | CallsClass::Opaque)
}

/// 每次执行发出几次调用（含重试）；`None` = 口径未建（上界未知）。读 `passes/plan.rs` 的分类与 `attempts_of`。
fn calls_per_exec(c: CallsClass, attempts: u64) -> Option<u64> {
    match c {
        CallsClass::Reading => Some(attempts),
        CallsClass::Call => Some(1),
        CallsClass::NoCall => Some(0),
        CallsClass::Opaque | CallsClass::Other => None,
    }
}

/// 站点表的一行（文本与 JSON 共用）。
#[derive(Clone, Debug, PartialEq)]
pub struct SiteRow {
    pub site: SiteId,
    /// 规划遍历到的先后
    pub order: u32,
    /// 种类名；没给 `Program` 时为空
    pub kind: Option<String>,
    /// 源位置（字节偏移，全程序）；没给 `Program` 时为空
    pub span: Option<Span>,
    pub plan: SitePlan,
    /// 规划眼里的站点分类（`passes/plan.rs`）；没给 `Program` 时为空
    pub class: Option<CallsClass>,
    /// 这个站点最多发出几次调用（含重试）；执行次数未知或口径未建为 `None`；没给 `Program` 时为空
    pub calls_hi: Option<u64>,
}

/// 站点表：给了 `Program` 按源码位置排，否则按登记顺序。
pub fn site_rows(plan: &Plan, program: Option<&Program>) -> Vec<SiteRow> {
    let at = program.map(|p| attempts_of(&p.budget));
    let mut rows: Vec<SiteRow> = plan
        .per_site
        .iter()
        .map(|(id, sp)| {
            let info = program.and_then(|p| p.sites.get(*id));
            let calls_hi = match (info, at) {
                (Some(i), Some(a)) => calls_per_exec(site_calls_class(&i.kind), a)
                    .and_then(|per| sp.execs_hi.map(|e| e.saturating_mul(per))),
                _ => None,
            };
            SiteRow {
                site: *id,
                order: sp.order,
                kind: info.map(|i| kind_name(&i.kind)),
                class: info.map(|i| site_calls_class(&i.kind)),
                span: info.map(|i| i.span),
                plan: sp.clone(),
                calls_hi,
            }
        })
        .collect();
    rows.sort_by_key(|r| (r.span.map_or(0, |s| s.start), r.order));
    rows
}

/// 逐站点调用数上界，给整程序上界做一致性核对用。
pub fn site_calls_hi(plan: &Plan, program: &Program) -> BTreeMap<SiteId, Option<u64>> {
    site_rows(plan, Some(program))
        .into_iter()
        .map(|r| (r.site, r.calls_hi))
        .collect()
}

/// EXPLAIN 文本（设计签名：只凭 `Plan`）。
pub fn explain(plan: &Plan) -> String {
    explain_with(plan, &Explain::default())
}

/// EXPLAIN 文本，可带口径、预算与站点位置（见 [`Explain`]）。
pub fn explain_with(plan: &Plan, cx: &Explain<'_>) -> String {
    let mut o = String::new();
    o.push_str("EXPLAIN（计划：运行前得出，不调用判断器，不花钱）\n");
    if let Some(c) = cx.ctx {
        let _ = writeln!(o, "口径：{}", basis(c));
    }
    if let Some(p) = cx.program {
        let b = &p.budget;
        let unset = || "未设".to_string();
        let lat = b
            .latency_p95
            .map_or_else(unset, |l| format!("{} 秒", num(l)));
        let esc = b.escalate.map_or_else(unset, |n| n.to_string());
        let uns = b.unsure.map_or_else(unset, num);
        let _ = writeln!(
            o,
            "预算：calls {} · cost {} 美元 · latency_p95 {lat} · escalate {esc} · unsure {uns}（escalate、unsure 计划期不比对）",
            b.calls,
            num(b.cost)
        );
    }
    estimates(&mut o, plan, cx.program);
    if let Some(c) = cx.confirm {
        confirm_section(&mut o, c);
    }
    sites(&mut o, plan, cx);
    early_registration(&mut o, plan);
    diagnostics(&mut o, plan);
    unestimated(&mut o, plan, cx.program);
    o
}

/// 「未估」节：全部由 `Plan` 推出，不是常量清单（裁定三十二）。估计为 `Unknown` 或只有下界的项、`Plan` 里没有的
/// `unsure_bound`、层未估与次数未知的站点数、`SitePlan` 没有的字段，都在这里；没有一处写 0。
fn unestimated(o: &mut String, plan: &Plan, program: Option<&Program>) {
    o.push_str("未估（由 Plan 推出，不写 0）\n");
    let item = |o: &mut String, label: &str, text: String| {
        let _ = writeln!(o, "  {label}：{text}");
    };
    fn gap<T: Copy>(e: &Estimate<T>, fmt: impl Fn(T) -> String) -> Option<String> {
        match e {
            Estimate::Known { .. } => None,
            Estimate::AtLeast { lo } => Some(format!("上界未估（只有下界 {}）", fmt(*lo))),
            Estimate::Unknown(r) => Some(reason_text(r)),
        }
    }
    let ints = |x: u64| x.to_string();
    for (label, g) in [
        ("调用数", gap(&plan.calls_est, ints)),
        ("判断层数", gap(&plan.layers_est, ints)),
        ("费用", gap(&plan.cost_est, num)),
        ("时延", gap(&plan.latency_est, num)),
    ] {
        if let Some(t) = g {
            item(o, label, t);
        }
    }
    item(o, "unsure 上界", "Plan 没有这个字段，没有计算".to_string());
    let rows = site_rows(plan, program);
    let layer_unknown = rows
        .iter()
        .filter(|r| r.plan.layer_lo.is_none() && r.class.is_none_or(is_reading))
        .count();
    let execs_unknown = rows.iter().filter(|r| r.plan.execs_hi.is_none()).count();
    if layer_unknown + execs_unknown > 0 {
        item(
            o,
            "站点",
            format!(
                "{layer_unknown} 个站点的层未估、{execs_unknown} 个站点的执行次数未知（见上表）"
            ),
        );
    }
    if !rows.is_empty() {
        item(
            o,
            "每个站点的下沉、裂变块数、预估 token、$、s",
            "SitePlan 没有这些字段，没有估计".to_string(),
        );
    }
}

fn estimates(o: &mut String, plan: &Plan, program: Option<&Program>) {
    o.push_str(
        "\n估计（下界只数每条路径必经的判断层、每层至少一次调用，且只在账本为空、缓存关闭时非 0；上界不计合批，偏松）\n",
    );
    let cmp = program.map(|p| comparisons(plan, &p.budget));
    let verdict = |name: &str| -> String {
        let Some(c) = &cmp else { return String::new() };
        match c.iter().find(|(n, _, _)| *n == name) {
            Some((n, Some(b), v)) if *v != Verdict::NoBudget => {
                format!("预算 {n} {}：{}", num(*b), v.text())
            }
            Some((n, _, _)) => format!("预算 {n} 未设"),
            None => String::new(),
        }
    };
    let line = |o: &mut String, label: &str, est: String, cmp: String| {
        if cmp.is_empty() {
            let _ = writeln!(o, "  {label}：{est}");
        } else {
            let _ = writeln!(o, "  {label}：{est}。{cmp}");
        }
    };
    line(
        o,
        "调用数（含重试）",
        est_text(&plan.calls_est, |x| x.to_string(), "次"),
        verdict("calls"),
    );
    line(
        o,
        "判断层数",
        est_text(&plan.layers_est, |x| x.to_string(), "层"),
        String::new(),
    );
    line(
        o,
        "费用",
        est_text(&plan.cost_est, num, "美元"),
        verdict("cost"),
    );
    line(
        o,
        "时延（层数 × 画像 p95，不含排队）",
        est_text(&plan.latency_est, num, "秒"),
        verdict("latency_p95"),
    );
}

/// 「确认阈值」一节：结论由宿主给，这里不比大小。
fn confirm_section(o: &mut String, c: &ConfirmView) {
    o.push_str("\n确认阈值（11 §5.5：费用上界超阈值，run 要 --confirm）\n");
    let which = if c.threshold_is_default {
        "默认阈值"
    } else {
        "--confirm-above"
    };
    let from = match c.upper_from {
        "plan" => "计划估计的费用上界",
        "carry" => {
            "上游余额收紧后的费用上限（余额比程序声明的 budget cost 更紧）；运行时已花超过它才停，可超出最后一次调用，token 上界估计未建"
        }
        _ => {
            "生效费用上限 budget cost（程序声明与上游余额取小）；运行时已花超过它才停，可超出最后一次调用，token 上界估计未建"
        }
    };
    let upper = c.upper_usd.map_or_else(|| "未知".to_string(), num);
    let t = num(c.threshold_usd);
    let line = match c.verdict {
        ConfirmVerdict::Over => {
            let tail = if c.confirmed {
                "已给 --confirm，放行"
            } else {
                "run 需要 --confirm"
            };
            format!("超阈值：费用上界 {upper} 美元（{from}）> 阈值 {t} 美元（{which}）；{tail}")
        }
        ConfirmVerdict::Within => {
            format!("未超：费用上界 {upper} 美元（{from}）不大于阈值 {t} 美元（{which}）")
        }
        ConfirmVerdict::Unchecked => format!(
            "阈值未核：判断器画像没有单价，没有生成器，美元上界无法折算（阈值 {t} 美元，{which}）；放行"
        ),
        ConfirmVerdict::ExemptReplay => "重放不发调用，不核阈值".to_string(),
        ConfirmVerdict::ExemptNoPaidPort => {
            "没有会花钱的端口（固定观察、没有生成器），不核阈值".to_string()
        }
    };
    let _ = writeln!(o, "  {line}");
}

fn sites(o: &mut String, plan: &Plan, cx: &Explain<'_>) {
    o.push_str("\n站点（每个会发调用的位置；按源码位置排，没有源码时按登记顺序）\n");
    let rows = site_rows(plan, cx.program);
    // 表空或不全时，按整程序调用数估计的形状说话：只有「上界为 0」才是真没有站点；
    // 其余如实写未估，不能把没解析出来写成没有
    let none = matches!(plan.calls_est, Estimate::Known { hi: 0, .. });
    if rows.is_empty() {
        let line = match &plan.calls_est {
            _ if none => "没有会发调用的站点".to_string(),
            Estimate::AtLeast { .. } => "逐站点信息未估：规划器没有解析出站点，效应在未知调用、非字面循环或经形参传入的方法值里".to_string(),
            Estimate::Unknown(r) => format!("逐站点信息未估：{}", reason_text(r)),
            Estimate::Known { .. } => "逐站点信息未估：站点表为空".to_string(),
        };
        let _ = writeln!(o, "  {line}");
        return;
    }
    for r in rows {
        let located = match (cx.locate, r.span) {
            (Some(f), Some(s)) => f(s),
            _ => None,
        };
        let kind = r.kind.clone().unwrap_or_default();
        match (&located, r.span) {
            (Some(l), _) => {
                let _ = writeln!(o, "  {}  {kind}  {}", l.place, l.source);
            }
            (None, Some(s)) => {
                let _ = writeln!(o, "  源偏移 {}  {kind}", s.start);
            }
            (None, None) => {
                let _ = writeln!(o, "  站点 #{}", r.site.0);
            }
        }
        // 只有读数站点记必经与层（`passes/plan.rs`）；其余站点的 `must`、`layer_lo` 恒为空，那是没记，不是「非必经」
        let layer = match (r.class, r.plan.must, r.plan.layer_lo) {
            (_, true, Some(n)) => format!("必经 · 层 ≥ {n}"),
            (Some(CallsClass::Opaque), _, _) => "会发调用 · 次数与层口径未建".to_string(),
            (Some(CallsClass::Reading), _, _) => "非必经 · 层未估".to_string(),
            (Some(_), _, _) => "不占判断层 · 是否必经规划器未记录".to_string(),
            (None, _, _) => "非必经或非判断类站点 · 层未估".to_string(),
        };
        if r.class == Some(CallsClass::Opaque) {
            let _ = writeln!(o, "      {layer}");
            continue;
        }
        let times = match (r.calls_hi, r.plan.execs_hi, r.class.is_some()) {
            (Some(c), Some(e), _) => format!("最多 {c} 次调用（执行 {e} 次）"),
            (None, Some(e), false) => format!("最多执行 {e} 次"),
            (None, Some(e), true) => format!("最多执行 {e} 次，调用数上界未建"),
            (_, None, _) => "调用数上界未知".to_string(),
        };
        let _ = writeln!(o, "      {layer} · {times}");
    }
    if matches!(plan.calls_est, Estimate::AtLeast { .. }) {
        o.push_str("  （整程序调用数上界未知：可能另有站点在未知调用、非字面循环或经形参传入的方法值里，规划器解析不了、不在此表；也可能只是上面某个站点的执行次数未知）\n");
    }
}

fn early_registration(o: &mut String, plan: &Plan) {
    let on = |b: bool| if b { "开" } else { "关" };
    o.push_str("\n运行期提前登记（计划；运行时还要按环境再核）\n");
    let _ = writeln!(
        o,
        "  同层同状态的题合并（fuse）：{}；向量化（vectorize）：{}；惰性过桥（lazy_cut）：{}",
        on(plan.fuse),
        on(plan.vectorize),
        on(plan.lazy_cut)
    );
    let spec_targets: usize = plan.triggers.values().map(|t| t.targets.len()).sum();
    let lift_steps: usize = plan
        .lifts
        .values()
        .map(|l| l.steps.iter().filter(|s| s.lift).count())
        .sum();
    let seg_targets: usize = plan.segments.values().map(Vec::len).sum();
    let bodies = plan.bodies.values().filter(|b| !b.is_empty()).count();
    let _ = writeln!(
        o,
        "  推测：{} 个触发点、{spec_targets} 个候选站点；提升：{} 个触发点、{lift_steps} 步；直线段提升：{} 个触发点、{seg_targets} 个候选；向量化：{bodies} 个函数体有候选",
        plan.triggers.len(),
        plan.lifts.len(),
        plan.segments.len()
    );
}

fn diagnostics(o: &mut String, plan: &Plan) {
    o.push('\n');
    match &plan.rejected {
        Some(d) => {
            let _ = writeln!(o, "拒绝：{}", diag_text(&d.code, &d.message));
        }
        None => o.push_str("拒绝：无\n"),
    }
    if plan.warnings.is_empty() {
        o.push_str("告警：无\n");
    }
    for w in &plan.warnings {
        let _ = writeln!(o, "告警：{}", diag_text(&w.code, &w.message));
    }
    o.push('\n');
}

/// `编号：报文`；报文自己已带「编号: 」前缀的去掉，免得重复。
pub fn diag_text(code: &str, message: &str) -> String {
    let msg = message
        .strip_prefix(&format!("{code}: "))
        .unwrap_or(message);
    format!("{code}：{msg}")
}

/// 一项估计写成一句话；三种形状与 [`Estimate`] 的三个变体一一对应。
pub fn est_text<T: Copy>(e: &Estimate<T>, fmt: impl Fn(T) -> String, unit: &str) -> String {
    match e {
        Estimate::Known { lo, hi } => {
            let (l, h) = (fmt(*lo), fmt(*hi));
            if l == h {
                format!("{l} {unit}")
            } else {
                format!("{l} 至 {h} {unit}")
            }
        }
        Estimate::AtLeast { lo } => format!("至少 {} {unit}，上界未知", fmt(*lo)),
        Estimate::Unknown(r) => format!("未估：{}", reason_text(r)),
    }
}

/// 未知的原因写成一句话（字段名原样保留）。
pub fn reason_text(r: &UnknownReason) -> String {
    match r {
        UnknownReason::NotComputed => "plan pass 关着，没有计算".to_string(),
        UnknownReason::Untested(field) => format!("画像字段 {field} 未测"),
        UnknownReason::PriceNotGiven => "宿主没有给判断器单价".to_string(),
        UnknownReason::TooComplex => "程序太大，分析在访问上限处放弃".to_string(),
    }
}

/// 数字：数量级过小或过大的用科学计数法（最短表示，如 `4.2e-8`），其余最多保留 6 位小数、去尾零。
/// 不写数值字面量：这里只是显示口径，不是判断器属性（`scripts/grep_constants.py` 按字面量计数）。
pub fn num(x: f64) -> String {
    if x.classify() == FpCategory::Zero {
        return "0".into();
    }
    let sci = format!("{x:e}");
    let exp: i32 = sci
        .split_once('e')
        .and_then(|(_, e)| e.parse().ok())
        .unwrap_or_default();
    if !(-3..9).contains(&exp) {
        return sci;
    }
    let s = format!("{x:.6}");
    s.trim_end_matches('0').trim_end_matches('.').to_string()
}
