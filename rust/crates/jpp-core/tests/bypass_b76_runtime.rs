//! B76 运行时半（步 12e-2）：题类由 `jpp-ir::question_kind` 从题与状态推出，不改任何哈希。
//!
//! 依据：`附注/2026-09-24-评估①裁定.md` §六（B76）；预注册 `地基/过程记录/工程-步12e-2.md`。
//! 子集（`request: all` 运行时拒收）、充分性（无题值材料）、提及（`accepts` 未落地）运行时不可达，
//! 只在 12e-1 的函数层测试（`bypass_b76_kind.rs`）里有。

use std::cell::RefCell;

use jpp_core::effects::{CalibStore, Client, EffectError, GenResult, JudgeResult};
use jpp_core::ledger::Ledger;
use jpp_core::value::{
    Answer, Mat, OnShape, OverKind, OverShape, Question, QuestionKind, State, Value,
};
use jpp_core::{ActionRegistry, TaintOut, run};
use jpp_core::{lower, syntax::parse};
use serde_json::{Value as Json, json};

struct 不判(RefCell<u64>);
impl Client for 不判 {
    fn model_id(&self) -> String {
        "fixed-0".into()
    }
    fn judge(&mut self, _s: &State, qs: &[&Question]) -> Result<JudgeResult, EffectError> {
        *self.0.borrow_mut() += 1;
        Ok(JudgeResult {
            answers: qs.iter().map(|_| Answer::Noul(0.9)).collect(),
            tokens: 0,
            cost: 0.0,
            mode_share: vec![],
            perms: vec![],
        })
    }
    fn generate(
        &mut self,
        _p: &str,
        _c: &[Json],
        _n: usize,
        _r: u64,
    ) -> Result<GenResult, EffectError> {
        Err(EffectError("不该 gen".into()))
    }
    fn ask(&mut self, _s: &State, _q: &Question) -> Result<Option<Answer>, EffectError> {
        Err(EffectError("不该 ask".into()))
    }
    fn calls(&self) -> u64 {
        *self.0.borrow()
    }
}

fn 求值(src: &str) -> Result<Value, String> {
    let program = lower(&parse(src).expect("解析")).expect("lower");
    let mut c = 不判(RefCell::new(0));
    let mut a = ActionRegistry::new();
    a.register("取候选", 0.0, true, TaintOut::Trusted, |_| {
        Ok(Value::text("算出来的候选"))
    });
    let mut l = Ledger::new();
    run(&program, &mut c, &CalibStore::new(), &a, &mut l)
        .map(|o| o.value.expect("有值"))
        .map_err(|e| format!("{e:?}"))
}

fn 题(v: &Value) -> Question {
    match v {
        Value::Question(q) => (**q).clone(),
        x => panic!("该是题：{x:?}"),
    }
}

#[test]
fn 题式声明_actions_经fill到题_题类为决定_哈希不变() {
    let 声明 = 求值(
        r#"budget {calls: 1, cost: 1, depth: 8};
let f = form("select", "对{x}下一步做什么？", {calib: "k", over_kind: "actions"});
fill(f, {x: "这单"})"#,
    )
    .expect("跑得完");
    let 不声明 = 求值(
        r#"budget {calls: 1, cost: 1, depth: 8};
let f = form("select", "对{x}下一步做什么？", {calib: "k"});
fill(f, {x: "这单"})"#,
    )
    .expect("跑得完");
    let (a, b) = (题(&声明), 题(&不声明));
    assert_eq!(a.over_kind, Some(OverKind::Actions));
    assert_eq!(a.kind(), QuestionKind::Decide);
    assert_eq!(b.kind(), QuestionKind::Class, "不声明按缺省：归类");
    assert_eq!(a.hash, b.hash, "题哈希不含声明");
    assert_eq!(a.form_hash, b.form_hash, "题式哈希不含声明");
    assert_eq!(a.to_owned().hash, a.hash);
    assert!(!serde_json::to_string(&b).unwrap().contains("over_kind"));
}

#[test]
fn 字面题的基础类() {
    let v = 求值(
        r#"budget {calls: 1, cost: 1, depth: 8};
[test("是吗？", "k"), select("哪个？", "k"), measure("多少？", ["低", "高"], "k")]"#,
    )
    .expect("跑得完");
    let Value::List(l) = v else { panic!() };
    let kinds: Vec<QuestionKind> = l.iter().map(|x| 题(x).kind()).collect();
    assert_eq!(
        kinds,
        vec![
            QuestionKind::Attr,
            QuestionKind::Class,
            QuestionKind::Degree
        ]
    );
}

#[test]
fn 状态槽形与精化类() {
    let v = 求值(
        r#"budget {calls: 1, cost: 1, depth: 8};
let 候选 = do("取候选", [], 0);
{一: state(mat("对象")), 对: state([mat("甲"), mat("乙")]),
 标签: state(mat("对象"), {over: [mat("A"), mat("B")]}),
 材料: state(mat("对象"), {over: [候选, mat("B")]})}"#,
    )
    .expect("跑得完");
    let 取 = |k: &str| match v.get(k).expect(k) {
        Value::State(s) => (*s).clone(),
        x => panic!("{x:?}"),
    };
    let (一, 对, 标签, 材料) = (取("一"), 取("对"), 取("标签"), 取("材料"));
    assert_eq!(一.slot_shape().on, OnShape::One);
    assert_eq!(一.slot_shape().over, OverShape::Empty);
    assert_eq!(对.slot_shape().on, OnShape::Pair);
    assert_eq!(标签.slot_shape().over, OverShape::Labels);
    assert_eq!(材料.slot_shape().over, OverShape::Materials);

    let t = Question::new(jpp_core::value::Op::Test, "相配吗？", "k", vec![]);
    let s = Question::new(jpp_core::value::Op::Select, "挑一个", "k", vec![]);
    assert_eq!(t.kind(), QuestionKind::Attr, "基础类");
    assert_eq!(
        t.kind_on(&对.slot_shape()),
        QuestionKind::Rel,
        "精化为关系，以精化为准"
    );
    assert_eq!(s.kind_on(&标签.slot_shape()), QuestionKind::Class);
    assert_eq!(s.kind_on(&材料.slot_shape()), QuestionKind::Cmp);
    let _ = Mat::literal(json!(1));
}

#[test]
fn over_kind_非法值报错() {
    let e = 求值(
        r#"budget {calls: 1, cost: 1, depth: 8};
form("select", "哪个{x}？", {calib: "k", over_kind: "options"})"#,
    )
    .expect_err("非法声明");
    assert!(e.contains("E-rt-question"), "{e}");
}
