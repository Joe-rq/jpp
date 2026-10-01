//! L7（2026-09-28）：`jpp check` 走到 J-10 静态面（K-084/K-160）与列出校准键（K-088）。
//! L7: `jpp check` reaches the J-10 static bound (K-084/K-160) and lists calibration keys (K-088).
//!
//! 在这之前 `.jpp` 已能写 `budget {unsure: …}`，`jpp run` 也经 `Session::go` 报 J-10；
//! 缺的是 `jpp check` 不收 `--calib`、不带记录，这条检查只有跑起来才看得见。
use jpp::effects::{CalibStore, LiteralMode, Sample};
use jpp_effects::views::CalibView;
use serde_json::Value;
use std::{fs, path::PathBuf, process::Command};

fn scratch(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("jpp-l7-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&d);
    fs::create_dir_all(&d).unwrap();
    d
}

fn jpp(dir: &PathBuf, args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_jpp"))
        .current_dir(dir)
        .env("HOME", dir)
        .args(args)
        .output()
        .unwrap()
}

/// 与 `j10_static.rs::记录本` 同法：每个键真的折样本、上岗，再按需覆盖 `unsure_rate`。
fn 记录本(keys: &[(&str, f64)]) -> CalibStore {
    let mut c = CalibStore::new();
    for (k, u) in keys {
        for i in 0..60 {
            let p = 0.02 + i as f64 * 0.016;
            c.absorb(
                k,
                Sample {
                    p: Some(p),
                    label: Some(if p > 0.55 { 1 } else { 0 }),
                    perms: 0,
                    mode_share: None,
                    mode: LiteralMode::default(),
                    phys: "noul".into(),
                    cluster: None,
                    stratum: None,
                },
            )
            .expect("折得进");
        }
        c.commission(k, 0.10, 0.10, "条").expect("认得动");
        c.set_unsure_rate(k, *u).expect("设得上");
    }
    c
}

fn 程序(unsure: f64) -> String {
    format!(
        r#"budget {{calls: 4, cost: 1, unsure: {unsure}}};
handle(cut(judge(state(mat("材料")), [test("甲行吗", "ka"), test("乙行吗", "kb")])[0]), {{
    act: fn() {{ "act" }}, ignore: fn() {{ "ig" }},
    unsure: fn(u) {{ consume(u, "drop"); "un" }}}})
"#
    )
}

/// 写目录并按 CLI 同一个装载函数读回，取两键的 Σuᵢ（装载会按标注重算 unsure_rate，以读回的为准）。
fn 目录与和(d: &PathBuf) -> f64 {
    jpp::store::calib::save(&记录本(&[("ka", 0.2), ("kb", 0.2)]), &d.join("calib")).unwrap();
    let s = jpp::store::calib::open(&d.join("calib")).unwrap();
    let 和 = s.unsure_rate("ka").unwrap() + s.unsure_rate("kb").unwrap();
    assert!(
        和 > 0.0 && 和 < 1.9,
        "前提：两键都有可用的 unsure_rate：{和}"
    );
    和
}

fn 布置(name: &str, 相对: f64) -> (PathBuf, f64) {
    let d = scratch(name);
    let 和 = 目录与和(&d);
    fs::write(d.join("p.jpp"), 程序(和 + 相对)).unwrap();
    (d, 和)
}

/// 反例：`budget.unsure` 比 Σuᵢ 小 0.1，`check --calib` 在任何调用之前报 J-10（告警，不拦，退出码 0）。
#[test]
fn check_带calib_超了报j10() {
    let (d, 和) = 布置("tight", -0.1);
    let out = jpp(&d, &["check", "p.jpp", "--calib", "calib"]);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(out.status.success(), "J-10 只报不拦：{stderr}");
    assert!(stderr.contains("J-10"), "{stderr}");
    assert!(
        stderr.contains(&format!("{和:.4}")),
        "报文带出联合界 {和}：{stderr}"
    );
    assert!(
        stderr.contains("其中 0 个站点没有可用的 unsure_rate"),
        "两键都用上了目录里的记录：{stderr}"
    );
    let _ = fs::remove_dir_all(&d);
}

/// 正例：`budget.unsure` 比 Σuᵢ 大 0.1，不报 J-10。
#[test]
fn check_带calib_够了不报() {
    let (d, _) = 布置("loose", 0.1);
    let out = jpp(&d, &["check", "p.jpp", "--calib", "calib"]);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(out.status.success(), "{stderr}");
    assert!(!stderr.contains("J-10"), "{stderr}");
    let _ = fs::remove_dir_all(&d);
}

/// 同一份够用的程序不给 `--calib`：按空记录本查，没有记录的键按 1 计（与 `run` 不给 `--calib` 同口径），Σ = 2 报。
#[test]
fn check_不给calib_与run同口径() {
    let (d, _) = 布置("none", 0.1);
    let out = jpp(&d, &["check", "p.jpp"]);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(out.status.success(), "{stderr}");
    assert!(
        stderr.contains("J-10") && stderr.contains("2.0000"),
        "{stderr}"
    );
    let _ = fs::remove_dir_all(&d);
}

/// `--json`：J-10 进文档的 diagnostics，stderr 不出东西。
#[test]
fn check_json_带出j10() {
    let (d, _) = 布置("json", -0.1);
    let out = jpp(&d, &["check", "p.jpp", "--calib", "calib", "--json"]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        out.stderr.is_empty(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let doc: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert!(
        doc["diagnostics"]
            .as_array()
            .unwrap()
            .iter()
            .any(|x| x["code"] == "J-10"),
        "{doc}"
    );
    let _ = fs::remove_dir_all(&d);
}

/// `--calib` 缺值、给两次都是用法错误。
#[test]
fn check_calib_用法错误() {
    let (d, _) = 布置("usage", 0.1);
    for argv in [
        vec!["check", "p.jpp", "--calib"],
        vec!["check", "p.jpp", "--calib", "calib", "--calib", "calib"],
    ] {
        let out = jpp(&d, &argv);
        assert_eq!(out.status.code(), Some(2), "{argv:?}");
    }
    let _ = fs::remove_dir_all(&d);
}

// ---------- `check --profile`：B32 时延预算的静态面在检查期可达 ----------

fn 画像() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../profiles/jev-1.13.0.json")
}

const 时延程序: &str = r#"budget {calls: 4, cost: 1, latency_p95: 0.001};
let e = cut(judge(state(mat("m")), test("行吗", "k")));
handle(e, {act: fn() { 1 }, ignore: fn() { 0 }, unsure: fn(u) { consume(u, "drop"); 2 }})
"#;

/// 反例：时延预算 0.001 秒小于画像的一层 p95，`check --profile` 在检查期报 `E-latency`（与 `run` 同口径）。
/// 在这之前 `check` 只报「没有档案的 p95……修法：--profile 加载档案」，而 `check` 不收 `--profile`。
#[test]
fn check_带profile_时延预算不够报错() {
    let d = scratch("latency-tight");
    fs::write(d.join("p.jpp"), 时延程序).unwrap();
    let p = 画像();
    let out = jpp(&d, &["check", "p.jpp", "--profile", p.to_str().unwrap()]);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(!out.status.success(), "{stderr}");
    // Z0157：`E-latency` 已撤（检查器不按上界拒）。这个程序的判断在 `handle` 外、必经，可靠下界
    // 1 层 × p95 确超预算，由规划器拒；`jpp check` 现在也调规划器。
    assert!(stderr.contains("E-budget-plan"), "{stderr}");
    assert!(!stderr.contains("E-latency"), "{stderr}");
    let _ = fs::remove_dir_all(&d);
}

/// 正例：不给 `--profile` 照旧只告警（估不了），退出码 0；预算放宽到 60 秒、给画像也不报 `E-latency`。
#[test]
fn check_时延预算_正例() {
    let d = scratch("latency-ok");
    fs::write(d.join("p.jpp"), 时延程序).unwrap();
    let out = jpp(&d, &["check", "p.jpp"]);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(out.status.success(), "{stderr}");
    assert!(!stderr.contains("E-latency"), "{stderr}");
    fs::write(d.join("q.jpp"), 时延程序.replace("0.001", "60")).unwrap();
    let p = 画像();
    let out = jpp(&d, &["check", "q.jpp", "--profile", p.to_str().unwrap()]);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(out.status.success(), "{stderr}");
    assert!(
        !stderr.contains("E-latency") && !stderr.contains("W-latency"),
        "{stderr}"
    );
    let _ = fs::remove_dir_all(&d);
}

// ---------- K-088：`jpp check` 列出程序用到的校准键 ----------

const 键程序: &str = r#"budget {calls: 8, cost: 1};
let f = form("test", "这段话是否提到了{city}？", {calib: "kf"});
let lab = "k" + "x";
let a = cut(judge(state(mat("甲")), test("甲行吗", "ka")));
let b = cut(judge(state(mat("乙")), test("乙行吗", "kb")), {cost: [1, 5]});
let c = cut(judge(state(mat("丙")), test("丙行吗", lab)));
let d = cut(judge(state(mat("我去了上海")), fill(f, {city: "上海"})));
let h = fn(e) { handle(e, {act: fn() { 1 }, ignore: fn() { 0 }, unsure: fn(u) { consume(u, "drop"); 2 }}) };
[h(a), h(b), h(c), h(d)]
"#;

/// 文本模式：列出标签、每处用得上哪一层记录、代价矩阵、跳过的非字面标签；stdout 的 `Checked` 一行不变。
#[test]
fn check_列出校准键_文本() {
    let d = scratch("keys-text");
    fs::write(d.join("p.jpp"), 键程序).unwrap();
    jpp::store::calib::save(&记录本(&[("ka", 0.2)]), &d.join("calib")).unwrap();
    let out = jpp(&d, &["check", "p.jpp", "--calib", "calib"]);
    let stderr = String::from_utf8_lossy(&out.stderr);
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(out.status.success(), "{stderr}");
    assert!(stdout.starts_with("Checked p.jpp"), "{stdout}");
    assert!(
        stderr.contains("校准键（K-088）：3 个标签、3 处题，代价矩阵 1 处，跳过 1 处"),
        "{stderr}"
    );
    let 行 = |k: &str| {
        stderr
            .lines()
            .find(|l| l.trim_start().starts_with(k))
            .unwrap_or_else(|| panic!("缺 {k}：{stderr}"))
            .to_string()
    };
    assert!(行("ka @").contains("题级记录可用"), "{stderr}");
    assert!(行("kb @").contains("没有可用记录"), "{stderr}");
    assert!(行("kf @").contains("题式 "), "模板题式带题式哈希：{stderr}");
    assert!(
        行("代价 [1, 5]").contains("calib-import --cost 1,5"),
        "{stderr}"
    );
    assert!(行("跳过 test").contains("校准标签不是字面文本"), "{stderr}");
    let _ = fs::remove_dir_all(&d);
}

/// `--json`：清单进文档的 `calib_keys`；模板题式的静态题式哈希与运行时 `f.hash` 相同。
#[test]
fn check_列出校准键_json_题式哈希与运行时一致() {
    let d = scratch("keys-json");
    fs::write(d.join("p.jpp"), 键程序).unwrap();
    let out = jpp(&d, &["check", "p.jpp", "--json"]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let doc: Value = serde_json::from_slice(&out.stdout).unwrap();
    let k = &doc["calib_keys"];
    assert_eq!(k["labels"], serde_json::json!(["kf", "ka", "kb"]), "{k}");
    let uses = k["uses"].as_array().unwrap();
    assert!(
        uses.iter().all(|u| u["line"].is_null()),
        "不给 --calib：每处都没有线：{k}"
    );
    assert_eq!(k["costs"][0]["fp"], 1.0);
    assert_eq!(k["costs"][0]["fn"], 5.0);
    assert_eq!(k["skipped"].as_array().unwrap().len(), 1, "{k}");
    let 静态 = uses
        .iter()
        .find(|u| u["label"] == "kf")
        .and_then(|u| u["form_hash"].as_str())
        .unwrap()
        .to_string();
    // 运行时的题式哈希：同一个模板，程序只返回 `f.hash`，不发调用
    fs::write(
        d.join("h.jpp"),
        "budget {calls: 0, cost: 0};\nlet f = form(\"test\", \"这段话是否提到了{city}？\", {calib: \"kf\"});\nf.hash\n",
    )
    .unwrap();
    let out = jpp(&d, &["run", "h.jpp"]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let r: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(r["value"], Value::String(静态.clone()), "{r}");
    let _ = fs::remove_dir_all(&d);
}
