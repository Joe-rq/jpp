//! `jpp bank recert`：重取读数并重认（补缺 2；`20` B48「复审周期 = 每次 `render_version` 或判断器版本变化后全库
//! 重认」）。重认要新读数才算，不是拿已存读数重算认证。
//!
//! **重取读数的方式**：每条目一个配方 `entries/<form_hash>/recert.json`，`{"read": <读数程序>, "input": <材料 JSON>}`，
//! 路径相对题库目录。读数程序与首次认证时同一类（`评估/27a-准备/F*/read.jpp`）：一条材料一道题，返回
//! `rows: [{id, key}]`，`key` 是账本里该读数的键。命令用 `jpp run --ledger-out` 跑它，按 `key` 从账本取新读数，
//! 覆盖 `labels.jsonl` 里每个带 `p` 的行（按 `item` 与材料 `id` 对上；`select`、`measure` 同时覆盖 `pick`），
//! 再走现有认证（`jpp calib-import`）写新记录，`verify_calib_at` 验过才替换条目的 `labels.jsonl` 与 `calib/`。
//! 认证不过：条目保留待重认，`labels.jsonl` 与 `calib/` 不动。
//!
//! **花钱**：`fixed`、`stub` 不花钱。其他后端（真机）必须给 `--max-cost`；花钱前打印预计调用数与读数程序 `budget` 的
//! 费用上限合计，合计超过 `--max-cost` 就不跑。不做交互确认。
use std::path::{Path, PathBuf};
use std::process::Command as Proc;

use jpp::store::bank::{QuestionBank, ReviewEnv};
use serde_json::{Value as Json, json};

use super::Args;

struct Plan {
    hash: String,
    /// 条目目录（旧 `calib/` 在里面，写回前与新记录比较）
    dir: PathBuf,
    slug: String,
    read: PathBuf,
    input: PathBuf,
    labels: Vec<String>,
    calls: usize,
    budget_cost: f64,
}

fn copy_dir(from: &Path, to: &Path) -> Result<(), String> {
    std::fs::create_dir_all(to).map_err(|e| format!("{}: {e}", to.display()))?;
    for e in std::fs::read_dir(from).map_err(|e| format!("{}: {e}", from.display()))? {
        let e = e.map_err(|e| e.to_string())?;
        let (f, t) = (e.path(), to.join(e.file_name()));
        if f.is_dir() {
            copy_dir(&f, &t)?;
        } else {
            std::fs::copy(&f, &t).map_err(|e| format!("{}: {e}", f.display()))?;
        }
    }
    Ok(())
}

/// 读数程序 `budget {… cost: X …}` 里的费用上限；找不到按无穷大（花钱的后端因此拒绝，不猜）。
fn budget_cost(src: &str) -> f64 {
    let Some(i) = src.find("budget") else {
        return f64::INFINITY;
    };
    let rest = &src[i..];
    let Some(j) = rest.find("cost:") else {
        return f64::INFINITY;
    };
    rest[j + 5..]
        .trim_start()
        .split(|c: char| !(c.is_ascii_digit() || c == '.'))
        .next()
        .and_then(|x| x.parse().ok())
        .unwrap_or(f64::INFINITY)
}

