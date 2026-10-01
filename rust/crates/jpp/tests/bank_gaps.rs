//! 题库补缺（主控:B0465 复核缺口 1–5；`地基/过程记录/工程-步27.md`「补缺」节）：
//! 复审基线写入门槛、待重认与重取读数、非在岗仍被引用、`admit` 强制复审、判断器版本来源。
//!
//! 全部在临时目录的题库副本上做；不读工作区外的文件（变更记录自带一份最小夹具），重认用固定观察，费用为 0。

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use jpp::store::QuestionBank;
use jpp::store::bank::ReviewEnv;
use serde_json::{Value as Json, json};

const F1: &str = "ceb097176786d327fe1a0ba3"; // json_field
const F2: &str = "1d77f7a203b4258048dfefea"; // answers_question
const PROFILE: &str = "profiles/jev-1.13.0.json";
const READ_JPP: &str = r#"budget {calls: 70, cost: 0.05, depth: 256};
let has_field = form("test", "这个 JSON 是否有字段 {F}？（字段名必须完全一致）", {calib: "bank-l2-json-field"});
fn one(x) !{judge} {
    let r = judge(state(mat(x.json)), fill(has_field, {F: x.field}));
    handle(cut(r), {
        act: fn() { {id: x.id, key: key_of(r), exit: "act", open: []} },
        ignore: fn() { {id: x.id, key: key_of(r), exit: "ignore", open: []} },
        unsure: fn(u) { {id: x.id, key: key_of(r), exit: "unsure", open: [u]} }
    })
}
let rows = map(input.items, fn(x) !{judge} { one(x) });
{rows: map(rows, fn(x) { {id: x.id, key: x.key, exit: x.exit} }),
 pending: fold(rows, [], fn(acc, x) { concat(acc, x.open) })}
"#;

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
}

impl Tree {
    /// `<tmp>/地基/rust-jpp/{bank, lib, examples, profiles}`，`<tmp>/地基/题库/变更记录.md`，并 `git init` 一次提交。
    fn new(tag: &str) -> Tree {
        let top = std::env::temp_dir().join(format!("jpp-bank-gaps-{}-{tag}", std::process::id()));
        let _ = fs::remove_dir_all(&top);
        let rj = top.join("地基/rust-jpp");
        copy_dir(&root().join("bank"), &rj.join("bank"));
        copy_dir(&root().join("lib"), &rj.join("lib"));
        copy_dir(&root().join("examples"), &rj.join("examples"));
        copy_dir(&root().join("profiles"), &rj.join("profiles"));
        fs::create_dir_all(top.join("地基/题库")).unwrap();
        fs::write(
            top.join("地基/题库/变更记录.md"),
            "# 题库变更记录\n\n（测试夹具）\n",
        )
        .unwrap();
        let t = Tree { top };
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
    fn head(&self) -> String {
        self.git(&["rev-parse", "HEAD"])
    }
    fn rj(&self) -> PathBuf {
        self.top.join("地基/rust-jpp")
    }
    fn bank_dir(&self) -> PathBuf {
        self.rj().join("bank")
    }
    fn bank_text(&self) -> String {
        fs::read_to_string(self.bank_dir().join("bank.json")).unwrap()
    }
    fn bank(&self) -> Json {
        serde_json::from_str(&self.bank_text()).unwrap()
    }
    fn version(&self) -> u64 {
        self.bank()["version"].as_u64().unwrap()
    }
    fn entry(&self, slug: &str) -> Json {
        self.bank()["entries"]
            .as_array()
            .unwrap()
            .iter()
            .find(|e| e["slug"] == slug)
            .unwrap()
            .clone()
    }
    fn sections(&self) -> usize {
        fs::read_to_string(self.top.join("地基/题库/变更记录.md"))
            .unwrap()
            .matches("\n## ")
            .count()
    }
    fn raw(&self, cwd: &Path, args: &[String]) -> (bool, String) {
        let o = Command::new(env!("CARGO_BIN_EXE_jpp"))
            .current_dir(cwd)
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
    /// `jpp bank <args> --bank <临时题库>`
    fn bank_cmd(&self, args: &[&str]) -> (bool, String) {
        let mut a: Vec<String> = vec!["bank".into()];
        a.extend(args.iter().map(|s| s.to_string()));
        a.push("--bank".into());
        a.push(self.bank_dir().display().to_string());
        self.raw(&self.top, &a)
    }
    fn profile(&self) -> String {
        self.rj().join(PROFILE).display().to_string()
    }
    /// 跑一个 bank 示例，写账本；`profile` 给了则带 `--profile`。返回 (账本路径, 报告)。
    fn run_example(&self, slug: &str, hash: &str, profile: Option<&str>) -> (PathBuf, Json) {
        let led = self.top.join(format!("{slug}-{}.jsonl", profile.is_some()));
        let rep = self.top.join(format!("{slug}-report.json"));
        let mut a: Vec<String> = vec![
            "run".into(),
            self.rj()
                .join(format!("examples/bank-{slug}.jpp"))
                .display()
                .to_string(),
            "--fixtures".into(),
            self.rj()
                .join(format!("examples/fixtures/bank-{slug}.json"))
                .display()
                .to_string(),
            "--calib".into(),
            self.bank_dir()
                .join(format!("entries/{hash}/calib"))
                .display()
                .to_string(),
            "--ledger-out".into(),
            led.display().to_string(),
            "--output".into(),
            rep.display().to_string(),
        ];
        if let Some(p) = profile {
            a.push("--profile".into());
            a.push(p.into());
        }
        let (ok, out) = self.raw(&self.top, &a);
        assert!(ok, "{out}");
        let r: Json = serde_json::from_slice(&fs::read(&rep).unwrap()).unwrap();
        (led, r)
    }
    /// 把 `bank.json` 里一条状态改成别的（测试用手改），再经库 API 记基线，使 `index_hash` 与索引重新相符。
    fn force_status(&self, slug: &str, to: &str) {
        let p = self.bank_dir().join("bank.json");
        let e = self.entry(slug);
        let from = e["status"].as_str().unwrap();
        let s = fs::read_to_string(&p).unwrap().replacen(
            &format!("\"slug\": \"{slug}\""),
            &format!("\"slug\": \"{slug}\""),
            1,
        );
        let old = format!(
            "\"slug\": \"{slug}\", \"op\": \"{}\", \"kind\": \"{}\", \"status\": \"{from}\"",
            e["op"].as_str().unwrap(),
            e["kind"].as_str().unwrap()
        );
        assert!(s.contains(&old), "找不到条目行：{old}");
        fs::write(&p, s.replacen(&old, &old.replace(from, to), 1)).unwrap();
        let mut b = QuestionBank::open(&self.bank_dir()).unwrap();
        let env = ReviewEnv {
            render_version: jpp_ir_render_version(),
            profile_hash: None,
        };
        b.record_review(&env);
        b.save().unwrap();
    }
}

fn jpp_ir_render_version() -> String {
    // 与命令行里 `jpp_ir::key::RENDER_VERSION` 同值：从基线里取（仓库里的 bank.json 记的就是它）
    let b: Json =
        serde_json::from_slice(&fs::read(root().join("bank/bank.json")).unwrap()).unwrap();
    b["reviewed_at"]["render_version"]
        .as_str()
        .unwrap()
        .to_string()
}

impl Drop for Tree {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.top);
    }
}

