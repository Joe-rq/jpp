//! 预注册 4.1 早截断三个方向（C1-1 至 C1-8）与 4.2 菱形与三层链（C1-9 至 C1-12）。
//! 原型：`demos::early_cutoff_suite`、`item1_chain3`、`item1_diamond`（过程记录第 13、15、16 节）。

use super::*;

// ───────────── 早截断 ─────────────

/// 子单元 C：值不随出口变；`s` = 2 时过桥，`handled` 决定空分支还是升级。
fn cell_c(c: &mut Ctx<'_, TV>, handled: bool) -> Result<TV, Pend> {
    let key = if handled { "C2" } else { "C" };
    c.code(key, move |c| {
        if c.source("s") == Some(TV::Int(2)) {
            let e = cut(c, "tie")?;
            if handled {
                duty(c, e, DutyKind::Escalate, "C2 里交出：升级");
            }
        }
        Ok(TV::Int(1))
    })
}

fn early_setup(direct: bool, handled: bool) -> impl Fn(&mut CellGraph<TV>) {
    move |g| {
        g.add_program(
            "E",
            prog(move |c| {
                let v = if direct {
                    cell_c(c, handled).map_err(|_| wait())?
                } else {
                    let pk = if handled { "P2" } else { "P" };
                    c.code(pk, move |c| cell_c(c, handled))
                        .map_err(|_| wait())?
                };
                Ok(TV::map(vec![("v", v)]))
            }),
        )
    }
}

/// 变更之后各次尝试的违规合计、最后一次尝试的单元去向（原型 `early_cutoff` 的口径）。
fn early_summary(g: &CellGraph<TV>, from_epoch: u32) -> (usize, BTreeMap<DutyKind, u32>) {
    let atts: Vec<&AttemptRec> = g
        .attempts("E")
        .iter()
        .filter(|a| a.host_epoch >= from_epoch)
        .collect();
    let viol = atts.iter().map(|a| a.violations.len()).sum();
    (
        viol,
        atts.last()
            .map(|a| a.cell_duties.clone())
            .unwrap_or_default(),
    )
}

struct EarlyRow {
    inc: (usize, BTreeMap<DutyKind, u32>),
    fresh: (usize, BTreeMap<DutyKind, u32>),
}

fn early(
    from: i64,
    to: i64,
    direct: bool,
    handled: bool,
    knobs: Knobs,
    seed: Option<u64>,
) -> EarlyRow {
    let setup = early_setup(direct, handled);
    let inc = run(
        &setup,
        knobs,
        &[("s", TV::Int(from)), ("s", TV::Int(to))],
        seed,
        &mut half,
    );
    let fresh = run(&setup, knobs, &[("s", TV::Int(to))], seed, &mut half);
    EarlyRow {
        inc: early_summary(&inc, 2),
        fresh: early_summary(&fresh, 1),
    }
}

const ROWS: [(i64, i64, bool); 3] = [(1, 2, false), (2, 1, false), (1, 2, true)];

#[test]
fn c1_1_至_6_早截断_增量等于从零_三个方向() {
    let esc = BTreeMap::from([(DutyKind::Escalate, 1)]);
    for (from, to, handled) in ROWS {
        for direct in [false, true] {
            let r = early(from, to, direct, handled, Knobs::default(), None);
            assert_eq!(
                r.inc, r.fresh,
                "{from}→{to} direct={direct} handled={handled}"
            );
            let want_v = if to == 2 && !handled { 1 } else { 0 };
            assert_eq!(
                r.inc.0, want_v,
                "{from}→{to} direct={direct} handled={handled}"
            );
            if handled {
                assert_eq!(r.inc.1, esc, "C1-5/6 单元去向 {{escalate: 1}}");
            }
        }
    }
}