fn plan_of(bank: &QuestionBank, hash: &str) -> Result<Plan, String> {
    let e = bank.find(hash).ok_or(format!("题库里没有条目 {hash}"))?;
    let slug = e["slug"].as_str().unwrap_or("").to_string();
    let dir = bank.entry_dir(hash);
    let rp = dir.join("recert.json");
    let recipe: Json = serde_json::from_slice(&std::fs::read(&rp).map_err(|err| {
        format!(
            "{slug}：没有重取读数的配方 {}（{err}）。配方 {{\"read\": <读数程序>, \"input\": <材料 JSON>}}，路径相对题库目录",
            rp.display()
        )
    })?)
    .map_err(|err| format!("{}: {err}", rp.display()))?;
    let rel = |k: &str| -> Result<PathBuf, String> {
        let p = bank.dir().join(
            recipe[k]
                .as_str()
                .ok_or_else(|| format!("{}: 缺 {k}", rp.display()))?,
        );
        if p.is_file() {
            p.canonicalize().map_err(|e| e.to_string())
        } else {
            Err(format!("{slug}：配方里的 {k} 文件不存在：{}", p.display()))
        }
    };
    let read = rel("read")?;
    let input = rel("input")?;
    let lp = dir.join("labels.jsonl");
    let text = std::fs::read_to_string(&lp).map_err(|e| format!("{}: {e}", lp.display()))?;
    let labels: Vec<String> = text.lines().map(str::to_string).collect();
    let calls = labels
        .iter()
        .filter_map(|l| serde_json::from_str::<Json>(l).ok())
        .filter(|r| r["p"].is_number())
        .count();
    let src = std::fs::read_to_string(&read).map_err(|e| format!("{}: {e}", read.display()))?;
    Ok(Plan {
        hash: hash.to_string(),
        dir,
        slug,
        read,
        input,
        labels,
        calls,
        budget_cost: budget_cost(&src),
    })
}

/// 账本答案 → `(p, pick)`（与首次认证的 `build_labels.py::reading` 同一口径）。
fn reading(answer: &Json) -> Option<(f64, Option<usize>)> {
    if let Some(p) = answer["Noul"].as_f64() {
        return Some((p, None));
    }
    let v = answer["Choice"].as_array().or(answer["Score"].as_array())?;
    let (k, p) = v
        .iter()
        .filter_map(Json::as_f64)
        .enumerate()
        .max_by(|a, b| a.1.total_cmp(&b.1))?;
    Some((p, Some(k)))
}

struct Opts<'a> {
    backend: &'a str,
    fixtures: Option<&'a str>,
    profile: Option<&'a str>,
    model: Option<&'a str>,
}

fn sub(args: &[String], cwd: &Path) -> Result<(), String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let o = Proc::new(exe)
        .current_dir(cwd)
        .args(args)
        .output()
        .map_err(|e| e.to_string())?;
    if o.status.success() {
        Ok(())
    } else {
        Err(format!(
            "jpp {} 失败：{}{}",
            args.first().map_or("", String::as_str),
            String::from_utf8_lossy(&o.stderr),
            String::from_utf8_lossy(&o.stdout)
        ))
    }
}

