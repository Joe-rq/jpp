//! EXPLAIN 文本（Z0190 后一半，`20` §2.3 `explain(plan) -> String`）。手造 `Plan` 与最小 `Program`，
//! 不降级源码：这里钉的是「三种估计形状各写成什么话、与预算比对的五种状态、没有估计的项如实标未估、
//! 站点行的层与调用数」。示例级的输出在 `crates/jpp/tests/explain_cli.rs`。
//! 预注册：`地基/过程记录/工程-Z0190-EXPLAIN.md` §二。

use jpp_effects::{ALL, EffectId, spec};
use jpp_ir::ir::{
    AbsentPolicy, Block, Budget, EntryDecl, IR_VERSION, IrDiag, Program, SiteInfo, SiteKind,
    SiteTable, Span,
};
use jpp_ir::key::{NodeId, SiteId};
use jpp_ir::plan::{Estimate, JudgePrice, Plan, PlanCtx, SitePlan, UnknownReason};
use jpp_plan::explain::{Verdict, compare, site_calls_hi};
use jpp_plan::{Explain, Located, explain, explain_with};

fn effect(name: &str) -> EffectId {
    ALL.into_iter()
        .find(|i| spec(*i).name == name)
        .expect("效应名")
}

fn known_plan(calls: (u64, u64), layers: (u64, u64)) -> Plan {
    let mut p = Plan::empty();
    p.calls_est = Estimate::Known {
        lo: calls.0,
        hi: calls.1,
    };
    p.layers_est = Estimate::Known {
        lo: layers.0,
        hi: layers.1,
    };
    p.cost_est = Estimate::Known { lo: 0.0, hi: 0.0 };
    p.latency_est = Estimate::Unknown(UnknownReason::Untested("concurrency.latency_s.p95".into()));
    p
}

fn line_of<'a>(text: &'a str, head: &str) -> &'a str {
    text.lines()
        .find(|l| l.trim_start().starts_with(head))
        .unwrap_or_else(|| panic!("找不到以「{head}」开头的行：\n{text}"))
}

fn budget(calls: u64) -> Budget {
    Budget {
        calls,
        cost: 0.01,
        depth: None,
        escalate: None,
        unsure: None,
        absent: None,
        latency_p95: None,
    }
}

fn program(b: Budget, sites: Vec<SiteInfo>) -> Program {
    Program {
        version: IR_VERSION,
        budget: b,
        body: Block {
            statements: vec![],
            result: None,
            span: Span::default(),
        },
        sites: SiteTable { sites },
        span: Span::default(),
        entry: EntryDecl::default(),
        unsure_default_sites: Default::default(),
        site_keys: Default::default(),
    }
}

fn site(id: u32, kind: SiteKind, start: usize) -> SiteInfo {
    SiteInfo {
        id: SiteId(id),
        node: NodeId(id),
        kind,
        span: Span::new(start, start + 5),
        function: None,
        enclosing: None,
    }
}

fn site_plan(order: u32, must: bool, layer: Option<u32>, execs: Option<u64>) -> SitePlan {
    SitePlan {
        order,
        must,
        layer_lo: layer,
        execs_hi: execs,
        downstream: 0,
    }
}

#[test]
fn explain_known_range_prints_lo_and_hi() {
    let t = explain(&known_plan((1, 90), (1, 30)));
    assert!(line_of(&t, "调用数").contains("1 至 90 次"), "{t}");
    assert!(line_of(&t, "判断层数").contains("1 至 30 层"), "{t}");
    // 上下界相等时只写一个数
    let t = explain(&known_plan((3, 3), (3, 3)));
    assert!(line_of(&t, "调用数").contains("3 次"), "{t}");
    assert!(!line_of(&t, "调用数").contains("至"), "{t}");
}

#[test]
fn explain_at_least_says_upper_unknown_and_never_invents_a_ceiling() {
    let mut p = known_plan((1, 90), (1, 30));
    p.calls_est = Estimate::AtLeast { lo: 1 };
    p.layers_est = Estimate::AtLeast { lo: 0 };
    let t = explain(&p);
    let calls = line_of(&t, "调用数");
    assert!(calls.contains("至少 1 次，上界未知"), "{t}");
    assert!(!calls.contains(" 至 "), "只有下界时不许写出区间：{calls}");
    assert!(
        line_of(&t, "判断层数").contains("至少 0 层，上界未知"),
        "{t}"
    );
}

