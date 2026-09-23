use std::path::PathBuf;

pub const HELP: &str = "J++ native source tools\nUsage:\n  jpp parse <file.jpp> [--ast]\n  jpp check <file.jpp>\n  jpp run <file.jpp> [--fixtures <file.json>] [--output <report.json>]\n          [--ledger-out <ledger.json>] [--replay <ledger.json> | --resume <ledger.json>]
          [--profile <profile.json>] [--calib <calib-dir>] [--calib-out <calib-dir>]
          [--backend fixed|live] [--model <name>]\n  jpp calib-import <labels.jsonl> --calib-out <calib-dir> [--calib <calib-dir>] [--profile <profile.json>]\n          [--alpha 0.1] [--conf-delta 0.1] [--spot-check-min 0.9] [--spot-check-conf 0.95] [--abstain-warn 0.1] [--seed 20260923]\n\nLeading relative imports load source libraries. Run defaults to fixed generation/judgment/response records; no model API requests are made. --backend live switches to the real JEV backend (JevClient::live), reading the API key only from ~/.typesafe-key (never logged or written to any report); it requires jpp-cli to be built with `--features live` and is mutually exclusive with --fixtures. --model names the model for --backend live (default jev-1.13.0). Registered actions: record_check, read_json(path), write_json(path,value). --profile loads a model profile (lines, deltas, windows, class-assumption fields); without it the kernel falls back to code defaults and says so. --calib loads calibration records from a directory of per-key JSON files. --calib-out folds this run's readings into those records and writes them back, which is the only way the calibration loop closes: J-03 forbids a program from writing a line itself. File paths use the working directory. Resume may perform unrecorded actions; replay rejects them. Replay restores the calibration records the ledger recorded as used when they are not supplied again. calib-import is the truth channel: it folds labelled readings (JSONL: key or form, item, p, label true|false|\"ambiguous\", source human|computed|model:<name>, optional spot_check) into calibration records and certifies them by split-sample two-sided commission (B24: lines are chosen on one half, selected by --seed, and certified once on the other half; the method, seed and both halves' sizes are written into the certificate); model-only truth is certified only when a same-key human spot check reaches --spot-check-min: the point estimate below it keeps the record pending; a point estimate at or above it whose one-sided --spot-check-conf lower bound is still below it certifies the line provisionally (gate \"临时上岗\", W-provisional at use) and reports how many more all-agreeing checks would confirm it.";

/// 默认的真机模型名：与 `crates/jpp-core/tests/e_jpp_live.rs`、`tests/jev_client.rs`
/// 及 `foundation/profile/profiles/jev-1.13.0.json` 用的字符串一致。
pub const DEFAULT_LIVE_MODEL: &str = "jev-1.13.0";

#[derive(Debug, PartialEq)]
pub enum Command {
    Help,
    Parse { source: PathBuf, ast: bool },
    Check { source: PathBuf },
    Run(RunOptions),
    CalibImport(ImportArgs),
}

#[derive(Debug, PartialEq)]
pub struct ImportArgs {
    pub labels: PathBuf,
    pub calib: Option<PathBuf>,
    pub calib_out: PathBuf,
    pub profile: Option<PathBuf>,
    pub alpha: f64,
    pub conf_delta: f64,
    pub spot_check_min: f64,
    pub spot_check_conf: f64,
    pub abstain_warn: f64,
    pub seed: u64,
}

fn parse_import(args: &[String]) -> Result<Command, String> {
    let labels = args.get(1).filter(|s| !s.starts_with("--")).ok_or("calib-import requires a labels file (JSONL)")?;
    let (mut calib, mut calib_out, mut profile) = (None, None, None);
    let (mut alpha, mut conf_delta, mut spot_check_min, mut abstain_warn) = (0.1, 0.1, 0.9, 0.1);
    let mut spot_check_conf = 0.95;
    let mut seed: u64 = 20260923;
    let mut i = 2;
    while i < args.len() {
        let v = args.get(i + 1).filter(|s| !s.starts_with("--")).ok_or_else(|| format!("{} requires a value", args[i]))?;
        let num = |x: &str| x.parse::<f64>().map_err(|_| format!("{} expects a number, got {x}", args[i]));
        match args[i].as_str() {
            "--calib" => calib = Some(PathBuf::from(v)),
            "--calib-out" => calib_out = Some(PathBuf::from(v)),
            "--profile" => profile = Some(PathBuf::from(v)),
            "--alpha" => alpha = num(v)?,
            "--conf-delta" => conf_delta = num(v)?,
            "--spot-check-min" => spot_check_min = num(v)?,
            "--spot-check-conf" => spot_check_conf = num(v)?,
            "--abstain-warn" => abstain_warn = num(v)?,
            "--seed" => seed = v.parse::<u64>().map_err(|_| format!("--seed expects a non-negative integer, got {v}"))?,
            other => return Err(format!("unknown calib-import option '{other}'")),
        }
        i += 2;
    }
    let calib_out = calib_out.ok_or("calib-import requires --calib-out <dir>")?;
    Ok(Command::CalibImport(ImportArgs { labels: labels.into(), calib, calib_out, profile, alpha, conf_delta, spot_check_min, spot_check_conf, abstain_warn, seed }))
}

/// 观察后端：默认固定观察（不越界）；`live` 接真机 `JevClient`。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Backend {
    #[default]
    Fixed,
    Live,
}

