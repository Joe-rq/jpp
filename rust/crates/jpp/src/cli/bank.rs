//! `jpp bank <动词>` 与 `jpp bank-stats`：题库生命周期与使用统计的命令行（`21` 步 27；`20` B48；规范 §二、§四）。
//!
//! 全部是离线宿主工具：运行时不写题库（B48）。每个写命令：改 `bank.json`（版本加一、`index_hash` 重算）、
//! 向 `题库/变更记录.md` 追加一条（只追加）。参数解析在本文件，`options.rs` 只把 `bank`、`bank-stats` 后面的
//! 参数原样交进来。

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command as Proc;

use jpp::store::bank::{Provenance, QuestionBank, ReviewEnv, Status, today};
use jpp::store::bank_stats::{self, BankStats};

mod recert;

const USAGE: &str = "\
jpp bank list [--bank <dir>]
jpp bank propose <slug> --source <text> --reason <text> [--kind <kind>] [--batch <id>] [--bank <dir>]
        [--derived-by <fill|refine|premise|elicit>] [--source-exit <ledger-key>] [--run-key <ledger-key>]   (a derived form: provenance)
jpp bank diagnose <form_hash|slug> [--waive <W-code>:<reason>]... [--bank <dir>]
jpp bank admit <form_hash|slug> --prereg <commit> --reviewer <name> --reason <text> [--bank <dir>]
jpp bank reject <form_hash|slug> --reason <text> [--bank <dir>]
jpp bank promote <form_hash|slug> --evidence <file.json> --reason <text> [--bank <dir>]
jpp bank split <form_hash|slug> --into <new1>,<new2>,... --reason <text> [--bank <dir>]
jpp bank merge <old1>,<old2>,... --into <new> --reason <text> [--bank <dir>]
jpp bank retire <form_hash|slug> --reason <text> [--bank <dir>]
jpp bank reinstate <form_hash|slug> --to <status> --reason <text> [--bank <dir>]
jpp bank review [--record [--waive <条目|判断器版本>:<理由>]...] [--mark-pending] [--suspend-candidates]
                [--ledger <ledger>]... [--profile <profile.json>] [--bank <dir>]
jpp bank recert <form_hash|slug>...|--pending --backend <name> [--fixtures <f>] [--profile <p>] [--model <m>]
                [--max-cost <usd>] --reason <text> [--bank <dir>]
jpp bank-stats <ledger>... [--bank <dir>] [--json]
common: --who <name> (default: jpp bank)";

#[derive(Default)]
struct Args {
    pos: Vec<String>,
    opts: BTreeMap<String, Vec<String>>,
    flags: Vec<String>,
}

const FLAG_ONLY: [&str; 4] = [
    "--record",
    "--suspend-candidates",
    "--mark-pending",
    "--pending",
];

fn parse_args(args: &[String]) -> Result<Args, String> {
    let mut a = Args::default();
    let mut i = 0;
    while i < args.len() {
        let x = &args[i];
        if FLAG_ONLY.contains(&x.as_str()) {
            a.flags.push(x.clone());
            i += 1;
        } else if x.starts_with("--") {
            let v = args
                .get(i + 1)
                .filter(|s| !s.starts_with("--"))
                .ok_or_else(|| format!("{x} requires a value"))?;
            a.opts.entry(x.clone()).or_default().push(v.clone());
            i += 2;
        } else {
            a.pos.push(x.clone());
            i += 1;
        }
    }
    Ok(a)
}

impl Args {
    fn one(&self, k: &str) -> Option<&str> {
        self.opts.get(k).and_then(|v| v.last()).map(String::as_str)
    }
    fn need(&self, k: &str) -> Result<&str, String> {
        self.one(k).ok_or_else(|| format!("缺 {k}\n\n{USAGE}"))
    }
    fn flag(&self, k: &str) -> bool {
        self.flags.iter().any(|f| f == k)
    }
    fn bank_dir(&self) -> PathBuf {
        PathBuf::from(self.one("--bank").unwrap_or("bank"))
    }
    fn who(&self) -> &str {
        self.one("--who").unwrap_or("jpp bank（命令行）")
    }
}

