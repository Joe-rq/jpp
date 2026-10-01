//! **赌 2 的另一半：两内核的账本键对不对得上。**
//!
//! 结论（**$0 拿到的**）：**对不上，而且不可能对得上**——
//! 两边的**分量完全一致**，**哈希构造不同**。
//!
//! | | Python | Rust |
//! |---|---|---|
//! | 分量 | `judge, model_id, state_hash, q_hash, phys, render_version, perm_seed, run_seq, site` | **逐项相同** |
//! | 构造 | `sha256(canon([分量…]))` | `sha256(分量 \u{1f} 连接)` |
//! | 长度 | **16 个十六进制字符** | **24 个** |
//!
//! **所以「观察身份」这件事上两边没有语义差——分量一模一样；差的是编码。**
//! 预注册写的是「对不上就是内核语义差」，**而这次对不上不是语义差**，
//! 修法也因此不同：不是回到对照移植逐层查，是**统一哈希构造**（带迁移代价）。
//!
//! **而同一个仓库里 `profile_hash` 两边是逐字节相同的**（`e1a83f986488d581`，Z0334 画像 profile_revision 2；此前 `ea01589429412ed0`）——
//! **因为那一条被明确要求过必须同值，而账本键没人看过。**
//! **「被要求过的对上了，没被要求的没对上」——这不是运气，是覆盖面。**

use jpp::effects::profile_hash;
use jpp::ledger::judge_key;

/// **`profile_hash` 两边必须同值**（它进账本头，`12` §J-18 的重放判定建在它上面）。
#[test]
fn profile_hash与python逐字节相同() {
    let p = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../../src/foundation/profile/profiles/jev-1.13.0.json");
    let j: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(p).expect("真档案在")).expect("合法 JSON");
    assert_eq!(
        profile_hash(&j),
        "e1a83f986488d581",
        "**它与 Python 的 H(profile) 必须同值**——两边算不出同一个数，跨内核的重放判定就废了"
    );
}

/// **账本键今天两边对不上，把它钉住。**
///
/// **钉的不是「它应该不一样」**——钉的是**这个已知的不一致不许被静默改掉或静默留着**。
/// 哪天有人统一了哈希构造，这条会红，**而那正是该有人看一眼的时刻**：
/// 统一会让**所有既有账本失配**，那是一次迁移，不是一次重构。
#[test]
fn 账本键与python的不一致是已知的() {
    let k = judge_key("jev-1.13.0", "sh", "qh", "noul", 0, 0, 42);
    assert_eq!(
        k.len(),
        24,
        "Rust 侧取 sha256 前 12 字节 = 24 个十六进制字符"
    );
    // 步 15i（B155）：渲染版本 r1 → r2，键随之换值（r1 下是 66b055d30697ec3173510075）
    assert_eq!(k, "6baeabe7a0e84834d5c70d09");
    const PYTHON: &str = "718d5bfb5eac67dd";
    assert_eq!(PYTHON.len(), 16, "Python 侧取前 16 个十六进制字符");
    assert_ne!(
        k, PYTHON,
        "**今天对不上**；哪天对上了，这条会红，而那时要先想清楚迁移"
    );
}