#[test]
fn explain_unknown_prints_the_reason_never_zero() {
    let cases = [
        (
            UnknownReason::Untested("concurrency.latency_s.p95".into()),
            "画像字段 concurrency.latency_s.p95 未测",
        ),
        (UnknownReason::PriceNotGiven, "宿主没有给判断器单价"),
        (UnknownReason::NotComputed, "plan pass 关着，没有计算"),
        (UnknownReason::TooComplex, "程序太大，分析在访问上限处放弃"),
    ];
    for (reason, want) in cases {
        let mut p = known_plan((1, 2), (1, 2));
        p.latency_est = Estimate::Unknown(reason.clone());
        p.cost_est = Estimate::Unknown(reason);
        let t = explain(&p);
        for head in ["时延", "费用"] {
            let l = line_of(&t, head);
            assert!(l.contains("未估"), "{l}");
            assert!(l.contains(want), "{l}");
            // 未估不写数
            assert!(!l.contains("0 秒") && !l.contains("0 美元"), "{l}");
        }
    }
}

#[test]
fn explain_unestimated_section_is_derived_from_the_plan() {
    // 全部已知、没有站点：只剩 Plan 没有的 unsure_bound
    let t = explain(&known_plan((1, 2), (1, 2)));
    let 节 = &t[t.find("未估（由 Plan 推出，不写 0）").expect("有未估节")..];
    assert!(
        节.contains("unsure 上界：Plan 没有这个字段，没有计算"),
        "{t}"
    );
    assert!(
        !节.contains("每个站点的下沉"),
        "没有站点就没有站点字段可说：{t}"
    );
    // 估计里的 Unknown 与只有下界的项进节，原因原样
    let mut p = known_plan((1, 2), (1, 2));
    p.calls_est = Estimate::AtLeast { lo: 1 };
    p.cost_est = Estimate::Unknown(UnknownReason::PriceNotGiven);
    let t = explain(&p);
    let 节 = &t[t.find("未估（由 Plan 推出，不写 0）").unwrap()..];
    assert!(节.contains("调用数：上界未估（只有下界 1）"), "{t}");
    assert!(节.contains("费用：宿主没有给判断器单价"), "{t}");
    assert!(
        节.contains("时延：画像字段 concurrency.latency_s.p95 未测"),
        "{t}"
    );
    assert!(!节.contains("判断层数："), "已知的项不进未估节：{t}");
    // 站点：层未估与次数未知按 Plan 里的站点数出；SitePlan 没有的字段只在有站点时说
    let mut p = known_plan((1, 2), (1, 2));
    p.per_site
        .insert(SiteId(0), site_plan(0, true, Some(1), Some(1)));
    p.per_site
        .insert(SiteId(1), site_plan(1, false, None, Some(1)));
    p.per_site
        .insert(SiteId(2), site_plan(2, false, None, None));
    let t = explain(&p);
    assert!(
        t.contains("站点：2 个站点的层未估、1 个站点的执行次数未知（见上表）"),
        "{t}"
    );
    assert!(
        t.contains("每个站点的下沉、裂变块数、预估 token、$、s：SitePlan 没有这些字段，没有估计"),
        "{t}"
    );
    // 旧的固定清单没了
    assert!(!t.contains("物理形式的选择 pass 尚未实现"), "{t}");
}

