mod bank;
mod calib_confirm;
mod calib_import;
mod calib_keys_out;
mod derive_admit;
mod diag_json;
mod fixture;
mod options;
mod profile_check;
mod profile_resolve;
mod questions_out;
mod run_io;
mod runner;

use options::{Command, help};
use std::{env, process::ExitCode};

fn execute(
    command: Command,
    questions_out: Option<std::path::PathBuf>,
    explain: bool,
    入口开关: options::EntryFlags,
) -> Result<(), String> {
    let path = match &command {
        Command::Help => {
            println!("{}", help());
            return Ok(());
        }
        Command::CalibImport(a) => return calib_import::run(a),
        Command::CalibConfirm { dir, key, suspend } => {
            return calib_confirm::run(dir, key, *suspend);
        }
        Command::LedgerMigrate { from, to } => return run_io::ledger_migrate(from, to),
        Command::LedgerTree { ledgers } => return run_io::ledger_tree(ledgers),
        Command::Bank { stats, args } => return bank::run(*stats, args),
        Command::DeriveAdmit { args } => return derive_admit::run(args),
        Command::ProfileCheck { profile } => return profile_check::run(profile),
        Command::Parse { source, .. } | Command::Check { source, .. } => source,
        Command::Run(options) => &options.source,
    };
    let filename = path.to_string_lossy();
    let loaded = jpp_syntax::loader::load(path)?;
    let parsed = &loaded.program;
    if let Command::Parse { ast, .. } = &command {
        if *ast {
            println!("{parsed:#?}");
        } else {
            println!(
                "Parsed {filename}: {} statements and {} result expression",
                parsed.body.statements.len(),
                usize::from(parsed.body.result.is_some())
            );
        }
        return Ok(());
    }
    // 宿主入口声明（B106）：给了 `--input` 就声明一条名为 `input` 的值条目，交给 `compile`
    // 写进 `Program.entry`。只看有没有给、不读文件，保持「降级诊断先于读输入」的报错顺序。
    // taint 按 `--input-trusted` 给（步 14b-1，B108）：静态 J-08（步 24-0，`Session::go` 执行前
    // 复检）只读 `Program.entry.params[].taint`，不声明可信这里就放行不了不可逆 `do`——
    // 必须与下面读文件时建的运行期 `输入` 用同一个 taint，否则 CLI 会先撞见静态报文。
    let 给了输入 = match &command {
        Command::Run(r) => r.input.is_some(),
        Command::Check { input, .. } => input.is_some(),
        _ => false,
    };
    let 输入可信 = match &command {
        Command::Run(r) => r.input_trusted,
        Command::Check { input_trusted, .. } => *input_trusted,
        _ => false,
    };
    // 步 20j-2（B128）：宿主接受作者声明线放行，与入口声明同路进 `Program.entry`（J-08 静态子面读它）
    let 接受 = jpp::HostAccept {
        declared_lines: match &command {
            Command::Run(r) => r.release_on_declared,
            Command::Check {
                release_on_declared,
                ..
            } => *release_on_declared,
            _ => false,
        },
    };
    // 意图汇编 11a：宿主开启放行把关，与接受位同路进 `Program.entry`（检查器与运行时都只读那一位）
    let guard = match &command {
        Command::Run(r) => r.guard,
        Command::Check { guard, .. } => *guard,
        _ => false,
    };
    // B0472：`--purpose` 与 `--mat` 同法只看给没给、不读文件；材料条目一律按不可信声明（CLI 不开可信开关）
    let 声明材料: Vec<jpp::EntryMat> = 入口开关
        .mats
        .iter()
        .map(|(名, _)| jpp::EntryMat::untrusted(名, serde_json::Value::Null))
        .collect();
    let 入口声明 = if 给了输入 {
        let taint = if 输入可信 {
            jpp::Taint::Trusted
        } else {
            jpp::Taint::Untrusted
        };
        jpp::EntryArgs {
            purpose: 入口开关.purpose.clone(),
            values: vec![jpp::EntryValue::new("input", serde_json::Value::Null).with_taint(taint)],
            materials: 声明材料,
            accept: 接受,
            guard,
        }
        .decl()
    } else {
        jpp::EntryArgs {
            purpose: 入口开关.purpose.clone(),
            materials: 声明材料,
            accept: 接受,
            guard,
            ..Default::default()
        }
        .decl()
    };
    // 降级诊断与检查诊断走同一渲染层（步 9a）：同码同址折叠，`--json` 时出机读格式
    let program = match jpp::Session::compile(parsed, &入口声明) {
        Ok(p) => p,
        Err(ds) => {
            let items: Vec<_> = ds.iter().map(diag_json::from_lower).collect();
            if diag_json::json_mode() && matches!(command, Command::Check { .. }) {
                print_check_doc(&filename, &loaded, items, None, None, None);
                return Err(String::new());
            }
            return Err(diag_json::render_all(&loaded, items).join("\n"));
        }
    };
    // 步 20a-2b（B116 (5)）：`check --questions-out` 在降级成功之后、静态检查之前写字面题导出——
    // 程序有检查错误也照写（导出只读语法与字面量），降级失败则走不到这里、不写。
    // PR #36 复核 P2：`--json` 模式下 stderr 必须为空（`print_check_doc` 头注的既有契约），
    // 这条摘要挪进 JSON 文档的 `questions_out` 字段；非 JSON 模式沿用原来的 `eprintln!`。
    let mut questions_out_summary: Option<QuestionsOutSummary> = None;
    if let Some(out) = &questions_out {
        let (标签, 题, 跳过) = questions_out::write(&program, &loaded, path, out)?;
        if diag_json::json_mode() {
            questions_out_summary = Some(QuestionsOutSummary {
                labels: 标签,
                questions: 题,
                skipped: 跳过,
                path: out.clone(),
            });
        } else {
            eprintln!(
                "题面导出（B116）：{标签} 个标签、{题} 道题、跳过 {跳过} 处 → {}",
                out.display()
            );
        }
    }
    // 画像只解析一次（B73）：静态检查、账本头 `profile_hash`、「档案：…」提示都用这一个结果。
    // 真机运行解析不到画像即报 `E-profile-missing`，在检查与初始化真机客户端之前。
    let 画像 = match &command {
        Command::Run(r) => profile_resolve::resolve(r)?,
        // L7（2026-09-28）：`check --profile`，依赖画像的检查（B32 时延静态面等）与 `run` 同口径
        Command::Check {
            profile: Some(p), ..
        } => Some(profile_resolve::load(p)?),
        _ => None,
    };
    // 宿主入口（步 14b-0 `--input`；B105 起为一条值条目）：检查前读文件，读不成在检查前报错；
    // taint 与上面的 `入口声明` 同一个 `输入可信`（步 14b-1）
    let 输入 = match &command {
        Command::Run(r) => r.input.as_deref(),
        Command::Check { input, .. } => input.as_deref(),
        _ => None,
    }
    .map(|p| run_io::read_host_input(p, 输入可信))
    .transpose()?
    .unwrap_or_default();
    // B0472：目的与材料条目（检查前读文件，读不成在检查前报错）
    let 材料 = 入口开关
        .mats
        .iter()
        .map(|(名, 文件)| run_io::read_entry_mat(名, 文件))
        .collect::<Result<Vec<_>, _>>()?;
    let 输入 = jpp::EntryArgs {
        purpose: 入口开关.purpose.clone(),
        materials: 材料,
        accept: 接受,
        guard,
        ..输入
    };
    // 步 24c（B108 已知限制收口）：`check` 与 `run` 共用这一次预检查，都带上 CLI 唯一注册的
    // 三个内置动作（与用户输入无关，随时能给）——J-08 静态子面从此能对 `record_check` 这类可逆
    // 动作不报、对 `write_json` 这类不可逆动作在检查期就报 error，不必等 `Session::go` 内部
    // 真正带表的那次检查。
    // L7（2026-09-28，K-084/K-160）：`check` 带上校准记录（`--calib`，不给时空记录本，与 `run` 不给时同口径），
    // J-10 静态面在这里就跑；`run` 仍走原来那次，J-10 由 `Session::go` 执行前带出，不报两遍
    let 校准 = match &command {
        Command::Check { calib, .. } => Some(match calib {
            Some(dir) => jpp::store::calib::open(dir)?,
            None => jpp::effects::CalibStore::new(),
        }),
        _ => None,
    };
    let report = match &校准 {
        Some(c) => jpp::Session::explain_with_calib_actions(
            &program,
            画像.as_ref().map(|p| &p.profile),
            c,
            &jpp::actions::check_table(),
        ),
        None => jpp::Session::explain_with_actions(
            &program,
            画像.as_ref().map(|p| &p.profile),
            &jpp::actions::check_table(),
        ),
    };
    // Z0157：`check` 也过一遍规划器，让可靠下界超预算的 `E-budget-plan`（J-07b 静态面，B42）在检查期可见；
    // 与 `run` 首跑同口径（账本为空、缓存关闭）。`run` 自己在执行前规划，这里不重复。
    let mut report = report;
    let mut check_plan: Option<(jpp::interp::Plan, jpp::interp::PlanCtx)> = None;
    if matches!(command, Command::Check { .. }) {
        let price = match 画像.as_ref() {
            Some(r) => match r.profile.price_per_input_token() {
                Some(p) => jpp::interp::JudgePrice::Known(p),
                None => jpp::interp::JudgePrice::Untested,
            },
            None => jpp::interp::JudgePrice::NotGiven,
        };
        let ctx = jpp::interp::PlanCtx {
            ledger_empty: true,
            cache_off: true,
            judge_price: price,
        };
        let plan = jpp::interp::plan_with(
            &program,
            &jpp::interp::Passes::default(),
            画像.as_ref().map(|p| &p.profile),
            &ctx,
        );
        let strip = |d: &jpp_ir::ir::IrDiag| {
            d.message
                .strip_prefix(&format!("{}: ", d.code))
                .unwrap_or(&d.message)
                .to_string()
        };
        for w in &plan.warnings {
            report
                .diagnostics
                .push(jpp::Diagnostic::warning(&w.code, strip(w), w.span));
        }
        if let Some(d) = &plan.rejected {
            report
                .diagnostics
                .push(jpp::Diagnostic::error(&d.code, strip(d), d.span));
        }
        check_plan = Some((plan, ctx));
    }
    // `--explain`（Z0190 后一半）：`check` 在这里取计划与语境；`run` 由 `execute_with` 在运行前一刻自己取
    let check_explain = match (&check_plan, explain) {
        (Some((plan, ctx)), true) => Some((plan, ctx)),
        _ => None,
    };
    let items: Vec<_> = report
        .diagnostics
        .iter()
        .map(diag_json::from_check)
        .collect();
    // L7（2026-09-28，K-088）：`check` 列出程序用到的校准键、各处用得上哪一层记录、`cut` 上的代价矩阵
    let 键清单 = 校准
        .as_ref()
        .map(|c| calib_keys_out::build(&jpp_check::calib_keys::calib_keys(&program), c, &loaded));
    if diag_json::json_mode() && matches!(command, Command::Check { .. }) {
        let explain_doc =
            check_explain.map(|(plan, ctx)| runner::explain_json(plan, ctx, &program, &loaded));
        print_check_doc(
            &filename,
            &loaded,
            items,
            questions_out_summary.as_ref(),
            键清单.as_ref().map(|(doc, _)| doc),
            explain_doc.as_ref(),
        );
        return if report.is_ok() {
            Ok(())
        } else {
            Err(String::new())
        };
    }
    for line in diag_json::render_all(&loaded, items) {
        eprintln!("{line}");
    }
    if let Some((_, 行)) = &键清单 {
        for line in 行 {
            eprintln!("{line}");
        }
    }
    // 即使检查有错（含 `E-budget-plan`）也先打印计划：「拒绝前先看到为什么」
    if let Some((plan, ctx)) = check_explain {
        print!("{}", runner::explain_text(plan, ctx, &program, &loaded));
    }
    if !report.is_ok() {
        return Err(format!(
            "{filename}: check failed ({} errors)",
            report.errors().len()
        ));
    }
    match command {
        Command::Check { .. } => println!(
            "Checked {filename}: no static errors ({} warnings)",
            report.warnings().len()
        ),
        Command::Run(options) => {
            run_io::run_checked(&program, &options, &loaded, 画像, 输入, explain)?
        }
        _ => unreachable!(),
    }
    Ok(())
}

