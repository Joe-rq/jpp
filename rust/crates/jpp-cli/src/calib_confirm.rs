//! `jpp calib-confirm <calib-dir> <key> --suspend | --keep`：人确认停岗候选（B25）。
//!
//! 系统只把记录标成「停岗候选」并停止用它放行不可逆 `do`；正式停岗由人确认。
//! `--suspend`：候选 → 停岗。`--keep`：候选 → 上岗（人判定漂移不影响这条线）。
//! 只处理状态为「停岗候选」的记录；其余状态不动并报错，免得把这条命令当成通用写线口。

use jpp_core::effects::CalibStore;

pub fn run(dir: &std::path::Path, key: &str, suspend: bool) -> Result<(), String> {
    let mut store = CalibStore::load(dir)?;
    let k = store
        .records
        .keys()
        .find(|k| k.as_str() == key || k.replace('\u{1f}', ":").trim_start_matches(':') == key.trim_start_matches(':'))
        .cloned()
        .ok_or_else(|| format!("{} 里没有校准记录 {key}", dir.display()))?;
    let r = store.records.get_mut(&k).expect("刚找到");
    if r.status != "停岗候选" {
        return Err(format!("{key} 的状态是「{}」，不是停岗候选；calib-confirm 只确认候选", r.status));
    }
    r.status = if suspend { "停岗".into() } else { "上岗".into() };
    let to = r.status.clone();
    store.save(dir)?;
    eprintln!("{key}：停岗候选 → {to}（人确认，B25）");
    Ok(())
}
