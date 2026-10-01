//! Z0398 B 步（过程记录 5.20；裁定五十五）：默认链取材料四级——作者 `unsure_source.fetch` > 程序料库 `CategoryStore` >
//! 宿主取材料端口 `MaterialSource` > 无（路 C）。逐级往下：上一级没配置或取不到就试下一级；配置了的都取不到记 missed，
//! 末端记 Drop。报告行 `fetched_by` 写材料由哪一级给。料库与端口都用闭包作替身。固定关伴随题。

use jpp::effects::{CalibStore, FnPort, JudgeResult, Ports};
use jpp::interp::CompanionMode;
use jpp::ledger::{Entry, Ledger};
use jpp::value::{Answer, Mat, Question};
use jpp::{ActionRegistry, EntryArgs, Outcome, Session, lower, syntax::parse};
use serde_json::{Value as Json, json};
use std::cell::RefCell;
use std::rc::Rc;

/// 没补过（ctx 空）读 0.5（并列、未决），补过读 0.9；补进来的材料文本记下
fn 端口<'a>(calls: &'a RefCell<u64>, 见: &'a RefCell<Vec<String>>) -> Ports<'a> {
    Ports::new().with(FnPort::judge("fixed-0", move |s, qs| {
        *calls.borrow_mut() += 1;
        见.borrow_mut().extend(s.ctx.iter().map(|m| m.text()));
        Ok(JudgeResult {
            answers: qs
                .iter()
                .map(|_| Answer::Noul(if s.ctx.is_empty() { 0.5 } else { 0.9 }))
                .collect(),
            tokens: 0,
            cost: 0.0,
            mode_share: vec![],
            perms: vec![],
            confidence: vec![],
        })
    }))
}

fn 料(t: &str) -> Mat {
    Mat::literal(Json::String(t.into()))
}

struct 跑出 {
    outcome: Result<Outcome, String>,
    ledger: Ledger,
    见: Vec<String>,
}

type 料库 = Option<Rc<dyn jpp::effects::CategoryStore>>;
type 端口 = Option<Rc<dyn jpp::effects::MaterialSource>>;

fn 跑(fetch: &str, 库: 料库, 宿主: 端口, ledger: &mut Ledger, 重放: bool) -> 跑出 {
    let src = format!(
        "budget {{calls: 8, cost: 0, depth: 16}};\nunsure_source({{need: [\"过往合作\"]{fetch}}});\nlet e = cut(judge(state(mat(\"甲方与乙方的合作意向\")), test(\"这两方适合合作吗？\", \"k\")));\nhandle(e, {{act: fn() {{ \"合作\" }}, ignore: fn() {{ \"不合作\" }}}})\n"
    );
    let program = lower(&parse(&src).expect("解析")).expect("lower");
    let (calls, 见) = (RefCell::new(0), RefCell::new(vec![]));
    let (calib, a) = (CalibStore::new(), ActionRegistry::new());
    let mut s = Session::new(端口(&calls, &见), &calib, &a).with_companions(CompanionMode::Off);
    if let Some(k) = 库 {
        s = s.with_category_store(k);
    }
    if let Some(h) = 宿主 {
        s = s.with_material_source(h);
    }
    let outcome = if 重放 {
        s.replay(&program, &EntryArgs::default(), ledger)
    } else {
        s.run(&program, &EntryArgs::default(), ledger)
    }
    .map_err(|e| e.render());
    let 见 = 见.borrow().clone();
    跑出 {
        outcome,
        ledger: ledger.clone(),
        见,
    }
}

const 作者: &str = ", fetch: fn(q, need, m) { \"作者取的\" + need }";
const 作者取不到: &str = ", fetch: fn(q, need, m) { fail(\"没有\") }";