/// `check --questions-out` 在 `--json` 模式下的摘要（PR #36 复核 P2）：非 JSON 模式打
/// `eprintln!`，JSON 模式下改进这个文档的 `questions_out` 字段——两种模式下 stderr 都不受
/// 这一步污染，`check --json` 的「stdout 一个文档，stderr 不出东西」契约对这个组合同样成立。
struct QuestionsOutSummary {
    labels: usize,
    questions: usize,
    skipped: usize,
    path: std::path::PathBuf,
}

/// `check --json`：stdout 一个文档，stderr 不出东西（步 9a）。`questions_out` 非空时，
/// `--questions-out` 的导出摘要进文档的 `questions_out` 字段（见 [`QuestionsOutSummary`]）。
fn print_check_doc(
    filename: &str,
    loaded: &jpp_syntax::loader::LoadedProgram,
    items: Vec<diag_json::Item>,
    questions_out: Option<&QuestionsOutSummary>,
    calib_keys: Option<&serde_json::Value>,
    explain: Option<&serde_json::Value>,
) {
    let folded = diag_json::fold(items);
    let count = |l: diag_json::Level| {
        folded
            .iter()
            .filter(|(it, _)| it.level == l)
            .map(|(_, n)| n)
            .sum::<usize>()
    };
    let mut doc = serde_json::json!({
        "file": filename,
        "ok": count(diag_json::Level::Error) == 0,
        "errors": count(diag_json::Level::Error),
        "warnings": count(diag_json::Level::Warning),
        "diagnostics": folded.iter().map(|(it, n)| diag_json::to_json(loaded, it, *n)).collect::<Vec<_>>(),
    });
    if let Some(q) = questions_out {
        doc["questions_out"] = serde_json::json!({
            "labels": q.labels,
            "questions": q.questions,
            "skipped": q.skipped,
            "path": q.path.display().to_string(),
        });
    }
    if let Some(k) = calib_keys {
        doc["calib_keys"] = k.clone();
    }
    if let Some(e) = explain {
        doc["explain"] = e.clone();
    }
    println!("{doc}");
}

