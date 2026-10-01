//! `plan` pass（`21` 步 22 与 23b 的 `plan`，`20` §2.3 不变量 (3)(4)、B42、K-041、K-093）：
//! 计划期估计调用数、层数、费用、时延，可靠下界超预算即拒绝。
//!
//! 口径与预注册在 `地基/过程记录/工程-步22.md` §五·1，这里只写实现要点。
//!
//! **可靠下界**（B42 修订）：只在账本为空且缓存关闭时算；对每条路径必经、含读数站点的层按每层一次调用计。
//! 层 = 必经读数站点之间的数据依赖深度（站点 B 的输入用到站点 A 的出口，B 至少在 A 的下一层）。`lift`、
//! `speculate`、`fuse` 只会把互不依赖的题并到同一层，依赖链上的先后拆不掉，所以这个数与那些 pass 开不开无关。
//! 按「效应都成功」计（主会话裁定 2026-09-29 第十六条）：作者显式写的失败分支（`if is_fail(x)`、`handle`
//! 的 unsure 臂）本来就是分支、不算必经；失败没人处理、直接往上传的，不算另一条路径。
//!
//! **必经**（严格口径，主控 2026-09-29 (A)）：不做静态条件求值；`if` 两支、`&&`/`||` 右侧、`handle` 各臂
//! 都不必经；经形参传进来的方法值不解析（调用它是未知调用）。
//!
//! **上界**（`Estimate` 的 `hi`，只报告不拒）：K-093——`iterate(N, …)` 的上界 = N × step + (N + 1) × measure；
//! `loop(N, …)` = N × 方法体（停止检查写在体内）。判断的每次发出含重试都计入 `cost.calls`，所以判断站点按
//! `attempts` 计。
//!
//! **给 B0487、B0488 的接口**：`Plan.per_site`（登记顺序 `order`、必经、层、执行次数上界）。
//! `select_within`（层内挑选）按 `order` 与剩余预算挑本层子集；`value_density`（价值密度）替换 `order`
//! 的占位，建议放在本文件：`fn value_density(site: &SitePlan) -> f64`（步 30 / B0488）。
//!
//! **下游层数**（步 30 / B0488，K-135 的层数目标；主控 Q5）：`SitePlan.downstream` = 从这个读数站点的输出出发，
//! 沿数据依赖与控制依赖（`if` 条件、`handle` 的出口、`&&`/`||` 的左侧）往后最多还有几层判断。抽象值带一张
//! 「上游读数站点 → 已隔几跳」的表（[`Deps`]），走到读数站点时更新上游各站点的下游层数。循环第二轮起的抽象值带
//! 第一轮输出的表，多轮只展开到第二轮，所以是下界。**上面各项估计的算法不读这张表**，它是旁路，只作层内挑选的排序提示。

use jpp_effects::Profile;
use jpp_ir::ir::{
    Block, Budget, ConsumeHow, Expr, Function, Host, IrDiag, Node, Program, SiteKind, Stmt,
};
use jpp_ir::key::{NodeId, SiteId};
use jpp_ir::plan::{
    CostModel, Estimate, IMPLICIT_RETRY, JudgePrice, Plan, PlanCtx, SitePlan, UnknownReason,
};
use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;

/// 拒绝的诊断码（J-07b 静态面）
pub const E_BUDGET_PLAN: &str = "E-budget-plan";
/// 账本非空或开缓存时，按首跑算会拒的计划只告警
pub const W_COST: &str = "W-cost";
/// 有读数站点而画像没有价格
pub const W_COST_UNKNOWN: &str = "W-cost-unknown";

/// 分析访问节点的上限：超出即放弃，四项估计记 `TooComplex`，不拒
const VISIT_LIMIT: u64 = 2_000_000;

/// 零费用、零时延（不是判断器属性，`scripts/grep_constants.py` 按字面量计数，集中在这一处）
const 零: f64 = 0.0;

/// 会自己发调用、但口径未建的构造：上界未知，不算必经站点
const OPAQUE_CALLERS: &[&str] = &["repeat", "order", "agg", "allocate"];

