//! 层开着时的直接写入（步 15h-3，B160）：生成交出、层开着、生成还没读的时候，程序执行的 `do`、`ask`、
//! `transform` 与这期间 `cut` 记下的 `CalibUsed` 也进层，收层时与层里的生成、判断一起按登记序入账；
//! 不可逆 `do` 先收层再写意向（B55）；层开着时同键的 `do` 从层里取、不重复执行。闭包端口，不发请求。
//!
//! 依据：B160；B55；B124；预注册 `地基/过程记录/工程-步15h-3.md` 一·3 (a)–(h)；(i) 见记录二·1 出入 4
//! （谱系按键查先查层，B72-4），(j)(k) 见出入 5（效应复用命中的条目进层，B171）。

mod common;

use jpp::effects::{
    CalibStore, CertGrade, EffectError, FnPort, GenResult, JudgeResult, Ports, ReplayPorts,
};
use jpp::interp::{ActionRegistry, TaintOut};
use jpp::ledger::Ledger;
use jpp::value::{Answer, Value};
use jpp::{Outcome, lower, run, run_replay, syntax::parse};
use serde_json::{Value as Json, json};
use std::cell::Cell;
use std::rc::Rc;

fn 库() -> CalibStore {
    let mut c = CalibStore::new();
    c.put("k", 0.75, 0.25, 50, "上岗", Some(0.05)).unwrap();
    c
}

fn 端口<'a>() -> Ports<'a> {
    Ports::new()
        .with(FnPort::judge("fixed-0", |_s, qs| {
            Ok::<_, EffectError>(JudgeResult {
                answers: qs.iter().map(|_| Answer::Noul(0.9)).collect(),
                tokens: 0,
                cost: 0.0,
                mode_share: vec![],
                perms: vec![],
                confidence: vec![],
            })
        }))
        .with(FnPort::generate("fixed-0", |_p, _c, _n, _r| {
            Ok(GenResult {
                outputs: vec![json!("候选")],
                ..Default::default()
            })
        }))
        .with(FnPort::ask("fixed-0", |_s, _q| Ok(Some(Answer::Noul(0.9)))))
}

/// 可逆的「记下」与不可逆的「发出」：都原样返回实参，各计调用次数
fn 动作(记: Rc<Cell<usize>>, 发: Rc<Cell<usize>>) -> ActionRegistry {
    let mut a = ActionRegistry::new();
    a.register("记下", 0.0, true, TaintOut::Trusted, move |args| {
        记.set(记.get() + 1);
        Ok(args.first().cloned().unwrap_or(Value::Unit))
    });
    a.register("发出", 0.0, false, TaintOut::Trusted, move |args| {
        发.set(发.get() + 1);
        Ok(args.first().cloned().unwrap_or(Value::Unit))
    });
    a
}

/// 程序骨架：先登记生成，再对另一份材料切一道判断（刷新交出生成、开层），不读生成，做被测的效应，最后读生成
fn 程序(效应: &str, 返回: &str) -> String {
    format!(
        r#"budget {{calls: 20, cost: 0, depth: 64, escalate: 1}};
let g = gen("提候选", [mat("需求")], 1, 0);
let e = cut(judge(state(mat("甲")), test("行吗", "k")));
let k = exit_kind(e);
{效应}
{{k: k, x: content(g[0]){返回}}}"#
    )
}

fn 跑(src: &str, acts: &ActionRegistry, ledger: &mut Ledger) -> Outcome {
    let program = lower(&parse(src).expect("parse")).expect("lower");
    run(&program, 端口(), &库(), acts, ledger).unwrap_or_else(|e| panic!("{}", e.render()))
}

/// 账本条目的种类，按账本顺序（头行不计）
fn 条目序(ledger: &Ledger) -> Vec<String> {
    ledger
        .encode()
        .lines()
        .filter_map(|l| {
            let j: Json = serde_json::from_str(l).ok()?;
            let e = j.get("entry")?;
            let (k, v) = e.as_object()?.iter().next()?;
            Some(match k.as_str() {
                "Effect" => v["kind"].as_str().unwrap_or("effect").to_string(),
                "Judge" => "judge".into(),
                "CalibUsed" => "calib_used".into(),
                "Intent" => "intent".into(),
                "Ask" => "ask".into(),
                other => other.to_lowercase(),
            })
        })
        .collect()
}

