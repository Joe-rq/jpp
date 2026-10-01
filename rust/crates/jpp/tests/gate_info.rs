//! 步 30 B 段：闸门信息值 `gate_info`（主会话裁定四十三）与切分点 `split_point`（B7 后半）两个内置。
//! 数值在过程记录 `地基/过程记录/工程-步30-价值函数.md` §7.4、§7.8 预注册。

mod value_support;

use jpp::effects::{CalibStore, EffectError, FnPort, JudgeResult, Ports};
use jpp::ledger::Ledger;
use jpp::{ActionRegistry, lower, run, syntax::parse};

fn 跑(src: &str, calib: &CalibStore) -> serde_json::Value {
    let program = lower(&parse(src).expect("解析")).expect("lower");
    let ports = Ports::new()
        .with(FnPort::judge("fixed-0", |_s, _qs| {
            Err::<JudgeResult, _>(EffectError("不该判断".into()))
        }))
        .with(FnPort::generate("fixed-0", |_p, _c, _n, _r| {
            Err(EffectError("不该 gen".into()))
        }))
        .with(FnPort::ask("fixed-0", |_s, _q| {
            Err(EffectError("不该 ask".into()))
        }));
    let mut ledger = Ledger::new();
    run(&program, ports, calib, &ActionRegistry::new(), &mut ledger)
        .unwrap_or_else(|e| panic!("{}", e.render()))
        .value_json()
}

fn 数(v: &serde_json::Value) -> Vec<f64> {
    v.as_array()
        .unwrap()
        .iter()
        .map(|x| x.as_f64().unwrap())
        .collect()
}

/// 模板记录（j = 0）的互信息与 j = 40 那条的（过程记录 §7.9）
const 模板值: f64 = 0.822475;
const 半未决: f64 = 0.362869;

/// 两道是非题候选：一道键 g1（有记录），一道键 g0（无记录）；题类都是 attr
const 两候选: &str = r#"
budget {calls: 0, cost: 0, depth: 16};
let st = state(mat("一段材料"));
gate_info([test("有作者吗？", "g1"), test("有读者吗？", "g0")], [st, st])
"#;

#[test]
fn 只有同题类一条记录时_无记录的取它_两者打平() {
    let v = 数(&跑(两候选, &value_support::认证库(&[("g1", 0, None)])));
    assert!((v[0] - 模板值).abs() < 1e-6, "{v:?}");
    assert!((v[1] - 模板值).abs() < 1e-6, "{v:?}");
}

#[test]
fn 再加一条折扣更低的_无记录的取最低_落在不利侧但不为零() {
    let v = 数(&跑(
        两候选,
        &value_support::认证库(&[("g1", 0, None), ("g2", 40, None)]),
    ));
    assert!((v[0] - 模板值).abs() < 1e-6, "{v:?}");
    assert!((v[1] - 半未决).abs() < 1e-6, "{v:?}");
    assert!(v[1] > 0.0);
}

#[test]
fn 空库_全体只按熵() {
    let v = 数(&跑(两候选, &CalibStore::new()));
    assert_eq!(v, vec![1.0, 1.0]);
}

#[test]
fn 只凭账本重放_闸门信息值与首跑相同() {
    use jpp::{EntryArgs, Session};
    let program = lower(&parse(两候选).expect("解析")).expect("lower");
    let calib = value_support::认证库(&[("g1", 0, None), ("g2", 40, None)]);
    let 端口 = || {
        Ports::new()
            .with(FnPort::judge("fixed-0", |_s, _qs| {
                Err::<JudgeResult, _>(EffectError("不该判断".into()))
            }))
            .with(FnPort::generate("fixed-0", |_p, _c, _n, _r| {
                Err(EffectError("不该 gen".into()))
            }))
            .with(FnPort::ask("fixed-0", |_s, _q| {
                Err(EffectError("不该 ask".into()))
            }))
    };
    let acts = ActionRegistry::new();
    let mut ledger = Ledger::new();
    let first = Session::new(端口(), &calib, &acts)
        .run(&program, &EntryArgs::default(), &mut ledger)
        .unwrap_or_else(|e| panic!("{}", e.render()))
        .value_json();
    // 候选自己的记录 g1 与取了最低折扣的 g2 都进账本
    let used: Vec<&String> = ledger.calib_used.keys().collect();
    assert!(
        used.contains(&&"g1".to_string()) && used.contains(&&"g2".to_string()),
        "{used:?}"
    );
    let mut restored = CalibStore::new();
    Session::restore_calib(&mut restored, &ledger).unwrap();
    let mut l2 = ledger.clone();
    let again = Session::new(端口(), &restored, &acts)
        .replay(&program, &EntryArgs::default(), &mut l2)
        .unwrap_or_else(|e| panic!("只凭账本重放：{}", e.render()))
        .value_json();
    assert_eq!(again, first);
}

