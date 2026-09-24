//! K 元出口的重放一致性（修复 folio 重放，2026-09-24）。
//!
//! 缺陷：`select` 读数的置换测量（`perms`, `mode_share`）没有进账本，账本命中只填答案。
//! 于是同一条读数在首发处出 `pick` / `band` / `tie`，在账本命中处（只凭账本重放、续跑、
//! 同键去重的其余读数、同站点再问一次）一律变成 `untested:permutation`（J-15）。
//!
//! 本文件覆盖每一种 K 元出口：select 的 pick、tie、band、untested:permutation、cold、drift、
//! no_candidate；measure 的 at、band、cold。每种都比首跑与「编码—解码后的账本」重放：值逐项相同、
//! 重放 0 调用。另有两条首跑内的路径：同键去重（`map` 在同一站点登记同一道题两次）与
//! 同站点再问一次（函数调用两次，第二次命中账本）。缺席与超时的重放见 `b32_absent_latency.rs`。
//! 依据：`12`:151、J-15；`jpp-core/INTERFACE.md` §四·二·七·五；B63；过程记录 `工程-修复-folio重放.md`。

mod common;
use std::cell::RefCell;

use jpp_core::effects::{CalibStore, Client, EffectError, GenResult, JudgeResult};
use jpp_core::interp::ActionRegistry;
use jpp_core::ledger::{Entry, Ledger};
use jpp_core::value::{Answer, Question, State};
use jpp_core::{run, run_replay};
use jpp_core::{lower, syntax::parse};
use serde_json::{Value as Json, json};

/// 按题面里的标记答题：`[pick]` 高 p、众数一致；`[tie]` 高 p、众数不一致；`[band]` 低 p、一致；
/// `[raw]` 高 p、不测置换；measure 同理（measure 不用置换）。
struct 桩 {
    calls: RefCell<u64>,
    /// 重放时发出任何调用都是缺陷
    禁发: bool,
}
impl Client for 桩 {
    fn model_id(&self) -> String {
        "m".into()
    }
    fn judge(&mut self, s: &State, qs: &[&Question]) -> Result<JudgeResult, EffectError> {
        assert!(!self.禁发, "重放发出了调用");
        *self.calls.borrow_mut() += 1;
        let mut answers = vec![];
        let mut mode_share = vec![];
        let mut perms = vec![];
        for q in qs {
            let k = s.over.len().max(2);
            let 分布 = |p: f64| {
                let mut v = vec![(1.0 - p) / (k - 1) as f64; k];
                v[0] = p;
                v
            };
            let (p, ms) = if q.text.contains("[pick]") || q.text.contains("[at]") {
                (0.99, Some(1.0))
            } else if q.text.contains("[tie]") {
                (0.99, Some(0.5))
            } else if q.text.contains("[band]") {
                (0.62, Some(1.0))
            } else {
                (0.99, None)
            };
            if q.text.contains("[at]") || q.text.contains("[mband]") || q.text.contains("[mcold]") {
                let p = if q.text.contains("[mband]") { 0.62 } else { p };
                answers.push(Answer::Score(vec![1.0 - p, p]));
                mode_share.push(None);
                perms.push(0);
            } else {
                answers.push(Answer::Choice(分布(p)));
                mode_share.push(ms);
                perms.push(if ms.is_some() { 2 } else { 0 });
            }
        }
        Ok(JudgeResult {
            answers,
            tokens: 0,
            cost: 0.0,
            mode_share,
            perms,
        })
    }
    fn generate(
        &mut self,
        _p: &str,
        _c: &[Json],
        _n: usize,
        _r: u64,
    ) -> Result<GenResult, EffectError> {
        Err(EffectError("x".into()))
    }
    fn ask(&mut self, _s: &State, _q: &Question) -> Result<Option<Answer>, EffectError> {
        Err(EffectError("x".into()))
    }
    fn calls(&self) -> u64 {
        *self.calls.borrow()
    }
}

fn 库() -> CalibStore {
    let mut c = CalibStore::new();
    for k in ["k-sel", "k-mea", "k-drift"] {
        common::certified(&mut c, k, 0.6, 0.0, 50);
    }
    c.records.get_mut("k-drift").unwrap().status = "停岗".into();
    c
}

