//! 预注册 4.8 去向规则（C1-32 缺席类不能放弃、C1-33 缺席重试）。原型：`demos::drop_unobserved`、`absent_suite`
//! （过程记录第 23、24 节）。

use super::*;

#[derive(Clone, Copy, Debug, PartialEq)]
enum How {
    Drop,
    Escalate,
    Output,
}

fn probe(how: How) -> Rc<dyn Program<TV>> {
    prog(move |c| {
        let e = cut(c, "q").map_err(|_| wait())?;
        match how {
            How::Drop => duty(c, e, DutyKind::DropAccounted, "放弃"),
            How::Escalate => duty(c, e, DutyKind::Escalate, "升级"),
            How::Output => return Ok(TV::map(vec![("e", TV::Exit(e))])),
        }
        Ok(TV::map(vec![("done", TV::Bool(true))]))
    })
}

fn one(how: How, decide: &mut dyn FnMut(&str, &[String]) -> FlushAnswer<TV>) -> AttemptRec {
    let mut g = CellGraph::new();
    g.add_program("P", probe(how));
    g.quiesce(decide);
    g.attempts("P")
        .iter()
        .rev()
        .find(|a| a.concluded)
        .unwrap()
        .clone()
}

#[test]
fn c1_32_缺席类原因的出口不能放弃() {
    type D = fn(&str, &[String]) -> FlushAnswer<TV>;
    let absent: D = |_, _| FlushAnswer::Absent;
    let budget: D = |_, _| FlushAnswer::Unasked(UnsureCause::Budget);
    for (name, d) in [("absent", absent), ("budget", budget)] {
        let a = one(How::Drop, &mut { d });
        assert_eq!(a.violations.len(), 1, "{name}");
        assert_eq!(
            a.duty_counts().get(crate::debt::E_DROP_UNOBSERVED),
            Some(&1),
            "{name}"
        );
        assert_eq!(one(How::Escalate, &mut { d }).violations.len(), 0, "{name}");
        let o = one(How::Output, &mut { d });
        assert_eq!(
            (o.violations.len(), o.duty_counts().get("handoff").copied()),
            (0, Some(1)),
            "{name}"
        );
    }
    let t = one(How::Drop, &mut half);
    assert_eq!(
        (
            t.violations.len(),
            t.duty_counts().get("drop_accounted").copied()
        ),
        (0, Some(1)),
        "并列可以放弃"
    );
}

#[test]
fn c1_32b_占用被拒不属缺席类_可以放弃并记账() {
    let mut g = CellGraph::new();
    g.declare_shared("occupy", Reducer::Claim, Writers::Multi)
        .unwrap();
    g.add_program(
        "H",
        prog(|c| match c.claim("occupy", &["m1".to_string()]) {
            ClaimResult::Waiting => Err(wait()),
            _ => Ok(TV::Bool(true)),
        }),
    );
    g.add_program(
        "R",
        prog(|c| match c.claim("occupy", &["m1".to_string()]) {
            ClaimResult::Waiting => Err(wait()),
            ClaimResult::Granted => Ok(TV::s("占上")),
            ClaimResult::Rejected { tok, .. } => {
                c.duty(
                    Some(tok),
                    UnsureCause::ClaimConflict,
                    DutyKind::DropAccounted,
                    "放弃",
                );
                Ok(TV::s("放弃"))
            }
        }),
    );
    g.quiesce(&mut half);
    let a = g.attempts("R").iter().rev().find(|a| a.concluded).unwrap();
    assert_eq!(a.violations.len(), 0);
    assert_eq!(a.duty_counts().get("drop_accounted"), Some(&1));
}

#[test]
fn c1_33_缺席重试_账本同键两次() {
    // 先缺席后答到
    let mut n = 0;
    let mut once = |_k: &str, _r: &[String]| {
        n += 1;
        if n == 1 {
            FlushAnswer::Absent
        } else {
            FlushAnswer::Answer(TV::Reading(0.5))
        }
    };
    let mut g = CellGraph::new();
    g.add_program("P", probe(How::Output));
    g.quiesce(&mut once);
    let asked: Vec<String> = g
        .flushes
        .iter()
        .flat_map(|f| f.questions.iter().map(|q| format!("{}:{}", q.0, q.2)))
        .collect();
    assert_eq!(asked, ["q:absent", "q:answer"]);
    assert!(matches!(
        g.latest("P").unwrap().value.get("e"),
        TV::Exit(TExit::Unsure(UnsureCause::Tie, Some(_)))
    ));
    // 一直缺席：重试一次后记未问（absent）
    let mut g = CellGraph::new();
    g.add_program("P", probe(How::Output));
    g.quiesce(&mut |_k, _r| FlushAnswer::Absent);
    let asked: Vec<String> = g
        .flushes
        .iter()
        .flat_map(|f| f.questions.iter().map(|q| format!("{}:{}", q.0, q.2)))
        .collect();
    assert_eq!(asked, ["q:absent", "q:absent"]);
    assert!(matches!(
        g.latest("P").unwrap().value.get("e"),
        TV::Exit(TExit::Unsure(UnsureCause::Absent, Some(_)))
    ));
}