// ---- 缺口 1、5：复审基线的写入门槛与判断器身份 ----

#[test]
fn record_is_refused_while_judge_version_is_unchecked_and_written_with_a_waiver() {
    let t = Tree::new("g1");
    let (v0, n0) = (t.version(), t.sections());
    let before = t.bank_text();
    let (ok, out) = t.bank_cmd(&["review"]);
    assert!(ok, "{out}");
    assert!(out.contains("未核判断器版本"), "{out}");
    // 没给 --profile、没给账本：核不了，不能当核过；拒写基线，文件逐字节不变
    let (ok, out) = t.bank_cmd(&["review", "--record"]);
    assert!(
        !ok && out.contains("拒绝写复审基线") && out.contains("未核判断器版本"),
        "{out}"
    );
    assert_eq!(t.bank_text(), before);
    assert_eq!(t.sections(), n0, "被拒不写变更记录");
    // 显式豁免：写基线，版本不变，理由进 reviewed_at.waived 与变更记录
    let (ok, out) = t.bank_cmd(&["review", "--record", "--waive", "判断器版本:本机没有画像"]);
    assert!(ok, "{out}");
    assert_eq!(t.version(), v0, "基线不改条目内容，不升版本");
    assert_eq!(t.sections(), n0 + 1);
    let w = &t.bank()["reviewed_at"]["waived"][0];
    assert_eq!(w["what"], "判断器版本");
    assert_eq!(w["reason"], "本机没有画像");
    // 「下次复审仍列出」：豁免不因写过基线而消失，也不算到期或问题
    let (ok, out) = t.bank_cmd(&["review"]);
    assert!(
        ok && out.contains("上次基线带豁免：判断器版本（本机没有画像）"),
        "{out}"
    );
}

#[test]
fn record_is_refused_with_an_entry_problem_and_needs_a_per_entry_waiver() {
    let t = Tree::new("g1b");
    // 弄坏一条的标注集：样本数与记录不符，重跑认证不过
    let lp = t.bank_dir().join(format!("entries/{F1}/labels.jsonl"));
    let text = fs::read_to_string(&lp).unwrap();
    let cut: Vec<&str> = text.lines().skip(1).collect();
    fs::write(&lp, cut.join("\n") + "\n").unwrap();
    let judge = ["--profile", &t.profile()];
    let mut a = vec!["review", "--record"];
    a.extend(judge);
    let (ok, out) = t.bank_cmd(&a);
    assert!(
        !ok && out.contains(F1) && out.contains("重跑认证不过"),
        "{out}"
    );
    // 只豁免判断器版本不够（这里给了画像，本就已核）；须逐条豁免并写理由
    let mut a = vec!["review", "--record", "--waive", "json_field:标注集待补"];
    a.extend(judge);
    let (ok, out) = t.bank_cmd(&a);
    assert!(ok, "{out}");
    let w = &t.bank()["reviewed_at"]["waived"][0];
    assert_eq!(w["what"], F1);
    // 豁免项要有理由
    let (ok, out) = t.bank_cmd(&["review", "--record", "--waive", "json_field:"]);
    assert!(!ok && out.contains("理由必填"), "{out}");
}

#[test]
fn hand_edit_blocks_record_and_cannot_be_waived() {
    let t = Tree::new("g1c");
    let p = t.bank_dir().join("bank.json");
    fs::write(
        &p,
        t.bank_text()
            .replacen("\"grade\": \"formal\"", "\"grade\": \"trial\"", 1),
    )
    .unwrap();
    let (ok, out) = t.bank_cmd(&[
        "review",
        "--record",
        "--profile",
        &t.profile(),
        "--waive",
        "判断器版本:x",
    ]);
    assert!(
        !ok && out.contains("手改") && out.contains("不可豁免"),
        "{out}"
    );
}

#[test]
fn judge_identity_comes_from_the_ledger_header_and_conflicts_are_reported() {
    let t = Tree::new("g5");
    let (with_a, _) = t.run_example("json_field", F1, Some(&t.profile()));
    let other = t
        .rj()
        .join("profiles/judge-claude-p.json")
        .display()
        .to_string();
    // 第二份账本用另一份画像：文件名不同，避免覆盖
    let led_b = t.top.join("b.jsonl");
    let (ok, out) = t.raw(
        &t.top,
        &[
            "run".into(),
            t.rj()
                .join("examples/bank-json_field.jpp")
                .display()
                .to_string(),
            "--fixtures".into(),
            t.rj()
                .join("examples/fixtures/bank-json_field.json")
                .display()
                .to_string(),
            "--calib".into(),
            t.bank_dir()
                .join(format!("entries/{F1}/calib"))
                .display()
                .to_string(),
            "--ledger-out".into(),
            led_b.display().to_string(),
            "--profile".into(),
            other,
        ],
    );
    assert!(ok, "{out}");
    let a = with_a.display().to_string();
    // 一份账本头有画像哈希：不再报「未核」
    let (ok, out) = t.bank_cmd(&["review", "--ledger", &a]);
    assert!(ok, "{out}");
    assert!(!out.contains("未核判断器版本"), "{out}");
    // 两份账本头的画像哈希不一致：如实列出，按「未核」处理
    let b = led_b.display().to_string();
    let (ok, out) = t.bank_cmd(&["review", "--ledger", &a, "--ledger", &b]);
    assert!(ok, "{out}");
    assert!(
        out.contains("不一致") && out.contains("未核判断器版本"),
        "{out}"
    );
}

