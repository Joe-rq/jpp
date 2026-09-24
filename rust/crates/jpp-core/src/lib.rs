//! J++ 共同内核：程序表示、值与环境、静态检查、解释执行、效应适配、账本与重放。
//!
//! 唯一入口是 [`run`]：先静态检查（[`check::check`]），无错再解释执行。`.jpp` 经 [`syntax::parse`] 与 [`lower`]
//! 降到 IR（[`Program`]），降级不执行程序；CLI 负责文件、固定观察表与报告。接口说明见 `INTERFACE.md`。
//!
//! ```ignore
//! use jpp_core::{run, effects::{FixedClient, CalibStore}, interp::ActionRegistry, ledger::Ledger};
//! let mut client = FixedClient::new();
//! let mut ledger = Ledger::new();
//! let outcome = run(&program, &mut client, &CalibStore::new(), &ActionRegistry::new(), &mut ledger)?;
//! ```

pub use jpp_check as check;
pub use jpp_value::stat as conformal;
pub mod effects;
pub mod interp;
pub mod ledger;
pub mod names;
pub use jpp_calib::strength;
pub use jpp_calib::truth;
pub use jpp_value::value;

pub use jpp_ir::ir;
pub use jpp_ir::ir::{Block, Budget, Expr, Function, Parameter, Program, Span, Stmt, Type};
pub use check::{Diagnostic, Report, Severity, check, check_with_calib, check_with_profile};
pub use effects::{CalibRecord, CalibStore, Client, EffectError, FixedClient, JevClient, NoCallClient, obs_key};
pub use interp::{ActionRegistry, Cost, Interp, Outcome, RtError, TaintOut};
pub use ledger::{Entry, Header, Ledger, Trace, TraceEvent};
pub use value::{Answer, Env, Exit, ExitKind, Mat, Op, Pending, Question, Reading, State, Taint, Value};

/// 运行失败的两种成色：静态检查不过，或运行期出错。两者都带 `Span`，前端据此定位到 `.jpp`。
#[derive(Debug)]
pub enum Error {
    /// 静态检查有错；程序没有开始执行
    Check(Report),
    /// 运行期错（J-01/J-02/J-05 运行面、类型不符、客户端错误等）
    Runtime(RtError),
}

impl Error {
    pub fn render(&self) -> String {
        match self {
            Error::Check(r) => r.render(),
            Error::Runtime(e) => e.render(),
        }
    }
    /// 所有诊断位置（前端用来渲染源码行）
    pub fn spans(&self) -> Vec<Span> {
        match self {
            Error::Check(r) => r.errors().iter().map(|d| d.span).collect(),
            Error::Runtime(e) => vec![e.span],
        }
    }
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.render())
    }
}

impl std::error::Error for Error {}

/// 检查并执行一个程序。预算来自程序自己的 `budget`（E12 必填，由检查器把关）。
///
/// `ledger` 既是输出也是输入：把上一次运行的账本传进来即重放，命中的键零调用（配 [`NoCallClient`]
/// 可以验证「同程序重放零调用」）。`actions` 是 `do` 能触发的登记动作表；没登记的动作是 J-11 错。
pub fn run(
    program: &Program,
    client: &mut dyn Client,
    calib: &CalibStore,
    actions: &ActionRegistry,
    ledger: &mut Ledger,
) -> Result<Outcome, Error> {
    // **档案走到检查器**（`12` §1.2）。传 `check(program)` 会让降级规则永远够不着真实运行
    // ——那样管道就只在测试里通，而**一个只在测试里通的管道是构造，不是功能**。
    // **整本记录走到检查器**，不只是档案：J-10 的静态那一半要各题的 `unsure_rate`，
    // 而那住在 `CalibRecord` 里。**与 §1.2 那根「档案到不了检查器」的管道是同一种缺结构**，
    // 只是这次缺的是记录不是档案。
    let report = check_with_calib(program, calib);
    if !report.is_ok() {
        return Err(Error::Check(report));
    }
    let budget = program.budget.clone();
    let out = Interp::new(client, ledger, calib, actions, budget).run(program).map_err(Error::Runtime)?;
    Ok(带出静态告警(out, &report))
}