#[test]
fn explain_budget_comparison_has_five_states() {
    // 直接核比对函数：五种结论加「没设预算」
    assert_eq!(compare(Some(1.0), Some(5.0), Some(12.0)), Verdict::Within);
    assert_eq!(
        compare(Some(1.0), Some(90.0), Some(12.0)),
        Verdict::UpperOver
    );
    assert_eq!(
        compare(Some(15.0), Some(90.0), Some(12.0)),
        Verdict::LowerOver
    );
    assert_eq!(compare(Some(1.0), None, Some(12.0)), Verdict::UpperUnknown);
    assert_eq!(compare(None, None, Some(12.0)), Verdict::NotEstimated);
    assert_eq!(compare(Some(1.0), Some(2.0), None), Verdict::NoBudget);
    // 下界恰等于预算不算超
    assert_eq!(compare(Some(12.0), Some(12.0), Some(12.0)), Verdict::Within);
    // 文本里各写一句
    let prog = program(budget(12), vec![]);
    let cx = Explain {
        program: Some(&prog),
        ..Explain::default()
    };
    let cases: [(Estimate<u64>, &str); 5] = [
        (Estimate::Known { lo: 1, hi: 5 }, "上界在预算内"),
        (Estimate::Known { lo: 1, hi: 90 }, "上界超出预算，下界没超"),
        (Estimate::Known { lo: 15, hi: 90 }, "下界已超预算：一定超"),
        (Estimate::AtLeast { lo: 1 }, "上界未知，不能保证在预算内"),
        (
            Estimate::Unknown(UnknownReason::NotComputed),
            "未估，不能核",
        ),
    ];
    for (est, want) in cases {
        let mut p = known_plan((1, 2), (1, 2));
        p.calls_est = est;
        let t = explain_with(&p, &cx);
        assert!(
            t.contains("预算：calls 12 · cost 0.01 美元 · latency_p95 未设"),
            "{t}"
        );
        assert!(line_of(&t, "调用数").contains(want), "{t}");
        assert!(line_of(&t, "调用数").contains("预算 calls 12"), "{t}");
    }
    // 预算行列出 escalate 与 unsure，并说明计划期不比对；时延一行说明不含排队
    let mut b = budget(12);
    b.escalate = Some(4);
    b.unsure = Some(0.25);
    let prog2 = program(b, vec![]);
    let t = explain_with(
        &known_plan((1, 2), (1, 2)),
        &Explain {
            program: Some(&prog2),
            ..Explain::default()
        },
    );
    assert!(
        t.contains("escalate 4 · unsure 0.25（escalate、unsure 计划期不比对）"),
        "{t}"
    );
    let t = explain_with(&known_plan((1, 2), (1, 2)), &cx);
    assert!(t.contains("escalate 未设 · unsure 未设"), "{t}");
    assert!(
        line_of(&t, "时延").contains("时延（层数 × 画像 p95，不含排队）"),
        "{t}"
    );
    // 时延预算没设：写「预算 latency_p95 未设」
    let t = explain_with(&known_plan((1, 2), (1, 2)), &cx);
    assert!(line_of(&t, "时延").contains("预算 latency_p95 未设"), "{t}");
}

#[test]
fn explain_shows_rejection_and_warnings_verbatim() {
    let mut p = known_plan((3, 3), (3, 3));
    p.rejected = Some(IrDiag::new(
        "E-budget-plan",
        "计划期拒绝（B42，J-07b）：调用数下界 3 次 > 预算 calls 2。修法：放宽 budget",
        Span::default(),
    ));
    p.warnings.push(IrDiag::new(
        "W-cost-unknown",
        "W-cost-unknown: 画像没有 cost.price_usd_per_input_token",
        Span::default(),
    ));
    let t = explain(&p);
    assert!(
        t.contains("拒绝：E-budget-plan：计划期拒绝（B42，J-07b）：调用数下界 3 次 > 预算 calls 2。修法：放宽 budget"),
        "{t}"
    );
    // 报文自带的「编号: 」前缀不重复
    assert!(
        t.contains("告警：W-cost-unknown：画像没有 cost.price_usd_per_input_token"),
        "{t}"
    );
    assert!(!t.contains("W-cost-unknown：W-cost-unknown"), "{t}");
    // 没有拒绝与告警时明说「无」
    let t = explain(&known_plan((1, 2), (1, 2)));
    assert!(t.contains("拒绝：无") && t.contains("告警：无"), "{t}");
}

#[test]
fn bare_explain_signature_is_usable_alone() {
    // 设计签名 `explain(plan) -> String` 单用：四项估计、告警与拒绝、未估节都在，不依赖预算与源码
    let mut p = known_plan((1, 90), (1, 30));
    p.cost_est = Estimate::AtLeast { lo: 4.2e-8 };
    p.latency_est = Estimate::Known {
        lo: 1.026,
        hi: 36.936,
    };
    p.rejected = Some(IrDiag::new("E-budget-plan", "拒绝的原因", Span::default()));
    p.warnings
        .push(IrDiag::new("W-cost", "W-cost: 只告警", Span::default()));
    p.per_site
        .insert(SiteId(7), site_plan(0, true, Some(1), Some(30)));
    let t = explain(&p);
    assert!(t.starts_with("EXPLAIN"), "{t}");
    assert!(line_of(&t, "调用数").contains("1 至 90 次"), "{t}");
    assert!(line_of(&t, "判断层数").contains("1 至 30 层"), "{t}");
    assert!(
        line_of(&t, "费用").contains("至少 4.2e-8 美元，上界未知"),
        "{t}"
    );
    assert!(line_of(&t, "时延").contains("1.026 至 36.936 秒"), "{t}");
    assert!(t.contains("拒绝：E-budget-plan：拒绝的原因"), "{t}");
    assert!(t.contains("告警：W-cost：只告警"), "{t}");
    assert!(t.contains("未估（由 Plan 推出，不写 0）"), "{t}");
    // 没有 Program 时站点只有编号、层与执行次数，没有调用数（不知道种类就不猜）
    assert!(t.contains("站点 #7"), "{t}");
    assert!(t.contains("必经 · 层 ≥ 1 · 最多执行 30 次"), "{t}");
    // 没有预算比对、没有口径行
    assert!(!t.contains("预算：") && !t.contains("口径："), "{t}");
}