#[test]
fn c1_7_反证_记忆哈希去掉欠账摘要_经父单元的三处不等() {
    let off = Knobs {
        debt_digest: false,
        ..Knobs::default()
    };
    let r = early(1, 2, false, false, off, None);
    assert_eq!((r.inc.0, r.fresh.0), (0, 1), "C1-1 改前：增量 0 / 从零 1");
    let r = early(2, 1, false, false, off, None);
    assert_eq!((r.inc.0, r.fresh.0), (1, 0), "C1-3 改前：幻影违规");
    let r = early(1, 2, false, true, off, None);
    assert_eq!(
        r.inc.1,
        BTreeMap::new(),
        "C1-5 改前：增量的单元去向停在旧集合（空）"
    );
    assert_eq!(r.fresh.1, BTreeMap::from([(DutyKind::Escalate, 1)]));
    // 直读不经父单元，不受摘要影响
    let r = early(1, 2, true, false, off, None);
    assert_eq!(r.inc, r.fresh);
}

#[test]
fn c1_8_早截断_种子调度三个种子_统计与静止状态相同() {
    for (from, to, handled) in ROWS {
        for direct in [false, true] {
            let base = early(from, to, direct, handled, Knobs::default(), None);
            let setup = early_setup(direct, handled);
            let ev = [("s", TV::Int(from)), ("s", TV::Int(to))];
            let g0 = run(&setup, Knobs::default(), &ev, None, &mut half);
            for seed in [11, 12, 13] {
                let r = early(from, to, direct, handled, Knobs::default(), Some(seed));
                assert_eq!(r.inc, base.inc, "种子 {seed}");
                assert_eq!(r.fresh, base.fresh, "种子 {seed}");
                let g = run(&setup, Knobs::default(), &ev, Some(seed), &mut half);
                assert_eq!(finals(&g), finals(&g0), "种子 {seed}");
            }
        }
    }
}

// ───────────── 三层链与菱形 ─────────────

fn cell_z(c: &mut Ctx<'_, TV>) -> Result<TV, Pend> {
    c.code("Z", |c| {
        let e = cut(c, "tie")?;
        Ok(TV::Str(e.name()))
    })
}

fn chain3(g: &mut CellGraph<TV>) {
    g.add_program(
        "P",
        prog(|c| {
            let x = c
                .code("X", |c| {
                    let y = c.code("Y", |c| {
                        let _z = cell_z(c)?;
                        Ok(c.source("s/y").unwrap_or(TV::Null))
                    })?;
                    Ok(y)
                })
                .map_err(|_| wait())?;
            Ok(TV::map(vec![("y", x)]))
        }),
    )
}

fn per_attempt(g: &CellGraph<TV>, id: &str) -> Vec<(bool, usize, BTreeMap<DutyKind, u32>)> {
    g.attempts(id)
        .iter()
        .map(|a| (a.concluded, a.violations.len(), a.cell_duties.clone()))
        .collect()
}

#[test]
fn c1_9_三层链_每次有结论的尝试违规1_变更后z不重算() {
    let mut g = CellGraph::new();
    chain3(&mut g);
    host(&mut g, "s/y", TV::Int(1));
    g.quiesce(&mut half);
    let before = g.stats().clone();
    host(&mut g, "s/y", TV::Int(2));
    g.quiesce(&mut half);
    let after = g.stats().clone();
    let per = per_attempt(&g, "P");
    let settled: Vec<_> = per.iter().filter(|a| a.0).collect();
    assert!(
        settled.len() >= 2 && settled.iter().all(|a| a.1 == 1),
        "{per:?}"
    );
    assert_eq!(after.computed - before.computed, 2, "只重算 Y、X");
}