// ---- 缺口 2：待重认与重取读数 ----

/// 重认用的配方：读数程序、材料（取自 json_field 的标注行）、固定观察夹具（读数取原读数，或被 `flip` 翻转）。
fn setup_recipe(t: &Tree, flip: bool) -> PathBuf {
    let dir = t.top.join("recert-work");
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("read.jpp"), READ_JPP).unwrap();
    let text = fs::read_to_string(t.bank_dir().join(format!("entries/{F1}/labels.jsonl"))).unwrap();
    let (mut items, mut obs) = (vec![], vec![]);
    for l in text.lines() {
        let r: Json = serde_json::from_str(l).unwrap();
        let Some(p) = r["p"].as_f64() else { continue };
        let p = if flip { 1.0 - p } else { p };
        items.push(json!({"id": r["item"], "json": r["text"], "field": "order_no"}));
        obs.push(json!({"on": [r["text"]], "op": "test",
            "text": "这个 JSON 是否有字段 order_no？（字段名必须完全一致）",
            "calib": "bank-l2-json-field", "answer": {"Noul": p}}));
    }
    fs::write(
        dir.join("materials.json"),
        json!({"items": items}).to_string(),
    )
    .unwrap();
    let fx = dir.join("fixtures.json");
    fs::write(
        &fx,
        json!({"description": "重认夹具", "observations": obs}).to_string(),
    )
    .unwrap();
    fs::write(
        t.bank_dir().join(format!("entries/{F1}/recert.json")),
        json!({"read": "../../../recert-work/read.jpp",
               "input": "../../../recert-work/materials.json"})
        .to_string(),
    )
    .unwrap();
    fx
}

/// 把基线的 render_version 改成别的，使当前二进制的环境相对基线「变了」。
fn stale_baseline(t: &Tree) {
    // Z0308：真实题库里已落账重认的条目带当前环境的 `recertified_at`（F3、F4 起），它们在换环境后按设计不待重认。
    // 模拟「全库在旧环境下认证过」要把这些 `recertified_at` 一并老化（保留字段，只改 render_version），
    // 再按库 API 重算的索引哈希写回，免得复审把它当手改
    let p = t.bank_dir().join("bank.json");
    let mut doc: Json = serde_json::from_slice(&fs::read(&p).unwrap()).unwrap();
    let mut aged = false;
    for e in doc["entries"].as_array_mut().unwrap() {
        if let Some(r) = e.get_mut("recertified_at") {
            r["render_version"] = json!("r0-old");
            aged = true;
        }
    }
    if aged {
        fs::write(&p, serde_json::to_string_pretty(&doc).unwrap()).unwrap();
        doc["index_hash"] = json!(QuestionBank::open(&t.bank_dir()).unwrap().index_hash());
        fs::write(&p, serde_json::to_string_pretty(&doc).unwrap()).unwrap();
    }
    stale_review_only(t);
}

/// 只把复审基线换成旧环境，不动条目的 `recertified_at`（测「当前环境重认过、但重认时没有判断器身份」用）。
fn stale_review_only(t: &Tree) {
    let mut b = QuestionBank::open(&t.bank_dir()).unwrap();
    b.record_review(&ReviewEnv {
        render_version: "r0-old".into(),
        profile_hash: None,
    });
    b.save().unwrap();
}

#[test]
fn env_change_blocks_record_until_recert_then_recert_clears_pending() {
    let t = Tree::new("g2");
    let fx = setup_recipe(&t, false);
    // 条目数从 bank.json 读，不写死（题库会增条目）
    let n = entry_hashes(&t).len();
    stale_baseline(&t);
    let prof = t.profile();
    let (ok, out) = t.bank_cmd(&["review", "--profile", &prof]);
    assert!(
        ok && out.contains("全库到期") && out.contains("render_version"),
        "{out}"
    );
    // 环境变了、条目没在当前环境重认：拒写基线，逐条列出
    let (ok, out) = t.bank_cmd(&["review", "--record", "--profile", &prof]);
    let blockers = out.split("拒绝写复审基线").nth(1).unwrap_or("");
    assert!(
        !ok && blockers.matches("尚未在当前环境重取读数重认").count() == n,
        "{out}"
    );
    // 显式标待重认：全部条目有 recert，版本 +1，变更记录 +1
    let (v0, n0) = (t.version(), t.sections());
    let (ok, out) = t.bank_cmd(&["review", "--mark-pending", "--profile", &prof]);
    assert!(ok && out.contains(&format!("待重认：{n} 条")), "{out}");
    assert_eq!(t.version(), v0 + 1);
    assert_eq!(t.sections(), n0 + 1);
    let pending = t.bank()["entries"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|e| e.get("recert").is_some())
        .count();
    assert_eq!(pending, n);
    // 待重认的条目仍在岗、状态不变（不拦程序）
    assert_eq!(t.entry("json_field")["status"], "已认证·未复用");
    // 重认 json_field：固定观察，费用 0；版本 +1，变更记录 +1，recert 清除
    let (v1, n1) = (t.version(), t.sections());
    let (ok, out) = t.bank_cmd(&[
        "recert",
        "json_field",
        "--backend",
        "fixed",
        "--fixtures",
        fx.to_str().unwrap(),
        "--profile",
        &prof,
        "--reason",
        "测试重认",
    ]);
    assert!(ok, "{out}");
    assert!(out.contains("预计调用"), "花钱前先打印预计调用数：{out}");
    assert_eq!(t.version(), v1 + 1);
    assert_eq!(t.sections(), n1 + 1);
    let e = t.entry("json_field");
    assert!(e.get("recert").is_none());
    assert_eq!(
        e["recertified_at"]["render_version"],
        jpp_ir_render_version()
    );
    // 重认后的条目通过装载重跑认证
    QuestionBank::open(&t.bank_dir())
        .unwrap()
        .verify_calib(F1)
        .unwrap();
    // 其余 n − 1 条仍待重认：复审仍列出它们，基线仍不能写
    let (ok, out) = t.bank_cmd(&["review", "--profile", &prof]);
    assert!(ok && out.matches("待重认：").count() == n - 1, "{out}");
    let (ok, _) = t.bank_cmd(&["review", "--record", "--profile", &prof]);
    assert!(!ok);
}