fn 库() -> 料库 {
    Some(Rc::new(|need: &str, _m: &Mat| {
        Some(料(&format!("料库取的{need}")))
    }))
}
fn 宿主() -> 端口 {
    Some(Rc::new(|_q: &Question, need: &str, _m: &Mat| {
        Some(料(&format!("宿主取的{need}")))
    }))
}

fn 行(r: &跑出) -> Json {
    let o = r.outcome.as_ref().unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(o.unsure_default.len(), 1, "{:?}", o.unsure_default);
    o.unsure_default[0].clone()
}

#[test]
fn 顺序_作者fetch_料库_宿主端口_路c() {
    let r = 跑(作者, 库(), 宿主(), &mut Ledger::new(), false);
    assert_eq!(行(&r)["fetched_by"], json!(["fetch"]));
    assert_eq!(行(&r)["end"], "decided");
    assert!(
        r.见.iter().any(|t| t.contains("作者取的过往合作")),
        "{:?}",
        r.见
    );
    assert_eq!(r.outcome.unwrap().value_json(), json!("合作"));

    let r = 跑("", 库(), 宿主(), &mut Ledger::new(), false);
    assert_eq!(行(&r)["fetched_by"], json!(["store"]));
    assert!(
        r.见.iter().any(|t| t.contains("料库取的过往合作")),
        "{:?}",
        r.见
    );

    let r = 跑("", None, 宿主(), &mut Ledger::new(), false);
    assert_eq!(行(&r)["fetched_by"], json!(["host"]));
    assert!(
        r.见.iter().any(|t| t.contains("宿主取的过往合作")),
        "{:?}",
        r.见
    );

    let r = 跑("", None, None, &mut Ledger::new(), false);
    let row = 行(&r);
    assert_eq!(row["end"], "handoff");
    assert_eq!(row["needed"], json!(["过往合作"]));
    assert!(
        r.outcome.as_ref().unwrap().pending.is_empty(),
        "路 C 不进报告 pending"
    );
    let ev: Vec<&Entry> = r
        .ledger
        .entries
        .iter()
        .filter(|e| e.is_duty_event())
        .collect();
    assert!(matches!(ev.as_slice(), [Entry::Handoff { .. }]), "{ev:?}");
}

#[test]
fn 作者fetch取不到_落到料库() {
    let r = 跑(作者取不到, 库(), None, &mut Ledger::new(), false);
    let row = 行(&r);
    assert_eq!(row["fetched_by"], json!(["store"]));
    assert_eq!(row["missed"], json!([]));
    assert_eq!(row["end"], "decided");
}

/// 配置了的各级都取不到：Z0398 返修起照裁定五十五走路 C（主控板 Z0502 默认读法，过程记录 5.23）
#[test]
fn 配置了的各级都取不到_路c转交() {
    let 空库: 料库 = Some(Rc::new(|_n: &str, _m: &Mat| None));
    let 空端口: 端口 = Some(Rc::new(|_q: &Question, _n: &str, _m: &Mat| None));
    let r = 跑(作者取不到, 空库, 空端口, &mut Ledger::new(), false);
    let row = 行(&r);
    assert_eq!(row["missed"], json!(["过往合作"]));
    assert_eq!(row["needed"], json!(["过往合作"]));
    assert_eq!(row["end"], "handoff");
    assert!(row.get("fetched_by").is_none());
    let ev: Vec<&Entry> = r
        .ledger
        .entries
        .iter()
        .filter(|e| e.is_duty_event())
        .collect();
    assert!(
        matches!(
            ev.as_slice(),
            [Entry::Enrich { got: false, .. }, Entry::Handoff { .. }]
        ),
        "{ev:?}"
    );
    assert!(
        r.outcome.as_ref().unwrap().pending.is_empty(),
        "路 C 不进报告 pending"
    );
}

