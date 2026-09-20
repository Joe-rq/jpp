mod fixture;
mod options;
mod run_io;
mod runner;

use options::{Command, HELP};
use std::{env, fs, process::ExitCode};

fn execute(command: Command) -> Result<(), String> {
    let path = match &command {
        Command::Help => {
            println!("{HELP}");
            return Ok(());
        }
        Command::Parse { source, .. } | Command::Check { source } => source,
        Command::Run(options) => &options.source,
    };
    let filename = path.to_string_lossy();
    let source = fs::read_to_string(path).map_err(|e| format!("{filename}: {e}"))?;
    let parsed = jpp_frontend::parse(&source).map_err(|e| e.render(&filename, &source))?;
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
    let program = jpp_frontend::lower(&parsed).map_err(|e| e.render(&filename, &source))?;
    let report = jpp_core::check::check(&program);
    for d in &report.diagnostics {
        let diagnostic = jpp_frontend::Diagnostic::new(
            format!("{}: {}", d.rule, d.message),
            jpp_frontend::ast::Span {
                start: d.span.start,
                end: d.span.end,
            },
        );
        eprintln!("{}", diagnostic.render(&filename, &source));
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
        Command::Run(options) => run_io::run_checked(&program, &options, &source)?,
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
