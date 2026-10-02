//! 裂变补三处（Z0364，主会话裁定四十九 (2)(3)、裁定五十；预注册 `地基/过程记录/工程-Z0364-裂变补.md` §七）。
//!
//! - (2) 合回已决时块的未决被吸收：置已消费、账本里一条 `Refine{how: "fission-merge"}`、合回出口的报告行带
//!   `fission = {blocks, unsure_blocks, causes}`；
//! - (3) 画像没测窗口而题声明了裂变：不切（现行为不变），出口置正交位 `window_untested`，`--guard` 下不单独放行不可逆 `do`，
//!   报 `W-untested`（载体 window）；
//! - (4) select 合回：tie 判据是第二层最大两读数相差 ≤ 画像 δ；合回出口行带 `winners`、`second`、`delta`。
//!
//! 判断端口按内容答（同 `fission_ablation.rs`）：是非题看题面最后一对「」里的词在不在 `on` 里；这里加一处可调：
//! 含「梨」的块答 0.5（落在声明线 {hi: 0.7, lo: 0.3} 的未决带里），第二层派生题里「葡萄」的读数可调。

use jpp::effects::{CalibStore, EffectError, FnPort, JudgeResult, Ports, Profile};
use jpp::interp::{ActionRegistry, Passes, TaintOut};
use jpp::ledger::{Entry, Ledger};
use jpp::syntax::parse;
use jpp::value::{Answer, Op, Question, State, Taint, Value};
use serde_json::{Value as Json, json};

fn 词(q: &Question) -> String {
    let t = &q.text;
    match (t.rfind('「'), t.rfind('」')) {
        (Some(a), Some(b)) if a < b => t[a + '「'.len_utf8()..b].to_string(),
        _ => String::new(),
    }
}

fn 答(s: &State, q: &Question, 葡萄: f64) -> Answer {
    let on: String = s.on.iter().map(|m| m.text()).collect::<Vec<_>>().join("\n");
    match q.op {
        Op::Test => {
            let w = 词(q);
            let p = if on.contains(&w) {
                if w == "葡萄" { 葡萄 } else { 0.9 }
            } else if on.contains("梨") {
                0.5
            } else {
                0.1
            };
            Answer::Noul(p)
        }
        Op::Select => {
            let w: Vec<f64> = s
                .over
                .iter()
                .map(|m| if on.contains(&m.text()) { 1.0 } else { 0.01 })
                .collect();
            let z: f64 = w.iter().sum();
            Answer::Choice(w.iter().map(|x| x / z).collect())
        }
        Op::Measure => Answer::Score(vec![0.9, 0.1]),
    }
}

fn 端口(葡萄: f64) -> Ports<'static> {
    Ports::new()
        .with(FnPort::judge("fixed-0", move |s, qs| {
            Ok(JudgeResult {
                answers: qs.iter().map(|q| 答(s, q, 葡萄)).collect(),
                tokens: 0,
                cost: 0.0,
                perms: vec![],
                mode_share: vec![],
                confidence: vec![],
            })
        }))
        .with(FnPort::generate("fixed-0", |_p, _c, _n, _r| {
            Err(EffectError("不该 gen".into()))
        }))
        .with(FnPort::ask("fixed-0", |_s, _q| {
            Err(EffectError("不该 ask".into()))
        }))
}

/// `jev-1.13.0` 的副本，对象槽窗口改为 `w`；`None` = 画像没测窗口
fn 画像(w: Option<u64>) -> Profile {
    let Some(w) = w else {
        return Profile::untested();
    };
    let p = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../profiles/jev-1.13.0.json");
    let mut j: Json = serde_json::from_str(&std::fs::read_to_string(p).unwrap()).unwrap();
    j["window"]["text_slots"]["claim_bearing_ctx"]["usable_lower"] = Json::from(w);
    Profile::from_json(&j).unwrap()
}