pub fn run(stats: bool, args: &[String]) -> Result<(), String> {
    if stats {
        return bank_stats_cmd(&parse_args(args)?);
    }
    let Some((verb, rest)) = args.split_first() else {
        return Err(USAGE.into());
    };
    let a = parse_args(rest)?;
    let mut bank = QuestionBank::open(&a.bank_dir())?;
    let date = today();
    let who = a.who().to_string();
    match verb.as_str() {
        "list" => list(&bank),
        "propose" => {
            let slug = a.pos.first().ok_or("propose 需要 <slug>")?.clone();
            let source = a.need("--source")?.to_string();
            let reason = a.need("--reason")?.to_string();
            let lib = bank
                .dir()
                .join("..")
                .join("lib/bank")
                .join(format!("{slug}.jpp"));
            let (h, op) = form_of(&lib, &slug)?;
            let kind = a.one("--kind").unwrap_or("unspecified");
            let batch = a.one("--batch").unwrap_or("");
            // 派生来源（步 28，主控:B0468）：给了任一个就写进条目的正式字段 provenance.*（B45 推翻条件按它统计）
            let prov = derived_provenance(&a)?;
            match prov {
                None => bank.propose_as(&who, &slug, &h, &op, kind, batch, &source)?,
                Some(p) => bank.propose_with_as(&who, &slug, &h, &op, kind, batch, &source, &p)?,
            }
            log(
                &bank,
                &date,
                "新增（提出）",
                &format!("提出题式 `{slug}`（`{h}`，{op}），状态「提出」；来源：{source}"),
                &reason,
                &format!(
                    "`lib/bank/{slug}.jpp`；条目目录 `bank/entries/{h}/`；`bank.json` 版本 {}",
                    bank.version()
                ),
                &who,
            )?;
            bank.save()?;
            println!("提出 {slug} → {h}（bank 版本 {}）", bank.version());
            Ok(())
        }
        "diagnose" => {
            let h = bank.resolve(a.pos.first().ok_or("diagnose 需要 <form_hash|slug>")?)?;
            let slug = bank
                .find(&h)
                .and_then(|e| e["slug"].as_str())
                .unwrap_or("")
                .to_string();
            let lib = bank
                .dir()
                .join("..")
                .join("lib/bank")
                .join(format!("{slug}.jpp"));
            let warnings = static_diagnosis(&lib, &slug)?;
            let waived: Vec<(String, String)> = a
                .opts
                .get("--waive")
                .map(|v| {
                    v.iter()
                        .filter_map(|s| s.split_once(':'))
                        .map(|(c, r)| (c.to_string(), r.to_string()))
                        .collect()
                })
                .unwrap_or_default();
            let open = bank.record_diagnosis(&h, &warnings, &waived)?;
            for (c, m) in &warnings {
                let mark = if waived.iter().any(|(w, _)| w == c) {
                    "豁免"
                } else {
                    "未过"
                };
                println!("[{mark}] {c}: {m}");
            }
            let st = bank.status_of(&h).map_or("?", Status::as_str);
            log(
                &bank,
                &date,
                "改状态（静态诊断）",
                &format!(
                    "`{slug}`（`{h}`）静态诊断：{} 条告警，{} 条未被豁免，状态现为「{st}」",
                    warnings.len(),
                    open.len()
                ),
                "生命周期第 2 步（规范 §二）：上库前过静态诊断，每条告警改题面或有书面理由",
                &format!(
                    "`bank.json` 版本 {}；豁免：{}",
                    bank.version(),
                    waived
                        .iter()
                        .map(|(c, r)| format!("{c}（{r}）"))
                        .collect::<Vec<_>>()
                        .join("；")
                ),
                &who,
            )?;
            bank.save()?;
            println!("{slug}：{st}");
            Ok(())
        }
        "admit" => {
            let h = bank.resolve(a.pos.first().ok_or("admit 需要 <form_hash|slug>")?)?;
            let prereg = a.need("--prereg")?.to_string();
            let reviewer = a.need("--reviewer")?.to_string();
            let reason = a.need("--reason")?.to_string();
            bank.admit(&h, &prereg, &reviewer)?;
            log(
                &bank,
                &date,
                "改状态（上岗）",
                &format!(
                    "`{h}` 上岗，状态「已认证·未复用」；预注册 {prereg}（已核是本仓库的提交）；复审人 {reviewer}；装载时重跑认证通过"
                ),
                &reason,
                &format!("`bank.json` 版本 {}", bank.version()),
                &who,
            )?;
            bank.save()?;
            println!("{h}：已认证·未复用（bank 版本 {}）", bank.version());
            Ok(())
        }
        "reject" => simple(
            &mut bank,
            &a,
            &date,
            &who,
            "reject",
            |b, h, r| b.reject(h, r),
            "改状态（拒绝）",
        ),
        "retire" => simple(
            &mut bank,
            &a,
            &date,
            &who,
            "retire",
            |b, h, r| b.retire(h, r),
            "退役",
        ),
        "promote" => {
            let h = bank.resolve(a.pos.first().ok_or("promote 需要 <form_hash|slug>")?)?;
            let ev = PathBuf::from(a.need("--evidence")?);
            let reason = a.need("--reason")?.to_string();
            let n = bank.promote(&h, &ev)?;
            log(
                &bank,
                &date,
                "改状态（升共享）",
                &format!(
                    "`{h}` 升「共享」；复用证据 {n} 个独立目的（{}）",
                    ev.display()
                ),
                &reason,
                &format!("`bank.json` 版本 {}", bank.version()),
                &who,
            )?;
            bank.save()?;
            println!("{h}：共享（{n} 个独立目的）");
            Ok(())
        }
        "split" | "merge" => {
            let (olds, news): (Vec<String>, Vec<String>) = if verb == "split" {
                let o = bank.resolve(a.pos.first().ok_or("split 需要 <form_hash|slug>")?)?;
                (vec![o], list_arg(&bank, a.need("--into")?)?)
            } else {
                (
                    list_arg(&bank, a.pos.first().ok_or("merge 需要 <old1>,<old2>")?)?,
                    list_arg(&bank, a.need("--into")?)?,
                )
            };
            let reason = a.need("--reason")?.to_string();
            bank.supersede(&olds, &news)?;
            log(
                &bank,
                &date,
                if verb == "split" { "拆分" } else { "合并" },
                &format!(
                    "旧条目 {} 记「被取代」，由 {} 取代",
                    olds.join("、"),
                    news.join("、")
                ),
                &reason,
                &format!("旧条目目录保留；`bank.json` 版本 {}", bank.version()),
                &who,
            )?;
            bank.save()?;
            println!("{verb}：{} → {}", olds.join(","), news.join(","));
            Ok(())
        }
        "reinstate" => {
            let h = bank.resolve(a.pos.first().ok_or("reinstate 需要 <form_hash|slug>")?)?;
            let to = Status::parse(a.need("--to")?)
                .ok_or("--to 要写状态名，如 已认证·未复用 或 共享")?;
            let reason = a.need("--reason")?.to_string();
            bank.reinstate(&h, to)?;
            log(
                &bank,
                &date,
                "改状态（恢复上岗）",
                &format!(
                    "`{h}` 由停岗候选恢复为「{}」（人确认保留，calib-confirm --keep 之后）",
                    to.as_str()
                ),
                &reason,
                &format!("`bank.json` 版本 {}", bank.version()),
                &who,
            )?;
            bank.save()?;
            println!("{h}：{}", to.as_str());
            Ok(())
        }
        "review" => review(&mut bank, &a, &date, &who),
        "recert" => recert::run(&mut bank, &a, &date, &who),
        other => Err(format!("unknown bank verb '{other}'\n\n{USAGE}")),
    }
}

