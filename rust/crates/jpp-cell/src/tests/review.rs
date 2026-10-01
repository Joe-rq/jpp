//! 复核后修补（主控板 Z0509，复核 `地基/规划/主控核查/复核-C1-单元图.md`）的回归测试：
//! 复核附录的 E1–E5 入库，另加 R-4、R-5（缺席未问过期）、R-9（立刻读的承接）、R-10（`Union`）。
//! 预注册见过程记录「复核后修补」节。

use super::z0419::{concl_summary, take_x};
use super::*;

fn flushes_of(g: &CellGraph<TV>, key: &str) -> Vec<String> {
    g.flushes
        .iter()
        .flat_map(|f| {
            f.questions
                .iter()
                .filter(|q| q.0 == key)
                .map(|q| q.2.clone())
        })
        .collect()
}

fn exit_cause(v: &TV) -> Option<UnsureCause> {
    match v {
        TV::Exit(TExit::Unsure(c, _)) => Some(*c),
        _ => None,
    }
}

/// 前 `n` 次缺席，之后恒答 0.5。
fn absent_first(n: u32) -> impl FnMut(&str, &[String]) -> FlushAnswer<TV> {
    let mut i = 0;
    move |_k, _r| {
        i += 1;
        if i <= n {
            FlushAnswer::Absent
        } else {
            FlushAnswer::Answer(TV::Reading(0.5))
        }
    }
}

// ───────────── F2：缺席的未问不跨宿主事件 ─────────────

fn e1_direct(g: &mut CellGraph<TV>) {
    g.add_program(
        "P",
        prog(|c| {
            let s = c.source("s").unwrap_or(TV::Null);
            let e = cut(c, "q").map_err(|_| wait())?;
            Ok(TV::map(vec![("s", s), ("e", TV::Exit(e))]))
        }),
    )
}

fn e1_in_cell(g: &mut CellGraph<TV>) {
    g.add_program(
        "P",
        prog(|c| {
            let s = c.source("s").unwrap_or(TV::Null);
            let e = c
                .code("C", |c| Ok(TV::Exit(cut(c, "q")?)))
                .map_err(|_| wait())?;
            Ok(TV::map(vec![("s", s), ("e", e)]))
        }),
    )
}

fn e1_run(
    setup: fn(&mut CellGraph<TV>),
    decide: &mut dyn FnMut(&str, &[String]) -> FlushAnswer<TV>,
) -> CellGraph<TV> {
    let mut g = CellGraph::new();
    setup(&mut g);
    host(&mut g, "s", TV::Int(1));
    g.quiesce(decide);
    let mid_flushes = flushes_of(&g, "q");
    let mid_cause = exit_cause(&g.latest("P").unwrap().value.get("e"));
    assert_eq!(mid_flushes, ["absent", "absent"]);
    assert_eq!(mid_cause, Some(UnsureCause::Absent));
    host(&mut g, "s", TV::Int(2));
    g.quiesce(decide);
    g
}

#[test]
fn r3_e1_缺席未问过了宿主事件再问() {
    let g = &e1_run(e1_direct, &mut absent_first(2));
    assert_eq!(flushes_of(g, "q"), ["absent", "absent", "answer"]);
    assert_eq!(
        exit_cause(&g.latest("P").unwrap().value.get("e")),
        Some(UnsureCause::Tie)
    );
}

#[test]
fn r4_代码单元里读过的缺席未问_宿主事件后重新核验() {
    let g = &e1_run(e1_in_cell, &mut absent_first(2));
    assert_eq!(flushes_of(g, "q"), ["absent", "absent", "answer"]);
    assert_eq!(
        exit_cause(&g.latest("P").unwrap().value.get("e")),
        Some(UnsureCause::Tie)
    );
}

#[test]
fn r5_一直缺席_每个宿主纪元各问满重试() {
    // Z0581（C2b，主控认可读法）：缺席重试计数每个宿主纪元从零起，宿主事件后同一道题再问满 absent_retry + 1 = 2 次；
    // C1 时这里是 3 条（事件后只再问 1 次）
    for setup in [e1_direct as fn(&mut CellGraph<TV>), e1_in_cell] {
        let g = &e1_run(setup, &mut |_k, _r| FlushAnswer::Absent);
        assert_eq!(flushes_of(g, "q"), ["absent", "absent", "absent", "absent"]);
        assert_eq!(
            exit_cause(&g.latest("P").unwrap().value.get("e")),
            Some(UnsureCause::Absent)
        );
    }
}

// ───────────── F3：单元里的放弃报错随记忆项 ─────────────

