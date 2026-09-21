//! **漂移监控接线**（`12`:396「漂移监控（无标签：读数分布偏移 + 保形覆盖跌落**告警**）」）。
//!
//! `conformal::drift` 有实现、**零调用点**——前两轮顾问各点一次。
//!
//! **三问先答**：
//! 1. **谁来调**：内核在 `cut` 那一步算，**因为数据已经在记录里**——
//!    标注样本是参照分布，运行期积累的无标注观察是近期分布。**不需要新的数据源。**
//! 2. **`underpowered` 怎么用**：**照报，但不构成停岗的依据**。
//! 3. **停岗之后怎么复岗**：**这一问不成立，因为漂移不停岗**——
//!    `12`:396 写的是**告警**。停岗仍然是人下的判断（走 `put`）。
//!    **而这一问必须先答，正是因为我今晚在它上面栽过一次**（把「拦住」改成「停岗」造出永久锁）。

use jpp_core::effects::{CalibStore, LiteralMode, Sample};

fn 观察(c: &mut CalibStore, key: &str, ps: &[f64], label: Option<u8>) {
    for p in ps {
        c.absorb(key, Sample {
            p: Some(*p), label, perms: 0, mode_share: None,
            mode: LiteralMode::default(), phys: "noul".into(), cluster: None,
        }).unwrap();
    }
}

/// 参照分布 = 带标注的那些；近期分布 = 运行期积累的无标注观察。**数据已经在记录里。**
#[test]
fn 参照与近期分别取标注与无标注() {
    let mut c = CalibStore::new();
    let 标注: Vec<f64> = (0..40).map(|i| 0.10 + i as f64 * 0.02).collect();
    观察(&mut c, "k", &标注, Some(1));
    // 近期读数整体右移
    let 近期: Vec<f64> = (0..40).map(|i| 0.50 + i as f64 * 0.012).collect();
    观察(&mut c, "k", &近期, None);

    let d = c.drift_of("k").expect("两侧都有样本就算得出");
    println!("ks={:.3} psi={:.3} n_ref={} n_recent={} underpowered={}", d.ks, d.psi, d.n_ref, d.n_recent, d.underpowered);
    assert_eq!((d.n_ref, d.n_recent), (40, 40));
    assert!(d.ks > 0.3, "整体右移，KS 该明显：{}", d.ks);
    assert!(!d.underpowered, "两边各 40 条，不算功效不足");
}

/// **一侧为空就算不出**——不是「漂移为 0」。与 `binomial_upper` 的 `n == 0 → 1.0` 同一条：
/// **算不出来不是一个值。**
#[test]
fn 一侧没有样本时算不出而不是零() {
    let mut c = CalibStore::new();
    观察(&mut c, "k", &[0.3, 0.4, 0.5], Some(1));
    assert!(c.drift_of("k").is_none(), "只有参照没有近期 → 算不出");
    let mut d = CalibStore::new();
    观察(&mut d, "k", &[0.3, 0.4, 0.5], None);
    assert!(d.drift_of("k").is_none(), "只有近期没有参照 → 算不出");
    assert!(CalibStore::new().drift_of("没这键").is_none());
}

/// **`underpowered` 照报，但不构成停岗的依据。**
/// 当初加这一位的理由是「一次 20 条的抽样不该把一个键停岗」；
/// **而更硬的理由是今晚长出来的：复岗今天不存在，所以一次假停岗是永久的。**
#[test]
fn 功效不足时照报但不算依据() {
    let mut c = CalibStore::new();
    观察(&mut c, "k", &[0.20, 0.25, 0.30], Some(1));
    观察(&mut c, "k", &[0.70, 0.75, 0.80], None);
    let d = c.drift_of("k").expect("算得出");
    assert!(d.underpowered, "各 3 条，KS 再大也分不出移没移：ks={}", d.ks);
    assert!(!d.可停岗(), "**功效不足时它不构成停岗的依据**");

    // 样本够了、且真的移了，才算得上依据
    let mut e = CalibStore::new();
    观察(&mut e, "k", &(0..60).map(|i| 0.05 + i as f64 * 0.008).collect::<Vec<_>>(), Some(1));
    观察(&mut e, "k", &(0..60).map(|i| 0.60 + i as f64 * 0.006).collect::<Vec<_>>(), None);
    let f = e.drift_of("k").expect("算得出");
    assert!(!f.underpowered);
    assert!(f.可停岗(), "样本够、移得明显：ks={}", f.ks);
}

