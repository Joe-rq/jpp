//! 规划目标（步 30 / B0488；`21`:311 `tests/ablation/plan.rs`）：下游层数（K-135 层数目标、主控 Q5）、层内挑选两臂
//! （规划目标对登记顺序）、逐组费用模型。断言写死值，数见 `地基/过程记录/工程-步30-价值函数.md` §3.3。
//! B 段（价值密度）的两臂随出题线 `value.rs` 合入后加在本文件。

use jpp_effects::{ALL, EffectId, Profile, spec};
use jpp_ir::ir::{ConsumeHow, NameClass, NameTable, Program};
use jpp_ir::plan::{BudgetLeft, PendingSite, Plan, PlanCtx, PlanHooks, Selection, SiteClass};
use jpp_plan::{Hooks, Passes, plan_with};
use std::path::{Path, PathBuf};

/// 与 `jpp::names::CurrentNames` 同一张表（本 crate 不依赖外观层；与 `plan_estimate.rs` 同样照抄一份）
struct Names;

const CONSTRUCTS: &[&str] = &[
    "test",
    "select",
    "measure",
    "form",
    "fill",
    "sieve",
    "pair",
    "tally",
    "first_k",
    "iterate",
    "outcome",
    "key_of",
    "allocate",
    "unsure_bound",
    "agg",
    "repeat",
    "order",
    "element",
];

impl NameTable for Names {
    fn classify(&self, name: &str) -> NameClass {
        if let Some(id) = ALL.into_iter().find(|i| spec(*i).name == name) {
            return NameClass::Effect(id);
        }
        match name {
            "state" => NameClass::State,
            "cut" => NameClass::Cut,
            "fit" => NameClass::Fit,
            "loop" => NameClass::Loop,
            "handle" => NameClass::Handle,
            "consume" => NameClass::Consume(ConsumeHow::Consume),
            "escalate" => NameClass::Consume(ConsumeHow::Escalate),
            "literalize" => NameClass::Consume(ConsumeHow::Literalize),
            n if CONSTRUCTS.contains(&n) => NameClass::Construct,
            "map" | "filter" | "fold" => NameClass::HigherOrder,
            _ => NameClass::Plain,
        }
    }
    fn slots(&self, effect: EffectId) -> Vec<&'static str> {
        spec(effect).input_schema.iter().map(|d| d.name).collect()
    }
}

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn prog(src: &str) -> Program {
    let ast = jpp_syntax::parse(src).expect("解析");
    jpp_syntax::lower(&ast, &Names).expect("降级")
}

fn 首跑() -> PlanCtx {
    PlanCtx {
        ledger_empty: true,
        cache_off: true,
        judge_price: jpp_ir::plan::JudgePrice::Known(0.0),
    }
}

/// 各站点按登记顺序的下游层数
fn 下游(src: &str) -> Vec<u32> {
    let pl = plan_with(&prog(src), &Passes::default(), None, &首跑());
    let mut v: Vec<_> = pl
        .per_site
        .values()
        .map(|s| (s.order, s.downstream))
        .collect();
    v.sort();
    v.into_iter().map(|(_, d)| d).collect()
}

const 头: &str = r#"budget {calls: 10, cost: 1, depth: 8};
let q = test("行吗", "k");
"#;

#[test]
fn 下游层数_控制依赖逐层累加() {
    let src = format!(
        "{头}{}",
        r#"let a = cut(judge(state(mat("甲")), q));
let b = cut(judge(state(mat("乙")), q));
let r = if exit_kind(b) == "act" {
    let c = cut(judge(state(mat("丙")), q));
    if exit_kind(c) == "act" { cut(judge(state(mat("丁")), q)) } else { c }
} else { b };
{a: a, r: r}"#
    );
    assert_eq!(下游(&src), vec![0, 2, 1, 0]);
}

#[test]
fn 下游层数_穿过形参() {
    let src = format!(
        "{头}{}",
        r#"fn f(e) !{judge} { if exit_kind(e) == "act" { cut(judge(state(mat("乙")), q)) } else { e } }
let a = cut(judge(state(mat("甲")), q));
f(a)"#
    );
    assert_eq!(下游(&src), vec![1, 0]);
}

