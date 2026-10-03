//! 手工构造表层 AST 的小建造器（步 12d 前构造核心语法树；核心语法树删除后改造表层 AST，再经
//! `jpp::lower` 降到 IR）。每个节点拿一个唯一的 `Span`——测试要断言诊断落在哪个节点上，
//! 账本里 `do` 的键也含 `span.start`，所以不能让它们都是 0。

#![allow(dead_code)]

use std::cell::Cell;

pub use jpp::syntax::ast::{Block, Expr, ExprKind, Function, Parameter, Span, Statement, Type};
use jpp::{Budget, Program};

thread_local! {
    static NEXT: Cell<usize> = const { Cell::new(0) };
}

/// 取一个此后不会重复的位置
pub fn sp() -> Span {
    NEXT.with(|n| {
        let v = n.get() + 2;
        n.set(v);
        Span {
            start: v,
            end: v + 1,
        }
    })
}

fn e(kind: ExprKind) -> Expr {
    Expr { kind, span: sp() }
}

pub fn int(v: i64) -> Expr {
    e(ExprKind::Integer(v))
}
pub fn dec(v: f64) -> Expr {
    e(ExprKind::Decimal(v))
}
pub fn boolean(v: bool) -> Expr {
    e(ExprKind::Bool(v))
}
pub fn text(s: &str) -> Expr {
    e(ExprKind::Text(s.to_string()))
}
pub fn name(n: &str) -> Expr {
    e(ExprKind::Name(n.to_string()))
}
pub fn unit() -> Expr {
    e(ExprKind::Unit)
}
pub fn list(items: Vec<Expr>) -> Expr {
    e(ExprKind::List(items))
}
pub fn rec(fields: Vec<(&str, Expr)>) -> Expr {
    e(ExprKind::Record(
        fields
            .into_iter()
            .map(|(k, v)| (k.to_string(), v))
            .collect(),
    ))
}
pub fn call(f: &str, args: Vec<Expr>) -> Expr {
    e(ExprKind::Call {
        function: Box::new(name(f)),
        arguments: args,
    })
}
pub fn call_of(f: Expr, args: Vec<Expr>) -> Expr {
    e(ExprKind::Call {
        function: Box::new(f),
        arguments: args,
    })
}
pub fn field(v: Expr, f: &str) -> Expr {
    e(ExprKind::Field {
        value: Box::new(v),
        field: f.to_string(),
    })
}
pub fn index(v: Expr, i: Expr) -> Expr {
    e(ExprKind::Index {
        value: Box::new(v),
        index: Box::new(i),
    })
}
pub fn bin(op: &str, l: Expr, r: Expr) -> Expr {
    e(ExprKind::Binary {
        op: op.to_string(),
        left: Box::new(l),
        right: Box::new(r),
    })
}
pub fn not(v: Expr) -> Expr {
    e(ExprKind::Unary {
        op: "!".to_string(),
        value: Box::new(v),
    })
}
pub fn if_(c: Expr, yes: Expr, no: Expr) -> Expr {
    e(ExprKind::If {
        condition: Box::new(c),
        yes: body(vec![], yes),
        no: body(vec![], no),
    })
}

pub fn body(statements: Vec<Statement>, result: Expr) -> Block {
    Block {
        statements,
        result: Some(Box::new(result)),
        span: sp(),
    }
}

/// 表达式语句（求值后丢掉结果）
pub fn discard(e: Expr) -> Statement {
    Statement::Expression(e)
}

pub fn bind(n: &str, v: Expr) -> Statement {
    Statement::Let {
        name: n.to_string(),
        annotation: None,
        value: v,
        span: sp(),
    }
}

fn make(
    params: &[&str],
    effects: Option<&[&str]>,
    result_type: Option<Type>,
    b: Block,
) -> Function {
    Function {
        parameters: params
            .iter()
            .map(|p| Parameter {
                name: p.to_string(),
                annotation: None,
                span: sp(),
            })
            .collect(),
        result_type,
        effects: effects.map(|e| e.iter().map(|x| x.to_string()).collect()),
        body: b,
    }
}