#[test]
fn recert_with_readings_that_break_certification_leaves_the_entry_untouched() {
    let t = Tree::new("g2b");
    let fx = setup_recipe(&t, true);
    let dir = t.bank_dir().join(format!("entries/{F1}"));
    let labels0 = fs::read(dir.join("labels.jsonl")).unwrap();
    let calib0 = fs::read(dir.join(format!("calib/_form_{F1}.json"))).unwrap();
    let v0 = t.version();
    let n0 = t.sections();
    let prof = t.profile();
    let (ok, out) = t.bank_cmd(&[
        "recert",
        "json_field",
        "--backend",
        "fixed",
        "--fixtures",
        fx.to_str().unwrap(),
        "--profile",
        &prof,
        "--reason",
        "测试",
    ]);
    // 读数与标注反向：认证过不去（记录装载校验不过或 calib-import 失败），条目保持原状
    assert!(!ok && out.contains("重认不过"), "{out}");
    assert_eq!(fs::read(dir.join("labels.jsonl")).unwrap(), labels0);
    assert_eq!(
        fs::read(dir.join(format!("calib/_form_{F1}.json"))).unwrap(),
        calib0
    );
    assert_eq!(t.version(), v0);
    assert_eq!(t.sections(), n0);
}

#[test]
fn recert_checks_recipe_and_cost_cap_before_anything_runs() {
    let t = Tree::new("g2c");
    // 没有配方：花钱前就报，且不改任何文件（真实题库条目现在都带配方，先删掉 json_field 的）
    fs::remove_file(t.bank_dir().join(format!("entries/{F1}/recert.json"))).unwrap();
    let before = t.bank_text();
    let (ok, out) = t.bank_cmd(&[
        "recert",
        "json_field",
        "--backend",
        "fixed",
        "--reason",
        "x",
    ]);
    assert!(!ok && out.contains("没有重取读数的配方"), "{out}");
    assert_eq!(t.bank_text(), before);
    // 真机后端必须给 --max-cost；读数程序 budget 上限超过它就不跑
    let _ = setup_recipe(&t, false);
    let (ok, out) = t.bank_cmd(&["recert", "json_field", "--backend", "live", "--reason", "x"]);
    assert!(!ok && out.contains("--max-cost"), "{out}");
    let (ok, out) = t.bank_cmd(&[
        "recert",
        "json_field",
        "--backend",
        "live",
        "--max-cost",
        "0.01",
        "--reason",
        "x",
    ]);
    assert!(!ok && out.contains("超过 --max-cost"), "{out}");
}

// ---- 缺口 3：非在岗仍被引用 ----

#[test]
fn bank_stats_reports_retired_forms_still_in_use_and_lists_suspend_candidates_apart() {
    let t = Tree::new("g3");
    let (l1, _) = t.run_example("json_field", F1, None);
    let (l2, _) = t.run_example("answers_question", F2, None);
    let stats = |t: &Tree| -> Json {
        let (ok, out) = t.raw(
            &t.top,
            &[
                "bank-stats".into(),
                l1.display().to_string(),
                l2.display().to_string(),
                "--bank".into(),
                t.bank_dir().display().to_string(),
                "--json".into(),
            ],
        );
        assert!(ok, "{out}");
        serde_json::from_str(out.lines().find(|l| l.starts_with('{')).unwrap()).unwrap()
    };
    let j = stats(&t);
    assert_eq!(j["non_service_in_use"].as_array().unwrap().len(), 0);
    assert_eq!(j["forms"][F1]["calls"], 4);
    assert_eq!(j["forms"][F2]["calls"], 4);
    // 停岗候选按 B25 仍算在岗：单列，不进「非在岗」
    let mut b = QuestionBank::open(&t.bank_dir()).unwrap();
    b.suspend_candidate(F2).unwrap();
    b.save().unwrap();
    let j = stats(&t);
    assert_eq!(j["non_service_in_use"].as_array().unwrap().len(), 0);
    assert_eq!(j["suspend_candidates_in_use"].as_array().unwrap().len(), 1);
    // 退役后账本里仍有读数：如实报，数字不变
    let mut b = QuestionBank::open(&t.bank_dir()).unwrap();
    b.retire(F2, "测试").unwrap();
    b.save().unwrap();
    let j = stats(&t);
    let ns = j["non_service_in_use"].as_array().unwrap();
    assert_eq!(ns.len(), 1);
    assert_eq!(ns[0]["slug"], "answers_question");
    assert_eq!(ns[0]["status"], "退役");
    assert_eq!(ns[0]["calls"], 4);
    assert_eq!(j["forms"][F2]["calls"], 4);
}

#[test]
fn running_a_program_that_imports_a_retired_form_warns_but_does_not_block() {
    let t = Tree::new("g3b");
    let (_, ok_rep) = t.run_example("answers_question", F2, None);
    let warns = |r: &Json| -> usize {
        r["trace"]["warnings"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|w| w.as_str().unwrap_or("").starts_with("W-bank-status"))
            .count()
    };
    assert_eq!(warns(&ok_rep), 0, "在岗条目不报");
    let mut b = QuestionBank::open(&t.bank_dir()).unwrap();
    b.retire(F2, "测试").unwrap();
    b.save().unwrap();
    let (_, rep) = t.run_example("answers_question", F2, None);
    assert_eq!(warns(&rep), 1);
    assert!(
        rep["trace"]["warnings"]
            .as_array()
            .unwrap()
            .iter()
            .any(|w| w.as_str().unwrap_or("").contains("answers_question")
                && w.as_str().unwrap_or("").contains("退役"))
    );
    // 只报不拦：出口与结果同没退役时一致
    assert_eq!(rep["status"], ok_rep["status"]);
    assert_eq!(rep["value"], ok_rep["value"]);
}

// ---- 缺口 4：admit 强制复审 ----