#[test]
fn 下游层数_map_各元素互不依赖() {
    let src = format!(
        "{头}{}",
        r#"map(["a", "b", "c"], fn(x) { cut(judge(state(mat(x)), q)) })"#
    );
    assert_eq!(下游(&src), vec![0]);
}

#[test]
fn 下游层数_iterate_展开到第二轮() {
    let src = format!(
        "{头}{}",
        r#"fn step(acc, i) !{judge} { exit_kind(cut(judge(state(mat(acc)), q))) }
iterate(3, "起点", step, "tokens")"#
    );
    assert_eq!(下游(&src), vec![1]);
}

#[test]
fn 下游层数_handle_的臂受出口控制() {
    let src = format!(
        "{头}{}",
        r#"let a = cut(judge(state(mat("甲")), q));
handle(a, {act: fn() { cut(judge(state(mat("乙")), q)) }, ignore: fn() { a }, unsure: fn(c) { a }})"#
    );
    assert_eq!(下游(&src), vec![1, 0]);
}

#[test]
fn 下游层数_短路右侧受左侧控制() {
    let src = format!(
        "{头}{}",
        r#"let a = cut(judge(state(mat("甲")), q));
exit_kind(a) == "act" && exit_kind(cut(judge(state(mat("乙")), q))) == "act""#
    );
    assert_eq!(下游(&src), vec![1, 0]);
}

/// 最长链与后继个数分得开：a 的出口控制两个互不依赖的判断，最长链 1（按后继个数会是 2）
#[test]
fn 下游层数_扇出取最长链不数后继() {
    let src = format!(
        "{头}{}",
        r#"let a = cut(judge(state(mat("甲")), q));
if exit_kind(a) == "act" {
    let b = cut(judge(state(mat("乙")), q));
    let c = cut(judge(state(mat("丙")), q));
    [b, c]
} else { [a] }"#
    );
    assert_eq!(下游(&src), vec![1, 0, 0]);
}

/// `pair` 调闭包时，列表实参的依赖进闭包里的判断（复核 B0488-A 缺口 2；改前为 [0, 0]）
#[test]
fn 下游层数_pair_闭包里的判断() {
    let src = format!(
        "{头}{}",
        r#"let a = cut(judge(state(mat("甲")), q));
pair([a], ["x"], fn(l, r) { exit_kind(cut(judge(state(mat(r)), q))) == "act" })"#
    );
    assert_eq!(下游(&src), vec![1, 0]);
}

#[test]
fn plan_pass_关时没有站点表() {
    let pl = plan_with(
        &prog(&format!("{头}cut(judge(state(mat(\"甲\")), q))")),
        &Passes {
            plan: false,
            ..Passes::default()
        },
        None,
        &首跑(),
    );
    assert!(pl.per_site.is_empty());
}

// ---------------------------------------------------------------- 排序两臂（纯函数）

fn 组(class: SiteClass, downstream: u32, pos: usize) -> PendingSite {
    PendingSite {
        downstream,
        ..PendingSite::basic(pos, class, 1, 0)
    }
}

fn 挑(规划目标: bool, layer: &[PendingSite], calls: u64) -> Selection {
    let mut plan = Plan::empty();
    plan.select_within = true;
    plan.critical_path = 规划目标;
    Hooks.select_within(&plan, layer, BudgetLeft { calls, usd: 1.0 })
}

// ---------------------------------------------------------------- B 段：价值函数的已知数（过程记录 §7.2、§7.4）

use jpp_ir::question_kind::Request;
use jpp_plan::value;

