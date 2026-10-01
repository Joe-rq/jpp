//! C2c（步 41）：解释器驱动的代码单元（`begin_code`/`end_code`）、B194 由引擎强制、代码单元帧里不许占用、写多写者
//! 单元、立刻读（Z0510）。预注册：`地基/过程记录/工程-C2-单元图求值.md` 附录二 A2.4、A2.8（C2c-4）。

use super::*;
use crate::reducer::{Reducer, Writers};
use std::cell::Cell;

/// 解释器驱动地求一个读源 `s` 的代码单元：体是 `s + 1`，计数求值次数。
fn ext_cell(c: &mut Ctx<'_, TV>, n: &Cell<u32>, force: bool) -> TV {
    match c.begin_code("K", force) {
        CodeStep::Hit(v) => v,
        CodeStep::Compute => {
            n.set(n.get() + 1);
            let s = match c.source("s") {
                Some(TV::Int(i)) => i,
                _ => 0,
            };
            c.end_code("K", Ok(TV::Int(s + 1))).unwrap()
        }
    }
}

fn 外置跑(g: &mut CellGraph<TV>, n: &Cell<u32>, force: bool, 次: usize) -> Vec<TV> {
    let a = g.open("P");
    let mut c = Ctx::resume(g, a);
    let vs: Vec<TV> = (0..次).map(|_| ext_cell(&mut c, n, force)).collect();
    let a = c.suspend();
    g.close("P", a, Ok(TV::List(vs.clone())));
    vs
}

/// C2c-4：`begin_code`/`end_code` 与 `code` 同一场景给出相同的记忆与依赖：同一趟第二次命中；源变了被标脏、重算。
#[test]
fn c2c_4_begin_end_与_code_同一结果() {
    // 引擎驱动
    let mut g1 = CellGraph::<TV>::new();
    let n1 = Rc::new(Cell::new(0));
    let m = n1.clone();
    g1.add_program(
        "P",
        prog(move |c| {
            let m2 = m.clone();
            let f = move |c: &mut Ctx<'_, TV>| {
                m2.set(m2.get() + 1);
                let s = match c.source("s") {
                    Some(TV::Int(i)) => i,
                    _ => 0,
                };
                Ok(TV::Int(s + 1))
            };
            let a = c.code("K", f.clone()).map_err(|_| wait())?;
            let b = c.code("K", f).map_err(|_| wait())?;
            Ok(TV::List(vec![a, b]))
        }),
    );
    host(&mut g1, "s", TV::Int(1));
    g1.quiesce(&mut half);
    // 解释器驱动
    let mut g2 = CellGraph::<TV>::new();
    g2.add_external_program("P");
    let n2 = Cell::new(0);
    host(&mut g2, "s", TV::Int(1));
    let v2 = 外置跑(&mut g2, &n2, false, 2);
    assert_eq!(g1.latest("P").unwrap().value, TV::List(v2.clone()));
    assert_eq!((n1.get(), n2.get()), (1, 1), "同一趟第二次命中");
    assert_eq!(g1.code_cells(), g2.code_cells());
    // 源变了：两边都标脏、重算一次
    host(&mut g1, "s", TV::Int(5));
    g1.quiesce(&mut half);
    host(&mut g2, "s", TV::Int(5));
    let v2 = 外置跑(&mut g2, &n2, false, 2);
    assert_eq!(g1.latest("P").unwrap().value, TV::List(v2));
    assert_eq!((n1.get(), n2.get()), (2, 2));
}

/// C2c-4：`force` 时重算并写回同一记忆项。
#[test]
fn c2c_4_force_重算写回() {
    let mut g = CellGraph::<TV>::new();
    g.add_external_program("P");
    host(&mut g, "s", TV::Int(1));
    let n = Cell::new(0);
    外置跑(&mut g, &n, true, 3);
    assert_eq!(n.get(), 3, "每次都重算");
    assert_eq!(g.code_cells(), 1, "写回同一记忆项");
}

