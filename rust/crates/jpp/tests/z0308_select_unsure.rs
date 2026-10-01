//! Z0308：题库 F3（which_named）、F4（same_name）两条 select 条目经 `jpp bank recert`（固定观察，不花钱）
//! 重认后，`unsure_rate` 与 `cut` 同口径：按入库线重算 F3 1/64 = 0.0156、F4 0/63 = 0.0；线不变。
//!
//! 夹具 `tests/fixtures/bank-recert/fixtures.json` 的 F3、F4 观察带两序置换测量（`scripts/fixture_add_perm_z0308.py`
//! 从 `评估/27a-置换重跑/F*/readings.json` 补）；固定观察后端把它写进账本 `perm`，`recert` 连同读数写回标注行，
//! 导入时随样本进记录。全部在临时目录的题库副本上做。
//!
//! 依据：主控板 Z0308；`地基/过程记录/工程-Z0308-select未决率.md` 第三节预注册 T8–T10。

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use jpp::store::QuestionBank;
use jpp_calib::CalibStore;
use jpp_effects::views::CalibView;
use serde_json::{Value as Json, json};

const F3: &str = "e49770913911a79525896988"; // which_named
const F4: &str = "f4886dabf4e089bbdfd9f432"; // same_name

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn copy_dir(from: &Path, to: &Path) {
    fs::create_dir_all(to).unwrap();
    for e in fs::read_dir(from).unwrap() {
        let e = e.unwrap();
        let (f, t) = (e.path(), to.join(e.file_name()));
        if f.is_dir() {
            copy_dir(&f, &t);
        } else {
            fs::copy(&f, &t).unwrap();
        }
    }
}

fn fixtures() -> String {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/bank-recert/fixtures.json")
        .display()
        .to_string()
}

/// `<tmp>/地基/rust-jpp/{bank, lib, profiles}` 与最小的 `<tmp>/地基/题库/变更记录.md`，`git init` 一次提交。
struct Tree {
    top: PathBuf,
}

impl Tree {
    fn new(tag: &str) -> Tree {
        let top = std::env::temp_dir().join(format!("jpp-z0308-{}-{tag}", std::process::id()));
        let _ = fs::remove_dir_all(&top);
        let rj = top.join("地基/rust-jpp");
        for d in ["bank", "lib", "profiles"] {
            copy_dir(&root().join(d), &rj.join(d));
        }
        fs::create_dir_all(top.join("地基/题库")).unwrap();
        fs::write(
            top.join("地基/题库/变更记录.md"),
            "# 题库变更记录\n\n（测试夹具）\n",
        )
        .unwrap();
        for a in [
            &["init", "-q"][..],
            &["add", "."],
            &[
                "-c",
                "user.name=t",
                "-c",
                "user.email=t@t",
                "commit",
                "-q",
                "-m",
                "fixture",
            ],
        ] {
            let o = Command::new("git")
                .current_dir(&top)
                .args(a)
                .output()
                .unwrap();
            assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
        }
        Tree { top }
    }
    fn bank_dir(&self) -> PathBuf {
        self.top.join("地基/rust-jpp/bank")
    }
    fn profile(&self) -> String {
        self.top
            .join("地基/rust-jpp/profiles/jev-1.13.0.json")
            .display()
            .to_string()
    }
    /// 发行画像的副本，K 选一中段 δ 设回 F3、F4 入库证书的 0.0781（Z0334：发行画像 profile_revision 2 的中段 δ
    /// 是 0.0971，重认会把线迁过去；这条测试核的是「δ 不变时重认，率与线稳定」，所以用 δ 与入库相同的画像）
    fn profile_same_delta(&self) -> String {
        let mut j: Json =
            serde_json::from_str(&fs::read_to_string(self.profile()).unwrap()).unwrap();
        j["delta"]["choice_prob_chosen"]["mid"]["immediate"]["p99"] = json!(0.0781);
        let p = self.top.join("profile-same-delta.json");
        fs::write(&p, j.to_string()).unwrap();
        p.display().to_string()
    }
    fn jpp(&self, args: &[String]) -> (bool, String) {
        let o = Command::new(env!("CARGO_BIN_EXE_jpp"))
            .current_dir(&self.top)
            .args(args)
            .output()
            .unwrap();
        (
            o.status.success(),
            format!(
                "{}{}",
                String::from_utf8_lossy(&o.stdout),
                String::from_utf8_lossy(&o.stderr)
            ),
        )
    }
    fn calib_dir(&self, h: &str) -> PathBuf {
        self.bank_dir().join(format!("entries/{h}/calib"))
    }
    fn record(&self, h: &str) -> Json {
        serde_json::from_slice(
            &fs::read(self.calib_dir(h).join(format!("_form_{h}.json"))).unwrap(),
        )
        .unwrap()
    }
    fn labels(&self, h: &str) -> Vec<Json> {
        fs::read_to_string(self.bank_dir().join(format!("entries/{h}/labels.jsonl")))
            .unwrap()
            .lines()
            .filter(|l| !l.trim().is_empty())
            .map(|l| serde_json::from_str(l).unwrap())
            .collect()
    }
    /// 用某份校准目录以固定观察跑条目的重认读数程序；返回（账本路径, 报告）。
    fn read_run(&self, h: &str, calib: &Path) -> (PathBuf, Json) {
        let e = self.bank_dir().join(format!("entries/{h}/recert"));
        let (led, rep) = (
            self.top.join(format!("{h}.ledger.jsonl")),
            self.top.join(format!("{h}.report.json")),
        );
        let s = |p: &Path| p.display().to_string();
        let (ok, out) = self.jpp(&[
            "run".into(),
            s(&e.join("read.jpp")),
            "--input".into(),
            s(&e.join("materials.json")),
            "--backend".into(),
            "fixed".into(),
            "--fixtures".into(),
            fixtures(),
            "--profile".into(),
            self.profile(),
            "--calib".into(),
            s(calib),
            "--ledger-out".into(),
            s(&led),
            "--output".into(),
            s(&rep),
        ]);
        assert!(ok, "{out}");
        (
            led,
            serde_json::from_slice(&fs::read(&rep).unwrap()).unwrap(),
        )
    }
}