/// 对一个条目重取读数并重认：成功返回新 `labels.jsonl` 与新 `calib/` 所在的临时目录（尚未替换），
/// 以及线变化来自画像 δ 迁移时的说明（Z0334；没有迁移为 `None`）。
fn recertify(plan: &Plan, o: &Opts, work: &Path) -> Result<(PathBuf, Option<String>), String> {
    let w = work.join(&plan.hash);
    std::fs::create_dir_all(&w).map_err(|e| e.to_string())?;
    let (report, ledger) = (w.join("report.json"), w.join("ledger.jsonl"));
    let mut a: Vec<String> = vec![
        "run".into(),
        plan.read.display().to_string(),
        "--input".into(),
        plan.input.display().to_string(),
        "--backend".into(),
        o.backend.into(),
        "--output".into(),
        report.display().to_string(),
        "--ledger-out".into(),
        ledger.display().to_string(),
    ];
    for (k, v) in [
        ("--fixtures", o.fixtures),
        ("--profile", o.profile),
        ("--model", o.model),
    ] {
        if let Some(v) = v {
            a.push(k.into());
            a.push(v.into());
        }
    }
    sub(&a, &w)?;
    let rep: Json = serde_json::from_slice(&std::fs::read(&report).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())?;
    let lg = jpp::Ledger::decode(&std::fs::read_to_string(&ledger).map_err(|e| e.to_string())?)
        .map_err(|e| format!("账本读不了：{e:?}"))?
        .0;
    // 读数 = 答案 + 置换测量（Z0308：两序读数的 `perm` 与 p、pick 一起写回标注行，否则认证时众数丢失）
    let mut by_key: std::collections::BTreeMap<String, (Json, Option<(usize, f64)>)> =
        Default::default();
    for e in &lg.entries {
        if let jpp_ledger::Entry::Judge {
            key, answer, perm, ..
        } = e
        {
            by_key.insert(
                key.clone(),
                (
                    serde_json::to_value(answer).map_err(|e| e.to_string())?,
                    perm.map(|m| (m.perms, m.mode_share)),
                ),
            );
        }
    }
    type 读数 = (f64, Option<usize>, Option<(usize, f64)>);
    let mut by_item: std::collections::BTreeMap<String, 读数> = Default::default();
    // 关系题式（F2）的读数行没有 `id`，只有 `a`、`b`；标注行的 `item` 是标注包里的不透明编号，但 `text`
    // 就是 `{"a","b"}` 的紧凑 JSON（运行时指纹用的 on_text）。所以没有 `id` 的行按这个文本回接标注行。
    let mut by_text: std::collections::BTreeMap<String, 读数> = Default::default();
    for r in rep["value"]["rows"].as_array().into_iter().flatten() {
        let Some(k) = r["key"].as_str() else { continue };
        let rd = by_key
            .get(k)
            .and_then(|(a, m)| reading(a).map(|(p, pick)| (p, pick, *m)))
            .ok_or_else(|| format!("材料 {r}：账本里没有键 {k} 的读数"))?;
        if let Some(id) = r["id"].as_str() {
            by_item.insert(id.to_string(), rd);
        } else if let (Some(a), Some(b)) = (r["a"].as_str(), r["b"].as_str()) {
            by_text.insert(json!({"a": a, "b": b}).to_string(), rd);
        }
    }
    // 旧记录的题类（B120 (a)）：入库时可能经 `--report` 取得（如 F4 的 cmp），重认不带报告，标注行也没写，
    // 导入会回落到基础类。缺 `kind` 的行搬运旧记录的题类（行上题类优先于基础类），重认不改题类（Z0308，主控定 A）
    let old_kind = old_record_kind(&plan.dir.join("calib"), &plan.hash)?;
    let mut lines: Vec<String> = vec![];
    for l in &plan.labels {
        if l.trim().is_empty() {
            continue;
        }
        let mut row: Json = serde_json::from_str(l).map_err(|e| format!("labels.jsonl：{e}"))?;
        if let (Some(k), Some(o)) = (&old_kind, row.as_object_mut()) {
            o.entry("kind").or_insert_with(|| k.clone());
        }
        if row["p"].is_number() {
            let item = row["item"].as_str().unwrap_or("").to_string();
            let (p, pick, perm) = by_item
                .get(&item)
                .or_else(|| row["text"].as_str().and_then(|t| by_text.get(t)))
                .ok_or_else(|| {
                    format!("标注行 {item} 没有对上新读数（读数程序的 rows 里没有这个 id，也没有文本相同的 a/b 对）")
                })?;
            row["p"] = json!(p);
            if row.get("pick").is_some() {
                let k = pick.ok_or_else(|| format!("{item}：原行有 pick，新读数不是选择答案"))?;
                row["pick"] = json!(k);
            }
            // 置换测量随新读数换：新读数测了就写，没测就去掉旧的（原来没有的行不加键，逐字节不变）
            match perm {
                Some((k, s)) => {
                    row["perms"] = json!(k);
                    row["mode_share"] = json!(s);
                }
                None => {
                    if let Some(o) = row.as_object_mut() {
                        o.remove("perms");
                        o.remove("mode_share");
                    }
                }
            }
        }
        lines.push(serde_json::to_string(&row).map_err(|e| e.to_string())?);
    }
    let out = w.join("entry");
    std::fs::create_dir_all(&out).map_err(|e| e.to_string())?;
    let labels = out.join("labels.jsonl");
    std::fs::write(&labels, lines.join("\n") + "\n").map_err(|e| e.to_string())?;
    let mut ci: Vec<String> = vec![
        "calib-import".into(),
        labels.display().to_string(),
        "--calib-out".into(),
        out.join("calib").display().to_string(),
    ];
    if let Some(p) = o.profile {
        ci.push("--profile".into());
        ci.push(p.into());
    }
    sub(&ci, &w)?;
    QuestionBank::verify_calib_at(&out, &plan.hash)
        .map_err(|e| format!("新读数重认后装载校验不过：{e}"))?;
    // 装载校验只查记录自洽；读数变差时认证会给出「不上岗」的记录也能装载，这里要求新记录仍是「上岗」
    for f in std::fs::read_dir(out.join("calib")).map_err(|e| e.to_string())? {
        let f = f.map_err(|e| e.to_string())?.path();
        if f.extension().is_some_and(|x| x == "json") {
            let r: Json = serde_json::from_slice(&std::fs::read(&f).map_err(|e| e.to_string())?)
                .map_err(|e| format!("{}: {e}", f.display()))?;
            if r["key"].as_str().is_some_and(|k| k.ends_with(&plan.hash))
                && r["status"].as_str() != Some("上岗")
            {
                return Err(format!(
                    "新读数认证后记录状态是「{}」，不是「上岗」：读数变差到认证不过",
                    r["status"].as_str().unwrap_or("?")
                ));
            }
        }
    }
    // 读数逐行没变而认证线变了：只有画像 δ 迁到中段能解释（Z0334，主控 2026-09-30 定：新旧 δ 不同，且用旧 δ
    // 按同一认证路径重算的线与旧记录在容差 1e-9 内相同，`line_drift` 的口径）；其余一律当认证路径问题，拒绝写回
    let mut 迁移 = None;
    if same_readings(&plan.labels, &lines)
        && let Some(d) = line_drift(&plan.dir.join("calib"), &out.join("calib"))? {
            match delta_migration(&plan.dir.join("calib"), &out.join("calib"), &labels, o, &w) {
                Ok(note) => 迁移 = Some(note),
                Err(why) => {
                    return Err(format!(
                        "认证结果与旧记录不一致，疑认证路径问题（读数逐行未变，认证线变了）：{d}；不是画像 δ 迁移：{why}"
                    ));
                }
            }
        }
    Ok((out, 迁移))
}

