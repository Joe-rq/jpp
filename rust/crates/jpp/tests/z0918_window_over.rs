//! Z0918（裁定七十三 (3)，推进 B0672）：超窗次数进报告计数字段——对象槽单段超窗、语境槽超窗、同材料一次请求超窗分开计，
//! 与 `W-window` 告警同一判据。画像测过窗口时 `Outcome.window_over` 是 `{text, ctx, group}`，没测时为 `Null`。
use jpp::effects::{CalibStore, FnPort, JudgeResult, Ports};
use jpp::interp::CompanionMode;
use jpp::ledger::Ledger;
use jpp::value::Answer;
use jpp::{ActionRegistry, EntryArgs, Outcome, Session, lower, syntax::parse};
use serde_json::{Value as Json, json};

fn 跑(src: &str, calib: CalibStore) -> Outcome {
    let program = lower(&parse(src).expect("解析")).expect("lower");
    let ports = Ports::new().with(FnPort::judge("fixed-0", |_s, qs| {
        Ok(JudgeResult {
            answers: qs.iter().map(|_| Answer::Noul(0.9)).collect(),
            tokens: 0,
            cost: 0.0,
            mode_share: vec![],
            perms: vec![],
            confidence: vec![],
        })
    }));
    let mut ledger = Ledger::new();
    let a = ActionRegistry::new();
    Session::new(ports, &calib, &a)
        .with_companions(CompanionMode::Off)
        .run(&program, &EntryArgs::default(), &mut ledger)
        .unwrap_or_else(|e| panic!("运行失败：{}", e.render()))
}

fn 带画像() -> CalibStore {
    let p = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../profiles/jev-1.13.0.json");
    let j: Json = serde_json::from_str(&std::fs::read_to_string(p).unwrap()).unwrap();
    let mut calib = CalibStore::new();
    calib.profile = jpp::effects::Profile::from_json(&j).unwrap();
    calib
}

fn 程序(on_words: usize, ctx_words: usize, n: usize) -> String {
    let on = "word ".repeat(on_words);
    let ctx = "context ".repeat(ctx_words);
    format!(
        "budget {{calls: 50, cost: 0, depth: 8}};\n\
         map(range(0, {n}), fn(i) {{ cut(judge(state(mat(\"{on}\" + text(i)), {{ctx: mat(\"{ctx}\")}}), test(\"这段话说的是天气吗？\", \"k\"))) }})\n"
    )
}

#[test]
fn 超窗按种类计数() {
    // 语境约 4,000 token（超 JSON 槽已测窗口），对象槽短：每次判断计一次语境超窗
    let o = 跑(&程序(5, 4000, 3), 带画像());
    assert_eq!(o.window_over["ctx"], json!(3), "{}", o.window_over);
    assert_eq!(o.window_over["text"], json!(0));
    // 对象槽单段约 4,000 token、语境短：计对象槽超窗
    let o = 跑(&程序(4000, 5, 2), 带画像());
    assert_eq!(o.window_over["text"], json!(2), "{}", o.window_over);
    assert_eq!(o.window_over["ctx"], json!(0));
    // 都不超：全是 0，字段照样出现
    let o = 跑(&程序(5, 5, 2), 带画像());
    assert_eq!(o.window_over, json!({"text": 0, "ctx": 0, "group": 0}));
}

#[test]
fn 画像没测窗口时不出() {
    let o = 跑(&程序(5, 4000, 2), CalibStore::new());
    assert!(o.window_over.is_null());
}
