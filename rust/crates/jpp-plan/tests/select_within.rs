//! 层内挑选的排序（步 22 / B0487；`20·B43`、`20·B51` C1）：丢弃顺序是跨状态推测 → 同状态推测 → 真站点按价值密度，
//! 价值密度用登记位置占位。开关关时退回登记顺序（消融矩阵第 ⑧ 行）。数值见过程记录 §3.3。

use jpp_ir::plan::{BudgetLeft, PendingSite, Plan, PlanHooks, Selection, SiteClass};
use jpp_plan::Hooks;

fn 层(classes: &[SiteClass]) -> Vec<PendingSite> {
    classes
        .iter()
        .enumerate()
        .map(|(pos, &class)| PendingSite::basic(pos, class, 1, 0))
        .collect()
}

fn 剩(calls: u64) -> BudgetLeft {
    BudgetLeft { calls, usd: 1.0 }
}

fn 挑(开: bool, layer: &[PendingSite], left: BudgetLeft) -> Selection {
    let mut plan = Plan::empty();
    plan.select_within = 开;
    Hooks.select_within(&plan, layer, left)
}

use SiteClass::{CrossStateSpec as S, Real as R};

#[test]
fn 真站点先发_跨状态推测先让() {
    let l = 层(&[S, S, R, R, S]);
    let x = 挑(true, &l, 剩(2));
    assert_eq!((x.send, x.defer), (vec![2, 3], vec![0, 1, 4]));
}

#[test]
fn 剩零全推迟_真站点仍排在推测前() {
    let l = 层(&[S, S, R, R, S]);
    let x = 挑(true, &l, 剩(0));
    assert!(x.send.is_empty());
    assert_eq!(x.defer, vec![2, 3, 0, 1, 4]);
}

#[test]
fn 真站点装不下时按登记位置从末尾推迟() {
    let l = 层(&[R, S, R, R]);
    let x = 挑(true, &l, 剩(2));
    assert_eq!((x.send, x.defer), (vec![0, 2], vec![3, 1]));
}

#[test]
fn 装得下时只是真站点在前() {
    let l = 层(&[S, S, R, R, S]);
    let x = 挑(true, &l, 剩(5));
    assert_eq!((x.send, x.defer), (vec![2, 3, 0, 1, 4], vec![]));
}

#[test]
fn 开关关时是登记顺序从末尾切() {
    let l = 层(&[S, S, R, R, S]);
    let x = 挑(false, &l, 剩(2));
    assert_eq!((x.send, x.defer), (vec![0, 1], vec![2, 3, 4]));
    // trait 缺省实现与关臂相同（运行时的占位钩子、出题线的实现都拿到这一条）
    struct 缺省;
    impl PlanHooks for 缺省 {
        fn instantiate(
            &self,
            _: &Plan,
            _: &jpp_ir::ir::Function,
            _: &dyn jpp_ir::plan::EnvView,
        ) -> Vec<jpp_ir::plan::Target> {
            vec![]
        }
        fn speculate<'b>(
            &self,
            _: &Plan,
            _: jpp_ir::key::NodeId,
            _: &'b jpp_ir::ir::Block,
            _: &dyn jpp_ir::plan::EnvView,
        ) -> Vec<&'b jpp_ir::ir::Expr> {
            vec![]
        }
        fn segment(
            &self,
            _: &Plan,
            _: jpp_ir::key::NodeId,
            _: &jpp_ir::ir::Block,
            _: &dyn jpp_ir::plan::EnvView,
        ) -> Vec<jpp_ir::plan::Target> {
            vec![]
        }
        fn may_effect(
            &self,
            _: &jpp_ir::ir::Expr,
            _: &dyn jpp_ir::plan::EnvView,
            _: jpp_ir::plan::Reach,
        ) -> bool {
            false
        }
    }
    let mut plan = Plan::empty();
    plan.select_within = true;
    let y = 缺省.select_within(&plan, &l, 剩(2));
    assert_eq!((y.send, y.defer), (vec![0, 1], vec![2, 3, 4]));
}

#[test]
fn 不发的组不占额度() {
    // 已记缺席、不重发的组 calls = 0（运行时预判给的）：不挤掉后面的真站点
    let mut l = 层(&[R, R, R]);
    l[0].calls = 0;
    let x = 挑(true, &l, 剩(2));
    assert_eq!((x.send, x.defer), (vec![0, 1, 2], vec![]));
}

#[test]
fn 同状态推测今天不剥() {
    // 捎带同状态推测题的真站点组照样整组发出（主控 Z0209 Q3：发出前没有 usd 估计，7c 不丢免费的判断）
    let mut l = 层(&[S, R]);
    l[1].same_state_spec = 3;
    let x = 挑(true, &l, 剩(1));
    assert_eq!((x.send, x.defer), (vec![1], vec![0]));
}
