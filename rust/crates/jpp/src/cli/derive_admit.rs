//! `jpp derive-admit`：出题机制 derive 的留出比较与上岗（`21` 步 28；`20` B45；主控:B0468）。
//!
//! B45：由数据打分选出的题面必须在留出集上不劣于原版才上岗（与 J-16 训练集与保形集不相交同一纪律）。
//! 「原版」按派生方式定（过程记录 `工程-步28.md` §1.5）：唤出的题面变体比来源题面，划分细化比原题的直接回答；
//! 先选后填与前提反面没有原版，只过闸门，不走本命令。
//!
//! 离线宿主工具，与 `jpp bank` 同类：运行时不写题库（B48）。结果经题库 API（`record_holdout`、`reject`、`admit`）
//! 写进条目，每次写都追加变更记录。行怎么来（跑两个版本的程序、取出口、配真值）由调用者负责，本命令只比。

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use jpp::store::bank::{Provenance, QuestionBank, Status, today};
use jpp_value::stat::n_needed_zero_error;
use serde_json::{Value as Json, json};

const USAGE: &str = "\
jpp derive-admit <form_hash|slug> --rows <rows.jsonl> --split <method> --seed <n>
        [--derived-by <refine|elicit>] [--alpha <a>] [--conf-delta <d>] [--prereg <commit> --reviewer <name>]
        --reason <text> [--who <name>] [--bank <dir>]
The entry must have been proposed as a derived form (`jpp bank propose … --derived-by refine|elicit`);
its provenance.derived_by decides what the original is. --derived-by, if given, must agree with it.
rows: one JSON object per line {\"item\", \"set\": \"score\"|\"holdout\", \"truth\", \"orig\", \"cand\"}
  (score rows need only item and set). truth: true/false for a yes/no question, the block index for select
  and measure. orig / cand: act, ignore, pick(k), at(l); anything starting with unsure is undecided.
  Both exits are in the original question's answer space.";

/// α、δ 缺省：与 `calib-import` 的缺省同值（`options.rs` `parse_import`；单元测试钉住两处相同）。
pub const ALPHA_DEFAULT: f64 = 0.1;
pub const CONF_DELTA_DEFAULT: f64 = 0.1;

/// 一个出口对真值：`None` = 未决；`Some(对不对)`。出口写不认得的形状即报错。
pub fn score_exit(exit: &str, truth: &Json) -> Result<Option<bool>, String> {
    let e = exit.trim();
    if e.starts_with("unsure") {
        return Ok(None);
    }
    match e {
        "act" | "ignore" => {
            let t = truth
                .as_bool()
                .ok_or_else(|| format!("出口 {e} 是是非题的，真值要是 true/false，收到 {truth}"))?;
            Ok(Some((e == "act") == t))
        }
        _ => {
            let inner = e
                .strip_prefix("pick(")
                .or_else(|| e.strip_prefix("at("))
                .and_then(|s| s.strip_suffix(')'))
                .ok_or_else(|| {
                    format!("不认得的出口「{e}」：写 act、ignore、pick(k)、at(l) 或 unsure…")
                })?;
            let k: u64 = inner
                .trim()
                .parse()
                .map_err(|_| format!("出口「{e}」的下标不是非负整数"))?;
            let t = truth.as_u64().ok_or_else(|| {
                format!("出口 {e} 是 K 选一 / 程度题的，真值要是块下标，收到 {truth}")
            })?;
            Ok(Some(k == t))
        }
    }
}

/// 一边在留出集上的计数。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Tally {
    pub decided: usize,
    pub errors: usize,
}

impl Tally {
    fn add(&mut self, s: Option<bool>) {
        if let Some(ok) = s {
            self.decided += 1;
            if !ok {
                self.errors += 1;
            }
        }
    }
    fn to_json(self) -> Json {
        json!({"decided": self.decided, "errors": self.errors})
    }
}

