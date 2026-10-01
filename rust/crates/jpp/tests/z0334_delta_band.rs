//! Z0334：画像 δ 按读数段分层（裁定四十四、四十五）。
//!
//! - 画像 `delta.<题式>.mid` 是中段 δ，语言用它（`Profile.delta`）；原来的 `delta.<题式>.immediate` 是尾段
//!   （`Profile.delta_tail`），只作记录。
//! - 缺 mid（测了尾段、没有完整中段）一律报错：认证导入取不到 δ 先验、`cut` 兜底报 `E-delta-mid`；
//!   一个段都没测的画像照 B187 取 0。
//! - `jpp profile check` 列缺项，齐全退出 0、有缺项退出 1。
//!
//! 预注册：`地基/过程记录/工程-Z0334-δ分层.md` §七（提交 0faedb82e）。

use jpp::effects::{CalibStore, Profile, behavior_hash};
use jpp::truth::{CertifyMethod, ImportOptions, LabelRow, import_labels};
use serde_json::{Value as Json, json};
use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Output},
};

fn 发行画像路径() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../profiles/jev-1.13.0.json")
}

fn 发行画像() -> Json {
    serde_json::from_str(&fs::read_to_string(发行画像路径()).unwrap()).unwrap()
}

/// 只有尾段 δ 的画像（旧画像的形状）
fn 只有尾段() -> Json {
    json!({
        "model_version": "fixed-0",
        "delta": {
            "noul": {"immediate": {"p99": 0.04}},
            "choice_prob_chosen": {"immediate": {"p99": 0.0781}},
            "score": {"immediate": {"p99": 0.1141}}
        }
    })
}

/// 两段都有的画像，mid 三列取给定值
fn 两段(mid: (f64, f64, f64)) -> Json {
    json!({
        "model_version": "fixed-0",
        "profile_revision": 2,
        "delta": {
            "noul": {"immediate": {"p99": 0.04}, "mid": {"immediate": {"p99": mid.0}}},
            "choice_prob_chosen": {"immediate": {"p99": 0.0781}, "mid": {"immediate": {"p99": mid.1}}},
            "score": {"immediate": {"p99": 0.1141}, "mid": {"immediate": {"p99": mid.2}}}
        }
    })
}