fn 计数() -> (Rc<Cell<usize>>, Rc<Cell<usize>>) {
    (Rc::new(Cell::new(0)), Rc::new(Cell::new(0)))
}

/// (a) 可逆 do：按登记序排在层内条目之后
#[test]
fn a_可逆_do_进层() {
    let (记, 发) = 计数();
    let acts = 动作(记.clone(), 发);
    let mut ledger = Ledger::new();
    let src = 程序("let d = do(\"记下\", [\"甲\"], 0);", ", d: content(d)");
    let o = 跑(&src, &acts, &mut ledger);
    assert_eq!(条目序(&ledger), ["gen", "judge", "calib_used", "do"]);
    assert_eq!(o.value_json()["d"], json!("甲"));
    assert_eq!(记.get(), 1);
}

/// (b) ask：进层
#[test]
fn b_ask_进层() {
    let (记, 发) = 计数();
    let acts = 动作(记, 发);
    let mut ledger = Ledger::new();
    let src = 程序(
        "let a = exit_kind(ask(state(mat(\"乙\")), test(\"批准吗？\", \"human\")));",
        ", a: a",
    );
    let o = 跑(&src, &acts, &mut ledger);
    assert_eq!(条目序(&ledger), ["gen", "judge", "calib_used", "ask"]);
    assert_eq!(o.value_json()["a"], json!("act"));
}

/// (c) transform：进层
#[test]
fn c_transform_进层() {
    let (记, 发) = 计数();
    let acts = 动作(记, 发);
    let mut ledger = Ledger::new();
    let src = 程序(
        "let t = transform(fn(m) { \"变 \" + content(m) }, mat(\"丙\"));",
        ", t: content(t)",
    );
    let o = 跑(&src, &acts, &mut ledger);
    assert_eq!(条目序(&ledger), ["gen", "judge", "calib_used", "transform"]);
    assert_eq!(o.value_json()["t"], json!("变 丙"));
}

/// (d) 只有 cut：CalibUsed 排在它那条判断之后
#[test]
fn d_calib_used_进层() {
    let (记, 发) = 计数();
    let acts = 动作(记, 发);
    let mut ledger = Ledger::new();
    跑(&程序("", ""), &acts, &mut ledger);
    assert_eq!(条目序(&ledger), ["gen", "judge", "calib_used"]);
}

/// (e) 不可逆 do：先收层，意向与结果随后即刻入账
#[test]
fn e_不可逆_do_先收层() {
    let (记, 发) = 计数();
    let acts = 动作(记, 发.clone());
    let mut ledger = Ledger::new();
    let src = 程序("let s = do(\"发出\", [\"丁\"], 0);", ", s: content(s)");
    let o = 跑(&src, &acts, &mut ledger);
    assert_eq!(
        条目序(&ledger),
        ["gen", "judge", "calib_used", "intent", "do"]
    );
    assert_eq!(o.value_json()["s"], json!("丁"));
    assert_eq!(发.get(), 1);
}

/// (f) 层开着时同一站点、同一实参的可逆 do 调两次：只执行一次，第二次从层里取
#[test]
fn f_同键_do_从层里取() {
    let (记, 发) = 计数();
    let acts = 动作(记.clone(), 发);
    let mut ledger = Ledger::new();
    let src = 程序(
        "let f = fn() { do(\"记下\", [\"甲\"], 0) };\nlet d1 = f();\nlet d2 = f();",
        ", d: [content(d1), content(d2)]",
    );
    let o = 跑(&src, &acts, &mut ledger);
    assert_eq!(记.get(), 1, "第二次从层里取");
    assert_eq!(o.value_json()["d"], json!(["甲", "甲"]));
    assert_eq!(条目序(&ledger), ["gen", "judge", "calib_used", "do"]);
}

