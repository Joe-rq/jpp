//! 伴随题（主控板 B0492 S5；预注册见过程记录 `工程-未决去向.md` 5.9）：没有可信记录的题在同一状态上带五道伴随题，
//! 读数只给默认链选路、进报告 `improve` 段，不进原题出口。两种发法：同请求（并进原题那一次调用）、并行另一请求
//! （同一刷新时刻另发一次调用）。

use jpp::effects::{CalibStore, FnPort, JudgeResult, Ports};
use jpp::interp::CompanionMode;
use jpp::ledger::{Entry, Ledger};
use jpp::value::Answer;
use jpp::{ActionRegistry, EntryArgs, Outcome, Session, lower, syntax::parse};
use serde_json::{Value as Json, json};
use std::cell::RefCell;

/// 判断器替身：伴随题按校准键回 `伴随`（没列的回 0.5）；原题回 `原`。「最缺哪类」那道 select（`unsure-companion-material`）
/// 在 `伴随` 里给的数当作选中项的下标，没给时均匀（没选出）；「为什么拿不准」选第一类。`calls` 数闭包被调几次
/// （同材料合批的一次调用按状态分段调闭包，所以「最缺哪类」换了 over 槽会多调一次；真调用数看报告 `cost.calls`）
fn 端口<'a>(calls: &'a RefCell<u64>, 原: f64, 伴随: &'a [(&'a str, f64)]) -> Ports<'a> {
    Ports::new().with(FnPort::judge("fixed-0", move |s, qs| {
        *calls.borrow_mut() += 1;
        Ok(JudgeResult {
            answers: qs
                .iter()
                .map(|q| {
                    if q.op == jpp::value::Op::Select {
                        let n = s.over.len();
                        if q.calib == "unsure-companion-material" {
                            return match 伴随.iter().find(|(k, _)| *k == q.calib) {
                                Some((_, k)) => {
                                    let mut v = vec![0.1 / n as f64; n];
                                    v[*k as usize] = 0.9;
                                    Answer::Choice(v)
                                }
                                None => Answer::Choice(vec![1.0 / n as f64; n]),
                            };
                        }
                        // 「为什么拿不准」：选第一类
                        let mut v = vec![0.05; n];
                        v[0] = 0.7;
                        return Answer::Choice(v);
                    }
                    if q.text.starts_with("题「")
                        || q.text.starts_with("把题「")
                        || q.text.starts_with("判断题「")
                    {
                        let p = 伴随
                            .iter()
                            .find(|(k, _)| *k == q.calib)
                            .map(|(_, p)| *p)
                            .unwrap_or(0.5);
                        Answer::Noul(p)
                    } else {
                        Answer::Noul(原)
                    }
                })
                .collect(),
            tokens: 0,
            cost: 0.0,
            mode_share: vec![],
            perms: vec![],
            confidence: vec![],
        })
    }))
}

fn 库() -> String {
    std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../lib/unsure.jpp"),
    )
    .expect("读得到 lib/unsure.jpp")
}

struct 跑出 {
    outcome: Outcome,
    ledger: Ledger,
    calls: u64,
}

fn 跑(
    正文: &str,
    mode: CompanionMode,
    原: f64,
    伴随: &[(&str, f64)],
    calib: CalibStore,
) -> 跑出 {
    let src = format!(
        "budget {{calls: 16, cost: 0, depth: 16}};\n{}\n{正文}",
        库()
    );
    let program = lower(&parse(&src).expect("解析")).expect("lower");
    let calls = RefCell::new(0);
    let mut ledger = Ledger::new();
    let a = ActionRegistry::new();
    let outcome = Session::new(端口(&calls, 原, 伴随), &calib, &a)
        .with_companions(mode)
        .run(&program, &EntryArgs::default(), &mut ledger)
        .unwrap_or_else(|e| panic!("运行失败：{}", e.render()));
    let calls = *calls.borrow();
    跑出 {
        outcome,
        ledger,
        calls,
    }
}

const 一题: &str = r#"let e = cut(judge(state(mat("甲方与乙方的合作意向")), test("这两方适合合作吗？", "k")), {declare: {hi: 0.7, lo: 0.3}});
handle(e, {act: fn() { "合作" }, ignore: fn() { "不合作" }, unsure: fn(u) { {exit: u} }})
"#;

fn 判断条目(l: &Ledger) -> usize {
    l.entries
        .iter()
        .filter(|e| matches!(e, Entry::Judge { .. }))
        .count()
}