// ---------------------------------------------------------------- split_point

#[test]
fn 切分点_题库是非题记录() {
    let dir = value_support::root().join("bank/entries/1d77f7a203b4258048dfefea/calib");
    let calib = CalibStore::load(&dir).expect("装载");
    // 记录键即模板的键；用题式键的题取不到，这里直接用记录键作题的校准键
    let key = {
        let f = std::fs::read_dir(&dir)
            .unwrap()
            .map(|e| e.unwrap().path())
            .find(|p| p.extension().is_some_and(|x| x == "json"))
            .unwrap();
        let j: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(f).unwrap()).unwrap();
        j["key"].as_str().unwrap().to_string()
    };
    let src = format!(
        "budget {{calls: 0, cost: 0, depth: 16}};\n{{t: split_point(test(\"材料回答了题问的问题吗？\", \"{key}\")), s: split_point(select(\"哪一个？\", \"{key}\")), none: split_point(test(\"别的\", \"nokey\"))}}"
    );
    let v = 跑(&src, &calib);
    let t = &v["t"];
    assert_eq!(t["n"], serde_json::json!(80), "{v}");
    let u = t["u"].as_f64().unwrap();
    let c = t["capacity"].as_f64().unwrap();
    // 平滑后的矩阵（与价值同口径，§10.1 第 4 条）：u* 0.510227、C 0.822744；两行对调会得 0.489773，这条抓得住
    assert!((u - 0.510227).abs() < 1e-6, "u* = {u}");
    assert!((c - 0.822744).abs() < 1e-6, "C = {c}");
    println!("SPLIT_POINT answers_question: u* = {u}, C = {c}");
    assert_eq!(v["s"], serde_json::Value::Null, "{v}");
    assert_eq!(v["none"], serde_json::Value::Null, "{v}");
}

// ---------------------------------------------------------------- 复核后补（过程记录 §10.2）

use value_support::记;

/// 库里只有一条 class 类记录（j = 40，折扣 0.362869）：无记录的 attr 候选没有同题类，取全库最低
#[test]
fn 无同题类取全库最低() {
    let calib = value_support::认证库2(
        &[记 {
            key: "c40",
            j: 40,
            keep: None,
            kind: Some("class"),
            status: None,
        }],
        false,
    );
    let v = 数(&跑(两候选, &calib));
    assert!((v[0] - 半未决).abs() < 1e-6, "{v:?}");
    assert!((v[1] - 半未决).abs() < 1e-6, "{v:?}");
}

/// 库里 attr（j = 0，0.822475）与 class（j = 40，0.362869）都有：无记录的 attr 候选取同题类的，不取全库最低
#[test]
fn 有同题类时不取全库最低() {
    let calib = value_support::认证库2(
        &[
            记 {
                key: "a0",
                j: 0,
                keep: None,
                kind: None,
                status: None,
            },
            记 {
                key: "c40",
                j: 40,
                keep: None,
                kind: Some("class"),
                status: None,
            },
        ],
        false,
    );
    let v = 数(&跑(两候选, &calib));
    assert!((v[0] - 模板值).abs() < 1e-6, "{v:?}");
    assert!((v[1] - 模板值).abs() < 1e-6, "{v:?}");
}

/// 降为夹具的记录不算已认证（复核 B0488-B 缺口 1）：改过样本的记录经 `CalibStore::load` 重跑认证后降为夹具，候选自己的记录
/// 取不到、也不进最低折扣——库里只有它时只按熵；`split_point` 返回 unit
#[test]
fn 降为夹具的记录不算已认证() {
    let calib = value_support::认证库2(
        &[记 {
            key: "g1",
            j: 40,
            keep: None,
            kind: None,
            status: None,
        }],
        true,
    );
    let v = 数(&跑(两候选, &calib));
    assert_eq!(v, vec![1.0, 1.0], "降为夹具的记录不该给出折扣");
    let src = "budget {calls: 0, cost: 0, depth: 16};\nsplit_point(test(\"有作者吗？\", \"g1\"))";
    assert_eq!(跑(src, &calib), serde_json::Value::Null);
}
