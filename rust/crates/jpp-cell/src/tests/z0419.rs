//! 预注册 4.3 记号撞车与换题、多写者单元承接（C1-13 至 C1-16）。原型：`demos::z0419_extra`（过程记录第 20 节）。

use super::*;

/// X：`b` 总是过桥得未决；`a` 在 `s` = 2 时同样是未决，`s` = 1 时是已定的字符串；`c` 是已定的数。
pub(crate) fn take_x(c: &mut Ctx<'_, TV>) -> Result<TV, TV> {
    let s = c.source("s");
    let b = cut(c, "tie").map_err(|_| wait())?;
    let a = if s == Some(TV::Int(2)) {
        TV::Exit(cut(c, "tie").map_err(|_| wait())?)
    } else {
        TV::s("定了")
    };
    Ok(TV::map(vec![
        ("a", a),
        ("b", TV::Exit(b)),
        ("c", TV::Int(1)),
    ]))
}

fn collide(key: &'static str) -> CellGraph<TV> {
    let setup = move |g: &mut CellGraph<TV>| {
        g.add_program("X", prog(take_x));
        g.add_program(
            "Y",
            prog(move |c| {
                match c.settled_project("X", |v| v.get("b")) {
                    Some(TV::Exit(e @ TExit::Unsure(..))) => {
                        duty(c, e, DutyKind::Escalate, "承接来的 b 升级")
                    }
                    _ => return Err(TV::s("等 X")),
                }
                c.code(key, |c| {
                    let _ = cut(c, "tie")?;
                    Ok(TV::Int(1))
                })
                .map_err(|_| wait())?;
                Ok(TV::map(vec![("done", TV::Bool(true))]))
            }),
        );
    };
    run(
        &setup,
        Knobs::default(),
        &[("s", TV::Int(2))],
        None,
        &mut half,
    )
}

#[test]
fn c1_13_记号撞车_单元键与程序同名也各算各的() {
    for key in ["X", "XX"] {
        let g = collide(key);
        let last = g.attempts("Y").iter().rev().find(|a| a.concluded).unwrap();
        assert_eq!(last.violations.len(), 1, "键 {key}：{:?}", last.violations);
        assert_eq!(last.duty_counts().get("escalate"), Some(&1));
    }
}

fn swap_setup(g: &mut CellGraph<TV>) {
    g.add_program(
        "P",
        prog(|c| {
            let v = c
                .code("PV3", |c| {
                    c.code("V3", |c| {
                        let q = if c.source("s") == Some(TV::Int(3)) {
                            "gap"
                        } else {
                            "tie"
                        };
                        let _ = cut(c, q)?;
                        Ok(TV::Int(1))
                    })
                })
                .map_err(|_| wait())?;
            Ok(TV::map(vec![("v", v)]))
        }),
    )
}

/// `gap` 题恒缺席，其余恒答 0.5。
fn gap_absent(k: &str, _r: &[String]) -> FlushAnswer<TV> {
    if k == "gap" {
        FlushAnswer::Absent
    } else {
        FlushAnswer::Answer(TV::Reading(0.5))
    }
}

fn swap_violations(knobs: Knobs, events: &[i64]) -> Vec<String> {
    let ev: Vec<(&str, TV)> = events.iter().map(|v| ("s", TV::Int(*v))).collect();
    let g = run(&swap_setup, knobs, &ev, None, &mut gap_absent);
    let from = events.len() as u32;
    g.attempts("P")
        .iter()
        .filter(|a| a.host_epoch >= from && a.concluded)
        .flat_map(|a| a.violations.iter().map(viol_key))
        .collect()
}

#[test]
fn c1_14_同一处换题_增量与从零的违规逐项相同() {
    let inc = swap_violations(Knobs::default(), &[2, 3]);
    let fresh = swap_violations(Knobs::default(), &[3]);
    assert_eq!(inc, fresh);
    assert_eq!(fresh.len(), 1);
    assert!(
        fresh[0].contains("（absent）") && fresh[0].contains("q=gap"),
        "{fresh:?}"
    );
}

#[test]
fn c1_15_反证_记号不含题与原因_换题后增量记旧原因() {
    let off = Knobs {
        token_has_question: false,
        ..Knobs::default()
    };
    let inc = swap_violations(off, &[2, 3]);
    let fresh = swap_violations(off, &[3]);
    assert_ne!(inc, fresh, "{inc:?} {fresh:?}");
    assert!(inc.iter().any(|v| v.contains("（tie）")), "{inc:?}");
}

fn shared_setup(pass: bool) -> impl Fn(&mut CellGraph<TV>) {
    move |g| {
        g.declare_shared("ch", Reducer::Latest, Writers::Single)
            .unwrap();
        g.add_program(
            "X",
            prog(|c| {
                let v = take_x(c)?;
                c.write_shared("ch", "k", v.get("a"));
                Ok(v)
            }),
        );
        g.add_program(
            "Y",
            prog(move |c| {
                if c.settled_version("X").is_none() {
                    return Err(TV::s("等 X"));
                }
                let a = TV::from_shared(c.read_shared("ch", "k"));
                Ok(if pass {
                    TV::map(vec![("a", a)])
                } else {
                    TV::map(vec![("done", TV::Bool(true))])
                })
            }),
        );
    }
}

/// 有结论尝试的违规合计与最后一次的去向计数（读者承接的口径）。
pub(crate) fn concl_summary(
    g: &CellGraph<TV>,
    id: &str,
    from: u32,
) -> (usize, BTreeMap<&'static str, u32>) {
    let atts: Vec<&AttemptRec> = g
        .attempts(id)
        .iter()
        .filter(|a| a.host_epoch >= from && a.concluded)
        .collect();
    (
        atts.iter().map(|a| a.violations.len()).sum(),
        atts.last().map(|a| a.duty_counts()).unwrap_or_default(),
    )
}

#[test]
fn c1_16_多写者单元也承接_丢1_传0转交1_增量等于从零() {
    for pass in [false, true] {
        let fresh = run(
            &shared_setup(pass),
            Knobs::default(),
            &[("s", TV::Int(2))],
            None,
            &mut half,
        );
        let inc = run(
            &shared_setup(pass),
            Knobs::default(),
            &[("s", TV::Int(1)), ("s", TV::Int(2))],
            None,
            &mut half,
        );
        let f = concl_summary(&fresh, "Y", 1);
        let i = concl_summary(&inc, "Y", 2);
        assert_eq!(f, i, "pass={pass}");
        if pass {
            assert_eq!(f.0, 0);
            assert_eq!(f.1.get("handoff"), Some(&1));
        } else {
            assert_eq!(f.0, 1);
        }
    }
}
