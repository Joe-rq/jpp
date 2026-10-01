//! `plan` pass 的消融与估计（步 22，B0466；预注册 `地基/过程记录/工程-步22.md` §五）。
//!
//! 断言写死值。示例的数与预注册 §五·2 的表逐格对照。

use jpp_effects::{ALL, EffectId, Profile, spec};
use jpp_ir::ir::{ConsumeHow, NameClass, NameTable, Program};
use jpp_ir::plan::{Estimate, JudgePrice, Plan, PlanCtx, UnknownReason};
use jpp_plan::{Passes, plan_with};
use std::path::{Path, PathBuf};

/// 与 `jpp::names::CurrentNames` 同一张表（本 crate 不依赖外观层，测试里照抄一份）
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

fn example(name: &str) -> Program {
    let l =
        jpp_syntax::loader::load(&root().join(format!("examples/{name}.jpp"))).expect("装载示例");
    jpp_syntax::lower(&l.program, &Names).expect("降级")
}

fn 真机画像() -> Profile {
    Profile::load(&root().join("profiles/jev-1.13.0.json")).expect("画像")
}

fn 单价() -> f64 {
    真机画像().price_per_input_token().expect("画像有价格")
}

fn ctx(price: JudgePrice) -> PlanCtx {
    PlanCtx {
        ledger_empty: true,
        cache_off: true,
        judge_price: price,
    }
}

fn 固定() -> PlanCtx {
    ctx(JudgePrice::Known(0.0))
}

fn est(p: &Program, profile: Option<&Profile>, c: &PlanCtx) -> Plan {
    plan_with(p, &Passes::default(), profile, c)
}

fn codes(pl: &Plan) -> Vec<String> {
    pl.warnings.iter().map(|w| w.code.clone()).collect()
}

const 一题: &str = r#"let e = cut(judge(state(mat("甲")), test("行吗", "k")));"#;

fn 三层链(budget: &str) -> String {
    format!(
        r#"budget {budget};
let a = cut(judge(state(mat("甲")), test("行吗", "k")));
let b = cut(judge(state(mat(exit_kind(a))), test("行吗", "k")));
let c = cut(judge(state(mat(exit_kind(b))), test("行吗", "k")));
consume([a, b, c], "drop");
1
"#
    )
}

fn 三题互不依赖(budget: &str) -> String {
    format!(
        r#"budget {budget};
let a = cut(judge(state(mat("甲")), test("行吗", "k")));
let b = cut(judge(state(mat("乙")), test("行吗", "k")));
let c = cut(judge(state(mat("丙")), test("行吗", "k")));
consume([a, b, c], "drop");
1
"#
    )
}

#[test]
fn 无效应程序四项全零() {
    let pl = est(&prog("budget {calls: 0, cost: 0};\n1 + 2"), None, &固定());
    assert_eq!(pl.calls_est, Estimate::Known { lo: 0, hi: 0 });
    assert_eq!(pl.layers_est, Estimate::Known { lo: 0, hi: 0 });
    assert_eq!(pl.cost_est, Estimate::Known { lo: 0.0, hi: 0.0 });
    assert_eq!(pl.latency_est, Estimate::Known { lo: 0.0, hi: 0.0 });
    assert!(pl.rejected.is_none() && pl.warnings.is_empty());
}

#[test]
fn 三层依赖链下界三() {
    let pl = est(&prog(&三层链("{calls: 2, cost: 1}")), None, &固定());
    // 三个判断，隐含重试 2 次：上界 3 × 3
    assert_eq!(pl.calls_est, Estimate::Known { lo: 3, hi: 9 });
    assert_eq!(pl.layers_est, Estimate::Known { lo: 3, hi: 3 });
    let r = pl.rejected.expect("下界 3 > calls 2");
    assert_eq!(r.code, "E-budget-plan");
    assert!(r.message.contains("按效应都成功计"), "{}", r.message);
    assert!(
        r.message.contains("调用数下界 3 次 > 预算 calls 2"),
        "{}",
        r.message
    );
}

#[test]
fn 三个互不依赖的站点下界一() {
    let pl = est(&prog(&三题互不依赖("{calls: 1, cost: 1}")), None, &固定());
    assert_eq!(pl.calls_est, Estimate::Known { lo: 1, hi: 9 });
    assert_eq!(pl.layers_est, Estimate::Known { lo: 1, hi: 3 });
    assert!(pl.rejected.is_none());
}