const 程序: &str = r#"
budget {calls: 40, cost: 1, depth: 64};
let d = mat("一份租赁合同：甲方出租住房给乙方，月租四千五。");
let opts = ["租赁", "买卖", "劳动"];
fn sel(text, key, over) {
    handle(cut(judge(state(d, {over: over}), select(text, key))), {
        pick: fn(k) { "pick" },
        unsure: fn(u) { let c = unsure_cause(u); consume(u, "drop"); c }
    })
}
fn mea(text, key) {
    handle(cut(judge(state(d), measure(text, ["低", "高"], key))), {
        at: fn(l) { "at" },
        unsure: fn(u) { let c = unsure_cause(u); consume(u, "drop"); c }
    })
}
{pick: sel("[pick] 归哪类", "k-sel", opts),
 tie: sel("[tie] 归哪类", "k-sel", opts),
 band: sel("[band] 归哪类", "k-sel", opts),
 untested: sel("[raw] 归哪类", "k-sel", opts),
 cold: sel("[pick] 归哪类（无线）", "k-none", opts),
 drift: sel("[pick] 归哪类（停岗）", "k-drift", opts),
 no_candidate: sel("[pick] 归哪类（无候选）", "k-sel", []),
 at: mea("[at] 有多像", "k-mea"),
 mband: mea("[mband] 有多像", "k-mea"),
 mcold: mea("[mcold] 有多像", "k-none")}
"#;

fn 首跑与重放(src: &str) -> (Json, Json, Vec<String>, Vec<String>, u64, Ledger) {
    let program = lower(&parse(src).expect("解析")).expect("lower");
    let calib = 库();
    let mut l = Ledger::new();
    let mut c1 = 桩 {
        calls: RefCell::new(0),
        禁发: false,
    };
    let o1 = run(&program, &mut c1, &calib, &ActionRegistry::new(), &mut l)
        .map_err(|e| e.render())
        .unwrap();
    // 经落盘形态走一遍：置换测量必须活过编码与解码
    let (mut l2, trunc) = Ledger::decode(&l.encode()).expect("解码");
    assert!(trunc.is_none());
    l2.rebuild_index();
    let mut c2 = 桩 {
        calls: RefCell::new(0),
        禁发: true,
    };
    let o2 = run_replay(&program, &mut c2, &calib, &ActionRegistry::new(), &mut l2)
        .map_err(|e| e.render())
        .unwrap();
    (
        o1.value_json(),
        o2.value_json(),
        o1.returned_unsure,
        o2.returned_unsure,
        o2.cost.calls,
        l,
    )
}

#[test]
fn 每种k元出口重放与首跑相同() {
    let (v1, v2, u1, u2, calls, _) = 首跑与重放(程序);
    // 首跑确实走到了每一种出口，否则下面的相等是空的
    assert_eq!(
        v1,
        json!({"pick": "pick", "tie": "tie", "band": "band", "untested": "untested", "cold": "cold",
               "drift": "drift", "no_candidate": "no_candidate", "at": "at", "mband": "band", "mcold": "cold"}),
    );
    assert_eq!(v2, v1, "重放的出口与首跑不同");
    assert_eq!(u2, u1);
    assert_eq!(calls, 0, "重放新增了调用");
}

/// 置换测量写进账本，成对（`perms` 与 `mode_share`）；没测的读数不写这个字段。
#[test]
fn 置换测量成对进账本_未测不写() {
    let (_, _, _, _, _, l) = 首跑与重放(程序);
    let mut 测了 = 0;
    let mut 没测 = 0;
    for e in &l.entries {
        if let Entry::Judge {
            answer: Answer::Choice(_),
            perm,
            ..
        } = e
        {
            match perm {
                Some(pm) => {
                    assert_eq!(pm.perms, 2);
                    测了 += 1;
                }
                None => 没测 += 1,
            }
        }
    }
    // pick、tie、band、drift、cold 测了置换；[raw] 没测
    assert_eq!((测了, 没测), (5, 1));
    let text = l.encode();
    assert_eq!(
        text.matches("\"perm\"").count(),
        5,
        "没测的读数不该序列化 perm 字段"
    );
}

