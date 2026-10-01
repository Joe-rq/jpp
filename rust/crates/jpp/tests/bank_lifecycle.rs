//! 题库生命周期（步 27；规范 §二、§四；B48、B49、B118）：提出 → 静态诊断 → 上岗 → 共享 → 拆分／合并／退役，
//! 复审，以及 derive 入库流程用的库 API（来源、留出结果、拒绝、按 `form_hash` 查是否已存在）。
//!
//! 全部在临时目录里的题库副本上做，不改仓库里的 `bank/`。

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use jpp::store::QuestionBank;
use jpp::store::bank::{Provenance, ReviewEnv, Status};
use serde_json::{Value as Json, json};

const F1: &str = "ceb097176786d327fe1a0ba3"; // json_field

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

/// 临时题库：`<tmp>/地基/rust-jpp/{bank, lib/bank}` 与 `<tmp>/地基/题库/变更记录.md`
/// （`bank.json` 的缺省变更记录路径 `../../题库/变更记录.md` 从这里解析）。
struct Tree {
    top: PathBuf,
}

impl Tree {
    fn new(tag: &str) -> Tree {
        let top = std::env::temp_dir().join(format!("jpp-bank-life-{}-{tag}", std::process::id()));
        let _ = fs::remove_dir_all(&top);
        let rj = top.join("地基/rust-jpp");
        copy_dir(&root().join("bank"), &rj.join("bank"));
        copy_dir(&root().join("lib/bank"), &rj.join("lib/bank"));
        fs::create_dir_all(top.join("地基/题库")).unwrap();
        // 变更记录自带最小夹具：真文件在 `地基/题库/`，`cargoq --remote` 只同步 `地基/rust-jpp`，读不到（补缺 6）
        fs::write(
            top.join("地基/题库/变更记录.md"),
            "# 题库变更记录\n\n（测试夹具）\n",
        )
        .unwrap();
        let t = Tree { top };
        // 上岗核预注册是真实提交：临时树也是一个仓库
        t.git(&["init", "-q"]);
        t.git(&["add", "."]);
        t.git(&[
            "-c",
            "user.name=t",
            "-c",
            "user.email=t@t",
            "commit",
            "-q",
            "-m",
            "fixture",
        ]);
        t
    }
    fn git(&self, args: &[&str]) -> String {
        let o = Command::new("git")
            .current_dir(&self.top)
            .args(args)
            .output()
            .unwrap();
        assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
        String::from_utf8_lossy(&o.stdout).trim().to_string()
    }
    fn bank_dir(&self) -> PathBuf {
        self.top.join("地基/rust-jpp/bank")
    }
    fn lib(&self, slug: &str, text: &str) {
        fs::write(
            self.top.join(format!("地基/rust-jpp/lib/bank/{slug}.jpp")),
            text,
        )
        .unwrap();
    }
    fn changelog(&self) -> String {
        fs::read_to_string(self.top.join("地基/题库/变更记录.md")).unwrap()
    }
    fn sections(&self) -> usize {
        self.changelog().matches("\n## ").count()
    }
    fn bank(&self) -> Json {
        serde_json::from_slice(&fs::read(self.bank_dir().join("bank.json")).unwrap()).unwrap()
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
    fn status(&self, slug: &str) -> String {
        self.bank()["entries"]
            .as_array()
            .unwrap()
            .iter()
            .find(|e| e["slug"] == slug)
            .map(|e| e["status"].as_str().unwrap().to_string())
            .unwrap_or_default()
    }
}

impl Drop for Tree {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.top);
    }
}

const NEW_FORM: &str =
    "let new_form = form(\"test\", \"这段话的语气是否友好？\", {calib: \"bank-new-form\"});\n";

#[test]
fn bank_json_round_trips_byte_for_byte_through_the_writer() {
    let p = root().join("bank");
    let b = QuestionBank::open(&p).unwrap();
    assert_eq!(b.render(), fs::read_to_string(p.join("bank.json")).unwrap());
}

#[test]
fn transition_table_is_closed_and_retired_is_terminal() {
    assert!(Status::Retired.next().is_empty());
    for s in Status::ALL {
        if s != Status::Retired {
            assert!(s.next().contains(&Status::Retired), "{s:?} 应能退役");
        }
        for t in s.next() {
            assert!(Status::ALL.contains(t));
        }
        assert_eq!(Status::parse(s.as_str()), Some(s));
    }
}