/// 比较结果。
#[derive(Debug, PartialEq)]
pub struct Holdout {
    pub score_ids: BTreeSet<String>,
    pub holdout_ids: BTreeSet<String>,
    pub cand: Tally,
    pub orig: Tally,
    pub n_needed: usize,
    /// `not_worse` / `worse` / `insufficient`
    pub verdict: &'static str,
}

/// 读行、核不相交、计数、判定。打分集与留出集按 `item` 有重叠即报错（J-16 同一纪律）。
pub fn compare(rows: &[Json], alpha: f64, conf_delta: f64) -> Result<Holdout, String> {
    let mut score_ids = BTreeSet::new();
    let mut holdout_ids = BTreeSet::new();
    let (mut cand, mut orig) = (Tally::default(), Tally::default());
    for (i, r) in rows.iter().enumerate() {
        let line = i + 1;
        let item = match &r["item"] {
            Json::String(s) => s.clone(),
            Json::Number(n) => n.to_string(),
            _ => return Err(format!("第 {line} 行缺 item")),
        };
        match r["set"].as_str() {
            Some("score") => {
                score_ids.insert(item);
            }
            Some("holdout") => {
                if !holdout_ids.insert(item.clone()) {
                    return Err(format!("第 {line} 行：留出条目 {item} 重复"));
                }
                let truth = &r["truth"];
                if truth.is_null() {
                    return Err(format!("第 {line} 行（留出）缺 truth"));
                }
                for (side, t) in [("orig", &mut orig), ("cand", &mut cand)] {
                    let e = r[side]
                        .as_str()
                        .ok_or_else(|| format!("第 {line} 行（留出）缺 {side}"))?;
                    t.add(score_exit(e, truth).map_err(|m| format!("第 {line} 行 {side}：{m}"))?);
                }
            }
            _ => return Err(format!("第 {line} 行：set 要是 \"score\" 或 \"holdout\"")),
        }
    }
    let overlap: Vec<&String> = score_ids.intersection(&holdout_ids).collect();
    if !overlap.is_empty() {
        return Err(format!(
            "打分集与留出集有重叠（B45：与 J-16 同一纪律，按条目不相交）：{}",
            overlap
                .iter()
                .map(|s| s.as_str())
                .collect::<Vec<_>>()
                .join("、")
        ));
    }
    let n_needed = n_needed_zero_error(alpha, conf_delta);
    let verdict = if holdout_ids.len() < n_needed {
        "insufficient"
    } else if cand.errors <= orig.errors && cand.decided >= orig.decided {
        "not_worse"
    } else {
        "worse"
    };
    Ok(Holdout {
        score_ids,
        holdout_ids,
        cand,
        orig,
        n_needed,
        verdict,
    })
}

fn digest(ids: &BTreeSet<String>) -> String {
    let v: Vec<&str> = ids.iter().map(String::as_str).collect();
    jpp_ir::key::hash_of(&["derive-admit-ids", &v.join("\n")])
}

#[derive(Default)]
struct Args {
    pos: Vec<String>,
    opts: BTreeMap<String, String>,
}