fn simple(
    bank: &mut QuestionBank,
    a: &Args,
    date: &str,
    who: &str,
    verb: &str,
    op: impl Fn(&mut QuestionBank, &str, &str) -> Result<(), String>,
    kind: &str,
) -> Result<(), String> {
    let h = bank.resolve(
        a.pos
            .first()
            .ok_or(format!("{verb} 需要 <form_hash|slug>"))?,
    )?;
    let reason = a.need("--reason")?.to_string();
    op(bank, &h, &reason)?;
    log(
        bank,
        date,
        kind,
        &format!("`{h}` 状态记「退役」（{verb}）；条目目录保留以供追溯"),
        &reason,
        &format!(
            "`bank.json` 版本 {}；`bank-stats` 的在岗汇总少一条，历史仍列出",
            bank.version()
        ),
        who,
    )?;
    bank.save()?;
    println!("{h}：退役（bank 版本 {}）", bank.version());
    Ok(())
}

fn list_arg(bank: &QuestionBank, s: &str) -> Result<Vec<String>, String> {
    s.split(',').map(|x| bank.resolve(x.trim())).collect()
}

fn log(
    bank: &QuestionBank,
    date: &str,
    kind: &str,
    what: &str,
    why: &str,
    impact: &str,
    who: &str,
) -> Result<(), String> {
    bank.append_change(date, kind, what, why, impact, who)
}