#[test]
fn if分支里的站点不算必经() {
    let src = format!(
        "budget {{calls: 0, cost: 0}};\nlet x = 3;\nlet v = if x > 1 {{ {一题} consume(e, \"drop\"); 1 }} else {{ 0 }};\nv"
    );
    let pl = est(&prog(&src), None, &固定());
    assert_eq!(pl.calls_est, Estimate::Known { lo: 0, hi: 3 });
    assert!(pl.rejected.is_none());
    let s = pl.per_site.values().next().expect("一个站点");
    assert!(!s.must && s.layer_lo.is_none());
}

#[test]
fn 经形参调用不算必经且上界未知() {
    let 定义 = format!("fn ask1(m) {{ {} consume(e, \"drop\"); 1 }}\n", 一题);
    let 经形参 =
        format!("budget {{calls: 0, cost: 0}};\n{定义}fn apply(v, f) {{ f(v) }}\napply(1, ask1)");
    let pl = est(&prog(&经形参), None, &固定());
    assert_eq!(pl.calls_est, Estimate::AtLeast { lo: 0 });
    assert!(pl.rejected.is_none());
    // 对照：直接按名字调用即必经
    let 直接 = format!("budget {{calls: 0, cost: 0}};\n{定义}ask1(1)");
    let pl = est(&prog(&直接), None, &固定());
    assert_eq!(pl.calls_est, Estimate::Known { lo: 1, hi: 3 });
    assert!(pl.rejected.is_some());
}

#[test]
fn 字面列表上的map下界一上界按长度() {
    let src = r#"budget {calls: 5, cost: 0};
fn ask1(t) { let e = cut(judge(state(mat(t)), test("行吗", "k"))); consume(e, "drop"); 1 }
map(["甲", "乙", "丙", "丁"], ask1)"#;
    let pl = est(&prog(src), None, &固定());
    assert_eq!(pl.calls_est, Estimate::Known { lo: 1, hi: 12 });
    assert_eq!(pl.layers_est, Estimate::Known { lo: 1, hi: 4 });
}

#[test]
fn k093_iterate上界是n乘step加n加一乘measure() {
    let src = r#"budget {calls: 20, cost: 0, absent: {retry: 0, backoff: 0, then: "fail"}};
fn step(acc, i) { let e = cut(judge(state(mat("甲")), test("行吗", "k"))); consume(e, "drop"); acc + 1 }
fn size(acc) { let e = cut(judge(state(mat("乙")), test("大吗", "k"))); consume(e, "drop"); 10 - acc }
let r = iterate(5, 0, step, size);
r.value"#;
    let pl = est(&prog(src), None, &固定());
    // 5 × step + 6 × measure，重试 0 次
    assert_eq!(pl.calls_est, Estimate::Known { lo: 1, hi: 11 });
    assert_eq!(pl.layers_est, Estimate::Known { lo: 1, hi: 11 });
}

#[test]
fn loop上界是n乘方法体() {
    let src = r#"budget {calls: 20, cost: 0, absent: {retry: 0, backoff: 0, then: "fail"}};
loop(4, 0, fn(acc, i) { let e = cut(judge(state(mat("甲")), test("行吗", "k"))); consume(e, "drop"); acc + 1 })"#;
    let pl = est(&prog(src), None, &固定());
    assert_eq!(pl.calls_est, Estimate::Known { lo: 1, hi: 4 });
}

#[test]
fn 账本非空或开缓存下界为零不拒报w_cost() {
    let p = prog(&三层链("{calls: 2, cost: 1}"));
    for c in [
        PlanCtx {
            ledger_empty: false,
            ..固定()
        },
        PlanCtx {
            cache_off: false,
            ..固定()
        },
    ] {
        let pl = est(&p, None, &c);
        assert_eq!(pl.calls_est, Estimate::Known { lo: 0, hi: 9 });
        assert!(pl.rejected.is_none());
        assert_eq!(codes(&pl), vec!["W-cost"]);
    }
}

#[test]
fn 单价未测报w_cost_unknown不按费用拒但调用数仍拒() {
    let pl = est(
        &prog(&三题互不依赖("{calls: 5, cost: 0}")),
        None,
        &ctx(JudgePrice::Untested),
    );
    assert_eq!(
        pl.cost_est,
        Estimate::Unknown(UnknownReason::Untested(
            "cost.price_usd_per_input_token".into()
        ))
    );
    assert_eq!(codes(&pl), vec!["W-cost-unknown"]);
    assert!(pl.rejected.is_none());
    let pl = est(
        &prog(&三题互不依赖("{calls: 0, cost: 0}")),
        None,
        &ctx(JudgePrice::Untested),
    );
    assert!(
        pl.rejected
            .expect("calls 0")
            .message
            .contains("调用数下界 1 次 > 预算 calls 0")
    );
}

