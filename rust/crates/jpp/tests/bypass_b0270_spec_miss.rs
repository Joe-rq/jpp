//! 步 13a-1（B0270）：只含推测登记的组发出后端口报错——放弃这一组、报 `W-spec-fixture-miss`、程序照常；
//! 真站点未命中仍是错误；审计重放按账本 `spec_miss` 记录跳过；续跑时真站点不误用推测的放弃。
//!
//! 形状同 `地基/比赛/改写套件/实测/if-branch-speculation/repro.jpp`。依据：黑板 B0270；预注册
//! `地基/过程记录/工程-步13a-1.md` §一·2。

use std::cell::RefCell;

use jpp::effects::{CalibStore, EffectError, FnPort, JudgeResult, Ports};
use jpp::interp::{ActionRegistry, Passes};
use jpp::ledger::{Entry, Ledger};
use jpp::value::{Answer, Question, State};
use jpp::{lower, syntax::parse};

/// 两个元素，b 的 cond 为假：q2 只该问 a；推测会把 b 的 q2 一起登记。cond 用 0/1，改值不改源码长度
fn 程序(b_cond: u8) -> String {
    format!(
        r#"budget {{calls: 10, cost: 1.0, depth: 64}};
let items = [{{id: "a", cond: 1}}, {{id: "b", cond: {b_cond}}}];
let q1 = test("q1 on the item?", "c1");
let q2 = test("q2 on the item, only relevant when cond is true", "c2");
fn one(it) !{{judge}} {{
    let r1 = judge(state(mat(it.id)), q1);
    let r2 = if it.cond == 1 {{ judge(state(mat(it.id)), q2) }} else {{ unit }};
    {{id: it.id, r1: r1, r2: r2}}
}}
map(items, fn(it) !{{judge}} {{ one(it) }})
"#
    )
}

#[derive(Clone, Copy)]
enum 坏法 {
    无,
    未命中,
    超时,
    题数不符,
    越界,
}

struct 结果 {
    r: Result<serde_json::Value, String>,
    告警: Vec<String>,
    /// 端口收到 (b, q2) 的次数
    问b2: usize,
}

fn 端口<'a>(坏: 坏法, 问b2: &'a RefCell<usize>) -> Ports<'a> {
    Ports::new().with(FnPort::judge("m", move |s: &State, qs: &[&Question]| {
        let b = s.on_text().contains('b');
        let q2 = qs.iter().any(|q| q.text.starts_with("q2"));
        if b && q2 {
            *问b2.borrow_mut() += 1;
            match 坏 {
                坏法::未命中 => {
                    return Err(EffectError(
                        "固定观察未命中：题「q2…」× 状态 b（测试端口）".into(),
                    ));
                }
                坏法::超时 => return Err(EffectError("判断器超时（测试端口）".into())),
                坏法::题数不符 => {
                    return Ok(JudgeResult {
                        answers: vec![],
                        tokens: 0,
                        cost: 0.0,
                        mode_share: vec![],
                        perms: vec![],
                        confidence: vec![],
                    });
                }
                坏法::越界 => {
                    return Ok(JudgeResult {
                        answers: qs.iter().map(|_| Answer::Noul(1.5)).collect(),
                        tokens: 0,
                        cost: 0.0,
                        mode_share: vec![None; qs.len()],
                        perms: vec![0; qs.len()],
                        confidence: vec![],
                    });
                }
                坏法::无 => {}
            }
        }
        Ok(JudgeResult {
            answers: qs.iter().map(|_| Answer::Noul(0.9)).collect(),
            tokens: 0,
            cost: 0.0,
            mode_share: vec![None; qs.len()],
            perms: vec![0; qs.len()],
            confidence: vec![],
        })
    }))
}

fn 跑(src: &str, 坏: 坏法, ledger: &mut Ledger, 审计: bool) -> 结果 {
    let program = lower(&parse(src).unwrap_or_else(|e| panic!("解析：{e:?}"))).expect("lower");
    let calib = CalibStore::new();
    let actions = ActionRegistry::new();
    let 问b2 = RefCell::new(0usize);
    let mut it = jpp::interp::Interp::new(
        端口(坏, &问b2),
        ledger,
        &calib,
        &actions,
        program.budget.clone(),
    );
    if 审计 {
        it = it.audit_replay();
    }
    it.passes = Passes::default();
    let (r, 告警) = match it.run(&program) {
        Ok(o) => (Ok(o.value_json()), o.trace.warnings.clone()),
        Err(e) => (Err(e.render()), vec![]),
    };
    let n = *问b2.borrow();
    结果 {
        r, 告警, 问b2: n
    }
}

fn 有(告警: &[String], 前缀: &str) -> bool {
    告警.iter().any(|w| w.starts_with(前缀))
}

fn spec_miss条目(ledger: &Ledger) -> usize {
    ledger
        .entries
        .iter()
        .filter(|e| matches!(e, Entry::Absent { cause, .. } if cause == "spec_miss"))
        .count()
}

