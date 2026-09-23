//! 真值通道（B19）：把一份标注文件折进校准记录，经现有 `commission` 认证上岗。
//!
//! **它不写线。** 线仍由 `commission` 的保形认证定；这里只做三件事：
//! 按键归组、决定每条材料用哪一份真值（人工或构造的优先于模型的）、
//! 以及算出「模型标注能不能单独撑起上岗」这道门。
//!
//! 门槛（抽检一致率下限、弃权率提示线）是导入参数，**不写死在规则里**。

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};
use serde_json::Value as Json;

use crate::conformal::binomial_upper;
use crate::effects::{CalibScope, CalibStore, LabelSource, LiteralMode, Refusal, Sample, Selection, SpotCheck, TruthSummary};
use crate::value::{Form, Op};

/// 标注文件里的一行（JSONL）。
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct LabelRow {
    /// 直接给校准键（与 `test(题面, calib)` 的 calib 同一个字符串）
    #[serde(default)]
    pub key: Option<String>,
    /// 或者给题式：键取题式键 `CalibStore::form_key(题式哈希)`
    #[serde(default)]
    pub form: Option<FormSpec>,
    /// 这一行对应哪条材料。**同一条材料的人工与模型标注靠它配对**（抽检一致率）
    pub item: String,
    /// 那次判断的读数（真值通道只收真值，读数来自一次实际运行）
    pub p: f64,
    /// `true` / `false` / `"ambiguous"`
    pub label: Json,
    /// `human` / `computed` / `model:<名>`
    pub source: String,
    /// 人工抽检批次号：带它的人工行参与一致率计算
    #[serde(default)]
    pub spot_check: Option<String>,
    #[serde(default)]
    pub cluster: Option<String>,
}

/// 题式的规格：与 `.jpp` 里 `form(op, 模板, {…})` 的参数一一对应，**哈希同算法**，
/// 所以导入时声明的题式与程序里写的题式落在同一个键上。
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct FormSpec {
    pub op: String,
    pub template: String,
    #[serde(default)]
    pub scale: Vec<String>,
    #[serde(default)]
    pub evidence: Vec<String>,
    #[serde(default)]
    pub presupposition: Option<String>,
    #[serde(default)]
    pub request: Option<String>,
}

impl FormSpec {
    pub fn key(&self) -> Result<String, String> {
        let op = match self.op.as_str() {
            "test" => Op::Test,
            "select" => Op::Select,
            "measure" => Op::Measure,
            o => return Err(format!("题式 op 只能是 test/select/measure，收到 {o:?}")),
        };
        let f = Form::new(op, &self.template, "", self.scale.clone(), self.evidence.clone(), self.presupposition.clone(), self.request.clone())?;
        Ok(CalibStore::form_key(&f.hash))
    }
}

#[derive(Clone, Debug)]
pub struct ImportOptions {
    pub alpha: f64,
    pub conf_delta: f64,
    /// 模型标注单独上岗所需的人工抽检一致率下限（B19 写 0.9）。
    /// **按一致率的单侧置信下界判，不按点估计**（B19 修正：与 B24 同一纪律）。
    pub spot_check_min: f64,
    /// 上面那个下界的置信水平（B19 修正写 0.95）
    pub spot_check_conf: f64,
    /// 弃权率超过它就提示题面外延可能未定（B13）
    pub abstain_warn: f64,
    /// 批次名，进 `label_set_id` 与 `truth.batch`
    pub batch: String,
    /// 拆分样本认证（B24）的分半种子；写进证书
    pub seed: u64,
}

#[derive(Clone, Debug, Serialize)]
pub struct KeyReport {
    pub key: String,
    pub items: usize,
    pub truth: TruthSummary,
    pub status: String,
    pub hi: f64,
    pub lo: f64,
    /// 认证方式与各侧数字（B24）；未上岗时为空
    pub certification: Option<CertReport>,
    pub warnings: Vec<String>,
}

/// 给人看的认证摘要：怎么选的线、在多少条上认证、两侧各自的错误与上界。
#[derive(Clone, Debug, Serialize)]
pub struct CertReport {
    pub selection: Option<Selection>,
    pub alpha: f64,
    pub conf_delta: f64,
    pub upper: (usize, usize, f64),
    pub lower: Option<(usize, usize, f64)>,
}

fn is_human(src: &str) -> bool {
    src == "human" || src == "computed"
}