#[test]
fn propose_diagnose_and_illegal_admit_by_the_cli() {
    let t = Tree::new("propose");
    t.lib("new_form", NEW_FORM);
    let (n0, v0) = (t.sections(), t.bank()["version"].as_u64().unwrap());
    let (ok, out) = t.jpp(&[
        "bank",
        "propose",
        "new_form",
        "--source",
        "测试",
        "--reason",
        "测试提出",
    ]);
    assert!(ok, "{out}");
    assert_eq!(t.status("new_form"), "提出");
    assert_eq!(
        t.bank()["version"].as_u64().unwrap(),
        v0 + 1,
        "写命令升版本"
    );
    assert_eq!(t.sections(), n0 + 1, "变更记录追加一条");
    let h = t.bank()["entries"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["slug"] == "new_form")
        .unwrap()["form_hash"]
        .as_str()
        .unwrap()
        .to_string();
    assert!(
        t.bank_dir()
            .join("entries")
            .join(&h)
            .join("条目.md")
            .exists()
    );

    // 「提出」直接上岗：非法迁移，报合法目标
    let (ok, out) = t.jpp(&[
        "bank",
        "admit",
        "new_form",
        "--prereg",
        "abc1234",
        "--reviewer",
        "复审甲",
        "--reason",
        "x",
    ]);
    assert!(!ok);
    assert!(
        out.contains("非法迁移") && out.contains("诊断通过"),
        "{out}"
    );
    assert_eq!(t.sections(), n0 + 1, "失败的命令不写变更记录");

    // 题面没改（同 form_hash）不是新条目
    t.lib(
        "dup",
        "let dup = form(\"test\", \"这个 JSON 是否有字段 {F}？（字段名必须完全一致）\", {calib: \"bank-l2-json-field\"});\n",
    );
    let (ok, out) = t.jpp(&["bank", "propose", "dup", "--source", "x", "--reason", "x"]);
    assert!(!ok && out.contains("已在题库里"), "{out}");

    // 静态诊断：未被豁免的告警留在「提出」，每条告警有书面理由才「诊断通过」
    let (ok, out) = t.jpp(&["bank", "diagnose", "new_form"]);
    assert!(ok, "{out}");
    let open: Vec<String> = out
        .lines()
        .filter(|l| l.starts_with("[未过] "))
        .map(|l| l[l.find(' ').unwrap() + 1..l.find(':').unwrap()].to_string())
        .collect();
    if open.is_empty() {
        assert_eq!(t.status("new_form"), "诊断通过");
    } else {
        assert_eq!(t.status("new_form"), "提出");
        let waives: Vec<String> = open.iter().map(|c| format!("{c}:测试豁免")).collect();
        let mut a = vec!["bank", "diagnose", "new_form"];
        for w in &waives {
            a.push("--waive");
            a.push(w);
        }
        let (ok, out) = t.jpp(&a);
        assert!(ok, "{out}");
        assert_eq!(t.status("new_form"), "诊断通过");
    }

    // 诊断通过后上岗：新条目没有校准记录，重跑认证的闸门拒绝（不能只查文件存在，J-03）
    let head = t.git(&["rev-parse", "HEAD"]);
    let (ok, out) = t.jpp(&[
        "bank",
        "admit",
        "new_form",
        "--prereg",
        &head,
        "--reviewer",
        "复审甲",
        "--reason",
        "x",
    ]);
    assert!(!ok && out.contains("没有校准记录"), "{out}");
    assert_eq!(t.status("new_form"), "诊断通过");
}

