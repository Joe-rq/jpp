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
    // 越界接线：`--calib` 先装目录里的记录，`--fixtures` 的 `calibrations` 再覆盖同名键。
    // **顺序是「夹具优先」**，因为夹具是这一次跑的显式布置，而目录是常备资产；
    // 两边都给同一个键时**谁赢要说得出来**，所以下面会把被覆盖的键报出来。
    let mut 目录记录 = match &options.calib {
        Some(dir) => CalibStore::load(dir).map_err(|e| format!("{}: {e}", dir.display()))?,
        None => CalibStore::new(),
    };
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
    // 合并：夹具里的键覆盖目录里的同名键，**并把覆盖了哪些键打出来**
    let mut 被覆盖: Vec<String> = vec![];
    for (k, rec) in &calibrations.records {
        if 目录记录.records.contains_key(k) {
            被覆盖.push(k.clone());
        }
        目录记录.records.insert(k.clone(), rec.clone());
    }
    被覆盖.sort();
    if !被覆盖.is_empty() {
        eprintln!("注意：--fixtures 的 calibrations 覆盖了 --calib 目录里的同名键：{}", 被覆盖.join("、"));
    }
    if let Some(path) = &options.profile {
        目录记录.profile = jpp_core::effects::Profile::load(path).map_err(|e| format!("{}: {e}", path.display()))?;
    }
    // **「这次用了哪份档案」要说得出来**：接上档案之后，同一个程序会从「算不出 + 告警」
    // 变成「真的排出来」——**那是使用者那一侧的行为变化，不能没有可见的成因。**
    //
    // **只在真的给了参数时才打**：一个参数都不给时，输出与接线前**逐字节相同**——
    // **不越界的人不受影响**，这是那条保障的技术形式。
    if options.profile.is_some() || options.calib.is_some() {
    eprintln!(
        "档案：{}；校准记录：{} 条（{}）",
        match (&options.profile, &目录记录.profile.hash) {
            (Some(p), Some(h)) => format!("{} (hash {h})", p.display()),
            _ => "未加载，线与 δ 用的是代码兜底".to_string(),
        },
        目录记录.records.len(),
        match &options.calib { Some(d) => format!("--calib {}", d.display()), None => "仅来自 --fixtures".into() }
    );
    }
    let calibrations = 目录记录;
    let mut ledger = match options.replay.as_ref().or(options.resume.as_ref()) {
        Some(path) => read_json::<Ledger>(path)?,
        None => Ledger::new(),
    };
    ledger.rebuild_index();
    let mut evidence: Vec<(String, jpp_core::effects::Sample)> = vec![];
    let result = if options.replay.is_some() {
        runner::execute(program, &mut NoCallClient, &calibrations, &mut ledger, true, &mut evidence)
    } else {
        runner::execute(program, &mut client, &calibrations, &mut ledger, false, &mut evidence)
    };
    // **出料那一半**：把这一趟判出来的读数折进记录并落盘。
    // **只有这样那条环才闭得上**——`--calib` 是入料，J-03 决定了程序自己写不了线。
    //
    // **折进去的是无标注观察**：`absorb` 不让记录上岗（`n` 不动、冷记录推到「待真值」），
    // **上岗仍要 `commission`，而认证是校准过程不是程序行为。**
    if let Some(dir) = &options.calib_out {
        let mut 出 = calibrations.clone();
        for (k, sm) in &evidence {
            出.absorb(k, sm.clone()).map_err(|e| format!("折证据进 {k} 失败：{e}"))?;
        }
        出.save(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        eprintln!("校准记录已写回 {}（{} 条记录，本趟折进 {} 条观察）", dir.display(), 出.records.len(), evidence.len());
    }
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