/// 把重认出的 `labels.jsonl` 与 `calib/` 换进条目目录（复查可后补 1：写回原子性）。先整份复制到条目目录里的
/// 暂存名，全部成功后再改名替换；复制阶段出错，条目原样不动、删掉暂存；改名阶段只有三次 `rename`，
/// 出错时报出条目目录里留下的是哪几个名字，由人按报文收拾（`calib.recert-old/` 是旧记录）。
fn replace_entry(dir: &Path, out: &Path) -> Result<(), String> {
    let (新标注, 新记录, 旧记录) = (
        dir.join("labels.jsonl.recert-new"),
        dir.join("calib.recert-new"),
        dir.join("calib.recert-old"),
    );
    let _ = std::fs::remove_file(&新标注);
    let _ = std::fs::remove_dir_all(&新记录);
    let _ = std::fs::remove_dir_all(&旧记录);
    let 暂存 = std::fs::copy(out.join("labels.jsonl"), &新标注)
        .map_err(|e| format!("暂存新标注：{e}"))
        .and_then(|_| copy_dir(&out.join("calib"), &新记录));
    if let Err(e) = 暂存 {
        let _ = std::fs::remove_file(&新标注);
        let _ = std::fs::remove_dir_all(&新记录);
        return Err(format!("{e}（条目未改动）"));
    }
    let 换 = |a: &Path, b: &Path| {
        std::fs::rename(a, b).map_err(|e| format!("{} → {}：{e}", a.display(), b.display()))
    };
    if dir.join("calib").exists() {
        换(&dir.join("calib"), &旧记录)?;
    }
    换(&新记录, &dir.join("calib")).map_err(|e| format!("{e}（旧记录在 calib.recert-old/）"))?;
    换(&新标注, &dir.join("labels.jsonl")).map_err(|e| {
        format!("{e}（calib/ 已换新，labels.jsonl 未换，新标注在 labels.jsonl.recert-new）")
    })?;
    let _ = std::fs::remove_dir_all(&旧记录);
    Ok(())
}