fn exits(rep: &Json, e: &str) -> usize {
    rep["value"]["rows"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|r| r["exit"] == e)
        .count()
}

/// T8 + T9：重认 F3、F4；率 0.0156 / 0.0，线、δ、证书不变；标注行带上置换测量；J-10 视图取到这个数；
/// 用新记录重放读数程序，`unsure` 出口数与率一致（F3 1 条、F4 0 条）。
#[test]
fn recert_f3_f4_gives_cut_consistent_unsure_rate_and_keeps_lines() {
    let t = Tree::new("recert");
    let before: Vec<(Json, Vec<Json>)> = [F3, F4]
        .iter()
        .map(|h| (t.record(h), t.labels(h)))
        .collect();
    let v0 = QuestionBank::open(&t.bank_dir()).unwrap().version();
    let mut a: Vec<String> = [
        "bank",
        "recert",
        "which_named",
        "same_name",
        "--backend",
        "fixed",
        "--fixtures",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect();
    a.extend([
        fixtures(),
        "--profile".into(),
        t.profile_same_delta(),
        "--reason".into(),
        "测试：Z0308".into(),
        "--bank".into(),
        t.bank_dir().display().to_string(),
    ]);
    let (ok, out) = t.jpp(&a);
    assert!(ok, "{out}");
    assert_eq!(out.matches("重认通过").count(), 2, "{out}");
    assert_eq!(QuestionBank::open(&t.bank_dir()).unwrap().version(), v0 + 1);

    for ((h, want, n_unsure), (old, old_lab)) in
        [(F3, 0.0156, 1), (F4, 0.0, 0)].into_iter().zip(&before)
    {
        let new = t.record(h);
        // 题库已在本分支重认过（修前恒为 1.0，见过程记录 3.3）；这里重认一次，率与线都要稳定
        assert_eq!(new["unsure_rate"], json!(want), "{h}");
        // 题类也不变（recert 不带 --report，旧记录的题类经标注行搬运，B120 (a)；主控定 (A)）
        for k in [
            "hi",
            "lo",
            "unsure_rate_delta",
            "status",
            "certs",
            "label_fp",
            "n",
            "kind",
        ] {
            assert_eq!(new[k], old[k], "{h} {k} 不变");
        }
        assert_eq!(
            new["delta"],
            json!(0.0781),
            "{h} 顶层 δ 由空变成画像值（Z0238 已记）"
        );
        let (ns, os) = (
            new["samples"].as_array().unwrap(),
            old["samples"].as_array().unwrap(),
        );
        assert_eq!(ns.len(), os.len());
        for (x, y) in ns.iter().zip(os) {
            assert_eq!(
                (&x["perms"], &x["mode_share"]),
                (&json!(2), &json!(1.0)),
                "{h}"
            );
            for k in ["p", "label", "phys", "mode", "cluster"] {
                assert_eq!(x[k], y[k], "{h} 样本 {k}");
            }
        }
        // 标注行：读数逐行不变，每行添上两序置换测量
        let lab = t.labels(h);
        assert_eq!(lab.len(), old_lab.len());
        for (x, y) in lab.iter().zip(old_lab) {
            let (mut x2, mut y2) = (x.clone(), y.clone());
            let o = x2.as_object_mut().unwrap();
            assert_eq!(o.remove("perms"), Some(json!(2)), "{h}");
            assert_eq!(o.remove("mode_share"), Some(json!(1.0)), "{h}");
            assert_eq!(
                o.remove("kind"),
                Some(old["kind"].clone()),
                "{h} 行上搬运旧记录的题类"
            );
            // 重认前的行可能已带这三项（题库重认过）或没带（修前）：去掉后其余字段逐项不变
            for k in ["perms", "mode_share", "kind"] {
                y2.as_object_mut().unwrap().remove(k);
            }
            assert_eq!(x2, y2, "{h} 其余字段逐项不变");
        }
        // J-10 视图（usable_unsure_rate：上岗、δ 绑定一致）
        let s = CalibStore::load(&t.calib_dir(h)).unwrap();
        assert!(s.load_report.is_empty(), "{:?}", s.load_report);
        let key = format!("\u{1f}form\u{1f}{h}");
        assert_eq!(CalibView::unsure_rate(&s, &key), Some(want), "{h}");
        // T9：用新记录重放，unsure 出口数与率一致
        let (_, rep) = t.read_run(h, &t.calib_dir(h));
        let n = t.labels(h).len();
        assert_eq!(exits(&rep, "unsure"), n_unsure, "{h}");
        assert_eq!(exits(&rep, "pick"), n - n_unsure, "{h}");
    }
    let _ = fs::remove_dir_all(&t.top);
}

/// T10：`calib-import --from-ledger` 回填 select 行时，置换测量与读数一起从账本回接，记录率与 cut 同口径。
#[test]
fn from_ledger_backfill_carries_permutation() {
    let t = Tree::new("ledger");
    let (led, rep) = t.read_run(F3, &t.calib_dir(F3));
    // 报告的 rows 给 (id, key)；标注行按 key（校准键）与 item（状态哈希）回接，这里从账本取状态哈希
    let ledger_text = fs::read_to_string(&led).unwrap();
    let states: std::collections::BTreeMap<String, String> = ledger_text
        .lines()
        .skip(1)
        .filter_map(|l| serde_json::from_str::<Json>(l).ok())
        .filter_map(|v| {
            let j = v["entry"]["Judge"].clone();
            Some((
                j["key"].as_str()?.to_string(),
                j["jkey"]["state"].as_str()?.to_string(),
            ))
        })
        .collect();
    let labels: std::collections::BTreeMap<String, Json> = t
        .labels(F3)
        .into_iter()
        .map(|r| (r["item"].as_str().unwrap().to_string(), r))
        .collect();
    let mut lines = vec![];
    for r in rep["value"]["rows"].as_array().unwrap() {
        let lab = &labels[r["id"].as_str().unwrap()];
        lines.push(
            json!({"key": "cls-which-mentioned", "op": "select",
                   "item": states[r["key"].as_str().unwrap()],
                   "label": lab["label"], "source": "computed"})
            .to_string(),
        );
    }
    let lp = t.top.join("backfill.jsonl");
    fs::write(&lp, lines.join("\n") + "\n").unwrap();
    let out_dir = t.top.join("calib-out");
    let (ok, out) = t.jpp(&[
        "calib-import".into(),
        lp.display().to_string(),
        "--from-ledger".into(),
        led.display().to_string(),
        "--calib-out".into(),
        out_dir.display().to_string(),
        "--profile".into(),
        t.profile(),
    ]);
    assert!(ok, "{out}");
    let s = CalibStore::load(&out_dir).unwrap();
    let r = s.get("cls-which-mentioned");
    assert_eq!(r.status, "上岗", "{out}");
    assert!(
        r.samples
            .iter()
            .all(|x| x.perms == 2 && x.mode_share == Some(1.0)),
        "回填带上置换测量"
    );
    // 手写键的记录与题库记录同一批读数、同一认证：率同为 1/64
    assert_eq!(r.unsure_rate, Some(0.0156), "{out}");
    let _ = fs::remove_dir_all(&t.top);
}