/// 解释器驱动的记忆项没有求值体：被它的读者经核依赖要求时，引擎修不了它，按「变了」处理，读者重算。
#[test]
fn c2c_外置记忆项核依赖从严() {
    let mut g = CellGraph::<TV>::new();
    g.add_external_program("P");
    host(&mut g, "s", TV::Int(1));
    let 外层 = |c: &mut Ctx<'_, TV>, n: &Cell<u32>, m: &Cell<u32>| match c.begin_code("O", false) {
        CodeStep::Hit(v) => v,
        CodeStep::Compute => {
            m.set(m.get() + 1);
            let v = ext_cell(c, n, false);
            c.end_code("O", Ok(v)).unwrap()
        }
    };
    let (n, m) = (Cell::new(0), Cell::new(0));
    for _ in 0..2 {
        let a = g.open("P");
        let mut c = Ctx::resume(&mut g, a);
        let v = 外层(&mut c, &n, &m);
        let a = c.suspend();
        g.close("P", a, Ok(v));
        host(&mut g, "s", TV::Int(2));
    }
    assert_eq!((n.get(), m.get()), (2, 2), "内层脏了核不了，外层也重算");
}

/// Z0510 第一条（B194）：等到读碰到上游未定，程序交来有结论也降为进行中；上游定下后被标脏再尝试，这次有结论。
#[test]
fn z0510_b194_等到读未定交ok被降为进行中() {
    let mut g = CellGraph::<TV>::new();
    g.add_program(
        "U",
        prog(|c| Ok(TV::Exit(cut(c, "q").map_err(|_| wait())?))),
    );
    // 读者吞掉「未定」直接交 Ok（错的写法）
    g.add_program("R", prog(|c| Ok(c.settled("U").unwrap_or(TV::Null))));
    // 先只让 R 尝试一次：U 还没有定下的版本
    g.attempt("R");
    let r = g.latest("R").unwrap();
    assert_eq!(r.state, PState::InProgress, "交了 Ok 也不发布有结论的版本");
    assert!(!g.attempts("R")[0].concluded);
    g.quiesce(&mut |_k, _r| FlushAnswer::Answer(TV::Reading(0.9)));
    let r = g.latest("R").unwrap();
    assert!(
        r.state.settled(),
        "上游定下后被标脏、再尝试有结论：{:?}",
        r.state
    );
}

#[test]
#[should_panic(expected = "E-cell-impure")]
fn z0510_代码单元里占用被拒() {
    let mut g = CellGraph::<TV>::new();
    g.declare_shared("m", Reducer::Claim, Writers::Multi)
        .unwrap();
    g.add_program(
        "P",
        prog(|c| {
            let _ = c.code("K", |c| {
                let _ = c.claim("m", &["x".to_string()]);
                Ok(TV::Null)
            });
            Ok(TV::Null)
        }),
    );
    host(&mut g, "s", TV::Int(1));
    g.quiesce(&mut half);
}

#[test]
#[should_panic(expected = "E-cell-impure")]
fn z0510_代码单元里写多写者单元被拒() {
    let mut g = CellGraph::<TV>::new();
    g.declare_shared("u", Reducer::Union, Writers::Multi)
        .unwrap();
    g.add_program(
        "P",
        prog(|c| {
            let _ = c.code("K", |c| {
                c.write_shared("u", "k", TV::Int(1));
                Ok(TV::Null)
            });
            Ok(TV::Null)
        }),
    );
    host(&mut g, "s", TV::Int(1));
    g.quiesce(&mut half);
}

#[test]
#[should_panic(expected = "E-cell-impure")]
fn z0510_代码单元里立刻读被拒() {
    let mut g = CellGraph::<TV>::new();
    g.add_program("U", prog(|_c| Ok(TV::Int(1))));
    g.add_program(
        "P",
        prog(|c| {
            let _ = c.code("K", |c| {
                let _ = c.peek("U");
                Ok(TV::Null)
            });
            Ok(TV::Null)
        }),
    );
    host(&mut g, "s", TV::Int(1));
    g.quiesce(&mut half);
}