/// `propose` 的派生来源参数：`--derived-by`（两套名字都收，按 [`Provenance::canonical_derived_by`] 归一）、
/// `--source-exit`、`--run-key`；都没给返回 `None`（手写题式）。
fn derived_provenance(a: &Args) -> Result<Option<Provenance>, String> {
    let (by, exit, run) = (
        a.one("--derived-by"),
        a.one("--source-exit"),
        a.one("--run-key"),
    );
    if by.is_none() && exit.is_none() && run.is_none() {
        return Ok(None);
    }
    let by = by.ok_or("给了 --source-exit / --run-key 就要给 --derived-by（派生方式）")?;
    let canon = Provenance::canonical_derived_by(by).ok_or_else(|| {
        format!("--derived-by 不认得「{by}」：fill / refine / premise / elicit（或 pick_then_fill / refine_partition / premise_negation / gen）")
    })?;
    Ok(Some(Provenance {
        run_ledger_key: run.map(str::to_string),
        source_exit_key: exit.map(str::to_string),
        derived_by: Some(canon.to_string()),
    }))
}

fn list(bank: &QuestionBank) -> Result<(), String> {
    println!("题库版本 {}，{} 条", bank.version(), bank.entries().len());
    for e in bank.entries() {
        // 派生条目在行尾加一列派生方式（步 28）；手写条目的行不变
        let by = e["provenance"]["derived_by"]
            .as_str()
            .map_or(String::new(), |b| format!("  derived_by {b}"));
        println!(
            "{}  {:<18} {}  {}{by}",
            e["form_hash"].as_str().unwrap_or("?"),
            e["slug"].as_str().unwrap_or("?"),
            e["op"].as_str().unwrap_or("?"),
            e["status"].as_str().unwrap_or("?"),
        );
    }
    Ok(())
}

// ---- 求题式哈希与静态诊断：用同一个二进制跑一个只 import 题式文件的小程序 ----

fn rel_path(from_dir: &Path, to: &Path) -> Result<PathBuf, String> {
    let f = from_dir
        .canonicalize()
        .map_err(|e| format!("{}: {e}", from_dir.display()))?;
    let t = to
        .canonicalize()
        .map_err(|e| format!("{}: {e}", to.display()))?;
    let (fc, tc): (Vec<_>, Vec<_>) = (f.components().collect(), t.components().collect());
    let n = fc.iter().zip(&tc).take_while(|(a, b)| a == b).count();
    let mut out = PathBuf::new();
    for _ in n..fc.len() {
        out.push("..");
    }
    for c in &tc[n..] {
        out.push(c);
    }
    Ok(out)
}

fn temp_program(lib: &Path, slug: &str, body: &str) -> Result<(PathBuf, PathBuf), String> {
    if !lib.exists() {
        return Err(format!("{}：题式文件不存在", lib.display()));
    }
    let dir = std::env::temp_dir().join(format!("jpp-bank-{}-{slug}", std::process::id()));
    std::fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let rel = rel_path(&dir, lib)?;
    let src = format!(
        "import \"{}\";\nbudget {{calls: 1, cost: 0.01, depth: 8}};\n{body}\n",
        rel.to_string_lossy().replace('\\', "/")
    );
    let prog = dir.join("h.jpp");
    std::fs::write(&prog, src).map_err(|e| format!("{}: {e}", prog.display()))?;
    Ok((dir, prog))
}

