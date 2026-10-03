//! Host wiring only: fixed observations, local action registration and report I/O.
//! 动作的事实与实现在 lib 目标 `jpp::actions` 的表里（B150；比赛块 C-1、C-1b；R2b 六个
//! `graph:*` 动作已接入该表，见 `crates/jpp/src/actions/mod.rs::builtin_actions()`），这里只注册。
use jpp::interp::{Estimate, UnknownReason};
use jpp::{Program, effects::CalibStore, interp::ActionRegistry, ledger::LedgerPort};
use serde_json::{Value, json};

/// 跨运行缓存与生成器身份（步 19，B151）：`--cache` 目录建的索引、`--gen-model` 的模型与生成器画像哈希。
#[derive(Default)]
pub struct CacheArgs<'c> {
    pub cache: Option<&'c dyn jpp::effects::CacheLookup>,
    pub gen_model: Option<String>,
    pub gen_profile_hash: Option<String>,
    /// 标准库与题库版本（步 27，B48）：进账本头 `lib_version`、`bank_version`；首跑、续接、重放都要给，
    /// 否则重放会把「版本不同」误报成 `W-header`
    pub lib_version: Option<String>,
    pub bank_version: Option<String>,
    /// 判断器的实际单价（步 22）：CLI 按后端填；缺省「没说」（重放）
    pub judge_price: jpp::interp::JudgePrice,
    /// `--explain`（Z0190 后一半）：运行前一刻用同一份计划调它；文本模式在回调里打印并返回 `None`，
    /// `--json` 模式返回要并入报告 `explain` 键的 JSON。缺省不调，报告逐字节不变
    pub explain: Option<&'c ExplainFn<'c>>,
    /// 宿主给的本段追踪上下文（C-2，`--trace-parent` / `--trace-seed`）；不给由 `Session` 按账本推导。
    /// 给了，报告多一栏 `span`（本段的 `{trace, span, parent, traceparent}`，宿主拿 `traceparent` 传给下一次调用）
    pub trace: Option<jpp::ledger::TraceCtx>,
    /// 费用确认（Z0236，`11` §5.5）：给了就在开跑前核阈值，超了没有 `--confirm` 即拒；缺省不设门（库与测试入口不变）
    pub confirm: Option<ConfirmArgs>,
    /// 上游余额文件（C-3，`--carry-in`）：读成 `CarryRecord` 交给会话；重放不给（重放取账本头）
    pub carry_in: Option<std::path::PathBuf>,
    /// 交回余额写到哪里（C-3，`--carry-out`）：与报告的 `carry` 段同一份
    pub carry_out: Option<std::path::PathBuf>,
    /// 显式重新授权（C-3 R1，`--carry-reauthorize`）
    pub carry_reauthorize: bool,
    /// 伴随题的发法（B0492 S5，`--companions`）：`None` 用 `Passes` 的默认值
    pub companions: Option<jpp::interp::CompanionMode>,
    /// 料库目录（B0472，`--mat-store`）：装文件料库（`FileMatStore`），`select` 的标记落盘、跨运行复用。
    /// 只凭账本重放不装（变换结果取自账本）
    pub mat_store: Option<std::path::PathBuf>,
    /// 宿主登记的世界（Z0885，`--env`）：进动作上下文，`env:step` 按名字找命令
    pub envs: Vec<(String, Vec<String>)>,
    /// 单元图开关（C2b，`--cells`）：`None` 用会话缺省（开）
    pub cells: Option<bool>,
    /// 报告带单元图统计（C2c，`--cells-stats`）
    pub cells_stats: bool,
}

/// 上游余额文件读写出错（C-3）：用法层面的错误，程序没有开始执行
fn carry_err(msg: String) -> jpp::Error {
    jpp::Error::Runtime(jpp::RtError::new(
        Some("E-carry"),
        msg,
        jpp::Span::default(),
    ))
}

/// 费用确认的命令行输入：是否给了 `--confirm`、这一趟的阈值（美元）、阈值是否取的默认。
#[derive(Clone, Copy, Debug)]
pub struct ConfirmArgs {
    pub confirmed: bool,
    pub threshold_usd: f64,
    pub threshold_is_default: bool,
}

/// `--explain` 的回调：拿到运行前的计划与规划语境。
/// 第三个参数是规划实际用的程序（C-3 G1：带上游余额时是收紧了预算的那份，预算行与拒绝结论同源，裁定三十二）。
pub type ExplainFn<'c> = dyn Fn(
        &jpp::interp::Plan,
        &jpp::interp::PlanCtx,
        &Program,
        Option<&jpp_plan::explain::ConfirmView>,
    ) -> Option<Value>
    + 'c;

