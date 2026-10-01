//! C2b（步 41）：解释器驱动的接口——外置的尝试状态（`open`/`close`）、判断单元与账本单元的读写、刷新纪元。
//! 预注册：`地基/过程记录/工程-C2-单元图求值.md` 附录一 A1.2、A1.4（C2b-7、C2b-8）。

use super::*;

fn 读两个源(c: &mut Ctx<'_, TV>) -> Result<TV, TV> {
    let a = c.source("a").unwrap_or(TV::Null);
    let e = cut(c, "q").map_err(|_| wait())?;
    Ok(TV::map(vec![("a", a), ("e", TV::Exit(e))]))
}

/// C2b-7：同一段程序，引擎驱动（`attempt`）与解释器驱动（`open` + `Ctx::resume` + `close`）给出相同的发布与尝试记录。
#[test]
fn c2b_7_open_close_与_attempt_同一结果() {
    let run = |external: bool| {
        let mut g = CellGraph::<TV>::new();
        if external {
            g.add_external_program("P");
        } else {
            g.add_program("P", prog(读两个源));
        }
        host(&mut g, "a", TV::Int(1));
        let step = |g: &mut CellGraph<TV>| {
            if external {
                let a = g.open("P");
                let mut c = Ctx::resume(g, a);
                let out = 读两个源(&mut c);
                let a = c.suspend();
                g.close("P", a, out);
            } else {
                g.attempt("P");
            }
        };
        step(&mut g);
        g.flush(&mut half);
        step(&mut g);
        let pubs: Vec<(u32, PState, String)> = g
            .published("P")
            .iter()
            .map(|p| (p.version, p.state, p.hash.clone()))
            .collect();
        let recs: Vec<(u32, u32, u32, bool, usize, usize)> = g
            .attempts("P")
            .iter()
            .map(|r| {
                (
                    r.n,
                    r.host_epoch,
                    r.flush_epoch,
                    r.concluded,
                    r.violations.len(),
                    r.pending.len(),
                )
            })
            .collect();
        (pubs, recs)
    };
    let (p1, r1) = run(false);
    let (p2, r2) = run(true);
    assert_eq!(p1, p2);
    assert_eq!(r1, r2);
    // 第二次尝试读到并列，出口随返回值转交 → 违规 0，这一版「未决」（R2：值里带未销未决）
    assert_eq!(p1.last().unwrap().1, PState::Unsure);
    assert_eq!(r1.last().unwrap().4, 0);
    assert_eq!(r1.len(), 2);
}

/// 解释器驱动的程序不进引擎的调度。
#[test]
fn c2b_外置程序不由引擎调度() {
    let mut g = CellGraph::<TV>::new();
    g.add_external_program("P");
    assert!(g.runnable().is_empty());
    g.quiesce(&mut half);
    assert!(g.attempts("P").is_empty());
}

/// C2b-7：`record_answer` 先到先得；`answer` 不看刷新纪元，引擎驱动的 `Ctx::judge` 照旧看。
#[test]
fn c2b_7_判断单元先到先得_刷新纪元() {
    let mut g = CellGraph::<TV>::new();
    assert!(g.record_answer("q", TV::Reading(0.9)));
    assert!(!g.record_answer("q", TV::Reading(0.1)), "已有不覆盖");
    assert_eq!(g.answer("q"), Some(&TV::Reading(0.9)));
    assert_eq!(g.judge_cells(), 1);
    // 同一刷新纪元里写下的答案，引擎驱动的读法还看不见
    g.add_external_program("P");
    let a = g.open("P");
    let mut c = Ctx::resume(&mut g, a);
    assert!(matches!(c.judge("q"), JudgeRead::Pending));
    let a = c.suspend();
    g.close("P", a, Err(wait()));
    // 刷新纪元推进后看得见
    g.advance_flush();
    assert_eq!(g.flush_epoch(), 1);
    let a = g.open("P");
    let mut c = Ctx::resume(&mut g, a);
    assert!(matches!(c.judge("q"), JudgeRead::Answered(TV::Reading(p)) if p == 0.9));
    let a = c.suspend();
    g.close("P", a, Ok(TV::Null));
}

/// 账本求值体的代码单元：先到先得。
#[test]
fn c2b_账本单元先到先得() {
    let mut g = CellGraph::<TV>::new();
    assert!(g.record_ledger_cell("k", TV::s("第一条")));
    assert!(!g.record_ledger_cell("k", TV::s("第二条")));
    assert_eq!(g.ledger_cell("k"), Some(&TV::s("第一条")));
    assert_eq!(g.ledger_cells(), 1);
    assert_eq!(g.ledger_cell("别的"), None);
}

/// 解释器驱动时记的判断依赖进程序的依赖边：判断单元缺席过期（宿主事件）时程序被标脏。
#[test]
fn c2b_judge_dep_进依赖边() {
    let mut g = CellGraph::<TV>::new();
    g.add_external_program("P");
    let a = g.open("P");
    let mut c = Ctx::resume(&mut g, a);
    c.judge_dep("q", JudgeSeen::Answered);
    let a = c.suspend();
    g.close("P", a, Ok(TV::Null));
    assert_eq!(g.stats().marked, 0);
    g.mark(crate::graph::Node::Judge("q".into()));
    assert_eq!(g.stats().marked, 1, "读过 q 的程序被标脏");
}

/// C2b-8（Z0581）：缺席重试计数每个宿主纪元从零起——宿主事件后同一道题再问满 absent_retry + 1 = 2 次。
#[test]
fn c2b_8_宿主事件后缺席题再问满两次() {
    let mut g = CellGraph::<TV>::new();
    g.add_program(
        "P",
        prog(|c| Ok(TV::Exit(cut(c, "q").map_err(|_| wait())?))),
    );
    host(&mut g, "s", TV::Int(1));
    g.quiesce(&mut |_k, _r| FlushAnswer::Absent);
    let n1 = g
        .flushes
        .iter()
        .flat_map(|f| &f.questions)
        .filter(|q| q.0 == "q")
        .count();
    assert_eq!(n1, 2);
    // 宿主事件：缺席的未问过期、程序被标脏、重试计数清零
    host(&mut g, "s", TV::Int(2));
    g.quiesce(&mut |_k, _r| FlushAnswer::Absent);
    let n2 = g
        .flushes
        .iter()
        .flat_map(|f| &f.questions)
        .filter(|q| q.0 == "q")
        .count();
    assert_eq!(n2 - n1, 2, "事件后再问满两次（改前只再问 1 次）");
    assert_eq!(g.host_epoch(), 2);
}