/// 求 `lib/bank/<slug>.jpp` 里题式的 `(form_hash, op)`。
fn form_of(lib: &Path, slug: &str) -> Result<(String, String), String> {
    let (dir, prog) = temp_program(lib, slug, &format!("{{h: {slug}.hash, op: {slug}.op}}"))?;
    let out = dir.join("r.json");
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let o = Proc::new(exe)
        .current_dir(&dir)
        .args([
            "run",
            prog.to_str().unwrap_or(""),
            "--output",
            out.to_str().unwrap_or(""),
        ])
        .output()
        .map_err(|e| e.to_string())?;
    let r = (|| -> Result<(String, String), String> {
        if !o.status.success() {
            return Err(format!(
                "求题式哈希失败：{}",
                String::from_utf8_lossy(&o.stderr)
            ));
        }
        let j: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&out).map_err(|e| e.to_string())?)
                .map_err(|e| e.to_string())?;
        let h = j["value"]["h"]
            .as_str()
            .ok_or("报告里没有 value.h")?
            .to_string();
        let op = j["value"]["op"]
            .as_str()
            .ok_or("报告里没有 value.op")?
            .to_string();
        Ok((h, op))
    })();
    let _ = std::fs::remove_dir_all(&dir);
    r
}

/// 对题式文件跑现有静态诊断（`jpp check --json` 同一路径），收 `W-diag-*`。运行期闸门（步 26）落地后，
/// 把闸门的告警并进这里返回的列表即可（`QuestionBank::record_diagnosis` 只认 `(码, 文)`）。
fn static_diagnosis(lib: &Path, slug: &str) -> Result<Vec<(String, String)>, String> {
    let (dir, prog) = temp_program(lib, slug, &format!("{{h: {slug}.hash}}"))?;
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let o = Proc::new(exe)
        .current_dir(&dir)
        .args(["check", "--json", prog.to_str().unwrap_or("")])
        .output()
        .map_err(|e| e.to_string())?;
    let _ = std::fs::remove_dir_all(&dir);
    let j: serde_json::Value = serde_json::from_slice(&o.stdout).map_err(|e| {
        format!(
            "check --json 输出不是 JSON：{e}；{}",
            String::from_utf8_lossy(&o.stderr)
        )
    })?;
    Ok(j["diagnostics"]
        .as_array()
        .map(|a| {
            a.iter()
                .filter_map(|d| {
                    let c = d["code"].as_str()?;
                    c.starts_with("W-diag").then(|| {
                        (
                            c.to_string(),
                            d["message"].as_str().unwrap_or("").to_string(),
                        )
                    })
                })
                .collect()
        })
        .unwrap_or_default())
}

// ---- 使用统计 ----

fn read_ledgers(paths: &[String]) -> Result<Vec<jpp::Ledger>, String> {
    paths
        .iter()
        .map(|p| {
            let text = std::fs::read_to_string(p).map_err(|e| format!("{p}: {e}"))?;
            let (l, _, _) = jpp::store::migrations::ledger_v2::read_any(&text)
                .map_err(|e| format!("{p}: {e}"))?;
            Ok(l)
        })
        .collect()
}

fn stats_of(bank: &QuestionBank, ledgers: &[jpp::Ledger]) -> BankStats {
    let roster: Vec<String> = bank
        .entries()
        .iter()
        .filter_map(|e| e["form_hash"].as_str().map(str::to_string))
        .collect();
    bank_stats::aggregate(ledgers, &roster)
}