/// 匿名方法值（可传入、可返回、可再组合）
pub fn lambda(params: &[&str], b: Block) -> Expr {
    e(ExprKind::Function(make(params, None, None, b)))
}
pub fn lambda_eff(params: &[&str], effects: &[&str], b: Block) -> Expr {
    e(ExprKind::Function(make(params, Some(effects), None, b)))
}
/// 具名方法
pub fn func(n: &str, params: &[&str], b: Block) -> Statement {
    Statement::Function {
        name: n.to_string(),
        function: make(params, None, None, b),
        span: sp(),
    }
}
pub fn func_eff(n: &str, params: &[&str], effects: &[&str], b: Block) -> Statement {
    Statement::Function {
        name: n.to_string(),
        function: make(params, Some(effects), None, b),
        span: sp(),
    }
}
/// 带返回类型标注（J-05：标注含 Exit 才许把出口带出）
pub fn func_ret(
    n: &str,
    params: &[&str],
    effects: Option<&[&str]>,
    ret: Type,
    b: Block,
) -> Statement {
    Statement::Function {
        name: n.to_string(),
        function: make(params, effects, Some(ret), b),
        span: sp(),
    }
}

pub fn budget(calls: u64, depth: u32) -> Budget {
    Budget {
        calls,
        cost: 0.0,
        depth: Some(depth),
        escalate: None,
        unsure: None,
        absent: None,
        latency_p95: None,
    }
}

/// 预算写回表层的 `budget {…}` 记录（降级再把它解析回来）。预算不进程序体，节点都用零位置，
/// 不占 [`sp`] 的序号：程序体各节点的位置与步 12d 前逐一相同。
fn budget_expr(b: &Budget) -> Expr {
    let z = |kind| Expr {
        kind,
        span: Span { start: 0, end: 0 },
    };
    let int = |v: i64| z(ExprKind::Integer(v));
    let num = |v: f64| {
        if v.fract() == 0.0 && v.abs() < 1e15 {
            z(ExprKind::Integer(v as i64))
        } else {
            z(ExprKind::Decimal(v))
        }
    };
    let rec = |fs: Vec<(&str, Expr)>| {
        z(ExprKind::Record(
            fs.into_iter().map(|(k, v)| (k.to_string(), v)).collect(),
        ))
    };
    let mut fs = vec![("calls", int(b.calls as i64)), ("cost", num(b.cost))];
    if let Some(d) = b.depth {
        fs.push(("depth", int(d as i64)));
    }
    if let Some(x) = b.escalate {
        fs.push(("escalate", int(x as i64)));
    }
    if let Some(u) = b.unsure {
        fs.push(("unsure", num(u)));
    }
    if let Some(l) = b.latency_p95 {
        fs.push(("latency_p95", num(l)));
    }
    if let Some(a) = &b.absent {
        fs.push((
            "absent",
            rec(vec![
                ("retry", int(a.retry as i64)),
                ("backoff", num(a.backoff)),
                ("then", z(ExprKind::Text(a.then.clone()))),
                ("breaker", int(a.breaker as i64)),
            ]),
        ));
    }
    rec(fs)
}

/// 表层程序（不降级）：缺预算的用例要看降级报的 `J-07a`
pub fn source(
    b: Option<Budget>,
    statements: Vec<Statement>,
    result: Expr,
) -> jpp::syntax::ast::Program {
    jpp::syntax::ast::Program {
        budget: b.as_ref().map(budget_expr),
        body: body(statements, result),
        lib_ranges: vec![],
    }
}

/// 建好并降到 IR 的程序
pub fn program(b: Option<Budget>, statements: Vec<Statement>, result: Expr) -> Program {
    let src = source(b, statements, result);
    let mut p = jpp::lower(&src).unwrap_or_else(|ds| panic!("降级失败：{ds:?}"));
    // 整个程序的位置单取一个（与步 12d 前的建造器同序：先程序体，后程序）
    let at = sp();
    p.span = jpp::Span {
        start: at.start,
        end: at.end,
    };
    p
}

/// 把某个具名方法的效应标注换掉，用来验「错标注拦不拦得住」。返回改过的程序与那个方法的 Span。
/// 直接改 IR 的效应标注（只给检查器看；函数的结构哈希 `source_hash` 不随之重算）。
pub fn with_effects(p: &Program, name: &str, effects: &[&str]) -> (Program, jpp::Span) {
    let mut out = p.clone();
    let mut at = None;
    for s in &mut out.body.statements {
        if let jpp::Stmt::Function {
            name: n,
            function,
            span,
        } = s
            && n == name
        {
            function.effects = Some(effects.iter().map(|e| e.to_string()).collect());
            at = Some(*span);
        }
    }
    (
        out,
        at.unwrap_or_else(|| panic!("程序里没有具名方法 {name}")),
    )
}

