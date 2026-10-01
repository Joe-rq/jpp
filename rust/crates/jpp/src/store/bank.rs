//! 题库（`21` 步 27；`20` B48、B49、B118；规范 `地基/题库/规范.md` v2）：`bank/bank.json` 加
//! `bank/entries/<form_hash>/` 的读写、条目状态机、版本、变更记录与复审。
//!
//! **隐藏的决定**：题库在磁盘上长什么样、状态怎么迁移、版本怎么自增、变更记录写什么格式。
//! 命令行（`cli/bank.rs`）只传参数；账本聚合在 [`super::bank_stats`]，本模块不读账本（B48：使用统计由账本聚合
//! 工具生成，运行时不写题库）。
//!
//! **禁止依赖**：运行时与 `crate::backends`（`scripts/deps.py` 的模块级约束）；只依赖 `jpp-calib`
//! （`CalibStore::load` 装载即重跑认证，B117）与 `jpp-ir` 的哈希。
//!
//! 状态全集与合法迁移（规范 §二、§三加 B25 的停岗候选；规范缺的五档见 `附注/2026-09-29-题库规范补全提议.md`）：
//! 提出 → 诊断通过 → 已认证·未复用 → 共享；上岗两档可到停岗候选；上岗两档与停岗候选可到被取代；
//! 除退役外任一档可退役。退役是终态；条目目录永远保留以供追溯。

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use jpp_calib::CalibStore;
use serde_json::{Map, Value as Json, json};

/// 条目状态。字符串是写进 `bank.json` 的取值。
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Status {
    Proposed,
    Diagnosed,
    Certified,
    Shared,
    SuspendCandidate,
    Superseded,
    Retired,
}

impl Status {
    pub const ALL: [Status; 7] = [
        Status::Proposed,
        Status::Diagnosed,
        Status::Certified,
        Status::Shared,
        Status::SuspendCandidate,
        Status::Superseded,
        Status::Retired,
    ];
    pub fn as_str(self) -> &'static str {
        match self {
            Status::Proposed => "提出",
            Status::Diagnosed => "诊断通过",
            Status::Certified => "已认证·未复用",
            Status::Shared => "共享",
            Status::SuspendCandidate => "停岗候选",
            Status::Superseded => "被取代",
            Status::Retired => "退役",
        }
    }
    pub fn parse(s: &str) -> Option<Status> {
        Status::ALL.into_iter().find(|x| x.as_str() == s)
    }
    /// 合法的下一档（迁移表，唯一定义处）。
    pub fn next(self) -> &'static [Status] {
        use Status::*;
        match self {
            Proposed => &[Diagnosed, Retired],
            Diagnosed => &[Proposed, Certified, Retired],
            Certified => &[Shared, SuspendCandidate, Superseded, Retired],
            Shared => &[SuspendCandidate, Superseded, Retired],
            SuspendCandidate => &[Certified, Shared, Superseded, Retired],
            Superseded => &[Retired],
            Retired => &[],
        }
    }
    /// 在岗：上岗两档与停岗候选（停岗候选仍供线，B25：由人确认后才停）。
    pub fn in_service(self) -> bool {
        matches!(
            self,
            Status::Certified | Status::Shared | Status::SuspendCandidate
        )
    }
}

/// 漂移信号的倍数与样本下限：在用带内率超过认证集带内率的三倍即漂移信号（规范 §1.1「标注口径」栏同一口径：
/// 「带内率超过认证集三倍」，B25）；样本不足时不出信号。
pub const DRIFT_FACTOR: f64 = 3.0;
/// 出信号所需的最少在用读数（样本太少时三倍没有统计含义；口径限制，规范未定，见补全提议第 1 条）。
pub const DRIFT_MIN_CALLS: u64 = 30;

/// 条目来源（derive 入库流程，步 28）：这条题式是怎么来的。手写题式没有。
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Provenance {
    /// 产生它的那趟运行的账本键（`Entry::Judge` 的键；派生发生在哪次判断之后）
    pub run_ledger_key: Option<String>,
    /// 来源出口键（B45：派生题记来源出口的账本键）
    pub source_exit_key: Option<String>,
    /// 派生方式（B45 三形式：`pick_then_fill`、`refine_partition`、`premise_negation`，或 `gen` 唤出）
    pub derived_by: Option<String>,
}

impl Provenance {
    /// 派生方式的名字归一（唯一定义处）：收 derive 库节点的 `by`（fill / refine / premise / elicit）与本词表本身，
    /// 返回写进题库的名字；不认得的返回 `None`。步 28，主控:B0468。
    pub fn canonical_derived_by(by: &str) -> Option<&'static str> {
        match by {
            "fill" | "pick_then_fill" => Some("pick_then_fill"),
            "refine" | "refine_partition" => Some("refine_partition"),
            "premise" | "premise_negation" => Some("premise_negation"),
            "elicit" | "gen" => Some("gen"),
            _ => None,
        }
    }

    /// 有原版可比、上岗前须过留出比较的派生方式（B45「由数据打分选出的题面」：划分细化比原题的直接回答，
    /// 唤出比来源题面）。先选后填与前提反面没有原版。
    pub fn needs_holdout(derived_by: &str) -> bool {
        matches!(derived_by, "refine_partition" | "gen")
    }

    pub fn to_json(&self) -> Json {
        json!({
            "run_ledger_key": self.run_ledger_key,
            "source_exit_key": self.source_exit_key,
            "derived_by": self.derived_by,
        })
    }
}

/// 题库对象：`bank.json` 整份文档加所在目录。
#[derive(Clone, Debug)]
pub struct QuestionBank {
    dir: PathBuf,
    doc: Map<String, Json>,
}

/// 复审环境：换 `render_version` 或判断器画像即全库到期（B48；规范 §四·复审 1）。
#[derive(Clone, Debug, PartialEq)]
pub struct ReviewEnv {
    pub render_version: String,
    pub profile_hash: Option<String>,
}