fn diamond(handled: bool, touch_q: bool) -> impl Fn(&mut CellGraph<TV>) {
    move |g| {
        g.add_program(
            "D",
            prog(move |c| {
                let s_cell = move |c: &mut Ctx<'_, TV>| -> Result<TV, Pend> {
                    c.code("S", move |c| {
                        if touch_q {
                            let _ = c.source("q");
                        }
                        let e = cut(c, "tie")?;
                        if handled {
                            duty(c, e.clone(), DutyKind::Escalate, "S 里交出：升级");
                        }
                        Ok(TV::Str(e.name()))
                    })
                };
                let m = c.source("s/m");
                let mut got = vec![];
                if m == Some(TV::Int(1)) {
                    got.push(c.code("L", move |c| s_cell(c)).map_err(|_| wait())?);
                }
                got.push(c.code("R", move |c| s_cell(c)).map_err(|_| wait())?);
                Ok(TV::map(vec![("read", TV::List(got))]))
            }),
        )
    }
}

#[test]
fn c1_10_11_菱形_同一笔只算一笔_交出后去向不重复() {
    let ev = [("s/m", TV::Int(1)), ("s/m", TV::Int(2))];
    let g = run(
        &diamond(false, false),
        Knobs::default(),
        &ev,
        None,
        &mut half,
    );
    let per = per_attempt(&g, "D");
    let settled: Vec<_> = per.iter().filter(|a| a.0).collect();
    assert!(
        settled.len() >= 2 && settled.iter().all(|a| a.1 == 1),
        "C1-10 {per:?}"
    );

    let g = run(
        &diamond(true, false),
        Knobs::default(),
        &ev,
        None,
        &mut half,
    );
    let per = per_attempt(&g, "D");
    let esc = BTreeMap::from([(DutyKind::Escalate, 1)]);
    let settled: Vec<_> = per.iter().filter(|a| a.0).collect();
    assert!(
        settled.len() >= 2 && settled.iter().all(|a| a.1 == 0 && a.2 == esc),
        "C1-11 {per:?}"
    );
}

/// C1-12 按预注册：去掉「核依赖后没变」路径上的欠账并入，预测菱形第二次有结论的尝试违规变 0。
/// 这条预测会落空（见结果节）：`s/m` 改了只标脏程序 D，L、R、S 都不脏，第二次读 R 走的是「干净命中」，不经核依赖。
/// 这里断言实测（违规照旧 1），反证改由 `c1_12b` 的场景承担。
#[test]
fn c1_12_反证_预注册的场景不经核依赖路径() {
    let off = Knobs {
        inherit_on_verified: false,
        ..Knobs::default()
    };
    let ev = [("s/m", TV::Int(1)), ("s/m", TV::Int(2))];
    let g = run(&diamond(false, false), off, &ev, None, &mut half);
    let per = per_attempt(&g, "D");
    let settled: Vec<_> = per.iter().filter(|a| a.0).collect();
    assert!(settled.iter().all(|a| a.1 == 1), "{per:?}");
}

/// C1-12b（事后加的场景）：S 读源 `q`，`q` 变了而 S 的值与欠账都没变。S、R、D 都被标脏；D 读 R 时 R 核依赖，
/// S 重算后记忆哈希没变，R 走「核依赖后没变」返回。这条路径不并欠账时违规变 0。
#[test]
fn c1_12b_反证_核依赖后没变的路径也要并欠账() {
    let ev = [("s/m", TV::Int(2)), ("q", TV::Int(1)), ("q", TV::Int(2))];
    let on = run(
        &diamond(false, true),
        Knobs::default(),
        &ev,
        None,
        &mut half,
    );
    let last_on = on.attempts("D").last().unwrap().clone();
    assert!(last_on.concluded && last_on.host_epoch == 3);
    assert_eq!(last_on.violations.len(), 1);
    assert!(on.stats().verified_clean >= 1, "要真走到核依赖后没变");
    let off = Knobs {
        inherit_on_verified: false,
        ..Knobs::default()
    };
    let g = run(&diamond(false, true), off, &ev, None, &mut half);
    assert_eq!(
        g.attempts("D").last().unwrap().violations.len(),
        0,
        "反证：不并欠账则违规丢了"
    );
}