#[test]
fn admit_requires_reviewer_a_real_commit_and_an_untouched_index() {
    let t = Tree::new("g4");
    t.force_status("json_field", "诊断通过");
    let head = t.head();
    let base = ["admit", "json_field", "--reason", "测试"];
    let go = |extra: &[&str]| -> (bool, String) {
        let mut a: Vec<&str> = base.to_vec();
        a.extend(extra);
        t.bank_cmd(&a)
    };
    // 缺复审人
    let (ok, out) = go(&["--prereg", &head]);
    assert!(!ok && out.contains("--reviewer"), "{out}");
    // 预注册不是仓库里的提交
    let (ok, out) = go(&["--prereg", "deadbeefdeadbeef", "--reviewer", "复审甲"]);
    assert!(!ok && out.contains("不是") && out.contains("提交"), "{out}");
    assert_eq!(t.entry("json_field")["status"], "诊断通过");
    // 手改 bank.json（没升版本）：拒
    let p = t.bank_dir().join("bank.json");
    let good = t.bank_text();
    fs::write(
        &p,
        good.replacen("\"grade\": \"formal\"", "\"grade\": \"trial\"", 1),
    )
    .unwrap();
    let (ok, out) = go(&["--prereg", &head, "--reviewer", "复审甲"]);
    assert!(!ok && out.contains("手改"), "{out}");
    fs::write(&p, &good).unwrap();
    // 三项都对：通过，条目记复审记录
    let v0 = t.version();
    let (ok, out) = go(&["--prereg", &head, "--reviewer", "复审甲"]);
    assert!(ok, "{out}");
    let e = t.entry("json_field");
    assert_eq!(e["status"], "已认证·未复用");
    assert_eq!(e["admitted"]["reviewer"], "复审甲");
    assert_eq!(e["admitted"]["prereg"], head);
    assert_eq!(e["admitted"]["at_version"], v0 + 1);
    assert_eq!(t.version(), v0 + 1);
}

#[test]
fn propose_records_the_proposer() {
    let t = Tree::new("g4b");
    fs::write(
        t.rj().join("lib/bank/tone.jpp"),
        "let tone = form(\"test\", \"这段话的语气是否友好？\", {calib: \"bank-new-form\"});\n",
    )
    .unwrap();
    let (ok, out) = t.bank_cmd(&[
        "propose",
        "tone",
        "--source",
        "测试",
        "--reason",
        "测试",
        "--who",
        "提出者乙",
    ]);
    assert!(ok, "{out}");
    assert_eq!(t.entry("tone")["proposer"], "提出者乙");
    // 提出后索引哈希与索引相符（不算手改）
    let (ok, out) = t.bank_cmd(&["review", "--profile", &t.profile()]);
    assert!(ok && !out.contains("index_hash 不符"), "{out}");
}

#[test]
fn mark_pending_needs_a_changed_environment() {
    let t = Tree::new("g2d");
    let before = t.bank_text();
    let (ok, out) = t.bank_cmd(&["review", "--mark-pending", "--profile", &t.profile()]);
    assert!(!ok && out.contains("没有变"), "{out}");
    assert_eq!(t.bank_text(), before);
}

#[test]
fn baseline_without_profile_is_a_note_not_a_blocker_once_a_profile_is_given() {
    let t = Tree::new("g5b");
    let prof = t.profile();
    let (ok, out) = t.bank_cmd(&["review", "--profile", &prof]);
    assert!(
        ok && out.contains("判断器基线") && !out.contains("未核判断器版本"),
        "{out}"
    );
    let (ok, out) = t.bank_cmd(&["review", "--record", "--profile", &prof]);
    assert!(ok, "{out}");
    // 基线现在有画像哈希：下次比得了；同一画像无到期
    let (ok, out) = t.bank_cmd(&["review", "--profile", &prof]);
    assert!(ok && out.contains("无到期、无问题"), "{out}");
    // 换一份画像：两侧都有哈希且不同，全库到期
    let other = t
        .rj()
        .join("profiles/judge-claude-p.json")
        .display()
        .to_string();
    let (ok, out) = t.bank_cmd(&["review", "--profile", &other]);
    assert!(ok && out.contains("判断器画像哈希由"), "{out}");
}

// ---- 缺口 2（路 A）：真实题库每个条目都带配方，固定观察重认一次全清 ----

/// 题库条目的认证读数（真机账本导出的固定观察夹具，在 `tests/fixtures` 里，工作区内；首批五条加第二批 C01）。
fn recert_fixtures() -> String {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/bank-recert/fixtures.json")
        .display()
        .to_string()
}

fn entry_hashes(t: &Tree) -> Vec<String> {
    t.bank()["entries"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["form_hash"].as_str().unwrap().to_string())
        .collect()
}

/// 条目 `labels.jsonl` 里带 `p` 的行：`(item, p, pick)`。
fn label_readings(t: &Tree, hash: &str) -> Vec<(String, f64, Option<u64>)> {
    fs::read_to_string(t.bank_dir().join(format!("entries/{hash}/labels.jsonl")))
        .unwrap()
        .lines()
        .filter(|l| !l.trim().is_empty())
        .filter_map(|l| {
            let r: Json = serde_json::from_str(l).unwrap();
            Some((
                r["item"].as_str()?.to_string(),
                r["p"].as_f64()?,
                r["pick"].as_u64(),
            ))
        })
        .collect()
}

/// 认证记录里决定出口的三个量：`(文件名, hi, lo, unsure_rate_delta)`。
fn entry_lines(t: &Tree, hash: &str) -> Vec<(String, f64, f64, Option<f64>)> {
    let dir = t.bank_dir().join(format!("entries/{hash}/calib"));
    let mut cs: Vec<_> = fs::read_dir(&dir)
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|x| x == "json"))
        .collect();
    cs.sort();
    cs.iter()
        .map(|c| {
            let r: Json = serde_json::from_slice(&fs::read(c).unwrap()).unwrap();
            (
                c.file_name().unwrap().to_string_lossy().to_string(),
                r["hi"].as_f64().unwrap(),
                r["lo"].as_f64().unwrap(),
                r["unsure_rate_delta"].as_f64(),
            )
        })
        .collect()
}