/// 断言静态检查一条诊断都没有；有就把它们打出来（报文本身是交付物的一部分）
pub fn assert_clean(p: &Program) {
    let report = jpp::check(p);
    assert!(
        report.diagnostics.is_empty(),
        "静态检查本不该有话说，却报了：\n{}",
        report.render()
    );
}

/// **测试用的认证线**（B29 之后）：`put` 只写夹具记录，夹具线的出口不算放行不可逆 `do`
/// 的可信合取项。测 J-08 放行路径的测试需要一条「有证书」的线，这里附一张**明写为测试合成**
/// 的证书并清掉夹具位。它只存在于测试建造器里，语言与 CLI 没有这条路。
pub fn certified(calib: &mut jpp::effects::CalibStore, key: &str, hi: f64, lo: f64, n: u64) {
    calib.put(key, hi, lo, n, "上岗", Some(0.05)).unwrap();
    let cert = jpp::effects::Cert {
        alpha: 0.10,
        conf_delta: 0.10,
        hi,
        n_accepted: n as usize,
        n_errors: 0,
        ucb: 0.05,
        cluster_unit: "测试合成证书".into(),
        resample: None,
        cost: None,
        bounded_side: "单侧".into(),
        label_fp: String::new(),
        selection: None,
        grade: Default::default(),
        eff: None,
        label_source: jpp::effects::LabelSource::全体,
    };
    let r = calib.records.get_mut(key).unwrap();
    r.certs.insert(cert.addr(), cert);
    r.fixture = false;
    // B104-2（步 20h-1）：无指纹 = 范围未知 = 不放行。测试夹具的「正式线」给一个包含一切材料的范围
    r.scope = Some(jpp::effects::CalibScope {
        batches: vec!["测试合成".into()],
        sources: Default::default(),
        note: "测试夹具：覆盖一切材料的范围".into(),
        fingerprint: Some(jpp::conformal::ScopeRanges {
            quantiles: (0.0, 1.0),
            n: n as usize,
            ranges: jpp::conformal::FP_NAMES
                .iter()
                .map(|x| (x.to_string(), -1e12, 1e12))
                .collect(),
            margins: None,
        }),
        n_text: None,
        extensions: vec![],
    });
}

/// 伴随题开关（B0492 S5，主控 2026-09-30 路 3）：与伴随题无关的测试集中在这里显式关掉；
/// `JPP_TEST_COMPANIONS=on` 时整体开着跑（语言、库与 CLI 的默认都是开）
pub fn 伴随() -> jpp::interp::CompanionMode {
    if std::env::var("JPP_TEST_COMPANIONS").is_ok_and(|v| v == "on") {
        jpp::interp::CompanionMode::Same
    } else {
        jpp::interp::CompanionMode::Off
    }
}

/// 单元图开关（C2b，步 41；H1a 对照臂）：`JPP_TEST_CELLS=off` 时整体关着跑，缺省开（与运行时缺省相同）
pub fn 单元图() -> bool {
    !std::env::var("JPP_TEST_CELLS").is_ok_and(|v| v == "off")
}

/// CLI 测试用：关时给 `--companions off`，开时不给（用 CLI 的默认）
pub fn 伴随参数() -> Vec<&'static str> {
    if 伴随() == jpp::interp::CompanionMode::Off {
        vec!["--companions", "off"]
    } else {
        vec![]
    }
}

/// `jpp::run` 的同形包装，带 [`伴随`]（与伴随题无关的测试用它，免得默认开的伴随题改变替身看到的题）
pub fn run(
    program: &jpp::ir::Program,
    ports: jpp::effects::Ports<'_>,
    calib: &jpp::effects::CalibStore,
    actions: &jpp::ActionRegistry,
    ledger: &mut dyn jpp::LedgerPort,
) -> Result<jpp::Outcome, jpp::Error> {
    jpp::Session::new(ports, calib, actions)
        .with_companions(伴随())
        .with_cells(单元图())
        .run(program, &jpp::EntryArgs::default(), ledger)
}