#[test]
fn 开关关时四项未知不拒其余条目不变() {
    let p = prog(&三层链("{calls: 2, cost: 0}"));
    let on = est(&p, None, &固定());
    let off = plan_with(
        &p,
        &Passes {
            plan: false,
            ..Passes::default()
        },
        None,
        &固定(),
    );
    let u = Estimate::Unknown(UnknownReason::NotComputed);
    assert_eq!(off.calls_est, u);
    assert_eq!(off.layers_est, u);
    assert!(off.rejected.is_none() && off.per_site.is_empty());
    assert_eq!(
        (&on.triggers, &on.lifts, &on.bodies, &on.segments),
        (&off.triggers, &off.lifts, &off.bodies, &off.segments)
    );
}

#[test]
fn 时延下界乘p95超预算即拒() {
    let profile = Profile::untested().with_latency_p95(1.0, "测试");
    let p = prog(&三层链("{calls: 10, cost: 1, latency_p95: 2.5}"));
    let pl = est(&p, Some(&profile), &固定());
    assert_eq!(pl.latency_est, Estimate::Known { lo: 3.0, hi: 3.0 });
    let r = pl.rejected.expect("3 层 × 1.0 秒 > 2.5 秒");
    assert!(r.message.contains("时延下界 3.000 秒"), "{}", r.message);
    let p = prog(&三题互不依赖(
        "{calls: 10, cost: 1, latency_p95: 2.5}",
    ));
    assert!(est(&p, Some(&profile), &固定()).rejected.is_none());
}

#[test]
fn 价格为零不拒价格为正拒cost零() {
    let p = prog(&format!(
        "budget {{calls: 4, cost: 0}};\n{一题}\nconsume(e, \"drop\");\n1"
    ));
    let pl = est(&p, None, &固定());
    assert!(pl.rejected.is_none());
    assert_eq!(pl.cost_est, Estimate::Known { lo: 0.0, hi: 0.0 });
    let pl = est(&p, None, &ctx(JudgePrice::Known(单价())));
    assert_eq!(pl.cost_est, Estimate::AtLeast { lo: 单价() });
    let r = pl.rejected.expect("费用下界 > 0");
    assert!(
        r.message.contains("费用下界") && r.message.contains("预算 cost 0"),
        "{}",
        r.message
    );
}

// ---------------------------------------------------------------- 示例（预注册 §五·2）

#[derive(Debug, PartialEq)]
struct 行 {
    calls: Estimate<u64>,
    layers: Estimate<u64>,
    cost: Estimate<f64>,
    latency: Estimate<f64>,
    rejected: bool,
    warnings: Vec<String>,
}

fn 行(p: &Program, profile: Option<&Profile>, c: &PlanCtx) -> 行 {
    let pl = est(p, profile, c);
    let warnings = codes(&pl);
    行 {
        calls: pl.calls_est,
        layers: pl.layers_est,
        cost: pl.cost_est,
        latency: pl.latency_est,
        rejected: pl.rejected.is_some(),
        warnings,
    }
}

/// 浮点比较到 1e-9 相对精度：p95 × 层数
fn 近似(a: &Estimate<f64>, b: &Estimate<f64>) -> bool {
    let eq = |x: f64, y: f64| (x - y).abs() <= 1e-9 * x.abs().max(y.abs()).max(1.0);
    match (a, b) {
        (Estimate::Known { lo, hi }, Estimate::Known { lo: l2, hi: h2 }) => {
            eq(*lo, *l2) && eq(*hi, *h2)
        }
        (Estimate::AtLeast { lo }, Estimate::AtLeast { lo: l2 }) => eq(*lo, *l2),
        _ => a == b,
    }
}

fn 核(name: &str, 场合: &str, got: 行, want: 行) {
    let ok = got.calls == want.calls
        && got.layers == want.layers
        && 近似(&got.cost, &want.cost)
        && 近似(&got.latency, &want.latency)
        && got.rejected == want.rejected
        && got.warnings == want.warnings;
    assert!(ok, "{name} {场合}\n实得 {got:?}\n预注册 {want:?}");
}