#[test]
fn 同请求_一次调用_六条判断_improve五项() {
    let r = 跑(一题, CompanionMode::Same, 0.9, &[], CalibStore::new());
    assert_eq!(
        r.outcome.cost.calls, 1,
        "「最缺哪类」换了 over 槽，同材料仍并进一次调用"
    );
    assert_eq!(判断条目(&r.ledger), 6);
    assert_eq!(r.outcome.value_json(), json!("合作"), "伴随题不进原题出口");
    assert_eq!(r.outcome.improve.len(), 1);
    let cs = r.outcome.improve[0]["companions"].as_array().unwrap();
    assert_eq!(cs.len(), 5);
    let kinds: Vec<&str> = cs.iter().map(|c| c["kind"].as_str().unwrap()).collect();
    assert_eq!(
        kinds,
        vec![
            "unsure-companion-premise",
            "diag-two-judgments",
            "unsure-companion-cut",
            "unsure-companion-reference",
            "unsure-companion-material"
        ]
    );
    // 「最缺哪类」没选出（均匀）：lacks 为 null；它的候选来自序言通用表，另加「材料不缺」
    assert_eq!(r.outcome.improve[0]["lacks"], Json::Null);
    let 材 = cs
        .iter()
        .find(|c| c["kind"] == "unsure-companion-material")
        .unwrap();
    assert!(材.get("pick").is_some(), "{材}");
}

#[test]
fn 并行另一请求_两次调用() {
    let r = 跑(一题, CompanionMode::Parallel, 0.9, &[], CalibStore::new());
    assert_eq!(r.outcome.cost.calls, 2, "同一刷新时刻另发一次");
    assert_eq!(判断条目(&r.ledger), 6);
    assert_eq!(r.outcome.value_json(), json!("合作"));
}

#[test]
fn 关_不带() {
    let r = 跑(一题, CompanionMode::Off, 0.9, &[], CalibStore::new());
    assert_eq!(r.calls, 1);
    assert_eq!(判断条目(&r.ledger), 1);
    assert!(r.outcome.improve.is_empty());
}

#[test]
fn 有可信记录的题不带() {
    let mut c = CalibStore::new();
    c.put("k", 0.7, 0.3, 50, "上岗", Some(0.05)).unwrap();
    let 有线 = r#"let e = cut(judge(state(mat("甲方与乙方的合作意向")), test("这两方适合合作吗？", "k")));
handle(e, {act: fn() { "合作" }, ignore: fn() { "不合作" }, unsure: fn(u) { {exit: u} }})
"#;
    let r = 跑(有线, CompanionMode::Same, 0.9, &[], c);
    assert_eq!(判断条目(&r.ledger), 1);
    assert!(r.outcome.improve.is_empty());
}

/// 默认链按伴随题读数选路：handle 没写 unsure 臂、原题并列（0.5）、声明了两类取材料来源
const 走链: &str = r#"unsure_source({need: ["双方目标", "过往合作"], fetch: fn(q, need, m) { "关于" + need + "的补充" }});
let e = cut(judge(state(mat("甲方与乙方的合作意向")), test("这两方适合合作吗？", "k")));
handle(e, {act: fn() { "合作" }, ignore: fn() { "不合作" }})
"#;

/// 发过「为什么拿不准」：K 选一的判断条目多于伴随题「最缺哪类」那一道
fn 为什么发了(l: &Ledger) -> bool {
    l.entries
        .iter()
        .filter(|e| matches!(e, Entry::Judge { jkey: Some(k), .. } if k.phys == "choice"))
        .count()
        > 1
}

#[test]
fn 伴随选路_前提不成立_题不清不补() {
    let r = 跑(
        走链,
        CompanionMode::Same,
        0.5,
        &[("unsure-companion-premise", 0.2)],
        CalibStore::new(),
    );
    assert_eq!(r.outcome.unsure_default[0]["why"], "unclear");
    assert!(!为什么发了(&r.ledger), "有伴随题读数就不发「为什么拿不准」");
    assert_eq!(r.outcome.cost.calls, 1);
}

#[test]
fn 伴随选路_都成立_两可不补() {
    // 「最缺哪类」选「材料不缺」（候选两类之后那一项，下标 2）
    let 都成立 = [
        ("unsure-companion-premise", 0.9),
        ("unsure-companion-material", 2.0),
        ("diag-two-judgments", 0.1),
        ("unsure-companion-cut", 0.1),
        ("unsure-companion-reference", 0.9),
    ];
    let r = 跑(走链, CompanionMode::Same, 0.5, &都成立, CalibStore::new());
    assert_eq!(r.outcome.unsure_default[0]["why"], "ambiguous");
    assert!(!为什么发了(&r.ledger));
    assert_eq!(r.outcome.improve[0]["lacks"], "材料不缺");
}