/// 新旧 `calib/` 各份记录的 δ 有变化时，给出「δ 迁移：记录：旧 → 新」的说明；都没变为 `None`（Z0334）。
fn delta_change(old: &Path, new: &Path) -> Result<Option<String>, String> {
    let read = |p: &Path| -> Result<Json, String> {
        serde_json::from_slice(&std::fs::read(p).map_err(|e| format!("{}: {e}", p.display()))?)
            .map_err(|e| format!("{}: {e}", p.display()))
    };
    let mut 各份: Vec<String> = vec![];
    for f in std::fs::read_dir(new).map_err(|e| format!("{}: {e}", new.display()))? {
        let f = f.map_err(|e| e.to_string())?.path();
        if f.extension().is_none_or(|x| x != "json") {
            continue;
        }
        let name = f
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();
        let b = record_delta(&read(&f)?);
        let a = match read(&old.join(&name)) {
            Ok(j) => record_delta(&j),
            Err(_) => None,
        };
        let 变了 = match (a, b) {
            (Some(x), Some(y)) => (x - y).abs() >= 1e-12,
            (x, y) => x.is_some() || y.is_some(),
        };
        if 变了 {
            let 显示 = |v: Option<f64>| v.map_or("无".to_string(), |x| x.to_string());
            各份.push(format!("{name}：δ {} → {}", 显示(a), 显示(b)));
        }
    }
    各份.sort();
    Ok((!各份.is_empty()).then(|| {
        format!(
            "δ 按新画像迁移（读数有变，旧 δ → 新 δ；{}）",
            各份.join("；")
        )
    }))
}

/// 一份认证记录用的 δ：顶层 `delta`，没有取 `unsure_rate_delta`，再没有取证书的 `selection.delta`（Z0334）。
fn record_delta(r: &Json) -> Option<f64> {
    r["delta"]
        .as_f64()
        .or_else(|| r["unsure_rate_delta"].as_f64())
        .or_else(|| {
            r["certs"]
                .as_object()?
                .values()
                .find_map(|c| c["selection"]["delta"].as_f64())
        })
}