fn bank_stats_cmd(a: &Args) -> Result<(), String> {
    if a.pos.is_empty() {
        return Err(format!("bank-stats 需要至少一份账本\n\n{USAGE}"));
    }
    let bank = QuestionBank::open(&a.bank_dir())?;
    let ledgers = read_ledgers(&a.pos)?;
    let st = stats_of(&bank, &ledgers);
    let non_service = st.non_service_in_use(|h| bank.status_of(h));
    let suspend_in_use = st.suspend_candidates_in_use(|h| bank.status_of(h));
    let in_use_json = |v: &[bank_stats::InUse]| -> serde_json::Value {
        v.iter()
            .map(|u| {
                serde_json::json!({
                    "form_hash": u.form_hash,
                    "slug": bank.find(&u.form_hash).map_or(serde_json::Value::Null, |e| e["slug"].clone()),
                    "status": u.status.as_str(), "calls": u.calls, "reused": u.reused,
                })
            })
            .collect()
    };
    if crate::diag_json::json_mode() {
        let mut forms = serde_json::Map::new();
        for (h, s) in &st.forms {
            let e = bank.find(h);
            let mut j = s.to_json();
            j["slug"] = e.map_or(serde_json::Value::Null, |e| e["slug"].clone());
            j["status"] = e.map_or(serde_json::Value::Null, |e| e["status"].clone());
            forms.insert(h.clone(), j);
        }
        let outside: serde_json::Map<_, _> = st
            .outside
            .iter()
            .map(|(h, s)| (h.clone(), s.to_json()))
            .collect();
        println!(
            "{}",
            serde_json::json!({
                "bank_version": bank.version(), "ledgers": st.ledgers, "forms": forms,
                "outside_bank": outside,
                "non_service_in_use": in_use_json(&non_service),
                "suspend_candidates_in_use": in_use_json(&suspend_in_use),
                "unattributed": {"calls": st.unattributed_calls, "reused": st.unattributed_reused,
                                 "note": "无题式归属：判断条目没有 calib_ref.key（手写题或旧账本），不猜"},
            })
        );
        return Ok(());
    }
    println!("题库版本 {}；账本 {} 份", bank.version(), st.ledgers);
    println!(
        "{:<18} {:<10} {:>5} {:>6} {:>8} {:>4} {:>5}  出口 / 带内 / 漂移",
        "题式", "状态", "调用", "复用", "费用$", "运行", "站点"
    );
    for (h, s) in &st.forms {
        let e = bank.find(h);
        let exits = s
            .exits
            .iter()
            .map(|(k, v)| format!("{k}:{v}"))
            .collect::<Vec<_>>()
            .join(" ");
        println!(
            "{:<18} {:<10} {:>5} {:>6} {:>8.5} {:>4} {:>5}  {} / {} / {}",
            e.and_then(|e| e["slug"].as_str()).unwrap_or(h),
            e.and_then(|e| e["status"].as_str()).unwrap_or("?"),
            s.calls,
            s.reused,
            s.cost,
            s.runs,
            s.sites,
            if exits.is_empty() { "-".into() } else { exits },
            s.in_band_share().map_or("-".into(), |x| format!("{x:.3}")),
            s.drift(),
        );
    }
    for (h, s) in &st.outside {
        println!(
            "名单外题式 {h}：调用 {} 复用 {} 费用 {:.5}",
            s.calls, s.reused, s.cost
        );
    }
    for u in &non_service {
        println!(
            "非在岗仍被引用：{}（{}）调用 {} 复用 {}——只如实报，不拦程序",
            bank.find(&u.form_hash)
                .and_then(|e| e["slug"].as_str())
                .unwrap_or(&u.form_hash),
            u.status.as_str(),
            u.calls,
            u.reused
        );
    }
    if non_service.is_empty() {
        println!("非在岗仍被引用：无");
    }
    for u in &suspend_in_use {
        println!(
            "停岗候选仍供线：{}（B25：人确认前仍在岗）调用 {} 复用 {}",
            bank.find(&u.form_hash)
                .and_then(|e| e["slug"].as_str())
                .unwrap_or(&u.form_hash),
            u.calls,
            u.reused
        );
    }
    println!(
        "无题式归属的判断条目：调用 {}，复用 {}（手写题或旧账本，不猜归属）",
        st.unattributed_calls, st.unattributed_reused
    );
    let live = bank
        .entries()
        .iter()
        .filter(|e| {
            e["status"]
                .as_str()
                .and_then(Status::parse)
                .is_some_and(Status::in_service)
        })
        .count();
    println!(
        "在岗条目 {live}，未被引用 {}",
        st.forms
            .iter()
            .filter(|(h, s)| s.readings() == 0 && bank.status_of(h).is_some_and(Status::in_service))
            .count()
    );
    Ok(())
}

// ---- 复审 ----