/// (g) (a)–(d) 的程序只凭账本重放：零调用、值逐字段相同
#[test]
fn g_只凭账本重放() {
    for (效应, 返回) in [
        ("let d = do(\"记下\", [\"甲\"], 0);", ", d: content(d)"),
        (
            "let a = exit_kind(ask(state(mat(\"乙\")), test(\"批准吗？\", \"human\")));",
            ", a: a",
        ),
        (
            "let t = transform(fn(m) { \"变 \" + content(m) }, mat(\"丙\"));",
            ", t: content(t)",
        ),
        ("", ""),
    ] {
        let (记, 发) = 计数();
        let acts = 动作(记.clone(), 发);
        let src = 程序(效应, 返回);
        let mut ledger = Ledger::new();
        let o = 跑(&src, &acts, &mut ledger);
        ledger.rebuild_index();
        let program = lower(&parse(&src).expect("parse")).expect("lower");
        let again = run_replay(
            &program,
            ReplayPorts::ports("fixed-0"),
            &库(),
            &acts,
            &mut ledger,
        )
        .unwrap_or_else(|e| panic!("{效应}：{}", e.render()));
        assert_eq!(again.value_json(), o.value_json(), "{效应}");
        assert_eq!(again.cost.calls, 0, "{效应}");
        assert_eq!(记.get(), if 效应.contains("记下") { 1 } else { 0 });
    }
}

