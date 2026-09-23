//! **模式级键回退查找**（`12`:136）：
//! `calib_key = (q_text_hash, slot_kinds, phys, render_version, literal_mode)`，
//! 「**题级样本不够时用模式级校准做先验收缩**（jev-f6）」。
//!
//! **只做查找那一半。** 收缩估计器（定收缩强度、标定）是研究，**明确不做**——见文末。
//!
//! **为什么它是长处不是栅栏**：现在所有冷键一律 `Unsure(cold)`。这一格填上，
//! **整类样本不足的题跟着受益**。
//!
//! **硬边界：模式级的线不能冒充题级的线。** 用了回退要在出口上留痕，
//! 让 handler 分得出「这道题自己的线」和「这类题的线」——**不留痕就是把两种
//! 证据强度压平**，那正是这几轮一直在拆的东西。

use jpp_core::effects::{CalibStore, LiteralMode};
use jpp_core::ledger::Ledger;
use jpp_core::value::Answer;
use jpp_core::{run, ActionRegistry};
use jpp_frontend::{lower, parse};

mod 桩 {
    use jpp_core::effects::{Client, EffectError, GenResult, JudgeResult};
    use jpp_core::value::{Answer, Question, State};
    use serde_json::Value as Json;
    pub struct 定值(pub f64, pub std::cell::Cell<u64>);
    impl Client for 定值 {
        fn model_id(&self) -> String { "fixed-0".into() }
        fn judge(&mut self, _s: &State, qs: &[&Question]) -> Result<JudgeResult, EffectError> {
            self.1.set(self.1.get() + 1);
            Ok(JudgeResult { answers: qs.iter().map(|_| Answer::Noul(self.0)).collect(), tokens: 0, cost: 0.0, mode_share: vec![], perms: vec![] })
        }
        fn generate(&mut self, _p: &str, _c: &[Json], _n: usize, _r: u64) -> Result<GenResult, EffectError> { Err(EffectError("不该 gen".into())) }
        fn ask(&mut self, _s: &State, _q: &Question) -> Result<Option<Answer>, EffectError> { Err(EffectError("不该 ask".into())) }
        fn calls(&self) -> u64 { self.1.get() }
    }
}

/// handler 两臂都把「线是哪来的」读出来——**它对 handler 可见，不只进 trace**。
const 程序: &str = r#"
budget {calls: 4, cost: 1, depth: 8};
let s = state(mat("对象"));
let e = cut(judge(s, test("行吗", "题级.没测过")));
handle(e, {
  act: fn() { {结果: "act", 线源: line_source(e)} },
  ignore: fn() { {结果: "ignore", 线源: line_source(e)} },
  unsure: fn(u) { consume(u, "drop"); {结果: unsure_cause(u), 线源: line_source(e)} }
})
"#;

fn 跑(calib: &CalibStore, p: f64) -> serde_json::Value {
    let program = lower(&parse(程序).expect("解析")).expect("lower");
    let mut c = 桩::定值(p, std::cell::Cell::new(0));
    let mut ledger = Ledger::new();
    run(&program, &mut c, calib, &ActionRegistry::new(), &mut ledger).expect("跑得完").value_json()
}

/// **B 的红**：题级冷、模式级也没有 → 还是 `Unsure(cold)`。这一条是对照臂，
/// **它证明回退没有把所有冷键都放行**。
#[test]
fn 题级冷且模式级也没有时仍然冷() {
    let v = 跑(&CalibStore::new(), 0.95);
    assert_eq!(v["结果"], "cold", "两级都没线，就该还是冷：{v}");
    assert_eq!(v["线源"], "", "没有线可用，就不该谎称有来源");
}

/// **B 的绿**：题级冷、**模式级有线** → 拿到线，且出口带着「用的是模式级」的痕迹。
#[test]
fn 题级冷时回退到模式级并留痕() {
    let mut calib = CalibStore::new();
    // 这一类题（noul + 默认字面模式）的线：在 200 条标注上定过
    calib.put(&CalibStore::mode_key("noul", LiteralMode::default()), 0.80, 0.20, 200, "上岗").expect("写得进");

    let v = 跑(&calib, 0.95);
    assert_eq!(v["结果"], "act", "0.95 过了模式级的 hi=0.80：{v}");
    assert_eq!(v["线源"], "模式级·手填", "**模式级的线不能冒充题级的线**");

    // 同一条线，p 落在带内 → 仍是 unsure，但来源照样留痕
    let v = 跑(&calib, 0.5);
    assert_eq!(v["结果"], "band");
    assert_eq!(v["线源"], "模式级·手填");
}