/// 复审结论：到期原因与逐条问题。
#[derive(Debug, Default)]
pub struct ReviewReport {
    /// 全库到期的原因（`render_version` 或画像变了、没有基线）
    pub due_all: Vec<String>,
    /// `bank.json` 的索引与记录的 `index_hash` 不符：有人绕过命令手改了条目、没有升版本
    pub hand_edited: bool,
    /// 逐条：`(form_hash, 问题)`，问题为空的不列。含「待重认」（条目带 `recert`）与「环境变了但没在当前环境重认」
    pub entries: Vec<(String, Vec<String>)>,
    /// 判断器版本没核成（比不了）的原因；`None` = 已核（两侧都有画像哈希，相同或已计入 `due_all`）。
    /// 不算到期也不算问题，但不能当成核过了：`--record` 须显式豁免（补缺 5）
    pub judge_unchecked: Option<String>,
    /// 基线没记判断器画像、本次有：无从与过去比较，记基线后开始可比。只提示，不拦（过去的未知不能靠现在补，
    /// 但也不该让每个第一次给画像的人写豁免）
    pub baseline_note: Option<String>,
    /// 相对已有基线，`render_version` 或判断器画像变了（不含「没有基线」）：全库须在当前环境重认
    pub env_changed: bool,
    /// 上次写基线时带的显式豁免 `(项, 理由)`（`reviewed_at.waived`）：每次复审都列出，不算到期也不算问题，
    /// 不改 `--record` 的门槛（补缺 1：豁免的理由「下次复审仍列出」）
    pub prior_waived: Vec<(String, String)>,
}

impl ReviewReport {
    pub fn is_clean(&self) -> bool {
        self.due_all.is_empty() && !self.hand_edited && self.entries.is_empty()
    }
    /// 写基线的拦路项（`--record` 的门槛，补缺 1）：手改、逐条问题（含待重认）。「到期」本身不拦——
    /// 写基线正是把它消掉；到期若因环境变了，则条目问题里会有「未在当前环境重认」。
    pub fn record_blockers(&self) -> Vec<String> {
        let mut v = vec![];
        if self.hand_edited {
            v.push("bank.json 的条目被绕过命令手改过（index_hash 不符）".to_string());
        }
        for (h, ps) in &self.entries {
            for p in ps {
                v.push(format!("{h}：{p}"));
            }
        }
        v
    }
}

/// 环境是否与条目上的重认环境相容：`render_version` 相同；画像哈希相同，或环境这侧未知（环境未知由判断器
/// 版本「未核」单独报，不在这里重复拦）。条目侧画像哈希为 `null`（重认时没取到判断器身份）而环境有画像，
/// 不算相容：认证时用的是谁不知道，就不能当作没变（补缺 5；主控 2026-09-29 批）。
fn env_compatible(rec: &Json, env: &ReviewEnv) -> bool {
    env_incompat_reason(rec, env).is_none()
}

/// 不相容的原因，供逐条问题的文字用。
fn env_incompat_reason(rec: &Json, env: &ReviewEnv) -> Option<&'static str> {
    if rec["render_version"] != env.render_version.as_str() {
        return Some("环境变了，尚未在当前环境重取读数重认（`bank recert`）");
    }
    match (rec["profile_hash"].as_str(), env.profile_hash.as_deref()) {
        (Some(a), Some(b)) if a != b => {
            Some("环境变了，尚未在当前环境重取读数重认（`bank recert`）")
        }
        (None, Some(_)) => Some(
            "基线未含画像，需重认：上次重认没取到判断器身份，无从知道认证用的是哪个判断器（`bank recert`）",
        ),
        _ => None,
    }
}

fn fmt_kv(k: &str, v: &Json) -> String {
    format!("{}: {}", serde_json::to_string(k).unwrap(), v)
}

const ENTRY_KEY_ORDER: [&str; 7] = [
    "form_hash",
    "slug",
    "op",
    "kind",
    "status",
    "grade",
    "batch",
];

fn render_entry(e: &Json) -> String {
    let m = e.as_object().expect("条目是对象");
    let mut parts: Vec<String> = vec![];
    for k in ENTRY_KEY_ORDER {
        if let Some(v) = m.get(k) {
            parts.push(fmt_kv(k, v));
        }
    }
    let mut rest: Vec<&String> = m
        .keys()
        .filter(|k| !ENTRY_KEY_ORDER.contains(&k.as_str()))
        .collect();
    rest.sort();
    for k in rest {
        parts.push(fmt_kv(k, &m[k]));
    }
    format!("{{{}}}", parts.join(", "))
}

const TOP_KEY_ORDER: [&str; 5] = ["about", "version", "index_hash", "reviewed_at", "changelog"];