/// 同 `画像(Some(w))`，是非题中段 δ 换成 `d`（Z0334：裂变 tie 取画像中段 δ；只为钉容差逻辑，不是测量）
fn 画像_中段delta(w: u64, d: f64) -> Profile {
    let p = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../profiles/jev-1.13.0.json");
    let mut j: Json = serde_json::from_str(&std::fs::read_to_string(p).unwrap()).unwrap();
    j["window"]["text_slots"]["claim_bearing_ctx"]["usable_lower"] = Json::from(w);
    j["delta"]["noul"]["mid"]["immediate"]["p99"] = Json::from(d);
    Profile::from_json(&j).unwrap()
}

/// 同 `画像(Some(w))`，去掉三列中段 δ、只留尾段（旧画像的形状，Z0411）
fn 画像_只有尾段(w: u64) -> Profile {
    let p = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../profiles/jev-1.13.0.json");
    let mut j: Json = serde_json::from_str(&std::fs::read_to_string(p).unwrap()).unwrap();
    j["window"]["text_slots"]["claim_bearing_ctx"]["usable_lower"] = Json::from(w);
    for c in ["noul", "choice_prob_chosen", "score"] {
        j["delta"][c].as_object_mut().unwrap().remove("mid");
    }
    Profile::from_json(&j).unwrap()
}

/// 同 `画像(Some(w))`，另去掉 `delta`（δ 先验未测）
fn 画像_无delta(w: u64) -> Profile {
    let p = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../profiles/jev-1.13.0.json");
    let mut j: Json = serde_json::from_str(&std::fs::read_to_string(p).unwrap()).unwrap();
    j["window"]["text_slots"]["claim_bearing_ctx"]["usable_lower"] = Json::from(w);
    j.as_object_mut().unwrap().remove("delta");
    Profile::from_json(&j).unwrap()
}

fn 动作表() -> ActionRegistry {
    let mut a = ActionRegistry::new();
    // 不可逆（第三个参数 false = 不可逆，同 b128_declared_line.rs）
    a.register("发退款", 0.0, false, TaintOut::Trusted, |_| {
        Ok(Value::Text("已退".into(), Taint::Trusted.into()))
    });
    a
}

struct 结果 {
    值: String,
    告警: Vec<String>,
    行: Vec<Json>,
    未决: Vec<String>,
    账本: Ledger,
}

/// `guard` = 开放行把关；`接受声明线` = 宿主接受作者声明线放行（`--release-on-declared`）
fn 跑(
    src: &str,
    w: Option<u64>,
    guard: bool,
    接受声明线: bool,
    葡萄: f64,
) -> Result<结果, String> {
    跑_画像(src, 画像(w), guard, 接受声明线, 葡萄)
}

fn 跑_画像(
    src: &str,
    profile: Profile,
    guard: bool,
    接受声明线: bool,
    葡萄: f64,
) -> Result<结果, String> {
    let entry = jpp::EntryArgs {
        accept: jpp::HostAccept {
            declared_lines: 接受声明线,
        },
        guard,
        ..Default::default()
    };
    let program =
        jpp::Session::compile(&parse(src).expect("解析"), &entry.decl()).expect("compile");
    let mut calib = CalibStore::new();
    calib.profile = profile;
    let mut ledger = Ledger::new();
    let actions = 动作表();
    let o = {
        let mut it = jpp::interp::Interp::new(
            端口(葡萄),
            &mut ledger,
            &calib,
            &actions,
            program.budget.clone(),
        )
        .with_entry(entry.clone());
        it.passes = Passes {
            fission: true,
            select_within: true,
            ..Passes::default()
        };
        it.run(&program)
            .map_err(|e| format!("[{}] {}", e.rule.clone().unwrap_or_default(), e.message))?
    };
    Ok(结果 {
        值: o.value_json().to_string(),
        告警: o.trace.warnings.clone(),
        行: o.exits.clone(),
        未决: o.returned_unsure.clone(),
        账本: ledger,
    })
}

fn 数(告警: &[String], 码: &str) -> usize {
    告警
        .iter()
        .filter(|w| w.starts_with(&format!("{码}:")))
        .count()
}

/// 十二句的长材料（约 100 token，超 30 的窗口），关键词只在第 `at` 句
fn 长(at: usize, 词: &str) -> String {
    (0..12)
        .map(|i| {
            if i == at {
                format!("第{i}句写到{词}。")
            } else {
                format!("第{i}句只是闲话。")
            }
        })
        .collect::<Vec<_>>()
        .concat()
}