fn 临时(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("jpp-z0334-{tag}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&d);
    fs::create_dir_all(&d).unwrap();
    d
}

fn jpp(dir: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_jpp"))
        .current_dir(dir)
        .args(args)
        .output()
        .unwrap()
}

/// 1. 发行画像：中段是补测结果，尾段是 E1，版本 2，不缺 mid
#[test]
fn 发行画像读出两段() {
    let p = Profile::load(&发行画像路径()).unwrap();
    assert_eq!(p.delta.get().copied(), Some((0.1281, 0.0971, 0.0821)));
    assert_eq!(p.delta_tail.get().copied(), Some((0.04, 0.0781, 0.1141)));
    assert_eq!(p.revision, 2);
    assert!(!p.delta_mid_missing());
    assert_eq!(p.delta_prior(jpp::value::Op::Test), Some(0.1281));
    assert_eq!(p.delta_prior(jpp::value::Op::Select), Some(0.0971));
    assert_eq!(p.delta_prior(jpp::value::Op::Measure), Some(0.0821));
}

/// 2. 只有尾段：中段未测、尾段已测、缺 mid、δ 先验取不到（不退回尾段）；版本缺即 1
#[test]
fn 只有尾段是缺mid() {
    let p = Profile::from_json(&只有尾段()).unwrap();
    assert!(p.delta.get().is_none());
    assert_eq!(p.delta_tail.get().copied(), Some((0.04, 0.0781, 0.1141)));
    assert!(p.delta_mid_missing());
    assert_eq!(
        p.delta_prior(jpp::value::Op::Test),
        None,
        "不退回尾段（裁定四十五）"
    );
    assert_eq!(p.revision, 1);
    // 中段只给一列也算缺：三列都在才算已测
    let mut j = 只有尾段();
    j["delta"]["noul"]["mid"] = json!({"immediate": {"p99": 0.1}});
    let p = Profile::from_json(&j).unwrap();
    assert!(p.delta.get().is_none() && p.delta_mid_missing());
    // 版本号要是正整数
    let mut j = 只有尾段();
    j["profile_revision"] = json!("二");
    assert!(Profile::from_json(&j).is_err());
}

fn 选项() -> ImportOptions {
    ImportOptions {
        alpha: 0.1,
        conf_delta: 0.1,
        spot_check_min: 0.9,
        spot_check_conf: 0.95,
        abstain_warn: 0.1,
        batch: "z0334".into(),
        seed: 20260930,
        extent_min_disagree: 3,
        extent_same_dir: 0.8,
        extent_same_tier: 2.0 / 3.0,
        scope_quantiles: (0.01, 0.99),
        scope_margins: Default::default(),
        class_min_sources: 2,
        alpha_trial: Some(0.25),
        certify: CertifyMethod::Split,
        step: None,
        sequential: None,
    }
}

/// 两极分明的标注行：一半 p 高且真、一半 p 低且假
fn 行(n: usize) -> Vec<LabelRow> {
    (0..n)
        .map(|i| {
            let 真 = i % 2 == 0;
            serde_json::from_value(json!({
                "key": "k", "item": format!("m{i}"), "p": if 真 { 0.97 } else { 0.03 },
                "label": 真, "source": "computed", "text": "顾客说要退款"
            }))
            .unwrap()
        })
        .collect()
}

/// 3. 认证导入：缺 mid 的画像报错；有 mid 时记录 δ = 中段
#[test]
fn 认证导入取中段_缺mid报错() {
    let mut s = CalibStore::new();
    s.profile = Profile::from_json(&只有尾段()).unwrap();
    let e = import_labels(&mut s, &行(200), &选项()).expect_err("缺 mid 不许认证");
    assert!(e.contains("δ"), "{e}");

    let mut s = CalibStore::new();
    s.profile = Profile::from_json(&两段((0.1281, 0.0971, 0.0821))).unwrap();
    import_labels(&mut s, &行(200), &选项()).expect("有中段 δ 可以认证");
    assert_eq!(s.get("k").delta, Some(0.1281), "记录 δ 取中段");
}

/// `cut` 兜底用的小程序：记录有线（上岗、无 δ），读数 0.70、线 0.60
fn cut程序(dir: &Path) {
    fs::write(
        dir.join("p.jpp"),
        "budget {calls: 2, cost: 0};\nhandle(cut(judge(state(mat(\"x\")), test(\"行吗\",\"k\"))), {act: fn(){\"act\"}, ignore: fn(){\"ig\"}, unsure: fn(u){consume(u,\"drop\");unsure_cause(u)}})\n",
    )
    .unwrap();
    fs::write(
        dir.join("f.json"),
        r#"{"calibrations":[{"key":"k","hi":0.60,"lo":0.10,"n":60,"status":"上岗"}],"observations":[{"on":["x"],"op":"test","text":"行吗","calib":"k","answer":{"Noul":0.7}}]}"#,
    )
    .unwrap();
}

fn 跑cut(dir: &Path, profile: Option<&Json>) -> Output {
    cut程序(dir);
    let mut args = vec!["run", "p.jpp", "--fixtures", "f.json"];
    if let Some(j) = profile {
        fs::write(dir.join("画像.json"), j.to_string()).unwrap();
        args.extend(["--profile", "画像.json"]);
    }
    jpp(dir, &args)
}

fn 出口(o: &Output) -> Json {
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    let rep: Json = serde_json::from_slice(&o.stdout).unwrap();
    rep["value"].clone()
}

/// 4. `cut` 兜底读中段：mid 0.1281 时 0.70 落在 0.60 ± δ 带内 → band；mid 改成 0.04 → act
#[test]
fn cut兜底取中段() {
    let d = 临时("cut-mid");
    assert_eq!(
        出口(&跑cut(&d, Some(&两段((0.1281, 0.0971, 0.0821))))),
        json!("band")
    );
    assert_eq!(
        出口(&跑cut(&d, Some(&两段((0.04, 0.0781, 0.0821))))),
        json!("act")
    );
    let _ = fs::remove_dir_all(&d);
}

/// 5. `cut` 兜底遇缺 mid 的画像：运行期 `E-delta-mid`
#[test]
fn cut兜底缺mid报错() {
    let d = 临时("cut-missing");
    let o = 跑cut(&d, Some(&只有尾段()));
    assert!(!o.status.success());
    let err = String::from_utf8_lossy(&o.stderr);
    assert!(err.contains("E-delta-mid"), "{err}");
    let _ = fs::remove_dir_all(&d);
}

/// 6. 一个段都没测的画像：照线切、不加迁移带（裁定五十六读法乙，出口带 `delta_unknown`、不放行，见 `z0411_missing_mid.rs`），
/// 0.70 过 0.60 → act（出口不变）
#[test]
fn 没测delta照旧取零() {
    let d = 临时("cut-untested");
    assert_eq!(
        出口(&跑cut(&d, Some(&json!({"model_version": "fixed-0"})))),
        json!("act")
    );
    let _ = fs::remove_dir_all(&d);
}

/// 7. `jpp profile check`：发行画像齐全退出 0；只有尾段退出 1 并列出三列缺项
#[test]
fn profile_check列缺项() {
    let d = 临时("check");
    let o = jpp(&d, &["profile", "check", 发行画像路径().to_str().unwrap()]);
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    let out = String::from_utf8_lossy(&o.stdout);
    assert!(
        out.contains("profile_revision 2") && out.contains("mid p99 0.1281"),
        "{out}"
    );

    fs::write(d.join("旧.json"), 只有尾段().to_string()).unwrap();
    let o = jpp(&d, &["profile", "check", "旧.json"]);
    assert_eq!(o.status.code(), Some(1));
    let err = String::from_utf8_lossy(&o.stderr);
    for k in ["noul", "choice_prob_chosen", "score"] {
        assert!(
            err.contains(&format!("delta.{k}.mid.immediate.p99")),
            "{err}"
        );
    }
    // 只有一列有中段：那一列照实显示数值，缺项只列另外两列
    let mut 一列 = 只有尾段();
    一列["delta"]["noul"]["mid"] = json!({"immediate": {"p99": 0.1281}});
    fs::write(d.join("一列.json"), 一列.to_string()).unwrap();
    let o = jpp(&d, &["profile", "check", "一列.json"]);
    assert_eq!(o.status.code(), Some(1));
    let out = String::from_utf8_lossy(&o.stdout);
    assert!(
        out.contains("delta.noul: tail p99 0.04 · mid p99 0.1281"),
        "{out}"
    );
    let err = String::from_utf8_lossy(&o.stderr);
    assert!(!err.contains("delta.noul.mid"), "{err}");
    assert!(
        err.contains("delta.choice_prob_chosen.mid.immediate.p99")
            && err.contains("delta.score.mid.immediate.p99"),
        "{err}"
    );
    // 报文按画像实际情况写：三列尾段齐 → 「有尾段」；只有中段一列、没有尾段 → 「没有尾段」
    assert!(err.contains("这份画像有尾段 δ"), "{err}");
    fs::write(
        d.join("只中段.json"),
        json!({"model_version": "fixed-0", "delta": {"score": {"mid": {"immediate": {"p99": 0.0821}}}}})
            .to_string(),
    )
    .unwrap();
    let o = jpp(&d, &["profile", "check", "只中段.json"]);
    assert_eq!(o.status.code(), Some(1));
    let err = String::from_utf8_lossy(&o.stderr);
    assert!(err.contains("这份画像没有尾段 δ"), "{err}");
    let o = jpp(&d, &["profile", "lint", "旧.json"]);
    assert_eq!(o.status.code(), Some(2), "未知子命令是用法错误");
    let _ = fs::remove_dir_all(&d);
}

/// 8. 行为摘要覆盖中段：只改 mid 的 p99，行为摘要要变
#[test]
fn 行为摘要覆盖中段() {
    let j = 发行画像();
    let mut 改 = j.clone();
    改["delta"]["noul"]["mid"]["immediate"]["p99"] = json!(0.2);
    assert_ne!(behavior_hash(&j), behavior_hash(&改));
}