/// 费用确认的判定（Z0236，`11` §5.5；过程记录 `工程-Z0236-确认阈值.md` §一、§四）。比大小只在这一处，
/// `--explain` 的文本与 JSON、拒绝、报告 `confirm` 键都读它的结果。
///
/// 会不会核：重放不发调用，免核；其余看有没有会花钱的端口——判断器单价大于 0，或给了 `--gen-model`
/// （生成器 `claude -p` 也花钱，固定观察后端的判断器单价是 0，只看判断器单价会漏）。两者都没有：单价为 0
/// 免核（固定观察），单价未测或没给则美元折不出来，放行并标「未核」（主控 2026-09-29 定，读法 2）。
///
/// 上界：生效费用上限 `budget.cost`（程序声明与上游余额取小，C-3；`budget_cost` 参数传的就是生效预算的 cost）与
/// 计划估计的费用上界（已知时）取小。`budget.cost` 是天花板，
/// 不是花费的保证：运行时发出前预检的 usd 传 0，已花超过它才停，所以首发与最后一次调用可超出，生成器按调用数
/// 预留、usd 事后计，一层里多个生成可超出多次；token 上界估计未建（复核 G1）。
/// 今天规划器在单价大于 0 时没有上界（没有 token 上界估计器），取小恒等于 `budget.cost`；规划器补上界后，
/// 这里不用改。超阈值是严格大于。
pub fn confirm_view(
    replay: bool,
    price: jpp::interp::JudgePrice,
    has_gen: bool,
    plan: &jpp::interp::Plan,
    budget_cost: f64,
    args: ConfirmArgs,
) -> jpp_plan::explain::ConfirmView {
    use jpp::interp::JudgePrice;
    use jpp_plan::explain::{ConfirmVerdict as V, ConfirmView};
    let mk = |verdict: V, upper: Option<f64>, from: &'static str| ConfirmView {
        threshold_usd: args.threshold_usd,
        threshold_is_default: args.threshold_is_default,
        upper_usd: upper,
        upper_from: from,
        verdict,
        confirmed: args.confirmed,
    };
    if replay {
        return mk(V::ExemptReplay, None, "");
    }
    let priced = matches!(price, JudgePrice::Known(p) if p > 0.0);
    if !priced && !has_gen {
        return match price {
            JudgePrice::Known(_) => mk(V::ExemptNoPaidPort, None, ""),
            JudgePrice::Untested | JudgePrice::NotGiven => mk(V::Unchecked, None, ""),
        };
    }
    let (upper, from) = match plan.cost_est.hi() {
        Some(h) if h < budget_cost => (h, "plan"),
        _ => (budget_cost, "budget.cost"),
    };
    let verdict = if upper > args.threshold_usd {
        V::Over
    } else {
        V::Within
    };
    mk(verdict, Some(upper), from)
}

/// 判定写成 JSON（`explain` 键与报告 `confirm` 键共用一个形状）。
pub fn confirm_json(v: &jpp_plan::explain::ConfirmView) -> Value {
    json!({
        "threshold_usd": v.threshold_usd,
        "threshold_is_default": v.threshold_is_default,
        "upper_usd": v.upper_usd,
        "upper_from": (!v.upper_from.is_empty()).then_some(v.upper_from),
        "verdict": v.verdict.code(),
        "confirmed": v.confirmed,
    })
}

/// 报告 `confirm` 键只在这一趟真的核过阈值时出现：免确认（固定观察、重放）的报告逐字节不变。
fn is_checked(v: &jpp_plan::explain::ConfirmView) -> bool {
    use jpp_plan::explain::ConfirmVerdict as V;
    !matches!(v.verdict, V::ExemptReplay | V::ExemptNoPaidPort)
}

/// 被拒的报文（`E-confirm-required`）：阈值、上界与来源、修法。
fn refusal_message(v: &jpp_plan::explain::ConfirmView) -> String {
    let which = if v.threshold_is_default {
        "默认阈值"
    } else {
        "--confirm-above"
    };
    let from = if v.upper_from == "plan" {
        "计划估计的费用上界"
    } else if v.upper_from == "carry" {
        "上游余额收紧后的费用上限（余额比程序声明的 budget cost 更紧；运行时已花超过它才停，可超出最后一次调用；token 上界估计未建）"
    } else {
        "生效费用上限 budget cost（程序声明与上游余额取小；运行时已花超过它才停，可超出最后一次调用；token 上界估计未建）"
    };
    format!(
        "费用上界 {} 美元（{from}）超过确认阈值 {} 美元（{which}），没有给 --confirm，一次调用都没发（11 §5.5）。修法：确认这一趟的花费就加 --confirm；调整阈值用 --confirm-above <美元>；或把程序的 budget cost 改到阈值以内",
        jpp_plan::explain::num(v.upper_usd.unwrap_or_default()),
        jpp_plan::explain::num(v.threshold_usd),
    )
}

/// 写 `--carry-out`（C-3）：与报告的 `carry` 段同一份
fn 写余额(p: &std::path::Path, c: &jpp::BudgetCarry) -> Result<(), jpp::Error> {
    let text = serde_json::to_string_pretty(c.record()).expect("余额可序列化");
    std::fs::write(p, text + "\n")
        .map_err(|e| carry_err(format!("--carry-out {} 写不进：{e}", p.display())))
}

/// 程序里第一个可能不可逆的 `do` 的动作名（步 18b，`E-ledger-required` 用）：动作名是字面量且在
/// 宿主动作表（lib 目标 `jpp::actions`，比赛块 C-1、C-1b）里登记为不可逆；动作名不是字面量的 `do` 按可能不可逆处理
/// （表里有不可逆动作时）。没登记的字面名不算（运行到那里是 J-11）。
pub(crate) fn irreversible_action_in(program: &Program) -> Option<String> {
    let 表 = jpp::actions::builtin_actions();
    let 有不可逆 = 表.iter().any(|a| !a.reversible);
    jpp::check::do_sites(program)
        .into_iter()
        .find_map(|name| match name {
            Some(n) => 表
                .iter()
                .find(|a| a.name == n)
                .filter(|a| !a.reversible)
                .map(|_| n),
            None if 有不可逆 => Some("<动作名不是字面量>".to_string()),
            None => None,
        })
}

