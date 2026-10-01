//! EXPLAIN 的「确认阈值」一节（Z0236，`11` §5.5）。手造 `ConfirmView`：这里钉的是 EXPLAIN 只把宿主的判定读给人看，
//! 五种结论各写成什么话，不比大小；不给判定时文本与改前逐字节相同。二进制级的两面测试在
//! `crates/jpp/tests/confirm_threshold.rs`。预注册：`地基/过程记录/工程-Z0236-确认阈值.md` §五。

use jpp_ir::plan::{Estimate, Plan};
use jpp_plan::explain::{ConfirmVerdict, ConfirmView};
use jpp_plan::{Explain, explain, explain_with};

fn plan() -> Plan {
    let mut p = Plan::empty();
    p.calls_est = Estimate::Known { lo: 1, hi: 3 };
    p.layers_est = Estimate::Known { lo: 1, hi: 1 };
    p.cost_est = Estimate::AtLeast { lo: 0.0 };
    p
}

fn view(verdict: ConfirmVerdict) -> ConfirmView {
    ConfirmView {
        threshold_usd: 0.1,
        threshold_is_default: true,
        upper_usd: Some(1.0),
        upper_from: "budget.cost",
        verdict,
        confirmed: false,
    }
}

fn text(v: &ConfirmView) -> String {
    explain_with(
        &plan(),
        &Explain {
            confirm: Some(v),
            ..Explain::default()
        },
    )
}

fn confirm_line(t: &str) -> String {
    let i = t.find("确认阈值（").expect("有确认阈值一节");
    t[i..]
        .lines()
        .nth(1)
        .expect("节里有一行")
        .trim()
        .to_string()
}

#[test]
fn over_says_threshold_upper_bound_and_needs_confirm() {
    let t = text(&view(ConfirmVerdict::Over));
    let l = confirm_line(&t);
    assert!(l.starts_with("超阈值"), "{l}");
    assert!(l.contains("费用上界 1 美元"), "{l}");
    assert!(l.contains("阈值 0.1 美元"), "{l}");
    assert!(l.contains("默认阈值"), "{l}");
    assert!(l.contains("budget cost"), "{l}");
    // C-3：带上游余额时上界是生效预算的 cost（程序声明与上游余额取小）
    assert!(l.contains("生效费用上限"), "{l}");
    assert!(l.contains("run 需要 --confirm"), "{l}");
}

#[test]
fn over_with_confirm_given_says_confirmed() {
    let mut v = view(ConfirmVerdict::Over);
    v.confirmed = true;
    v.threshold_is_default = false;
    let l = confirm_line(&text(&v));
    assert!(l.contains("已给 --confirm，放行"), "{l}");
    assert!(l.contains("--confirm-above"), "{l}");
    assert!(!l.contains("run 需要 --confirm"), "{l}");
}

#[test]
fn within_says_not_over() {
    let mut v = view(ConfirmVerdict::Within);
    v.upper_usd = Some(0.01);
    let l = confirm_line(&text(&v));
    assert!(l.starts_with("未超"), "{l}");
    assert!(l.contains("费用上界 0.01 美元"), "{l}");
}

#[test]
fn plan_source_is_named_when_the_planner_bound_is_smaller() {
    let mut v = view(ConfirmVerdict::Within);
    v.upper_from = "plan";
    v.upper_usd = Some(0.02);
    let l = confirm_line(&text(&v));
    assert!(l.contains("计划估计的费用上界"), "{l}");
    assert!(!l.contains("budget cost"), "{l}");
}

#[test]
fn unchecked_says_threshold_not_checked_and_why() {
    let mut v = view(ConfirmVerdict::Unchecked);
    v.upper_usd = None;
    v.upper_from = "";
    let l = confirm_line(&text(&v));
    assert!(l.starts_with("阈值未核"), "{l}");
    assert!(l.contains("没有单价"), "{l}");
    assert!(l.contains("放行"), "{l}");
    // 未知不写成 0
    assert!(!l.contains("费用上界 0 "), "{l}");
}

#[test]
fn exempt_cases_each_give_their_reason() {
    let l = confirm_line(&text(&view(ConfirmVerdict::ExemptReplay)));
    assert!(l.contains("重放不发调用"), "{l}");
    let l = confirm_line(&text(&view(ConfirmVerdict::ExemptNoPaidPort)));
    assert!(l.contains("没有会花钱的端口"), "{l}");
}

#[test]
fn explain_does_not_compare_it_prints_the_hosts_verdict() {
    // 自相矛盾的输入：上界 1 > 阈值 0.1，结论却写 Within。EXPLAIN 照结论写，不自己重判（比较只在宿主一处）
    let l = confirm_line(&text(&view(ConfirmVerdict::Within)));
    assert!(l.starts_with("未超"), "{l}");
}

#[test]
fn without_a_view_the_text_is_byte_identical_to_before() {
    let p = plan();
    let bare = explain(&p);
    let with_none = explain_with(&p, &Explain::default());
    assert_eq!(bare, with_none);
    assert!(!bare.contains("确认阈值"), "{bare}");
}

#[test]
fn verdict_codes_are_stable() {
    let codes: Vec<&str> = [
        ConfirmVerdict::Within,
        ConfirmVerdict::Over,
        ConfirmVerdict::Unchecked,
        ConfirmVerdict::ExemptReplay,
        ConfirmVerdict::ExemptNoPaidPort,
    ]
    .iter()
    .map(|v| v.code())
    .collect();
    assert_eq!(
        codes,
        [
            "within",
            "over",
            "unchecked",
            "exempt_replay",
            "exempt_no_paid_port"
        ]
    );
}
