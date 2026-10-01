//! 题库使用统计：从账本聚合每条题式的使用情况（`20` B48「使用统计由账本聚合工具生成，运行时不写题库」；
//! 规范 §1.5；`21` 步 27）。B22 消费者「题库维护」的读数通道：统计回到 `bank review` 与生命周期命令。
//!
//! **只读账本。** 判断条目靠 `calib_ref.key`（题式键 `\u{1f}form\u{1f}<form_hash>`，运行时填，步 27 做的是步 20a-2
//! 里「账本 `calib_ref` 填值」这一部分）归到题式；没有 `key` 的判断条目（手写题、旧账本）不猜，
//! 只数进「无题式归属」。
//!
//! **口径**
//! - `calls`：该题式的 `Judge` 条目里非复用（`reused_from` 为空）的条数；`reused`：复用条数（B40，不花钱）。
//! - `cost`：这些条目的 `cost` 之和。合批时费用只记在同调用号的第一条（账本注释，L7 2026-09-28），按条目相加即实际
//!   花费；混合批里费用归第一条所属题式，拆不开，不假装拆。
//! - `runs`：出现该题式的账本数；`sites`：各账本里不同调用站点数之和。
//! - 出口分布：读数按该题式在账本里的 `CalibUsed` 记录（线 `hi`/`lo`、δ）经 `jpp_value::bridge::decide` 重算，
//!   与运行时同一个判序函数，不另写阈值比较。账本不记出口（出口 = f(读数, 线)，`20` §3.7），所以这是重算，
//!   且把每条读数都当作过了一次桥——程序没对某读数调用 `cut` 时也会被算入，是口径限制。账本里没有这个题式的记录
//!   时按判断器的回答走（B187）。`budget`、`unobserved` 类的未决不来自账本读数，不在这里。
//! - `in_band_share`：出口为 `Unsure(band)` 的占比；`band_ref` 是记录里认证集上的未决率（`unsure_rate`）；
//!   在用读数够 [`DRIFT_MIN_CALLS`] 条且带内占比 ≥ [`DRIFT_FACTOR`] 倍认证集时出漂移信号（规范 §1.1 标注口径栏
//!   「带内率超过认证集三倍」同一口径；样本不足不出信号）。

use std::collections::{BTreeMap, BTreeSet};

use jpp_ledger::{Entry, Ledger};
use jpp_value::bridge::{CutInput, decide};
use jpp_value::value::ExitKind;
use serde_json::{Value as Json, json};

use super::bank::{DRIFT_FACTOR, DRIFT_MIN_CALLS, Status};

const FORM_PREFIX: &str = "\u{1f}form\u{1f}";

/// 一条题式的统计。
#[derive(Clone, Debug, Default)]
pub struct FormStats {
    pub calls: u64,
    pub reused: u64,
    pub cost: f64,
    pub runs: u64,
    pub sites: u64,
    /// 出口分布：`act`、`ignore`、`pick`、`at`、`unsure`
    pub exits: BTreeMap<String, u64>,
    /// 未决按原因分：`band`、`tie`、`untested`、…
    pub unsure_causes: BTreeMap<String, u64>,
    /// 出口为 `Unsure(band)` 的条数
    pub in_band: u64,
    /// 记录里认证集上的未决率（取见到的第一份记录）
    pub band_ref: Option<f64>,
}

impl FormStats {
    /// 参与出口重算的读数条数（非复用加复用，都是一次读数）
    pub fn readings(&self) -> u64 {
        self.calls + self.reused
    }
    pub fn in_band_share(&self) -> Option<f64> {
        (self.readings() > 0).then(|| self.in_band as f64 / self.readings() as f64)
    }
    /// 漂移信号：`"signal"`、`"none"`、`"insufficient_sample"`（读数不足）或 `"no_reference"`（记录没给认证集未决率）
    pub fn drift(&self) -> &'static str {
        let (Some(share), Some(r)) = (self.in_band_share(), self.band_ref) else {
            return if self.readings() == 0 {
                "insufficient_sample"
            } else {
                "no_reference"
            };
        };
        if self.readings() < DRIFT_MIN_CALLS {
            "insufficient_sample"
        } else if share >= DRIFT_FACTOR * r.max(f64::EPSILON) {
            "signal"
        } else {
            "none"
        }
    }
}

/// 聚合结果。
#[derive(Debug, Default)]
pub struct BankStats {
    /// 名单内每条题式一行（没被用到的全 0），键是 `form_hash`
    pub forms: BTreeMap<String, FormStats>,
    /// 账本里出现、但不在名单里的题式（作者在别处定义的题式）
    pub outside: BTreeMap<String, FormStats>,
    /// 没有 `calib_ref.key` 的判断条目：手写题或旧账本（不猜归属）
    pub unattributed_calls: u64,
    pub unattributed_reused: u64,
    pub ledgers: usize,
}

/// 非在岗仍被引用的一行（补缺 3）。
#[derive(Clone, Debug, PartialEq)]
pub struct InUse {
    pub form_hash: String,
    pub status: Status,
    pub calls: u64,
    pub reused: u64,
}

impl BankStats {
    /// 状态不在岗（提出、诊断通过、被取代、退役）而账本里仍有读数的题式。`status_of` 由调用者从题库取。
    /// 只如实报，不拦（意图汇编 11a）。停岗候选按 B25 算在岗，不在这里，见 [`Self::suspend_candidates_in_use`]。
    pub fn non_service_in_use(&self, status_of: impl Fn(&str) -> Option<Status>) -> Vec<InUse> {
        self.in_use_where(status_of, |s| !s.in_service())
    }