#[test]
fn 伴随选路_缺材料_去补() {
    // 「最缺哪类」选第二类「过往合作」（下标 1）：第一轮直接取它，不再发「为什么」（裁定五十一，过程记录 5.23）
    let 缺材料 = [
        ("unsure-companion-premise", 0.9),
        ("unsure-companion-material", 1.0),
        ("diag-two-judgments", 0.1),
        ("unsure-companion-cut", 0.1),
        ("unsure-companion-reference", 0.9),
    ];
    let r = 跑(走链, CompanionMode::Same, 0.5, &缺材料, CalibStore::new());
    // 替身补后仍读 0.5，第二轮只剩一类、代码能定；第一轮取的是伴随题选出的那一类
    assert_eq!(
        r.outcome.unsure_default[0]["fetched"][0], "过往合作",
        "{:?}",
        r.outcome.unsure_default
    );
    assert_eq!(r.outcome.unsure_default[0]["asked"], json!([]));
    assert!(!为什么发了(&r.ledger), "伴随题已选出类别，不再发「为什么」");
    assert_eq!(r.outcome.improve[0]["lacks"], "过往合作");
}

/// 「最缺哪类」没选出而参照不够：照旧去补，类别由「为什么」选
#[test]
fn 伴随选路_没选出_参照不够_问为什么() {
    let 参照不够 = [
        ("unsure-companion-premise", 0.9),
        ("diag-two-judgments", 0.1),
        ("unsure-companion-cut", 0.1),
        ("unsure-companion-reference", 0.2),
    ];
    let r = 跑(走链, CompanionMode::Same, 0.5, &参照不够, CalibStore::new());
    assert!(为什么发了(&r.ledger));
    assert_eq!(
        r.outcome.unsure_default[0]["fetched"][0], "双方目标",
        "「为什么」选第一类"
    );
}

#[test]
fn unsure_source的companions覆盖标准题式() {
    let 自己的 = r#"let mine = form("test", "题「{q}」说得够具体吗？", {calib: "unsure-companion-specific"});
unsure_source({need: ["双方目标"], fetch: fn(q, need, m) { need }, companions: [mine]});
let e = cut(judge(state(mat("甲方与乙方的合作意向")), test("这两方适合合作吗？", "k")), {declare: {hi: 0.7, lo: 0.3}});
handle(e, {act: fn() { "合作" }, ignore: fn() { "不合作" }, unsure: fn(u) { {exit: u} }})
"#;
    let r = 跑(自己的, CompanionMode::Same, 0.9, &[], CalibStore::new());
    let cs = r.outcome.improve[0]["companions"].as_array().unwrap();
    assert_eq!(cs.len(), 1);
    assert_eq!(cs[0]["kind"], Json::from("unsure-companion-specific"));
}

#[test]
fn 固定观察_没登记的伴随题给中性读数_别的照旧报错() {
    use jpp::effects::FixedPorts;
    use jpp::value::{Mat, Op, Question, State, Taint};
    let src = format!(
        "budget {{calls: 16, cost: 0, depth: 16}};\n{}\n{一题}",
        库()
    );
    let program = lower(&parse(&src).expect("解析")).expect("lower");
    let st = State::new(
        vec![Mat::new(
            json!("甲方与乙方的合作意向"),
            "",
            vec![],
            Taint::Trusted,
            Default::default(),
        )],
        vec![],
        vec![],
        vec![],
        false,
    );
    let 原题 = Question::new(Op::Test, "这两方适合合作吗？", "k", vec![]);
    let calib = CalibStore::new();
    let a = ActionRegistry::new();
    // 只登记原题：伴随题没登记，固定观察给 0.5，程序照常跑完
    let mut fp = FixedPorts::new();
    fp.observe(&st, &原题, Answer::Noul(0.9));
    let mut l = Ledger::new();
    let o = Session::new(fp.ports(), &calib, &a)
        .with_companions(CompanionMode::Same)
        .run(&program, &EntryArgs::default(), &mut l)
        .unwrap_or_else(|e| panic!("{}", e.render()));
    // 是非伴随题 0.5；「最缺哪类」在候选（通用表三类 + 「材料不缺」）上均匀，最大项 0.25
    let ps: Vec<&Json> = o.improve[0]["companions"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| &c["p"])
        .collect();
    assert_eq!(
        ps,
        vec![
            &json!(0.5),
            &json!(0.5),
            &json!(0.5),
            &json!(0.5),
            &json!(0.25)
        ],
        "{ps:?}"
    );
    assert_eq!(o.improve[0]["lacks"], Json::Null);
    // 原题也不登记：照旧「固定观察未命中」
    let mut fp = FixedPorts::new();
    let mut l = Ledger::new();
    let e = Session::new(fp.ports(), &calib, &a)
        .with_companions(CompanionMode::Same)
        .run(&program, &EntryArgs::default(), &mut l)
        .err()
        .expect("原题没登记照旧报错");
    assert!(e.render().contains("固定观察未命中"), "{}", e.render());
}