fn e2_setup(g: &mut CellGraph<TV>) {
    g.add_program(
        "P",
        prog(|c| {
            let _ = c.source("s");
            c.code("C", |c| {
                let e = cut(c, "q")?;
                duty(c, e, DutyKind::DropAccounted, "单元里放弃缺席");
                Ok(TV::Int(1))
            })
            .map_err(|_| wait())?;
            Ok(TV::map(vec![("done", TV::Bool(true))]))
        }),
    )
}

#[test]
fn r6_e2_单元里放弃缺席类_增量与从零() {
    let mut absent = |_k: &str, _r: &[String]| FlushAnswer::<TV>::Absent;
    let fresh = run(
        &e2_setup,
        Knobs::default(),
        &[("s", TV::Int(2))],
        None,
        &mut absent,
    );
    let inc = run(
        &e2_setup,
        Knobs::default(),
        &[("s", TV::Int(1)), ("s", TV::Int(2))],
        None,
        &mut absent,
    );
    let f = concl_summary(&fresh, "P", 1);
    let i = concl_summary(&inc, "P", 2);
    assert_eq!(f, i);
    assert_eq!(f.0, 1);
    assert!(f.1.is_empty(), "{:?}", f.1);
    let errs = |g: &CellGraph<TV>, from: u32| {
        g.attempts("P")
            .iter()
            .rev()
            .find(|a| a.concluded && a.host_epoch >= from)
            .unwrap()
            .cell_errors
            .clone()
    };
    let want = BTreeMap::from([(crate::debt::E_DROP_UNOBSERVED, 1)]);
    assert_eq!(errs(&fresh, 1), want);
    assert_eq!(errs(&inc, 2), want);
}

// ───────────── F1：发布比对含记号 ─────────────

fn e3_setup(g: &mut CellGraph<TV>) {
    g.add_program(
        "W",
        prog(|c| {
            let a = matches!(c.judge("t1"), JudgeRead::Pending);
            let b = matches!(c.judge("t2"), JudgeRead::Pending);
            if a || b {
                Err(wait())
            } else {
                Ok(TV::Bool(true))
            }
        }),
    );
    let cell = |c: &mut Ctx<'_, TV>| -> Result<TV, Pend> {
        c.code("C", |c| {
            let q = if c.source("s") == Some(TV::Int(2)) {
                "t1"
            } else {
                "t2"
            };
            Ok(TV::Exit(cut(c, q)?))
        })
    };
    g.add_program(
        "X",
        prog(move |c| {
            let a = cell(c).map_err(|_| wait())?;
            Ok(TV::map(vec![("a", a)]))
        }),
    );
    g.add_program(
        "Y",
        prog(move |c| {
            let _from_x = c.settled_project("X", |v| v.get("a")).ok_or_else(wait)?;
            if let TV::Exit(e) = cell(c).map_err(|_| wait())? {
                duty(c, e, DutyKind::Escalate, "经共享单元读到的那一笔升级");
            }
            Ok(TV::map(vec![("done", TV::Bool(true))]))
        }),
    );
}

#[test]
fn r1_e3_记号换了也发布_读者对得上() {
    for seed in [None, Some(31), Some(32), Some(33)] {
        let fresh = run(
            &e3_setup,
            Knobs::default(),
            &[("s", TV::Int(3))],
            seed,
            &mut half,
        );
        let inc = run(
            &e3_setup,
            Knobs::default(),
            &[("s", TV::Int(2)), ("s", TV::Int(3))],
            seed,
            &mut half,
        );
        let f = concl_summary(&fresh, "Y", 1);
        assert_eq!(f, concl_summary(&inc, "Y", 2), "{seed:?}");
        assert_eq!(f.0, 0, "{seed:?}");
    }
}

fn e4_setup(g: &mut CellGraph<TV>) {
    g.add_program(
        "X",
        prog(|c| {
            let s = c.source("s");
            let b = cut(c, "tb").map_err(|_| wait())?;
            let a = cut(c, "ta").map_err(|_| wait())?;
            if s == Some(TV::Int(2)) {
                duty(c, b, DutyKind::Escalate, "b 升级");
            }
            Ok(TV::map(vec![("a", TV::Exit(a))]))
        }),
    );
    g.add_program(
        "Y",
        prog(|c| {
            let v = c.settled("X").ok_or_else(wait)?;
            Ok(TV::map(vec![("x", v)]))
        }),
    );
}

