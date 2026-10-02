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

fn 模块() -> Json {
    json!({"material": "数轴上的位置", "predicates": [{"text": "离目标还远", "cut": "binary", "field": "far", "cut_from": "system"}],
           "request": null, "presupposition": null, "context": null, "reference": null, "budget": null, "unsure": null, "done": null})
}

/// 能把开局观测分成两边的前提（目标在右边的局判是，在左边的局判否）
const 前提题: &str = "材料里写明了位置在目标左边吗？";

fn 生成(p: &str) -> Vec<Json> {
    if p.contains("字面前提题") {
        vec![json!({"op": "test", "text": 前提题, "if_false": {"falls_to": "unanswerable"}})]
    } else if p.starts_with("下面是一段目的") {
        vec![模块()]
    } else if p.contains("离目标还远") {
        vec![json!({"op": "test", "text": "离目标还远吗？"})]
    } else {
        vec![]
    }
}

/// 动作题：往目标走；`tie_t` 那一步给并列
fn 读数(q: &Question, s: &State, tie_t: Option<i64>) -> Answer {
    let t = &q.text;
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
        if tie_t == Some(tt) {
            return Answer::Choice(vec![0.4, 0.4, 0.2]);
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
    /// 运行报告
    report: Json,
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
    format!(
        "import \"../../lib/derive/drive.jpp\";\nbudget {{calls: {calls}, cost: 0, depth: 8192}};\n\
         let r = purpose_drive(\"走到目标位置。\", \"line\", {{reset: {resets}, bound: 200{play}}});\n\
         {{value: r.value, pending: map(r.pending, fn(p) {{ {{cause: p.cause, pos: p.pos}} }}), detail: r.detail}}\n"
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
    let ctxs = Rc::new(RefCell::new(Vec::<String>::new()));
    let c2 = ctxs.clone();
    let asked = Rc::new(RefCell::new(Vec::<String>::new()));
    let a2 = asked.clone();
    let ports = Ports::new()
        .with(FnPort::judge("fixed-0", move |s, qs| {
            a2.borrow_mut().extend(qs.iter().map(|q| q.text.clone()));
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
        .with(FnPort::generate("fixed-0", move |p, _c, _n, _r| {
            *g2.borrow_mut() += 1;
            Ok(GenResult { outputs: 生成(p), ..Default::default() })
        }))
        .with(FnPort::ask("fixed-0", |_s, _q| Err(EffectError("不该 ask".into()))));
    let mut ledger = Ledger::new();
    let calib = CalibStore::new();
    let out = Session::new(ports, &calib, &acts)
        .with_companions(jpp::interp::CompanionMode::Off)
        .run(&program, &EntryArgs::default(), &mut ledger)
        .unwrap_or_else(|e| panic!("{}", e.render()));
    let gens = *gens.borrow();
    let ctxs = ctxs.borrow().clone();
    let asked = asked.borrow().clone();
    let report = json!({"budget": out.budget.as_ref().map(|b| json!({"exhausted": b.exhausted, "unsent": b.unsent})),
                        "selections": out.selections});
    跑出 { value: out.value_json(), ledger, gens, ctxs, asked, report }
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
    assert_eq!(a.gens, 2, "抽模块 + 唤出（过程入口不派生前提，裁定七十五）");
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

#[test]
fn 并列取空动作_未决带步号转交() {
    let r = 跑("[2]", Some(0), None);
    let v = &r.value["value"];
    let rows = v["rows"].as_array().unwrap();
    assert_eq!(rows[0]["by"], "idle", "{v}");
    assert_eq!(rows[0]["action"], "wait");
    assert!(rows[1..].iter().all(|x| x["by"] == "judge"), "{v}");
    let pend = r.value["pending"].as_array().unwrap();
    let dropped = r.value["detail"]["dropped"].as_array().unwrap();
    // 并列先走默认链：放弃的进 detail.dropped，其余转交进 pending；两处合起来恰好记着第 0 步
    let 记着 = pend.iter().any(|p| p["pos"] == 0) || dropped.iter().any(|d| d["pos"] == 0);
    assert!(记着, "pending {pend:?} dropped {dropped:?}");
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
                let n = pend.iter().filter(|p| p["cause"] == "budget" && p["pos"] == x["step"]).count();
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
    assert_eq!(r.gens, 2, "不调前提派生");
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