/// (h) 两层嵌套：map 里两份材料各 do 一次，层开着，两条 do 按调用先后排在层内条目之后
#[test]
fn h_map_里的_do_按调用先后() {
    let (记, 发) = 计数();
    let acts = 动作(记.clone(), 发);
    let mut ledger = Ledger::new();
    let src = 程序(
        "let ds = map([\"甲\", \"乙\"], fn(t) { do(\"记下\", [t], 0) });",
        ", d: map(ds, fn(x) { content(x) })",
    );
    let o = 跑(&src, &acts, &mut ledger);
    assert_eq!(条目序(&ledger), ["gen", "judge", "calib_used", "do", "do"]);
    let lines: Vec<String> = ledger.encode().lines().map(String::from).collect();
    let 甲 = lines
        .iter()
        .position(|l| l.contains(r#""kind":"do""#) && l.contains("甲"));
    let 乙 = lines
        .iter()
        .position(|l| l.contains(r#""kind":"do""#) && l.contains("乙"));
    assert!(甲 < 乙, "{lines:?}");
    assert_eq!(o.value_json()["d"], json!(["甲", "乙"]));
    assert_eq!(记.get(), 2);
}

/// (i) 谱系查找先查层（施工中发现，预注册之后补，见记录二·1 出入 4）：层开着时切出正式线出口，
/// 被判断材料由试用线出口选出，这道判断的条目还在层里。谱系要看得见它的 `parents`，
/// 否则祖先一个也查不到、按「谱系完整」放行，不可逆 do 就执行了（B72-4、J-08）。
#[test]
fn i_谱系先查层_试用线祖先不放行() {
    let mut c = CalibStore::new();
    common::certified(&mut c, "k2", 0.8, 0.2, 50);
    common::certified(&mut c, "t", 0.8, 0.2, 50);
    for cert in c.records.get_mut("t").unwrap().certs.values_mut() {
        cert.grade = CertGrade::Trial;
    }
    let (记, 发) = 计数();
    let acts = 动作(记, 发.clone());
    let src = r#"budget {calls: 20, cost: 0, depth: 64, escalate: 1};
let r = sieve([mat("甲"), mat("乙")], test("选哪个", "t"));
let m = r.value[0].item;
let g = gen("提候选", [mat("需求")], 1, 0);
let out = handle(cut(judge(state(m), test("该发吗", "k2"))), {act: fn() { content(do("发出", ["丁"], 0)) }, ignore: fn() { "不发" }, unsure: fn(u) { consume(u, "drop"); "不发" }});
{out: out, x: content(g[0])}"#;
    // 测谱系放行本身：开 --guard（意图汇编 11a、B187）
    let mut program = lower(&parse(src).expect("parse")).expect("lower");
    program.entry.guard = true;
    let e = run(&program, 端口(), &c, &acts, &mut Ledger::new())
        .map(|o| o.value_json())
        .expect_err("谱系里有试用线出口，不可逆 do 应被拒");
    let msg = e.render();
    assert!(msg.contains("J-08") && msg.contains("Trial"), "{msg}");
    assert_eq!(发.get(), 0, "动作不该执行");
    // 不开把关（默认）：同一程序照常执行，动作做了一次
    program.entry.guard = false;
    run(&program, 端口(), &c, &acts, &mut Ledger::new()).expect("默认不拦");
    assert_eq!(发.get(), 1, "默认照常执行");
}

/// 账本里 `Effect` 条目的 (kind, 有没有 reused_from)，按账本顺序
fn 效应条目(ledger: &Ledger) -> Vec<(String, bool)> {
    ledger
        .encode()
        .lines()
        .filter_map(|l| {
            let j: Json = serde_json::from_str(l).ok()?;
            let e = j.get("entry")?.get("Effect")?;
            Some((e["kind"].as_str()?.to_string(), !e["reused_from"].is_null()))
        })
        .collect()
}

/// (j) 效应复用（步 19）命中的变换条目也进层（Q3，B171；记录二·1 出入 5）：层开着时同方法、同材料、
/// 不同站点调两次，第二次复用命中，复用条目按登记序排在层内条目之后
#[test]
fn j_变换复用条目进层() {
    let (记, 发) = 计数();
    let acts = 动作(记, 发);
    let mut ledger = Ledger::new();
    let src = 程序(
        "let f = fn(m) { \"变 \" + content(m) };\nlet t1 = transform(f, mat(\"丙\"));\nlet t2 = transform(f, mat(\"丙\"));",
        ", t: [content(t1), content(t2)]",
    );
    let o = 跑(&src, &acts, &mut ledger);
    assert_eq!(
        条目序(&ledger),
        ["gen", "judge", "calib_used", "transform", "transform"]
    );
    assert_eq!(
        效应条目(&ledger)[1..],
        [
            ("transform".to_string(), false),
            ("transform".to_string(), true)
        ]
    );
    assert_eq!(o.value_json()["t"], json!(["变 丙", "变 丙"]));
}

/// (k) 效应复用命中的生成条目进层，同一站点再调一次按账本键从层里取（Q3，B171；出入 5）：
/// g1 先读（记进本运行复用表）；再开一层，`h` 里同一个 gen 调两次（站点与 g1 不同）。
/// 第一次复用命中、条目进层；第二次查层命中，不再复用、不多写一条
#[test]
fn k_生成复用条目进层_同键查层() {
    let (记, 发) = 计数();
    let acts = 动作(记, 发);
    let 调用 = Rc::new(Cell::new(0usize));
    let n = 调用.clone();
    let ports = Ports::new()
        .with(FnPort::judge("fixed-0", |_s, qs| {
            Ok::<_, EffectError>(JudgeResult {
                answers: qs.iter().map(|_| Answer::Noul(0.9)).collect(),
                tokens: 0,
                cost: 0.0,
                mode_share: vec![],
                perms: vec![],
                confidence: vec![],
            })
        }))
        .with(FnPort::generate("fixed-0", move |_p, _c, _n, _r| {
            n.set(n.get() + 1);
            Ok(GenResult {
                outputs: vec![json!("候选")],
                ..Default::default()
            })
        }));
    let src = r#"budget {calls: 20, cost: 0, depth: 64};
let g1 = gen("提候选", [mat("需求")], 1, 0);
let x1 = content(g1[0]);
let h = fn() { gen("提候选", [mat("需求")], 1, 0) };
let g0 = gen("另一个", [mat("需求")], 1, 0);
let e = cut(judge(state(mat("甲")), test("行吗", "k")));
let k = exit_kind(e);
let g2 = h();
let g3 = h();
{k: k, x: [x1, content(g2[0]), content(g3[0]), content(g0[0])]}"#;
    let program = lower(&parse(src).expect("parse")).expect("lower");
    let mut ledger = Ledger::new();
    let o = run(&program, ports, &库(), &acts, &mut ledger)
        .unwrap_or_else(|e| panic!("{}", e.render()));
    assert_eq!(o.value_json()["x"], json!(["候选", "候选", "候选", "候选"]));
    assert_eq!(调用.get(), 2, "g1 与 g0 各发一次，g2 复用、g3 查层");
    // g3 查层命中、不再算一次复用（只查账本时这里是 2：同键条目进层两次，账本追加时丢掉后一条）
    assert_eq!(
        o.cache.as_ref().map(|c| c.hits.get("gen").copied()),
        Some(Some(1))
    );
    assert_eq!(
        条目序(&ledger),
        ["gen", "gen", "judge", "calib_used", "gen"]
    );
    assert_eq!(
        效应条目(&ledger),
        [
            ("gen".to_string(), false),
            ("gen".to_string(), false),
            ("gen".to_string(), true)
        ]
    );
}
