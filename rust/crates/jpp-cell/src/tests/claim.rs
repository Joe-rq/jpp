//! 预注册 4.4 占用：申请—被拒—释放—重申（C1-17 至 C1-19，按主控 Q2：被拒记一笔欠账）与
//! 4.5 多写者拒取最新（C1-20 至 C1-22）。原型：`demos::item3_reapply`、`demos::flaw2`。

use super::*;

/// 被拒方怎么处理被拒的结果。
#[derive(Clone, Copy, Debug, PartialEq)]
enum OnReject {
    /// 写进返回值（转交）
    Output,
    /// 显式放弃并记账
    Drop,
    /// 什么都不做
    Ignore,
}

fn claimer(
    want: impl Fn(&mut Ctx<'_, TV>) -> Vec<String> + 'static,
    on: OnReject,
) -> Rc<dyn Program<TV>> {
    prog(move |c| {
        let group = want(c);
        match c.claim("occupy", &group) {
            ClaimResult::Waiting => Err(TV::s("等占用结果")),
            ClaimResult::Granted => Ok(TV::map(vec![("holds", TV::strs(&group))])),
            ClaimResult::Rejected { held_by, tok } => {
                let e = TExit::Unsure(UnsureCause::ClaimConflict, Some(tok));
                let mut kv = vec![("rejected_by", TV::strs(&held_by))];
                match on {
                    OnReject::Output => kv.push(("rejected", TV::Exit(e))),
                    OnReject::Drop => duty(c, e, DutyKind::DropAccounted, "占用被拒，放弃这一组"),
                    OnReject::Ignore => {}
                }
                Ok(TV::map(kv))
            }
        }
    })
}

fn claim_setup(on: OnReject) -> impl Fn(&mut CellGraph<TV>) {
    move |g| {
        g.declare_shared("occupy", Reducer::Claim, Writers::Multi)
            .unwrap();
        // 先占方 H：`h/want` 为真时占 {m1, m2}，为假时撤回（占空组）
        g.add_program(
            "H",
            claimer(
                |c| {
                    if c.source("h/want") == Some(TV::Bool(true)) {
                        vec!["m1".into(), "m2".into()]
                    } else {
                        vec![]
                    }
                },
                OnReject::Output,
            ),
        );
        // 后到方 R：一直要 {m2, m3}
        g.add_program("R", claimer(|_| vec!["m2".into(), "m3".into()], on));
    }
}

fn reapply(on: OnReject) -> CellGraph<TV> {
    run(
        &claim_setup(on),
        Knobs::default(),
        &[("h/want", TV::Bool(true)), ("h/want", TV::Bool(false))],
        None,
        &mut half,
    )
}

#[test]
fn c1_17_占用事件依次_占上_被拒_释放_重申占上() {
    for on in [OnReject::Output, OnReject::Drop, OnReject::Ignore] {
        let g = reapply(on);
        let ev: Vec<(&str, String)> = g
            .shared_events("occupy")
            .iter()
            .map(|e| (e.kind.name(), e.writer.clone()))
            .collect();
        assert_eq!(
            ev,
            [
                ("claim_granted", "H".to_string()),
                ("claim_rejected", "R".into()),
                ("claim_granted", "H".into()),
                ("claim_granted", "R".into())
            ],
            "{on:?}"
        );
        assert_eq!(g.shared_events("occupy")[2].released, ["m1", "m2"]);
        let holders: Vec<(String, String)> = g
            .shared_now("occupy")
            .holders()
            .iter()
            .map(|(m, w)| (m.clone(), w.clone()))
            .collect();
        assert_eq!(
            holders,
            [("m2".into(), "R".into()), ("m3".into(), "R".into())]
        );
        assert_eq!(
            g.latest("R").unwrap().value.get("holds"),
            TV::strs(&["m2".into(), "m3".into()])
        );
        assert_eq!(g.latest("H").unwrap().value.get("holds"), TV::List(vec![]));
    }
}

#[test]
fn c1_18_19_被拒是一笔欠账_写出_放弃_不理三种() {
    for (on, viol, kind, state, cause) in [
        (
            OnReject::Output,
            0,
            Some("handoff"),
            PState::Unsure,
            Some(UnsureCause::ClaimConflict),
        ),
        (
            OnReject::Drop,
            0,
            Some("drop_accounted"),
            PState::Settled,
            None,
        ),
        (
            OnReject::Ignore,
            1,
            None,
            PState::Unsure,
            Some(UnsureCause::Violation),
        ),
    ] {
        let g = reapply(on);
        let concluded: Vec<&AttemptRec> = g.attempts("R").iter().filter(|a| a.concluded).collect();
        let rejected = concluded[0];
        assert_eq!(rejected.violations.len(), viol, "{on:?}");
        let dc = rejected.duty_counts();
        match kind {
            Some(k) => assert_eq!(dc.get(k), Some(&1), "{on:?} {dc:?}"),
            None => assert!(dc.is_empty(), "{on:?} {dc:?}"),
        }
        if viol == 1 {
            let d = &rejected.violations[0].debt;
            assert_eq!(
                (d.bridge, d.cause),
                (Some(BridgeKind::Claim), UnsureCause::ClaimConflict)
            );
        }
        let (v, _) = rejected.publish.unwrap();
        let p = &g.published("R")[v as usize - 1];
        assert_eq!(p.state, state, "{on:?}");
        assert_eq!(p.causes.iter().next().copied(), cause, "{on:?}");
        // 重申占上以后：最后一版已修正、占着 [m2, m3]，不再欠
        let last = concluded.last().unwrap();
        assert!(last.violations.is_empty());
        assert_eq!(g.latest("R").unwrap().state, PState::Revised, "{on:?}");
    }
}

// ───────────── 多写者拒取最新（硬伤 2） ─────────────

fn folder(tag: &'static str, members: [&'static str; 3]) -> Rc<dyn Program<TV>> {
    prog(move |c| {
        if matches!(c.judge(&format!("t1.tight/{tag}")), JudgeRead::Pending) {
            return Err(TV::s("等判断"));
        }
        let m: Vec<String> = members.iter().map(|x| x.to_string()).collect();
        c.write_shared("folds", "F", TV::map(vec![("members", TV::strs(&m))]));
        Ok(TV::map(vec![("wrote", TV::strs(&m))]))
    })
}