/// 审计重放（B35；21 步 3）：只凭账本重现首跑。与 [`run`] 相同，只是账本里记过的调用照记录计入预算，
/// 缺的记录报 `E-replay`（致命，不进 cause）。续跑用 [`run`]（已记录的不付费、继续往下）。
pub fn run_replay(
    program: &Program,
    client: &mut dyn Client,
    calib: &CalibStore,
    actions: &ActionRegistry,
    ledger: &mut Ledger,
) -> Result<Outcome, Error> {
    let report = check_with_calib(program, calib);
    if !report.is_ok() {
        return Err(Error::Check(report));
    }
    let budget = program.budget.clone();
    let out = Interp::new(client, ledger, calib, actions, budget).audit_replay().run(program).map_err(Error::Runtime)?;
    Ok(带出静态告警(out, &report))
}

/// **J-10 的静态告警要带到调用者手里。** 它只是告警，`report.is_ok()` 仍为真，
/// 这份报告若在这里丢掉，调用者就永远看不到「跑之前就知道 unsure 预算不够」这句话。
/// 只带 J-10：别的静态告警 CLI 在执行前已经用 `check`/`check_with_profile` 打过，
/// 而只有要整本校准记录的 J-10 必须走这一处（`check_with_calib`）。
/// 放在 `trace.warnings` 最前面，表示它们先于任何调用成立。运行期出错时这份告警不随 `RtError` 带出。
fn 带出静态告警(mut out: Outcome, report: &Report) -> Outcome {
    let 静态: Vec<String> = report
        .warnings()
        .iter()
        .filter(|d| d.rule == "J-10")
        .map(|d| format!("{}: @{} {}", d.rule, d.span.start, d.message))
        .collect();
    if !静态.is_empty() {
        out.trace.warnings.splice(0..0, 静态);
    }
    out
}

/// 带 `fit` 注册表的入口（`12` §6.0 的 fit 桥要用它）。
/// `run` 是它 fit 表为空的特例——绝大多数程序不用 fit。
pub fn run_with_fits(
    program: &Program,
    client: &mut dyn Client,
    calib: &CalibStore,
    actions: &ActionRegistry,
    fits: &effects::FitRegistry,
    ledger: &mut Ledger,
) -> Result<Outcome, Error> {
    // **档案走到检查器**（`12` §1.2）。传 `check(program)` 会让降级规则永远够不着真实运行
    // ——那样管道就只在测试里通，而**一个只在测试里通的管道是构造，不是功能**。
    let report = check_with_calib(program, calib);
    if !report.is_ok() {
        return Err(Error::Check(report));
    }
    let budget = program.budget.clone();
    let out = interp::Interp::with_fits(client, ledger, calib, actions, fits, budget).run(program).map_err(Error::Runtime)?;
    Ok(带出静态告警(out, &report))
}

/// 跳过静态检查直接执行——只给检查器本身的对照测试用；正常路径请用 [`run`]。
pub fn run_unchecked(
    program: &Program,
    client: &mut dyn Client,
    calib: &CalibStore,
    actions: &ActionRegistry,
    ledger: &mut Ledger,
) -> Result<Outcome, RtError> {
    let budget = program.budget.clone();
    Interp::new(client, ledger, calib, actions, budget).run(program)
}

pub use jpp_syntax as syntax;

/// 把表层程序降到 IR（步 12d）：`jpp_syntax::lower` 配上外观层组装的名字表（[`names::CurrentNames`]）。
/// 缺预算、预算块写错在这里报（`J-07a`）。
pub fn lower(p: &syntax::ast::Program) -> Result<Program, Vec<syntax::Diagnostic>> {
    syntax::lower(p, &names::CurrentNames)
}