/// **漂移不停岗，只告警**（`12`:396）。**这条是「复岗不存在」那个坑的正面防线**：
/// 一个只报不动的机制，造不出永久锁。
#[test]
fn 漂移不改记录状态() {
    let mut c = CalibStore::new();
    // **这里必须走 commission 而不是 put**：证书门拦的正是「带标注的证据靠手写的线上岗」。
    // 第一版我写 put，被自己那道门当场拦住——**把这一包和上一包合起来看，第 4 问当场抓到一次。**
    观察(&mut c, "k", &(0..60).map(|i| if i % 7 == 0 { 0.9 } else { 0.05 + i as f64 * 0.008 }).collect::<Vec<_>>(), Some(1));
    c.commission("k", 0.60, 0.10, "条").expect("认得动");
    观察(&mut c, "k", &(0..60).map(|i| 0.60 + i as f64 * 0.006).collect::<Vec<_>>(), None);
    let d = c.drift_of("k").expect("算得出");
    assert!(d.可停岗(), "这批数据够得上依据");
    assert_eq!(c.get("k").status, "上岗", "**但它自己不动状态**——停岗仍是人下的判断（走 put）");
}

/// **`cut` 是消费方**——否则 `drift_of` 就成了第二个「有实现没调用点」的东西，
/// **而那正是这一包要修的毛病**。
#[test]
fn cut那一步会因漂移告警() {
    use std::cell::RefCell;
    use jpp_core::effects::{Client, EffectError, GenResult, JudgeResult};
    use jpp_core::interp::ActionRegistry;
    use jpp_core::ledger::Ledger;
    use jpp_core::value::{Answer, Question, State};
    use jpp_core::run;
    struct 桩(RefCell<u64>);
    impl Client for 桩 {
        fn model_id(&self) -> String { "m".into() }
        fn judge(&mut self, _s: &State, qs: &[&Question]) -> Result<JudgeResult, EffectError> {
            *self.0.borrow_mut() += 1;
            Ok(JudgeResult { answers: qs.iter().map(|_| Answer::Noul(0.9)).collect(), tokens: 0, cost: 0.0, mode_share: vec![], perms: vec![] })
        }
        fn generate(&mut self, _p: &str, _c: &[serde_json::Value], _n: usize, _r: u64) -> Result<GenResult, EffectError> { Err(EffectError("x".into())) }
        fn ask(&mut self, _s: &State, _q: &Question) -> Result<Option<Answer>, EffectError> { Err(EffectError("x".into())) }
        fn calls(&self) -> u64 { *self.0.borrow() }
    }
    let 跑 = |c: &CalibStore| {
        let program = jpp_frontend::lower(&jpp_frontend::parse(r#"
budget {calls: 4, cost: 1};
handle(cut(judge(state(mat("材料")), test("行吗", "k"))), {
    act: fn() { "act" }, ignore: fn() { "ig" },
    unsure: fn(u) { consume(u, "drop"); "un" }})
"#).expect("解析")).expect("lower");
        let mut l = Ledger::new();
        run(&program, &mut 桩(RefCell::new(0)), c, &ActionRegistry::new(), &mut l).expect("跑得完").trace.warnings.clone()
    };

    // 漂了
    let mut 漂 = CalibStore::new();
    观察(&mut 漂, "k", &(0..60).map(|i| if i % 7 == 0 { 0.9 } else { 0.05 + i as f64 * 0.008 }).collect::<Vec<_>>(), Some(1));
    漂.commission("k", 0.60, 0.10, "条").expect("认得动");
    观察(&mut 漂, "k", &(0..60).map(|i| 0.60 + i as f64 * 0.006).collect::<Vec<_>>(), None);
    let w = 跑(&漂);
    assert!(w.iter().any(|x| x.starts_with("W-drift")), "**漂了要告警**：{w:?}");

    // 没漂：同一个分布 → 不告警（**一条天天响的告警等于没有告警**）
    let mut 没漂 = CalibStore::new();
    let ps: Vec<f64> = (0..60).map(|i| if i % 7 == 0 { 0.9 } else { 0.05 + i as f64 * 0.008 }).collect();
    观察(&mut 没漂, "k", &ps, Some(1));
    没漂.commission("k", 0.60, 0.10, "条").expect("认得动");
    观察(&mut 没漂, "k", &ps, None);
    let w2 = 跑(&没漂);
    assert!(!w2.iter().any(|x| x.starts_with("W-drift")), "没漂不该响：{w2:?}");
}