/// **题级有线时不回退**：自己的线优先，来源是题级。
#[test]
fn 题级有线时用自己的() {
    let mut calib = CalibStore::new();
    calib.put("题级.没测过", 0.90, 0.10, 50, "上岗").expect("写得进");
    calib.put(&CalibStore::mode_key("noul", LiteralMode::default()), 0.30, 0.20, 200, "上岗").expect("写得进");
    // 0.5 在题级线（0.90/0.10）的带内；若错用了模式级线（0.30）就会变成 act
    let v = 跑(&calib, 0.5);
    assert_eq!(v["结果"], "band", "该用题级的线：{v}");
    assert_eq!(v["线源"], "题级·手填", "**层级与凭据是两件正交的事，都要说**");
}

/// **硬边界：`停岗` 不得被模式级的线救回来。**
/// 停岗是人下的判断（这条线不能再用了），**回退到一个类级先验把它放行，是彻头彻尾的假放行**。
#[test]
fn 停岗不被模式级救回() {
    let mut calib = CalibStore::new();
    calib.put("题级.没测过", 0.90, 0.10, 50, "停岗").expect("写得进");
    calib.put(&CalibStore::mode_key("noul", LiteralMode::default()), 0.80, 0.20, 200, "上岗").expect("写得进");
    let v = 跑(&calib, 0.95);
    assert_ne!(v["结果"], "act", "停岗的题不该因为同类有线就放行：{v}");
    assert_eq!(v["线源"], "", "停岗时不回退，也就没有来源可留");
}

/// **模式级的冷记录不得注入假线**：`get` 命不中时合成的记录带着 `0.65/0.35`，
/// 直接读 `rec.hi`/`rec.lo` 就会把它当成一条线。必须走 `lines_for`（只认上岗）。
#[test]
fn 模式级的冷记录不算线() {
    let mut calib = CalibStore::new();
    // 模式级那一格存在但不是上岗
    calib.put(&CalibStore::mode_key("noul", LiteralMode::default()), 0.80, 0.20, 5, "待真值").expect("写得进");
    let v = 跑(&calib, 0.95);
    assert_eq!(v["结果"], "cold", "模式级没上岗 = 没有线可借：{v}");
    assert_eq!(v["线源"], "");
}

/// 模式键按**物理形式**分格：`12` 自己的 `delta_for` 就按 `op.phys()` 分，
/// 一把尺子上的线不能给另一把尺子用（本项目「不同尺不可比」的判据）。
#[test]
fn 模式键按物理形式与字面模式分格() {
    let a = CalibStore::mode_key("noul", LiteralMode::default());
    let b = CalibStore::mode_key("choice", LiteralMode::default());
    let c = CalibStore::mode_key("noul", LiteralMode::CodeLiteral);
    assert_ne!(a, b, "noul 与 choice 不同尺");
    assert_ne!(a, c, "字面模式是键的一维（12:136 第五维）");
    // 模式键落在保留命名空间里，不会与任何题级键名相撞
    assert!(a.starts_with('\u{1f}'), "{a}");
}

/// 为 `Answer` 留一个编译期用到的引用，免得 import 被判无用。
#[allow(dead_code)]
fn _用到answer(a: &Answer) -> bool {
    matches!(a, Answer::Noul(_))
}

/// **回归：`待真值` 的记录不得供线。**
///
/// A 部分的 `absorb` 会把冷记录推到 `待真值`。而 `get` 命不中时合成的记录带着
/// `0.65/0.35` 这个**缺省值**——若 `待真值` 照样走过线比较，那么「程序积累了几条
/// 无标注观察」就会**静默地**把一个原本 `Unsure(cold)` 的键变成 0.65 就放行。
/// **积累证据不该改变判定**，这是 A 与 B 交界处的坑，两包合起来才看得见。
#[test]
fn 待真值不供线() {
    use jpp_core::effects::Sample;
    let mut calib = CalibStore::new();
    calib.absorb("题级.没测过", Sample { p: Some(0.9), label: None, perms: 0, mode_share: None, mode: LiteralMode::default(), phys: "noul".into(), cluster: None }).expect("折得进");
    assert_eq!(calib.get("题级.没测过").status, "待真值");

    let v = 跑(&calib, 0.95);
    assert_eq!(v["结果"], "cold", "**积累无标注证据不得把冷键变成会放行的键**：{v}");
    assert_eq!(v["线源"], "");
}