/// 计划估计的一项进报告（Z0190；`20` §2.3 `Estimate`）：已知区间、只知下界、未知（带原因）三种形状，
/// 与 `Estimate` 三个变体一一对应；未知显示为未知，不写 0（`14` 附录）。
fn estimate_json<T: Copy + serde::Serialize>(e: &Estimate<T>) -> Value {
    match e {
        Estimate::Known { lo, hi } => json!({"kind": "known", "lo": lo, "hi": hi}),
        Estimate::AtLeast { lo } => json!({"kind": "at_least", "lo": lo}),
        Estimate::Unknown(r) => json!({"kind": "unknown", "reason": reason_str(r)}),
    }
}

/// 未知原因的稳定字符串（报告 `plan` 段与 `explain` 键共用）。
fn reason_str(r: &UnknownReason) -> String {
    match r {
        UnknownReason::NotComputed => "not_computed".to_string(),
        UnknownReason::Untested(field) => format!("untested:{field}"),
        UnknownReason::PriceNotGiven => "price_not_given".to_string(),
        UnknownReason::TooComplex => "too_complex".to_string(),
    }
}

/// `explain` 键里的估计（裁定三十二）：`{"lo","hi"}` / `{"lo"}` / `{"unknown": 原因}`，未知不写 0。
fn est_plain_json<T: Copy + serde::Serialize>(e: &Estimate<T>) -> Value {
    match e {
        Estimate::Known { lo, hi } => json!({"lo": lo, "hi": hi}),
        Estimate::AtLeast { lo } => json!({"lo": lo}),
        Estimate::Unknown(r) => json!({"unknown": reason_str(r)}),
    }
}

/// 运行前一刻的计划与规划语境（Z0190）：输入与 `Interp::run` 入口算计划时同源（画像取校准视图里的、
/// 账本是否为空、缓存是否关、判断器单价），所以与运行时看到的是同一份：续接与审计重放的账本非空，
/// 下界为 0（`plan_estimate.rs` ③ 列）。报告 `plan` 段与 `--explain` 都从这一份 `Plan` 出（裁定三十二：三处同源）。
fn plan_of(
    program: &Program,
    calibrations: &CalibStore,
    ledger: &dyn LedgerPort,
    cache_off: bool,
    judge_price: jpp::interp::JudgePrice,
) -> (jpp::interp::Plan, jpp::interp::PlanCtx) {
    let ctx = jpp::interp::PlanCtx {
        ledger_empty: ledger.view().entries.is_empty(),
        cache_off,
        judge_price,
    };
    let plan = jpp::interp::plan_with(
        program,
        &jpp::interp::Passes::default(),
        Some(jpp_effects::views::CalibView::profile(calibrations)),
        &ctx,
    );
    (plan, ctx)
}

/// 报告的 `plan` 段：`plan_of` 算出的四项估计（调用数、层数、费用、时延）。
fn plan_section(plan: &jpp::interp::Plan) -> Value {
    json!({
        "calls": estimate_json(&plan.calls_est),
        "layers": estimate_json(&plan.layers_est),
        "cost_usd": estimate_json(&plan.cost_est),
        "latency_s": estimate_json(&plan.latency_est),
    })
}

/// 站点偏移换成「文件名:行:列」与那一行源码（`jpp-plan` 是纯函数 crate，不读源文件）。
fn explain_locator(
    loaded: &jpp_syntax::loader::LoadedProgram,
) -> impl Fn(jpp_ir::ir::Span) -> Option<jpp_plan::Located> + '_ {
    move |s| {
        let loc = loaded.locate(jpp_syntax::ast::Span {
            start: s.start,
            end: s.end,
        })?;
        let file = loaded
            .sources
            .iter()
            .find(|f| f.path.to_string_lossy() == loc.file)?;
        let name = file.path.file_name()?.to_string_lossy().into_owned();
        let line: String = file
            .text
            .lines()
            .nth(loc.line.saturating_sub(1))
            .unwrap_or("")
            .trim()
            .to_string();
        // 站点行只需认得是哪一句：过长的截断
        let source = if line.chars().count() > 72 {
            format!("{}…", line.chars().take(72).collect::<String>())
        } else {
            line
        };
        Some(jpp_plan::Located {
            place: format!("{name}:{}:{}", loc.line, loc.col),
            source,
        })
    }
}

/// `--explain` 的文本（`check` 打到 stdout，`run` 打到 stderr）。
pub fn explain_text(
    plan: &jpp::interp::Plan,
    ctx: &jpp::interp::PlanCtx,
    program: &Program,
    loaded: &jpp_syntax::loader::LoadedProgram,
) -> String {
    // `check` 不知道后端与命令行的阈值：按默认阈值预览（首跑、非重放、无生成器）
    let view = check_confirm_view(plan, ctx, program);
    explain_text_with(plan, ctx, program, loaded, Some(&view))
}

/// `check --explain` 的确认预览：默认阈值，非重放，不带 `--gen-model`（`check` 没有这些参数）。
fn check_confirm_view(
    plan: &jpp::interp::Plan,
    ctx: &jpp::interp::PlanCtx,
    program: &Program,
) -> jpp_plan::explain::ConfirmView {
    confirm_view(
        false,
        ctx.judge_price,
        false,
        plan,
        program.budget.cost,
        ConfirmArgs {
            confirmed: false,
            threshold_usd: crate::options::CONFIRM_DEFAULT_USD,
            threshold_is_default: true,
        },
    )
}

