//! File and client setup after the caller has checked the shared core program.
use crate::{fixture::Fixture, options::RunOptions, runner};
use jpp_core::{
    ast::Program,
    effects::{CalibStore, FixedClient, NoCallClient},
    ledger::Ledger,
};
use serde::{Serialize, de::DeserializeOwned};
use std::{fs, path::Path};

fn read_json<T: DeserializeOwned>(path: &Path) -> Result<T, String> {
    let bytes = fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
    serde_json::from_slice(&bytes).map_err(|e| format!("{}: {e}", path.display()))
}

fn write_json<T: Serialize>(path: &Path, value: &T) -> Result<(), String> {
    let mut bytes = serde_json::to_vec_pretty(value).map_err(|e| e.to_string())?;
    bytes.push(b'\n');
    fs::write(path, bytes).map_err(|e| format!("{}: {e}", path.display()))
}

pub fn run_checked(
    program: &Program,
    options: &RunOptions,
    loaded: &jpp_frontend::loader::LoadedProgram,
) -> Result<(), String> {
    let (mut client, calibrations, description) = match &options.fixtures {
        Some(path) => {
            let fixture: Fixture = read_json(path)?;
            let (client, calibrations) = fixture
                .build()
                .map_err(|e| format!("{}: {e}", path.display()))?;
            (client, calibrations, Some(fixture.description))
        }
        None => (FixedClient::new(), CalibStore::new(), None),
    };
    let mut ledger = match options.replay.as_ref().or(options.resume.as_ref()) {
        Some(path) => read_json::<Ledger>(path)?,
        None => Ledger::new(),
    };
    ledger.rebuild_index();
    let result = if options.replay.is_some() {
        runner::execute(program, &mut NoCallClient, &calibrations, &mut ledger, true)
    } else {
        runner::execute(program, &mut client, &calibrations, &mut ledger, false)
    };
    // Preserve any completed effects even when execution ends in a runtime error.
    if let Some(path) = &options.ledger_out {
        write_json(path, &ledger)?;
    }
    let mut report = result.map_err(|error| {
        let e = match error {
            jpp_core::Error::Runtime(e) => e,
            jpp_core::Error::Check(report) => {
                return report
                    .diagnostics
                    .iter()
                    .map(|d| {
                        loaded.render(&jpp_frontend::Diagnostic::new(
                            format!("{}: {}", d.rule, d.message),
                            jpp_frontend::ast::Span {
                                start: d.span.start,
                                end: d.span.end,
                            },
                        ))
                    })
                    .collect::<Vec<_>>()
                    .join("\n");
            }
        };
        let diagnostic = jpp_frontend::Diagnostic::new(
            match e.rule {
                Some(rule) => format!("{rule}: {}", e.message),
                None => e.message,
            },
            jpp_frontend::ast::Span {
                start: e.span.start,
                end: e.span.end,
            },
        );
        loaded.render(&diagnostic)
    })?;
    report["fixture_description"] = serde_json::json!(description);
    report["replay"] = serde_json::json!(options.replay.is_some());
    report["resumed"] = serde_json::json!(options.resume.is_some());
    if let Some(path) = &options.output {
        write_json(path, &report)?;
        println!(
            "{}: {} (new model calls: {}); report: {}",
            options.source.display(),
            report["status"].as_str().unwrap_or("unknown"),
            report["cost"]["calls"],
            path.display()
        );
    } else {
        println!(
            "{}",
            serde_json::to_string_pretty(&report).map_err(|e| e.to_string())?
        );
    }
    Ok(())
}
