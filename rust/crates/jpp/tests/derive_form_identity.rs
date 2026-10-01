//! 步 28（B0468 阻断项、B45）：派生题的题式身份（过程记录 §16.1 第 1 项）。
//! (a) 每道派生题由题式填出，`calib_ref` 写它自己的题式键，一个题式一个标签，不再共用 `"derive"`；
//! (b) 唤出题经题库上岗（`bank propose --derived-by elicit` → `bank diagnose` → `calib-import` 认证 →
//!     `derive-admit` 留出不劣 → `bank admit`），再跑同一个 chain、带该条目的校准目录：这道题的 cut 用上那条线。
//! 闭包端口，不发请求；题库在临时目录的副本里（git 仓库，与 bank_lifecycle.rs 同法）。

mod derive_support;
use derive_support::*;
use jpp::effects::CalibStore;
use jpp::value::Answer;
use serde_json::{Value as Json, json};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

fn noul(p: f64) -> Answer {
    Answer::Noul(p)
}

#[test]
fn a_派生题各带自己的题式键_不共用标签() {
    let src = r#"budget {calls: 30, cost: 0, depth: 512};
let why = {params: [{slot: "主要原因", choices: ["价格", "服务"]}]};
let root = form("test", "这位客户会续约吗？", {calib: "t-root", on: {act: why}});
let first = fn(item) { if has(item, "m") { {q: measure("客户满意度有多高？", ["低", "中", "高"], "t-m"), line: {declare: {hi: 0.7}}} }
                       else { {form: root} } };
let r = chain([{on: mat("客户的沟通记录")}, {on: mat("客户的评价"), m: true}], first, 2, {judge_diag: false});
"#
    .to_string()
        + 读出;
    let r = 跑(
        &src,
        |t, _q, _s| {
            if t.contains("满意度有多高") && !t.contains("至少达到") {
                Answer::Score(vec![0.3, 0.4, 0.3])
            } else if t.contains("续约吗") && !t.contains("已判") {
                noul(0.8)
            } else if t.contains("最可能是哪一项") {
                Answer::Choice(vec![0.7, 0.3])
            } else {
                noul(0.6)
            }
        },
        vec![],
    )
    .unwrap();
    let refs = 判断引用(&r.ledger);
    let derived: Vec<&(String, jpp_ir::key::CalibRef)> = refs
        .iter()
        .filter(|(_, c)| c.declared.starts_with("derive"))
        .collect();
    assert_eq!(derived.len(), 3, "填空题 1 道、程度细化 2 道：{refs:?}");
    for (_, c) in &derived {
        assert_ne!(c.declared, "derive", "不再共用一个标签");
        let h = c
            .declared
            .strip_prefix("derive:")
            .expect("标签是「derive:题式哈希」");
        assert_eq!(
            c.key.as_deref(),
            Some(format!("\u{1f}form\u{1f}{h}").as_str()),
            "calib_ref 写题式键"
        );
    }
    let keys: std::collections::BTreeSet<&str> = derived
        .iter()
        .filter_map(|(_, c)| c.key.as_deref())
        .collect();
    assert_eq!(
        keys.len(),
        2,
        "细化的两道共用一个题式（填法继承），填空题另一个"
    );
}

fn copy_dir(from: &Path, to: &Path) {
    fs::create_dir_all(to).unwrap();
    for e in fs::read_dir(from).unwrap() {
        let e = e.unwrap();
        let (f, t) = (e.path(), to.join(e.file_name()));
        if f.is_dir() {
            copy_dir(&f, &t)
        } else {
            fs::copy(&f, &t).map(|_| ()).unwrap()
        }
    }
}