/// 在 `plan` 上写估计、`per_site`、告警与拒绝。
pub fn run(p: &Program, profile: Option<&Profile>, ctx: &PlanCtx, out: &mut Plan) {
    let scan = Scan::of(p);
    let attempts = attempts_of(&p.budget);
    let mut w = W {
        attempts,
        effectful: scan.effectful,
        calls_hi: Some(0),
        judge_hi: Some(0),
        must_max: 0,
        sites: BTreeMap::new(),
        order: 0,
        stack: vec![],
        visits: 0,
        scopes: vec![],
        ctl: vec![],
    };
    let top = w.scope(None);
    let mode = Mode {
        must: true,
        mult: Some(1),
    };
    w.block(&p.body, &top, mode);
    // 作用域与闭包互相引用（递归函数），分析完清空以断环
    for s in &w.scopes {
        s.vars.borrow_mut().clear();
    }
    if w.visits > VISIT_LIMIT {
        out.calls_est = Estimate::Unknown(UnknownReason::TooComplex);
        out.layers_est = Estimate::Unknown(UnknownReason::TooComplex);
        out.cost_est = Estimate::Unknown(UnknownReason::TooComplex);
        out.latency_est = Estimate::Unknown(UnknownReason::TooComplex);
        return;
    }
    out.per_site = w.sites.clone();
    out.cost_model = cost_model(profile);
    if !scan.effectful {
        out.calls_est = Estimate::Known { lo: 0, hi: 0 };
        out.layers_est = Estimate::Known { lo: 0, hi: 0 };
        out.cost_est = Estimate::Known { lo: 零, hi: 零 };
        out.latency_est = Estimate::Known { lo: 零, hi: 零 };
        return;
    }
    let fresh = ctx.ledger_empty && ctx.cache_off;
    let lo_fresh = u64::from(w.must_max);
    let lo = if fresh { lo_fresh } else { 0 };
    out.calls_est = match w.calls_hi {
        Some(h) => Estimate::Known { lo, hi: h.max(lo) },
        None => Estimate::AtLeast { lo },
    };
    out.layers_est = match w.judge_hi {
        Some(h) => Estimate::Known { lo, hi: h.max(lo) },
        None => Estimate::AtLeast { lo },
    };
    out.cost_est = if !scan.reading {
        if scan.other {
            Estimate::AtLeast { lo: 零 }
        } else {
            Estimate::Known { lo: 零, hi: 零 }
        }
    } else {
        match ctx.judge_price {
            JudgePrice::Known(pr) => {
                let c = lo as f64 * pr;
                if pr == 零 && w.calls_hi.is_some() && !scan.other {
                    Estimate::Known { lo: 零, hi: 零 }
                } else {
                    Estimate::AtLeast { lo: c }
                }
            }
            JudgePrice::Untested => {
                out.warnings.push(IrDiag::new(
                    W_COST_UNKNOWN,
                    format!(
                        "{W_COST_UNKNOWN}: 画像没有 cost.price_usd_per_input_token，计划期的费用估计是 Unknown，预算的费用上限没有核到（20 §3.9；计划期不按费用拒）"
                    ),
                    p.span,
                ));
                Estimate::Unknown(UnknownReason::Untested(
                    "cost.price_usd_per_input_token".into(),
                ))
            }
            JudgePrice::NotGiven => Estimate::Unknown(UnknownReason::PriceNotGiven),
        }
    };
    let p95 = profile.and_then(|pr| pr.latency_p95());
    out.latency_est = if !scan.reading {
        Estimate::Known { lo: 零, hi: 零 }
    } else {
        match p95 {
            None => Estimate::Unknown(UnknownReason::Untested("concurrency.latency_s.p95".into())),
            Some(t) => match w.judge_hi {
                Some(h) => Estimate::Known {
                    lo: lo as f64 * t,
                    hi: h.max(lo) as f64 * t,
                },
                None => Estimate::AtLeast { lo: lo as f64 * t },
            },
        }
    };
    // 拒绝：按首跑的下界核（B42），非首跑只告警
    let b = &p.budget;
    let mut why: Vec<String> = vec![];
    if lo_fresh > b.calls {
        why.push(format!("调用数下界 {lo_fresh} 次 > 预算 calls {}", b.calls));
    }
    if let JudgePrice::Known(pr) = ctx.judge_price
        && pr > 零
        && scan.reading
    {
        let c = lo_fresh as f64 * pr;
        if c > b.cost {
            why.push(format!(
                "费用下界 {c:e} 美元（{lo_fresh} 次判断调用 × 单价 {pr:e} × 每次至少 1 个 token）> 预算 cost {}",
                b.cost
            ));
        }
    }
    if let (Some(t), Some(l)) = (p95, b.latency_p95)
        && scan.reading
    {
        let s = lo_fresh as f64 * t;
        if s > l {
            why.push(format!(
                "时延下界 {s:.3} 秒（{lo_fresh} 层 × 画像 p95 {t} 秒）> 预算 latency_p95 {l} 秒"
            ));
        }
    }
    if why.is_empty() {
        return;
    }
    let 理由 = why.join("；");
    if fresh {
        out.rejected = Some(IrDiag::new(
            E_BUDGET_PLAN,
            format!(
                "计划期拒绝（B42，J-07b）：按效应都成功计，{理由}。下界只数每条路径必经的判断层、每层一次调用，运行一定会超预算，所以一次调用都没发。修法：放宽 budget"
            ),
            p.span,
        ));
    } else {
        out.warnings.push(IrDiag::new(
            W_COST,
            format!(
                "{W_COST}: 按首跑计（效应都成功）{理由}；本次账本非空或开了缓存，站点可能命中而不发，计划期不拒（B42），运行期层边界照核"
            ),
            p.span,
        ));
    }
}

/// 整个程序的静态扫描：有没有效应、读数站点、其他效应。
struct Scan {
    /// 有任何会发调用的节点
    effectful: bool,
    /// 有读数站点（判断类调用）
    reading: bool,
    /// 有 `gen`/`do`/`ask`/`escalate`
    other: bool,
}

impl Scan {
    fn of(p: &Program) -> Scan {
        let mut s = Scan {
            effectful: false,
            reading: false,
            other: false,
        };
        scan_block(&p.body, &mut s);
        s
    }
}

