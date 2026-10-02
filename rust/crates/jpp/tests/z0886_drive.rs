//! Z0886（推进 B0668、B0670 的 B2-1）：对运行中的过程按步出题（第二靶子缺口 N-T1）。
//! 第三靶子预注册 §2.2–§2.4、§5.2（`地基/规划/第三靶子-预注册-草稿.md`）；库：`lib/derive/drive.jpp`。
//!
//! 世界是进程内的一个小闭包（数轴上走到目标），登记成 `env:step`——本文件只测按步出题，不测子进程（那是 Z0885，
//! 见 `z0885_env_step.rs`）。闭包端口按题面给读数，不发请求。
//!
//! 正面：同一目的在不同步数、不同局数上，生成器调用数相同（与步数无关），每个谓词的题式哈希只有一个；每步一条动作题的
//! 判断记录、状态各不相同；动作取判断器选中的候选，走到目标。反面：动作题并列的那一步取空动作、未决带步号转交；
//! 世界某步不给候选时不判、取空动作。
//!
//! 裁定七十五（守则 premise_process）：过程入口不派生前提——生成器对前提提示给一道能把开局观测分成两边的前提，
//! 过程入口不问它、不排除任何一步；批量入口（purpose_run）拿同一道前提照旧派生、筛项。
use jpp::effects::{CalibStore, EffectError, FnPort, GenResult, JudgeResult, Ports};
use jpp::ledger::{Entry, Ledger};
use jpp::value::{Answer, Question, State, Value};
use jpp::interp::{TaintOut, json_to_value};
use jpp::{ActionRegistry, EntryArgs, Session};
use serde_json::{Value as Json, json};
use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT: AtomicU64 = AtomicU64::new(0);

/// 世界：位置 pos 从 0 出发，走到 goal 即结束；最多 max 步。`gap` 步不给候选（反面用例）
fn 世界(req: &Json, gap: Option<i64>, effect: bool) -> Json {
    let st = if req["state"].is_null() {
        json!({"pos": 0, "goal": req["reset"], "t": 0})
    } else {
        let s = &req["state"];
        let d = match req["action"].as_str() {
            Some("right") => 1,
            Some("left") => -1,
            _ => 0,
        };
        json!({"pos": s["pos"].as_i64().unwrap() + d, "goal": s["goal"], "t": s["t"].as_i64().unwrap() + 1})
    };
    let t = st["t"].as_i64().unwrap();
    let done = st["pos"] == st["goal"] || t >= 12;
    let actions = if gap == Some(t) { json!([]) } else { json!(["left", "right", "wait"]) };
    let mut out = json!({"state": st, "obs": {"t": t, "pos": st["pos"], "goal": st["goal"]}, "actions": actions, "idle": "wait",
           "done": done, "hash": format!("h{}-{}-{}", st["goal"], st["pos"], t),
           "result": {"reached": st["pos"] == st["goal"]}});
    // Z0895：世界自报上一步引起的事件（开局那一步不报）
    if effect && !req["state"].is_null() {
        out["effect"] = json!([format!("moved_to {}", st["pos"])]);
    }
    out
}