#[test]
fn a_固定观察未命中_放弃推测组程序照常() {
    let mut ledger = Ledger::new();
    let x = 跑(&程序(0), 坏法::未命中, &mut ledger, false);
    let v = x.r.as_ref().unwrap_or_else(|e| panic!("{e}"));
    assert!(v[1]["r2"].is_null(), "b 的 r2 为 unit：{v}");
    assert!(!v[0]["r2"].is_null(), "a 的 r2 有值：{v}");
    assert!(
        x.告警
            .iter()
            .any(|w| w.starts_with("W-spec-fixture-miss") && w.contains("fixture-miss")),
        "{:?}",
        x.告警
    );
    assert!(有(&x.告警, "W-spec-unused"), "{:?}", x.告警);
    assert_eq!(spec_miss条目(&ledger), 1);
}

#[test]
fn b_其他端口错误_不重试() {
    // 声明了缺席策略（重试 2 次）也不重试：推测组本来就可放弃
    let src = 程序(0).replace(
        "depth: 64}",
        r#"depth: 64, absent: {retry: 2, backoff: 0, then: "conservative"}}"#,
    );
    let mut ledger = Ledger::new();
    let x = 跑(&src, 坏法::超时, &mut ledger, false);
    assert!(x.r.is_ok(), "{:?}", x.r);
    assert!(
        x.告警
            .iter()
            .any(|w| w.starts_with("W-spec-fixture-miss") && w.contains("port-error")),
        "{:?}",
        x.告警
    );
    assert_eq!(x.问b2, 1, "推测组不重试");
    assert!(!有(&x.告警, "W-absent"), "不走缺席策略：{:?}", x.告警);
}

#[test]
fn c_回复题数不符_按坏回复放弃() {
    let mut ledger = Ledger::new();
    let x = 跑(&程序(0), 坏法::题数不符, &mut ledger, false);
    assert!(x.r.is_ok(), "{:?}", x.r);
    assert!(
        x.告警
            .iter()
            .any(|w| w.starts_with("W-spec-fixture-miss") && w.contains("bad-reply")),
        "{:?}",
        x.告警
    );
}

#[test]
fn d_真站点未命中仍是错误() {
    let mut ledger = Ledger::new();
    let x = 跑(&程序(1), 坏法::未命中, &mut ledger, false);
    let e = x.r.unwrap_err();
    assert!(
        e.contains("E-rt-client") && e.contains("固定观察未命中"),
        "{e}"
    );
}

#[test]
fn e_审计重放_跳过被放弃的推测组() {
    let mut ledger = Ledger::new();
    let first = 跑(&程序(0), 坏法::未命中, &mut ledger, false);
    let again = 跑(&程序(0), 坏法::未命中, &mut ledger, true);
    assert_eq!(again.r, first.r);
    assert_eq!(again.问b2, 0, "重放不发");
    let 码 = |w: &[String]| {
        let mut v: Vec<String> = w
            .iter()
            .filter_map(|x| x.split(':').next().map(str::to_string))
            .filter(|c| c.starts_with("W-"))
            .collect();
        v.sort();
        v.dedup();
        v
    };
    assert_eq!(码(&again.告警), 码(&first.告警));
}

#[test]
fn f_续跑时真站点不误用推测的放弃() {
    // 首跑：b 的 cond 为 0，(b, q2) 只被推测、被放弃，账本记 spec_miss。续跑同一份账本、b 的 cond 改为 1：
    // 真站点走到同一个键，照常发问得到答案，而不是 Unsure(spec_miss)
    let mut ledger = Ledger::new();
    let _ = 跑(&程序(0), 坏法::未命中, &mut ledger, false);
    assert_eq!(spec_miss条目(&ledger), 1);
    let x = 跑(&程序(1), 坏法::无, &mut ledger, false);
    let v = x.r.as_ref().unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(x.问b2, 1, "真站点照常发问");
    assert!(!v[1]["r2"].is_null(), "b 的 r2 有值：{v}");
    assert!(
        !serde_json::to_string(v).unwrap().contains("spec_miss"),
        "{v}"
    );
}

#[test]
fn g_答案形状不合法_推测组放弃_真站点仍报错() {
    // 预注册订正（主会话答复问题 1）：推测组回复里的读数越界（p = 1.5），放弃这一组，种类 malformed；
    // 真站点走到同一个键时同样的错误由真站点正式报出
    let mut ledger = Ledger::new();
    let x = 跑(&程序(0), 坏法::越界, &mut ledger, false);
    assert!(x.r.is_ok(), "{:?}", x.r);
    assert!(
        x.告警
            .iter()
            .any(|w| w.starts_with("W-spec-fixture-miss") && w.contains("malformed")),
        "{:?}",
        x.告警
    );
    let mut ledger = Ledger::new();
    let y = 跑(&程序(1), 坏法::越界, &mut ledger, false);
    assert!(y.r.is_err(), "真站点仍报错：{:?}", y.r);
}