/// 真实题库全部条目一次 `recert --pending`（固定观察，发行画像；条目数从 bank.json 读）。F3、F4 用 27a-置换重跑的
/// 两序测量，C01 用第二批的真机读数，重认读数与入库标注行逐行相同，所以全部通过。
/// Z0334 起发行画像的 δ 取中段（profile_revision 2），入库证书是按旧画像尾段 δ 认的：重认后每条的线随 δ 迁移而变，
/// `recert` 只在「新旧 δ 不同、用旧 δ 按同一认证路径重算的线与旧记录在容差 1e-9 内相同」时放行，并写明迁移（旧 δ → 新 δ）；
/// 这里核每条都走这一支、`unsure_rate_delta` 变成画像中段 δ。
///
/// Z0238：`jpp-calib/src/truth.rs` 修好之前，select、measure 行的 δ 先验按 noul 列取，F3、F4、F5 重认出的线与
/// 入库线不同而被 `line_drift` 拒（这条测试那时断言「被拒三条」）；修好后改回全部通过
/// （`地基/过程记录/工程-Z0238-认证δ映射.md` §4.5）。
#[test]
fn real_bank_recert_pending_clears_all_entries_with_recorded_readings() {
    let t = Tree::new("g2real");
    let hs = entry_hashes(&t);
    let n = hs.len();
    let before: Vec<_> = hs.iter().map(|h| label_readings(&t, h)).collect();
    let lines_before: Vec<_> = hs.iter().map(|h| entry_lines(&t, h)).collect();
    stale_baseline(&t);
    let prof = t.profile();
    let (ok, out) = t.bank_cmd(&["review", "--mark-pending", "--profile", &prof]);
    assert!(ok && out.contains(&format!("待重认：{n} 条")), "{out}");
    let (v1, n1) = (t.version(), t.sections());
    let (ok, out) = t.bank_cmd(&[
        "recert",
        "--pending",
        "--backend",
        "fixed",
        "--fixtures",
        &recert_fixtures(),
        "--profile",
        &prof,
        "--reason",
        "测试：真实题库全部条目一次重认",
    ]);
    assert!(ok, "{out}");
    assert_eq!(out.matches("重认通过").count(), n, "{out}");
    assert_eq!(out.matches("认证结果与旧记录不一致").count(), 0, "{out}");
    assert_eq!(
        out.matches("线变化来自画像 δ 迁到 mid").count(),
        n,
        "每条的线变化都要由 δ 迁移解释：{out}"
    );
    let log = fs::read_to_string(t.top.join("地基/题库/变更记录.md")).unwrap();
    let last = log.rsplit("\n## ").next().unwrap();
    assert_eq!(
        last.matches("线变化来自画像 δ 迁到 mid").count(),
        n,
        "变更记录新一节逐条写明迁移：{last}"
    );
    assert_eq!(t.version(), v1 + 1, "整批只升一次版本");
    assert_eq!(t.sections(), n1 + 1);
    let b = QuestionBank::open(&t.bank_dir()).unwrap();
    for (k, h) in hs.iter().enumerate() {
        let e = t.bank()["entries"]
            .as_array()
            .unwrap()
            .iter()
            .find(|e| e["form_hash"] == h.as_str())
            .unwrap()
            .clone();
        assert!(e.get("recert").is_none(), "{h}");
        b.verify_calib(h).unwrap();
        // 读数与入库逐行相同（含 pick）
        assert_eq!(label_readings(&t, h), before[k], "{h}");
        // Z0334：同读数、同认证程序，δ 从旧画像尾段迁到发行画像中段，`unsure_rate_delta` 按题型变成中段 δ
        // （test 0.1281 / select 0.0971 / measure 0.0821）；线与旧记录的一致性由 recert 用旧 δ 重算核过
        let want = match e["op"].as_str().unwrap() {
            "test" => 0.1281,
            "select" => 0.0971,
            "measure" => 0.0821,
            op => panic!("{h} 未知 op {op}"),
        };
        let now = entry_lines(&t, h);
        assert_eq!(now.len(), lines_before[k].len(), "{h}");
        for (a, o) in now.iter().zip(&lines_before[k]) {
            assert_eq!(a.0, o.0, "{h}");
            let d = a.3.expect("重认记录绑 δ");
            assert!(
                (d - want).abs() < 1e-12,
                "{h} {} unsure_rate_delta 应为 {} 的中段 δ {want}：{:?} → {d}",
                a.0,
                e["op"],
                o.3
            );
            assert_ne!(o.3, Some(d), "{h} {} δ 没迁移", a.0);
        }
    }
    // 给了 `--profile`：重认后写基线不需要任何豁免
    let (ok, out) = t.bank_cmd(&["review", "--record", "--profile", &prof]);
    assert!(ok, "{out}");
}

/// 条目目录里 `labels.jsonl` 与 `calib/*.json` 的字节（比较「没动」用）。
fn entry_bytes(t: &Tree, hash: &str) -> Vec<(String, Vec<u8>)> {
    let dir = t.bank_dir().join(format!("entries/{hash}"));
    let mut v = vec![(
        "labels.jsonl".to_string(),
        fs::read(dir.join("labels.jsonl")).unwrap(),
    )];
    let mut cs: Vec<_> = fs::read_dir(dir.join("calib"))
        .unwrap()
        .map(|e| e.unwrap().path())
        .collect();
    cs.sort();
    for c in cs {
        v.push((
            c.file_name().unwrap().to_string_lossy().to_string(),
            fs::read(&c).unwrap(),
        ));
    }
    v
}

/// 复核 B0465 条件 (ii) 的拒绝分支：读数逐行不变而认证线变了，`recert` 拒绝写回，条目逐字节不变。
/// 强制手段：把入库记录的 `hi` 改掉（模拟旧线不是这条认证路径按旧 δ 算出来的），再用发行画像重认 F3
/// （which_named，置换重跑两序读数与入库逐行相同）。Z0334 起新旧 δ 不同（0.0781 → 0.0971）本身可以放行，
/// 但用旧 δ 按同一认证路径重算的线对不上被改过的旧线，`recert` 必须照样拒（主控 2026-09-30 定的收紧判据）。
/// （Z0238 修好 δ 映射后，真实题库重认全部通过，这条分支不再被那条测试打到，所以单独钉住。）
#[test]
fn real_bank_recert_refuses_line_change_from_a_different_delta_profile() {
    let t = Tree::new("g2drift");
    let f3 = "e49770913911a79525896988"; // which_named（F3）
    stale_baseline(&t);
    let prof = t.profile();
    let (ok, out) = t.bank_cmd(&["review", "--mark-pending", "--profile", &prof]);
    assert!(ok, "{out}");
    // 旧记录的线不是按旧 δ 算出来的：把入库 hi 改掉一位
    let rec = t
        .bank_dir()
        .join(format!("entries/{f3}/calib/_form_{f3}.json"));
    let mut r: Json = serde_json::from_str(&fs::read_to_string(&rec).unwrap()).unwrap();
    assert_eq!(r["hi"], serde_json::json!(0.9169));
    r["hi"] = serde_json::json!(0.9168);
    fs::write(&rec, serde_json::to_string_pretty(&r).unwrap()).unwrap();
    let other = std::path::PathBuf::from(&prof);
    let bytes_before = entry_bytes(&t, f3);
    let v1 = t.version();
    let (ok, out) = t.bank_cmd(&[
        "recert",
        "which_named",
        "--backend",
        "fixed",
        "--fixtures",
        &recert_fixtures(),
        "--profile",
        &other.display().to_string(),
        "--reason",
        "测试：δ 先验不同的画像重认",
    ]);
    assert!(!ok, "读数未变而线变了，要拒绝写回：{out}");
    assert!(out.contains("认证结果与旧记录不一致"), "{out}");
    assert!(out.contains("hi 旧 0.9168"), "拒因要列出前后值：{out}");
    assert!(
        out.contains("用旧 δ 0.0781 按同一认证路径重算，线与旧记录对不上"),
        "拒因要说明不是 δ 迁移：{out}"
    );
    assert!(!out.contains("重认通过"), "{out}");
    let e = t.bank()["entries"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["form_hash"] == f3)
        .unwrap()
        .clone();
    assert!(e.get("recert").is_some(), "被拒：条目保持待重认");
    assert_eq!(
        entry_bytes(&t, f3),
        bytes_before,
        "标注集与认证记录逐字节不变"
    );
    assert_eq!(t.version(), v1, "没有条目落账，版本不升");
}