/// 读数不变、线变了时，判线变化是否只来自画像 δ 迁到中段（Z0334）：新旧 δ 每份记录都不同，且把画像里记录所属
/// 题型那一列的中段 δ 换成旧 δ、同一份标注行按同一个 `calib-import` 重算出的记录，与旧记录的线在容差 1e-9 内
/// 相同（`line_drift` 的口径）。
/// 是则返回说明，否则说明哪条不满足。
fn delta_migration(
    old: &Path,
    new: &Path,
    labels: &Path,
    o: &Opts,
    w: &Path,
) -> Result<String, String> {
    let read = |p: &Path| -> Result<Json, String> {
        serde_json::from_slice(&std::fs::read(p).map_err(|e| format!("{}: {e}", p.display()))?)
            .map_err(|e| format!("{}: {e}", p.display()))
    };
    let mut 旧带宽: Option<f64> = None;
    let mut 各份: Vec<String> = vec![];
    // 旧 δ 只写回记录自己那一列（按样本的物理题型）：题型映射出错时重算取到别的列，对不上旧线，照样拒
    let mut 列: Vec<&'static str> = vec![];
    for f in std::fs::read_dir(old).map_err(|e| format!("{}: {e}", old.display()))? {
        let f = f.map_err(|e| e.to_string())?.path();
        if f.extension().is_none_or(|x| x != "json") {
            continue;
        }
        let name = f
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();
        let 旧 = read(&f)?;
        let a = record_delta(&旧).ok_or(format!("{name} 旧记录取不到 δ"))?;
        match 旧["samples"][0]["phys"].as_str() {
            Some("noul") => 列.push("noul"),
            Some("choice") => 列.push("choice_prob_chosen"),
            Some("score") => 列.push("score"),
            // 认不出题型就不知道旧 δ 该写哪一列；三列都写会让题型映射防护失效（复核 E2b），直接拒
            other => {
                return Err(format!(
                    "{name} 旧记录没有可认的题型（样本 phys = {other:?}），无法按旧 δ 复现"
                ));
            }
        }
        let b = record_delta(&read(&new.join(&name))?).ok_or(format!("{name} 新记录取不到 δ"))?;
        if (a - b).abs() < 1e-12 {
            return Err(format!("{name} 新旧 δ 相同（{a}）"));
        }
        match 旧带宽 {
            Some(x) if (x - a).abs() >= 1e-12 => {
                return Err(format!(
                    "条目里各记录的旧 δ 不同（{x}、{a}），无法用一份画像按旧 δ 复现"
                ));
            }
            _ => 旧带宽 = Some(a),
        }
        各份.push(format!("{name}：δ {a} → {b}"));
    }
    let 旧带宽 = 旧带宽.ok_or("旧记录目录里没有记录")?;
    let 画像 = o.profile.ok_or("没有画像路径，无法按旧 δ 重算")?;
    let mut j = read(Path::new(画像))?;
    for col in 列 {
        j["delta"][col]["mid"]["immediate"]["p99"] = serde_json::json!(旧带宽);
    }
    let 旧画像 = w.join("profile-old-delta.json");
    std::fs::write(&旧画像, j.to_string()).map_err(|e| e.to_string())?;
    let 重算 = w.join("old-delta").join("calib");
    sub(
        &[
            "calib-import".into(),
            labels.display().to_string(),
            "--calib-out".into(),
            重算.display().to_string(),
            "--profile".into(),
            旧画像.display().to_string(),
        ],
        w,
    )?;
    if let Some(d) = line_drift(old, &重算)? {
        return Err(format!(
            "用旧 δ {旧带宽} 按同一认证路径重算，线与旧记录对不上：{d}"
        ));
    }
    Ok(format!(
        "线变化来自画像 δ 迁到 mid（旧 δ → 新 δ；{}）",
        各份.join("；")
    ))
}

/// 条目 `calib/` 里本条目那份记录的 `kind`（没有记录或没写题类为 `None`）。
fn old_record_kind(dir: &Path, hash: &str) -> Result<Option<Json>, String> {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return Ok(None);
    };
    for f in rd {
        let f = f.map_err(|e| e.to_string())?.path();
        if f.extension().is_none_or(|x| x != "json") {
            continue;
        }
        let r: Json = serde_json::from_slice(&std::fs::read(&f).map_err(|e| e.to_string())?)
            .map_err(|e| format!("{}: {e}", f.display()))?;
        if r["key"].as_str().is_some_and(|k| k.ends_with(hash)) {
            return Ok(r.get("kind").filter(|k| !k.is_null()).cloned());
        }
    }
    Ok(None)
}

/// 新旧标注行里带 `p` 的行，`p` 与 `pick` 是否逐行相同（按行序比；重认不改行序与行数）。
fn same_readings(old: &[String], new: &[String]) -> bool {
    let rows = |v: &[String]| -> Vec<(Option<f64>, Option<u64>)> {
        v.iter()
            .filter(|l| !l.trim().is_empty())
            .filter_map(|l| serde_json::from_str::<Json>(l).ok())
            .map(|r| (r["p"].as_f64(), r["pick"].as_u64()))
            .collect()
    };
    let (a, b) = (rows(old), rows(new));
    a.len() == b.len()
        && a.iter().zip(&b).all(|(x, y)| {
            x.1 == y.1
                && match (x.0, y.0) {
                    (Some(p), Some(q)) => (p - q).abs() < 1e-12,
                    (None, None) => true,
                    _ => false,
                }
        })
}

