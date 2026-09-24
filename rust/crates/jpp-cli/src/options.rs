use std::path::PathBuf;

pub const HELP: &str = "J++ native source tools\nUsage:\n  jpp parse <file.jpp> [--ast]\n  jpp check <file.jpp> [--json]\n  jpp run <file.jpp> [--json] [--fixtures <file.json>] [--output <report.json>]\n          [--ledger-out <ledger.json>] [--replay <ledger.json> | --resume <ledger.json>]
          [--profile <profile.json>] [--calib <calib-dir>] [--calib-out <calib-dir>]
          [--backend fixed|live] [--model <name>] [--profiles-dir <dir>]\n  jpp calib-import <labels.jsonl> --calib-out <calib-dir> [--calib <calib-dir>] [--profile <profile.json>]\n          [--alpha 0.1] [--conf-delta 0.1] [--spot-check-min 0.9] [--spot-check-conf 0.95] [--abstain-warn 0.1] [--seed 20260923]\n          [--extent-min-disagree 3] [--extent-same-dir 0.8] [--extent-same-tier 0.667] [--scope-quantiles 0.01,0.99] [--scope-margins 2,0.10] [--class-min-sources 2] [--alpha-trial 0.25]\n  jpp calib-confirm <calib-dir> <key> --suspend|--keep\n\nLeading relative imports load source libraries. --json makes diagnostics machine-readable, one JSON object each: {code, level, span {file, line, col, start, end}, message, fix, applicability manual|wiring|null, count}, plus explain for runtime codes E-rt-<name>; check --json prints one document {file, ok, errors, warnings, diagnostics} on stdout, run --json writes diagnostics as JSON lines (each starting with {) on stderr and leaves the report unchanged. Diagnostics with the same code, location and message are folded into one with a count (text output appends （同码同址 ×N）). Run defaults to fixed generation/judgment/response records; no model API requests are made. --backend live switches to the real JEV backend (JevClient::live), reading the API key only from ~/.typesafe-key (never logged or written to any report); it requires jpp-cli to be built with `--features live` and is mutually exclusive with --fixtures. --model names the model for --backend live (default jev-1.13.0). Registered actions: record_check, read_json(path), write_json(path,value). --profile loads a model profile (lines, deltas, windows, prices, class-assumption fields). A live run (first run or --resume) must have a profile (B73): --profile <file>, else --profiles-dir <dir>/<model>.json, else profiles/<model>.json next to the jpp executable (releases ship profiles/); if none is found the run stops with E-profile-missing listing the paths tried, and never falls back to code defaults. The profile hash goes into the ledger header, and the price comes only from the profile's cost field (no price: cost is reported as Unknown with W-cost-unknown). Replay makes no requests and needs no profile. A fixed-observation run without a profile still runs until step 15d and always prints that no profile is loaded and lines and deltas are code defaults. --calib loads calibration records from a directory of per-key JSON files. --calib-out folds this run's readings into those records and writes them back, which is the only way the calibration loop closes: J-03 forbids a program from writing a line itself. File paths use the working directory. Resume may perform unrecorded actions; replay rejects them. Replay restores the calibration records the ledger recorded as used when they are not supplied again. calib-import is the truth channel: it folds labelled readings (JSONL: key or form, item, p, label true|false|\"ambiguous\", source human|computed|model:<name>, optional spot_check review-batch id, optional generator) into calibration records and certifies them by split-sample two-sided commission (B24: lines are chosen on one half, selected by --seed, and certified once on the other half; the method, seed and both halves' sizes are written into the certificate); truth per item is taken in the order computed > human > review row > annotation row (B36; a review row is any row carrying spot_check, human or model:<name>; a review from the same source as that item's annotation or from the material generator is rejected); model-only truth is certified only when a same-key review batch reaches --spot-check-min, and the gate names model reviewers: the point estimate below it keeps the record pending; a point estimate at or above it whose one-sided --spot-check-conf lower bound is still below it certifies the line provisionally (gate \"临时上岗\", W-provisional at use) and reports how many more all-agreeing checks would confirm it. When review disagreements reach --extent-min-disagree and their direction agrees at --extent-same-dir or higher (or, with fill_tier on the rows, --extent-same-tier or more fall in one fill tier), the question's scope is judged undetermined (B36 5(c)): the record stays pending with the reason in the gate and no further review is requested; rewrite the question first. select and measure rows (B63) add op select|measure (or use a form with that op), p = the winning candidate's or level's probability, pick = the reading's argmax index, and label = the true candidate index (over order) or level index (scale order); they are certified one-sided on p_max (Pick/At only when p_max >= hi + delta; there is no low side). Two certification grades (B72): the key is first certified at --alpha (formal grade); only if that fails (certification refused or too few rows) is it certified again at --alpha-trial (default 0.25; a value not above --alpha disables it), and the certificate is marked trial. A trial line routes act/ignore like any line but reports W-trial-line and never releases an irreversible action; a trial import never overwrites a key that already holds a certified formal line. Sample size (split-sample certification needs every one of its four cells filled — selection half positive and negative, certification half upper and lower side — at alpha 0.1 each needs 22 zero-error decided rows, at alpha 0.25 each needs 9): measured offline (地基/评估/2026-09-24-B72对照/results_sweep.md), a formal line needs about 160 labelled rows for a literal question form and 200 rows do not guarantee it for a semantic one; a trial line needs about 60–80 rows for a literal form and about 80–100 for a semantic one. When every certified row carries text (the judged material), the record stores a material fingerprint of the certification set (B68: character count, Chinese / Latin / digit / punctuation-and-space ratios, line count, mean line length, each as a --scope-quantiles interval widened by --scope-margins k,m: count-like quantities are divided/multiplied by k, ratios are widened by m and clipped to [0, 1]; the import prints how many certification rows fall outside their own range, which must be 0, else W-scope-self); at run time a line used on material outside that range still routes but reports W-calib-scope and cannot release an irreversible action. Records without a fingerprint are treated as in scope. A row with class <label> (B34) goes to the class record of that calibration class instead of its own key; the sample's source is the row's form (its form hash), else the hash of its question text (a hand-written question), else its key — different fills of one form are one source (B75). A class record is certified only when the batch mixes at least --class-min-sources distinct sources and each source has at least the grade's zero-error row count (22 formal, 9 trial); the split is stratified by source, and the record lists its sources (else it stays pending with the reason in the gate). At run time a question whose own key and form have no certified line borrows the class line of the key it was written with (W-class-line); a class line routes but does not release an irreversible action (B75). calib-confirm is the human confirmation of a suspension candidate (B25): a drift signal marks a certified line as a candidate (its exits still route but cannot release an irreversible action), --calib-out writes the candidate status, and --suspend or --keep settles it.";

/// 默认的真机模型名：与 `crates/jpp-core/tests/e_jpp_live.rs`、`tests/jev_client.rs`
/// 及发行画像 `profiles/jev-1.13.0.json`（由 `地基/foundation/profile/profiles/jev-1.13.0.json` 经
/// `scripts/gen_profiles.py` 生成）的文件名一致。
pub const DEFAULT_LIVE_MODEL: &str = "jev-1.13.0";

#[derive(Debug, PartialEq)]
pub enum Command {
    Help,
    Parse { source: PathBuf, ast: bool },
    Check { source: PathBuf },
    Run(RunOptions),
    CalibImport(ImportArgs),
    CalibConfirm { dir: PathBuf, key: String, suspend: bool },
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
    pub extent_min_disagree: usize,
    pub extent_same_dir: f64,
    pub extent_same_tier: f64,
    pub scope_quantiles: (f64, f64),
    pub scope_margins: (f64, f64),
    pub class_min_sources: usize,
    pub alpha_trial: f64,
}

fn parse_import(args: &[String]) -> Result<Command, String> {
    let labels = args.get(1).filter(|s| !s.starts_with("--")).ok_or("calib-import requires a labels file (JSONL)")?;
    let (mut calib, mut calib_out, mut profile) = (None, None, None);
    let (mut alpha, mut conf_delta, mut spot_check_min, mut abstain_warn) = (0.1, 0.1, 0.9, 0.1);
    let mut spot_check_conf = 0.95;
    let mut seed: u64 = 20260923;
    let (mut extent_min_disagree, mut extent_same_dir, mut extent_same_tier) = (3usize, 0.8, 2.0 / 3.0);
    let mut scope_quantiles = (0.01, 0.99);
    let mut scope_margins = (2.0, 0.10);
    // B75：类记录「混合样本」的来源数下限（来源 = 题式，填法不算不同来源）
    let mut class_min_sources = 2usize;
    // B72：试用 α（正式 α 不过时再认证一次；不大于 --alpha 即不试）
    let mut alpha_trial = 0.25;
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
            "--extent-min-disagree" => extent_min_disagree = v.parse::<usize>().map_err(|_| format!("--extent-min-disagree expects a non-negative integer, got {v}"))?,
            "--extent-same-dir" => extent_same_dir = num(v)?,
            "--extent-same-tier" => extent_same_tier = num(v)?,
            "--scope-quantiles" => {
                let bad = || format!("--scope-quantiles expects two numbers lo,hi in [0, 1] with lo < hi, got {v}");
                let (a, b) = v.split_once(',').ok_or_else(bad)?;
                let (a, b) = (a.trim().parse::<f64>().map_err(|_| bad())?, b.trim().parse::<f64>().map_err(|_| bad())?);
                if !(0.0 <= a && a < b && b <= 1.0) {
                    return Err(bad());
                }
                scope_quantiles = (a, b);
            }
            "--scope-margins" => {
                let bad = || format!("--scope-margins expects k,m with k >= 1 and 0 <= m < 1, got {v}");
                let (a, b) = v.split_once(',').ok_or_else(bad)?;
                let (a, b) = (a.trim().parse::<f64>().map_err(|_| bad())?, b.trim().parse::<f64>().map_err(|_| bad())?);
                if !(a >= 1.0 && (0.0..1.0).contains(&b)) {
                    return Err(bad());
                }
                scope_margins = (a, b);
            }
            "--class-min-sources" => class_min_sources = v.parse::<usize>().map_err(|_| format!("--class-min-sources expects a non-negative integer, got {v}"))?,
            "--alpha-trial" => alpha_trial = num(v)?,
            "--seed" => seed = v.parse::<u64>().map_err(|_| format!("--seed expects a non-negative integer, got {v}"))?,
            other => return Err(format!("unknown calib-import option '{other}'")),
        }
        i += 2;
    }
    let calib_out = calib_out.ok_or("calib-import requires --calib-out <dir>")?;
    Ok(Command::CalibImport(ImportArgs { labels: labels.into(), calib, calib_out, profile, alpha, conf_delta, spot_check_min, spot_check_conf, abstain_warn, seed, extent_min_disagree, extent_same_dir, extent_same_tier, scope_quantiles, scope_margins, class_min_sources, alpha_trial }))
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
    /// 真机画像目录（B73）：没给 `--profile` 时按 `<目录>/<model>.json` 找；缺省为可执行文件旁的 `profiles/`。
    pub profiles_dir: Option<PathBuf>,
}

