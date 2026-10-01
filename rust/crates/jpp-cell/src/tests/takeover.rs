//! 预注册 4.6 读者承接按元素（C1-23 至 C1-27）与 4.7 转交要值里真带着（C1-28 至 C1-31）。
//! 原型：`demos::takeover_suite`（过程记录第 16 节 3.1–3.6、4.1、4.2 与交回前补的一个场景）。

use super::z0419::{concl_summary, take_x};
use super::*;

/// 读者的写法。
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Tk {
    丢,
    传,
    升级,
    假转交,
    整读丢,
    只读c,
    单元丢,
    单元假转交,
    单元转交写出,
    升级后单元再读,
    /// 新场景（C1-30）：单元值带着出口、单元级转交成立，程序读了单元却不写出
    单元转交后丢,
    /// 新场景（C1-31）：单元值 `{a, c}`，程序只写出 `c`
    单元投影只写c,
    /// 新场景（C1-31）：同上，程序写出 `a`
    单元投影写a,
}

fn read_a(c: &mut Ctx<'_, TV>, from: &str) -> Option<TV> {
    c.settled_project(from, |v| v.get("a"))
}

fn unsure_of(v: &TV) -> Option<TExit> {
    match v {
        TV::Exit(e @ TExit::Unsure(_, Some(_))) => Some(e.clone()),
        _ => None,
    }
}

pub(crate) fn take_y(from: &'static str, how: Tk) -> Rc<dyn Program<TV>> {
    prog(move |c| {
        let done = || Ok(TV::map(vec![("done", TV::Bool(true))]));
        match how {
            Tk::整读丢 => {
                c.settled(from).ok_or_else(wait)?;
                done()
            }
            Tk::只读c => {
                let v = c.settled_project(from, |v| v.get("c")).ok_or_else(wait)?;
                Ok(TV::map(vec![("c", v)]))
            }
            Tk::单元丢 => {
                let n = c
                    .code(&format!("读{from}.a"), move |c| {
                        read_a(c, from).ok_or(Pend)?;
                        Ok(TV::Int(1))
                    })
                    .map_err(|_| wait())?;
                Ok(TV::map(vec![("v", n)]))
            }
            Tk::升级后单元再读 => {
                if let Some(e) = read_a(c, from).as_ref().and_then(unsure_of) {
                    duty(c, e, DutyKind::Escalate, "承接来的未决升级");
                }
                let n = c
                    .code(&format!("再读{from}.a"), move |c| {
                        read_a(c, from).ok_or(Pend)?;
                        Ok(TV::Int(1))
                    })
                    .map_err(|_| wait())?;
                Ok(TV::map(vec![("v", n)]))
            }
            Tk::单元假转交 => {
                let n = c
                    .code(&format!("假转交{from}.a"), move |c| {
                        if let Some(e) = read_a(c, from).as_ref().and_then(unsure_of) {
                            duty(c, e, DutyKind::Handoff, "单元里只记转交，值里不带");
                        }
                        Ok(TV::Int(1))
                    })
                    .map_err(|_| wait())?;
                Ok(TV::map(vec![("v", n)]))
            }
            Tk::单元转交写出 | Tk::单元转交后丢 => {
                let a = c
                    .code(&format!("转交写出{from}.a"), move |c| {
                        let v = read_a(c, from).ok_or(Pend)?;
                        if let Some(e) = unsure_of(&v) {
                            duty(c, e, DutyKind::Handoff, "单元里转交，值里带着");
                        }
                        Ok(v)
                    })
                    .map_err(|_| wait())?;
                if how == Tk::单元转交写出 {
                    Ok(TV::map(vec![("a", a)]))
                } else {
                    done()
                }
            }
            Tk::单元投影只写c | Tk::单元投影写a => {
                let v = c
                    .code(&format!("投影{from}"), move |c| {
                        let a = read_a(c, from).ok_or(Pend)?;
                        Ok(TV::map(vec![("a", a), ("c", TV::Int(1))]))
                    })
                    .map_err(|_| wait())?;
                let k = if how == Tk::单元投影写a {
                    "a"
                } else {
                    "c"
                };
                Ok(TV::map(vec![(k, v.get(k))]))
            }
            how => {
                let a = read_a(c, from).ok_or_else(wait)?;
                match (how, unsure_of(&a)) {
                    (Tk::传, _) => Ok(TV::map(vec![("a", a)])),
                    (Tk::升级, Some(e)) => {
                        duty(c, e, DutyKind::Escalate, "承接来的未决升级");
                        done()
                    }
                    (Tk::假转交, Some(e)) => {
                        duty(c, e, DutyKind::Handoff, "只记转交，值里不带");
                        done()
                    }
                    // 丢：只看它是不是未决（读原因不算处理），返回里不带它
                    (_, u) => Ok(TV::map(vec![("seen_unsure", TV::Bool(u.is_some()))])),
                }
            }
        }
    })
}