    /// 停岗候选仍供线的题式（候选 = 人还没确认停，B25）：单列提示，不归入「非在岗」。
    pub fn suspend_candidates_in_use(
        &self,
        status_of: impl Fn(&str) -> Option<Status>,
    ) -> Vec<InUse> {
        self.in_use_where(status_of, |s| s == Status::SuspendCandidate)
    }

    fn in_use_where(
        &self,
        status_of: impl Fn(&str) -> Option<Status>,
        pred: impl Fn(Status) -> bool,
    ) -> Vec<InUse> {
        self.forms
            .iter()
            .filter(|(_, f)| f.readings() > 0)
            .filter_map(|(h, f)| {
                let st = status_of(h)?;
                pred(st).then(|| InUse {
                    form_hash: h.clone(),
                    status: st,
                    calls: f.calls,
                    reused: f.reused,
                })
            })
            .collect()
    }
}

fn form_of(key: &str) -> Option<&str> {
    key.strip_prefix(FORM_PREFIX)
}

/// 从一份账本的 `CalibUsed` 记录取线与 δ 与认证集未决率。返回 `(线, δ, 状态供线, 认证集未决率)`。
fn record_line(rec: &Json) -> (Option<(f64, f64)>, Option<f64>, Option<f64>) {
    let 供线 = matches!(rec["status"].as_str(), Some("上岗" | "停岗候选"));
    let line = if 供线 {
        rec["hi"].as_f64().zip(rec["lo"].as_f64())
    } else {
        None
    };
    let delta = rec["delta"].as_f64().or(rec["unsure_rate_delta"].as_f64());
    (line, delta, rec["unsure_rate"].as_f64())
}

/// 聚合。`roster` 是题库名单里的 `form_hash`（列在输出里，没被用到的全 0）。
pub fn aggregate(ledgers: &[Ledger], roster: &[String]) -> BankStats {
    let mut out = BankStats {
        ledgers: ledgers.len(),
        ..Default::default()
    };
    for h in roster {
        out.forms.insert(h.clone(), FormStats::default());
    }
    for l in ledgers {
        let mut seen: BTreeSet<String> = BTreeSet::new();
        let mut sites: BTreeMap<String, BTreeSet<usize>> = BTreeMap::new();
        for e in &l.entries {
            let Entry::Judge {
                jkey,
                answer,
                cost,
                calib_ref,
                reused_from,
                perm,
                ..
            } = e
            else {
                continue;
            };
            let Some(hash) = calib_ref
                .as_ref()
                .and_then(|c| c.key.as_deref())
                .and_then(form_of)
            else {
                if reused_from.is_some() {
                    out.unattributed_reused += 1;
                } else {
                    out.unattributed_calls += 1;
                }
                continue;
            };
            let s = if let Some(s) = out.forms.get_mut(hash) {
                s
            } else {
                out.outside.entry(hash.to_string()).or_default()
            };
            if reused_from.is_some() {
                s.reused += 1;
            } else {
                s.calls += 1;
                s.cost += *cost;
            }
            if let Some(k) = jkey {
                sites.entry(hash.to_string()).or_default().insert(k.site);
            }
            seen.insert(hash.to_string());
            // 出口重算：题式记录（线、δ）取自本账本的 CalibUsed
            let rec = l.calib_used.get(&format!("{FORM_PREFIX}{hash}"));
            let (line, delta, rate) = rec
                .map(|v| record_line(&v["record"]))
                .unwrap_or((None, None, None));
            if s.band_ref.is_none() {
                s.band_ref = rate;
            }
            let input = CutInput {
                fail: None,
                absent: None,
                line,
                cost_requested: false,
                alpha_requested: false,
                answer: Some(answer.clone()),
                delta,
                mode_share: perm.as_ref().map(|p| p.mode_share),
            };
            let (kind, _) = decide(&input);
            let name = match &kind {
                ExitKind::Act => "act",
                ExitKind::Ignore => "ignore",
                ExitKind::Pick(_) => "pick",
                ExitKind::At(_) => "at",
                ExitKind::Unsure(_) => "unsure",
            };
            *s.exits.entry(name.into()).or_default() += 1;
            // 步 36 G3：直方图按原因名计（十六种之一，不带细节）
            if let ExitKind::Unsure(w) = &kind {
                *s.unsure_causes
                    .entry(w.cause.name().to_string())
                    .or_default() += 1;
                if w.cause == jpp_value::value::UnsureCause::Band {
                    s.in_band += 1;
                }
            }
        }
        for h in seen {
            let s = if let Some(s) = out.forms.get_mut(&h) {
                s
            } else {
                out.outside.entry(h.clone()).or_default()
            };
            s.runs += 1;
            s.sites += sites.get(&h).map_or(0, |x| x.len() as u64);
        }
    }
    out
}

impl FormStats {
    pub fn to_json(&self) -> Json {
        json!({
            "calls": self.calls, "reused": self.reused, "cost": self.cost,
            "runs": self.runs, "sites": self.sites,
            "exits": self.exits, "unsure_causes": self.unsure_causes,
            "in_band_share": self.in_band_share(), "band_ref": self.band_ref,
            "drift": self.drift(),
        })
    }
}