/// 同 [`explain_text`]，另带这一趟的确认判定（`run` 用）。
pub fn explain_text_with(
    plan: &jpp::interp::Plan,
    ctx: &jpp::interp::PlanCtx,
    program: &Program,
    loaded: &jpp_syntax::loader::LoadedProgram,
    confirm: Option<&jpp_plan::explain::ConfirmView>,
) -> String {
    let locate = explain_locator(loaded);
    jpp_plan::explain_with(
        plan,
        &jpp_plan::Explain {
            ctx: Some(ctx),
            program: Some(program),
            locate: Some(&locate),
            confirm,
        },
    )
}

/// `--explain --json` 的对象：`check` 并入它那一个文档的 `explain` 键，`run` 并入报告的 `explain` 键。
/// 形状（主会话裁定三十二，第一版、不承诺稳定，随 `Plan` 结构走）：键名与 `Plan` 一一对应——`layers_est`、
/// `calls_est`、`cost_est`、`latency_est`、`unsure_bound` 五个估计，形状 `{"lo","hi"}` / `{"lo"}` /
/// `{"unknown": 原因}`；`per_site` 以站点号为键，每站点 `order`、`must`、`layer_lo`、`execs_hi`（照 `SitePlan`），
/// 另有派生的 `kind`、`at`、`class`、`calls_hi`，以及读数站点的 `downstream`（步 30，`SitePlan.downstream`）；`rejected`、`warnings` 照 `IrDiag` 的 serde。
/// 另有三个非 `Plan` 的输入键：`budget`（程序的预算）、`ctx`（规划语境，照 `PlanCtx`）与 `confirm`（确认阈值的判定，Z0236，形状同报告的 `confirm` 键）。
/// 预算结论就是 `rejected` 有没有值，不另加；「未估」就是估计里的 `unknown`、`{"lo"}` 与 `per_site` 里的 `null`，不另出数组。
/// `Plan` 目前没有 `unsure_bound` 字段，按 `{"unknown": "not_computed"}` 出。
pub fn explain_json(
    plan: &jpp::interp::Plan,
    ctx: &jpp::interp::PlanCtx,
    program: &Program,
    loaded: &jpp_syntax::loader::LoadedProgram,
) -> Value {
    let view = check_confirm_view(plan, ctx, program);
    explain_json_with(plan, ctx, program, loaded, Some(&view))
}

/// 同 [`explain_json`]，另带这一趟的确认判定（`run` 用）：多一个 `confirm` 键，形状同报告的 `confirm` 键。
pub fn explain_json_with(
    plan: &jpp::interp::Plan,
    ctx: &jpp::interp::PlanCtx,
    program: &Program,
    loaded: &jpp_syntax::loader::LoadedProgram,
    confirm: Option<&jpp_plan::explain::ConfirmView>,
) -> Value {
    let locate = explain_locator(loaded);
    let b = &program.budget;
    let per_site: serde_json::Map<String, Value> =
        jpp_plan::explain::site_rows(plan, Some(program))
            .iter()
            .map(|r| {
                let place = r.span.and_then(&locate).map(|l| l.place);
                let reading = r.class.map(jpp_plan::explain::is_reading);
                (
                    r.site.0.to_string(),
                    json!({
                        "order": r.plan.order,
                        // 只有读数站点记必经与层；其余站点为 null（没记，不是非必经）
                        "must": reading.filter(|x| *x).map(|_| r.plan.must),
                        "layer_lo": r.plan.layer_lo,
                        "execs_hi": r.plan.execs_hi,
                        // 步 30 / B0488：下游层数（`SitePlan.downstream`，层内挑选的关键路径依据；裁定三十二「Plan 加字段 JSON 就加字段」）
                        "downstream": reading.filter(|x| *x).map(|_| r.plan.downstream),
                        "kind": r.kind,
                        "at": place,
                        "class": r.class.map(jpp_plan::explain::class_name),
                        "calls_hi": r.calls_hi,
                    }),
                )
            })
            .collect();
    let price = match ctx.judge_price {
        jpp::interp::JudgePrice::Known(p) => json!({"known": p}),
        jpp::interp::JudgePrice::Untested => json!("untested"),
        jpp::interp::JudgePrice::NotGiven => json!("not_given"),
    };
    let mut doc = json!({
        "layers_est": est_plain_json(&plan.layers_est),
        "calls_est": est_plain_json(&plan.calls_est),
        "cost_est": est_plain_json(&plan.cost_est),
        "latency_est": est_plain_json(&plan.latency_est),
        "unsure_bound": {"unknown": "not_computed"},
        "per_site": per_site,
        // 步 30 / B0488：逐组费用模型（画像单价与 token 回归，任一未测为 null）
        "cost_model": plan.cost_model.map(|m| json!({"price": m.price, "intercept_tokens": m.intercept_tokens,
            "state_char_coef": m.state_char_coef, "question_char_coef": m.question_char_coef})),
        "rejected": plan.rejected,
        "warnings": plan.warnings,
        "budget": {"calls": b.calls, "cost": b.cost, "latency_p95": b.latency_p95,
                   "escalate": b.escalate, "unsure": b.unsure},
        "ctx": {"ledger_empty": ctx.ledger_empty, "cache_off": ctx.cache_off, "judge_price": price},
    });
    if let Some(v) = confirm {
        doc["confirm"] = confirm_json(v);
    }
    doc
}

