mod calib_confirm;
mod calib_import;
mod diag_json;
mod fixture;
mod options;
mod profile_resolve;
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
        Command::CalibConfirm { dir, key, suspend } => return calib_confirm::run(dir, key, *suspend),
        Command::Parse { source, .. } | Command::Check { source } => source,
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
    // 降级诊断与检查诊断走同一渲染层（步 9a）：同码同址折叠，`--json` 时出机读格式
    let program = match jpp_core::lower(parsed) {
        Ok(p) => p,
        Err(ds) => {
            let items: Vec<_> = ds.iter().map(diag_json::from_lower).collect();
            if diag_json::json_mode() && matches!(command, Command::Check { .. }) {
                print_check_doc(&filename, &loaded, items);
                return Err(String::new());
            }
            return Err(diag_json::render_all(&loaded, items).join("\n"));
        }
    };
    // 画像只解析一次（B73）：静态检查、账本头 `profile_hash`、「档案：…」提示都用这一个结果。
    // 真机运行解析不到画像即报 `E-profile-missing`，在检查与初始化真机客户端之前。
    let 画像 = match &command {
        Command::Run(r) => profile_resolve::resolve(r)?,
        _ => None,
    };
    let report = match &画像 {
        Some(p) => jpp_core::check::check_with_profile(&program, &p.profile),
        None => jpp_core::check::check(&program),
    };
    let items: Vec<_> = report.diagnostics.iter().map(diag_json::from_check).collect();
    if diag_json::json_mode() && matches!(command, Command::Check { .. }) {
        print_check_doc(&filename, &loaded, items);
        return if report.is_ok() { Ok(()) } else { Err(String::new()) };
    }
    for line in diag_json::render_all(&loaded, items) {
        eprintln!("{line}");
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
        Command::Run(options) => run_io::run_checked(&program, &options, &loaded, 画像)?,
        _ => unreachable!(),
    }
    Ok(())
}

/// `check --json`：stdout 一个文档，stderr 不出东西（步 9a）。
fn print_check_doc(filename: &str, loaded: &jpp_syntax::loader::LoadedProgram, items: Vec<diag_json::Item>) {
    let folded = diag_json::fold(items);
    let count = |l: diag_json::Level| folded.iter().filter(|(it, _)| it.level == l).map(|(_, n)| n).sum::<usize>();
    let doc = serde_json::json!({
        "file": filename,
        "ok": count(diag_json::Level::Error) == 0,
        "errors": count(diag_json::Level::Error),
        "warnings": count(diag_json::Level::Warning),
        "diagnostics": folded.iter().map(|(it, n)| diag_json::to_json(loaded, it, *n)).collect::<Vec<_>>(),
    });
    println!("{doc}");
}

fn main() -> ExitCode {
    let mut args: Vec<String> = env::args().skip(1).collect();
    match options::take_json(&mut args) {
        Ok(on) => diag_json::set_json(on),
        Err(error) => {
            eprintln!("{error}\n\n{HELP}");
            return ExitCode::from(2);
        }
    }
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
            // 空报文：诊断已按 `--json` 输出过
            if !error.is_empty() {
                eprintln!("{error}");
            }
            ExitCode::FAILURE
        }
    }
}