fn scan_block(b: &Block, s: &mut Scan) {
    for st in &b.statements {
        match st {
            Stmt::Let { value, .. } => scan_expr(value, s),
            Stmt::Expr(e) => scan_expr(e, s),
            Stmt::Function { function, .. } => scan_block(&function.body, s),
        }
    }
    if let Some(r) = &b.result {
        scan_expr(r, s);
    }
}

/// 效应在规划眼里的三类，按注册表字段分（效应名只许出现在注册处，`scripts/grep_effect_names.py`）
enum EffectKind {
    /// 产出读数（`judge`）：读数站点，按 `attempts` 计调用
    Reading,
    /// 不进效应行的宿主记账变换（`transform`）：不计调用（B38）
    HostTransform,
    /// 其余（`gen`、`do`、`ask`）：每次一次调用
    Call,
}

fn kind_of(effect: jpp_ir::key::EffectId) -> EffectKind {
    let s = jpp_effects::spec(effect);
    if s.produces_reading {
        EffectKind::Reading
    } else if !s.in_effect_row {
        EffectKind::HostTransform
    } else {
        EffectKind::Call
    }
}

/// 判断类调用每次执行发出几次：`1 + 重试`，作者没声明 `absent` 时按隐含重试（`IMPLICIT_RETRY`）。
/// `run` 与 EXPLAIN 的站点行共用这一处。
pub(crate) fn attempts_of(b: &Budget) -> u64 {
    1 + u64::from(b.absent.as_ref().map_or(IMPLICIT_RETRY, |a| a.retry))
}

/// 站点每次执行发出几次调用的分类（EXPLAIN 的站点行读它，分类只在这里一份，不许另写拷贝）。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CallsClass {
    /// 读数站点（占判断层）：每次执行发 `attempts_of` 次
    Reading,
    /// `gen`、`do`、`ask`、`escalate`：每次执行发 1 次，不占判断层
    Call,
    /// 宿主记账变换与 `consume`：不计调用
    NoCall,
    /// 会自己发判断调用但口径未建的构造（`OPAQUE_CALLERS`）：次数与层未知
    Opaque,
    /// 不是效应站点（规划不为它登记调用）
    Other,
}

/// 按站点种类分类，与 `run` 记账的口径逐条对应：`Effect` 按效应注册表字段（`kind_of`），
/// `literalize` 计读数、`escalate` 计一次、`consume` 不计，`sieve` 计读数，`OPAQUE_CALLERS` 未建口径。
pub(crate) fn site_calls_class(kind: &SiteKind) -> CallsClass {
    match kind {
        SiteKind::Effect(id) => match kind_of(*id) {
            EffectKind::Reading => CallsClass::Reading,
            EffectKind::HostTransform => CallsClass::NoCall,
            EffectKind::Call => CallsClass::Call,
        },
        SiteKind::Consume(ConsumeHow::Literalize) => CallsClass::Reading,
        SiteKind::Consume(ConsumeHow::Escalate) => CallsClass::Call,
        SiteKind::Consume(ConsumeHow::Consume) => CallsClass::NoCall,
        SiteKind::Construct(n) if n == "sieve" => CallsClass::Reading,
        SiteKind::Construct(n) if OPAQUE_CALLERS.contains(&n.as_str()) => CallsClass::Opaque,
        _ => CallsClass::Other,
    }
}

fn scan_expr(e: &Expr, s: &mut Scan) {
    match &e.node {
        Node::Effect { effect, .. } => match kind_of(*effect) {
            EffectKind::Reading => {
                s.effectful = true;
                s.reading = true;
            }
            EffectKind::HostTransform => {}
            EffectKind::Call => {
                s.effectful = true;
                s.other = true;
            }
        },
        Node::Construct { name, .. }
            if name == "sieve" || OPAQUE_CALLERS.contains(&name.as_str()) =>
        {
            s.effectful = true;
            s.reading = true;
        }
        Node::Fit { .. } => {
            s.effectful = true;
            s.reading = true;
        }
        Node::Consume {
            how: ConsumeHow::Literalize,
            ..
        } => {
            s.effectful = true;
            s.reading = true;
        }
        Node::Consume {
            how: ConsumeHow::Escalate,
            ..
        } => {
            s.effectful = true;
            s.other = true;
        }
        _ => {}
    }
    for c in e.children() {
        scan_expr(c, s);
    }
    for b in e.blocks() {
        scan_block(b, s);
    }
}

#[derive(Clone, Copy)]
struct Mode {
    /// 这里的节点每条路径都会求值
    must: bool,
    /// 这里的节点最多执行几次（`None` = 未知）
    mult: Option<u64>,
}

impl Mode {
    fn branch(self) -> Mode {
        Mode {
            must: false,
            mult: self.mult,
        }
    }
    fn times(self, k: u64, must: bool) -> Mode {
        Mode {
            must: self.must && must,
            mult: self.mult.map(|m| m * k),
        }
    }
    fn unknown() -> Mode {
        Mode {
            must: false,
            mult: None,
        }
    }
}

/// 抽象值：这个值出现之前至少已完成多少层判断调用；已知的列表长度与记录字段；可解析的方法值。
#[derive(Clone, Default)]
struct AVal<'p> {
    layer: u32,
    len: Option<usize>,
    fields: Option<Rc<BTreeMap<String, AVal<'p>>>>,
    func: Option<Rc<Clo<'p>>>,
    /// 经形参传进来的方法值（不解析；调用或交给内置都是未知调用）
    opaque_fn: bool,
    /// 这个值依赖的上游读数站点与跳数（步 30 下游层数的旁路）
    deps: Deps,
}