const 声明线: &str = r#"{declare: {hi: 0.7, lo: 0.3}}"#;

fn 源_test(decl: &str, 材料: &str) -> String {
    format!(
        r#"budget {{calls: 50, cost: 1, depth: 64}};
let e = cut(judge(state(mat("{材料}")), test("材料里写到「苹果」吗？", "fx-k"{decl})), {声明线});
{{exit: exit_kind(e), e: e}}"#
    )
}

/// 合回出口的报告行（带 `fission.blocks` 的那一行）
fn 合回行(r: &结果) -> Vec<&Json> {
    r.行
        .iter()
        .filter(|x| x["fission"].get("blocks").is_some())
        .collect()
}

/// (2) 合回已决时块未决入账并报汇总：一块 act、一块落在声明线的未决带里（0.5），其余 ignore → 合回 act，
/// 未决块被吸收：返回未决为空、账本里恰一条 `Refine{how: "fission-merge", cause: "band"}`、无 `W-duty-twice`，
/// 合回出口的行 `fission == {blocks, unsure_blocks: 1, causes: ["band"]}`
#[test]
fn 合回已决时块未决入账并报汇总() {
    // 第 0 句写苹果（首块 act）、末句写梨（末块 0.5 → band）
    let m = 长(0, "苹果").replace("第11句只是闲话", "第11句写到梨");
    let r = 跑(
        &源_test(r#", {fission: "approx"}"#, &m),
        Some(30),
        false,
        false,
        0.9,
    )
    .unwrap();
    assert!(r.值.contains("\"exit\":\"act\""), "{}", r.值);
    assert_eq!(
        r.未决,
        Vec::<String>::new(),
        "块的未决被合回吸收，不随返回值交出"
    );
    let 去向: Vec<&Entry> = r
        .账本
        .entries
        .iter()
        .filter(|e| matches!(e, Entry::Refine { .. }))
        .collect();
    assert_eq!(去向.len(), 1, "{去向:?}");
    assert!(
        matches!(去向[0], Entry::Refine { how, cause, to: None, .. } if how == "fission-merge" && cause == "band"),
        "{:?}",
        去向[0]
    );
    assert_eq!(数(&r.告警, "W-duty-twice"), 0, "{:?}", r.告警);
    let 块行数 = r
        .行
        .iter()
        .filter(|x| x["fission"].get("block").is_some())
        .count();
    let 行 = 合回行(&r);
    assert_eq!(行.len(), 1, "{:?}", r.行);
    assert_eq!(
        行[0]["fission"],
        json!({"blocks": 块行数, "unsure_blocks": 1, "causes": ["band"]})
    );
    assert!(块行数 >= 2, "材料应被切成多块");
    assert_eq!(行[0]["exit"], "act");
}

/// (3) 未测窗口的裂变声明读数置 `window_untested`，且不切、调用数与不声明相同；测过窗口且材料在窗内的同一程序不置位
#[test]
fn 未测窗口的裂变声明读数置窗口未测位() {
    let m = 长(9, "苹果");
    let 声明 = 跑(
        &源_test(r#", {fission: "approx"}"#, &m),
        None,
        false,
        false,
        0.9,
    )
    .unwrap();
    let 不声明 = 跑(&源_test("", &m), None, false, false, 0.9).unwrap();
    // 不切：出口行只有一行（没有块行、没有合回行），行上带位、不放行
    assert_eq!(声明.行.len(), 1, "{:?}", 声明.行);
    assert_eq!(声明.行[0]["window_untested"], true);
    assert_eq!(声明.行[0]["releases"], false);
    assert!(合回行(&声明).is_empty());
    assert_eq!(声明.值, 不声明.值);
    // 没声明裂变的读数不置位（它们只有既有的每站点 W-window-untested）
    assert!(
        不声明.行[0].get("window_untested").is_none(),
        "{:?}",
        不声明.行
    );
    assert_eq!(数(&声明.告警, "W-window-untested"), 1);
    // 不开 --guard：不报 W-untested
    assert_eq!(数(&声明.告警, "W-untested"), 0, "{:?}", 声明.告警);
    // 测过窗口、材料在窗内：声明了裂变也不置位
    let 窗内 = 跑(
        &源_test(r#", {fission: "approx"}"#, "写到苹果。"),
        Some(30),
        false,
        false,
        0.9,
    )
    .unwrap();
    assert!(窗内.行[0].get("window_untested").is_none(), "{:?}", 窗内.行);
    // 全局开关关掉（消融）：声明了也不置位，同不声明
    // （开关经 `Passes.fission`；这里靠 `跑` 固定为开，开关关的等价性由 fission_ablation.rs 钉住）
}

const 守卫源: &str = r#"budget {calls: 4, cost: 0, depth: 16};
let e = cut(judge(state(mat("写到苹果。")), test("材料里写到「苹果」吗？", "fx-k"DECL)), {declare: {hi: 0.7, lo: 0.3}});
handle(e, {
    act: fn() { do("发退款", [], 0) },
    ignore: fn() { "不退" },
    unsure: fn(u) { consume(u, "drop"); "转人工" }})
"#;

/// (3) `--guard` 下未测窗口读数不单独放行不可逆 `do`：同一程序（宿主接受作者声明线，声明线出口本可放行），
/// 声明裂变且画像未测窗口 → J-08，报文点名窗口未测；测过窗口 → 放行；没声明裂变 → 放行。
/// `W-untested`（载体 window）只在 `--guard` 开时报
#[test]
fn 守卫下未测窗口读数不单独放行不可逆do() {
    let 声明 = 守卫源.replace("DECL", r#", {fission: "approx"}"#);
    let 不声明 = 守卫源.replace("DECL", "");
    // 未测窗口 + 声明裂变：拒
    let e = match 跑(&声明, None, true, true, 0.9) {
        Ok(r) => panic!("应被 J-08 拒，却放行了：{}", r.值),
        Err(e) => e,
    };
    assert!(e.contains("J-08") && e.contains("窗口"), "{e}");
    // 测过窗口、材料在窗内：放行
    let 好 = 跑(&声明, Some(30), true, true, 0.9).unwrap();
    assert!(好.值.contains("已退"), "{}", 好.值);
    // 没声明裂变、窗口未测：放行（位只对声明了裂变的读数置）
    let 没声明 = 跑(&不声明, None, true, true, 0.9).unwrap();
    assert!(没声明.值.contains("已退"), "{}", 没声明.值);
    // 不开 --guard：不拦，也不报 W-untested
    let 不守 = 跑(&声明, None, false, true, 0.9).unwrap();
    assert!(不守.值.contains("已退"), "{}", 不守.值);
    assert_eq!(数(&不守.告警, "W-untested"), 0);
    // 开 --guard 而只切不做（不进 handle）：报一条 W-untested，载体 window
    let 只切 = r#"budget {calls: 4, cost: 0, depth: 16};
let e = cut(judge(state(mat("写到苹果。")), test("材料里写到「苹果」吗？", "fx-k", {fission: "approx"})), {declare: {hi: 0.7, lo: 0.3}});
exit_kind(e)"#;
    let 报 = 跑(只切, None, true, true, 0.9).unwrap();
    assert_eq!(数(&报.告警, "W-untested"), 1, "{:?}", 报.告警);
    assert!(
        报.告警
            .iter()
            .any(|w| w.starts_with("W-untested:") && w.contains("window"))
    );
}

/// (4)（裁定五十）select 合回：第二层最大两读数相差 ≤ 画像 δ 出 `unsure(tie)`，合回出口行带 `winners`、`second`、`delta`；
/// 差得远则 `pick`
#[test]
fn select第二层读数差在delta内出tie并记胜者与读数() {
    let src = |m: &str| {
        format!(
            r#"budget {{calls: 50, cost: 1, depth: 64}};
let e = cut(judge(state(mat("{m}"), {{over: [mat("香蕉"), mat("苹果"), mat("葡萄")]}}), select("材料里写到的是哪种水果？", "fx-s", {{fission: "approx"}})));
{{exit: exit_kind(e), e: e}}"#
        )
    };
    // 两块各写一种：香蕉在第 1 句、葡萄在第 10 句；第二层里香蕉 0.90
    let m = 长(1, "香蕉").replace("第10句只是闲话", "第10句写到葡萄");
    let 阈 = 画像(Some(30))
        .delta_prior(Op::Test)
        .expect("jev-1.13.0 有 δ 先验");
    assert!(阈 > 0.03, "本测试假设 δ 大于 0.03：{阈}");
    // 葡萄 0.88：差 0.02 ≤ δ → tie
    let 近 = 跑(&src(&m), Some(30), false, false, 0.88).unwrap();
    assert!(近.值.contains("unsure(tie)"), "{}", 近.值);
    let 行 = 合回行(&近);
    assert_eq!(行.len(), 1, "{:?}", 近.行);
    let f = &行[0]["fission"];
    assert_eq!(f["winners"].as_array().unwrap().len(), 2, "{f}");
    assert_eq!(f["second"].as_array().unwrap().len(), 2, "{f}");
    assert_eq!(f["delta"].as_f64().unwrap(), 阈);
    let 标: Vec<String> = f["second"]
        .as_array()
        .unwrap()
        .iter()
        .map(|x| x["label"].as_str().unwrap().to_string())
        .collect();
    assert_eq!(
        标,
        vec!["香蕉".to_string(), "葡萄".to_string()],
        "按读数降序"
    );
    // 葡萄 0.50：差 0.40 → pick(0)（香蕉）
    let 远 = 跑(&src(&m), Some(30), false, false, 0.5).unwrap();
    assert!(远.值.contains("pick(0)"), "{}", 远.值);
    let 行 = 合回行(&远);
    assert_eq!(行[0]["fission"]["winners"].as_array().unwrap().len(), 2);
    // 差略小于 δ 是 tie，略大于 δ 不 tie（边界按 ≤，B166 并档口径；浮点相等点不测）
    let 边 = 跑(&src(&m), Some(30), false, false, 0.9 - 阈 + 0.001).unwrap();
    assert!(边.值.contains("unsure(tie)"), "{}", 边.值);
    let 外 = 跑(&src(&m), Some(30), false, false, 0.9 - 阈 - 0.01).unwrap();
    assert!(外.值.contains("pick(0)"), "{}", 外.值);
}

/// 复核 Z0364：「差恰为 δ」不是零测度事件，tie 判据按 ≤ 并带 `BOUNDARY_EPS` 容差，差恰等于 δ 出 tie。
/// Z0334 起裂变取画像的中段 δ（发行画像是非题 0.1281）：(a) 用发行画像，葡萄取 0.90 − δ，差恰为 δ 出 tie，
/// 再低 0.01 出 pick；(b) 两位小数读数上的浮点噪声（0.90 − 0.86 = 0.040000000000000036 > 0.04）由容差吸收——
/// 这层与 δ 取值无关，用中段 δ 设成 0.04 的画像副本照原样钉住
#[test]
fn select第二层差恰等于delta出tie() {
    let m = 长(1, "香蕉").replace("第10句只是闲话", "第10句写到葡萄");
    let src = format!(
        r#"budget {{calls: 50, cost: 1, depth: 64}};
let e = cut(judge(state(mat("{m}"), {{over: [mat("香蕉"), mat("苹果"), mat("葡萄")]}}), select("材料里写到的是哪种水果？", "fx-s", {{fission: "approx"}})));
{{exit: exit_kind(e), e: e}}"#
    );
    // (a) 发行画像：裂变取中段 δ，不是尾段 0.04
    let 画 = 画像(Some(30));
    let 阈 = 画.delta_prior(Op::Test).expect("jev-1.13.0 有中段 δ");
    assert_eq!(
        Some(阈),
        画.delta.get().map(|d| d.0),
        "裂变取画像中段 δ（delta.noul.mid）"
    );
    assert_ne!(Some(阈), 画.delta_tail.get().map(|d| d.0), "不是尾段 δ");
    // 端口给的第二层读数：香蕉 0.90、葡萄 0.90 − δ（四位小数）
    let 恰 = ((0.90 - 阈) * 1e4).round() / 1e4;
    let r = 跑(&src, Some(30), false, false, 恰).unwrap();
    assert!(r.值.contains("unsure(tie)"), "差恰为 δ {阈}：{}", r.值);
    let r = 跑(&src, Some(30), false, false, 恰 - 0.01).unwrap();
    assert!(r.值.contains("pick(0)"), "差比 δ 大 0.01：{}", r.值);
    // (b) 浮点噪声：中段 δ 设 0.04 的副本；裸浮点 0.90 − 0.86 > 0.04
    let 副本 = 画像_中段delta(30, 0.04);
    let 噪声差 = 0.90_f64 - 0.86;
    assert!(噪声差 > 0.04, "本测试要钉住的正是这个浮点噪声");
    let r = 跑_画像(&src, 副本.clone(), false, false, 0.86).unwrap();
    assert!(r.值.contains("unsure(tie)"), "{}", r.值);
    // 差 0.05 才出 pick
    let r = 跑_画像(&src, 副本, false, false, 0.85).unwrap();
    assert!(r.值.contains("pick(0)"), "{}", r.值);
}

/// δ 未知（画像没有 `delta`）：裁定五十六（主控板 Z0412）起不编 0、不判 tie——最大两读数相等也按最大读数出 pick，
/// 合回出口带 `delta_unknown`、不放行（`releases` 为假），行上 `delta: null`；`--guard` 下报 `W-delta-unknown`。
/// （此前是「δ 未测取 0、退化为恰好相等才 tie」，那等于替未测的 δ 编了最乐观的 0）
#[test]
fn select第二层delta未知不判tie带位() {
    let m = 长(1, "香蕉").replace("第10句只是闲话", "第10句写到葡萄");
    let src = format!(
        r#"budget {{calls: 50, cost: 1, depth: 64}};
let e = cut(judge(state(mat("{m}"), {{over: [mat("香蕉"), mat("苹果"), mat("葡萄")]}}), select("材料里写到的是哪种水果？", "fx-s", {{fission: "approx"}})));
{{exit: exit_kind(e), e: e}}"#
    );
    assert!(画像_无delta(30).delta_prior(Op::Test).is_none());
    assert!(
        !画像_无delta(30).delta_mid_missing(),
        "两段都没测不算缺 mid，不报 E-delta-mid"
    );
    // 两个第二层读数都是 0.9：有 δ 时是 tie，δ 未知时不判 tie
    let 等 = 跑_画像(&src, 画像_无delta(30), false, false, 0.9).unwrap();
    assert!(等.值.contains("pick(0)"), "{}", 等.值);
    let 行 = 合回行(&等);
    assert_eq!(行.len(), 1);
    assert_eq!(行[0]["fission"]["delta"], Json::Null);
    assert_eq!(行[0]["delta_unknown"], Json::Bool(true), "{}", 行[0]);
    assert_eq!(行[0]["releases"], Json::Bool(false), "{}", 行[0]);
    assert_eq!(
        数(&等.告警, "W-delta-unknown"),
        0,
        "不开 --guard 不报（线等级告警只在 --guard 下发）"
    );
    // --guard：同样出 pick，报 W-delta-unknown 一次
    let 守 = 跑_画像(&src, 画像_无delta(30), true, false, 0.9).unwrap();
    assert!(守.值.contains("pick(0)"), "{}", 守.值);
    assert_eq!(数(&守.告警, "W-delta-unknown"), 1, "{:?}", 守.告警);
}

/// 合回已决时块的未决键已由别处解除：不重复写 `Refine`，也不报 `W-duty-twice`。
/// 同一合成读数切两次：窄线（全 band，合回未决）先被 `unsure` 臂 `consume(u, "drop")`，宽线（合回已决、吸收一个 band 块）后解析
#[test]
fn 合回已决时已解除的键不重复写() {
    let m = 长(0, "苹果").replace("第11句只是闲话", "第11句写到梨");
    let src = format!(
        r#"budget {{calls: 50, cost: 1, depth: 64}};
let r = judge(state(mat("{m}")), test("材料里写到「苹果」吗？", "fx-k", {{fission: "approx"}}));
let a = cut(r, {{declare: {{hi: 0.7, lo: 0.3}}}});
let b = cut(r, {{declare: {{hi: 0.99, lo: 0.01}}}});
let hb = handle(b, {{act: fn() {{ "b-act" }}, ignore: fn() {{ "b-ign" }}, unsure: fn(u) {{ consume(u, "drop"); "b-drop" }}}});
{{a: exit_kind(a), b: hb}}"#
    );
    let r = 跑(&src, Some(30), false, false, 0.9).unwrap();
    assert!(
        r.值.contains("\"a\":\"act\"") && r.值.contains("b-drop"),
        "{}",
        r.值
    );
    let refine = r
        .账本
        .entries
        .iter()
        .filter(|e| matches!(e, Entry::Refine { .. }))
        .count();
    assert_eq!(refine, 0, "宽线合回时块键已由窄线的 drop 解除，不重复写");
    let drops: Vec<&Entry> = r
        .账本
        .entries
        .iter()
        .filter(|e| matches!(e, Entry::Drop { .. }))
        .collect();
    assert_eq!(drops.len(), 1, "{drops:?}");
    let 块数 = r
        .行
        .iter()
        .filter(|x| x["fission"].get("block").is_some())
        .count()
        / 2;
    assert!(
        matches!(drops[0], Entry::Drop { of, .. } if of.len() == 块数),
        "{:?} 块数 {块数}",
        drops[0]
    );
    assert_eq!(数(&r.告警, "W-duty-twice"), 0, "{:?}", r.告警);
}

/// Z0411 (i)：画像只有尾段 δ（缺 mid）时，裂变 select 合回要用 δ 比第二层两个候选，报 `E-delta-mid`，不静默取 0
#[test]
fn select第二层缺mid报错() {
    let m = 长(1, "香蕉").replace("第10句只是闲话", "第10句写到葡萄");
    let src = format!(
        r#"budget {{calls: 50, cost: 1, depth: 64}};
let e = cut(judge(state(mat("{m}"), {{over: [mat("香蕉"), mat("苹果"), mat("葡萄")]}}), select("材料里写到的是哪种水果？", "fx-s", {{fission: "approx"}})));
{{exit: exit_kind(e), e: e}}"#
    );
    let 旧 = 画像_只有尾段(30);
    assert!(旧.delta_mid_missing());
    let e = match 跑_画像(&src, 旧, false, false, 0.86) {
        Err(e) => e,
        Ok(r) => panic!("缺 mid 要报错，却出了 {}", r.值),
    };
    assert!(e.contains("E-delta-mid"), "{e}");
    // 同一程序、有中段 δ 的发行画像照常出口
    let r = 跑(&src, Some(30), false, false, 0.5).unwrap();
    assert!(r.值.contains("pick(0)"), "{}", r.值);
}

/// Z0425（复核可后补）：画像缺 mid、但第二层只有一个候选（不需要比读数差与 δ）时不报错，照常出 pick
#[test]
fn select第二层只有一个候选缺mid不报错() {
    let m = 长(1, "香蕉");
    let src = format!(
        r#"budget {{calls: 50, cost: 1, depth: 64}};
let e = cut(judge(state(mat("{m}"), {{over: [mat("香蕉"), mat("苹果"), mat("葡萄")]}}), select("材料里写到的是哪种水果？", "fx-s", {{fission: "approx"}})));
{{exit: exit_kind(e), e: e}}"#
    );
    let 旧 = 画像_只有尾段(30);
    assert!(旧.delta_mid_missing());
    let r = match 跑_画像(&src, 旧, false, false, 0.86) {
        Ok(r) => r,
        Err(e) => panic!("只有一个候选，用不到 δ，不该报错：{e}"),
    };
    assert!(r.值.contains("pick(0)"), "{}", r.值);
    let 行 = 合回行(&r);
    assert_eq!(
        行[0]["fission"]["second"].as_array().unwrap().len(),
        1,
        "{}",
        行[0]
    );
}
