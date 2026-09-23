mod calib_import;
mod fixture;
mod options;
mod run_io;
mod runner;

use options::{Command, HELP};
use std::{env, process::ExitCode};

fn execute(command: Command) -> Result<(), String> {
    let path = match &command {
        Command::Help => {
            println!("{HELP}");
            return Ok(());
        }
        Command::CalibImport(a) => return calib_import::run(a),
        Command::Parse { source, .. } | Command::Check { source } => source,
        Command::Run(options) => &options.source,
    };
    let filename = path.to_string_lossy();
    let loaded = jpp_frontend::loader::load(path)?;
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
    let program = jpp_frontend::lower(parsed).map_err(|e| loaded.render(&e))?;
    // 越界接线：类假设降级（`12` §1）要靠档案才走得到。没有 `--profile` 时
    // `Profile::default()` 的 `hash` 是 `None`，`check_with_profile` 会退回「没档案」那一路
    // ——**行为与以前逐字节相同**。
    let 档案 = match &command {
        options::Command::Run(r) => r.profile.as_ref(),
        _ => None,
    };
    let report = match 档案 {
        Some(p) => {
            let prof = jpp_core::effects::Profile::load(p).map_err(|e| format!("{}: {e}", p.display()))?;
            jpp_core::check::check_with_profile(&program, &prof)
        }
        None => jpp_core::check::check(&program),
    };
    for d in &report.diagnostics {
        let diagnostic = jpp_frontend::Diagnostic::new(
            format!("{}: {}", d.rule, d.message),
            jpp_frontend::ast::Span {
                start: d.span.start,
                end: d.span.end,
            },
        );
        eprintln!("{}", loaded.render(&diagnostic));
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
        Command::Run(options) => run_io::run_checked(&program, &options, &loaded)?,
        _ => unreachable!(),
    }
    Ok(())
}

fn main() -> ExitCode {
    let args: Vec<String> = env::args().skip(1).collect();
    let command = match options::parse(&args) {
        Ok(command) => command,
        Err(error) => {
            eprintln!("{error}\n\n{HELP}");
            return ExitCode::from(2);
        }
    };
    match execute(command) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{error}");
            ExitCode::FAILURE
        }
    }
}