/// 上游读数站点 → 从它的输出到这里隔了几层判断（0 = 就是它的输出）。`None` = 不依赖任何读数站点。
type Deps = Option<Rc<BTreeMap<SiteId, u32>>>;

/// 两张依赖表的并（同一站点取跳数大的）
fn join(a: &Deps, b: &Deps) -> Deps {
    match (a, b) {
        (None, x) | (x, None) => x.clone(),
        (Some(x), Some(y)) => {
            if Rc::ptr_eq(x, y) {
                return a.clone();
            }
            let mut m = (**x).clone();
            for (k, v) in y.iter() {
                let e = m.entry(*k).or_insert(*v);
                *e = (*e).max(*v);
            }
            Some(Rc::new(m))
        }
    }
}

/// 一组抽象值的依赖之并
fn joins(vs: &[AVal<'_>]) -> Deps {
    vs.iter().fold(None, |acc, v| join(&acc, &v.deps))
}

/// 构造或内置调用闭包实参 `f` 时给它的形参（步 30）：每个形参都带「全部非方法实参的依赖之并」。层数仍取 0（与改前
/// 空实参相同），所以只影响下游层数旁路，调用数、层数估计照旧（这里本来按 `Mode::unknown()` 计为未知）
fn closure_args<'p>(f: &AVal<'p>, all: &[AVal<'p>]) -> Vec<AVal<'p>> {
    let n = f.func.as_ref().map_or(0, |c| c.f.parameters.len());
    let data: Vec<AVal<'p>> = all
        .iter()
        .filter(|a| a.func.is_none() && !a.opaque_fn)
        .cloned()
        .collect();
    let e = AVal::at(0).dep(joins(&data));
    vec![e; n]
}

/// 价值密度（步 30 B 段 / B0488；B7、B43，`21`:311 `passes/plan.rs::value_density`）：一组的价值 = 组内各题（账本键去重）的
/// 价值之和。价值按主会话裁定四十六的第二版：记录混淆矩阵（Jeffreys 平滑）上的互信息，无记录为 0（裁定三十八、四十三），
/// 未决比例用本趟读数更新（层边界重规划，K-135；读数进「接下来先问什么」，B22 第 5 类）。成本单位是状态（P5）：一组是一次
/// 调用，调用数是约束时按组的价值排，所以「密度」的分母是 1 次调用，不除以估计的美元费用（过程记录 工程-步30 §一 (d)）。
pub fn value_density(site: &jpp_ir::plan::PendingSite) -> f64 {
    site.values
        .iter()
        .map(|v| {
            let kc = &site.keys[v.key];
            kc.record.as_ref().map_or(0.0, |ch| {
                crate::value::channel_value(v.request, v.k, ch, kc.seen, kc.unsure)
            })
        })
        .sum()
}

/// 报告用的键统计（平滑后 u₀、平滑后 n、本趟更新后的 u_run）；无记录为 `None`
pub fn key_stats(kc: &jpp_ir::plan::KeyCount) -> Option<(f64, f64, f64)> {
    kc.record.as_ref().map(|ch| {
        let (u0, n) = crate::value::unsure_stats(ch);
        (u0, n, crate::value::run_unsure(ch, kc.seen, kc.unsure))
    })
}

/// 从画像读逐组费用模型（步 30，给费用约束下的挑选）：单价与 `cost.regression` 三个系数，任一未测即 `None`
fn cost_model(profile: Option<&Profile>) -> Option<CostModel> {
    let pr = profile?;
    let r = pr.token_regression()?;
    Some(CostModel {
        price: pr.price_per_input_token()?,
        intercept_tokens: r.intercept_tokens,
        state_char_coef: r.state_char_coef,
        question_char_coef: r.question_char_coef,
    })
}

impl<'p> AVal<'p> {
    fn at(layer: u32) -> AVal<'p> {
        AVal {
            layer,
            ..AVal::default()
        }
    }
    /// 带上依赖表
    fn dep(mut self, d: Deps) -> AVal<'p> {
        self.deps = d;
        self
    }
    /// 形参上的值：方法值不透明（严格口径，主控 2026-09-29 (A)）
    fn as_param(&self) -> AVal<'p> {
        AVal {
            layer: self.layer,
            len: self.len,
            fields: self.fields.clone(),
            func: None,
            opaque_fn: self.opaque_fn || self.func.is_some(),
            deps: self.deps.clone(),
        }
    }
}

struct Clo<'p> {
    f: &'p Function,
    env: Rc<Scope<'p>>,
}

struct Scope<'p> {
    vars: RefCell<Vec<(String, AVal<'p>)>>,
    parent: Option<Rc<Scope<'p>>>,
}

impl<'p> Scope<'p> {
    fn lookup(&self, n: &str) -> Option<AVal<'p>> {
        if let Some((_, v)) = self.vars.borrow().iter().rev().find(|(k, _)| k == n) {
            return Some(v.clone());
        }
        self.parent.as_ref().and_then(|p| p.lookup(n))
    }
    fn bind(&self, n: &str, v: AVal<'p>) {
        self.vars.borrow_mut().push((n.to_string(), v));
    }
}