/// 主控复核 2026-09-30：审计重放时带不带伴随题只看账本——重放给了上岗记录，或重放给了 --companions off，都照账本零调用重放
#[test]
fn 重放带不带伴随题只看账本() {
    let src = format!("budget {{calls: 16, cost: 0, depth: 16}};\n{一题}");
    let program = lower(&parse(&src).expect("解析")).expect("lower");
    let a = ActionRegistry::new();
    let calls = RefCell::new(0);
    let mut l = Ledger::new();
    Session::new(端口(&calls, 0.9, &[]), &CalibStore::new(), &a)
        .with_companions(CompanionMode::Same)
        .run(&program, &EntryArgs::default(), &mut l)
        .unwrap_or_else(|e| panic!("{}", e.render()));
    assert_eq!(判断条目(&l), 6, "首跑带了伴随题");
    // ① 重放时这道题已上岗
    let mut c = CalibStore::new();
    c.put("k", 0.7, 0.3, 50, "上岗", Some(0.05)).unwrap();
    let mut l1 = l.clone();
    let z = RefCell::new(0);
    Session::new(端口(&z, 0.9, &[]), &c, &a)
        .with_companions(CompanionMode::Same)
        .replay(&program, &EntryArgs::default(), &mut l1)
        .unwrap_or_else(|e| panic!("重放给了上岗记录：{}", e.render()));
    assert_eq!(*z.borrow(), 0);
    // ② 重放时给了关
    let mut l2 = l.clone();
    Session::new(端口(&z, 0.9, &[]), &CalibStore::new(), &a)
        .with_companions(CompanionMode::Off)
        .replay(&program, &EntryArgs::default(), &mut l2)
        .unwrap_or_else(|e| panic!("重放给了关：{}", e.render()));
    assert_eq!(*z.borrow(), 0);
}

/// 程序自己定义了非题式的 unsure_companions：回落到标准库（序言）的题式
#[test]
fn 非题式的unsure_companions回落到序言() {
    let src = r#"budget {calls: 16, cost: 0, depth: 16};
let unsure_companions = ["不是题式"];
let e = cut(judge(state(mat("甲方与乙方的合作意向")), test("这两方适合合作吗？", "k")), {declare: {hi: 0.7, lo: 0.3}});
handle(e, {act: fn() { "合作" }, ignore: fn() { "不合作" }, unsure: fn(u) { {exit: u} }})
"#;
    let program = lower(&parse(src).expect("解析")).expect("lower");
    let calls = RefCell::new(0);
    let mut l = Ledger::new();
    let a = ActionRegistry::new();
    let o = Session::new(端口(&calls, 0.9, &[]), &CalibStore::new(), &a)
        .with_companions(CompanionMode::Same)
        .run(&program, &EntryArgs::default(), &mut l)
        .unwrap_or_else(|e| panic!("{}", e.render()));
    assert_eq!(o.improve[0]["companions"].as_array().unwrap().len(), 5);
}