#[test]
fn recert_reports_missing_judge_identity_and_unknown_identity_is_not_taken_as_unchanged() {
    let t = Tree::new("g5r");
    let n = entry_hashes(&t).len();
    stale_baseline(&t);
    let prof = t.profile();
    let (ok, out) = t.bank_cmd(&["review", "--mark-pending", "--profile", &prof]);
    assert!(ok, "{out}");
    // 不给 `--profile`（测试二进制旁也没有默认画像）：身份取不到，花钱前就提示；
    // 认证导入要 δ、只从画像取，所以这次重认不过，条目保持原状
    let before = t.bank_text();
    let (ok, out) = t.bank_cmd(&[
        "recert",
        "--pending",
        "--backend",
        "fixed",
        "--fixtures",
        &recert_fixtures(),
        "--reason",
        "测试：无身份重认",
    ]);
    assert!(!ok && out.contains("判断器身份未取到"), "{out}");
    assert_eq!(t.bank_text(), before, "重认不过：条目保持原状");
    // 有身份的重认过后，把条目上记的画像哈希手改成 null（重认时没取到身份的样子），再把基线改旧使环境「变了」
    let (ok, out) = t.bank_cmd(&[
        "recert",
        "--pending",
        "--backend",
        "fixed",
        "--fixtures",
        &recert_fixtures(),
        "--profile",
        &prof,
        "--reason",
        "测试：有身份重认",
    ]);
    assert!(ok, "{out}");
    let p = t.bank_dir().join("bank.json");
    let text = fs::read_to_string(&p).unwrap();
    let mut doc: Json = serde_json::from_str(&text).unwrap();
    for e in doc["entries"].as_array_mut().unwrap() {
        e["recertified_at"]["profile_hash"] = Json::Null;
    }
    fs::write(&p, serde_json::to_string_pretty(&doc).unwrap()).unwrap();
    // 条目的重认是在当前环境做的（只缺判断器身份）：只老化复审基线，不老化 recertified_at（Z0308）
    stale_review_only(&t);
    // 带画像复审：全部条目都因「重认时没有判断器身份」重新列问题（不把未知当作没变）
    let (ok, out) = t.bank_cmd(&["review", "--profile", &prof]);
    assert!(ok, "{out}");
    assert_eq!(out.matches("基线未含画像，需重认").count(), n, "{out}");
    let (ok, out) = t.bank_cmd(&["review", "--record", "--profile", &prof]);
    assert!(!ok && out.contains("基线未含画像，需重认"), "{out}");
}

#[test]
fn recert_without_profile_flag_uses_the_default_profile_beside_the_executable() {
    let t = Tree::new("g5d");
    let n = entry_hashes(&t).len();
    stale_baseline(&t);
    let prof = t.profile();
    let (ok, out) = t.bank_cmd(&["review", "--mark-pending", "--profile", &prof]);
    assert!(ok, "{out}");
    // 默认画像在可执行文件旁 `profiles/`：把二进制连同发行画像复制到临时目录，从那里跑
    let bin = t.top.join("bin");
    copy_dir(&root().join("profiles"), &bin.join("profiles"));
    let exe = bin.join("jpp");
    fs::copy(env!("CARGO_BIN_EXE_jpp"), &exe).unwrap();
    let o = Command::new(&exe)
        .current_dir(&t.top)
        .args([
            "bank",
            "recert",
            "--pending",
            "--backend",
            "fixed",
            "--fixtures",
            &recert_fixtures(),
            "--reason",
            "测试：默认画像重认",
            "--bank",
            &t.bank_dir().display().to_string(),
        ])
        .output()
        .unwrap();
    let out = format!(
        "{}{}",
        String::from_utf8_lossy(&o.stdout),
        String::from_utf8_lossy(&o.stderr)
    );
    assert!(o.status.success(), "{out}");
    assert!(!out.contains("判断器身份未取到"), "{out}");
    assert_eq!(out.matches("重认通过").count(), n, "{out}");
    // 全部条目记下的画像哈希都是非 null 的同一个值
    let hashes: Vec<Json> = t.bank()["entries"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["recertified_at"]["profile_hash"].clone())
        .collect();
    assert_eq!(hashes.len(), n);
    assert!(hashes[0].is_string(), "{hashes:?}");
    assert!(hashes.iter().all(|h| *h == hashes[0]), "{hashes:?}");
    // 它就是发行画像的哈希：带 `--profile` 复审，全部条目都不再列问题
    let (ok, out) = t.bank_cmd(&["review", "--profile", &prof]);
    assert!(
        ok && !out.contains("基线未含画像") && !out.contains("尚未在当前环境"),
        "{out}"
    );
}

#[test]
fn recert_accepts_relative_profile_and_fixtures_paths() {
    let t = Tree::new("g2rel");
    stale_baseline(&t);
    let prof = t.profile();
    let (ok, out) = t.bank_cmd(&["review", "--mark-pending", "--profile", &prof]);
    assert!(ok, "{out}");
    // 相对当前目录（临时树顶层）的路径：子进程的工作目录不是它，转绝对路径后才找得到
    fs::copy(recert_fixtures(), t.top.join("fx.json")).unwrap();
    let (ok, out) = t.bank_cmd(&[
        "recert",
        "json_field",
        "--backend",
        "fixed",
        "--fixtures",
        "fx.json",
        "--profile",
        &format!("地基/rust-jpp/{PROFILE}"),
        "--reason",
        "测试：相对路径",
    ]);
    assert!(ok && out.contains("json_field：重认通过"), "{out}");
}