#[test]
fn explain_sites_sorted_by_source_position_when_program_given() {
    let sites = vec![
        site(0, SiteKind::Effect(effect("judge")), 50),
        site(1, SiteKind::Construct("sieve".into()), 10),
        site(2, SiteKind::Effect(effect("gen")), 30),
    ];
    let prog = program(budget(100), sites);
    let mut p = known_plan((1, 100), (1, 3));
    p.per_site
        .insert(SiteId(0), site_plan(0, true, Some(2), Some(2)));
    p.per_site
        .insert(SiteId(1), site_plan(1, true, Some(1), Some(30)));
    p.per_site
        .insert(SiteId(2), site_plan(2, false, None, Some(1)));
    let locate = |s: Span| {
        Some(Located {
            place: format!("t.jpp:{}", s.start),
            source: format!("源码@{}", s.start),
        })
    };
    let cx = Explain {
        program: Some(&prog),
        locate: Some(&locate),
        ..Explain::default()
    };
    let t = explain_with(&p, &cx);
    let at = |s: &str| t.find(s).unwrap_or_else(|| panic!("缺 {s}：\n{t}"));
    assert!(
        at("t.jpp:10") < at("t.jpp:30") && at("t.jpp:30") < at("t.jpp:50"),
        "{t}"
    );
    // 站点行：种类、源码行、层、调用数（含重试：没声明 absent 时每次判断发 1 + 2 = 3 次）
    assert!(t.contains("t.jpp:10  sieve  源码@10"), "{t}");
    assert!(
        t.contains("必经 · 层 ≥ 1 · 最多 90 次调用（执行 30 次）"),
        "{t}"
    );
    assert!(t.contains("t.jpp:50  judge  源码@50"), "{t}");
    assert!(
        t.contains("必经 · 层 ≥ 2 · 最多 6 次调用（执行 2 次）"),
        "{t}"
    );
    // gen 每次一次调用，不计重试；不是读数站点，规划器没给它记必经与层，不能写成「非必经」
    assert!(t.contains("t.jpp:30  gen  源码@30"), "{t}");
    assert!(
        t.contains("不占判断层 · 是否必经规划器未记录 · 最多 1 次调用（执行 1 次）"),
        "{t}"
    );
}

#[test]
fn explain_non_must_site_says_layer_not_estimated() {
    let prog = program(
        budget(10),
        vec![site(0, SiteKind::Effect(effect("judge")), 0)],
    );
    let mut p = known_plan((0, 3), (0, 1));
    p.per_site
        .insert(SiteId(0), site_plan(0, false, None, Some(1)));
    let t = explain_with(
        &p,
        &Explain {
            program: Some(&prog),
            ..Explain::default()
        },
    );
    assert!(t.contains("非必经 · 层未估"), "{t}");
    assert!(!t.contains("层 ≥"), "{t}");
    // 执行次数未知：写「调用数上界未知」，不写数
    p.per_site
        .insert(SiteId(0), site_plan(0, false, None, None));
    let t = explain_with(
        &p,
        &Explain {
            program: Some(&prog),
            ..Explain::default()
        },
    );
    assert!(t.contains("非必经 · 层未估 · 调用数上界未知"), "{t}");
}

#[test]
fn explain_site_calls_use_declared_retry() {
    // 作者声明 `absent.retry = 0` 时每次判断只发 1 次；声明 4 次重试时发 5 次
    let mk = |retry| {
        let mut b = budget(100);
        b.absent = Some(AbsentPolicy {
            retry,
            backoff: 0.0,
            then: "fail".into(),
            breaker: 1,
        });
        program(b, vec![site(0, SiteKind::Effect(effect("judge")), 0)])
    };
    let mut p = known_plan((1, 2), (1, 2));
    p.per_site
        .insert(SiteId(0), site_plan(0, true, Some(1), Some(2)));
    assert_eq!(site_calls_hi(&p, &mk(0))[&SiteId(0)], Some(2));
    assert_eq!(site_calls_hi(&p, &mk(4))[&SiteId(0)], Some(10));
}