#[test]
fn r7_e4_值不变原因变了也发布() {
    let fresh = run(
        &e4_setup,
        Knobs::default(),
        &[("s", TV::Int(2))],
        None,
        &mut half,
    );
    let inc = run(
        &e4_setup,
        Knobs::default(),
        &[("s", TV::Int(1)), ("s", TV::Int(2))],
        None,
        &mut half,
    );
    assert_eq!(finals(&fresh), finals(&inc));
    let vers: Vec<(PState, Vec<&str>)> = inc
        .published("X")
        .iter()
        .map(|p| (p.state, p.causes.iter().map(|c| c.name()).collect()))
        .collect();
    assert_eq!(
        vers,
        [
            (PState::InProgress, vec![]),
            (PState::Unsure, vec!["violation"]),
            (PState::Unsure, vec!["tie"])
        ]
    );
}

fn e5_setup(g: &mut CellGraph<TV>) {
    g.add_program(
        "P",
        prog(|c| {
            let v = c
                .code("PAR", |c| {
                    c.code("C", |c| {
                        let s = c.source("s");
                        let e = cut(c, "q")?;
                        let k = if s == Some(TV::Int(1)) {
                            DutyKind::Escalate
                        } else {
                            DutyKind::Refine
                        };
                        duty(c, e, k, "交出");
                        Ok(TV::Int(1))
                    })
                })
                .map_err(|_| wait())?;
            Ok(TV::map(vec![("v", v)]))
        }),
    )
}

#[test]
fn r8_e5_单元去向种类变了_增量等于从零() {
    let fresh = run(
        &e5_setup,
        Knobs::default(),
        &[("s", TV::Int(2))],
        None,
        &mut half,
    );
    let inc = run(
        &e5_setup,
        Knobs::default(),
        &[("s", TV::Int(1)), ("s", TV::Int(2))],
        None,
        &mut half,
    );
    let last = |g: &CellGraph<TV>| {
        g.attempts("P")
            .iter()
            .rev()
            .find(|a| a.concluded)
            .unwrap()
            .cell_duties
            .clone()
    };
    assert_eq!(last(&fresh), last(&inc));
    assert_eq!(last(&fresh), BTreeMap::from([(DutyKind::Refine, 1)]));
}

// ───────────── 立刻读的承接（R10 第 7 条） ─────────────

fn peek_setup(write_b: bool) -> impl Fn(&mut CellGraph<TV>) {
    move |g| {
        g.add_program("X", prog(take_x));
        g.add_program(
            "Y",
            prog(move |c| {
                // 先等到读 X 的版本号（记依赖，X 定下后被标脏），再立刻读它的值
                if c.settled_version("X").is_none() {
                    return Err(TV::s("等 X 有结论"));
                }
                let p = c.peek("X").expect("X 已有版本");
                Ok(if write_b {
                    TV::map(vec![("b", p.value.get("b"))])
                } else {
                    TV::map(vec![("done", TV::Bool(true))])
                })
            }),
        );
    }
}

#[test]
fn r9_立刻读到的未决照样接下() {
    // X 在 s = 2 时 a、b 都是未决
    let g = run(
        &peek_setup(false),
        Knobs::default(),
        &[("s", TV::Int(2))],
        None,
        &mut half,
    );
    let s = concl_summary(&g, "Y", 1);
    assert_eq!(s.0, 2, "{s:?}");
    let g = run(
        &peek_setup(true),
        Knobs::default(),
        &[("s", TV::Int(2))],
        None,
        &mut half,
    );
    let s = concl_summary(&g, "Y", 1);
    assert_eq!((s.0, s.1.get("handoff").copied()), (1, Some(1)), "{s:?}");
}

// ───────────── Union 归约器 ─────────────

#[test]
fn r10_并集_两份都留_同值不算合并() {
    let mut c = crate::reducer::declare::<TV>(Reducer::Union, Writers::Multi).unwrap();
    assert_eq!(c.write("A", "k", TV::Int(1)), Ok(true));
    assert_eq!(c.write("B", "k", TV::Int(2)), Ok(true));
    assert_eq!(
        c.write("A", "k", TV::Int(1)),
        Ok(false),
        "同一写者重复写同值"
    );
    assert_eq!(
        c.write("B", "k", TV::Int(1)),
        Ok(false),
        "另一写者写已有的同值：按值哈希去重"
    );
    match c.read("k") {
        SharedRead::Many(xs) => assert_eq!(xs.len(), 2),
        other => panic!("{other:?}"),
    }
    assert_eq!(c.events.len(), 2);
    assert!(c.events.iter().all(|e| e.kind == MergeKind::Merge));
}