struct W<'p> {
    attempts: u64,
    effectful: bool,
    /// 调用数上界（含重试；`None` = 未知）
    calls_hi: Option<u64>,
    /// 判断站点执行次数上界（层数上界）
    judge_hi: Option<u64>,
    /// 必经读数站点的最大层
    must_max: u32,
    sites: BTreeMap<SiteId, SitePlan>,
    order: u32,
    stack: Vec<NodeId>,
    visits: u64,
    scopes: Vec<Rc<Scope<'p>>>,
    /// 控制依赖：外层 `if` 条件、`handle` 出口、`&&`/`||` 左侧的依赖表（进入分支时压栈）
    ctl: Vec<Deps>,
}

fn add(acc: &mut Option<u64>, per: u64, m: Mode) {
    if per == 0 {
        return;
    }
    *acc = match (*acc, m.mult) {
        (Some(a), Some(k)) => Some(a + k * per),
        _ => None,
    };
}

fn literal_int(e: &Expr) -> Option<i64> {
    match &e.node {
        Node::Host(Host::Integer(n)) => Some(*n),
        _ => None,
    }
}

impl<'p> W<'p> {
    fn scope(&mut self, parent: Option<Rc<Scope<'p>>>) -> Rc<Scope<'p>> {
        let s = Rc::new(Scope {
            vars: RefCell::new(vec![]),
            parent,
        });
        self.scopes.push(s.clone());
        s
    }

    /// 未知调用：程序里有效应时，上界变未知
    fn unknown_call(&mut self) {
        if self.effectful {
            self.calls_hi = None;
            self.judge_hi = None;
        }
    }

    /// 记一个效应站点
    fn site(&mut self, id: SiteId, m: Mode, layer: Option<u32>, execs: Option<u64>) {
        if !self.sites.contains_key(&id) {
            self.sites.insert(
                id,
                SitePlan {
                    order: self.order,
                    must: false,
                    layer_lo: None,
                    execs_hi: Some(0),
                    downstream: 0,
                },
            );
            self.order += 1;
        }
        if m.must
            && let Some(l) = layer
        {
            self.must_max = self.must_max.max(l);
        }
        let e = self.sites.get_mut(&id).expect("刚插入");
        if m.must
            && let Some(l) = layer
        {
            e.must = true;
            e.layer_lo = Some(e.layer_lo.map_or(l, |x| x.max(l)));
        }
        e.execs_hi = match (e.execs_hi, execs, m.mult) {
            (Some(a), Some(x), Some(k)) => Some(a + x * k),
            _ => None,
        };
    }

    /// 读数站点 `id` 的依赖旁路（步 30）：输入依赖并上控制依赖，更新上游各站点的下游层数，返回这个站点输出的依赖表
    fn reading_deps(&mut self, id: SiteId, input: Deps) -> Deps {
        let input = self.ctl.iter().fold(input, |acc, c| join(&acc, c));
        let mut out: BTreeMap<SiteId, u32> = BTreeMap::new();
        if let Some(m) = &input {
            for (s, h) in m.iter() {
                if let Some(e) = self.sites.get_mut(s) {
                    e.downstream = e.downstream.max(h + 1);
                }
                out.insert(*s, h + 1);
            }
        }
        out.insert(id, 0);
        Some(Rc::new(out))
    }

    /// 在控制依赖 `c` 之下求值 `f`
    fn under<T>(&mut self, c: Deps, f: impl FnOnce(&mut Self) -> T) -> T {
        self.ctl.push(c);
        let r = f(self);
        self.ctl.pop();
        r
    }

    fn block(&mut self, b: &'p Block, env: &Rc<Scope<'p>>, m: Mode) -> AVal<'p> {
        let s = self.scope(Some(env.clone()));
        for st in &b.statements {
            if let Stmt::Function { name, function, .. } = st {
                s.bind(
                    name,
                    AVal {
                        func: Some(Rc::new(Clo {
                            f: function,
                            env: s.clone(),
                        })),
                        ..AVal::default()
                    },
                );
            }
        }
        for st in &b.statements {
            match st {
                Stmt::Let { name, value, .. } => {
                    let v = self.expr(value, &s, m);
                    s.bind(name, v);
                }
                Stmt::Expr(e) => {
                    self.expr(e, &s, m);
                }
                Stmt::Function { .. } => {}
            }
        }
        match &b.result {
            Some(r) => self.expr(r, &s, m),
            None => AVal::default(),
        }
    }