#[test]
fn explain_is_deterministic_and_does_not_mutate_plan() {
    let mut p = known_plan((1, 90), (1, 30));
    p.per_site
        .insert(SiteId(0), site_plan(0, true, Some(1), Some(30)));
    let before = p.clone();
    let a = explain(&p);
    let b = explain(&p);
    assert_eq!(a, b);
    assert_eq!(p, before);
}

#[test]
fn explain_basis_line_says_what_the_estimate_assumed() {
    let with = |ledger_empty, cache_off, judge_price| {
        let ctx = PlanCtx {
            ledger_empty,
            cache_off,
            judge_price,
        };
        explain_with(
            &known_plan((1, 2), (1, 2)),
            &Explain {
                ctx: Some(&ctx),
                ..Explain::default()
            },
        )
    };
    let t = with(true, true, JudgePrice::Known(0.0));
    assert!(
        t.contains("口径：账本为空、缓存关闭（首跑）；判断器单价 0（固定观察，不付费）"),
        "{t}"
    );
    let t = with(false, true, JudgePrice::Untested);
    assert!(
        t.contains("账本非空（续接或重放）") && t.contains("下界记 0"),
        "{t}"
    );
    assert!(t.contains("画像没有判断器单价"), "{t}");
    let t = with(true, false, JudgePrice::NotGiven);
    assert!(
        t.contains("开了跨运行缓存") && t.contains("没有给判断器单价"),
        "{t}"
    );
    let t = with(true, true, JudgePrice::Known(4.2e-8));
    assert!(t.contains("判断器单价 4.2e-8 美元每 input token"), "{t}");
}

#[test]
fn explain_says_when_sites_cannot_be_resolved() {
    // 调用数只有下界（上界未知）：表空时写没解析出来，不写「没有会发调用的站点」
    let mut p = known_plan((0, 0), (0, 0));
    p.calls_est = Estimate::AtLeast { lo: 0 };
    let t = explain(&p);
    assert!(t.contains("逐站点信息未估：规划器没有解析出站点"), "{t}");
    assert!(!t.contains("没有会发调用的站点"), "{t}");
    // 表非空：写条件式，两种原因都列，不断言另有站点
    p.per_site
        .insert(SiteId(1), site_plan(0, true, Some(1), Some(1)));
    let t = explain(&p);
    assert!(t.contains("可能另有站点在未知调用"), "{t}");
    assert!(t.contains("也可能只是上面某个站点的执行次数未知"), "{t}");
    // 整程序没有效应（上界 0）：如实写没有站点，没有「未估」
    let t = explain(&known_plan((0, 0), (0, 0)));
    assert!(t.contains("没有会发调用的站点"), "{t}");
    assert!(!t.contains("逐站点信息未估"), "{t}");
    // 有站点但上界已知：不加「整程序调用数上界未知」那句
    let mut p = known_plan((1, 3), (1, 1));
    p.per_site
        .insert(SiteId(1), site_plan(0, true, Some(1), Some(1)));
    assert!(!explain(&p).contains("整程序调用数上界未知"));
}

#[test]
fn explain_empty_site_table_with_unknown_estimate_says_not_estimated_not_none() {
    // 程序太大、分析在访问上限处放弃时（`Unknown(TooComplex)`）`per_site` 没写：不能显示成「没有站点」
    let mut p = Plan::empty();
    p.calls_est = Estimate::Unknown(UnknownReason::TooComplex);
    p.layers_est = Estimate::Unknown(UnknownReason::TooComplex);
    let t = explain(&p);
    assert!(
        t.contains("逐站点信息未估：程序太大，分析在访问上限处放弃"),
        "{t}"
    );
    assert!(!t.contains("没有会发调用的站点"), "{t}");
    // plan pass 关着（`NotComputed`）同理
    let t = explain(&Plan::empty());
    assert!(
        t.contains("逐站点信息未估：plan pass 关着，没有计算"),
        "{t}"
    );
    assert!(!t.contains("没有会发调用的站点"), "{t}");
}