/// 判断器身份（画像哈希）的来源，依次：`--profile`；`--ledger` 账本头的 `profile_hash`（多份须一致）；
/// 与 `jpp run` 同一规则的默认画像（可执行文件旁 `profiles/<默认模型>.json`，找得到才用）。
/// 都取不到返回 `None`，由复审报「未核判断器版本」；账本之间不一致时另返回说明，逐份列出。
fn judge_identity(
    a: &Args,
    ledgers: &[jpp::Ledger],
) -> Result<(Option<String>, Option<String>), String> {
    if let Some(p) = a.one("--profile") {
        return Ok((
            crate::profile_resolve::load(Path::new(p))?
                .profile
                .hash
                .clone(),
            None,
        ));
    }
    let paths = a.opts.get("--ledger").map_or(&[][..], Vec::as_slice);
    let heads: Vec<(&str, Option<String>)> = ledgers
        .iter()
        .zip(paths)
        .map(|(l, p)| {
            (
                p.as_str(),
                l.header
                    .as_ref()
                    .and_then(|h| h.compared.profile_hash.clone()),
            )
        })
        .collect();
    let mut distinct: Vec<&String> = heads.iter().filter_map(|(_, h)| h.as_ref()).collect();
    distinct.sort();
    distinct.dedup();
    match distinct.as_slice() {
        [one] => return Ok((Some((*one).clone()), None)),
        [] => {}
        _ => {
            let list = heads
                .iter()
                .map(|(p, h)| format!("{p}={}", h.as_deref().unwrap_or("（无）")))
                .collect::<Vec<_>>()
                .join("；");
            return Ok((
                None,
                Some(format!("各账本头的判断器画像哈希不一致：{list}")),
            ));
        }
    }
    if let Some(f) = default_profile_path() {
        return Ok((crate::profile_resolve::load(&f)?.profile.hash.clone(), None));
    }
    Ok((None, None))
}

/// 与 `jpp run` 同一规则的默认画像：可执行文件旁 `profiles/<默认模型>.json`，找得到才返回。
/// `judge_identity` 与 `recert`（把它当 `--profile` 传给子进程）共用。
fn default_profile_path() -> Option<PathBuf> {
    let spec = jpp::backends::by_name("live")?;
    let exe = std::env::current_exe().ok()?;
    let f = exe
        .parent()?
        .join("profiles")
        .join(format!("{}.json", spec.default_model));
    f.is_file().then_some(f)
}

/// `--waive <项>:<理由>`：项是条目（`form_hash`、前缀或 slug）或「判断器版本」；理由必填。
fn parse_waives(bank: &QuestionBank, a: &Args) -> Result<Vec<(String, String)>, String> {
    let mut out = vec![];
    for w in a.opts.get("--waive").into_iter().flatten() {
        let (what, why) = w
            .split_once(':')
            .filter(|(_, r)| !r.trim().is_empty())
            .ok_or_else(|| format!("--waive {w}：写成 <条目|判断器版本>:<理由>，理由必填"))?;
        let key = if what == "判断器版本" {
            what.to_string()
        } else {
            bank.resolve(what)?
        };
        out.push((key, why.trim().to_string()));
    }
    Ok(out)
}