fn parse_args(args: &[String]) -> Result<Args, String> {
    let mut a = Args::default();
    let mut i = 0;
    while i < args.len() {
        let x = &args[i];
        if x.starts_with("--") {
            let v = args
                .get(i + 1)
                .filter(|s| !s.starts_with("--"))
                .ok_or_else(|| format!("{x} requires a value\n\n{USAGE}"))?;
            a.opts.insert(x.clone(), v.clone());
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
        self.opts.get(k).map(String::as_str)
    }
    fn need(&self, k: &str) -> Result<&str, String> {
        self.one(k).ok_or_else(|| format!("缺 {k}\n\n{USAGE}"))
    }
    fn num(&self, k: &str, d: f64) -> Result<f64, String> {
        self.one(k)
            .map_or(Ok(d), |v| v.parse().map_err(|_| format!("{k} 要是数：{v}")))
    }
}

pub fn run(args: &[String]) -> Result<(), String> {
    let a = parse_args(args)?;
    let target = a.pos.first().ok_or_else(|| USAGE.to_string())?;
    let by_arg = match a.one("--derived-by") {
        Some(b) => Some(Provenance::canonical_derived_by(b).ok_or_else(|| {
            format!("--derived-by 不认得「{b}」：写 refine / elicit（或题库词表 refine_partition / gen）")
        })?),
        None => None,
    };
    let rows_path = PathBuf::from(a.need("--rows")?);
    let split = a.need("--split")?.to_string();
    let seed: i64 = a
        .need("--seed")?
        .parse()
        .map_err(|_| "--seed 要是整数".to_string())?;
    let reason = a.need("--reason")?.to_string();
    let alpha = a.num("--alpha", ALPHA_DEFAULT)?;
    let conf_delta = a.num("--conf-delta", CONF_DELTA_DEFAULT)?;
    let who = a
        .one("--who")
        .unwrap_or("jpp derive-admit（命令行）")
        .to_string();
    let bank_dir = PathBuf::from(a.one("--bank").unwrap_or("bank"));

    let text =
        std::fs::read_to_string(&rows_path).map_err(|e| format!("{}: {e}", rows_path.display()))?;
    let rows: Vec<Json> = text
        .lines()
        .enumerate()
        .filter(|(_, l)| !l.trim().is_empty())
        .map(|(i, l)| {
            serde_json::from_str(l)
                .map_err(|e| format!("{} 第 {} 行：{e}", rows_path.display(), i + 1))
        })
        .collect::<Result<_, _>>()?;

    let mut bank = QuestionBank::open(&bank_dir)?;
    let h = bank.resolve(target)?;
    // 派生方式取条目的正式字段 provenance.derived_by（提出时由 `jpp bank propose --derived-by` 写入）
    let by: &'static str = match bank
        .find(&h)
        .and_then(|e| e["provenance"]["derived_by"].as_str())
    {
        Some(have) => Provenance::canonical_derived_by(have)
            .ok_or_else(|| format!("{h} 的 provenance.derived_by「{have}」不在派生方式词表里"))?,
        None => {
            return Err(format!(
                "{h} 没有派生来源（provenance.derived_by）：留出比较只对派生条目做。\
                 修法：派生题式用 `jpp bank propose <slug> … --derived-by refine|elicit [--source-exit <键>] [--run-key <键>]` 提出"
            ));
        }
    };
    if let Some(b) = by_arg
        && b != by
    {
        return Err(format!("{h} 记的派生方式是 {by}，--derived-by 给的是 {b}"));
    }
    if !Provenance::needs_holdout(by) {
        return Err(format!(
            "{h} 的派生方式是 {by}：没有原版可比，只过闸门；要担保错误率走认证（B86，calib-import）。\
             derive-admit 只比划分细化（原版是原题的直接回答）与唤出（原版是来源题面）"
        ));
    }
    let st = bank
        .status_of(&h)
        .ok_or_else(|| format!("题库里没有条目 {h}"))?;
    if !matches!(st, Status::Proposed | Status::Diagnosed) {
        return Err(format!(
            "留出比较只对还没上岗的条目（提出、诊断通过）做；{h} 是「{}」",
            st.as_str()
        ));
    }

    let r = compare(&rows, alpha, conf_delta)?;
    let mut rec = json!({
        "split": split,
        "seed": seed,
        "alpha": alpha,
        "conf_delta": conf_delta,
        "n_needed": r.n_needed,
        "n_score": r.score_ids.len(),
        "n_holdout": r.holdout_ids.len(),
        "score_digest": digest(&r.score_ids),
        "holdout_digest": digest(&r.holdout_ids),
        "cand": r.cand.to_json(),
        "orig": r.orig.to_json(),
        "verdict": r.verdict,
    });
    let date = today();
    let nums = format!(
        "留出 {} 条（打分 {} 条，不相交；切分 {split}，种子 {seed}）；候选已决 {} 错 {}，原版已决 {} 错 {}；n_needed(α={alpha}, δ={conf_delta}) = {}",
        r.holdout_ids.len(),
        r.score_ids.len(),
        r.cand.decided,
        r.cand.errors,
        r.orig.decided,
        r.orig.errors,
        r.n_needed
    );
    bank.record_holdout(&h, rec.clone())?;
    bank.append_change(
        &date,
        "留出比较",
        &format!("`{h}`（派生方式 {by}）留出比较：{}；{nums}", r.verdict),
        &reason,
        &format!(
            "条目 `provenance.holdout`；`bank.json` 版本 {}",
            bank.version()
        ),
        &who,
    )?;
    let next: String = match r.verdict {
        "worse" => {
            let why = format!("留出比较劣于原版（B45）：{nums}");
            bank.reject(&h, &why)?;
            bank.append_change(
                &date,
                "拒绝",
                &format!("`{h}` 退役（rejected）"),
                &why,
                &format!("`bank.json` 版本 {}", bank.version()),
                &who,
            )?;
            "已拒绝（退役，rejected）".into()
        }
        "not_worse" => match a.one("--prereg") {
            Some(p) => {
                // 上岗须记复审人（题库 B48「谁能加」第三条），与 `jpp bank admit --reviewer` 同一口径
                let reviewer = a.need("--reviewer")?.to_string();
                bank.save()?;
                bank.admit(&h, p, &reviewer)?;
                bank.append_change(
                    &date,
                    "上岗",
                    &format!(
                        "`{h}` 留出不劣于原版，上岗「{}」",
                        Status::Certified.as_str()
                    ),
                    &reason,
                    &format!("预注册 {p}；`bank.json` 版本 {}", bank.version()),
                    &who,
                )?;
                format!("已上岗（{}）", Status::Certified.as_str())
            }
            None => "留出不劣；上岗再给 --prereg（条目须诊断通过、有装载时重跑认证通过的校准记录）"
                .into(),
        },
        _ => "证据不足：留出条数不到 n_needed，不判不劣，状态不动".into(),
    };
    bank.save()?;
    rec["derived_by"] = json!(by);
    rec["bank_version"] = json!(bank.version());
    rec["form_hash"] = json!(h);
    rec["status"] = json!(bank.status_of(&h).map(Status::as_str));
    rec["next"] = json!(next);
    println!("{}", serde_json::to_string_pretty(&rec).unwrap());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 缺省与_calib_import_同值() {
        let a: Vec<String> = ["calib-import", "l.jsonl", "--calib-out", "d"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        let crate::options::Command::CalibImport(i) = crate::options::parse(&a).unwrap() else {
            panic!("calib-import 解析成了别的命令")
        };
        assert_eq!((i.alpha, i.conf_delta), (ALPHA_DEFAULT, CONF_DELTA_DEFAULT));
        assert_eq!(n_needed_zero_error(ALPHA_DEFAULT, CONF_DELTA_DEFAULT), 22);
    }

    #[test]
    fn 出口对真值() {
        assert_eq!(score_exit("act", &json!(true)), Ok(Some(true)));
        assert_eq!(score_exit("ignore", &json!(true)), Ok(Some(false)));
        assert_eq!(score_exit("pick(2)", &json!(2)), Ok(Some(true)));
        assert_eq!(score_exit("at(1)", &json!(3)), Ok(Some(false)));
        assert_eq!(score_exit("unsure(band)", &json!(true)), Ok(None));
        assert!(score_exit("act", &json!(1)).is_err());
        assert!(score_exit("maybe", &json!(true)).is_err());
    }

    #[test]
    fn 派生方式两套名字() {
        let c = Provenance::canonical_derived_by;
        assert_eq!(c("elicit"), Some("gen"));
        assert_eq!(c("refine"), Some("refine_partition"));
        assert!(!Provenance::needs_holdout(c("fill").unwrap()));
        assert!(!Provenance::needs_holdout(c("premise").unwrap()));
    }
}