/// 料库取不到落到宿主端口（逐级往下的第二跳）
#[test]
fn 料库取不到_落到宿主端口() {
    let 空库: 料库 = Some(Rc::new(|_n: &str, _m: &Mat| None));
    let r = 跑("", 空库, 宿主(), &mut Ledger::new(), false);
    let row = 行(&r);
    assert_eq!(row["fetched_by"], json!(["host"]));
    assert_eq!(row["end"], "decided");
    assert!(
        r.见.iter().any(|t| t.contains("宿主取的过往合作")),
        "{:?}",
        r.见
    );
}

/// Z0494（过程记录 5.24）：料库取来的材料记成 V5 `Opaque`，审计重放不带料库时照账本取，与首跑一致
#[test]
fn 重放不带料库_照账本取_与首跑一致() {
    let mut l = Ledger::new();
    let 首跑 = 跑("", 库(), None, &mut l, false);
    assert_eq!(行(&首跑)["fetched_by"], json!(["store"]));
    let 值 = 首跑.outcome.as_ref().unwrap().value_json();
    // 账本里一条 Opaque：键 fetch/<读数键>/<类别>/<轮>，值带材料与来源
    let op: Vec<(String, Json)> = 首跑
        .ledger
        .entries
        .iter()
        .filter_map(|e| match e {
            Entry::Opaque { key, value, .. } => Some((key.clone(), value.clone())),
            _ => None,
        })
        .collect();
    assert_eq!(op.len(), 1, "{op:?}");
    assert!(
        op[0].0.starts_with("fetch/") && op[0].0.ends_with("/过往合作/1"),
        "{}",
        op[0].0
    );
    assert_eq!(op[0].1["by"], "store");
    let mut l2 = 首跑.ledger.clone();
    let r = 跑("", None, None, &mut l2, true);
    let o = r
        .outcome
        .as_ref()
        .unwrap_or_else(|e| panic!("重放应照账本取：{e}"));
    assert_eq!(o.value_json(), 值);
    assert_eq!(o.unsure_default[0]["fetched_by"], json!(["store"]));
    assert!(
        !o.trace
            .warnings
            .iter()
            .any(|w| w.starts_with("W-replay-duty")),
        "{:?}",
        o.trace.warnings
    );
    assert_eq!(o.cost.calls, 0, "重放不发新调用");
    assert!(r.见.is_empty(), "重放不调判断器");
}

/// 重放给了一个取出不同材料的料库，仍用账本里的材料；续跑时料库不被调用
#[test]
fn 重放与续跑都不再调料库() {
    let mut l = Ledger::new();
    let 首跑 = 跑("", 库(), None, &mut l, false);
    let 调了 = Rc::new(std::cell::Cell::new(0));
    let 计 = 调了.clone();
    let 别的库: 料库 = Some(Rc::new(move |need: &str, _m: &Mat| {
        计.set(计.get() + 1);
        Some(料(&format!("别的料库取的{need}")))
    }));
    let mut l2 = 首跑.ledger.clone();
    let r = 跑("", 别的库.clone(), None, &mut l2, true);
    assert_eq!(r.outcome.unwrap().value_json(), json!("合作"));
    assert_eq!(调了.get(), 0, "审计重放只凭账本");
    let mut l3 = 首跑.ledger.clone();
    let r = 跑("", 别的库, None, &mut l3, false);
    assert_eq!(r.outcome.unwrap().value_json(), json!("合作"));
    assert_eq!(调了.get(), 0, "续跑时已取过的照账本用");
}

/// 宿主端口那一级同样记 Opaque，by 为 host
#[test]
fn 宿主端口取来的也记账() {
    let r = 跑("", None, 宿主(), &mut Ledger::new(), false);
    let by: Vec<Json> = r
        .ledger
        .entries
        .iter()
        .filter_map(|e| match e {
            Entry::Opaque { value, .. } => Some(value["by"].clone()),
            _ => None,
        })
        .collect();
    assert_eq!(by, vec![json!("host")]);
}