/// 第二版的已知数（裁定四十六；过程记录 §7.9，按定义用同一公式算好）
#[test]
fn 价值_已知数() {
    use jpp_ir::plan::Channel;
    let 模板 = Channel::Binary {
        n: [[39.0, 0.0, 0.0], [1.0, 39.0, 1.0]],
    };
    let j40 = Channel::Binary {
        n: [[21.0, 0.0, 18.0], [1.0, 17.0, 23.0]],
    };
    let cv = |r, k, ch: &Channel| value::channel_value(r, k, ch, 0, 0);
    assert!((cv(Request::Whether, 0, &模板) - 0.822475).abs() < 1e-6);
    // K 元按对称近似，三格各 +1（§10.1 第 3 条）
    assert!((cv(Request::One, 3, &模板) - 1.359672).abs() < 1e-6);
    assert!((cv(Request::One, 4, &模板) - 1.750613).abs() < 1e-6);
    assert!((cv(Request::Whether, 0, &j40) - 0.362869).abs() < 1e-6);
    assert!((value::record_discount(&j40) - 0.362869).abs() < 1e-6);
    // 本趟更新：模板前 10 条（[4,0,0] / [0,6,0]），平滑后 u₀ = 1/13、n = 13；本趟 2 条全未决 → u′ = 0.2
    let a2 = Channel::Binary {
        n: [[4.0, 0.0, 0.0], [0.0, 6.0, 0.0]],
    };
    let (u0, n) = value::unsure_stats(&a2);
    assert!((u0 - 1.0 / 13.0).abs() < 1e-9 && (n - 13.0).abs() < 1e-9);
    assert!((value::run_unsure(&a2, 2, 2) - 0.2).abs() < 1e-9);
    assert!((value::channel_value(Request::Whether, 0, &a2, 2, 2) - 0.463438).abs() < 1e-6);
    assert!((value::channel_value(Request::Whether, 0, &a2, 2, 0) - 0.540677).abs() < 1e-6);
}

#[test]
fn 切分点_对称信道一半_z信道不是一半() {
    let (u, cap) = value::channel_capacity(&[0.9, 0.1, 0.0], &[0.1, 0.9, 0.0]);
    assert!(
        (u - 0.5).abs() < 1e-6 && (cap - 0.531004).abs() < 1e-6,
        "{u} {cap}"
    );
    let (u, cap) = value::channel_capacity(&[1.0, 0.0, 0.0], &[0.5, 0.5, 0.0]);
    assert!((u - 0.6).abs() < 1e-6, "Z 信道 u* = {u}");
    assert!((cap - 1.25f64.log2()).abs() < 1e-6, "{cap}");
    let (_, cap) = value::channel_capacity(&[0.3, 0.3, 0.4], &[0.3, 0.3, 0.4]);
    assert!(cap.abs() < 1e-9, "{cap}");
}

/// 四臂排序（纯函数）：同一层四组，价值 0、0.362869、0.822475、1.750613（登记位置 0–3，下游层数全 0），剩 2
#[test]
fn 四臂_价值密度开时价值大的先发() {
    use jpp_ir::plan::{Channel, KeyCount, ValueInput};
    let 组v = |pos: usize, req: Request, k: usize, rec: Option<Channel>| PendingSite {
        keys: vec![KeyCount {
            key: format!("k{pos}"),
            seen: 0,
            unsure: 0,
            record: rec,
        }],
        values: vec![ValueInput {
            request: req,
            k,
            key: 0,
        }],
        ..PendingSite::basic(pos, R, 1, 0)
    };
    let 模板 = Channel::Binary {
        n: [[39.0, 0.0, 0.0], [1.0, 39.0, 1.0]],
    };
    let j40 = Channel::Binary {
        n: [[21.0, 0.0, 18.0], [1.0, 17.0, 23.0]],
    };
    let l = vec![
        组v(0, Request::Whether, 0, None),
        组v(1, Request::Whether, 0, Some(j40)),
        组v(2, Request::Whether, 0, Some(模板)),
        组v(3, Request::One, 4, Some(模板)),
    ];
    let 臂 = |cp: bool, vd: bool| {
        let mut plan = Plan::empty();
        plan.select_within = true;
        plan.critical_path = cp;
        plan.value_density = vd;
        Hooks.select_within(&plan, &l, BudgetLeft { calls: 2, usd: 1.0 })
    };
    for (cp, vd) in [(false, true), (true, true)] {
        let x = 臂(cp, vd);
        assert_eq!(
            (x.send.clone(), x.defer.clone()),
            (vec![3, 2], vec![1, 0]),
            "{cp} {vd}"
        );
        let want = [0.0, 0.362869, 0.822475, 1.750613];
        for (a, b) in x.value.iter().zip(want) {
            assert!((a - b).abs() < 1e-6, "{:?}", x.value);
        }
    }
    for (cp, vd) in [(false, false), (true, false)] {
        let x = 臂(cp, vd);
        assert_eq!(
            (x.send.clone(), x.defer.clone()),
            (vec![0, 1], vec![2, 3]),
            "{cp} {vd}"
        );
        assert!(x.value.is_empty());
    }
}