#[test]
fn explain_site_with_unknown_execs_makes_the_conditional_note_not_a_claim() {
    // 表里有站点、只是它的执行次数未知，整程序上界因此未知：不断言「另有站点」
    let prog = program(
        budget(10),
        vec![site(0, SiteKind::Effect(effect("judge")), 0)],
    );
    let mut p = known_plan((1, 3), (1, 1));
    p.calls_est = Estimate::AtLeast { lo: 1 };
    p.per_site
        .insert(SiteId(0), site_plan(0, false, None, None));
    let t = explain_with(
        &p,
        &Explain {
            program: Some(&prog),
            ..Explain::default()
        },
    );
    assert!(t.contains("调用数上界未知"), "{t}");
    assert!(t.contains("可能另有站点"), "{t}");
    assert!(t.contains("也可能只是上面某个站点的执行次数未知"), "{t}");
    assert!(!t.contains("规划器没有解析出站点"), "{t}");
}

/// 站点分类只在 `passes/plan.rs` 一份；这里对每种站点种类断言 EXPLAIN 读到的分类与每次执行的发出数
/// （G2、G3）。`order`/`agg`/`repeat`/`allocate` 是规划器眼里「会发调用但口径未建」的构造。
#[test]
fn explain_site_classes_follow_the_planner_including_opaque_and_consume_kinds() {
    use jpp_ir::ir::ConsumeHow;
    use jpp_plan::explain::{class_name, site_rows};
    let kinds: Vec<(SiteKind, &str, Option<u64>)> = vec![
        // (种类, 分类名, 每次执行发出数：以 retry=4（attempts=5）计)
        (SiteKind::Effect(effect("judge")), "reading", Some(5)),
        (SiteKind::Effect(effect("gen")), "call", Some(1)),
        (SiteKind::Effect(effect("do")), "call", Some(1)),
        (SiteKind::Effect(effect("ask")), "call", Some(1)),
        (SiteKind::Construct("sieve".into()), "reading", Some(5)),
        (SiteKind::Construct("order".into()), "opaque", None),
        (SiteKind::Construct("agg".into()), "opaque", None),
        (SiteKind::Construct("repeat".into()), "opaque", None),
        (SiteKind::Construct("allocate".into()), "opaque", None),
        (
            SiteKind::Consume(ConsumeHow::Literalize),
            "reading",
            Some(5),
        ),
        (SiteKind::Consume(ConsumeHow::Escalate), "call", Some(1)),
        (SiteKind::Consume(ConsumeHow::Consume), "no_call", Some(0)),
        (SiteKind::Construct("tally".into()), "other", None),
    ];
    let mut b = budget(100);
    b.absent = Some(AbsentPolicy {
        retry: 4,
        backoff: 0.0,
        then: "fail".into(),
        breaker: 1,
    });
    let sites: Vec<SiteInfo> = kinds
        .iter()
        .enumerate()
        .map(|(i, (k, _, _))| site(i as u32, k.clone(), i * 10))
        .collect();
    let prog = program(b, sites);
    let mut p = known_plan((1, 100), (1, 10));
    for i in 0..kinds.len() {
        p.per_site
            .insert(SiteId(i as u32), site_plan(i as u32, false, None, Some(2)));
    }
    let rows = site_rows(&p, Some(&prog));
    assert_eq!(rows.len(), kinds.len());
    for (row, (k, name, per)) in rows.iter().zip(&kinds) {
        assert_eq!(class_name(row.class.unwrap()), *name, "{k:?}");
        assert_eq!(row.calls_hi, per.map(|x| x * 2), "{k:?}：执行 2 次");
    }
    // 声明 retry = 0：读数类每次 1 次；Literalize 同
    let mut b0 = budget(100);
    b0.absent = Some(AbsentPolicy {
        retry: 0,
        backoff: 0.0,
        then: "fail".into(),
        breaker: 1,
    });
    let prog0 = program(
        b0,
        vec![site(0, SiteKind::Consume(ConsumeHow::Literalize), 0)],
    );
    let mut p0 = known_plan((1, 2), (1, 2));
    p0.per_site
        .insert(SiteId(0), site_plan(0, true, Some(1), Some(2)));
    assert_eq!(site_calls_hi(&p0, &prog0)[&SiteId(0)], Some(2));
    // 文本：不透明站点写「会发调用 · 次数与层口径未建」，不写「不占判断层」
    let t = explain_with(
        &p,
        &Explain {
            program: Some(&prog),
            ..Explain::default()
        },
    );
    assert_eq!(
        t.matches("会发调用 · 次数与层口径未建").count(),
        4,
        "order/agg/repeat/allocate 各一行：\n{t}"
    );
    // gen/do/ask/escalate/consume 与 tally 站点写「不占判断层」，共 6 行（tally 是 other）
    assert_eq!(
        t.matches("不占判断层 · 是否必经规划器未记录").count(),
        6,
        "{t}"
    );
}