impl QuestionBank {
    /// 打开题库目录（内含 `bank.json`）。
    pub fn open(dir: &Path) -> Result<QuestionBank, String> {
        let p = dir.join("bank.json");
        let text = fs::read_to_string(&p).map_err(|e| format!("{}: {e}", p.display()))?;
        let doc: Json = serde_json::from_str(&text).map_err(|e| format!("{}: {e}", p.display()))?;
        let Json::Object(doc) = doc else {
            return Err(format!("{}: 顶层必须是对象", p.display()));
        };
        if !doc.get("entries").is_some_and(Json::is_array) {
            return Err(format!("{}: 缺 entries 数组", p.display()));
        }
        let b = QuestionBank {
            dir: dir.to_path_buf(),
            doc,
        };
        for e in b.entries() {
            let s = e["status"].as_str().unwrap_or("");
            if Status::parse(s).is_none() {
                return Err(format!(
                    "{}: 条目 {} 的状态「{s}」不在状态全集里（{}）",
                    p.display(),
                    e["form_hash"].as_str().unwrap_or("?"),
                    Status::ALL.map(Status::as_str).join("、")
                ));
            }
        }
        Ok(b)
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// 题库版本（`bank.json` 的 `version`，人可读的整数；B48）。进账本头 `bank_version` 的就是它的十进制串。
    pub fn version(&self) -> u64 {
        self.doc.get("version").and_then(Json::as_u64).unwrap_or(0)
    }

    pub fn entries(&self) -> &[Json] {
        self.doc["entries"].as_array().map_or(&[], Vec::as_slice)
    }

    pub fn find(&self, form_hash: &str) -> Option<&Json> {
        self.entries().iter().find(|e| e["form_hash"] == form_hash)
    }

    /// 按 `form_hash`、`form_hash` 前缀或 slug 找条目，返回完整 `form_hash`。前缀有歧义即报错。
    pub fn resolve(&self, q: &str) -> Result<String, String> {
        let hits: Vec<&str> = self
            .entries()
            .iter()
            .filter(|e| {
                let h = e["form_hash"].as_str().unwrap_or("");
                h == q || e["slug"] == q || (q.len() >= 6 && h.starts_with(q))
            })
            .filter_map(|e| e["form_hash"].as_str())
            .collect();
        match hits.as_slice() {
            [] => Err(format!("题库里没有条目 {q}（给 form_hash、前缀或 slug）")),
            [one] => Ok((*one).to_string()),
            many => Err(format!("{q} 对应多条条目：{}", many.join("、"))),
        }
    }

    pub fn status_of(&self, form_hash: &str) -> Option<Status> {
        self.find(form_hash)
            .and_then(|e| e["status"].as_str())
            .and_then(Status::parse)
    }

    /// 条目索引的哈希：`entries` 数组规范化后取哈希。每次写命令更新 `bank.json` 里的 `index_hash`；
    /// 复审（[`Self::review`]）比对两者，「改了内容没升版本」由此显形（B48；不放进版本号本身）。
    pub fn index_hash(&self) -> String {
        let s = Json::Array(self.entries().to_vec()).to_string();
        jpp_ir::key::hash_of(&["bank-index", &s])
    }

    /// 变更记录路径：`bank.json` 的 `changelog`（相对题库目录），缺省 `../../题库/变更记录.md`
    /// （`地基/rust-jpp/bank` → `地基/题库/变更记录.md`）。
    pub fn changelog_path(&self) -> PathBuf {
        let rel = self
            .doc
            .get("changelog")
            .and_then(Json::as_str)
            .unwrap_or("../../题库/变更记录.md");
        self.dir.join(rel)
    }

    /// 整份 `bank.json` 的文本（键序固定，条目每条一行，与手写版同形）。
    pub fn render(&self) -> String {
        let mut lines: Vec<String> = vec![];
        for k in TOP_KEY_ORDER {
            if let Some(v) = self.doc.get(k) {
                lines.push(format!("  {}", fmt_kv(k, v)));
            }
        }
        let mut rest: Vec<&String> = self
            .doc
            .keys()
            .filter(|k| !TOP_KEY_ORDER.contains(&k.as_str()) && k.as_str() != "entries")
            .collect();
        rest.sort();
        for k in rest {
            lines.push(format!("  {}", fmt_kv(k, &self.doc[k])));
        }
        let es: Vec<String> = self
            .entries()
            .iter()
            .map(|e| format!("    {}", render_entry(e)))
            .collect();
        lines.push(format!("  \"entries\": [\n{}\n  ]", es.join(",\n")));
        format!("{{\n{}\n}}\n", lines.join(",\n"))
    }

    pub fn save(&self) -> Result<(), String> {
        let p = self.dir.join("bank.json");
        fs::write(&p, self.render()).map_err(|e| format!("{}: {e}", p.display()))
    }

    /// 写命令的公共尾巴：版本加一、重算 `index_hash`。
    fn bump(&mut self) {
        let v = self.version() + 1;
        self.doc.insert("version".into(), json!(v));
        let h = self.index_hash();
        self.doc.insert("index_hash".into(), json!(h));
    }

    fn entry_mut(&mut self, form_hash: &str) -> Result<&mut Map<String, Json>, String> {
        self.doc
            .get_mut("entries")
            .and_then(Json::as_array_mut)
            .and_then(|a| a.iter_mut().find(|e| e["form_hash"] == form_hash))
            .and_then(Json::as_object_mut)
            .ok_or_else(|| format!("题库里没有条目 {form_hash}"))
    }

    /// 迁移合法性检查；不合法时报现状与合法目标。
    fn check_move(&self, form_hash: &str, to: Status) -> Result<Status, String> {
        let from = self
            .status_of(form_hash)
            .ok_or_else(|| format!("题库里没有条目 {form_hash}"))?;
        if from.next().contains(&to) {
            Ok(from)
        } else {
            let ok = from
                .next()
                .iter()
                .map(|s| s.as_str())
                .collect::<Vec<_>>()
                .join("、");
            Err(format!(
                "非法迁移：{form_hash} 现在是「{}」，不能直接到「{}」；合法的下一档：{}",
                from.as_str(),
                to.as_str(),
                if ok.is_empty() {
                    "（终态，没有）".to_string()
                } else {
                    ok
                }
            ))
        }
    }

    fn set_status(&mut self, form_hash: &str, to: Status) -> Result<(), String> {
        self.check_move(form_hash, to)?;
        self.entry_mut(form_hash)?
            .insert("status".into(), json!(to.as_str()));
        Ok(())
    }

    // ---- 生命周期 ----

    /// 提出：建条目目录与条目说明骨架，状态「提出」。身份是 `form_hash`：已有同 `form_hash` 即「题面没改」，拒绝；
    /// slug 也不许重（一个 slug 对应 `lib/bank/<slug>.jpp` 一个文件；派生题式由调用者给不冲突的 slug）。
    /// 手写题式用这个；出题机制 derive 派生的题式用 [`Self::propose_with`] 带来源。
    pub fn propose(
        &mut self,
        slug: &str,
        form_hash: &str,
        op: &str,
        kind: &str,
        batch: &str,
        source: &str,
    ) -> Result<(), String> {
        self.propose_with(slug, form_hash, op, kind, batch, source, None)
    }

    /// 提出并记提出者（`proposer`，只记录，不强制独立；`jpp bank propose` 用 `--who`）。
    pub fn propose_as(
        &mut self,
        who: &str,
        slug: &str,
        form_hash: &str,
        op: &str,
        kind: &str,
        batch: &str,
        source: &str,
    ) -> Result<(), String> {
        self.propose_with(slug, form_hash, op, kind, batch, source, None)?;
        // 与上面同一次写命令：只补字段，不再升版本；索引哈希重算
        self.entry_mut(form_hash)?
            .insert("proposer".into(), json!(who));
        let h = self.index_hash();
        self.doc.insert("index_hash".into(), json!(h));
        Ok(())
    }

    /// 提出并记提出者，带派生来源（`jpp bank propose --derived-by …` 用；步 28）：同 [`Self::propose_as`]，来源写进
    /// 条目的正式字段 `provenance.*`。
    #[allow(clippy::too_many_arguments)]
    pub fn propose_with_as(
        &mut self,
        who: &str,
        slug: &str,
        form_hash: &str,
        op: &str,
        kind: &str,
        batch: &str,
        source: &str,
        provenance: &Provenance,
    ) -> Result<(), String> {
        self.propose_with(slug, form_hash, op, kind, batch, source, Some(provenance))?;
        self.entry_mut(form_hash)?
            .insert("proposer".into(), json!(who));
        let h = self.index_hash();
        self.doc.insert("index_hash".into(), json!(h));
        Ok(())
    }

    /// 提出，带来源（出题机制 derive 入库流程用；步 28，主控:B0468、B0469）：`provenance` 存进条目的
    /// `provenance` 字段——运行账本键、来源出口键、派生方式，以及留出比较的结果（后到，用 [`Self::record_holdout`]）。
    /// 条目状态仍从「提出」起，经诊断（[`Self::record_diagnosis`]）到上岗（[`Self::admit`]），或被 [`Self::reject`]。
    #[allow(clippy::too_many_arguments)]
    pub fn propose_with(
        &mut self,
        slug: &str,
        form_hash: &str,
        op: &str,
        kind: &str,
        batch: &str,
        source: &str,
        provenance: Option<&Provenance>,
    ) -> Result<(), String> {
        if self.find(form_hash).is_some() {
            return Err(format!(
                "{form_hash} 已在题库里：条目以 form_hash 为身份，题面改动才是新条目（B48）"
            ));
        }
        if self.entries().iter().any(|e| e["slug"] == slug) {
            return Err(format!("slug「{slug}」已被别的条目占用"));
        }
        let dir = self.dir.join("entries").join(form_hash);
        fs::create_dir_all(dir.join("calib")).map_err(|e| format!("{}: {e}", dir.display()))?;
        let md = format!(
            "# 题库条目 {slug}（{form_hash}）\n\n状态：提出。来源：{source}。\n\n\
             按 `地基/题库/规范.md` §一填写：1.1 题式、1.2 槽与填法类型、1.3 校准记录与等级、1.4 诊断记录、\
             1.5 使用统计（由 `jpp bank-stats` 从账本聚合，不手工汇总）、1.6 来源、1.7 版本；\
             另按 `00-定位与方法论` §5.5 补隐性知识六项。\n",
        );
        let md_path = dir.join("条目.md");
        if !md_path.exists() {
            fs::write(&md_path, md).map_err(|e| format!("{}: {e}", md_path.display()))?;
        }
        let mut entry = json!({
            "form_hash": form_hash, "slug": slug, "op": op, "kind": kind,
            "status": Status::Proposed.as_str(), "batch": batch, "source": source,
        });
        if let Some(p) = provenance {
            entry["provenance"] = p.to_json();
        }
        self.doc
            .get_mut("entries")
            .and_then(Json::as_array_mut)
            .expect("entries")
            .push(entry);
        self.bump();
        Ok(())
    }

    /// 按 `form_hash` 查条目是否已存在（任何状态，含退役与被取代；派生前先查，避免重复入库）。
    pub fn contains(&self, form_hash: &str) -> bool {
        self.find(form_hash).is_some()
    }

    /// 记留出比较的结果（B45：由数据打分选出的题面须在留出集上不劣于原版才上岗）。`result` 由调用者定形，
    /// 原样存进条目的 `provenance.holdout`；条目须存在且未退役。
    pub fn record_holdout(&mut self, form_hash: &str, result: Json) -> Result<(), String> {
        let st = self
            .status_of(form_hash)
            .ok_or_else(|| format!("题库里没有条目 {form_hash}"))?;
        if st == Status::Retired {
            return Err(format!("{form_hash} 已退役"));
        }
        let e = self.entry_mut(form_hash)?;
        let prov = e
            .entry("provenance")
            .or_insert_with(|| json!({}))
            .as_object_mut()
            .ok_or("provenance 不是对象")?;
        prov.insert("holdout".into(), result);
        self.bump();
        Ok(())
    }

    /// 拒绝：还没上岗的条目（提出、诊断通过）被否决，记「退役」并标 `rejected`，理由必填。上岗后的条目走
    /// [`Self::retire`]。目录保留以供追溯。
    pub fn reject(&mut self, form_hash: &str, reason: &str) -> Result<(), String> {
        if !matches!(
            self.status_of(form_hash),
            Some(Status::Proposed | Status::Diagnosed)
        ) {
            return Err(format!(
                "拒绝只对「提出」「诊断通过」的条目；{form_hash} 是{}（上岗后的条目用 retire）",
                self.status_of(form_hash)
                    .map_or("不存在的条目".into(), |s| format!(
                        "「{}」",
                        s.as_str()
                    ))
            ));
        }
        self.retire(form_hash, reason)?;
        self.entry_mut(form_hash)?
            .insert("rejected".into(), json!(true));
        Ok(())
    }

    /// 静态诊断结论落状态：`warnings` 是 `(码, 文)`，`waived` 是作者书面豁免的码（每条带理由）。
    /// 每条告警都有豁免 → 「诊断通过」；否则留在（或退回）「提出」。返回未被豁免的告警。
    /// 运行期闸门（步 26）落地后经同一个入口接入：把闸门的告警并进 `warnings` 即可。
    pub fn record_diagnosis(
        &mut self,
        form_hash: &str,
        warnings: &[(String, String)],
        waived: &[(String, String)],
    ) -> Result<Vec<(String, String)>, String> {
        let st = self
            .status_of(form_hash)
            .ok_or_else(|| format!("题库里没有条目 {form_hash}"))?;
        if !matches!(st, Status::Proposed | Status::Diagnosed) {
            return Err(format!(
                "诊断只对「提出」「诊断通过」的条目做；{form_hash} 是「{}」",
                st.as_str()
            ));
        }
        let waived_codes: BTreeSet<&str> = waived.iter().map(|(c, _)| c.as_str()).collect();
        let open: Vec<(String, String)> = warnings
            .iter()
            .filter(|(c, _)| !waived_codes.contains(c.as_str()))
            .cloned()
            .collect();
        let to = if open.is_empty() {
            Status::Diagnosed
        } else {
            Status::Proposed
        };
        if to != st {
            self.set_status(form_hash, to)?;
        }
        let e = self.entry_mut(form_hash)?;
        e.insert(
            "diagnosis".into(),
            json!({"open": open.iter().map(|(c, _)| c).collect::<Vec<_>>(),
                   "waived": waived.iter().map(|(c, r)| json!({"code": c, "reason": r})).collect::<Vec<_>>()}),
        );
        self.bump();
        Ok(open)
    }

    /// 装载条目的校准目录并按证书重跑认证；通过才算「至少一条装载时重跑认证通过的记录」（B48，J-03）。
    /// 核四件（与 `tests/bank_entries.rs` 同口径）：有记录；`load` 无降级、无夹具、逐位复现；记录键属于该题式；
    /// 样本数等于标注集非复核行数。
    pub fn verify_calib(&self, form_hash: &str) -> Result<(), String> {
        Self::verify_calib_at(&self.dir.join("entries").join(form_hash), form_hash)
    }

    /// 同 [`Self::verify_calib`]，但条目目录由调用者给（重认先在临时目录里验，通过才替换）。
    pub fn verify_calib_at(base: &Path, form_hash: &str) -> Result<(), String> {
        let base = base.to_path_buf();
        let calib = base.join("calib");
        let raw = CalibStore::load_raw(&calib).map_err(|e| format!("{}: {e}", calib.display()))?;
        if raw.records.is_empty() {
            return Err(format!("{}：没有校准记录", calib.display()));
        }
        let s = CalibStore::load(&calib).map_err(|e| format!("{}: {e}", calib.display()))?;
        if !s.load_report.is_empty() {
            return Err(format!(
                "{}：装载重跑认证有降级或改写：{:?}",
                calib.display(),
                s.load_report
            ));
        }
        for (k, r) in &s.records {
            if r.fixture {
                return Err(format!("记录 {k:?} 降为夹具"));
            }
            if raw.records.get(k) != Some(r) {
                return Err(format!("记录 {k:?} 装载后与原文不同（未逐位复现）"));
            }
            if !k.ends_with(form_hash) {
                return Err(format!("记录键 {k:?} 不属于题式 {form_hash}"));
            }
        }
        let labels = base.join("labels.jsonl");
        let text = fs::read_to_string(&labels).map_err(|e| format!("{}: {e}", labels.display()))?;
        let annotated = text
            .lines()
            .filter(|l| !l.trim().is_empty())
            .filter_map(|l| serde_json::from_str::<Json>(l).ok())
            .filter(|r| r.get("spot_check").is_none())
            .count();
        for r in s.records.values() {
            if r.n as usize != annotated {
                return Err(format!(
                    "记录样本数 {} 与标注集非复核行数 {annotated} 不符",
                    r.n
                ));
            }
        }
        Ok(())
    }

    /// 上岗：状态须为「诊断通过」；须带复审人（补缺 4：「谁能加」第三条复审）；预注册须是本题库所在仓库里
    /// 真实存在的提交（`git cat-file -e <提交>^{commit}`，规范 §二第 3 步、`00-定位` §5.2）；`bank.json` 的
    /// `index_hash` 须与条目索引相符（有人手改过就不能借它上岗）；须过 [`Self::verify_calib`]。
    /// 通过后条目记 `admitted: {reviewer, prereg, at_version, date}`。复审人只记录，不强制独立于提出者（主控 2026-09-29）。
    pub fn admit(&mut self, form_hash: &str, prereg: &str, reviewer: &str) -> Result<(), String> {
        self.check_move(form_hash, Status::Certified)?;
        if reviewer.trim().is_empty() {
            return Err("上岗须记复审人（--reviewer）：「谁能加」第三条是复审（B48）".into());
        }
        if prereg.trim().is_empty() {
            return Err(
                "上岗须带预注册的提交号（--prereg）：认证前先写预注册并提交（规范 §二第 3 步）"
                    .into(),
            );
        }
        match self.doc.get("index_hash").and_then(Json::as_str) {
            Some(h) if h == self.index_hash() => {}
            Some(_) => {
                return Err(
                    "bank.json 的条目被绕过命令手改过（index_hash 不符）：先 `jpp bank review` 查清，再上岗（B48）"
                        .into(),
                );
            }
            None => {
                return Err(
                    "bank.json 没有 index_hash，无从核对是否手改：先 `jpp bank review --record`（B48）"
                        .into(),
                );
            }
        }
        self.check_commit(prereg.trim())?;
        self.verify_calib(form_hash)?;
        self.check_holdout(form_hash)?;
        let at = self.version() + 1;
        self.set_status(form_hash, Status::Certified)?;
        let e = self.entry_mut(form_hash)?;
        e.insert("prereg".into(), json!(prereg));
        e.insert(
            "admitted".into(),
            json!({"reviewer": reviewer.trim(), "prereg": prereg.trim(), "at_version": at,
                   "date": today()}),
        );
        self.bump();
        Ok(())
    }

    /// `prereg` 必须是题库目录所在仓库里的真实提交。git 不可用或目录不在仓库里同样拒（报因，不放行）。
    fn check_commit(&self, prereg: &str) -> Result<(), String> {
        let dir = self.dir.canonicalize().unwrap_or_else(|_| self.dir.clone());
        let o = std::process::Command::new("git")
            .arg("-C")
            .arg(&dir)
            .args(["cat-file", "-e", &format!("{prereg}^{{commit}}")])
            .output()
            .map_err(|e| format!("核预注册提交需要 git，执行失败：{e}"))?;
        if o.status.success() {
            Ok(())
        } else {
            Err(format!(
                "--prereg {prereg} 不是 {} 所在仓库里的提交（git cat-file -e 失败）：{}",
                dir.display(),
                String::from_utf8_lossy(&o.stderr).trim()
            ))
        }
    }

    /// 派生条目上岗前的留出结论（B45：由数据打分选出的题面在留出集上不劣于原版才上岗；步 28，主控:B0468）。
    /// 只管 `provenance.derived_by` 为有原版的派生方式（[`Provenance::needs_holdout`]）的条目；手写题式与先选后填、
    /// 前提反面不查。留出比较由 `jpp derive-admit` 做并经 [`Self::record_holdout`] 记下。
    fn check_holdout(&self, form_hash: &str) -> Result<(), String> {
        let e = self
            .find(form_hash)
            .ok_or_else(|| format!("题库里没有条目 {form_hash}"))?;
        let Some(by) = e["provenance"]["derived_by"].as_str() else {
            return Ok(());
        };
        if !Provenance::needs_holdout(by) {
            return Ok(());
        }
        match e["provenance"]["holdout"]["verdict"].as_str() {
            Some("not_worse") => Ok(()),
            Some(v) => Err(format!(
                "{form_hash} 是派生条目（{by}），留出比较的结论是 {v}，不是 not_worse：B45 要在留出集上不劣于原版才上岗。\
                 修法：换一批留出行重跑 `jpp derive-admit`，或改题面后重新提出"
            )),
            None => Err(format!(
                "{form_hash} 是派生条目（{by}），还没有留出比较记录：B45 要在留出集上不劣于原版才上岗。\
                 修法：先跑 `jpp derive-admit {form_hash} --rows …`"
            )),
        }
    }

    /// 升「共享」（B118）：证据是 ≥ 2 个独立 `purpose` 的程序列表 `{"uses": [{"purpose", "program", ...}]}`。
    pub fn promote(&mut self, form_hash: &str, evidence: &Path) -> Result<usize, String> {
        self.check_move(form_hash, Status::Shared)?;
        let text =
            fs::read_to_string(evidence).map_err(|e| format!("{}: {e}", evidence.display()))?;
        let j: Json =
            serde_json::from_str(&text).map_err(|e| format!("{}: {e}", evidence.display()))?;
        let purposes: BTreeSet<String> = j["uses"]
            .as_array()
            .map(|a| {
                a.iter()
                    .filter_map(|u| u["purpose"].as_str().map(str::to_string))
                    .collect()
            })
            .unwrap_or_default();
        if purposes.len() < 2 {
            return Err(format!(
                "复用证据只有 {} 个独立目的（purpose），升「共享」需 ≥ 2 个（B118）",
                purposes.len()
            ));
        }
        self.set_status(form_hash, Status::Shared)?;
        self.entry_mut(form_hash)?.insert(
            "evidence".into(),
            json!({"file": evidence.display().to_string(), "purposes": purposes.len()}),
        );
        self.bump();
        Ok(purposes.len())
    }

    /// 拆分（一旧多新）与合并（多旧一新）：旧条目记「被取代」，互记来源；新条目须已存在（先 `propose`），
    /// 不删目录。规范 §二第 5 步：依据（复核分歧同向集中，B36 5(c)）写进变更记录的 `reason`。
    pub fn supersede(&mut self, olds: &[String], news: &[String]) -> Result<(), String> {
        if olds.is_empty() || news.is_empty() || (olds.len() > 1 && news.len() > 1) {
            return Err("拆分是一个旧条目对多个新条目，合并是多个旧条目对一个新条目".into());
        }
        for o in olds {
            self.check_move(o, Status::Superseded)?;
            if news.contains(o) {
                return Err(format!("{o} 不能取代自己"));
            }
        }
        for n in news {
            if self.find(n).is_none() {
                return Err(format!("新条目 {n} 不存在：先 `jpp bank propose`"));
            }
            if self.status_of(n).is_some_and(|s| s == Status::Retired) {
                return Err(format!("新条目 {n} 已退役"));
            }
        }
        for o in olds {
            self.set_status(o, Status::Superseded)?;
            self.entry_mut(o)?
                .insert("superseded_by".into(), json!(news));
        }
        for n in news {
            self.entry_mut(n)?.insert("supersedes".into(), json!(olds));
        }
        self.bump();
        Ok(())
    }

    /// 退役：终态，目录保留。理由必填（写进条目与变更记录）。使用统计为 0 不自动退役（规范 §三），
    /// 本函数也不检查——依据由人在 `reason` 里写。
    pub fn retire(&mut self, form_hash: &str, reason: &str) -> Result<(), String> {
        if reason.trim().is_empty() {
            return Err("退役须写理由（--reason）".into());
        }
        self.set_status(form_hash, Status::Retired)?;
        self.entry_mut(form_hash)?
            .insert("retired_reason".into(), json!(reason));
        self.bump();
        Ok(())
    }

    /// 上岗两档到停岗候选（漂移信号，B25）。
    pub fn suspend_candidate(&mut self, form_hash: &str) -> Result<(), String> {
        self.set_status(form_hash, Status::SuspendCandidate)?;
        self.bump();
        Ok(())
    }

    /// 停岗候选由人确认保留：回到上岗两档之一（`calib-confirm --keep` 之后）。
    pub fn reinstate(&mut self, form_hash: &str, to: Status) -> Result<(), String> {
        if !matches!(to, Status::Certified | Status::Shared) {
            return Err("只能回到「已认证·未复用」或「共享」".into());
        }
        if self.status_of(form_hash) != Some(Status::SuspendCandidate) {
            return Err(format!("{form_hash} 不是停岗候选"));
        }
        self.set_status(form_hash, to)?;
        self.bump();
        Ok(())
    }

    // ---- 复审 ----

    /// 记录复审基线（不升版本：它不改条目内容，只记「这个环境下全库重认过」）。
    pub fn record_review(&mut self, env: &ReviewEnv) {
        self.record_review_waived(env, &[]);
    }

    /// 记复审基线，附显式豁免（`(豁免项, 理由)`；下次复审仍看得到，补缺 1、5）。
    pub fn record_review_waived(&mut self, env: &ReviewEnv, waived: &[(String, String)]) {
        let mut r = json!({"render_version": env.render_version, "profile_hash": env.profile_hash,
                   "version": self.version()});
        if !waived.is_empty() {
            r["waived"] = json!(
                waived
                    .iter()
                    .map(|(w, why)| json!({"what": w, "reason": why}))
                    .collect::<Vec<_>>()
            );
        }
        self.doc.insert("reviewed_at".into(), r);
        let h = self.index_hash();
        self.doc.insert("index_hash".into(), json!(h));
    }

    /// 复审（规范 §四·复审）：全库到期条件 = `render_version` 或判断器画像相对基线变了；每条重跑认证；
    /// 索引与 `index_hash` 不符即有人手改；漂移信号由调用者从 `bank-stats` 传入。
    pub fn review(&self, env: &ReviewEnv, drift: &[String]) -> ReviewReport {
        let mut rep = ReviewReport::default();
        let mut base_profile: Option<Option<String>> = None;
        match self.doc.get("reviewed_at") {
            None => rep
                .due_all
                .push("没有复审基线（首次复审，`bank review --record` 记录）".into()),
            Some(r) => {
                rep.prior_waived = r["waived"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(|w| {
                        Some((
                            w["what"].as_str()?.to_string(),
                            w["reason"].as_str()?.to_string(),
                        ))
                    })
                    .collect();
                if r["render_version"] != env.render_version.as_str() {
                    rep.env_changed = true;
                    rep.due_all.push(format!(
                        "render_version 由 {} 变为 {}",
                        r["render_version"], env.render_version
                    ));
                }
                base_profile = Some(r["profile_hash"].as_str().map(str::to_string));
                if let (Some(old), Some(new)) =
                    (r["profile_hash"].as_str(), env.profile_hash.as_deref())
                    && old != new
                {
                    rep.env_changed = true;
                    rep.due_all
                        .push(format!("判断器画像哈希由 {old} 变为 {new}"));
                }
            }
        }
        // 判断器版本：两侧都有才比得了；比不了如实报「未核」，不当成没变（补缺 5；设计要的是版本变化后全库重认）
        rep.judge_unchecked = match env.profile_hash {
            None => Some(
                "本次没有取到判断器身份（未给 --profile，账本头没有画像哈希，找不到默认画像）"
                    .to_string(),
            ),
            Some(_) => None,
        };
        if matches!(
            (&base_profile, env.profile_hash.as_deref()),
            (Some(None), Some(_))
        ) {
            rep.baseline_note =
                Some("基线没有记判断器画像哈希，本次无从与之比较；记基线后开始可比".to_string());
        }
        match self.doc.get("index_hash").and_then(Json::as_str) {
            Some(h) if h != self.index_hash() => rep.hand_edited = true,
            _ => {}
        }
        for e in self.entries() {
            let h = e["form_hash"].as_str().unwrap_or("").to_string();
            let st = Status::parse(e["status"].as_str().unwrap_or(""));
            if !st.is_some_and(Status::in_service) {
                continue;
            }
            let mut problems = vec![];
            if let Some(r) = e.get("recert") {
                problems.push(format!(
                    "待重认：{}（`bank recert` 重取读数后清除）",
                    r["reason"].as_str().unwrap_or("环境变了")
                ));
            } else if rep.env_changed {
                let why = match e.get("recertified_at") {
                    Some(r) => env_incompat_reason(r, env),
                    None => Some("环境变了，尚未在当前环境重取读数重认（`bank recert`）"),
                };
                if let Some(why) = why {
                    problems.push(why.to_string());
                }
            }
            if let Err(m) = self.verify_calib(&h) {
                problems.push(format!("重跑认证不过：{m}"));
            }
            if drift.contains(&h) {
                problems.push("在用带内率出现漂移信号，建议转停岗候选（B25）".into());
            }
            if !problems.is_empty() {
                rep.entries.push((h, problems));
            }
        }
        rep
    }

    /// 给所有在岗且尚未待重认的条目打上 `recert`（环境变了后的显式动作）。返回被打标的 `form_hash`；
    /// 有打标才升版本。条目仍在岗、仍供线（意图汇编 11a：不拦程序）。
    pub fn mark_pending(&mut self, env: &ReviewEnv, reason: &str) -> Vec<String> {
        let hs: Vec<String> = self
            .entries()
            .iter()
            .filter(|e| {
                Status::parse(e["status"].as_str().unwrap_or("")).is_some_and(Status::in_service)
                    && e.get("recert").is_none()
                    && !e
                        .get("recertified_at")
                        .is_some_and(|r| env_compatible(r, env))
            })
            .filter_map(|e| e["form_hash"].as_str().map(str::to_string))
            .collect();
        for h in &hs {
            if let Ok(e) = self.entry_mut(h) {
                e.insert(
                    "recert".into(),
                    json!({"reason": reason, "render_version": env.render_version,
                           "profile_hash": env.profile_hash}),
                );
            }
        }
        if !hs.is_empty() {
            self.bump();
        }
        hs
    }

    /// 在岗条目里带 `recert` 的 `form_hash`。
    pub fn pending(&self) -> Vec<String> {
        self.entries()
            .iter()
            .filter(|e| e.get("recert").is_some())
            .filter_map(|e| e["form_hash"].as_str().map(str::to_string))
            .collect()
    }

    /// 一批重认成功后落账：清 `recert`、记 `recertified_at`（重认所用环境），整批只升一次版本。
    pub fn finish_recert(&mut self, done: &[String], env: &ReviewEnv) -> Result<(), String> {
        let at = self.version() + 1;
        for h in done {
            let e = self.entry_mut(h)?;
            e.remove("recert");
            e.insert(
                "recertified_at".into(),
                json!({"render_version": env.render_version, "profile_hash": env.profile_hash,
                       "at_version": at}),
            );
        }
        self.bump();
        Ok(())
    }

    /// 条目目录（`entries/<form_hash>`）。
    pub fn entry_dir(&self, form_hash: &str) -> PathBuf {
        self.dir.join("entries").join(form_hash)
    }

    // ---- 变更记录 ----

    /// 向变更记录追加一条（只追加，不改历史；格式见该文件头部）。
    pub fn append_change(
        &self,
        date: &str,
        kind: &str,
        what: &str,
        why: &str,
        impact: &str,
        who: &str,
    ) -> Result<(), String> {
        let p = self.changelog_path();
        let mut text = fs::read_to_string(&p).unwrap_or_default();
        if !text.ends_with('\n') && !text.is_empty() {
            text.push('\n');
        }
        text.push_str(&format!(
            "\n---\n\n## {date} · {kind}\n\n**改了什么**：{what}\n\n**为什么**：{why}\n\n**影响**：{impact}\n\n**谁做的**：{who}\n"
        ));
        fs::write(&p, text).map_err(|e| format!("{}: {e}", p.display()))
    }
}

/// 今天的日期（UTC，`YYYY-MM-DD`），变更记录用。
pub fn today() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    let z = (secs / 86400) as i64 + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    format!("{y:04}-{m:02}-{d:02}")
}

// ---- 非在岗仍在供线（补缺 3）----

/// 本次运行装载的 `lib/bank/<slug>.jpp` 里，`bank.json` 判为非在岗的条目：`(slug, 状态)`。只如实报，不拦
/// （意图汇编 11a：不加默认限制）。停岗候选按 B25 算在岗（人确认前仍供线），不在这里；找不到 `bank.json`
/// 或 slug 不在名单里的不报（作者自己的题式）。判据是「装载了该文件」，不是「账本里出现过读数」：
/// 退役后仍被 import 但没走到那一步的，也是仍在供线。
pub fn non_service_loaded<'a>(files: impl IntoIterator<Item = &'a Path>) -> Vec<(String, Status)> {
    let mut root: Option<PathBuf> = None;
    let mut slugs: Vec<String> = vec![];
    for path in files {
        let comps: Vec<_> = path.components().collect();
        let Some(li) = comps
            .iter()
            .rposition(|c| c.as_os_str() == "lib")
            .filter(|i| *i + 1 < comps.len())
        else {
            continue;
        };
        let rel: PathBuf = comps[li + 1..].iter().collect();
        let mut rc = rel.components();
        if rc.next().is_some_and(|c| c.as_os_str() == "bank")
            && let Some(f) = rc.next().and_then(|c| c.as_os_str().to_str())
            && let Some(slug) = f.strip_suffix(".jpp")
        {
            if root.is_none() {
                let r: PathBuf = comps[..li].iter().collect();
                root = Some(r.join("bank"));
            }
            if !slugs.iter().any(|s| s == slug) {
                slugs.push(slug.to_string());
            }
        }
    }
    let Some(bank) = root.and_then(|d| QuestionBank::open(&d).ok()) else {
        return vec![];
    };
    slugs
        .into_iter()
        .filter_map(|slug| {
            let e = bank.entries().iter().find(|e| e["slug"] == slug.as_str())?;
            let st = Status::parse(e["status"].as_str()?)?;
            (!st.in_service()).then_some((slug, st))
        })
        .collect()
}

