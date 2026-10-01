//! `jpp profile check <画像>`：列画像的版本与 δ 各读数段，缺中段 δ 报缺项（裁定四十五 (2)）。
//!
//! 缺 mid 的画像仍能加载、能跑不需要画像 δ 的程序；认证导入与 `cut` 兜底遇到它报错（`E-delta-mid`）。
//! 这个命令让接线人在跑之前就知道画像缺什么。齐全退出 0，有缺项退出 1。

use std::path::Path;

pub fn run(path: &Path) -> Result<(), String> {
    let p = jpp::effects::Profile::load(path)?;
    println!(
        "画像：{}（模型 {}，版本 profile_revision {}，hash {}）",
        path.display(),
        p.model_id.as_deref().unwrap_or("?"),
        p.revision,
        p.hash.as_deref().unwrap_or("?")
    );
    // 按列从画像 JSON 读，只有一列有中段时那一列照实显示（`Profile.delta` 要三列齐才算已测，不能拿来按列显示）
    let j: serde_json::Value = serde_json::from_slice(
        &std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?,
    )
    .map_err(|e| format!("{}: {e}", path.display()))?;
    let 列 = ["noul", "choice_prob_chosen", "score"];
    let 取 = |col: &str, 段: &[&str]| 段.iter().fold(&j["delta"][col], |v, k| &v[*k]).as_f64();
    let mut 缺 = vec![];
    let mut 有测 = false;
    for name in 列 {
        let tail = 取(name, &["immediate", "p99"]);
        let mid = 取(name, &["mid", "immediate", "p99"]);
        有测 |= tail.is_some() || mid.is_some();
        let 显示 = |v: Option<f64>| v.map_or("未测".to_string(), |x| format!("{x}"));
        println!(
            "  delta.{name}: tail p99 {} · mid p99 {}",
            显示(tail),
            显示(mid)
        );
        if mid.is_none() {
            缺.push(format!("delta.{name}.mid.immediate.p99"));
        }
    }
    // 一个段都没测：不算缺 mid（按裁定五十六处理：带宽未知、不编 0）
    if !有测 {
        缺.clear();
    }
    if 缺.is_empty() {
        if p.delta.get().is_none() {
            // 一个段都没测：不算缺 mid（B187：cut 兜底取 0）；认证导入没有 δ 先验，照旧报错
            println!(
                "δ 未测（两段都没有）：cut 照线切、不加迁移带，出口带 delta_unknown、不放行；裂变不判 tie；认证导入要 δ 时报错（裁定五十六）"
            );
        } else {
            println!("δ 齐全：认证与 cut 取中段 δ");
        }
        return Ok(());
    }
    // 报文按画像实际情况写（复查可后补 3）：有没有尾段、缺哪几列
    let 有尾段: Vec<&str> = 列
        .iter()
        .copied()
        .filter(|c| 取(c, &["immediate", "p99"]).is_some())
        .collect();
    let 情况 = if 有尾段.is_empty() {
        "这份画像没有尾段 δ，中段 δ 也不全".to_string()
    } else if 有尾段.len() == 列.len() {
        "这份画像有尾段 δ（满信心材料测得，线附近偏小），中段 δ 不全".to_string()
    } else {
        format!("这份画像只有 {} 的尾段 δ，中段 δ 不全", 有尾段.join("、"))
    };
    Err(format!(
        "缺项：{}。{情况}；重测中段 δ 之前不能用于认证导入与 cut 兜底（裁定四十五）。\
         测法见画像 delta.note 与 foundation/profile/SCHEMA.md",
        缺.join("、")
    ))
}
