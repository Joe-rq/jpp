use std::path::PathBuf;

pub const HELP: &str = "J++ native source tools\nUsage:\n  jpp parse <file.jpp> [--ast]\n  jpp check <file.jpp>\n  jpp run <file.jpp> [--fixtures <file.json>] [--output <report.json>]\n          [--ledger-out <ledger.json>] [--replay <ledger.json> | --resume <ledger.json>]\n\nLeading relative imports load source libraries. Run uses fixed generation/judgment/response records; no model API requests are made. Registered actions: record_check, read_json(path), write_json(path,value). File paths use the working directory. Resume may perform unrecorded actions; replay rejects them.";

#[derive(Debug, PartialEq)]
pub enum Command {
    Help,
    Parse { source: PathBuf, ast: bool },
    Check { source: PathBuf },
    Run(RunOptions),
}

#[derive(Debug, PartialEq)]
pub struct RunOptions {
    pub source: PathBuf,
    pub fixtures: Option<PathBuf>,
    pub output: Option<PathBuf>,
    pub ledger_out: Option<PathBuf>,
    pub replay: Option<PathBuf>,
    pub resume: Option<PathBuf>,
}

pub fn parse(args: &[String]) -> Result<Command, String> {
    if args.is_empty() || args == ["--help"] || args == ["-h"] {
        return Ok(Command::Help);
    }
    let verb = args[0].as_str();
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
    };
    let mut i = 2;
    while i < args.len() {
        let target = match args[i].as_str() {
            "--fixtures" => &mut options.fixtures,
            "--output" => &mut options.output,
            "--ledger-out" => &mut options.ledger_out,
            "--replay" => &mut options.replay,
            "--resume" => &mut options.resume,
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
    if options.replay.is_some() && options.resume.is_some() {
        return Err(
            "use either --replay (no new requests) or --resume (continue with the client)".into(),
        );
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
        ] {
            assert!(parse(&args(&argv)).is_err(), "{argv:?}");
        }
    }
}