/// Z0494 之前录的账本没有 Opaque：重放不带料库仍报 E-replay（5.23 的兜底）
#[test]
fn 没有opaque的旧账本_重放不带料库报e_replay() {
    let mut l = Ledger::new();
    let 首跑 = 跑("", 库(), None, &mut l, false);
    let mut 旧 = 首跑.ledger.clone();
    旧.entries.retain(|e| !matches!(e, Entry::Opaque { .. }));
    let r = 跑("", None, None, &mut 旧, true);
    let e = r
        .outcome
        .err()
        .expect("旧账本重放取不到首跑取到的材料，报 E-replay");
    assert!(e.contains("E-replay") && e.contains("Z0494"), "{e}");
}

/// 复核 Z0398 第八节：完全不写 unsure_source、开着伴随题——「最缺哪类」在同一轮就从通用表候选里选出类别，
/// 默认链第一轮直接用它（不发「为什么」），由料库取到材料后再判定出口；报告 improve 写出选中的类别
#[test]
fn 不写unsure_source_伴随题选出类别_料库取到再判() {
    let src = "budget {calls: 8, cost: 0, depth: 16};\nlet e = cut(judge(state(mat(\"甲方与乙方的合作意向\")), test(\"这两方适合合作吗？\", \"k\")));\nhandle(e, {act: fn() { \"合作\" }, ignore: fn() { \"不合作\" }})\n";
    let program = lower(&parse(src).expect("解析")).expect("lower");
    let (calls, 见) = (RefCell::new(0u64), RefCell::new(Vec::<String>::new()));
    let ports = Ports::new().with(FnPort::judge("fixed-0", |s, qs| {
        *calls.borrow_mut() += 1;
        见.borrow_mut().extend(s.ctx.iter().map(|m| m.text()));
        Ok(JudgeResult {
            answers: qs
                .iter()
                .map(|q| match q.op {
                    // 「最缺哪类」：候选是通用表 [材料, 语境, 参照] 加「材料不缺」，选「语境」
                    jpp::value::Op::Select => Answer::Choice(vec![0.05, 0.85, 0.05, 0.05]),
                    _ if q.calib == "unsure-companion-premise"
                        || q.calib == "unsure-companion-reference" =>
                    {
                        Answer::Noul(0.9)
                    }
                    _ if q.calib.starts_with("unsure-companion-")
                        || q.calib == "diag-two-judgments" =>
                    {
                        Answer::Noul(0.1)
                    }
                    _ => Answer::Noul(if s.ctx.is_empty() { 0.5 } else { 0.9 }),
                })
                .collect(),
            tokens: 0,
            cost: 0.0,
            mode_share: vec![],
            perms: vec![],
            confidence: vec![],
        })
    }));
    let (calib, a) = (CalibStore::new(), ActionRegistry::new());
    let mut l = Ledger::new();
    let o = Session::new(ports, &calib, &a)
        .with_companions(CompanionMode::Same)
        .with_category_store(库().unwrap())
        .run(&program, &EntryArgs::default(), &mut l)
        .unwrap_or_else(|e| panic!("{}", e.render()));
    assert_eq!(o.value_json(), json!("合作"));
    let row = &o.unsure_default[0];
    assert_eq!(row["source"], "generic", "{row}");
    assert_eq!(
        row["asked"],
        json!([]),
        "伴随题已选出类别，不发「为什么」：{row}"
    );
    assert_eq!(row["fetched"], json!(["语境"]), "{row}");
    assert_eq!(row["fetched_by"], json!(["store"]), "{row}");
    assert_eq!(o.improve[0]["lacks"], "语境");
    assert!(见.borrow().iter().any(|t| t.contains("料库取的语境")));
    let choice = l
        .entries
        .iter()
        .filter(|e| matches!(e, Entry::Judge { jkey: Some(k), .. } if k.phys == "choice"))
        .count();
    assert_eq!(choice, 1, "K 选一只有伴随题「最缺哪类」这一道");
}