/// 首跑内：同一站点、同一状态登记同一道题两次，去重后只问一次；两条读数的出口相同。
#[test]
fn 同键去重的读数也带置换测量() {
    let src = r#"
budget {calls: 8, cost: 1, depth: 64};
let d = mat("一份租赁合同");
let q = select("[pick] 归哪类", "k-sel");
let rs = map([1, 2], fn(i) { judge(state(d, {over: ["租赁", "买卖"]}), q) });
map(rs, fn(r) { handle(cut(r), {
    pick: fn(k) { "pick" },
    unsure: fn(u) { let c = unsure_cause(u); consume(u, "drop"); c } }) })
"#;
    let (v1, v2, _, _, calls, l) = 首跑与重放(src);
    let judges = l
        .entries
        .iter()
        .filter(|e| matches!(e, Entry::Judge { .. }))
        .count();
    assert_eq!(judges, 1, "同键应只问一次");
    assert_eq!(v1, json!(["pick", "pick"]), "去重的第二条读数丢了置换测量");
    assert_eq!(v2, v1);
    assert_eq!(calls, 0);
}

/// 首跑内：同一站点再问一次（函数调用两次），第二次命中账本；出口与第一次相同。
/// tie 也要在命中处保持 tie，不能退成 untested。
#[test]
fn 同站点再问命中账本时出口不变() {
    let src = r#"
budget {calls: 8, cost: 1, depth: 64};
let d = mat("一份租赁合同");
fn once(t) {
    handle(cut(judge(state(d, {over: ["租赁", "买卖"]}), select(t, "k-sel"))), {
        pick: fn(k) { "pick" },
        unsure: fn(u) { let c = unsure_cause(u); consume(u, "drop"); c } })
}
[once("[pick] 归哪类"), once("[pick] 归哪类"), once("[tie] 归哪类"), once("[tie] 归哪类")]
"#;
    let (v1, v2, _, _, calls, l) = 首跑与重放(src);
    let judges = l
        .entries
        .iter()
        .filter(|e| matches!(e, Entry::Judge { .. }))
        .count();
    assert_eq!(judges, 2, "第二次应命中账本");
    assert_eq!(v1, json!(["pick", "pick", "tie", "tie"]));
    assert_eq!(v2, v1);
    assert_eq!(calls, 0);
}

/// 旧账本（没有 `perm` 字段）照读：字段缺省 = 没测过，按 J-15 失败关闭为 untested，不报错。
#[test]
fn 旧账本没有置换字段照读() {
    let (_, _, _, _, _, l) = 首跑与重放(程序);
    let mut old = l.clone();
    for e in old.entries.iter_mut() {
        if let Entry::Judge { perm, .. } = e {
            *perm = None;
        }
    }
    let (mut l2, _) = Ledger::decode(&old.encode()).expect("旧形态可解码");
    l2.rebuild_index();
    let program = lower(&parse(程序).unwrap()).unwrap();
    let mut c = 桩 {
        calls: RefCell::new(0),
        禁发: true,
    };
    let o = run_replay(&program, &mut c, &库(), &ActionRegistry::new(), &mut l2)
        .map_err(|e| e.render())
        .unwrap();
    let v = o.value_json();
    assert_eq!(v["pick"], "untested");
    assert_eq!(v["tie"], "untested");
    assert_eq!(v["at"], "at", "measure 不用置换，不受影响");
}

/// 首发的运行期证据（`--calib-out` 折进校准记录的那份）带着置换测量：
/// 测量要先落到读数上，再生成证据。没测的读数证据里是 `perms = 0`、`mode_share = None`。
#[test]
fn 首发证据带置换测量() {
    let program = lower(&parse(程序).unwrap()).unwrap();
    let mut l = Ledger::new();
    let mut c = 桩 {
        calls: RefCell::new(0),
        禁发: false,
    };
    let o = run(&program, &mut c, &库(), &ActionRegistry::new(), &mut l)
        .map_err(|e| e.render())
        .unwrap();
    let mut got: Vec<(usize, Option<f64>)> = o
        .evidence
        .iter()
        .filter(|(k, _)| k == "k-sel")
        .map(|(_, s)| (s.perms, s.mode_share))
        .collect();
    got.sort_by(|a, b| a.partial_cmp(b).unwrap());
    // [raw] 没测；pick、tie、band 测了（no_candidate 不发，没有证据）
    assert_eq!(
        got,
        vec![(0, None), (2, Some(0.5)), (2, Some(1.0)), (2, Some(1.0))]
    );
}
