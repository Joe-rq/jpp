//! 步 28（B45，主控:B0468）：`jpp derive-admit`——派生题面在留出集上不劣于原版才上岗。
//! 预注册见 `地基/过程记录/工程-步28.md` §14.2。全部在临时目录里的题库副本上跑命令行，不改仓库里的 `bank/`
//! （与 `bank_lifecycle.rs` 同法；变更记录用本文件自带的夹具，不读 rust-jpp 外的文件，远程模式也能跑）。
//! 被比的条目用 `json_field` 退回「诊断通过」并带上派生来源 `provenance.derived_by`（模拟经
//! `jpp bank propose --derived-by` 提出的派生条目；它有装载时重跑认证通过的校准记录，新提出的条目没有）。

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::{Value as Json, json};

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

struct Tree {
    top: PathBuf,
    /// 临时树是一个 git 仓库（题库上岗核预注册是真实提交，与 bank_lifecycle.rs 同法）：它的首个提交
    head: String,
}

const F1: &str = "ceb097176786d327fe1a0ba3"; // json_field

impl Tree {
    fn new(tag: &str) -> Tree {
        let top =
            std::env::temp_dir().join(format!("jpp-derive-admit-{}-{tag}", std::process::id()));
        let _ = fs::remove_dir_all(&top);
        let rj = top.join("地基/rust-jpp");
        copy_dir(&root().join("bank"), &rj.join("bank"));
        copy_dir(&root().join("lib/bank"), &rj.join("lib/bank"));
        fs::create_dir_all(top.join("地基/题库")).unwrap();
        fs::write(
            top.join("地基/题库/变更记录.md"),
            "# 题库变更记录（测试夹具）\n",
        )
        .unwrap();
        let git = |args: &[&str]| {
            let o = Command::new("git")
                .current_dir(&top)
                .args(args)
                .output()
                .unwrap();
            assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
            String::from_utf8_lossy(&o.stdout).trim().to_string()
        };
        git(&["init", "-q"]);
        git(&["add", "."]);
        git(&[
            "-c",
            "user.name=t",
            "-c",
            "user.email=t@t",
            "commit",
            "-q",
            "-m",
            "fixture",
        ]);
        let head = git(&["rev-parse", "HEAD"]);
        Tree { top, head }
    }
    /// `json_field` 退回「诊断通过」（还没上岗的条目才做留出比较），`derived_by` 给了就带上派生来源
    fn with_entry(tag: &str, derived_by: Option<&str>) -> Tree {
        let t = Tree::new(tag);
        let p = t.bank_dir().join("bank.json");
        let mut b: Json = serde_json::from_str(&fs::read_to_string(&p).unwrap()).unwrap();
        let e = b["entries"]
            .as_array_mut()
            .unwrap()
            .iter_mut()
            .find(|e| e["form_hash"] == F1)
            .unwrap();
        e["status"] = json!("诊断通过");
        if let Some(by) = derived_by {
            e["provenance"] =
                json!({"derived_by": by, "source_exit_key": "k-exit", "run_ledger_key": "k-run"});
        }
        fs::write(&p, serde_json::to_string_pretty(&b).unwrap()).unwrap();
        // 夹具改了条目：按题库自己的算法重算 index_hash，免得被当成绕过命令手改（复审、上岗核它）
        let h = jpp::store::QuestionBank::open(&t.bank_dir())
            .unwrap()
            .index_hash();
        b["index_hash"] = json!(h);
        fs::write(&p, serde_json::to_string_pretty(&b).unwrap()).unwrap();
        t
    }
    fn jpp(&self, args: &[&str]) -> (bool, String) {
        let mut a: Vec<String> = args.iter().map(|s| s.to_string()).collect();
        a.push("--bank".into());
        a.push(self.bank_dir().display().to_string());
        let o = Command::new(env!("CARGO_BIN_EXE_jpp"))
            .current_dir(&self.top)
            .args(&a)
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
    fn bank_dir(&self) -> PathBuf {
        self.top.join("地基/rust-jpp/bank")
    }
    fn bank_text(&self) -> String {
        fs::read_to_string(self.bank_dir().join("bank.json")).unwrap()
    }
    fn entry(&self) -> Json {
        let b: Json = serde_json::from_str(&self.bank_text()).unwrap();
        b["entries"]
            .as_array()
            .unwrap()
            .iter()
            .find(|e| e["slug"] == "json_field")
            .unwrap()
            .clone()
    }
    fn sections(&self) -> usize {
        fs::read_to_string(self.top.join("地基/题库/变更记录.md"))
            .unwrap()
            .matches("\n## ")
            .count()
    }
    fn rows(&self, rows: &[Json]) -> PathBuf {
        let p = self.top.join("rows.jsonl");
        let t: Vec<String> = rows.iter().map(|r| r.to_string()).collect();
        fs::write(&p, t.join("\n") + "\n").unwrap();
        p
    }
    fn admit(&self, rows: &[Json], extra: &[&str]) -> (bool, String) {
        let p = self.rows(rows);
        let mut a: Vec<String> = [
            "derive-admit",
            "json_field",
            "--rows",
            p.to_str().unwrap(),
            "--split",
            "按条目 id 奇偶",
            "--seed",
            "7",
            "--reason",
            "测试",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();
        a.extend(extra.iter().map(|s| s.to_string()));
        a.push("--bank".into());
        a.push(self.bank_dir().display().to_string());
        let o = Command::new(env!("CARGO_BIN_EXE_jpp"))
            .current_dir(&self.top)
            .args(&a)
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
}

impl Drop for Tree {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.top);
    }
}

/// n 条留出行（真值交替）加 5 条打分行；`orig_bad` / `cand_bad` 条原版 / 候选答错，`cand_unsure` 条候选未决。
/// 答错与未决取不同的条目，互不覆盖。
fn 行(n: usize, orig_bad: usize, cand_bad: usize, cand_unsure: usize) -> Vec<Json> {
    let mut v = vec![];
    for i in 0..5 {
        v.push(json!({"item": format!("s{i}"), "set": "score"}));
    }
    for i in 0..n {
        let truth = i % 2 == 0;
        let right = if truth { "act" } else { "ignore" };
        let wrong = if truth { "ignore" } else { "act" };
        let orig = if i < orig_bad { wrong } else { right };
        let cand = if i < cand_bad {
            wrong
        } else if i >= n - cand_unsure {
            "unsure(band)"
        } else {
            right
        };
        v.push(json!({"item": format!("h{i}"), "set": "holdout", "truth": truth, "orig": orig, "cand": cand}));
    }
    v
}

fn holdout(t: &Tree) -> Json {
    t.entry()["provenance"]["holdout"].clone()
}

#[test]
fn a_候选更好_给预注册即上岗() {
    let t = Tree::with_entry("a", Some("gen"));
    let before = t.sections();
    let (ok, out) = t.admit(
        &行(22, 2, 0, 0),
        &["--prereg", t.head.as_str(), "--reviewer", "复审人"],
    );
    assert!(ok, "{out}");
    let h = holdout(&t);
    assert_eq!(h["verdict"], "not_worse", "{h}");
    assert_eq!(h["cand"], json!({"decided": 22, "errors": 0}));
    assert_eq!(h["orig"], json!({"decided": 22, "errors": 2}));
    assert!(
        h["derived_by"].is_null(),
        "holdout 只放留出比较，派生方式在正式字段：{h}"
    );
    assert_eq!(t.entry()["provenance"]["derived_by"], "gen");
    for k in [
        "split",
        "seed",
        "alpha",
        "conf_delta",
        "n_needed",
        "n_score",
        "n_holdout",
        "score_digest",
        "holdout_digest",
    ] {
        assert!(!h[k].is_null(), "缺 {k}：{h}");
    }
    assert_eq!(
        (
            h["n_needed"].as_u64(),
            h["n_score"].as_u64(),
            h["n_holdout"].as_u64()
        ),
        (Some(22), Some(5), Some(22))
    );
    assert_eq!(t.entry()["status"], "已认证·未复用");
    assert_eq!(t.sections(), before + 2, "变更记录：留出比较、上岗");
}

#[test]
fn b_打平也上岗() {
    let t = Tree::with_entry("b", Some("gen"));
    let (ok, out) = t.admit(
        &行(22, 1, 1, 0),
        &["--prereg", t.head.as_str(), "--reviewer", "复审人"],
    );
    assert!(ok, "{out}");
    assert_eq!(holdout(&t)["verdict"], "not_worse");
    assert_eq!(t.entry()["status"], "已认证·未复用");
}

#[test]
fn c_错误数多_拒绝() {
    let t = Tree::with_entry("c", Some("gen"));
    let (ok, out) = t.admit(
        &行(22, 1, 3, 0),
        &["--prereg", t.head.as_str(), "--reviewer", "复审人"],
    );
    assert!(ok, "{out}");
    assert_eq!(holdout(&t)["verdict"], "worse");
    let e = t.entry();
    assert_eq!(
        (e["status"].as_str(), e["rejected"].as_bool()),
        (Some("退役"), Some(true)),
        "{e}"
    );
}

#[test]
fn d_已决条数少_拒绝() {
    let t = Tree::with_entry("d", Some("gen"));
    let (ok, out) = t.admit(&行(22, 1, 0, 2), &[]);
    assert!(ok, "{out}");
    let h = holdout(&t);
    assert_eq!(h["cand"], json!({"decided": 20, "errors": 0}), "{h}");
    assert_eq!(h["verdict"], "worse");
    assert_eq!(t.entry()["status"], "退役");
}

#[test]
fn e_打分集与留出集重叠_报错且题库不写() {
    let t = Tree::with_entry("e", Some("gen"));
    let before = t.bank_text();
    let mut rows = 行(22, 0, 0, 0);
    rows.push(json!({"item": "h3", "set": "score"}));
    let (ok, out) = t.admit(
        &rows,
        &["--prereg", t.head.as_str(), "--reviewer", "复审人"],
    );
    assert!(!ok && out.contains("重叠") && out.contains("h3"), "{out}");
    assert_eq!(t.bank_text(), before, "bank.json 逐字节不变");
}

#[test]
fn f_留出条数不足_证据不足_状态不动() {
    let t = Tree::with_entry("f", Some("gen"));
    let (ok, out) = t.admit(
        &行(10, 2, 0, 0),
        &["--prereg", t.head.as_str(), "--reviewer", "复审人"],
    );
    assert!(ok, "{out}");
    let h = holdout(&t);
    assert_eq!(
        (h["verdict"].as_str(), h["n_needed"].as_u64()),
        (Some("insufficient"), Some(22)),
        "{h}"
    );
    assert_eq!(t.entry()["status"], "诊断通过");
}

#[test]
fn g_先选后填没有原版_报错() {
    let t = Tree::with_entry("g", Some("pick_then_fill"));
    let before = t.bank_text();
    let (ok, out) = t.admit(&行(22, 0, 0, 0), &[]);
    assert!(!ok && out.contains("没有原版"), "{out}");
    assert_eq!(t.bank_text(), before);
}

#[test]
fn h_不劣但不给预注册_只记录() {
    let t = Tree::with_entry("h", Some("gen"));
    let (ok, out) = t.admit(&行(22, 0, 0, 0), &[]);
    assert!(ok, "{out}");
    assert_eq!(holdout(&t)["verdict"], "not_worse");
    assert_eq!(t.entry()["status"], "诊断通过");
    assert!(out.contains("--prereg"), "提示下一步：{out}");
}

#[test]
fn k_条目没有派生来源_报错且题库不写() {
    let t = Tree::with_entry("k", None);
    let before = t.bank_text();
    let (ok, out) = t.admit(&行(22, 0, 0, 0), &[]);
    assert!(
        !ok && out.contains("derived_by") && out.contains("--derived-by"),
        "{out}"
    );
    assert_eq!(t.bank_text(), before);
}

#[test]
fn i_派生题式提出时来源进正式字段_bank_list_读得到() {
    let t = Tree::new("i");
    fs::write(
        t.top.join("地基/rust-jpp/lib/bank/derived_form.jpp"),
        "let derived_form = form(\"test\", \"提案里写明了资金来源吗？\", {calib: \"bank-derived-form\"});\n",
    )
    .unwrap();
    let (ok, out) = t.jpp(&[
        "bank",
        "propose",
        "derived_form",
        "--source",
        "derive 唤出",
        "--reason",
        "测试",
        "--derived-by",
        "elicit",
        "--source-exit",
        "exit-key-1",
        "--run-key",
        "run-key-1",
    ]);
    assert!(ok, "{out}");
    let b: Json = serde_json::from_str(&t.bank_text()).unwrap();
    let e = b["entries"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["slug"] == "derived_form")
        .unwrap();
    assert_eq!(
        e["provenance"],
        json!({"derived_by": "gen", "source_exit_key": "exit-key-1", "run_ledger_key": "run-key-1"}),
        "{e}"
    );
    let (ok, out) = t.jpp(&["bank", "list"]);
    assert!(ok, "{out}");
    let line = out.lines().find(|l| l.contains("derived_form")).unwrap();
    assert!(line.ends_with("derived_by gen"), "{line}");
    assert!(
        !out.lines()
            .find(|l| l.contains("json_field"))
            .unwrap()
            .contains("derived_by"),
        "手写条目的行不变"
    );
    // 不认得的派生方式、只给来源键不给派生方式：报错
    let (ok, out) = t.jpp(&[
        "bank",
        "propose",
        "derived_form",
        "--source",
        "x",
        "--reason",
        "x",
        "--derived-by",
        "magic",
    ]);
    assert!(!ok && out.contains("magic"), "{out}");
    let (ok, out) = t.jpp(&[
        "bank",
        "propose",
        "derived_form",
        "--source",
        "x",
        "--reason",
        "x",
        "--run-key",
        "r",
    ]);
    assert!(!ok && out.contains("--derived-by"), "{out}");
}

#[test]
fn j_bank_admit_绕不过留出比较() {
    let t = Tree::with_entry("j", Some("refine_partition"));
    let (ok, out) = t.jpp(&[
        "bank",
        "admit",
        "json_field",
        "--prereg",
        t.head.as_str(),
        "--reviewer",
        "复审人",
        "--reason",
        "x",
    ]);
    assert!(
        !ok && out.contains("derive-admit"),
        "没有留出记录的派生条目不能上岗：{out}"
    );
    assert_eq!(t.entry()["status"], "诊断通过");
    // 留出比较劣于原版：拒掉后也不能再上岗（已退役），这里只核「不劣」之后 bank admit 放行
    let (ok, out) = t.admit(&行(22, 1, 0, 0), &[]);
    assert!(ok, "{out}");
    assert_eq!(holdout(&t)["verdict"], "not_worse");
    let (ok, out) = t.jpp(&[
        "bank",
        "admit",
        "json_field",
        "--prereg",
        t.head.as_str(),
        "--reviewer",
        "复审人",
        "--reason",
        "x",
    ]);
    assert!(ok, "{out}");
    assert_eq!(t.entry()["status"], "已认证·未复用");
}