/// 报告 `action_facts` 要列哪些动作：`None` = 程序没有 `do` 站点，不出这一节；`Some(None)` = 有动作名
/// 非字面量的站点，列全表；`Some(Some(名字们))` = 只列字面量用到的动作。
fn action_names_used(program: &Program) -> Option<Option<Vec<String>>> {
    let sites = jpp::check::do_sites(program);
    if sites.is_empty() {
        return None;
    }
    if sites.iter().any(|s| s.is_none()) {
        return Some(None);
    }
    Some(Some(sites.into_iter().flatten().collect()))
}

/// 不带缓存与版本的简写；只有 `run_io` 的测试用（生产路径都走 [`execute_with`]，重放也要带版本，步 27）。
#[cfg(test)]
pub fn execute(
    program: &Program,
    ports: jpp_effects::Ports<'_>,
    calibrations: &CalibStore,
    ledger: &mut dyn LedgerPort,
    replay_only: bool,
    evidence_out: &mut Vec<(String, jpp::effects::Sample)>,
    entry: &jpp::EntryArgs,
) -> Result<Value, jpp::Error> {
    execute_with(
        program,
        ports,
        calibrations,
        ledger,
        replay_only,
        evidence_out,
        entry,
        CacheArgs::default(),
        None,
    )
}