/// 新旧认证记录（`calib/*.json` 顶层）的差别：`hi`、`lo`、`unsure_rate_delta`，以及旧 `delta` 非空时的 `delta`。
/// 旧 `delta` 为 `null` 的不算变（只是由空变成显式值）。没有差别返回 `None`，有则列出前后值。
fn line_drift(old: &Path, new: &Path) -> Result<Option<String>, String> {
    let mut diffs: Vec<String> = vec![];
    for f in std::fs::read_dir(old).map_err(|e| format!("{}: {e}", old.display()))? {
        let f = f.map_err(|e| e.to_string())?.path();
        if f.extension().is_none_or(|x| x != "json") {
            continue;
        }
        let name = f.file_name().unwrap_or_default().to_owned();
        let read = |p: &Path| -> Result<Json, String> {
            serde_json::from_slice(&std::fs::read(p).map_err(|e| format!("{}: {e}", p.display()))?)
                .map_err(|e| format!("{}: {e}", p.display()))
        };
        let o = read(&f)?;
        let Ok(n) = read(&new.join(&name)) else {
            diffs.push(format!("{}：新记录里没有这份文件", name.to_string_lossy()));
            continue;
        };
        for k in ["hi", "lo", "unsure_rate_delta", "delta"] {
            if k == "delta" && o[k].is_null() {
                continue;
            }
            let same = match (o[k].as_f64(), n[k].as_f64()) {
                (Some(a), Some(b)) => (a - b).abs() < 1e-9,
                _ => o[k] == n[k],
            };
            if !same {
                diffs.push(format!(
                    "{} 的 {k} 旧 {} → 新 {}",
                    name.to_string_lossy(),
                    o[k],
                    n[k]
                ));
            }
        }
    }
    Ok((!diffs.is_empty()).then(|| diffs.join("；")))
}