/// 固定观察里改一条读数：`calib` 为 `key` 的观察里 Noul 最小的那条加 `bump`，写成新夹具文件。
fn shifted_fixtures(t: &Tree, key: &str, bump: f64) -> String {
    let mut fx: Json =
        serde_json::from_str(&fs::read_to_string(recert_fixtures()).unwrap()).unwrap();
    let obs = fx["observations"].as_array_mut().unwrap();
    let o = obs
        .iter_mut()
        .filter(|o| o["calib"] == key)
        .min_by(|a, b| {
            a["answer"]["Noul"]
                .as_f64()
                .unwrap()
                .total_cmp(&b["answer"]["Noul"].as_f64().unwrap())
        })
        .expect("夹具里有这条键的读数");
    let p = o["answer"]["Noul"].as_f64().unwrap();
    o["answer"]["Noul"] = serde_json::json!(((p + bump) * 1e6).round() / 1e6);
    let path = t.top.join("fixtures-shift.json");
    fs::write(&path, fx.to_string()).unwrap();
    path.display().to_string()
}

/// Z0334 复核 E5（阻断项）：读数有变的重认不进闸门，δ 照样按新画像迁移，输出与变更记录也要写明「旧 δ → 新 δ」。
/// C01（hard_to_undo）最小的一条读数加 0.001（离线很远），只重认这一条。
#[test]
fn recert_with_changed_readings_still_logs_delta_migration() {
    let t = Tree::new("z0334e5");
    stale_baseline(&t);
    let prof = t.profile();
    let (ok, out) = t.bank_cmd(&["review", "--mark-pending", "--profile", &prof]);
    assert!(ok, "{out}");
    let fx = shifted_fixtures(&t, "attr-hard-to-undo", 0.001);
    let (ok, out) = t.bank_cmd(&[
        "recert",
        "hard_to_undo",
        "--backend",
        "fixed",
        "--fixtures",
        &fx,
        "--profile",
        &prof,
        "--reason",
        "测试：读数有变的重认记 δ 迁移",
    ]);
    assert!(ok, "{out}");
    assert!(out.contains("重认通过"), "{out}");
    assert!(
        !out.contains("线变化来自画像 δ 迁到 mid"),
        "读数变了，不进闸门：{out}"
    );
    assert!(out.contains("δ 0.04 → 0.1281"), "输出写明 δ 迁移：{out}");
    let log = fs::read_to_string(t.top.join("地基/题库/变更记录.md")).unwrap();
    let last = log.rsplit("\n## ").next().unwrap();
    assert!(
        last.contains("δ 0.04 → 0.1281"),
        "变更记录写明 δ 迁移：{last}"
    );
}

/// Z0334 复核 E3：新旧 δ 相同而线变（旧 `hi` 被改过）→ 拒，报「新旧 δ 相同」，条目逐字节不变。
#[test]
fn recert_refuses_line_change_when_delta_is_unchanged() {
    let t = Tree::new("z0334e3");
    let f3 = "e49770913911a79525896988"; // which_named（F3）
    stale_baseline(&t);
    let prof = t.profile();
    let (ok, out) = t.bank_cmd(&["review", "--mark-pending", "--profile", &prof]);
    assert!(ok, "{out}");
    let rec = t
        .bank_dir()
        .join(format!("entries/{f3}/calib/_form_{f3}.json"));
    let mut r: Json = serde_json::from_str(&fs::read_to_string(&rec).unwrap()).unwrap();
    r["hi"] = serde_json::json!(0.9168);
    fs::write(&rec, serde_json::to_string_pretty(&r).unwrap()).unwrap();
    // 画像副本：K 选一中段 δ 设回入库证书的 0.0781，新旧 δ 相同
    let mut doc: Json = serde_json::from_str(&fs::read_to_string(&prof).unwrap()).unwrap();
    doc["delta"]["choice_prob_chosen"]["mid"]["immediate"]["p99"] = serde_json::json!(0.0781);
    let same = t.top.join("same-delta.json");
    fs::write(&same, doc.to_string()).unwrap();
    let before = entry_bytes(&t, f3);
    let v = t.version();
    let (ok, out) = t.bank_cmd(&[
        "recert",
        "which_named",
        "--backend",
        "fixed",
        "--fixtures",
        &recert_fixtures(),
        "--profile",
        &same.display().to_string(),
        "--reason",
        "测试：δ 相同而线变",
    ]);
    assert!(!ok, "{out}");
    assert!(out.contains("新旧 δ 相同（0.0781）"), "{out}");
    assert!(!out.contains("重认通过"), "{out}");
    assert_eq!(entry_bytes(&t, f3), before, "条目逐字节不变");
    assert_eq!(t.version(), v);
}

/// Z0334 复核 E2b：旧记录样本的 `phys` 认不出时，不知道旧 δ 该写哪一列 → 拒（不写三列），条目逐字节不变。
#[test]
fn recert_refuses_delta_migration_when_record_type_is_unknown() {
    let t = Tree::new("z0334e2b");
    let f3 = "e49770913911a79525896988"; // which_named（F3）
    stale_baseline(&t);
    let prof = t.profile();
    let (ok, out) = t.bank_cmd(&["review", "--mark-pending", "--profile", &prof]);
    assert!(ok, "{out}");
    let rec = t
        .bank_dir()
        .join(format!("entries/{f3}/calib/_form_{f3}.json"));
    let mut r: Json = serde_json::from_str(&fs::read_to_string(&rec).unwrap()).unwrap();
    for smp in r["samples"].as_array_mut().unwrap() {
        smp["phys"] = serde_json::json!("xx");
    }
    fs::write(&rec, serde_json::to_string_pretty(&r).unwrap()).unwrap();
    let before = entry_bytes(&t, f3);
    let v = t.version();
    let (ok, out) = t.bank_cmd(&[
        "recert",
        "which_named",
        "--backend",
        "fixed",
        "--fixtures",
        &recert_fixtures(),
        "--profile",
        &prof,
        "--reason",
        "测试：题型认不出",
    ]);
    assert!(!ok, "{out}");
    assert!(out.contains("旧记录没有可认的题型"), "{out}");
    assert_eq!(entry_bytes(&t, f3), before, "条目逐字节不变");
    assert_eq!(t.version(), v, "没有条目落账，版本不升");
}