fn scene(name: &str) -> Vec<(&'static str, &'static str, Tk)> {
    match name {
        "Y丢" => vec![("Y", "X", Tk::丢)],
        "Y传" => vec![("Y", "X", Tk::传)],
        "Y升级" => vec![("Y", "X", Tk::升级)],
        "Y假转交" => vec![("Y", "X", Tk::假转交)],
        "Y整读丢" => vec![("Y", "X", Tk::整读丢)],
        "Y只读c" => vec![("Y", "X", Tk::只读c)],
        "Y单元丢" => vec![("Y", "X", Tk::单元丢)],
        "Y单元假转交" => vec![("Y", "X", Tk::单元假转交)],
        "Y单元转交写出" => vec![("Y", "X", Tk::单元转交写出)],
        "Y升级后单元再读" => vec![("Y", "X", Tk::升级后单元再读)],
        "Z丢" => vec![("Y", "X", Tk::传), ("Z", "Y", Tk::丢)],
        "Z传" => vec![("Y", "X", Tk::传), ("Z", "Y", Tk::传)],
        "扇出" => vec![("Y1", "X", Tk::升级), ("Y2", "X", Tk::丢)],
        "Y单元转交后丢" => vec![("Y", "X", Tk::单元转交后丢)],
        "Y单元投影只写c" => vec![("Y", "X", Tk::单元投影只写c)],
        "Y单元投影写a" => vec![("Y", "X", Tk::单元投影写a)],
        _ => panic!("没有这个场景 {name}"),
    }
}

const SCENES: [&str; 13] = [
    "Y丢",
    "Y传",
    "Y升级",
    "Y假转交",
    "Y整读丢",
    "Y只读c",
    "Y单元丢",
    "Y单元假转交",
    "Y单元转交写出",
    "Y升级后单元再读",
    "Z丢",
    "Z传",
    "扇出",
];