/// 取出 `--json`（步 9a）：只有 `check` 与 `run` 收它，出现一次；其余参数原样交给 [`parse`]。
pub fn take_json(args: &mut Vec<String>) -> Result<bool, String> {
    let n = args.iter().skip(1).filter(|a| *a == "--json").count();
    if n == 0 {
        return Ok(false);
    }
    if !matches!(args.first().map(String::as_str), Some("check" | "run")) {
        return Err("--json is accepted only by check and run".into());
    }
    if n > 1 {
        return Err("--json was supplied twice".into());
    }
    let i = args.iter().skip(1).position(|a| a == "--json").unwrap() + 1;
    args.remove(i);
    Ok(true)
}

pub fn parse(args: &[String]) -> Result<Command, String> {
    if args.is_empty() || args == ["--help"] || args == ["-h"] {
        return Ok(Command::Help);
    }
    let verb = args[0].as_str();
    if verb == "calib-import" {
        return parse_import(args);
    }
    if verb == "calib-confirm" {
        let dir = args.get(1).ok_or("calib-confirm requires <calib-dir> <key> --suspend|--keep")?;
        let key = args.get(2).ok_or("calib-confirm requires <calib-dir> <key> --suspend|--keep")?;
        let suspend = match args.get(3).map(String::as_str) {
            Some("--suspend") => true,
            Some("--keep") => false,
            _ => return Err("calib-confirm requires --suspend or --keep".into()),
        };
        return Ok(Command::CalibConfirm { dir: dir.into(), key: key.clone(), suspend });
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
        profiles_dir: None,
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
                    "--profiles-dir" => &mut options.profiles_dir,
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
    if options.profiles_dir.is_some() && options.backend != Backend::Live {
        return Err("--profiles-dir requires --backend live".into());
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
            vec!["run", "a.jpp", "--profiles-dir", "profiles"],
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

    #[test]
    fn backend_live_accepts_a_profiles_dir() {
        let Command::Run(run) = parse(&args(&["run", "a.jpp", "--backend", "live", "--profiles-dir", "p"])).unwrap() else {
            panic!("run")
        };
        assert_eq!(run.profiles_dir, Some("p".into()));
    }
}