#[test]
fn admit_with_real_certified_records_then_promote_needs_two_purposes() {
    let t = Tree::new("admit");
    // 把 json_field 退回「诊断通过」，再走 admit：它的记录装载时重跑认证通过
    let p = t.bank_dir().join("bank.json");
    let s = fs::read_to_string(&p).unwrap();
    let s = s.replacen(
        &format!("\"form_hash\": \"{F1}\", \"slug\": \"json_field\", \"op\": \"test\", \"kind\": \"attr\", \"status\": \"已认证·未复用\""),
        &format!("\"form_hash\": \"{F1}\", \"slug\": \"json_field\", \"op\": \"test\", \"kind\": \"attr\", \"status\": \"诊断通过\""),
        1,
    );
    fs::write(&p, s).unwrap();
    // 测试里手改状态后经库 API 记基线，让 index_hash 与索引重新相符（`admit` 会核它）
    let mut b = QuestionBank::open(&t.bank_dir()).unwrap();
    b.record_review(&ReviewEnv {
        render_version: "r2".into(),
        profile_hash: None,
    });
    b.save().unwrap();
    let head = t.git(&["rev-parse", "HEAD"]);
    // 缺预注册提交号：拒绝
    let (ok, out) = t.jpp(&[
        "bank",
        "admit",
        "json_field",
        "--prereg",
        " ",
        "--reviewer",
        "复审甲",
        "--reason",
        "x",
    ]);
    assert!(!ok && out.contains("预注册"), "{out}");
    let (ok, out) = t.jpp(&[
        "bank",
        "admit",
        "json_field",
        "--prereg",
        &head,
        "--reviewer",
        "复审甲",
        "--reason",
        "重新上岗测试",
    ]);
    assert!(ok, "{out}");
    assert_eq!(t.status("json_field"), "已认证·未复用");

    // 升共享：证据只有 1 个独立目的 → 拒绝；2 个 → 升
    let ev = t.top.join("ev.json");
    fs::write(
        &ev,
        json!({"uses": [{"purpose": "a", "program": "p1"}, {"purpose": "a", "program": "p2"}]})
            .to_string(),
    )
    .unwrap();
    let (ok, out) = t.jpp(&[
        "bank",
        "promote",
        "json_field",
        "--evidence",
        ev.to_str().unwrap(),
        "--reason",
        "x",
    ]);
    assert!(!ok && out.contains("≥ 2"), "{out}");
    fs::write(
        &ev,
        json!({"uses": [{"purpose": "a", "program": "p1"}, {"purpose": "b", "program": "p2"}]})
            .to_string(),
    )
    .unwrap();
    let (ok, out) = t.jpp(&[
        "bank",
        "promote",
        "json_field",
        "--evidence",
        ev.to_str().unwrap(),
        "--reason",
        "两个目的",
    ]);
    assert!(ok, "{out}");
    assert_eq!(t.status("json_field"), "共享");
}

#[test]
fn retire_keeps_the_directory_bumps_version_and_blocks_further_moves() {
    let t = Tree::new("retire");
    let (n0, v0) = (t.sections(), t.bank()["version"].as_u64().unwrap());
    // 条目数从 bank.json 读，不写死（题库会增条目）
    let entries0 = t.bank()["entries"].as_array().unwrap().len();
    // 理由必填
    let (ok, out) = t.jpp(&["bank", "retire", "json_field"]);
    assert!(!ok && out.contains("--reason"), "{out}");
    let (ok, out) = t.jpp(&[
        "bank",
        "retire",
        "json_field",
        "--reason",
        "测试退役：字面可判，按 B186 不导出",
    ]);
    assert!(ok, "{out}");
    assert_eq!(t.status("json_field"), "退役");
    assert_eq!(t.bank()["version"].as_u64().unwrap(), v0 + 1);
    assert_eq!(t.sections(), n0 + 1);
    assert!(t.changelog().contains("退役") && t.changelog().contains("测试退役"));
    // 目录保留以供追溯，条目仍在索引里
    assert!(
        t.bank_dir()
            .join("entries")
            .join(F1)
            .join("条目.md")
            .exists()
    );
    assert_eq!(t.bank()["entries"].as_array().unwrap().len(), entries0);
    // 退役是终态
    let (ok, out) = t.jpp(&[
        "bank",
        "admit",
        "json_field",
        "--prereg",
        "abc1234",
        "--reviewer",
        "复审甲",
        "--reason",
        "x",
    ]);
    assert!(
        !ok && out.contains("非法迁移") && out.contains("终态"),
        "{out}"
    );
    let (ok, _) = t.jpp(&["bank", "retire", "json_field", "--reason", "再退一次"]);
    assert!(!ok);
    // 在岗汇总少一条，历史仍列出
    let led = t.top.join("empty.jsonl");
    let o = Command::new(env!("CARGO_BIN_EXE_jpp"))
        .current_dir(root())
        .args([
            "run".to_string(),
            root()
                .join("examples/bank-json_field.jpp")
                .display()
                .to_string(),
            "--fixtures".into(),
            root()
                .join("examples/fixtures/bank-json_field.json")
                .display()
                .to_string(),
            "--calib".into(),
            root()
                .join(format!("bank/entries/{F1}/calib"))
                .display()
                .to_string(),
            "--ledger-out".into(),
            led.display().to_string(),
        ])
        .output()
        .unwrap();
    assert!(o.status.success());
    let (ok, out) = t.jpp(&["bank-stats", led.to_str().unwrap(), "--json"]);
    assert!(ok, "{out}");
    let j: Json = serde_json::from_str(&out).unwrap();
    assert_eq!(j["forms"][F1]["status"], "退役");
    assert_eq!(j["forms"][F1]["calls"], 4, "退役后历史统计仍列出");
    let (ok, text) = t.jpp(&["bank-stats", led.to_str().unwrap()]);
    // 在岗 = 全部条目减去刚退役的这一条（条目数从 bank.json 读，不写死）
    assert!(
        ok && text.contains(&format!("在岗条目 {}", entries0 - 1)),
        "{text}"
    );
}

