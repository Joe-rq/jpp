//! 超窗裂变 `fission` 的运行时层消融与语义（步 23b，B0476；预注册 `地基/过程记录/工程-步23b.md` §六·2 第 3 条）。
//!
//! 两处分工（主控 2026-09-29 答复第 4 条）：计划期一半（开关、切点纯函数、计数规则）在
//! `crates/jpp-plan/tests/ablation/fission.rs`；本文件跑运行时：开关关掉与不声明同输出、未测窗口画像加声明不切、
//! 各合回规则（test exists / all、measure 计数、select 两层）的出口、块的未决有去向、合成读数只供 `cut`、声明的解析。
//!
//! 判断端口按内容答：是非题看题面最后一对「」里的词在不在 `on` 里（在 0.9，不在 0.1）；K 选一给 `on` 里出现的
//! 候选高分；打分题按词出现的次数给档。画像取 `jev-1.13.0` 的副本，只把对象槽窗口改小（改后 30 token）。

use std::cell::RefCell;

use jpp::effects::{CalibStore, EffectError, FnPort, JudgeResult, Ports, Profile};
use jpp::interp::Passes;
use jpp::ledger::Ledger;
use jpp::value::{Answer, Op, Question, State};
use jpp::{ActionRegistry, lower, syntax::parse};
use serde_json::Value as Json;

fn 词(q: &Question) -> String {
    let t = &q.text;
    match (t.rfind('「'), t.rfind('」')) {
        (Some(a), Some(b)) if a < b => t[a + '「'.len_utf8()..b].to_string(),
        _ => String::new(),
    }
}

fn 答(s: &State, q: &Question) -> Answer {
    let on: String = s.on.iter().map(|m| m.text()).collect::<Vec<_>>().join("\n");
    match q.op {
        Op::Test => Answer::Noul(if on.contains(&词(q)) { 0.9 } else { 0.1 }),
        Op::Select => {
            let w: Vec<f64> = s
                .over
                .iter()
                .map(|m| if on.contains(&m.text()) { 1.0 } else { 0.01 })
                .collect();
            let z: f64 = w.iter().sum();
            Answer::Choice(w.iter().map(|x| x / z).collect())
        }
        Op::Measure => {
            let n = on.matches(&词(q)).count().min(q.scale.len() - 1);
            Answer::Score(
                (0..q.scale.len())
                    .map(|i| {
                        if i == n {
                            0.9
                        } else {
                            0.1 / (q.scale.len() - 1) as f64
                        }
                    })
                    .collect(),
            )
        }
    }
}

