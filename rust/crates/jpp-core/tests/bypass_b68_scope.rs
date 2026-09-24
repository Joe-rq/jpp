//! B68：认证范围。线用在认证集材料范围之外时，出口照常路由，但不算放行不可逆 `do` 的可信合取项（J-08 拒）；
//! 范围内照常放行；记录没有指纹时按范围内处理。
//! 依据：B68（地基/附注/2026-09-24-探针首轮裁定.md）；`21` 步 20d-1。

mod common;
use std::cell::RefCell;

use jpp_core::effects::{CalibStore, Client, EffectError, GenResult, JudgeResult};
use jpp_core::interp::{ActionRegistry, TaintOut};
use jpp_core::ledger::Ledger;
use jpp_core::run;
use jpp_core::value::{Answer, Question, State, Taint, Value};
use jpp_core::{lower, syntax::parse};
use jpp_value::stat::ScopeRanges;
use serde_json::{json, Value as Json};

struct 桩(RefCell<u64>);
impl Client for 桩 {
    fn model_id(&self) -> String {
        "m".into()
    }
    fn judge(&mut self, _s: &State, qs: &[&Question]) -> Result<JudgeResult, EffectError> {
        *self.0.borrow_mut() += 1;
        Ok(JudgeResult {
            answers: qs.iter().map(|_| Answer::Noul(0.95)).collect(),
            tokens: 0,
            cost: 0.0,
            mode_share: vec![],
            perms: vec![],
        })
    }
    fn generate(&mut self, _p: &str, _c: &[Json], _n: usize, _r: u64) -> Result<GenResult, EffectError> {
        Err(EffectError("x".into()))
    }
    fn ask(&mut self, _s: &State, _q: &Question) -> Result<Option<Answer>, EffectError> {
        Err(EffectError("x".into()))
    }
    fn calls(&self) -> u64 {
        *self.0.borrow()
    }
}

/// 认证集：中文日常短句（与第四轮 R 题式同风格）
const 认证集: [&str; 6] = [
    "妈妈削了一个苹果，分给我们两半，酸甜正好。",
    "早高峰的地铁挤得人喘不过气，他差点坐过站。",
    "她把积蓄的一部分投进了基金，心里还是没底。",
    "周末一家三口去公园放风筝，孩子笑得很开心。",
    "医生建议他少吃油炸食品，多吃新鲜蔬菜水果。",
    "他一个人吃完了那顿火锅，手机始终没有响过。",
];

fn 跑(材料: &str, 带指纹: bool) -> Result<(Json, Vec<String>), String> {
    let src = format!(
        r#"
budget {{calls: 4, cost: 1, depth: 8}};
handle(cut(judge(state(mat({材料:?})), test("该发吗", "k"))), {{
    act: fn() {{ content(do("发邮件", [], 0)) }},
    ignore: fn() {{ "没发" }},
    unsure: fn(u) {{ consume(u, "drop"); "没发" }}}})
"#
    );
    let program = lower(&parse(&src).expect("解析")).expect("lower");
    let mut calib = CalibStore::new();
    common::certified(&mut calib, "k", 0.8, 0.2, 50);
    if 带指纹 {
        let r = calib.records.get_mut("k").unwrap();
        r.scope = Some(jpp_core::effects::CalibScope {
            batches: vec!["t".into()],
            sources: Default::default(),
            note: String::new(),
            fingerprint: ScopeRanges::from_texts(认证集, (0.0, 1.0), None),
        });
    }
    let mut a = ActionRegistry::new();
    a.register("发邮件", 0.0, false, TaintOut::Trusted, |_| Ok(Value::Text("已发".into(), Taint::Trusted.into())));
    let mut l = Ledger::new();
    run(&program, &mut 桩(RefCell::new(0)), &calib, &a, &mut l)
        .map(|o| (o.value_json(), o.trace.warnings.clone()))
        .map_err(|e| e.render())
}

const 范围外: &str = "ERROR: build failed at src/main.rs:42\nexpected `;`, found `}`\nerror: aborting due to 1 previous error";

/// 范围外：act 臂里的不可逆 do 被 J-08 拒，并报 W-calib-scope。
#[test]
fn 范围外的act不放行不可逆do() {
    let e = 跑(范围外, true).expect_err("范围外的线不该放行不可逆 do");
    assert!(e.contains("J-08"), "{e}");
}

/// 范围内：照常放行，不报 W-calib-scope。
#[test]
fn 范围内照常放行() {
    let (v, w) = 跑("他把旧自行车卖了，打算换一辆折叠车上下班。", true).expect("范围内该放行");
    assert_eq!(v, json!("已发"));
    assert!(!w.iter().any(|x| x.starts_with("W-calib-scope")), "{w:?}");
}

/// 记录没有指纹：缺指纹不是范围外，照常放行（旧记录的行为不变）。
#[test]
fn 没有指纹按范围内处理() {
    let (v, w) = 跑(范围外, false).expect("没有指纹的记录按范围内处理");
    assert_eq!(v, json!("已发"));
    assert!(!w.iter().any(|x| x.starts_with("W-calib-scope")), "{w:?}");
}