/// 主控 2026-09-30：伴随题跟原题走，不跟裂变块走——材料切成 N 块（N > 1）时，伴随题判断条目恰 5 条
#[test]
fn 裂变出多块时伴随题只问一次() {
    let p = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../profiles/jev-1.13.0.json");
    let mut j: Json = serde_json::from_str(&std::fs::read_to_string(p).unwrap()).unwrap();
    j["window"]["text_slots"]["claim_bearing_ctx"]["usable_lower"] = Json::from(40);
    let mut calib = CalibStore::new();
    calib.profile = jpp::effects::Profile::from_json(&j).unwrap();
    let 段 = "这是一段足够长的材料，用来让裂变按窗口把它切成好几块。".repeat(12);
    let src = format!(
        "budget {{calls: 64, cost: 0, depth: 16}};\n{}\nlet e = cut(judge(state(mat(\"{段}\")), test(\"这段材料提到了裂变吗？\", \"k\", {{fission: \"approx\"}})), {{declare: {{hi: 0.7, lo: 0.3}}}});\nhandle(e, {{act: fn() {{ 1 }}, ignore: fn() {{ 0 }}, unsure: fn(u) {{ {{exit: u}} }}}})\n",
        库()
    );
    let program = lower(&parse(&src).expect("解析")).expect("lower");
    let calls = RefCell::new(0);
    let mut l = Ledger::new();
    let a = ActionRegistry::new();
    let o = Session::new(端口(&calls, 0.9, &[]), &calib, &a)
        .with_companions(CompanionMode::Same)
        .run(&program, &EntryArgs::default(), &mut l)
        .unwrap_or_else(|e| panic!("{}", e.render()));
    let 伴随条目 = l
        .entries
        .iter()
        .filter(|e| matches!(e, Entry::Judge { calib_ref: Some(c), .. } if format!("{c:?}").contains("unsure-companion-premise")
            || format!("{c:?}").contains("diag-") || format!("{c:?}").contains("unsure-companion-")))
        .count();
    let 全部 = 判断条目(&l);
    assert!(
        全部 - 伴随条目 > 1,
        "要真的切成多块：全部 {全部}，伴随题 {伴随条目}"
    );
    assert_eq!(伴随条目, 5, "一道原题只问一次伴随题");
    assert_eq!(o.improve.len(), 1);
}

/// 复查 2026-09-30 小项 3：select 裂变出多个胜者时，第二层派生是非题也不带伴随题——全程伴随题判断条目恰 5 条
#[test]
fn select裂变第二层不带伴随题() {
    let p = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../profiles/jev-1.13.0.json");
    let mut j: Json = serde_json::from_str(&std::fs::read_to_string(p).unwrap()).unwrap();
    j["window"]["text_slots"]["claim_bearing_ctx"]["usable_lower"] = Json::from(40);
    let mut calib = CalibStore::new();
    calib.profile = jpp::effects::Profile::from_json(&j).unwrap();
    let 段 = "这是一段足够长的材料，用来让裂变按窗口把它切成好几块。".repeat(12);
    let src = format!(
        "budget {{calls: 64, cost: 0, depth: 16}};\n{}\nlet e = cut(judge(state(mat(\"{段}\"), {{over: [mat(\"香蕉\"), mat(\"苹果\"), mat(\"葡萄\"), mat(\"梨\")]}}), select(\"材料里写到的是哪种水果？\", \"k\", {{fission: \"approx\"}})));\n{{exit: exit_kind(e)}}\n",
        库()
    );
    let program = lower(&parse(&src).expect("解析")).expect("lower");
    let calls = RefCell::new(0);
    let mut l = Ledger::new();
    let a = ActionRegistry::new();
    Session::new(端口(&calls, 0.9, &[]), &calib, &a)
        .with_companions(CompanionMode::Same)
        .run(&program, &EntryArgs::default(), &mut l)
        .unwrap_or_else(|e| panic!("{}", e.render()));
    let 是伴随 = |c: &str| c.contains("unsure-companion-") || c.contains("diag-");
    let 伴随条目 = l
        .entries
        .iter()
        .filter(
            |e| matches!(e, Entry::Judge { calib_ref: Some(c), .. } if 是伴随(&format!("{c:?}"))),
        )
        .count();
    let 第二层 = l
        .entries
        .iter()
        .filter(|e| matches!(e, Entry::Judge { calib_ref: Some(c), .. } if format!("{c:?}").contains("fission-noul")))
        .count();
    assert!(第二层 > 1, "要真的有多个胜者进第二层：{第二层}");
    assert_eq!(伴随条目, 5, "第二层派生题不带伴随题，一道原题只问一次");
}