/// 把标注行折进 `store`。返回每个键的结论；**键内出错不中断其他键**，错误写进该键的 `gate`。
pub fn import_labels(store: &mut CalibStore, rows: &[LabelRow], opt: &ImportOptions) -> Result<Vec<KeyReport>, String> {
    // 1. 按键归组
    let mut by_key: BTreeMap<String, Vec<&LabelRow>> = BTreeMap::new();
    for (i, r) in rows.iter().enumerate() {
        let key = match (&r.key, &r.form) {
            (Some(k), None) => k.clone(),
            (None, Some(f)) => f.key().map_err(|e| format!("第 {} 行：{e}", i + 1))?,
            _ => return Err(format!("第 {} 行：key 与 form 必须恰好给一个", i + 1)),
        };
        if !(0.0..=1.0).contains(&r.p) {
            return Err(format!("第 {} 行：p 必须在 0..=1，收到 {}", i + 1, r.p));
        }
        if !(is_human(&r.source) || r.source.starts_with("model:")) {
            return Err(format!("第 {} 行：source 只能是 human / computed / model:<名>，收到 {:?}", i + 1, r.source));
        }
        match &r.label {
            Json::Bool(_) => {}
            Json::String(s) if s == "ambiguous" => {}
            other => return Err(format!("第 {} 行：label 只能是 true / false / \"ambiguous\"，收到 {other}", i + 1)),
        }
        if let Some(f) = &r.form {
            if f.op != "test" {
                return Err(format!("第 {} 行：本版真值通道只导入是非题（test）；select / measure 的线尚无标量定义", i + 1));
            }
        }
        by_key.entry(key).or_default().push(r);
    }

    let mut out = vec![];
    for (key, rs) in by_key {
        let mut warnings = vec![];
        // 2. 每条材料定一份真值：人工或构造的优先；没有就用模型的
        let mut per_item: BTreeMap<&str, Vec<&LabelRow>> = BTreeMap::new();
        for r in &rs {
            per_item.entry(r.item.as_str()).or_default().push(r);
        }
        let mut sources: BTreeMap<String, u64> = BTreeMap::new();
        let ambiguous = rs.iter().filter(|r| r.label == Json::String("ambiguous".into())).count() as u64;
        let mut chosen: Vec<(&LabelRow, bool)> = vec![];
        let mut model_only_items = 0usize;
        let (mut sc_n, mut sc_agree) = (0u64, 0u64);
        let mut sc_batches: BTreeSet<String> = BTreeSet::new();
        for (_item, group) in &per_item {
            let human: Vec<&&LabelRow> = group.iter().filter(|r| is_human(&r.source) && r.label.is_boolean()).collect();
            let model: Vec<&&LabelRow> = group.iter().filter(|r| r.source.starts_with("model:") && r.label.is_boolean()).collect();
            // 抽检：同一条材料既有带批次的人工行，又有模型行
            for h in human.iter().filter(|h| h.spot_check.is_some()) {
                if let Some(m) = model.first() {
                    sc_n += 1;
                    if h.label == m.label {
                        sc_agree += 1;
                    }
                    sc_batches.insert(h.spot_check.clone().unwrap_or_default());
                }
            }
            let pick = human.first().or(model.first());
            if let Some(r) = pick {
                if !is_human(&r.source) {
                    model_only_items += 1;
                }
                *sources.entry(r.source.clone()).or_default() += 1;
                chosen.push((r, r.label == Json::Bool(true)));
            }
        }
        // **门要看整条记录，不只看这一批。** `--calib` 装进来的旧记录里可能已经躺着
        // 被上一次导入挡在「待核」的模型标注（样本在门之前就折进去了）；这一批只有人工行时
        // `model_only_items == 0`，门会被绕过，而认证用的是记录里的**全部**样本。
        // 所以把旧的真值账（来源计数、抽检）并进来一起判，并写回合并后的账，
        // 下一次导入也看得见。没有真值账的旧记录（手写 JSON）查不出来源，不在此列。
        if let Some(prev) = store.records.get(&key).and_then(|r| r.truth.clone()) {
            for (src, n) in &prev.sources {
                if !is_human(src) {
                    model_only_items += *n as usize;
                }
                *sources.entry(src.clone()).or_default() += n;
            }
            if let Some(ps) = &prev.spot_check {
                sc_n += ps.n;
                sc_agree += ps.agree;
                sc_batches.extend(ps.batches.iter().cloned());
            }
        }
        let abstain_rate = if rs.is_empty() { 0.0 } else { ambiguous as f64 / rs.len() as f64 };
        if abstain_rate > opt.abstain_warn {
            warnings.push(format!(
                "W-abstain: 键 {key} 标注者弃权率 {:.2} > {:.2}：题面外延可能未定（B13），先改题面再标注",
                abstain_rate, opt.abstain_warn
            ));
        }
        let spot = if sc_n > 0 {
            let lower = agree_lower(sc_agree, sc_n, opt.spot_check_conf);
            Some(SpotCheck { batches: sc_batches.into_iter().collect(), n: sc_n, agree: sc_agree, rate: sc_agree as f64 / sc_n as f64, lower: Some(lower), conf: Some(opt.spot_check_conf) })
        } else {
            None
        };
        // 3. 上岗门：有模型单独撑起的真值时，要有抽检且一致率过门槛
        // 点估计不过门槛 → 待核；点估计过、下界不过 → **临时上岗**：线照常认证，
        // 但 `gate` 写明下界与转正所需的追加条数，`cut` 用到它时报 `W-provisional`。
        let mut provisional: Option<String> = None;
        let gate_block: Option<String> = if model_only_items == 0 {
            None
        } else {
            match &spot {
                None => Some(format!("待核：{model_only_items} 条真值只有模型标注，同键没有人工抽检")),
                Some(s) if s.rate < opt.spot_check_min => {
                    Some(format!("待核：人工抽检一致率 {:.2}（{}/{}）< 门槛 {:.2}", s.rate, s.agree, s.n, opt.spot_check_min))
                }
                Some(s) => {
                    let lo = s.lower.unwrap_or(0.0);
                    if lo < opt.spot_check_min {
                        let more = extra_needed(s.agree, s.n, opt.spot_check_min, opt.spot_check_conf);
                        provisional = Some(format!(
                            "临时上岗：人工抽检一致率 {}/{}，单侧 {:.0}% 置信下界 {:.3} < 门槛 {:.2}；{}",
                            s.agree, s.n, opt.spot_check_conf * 100.0, lo, opt.spot_check_min,
                            match more { Some(m) => format!("再追加 {m} 条全一致即转正"), None => "追加 200 条内全一致也到不了门槛，需重做抽检".into() }
                        ));
                    }
                    None
                }
            }
        };
        // 4. 折样本
        for (r, lab) in &chosen {
            store
                .absorb(&key, Sample {
                    p: Some(r.p),
                    label: Some(u8::from(*lab)),
                    perms: 0,
                    mode_share: None,
                    mode: LiteralMode::default(),
                    phys: "noul".into(),
                    cluster: r.cluster.clone(),
                })
                .map_err(|e| format!("键 {key}：{e}"))?;
        }
        let batch = opt.batch.clone();
        let _ = store.set_label_set_id(&key, &format!("truth:{batch}"));
        // 导入的真值覆盖了这批导入的全部读数：由导入者声明「全体」
        let _ = store.set_label_source(&key, LabelSource::全体);
        let gate = match gate_block {
            Some(why) => why,
            None if chosen.is_empty() => "待核：没有带真值的条目（全部为模棱两可）".to_string(),
            None => match store.commission_two_sided_split(&key, opt.alpha, opt.conf_delta, opt.seed) {
                Ok(_) => provisional.clone().unwrap_or_else(|| "上岗".to_string()),
                Err(Refusal::认证不过(c)) => format!("认证不过：{c:?}"),
                Err(Refusal::跑不成(w)) if w.starts_with("待核") => w,
                Err(Refusal::跑不成(w)) => format!("认证不过：{w}"),
            },
        };
        let scope = CalibScope {
            batches: vec![batch.clone()],
            sources: sources.clone(),
            note: "认证集来源；材料风格指纹与范围外告警（B24 补充）未做".into(),
        };
        let summary = TruthSummary { sources, ambiguous, abstain_rate, spot_check: spot, spot_check_min: opt.spot_check_min, gate: gate.clone(), batch };
        if let Some(rec) = store.records.get_mut(&key) {
            rec.truth = Some(summary.clone());
            rec.scope = Some(scope);
        }
        let certification = store.records.get(&key).and_then(|r| {
            if r.status != "上岗" { return None; }
            let up = r.certs.values().find(|c| c.selection.is_some()).or_else(|| r.certs.values().next())?;
            Some(CertReport {
                selection: up.selection.clone(),
                alpha: up.alpha,
                conf_delta: up.conf_delta,
                upper: (up.n_accepted, up.n_errors, up.ucb),
                lower: r.lower.as_ref().map(|c| (c.n_accepted, c.n_errors, c.ucb)),
            })
        });
        let rec = store.get(&key);
        out.push(KeyReport { key, items: chosen.len(), truth: summary, status: rec.status, hi: rec.hi, lo: rec.lo, certification, warnings });
    }
    Ok(out)
}

/// 一致率的单侧置信下界（Clopper–Pearson）：`1 − 不一致率的上界`。
/// 25/25、0.95 → 0.887（B19 修正引用的数）。
pub fn agree_lower(agree: u64, n: u64, conf: f64) -> f64 {
    if n == 0 {
        return 0.0;
    }
    1.0 - binomial_upper((n - agree) as usize, n as usize, 1.0 - conf)
}

/// 再追加多少条**全一致**的抽检，下界才到门槛；200 条内到不了返回 `None`。
pub fn extra_needed(agree: u64, n: u64, min: f64, conf: f64) -> Option<u64> {
    (0..=200u64).find(|m| agree_lower(agree + m, n + m, conf) >= min)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// B19 修正：25/25 的单侧 95% 下界约 0.887（不过 0.9）；零分歧要 29 条（0.05^(1/29) ≈ 0.902），即再追加 4 条。
    #[test]
    fn spot_check_gate_uses_the_lower_bound() {
        let lo = agree_lower(25, 25, 0.95);
        assert!((lo - 0.887).abs() < 0.002, "{lo}");
        assert_eq!(extra_needed(25, 25, 0.9, 0.95), Some(4));
        assert!(agree_lower(30, 30, 0.95) >= 0.9);
    }
}