// ---- 账本头版本（B48）----

/// 由本次运行装载的源文件算 `(lib_version, bank_version)`。
///
/// - `lib_version`：位于名为 `lib` 的目录下、且不在 `lib/bank/` 下的文件（路径相对 `lib/`），交
///   `jpp_lib::lib_version`（文件哈希加诊断规则版本，主会话裁定十五）。**总有值**：没装载标准库文件时也算。
/// - `bank_version`：装载了 `lib/bank/<slug>.jpp` 时，向上找同级 `bank/bank.json`，取它的 `version`（十进制串，
///   不加索引哈希；「改了内容没升版本」由 `bank review` 查，不放进版本号）；没装载或找不到时为 `None`。
pub fn versions_of<'a>(
    files: impl IntoIterator<Item = (&'a Path, &'a [u8])>,
) -> (String, Option<String>) {
    let mut lib: Vec<(String, Vec<u8>)> = vec![];
    let mut bank_root: Option<PathBuf> = None;
    for (path, bytes) in files {
        let comps: Vec<_> = path.components().collect();
        // 最靠近文件的名为 lib 的祖先目录
        let Some(li) = comps
            .iter()
            .rposition(|c| c.as_os_str() == "lib")
            .filter(|i| *i + 1 < comps.len())
        else {
            continue;
        };
        let rel: PathBuf = comps[li + 1..].iter().collect();
        if rel
            .components()
            .next()
            .is_some_and(|c| c.as_os_str() == "bank")
        {
            if bank_root.is_none() {
                let root: PathBuf = comps[..li].iter().collect();
                bank_root = Some(root.join("bank"));
            }
            continue;
        }
        let rel = rel.to_string_lossy().replace('\\', "/");
        if !lib.iter().any(|(p, _)| *p == rel) {
            lib.push((rel, bytes.to_vec()));
        }
    }
    let bank_version = bank_root
        .and_then(|d| QuestionBank::open(&d).ok())
        .map(|b| b.version().to_string());
    (jpp_lib::lib_version(&lib), bank_version)
}