#[derive(Debug, PartialEq)]
pub struct RunOptions {
    pub source: PathBuf,
    pub fixtures: Option<PathBuf>,
    pub output: Option<PathBuf>,
    pub ledger_out: Option<PathBuf>,
    pub replay: Option<PathBuf>,
    pub resume: Option<PathBuf>,
    pub profile: Option<PathBuf>,
    pub calib: Option<PathBuf>,
    pub calib_out: Option<PathBuf>,
    pub backend: Backend,
    pub model: Option<String>,
}

pub fn parse(args: &[String]) -> Result<Command, String> {
    if args.is_empty() || args == ["--help"] || args == ["-h"] {
        return Ok(Command::Help);
    }
    let verb = args[0].as_str();
    if verb == "calib-import" {
        return parse_import(args);
    }
    if !matches!(verb, "parse" | "check" | "run") {
        return Err(format!("unknown command '{verb}'"));
    }
    let source = args
        .get(1)
        .filter(|s| !s.starts_with("--"))
        .ok_or_else(|| format!("{verb} requires a .jpp source file"))?;
    if verb == "parse" {
        return match &args[2..] {
            [] => Ok(Command::Parse {
                source: source.into(),
                ast: false,
            }),
            [flag] if flag == "--ast" => Ok(Command::Parse {
                source: source.into(),
                ast: true,
            }),
            _ => Err("parse accepts only --ast after the source file".into()),
        };
    }
    if verb == "check" {
        return if args.len() == 2 {
            Ok(Command::Check {
                source: source.into(),
            })
        } else {
            Err("check accepts only a source file".into())
        };
    }
    let mut options = RunOptions {
        source: source.into(),
        fixtures: None,
        output: None,
        ledger_out: None,
        replay: None,
        resume: None,
        profile: None,
        calib: None,
        calib_out: None,
        backend: Backend::Fixed,
        model: None,
    };
    let mut backend_set = false;
    let mut i = 2;
    while i < args.len() {
        match args[i].as_str() {
            "--backend" => {
                if backend_set {
                    return Err("--backend was supplied twice".into());
                }
                let value = args
                    .get(i + 1)
                    .filter(|s| !s.starts_with("--"))
                    .ok_or_else(|| "--backend requires a value (fixed|live)".to_string())?;
                options.backend = match value.as_str() {
                    "fixed" => Backend::Fixed,
                    "live" => Backend::Live,
                    other => return Err(format!("unknown --backend '{other}' (expected fixed|live)")),
                };
                backend_set = true;
                i += 2;
            }
            "--model" => {
                if options.model.is_some() {
                    return Err("--model was supplied twice".into());
                }
                let value = args
                    .get(i + 1)
                    .filter(|s| !s.starts_with("--"))
                    .ok_or_else(|| "--model requires a value".to_string())?;
                options.model = Some(value.clone());
                i += 2;
            }
            _ => {
                let target = match args[i].as_str() {
                    "--fixtures" => &mut options.fixtures,
                    "--output" => &mut options.output,
                    "--ledger-out" => &mut options.ledger_out,
                    "--replay" => &mut options.replay,
                    "--resume" => &mut options.resume,
                    "--profile" => &mut options.profile,
                    "--calib" => &mut options.calib,
                    "--calib-out" => &mut options.calib_out,
                    other => return Err(format!("unknown run option '{other}'")),
                };
                if target.is_some() {
                    return Err(format!("{} was supplied twice", args[i]));
                }
                let value = args
                    .get(i + 1)
                    .filter(|s| !s.starts_with("--"))
                    .ok_or_else(|| format!("{} requires a file path", args[i]))?;
                *target = Some(value.into());
                i += 2;
            }
        }
    }
    if options.replay.is_some() && options.resume.is_some() {
        return Err(
            "use either --replay (no new requests) or --resume (continue with the client)".into(),
        );
    }
    if options.fixtures.is_some() && options.backend == Backend::Live {
        return Err("--fixtures and --backend live are mutually exclusive".into());
    }
    if options.model.is_some() && options.backend != Backend::Live {
        return Err("--model requires --backend live".into());
    }
    Ok(Command::Run(options))
}