/// 复查 2026-09-30 小项 2：命令行审计重放的 `lib_version` 按账本头定带不带序言——首跑关伴随题（不带序言）或开着
/// （带序言），重放都不报假的 `W-header … lib_version`
#[test]
fn 命令行重放的lib_version按账本头() {
    let d = std::env::temp_dir().join(format!("jpp-companions-libver-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    std::fs::write(
        d.join("p.jpp"),
        "budget {calls: 8, cost: 0, depth: 16};\nlet e = cut(judge(state(mat(\"甲\")), test(\"行吗\", \"k\")));\n{k: exit_kind(e), e: e}\n",
    )
    .unwrap();
    std::fs::write(
        d.join("fx.json"),
        json!({"observations": [{"on": ["甲"], "op": "test", "text": "行吗", "calib": "k", "answer": {"Noul": 0.9}}]}).to_string(),
    )
    .unwrap();
    let jpp = |args: &[&str]| {
        std::process::Command::new(env!("CARGO_BIN_EXE_jpp"))
            .current_dir(&d)
            .args(args)
            .output()
            .unwrap()
    };
    for (名, 发法) in [("关", "off"), ("开", "same")] {
        let l = format!("l-{发法}.json");
        let o = jpp(&[
            "run",
            "p.jpp",
            "--fixtures",
            "fx.json",
            "--companions",
            发法,
            "--ledger-out",
            &l,
            "--output",
            "r1.json",
        ]);
        assert!(
            o.status.success(),
            "{名}：{}",
            String::from_utf8_lossy(&o.stderr)
        );
        let o = jpp(&["run", "p.jpp", "--replay", &l, "--output", "r2.json"]);
        assert!(
            o.status.success(),
            "{名}：{}",
            String::from_utf8_lossy(&o.stderr)
        );
        let 全部 = format!(
            "{}{}",
            String::from_utf8_lossy(&o.stderr),
            std::fs::read_to_string(d.join("r2.json")).unwrap()
        );
        assert!(
            !全部.contains("lib_version 旧"),
            "{名}：重放不该报 lib_version 不一致：{全部}"
        );
    }
    let _ = std::fs::remove_dir_all(&d);
}

/// Z0912（裁定七十三 (1)）：带画像中段 δ（`delta.noul.mid` 0.1281）时，伴随题读数在 0.5 ± δ 内没有信号，
/// 不能当成「题不清」或「两可」，默认链照常往下走（发「为什么」、取）；带外的照旧
fn 带画像() -> CalibStore {
    let p = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../profiles/jev-1.13.0.json");
    let j: Json = serde_json::from_str(&std::fs::read_to_string(p).unwrap()).unwrap();
    let mut calib = CalibStore::new();
    calib.profile = jpp::effects::Profile::from_json(&j).unwrap();
    calib
}

#[test]
fn 伴随选路_带内没有信号_照常往下走() {
    // 前提 0.45（带内）：改前判「题不清」不补；改后不算数，参照也在带内 → 发「为什么」去补
    let 带内 = [
        ("unsure-companion-premise", 0.45),
        ("diag-two-judgments", 0.1),
        ("unsure-companion-cut", 0.1),
        ("unsure-companion-reference", 0.55),
    ];
    let r = 跑(走链, CompanionMode::Same, 0.5, &带内, 带画像());
    assert_ne!(r.outcome.unsure_default[0]["why"], "unclear", "{:?}", r.outcome.unsure_default);
    assert_ne!(r.outcome.unsure_default[0]["why"], "ambiguous", "{:?}", r.outcome.unsure_default);
    assert!(为什么发了(&r.ledger), "带内无信号，照常发「为什么」");
    // 同一组读数不带画像（δ 未知）：照改前，前提 0.45 < 0.5 判题不清
    let r0 = 跑(走链, CompanionMode::Same, 0.5, &带内, CalibStore::new());
    assert_eq!(r0.outcome.unsure_default[0]["why"], "unclear");
}

#[test]
fn 伴随选路_带外照旧() {
    let 带外 = [
        ("unsure-companion-premise", 0.2),
        ("diag-two-judgments", 0.1),
        ("unsure-companion-cut", 0.1),
        ("unsure-companion-reference", 0.9),
    ];
    let r = 跑(走链, CompanionMode::Same, 0.5, &带外, 带画像());
    assert_eq!(r.outcome.unsure_default[0]["why"], "unclear");
    assert!(!为什么发了(&r.ledger));
}

/// 裁定七十六（主控板 Z0933）：声明了证据槽而条目没有（`insufficient`）、候选来自题的 `lacks` 时，原因已经确定，
/// 默认链不按伴随题的「题不清」选路、不发「为什么」，按 lacks 顺序逐类取；非缺料的未决照旧先问伴随题
const 缺料链: &str = r#"unsure_source({fetch: fn(q, need, m) { if need == "参照" { mat("对方的过往合作记录") } else { fail("没有这一类") } }});
let f = form("test", "这两方适合合作吗？", {calib: "k", evidence: ["ref"], lacks: ["材料", "参照"]});
let e = cut(judge(state(mat("甲方与乙方的合作意向")), fill(f, {})));
handle(e, {act: fn() { "合作" }, ignore: fn() { "不合作" }})
"#;

const 带内链: &str = r#"unsure_source({fetch: fn(q, need, m) { if need == "参照" { mat("对方的过往合作记录") } else { fail("没有这一类") } }});
let f = form("test", "这两方适合合作吗？", {calib: "k", lacks: ["材料", "参照"]});
let e = cut(judge(state(mat("甲方与乙方的合作意向")), fill(f, {})), {declare: {hi: 0.7, lo: 0.3}});
handle(e, {act: fn() { "合作" }, ignore: fn() { "不合作" }})
"#;

/// 「前提成立」有把握地判否（0.1，中段 δ 之外）；其余伴随题在带内，「最缺哪类」没选出
const 前提否: [(&str, f64); 4] = [
    ("unsure-companion-premise", 0.1),
    ("diag-two-judgments", 0.5),
    ("unsure-companion-cut", 0.5),
    ("unsure-companion-reference", 0.5),
];

#[test]
fn 缺料直取_不问伴随题题不清() {
    let r = 跑(缺料链, CompanionMode::Same, 0.9, &前提否, 带画像());
    let ud = &r.outcome.unsure_default;
    assert_eq!(ud.len(), 1, "{ud:?}");
    assert_eq!(ud[0]["cause"], "insufficient", "{ud:?}");
    assert_eq!(ud[0]["source"], "lacks", "{ud:?}");
    assert_eq!(ud[0]["why"], Json::Null, "伴随题判「前提不成立」不再读成题不清：{ud:?}");
    assert_eq!(ud[0]["missed"], json!(["材料"]), "按 lacks 顺序逐类取：{ud:?}");
    assert_eq!(ud[0]["fetched"], json!(["参照"]), "{ud:?}");
    assert_eq!(ud[0]["end"], "decided", "{ud:?}");
    assert_eq!(ud[0]["asked"], json!([]), "{ud:?}");
    assert!(!为什么发了(&r.ledger), "不发「为什么拿不准」");
    // 这道题确实带了伴随题，「前提成立」读数是 0.1（不是没登记的中性读数）
    let imp = r.outcome.improve.iter().find(|x| x["q"] == "这两方适合合作吗？").expect("原题有伴随题");
    let p = imp["companions"].as_array().unwrap().iter().find(|c| c["kind"] == "unsure-companion-premise").unwrap();
    assert_eq!(p["p"], json!(0.1), "{imp}");
}

#[test]
fn 非缺料_照旧问伴随题() {
    let r = 跑(带内链, CompanionMode::Same, 0.5, &前提否, 带画像());
    let ud = &r.outcome.unsure_default;
    assert_eq!(ud.len(), 1, "{ud:?}");
    assert_ne!(ud[0]["cause"], "insufficient", "{ud:?}");
    assert_eq!(ud[0]["why"], "unclear", "{ud:?}");
    assert_eq!(ud[0]["fetched"], json!([]), "{ud:?}");
}

/// 裁定七十七（归 R-043）：伴随题在带外给出诊断，默认链就用它，不再串行问「为什么拿不准」；取不到直接到末端。
/// 元题只在伴随题全在带内时才问。报告每行 `route` 分得开走的是哪一路
const 三类链: &str = r#"unsure_source({need: ["材料", "语境", "参照"], fetch: fn(q, need, m) { "关于" + need + "的补充" }});
let e = cut(judge(state(mat("甲方与乙方的合作意向")), test("这两方适合合作吗？", "k")));
handle(e, {act: fn() { "合作" }, ignore: fn() { "不合作" }})
"#;

#[test]
fn 七十七_带外选出类别_只取它_不问为什么() {
    // 「最缺哪类」带外选出第二类「过往合作」；补后仍拿不准，也不再取别的类、不问为什么，到末端
    let 选出 = [
        ("unsure-companion-premise", 0.9),
        ("unsure-companion-material", 1.0),
        ("diag-two-judgments", 0.1),
        ("unsure-companion-cut", 0.5),
        ("unsure-companion-reference", 0.5),
    ];
    let r = 跑(走链, CompanionMode::Same, 0.5, &选出, 带画像());
    let ud = &r.outcome.unsure_default;
    assert_eq!(ud[0]["route"], "companion", "{ud:?}");
    assert_eq!(ud[0]["fetched"], json!(["过往合作"]), "{ud:?}");
    assert_eq!(ud[0]["asked"], json!([]), "{ud:?}");
    assert!(!为什么发了(&r.ledger), "带外诊断出了类别，不再问为什么");
}

#[test]
fn 七十七_参照与语境不够_按候选顺序取这两类_不问为什么() {
    // 「最缺哪类」没选出，「参照与语境够吗」带外判否：只取语境、参照（候选顺序），不取材料，不问为什么
    let 参照否 = [
        ("unsure-companion-premise", 0.9),
        ("diag-two-judgments", 0.1),
        ("unsure-companion-cut", 0.5),
        ("unsure-companion-reference", 0.2),
    ];
    let r = 跑(三类链, CompanionMode::Same, 0.5, &参照否, 带画像());
    let ud = &r.outcome.unsure_default;
    assert_eq!(ud[0]["route"], "companion", "{ud:?}");
    assert_eq!(ud[0]["fetched"], json!(["语境", "参照"]), "{ud:?}");
    assert_eq!(ud[0]["asked"], json!([]), "{ud:?}");
    assert!(!为什么发了(&r.ledger), "带外判否就是诊断，不问为什么");
}

#[test]
fn 七十七_伴随题全在带内_照旧问为什么() {
    let 带内 = [
        ("unsure-companion-premise", 0.55),
        ("diag-two-judgments", 0.45),
        ("unsure-companion-cut", 0.5),
        ("unsure-companion-reference", 0.5),
    ];
    let r = 跑(三类链, CompanionMode::Same, 0.5, &带内, 带画像());
    let ud = &r.outcome.unsure_default;
    assert_eq!(ud[0]["route"], "why", "{ud:?}");
    assert!(为什么发了(&r.ledger), "没有信号，照旧发「为什么」");
}

#[test]
fn 七十六的行_route_写缺料直取() {
    let r = 跑(缺料链, CompanionMode::Same, 0.9, &前提否, 带画像());
    assert_eq!(r.outcome.unsure_default[0]["route"], "missing-slot", "{:?}", r.outcome.unsure_default);
}

/// 裁定七十七交叉情形（预注册 10.2 第 4a 条，主控读法，待 Jpp 确认）：「最缺哪类」选出参照、「参照与语境够吗」也带外判否，
/// 参照取不到（只有语境取得到）：按「最缺哪类」这个更具体的诊断走，参照取不到就到末端，不改取语境、不问为什么
#[test]
fn 七十七_交叉_选出的取不到_不改取另一类() {
    let 只有语境 = r#"unsure_source({need: ["材料", "语境", "参照"], fetch: fn(q, need, m) { if need == "语境" { "关于语境的补充" } else { fail("没有这一类") } }});
let e = cut(judge(state(mat("甲方与乙方的合作意向")), test("这两方适合合作吗？", "k")));
handle(e, {act: fn() { "合作" }, ignore: fn() { "不合作" }})
"#;
    let 交叉 = [
        ("unsure-companion-premise", 0.9),
        ("unsure-companion-material", 2.0),
        ("diag-two-judgments", 0.1),
        ("unsure-companion-cut", 0.5),
        ("unsure-companion-reference", 0.2),
    ];
    let r = 跑(只有语境, CompanionMode::Same, 0.5, &交叉, 带画像());
    let ud = &r.outcome.unsure_default;
    assert_eq!(ud[0]["route"], "companion", "{ud:?}");
    assert_eq!(ud[0]["fetched"], json!([]), "不改取语境：{ud:?}");
    assert_eq!(ud[0]["missed"], json!(["参照"]), "{ud:?}");
    assert_eq!(ud[0]["needed"], json!(["参照"]), "{ud:?}");
    assert_eq!(ud[0]["end"], "handoff", "{ud:?}");
    assert!(!为什么发了(&r.ledger));
    // 点名类别无取法（Jpp 2026-10-02）：伴随题点名了参照、取不到，按类别计一次
    assert_eq!(r.outcome.named_unfetchable, json!({"参照": 1}), "{:?}", r.outcome.named_unfetchable);
}

/// 点名了、取到了，不计「点名类别无取法」；没有点名的不出这一段
#[test]
fn 点名取到不计() {
    let 选出 = [
        ("unsure-companion-premise", 0.9),
        ("unsure-companion-material", 1.0),
        ("diag-two-judgments", 0.1),
        ("unsure-companion-cut", 0.5),
        ("unsure-companion-reference", 0.5),
    ];
    let r = 跑(走链, CompanionMode::Same, 0.5, &选出, 带画像());
    assert_eq!(r.outcome.unsure_default[0]["fetched"], json!(["过往合作"]));
    assert_eq!(r.outcome.named_unfetchable, Json::Null, "{:?}", r.outcome.named_unfetchable);
}