#[test]
fn split_and_merge_mark_old_entries_superseded_and_keep_them() {
    let t = Tree::new("split");
    t.lib(
        "new_a",
        "let new_a = form(\"test\", \"这段话是否表达了感谢？\", {calib: \"bank-new-a\"});\n",
    );
    t.lib(
        "new_b",
        "let new_b = form(\"test\", \"这段话是否表达了道歉？\", {calib: \"bank-new-b\"});\n",
    );
    for s in ["new_a", "new_b"] {
        let (ok, out) = t.jpp(&["bank", "propose", s, "--source", "t", "--reason", "t"]);
        assert!(ok, "{out}");
    }
    let (ok, out) = t.jpp(&[
        "bank",
        "split",
        "json_field",
        "--into",
        "new_a,new_b",
        "--reason",
        "复核分歧同向集中（测试）",
    ]);
    assert!(ok, "{out}");
    assert_eq!(t.status("json_field"), "被取代");
    let e = t.bank()["entries"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["slug"] == "json_field")
        .unwrap()
        .clone();
    assert_eq!(e["superseded_by"].as_array().unwrap().len(), 2);
    assert!(t.bank_dir().join("entries").join(F1).exists());
    // 拆分是一旧对多新、合并是多旧对一新：多对多拒绝
    let (ok, _) = t.jpp(&[
        "bank",
        "merge",
        "which_named,same_name",
        "--into",
        "new_a,new_b",
        "--reason",
        "x",
    ]);
    assert!(!ok);
    // 合并两条旧的进一条新的
    let (ok, out) = t.jpp(&[
        "bank",
        "merge",
        "which_named,same_name",
        "--into",
        "new_a",
        "--reason",
        "合并（测试）",
    ]);
    assert!(ok, "{out}");
    assert_eq!(t.status("which_named"), "被取代");
    assert_eq!(t.status("same_name"), "被取代");
}