thread_local! {
    /// 裁定七十八的反面用例：模块另抽一道 K 选一题「离目标的方向」（候选来自目的，不是世界给的动作），读数恒并列
    static 方向题: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    /// Z0913：「为什么拿不准」选中间判断（缺省选第 0 项）
    static 选中间判断: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    /// Z0913：缩小后再问仍并列（核七十八在缩小集上取最大项）
    static 再问并列: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    /// Z0913：开伴随题，「最缺哪类」点名参照（3g.z 那 124 拍类型）
    static 点名参照: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    /// 跑_源 开伴随题（缺省关）
    static 开伴随: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    /// 跑_源 载入画像 profiles/jev-1.13.0.json（缺省不载：没有 δ）
    static 用画像: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    /// 缩小题里 wait 的读数（缺省 0.5）
    static 等的读数: std::cell::Cell<f64> = const { std::cell::Cell::new(0.5) };
    /// 子题并列（S 拿不准）
    static 子题并列: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    /// 跑完再做一次审计重放，值进 report.replay_value
    static 审计重放: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    /// 第四圈：缩小那组全判否（三个候选都带外否，缩小后为空）
    static 全判否: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    /// 第四圈：派生计分途径时生成器给什么——0 正常（第五圈起每条带可达条件）、1 不是 JSON 对象、2 一项都不给、
    /// 3 纯字符串（第四圈的写法，没有可达条件）
    static 完成条件坏: std::cell::Cell<u8> = const { std::cell::Cell::new(0) };
    /// 第五圈：S″ 的归属标注生成器给什么——0 正常、1 长度不等于候选数、2 不是 JSON 对象、3 超出途径条数
    static 标注坏: std::cell::Cell<u8> = const { std::cell::Cell::new(0) };
    /// 第五圈：S″ 唤不出（生成器一项都不给）
    static 子题三空: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// 第四圈：生成器从规则原文派生的计分途径
const 计分途径: [&str; 2] = ["走到目标那一格得 10 分", "每走一步扣 1 分"];

/// 第五圈：生成器派生的计分途径带可达条件（第二条的条件只有代价）
fn 可达途径() -> Json {
    json!([
        {"route": 计分途径[0], "when": "走到目标那一格时", "needs": ["还在数轴上"], "cost": "", "from": "走到目标那一格得 10 分"},
        {"route": 计分途径[1], "when": "每走一步", "needs": [], "cost": null, "from": "每走一步扣 1 分"}
    ])
}

/// 第五圈：S″ 的题面（含「眼下最要紧的是哪一件事」，判断器替身认得）
const 子题三: &str = "以现在的处境拿得到哪条，眼下最要紧的是哪一件事？";

/// 子题 S 的候选（生成器给的「要紧的事」）
const 要紧的事: [&str; 2] = ["往目标走", "原地等"];

fn 模块() -> Json {
    let mut preds = vec![json!({"text": "离目标还远", "cut": "binary", "field": "far", "cut_from": "system"})];
    if 方向题.with(|c| c.get()) {
        preds.push(json!({"text": "离目标的方向", "cut": "k_ary", "over": ["左边", "右边"], "request": "one", "field": "dir",
                          "cut_from": "purpose", "over_from": "purpose"}));
    }
    json!({"material": "数轴上的位置", "predicates": preds,
           "request": null, "presupposition": null, "context": null, "reference": null, "budget": null, "unsure": null, "done": null})
}

/// 能把开局观测分成两边的前提（目标在右边的局判是，在左边的局判否）
const 前提题: &str = "材料里写明了位置在目标左边吗？";

fn 生成(p: &str) -> Vec<Json> {
    if p.contains("字面前提题") {
        vec![json!({"op": "test", "text": 前提题, "if_false": {"falls_to": "unanswerable"}})]
    } else if p.starts_with("下面是一段目的") && p.contains("计分途径") {
        // 第四圈：派生完成条件（提示也以「下面是一段目的」开头，先认它）
        match 完成条件坏.with(|c| c.get()) {
            1 => vec![json!("没有计分途径")],
            2 => vec![],
            3 => vec![json!({"routes": 计分途径})],
            _ => vec![json!({"routes": 可达途径()})],
        }
    } else if p.starts_with("下面是这一局的计分途径") && p.contains("标上") {
        // 第五圈：S″ 候选的归属标注（两条途径、两个候选）
        match 标注坏.with(|c| c.get()) {
            1 => vec![json!({"labels": [1]})],
            2 => vec![json!("标不出")],
            3 => vec![json!({"labels": [1, 3]})],
            _ => vec![json!({"labels": [1, 0]})],
        }
    } else if p.contains("拿得到") {
        // 第五圈：S″（唤出提示里也有「眼下最要紧」，先认它）
        if 子题三空.with(|c| c.get()) {
            vec![]
        } else {
            vec![json!({"op": "select", "text": 子题三, "over": 要紧的事})]
        }
    } else if p.starts_with("下面是一段目的") {
        vec![模块()]
    } else if p.contains("照这一局的计分办法") {
        // 第四圈：带完成条件的子题 S′（题面与 S 不同，好分辨用的是哪一份）
        vec![json!({"op": "select", "text": "照计分办法，眼下最要紧的是哪一件事？", "over": 要紧的事})]
    } else if p.contains("眼下最要紧") {
        // 子题 S 的唤出提示里也带着模块原文（含别的谓词），所以先认它
        vec![json!({"op": "select", "text": "眼下最要紧的是哪一件事？", "over": 要紧的事})]
    } else if p.contains("离目标的方向") {
        vec![json!({"op": "select", "text": "目标在当前位置的哪一边？", "over": ["左边", "右边"]})]
    } else if p.contains("离目标还远") {
        vec![json!({"op": "test", "text": "离目标还远吗？"})]
    } else {
        vec![]
    }
}

/// 动作题：往目标走；`tie_t` 那一步给并列
fn 读数(q: &Question, s: &State, tie_t: Option<i64>) -> Answer {
    let t = &q.text;
    if t.contains("目标在当前位置的哪一边") {
        return Answer::Choice(vec![0.5, 0.5]);
    }
    // 伴随题（题面里嵌着原题，先认它们，免得被原题的分支接走）：「最缺哪类」点名参照（开关），「藏了两个判断」带外否，其余中性
    if t.starts_with("题「") || t.starts_with("判断题「") || t.starts_with("把题「") {
        if t.contains("最缺哪一类") {
            let mut v = vec![0.0; s.over.len()];
            let k = if 点名参照.with(|c| c.get()) {
                s.over.iter().position(|m| m.content.as_str() == Some("参照")).unwrap_or(0)
            } else {
                s.over.len() - 1
            };
            v[k] = 1.0;
            return Answer::Choice(v);
        }
        return Answer::Noul(if t.contains("需要分别回答") { 0.1 } else { 0.5 });
    }
    if t.contains("为什么拿不准") {
        let mut v = vec![0.0; s.over.len()];
        let k = if 选中间判断.with(|c| c.get()) {
            s.over.iter().position(|m| m.content.as_str() == Some("中间判断")).unwrap_or(0)
        } else {
            0
        };
        v[k] = 1.0;
        return Answer::Choice(v);
    }
    if t.contains("眼下最要紧的是哪一件事") {
        return Answer::Choice(if 子题并列.with(|c| c.get()) { vec![0.5, 0.5] } else { vec![0.9, 0.1] });
    }
    if t.contains("这件事吗") {
        // 缩小：right 是在往目标走（带外是），left 不是（带外否），wait 拿不准（带内，保留）
        if 全判否.with(|c| c.get()) {
            return Answer::Noul(0.1);
        }
        return Answer::Noul(if t.contains("「right」") { 0.9 } else if t.contains("「left」") { 0.1 } else { 等的读数.with(|c| c.get()) });
    }
    if t.contains("需要分别回答的判断") {
        Answer::Noul(0.1)
    } else if t.contains("这段材料里有没有") {
        Answer::Noul(0.9)
    } else if t.contains("同一件事") {
        let k = s.over.len();
        let mut v = vec![0.0; k];
        v[k - 1] = 1.0;
        Answer::Choice(v)
    } else if t.contains("下一步最该做的动作") {
        let c = &s.on[0].content;
        let (pos, goal, tt) = (c["pos"].as_i64().unwrap(), c["goal"].as_i64().unwrap(), c["t"].as_i64().unwrap());
        // 并列只给首问（原集三个候选、ctx 空）；缩小后或带了子题回答再问时照常判，除非要测再问仍并列。
        // 第四圈：完成条件那份语境（done 为 on 时每拍都在）不算，ctx 里只有它的仍是首问
        let 首问 = s.over.len() == 3 && s.ctx.iter().all(|m| m.content.to_string().contains("这一局怎么计分"));
        if tie_t == Some(tt) && 首问 {
            return Answer::Choice(vec![0.4, 0.4, 0.2]);
        }
        if tie_t == Some(tt) && 再问并列.with(|c| c.get()) {
            return Answer::Choice(vec![1.0 / s.over.len() as f64; s.over.len()]);
        }
        let want = if pos < goal { "right" } else if pos > goal { "left" } else { "wait" };
        Answer::Choice(
            s.over
                .iter()
                .map(|m| if m.content.as_str() == Some(want) { 0.8 } else { 0.1 })
                .collect(),
        )
    } else if t.as_str() == 前提题 {
        let c = &s.on[0].content;
        Answer::Noul(if c["pos"].as_i64() < c["goal"].as_i64() { 0.9 } else { 0.1 })
    } else if t.contains("离目标还远") {
        Answer::Noul(0.7)
    } else {
        Answer::Noul(0.5)
    }
}

struct 跑出 {
    value: Json,
    ledger: Ledger,
    gens: usize,
    /// 动作题每次判断时状态的 ctx（JSON 文本；没有 ctx 为空串），按收到的顺序
    ctxs: Vec<String>,
    /// 判断器收到的全部题面，按收到的顺序
    asked: Vec<String>,
    /// 判断器每次调用收到的题面（一次调用一组）
    批: Vec<Vec<String>>,
    /// 运行报告
    report: Json,
    /// 生成器每次调用：提示与收到的材料份数（第四圈：规则原文作材料）
    gen_calls: Vec<(String, usize)>,
}

fn 跑(resets: &str, tie_t: Option<i64>, gap: Option<i64>) -> 跑出 {
    跑_局(resets, "", tie_t, gap)
}

fn 跑_局(resets: &str, play: &str, tie_t: Option<i64>, gap: Option<i64>) -> 跑出 {
    跑_全(resets, play, tie_t, gap, false)
}

fn 跑_全(resets: &str, play: &str, tie_t: Option<i64>, gap: Option<i64>, effect: bool) -> 跑出 {
    跑_源(&驱动程序(resets, play, 2000), tie_t, gap, effect)
}

fn 驱动程序(resets: &str, play: &str, calls: usize) -> String {
    驱动程序_带(resets, play, calls, "")
}

/// `前置` 插在 budget 之后（例如 unsure_source）；pending 带 via（裁定七十八核去向）与 exit（投影保留出口，未决算转交，G2）
fn 驱动程序_带(resets: &str, play: &str, calls: usize, 前置: &str) -> String {
    format!(
        "import \"../../lib/derive/drive.jpp\";\nbudget {{calls: {calls}, cost: 0, depth: 8192}};\n{前置}\n\
         let r = purpose_drive(\"走到目标位置。\", \"line\", {{reset: {resets}, bound: 200{play}}});\n\
         {{value: r.value, pending: map(r.pending, fn(p) {{ {{cause: p.cause, pos: p.pos, via: p.via, exit: p.exit}} }}), detail: r.detail}}\n"
    )
}

fn 跑_源(src: &str, tie_t: Option<i64>, gap: Option<i64>, effect: bool) -> 跑出 {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let dir = root.join(format!("target/z0886-{}-{}", std::process::id(), NEXT.fetch_add(1, Ordering::Relaxed)));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("p.jpp");
    std::fs::write(&path, src).unwrap();
    let loaded = jpp::syntax::loader::load(&path);
    let _ = std::fs::remove_dir_all(&dir);
    let loaded = loaded.unwrap_or_else(|e| panic!("装载：{e:?}"));
    let program = jpp::lower(&loaded.program).unwrap_or_else(|e| panic!("lower：{e:?}"));
    let mut acts = ActionRegistry::new();
    acts.register("env:step", 0.0, true, TaintOut::Untrusted, move |args: &[Value]| {
        Ok(json_to_value(&世界(&args[0].to_json(), gap, effect)))
    });
    let gens = Rc::new(RefCell::new(0usize));
    let g2 = gens.clone();
    let gen_calls = Rc::new(RefCell::new(Vec::<(String, usize)>::new()));
    let gc2 = gen_calls.clone();
    let ctxs = Rc::new(RefCell::new(Vec::<String>::new()));
    let c2 = ctxs.clone();
    let asked = Rc::new(RefCell::new(Vec::<String>::new()));
    let a2 = asked.clone();
    let 批 = Rc::new(RefCell::new(Vec::<Vec<String>>::new()));
    let b2 = 批.clone();
    let ports = Ports::new()
        .with(FnPort::judge("fixed-0", move |s, qs| {
            a2.borrow_mut().extend(qs.iter().map(|q| q.text.clone()));
            b2.borrow_mut().push(qs.iter().map(|q| q.text.clone()).collect());
            if qs.iter().any(|q| q.text.contains("下一步最该做的动作")) {
                c2.borrow_mut().push(s.ctx.iter().map(|m| m.content.to_string()).collect::<Vec<_>>().join("|"));
            }
            Ok::<_, EffectError>(JudgeResult {
                answers: qs.iter().map(|q| 读数(q, s, tie_t)).collect(),
                tokens: 0,
                cost: 0.0,
                mode_share: vec![],
                perms: vec![],
                confidence: vec![],
            })
        }))
        .with(FnPort::generate("fixed-0", move |p, c, _n, _r| {
            *g2.borrow_mut() += 1;
            gc2.borrow_mut().push((p.to_string(), c.len()));
            Ok(GenResult { outputs: 生成(p), ..Default::default() })
        }))
        .with(FnPort::ask("fixed-0", |_s, _q| Err(EffectError("不该 ask".into()))));
    let mut ledger = Ledger::new();
    let mut calib = CalibStore::new();
    if 用画像.with(|c| c.get()) {
        let pf = root.join("profiles/jev-1.13.0.json");
        let j: Json = serde_json::from_str(&std::fs::read_to_string(pf).unwrap()).unwrap();
        calib.profile = jpp::effects::Profile::from_json(&j).unwrap();
    }
    let out = Session::new(ports, &calib, &acts)
        .with_companions(if 开伴随.with(|c| c.get()) { jpp::interp::CompanionMode::Same } else { jpp::interp::CompanionMode::Off })
        .run(&program, &EntryArgs::default(), &mut ledger)
        .unwrap_or_else(|e| panic!("{}", e.render()));
    // B0630：过程入口（drive.jpp）上进键的站点都查得到结构化标识，没有回退成偏移
    assert_eq!(out.site_key_fallback, 0, "站点回退成偏移");
    let gens = *gens.borrow();
    let gen_calls = gen_calls.borrow().clone();
    let ctxs = ctxs.borrow().clone();
    let asked = asked.borrow().clone();
    let 批 = 批.borrow().clone();
    let report = json!({"budget": out.budget.as_ref().map(|b| json!({"exhausted": b.exhausted, "unsent": b.unsent})),
                        "selections": out.selections, "unsure_default": out.unsure_default,
                        "violations": out.violations.len(), "violations_detail": format!("{:?}", out.violations),
                        "warnings": out.trace.warnings.clone(), "named_unfetchable": out.named_unfetchable.clone()});
    // O9：审计重放只凭账本，判断器、生成器都不该再被调用
    let report = if 审计重放.with(|c| c.get()) {
        let ports2 = Ports::new()
            .with(FnPort::judge("fixed-0", |_s, _q| Err::<JudgeResult, _>(EffectError("重放不该调判断器".into()))))
            .with(FnPort::generate("fixed-0", |_p, _c, _n, _r| Err::<GenResult, _>(EffectError("重放不该调生成器".into()))))
            .with(FnPort::ask("fixed-0", |_s, _q| Err(EffectError("不该 ask".into()))));
        let mut l2 = ledger.clone();
        let o2 = Session::new(ports2, &calib, &acts)
            .with_companions(if 开伴随.with(|c| c.get()) { jpp::interp::CompanionMode::Same } else { jpp::interp::CompanionMode::Off })
            .replay(&program, &EntryArgs::default(), &mut l2)
            .unwrap_or_else(|e| panic!("审计重放：{}", e.render()));
        let mut r = report;
        r["replay_value"] = o2.value_json();
        r
    } else {
        report
    };
    跑出 { value: out.value_json(), ledger, gens, ctxs, asked, 批, report, gen_calls }
}

/// 某道题（按题哈希）的判断记录的状态哈希
fn 判断状态(l: &Ledger, q: &str) -> Vec<String> {
    l.entries
        .iter()
        .filter_map(|e| match e {
            Entry::Judge { jkey: Some(k), .. } if k.q == q => Some(k.state.clone()),
            _ => None,
        })
        .collect()
}

#[test]
fn 题式跨步复用_生成器调用与步数无关() {
    let a = 跑("[3]", None, None);
    let b = 跑("[3, 6]", None, None);
    // 一局 3 步、两局 3 + 6 步：出题只做一次
    assert_eq!(a.gens, b.gens, "生成器调用数与步数、局数无关");
    assert_eq!(a.gens, 3, "抽模块 + 唤出 far + 唤出子题 S（Z0913；过程入口不派生前提，裁定七十五）");
    let v = &b.value["value"];
    let rows = v["rows"].as_array().unwrap();
    assert_eq!(rows.len(), 9, "{v}");
    assert!(rows.iter().all(|r| r["by"] == "judge"), "{v}");
    let res = v["results"].as_array().unwrap();
    assert_eq!(res.len(), 2);
    assert!(res.iter().all(|r| r["result"]["reached"] == true), "{v}");
    // 每个谓词一个题式哈希
    let qs = b.value["detail"]["plan"]["questions"].as_array().unwrap();
    assert_eq!(qs.len(), 2, "far 与系统补的 act：{qs:?}");
    let act = qs.iter().find(|q| q["over_from"] == "process").unwrap();
    assert_eq!(act["field"], "act");
    assert_eq!(act["from"], "assemble");
    // 动作题：每步一条判断记录、状态各不相同（题式只有一个：同一个题哈希）
    let qh = act["form"].as_str().unwrap();
    let ss = 判断状态(&b.ledger, qh);
    assert_eq!(ss.len(), 9, "每步一条：{ss:?}");
    let 不同: std::collections::BTreeSet<_> = ss.iter().collect();
    assert_eq!(不同.len(), 9);
}

/// 裁定七十八（正面）：过程入口的动作题第 0 步并列（0.4、0.4、0.2），默认链末端仍拿不准 → 按最大项行动
/// （概率相等取下标小的：left），行标签 by: "top"；未决照转交进 pending（带步号、去向「已按最大项行动」），不进 dropped；
/// 报告的默认链行 end: "top"、via「并列、按最大项」。改前这一步交空动作 wait（by: "idle"）
#[test]
fn 并列按最大项行动_未决带步号转交() {
    let r = 跑("[2]", Some(0), None);
    let v = &r.value["value"];
    let rows = v["rows"].as_array().unwrap();
    assert_eq!(rows[0]["by"], "top", "{v}");
    assert_eq!(rows[0]["action"], "left", "{v}");
    assert!(rows[1..].iter().all(|x| x["by"] == "judge"), "{v}");
    let pend = r.value["pending"].as_array().unwrap();
    let p0: Vec<_> = pend.iter().filter(|p| p["pos"] == 0).collect();
    assert_eq!(p0.len(), 1, "第 0 步恰好一条未决：{pend:?}");
    assert!(p0[0]["via"].as_array().unwrap().iter().any(|x| x == "已按最大项行动"), "{pend:?}");
    let dropped = r.value["detail"]["dropped"].as_array().unwrap();
    assert!(dropped.iter().all(|d| d["pos"] != 0), "{dropped:?}");
    let ud = r.report["unsure_default"].as_array().unwrap();
    assert!(ud.iter().any(|u| u["end"] == "top" && u["via"] == "并列、按最大项" && u["top"] == 0), "{ud:?}");
    assert_eq!(r.report["violations"], 0);
    let ws = r.report["warnings"].to_string();
    assert!(!ws.contains("W-duty-twice"), "{ws}");
}

/// 裁定七十八（正面，取不到那条路）：作者配了取法、每类都取不到，问出的类别进 needed——末端照样按最大项行动，
/// pending 的去向先写「缺<类别>、无取法」，再写「已按最大项行动」
#[test]
fn 点名类别取不到也按最大项行动() {
    let r = 跑_源(
        // 关题树（Z0913 的 off，即只有七十七、七十八）：中间判断也按取不到
        &驱动程序_带("[2]", ", tree: \"off\"", 2000, "unsure_source({fetch: fn(q, need, m) { fail(\"没有这一类材料\") }});"),
        Some(0),
        None,
        false,
    );
    let rows = r.value["value"]["rows"].as_array().unwrap();
    assert_eq!(rows[0]["by"], "top", "{rows:?}");
    assert_eq!(rows[0]["action"], "left");
    let pend = r.value["pending"].as_array().unwrap();
    let p0: Vec<_> = pend.iter().filter(|p| p["pos"] == 0).collect();
    assert_eq!(p0.len(), 1, "{pend:?}");
    let via: Vec<String> = p0[0]["via"].as_array().unwrap().iter().map(|x| x.as_str().unwrap_or("").to_string()).collect();
    assert!(via.iter().any(|x| x.starts_with("缺") && x.ends_with("无取法")), "{via:?}");
    assert!(via.iter().any(|x| x == "已按最大项行动"), "{via:?}");
    let ud = r.report["unsure_default"].as_array().unwrap();
    assert!(ud.iter().any(|u| u["end"] == "top" && u["needed"].as_array().is_some_and(|n| !n.is_empty())), "{ud:?}");
    assert_eq!(r.report["violations"], 0);
}

/// 裁定七十八（反面一）：过程里别的 K 选一题（模块抽出的「离目标的方向」，读数恒并列）不是动作题，没有标记，
/// 末端照旧留空、进 pending；动作照常由动作题定
#[test]
fn 过程里别的选择题末端照旧留空() {
    方向题.with(|c| c.set(true));
    let r = 跑("[2]", None, None);
    方向题.with(|c| c.set(false));
    let v = &r.value["value"];
    let rows = v["rows"].as_array().unwrap();
    assert!(!rows.is_empty(), "{v}");
    assert!(rows.iter().all(|x| x["by"] == "judge" && x["fields"]["dir"].is_null()), "{v}");
    let qs = r.value["detail"]["plan"]["questions"].as_array().unwrap();
    assert!(qs.iter().any(|q| q["field"] == "dir" && q["op"] == "select"), "方向题要真的问到：{qs:?}");
    let ud = r.report["unsure_default"].as_array().unwrap();
    assert!(!ud.is_empty() && ud.iter().all(|u| u["end"] != "top"), "方向题并列要走到默认链末端：{ud:?}");
}

/// 裁定七十八（反面二）：批量入口（purpose_run）同一道并列的选择题，末端照旧留空、进 pending，不按最大项
#[test]
fn 批量入口选择题末端照旧留空() {
    方向题.with(|c| c.set(true));
    let r = 跑_源(
        "import \"../../lib/derive/purpose.jpp\";\nbudget {calls: 2000, cost: 0, depth: 8192};\n\
         let r = purpose_run(\"走到目标位置。\", [{on: mat({t: 0, pos: 0, goal: 3})}, {on: mat({t: 0, pos: 1, goal: 3})}], {});\n\
         {value: r.value, pending: map(r.pending, fn(p) { {cause: p.cause, via: p.via} }), detail: r.detail}\n",
        None,
        None,
        false,
    );
    方向题.with(|c| c.set(false));
    let rows = r.value["value"].as_array().unwrap();
    assert_eq!(rows.len(), 2, "{rows:?}");
    assert!(rows.iter().all(|x| x["fields"]["dir"].is_null()), "{rows:?}");
    let ud = r.report["unsure_default"].as_array().unwrap();
    assert!(!ud.is_empty() && ud.iter().all(|u| u["end"] != "top"), "{ud:?}");
}

#[test]
fn 世界不给候选时不判取空动作() {
    let r = 跑("[3]", None, Some(1));
    let rows = r.value["value"]["rows"].as_array().unwrap();
    assert_eq!(rows[1]["by"], "no-candidates", "{rows:?}");
    assert_eq!(rows[1]["action"], "wait");
    assert_eq!(r.value["value"]["results"][0]["result"]["reached"], true);
}

/// play：一趟只推进指定的局（宿主逐局看花费，附录 9.2）；出题与一趟跑全部时相同（同题式、同生成器调用数）
#[test]
fn 只推进指定的局_出题不变() {
    let all = 跑("[3, 6]", None, None);
    let one = 跑_局("[3, 6]", ", play: [1]", None, None);
    assert_eq!(all.gens, one.gens);
    let qa = &all.value["detail"]["plan"]["questions"];
    let qb = &one.value["detail"]["plan"]["questions"];
    assert_eq!(qa, qb, "两趟的题集相同");
    let rows = one.value["value"]["rows"].as_array().unwrap();
    assert!(rows.iter().all(|r| r["match"] == 1), "{rows:?}");
    assert_eq!(rows.len(), 6);
    assert_eq!(one.value["value"]["results"].as_array().unwrap().len(), 1);
}

/// Z0895：世界自报 effect 时，每局第一步之外的每一步，动作题的状态带上一步所选的候选与 effect；不报时一步都不带
#[test]
fn 世界自报事件进下一步语境() {
    let on = 跑_全("[3]", "", None, None, true);
    let rows = on.value["value"]["rows"].as_array().unwrap();
    assert_eq!(rows.len(), 3);
    // 闸门不判动作题（组装、不唤出），所以收到的动作题判断恰好每步一次
    assert_eq!(on.ctxs.len(), 3, "{:?}", on.ctxs);
    assert_eq!(on.ctxs[0], "", "第一步没有上一步");
    for (i, c) in on.ctxs.iter().enumerate().skip(1) {
        let v: Json = serde_json::from_str(c).unwrap();
        assert_eq!(v["previous_action"], rows[i - 1]["action"], "{c}");
        assert_eq!(v["effect"][0], format!("moved_to {}", i), "{c}");
    }
    let off = 跑_全("[3]", "", None, None, false);
    assert!(off.ctxs.iter().all(|c| c.is_empty()), "{:?}", off.ctxs);
    // 题集不受 effect 影响（出题阶段的输入相同；第二圈预注册 §1.6）
    assert_eq!(on.value["detail"]["plan"]["questions"], off.value["detail"]["plan"]["questions"]);
}

/// Z0911（R-102，裁定七十二 (1)）：同一步的两道独立判断（是非题 far 与 K 选一 act）登记在同一层、一起发出，
/// 不被库里节点的 if 刷新拆成先后两层
#[test]
fn 同一步的独立判断同层发出() {
    let r = 跑("[3]", None, None);
    let qs = r.value["detail"]["plan"]["questions"].as_array().unwrap();
    let form = |f: &str| qs.iter().find(|q| q["field"] == f).unwrap()["form"].as_str().unwrap().to_string();
    let (far, act) = (form("far"), form("act"));
    let layers = |q: &str| -> Vec<u32> {
        r.ledger
            .entries
            .iter()
            .filter_map(|e| match e {
                Entry::Judge { jkey: Some(k), layer, .. } if k.q == q => Some(*layer),
                _ => None,
            })
            .collect()
    };
    let (lf, la) = (layers(&far), layers(&act));
    assert_eq!(la.len(), 3, "{la:?}");
    // far 在出题阶段的闸门样本上也可能判过（同题式不同状态），取逐步那几条：最后 3 条
    let lf = lf[lf.len() - 3..].to_vec();
    assert_eq!(lf, la, "同一步两道题同层：far {lf:?} act {la:?}");
}

/// Z0911 复核①：预算在一步中间用完时，过程入口同一步的两道题合在同一层同一次调用里（同材料融合），要么一起发、
/// 要么一起不发——不会出现改前那种 far 判了、act 因预算没判的半步。没判的那一步取空动作，两道题各一条预算未决带步号
#[test]
fn 预算紧时同一步的题一起发或一起不发() {
    let mut 截过 = 0;
    for calls in 9usize..=14 {
        let r = 跑_源(&驱动程序("[6]", "", calls), None, None, false);
        assert_eq!(r.report["budget"]["exhausted"], true, "calls={calls}");
        let rows = r.value["value"]["rows"].as_array().unwrap();
        let pend = r.value["pending"].as_array().unwrap();
        for x in rows {
            let (far, act) = (&x["fields"]["far"], &x["fields"]["act"]);
            assert_eq!(far.is_null(), act.is_null(), "calls={calls} 半步：{x}");
            if act.is_null() {
                截过 += 1;
                assert_eq!(x["by"], "idle", "{x}");
                // 只数两道逐步题（far、act）的预算未决；出题阶段闸门的预算未决也带 pos（样本下标），不在此列
                let n = pend
                    .iter()
                    .filter(|p| {
                        p["cause"] == "budget"
                            && p["pos"] == x["step"]
                            && p["via"].as_array().is_some_and(|v| v.iter().any(|y| y == "purpose:far" || y == "purpose:act"))
                    })
                    .count();
                assert_eq!(n, 2, "calls={calls} 两道题各一条预算未决：{pend:?}");
            }
        }
    }
    assert!(截过 >= 3, "这几档预算里至少有三档停在一步中间");
}

/// Z0911 复核①（批量入口）：深判的题同层发出，预算不够时由层内挑选决定先发哪些（真站点先、按价值；价值相同按登记序，
/// 即项序）。这里各项价值相同：剩几次调用就判前几项，其余每项一条预算未决。价值不同时先发的不再必然是前几项（改前逐项分层按项序）
#[test]
fn 预算紧时批量入口按层内挑选截断() {
    // 四项都在目标左边：生成器给的前提在样本上不分两边（no_split 弃掉），不筛项；出题阶段花多少次调用不在这里钉
    let mut 见过 = std::collections::BTreeSet::new();
    for calls in 6usize..=24 {
        let r = 跑_源(
            &format!(
                "import \"../../lib/derive/purpose.jpp\";\nbudget {{calls: {calls}, cost: 0, depth: 8192}};\n\
                 let r = purpose_run(\"走到目标位置。\", [{{on: mat({{t: 0, pos: 0, goal: 9}})}}, {{on: mat({{t: 0, pos: 1, goal: 9}})}}, \
                 {{on: mat({{t: 0, pos: 2, goal: 9}})}}, {{on: mat({{t: 0, pos: 5, goal: 9}})}}], {{}});\n\
                 {{value: r.value, pending: map(r.pending, fn(p) {{ {{cause: p.cause, item: p.item}} }})}}\n"
            ),
            None,
            None,
            false,
        );
        let rows = r.value["value"].as_array().unwrap();
        assert_eq!(rows.len(), 4, "calls={calls}");
        // 预算在出题阶段就用完的那几档，深判这一层没登记，不在本测试范围
        if rows.iter().any(|x| x["fields"].get("far").is_none()) {
            continue;
        }
        let 判了: Vec<bool> = rows.iter().map(|x| !x["fields"]["far"].is_null()).collect();
        let k = 判了.iter().filter(|b| **b).count();
        // 判了的恰是前 k 项（价值相同按登记序），其余每项一条预算未决
        assert_eq!(判了, (0..4).map(|j| j < k).collect::<Vec<_>>(), "calls={calls}");
        let pend = r.value["pending"].as_array().unwrap();
        for x in &rows[k..] {
            let n = pend.iter().filter(|p| p["cause"] == "budget" && p["item"] == x["item"]).count();
            assert_eq!(n, 1, "calls={calls} {x}");
        }
        见过.insert(k);
    }
    assert_eq!(见过, (0..=4).collect(), "预算从不够一项到够全部，每一档都见到");
}

/// 裁定七十五（正面）：过程入口不派生前提。两局的开局观测在前提题上分两边（旧库会派生它、过抽样检验，
/// 把目标在左边那一局的每一步排除成空动作，走不到）；现在出题明细没有前提、不问前提题、每步都由判断定动作
#[test]
fn 过程入口不派生前提_不排除任何一步() {
    let r = 跑("[3, -2]", None, None);
    let plan = &r.value["detail"]["plan"];
    assert!(plan["premise"].is_null(), "{plan}");
    assert_eq!(r.gens, 3, "不调前提派生（抽模块、唤出 far、唤出子题 S）");
    let v = &r.value["value"];
    let rows = v["rows"].as_array().unwrap();
    assert!(rows.iter().all(|x| x["by"] == "judge" && x["excluded"] != true), "{v}");
    assert!(v["results"].as_array().unwrap().iter().all(|x| x["result"]["reached"] == true), "{v}");
    assert!(!r.asked.iter().any(|t| t == 前提题), "过程入口不该问前提题");
}

/// 裁定七十五（反面）：批量入口照旧——同一生成器、同一道前提，purpose_run 派生、过抽样检验、按它筛掉目标在左边的那一项
#[test]
fn 批量入口照旧派生前提() {
    let r = 跑_源(
        "import \"../../lib/derive/purpose.jpp\";\nbudget {calls: 2000, cost: 0, depth: 8192};\n\
         let r = purpose_run(\"走到目标位置。\", [{on: mat({t: 0, pos: 0, goal: 3})}, {on: mat({t: 0, pos: 0, goal: -2})}], {});\n\
         {value: r.value, detail: r.detail}\n",
        None,
        None,
        false,
    );
    let d = &r.value["detail"]["premise"];
    assert_eq!(d["passed"], json!([前提题]), "{d}");
    let rows = r.value["value"].as_array().unwrap();
    assert_eq!(rows.len(), 2, "{rows:?}");
    assert!(rows[0]["excluded"] != true, "{rows:?}");
    assert_eq!(rows[1]["excluded"], true, "{rows:?}");
    assert!(r.asked.iter().any(|t| t == 前提题));
}

fn 跑_源_伴随(src: &str, tie_t: Option<i64>, gap: Option<i64>) -> 跑出 {
    开伴随.with(|c| c.set(true));
    let r = 跑_源(src, tie_t, gap, false);
    开伴随.with(|c| c.set(false));
    r
}

/// Z0913 用：开着题树的几个开关跑一局（目标 2，第 0 步动作题并列），返回 跑出
fn 跑_树(mode: &str, 前置: &str, 中间: bool, 再并: bool, 参照: bool) -> 跑出 {
    选中间判断.with(|c| c.set(中间));
    再问并列.with(|c| c.set(再并));
    点名参照.with(|c| c.set(参照));
    let play = if mode.is_empty() { String::new() } else { format!(", tree: \"{mode}\"") };
    let src = 驱动程序_带("[2]", &play, 2000, 前置);
    let r = if 参照 { 跑_源_伴随(&src, Some(0), None) } else { 跑_源(&src, Some(0), None, false) };
    选中间判断.with(|c| c.set(false));
    再问并列.with(|c| c.set(false));
    点名参照.with(|c| c.set(false));
    r
}

fn 树行(r: &跑出) -> Json {
    let ud = r.report["unsure_default"].as_array().unwrap();
    ud.iter().find(|u| u.get("tree").is_some()).cloned().unwrap_or_else(|| panic!("没有走到中间判断：{ud:?}"))
}

/// Z0913（正面，narrow）：「为什么」选中间判断 → 子题 S 选「往目标走」→ 缩小（left 带外否去掉、wait 带内留下）→ 在 [right, wait]
/// 上再问，已决 right。缩小那组三道是非题同一状态、同一次调用、同一层（R-102）；出题明细里有 S 与缩小题式
#[test]
fn 题树_缩小后再问已决() {
    let r = 跑_树("", "", true, false, false);
    let rows = r.value["value"]["rows"].as_array().unwrap();
    assert_eq!(rows[0]["by"], "judge", "{rows:?}");
    assert_eq!(rows[0]["action"], "right");
    let t = 树行(&r);
    assert_eq!(t["end"], "decided", "{t}");
    assert_eq!(t["fetched"], json!(["中间判断"]));
    assert_eq!(t["tree"]["mode"], "narrow");
    assert_eq!(t["tree"]["s"], 0);
    assert_eq!(t["tree"]["s_p"], 0.9, "子题最大一项的概率进报告（T12）");
    assert_eq!(t["tree"]["n0"], 3);
    assert_eq!(t["tree"]["n1"], 2);
    let plan = &r.value["detail"]["plan"]["tree"];
    assert_eq!(plan["s"], "眼下最要紧的是哪一件事？", "{plan}");
    assert!(plan["narrow_form"].is_string(), "{plan}");
    // 缩小那组：三道是非题在判断器的同一次调用里（同一状态、同一层发出，R-102）
    let 组: Vec<&Vec<String>> = r.批.iter().filter(|b| b.iter().any(|t| t.contains("这件事吗"))).collect();
    assert_eq!(组.len(), 1, "缩小题只发一次调用：{组:?}");
    assert_eq!(组[0].iter().filter(|t| t.contains("这件事吗")).count(), 3, "{组:?}");
}

/// Z0913（正面，ctx）：同上，但子题回答只进 ctx、不缩小，在原集上再问
#[test]
fn 题树_只进语境不缩小() {
    let r = 跑_树("ctx", "", true, false, false);
    let rows = r.value["value"]["rows"].as_array().unwrap();
    assert_eq!(rows[0]["by"], "judge", "{rows:?}");
    assert_eq!(rows[0]["action"], "right");
    let t = 树行(&r);
    assert_eq!(t["tree"]["mode"], "ctx", "{t}");
    assert_eq!(t["tree"]["n1"], 3);
    assert!(!r.asked.iter().any(|x| x.contains("这件事吗")), "ctx 不发缩小题");
}

/// Z0913（反面，off）：中间判断按取不到，链走到末端，按七十八取首问的最大项（left）；不问子题 S
#[test]
fn 题树_关时到末端取最大项() {
    let r = 跑_树("off", "", true, false, false);
    let rows = r.value["value"]["rows"].as_array().unwrap();
    assert_eq!(rows[0]["by"], "top", "{rows:?}");
    assert_eq!(rows[0]["action"], "left");
    assert_eq!(树行(&r)["tree"]["mode"], "off");
    assert!(!r.asked.iter().any(|x| x.contains("眼下最要紧的是哪一件事")), "off 不问子题");
}

/// Z0913 × 七十八：缩小到 [right, wait] 后再问仍并列 → 末端按最后那次读数（缩小集上的）最大项行动：right。
/// 若按原集下标取会是 left——这条锁住「最大项取值、不取下标」
#[test]
fn 题树_缩小后仍并列按缩小集最大项() {
    let r = 跑_树("", "", true, true, false);
    let rows = r.value["value"]["rows"].as_array().unwrap();
    assert_eq!(rows[0]["by"], "top", "{rows:?}");
    assert_eq!(rows[0]["action"], "right");
}

/// Z0913 × 七十七（3g.z 那 124 拍类型）：伴随题「最缺哪类」点名参照、取不到 → 直接走下一项中间判断（不问「为什么」）→
/// 子题、缩小、再问已决；off 时同样的拍到末端按最大项
#[test]
fn 题树_点名参照取不到走中间判断() {
    let 取法 = "unsure_source({fetch: fn(q, need, m) { fail(\"没有这一类材料\") }});";
    let on = 跑_树("", 取法, false, false, true);
    let t = 树行(&on);
    assert_eq!(t["route"], "companion", "{t}");
    assert_eq!(t["missed"], json!(["参照"]), "{t}");
    assert_eq!(t["fetched"], json!(["中间判断"]), "{t}");
    assert_eq!(t["end"], "decided", "{t}");
    assert_eq!(on.value["value"]["rows"][0]["action"], "right");
    let off = 跑_树("off", 取法, false, false, true);
    let rows = off.value["value"]["rows"].as_array().unwrap();
    assert_eq!(rows[0]["by"], "top", "{rows:?}");
    assert_eq!(树行(&off)["missed"], json!(["参照", "中间判断"]));
}

/// Z0913（反面）：批量入口与过程里别的 K 选一题的题式 lacks 里没有中间判断；动作题有
#[test]
fn 中间判断只在过程入口的动作题上() {
    方向题.with(|c| c.set(true));
    let r = 跑("[2]", None, None);
    方向题.with(|c| c.set(false));
    let qs = r.value["detail"]["plan"]["questions"].as_array().unwrap();
    let lacks = |f: &str| qs.iter().find(|q| q["field"] == f).unwrap()["lacks"].clone();
    assert!(lacks("act").as_array().unwrap().iter().any(|x| x == "中间判断"), "{qs:?}");
    assert!(!lacks("dir").as_array().unwrap().iter().any(|x| x == "中间判断"), "{qs:?}");
}

/// Z0913：预算在题树中途（子题、缩小那组、再问之间）用完——程序照常结束、不报违规；
/// 缩小那组没读到的候选按「没读到」保留，不当成「否」
#[test]
fn 题树_预算中途用完不违规() {
    let mut 中途 = 0;
    for calls in 6usize..=24 {
        选中间判断.with(|c| c.set(true));
        let r = 跑_源(&驱动程序_带("[2]", "", calls, ""), Some(0), None, false);
        选中间判断.with(|c| c.set(false));
        assert_eq!(r.report["violations"], 0, "calls={calls} {}", r.report["violations_detail"]);
        let 问了子题 = r.asked.iter().any(|t| t.contains("眼下最要紧的是哪一件事"));
        let 缩小了 = r.asked.iter().any(|t| t.contains("这件事吗"));
        if 问了子题 && !缩小了 && r.report["budget"]["exhausted"] == true {
            中途 += 1;
        }
    }
    assert!(中途 >= 1, "至少有一档预算停在子题之后、缩小之前");
}

fn 带开关跑<T>(开关: &'static std::thread::LocalKey<std::cell::Cell<T>>, 值: T, 复原: T, f: impl FnOnce() -> 跑出) -> 跑出
where
    T: Copy + 'static,
{
    开关.with(|c| c.set(值));
    let r = f();
    开关.with(|c| c.set(复原));
    r
}

/// Z0913 M1：账本里追得到三组判断——子题 S 的键（Enrich.asked_by 与 tree.s_key）、缩小那组的键（tree.narrow_keys）、
/// 再问的键（Refine.to）；都是账本里的判断记录
#[test]
fn 题树_账本追得到三组判断键() {
    let r = 跑_树("", "", true, false, false);
    let t = 树行(&r);
    let judges: std::collections::HashSet<String> = r
        .ledger
        .entries
        .iter()
        .filter_map(|e| match e {
            Entry::Judge { key, .. } => Some(key.clone()),
            _ => None,
        })
        .collect();
    let s_key = t["tree"]["s_key"].as_str().unwrap().to_string();
    assert!(judges.contains(&s_key), "{t}");
    let nk: Vec<String> = t["tree"]["narrow_keys"].as_array().unwrap().iter().map(|x| x.as_str().unwrap().to_string()).collect();
    assert_eq!(nk.len(), 3, "{t}");
    assert!(nk.iter().all(|k| judges.contains(k)), "{t}");
    let enrich_by: Vec<Option<String>> = r
        .ledger
        .entries
        .iter()
        .filter_map(|e| match e {
            Entry::Enrich { need, asked_by, .. } if need == "中间判断" => Some(asked_by.clone()),
            _ => None,
        })
        .collect();
    assert_eq!(enrich_by, vec![Some(s_key.clone())], "Enrich.asked_by 是子题的键");
    // 「为什么拿不准」选中了中间判断：那道元题的键也留着（报告 tree 行 why_key），是账本里的判断记录，不同于子题的键
    let why_key = t["tree"]["why_key"].as_str().unwrap().to_string();
    assert!(judges.contains(&why_key) && why_key != s_key, "{t}");
    let refine_to: Vec<String> = r
        .ledger
        .entries
        .iter()
        .filter_map(|e| match e {
            Entry::Refine { to: Some(k), .. } => Some(k.clone()),
            _ => None,
        })
        .collect();
    assert!(refine_to.iter().any(|k| judges.contains(k)), "再问经 Refine.to 连上：{refine_to:?}");
    assert_eq!(t["tree"]["delta_unknown"], true, "这个测试没有画像 δ（O6）");
    assert_eq!(t["tree"]["depth"], 1, "D1");
}

/// Z0913 M2：「带内保留」——载入画像 jev-1.13.0（是非题中段 δ p99 0.1281，带外否的线 0.3719）：
/// wait 的缩小读数 0.45 在带内留下（n1 = 2），0.30 在带外去掉（n1 = 1）
#[test]
fn 题树_带内保留带外否去掉() {
    for (p, n1) in [(0.45, 2), (0.30, 1)] {
        let r = 带开关跑(&用画像, true, false, || 带开关跑(&等的读数, p, 0.5, || 跑_树("", "", true, false, false)));
        let t = 树行(&r);
        assert_eq!(t["tree"]["n1"], n1, "wait {p}：{t}");
        assert!(t["tree"].get("delta_unknown").is_none(), "{t}");
        assert_eq!(r.value["value"]["rows"][0]["action"], "right");
    }
}

/// Z0913 O3：子题拿不准时，pending 的去向写「子题拿不准」，不计进 named_unfetchable，链照常到末端按最大项
#[test]
fn 题树_子题拿不准去向分开记() {
    let r = 带开关跑(&子题并列, true, false, || 跑_树("", "", true, false, false));
    let rows = r.value["value"]["rows"].as_array().unwrap();
    assert_eq!(rows[0]["by"], "top", "{rows:?}");
    let t = 树行(&r);
    assert_eq!(t["tree"]["why"], "子题拿不准", "{t}");
    let pend = r.value["pending"].as_array().unwrap();
    let p0: Vec<_> = pend.iter().filter(|p| p["pos"] == 0 && p["via"].as_array().is_some_and(|v| v.iter().any(|x| x == "purpose:act"))).collect();
    assert!(p0.iter().any(|p| p["via"].as_array().unwrap().iter().any(|x| x == "子题拿不准")), "{p0:?}");
    let nu = r.report["named_unfetchable"].to_string();
    assert!(!nu.contains("中间判断"), "{nu}");
}

/// Z0913 O1、O2（Jpp）：预算在题树中途用完——缩小那组一道都没读到时 tree 行记「未发出」、不记 n1；
/// 缩小后的再问没发出时，按最后一次有读数的动作读数（首问，原集）取最大项，via「预算、按最后读数最大项」
#[test]
fn 题树_预算停在缩小与再问() {
    let (mut 见未发出, mut 见按最后) = (false, false);
    for calls in 10usize..=24 {
        let r = 带开关跑(&选中间判断, true, false, || 跑_源(&驱动程序_带("[2]", "", calls, ""), Some(0), None, false));
        let ud = r.report["unsure_default"].as_array().unwrap();
        for u in ud {
            if u["tree"]["narrow"] == "未发出" {
                见未发出 = true;
                assert!(u["tree"].get("n1").is_none(), "{u}");
            }
            if u["via"] == "预算、按最后读数最大项" {
                见按最后 = true;
                let rows = r.value["value"]["rows"].as_array().unwrap();
                assert_eq!(rows[0]["by"], "top", "calls={calls} {rows:?}");
                assert_eq!(rows[0]["action"], "left", "首问（原集）的最大项");
            }
        }
    }
    assert!(见按最后, "至少一档预算停在再问之前");
    let _ = 见未发出;
    // N2：预算矩阵是确定的——calls = 14 每次都停在「缩小未发出」：tree 行记未发出、不记 n1，去向写原因
    let r = 带开关跑(&选中间判断, true, false, || 跑_源(&驱动程序_带("[2]", "", 14, ""), Some(0), None, false));
    let t = 树行(&r);
    assert_eq!(t["tree"]["narrow"], "未发出", "{t}");
    assert!(t["tree"].get("n1").is_none(), "{t}");
    assert_eq!(t["tree"]["why"], Json::Null, "{t}");
    let pend = r.value["pending"].as_array().unwrap();
    assert!(
        pend.iter().any(|p| p["via"].as_array().is_some_and(|v| v.iter().any(|x| x == "缩小未发出（预算）"))),
        "{pend:?}"
    );
}

/// Z0913 O9：带题树的一局，审计重放只凭账本，值与首跑相同（判断器、生成器一次都不调）
#[test]
fn 题树_审计重放一致() {
    let r = 带开关跑(&审计重放, true, false, || 跑_树("", "", true, false, false));
    assert_eq!(r.report["replay_value"], r.value, "重放的值与首跑相同");
}

// ———— 第三靶子第四圈（预注册 地基/规划/第三靶子-第四圈-预注册草稿.md §2）————

/// 程序的取法：「语境」给规则原文，别的类别取不到（同 botcraft 程序那一行通用取法的形状）
const 规则取法: &str = "unsure_source({fetch: fn(q, need, m) { if need == \"语境\" { mat(\"规则：走到目标那一格得 10 分；每走一步扣 1 分。\") } else { fail(\"没有这一类材料\") } }});";

/// 第四圈用：目标 2、第 0 步动作题并列、「为什么」选中间判断；`开关` 插进 purpose_drive 的 opts（如 `, done: "on"`）
fn 跑_四(开关: &str, 前置: &str) -> 跑出 {
    带开关跑(&选中间判断, true, false, || 跑_源(&驱动程序_带("[2]", 开关, 2000, 前置), Some(0), None, false))
}

fn 动作题式(r: &跑出) -> String {
    let qs = r.value["detail"]["plan"]["questions"].as_array().unwrap();
    qs.iter().find(|q| q["field"] == "act").unwrap()["form"].as_str().unwrap().to_string()
}

/// 第四圈 §2.1（正反两面）：取到语境时出题阶段派生完成条件、唤出 S′（生成器调用 3 → 5，与开关无关；第五圈起另有
/// 唤出 S″、归属标注两次，3 → 7）；
/// done 为 off（4.z）时每拍动作题的状态、动作与第三圈同配置逐字相同，完成条件不进语境；done 为 on（4.d）时每拍都在，
/// 题树问的是 S′
#[test]
fn 完成条件_派生且只在开时进语境() {
    let base = 跑_四("", "");
    let off = 跑_四("", 规则取法);
    let on = 跑_四(", done: \"on\"", 规则取法);
    assert_eq!(base.gens, 3, "没有取法：抽模块、唤出 far、唤出 S");
    assert_eq!(off.gens, 7, "取到语境：加派生完成条件、唤出 S′、唤出 S″、归属标注");
    assert_eq!(on.gens, 7);
    // 规则原文作生成器材料：派生完成条件、唤出 S′、唤出 S″ 各收到 1 份，别的调用（含归属标注）没有
    for (p, n) in &off.gen_calls {
        let want = if (p.contains("计分途径") || p.contains("照这一局的计分办法")) && !p.contains("标上") { 1 } else { 0 };
        assert_eq!(*n, want, "{p}");
    }
    let d = &off.value["detail"]["plan"]["done"];
    assert_eq!(d["mode"], "off", "{d}");
    assert_eq!(d["why"], Json::Null, "{d}");
    assert_eq!(d["routes"], json!(计分途径), "{d}");
    assert_eq!(d["source"], "语境");
    assert!(d["rules_hash"].is_string() && d.get("rules").is_none(), "规则材料只记哈希与长度：{d}");
    assert_eq!(d["rules_len"], "规则：走到目标那一格得 10 分；每走一步扣 1 分。".chars().count(), "{d}");
    // 代码复核 O2：账本里有一条取规则材料的标记（类别、材料哈希），只一条；程序值里没有规则原文
    let 标记: Vec<Json> = off
        .ledger
        .entries
        .iter()
        .filter_map(|e| match e {
            Entry::Opaque { key, value, .. } if key.starts_with("unsure_fetch/") => Some(value.clone()),
            _ => None,
        })
        .collect();
    assert_eq!(标记.len(), 1, "{标记:?}");
    assert_eq!(标记[0]["need"], "语境");
    assert_eq!(标记[0]["rules_hash"], d["rules_hash"]);
    assert!(!off.value.to_string().contains("规则：走到目标那一格"), "程序值里不留规则原文");
    assert!(!base.ledger.entries.iter().any(|e| matches!(e, Entry::Opaque { key, .. } if key.starts_with("unsure_fetch/"))));
    assert_eq!(d["s_done"], "照计分办法，眼下最要紧的是哪一件事？", "{d}");
    assert_eq!(on.value["detail"]["plan"]["done"]["mode"], "on");
    // 4.z：与第三圈同配置逐字相同——动作题式、逐拍状态、动作；题树用 S
    let h = 动作题式(&base);
    assert_eq!(动作题式(&off), h);
    assert_eq!(动作题式(&on), h, "完成条件进语境、不进题面");
    assert_eq!(判断状态(&off.ledger, &h), 判断状态(&base.ledger, &h));
    assert_eq!(off.value["value"], base.value["value"]);
    assert_eq!(off.ctxs, base.ctxs);
    assert!(off.ctxs.iter().all(|c| !c.contains("这一局怎么计分")), "{:?}", off.ctxs);
    assert_eq!(off.value["detail"]["plan"]["tree"]["used"], "s");
    assert!(off.asked.iter().any(|t| t == "眼下最要紧的是哪一件事？"));
    // 4.d：每次动作题判断（含题树再问）的语境里都有完成条件；状态因此与 4.z 不同；题树问 S′
    assert!(
        !on.ctxs.is_empty() && on.ctxs.iter().all(|c| c.contains("这一局怎么计分：走到目标那一格得 10 分；每走一步扣 1 分")),
        "{:?}",
        on.ctxs
    );
    assert_ne!(判断状态(&on.ledger, &h), 判断状态(&off.ledger, &h));
    assert_eq!(on.value["detail"]["plan"]["tree"]["used"], "s_done");
    assert!(on.asked.iter().any(|t| t == "照计分办法，眼下最要紧的是哪一件事？"), "{:?}", on.asked);
    assert!(!on.asked.iter().any(|t| t == "眼下最要紧的是哪一件事？"), "4.d 不问 S");
    let t = 树行(&on);
    assert_eq!(t["end"], "decided", "{t}");
    // 完成条件是生成器产物（untrusted）进了每拍的状态：不多出别种告警（去掉位置比；off 比 base 多的只有唤出 S′、S″ 那两次 W-gen-count，
    // 归属标注要 1 给 1，不报）
    let 告警 = |r: &跑出| -> Vec<String> {
        let mut v: Vec<String> = r.report["warnings"].as_array().unwrap().iter()
            .map(|w| w.as_str().unwrap().split(':').next().unwrap().to_string()).collect();
        v.sort();
        v
    };
    assert_eq!(告警(&on), 告警(&off));
    let mut b = 告警(&base);
    b.push("W-gen-count".into());
    b.push("W-gen-count".into());
    b.sort();
    assert_eq!(告警(&off), b);
    for r in [&off, &on] {
        assert_eq!(r.report["violations"], 0, "{}", r.report["violations_detail"]);
        let rows = r.value["value"]["rows"].as_array().unwrap();
        assert!(rows.iter().all(|x| x["by"] == "judge" || x["by"] == "top"), "{rows:?}");
        assert_eq!(rows[0]["action"], "right", "{rows:?}");
    }
}

/// 第四圈 §2.1 反面：取不到语境（没注册取法、取法回 fail）或派生不出（不是 JSON 对象）时完成条件留空、原因进
/// detail.plan.done.why；done 为 on 时题树留空（S′ 没有），完成条件不进语境，链到末端按最大项
#[test]
fn 完成条件_取不到语境时留空() {
    let 取不到 = "unsure_source({fetch: fn(q, need, m) { fail(\"没有这一类材料\") }});";
    for (前置, 坏, why, gens) in [("", 0, "no-rules", 3), (取不到, 0, "no-rules", 3), (规则取法, 1, "parse", 4), (规则取法, 2, "empty", 4)] {
        let r = 带开关跑(&完成条件坏, 坏, 0, || 跑_四(", done: \"on\"", 前置));
        let d = &r.value["detail"]["plan"]["done"];
        assert_eq!(d["why"], why, "{前置} {d}");
        assert_eq!(d["routes"], json!([]), "{d}");
        assert_eq!(d["s_done"], Json::Null, "{d}");
        assert_eq!(d["s_done_why"], "done-empty", "{d}");
        assert_eq!(r.value["detail"]["plan"]["tree"]["why"], "done-empty");
        assert_eq!(r.gens, gens, "{前置}");
        assert!(r.ctxs.iter().all(|c| !c.contains("这一局怎么计分")), "{:?}", r.ctxs);
        assert!(!r.asked.iter().any(|t| t.contains("眼下最要紧")), "题树留空，不问子题");
        // 中间判断按取不到（没有题树）；有取法的那一例默认链照常取到语境再问、已决，其余到末端按最大项
        let rows = r.value["value"]["rows"].as_array().unwrap();
        assert_eq!(rows[0]["by"], if 前置 == 规则取法 { "judge" } else { "top" }, "{rows:?}");
        assert_eq!(树行(&r)["tree"]["why"], "no-tree");
        assert_eq!(r.report["violations"], 0, "{}", r.report["violations_detail"]);
    }
}

/// 第四圈 §2.2（正反两面）：缩小后为空——fallback 缺省（end，4.z）落到末端按首问最大项（left）；fallback 为 ctx（4.e）
/// 带 S 的回答在原集上再问，已决 right，tree 行记 fallback: ctx、n1 = 0；再问仍并列时到末端按最大项；
/// S 拿不准、mode off 时 fallback 不起作用
#[test]
fn 缩小为空_带子题回答在原集上再问() {
    let 跑e = |开关: &'static str| 带开关跑(&全判否, true, false, || 跑_四(开关, ""));
    let end = 跑e("");
    let rows = end.value["value"]["rows"].as_array().unwrap();
    assert_eq!((rows[0]["by"].as_str(), rows[0]["action"].as_str()), (Some("top"), Some("left")), "{rows:?}");
    let t = 树行(&end);
    assert_eq!(t["tree"]["n1"], 0, "{t}");
    assert!(t["tree"].get("fallback").is_none(), "{t}");
    let 去向有 = |r: &跑出, w: &str| {
        r.value["pending"].as_array().unwrap().iter().any(|p| p["via"].as_array().is_some_and(|v| v.iter().any(|x| x == w)))
    };
    assert!(去向有(&end, "缩小后为空"), "{}", end.value["pending"]);
    assert_eq!(end.value["detail"]["plan"]["tree"]["fallback"], "end");

    let ctx = 跑e(", fallback: \"ctx\"");
    let rows = ctx.value["value"]["rows"].as_array().unwrap();
    assert_eq!((rows[0]["by"].as_str(), rows[0]["action"].as_str()), (Some("judge"), Some("right")), "{rows:?}");
    let t = 树行(&ctx);
    assert_eq!(t["end"], "decided", "{t}");
    assert_eq!(t["tree"]["fallback"], "ctx", "{t}");
    assert_eq!(t["tree"]["n1"], 0, "{t}");
    assert!(t["tree"].get("why").is_none(), "{t}");
    assert!(!去向有(&ctx, "缩小后为空"), "{}", ctx.value["pending"]);
    assert_eq!(ctx.value["detail"]["plan"]["tree"]["fallback"], "ctx");
    // 再问在原集上（三个候选），语境里有子题的回答；缩小那组照样发了一次
    assert!(ctx.ctxs.iter().any(|c| c.contains("先判了一件更小的事")), "{:?}", ctx.ctxs);
    assert_eq!(ctx.asked.iter().filter(|x| x.contains("这件事吗")).count(), 3);
    assert_eq!(ctx.report["violations"], 0, "{}", ctx.report["violations_detail"]);

    // 再问仍并列：到末端按最大项
    let tie = 带开关跑(&再问并列, true, false, || 跑e(", fallback: \"ctx\""));
    let rows = tie.value["value"]["rows"].as_array().unwrap();
    assert_eq!(rows[0]["by"], "top", "{rows:?}");
    assert_eq!(树行(&tie)["tree"]["fallback"], "ctx");

    // S 拿不准：与缺省相同，落到末端
    let su = 带开关跑(&子题并列, true, false, || 跑e(", fallback: \"ctx\""));
    let t = 树行(&su);
    assert_eq!(t["tree"]["why"], "子题拿不准", "{t}");
    assert!(t["tree"].get("fallback").is_none(), "{t}");
    assert_eq!(su.value["value"]["rows"][0]["by"], "top");

    // mode off：不走题树，fallback 不起作用
    let off = 跑e(", tree: \"off\", fallback: \"ctx\"");
    assert_eq!(off.value["value"]["rows"][0]["by"], "top");
    assert!(!off.asked.iter().any(|x| x.contains("眼下最要紧")));
}

/// 第四圈 4.d、4.e（O9 同款）：带完成条件的一局、缩小为空再问的一局，审计重放只凭账本，值与首跑相同
#[test]
fn 第四圈_审计重放一致() {
    let d = 带开关跑(&审计重放, true, false, || 跑_四(", done: \"on\"", 规则取法));
    assert_eq!(d.report["replay_value"], d.value);
    let e = 带开关跑(&审计重放, true, false, || 带开关跑(&全判否, true, false, || 跑_四(", fallback: \"ctx\"", "")));
    assert_eq!(e.report["replay_value"], e.value);
}

// ———— 第三靶子第五圈（预注册 地基/规划/第三靶子-第五圈-预注册草稿.md §2.1）————

/// 带可达条件的途径进语境的那一份（reach 为 on）
fn 可达语境() -> String {
    format!(
        "这一局怎么计分（每条带拿到它要满足什么）：{}（什么时候计：走到目标那一格时；要先满足：还在数轴上；代价：原文没写）；\
         {}（什么时候计：每走一步；要先满足：原文没写；代价：原文没写）",
        计分途径[0], 计分途径[1]
    )
}

/// 第五圈 §2.1（正反两面）：两臂同一份出题（途径带可达条件、S′、S″、归属标注都派生，生成器 7 次）；
/// reach 为 off（5.z）时与第四圈 4.d 同配置：语境是不带条件的途径、题树问 S′；reach 为 on（5.p）时语境换成带条件的那一份、
/// 题树问 S″；动作题式不变
#[test]
fn 可达条件_两臂同一份出题_只在开时换语境与子题() {
    let z = 跑_四(", done: \"on\"", 规则取法);
    let pr = 跑_四(", done: \"on\", reach: \"on\"", 规则取法);
    for r in [&z, &pr] {
        assert_eq!(r.gens, 7, "抽模块、唤出 far、唤出 S、派生途径、唤出 S′、唤出 S″、归属标注");
        let d = &r.value["detail"]["plan"]["done"];
        assert_eq!(d["routes"], json!(计分途径), "对照臂用的途径文本不变：{d}");
        assert_eq!(
            d["reach"],
            json!([
                {"route": 计分途径[0], "when": "走到目标那一格时", "needs": ["还在数轴上"], "cost": "", "from": "走到目标那一格得 10 分"},
                {"route": 计分途径[1], "when": "每走一步", "needs": [], "cost": "", "from": "每走一步扣 1 分"}
            ]),
            "规整：null 记空文本：{d}"
        );
        assert_eq!(d["reach_missing"], 0, "{d}");
        assert_eq!(d["s_done"], "照计分办法，眼下最要紧的是哪一件事？", "{d}");
        assert_eq!(d["s_reach"], 子题三, "{d}");
        assert!(d["s_reach_form"].is_string() && d["s_reach_form"] != d["s_done_form"], "{d}");
        assert_eq!(d["s_reach_over"], json!(要紧的事), "{d}");
        assert_eq!(d["s_reach_why"], Json::Null, "{d}");
        assert_eq!(d["labels"], json!([1, 0]), "{d}");
        assert_eq!(d["labels_why"], Json::Null, "{d}");
        assert_eq!(r.report["violations"], 0, "{}", r.report["violations_detail"]);
        assert_eq!(r.value["value"]["rows"][0]["action"], "right");
        assert_eq!(树行(r)["end"], "decided");
    }
    // S″ 的唤出提示带可达条件的途径、收到规则原文 1 份；归属标注不带材料，提示里有途径编号与 S″ 的候选
    let 唤出三: Vec<_> = pr.gen_calls.iter().filter(|(p, _)| p.contains("拿得到")).collect();
    assert_eq!(唤出三.len(), 1, "{:?}", pr.gen_calls);
    assert!(唤出三[0].0.contains("要先满足：还在数轴上") && 唤出三[0].1 == 1, "{:?}", 唤出三);
    let 标注: Vec<_> = pr.gen_calls.iter().filter(|(p, _)| p.contains("标上")).collect();
    assert_eq!(标注.len(), 1);
    assert!(标注[0].0.contains("1. 走到目标那一格得 10 分") && 标注[0].0.contains("往目标走") && 标注[0].1 == 0, "{:?}", 标注);
    // 两臂出题相同（同一份预跑）：生成器提示逐字相同
    assert_eq!(z.gen_calls, pr.gen_calls);
    assert_eq!(动作题式(&z), 动作题式(&pr), "可达条件进语境、不进题面");
    // 5.z：与第四圈 4.d 同配置
    assert_eq!(z.value["detail"]["plan"]["done"]["reach_mode"], "off");
    assert_eq!(z.value["detail"]["plan"]["tree"]["used"], "s_done");
    assert!(!z.ctxs.is_empty() && z.ctxs.iter().all(|c| c.contains("这一局怎么计分：走到目标那一格得 10 分；每走一步扣 1 分")), "{:?}", z.ctxs);
    assert!(z.ctxs.iter().all(|c| !c.contains("每条带")), "{:?}", z.ctxs);
    assert!(z.asked.iter().any(|t| t == "照计分办法，眼下最要紧的是哪一件事？"));
    assert!(!z.asked.iter().any(|t| t == 子题三 || t == "眼下最要紧的是哪一件事？"), "5.z 只问 S′");
    // 5.p：语境带可达条件，题树问 S″
    assert_eq!(pr.value["detail"]["plan"]["done"]["reach_mode"], "on");
    assert_eq!(pr.value["detail"]["plan"]["tree"]["used"], "s_reach");
    assert_eq!(pr.value["detail"]["plan"]["tree"]["why"], Json::Null);
    let 语境 = 可达语境();
    assert!(!pr.ctxs.is_empty() && pr.ctxs.iter().all(|c| c.contains(&语境)), "{:?}", pr.ctxs);
    assert!(pr.asked.iter().any(|t| t == 子题三), "{:?}", pr.asked);
    assert!(!pr.asked.iter().any(|t| t == "照计分办法，眼下最要紧的是哪一件事？" || t == "眼下最要紧的是哪一件事？"), "5.p 只问 S″");
    let h = 动作题式(&z);
    assert_ne!(判断状态(&pr.ledger, &h), 判断状态(&z.ledger, &h), "语境不同，状态不同");
}

/// 第五圈 §2.1 反面一：生成器给的途径没有可达条件（第四圈的纯字符串）——条件都空、reach_missing 记条数（when、needs、cost 都空才算）；
/// 5.p 照样跑，语境里写「原文没写」（arm3.py 的硬门槛读 reach_missing 拒跑真机）
#[test]
fn 可达条件_途径没有条件() {
    let r = 带开关跑(&完成条件坏, 3, 0, || 跑_四(", done: \"on\", reach: \"on\"", 规则取法));
    let d = &r.value["detail"]["plan"]["done"];
    assert_eq!(d["why"], Json::Null, "{d}");
    assert_eq!(d["routes"], json!(计分途径), "{d}");
    assert_eq!(d["reach_missing"], 2, "{d}");
    assert!(d["reach"].as_array().unwrap().iter().all(|x| x["when"] == "" && x["needs"] == json!([]) && x["cost"] == "" && x["from"] == ""), "{d}");
    assert!(r.ctxs.iter().all(|c| c.contains("走到目标那一格得 10 分（什么时候计：原文没写；要先满足：原文没写；代价：原文没写）")), "{:?}", r.ctxs);
    assert_eq!(r.value["detail"]["plan"]["tree"]["used"], "s_reach");
    assert_eq!(r.report["violations"], 0, "{}", r.report["violations_detail"]);
}

/// 第五圈 §2.1 反面二：归属标注不合格（长度不等于候选数、不是 JSON 对象、超出途径条数）——labels 留空、原因进明细；
/// 标注只供分列，不影响运行
#[test]
fn 可达条件_标注不合格时留空并写明原因() {
    let 好 = 跑_四(", done: \"on\", reach: \"on\"", 规则取法);
    for (坏, why) in [(1u8, "len"), (2, "parse"), (3, "range")] {
        let r = 带开关跑(&标注坏, 坏, 0, || 跑_四(", done: \"on\", reach: \"on\"", 规则取法));
        let d = &r.value["detail"]["plan"]["done"];
        assert_eq!(d["labels"], json!([]), "{d}");
        assert_eq!(d["labels_why"], why, "{d}");
        assert_eq!(d["s_reach"], 子题三, "{d}");
        assert_eq!(r.value["value"], 好.value["value"], "标注不进运行");
        assert_eq!(r.report["violations"], 0, "{}", r.report["violations_detail"]);
    }
}

/// 第五圈 §2.1 反面三：S″ 唤不出——reach 为 on 时题树留空、语境也不加（原因进明细），链到末端；reach 为 off 不受影响；
/// 途径留空（取不到规则材料）时 S″ 不唤出、不标注
#[test]
fn 可达条件_子题三没出来时题树与语境都留空() {
    let pr = 带开关跑(&子题三空, true, false, || 跑_四(", done: \"on\", reach: \"on\"", 规则取法));
    let d = &pr.value["detail"]["plan"]["done"];
    assert_eq!(d["s_reach"], Json::Null, "{d}");
    assert_eq!(d["s_reach_why"], "s-not-elicited", "{d}");
    assert_eq!(d["labels_why"], "no-s", "{d}");
    assert_eq!(pr.value["detail"]["plan"]["tree"]["why"], "s-not-elicited");
    assert!(pr.ctxs.iter().all(|c| !c.contains("这一局怎么计分")), "{:?}", pr.ctxs);
    assert!(!pr.asked.iter().any(|t| t.contains("眼下最要紧")), "题树留空，不问子题");
    assert_eq!(树行(&pr)["tree"]["why"], "no-tree");
    assert_eq!(pr.report["violations"], 0, "{}", pr.report["violations_detail"]);
    let z = 带开关跑(&子题三空, true, false, || 跑_四(", done: \"on\"", 规则取法));
    assert_eq!(z.value["detail"]["plan"]["tree"]["used"], "s_done");
    assert!(z.ctxs.iter().all(|c| c.contains("这一局怎么计分：")), "{:?}", z.ctxs);
    for 前置 in ["", 规则取法] {
        let 坏 = if 前置.is_empty() { 0 } else { 2 };
        let r = 带开关跑(&完成条件坏, 坏, 0, || 跑_四(", done: \"on\", reach: \"on\"", 前置));
        let d = &r.value["detail"]["plan"]["done"];
        assert_eq!((d["s_reach_why"].as_str(), d["labels_why"].as_str()), (Some("done-empty"), Some("no-s")), "{d}");
        assert_eq!(d["reach"], json!([]), "{d}");
        assert!(!r.gen_calls.iter().any(|(p, _)| p.contains("拿得到") || p.contains("标上")));
        assert!(r.ctxs.iter().all(|c| !c.contains("这一局怎么计分")), "{:?}", r.ctxs);
    }
}

/// 第五圈 5.p（O9 同款）：带可达条件的一局，审计重放只凭账本，值与首跑相同
#[test]
fn 第五圈_审计重放一致() {
    let r = 带开关跑(&审计重放, true, false, || 跑_四(", done: \"on\", reach: \"on\"", 规则取法));
    assert_eq!(r.report["replay_value"], r.value);
}

// ———— 第四圈代码复核（review-r4code）收进来的两条：跨库对照与 G2 矩阵（O7）————

/// 换库跑同一个驱动程序（`lib` 是 lib/ 下的目录名）；跨库对照用
fn 驱动程序_库(lib: &str, play: &str, calls: usize, 前置: &str) -> String {
    format!(
        "import \"../../lib/{lib}/drive.jpp\";\nbudget {{calls: {calls}, cost: 0, depth: 8192}};\n{前置}\n\
         let r = purpose_drive(\"走到目标位置。\", \"line\", {{reset: [2], bound: 200{play}}});\n\
         {{value: r.value, pending: map(r.pending, fn(p) {{ {{cause: p.cause, pos: p.pos, via: p.via, exit: p.exit}} }}), detail: r.detail}}\n"
    )
}

/// 账本里的判断，按出现顺序：(题式哈希, 状态哈希, 读数)；`qs` 为空时取全部
fn 判断序列(l: &Ledger, qs: &[String]) -> Vec<(String, String, String)> {
    l.entries
        .iter()
        .filter_map(|e| match e {
            Entry::Judge { jkey: Some(k), answer, .. } if qs.is_empty() || qs.contains(&k.q) => {
                Some((k.q.clone(), k.state.clone(), format!("{answer:?}")))
            }
            _ => None,
        })
        .collect()
}

/// 去掉随库源码偏移变的字段（账本键、站点、哈希）
fn 去键(v: &Json) -> Json {
    match v {
        Json::Object(m) => Json::Object(
            m.iter()
                .filter(|(k, _)| !k.ends_with("key") && !k.ends_with("keys") && *k != "site" && *k != "hash")
                .map(|(k, x)| (k.clone(), 去键(x)))
                .collect(),
        ),
        Json::Array(a) => Json::Array(a.iter().map(去键).collect()),
        _ => v.clone(),
    }
}

/// 一种配置下与库偏移无关的逐拍输出：动作题式、逐拍判断（动作题、S、缩小那组）的 (题式, 状态, 读数)、行、语境、
/// 默认链报告行与 pending（去键）；没有取法时另记全部判断
fn 跨库快照(lib: &str, 前置: &str, 全否: bool) -> Json {
    let r = 带开关跑(&全判否, 全否, false, || {
        带开关跑(&选中间判断, true, false, || 跑_源(&驱动程序_库(lib, "", 2000, 前置), Some(0), None, false))
    });
    let h = 动作题式(&r);
    let mut qs = vec![h.clone()];
    for k in ["s_form", "narrow_form"] {
        if let Some(x) = r.value["detail"]["plan"]["tree"][k].as_str() {
            qs.push(x.to_string());
        }
    }
    let 序 = |v: Vec<(String, String, String)>| json!(v.into_iter().map(|(a, b, c)| json!([a, b, c])).collect::<Vec<_>>());
    json!({
        "act_form": h,
        "step_judges": 序(判断序列(&r.ledger, &qs)),
        "all_judges": if 前置.is_empty() { 序(判断序列(&r.ledger, &[])) } else { Json::Null },
        "rows": r.value["value"],
        "ctxs": r.ctxs,
        "unsure_default": 去键(&r.report["unsure_default"]),
        "pending": 去键(&r.value["pending"]),
    })
}

/// 4.z 是同库对照（预注册 §3）：done、fallback 缺省时，新库的逐拍输出与第三圈的库（26bc0d008 的 lib/derive）逐字相同——
/// 动作题式、逐拍判断的状态与读数、行、语境、默认链报告行、pending；没有取法时连全部判断都相同（有取法时新库多出的只是
/// 出题阶段派生计分途径、唤出 S′ 与它的闸门判断）。第三圈那一份录在 tests/fixtures/z0886_r3_snapshot.json：
/// 重录时把 26bc0d008 的 lib/derive 拷到 lib/derive_r3，`JPP_R3_SNAPSHOT=record` 跑这条，录完删掉拷贝
#[test]
fn 第四圈_缺省时与第三圈库逐字相同() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/z0886_r3_snapshot.json");
    let 配置 = [("无取法", "", false), ("有取法", 规则取法, false), ("有取法+全判否", 规则取法, true)];
    if std::env::var("JPP_R3_SNAPSHOT").is_ok_and(|v| v == "record") {
        let mut m = serde_json::Map::new();
        for (名, 前置, 全否) in 配置 {
            m.insert(名.to_string(), 跨库快照("derive_r3", 前置, 全否));
        }
        std::fs::write(&path, serde_json::to_string_pretty(&Json::Object(m)).unwrap() + "\n").unwrap();
    }
    let want: Json = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    for (名, 前置, 全否) in 配置 {
        let got = 跨库快照("derive", 前置, 全否);
        let w = &want[名];
        for k in ["act_form", "step_judges", "all_judges", "rows", "ctxs", "unsure_default", "pending"] {
            assert_eq!(got[k], w[k], "[{名}] {k}");
        }
    }
}

/// G2 矩阵（复核）：done × fallback 四种组合（第五圈加 reach 为 on 的两种），预算 4–30 次调用，每档都跑完、不 panic、违规 0；缩小为空再问时预算停在
/// 再问之前的，按首问（原集）的最大项行动（left），不取缩小那组的读数
#[test]
fn 第四圈_预算矩阵不违规() {
    let mut 失败 = vec![];
    for (done, fb, reach) in [("off", "end", "off"), ("on", "end", "off"), ("off", "ctx", "off"), ("on", "ctx", "off"), ("on", "end", "on"), ("on", "ctx", "on")] {
        for calls in 4usize..=30 {
            let 开关 = format!(", done: \"{done}\", fallback: \"{fb}\", reach: \"{reach}\"");
            全判否.with(|c| c.set(true));
            选中间判断.with(|c| c.set(true));
            let src = 驱动程序_带("[2]", &开关, calls, 规则取法);
            let res = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| 跑_源(&src, Some(0), None, false)));
            全判否.with(|c| c.set(false));
            选中间判断.with(|c| c.set(false));
            let Ok(r) = res else {
                失败.push(format!("{done}/{fb}/{reach}/{calls}: panic"));
                continue;
            };
            let v = r.report["violations"].as_u64().unwrap_or(999);
            if v != 0 {
                失败.push(format!("{done}/{fb}/{reach}/{calls}: violations={v} {}", r.report["violations_detail"]));
            }
            let rows = r.value["value"]["rows"].as_array().cloned().unwrap_or_default();
            for u in r.report["unsure_default"].as_array().cloned().unwrap_or_default() {
                if u["tree"]["fallback"] == "ctx" && u["via"] == "预算、按最后读数最大项" && rows[0]["action"] != "left" {
                    失败.push(format!("{done}/{fb}/{reach}/{calls}: 预算停在再问之前却没按首问最大项：{rows:?}"));
                }
            }
        }
    }
    assert!(失败.is_empty(), "{失败:#?}");
}