pub(super) fn run(bank: &mut QuestionBank, a: &Args, date: &str, who: &str) -> Result<(), String> {
    let backend = a.need("--backend")?;
    let reason = a.need("--reason")?.to_string();
    let mut targets: Vec<String> = vec![];
    for q in &a.pos {
        targets.push(bank.resolve(q)?);
    }
    if a.flag("--pending") {
        for h in bank.pending() {
            if !targets.contains(&h) {
                targets.push(h);
            }
        }
    }
    if targets.is_empty() {
        return Err(
            "recert 需要条目（form_hash、前缀或 slug），或 --pending（没有待重认的条目）".into(),
        );
    }
    // 先把所有条目的配方、文件与预计调用数核完：缺什么在花钱前报，一个都不跑
    let plans: Vec<Plan> = targets
        .iter()
        .map(|h| plan_of(bank, h))
        .collect::<Result<_, _>>()?;
    let calls: usize = plans.iter().map(|p| p.calls).sum();
    let cap: f64 = plans.iter().map(|p| p.budget_cost).sum();
    println!(
        "重认 {} 条，预计调用 {calls} 次；读数程序 budget 费用上限合计 ${cap}{}",
        plans.len(),
        a.one("--max-cost")
            .map_or(String::new(), |m| format!("，--max-cost ${m}"))
    );
    if !matches!(backend, "fixed" | "stub") {
        let max: f64 = a
            .one("--max-cost")
            .ok_or("真机后端要花钱：须给 --max-cost <美元>（不做交互确认）")?
            .parse()
            .map_err(|_| "--max-cost 要写数字（美元）".to_string())?;
        if cap > max {
            return Err(format!(
                "读数程序 budget 费用上限合计 ${cap} 超过 --max-cost ${max}，不跑"
            ));
        }
    }
    // 没给 `--profile` 时用默认画像：认证导入要 δ、只认 `--profile`，所以解析到的路径要传给子进程
    let default_profile: Option<String> = a
        .one("--profile")
        .is_none()
        .then(super::default_profile_path)
        .flatten()
        .map(|f| f.display().to_string());
    // 子进程的工作目录是临时目录：相对路径先转绝对路径，否则读不到文件
    let abs = |p: &str| {
        std::fs::canonicalize(p).map_or_else(|_| p.to_string(), |x| x.display().to_string())
    };
    let fixtures_abs = a.one("--fixtures").map(abs);
    let profile_abs = a.one("--profile").map(abs);
    let o = Opts {
        backend,
        fixtures: fixtures_abs.as_deref(),
        profile: profile_abs.as_deref().or(default_profile.as_deref()),
        model: a.one("--model"),
    };
    // 判断器身份与 `bank review` 同一条回退链（`--profile` → 可执行文件旁的默认画像）；取不到记 null，
    // 之后带画像复审时该条目会被列为「基线未含画像，需重认」（补缺 5）
    let (profile_hash, _) = super::judge_identity(a, &[])?;
    if profile_hash.is_none() {
        println!(
            "判断器身份未取到（未给 --profile，可执行文件旁没有默认画像）：重认记录的画像哈希记 null，之后带画像复审会列为待重认"
        );
    }
    let env = ReviewEnv {
        render_version: jpp_ir::key::RENDER_VERSION.to_string(),
        profile_hash,
    };
    let work = std::env::temp_dir().join(format!("jpp-bank-recert-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&work);
    let mut done: Vec<String> = vec![];
    let mut failed: Vec<String> = vec![];
    let mut 迁移说明: Vec<String> = vec![];
    for p in &plans {
        match recertify(p, &o, &work) {
            Ok((out, 迁移)) => {
                let dir = bank.entry_dir(&p.hash);
                // 读数变了的重认不进闸门，δ 照样按新画像迁移：写回前比较新旧记录的 δ，不同就记下来
                // （Z0334 复核阻断：条目说明承诺「变更记录写明 δ 迁移」，不论读数变没变）。
                // 这里和写回出错都只算这一条不过，不让整个 `run()` 返回：同批已写回的条目照常落账（复查可后补 1）
                let 写回 = match 迁移 {
                    Some(note) => Ok(Some(note)),
                    None => delta_change(&dir.join("calib"), &out.join("calib")),
                }
                .and_then(|迁移| replace_entry(&dir, &out).map(|()| 迁移));
                match 写回 {
                    Ok(Some(note)) => {
                        println!("{}：重认通过；{note}", p.slug);
                        迁移说明.push(format!("{}：{note}", p.slug));
                        done.push(p.hash.clone());
                    }
                    Ok(None) => {
                        println!("{}：重认通过", p.slug);
                        done.push(p.hash.clone());
                    }
                    Err(e) => {
                        println!("{}：重认已算出、写回失败：{e}", p.slug);
                        failed.push(p.slug.clone());
                    }
                }
            }
            Err(e) => {
                println!("{}：重认不过，条目保持原状：{e}", p.slug);
                failed.push(p.slug.clone());
            }
        }
    }
    let _ = std::fs::remove_dir_all(&work);
    if !done.is_empty() {
        bank.finish_recert(&done, &env)?;
        bank.append_change(
            date,
            "重认",
            &format!(
                "重取读数重认 {} 条：{}（后端 {backend}，预计调用 {calls} 次）；`labels.jsonl` 的读数与 `calib/` 记录已换{}",
                done.len(),
                done.join("、"),
                if 迁移说明.is_empty() {
                    String::new()
                } else {
                    format!("。{}", 迁移说明.join("；"))
                }
            ),
            &reason,
            &format!(
                "`recert` 清除，记 `recertified_at`；`bank.json` 版本 {}；未通过：{}",
                bank.version(),
                if failed.is_empty() {
                    "无".to_string()
                } else {
                    failed.join("、")
                }
            ),
            who,
        )?;
        bank.save()?;
    }
    if failed.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "{} 条重认不过：{}",
            failed.len(),
            failed.join("、")
        ))
    }
}