fn flaw2(red: Reducer, seed: u64) -> CellGraph<TV> {
    let setup = move |g: &mut CellGraph<TV>| {
        g.declare_shared("folds", red, Writers::Multi).unwrap();
        g.add_program("sig4", folder("sig4", ["S00020", "S00021", "S00022"]));
        g.add_program("sig5", folder("sig5", ["S00020", "S00021", "S00038"]));
        g.add_program(
            "reader",
            prog(|c| {
                Ok(TV::map(vec![(
                    "seen",
                    TV::from_shared(c.read_shared("folds", "F")),
                )]))
            }),
        );
    };
    let mut g = CellGraph::new();
    setup(&mut g);
    settle(&mut g, Some(seed), &mut |_k, _r| {
        FlushAnswer::Answer(TV::Reading(0.0))
    });
    g
}

#[test]
fn c1_20_取最新只许单写者() {
    let mut g: CellGraph<TV> = CellGraph::new();
    assert!(
        g.declare_shared("folds", Reducer::Latest, Writers::Multi)
            .is_err()
    );
    let mut c = crate::reducer::declare::<TV>(Reducer::Latest, Writers::Single).unwrap();
    assert!(c.write("sig4", "k", TV::Int(1)).is_ok());
    assert!(c.write("sig5", "k", TV::Int(2)).is_err());
}

#[test]
fn c1_21_22_保住贡献与覆盖_换种子() {
    for (red, want_finals, want_kind) in [
        (Reducer::MergeByKey, 1, MergeKind::Conflict),
        (Reducer::Overwrite, 2, MergeKind::Overwritten),
    ] {
        let mut orders = std::collections::BTreeSet::new();
        let mut finals = std::collections::BTreeSet::new();
        for seed in 1..=16u64 {
            let g = flaw2(red, seed);
            let ev = g.shared_events("folds");
            orders.insert(ev.iter().map(|e| e.writer.clone()).collect::<Vec<_>>());
            assert!(ev.iter().any(|e| e.kind == want_kind), "{red:?} {ev:?}");
            finals.insert(g.latest("reader").unwrap().hash.clone());
        }
        assert_eq!(
            orders.len(),
            2,
            "{red:?} 交错要真的换过合并顺序：{orders:?}"
        );
        assert_eq!(finals.len(), want_finals, "{red:?}");
    }
}