fn 命令(top: &Path, args: &[&str]) -> (bool, String) {
    let o = Command::new(env!("CARGO_BIN_EXE_jpp"))
        .current_dir(top)
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

const 唤出题: &str = "提案里写明了资金来源吗？";

const 程序: &str = r#"budget {calls: 30, cost: 0, depth: 512};
let first = fn(item) { {q: test("这份提案可行吗？", "t-root"), line: {declare: {hi: 0.7, lo: 0.3}}} };
let r = chain([{on: mat("一份提案：预算 20 万，资金来自天使投资。")}], first, 2, {elicit: true, judge_diag: false});
{path: map(r.value, fn(v) { map(v.path, fn(p) { p.by + ":" + exit_kind(p.exit) }) }), pending: r.pending}
"#;

/// 默认链「为什么拿不准」那道 K 选一（候选类别加「两可」「题不清」）选第一项
fn 选第一项(k: usize) -> Answer {
    let mut v = vec![0.1 / (k - 1) as f64; k];
    v[0] = 0.9;
    Answer::Choice(v)
}

fn 读数(t: &str, _q: &jpp::value::Question, s: &jpp::value::State) -> Answer {
    if t.contains("可行吗") {
        noul(0.5)
    } else if t == 唤出题 {
        noul(0.6)
    } else if t.contains("为什么拿不准") {
        // Z0514：派生不出的未决先走默认链，「为什么拿不准」选「材料」，路 C 转交（仍在 pending）
        选第一项(s.over.len())
    } else {
        panic!("夹具没有：{t}")
    }
}

#[test]
fn b_唤出题经题库上岗后_下次运行_cut_用上它的线() {
    let 生成 = vec![json!({"op": "test", "text": 唤出题})];
    // 首跑：没有校准记录，唤出题按判断器的回答走（0.6 → act），calib_ref 写它的题式键
    let r1 = 跑(程序, 读数, 生成.clone()).unwrap();
    let refs1 = 判断引用(&r1.ledger);
    let (_, c1) = refs1
        .iter()
        .find(|(_, c)| c.declared.starts_with("derive:"))
        .expect("唤出题的判断条目");
    let k1 = c1.key.clone().expect("题式键");
    let v1: Json = r1.out.value_json();
    assert_eq!(
        v1["path"],
        json!([["author:unsure(band)", "elicit:act"]]),
        "{v1}"
    );

    // 题库副本：唤出题抄成题式文件（标签随意，form_hash 不含标签），带派生来源提出
    let top = std::env::temp_dir().join(format!("jpp-derive-form-{}", std::process::id()));
    let _ = fs::remove_dir_all(&top);
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let rj = top.join("地基/rust-jpp");
    copy_dir(&root.join("bank"), &rj.join("bank"));
    copy_dir(&root.join("lib/bank"), &rj.join("lib/bank"));
    fs::write(
        rj.join("lib/bank/funding_stated.jpp"),
        format!("let funding_stated = form(\"test\", \"{唤出题}\", {{calib: \"bank-funding-stated\"}});\n"),
    )
    .unwrap();
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
    let bank = rj.join("bank");
    let b = bank.to_str().unwrap();
    let (ok, out) = 命令(
        &top,
        &[
            "bank",
            "propose",
            "funding_stated",
            "--source",
            "derive 唤出",
            "--reason",
            "测试",
            "--derived-by",
            "elicit",
            "--bank",
            b,
        ],
    );
    assert!(ok, "{out}");
    let doc: Json =
        serde_json::from_str(&fs::read_to_string(bank.join("bank.json")).unwrap()).unwrap();
    let e = doc["entries"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["slug"] == "funding_stated")
        .unwrap()
        .clone();
    let h = e["form_hash"].as_str().unwrap().to_string();
    assert_eq!(
        format!("\u{1f}form\u{1f}{h}"),
        k1,
        "题库里的题式与运行时派生题同一个 form_hash"
    );
    let (ok, out) = 命令(&top, &["bank", "diagnose", "funding_stated", "--bank", b]);
    assert!(ok, "{out}");

    // 认证：40 条带真值的读数（题式级），写进条目目录
    let dir = bank.join("entries").join(&h);
    let rows: Vec<String> = (0..40)
        .map(|i| {
            let yes = i % 2 == 0;
            json!({"form": {"op": "test", "template": 唤出题}, "item": format!("l{i}"),
                   "p": if yes { 0.95 } else { 0.05 }, "label": yes, "source": "computed"})
            .to_string()
        })
        .collect();
    fs::write(dir.join("labels.jsonl"), rows.join("\n") + "\n").unwrap();
    let (ok, out) = 命令(
        &top,
        &[
            "calib-import",
            dir.join("labels.jsonl").to_str().unwrap(),
            "--calib-out",
            dir.join("calib").to_str().unwrap(),
            "--profile",
            concat!(env!("CARGO_MANIFEST_DIR"), "/tests/delta_prior_legacy.json"),
        ],
    );
    assert!(ok, "{out}");

    // 留出比较不劣，上岗
    let holdout: Vec<String> = (0..5)
        .map(|i| json!({"item": format!("s{i}"), "set": "score"}).to_string())
        .chain((0..22).map(|i| {
            let yes = i % 2 == 0;
            let x = if yes { "act" } else { "ignore" };
            json!({"item": format!("h{i}"), "set": "holdout", "truth": yes, "orig": x, "cand": x})
                .to_string()
        }))
        .collect();
    let rows_p = top.join("rows.jsonl");
    fs::write(&rows_p, holdout.join("\n") + "\n").unwrap();
    let (ok, out) = 命令(
        &top,
        &[
            "derive-admit",
            "funding_stated",
            "--rows",
            rows_p.to_str().unwrap(),
            "--split",
            "按条目 id",
            "--seed",
            "1",
            "--prereg",
            &head,
            "--reviewer",
            "复审人",
            "--reason",
            "测试",
            "--bank",
            b,
        ],
    );
    assert!(ok, "{out}");
    let doc: Json =
        serde_json::from_str(&fs::read_to_string(bank.join("bank.json")).unwrap()).unwrap();
    let e = doc["entries"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["slug"] == "funding_stated")
        .unwrap()
        .clone();
    assert_eq!(e["status"], "已认证·未复用", "{e}");

    // 再跑同一个 chain，带条目的校准目录：唤出题的 cut 用上认证线（0.6 落在线之间 → band），calib_ref 同一个题式键
    let calib = CalibStore::load(&dir.join("calib")).unwrap();
    let r2 = 跑_带校准(程序, 读数, 生成, &calib).unwrap();
    let refs2 = 判断引用(&r2.ledger);
    let (_, c2) = refs2
        .iter()
        .find(|(_, c)| c.declared.starts_with("derive:"))
        .unwrap();
    assert_eq!(c2.key.as_deref(), Some(k1.as_str()));
    let v2: Json = r2.out.value_json();
    // 这一趟唤出题判出 band；再唤出的候选与它同题面（第②段 same-as-source 拒），它随返回值转交（pending）
    assert_eq!(v2["pending"][0]["q"]["text"], json!(唤出题), "{v2}");
    assert_eq!(
        v2["pending"][0]["exit"]["exit"],
        json!("unsure(band)"),
        "{v2}"
    );
    assert!(
        r2.ledger.calib_used.contains_key(&k1),
        "用到的题式级记录进账本"
    );
    let _ = fs::remove_dir_all(&top);
}

/// 把一个派生题式抄进题库副本并走完上岗：propose（带派生方式）→ diagnose → calib-import（题式级，带填法）→
/// derive-admit（留出不劣，--prereg --reviewer）。返回题式哈希与条目的校准记录。
fn 题库上岗(
    tag: &str,
    slug: &str,
    template: &str,
    derived_by: &str,
    fills: &[(&str, &str)],
) -> (String, CalibStore) {
    let top = std::env::temp_dir().join(format!("jpp-derive-form-{tag}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&top);
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let rj = top.join("地基/rust-jpp");
    copy_dir(&root.join("bank"), &rj.join("bank"));
    copy_dir(&root.join("lib/bank"), &rj.join("lib/bank"));
    fs::write(
        rj.join(format!("lib/bank/{slug}.jpp")),
        format!(
            "let {slug} = form(\"test\", {}, {{calib: \"bank-{slug}\"}});\n",
            json!(template)
        ),
    )
    .unwrap();
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
    let bank = rj.join("bank");
    let b = bank.to_str().unwrap();
    let (ok, out) = 命令(
        &top,
        &[
            "bank",
            "propose",
            slug,
            "--source",
            "derive 派生",
            "--reason",
            "测试",
            "--derived-by",
            derived_by,
            "--bank",
            b,
        ],
    );
    assert!(ok, "{out}");
    let entry = || -> Json {
        let doc: Json =
            serde_json::from_str(&fs::read_to_string(bank.join("bank.json")).unwrap()).unwrap();
        doc["entries"]
            .as_array()
            .unwrap()
            .iter()
            .find(|e| e["slug"] == slug)
            .unwrap()
            .clone()
    };
    let h = entry()["form_hash"].as_str().unwrap().to_string();
    let (ok, out) = 命令(&top, &["bank", "diagnose", slug, "--bank", b]);
    assert!(ok, "{out}");
    let dir = bank.join("entries").join(&h);
    let rows: Vec<String> = (0..40)
        .map(|i| {
            let yes = i % 2 == 0;
            let (k, v) = fills[i % fills.len()];
            json!({"form": {"op": "test", "template": template}, "fill": {k: v}, "item": format!("l{i}"),
                   "p": if yes { 0.95 } else { 0.05 }, "label": yes, "source": "computed"})
            .to_string()
        })
        .collect();
    fs::write(dir.join("labels.jsonl"), rows.join("\n") + "\n").unwrap();
    let (ok, out) = 命令(
        &top,
        &[
            "calib-import",
            dir.join("labels.jsonl").to_str().unwrap(),
            "--calib-out",
            dir.join("calib").to_str().unwrap(),
            "--profile",
            concat!(env!("CARGO_MANIFEST_DIR"), "/tests/delta_prior_legacy.json"),
        ],
    );
    assert!(ok, "{out}");
    let holdout: Vec<String> = (0..5)
        .map(|i| json!({"item": format!("s{i}"), "set": "score"}).to_string())
        .chain((0..22).map(|i| {
            let yes = i % 2 == 0;
            let x = if yes { "act" } else { "ignore" };
            json!({"item": format!("h{i}"), "set": "holdout", "truth": yes, "orig": x, "cand": x})
                .to_string()
        }))
        .collect();
    let rows_p = top.join("rows.jsonl");
    fs::write(&rows_p, holdout.join("\n") + "\n").unwrap();
    let (ok, out) = 命令(
        &top,
        &[
            "derive-admit",
            slug,
            "--rows",
            rows_p.to_str().unwrap(),
            "--split",
            "按条目 id",
            "--seed",
            "1",
            "--prereg",
            &head,
            "--reviewer",
            "复审人",
            "--reason",
            "测试",
            "--bank",
            b,
        ],
    );
    assert!(ok, "{out}");
    assert_eq!(entry()["status"], "已认证·未复用", "{}", entry());
    assert_eq!(
        entry()["provenance"]["derived_by"],
        json!(jpp::store::bank::Provenance::canonical_derived_by(
            derived_by
        ))
    );
    let calib = CalibStore::load(&dir.join("calib")).unwrap();
    let _ = fs::remove_dir_all(&top);
    (h, calib)
}

const 细化程序: &str = r#"budget {calls: 30, cost: 0, depth: 512};
let first = fn(item) { {q: measure("这份提案的风险有多大？", ["低", "中", "高"], "t-root"), line: {declare: {hi: 0.7}}} };
let r = chain([{on: mat("一份提案：预算 20 万，资金来自天使投资。")}], first, 2, {judge_diag: false});
{path: map(r.value, fn(v) { map(v.path, fn(p) { p.by + ":" + exit_kind(p.exit) }) }),
 pending: map(r.pending, fn(p) { {q: p.q.text, kind: exit_kind(p.exit), exit: p.exit} })}
"#;

fn 细化读数(t: &str, _q: &jpp::value::Question, s: &jpp::value::State) -> Answer {
    if t.contains("为什么拿不准") {
        // Z0514：同上，默认链路 C 转交
        选第一项(s.over.len())
    } else if t.contains("至少达到") {
        noul(0.6)
    } else if t.contains("风险有多大") {
        Answer::Score(vec![0.3, 0.4, 0.3])
    } else {
        panic!("夹具没有：{t}")
    }
}

/// (c) 带槽的派生题式（划分细化「至少达到第 j 档」，档位是槽 {bin}）经题库上岗后，下次运行两道细化题的 cut 都用上
/// 那一条题式线（填法继承，R2-3）。
#[test]
fn c_带槽的细化题式经题库上岗后_下次运行两道都用上它的线() {
    // 首跑：没有校准记录，两道细化题读数 0.6 照回答走（act），同一个题式键
    let r1 = 跑(细化程序, 细化读数, vec![]).unwrap();
    let refs1: Vec<jpp_ir::key::CalibRef> = 判断引用(&r1.ledger)
        .into_iter()
        .filter(|(_, c)| c.declared.starts_with("derive:"))
        .map(|(_, c)| c)
        .collect();
    assert_eq!(refs1.len(), 2, "{refs1:?}");
    let k1 = refs1[0].key.clone().expect("题式键");
    assert!(
        refs1.iter().all(|c| c.key.as_deref() == Some(k1.as_str())),
        "两道细化题同一个题式"
    );
    let fills: Vec<String> = refs1
        .iter()
        .map(|c| c.fill.clone().unwrap()[0].1.clone())
        .collect();
    assert_eq!(fills, vec!["中", "高"]);
    let v1: Json = r1.out.value_json();
    assert_eq!(v1["pending"], json!([]), "{v1}");

    // 带槽模板与运行时逐字相同（守则 degree_text 的形状）
    let template = "「这份提案的风险有多大？」的答案是否至少达到「{bin}」？";
    let (h, calib) = 题库上岗(
        "refine",
        "risk_at_least",
        template,
        "refine",
        &[("bin", "中"), ("bin", "高")],
    );
    assert_eq!(
        format!("\u{1f}form\u{1f}{h}"),
        k1,
        "题库里的带槽题式与运行时派生题式同一个 form_hash"
    );

    // 第二次跑：两道细化题都按题式的认证线走（0.6 落在线之间 → band），随返回值转交
    let r2 = 跑_带校准(细化程序, 细化读数, vec![], &calib).unwrap();
    let refs2: Vec<jpp_ir::key::CalibRef> = 判断引用(&r2.ledger)
        .into_iter()
        .filter(|(_, c)| c.declared.starts_with("derive:"))
        .map(|(_, c)| c)
        .collect();
    assert!(
        refs2.len() == 2 && refs2.iter().all(|c| c.key.as_deref() == Some(k1.as_str())),
        "{refs2:?}"
    );
    assert!(
        r2.ledger.calib_used.contains_key(&k1),
        "用到的题式级记录进账本"
    );
    let v2: Json = r2.out.value_json();
    let got: Vec<(Json, Json)> = v2["pending"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| (p["q"].clone(), p["kind"].clone()))
        .collect();
    assert_eq!(
        got,
        vec![
            (
                json!("「这份提案的风险有多大？」的答案是否至少达到「中」？"),
                json!("unsure(band)")
            ),
            (
                json!("「这份提案的风险有多大？」的答案是否至少达到「高」？"),
                json!("unsure(band)")
            ),
        ],
        "{v2}"
    );
}