/// 同 [`execute`]，另可带跨运行缓存与生成器身份（步 19）。
#[allow(clippy::too_many_arguments)]
pub fn execute_with(
    program: &Program,
    // 端口表（步 15b、15c）：固定观察、真机或重放
    ports: jpp_effects::Ports<'_>,
    calibrations: &CalibStore,
    ledger: &mut dyn LedgerPort,
    replay_only: bool,
    // 越界接线：把这一趟的证据带出去，供 `--calib-out` 折进记录。
    // **J-03 决定了程序永远写不了线**，所以「跑程序 → 积累证据 → 认证 → 用上」这条环
    // **只能靠宿主/CLI 闭合**——而这是出料那一半。
    evidence_out: &mut Vec<(String, jpp::effects::Sample)>,
    // 宿主入口（B105；步 14b-0 起 `--input` 产一条值条目）；空入口与不设相同，逐字节不变
    entry: &jpp::EntryArgs,
    cache: CacheArgs<'_>,
    // 程序文件所在目录（现场稳定性三修 (2)）：`read_json` 的相对路径先按它找
    program_dir: Option<&std::path::Path>,
) -> Result<Value, jpp::Error> {
    let ctx = jpp::actions::Ctx {
        program_dir: program_dir.map(std::path::Path::to_path_buf),
        envs: std::rc::Rc::new(cache.envs.iter().cloned().collect()),
        ..Default::default()
    };
    let mut actions = ActionRegistry::new();
    jpp::actions::register_all(&mut actions, &ctx, replay_only);
    // 重放是审计重现（B35）：账本记过的调用照记录计预算，缺记录即 E-replay；续跑与首跑走 run
    let has_gen = cache.gen_model.is_some();
    let mut session = jpp::Session::new(ports, calibrations, &actions)
        .with_gen(cache.gen_model, cache.gen_profile_hash)
        .with_versions(cache.lib_version, cache.bank_version);
    let cache_off = cache.cache.is_none();
    if let Some(c) = cache.cache {
        session = session.with_cache(c);
    }
    session = session.with_judge_price(cache.judge_price);
    if let Some(m) = cache.companions {
        session = session.with_companions(m);
    }
    if let Some(d) = &cache.mat_store
        && !replay_only
    {
        session = session.with_mat_store(std::rc::Rc::new(jpp::store::FileMatStore::open_dir(d)));
    }
    let cells_stats = cache.cells_stats;
    if let Some(c) = cache.cells {
        session = session.with_cells(c);
    }
    let span_ctx = cache.trace.clone();
    if let Some(t) = cache.trace {
        session = session.with_trace(t);
    }
    // C-3：上游余额（`--carry-in`）；重放不收（选项解析已拒 `--carry-in` 与 `--replay` 同给）
    let mut 进门: Option<jpp::BudgetCarry> = None;
    if let Some(p) = &cache.carry_in {
        let text = std::fs::read_to_string(p)
            .map_err(|e| carry_err(format!("--carry-in {} 读不了：{e}", p.display())))?;
        let rec: jpp::ledger::CarryRecord = serde_json::from_str(&text).map_err(|e| {
            carry_err(format!(
                "--carry-in {} 不是余额文件（要 {{calls, cost, latency_p95, escalate, hop, round, depth_cap}}）：{e}",
                p.display()
            ))
        })?;
        进门 = Some(jpp::BudgetCarry::from_record(rec));
        session = session
            .with_carry(进门.clone())
            .reauthorize_carry(cache.carry_reauthorize);
    }
    // C-3：报告 `plan` 段与 `--explain` 与运行时的计划同源（裁定三十二），按同一份生效预算 min(程序声明, 上游余额)
    // 规划；重放的余额取账本头。没有余额时就是原程序，不克隆
    // 续跑时按账本里这一轮的剩余（`TripCarry::for_trip`，与运行时同一个函数）
    let 生效 = jpp::TripCarry::for_trip(
        进门.as_ref(),
        ledger.view(),
        replay_only,
        cache.carry_reauthorize,
    )
    .map(|t| t.effective.tighten(&program.budget));
    let 换了预算;
    let 计划用 = match 生效 {
        Some(b) if b != program.budget => {
            换了预算 = Program {
                budget: b,
                ..program.clone()
            };
            &换了预算
        }
        _ => program,
    };
    // 运行前先取估计（Z0190）：账本此刻的状态就是 `Interp::run` 入口看到的状态。只规划一次：
    // 报告 `plan` 段与 `--explain`（文本在回调里打印，所以先于任何调用，也先于计划期拒绝的报错）从同一份 `Plan` 出
    let (mut planned, plan_ctx) =
        plan_of(计划用, calibrations, &*ledger, cache_off, cache.judge_price);
    // C-3 G2：上游收紧时 EXPLAIN 的拒绝行与 JSON `rejected` 的修法改指上游余额（与运行时报错同一句）
    if let Some(d) = planned.rejected.as_mut()
        && let Some(m) = jpp::rewrite_plan_rejection(&d.message, &program.budget, &计划用.budget)
    {
        d.message = m;
    }
    let plan = plan_section(&planned);
    // 费用确认（Z0236）：判定只算这一次；EXPLAIN 先打印（拒绝前先看到为什么），再拒
    let confirm = cache.confirm.map(|a| {
        let mut v = confirm_view(
            replay_only,
            cache.judge_price,
            has_gen,
            &planned,
            计划用.budget.cost,
            a,
        );
        // C-3 Z0384：上界取的是预算、而生效预算的 cost 被上游余额压得比程序声明更低时，来源写明是余额
        if v.upper_from == "budget.cost" && 计划用.budget.cost < program.budget.cost {
            v.upper_from = "carry";
        }
        v
    });
    let explain_doc = cache
        .explain
        .and_then(|f| f(&planned, &plan_ctx, 计划用, confirm.as_ref()));
    // 计划已被拒（`E-budget-plan`，只在首跑口径下有）：注定跑不了的运行不该先要人确认花钱，
    // 让会话先报计划拒绝；判定照常显示在 EXPLAIN 里
    if planned.rejected.is_none()
        && let Some(v) = &confirm
        && v.verdict == jpp_plan::explain::ConfirmVerdict::Over
        && !v.confirmed
    {
        return Err(jpp::Error::Runtime(jpp::RtError::new(
            Some("E-confirm-required"),
            refusal_message(v),
            program.span,
        )));
    }
    let outcome = if replay_only {
        session.replay(program, entry, ledger)
    } else {
        // 首跑与续跑同一路径：`ledger` 为空即首跑，为上一趟账本即续跑
        session.resume(program, entry, ledger)
    };
    // C-3（主控第二轮答复第 1 条）：失败的一轮也从账本算出交回余额写 `--carry-out`，绝不因失败放宽
    let outcome = match outcome {
        Ok(o) => o,
        Err(e) => {
            if let (Some(_), Some(p)) = (&进门, &cache.carry_out)
                && let Some(c) = jpp::BudgetCarry::handed_back(进门.as_ref(), ledger.view())
            {
                写余额(p, &c)?;
            }
            return Err(e);
        }
    };
    evidence_out.extend(outcome.evidence.iter().cloned());
    // J-10 的静态告警只有带校准记录的那次检查报得出来（在 `jpp::run` 里），
    // CLI 执行前那次检查没有记录、报不出它；这里打到 stderr，`--output` 时终端也看得见。
    for w in outcome
        .trace
        .warnings
        .iter()
        .filter(|w| w.starts_with("J-10"))
    {
        eprintln!("warning: {w}");
    }
    let mut report = json!({
        "mode": "fixed observations; no model API requests",
        // G2（`12` R9 单次形态）：有违规时这次结论为「未决（violation）」，值照带
        "status": if !outcome.violations.is_empty() { "violation" } else if outcome.pending.is_empty() { "returned" } else { "pending" },
        "value": outcome.value_json(),
        "pending": outcome.pending,
        "returned_unsure": outcome.returned_unsure,
        "cost": {"calls": outcome.cost.calls, "replayed": outcome.cost.replayed,
                 "tokens": outcome.cost.tokens, "usd": outcome.cost.usd, "asks": outcome.cost.asks},
        "plan": plan,
        "trace": outcome.trace,
        "local_checks": *ctx.checks.borrow(),
    });
    // C2c：单元图统计（只在 `--cells-stats` 时写；不给时报告逐字节不变）
    if cells_stats && let Some(c) = &outcome.cells {
        report["cells"] = json!({
            "judge_cells": c.判断单元, "ledger_cells": c.账本单元, "versions": c.发布版本, "state": c.最新状态,
            "code_cells": c.代码单元, "code_hits": c.代码命中, "code_computed": c.代码计算,
            "unkeyable": c.不可键, "impure": c.不纯,
        });
    }
    // Z0565：守卫下推迟、结算时才失败的不可逆 do（没有不写，报告逐字节不变；不算 J-12，退出码不变）
    if !outcome.settle_failed.is_empty() {
        report["settle_failed"] = json!(outcome.settle_failed);
    }
    // G2：违规逐笔进报告（没有违规不写，报告逐字节不变）；CLI 据此在 stderr 逐笔报并以退出码 3 结束
    if !outcome.violations.is_empty() {
        report["violations"] = json!(
            outcome
                .violations
                .iter()
                .map(|v| {
                    let mut j = v.to_json();
                    j["site_end"] = json!(v.site.end);
                    j["message"] = json!(v.message);
                    j
                })
                .collect::<Vec<_>>()
        );
    }
    // 费用确认（Z0236）：这一趟核过阈值才出现（固定观察、重放不出现，报告逐字节不变）。记录是信息，不是防御
    if let Some(v) = confirm.as_ref().filter(|v| is_checked(v)) {
        report["confirm"] = confirm_json(v);
    }
    // `--explain --json`：只在给时出现，不给时报告逐字节不变
    if let Some(v) = explain_doc {
        report["explain"] = v;
    }
    // C-7（Z0173）：程序里 `do` 用到的动作的可撤回性事实（三值、理由、成立条件）。只是如实的事实与记录，
    // 不影响放行；只在程序有 `do` 站点时出现（动作名非字面量的站点列全表），没有 `do` 的程序输出逐字节不变。
    // 事实随宿主变的动作另放 `host.action_facts`（见下）
    if let Some(names) = action_names_used(program) {
        // Z0901：事实随宿主的操作系统沙箱变的动作（执行器三个）放进 `host` 块，金样比较排除 `host`；
        // 没有用到这类动作的程序不出 `host`，报告逐字节不变
        let (plain, host) = jpp::actions::action_facts_split(names.as_deref());
        report["action_facts"] = plain;
        if host.as_object().is_some_and(|m| !m.is_empty()) {
            report["host"] =
                json!({"sandbox": jpp::actions::host_sandbox_kind(), "action_facts": host});
        }
    }
    // B162（步 25d）：同一判断被多个持有者带回时，按键列持有者；只在有时出现，默认输出逐字节不变
    if !outcome.duties.is_empty() {
        report["duties"] = json!(outcome.duties);
    }
    // J-05 默认链（B0492 S2）：进过默认链的出口逐条列出；只在有时出现，默认输出逐字节不变
    if !outcome.unsure_default.is_empty() {
        report["unsure_default"] = json!(outcome.unsure_default);
    }
    // 伴随题（B0492 S5）：「这道题怎样能更拿得准」，只在有时出现，默认输出逐字节不变
    if !outcome.improve.is_empty() {
        report["improve"] = json!(outcome.improve);
    }
    // 超窗次数（Z0918）：画像测过窗口时出现（各项可为 0），没测时不出，默认输出逐字节不变
    if outcome.site_key_fallback > 0 {
        report["site_key_fallback"] = json!(outcome.site_key_fallback);
    }
    if !outcome.named_unfetchable.is_null() {
        report["named_unfetchable"] = outcome.named_unfetchable.clone();
    }
    if !outcome.window_over.is_null() {
        report["window_over"] = outcome.window_over.clone();
    }
    // 停岗候选（B25）只在有时出现，默认输出逐字节不变
    if !outcome.suspend_candidates.is_empty() {
        report["suspend_candidates"] = json!(outcome.suspend_candidates);
    }
    // 逐出口记线等级（步 20f）：只在运行里有 `cut` 出口时出现。出口不进账本（`20` §3.7(1)），
    // 只凭账本重放时按账本 `CalibUsed` 条目（账本 v3；v2 在头行 `calib_used`）的记录重算出同一张表
    // B107、B120 (a)（步 20h-2）：本趟问过的题（题面、填法、精化题类；不带读数）
    if !outcome.questions.is_empty() {
        report["questions"] = json!(outcome.questions);
    }
    if !outcome.exits.is_empty() {
        report["exits"] = json!(outcome.exits);
    }
    // 预算停机（B93，步 22-0）：只在耗尽时出现，默认输出逐字节不变
    if let Some(b) = &outcome.budget {
        report["budget"] =
            json!({"exhausted": b.exhausted, "unsent": b.unsent, "first_site": b.first_site});
        // C-3、G4：上游深度已到上限（这一趟只停发）时才有原因，预算耗尽的报告逐字节不变
        if let Some(c) = &b.cause {
            report["budget"]["cause"] = json!(c);
        }
    }
    // C-3：交给下一轮的余额，只在有上游余额时出现（默认输出逐字节不变）；`--carry-out` 写同一份
    if let Some(c) = &outcome.carry {
        report["carry"] = json!(c.record());
        if let Some(p) = &cache.carry_out {
            写余额(p, c)?;
        }
    }
    // 层内挑选的决定（步 30 / B0488，主控 Q7/Q9）：只在本趟有过层内挑选时出现，默认输出逐字节不变。
    // 作者据此看到哪几组被推迟、为什么（类别、下游层数、各校准键本趟计数）；不进账本，审计重放不重算
    // Z0425：order 并档容差与来源（只有用了 order 的程序才有，不给其余报告加空字段）
    if !outcome.orders.is_empty() {
        report["orders"] = json!(outcome.orders);
    }
    if !outcome.selections.is_empty() {
        report["selections"] = json!(outcome.selections);
    }
    // 按缓存键复用（步 19，B40、B151）：给了 `--cache` 或本趟有命中时出现，没用缓存的运行输出逐字节不变
    if let Some(c) = &outcome.cache {
        report["cache"] = json!(c);
    }
    // C-2：宿主给了追踪上下文才出现（默认的报告逐字节不变；账本里每条条目的 `trace` 一直在，与报告无关）
    // 实际用的段以账本为准：续跑时宿主给的上下文只用来核对追踪编号，本趟落在账本里的新段上
    if let (true, Some(t)) = (span_ctx.is_some(), ledger.view().current_trace()) {
        report["span"] = json!({
            "trace": t.trace, "span": t.span, "parent": t.parent, "traceparent": t.to_traceparent()
        });
    }
    // 宿主入口段（步 14b-1，B108）：直接复用 `Program.entry.params` 的既有序列化（`EntryParam`
    // 的 name/kind/taint），无入口为空数组；`purpose`（若给）已经是 params[0]（B58/17b）。
    report["entry"] = json!(program.entry.params);
    // 宿主接受声明（B128，步 20j-2）：只在任一接受位为真时出现，不带开关的报告逐字节不变
    if entry.accept.any() {
        report["accept"] = json!({"declared_lines": entry.accept.declared_lines});
    }
    // 宿主开启放行把关（意图汇编 11a，`--guard`）：只在开时出现，默认的报告逐字节不变
    if program.entry.guard {
        report["guard"] = json!(true);
    }
    Ok(report)
}