    fn call(&mut self, c: &Rc<Clo<'p>>, args: &[AVal<'p>], m: Mode) -> AVal<'p> {
        let top = args.iter().map(|a| a.layer).max().unwrap_or(0);
        if self.stack.contains(&c.f.id) {
            self.unknown_call();
            return AVal::at(top).dep(joins(args));
        }
        let s = self.scope(Some(c.env.clone()));
        for (i, p) in c.f.parameters.iter().enumerate() {
            s.bind(&p.name, args.get(i).map(AVal::as_param).unwrap_or_default());
        }
        self.stack.push(c.f.id);
        let r = self.block(&c.f.body, &s, m);
        self.stack.pop();
        r
    }

    /// 调一个抽象值：可解析的方法值进体，否则是未知调用
    fn apply(&mut self, f: &AVal<'p>, args: &[AVal<'p>], m: Mode) -> AVal<'p> {
        match &f.func {
            Some(c) => self.call(&c.clone(), args, m),
            None => {
                self.unknown_call();
                AVal::at(args.iter().map(|a| a.layer).max().unwrap_or(0)).dep(joins(args))
            }
        }
    }

    fn exprs(&mut self, es: &[&'p Expr], env: &Rc<Scope<'p>>, m: Mode) -> Vec<AVal<'p>> {
        es.iter().map(|e| self.expr(e, env, m)).collect()
    }

    fn expr(&mut self, e: &'p Expr, env: &Rc<Scope<'p>>, m: Mode) -> AVal<'p> {
        self.visits += 1;
        if self.visits > VISIT_LIMIT {
            return AVal::default();
        }
        let top = |vs: &[AVal<'p>]| vs.iter().map(|v| v.layer).max().unwrap_or(0);
        match &e.node {
            Node::State { .. } | Node::Cut { .. } => {
                let vs = self.exprs(&e.children(), env, m);
                AVal::at(top(&vs)).dep(joins(&vs))
            }
            Node::Fit { .. } => {
                let vs = self.exprs(&e.children(), env, m);
                self.unknown_call();
                AVal::at(top(&vs)).dep(joins(&vs))
            }
            Node::Effect { effect, site, .. } => {
                let vs = self.exprs(&e.children(), env, m);
                let d = top(&vs);
                match kind_of(*effect) {
                    EffectKind::Reading => {
                        add(&mut self.calls_hi, self.attempts, m);
                        add(&mut self.judge_hi, 1, m);
                        self.site(*site, m, Some(d + 1), Some(1));
                        let out = self.reading_deps(*site, joins(&vs));
                        AVal::at(d + 1).dep(out)
                    }
                    // 宿主记账变换不计调用（B38）；方法按 J-11 是纯的
                    EffectKind::HostTransform => AVal::at(d).dep(joins(&vs)),
                    EffectKind::Call => {
                        add(&mut self.calls_hi, 1, m);
                        self.site(*site, m, None, Some(1));
                        AVal::at(d).dep(joins(&vs))
                    }
                }
            }
            Node::Loop { bound, rest, .. } => {
                let b = self.expr(bound, env, m);
                let init = rest
                    .first()
                    .map(|x| self.expr(x, env, m))
                    .unwrap_or_default();
                let f = rest
                    .get(1)
                    .map(|x| self.expr(x, env, m))
                    .unwrap_or_default();
                let unk = AVal::at(init.layer).dep(init.deps.clone());
                let (r, rd) = match literal_int(bound) {
                    Some(n) if n >= 1 => {
                        let n = n as u64;
                        let r0 = self.apply(&f, &[init.clone(), AVal::at(0)], m.times(1, true));
                        if n > 1 {
                            // 第二轮起带第一轮输出的依赖（下游层数旁路；层数估计照旧用 init.layer）
                            let unk = unk.dep(join(&init.deps, &r0.deps));
                            self.apply(&f, &[unk, AVal::at(0)], m.times(n - 1, false));
                        }
                        (r0.layer, r0.deps)
                    }
                    _ => {
                        let r = self.apply(&f, &[unk, AVal::at(0)], Mode::unknown());
                        (init.layer, join(&init.deps, &r.deps))
                    }
                };
                AVal::at(r.max(b.layer)).dep(join(&rd, &b.deps))
            }
            Node::Handle {
                exit, arms, rest, ..
            } => {
                let x = self.expr(exit, env, m);
                let arm_m = m.branch();
                let mut least: Option<u32> = None;
                let mut rd = x.deps.clone();
                match &arms.node {
                    Node::Host(Host::Record(fs)) => {
                        for (_, a) in fs {
                            let v = self.expr(a, env, m);
                            let xa = AVal::at(x.layer).dep(x.deps.clone());
                            let r = self.under(x.deps.clone(), |w| w.apply(&v, &[xa], arm_m));
                            rd = join(&rd, &r.deps);
                            least = Some(least.map_or(r.layer, |l| l.min(r.layer)));
                        }
                    }
                    _ => {
                        self.expr(arms, env, m);
                        self.unknown_call();
                    }
                }
                for r in rest {
                    self.expr(r, env, m);
                }
                AVal::at(x.layer.max(least.unwrap_or(0))).dep(rd)
            }
            Node::Consume { how, args, site } => {
                let vs = self.exprs(&args.iter().collect::<Vec<_>>(), env, m);
                let d = top(&vs);
                match how {
                    ConsumeHow::Literalize => {
                        add(&mut self.calls_hi, self.attempts, m);
                        add(&mut self.judge_hi, 1, m);
                        self.site(*site, m, Some(d + 1), Some(1));
                        let out = self.reading_deps(*site, joins(&vs));
                        AVal::at(d + 1).dep(out)
                    }
                    ConsumeHow::Escalate => {
                        add(&mut self.calls_hi, 1, m);
                        self.site(*site, m, None, Some(1));
                        AVal::at(d).dep(joins(&vs))
                    }
                    ConsumeHow::Consume => AVal::at(d).dep(joins(&vs)),
                }
            }
            Node::Construct { name, args, site } => self.construct(name, args, *site, env, m),
            Node::Host(h) => self.host(h, e, env, m),
        }
    }

    fn construct(
        &mut self,
        name: &str,
        args: &'p [Expr],
        site: SiteId,
        env: &Rc<Scope<'p>>,
        m: Mode,
    ) -> AVal<'p> {
        let top = |vs: &[AVal<'p>]| vs.iter().map(|v| v.layer).max().unwrap_or(0);
        match name {
            "sieve" => {
                let vs = self.exprs(&args.iter().collect::<Vec<_>>(), env, m);
                let d = top(&vs);
                let k = vs.first().and_then(|v| v.len);
                // sieve(材料, 题式, [填法…]) 或 sieve(材料, [题…])：题数；按不融合计，保证是上界
                let q = vs
                    .get(2)
                    .and_then(|v| v.len)
                    .or_else(|| vs.get(1).and_then(|v| v.len))
                    .unwrap_or(1) as u64;
                match k {
                    Some(k) => {
                        let k = k as u64;
                        add(&mut self.calls_hi, k * q * self.attempts, m);
                        add(&mut self.judge_hi, k * q, m);
                        let mm = if k >= 1 { m } else { m.branch() };
                        self.site(site, mm, Some(d + 1), Some(k * q));
                    }
                    None => {
                        self.calls_hi = None;
                        self.judge_hi = None;
                        self.site(site, m.branch(), None, None);
                    }
                }
                let out = self.reading_deps(site, joins(&vs));
                AVal::at(d + 1).dep(out)
            }
            "iterate" if args.len() == 4 => {
                let b = self.expr(&args[0], env, m);
                let init = self.expr(&args[1], env, m);
                let step = self.expr(&args[2], env, m);
                let measure = self.expr(&args[3], env, m);
                // measure 为 "tokens"（文字）时不含调用
                let measure_calls = !matches!(args[3].node, Node::Host(Host::Text(_)));
                let unk = AVal::at(init.layer).dep(init.deps.clone());
                match literal_int(&args[0]) {
                    Some(n) if n >= 1 => {
                        let n = n as u64;
                        // K-093：先算一次 measure，每轮 step 之后再算一次——N × step + (N + 1) × measure
                        if measure_calls {
                            self.apply(&measure, std::slice::from_ref(&init), m.times(1, true));
                        }
                        let r0 = self.apply(&step, &[init.clone(), AVal::at(0)], m.times(1, true));
                        // 第二轮起带第一轮输出的依赖（下游层数旁路；层数估计照旧用 init.layer）
                        let unk = unk.dep(join(&init.deps, &r0.deps));
                        if n > 1 {
                            self.apply(&step, &[unk.clone(), AVal::at(0)], m.times(n - 1, false));
                        }
                        if measure_calls {
                            self.apply(&measure, &[unk], m.times(n, false));
                        }
                        AVal::at(r0.layer.max(b.layer)).dep(join(&r0.deps, &b.deps))
                    }
                    _ => {
                        let r = self.apply(&step, &[unk.clone(), AVal::at(0)], Mode::unknown());
                        if measure_calls {
                            self.apply(&measure, &[unk], Mode::unknown());
                        }
                        AVal::at(init.layer.max(b.layer))
                            .dep(join(&join(&init.deps, &r.deps), &b.deps))
                    }
                }
            }
            _ => {
                let vs = self.exprs(&args.iter().collect::<Vec<_>>(), env, m);
                if OPAQUE_CALLERS.contains(&name) {
                    self.calls_hi = None;
                    self.judge_hi = None;
                    self.site(site, m.branch(), None, None);
                }
                // 构造可能调用实参里的方法值，次数未知。形参取「带全部非方法实参依赖」的元素值（步 30：`pair`
                // 等调闭包时列表实参的依赖要进闭包里的判断，复核 B0488-A 缺口 2）
                let mut rd = joins(&vs);
                for v in &vs {
                    if v.func.is_some() || v.opaque_fn {
                        let args = closure_args(v, &vs);
                        let r = self.apply(v, &args, Mode::unknown());
                        rd = join(&rd, &r.deps);
                    }
                }
                AVal::at(top(&vs)).dep(rd)
            }
        }
    }

    fn host(&mut self, h: &'p Host, e: &'p Expr, env: &Rc<Scope<'p>>, m: Mode) -> AVal<'p> {
        let top = |vs: &[AVal<'p>]| vs.iter().map(|v| v.layer).max().unwrap_or(0);
        match h {
            Host::Integer(_) | Host::Decimal(_) | Host::Bool(_) | Host::Text(_) | Host::Unit => {
                AVal::default()
            }
            Host::Name(n) => env.lookup(n).unwrap_or_default(),
            Host::List(xs) => {
                let vs = self.exprs(&xs.iter().collect::<Vec<_>>(), env, m);
                AVal {
                    layer: top(&vs),
                    len: Some(xs.len()),
                    deps: joins(&vs),
                    ..AVal::default()
                }
            }
            Host::Record(fs) => {
                let mut map = BTreeMap::new();
                let mut l = 0;
                let mut d: Deps = None;
                for (k, x) in fs {
                    let v = self.expr(x, env, m);
                    l = l.max(v.layer);
                    d = join(&d, &v.deps);
                    map.insert(k.clone(), v);
                }
                AVal {
                    layer: l,
                    fields: Some(Rc::new(map)),
                    deps: d,
                    ..AVal::default()
                }
            }
            Host::Function(f) => AVal {
                func: Some(Rc::new(Clo {
                    f,
                    env: env.clone(),
                })),
                ..AVal::default()
            },
            Host::Call { callee, args, .. } => {
                let argv = self.exprs(&args.iter().collect::<Vec<_>>(), env, m);
                if let Node::Host(Host::Name(n)) = &callee.node {
                    match env.lookup(n) {
                        Some(v) => self.apply(&v, &argv, m),
                        None => self.builtin(n, &argv, m),
                    }
                } else {
                    let f = self.expr(callee, env, m);
                    self.apply(&f, &argv, m)
                }
            }
            Host::Field { value, field } => {
                let v = self.expr(value, env, m);
                match v.fields.as_ref().and_then(|fs| fs.get(field)) {
                    // 字段自带依赖（记录的依赖是各字段之并，取字段自己的更准）
                    Some(x) => {
                        let mut x = x.clone();
                        x.layer = x.layer.max(v.layer);
                        x
                    }
                    None => AVal::at(v.layer).dep(v.deps.clone()),
                }
            }
            Host::Index { value, index } => {
                let a = self.expr(value, env, m);
                let b = self.expr(index, env, m);
                AVal::at(a.layer.max(b.layer)).dep(join(&a.deps, &b.deps))
            }
            Host::Unary { value, .. } => {
                let v = self.expr(value, env, m);
                AVal::at(v.layer).dep(v.deps)
            }
            Host::Binary { op, left, right } => {
                let a = self.expr(left, env, m);
                let short = op == "&&" || op == "||";
                let rm = if short { m.branch() } else { m };
                // 短路：右侧要不要求值取决于左侧（控制依赖）
                let b = if short {
                    self.under(a.deps.clone(), |w| w.expr(right, env, rm))
                } else {
                    self.expr(right, env, rm)
                };
                AVal::at(a.layer.max(b.layer)).dep(join(&a.deps, &b.deps))
            }
            Host::If {
                condition, yes, no, ..
            } => {
                let c = self.expr(condition, env, m);
                let (y, n) = self.under(c.deps.clone(), |w| {
                    (w.block(yes, env, m.branch()), w.block(no, env, m.branch()))
                });
                AVal::at(c.layer.max(y.layer.min(n.layer)))
                    .dep(join(&c.deps, &join(&y.deps, &n.deps)))
            }
            Host::Block(b) => {
                let _ = e;
                self.block(b, env, m)
            }
        }
    }

    /// 宿主内置：`map`/`filter`/`fold` 按列表长度进方法体；其余内置若收了方法值，次数未知
    fn builtin(&mut self, n: &str, argv: &[AVal<'p>], m: Mode) -> AVal<'p> {
        let top = argv.iter().map(|v| v.layer).max().unwrap_or(0);
        match n {
            "map" | "filter" if argv.len() == 2 => {
                let list = &argv[0];
                let elem = AVal::at(list.layer).dep(list.deps.clone());
                let r = match list.len {
                    Some(0) => AVal::at(list.layer),
                    Some(k) => self.apply(&argv[1], &[elem], m.times(k as u64, true)),
                    None => {
                        let r = self.apply(&argv[1], &[elem], Mode::unknown());
                        AVal::at(list.layer).dep(r.deps)
                    }
                };
                AVal {
                    layer: list.layer.max(r.layer),
                    len: if n == "map" { list.len } else { None },
                    deps: join(&list.deps, &r.deps),
                    ..AVal::default()
                }
            }
            "fold" if argv.len() == 3 => {
                let list = &argv[0];
                let elem = AVal::at(list.layer).dep(list.deps.clone());
                let init = argv[1].clone();
                let unk = AVal::at(init.layer).dep(init.deps.clone());
                let (r, rd) = match list.len {
                    Some(0) => (init.layer, init.deps.clone()),
                    Some(k) => {
                        let r0 =
                            self.apply(&argv[2], &[init.clone(), elem.clone()], m.times(1, true));
                        if k > 1 {
                            // 第二轮起带第一轮输出的依赖（下游层数旁路）
                            let unk = unk.dep(join(&init.deps, &r0.deps));
                            self.apply(&argv[2], &[unk, elem], m.times(k as u64 - 1, false));
                        }
                        (r0.layer, r0.deps)
                    }
                    None => {
                        let r = self.apply(&argv[2], &[unk, elem], Mode::unknown());
                        (init.layer, join(&init.deps, &r.deps))
                    }
                };
                AVal::at(r.max(list.layer)).dep(join(&rd, &list.deps))
            }
            _ => {
                let mut rd = joins(argv);
                for v in argv {
                    if v.func.is_some() || v.opaque_fn {
                        let args = closure_args(v, argv);
                        let r = self.apply(v, &args, Mode::unknown());
                        rd = join(&rd, &r.deps);
                    }
                }
                AVal::at(top).dep(rd)
            }
        }
    }
}