#[test]
fn 示例估计与预注册表一致() {
    use Estimate::{AtLeast, Known, Unknown};
    let 画像 = 真机画像();
    let pr = 单价();
    let t = 画像.latency_p95().expect("p95");
    let p95未测 = || Unknown(UnknownReason::Untested("concurrency.latency_s.p95".into()));
    let 真机 = ctx(JudgePrice::Known(pr));
    let 续接真机 = PlanCtx {
        ledger_empty: false,
        ..真机
    };
    let 续接固定 = PlanCtx {
        ledger_empty: false,
        ..固定()
    };
    let 零 = |w: Vec<&str>| 行 {
        calls: Known { lo: 0, hi: 0 },
        layers: Known { lo: 0, hi: 0 },
        cost: Known { lo: 0.0, hi: 0.0 },
        latency: Known { lo: 0.0, hi: 0.0 },
        rejected: false,
        warnings: w.into_iter().map(String::from).collect(),
    };
    for n in ["composition", "library-methods", "env-snake"] {
        let p = example(n);
        核(n, "①", 行(&p, None, &固定()), 零(vec![]));
        核(n, "②", 行(&p, Some(&画像), &真机), 零(vec![]));
        核(n, "③", 行(&p, Some(&画像), &续接真机), 零(vec![]));
    }
    for n in [
        "graph-components",
        "graph-matching",
        "graph-max-clique",
        "graph-max-flow",
        "graph-set-cover",
        "graph-shortest-path",
    ] {
        let p = example(n);
        let want = || 行 {
            calls: Known { lo: 0, hi: 1 },
            layers: Known { lo: 0, hi: 0 },
            cost: AtLeast { lo: 0.0 },
            latency: Known { lo: 0.0, hi: 0.0 },
            rejected: false,
            warnings: vec![],
        };
        核(n, "①", 行(&p, None, &固定()), want());
        核(n, "②", 行(&p, Some(&画像), &真机), want());
        核(n, "③", 行(&p, Some(&画像), &续接真机), want());
    }
    // 下界 lo、上界 hi（None = 未知）、② 是否被拒、③ 是否报 W-cost
    type 表行 = (&'static str, u64, Option<u64>, Option<u64>, bool, bool);
    let 表: &[表行] = &[
        ("adaptive", 0, None, None, false, false),
        ("partial", 1, None, None, true, true),
        ("lifecycle", 1, None, None, true, true),
        ("iterate", 1, None, None, false, false),
        ("sieve", 1, Some(90), Some(30), false, false),
        ("sieve-budget", 1, Some(90), Some(30), false, false),
        ("tally", 1, Some(15), Some(5), false, false),
        ("tally-budget", 1, Some(15), Some(5), false, false),
    ];
    for &(n, lo, calls_hi, layers_hi, 拒, 告) in 表 {
        let mut p = example(n);
        // 预注册表是改预算之前的示例：这三例本步改为 `cost: 1`（`21`:392 写的是 0.01，实际用 1 以保字节偏移，见 `工程-步22.md` §7.2），这里还原成 cost 0 核预测，
        // 改后的文件另在下面核「真机不拒」
        if matches!(n, "adaptive" | "partial" | "lifecycle") {
            let 改后 = 行(&p, Some(&画像), &真机);
            assert!(!改后.rejected, "{n} 改预算后真机不该被拒：{改后:?}");
            p.budget.cost = 0.0;
        }
        let calls = |lo| calls_hi.map_or(AtLeast { lo }, |hi| Known { lo, hi });
        let layers = |lo| layers_hi.map_or(AtLeast { lo }, |hi| Known { lo, hi });
        let 纯判断 = calls_hi.is_some();
        // ① 固定观察：价格 0，无画像
        核(
            n,
            "①",
            行(&p, None, &固定()),
            行 {
                calls: calls(lo),
                layers: layers(lo),
                cost: if 纯判断 {
                    Known { lo: 0.0, hi: 0.0 }
                } else {
                    AtLeast { lo: 0.0 }
                },
                latency: p95未测(),
                rejected: false,
                warnings: vec![],
            },
        );
        // ② 真机画像、空账本、无缓存
        核(
            n,
            "②",
            行(&p, Some(&画像), &真机),
            行 {
                calls: calls(lo),
                layers: layers(lo),
                cost: AtLeast { lo: lo as f64 * pr },
                latency: layers_hi.map_or(AtLeast { lo: lo as f64 * t }, |h| Known {
                    lo: lo as f64 * t,
                    hi: h as f64 * t,
                }),
                rejected: 拒,
                warnings: vec![],
            },
        );
        // ③ 续接：下界 0，不拒；真机单价下按首跑会拒的报 W-cost，固定观察不报
        核(
            n,
            "③真机",
            行(&p, Some(&画像), &续接真机),
            行 {
                calls: calls(0),
                layers: layers(0),
                cost: AtLeast { lo: 0.0 },
                latency: layers_hi.map_or(AtLeast { lo: 0.0 }, |h| Known {
                    lo: 0.0,
                    hi: h as f64 * t,
                }),
                rejected: false,
                warnings: if 告 { vec!["W-cost".into()] } else { vec![] },
            },
        );
        let 续固 = 行(&p, None, &续接固定);
        assert!(
            !续固.rejected && 续固.warnings.is_empty(),
            "{n} ③固定 {续固:?}"
        );
    }
}