/// 费用确认判定的纯函数测试（Z0236）。`cli` 是二进制 crate，集成测试够不到内部函数；
/// `--gen-model` 需要 feature `live`，默认构建的二进制走不到，也只能在这里测。
#[cfg(test)]
mod confirm_tests {
    use super::*;
    use jpp::interp::{Estimate, JudgePrice, Plan};
    use jpp_plan::explain::ConfirmVerdict as V;

    fn args(threshold: f64) -> ConfirmArgs {
        ConfirmArgs {
            confirmed: false,
            threshold_usd: threshold,
            threshold_is_default: true,
        }
    }

    fn plan_with_hi(hi: Option<f64>) -> Plan {
        let mut p = Plan::empty();
        p.cost_est = match hi {
            Some(h) => Estimate::Known { lo: 0.0, hi: h },
            None => Estimate::AtLeast { lo: 0.0 },
        };
        p
    }

    fn view(
        replay: bool,
        price: JudgePrice,
        gen_model: bool,
        hi: Option<f64>,
        cost: f64,
        threshold: f64,
    ) -> jpp_plan::explain::ConfirmView {
        confirm_view(
            replay,
            price,
            gen_model,
            &plan_with_hi(hi),
            cost,
            args(threshold),
        )
    }

    const PRICED: JudgePrice = JudgePrice::Known(2.0e-6);