fn review(bank: &mut QuestionBank, a: &Args, date: &str, who: &str) -> Result<(), String> {
    let ledgers = read_ledgers(a.opts.get("--ledger").map_or(&[][..], Vec::as_slice))?;
    let (profile_hash, conflict) = judge_identity(a, &ledgers)?;
    let env = ReviewEnv {
        render_version: jpp_ir::key::RENDER_VERSION.to_string(),
        profile_hash,
    };
    let waives = parse_waives(bank, a)?;
    let st = stats_of(bank, &ledgers);
    let drift: Vec<String> = st
        .forms
        .iter()
        .filter(|(_, s)| s.drift() == "signal")
        .map(|(h, _)| h.clone())
        .collect();
    let mut rep = bank.review(&env, &drift);
    let print = |rep: &jpp::store::bank::ReviewReport| {
        for r in &rep.due_all {
            println!("全库到期：{r}");
        }
        if rep.hand_edited {
            println!(
                "索引与记录的 index_hash 不符：bank.json 的条目被绕过命令改过、没有升版本（B48：改了内容必须升版本并记变更）"
            );
        }
        for (h, ps) in &rep.entries {
            for p in ps {
                println!("{h}：{p}");
            }
        }
        if let Some(j) = &rep.judge_unchecked {
            println!("未核判断器版本：{j}");
        }
        for (what, why) in &rep.prior_waived {
            println!("上次基线带豁免：{what}（{why}）");
        }
        if let Some(n) = &rep.baseline_note {
            println!("判断器基线：{n}");
        }
        if rep.is_clean() && rep.judge_unchecked.is_none() {
            println!("复审：无到期、无问题");
        }
    };
    if let Some(c) = &conflict {
        println!("{c}");
    }
    print(&rep);
    if a.flag("--suspend-candidates") {
        for h in &drift {
            bank.suspend_candidate(h)?;
            bank.append_change(
                date,
                "改状态（停岗候选）",
                &format!("`{h}` 在用带内率出现漂移信号，转停岗候选"),
                "规范 §四·复审 2、B25：漂移信号触发复审，系统只标候选，正式停岗由人确认（`calib-confirm`）",
                &format!("`bank.json` 版本 {}；校准记录另需 `jpp calib-confirm` 确认", bank.version()),
                who,
            )?;
            println!("{h}：停岗候选");
        }
        if !drift.is_empty() {
            bank.save()?;
        }
    }
    if a.flag("--mark-pending") {
        if rep.hand_edited {
            return Err("bank.json 有手改（index_hash 不符）：先处理，再标待重认".into());
        }
        if !rep.env_changed {
            return Err(
                "环境相对基线没有变（render_version 与判断器画像哈希都未变），无需标待重认".into(),
            );
        }
        let why = rep.due_all.join("；");
        let hs = bank.mark_pending(&env, &why);
        if !hs.is_empty() {
            bank.append_change(
                date,
                "标待重认",
                &format!("{} 条在岗条目标「待重认」（`recert` 字段）：{}", hs.len(), hs.join("、")),
                &format!("B48：复审周期 = render_version 或判断器版本变化后全库重认。原因：{why}"),
                &format!(
                    "条目仍在岗、仍供线（不拦程序）；须 `jpp bank recert --pending` 重取读数重认；`bank.json` 版本 {}",
                    bank.version()
                ),
                who,
            )?;
            bank.save()?;
        }
        println!("待重认：{} 条", hs.len());
        rep = bank.review(&env, &drift);
    }
    if a.flag("--record") {
        // 门槛（补缺 1）：手改不可豁免；逐条问题（含待重认）与判断器「未核」可被显式豁免，理由留档
        let mut blockers: Vec<String> = vec![];
        if rep.hand_edited {
            blockers.push("bank.json 的条目被绕过命令手改过（index_hash 不符），不可豁免".into());
        }
        let mut used: Vec<(String, String)> = vec![];
        for (h, ps) in &rep.entries {
            if let Some(w) = waives.iter().find(|(k, _)| k == h) {
                used.push(w.clone());
            } else {
                for p in ps {
                    blockers.push(format!("{h}：{p}"));
                }
            }
        }
        if let Some(j) = &rep.judge_unchecked {
            if let Some(w) = waives.iter().find(|(k, _)| k == "判断器版本") {
                used.push(w.clone());
            } else {
                blockers.push(format!(
                    "未核判断器版本：{j}（核不了不能当成核过；给 --profile 或 --ledger，或 --waive 判断器版本:<理由>）"
                ));
            }
        }
        if !blockers.is_empty() {
            println!("拒绝写复审基线，拦下的原因：");
            for b in &blockers {
                println!("  - {b}");
            }
            return Err(format!("复审基线未写（{} 项未清）", blockers.len()));
        }
        bank.record_review_waived(&env, &used);
        bank.append_change(
            date,
            "记复审基线",
            &format!(
                "复审基线：render_version {}，判断器画像 {}；显式豁免：{}",
                env.render_version,
                env.profile_hash.as_deref().unwrap_or("（未核）"),
                if used.is_empty() {
                    "无".to_string()
                } else {
                    used.iter()
                        .map(|(k, r)| format!("{k}（{r}）"))
                        .collect::<Vec<_>>()
                        .join("；")
                }
            ),
            "B48 复审周期：问题清完（或逐条显式豁免并记理由）才写新基线",
            &format!(
                "`bank.json` 版本仍为 {}（基线不改条目内容）",
                bank.version()
            ),
            who,
        )?;
        bank.save()?;
        println!(
            "已记录复审基线：render_version {}，bank 版本 {}",
            env.render_version,
            bank.version()
        );
    }
    Ok(())
}