use SiteClass::{CrossStateSpec as S, Real as R};

#[test]
fn 两臂_关键路径上的真站点先发() {
    let l = vec![组(R, 0, 0), 组(R, 0, 1), 组(R, 1, 2), 组(S, 0, 3)];
    let 开 = 挑(true, &l, 2);
    assert_eq!((开.send, 开.defer), (vec![2, 0], vec![1, 3]));
    let 关 = 挑(false, &l, 2);
    assert_eq!((关.send, 关.defer), (vec![0, 1], vec![2, 3]));
}

#[test]
fn 两臂_类别先于下游层数() {
    let l = vec![组(S, 3, 0), 组(R, 0, 1)];
    for 开 in [true, false] {
        let x = 挑(开, &l, 1);
        assert_eq!((x.send, x.defer), (vec![1], vec![0]), "规划目标 {开}");
    }
}

#[test]
fn 两臂_下游层数全零时逐项相同() {
    // 步 22 select_within.rs 的七个层与剩余额度
    let 层们: Vec<(Vec<SiteClass>, u64)> = vec![
        (vec![S, S, R, R, S], 2),
        (vec![S, S, R, R, S], 0),
        (vec![R, S, R, R], 2),
        (vec![S, S, R, R, S], 5),
        (vec![R, R, R], 1),
        (vec![S, R], 1),
        (vec![S, S, S], 2),
    ];
    for (cs, calls) in 层们 {
        let l: Vec<PendingSite> = cs.iter().enumerate().map(|(i, &c)| 组(c, 0, i)).collect();
        assert_eq!(
            挑(true, &l, calls),
            挑(false, &l, calls),
            "{cs:?} 剩 {calls}"
        );
    }
}

// ---------------------------------------------------------------- 逐组费用模型

#[test]
fn 费用模型_按画像回归与单价() {
    let pr = Profile::load(&root().join("profiles/jev-1.13.0.json")).expect("画像");
    let pl = plan_with(
        &prog(&format!("{头}cut(judge(state(mat(\"甲\")), q))")),
        &Passes::default(),
        Some(&pr),
        &首跑(),
    );
    let m = pl.cost_model.expect("画像有单价与回归");
    let est = m.est_usd(300, 20);
    assert!(
        (est - 4.2e-8 * (271.0 + 300.0 + 0.88 * 20.0)).abs() < 1e-12,
        "{est}"
    );
    // 预注册 §3.3 写的 2.472e-5 是 588.6 × 4.2e-8 舍到四位有效数字，容差却写了 1e-12（过程记录 §4.2 第 2 条）
    assert!((est - 2.47212e-5).abs() < 1e-12, "{est}");
}

// 「有单价、缺 cost.regression」那一条要 serde_json 改画像，本 crate 没有这个依赖，放在 crates/jpp/tests/ablation/plan.rs
#[test]
fn 费用模型_画像未测即无() {
    let pr = Profile::untested();
    let pl = plan_with(
        &prog(&format!("{头}cut(judge(state(mat(\"甲\")), q))")),
        &Passes::default(),
        Some(&pr),
        &首跑(),
    );
    assert!(pl.cost_model.is_none());
    let pl = plan_with(
        &prog(&format!("{头}cut(judge(state(mat(\"甲\")), q))")),
        &Passes::default(),
        None,
        &首跑(),
    );
    assert!(pl.cost_model.is_none());
}