#[cfg(test)]
mod tests {
    use super::*;
    fn args(xs: &[&str]) -> Vec<String> {
        xs.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn accepts_combined_fixture_replay_and_report_paths() {
        let Command::Run(run) = parse(&args(&[
            "run",
            "a file.jpp",
            "--fixtures",
            "fixture.json",
            "--replay",
            "ledger.json",
            "--output",
            "report.json",
        ]))
        .unwrap() else {
            panic!("run")
        };
        assert_eq!(run.source, PathBuf::from("a file.jpp"));
        assert_eq!(run.replay, Some("ledger.json".into()));
        assert_eq!(run.output, Some("report.json".into()));
    }

    #[test]
    fn reports_usage_mistakes_before_execution() {
        for argv in [
            vec!["run"],
            vec!["run", "a.jpp", "--output"],
            vec!["run", "a.jpp", "--output", "--replay", "x"],
            vec!["run", "a.jpp", "--fixtures", "a", "--fixtures", "b"],
            vec!["check", "a.jpp", "--ast"],
            vec!["run", "a.jpp", "--resume", "a", "--replay", "b"],
            vec!["run", "a.jpp", "--backend", "bogus"],
            vec!["run", "a.jpp", "--backend", "live", "--fixtures", "f.json"],
            vec!["run", "a.jpp", "--model", "jev-1.13.0"],
            vec!["run", "a.jpp", "--backend", "live", "--backend", "fixed"],
        ] {
            assert!(parse(&args(&argv)).is_err(), "{argv:?}");
        }
    }

    #[test]
    fn backend_defaults_to_fixed_and_live_needs_explicit_opt_in() {
        let Command::Run(run) = parse(&args(&["run", "a.jpp"])).unwrap() else {
            panic!("run")
        };
        assert_eq!(run.backend, Backend::Fixed);
        assert_eq!(run.model, None);
    }

    #[test]
    fn backend_live_accepts_a_model_name() {
        let Command::Run(run) = parse(&args(&[
            "run",
            "a.jpp",
            "--backend",
            "live",
            "--model",
            "jev-1.13.0",
        ]))
        .unwrap() else {
            panic!("run")
        };
        assert_eq!(run.backend, Backend::Live);
        assert_eq!(run.model, Some("jev-1.13.0".into()));
    }
}