    #[test]
    fn replay_is_always_exempt_even_with_no_price_and_a_generator() {
        for price in [JudgePrice::NotGiven, JudgePrice::Untested, PRICED] {
            let v = view(true, price, true, None, 1.0, 0.1);
            assert_eq!(v.verdict, V::ExemptReplay);
            assert_eq!(v.upper_usd, None);
        }
    }

    #[test]
    fn free_judge_and_no_generator_is_exempt() {
        let v = view(false, JudgePrice::Known(0.0), false, None, 999.0, 0.1);
        assert_eq!(v.verdict, V::ExemptNoPaidPort);
    }

    #[test]
    fn free_judge_but_generator_given_is_checked() {
        // 堵漏洞：判断器单价 0（固定观察或零价画像）加 --gen-model，生成器真在花钱
        let v = view(false, JudgePrice::Known(0.0), true, None, 1.0, 0.1);
        assert_eq!(v.verdict, V::Over);
        assert_eq!(v.upper_from, "budget.cost");
    }

    #[test]
    fn untested_price_without_generator_is_unchecked_not_refused() {
        for price in [JudgePrice::Untested, JudgePrice::NotGiven] {
            let v = view(false, price, false, None, 999.0, 0.1);
            assert_eq!(v.verdict, V::Unchecked);
            assert_eq!(v.upper_usd, None);
        }
    }

    #[test]
    fn untested_price_with_generator_is_checked() {
        let v = view(false, JudgePrice::Untested, true, None, 1.0, 0.1);
        assert_eq!(v.verdict, V::Over);
    }

    #[test]
    fn upper_bound_is_the_smaller_of_plan_hi_and_budget_cost() {
        let v = view(false, PRICED, false, Some(0.02), 1.0, 0.1);
        assert_eq!((v.upper_usd, v.upper_from), (Some(0.02), "plan"));
        assert_eq!(v.verdict, V::Within);
        let v = view(false, PRICED, false, Some(5.0), 1.0, 0.1);
        assert_eq!((v.upper_usd, v.upper_from), (Some(1.0), "budget.cost"));
        assert_eq!(v.verdict, V::Over);
        let v = view(false, PRICED, false, None, 1.0, 0.1);
        assert_eq!((v.upper_usd, v.upper_from), (Some(1.0), "budget.cost"));
    }

    #[test]
    fn over_means_strictly_greater_than_the_threshold() {
        assert_eq!(
            view(false, PRICED, false, None, 0.5, 0.5).verdict,
            V::Within
        );
        assert_eq!(view(false, PRICED, false, None, 0.75, 0.5).verdict, V::Over);
        // 阈值 0：任何有花费的上界都要确认，0 花费不算
        assert_eq!(
            view(false, PRICED, false, None, 0.0, 0.0).verdict,
            V::Within
        );
        assert_eq!(view(false, PRICED, false, None, 0.25, 0.0).verdict, V::Over);
    }

    #[test]
    fn a_program_that_provably_costs_nothing_is_within_whatever_it_declares() {
        // 没有读数站点：计划费用是 Known{0,0}，取小得 0，来源 plan
        let v = view(false, PRICED, false, Some(0.0), 999.0, 0.1);
        assert_eq!((v.verdict, v.upper_from), (V::Within, "plan"));
    }

    #[test]
    fn confirmed_flag_is_carried_through() {
        let a = ConfirmArgs {
            confirmed: true,
            ..args(0.1)
        };
        let v = confirm_view(false, PRICED, false, &plan_with_hi(None), 1.0, a);
        assert!(v.confirmed);
        assert_eq!(v.verdict, V::Over);
    }
}