#[test]
fn review_finds_hand_edits_and_environment_changes() {
    let t = Tree::new("review");
    // 仓库里的 bank.json 已记基线；先去掉，测「没有基线」这一路
    let bj = t.bank_dir().join("bank.json");
    let stripped: String = fs::read_to_string(&bj)
        .unwrap()
        .lines()
        .filter(|l| !l.contains("\"index_hash\"") && !l.contains("\"reviewed_at\""))
        .collect::<Vec<_>>()
        .join("\n")
        + "\n";
    fs::write(&bj, stripped).unwrap();
    let mut b = QuestionBank::open(&t.bank_dir()).unwrap();
    let env = ReviewEnv {
        render_version: "r2".into(),
        profile_hash: None,
    };
    // 没有基线：全库到期（首次复审）
    assert!(!b.review(&env, &[]).due_all.is_empty());
    b.record_review(&env);
    b.save().unwrap();
    let b = QuestionBank::open(&t.bank_dir()).unwrap();
    let r = b.review(&env, &[]);
    assert!(r.is_clean(), "{r:?}");
    // 渲染版本变了：全库到期
    let r = b.review(
        &ReviewEnv {
            render_version: "r3".into(),
            profile_hash: None,
        },
        &[],
    );
    assert!(
        r.due_all.iter().any(|x| x.contains("render_version")),
        "{r:?}"
    );
    // 基线没记画像、现在有：比不了，只提示，不算到期（补缺 5）
    let r = b.review(
        &ReviewEnv {
            render_version: "r2".into(),
            profile_hash: Some("p1".into()),
        },
        &[],
    );
    assert!(r.baseline_note.is_some() && r.due_all.is_empty(), "{r:?}");
    // 两侧都有画像哈希且不同：全库到期，环境变了
    let mut b2 = b.clone();
    b2.record_review(&ReviewEnv {
        render_version: "r2".into(),
        profile_hash: Some("p0".into()),
    });
    let r = b2.review(
        &ReviewEnv {
            render_version: "r2".into(),
            profile_hash: Some("p1".into()),
        },
        &[],
    );
    assert!(
        r.due_all.iter().any(|x| x.contains("画像")) && r.env_changed,
        "{r:?}"
    );
    // 漂移信号：该条目复审
    let r = b.review(&env, &[F1.to_string()]);
    assert!(
        r.entries
            .iter()
            .any(|(h, ps)| h == F1 && ps.iter().any(|p| p.contains("漂移")))
    );
    // 绕过命令手改 bank.json、没升版本：显形
    let p = t.bank_dir().join("bank.json");
    let s = fs::read_to_string(&p).unwrap().replacen(
        "\"grade\": \"formal\"",
        "\"grade\": \"trial\"",
        1,
    );
    fs::write(&p, s).unwrap();
    let b = QuestionBank::open(&t.bank_dir()).unwrap();
    assert!(b.review(&env, &[]).hand_edited);
}

#[test]
fn derived_entries_carry_provenance_and_can_be_rejected() {
    let t = Tree::new("derive");
    let mut b = QuestionBank::open(&t.bank_dir()).unwrap();
    assert!(b.contains(F1));
    assert!(!b.contains("ffffffffffffffffffffffff"));
    let prov = Provenance {
        run_ledger_key: Some("ledgerkey1".into()),
        source_exit_key: Some("exitkey1".into()),
        derived_by: Some("refine_partition".into()),
    };
    let h = "aaaaaaaaaaaaaaaaaaaaaaaa";
    b.propose_with(
        "derived_1",
        h,
        "test",
        "attr",
        "derive",
        "derive:B45",
        Some(&prov),
    )
    .unwrap();
    assert!(b.contains(h));
    // 同 form_hash 再提出：拒绝（派生前先 contains 查）
    assert!(
        b.propose_with("derived_2", h, "test", "attr", "derive", "x", None)
            .is_err()
    );
    // 留出比较的结果原样存进来源
    b.record_holdout(h, json!({"holdout_n": 40, "not_worse": true}))
        .unwrap();
    let e = b.find(h).unwrap().clone();
    assert_eq!(e["provenance"]["derived_by"], "refine_partition");
    assert_eq!(e["provenance"]["source_exit_key"], "exitkey1");
    assert_eq!(e["provenance"]["run_ledger_key"], "ledgerkey1");
    assert_eq!(e["provenance"]["holdout"]["not_worse"], true);
    // 诊断通过后拒绝：记退役并标 rejected，目录与索引仍在
    b.record_diagnosis(h, &[], &[]).unwrap();
    assert_eq!(b.status_of(h), Some(Status::Diagnosed));
    b.reject(h, "留出集上劣于原版").unwrap();
    assert_eq!(b.status_of(h), Some(Status::Retired));
    assert_eq!(b.find(h).unwrap()["rejected"], true);
    assert!(b.contains(h), "拒绝后仍能按 form_hash 查到，不会被重复派生");
    // 上岗后的条目不能 reject，要 retire
    assert!(b.reject(F1, "x").is_err());
    // 写盘后重开，来源字段还在
    b.save().unwrap();
    let b2 = QuestionBank::open(&t.bank_dir()).unwrap();
    assert_eq!(
        b2.find(h).unwrap()["provenance"]["holdout"]["holdout_n"],
        40
    );
}