fn 端口(调用: &RefCell<u64>) -> Ports<'_> {
    Ports::new()
        .with(FnPort::judge("fixed-0", move |s, qs| {
            *调用.borrow_mut() += 1;
            Ok(JudgeResult {
                answers: qs.iter().map(|q| 答(s, q)).collect(),
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

struct 结果 {
    值: String,
    调用: u64,
    告警: Vec<String>,
    未决: Vec<String>,
    层: usize,
}

fn 跑(src: &str, w: Option<u64>, fission: bool) -> Result<结果, String> {
    跑_挑选(src, w, fission, true)
}

fn 跑_挑选(
    src: &str,
    w: Option<u64>,
    fission: bool,
    select_within: bool,
) -> Result<结果, String> {
    let program = lower(&parse(src).expect("解析")).expect("lower");
    let mut calib = CalibStore::new();
    calib.profile = 画像(w);
    let 调用 = RefCell::new(0);
    let mut ledger = Ledger::new();
    let actions = ActionRegistry::new();
    let budget = program.budget.clone();
    let mut it = jpp::interp::Interp::new(端口(&调用), &mut ledger, &calib, &actions, budget);
    it.passes = Passes {
        fission,
        select_within,
        ..Passes::default()
    };
    let o = it.run(&program).map_err(|e| e.render())?;
    let n = *调用.borrow();
    Ok(结果 {
        值: o.value_json().to_string(),
        调用: n,
        告警: o.trace.warnings.clone(),
        未决: o.returned_unsure.clone(),
        层: o.layers.len(),
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

fn 源_test(decl: &str, 材料: &str) -> String {
    format!(
        r#"budget {{calls: 50, cost: 1, depth: 64}};
let e = cut(judge(state(mat("{材料}")), test("材料里写到「苹果」吗？", "fx-k"{decl})));
{{exit: exit_kind(e), e: e}}"#
    )
}

#[test]
fn 开关关掉与不声明同输出() {
    let m = 长(9, "苹果");
    let 声明 = 源_test(r#", {fission: "approx"}"#, &m);
    let 不声明 = 源_test("", &m);
    let a = 跑(&声明, Some(30), false).unwrap();
    let b = 跑(&不声明, Some(30), true).unwrap();
    assert_eq!(a.值, b.值);
    assert_eq!(a.调用, b.调用);
    assert_eq!(a.调用, 1);
    assert_eq!(数(&a.告警, "W-window"), 数(&b.告警, "W-window"));
    assert_eq!(数(&a.告警, "W-window"), 1);
    assert_eq!(数(&a.告警, "W-fission-approx"), 0);
    // 开着：切块、每块一次调用、一层；W-window 换成 W-fission-approx
    let c = 跑(&声明, Some(30), true).unwrap();
    // 十二句约 76 token，窗口 30：递归二分 6/6 再 3/3，四块
    assert_eq!(c.调用, 4, "切了块：{}", c.调用);
    assert_eq!(c.层, 1);
    assert_eq!(数(&c.告警, "W-window"), 0);
    assert_eq!(数(&c.告警, "W-fission-approx"), 1);
    assert!(c.值.contains("\"act\""), "{}", c.值);
}

#[test]
fn 画像未测窗口加声明不切() {
    let m = 长(9, "苹果");
    let a = 跑(&源_test(r#", {fission: "approx"}"#, &m), None, true).unwrap();
    let b = 跑(&源_test("", &m), None, true).unwrap();
    assert_eq!(
        (a.值.clone(), a.调用, a.告警.clone()),
        (b.值, b.调用, b.告警)
    );
    assert_eq!(数(&a.告警, "W-window-untested"), 1);
    assert_eq!(数(&a.告警, "W-fission-approx"), 0);
}

#[test]
fn 在窗内的声明不切() {
    let a = 跑(
        &源_test(r#", {fission: "approx"}"#, "写到苹果。"),
        Some(30),
        true,
    )
    .unwrap();
    assert_eq!(a.调用, 1);
    assert_eq!(数(&a.告警, "W-fission-approx"), 0);
}

#[test]
fn test_exists_与_all() {
    let m = 长(9, "苹果");
    // exists：只有一块写到 → act；各块的 ignore 在合回里有了去向，程序跑完不报 J-05
    let e = 跑(&源_test(r#", {fission: "approx"}"#, &m), Some(30), true).unwrap();
    assert!(e.值.contains("\"exit\":\"act\""), "{}", e.值);
    // all：有块没写到 → ignore
    let a = 跑(
        &源_test(r#", {fission: "approx", merge: "all"}"#, &m),
        Some(30),
        true,
    )
    .unwrap();
    assert!(a.值.contains("\"exit\":\"ignore\""), "{}", a.值);
    // 都没写到 → ignore
    let n = 跑(
        &源_test(r#", {fission: "approx"}"#, &长(99, "苹果")),
        Some(30),
        true,
    )
    .unwrap();
    assert!(n.值.contains("\"exit\":\"ignore\""), "{}", n.值);
}

#[test]
fn 声明线下块的未决并入合回出口() {
    // 线 {hi: 0.95, lo: 0.05}：0.9 与 0.1 都在带内，每块 unsure(band) → 合回 unsure(band)，随返回值交出
    let src = format!(
        r#"budget {{calls: 50, cost: 1, depth: 64}};
let e = cut(judge(state(mat("{}")), test("材料里写到「苹果」吗？", "fx-k", {{fission: "approx"}})), {{declare: {{hi: 0.95, lo: 0.05}}}});
{{exit: exit_kind(e), e: e}}"#,
        长(9, "苹果")
    );
    let r = 跑(&src, Some(30), true).unwrap();
    assert!(r.值.contains("unsure(band)"), "{}", r.值);
    assert_eq!(r.未决, vec!["unsure(band)".to_string()]);
}

#[test]
fn measure_按出口计数() {
    let src = |m: &str| {
        format!(
            r#"budget {{calls: 50, cost: 1, depth: 64}};
let f = form("measure", "材料里「苹果」出现几次？", {{calib: "fx-m", scale: ["零", "一", "多"], fission: "approx"}});
let e = cut(judge(state(mat("{m}")), fill(f, {{}})));
{{exit: exit_kind(e), e: e}}"#
        )
    };
    // 多数块出现 0 次 → at(0)
    let r = 跑(&src(&长(9, "苹果")), Some(30), true).unwrap();
    assert!(r.值.contains("\"exit\":\"at(0)\""), "{}", r.值);
    assert_eq!(数(&r.告警, "W-fission-approx"), 1);
}

#[test]
fn select_两层() {
    let src = |m: &str| {
        format!(
            r#"budget {{calls: 50, cost: 1, depth: 64}};
let e = cut(judge(state(mat("{m}"), {{over: [mat("香蕉"), mat("苹果"), mat("葡萄")]}}), select("材料里写到的是哪种水果？", "fx-s", {{fission: "approx"}})));
{{exit: exit_kind(e), e: e}}"#
        )
    };
    // 只有一块写到苹果：第一层该块胜者苹果，其余块无胜者（并列）；第二层苹果 0.9 → pick(1)
    let r = 跑(&src(&长(9, "苹果")), Some(30), true).unwrap();
    assert!(r.值.contains("\"exit\":\"pick(1)\""), "{}", r.值);
    // 四块各一次（第一层）+ 一道派生是非题（第二层），两层
    assert_eq!((r.调用, r.层), (5, 2));
    // 第二层问在胜者所在的块上，不是整篇：整篇超窗，问在整篇上就会报 W-window（复核 B0476 第 6 条 (a)）
    assert_eq!(数(&r.告警, "W-window"), 0);
    // 两块各写一种：两道派生是非题都 0.9 → unsure(tie)（「两块各有一个真答案」，V8 已记）
    let m2 = 长(1, "香蕉").replace("第10句只是闲话", "第10句写到葡萄");
    let r2 = 跑(&src(&m2), Some(30), true).unwrap();
    assert!(r2.值.contains("unsure(tie)"), "{}", r2.值);
    assert_eq!(r2.调用, 6);
}

#[test]
fn 记录材料只切最长的文本字段() {
    let src = format!(
        r#"budget {{calls: 50, cost: 1, depth: 64}};
let e = cut(judge(state(mat({{a: "问苹果", b: "{}"}})), test("b 里写到「苹果」吗？", "fx-k", {{fission: "approx"}})));
{{exit: exit_kind(e), e: e}}"#,
        长(9, "苹果")
    );
    let r = 跑(&src, Some(40), true).unwrap();
    assert!(r.调用 > 1);
    assert!(r.值.contains("\"exit\":\"act\""), "{}", r.值);
}

#[test]
fn 合成读数只供cut() {
    let src = format!(
        r#"budget {{calls: 50, cost: 1, depth: 64}};
let r = judge(state(mat("{}")), test("材料里写到「苹果」吗？", "fx-k", {{fission: "approx"}}));
unsure_bound([r])"#,
        长(9, "苹果")
    );
    let e = 跑(&src, Some(30), true)
        .err()
        .expect("合成读数不能给 unsure_bound");
    assert!(e.contains("合成读数"), "{e}");
}

#[test]
fn 声明的解析() {
    let bad = |decl: &str| {
        跑(&源_test(decl, "短"), Some(30), true)
            .err()
            .unwrap_or_default()
    };
    assert!(bad(r#", {fission: "exact"}"#).contains("E-rt-question"));
    assert!(bad(r#", {merge: "all"}"#).contains("E-rt-question"));
    let sel = 跑(
        r#"budget {calls: 5, cost: 1, depth: 8};
let q = select("哪个？", "k", {fission: "approx", merge: "all"});
1"#,
        Some(30),
        true,
    )
    .err()
    .unwrap_or_default();
    assert!(sel.contains("merge 只用于 test"), "{sel}");
    // 可读字段
    let r = 跑(
        r#"budget {calls: 5, cost: 1, depth: 8};
let q = test("哪个？", "k", {fission: "approx", merge: "all"});
let p = test("哪个？", "k");
{a: q.fission, b: p.fission, same: q.hash == p.hash}"#,
        Some(30),
        true,
    )
    .unwrap();
    assert!(
        r.值.contains(r#""merge":"all""#) && r.值.contains(r#""same":true"#),
        "{}",
        r.值
    );
}

/// 提前登记（向量化）也按块登记：`map` 体里声明了裂变的题，整篇不会被推测发出（window-over-approx 替身首跑时
/// 整篇被推测发出，calls 20+1 / 20、W-spec-unused 36 个站点，修在 `register_speculative`）
#[test]
fn 提前登记也按块() {
    let src = format!(
        r#"budget {{calls: 8, cost: 1, depth: 64}};
let rs = map(["{}", "{}"], fn(m) !{{judge}} {{ judge(state(mat(m)), test("材料里写到「苹果」吗？", "fx-k", {{fission: "approx"}})) }});
let es = map(rs, fn(r) {{ cut(r) }});
{{v: map(es, fn(e) {{ exit_kind(e) }}), es: es}}"#,
        长(9, "苹果"),
        长(2, "苹果")
    );
    let r = 跑(&src, Some(30), true).unwrap();
    // 两篇各四块；第 3–5、6–8 句两块在两篇里逐字相同（同状态同题，本运行复用），8 − 2 = 6 次
    assert_eq!(r.调用, 6);
    assert_eq!(数(&r.告警, "W-spec-unused"), 0, "{:?}", r.告警);
    assert_eq!(数(&r.告警, "W-budget"), 0, "{:?}", r.告警);
    assert!(r.值.contains(r#""v":["act","act"]"#), "{}", r.值);
}

/// 超窗裂变出的调用按跨状态推测计（`20` v2 §4.5 第 3 条末句；预注册 工程-步23b.md §十二）：裂变站点先登记（四块，
/// 关键词在第四块）、真站点后登记，预算只剩 3 次。层内挑选开：真站点先发出 act，块只发前两块，合回 unsure(budget)；
/// 关：按登记顺序发前三块，真站点停发。
#[test]
fn 层内挑选里裂变块让真站点先发() {
    let src = format!(
        r#"budget {{calls: 3, cost: 1, depth: 64}};
let r1 = judge(state(mat("{}")), test("材料里写到「苹果」吗？", "fx-k", {{fission: "approx"}}));
let r2 = judge(state(mat("写到苹果。")), test("材料里写到「苹果」吗？", "fx-k"));
let e1 = cut(r1);
let e2 = cut(r2);
{{a: exit_kind(e1), b: exit_kind(e2), e1: e1, e2: e2}}"#,
        长(11, "苹果")
    );
    let on = 跑_挑选(&src, Some(30), true, true).unwrap();
    assert_eq!(on.调用, 3);
    assert!(on.值.contains(r#""b":"act""#), "{}", on.值);
    assert!(on.值.contains(r#""a":"unsure(budget)""#), "{}", on.值);
    let off = 跑_挑选(&src, Some(30), true, false).unwrap();
    assert_eq!(off.调用, 3);
    assert!(off.值.contains(r#""b":"unsure(budget)""#), "{}", off.值);
    assert!(off.值.contains(r#""a":"unsure(budget)""#), "{}", off.值);
}

/// sieve 收到合成读数走裂变合回（复核 B0476 缺口 2；预注册 工程-步23b.md §十三第 1 条）：改前它把合成读数当成预算没问到，
/// 花了调用、结果丢掉、记成 unsure(budget)
#[test]
fn sieve_对合成读数走合回() {
    let src = format!(
        r#"budget {{calls: 50, cost: 1, depth: 64}};
let o = sieve([mat("{}")], test("材料里写到「苹果」吗？", "fx-k", {{fission: "approx"}}));
{{n_pending: len(o.pending), o: o}}"#,
        长(11, "苹果")
    );
    let r = 跑(&src, Some(30), true).unwrap();
    assert_eq!(r.调用, 4);
    assert!(r.值.contains(r#""n_pending":0"#), "{}", r.值);
    assert!(!r.值.contains("budget"), "{}", r.值);
    assert!(r.未决.is_empty(), "{:?}", r.未决);
}

/// measure 按出口计数，不是取第一块或最小档（复核 B0476 第 6 条 (b)）：四块里首块 0 次、后三块各 1 次 → at(1)
#[test]
fn measure_计数不是首块也不是最小() {
    let m: String = (0..12)
        .map(|i| {
            if [4, 7, 10].contains(&i) {
                format!("第{i}句写到苹果。")
            } else {
                format!("第{i}句只是闲话。")
            }
        })
        .collect::<Vec<_>>()
        .concat();
    let src = format!(
        r#"budget {{calls: 50, cost: 1, depth: 64}};
let f = form("measure", "材料里「苹果」出现几次？", {{calib: "fx-m", scale: ["零", "一", "多"], fission: "approx"}});
let e = cut(judge(state(mat("{m}")), fill(f, {{}})));
{{exit: exit_kind(e), e: e}}"#
    );
    let r = 跑(&src, Some(30), true).unwrap();
    assert_eq!(r.调用, 4);
    assert!(r.值.contains(r#""exit":"at(1)""#), "{}", r.值);
}

/// 多个 select 站点的第二层同层合批（复核 B0476 缺口 3；§十三第 2 条）：三篇各切四块，第二层三道派生题一层发出；
/// 改前每个站点自己刷新，层数 1 + 3
#[test]
fn select_多站点第二层同一层() {
    let src = format!(
        r#"budget {{calls: 50, cost: 1, depth: 64}};
let rs = map(["{}", "{}", "{}"], fn(m) !{{judge}} {{
    judge(state(mat(m), {{over: [mat("香蕉"), mat("苹果"), mat("葡萄")]}}), select("材料里写到的是哪种水果？", "fx-s", {{fission: "approx"}}))
}});
let es = map(rs, fn(r) {{ cut(r) }});
{{v: map(es, fn(e) {{ exit_kind(e) }}), es: es}}"#,
        长(9, "苹果"),
        长(2, "香蕉"),
        长(5, "葡萄")
    );
    let r = 跑(&src, Some(30), true).unwrap();
    assert!(
        r.值.contains(r#""v":["pick(1)","pick(0)","pick(2)"]"#),
        "{}",
        r.值
    );
    // 第一层：三篇共 7 个不同的块状态（闲话块在三篇间逐字相同，同状态复用）；第二层：3 道派生题在 3 个块上
    assert_eq!((r.调用, r.层), (10, 2));
    assert_eq!(数(&r.告警, "W-window"), 0);
    assert_eq!(数(&r.告警, "W-spec-unused"), 0);
}

/// 两个 select：a、b 各切 4 块（两篇的填充句不同，块状态不重），关键词各在一块里，胜者各一个
fn 源_两个select(budget: &str, cut_a: bool) -> String {
    let b: String = (0..12)
        .map(|i| {
            if i == 2 {
                format!("第{i}段写到香蕉。")
            } else {
                format!("第{i}段是别的话。")
            }
        })
        .collect::<Vec<_>>()
        .concat();
    let 尾 = if cut_a {
        "let ea = cut(ra);\n{b: exit_kind(eb), a: exit_kind(ea), eb: eb, ea: ea}"
    } else {
        "{b: exit_kind(eb), eb: eb}"
    };
    format!(
        r#"budget {budget};
let q = select("材料里写到的是哪种水果？", "fx-s", {{fission: "approx"}});
let over = {{over: [mat("香蕉"), mat("苹果"), mat("葡萄")]}};
let ra = judge(state(mat("{}"), over), q);
let rb = judge(state(mat("{b}"), over), q);
let eb = cut(rb);
{尾}"#,
        长(9, "苹果")
    )
}

/// 复查 B0476 x12：只 `cut(b)`、预算刚够「两份第一层 + b 的第二层」。第二层合批时正在 `cut` 的站点排最前，
/// b 必须选出答案；改前（按读数登记先后，a 在前）a 的派生题先拿走预算，b 出 unsure(budget)
#[test]
fn 紧预算下当前站点的第二层先发() {
    let r = 跑(
        &源_两个select("{calls: 9, cost: 1, depth: 64}", false),
        Some(30),
        true,
    )
    .unwrap();
    assert_eq!(r.调用, 9);
    assert!(r.值.contains(r#""b":"pick(0)""#), "{}", r.值);
}

/// 没用上的派生题记 W-spec-unused（`20` §4.5 第 3 条；复查 B0476 条件 3）：只 `cut(b)` 时 a 的第二层是替它提前问的、
/// 没用上，报一条；两个都 `cut` 时不报
#[test]
fn 没用上的派生题记_w_spec_unused() {
    let 一 = 跑(
        &源_两个select("{calls: 50, cost: 1, depth: 64}", false),
        Some(30),
        true,
    )
    .unwrap();
    assert_eq!(一.调用, 10);
    assert_eq!(数(&一.告警, "W-spec-unused"), 1, "{:?}", 一.告警);
    assert!(
        一.告警
            .iter()
            .any(|w| w.starts_with("W-spec-unused: 推测了 1 个站点")),
        "{:?}",
        一.告警
    );
    let 二 = 跑(
        &源_两个select("{calls: 50, cost: 1, depth: 64}", true),
        Some(30),
        true,
    )
    .unwrap();
    assert_eq!(二.调用, 10);
    assert_eq!(数(&二.告警, "W-spec-unused"), 0, "{:?}", 二.告警);
    assert!(二.值.contains(r#""a":"pick(1)""#), "{}", 二.值);
}