type Summary = BTreeMap<String, (usize, BTreeMap<&'static str, u32>)>;

fn takeover(
    name: &str,
    steps: &[i64],
    knobs: Knobs,
    seed: Option<u64>,
) -> (Summary, CellGraph<TV>) {
    let readers = scene(name);
    let setup = move |g: &mut CellGraph<TV>| {
        g.add_program("X", prog(take_x));
        for (id, from, how) in &readers {
            g.add_program(id, take_y(from, *how));
        }
    };
    let ev: Vec<(&str, TV)> = steps.iter().map(|v| ("s", TV::Int(*v))).collect();
    let g = run(&setup, knobs, &ev, seed, &mut half);
    let from = steps.len() as u32;
    let sum = g
        .programs()
        .iter()
        .map(|id| (id.clone(), concl_summary(&g, id, from)))
        .collect();
    (sum, g)
}

fn v(s: &Summary, id: &str) -> usize {
    s[id].0
}
fn k(s: &Summary, id: &str, kind: &str) -> u32 {
    s[id].1.get(kind).copied().unwrap_or(0)
}

#[test]
fn c1_23_s2从零_各写法的违规与去向() {
    let f = |n: &str| takeover(n, &[2], Knobs::default(), None).0;
    let x = f("Y丢");
    assert_eq!(
        (v(&x, "X"), k(&x, "X", "handoff")),
        (0, 2),
        "X 把 a、b 写进返回值"
    );
    assert_eq!(v(&x, "Y"), 1, "丢掉转交来的未决记 1 条违规");
    let s = f("Y传");
    assert_eq!((v(&s, "Y"), k(&s, "Y", "handoff")), (0, 1));
    let s = f("Y升级");
    assert_eq!((v(&s, "Y"), k(&s, "Y", "escalate")), (0, 1));
    let s = f("Y假转交");
    assert_eq!((v(&s, "Y"), k(&s, "Y", "handoff")), (1, 0));
    assert_eq!(v(&f("Y整读丢"), "Y"), 2, "整读接下全部未决");
    assert_eq!(v(&f("Y只读c"), "Y"), 0, "只读到已定的元素，不接");
    assert_eq!(v(&f("Y单元丢"), "Y"), 1, "代码单元里接下的欠账经记忆项上传");
    let s = f("Y单元假转交");
    assert_eq!(v(&s, "Y"), 1);
    assert!(s["Y"].1.is_empty(), "没有程序级去向事件：{:?}", s["Y"].1);
    let s = f("Y单元转交写出");
    assert_eq!((v(&s, "Y"), k(&s, "Y", "handoff")), (0, 1));
    let s = f("Y升级后单元再读");
    assert_eq!((v(&s, "Y"), k(&s, "Y", "escalate")), (0, 1));
    assert_eq!(v(&f("Z丢"), "Z"), 1, "两跳：Z 丢了 Y 转交来的未决");
    let s = f("Z传");
    assert_eq!((v(&s, "Z"), k(&s, "Z", "handoff")), (0, 1));
    let s = f("扇出");
    assert_eq!((v(&s, "Y1"), k(&s, "Y1", "escalate")), (0, 1));
    assert_eq!(v(&s, "Y2"), 1, "各读者各自欠，Y1 升级不替 Y2 销账");
}

#[test]
fn c1_24_s1从零_只有整读丢欠() {
    for name in SCENES {
        let (s, _) = takeover(name, &[1], Knobs::default(), None);
        assert_eq!(k(&s, "X", "handoff"), 1, "{name}");
        for id in s.keys().filter(|id| *id != "X") {
            let want = if name == "Y整读丢" { 1 } else { 0 };
            assert_eq!(v(&s, id), want, "{name} {id}");
            if name != "Y整读丢" {
                assert!(
                    s[id].1.is_empty(),
                    "{name} {id} 没有去向事件：{:?}",
                    s[id].1
                );
            }
        }
    }
}

#[test]
fn c1_25_增量等于从零_13场景两个方向() {
    let mut n = 0;
    for name in SCENES {
        for (from, to) in [(1, 2), (2, 1)] {
            let (fresh, _) = takeover(name, &[to], Knobs::default(), None);
            let (inc, _) = takeover(name, &[from, to], Knobs::default(), None);
            assert_eq!(inc, fresh, "{name} {from}→{to}");
            n += 1;
        }
    }
    assert_eq!(n, 26);
}

#[test]
fn c1_26_反证_关掉读时承接() {
    let off = Knobs {
        takeover: false,
        ..Knobs::default()
    };
    let f = |n: &str| takeover(n, &[2], off, None).0;
    assert_eq!(v(&f("Y丢"), "Y"), 0);
    assert_eq!(v(&f("Y整读丢"), "Y"), 0);
    assert_eq!(v(&f("Y单元丢"), "Y"), 0);
    assert_eq!(v(&f("Z丢"), "Z"), 0);
    assert_eq!(v(&f("扇出"), "Y2"), 0);
    assert_eq!(k(&f("Y传"), "Y", "handoff"), 0);
    assert_eq!(k(&f("Z传"), "Z", "handoff"), 0);
}

#[test]
fn c1_27_种子调度三个种子_统计与静止状态相同() {
    for name in SCENES {
        for steps in [&[2][..], &[1, 2], &[2, 1]] {
            let (s0, g0) = takeover(name, steps, Knobs::default(), None);
            for seed in [21, 22, 23] {
                let (s, g) = takeover(name, steps, Knobs::default(), Some(seed));
                assert_eq!(s, s0, "{name} {steps:?} 种子 {seed}");
                assert_eq!(finals(&g), finals(&g0), "{name} {steps:?} 种子 {seed}");
            }
        }
    }
}

/// 与原型不同、按 R2 预测的发布状态，与单元级转交的计数（预注册 4.6 末段）。
#[test]
fn c1_23b_发布状态按值里未销的未决判定_单元级转交计数() {
    let state = |g: &CellGraph<TV>, id: &str| {
        let p = g.latest(id).unwrap();
        (
            p.state,
            p.causes.iter().map(|c| c.name()).collect::<Vec<_>>(),
        )
    };
    let (_, g) = takeover("Y丢", &[2], Knobs::default(), None);
    assert_eq!(
        state(&g, "X"),
        (PState::Unsure, vec!["tie"]),
        "X 值里带着 a、b 两个已转交的未决"
    );
    assert_eq!(state(&g, "Y"), (PState::Unsure, vec!["violation"]));
    let (_, g) = takeover("Y传", &[2], Knobs::default(), None);
    assert_eq!(state(&g, "Y").0, PState::Unsure);
    let (_, g) = takeover("Z传", &[2], Knobs::default(), None);
    assert_eq!(state(&g, "Z").0, PState::Unsure);
    for n in ["Y只读c", "Y升级"] {
        let (_, g) = takeover(n, &[2], Knobs::default(), None);
        assert_eq!(state(&g, "Y").0, PState::Settled, "{n}");
    }
    let (_, g) = takeover("Y单元转交写出", &[2], Knobs::default(), None);
    let last = g.attempts("Y").iter().rev().find(|a| a.concluded).unwrap();
    assert_eq!(
        last.cell_duties,
        BTreeMap::from([(DutyKind::Handoff, 1)]),
        "单元级转交成立（原型为空）"
    );
}

#[test]
fn c1_28_29_转交要值里真带着() {
    let (s, _) = takeover("Y假转交", &[2], Knobs::default(), None);
    assert_eq!((v(&s, "Y"), k(&s, "Y", "handoff")), (1, 0), "C1-28");
    let (s, g) = takeover("Y单元假转交", &[2], Knobs::default(), None);
    assert_eq!(v(&s, "Y"), 1, "C1-29");
    let last = g.attempts("Y").iter().rev().find(|a| a.concluded).unwrap();
    assert!(
        !last.cell_duties.contains_key(&DutyKind::Handoff),
        "单元值不带，单元级转交不成立"
    );
}

#[test]
fn c1_30_单元转交后程序丢_读者从值里接下() {
    let (s, g) = takeover("Y单元转交后丢", &[2], Knobs::default(), None);
    assert_eq!(v(&s, "Y"), 1);
    let last = g.attempts("Y").iter().rev().find(|a| a.concluded).unwrap();
    assert_eq!(last.cell_duties.get(&DutyKind::Handoff), Some(&1));
    let wrong = Knobs {
        handoff_blocks_reader: true,
        ..Knobs::default()
    };
    let (s, _) = takeover("Y单元转交后丢", &[2], wrong, None);
    assert_eq!(v(&s, "Y"), 0, "反证：单元转交挡住读者，违规就丢了");
}

#[test]
fn c1_31_单元转交投影() {
    let (s, _) = takeover("Y单元投影只写c", &[2], Knobs::default(), None);
    assert_eq!(v(&s, "Y"), 1);
    let (s, _) = takeover("Y单元投影写a", &[2], Knobs::default(), None);
    assert_eq!((v(&s, "Y"), k(&s, "Y", "handoff")), (0, 1));
}