/// G2（`12` R9 单次形态）：这次运行有违规，进程以退出码 3 结束（0 成功、1 出错、2 用法错）
pub(crate) static VIOLATION_EXIT: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);

fn main() -> ExitCode {
    let mut args: Vec<String> = env::args().skip(1).collect();
    match options::take_json(&mut args) {
        Ok(on) => diag_json::set_json(on),
        Err(error) => {
            eprintln!("{error}\n\n{}", help());
            return ExitCode::from(2);
        }
    }
    let questions_out = match options::take_questions_out(&mut args) {
        Ok(q) => q,
        Err(error) => {
            eprintln!("{error}\n\n{}", help());
            return ExitCode::from(2);
        }
    };
    let explain = match options::take_explain(&mut args) {
        Ok(on) => on,
        Err(error) => {
            eprintln!("{error}\n\n{}", help());
            return ExitCode::from(2);
        }
    };
    // B0472：`--purpose`、`--mat`、`--mat-store` 同法在解析前取出
    let mut 入口开关 = match options::take_entry(&mut args) {
        Ok(f) => f,
        Err(error) => {
            eprintln!("{error}\n\n{}", help());
            return ExitCode::from(2);
        }
    };
    let mut command = match options::parse(&args) {
        Ok(command) => command,
        Err(error) => {
            eprintln!("{error}\n\n{}", help());
            return ExitCode::from(2);
        }
    };
    if let Command::Run(r) = &mut command {
        r.mat_store = 入口开关.mat_store.take();
    }
    match execute(command, questions_out, explain, 入口开关) {
        Ok(()) if VIOLATION_EXIT.load(std::sync::atomic::Ordering::SeqCst) => ExitCode::from(3),
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            // 空报文：诊断已按 `--json` 输出过
            if !error.is_empty() {
                eprintln!("{error}");
            }
            ExitCode::FAILURE
        }
    }
}
