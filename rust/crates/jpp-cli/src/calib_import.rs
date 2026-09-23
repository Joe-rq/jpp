//! `jpp calib-import`：真值通道的命令行入口（B19）。逻辑在 `jpp_core::truth`。
use crate::options::ImportArgs;
use jpp_core::{effects::{CalibStore, Profile}, truth::{ImportOptions, LabelRow, import_labels}};
use std::fs;

pub fn run(a: &ImportArgs) -> Result<(), String> {
    let text = fs::read_to_string(&a.labels).map_err(|e| format!("{}: {e}", a.labels.display()))?;
    let mut rows: Vec<LabelRow> = vec![];
    for (i, line) in text.lines().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        rows.push(serde_json::from_str(line).map_err(|e| format!("{} 第 {} 行：{e}", a.labels.display(), i + 1))?);
    }
    let mut store = match &a.calib {
        Some(d) => CalibStore::load(d).map_err(|e| format!("{}: {e}", d.display()))?,
        None => CalibStore::new(),
    };
    if let Some(p) = &a.profile {
        store.profile = Profile::load(p).map_err(|e| format!("{}: {e}", p.display()))?;
    }
    let batch = a.labels.file_name().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
    let opt = ImportOptions { alpha: a.alpha, conf_delta: a.conf_delta, spot_check_min: a.spot_check_min, spot_check_conf: a.spot_check_conf, abstain_warn: a.abstain_warn, batch, seed: a.seed };
    let reports = import_labels(&mut store, &rows, &opt)?;
    for r in &reports {
        for w in &r.warnings {
            eprintln!("{w}");
        }
    }
    store.save(&a.calib_out).map_err(|e| format!("{}: {e}", a.calib_out.display()))?;
    println!("{}", serde_json::to_string_pretty(&reports).map_err(|e| e.to_string())?);
    eprintln!("导入 {} 行，{} 个键，写回 {}", rows.len(), reports.len(), a.calib_out.display());
    Ok(())
}