/// `jpp::run_replay` 的同形包装，带 [`伴随`]
pub fn run_replay(
    program: &jpp::ir::Program,
    ports: jpp::effects::Ports<'_>,
    calib: &jpp::effects::CalibStore,
    actions: &jpp::ActionRegistry,
    ledger: &mut dyn jpp::LedgerPort,
) -> Result<jpp::Outcome, jpp::Error> {
    jpp::Session::new(ports, calib, actions)
        .with_companions(伴随())
        .with_cells(单元图())
        .replay(program, &jpp::EntryArgs::default(), ledger)
}

/// 伴随题元题的中性读数约定（B0492 S5，主控 2026-09-30）：定义在 `jpp::testing`，命令行单元测试的 Jev 桩也引用那一处
pub use jpp::testing::伴随中性;

/// 判断替身的包装：伴随元题按 [`伴随中性`] 给中性读数，其余题交给 `f`（`f` 只看到非伴随题；全是伴随题时不调 `f`）
pub fn 伴随中性judge<'f>(
    model: &str,
    mut f: impl FnMut(
        &jpp::value::State,
        &[&jpp::value::Question],
    ) -> Result<jpp::effects::JudgeResult, jpp::effects::EffectError>
    + 'f,
) -> jpp::effects::FnPort<'f> {
    jpp::effects::FnPort::judge(model, move |s, qs| {
        let 其余: Vec<&jpp::value::Question> = qs
            .iter()
            .copied()
            .filter(|q| 伴随中性(q, s).is_none())
            .collect();
        let r = if 其余.is_empty() {
            jpp::effects::JudgeResult {
                answers: vec![],
                tokens: 0,
                cost: 0.0,
                mode_share: vec![],
                perms: vec![],
                confidence: vec![],
            }
        } else {
            f(s, &其余)?
        };
        let (mut ai, n) = (0usize, 其余.len());
        let mut out = jpp::effects::JudgeResult {
            answers: vec![],
            tokens: r.tokens,
            cost: r.cost,
            mode_share: vec![],
            perms: vec![],
            confidence: vec![],
        };
        for q in qs {
            match 伴随中性(q, s) {
                Some(a) => {
                    out.answers.push(a);
                    if r.mode_share.len() == n && n > 0 {
                        out.mode_share.push(None);
                    }
                    if r.perms.len() == n && n > 0 {
                        out.perms.push(0);
                    }
                    if r.confidence.len() == n && n > 0 {
                        out.confidence.push(None);
                    }
                }
                None => {
                    out.answers.push(r.answers[ai].clone());
                    if r.mode_share.len() == n && n > 0 {
                        out.mode_share.push(r.mode_share[ai]);
                    }
                    if r.perms.len() == n && n > 0 {
                        out.perms.push(r.perms[ai]);
                    }
                    if r.confidence.len() == n && n > 0 {
                        out.confidence.push(r.confidence[ai]);
                    }
                    ai += 1;
                }
            }
        }
        Ok(out)
    })
}

/// `run` 固定关伴随题：测试数的是题数、条目数，或测默认链的选路，伴随题会改变它们（主控 2026-09-30：第二、三类保持关）
pub fn run_关(
    program: &jpp::ir::Program,
    ports: jpp::effects::Ports<'_>,
    calib: &jpp::effects::CalibStore,
    actions: &jpp::ActionRegistry,
    ledger: &mut dyn jpp::LedgerPort,
) -> Result<jpp::Outcome, jpp::Error> {
    jpp::Session::new(ports, calib, actions)
        .with_companions(jpp::interp::CompanionMode::Off)
        .with_cells(单元图())
        .run(program, &jpp::EntryArgs::default(), ledger)
}

/// `run_replay` 固定关伴随题
pub fn run_replay_关(
    program: &jpp::ir::Program,
    ports: jpp::effects::Ports<'_>,
    calib: &jpp::effects::CalibStore,
    actions: &jpp::ActionRegistry,
    ledger: &mut dyn jpp::LedgerPort,
) -> Result<jpp::Outcome, jpp::Error> {
    jpp::Session::new(ports, calib, actions)
        .with_companions(jpp::interp::CompanionMode::Off)
        .with_cells(单元图())
        .replay(program, &jpp::EntryArgs::default(), ledger)
}
