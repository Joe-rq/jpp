//! J++ 共同内核：程序表示、值与环境、静态检查、解释执行、效应适配、账本与重放。
//!
//! 唯一入口是 [`run`]：先静态检查（[`check::check`]），无错再解释执行。前端负责 `.jpp` → [`ast::Program`]
//! 的解析与 lower，不执行程序；CLI 负责文件、固定观察表与报告。接口说明见 `INTERFACE.md`。
//!
//! ```ignore
//! use jpp_core::{run, effects::{FixedClient, CalibStore}, interp::ActionRegistry, ledger::Ledger};
//! let mut client = FixedClient::new();
//! let mut ledger = Ledger::new();
//! let outcome = run(&program, &mut client, &CalibStore::new(), &ActionRegistry::new(), &mut ledger)?;
//! ```

pub mod ast;
pub mod check;
pub mod effects;
pub mod interp;
pub mod ledger;
pub mod value;

pub use ast::{Block, Budget, Expr, ExprKind, Function, Parameter, Program, Span, Statement, Type};
pub use check::{Diagnostic, Report, Severity, check};
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
    let report = check(program);
    if !report.is_ok() {
        return Err(Error::Check(report));
    }
    let budget = program.budget.clone().expect("检查器保证预算存在（E12）");
    Interp::new(client, ledger, calib, actions, budget).run(program).map_err(Error::Runtime)
}

/// 跳过静态检查直接执行——只给检查器本身的对照测试用；正常路径请用 [`run`]。
pub fn run_unchecked(
    program: &Program,
    client: &mut dyn Client,
    calib: &CalibStore,
    actions: &ActionRegistry,
    ledger: &mut Ledger,
) -> Result<Outcome, RtError> {
    let budget = program.budget.clone().unwrap_or(Budget { calls: 0, cost: 0.0, depth: None, escalate: None });
    Interp::new(client, ledger, calib, actions, budget).run(program)
}