/// Z0389：两内核对三种画像形状的 δ 取值一致（Python 结果在 oracle 的 `profile_oracle.delta_shapes`，由
/// `tests/oracle/generate.py` 档案段生成）：发行画像取中段同值；只有尾段两边都报缺 mid（Python 抛 `E-delta-mid`，
/// Rust `delta_mid_missing()` 为真，`cut` / 裂变 / order 报同名错误）；没有 δ 两边都取不到（裁定五十六，不编数）
#[test]
fn delta取值与python一致() {
    use jpp::effects::Profile;
    use jpp::value::Op;
    let o: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/oracle/oracle.json"),
        )
        .unwrap(),
    )
    .unwrap();
    let shapes = &o["profile_oracle"]["delta_shapes"];
    let src: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../../src/foundation/profile/profiles/jev-1.13.0.json"),
        )
        .unwrap(),
    )
    .unwrap();
    let rust = |j: &serde_json::Value| Profile::from_json(j).unwrap();
    let 取 = |p: &Profile| [Op::Test, Op::Select, Op::Measure].map(|op| p.delta_prior(op));
    // 发行画像
    let r = rust(&src);
    let py = &shapes["release"]["delta"];
    assert_eq!(
        取(&r),
        [
            py["noul"].as_f64(),
            py["choice"].as_f64(),
            py["score"].as_f64()
        ],
        "发行画像：两内核取中段同值"
    );
    assert!(!r.delta_mid_missing());
    // 只有尾段
    let mut tail = src.clone();
    for c in ["noul", "choice_prob_chosen", "score"] {
        tail["delta"][c].as_object_mut().unwrap().remove("mid");
    }
    assert_eq!(
        shapes["tail_only"]["error"],
        serde_json::json!("E-delta-mid"),
        "Python 报缺 mid"
    );
    assert!(rust(&tail).delta_mid_missing(), "Rust 同样判缺 mid");
    // 没有 δ
    let mut none = src.clone();
    none.as_object_mut().unwrap().remove("delta");
    let py = &shapes["no_delta"]["delta"];
    assert!(
        py["noul"].is_null() && py["choice"].is_null() && py["score"].is_null(),
        "Python 取不到"
    );
    let r = rust(&none);
    assert_eq!(取(&r), [None, None, None], "Rust 同样取不到");
    assert!(
        !r.delta_mid_missing(),
        "两段都没有不算缺 mid（按裁定五十六处理）"
    );
    // 部分列（Z0334 §二十四第 3 件）：只有 score 中段；尾段三列齐、中段缺 choice 一列——两边都判缺 mid
    let mut 只有打分中段 = src.clone();
    let 打分中段 = src["delta"]["score"]["mid"].clone();
    只有打分中段["delta"] = serde_json::json!({"score": {"mid": 打分中段}});
    assert_eq!(
        shapes["score_mid_only"]["error"],
        serde_json::json!("E-delta-mid")
    );
    assert!(
        rust(&只有打分中段).delta_mid_missing(),
        "只有 score 中段：Rust 同样判缺 mid"
    );
    let mut 缺一列 = src.clone();
    缺一列["delta"]["choice_prob_chosen"]
        .as_object_mut()
        .unwrap()
        .remove("mid");
    assert_eq!(
        shapes["tail_mid_minus_choice"]["error"],
        serde_json::json!("E-delta-mid")
    );
    assert!(
        rust(&缺一列).delta_mid_missing(),
        "中段缺一列：Rust 同样判缺 mid"
    );
}

/// 冷键 cut（Z0334 §二十四第 4 件）：没有上岗记录就没有线，两内核都按 B187 无线默认按判断器的回答走——
/// Python 的结果在 oracle 的 `profile_oracle.cold_cut`（原来 Python 出 Unsure(cold) 加保守线上的临时出口）
#[test]
fn 冷键cut与python一致() {
    use jpp::value::Answer;
    let o: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/oracle/oracle.json"),
        )
        .unwrap(),
    )
    .unwrap();
    let py = o["profile_oracle"]["cold_cut"].as_object().unwrap();
    assert_eq!(py.len(), 3);
    for (p, want) in py {
        let k = jpp_value::bridge::follow_answer(&Answer::Noul(p.parse().unwrap()), None);
        let got = match k {
            jpp::value::ExitKind::Act => "act".to_string(),
            jpp::value::ExitKind::Ignore => "ignore".to_string(),
            // 步 36 G3：与 `Exit::label` 同一规则
            jpp::value::ExitKind::Unsure(w) => format!("unsure({})", w.text()),
            other => format!("{other:?}"),
        };
        assert_eq!(serde_json::json!(got), *want, "读数 {p}");
    }
}
