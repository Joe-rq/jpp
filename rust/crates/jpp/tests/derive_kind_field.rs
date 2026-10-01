//! 步 28 K2：题的只读字段 `kind`——基础题类（B76），名字与报告 questions 表相同（小写英文）。
//! 派生库按题类拆题要读它。

use jpp::effects::{CalibStore, EffectError, FnPort, JudgeResult, Ports};
use jpp::ledger::Ledger;
use jpp::{ActionRegistry, lower, run, syntax::parse};
use serde_json::json;

#[test]
fn 题的_kind_字段按题型与请求给基础题类() {
    let src = r#"
budget {calls: 0, cost: 0, depth: 16};
let f = form("test", "这段话是否提到了{city}？", {calib: "k"});
[test("这段话语气积极吗？", "k").kind,
 select("哪一个最合适？", "k").kind,
 measure("完成度如何？", ["低", "中", "高"], "k").kind,
 fill(f, {city: "北京"}).kind]
"#;
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
    let out = run(
        &program,
        ports,
        &CalibStore::new(),
        &ActionRegistry::new(),
        &mut ledger,
    )
    .unwrap_or_else(|e| panic!("{}", e.render()));
    let v = out.value_json();
    // 是非题缺省属性；K 选一缺省比较；打分即程度（jpp_ir::question_kind 的缺省推断）
    assert_eq!(v[0], json!("attr"), "{v}");
    assert_eq!(v[2], json!("degree"), "{v}");
    assert!(v[1].as_str().is_some_and(|s| !s.is_empty()), "{v}");
    assert_eq!(v[3], v[0], "题式填出的是非题同样有题类");
}
