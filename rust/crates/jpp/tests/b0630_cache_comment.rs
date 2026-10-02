//! B0630（收尾计划②，键与格式稳定）：库只改注释时，旧缓存全部命中；真改了题面的那一处不命中。
//!
//! 为什么：库里每个定义的站点身份曾含源码字节偏移。库文件里加几行注释，后面所有定义的偏移都变，旧账本攒下的判断、
//! 生成就对不上了。站点身份改为「定义路径 + 定义内序号」后，注释不再影响键。
//!
//! 做法：用 Z0886 的过程入口（`lib/derive/drive.jpp`，会产生判断与生成）。先在原库上跑一遍写账本、建缓存索引；
//! 再在临时目录复制一份 lib，给每个 `.jpp` 文件的头部和每个顶层定义前加注释行（不改仓库里的 lib），用旧缓存重跑。
//! 正面：判断器与生成器一次都不调用，每条判断与生成都带复用来源，值与原来相同。
//! 对照：只把动作题的题面模板（`lib/derive/rules.jpp` 的 `purpose_act_pred`）改一个字，重跑——动作题必须重判，
//! 说明上面的命中来自键不含偏移，不是缓存把什么都放过。
//!
//! 夹具取自 `z0886_drive.rs` 的基础部分（世界、模块、生成、读数；去掉了各反面用例的开关）。
use jpp::effects::{CalibStore, EffectError, FnPort, GenResult, JudgeResult, Ports};
use jpp::interp::{TaintOut, json_to_value};
use jpp::ledger::{Entry, Ledger};
use jpp::store::CacheIndex;
use jpp::value::{Answer, Question, State, Value};
use jpp::{ActionRegistry, EntryArgs, Session};
use serde_json::{Value as Json, json};
use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT: AtomicU64 = AtomicU64::new(0);

/// 世界：位置 pos 从 0 出发，走到 goal 即结束；最多 12 步
fn 世界(req: &Json) -> Json {
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
    json!({"state": st, "obs": {"t": t, "pos": st["pos"], "goal": st["goal"]}, "actions": ["left", "right", "wait"], "idle": "wait",
           "done": done, "hash": format!("h{}-{}-{}", st["goal"], st["pos"], t),
           "result": {"reached": st["pos"] == st["goal"]}})
}

const 要紧的事: [&str; 2] = ["往目标走", "原地等"];

fn 模块() -> Json {
    json!({"material": "数轴上的位置",
           "predicates": [{"text": "离目标还远", "cut": "binary", "field": "far", "cut_from": "system"}],
           "request": null, "presupposition": null, "context": null, "reference": null, "budget": null, "unsure": null, "done": null})
}

fn 生成(p: &str) -> Vec<Json> {
    if p.starts_with("下面是一段目的") {
        vec![模块()]
    } else if p.contains("眼下最要紧") {
        vec![json!({"op": "select", "text": "眼下最要紧的是哪一件事？", "over": 要紧的事})]
    } else if p.contains("离目标还远") {
        vec![json!({"op": "test", "text": "离目标还远吗？"})]
    } else {
        vec![]
    }
}

fn 读数(q: &Question, s: &State) -> Answer {
    let t = &q.text;
    if t.starts_with("题「") || t.starts_with("判断题「") || t.starts_with("把题「") {
        return Answer::Noul(if t.contains("需要分别回答") { 0.1 } else { 0.5 });
    }
    if t.contains("为什么拿不准") {
        let mut v = vec![0.0; s.over.len()];
        v[0] = 1.0;
        return Answer::Choice(v);
    }
    if t.contains("眼下最要紧的是哪一件事") {
        return Answer::Choice(vec![0.9, 0.1]);
    }
    if t.contains("这件事吗") {
        return Answer::Noul(if t.contains("「right」") { 0.9 } else if t.contains("「left」") { 0.1 } else { 0.5 });
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
        let (pos, goal) = (c["pos"].as_i64().unwrap(), c["goal"].as_i64().unwrap());
        let want = if pos < goal { "right" } else if pos > goal { "left" } else { "wait" };
        Answer::Choice(s.over.iter().map(|m| if m.content.as_str() == Some(want) { 0.8 } else { 0.1 }).collect())
    } else if t.contains("离目标还远") {
        Answer::Noul(0.7)
    } else {
        Answer::Noul(0.5)
    }
}

const 驱动: &str = "import \"../../lib/derive/drive.jpp\";\nimport \"../../lib/b0630_probe.jpp\";\nbudget {calls: 2000, cost: 0, depth: 8192};\n\n\
    let r = purpose_drive(\"走到目标位置。\", \"line\", {reset: [3], bound: 200});\n\
    {probe: b0630_probe(), value: r.value, pending: map(r.pending, fn(p) { {cause: p.cause, pos: p.pos, via: p.via, exit: p.exit} }), detail: r.detail}\n";

/// 闭包变换的载体：放在库里的第二个文件，闭包的方法身份进 `transform` 的缓存键（B0630 前含源码偏移，起取不含位置的 `shape_hash`）
const 探针库: &str = "fn b0630_probe() {\n    let t = transform(fn(m) { \"变 \" + content(m) }, mat(\"丙\"));\n    content(t)\n}\n";

/// 工作目录：`<根>/lib` 是库的一份拷贝，程序放 `<根>/target/p/p.jpp`，相对路径 `../../lib/…` 刚好落在拷贝上
struct 沙盒(PathBuf);

impl 沙盒 {
    fn 新(改库: impl Fn(&Path)) -> 沙盒 {
        let crate_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let root = std::env::temp_dir().join(format!("b0630-{}-{}", std::process::id(), NEXT.fetch_add(1, Ordering::Relaxed)));
        复制(&crate_root.join("lib"), &root.join("lib"));
        std::fs::write(root.join("lib/b0630_probe.jpp"), 探针库).unwrap();
        改库(&root.join("lib"));
        std::fs::create_dir_all(root.join("target/p")).unwrap();
        std::fs::write(root.join("target/p/p.jpp"), 驱动).unwrap();
        沙盒(root)
    }
    fn 程序(&self) -> PathBuf {
        self.0.join("target/p/p.jpp")
    }
}

impl Drop for 沙盒 {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn 复制(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for e in std::fs::read_dir(from).unwrap() {
        let e = e.unwrap();
        let (src, dst) = (e.path(), to.join(e.file_name()));
        if src.is_dir() {
            复制(&src, &dst);
        } else {
            std::fs::copy(&src, &dst).unwrap();
        }
    }
}

fn 遍历_jpp(dir: &Path, f: &mut dyn FnMut(&Path)) {
    for e in std::fs::read_dir(dir).unwrap() {
        let p = e.unwrap().path();
        if p.is_dir() {
            遍历_jpp(&p, f);
        } else if p.extension().is_some_and(|x| x == "jpp") {
            f(&p);
        }
    }
}

/// 只加注释：每个 `.jpp` 头部三行，每个顶层定义（行首的 `fn ` / `let ` / `export `）前一行。
/// 注释行里不放任何会被读成代码的东西；库里的字符串字面量不跨行含这些行首（跑通与否会暴露）。
fn 加注释(lib: &Path) {
    遍历_jpp(lib, &mut |p| {
        let src = std::fs::read_to_string(p).unwrap();
        let mut out = String::from("// B0630 测试：这几行只是注释，不改库的任何含义。\n// 第二行。\n// 第三行。\n");
        for line in src.lines() {
            if line.starts_with("fn ") || line.starts_with("let ") || line.starts_with("export ") {
                out.push_str("// 定义前的一行注释（B0630 测试加的）\n");
            }
            out.push_str(line);
            out.push('\n');
        }
        std::fs::write(p, out).unwrap();
    });
}

/// 真改题面：动作题的题面模板多一个字
fn 改题面(lib: &Path) {
    let f = lib.join("derive/rules.jpp");
    let src = std::fs::read_to_string(&f).unwrap();
    let from = "，下一步最该做的动作\" },";
    assert_eq!(src.matches(from).count(), 1, "找不到 purpose_act_pred 的题面");
    std::fs::write(&f, src.replace(from, "，下一步最该做的动作（改）\" },")).unwrap();
}

struct 跑出 {
    value: Json,
    ledger: Ledger,
    /// 判断器收到的题数、生成器被调用的次数（命中缓存的不进这两个数）
    judged: usize,
    gens: usize,
}

fn 跑(sandbox: &沙盒, cache: Option<&CacheIndex>) -> 跑出 {
    跑_账本(sandbox, cache, None)
}

/// `old` 给了就接着旧账本跑（`resume`：按账本键查，键里带站点）；不给则从空账本起跑
fn 跑_账本(sandbox: &沙盒, cache: Option<&CacheIndex>, old: Option<Ledger>) -> 跑出 {
    跑_内(sandbox, cache, old, false).unwrap_or_else(|e| panic!("{e}"))
}

/// 审计重放：只凭旧账本（`--replay`），缺记录报 E-replay
fn 重放(sandbox: &沙盒, old: Ledger) -> Result<跑出, String> {
    跑_内(sandbox, None, Some(old), true)
}

fn 跑_内(sandbox: &沙盒, cache: Option<&CacheIndex>, old: Option<Ledger>, replay: bool) -> Result<跑出, String> {
    let loaded = jpp::syntax::loader::load(&sandbox.程序()).unwrap_or_else(|e| panic!("装载：{e:?}"));
    let program = jpp::lower(&loaded.program).unwrap_or_else(|e| panic!("lower：{e:?}"));
    let mut acts = ActionRegistry::new();
    acts.register("env:step", 0.0, true, TaintOut::Untrusted, move |args: &[Value]| Ok(json_to_value(&世界(&args[0].to_json()))));
    let judged = Rc::new(RefCell::new(0usize));
    let j2 = judged.clone();
    let gens = Rc::new(RefCell::new(0usize));
    let g2 = gens.clone();
    let ports = Ports::new()
        .with(FnPort::judge("fixed-0", move |s, qs| {
            *j2.borrow_mut() += qs.len();
            Ok::<_, EffectError>(JudgeResult {
                answers: qs.iter().map(|q| 读数(q, s)).collect(),
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
    let resuming = old.is_some();
    let mut ledger = old.unwrap_or_else(Ledger::new);
    let calib = CalibStore::new();
    let mut s = Session::new(ports, &calib, &acts).with_companions(jpp::interp::CompanionMode::Off);
    if let Some(c) = cache {
        s = s.with_cache(c);
    }
    let out = if replay {
        s.replay(&program, &EntryArgs::default(), &mut ledger)
    } else if resuming {
        s.resume(&program, &EntryArgs::default(), &mut ledger)
    } else {
        s.run(&program, &EntryArgs::default(), &mut ledger)
    }
    .map_err(|e| e.render())?;
    // B0630：结构化键法下进键的站点都查得到结构化标识（旧键法重放不查表，恒为 0）
    assert_eq!(out.site_key_fallback, 0, "站点回退成偏移");
    let (judged, gens) = (*judged.borrow(), *gens.borrow());
    Ok(跑出 { value: out.value_json(), ledger, judged, gens })
}

/// 账本里判断、生成、闭包变换的条目数与其中带复用来源的条数
#[derive(Debug, PartialEq, Eq)]
struct 统计 {
    j: usize,
    jr: usize,
    g: usize,
    gr: usize,
    t: usize,
    tr: usize,
}

fn 复用统计(l: &Ledger) -> 统计 {
    let mut n = 统计 { j: 0, jr: 0, g: 0, gr: 0, t: 0, tr: 0 };
    for e in &l.entries {
        match e {
            Entry::Judge { reused_from, .. } => {
                n.j += 1;
                n.jr += reused_from.is_some() as usize;
            }
            Entry::Effect { kind, reused_from, .. } if kind == "gen" => {
                n.g += 1;
                n.gr += reused_from.is_some() as usize;
            }
            Entry::Effect { kind, reused_from, .. } if kind == "transform" => {
                n.t += 1;
                n.tr += reused_from.is_some() as usize;
            }
            _ => {}
        }
    }
    n
}

/// 账本里判断与效应条目的键摘要，按序（含站点）
fn 键序列(l: &Ledger) -> Vec<String> {
    l.entries
        .iter()
        .filter_map(|e| match e {
            Entry::Judge { jkey: Some(k), .. } => Some(format!("judge {}", k.digest())),
            Entry::Effect { ekey: Some(k), .. } => Some(format!("{} {}", k.kind, k.digest())),
            _ => None,
        })
        .collect()
}

fn 索引(first: &跑出) -> CacheIndex {
    CacheIndex::build(&[("first.jsonl".to_string(), first.ledger.clone())])
}

/// 第一趟：原库、空账本；返回（原沙盒、第一趟结果）
fn 第一趟() -> (沙盒, 跑出) {
    let 原 = 沙盒::新(|_| {});
    let first = 跑(&原, None);
    assert!(first.judged > 0 && first.gens > 0, "夹具得真产生判断与生成：判断 {} 生成 {}", first.judged, first.gens);
    let n = 复用统计(&first.ledger);
    assert!(n.j > 0 && n.g > 0 && n.t > 0, "夹具得含判断、生成、闭包变换各至少一条：{n:?}");
    (原, first)
}

fn 加了注释的沙盒(原: &沙盒) -> 沙盒 {
    let 改 = 沙盒::新(加注释);
    // 前提自检：注释确实让库文件变了（不然这条测试什么也没测）；两个 lib 文件都变
    for f in ["lib/derive/drive.jpp", "lib/b0630_probe.jpp"] {
        let (a, b) = (std::fs::read_to_string(原.0.join(f)).unwrap(), std::fs::read_to_string(改.0.join(f)).unwrap());
        assert!(b.len() > a.len() && b.contains("B0630 测试"), "{f}");
    }
    改
}

// 注意：库是字节哈希，改过注释后账本头的 lib_version 必然变，运行会出 `W-header: lib_version`——这里不断言「无告警」。

/// 跨运行缓存（`CacheIndex`）的判断与生成：缓存键本来就不含调用位置，今天已经全命中。守着它，键改法不许把它弄坏。
/// 出题预跑复用（`--preview <预跑目录>`，arm3.py 与 plan_preview.py 都转成 `--cache <预跑目录>/cache`）走的就是这一路，
/// 不是 resume；它之前整份不命中，原因在键的别的分量（题面、状态、渲染版本、生成提示）变了，不在偏移。
/// 闭包变换不在这条里断言，另有一条（B0630 起闭包方法身份取不含源码位置的 `shape_hash`，那条也命中）。
#[test]
fn 跨运行缓存_库只改注释_判断与生成全命中() {
    let (原, first) = 第一趟();
    let ix = 索引(&first);
    let c = ix.counts();
    assert!(c.judge > 0 && c.gen_ > 0, "旧缓存里要有判断与生成：{c:?}");
    let second = 跑(&加了注释的沙盒(&原), Some(&ix));
    assert_eq!(second.judged, 0, "判断一次也不该重调");
    assert_eq!(second.gens, 0, "生成一次也不该重调");
    assert_eq!(second.value, first.value, "值与原来相同");
    let n = 复用统计(&second.ledger);
    assert_eq!((n.jr, n.gr), (n.j, n.g), "每条判断、每条生成都是复用来源：{n:?}");
}

/// 预注册 §六 (iii)：带旧账本的缓存跑改过注释的库，判断、生成、闭包变换的命中数都等于各自条目数，请求全 0
#[test]
fn 跨运行缓存_库只改注释_闭包变换也命中() {
    let (原, first) = 第一趟();
    let ix = 索引(&first);
    let second = 跑(&加了注释的沙盒(&原), Some(&ix));
    assert_eq!((second.judged, second.gens), (0, 0), "请求全 0");
    assert_eq!(second.value, first.value);
    let n = 复用统计(&second.ledger);
    assert_eq!((n.jr, n.gr, n.tr), (n.j, n.g, n.t), "命中数等于条目数：{n:?}");
}

/// 对照：动作题的题面模板改一个字，动作题必须重判
#[test]
fn 跨运行缓存_库改了动作题题面_动作题不命中() {
    let (_原, first) = 第一趟();
    let ix = 索引(&first);
    let second = 跑(&沙盒::新(改题面), Some(&ix));
    assert!(second.judged > 0, "题面真改了：动作题必须重判，不能命中旧缓存");
    let n = 复用统计(&second.ledger);
    assert!(n.jr < n.j, "动作题那些判断不带复用来源：{n:?}");
}

/// 预注册 §六 (i)：注释改动前后两趟，账本键逐个相同
#[test]
fn 注释前后两趟_账本键逐个相同() {
    let (原, first) = 第一趟();
    let second = 跑(&加了注释的沙盒(&原), None);
    let (a, b) = (键序列(&first.ledger), 键序列(&second.ledger));
    assert_eq!(a.len(), b.len(), "条数相同");
    let 不同 = a.iter().zip(&b).filter(|(x, y)| x != y).count();
    assert_eq!(不同, 0, "{} 条里有 {不同} 条键变了", a.len());
}

/// 预注册 §六 (ii)：用改动前的账本 `--replay` 改动后的库，新增调用 0，值不变
#[test]
fn 旧账本重放_库只改注释_新增调用0_值不变() {
    let (原, first) = 第一趟();
    let second = 重放(&加了注释的沙盒(&原), first.ledger.clone()).unwrap_or_else(|e| panic!("重放失败：{e}"));
    assert_eq!((second.judged, second.gens), (0, 0), "新增调用 0");
    assert_eq!(second.value, first.value, "值不变");
}

/// 旧账本续跑（`resume`，按账本键查）：库只改注释，不该重调
#[test]
fn 旧账本续跑_库只改注释_全命中() {
    let (原, first) = 第一趟();
    let second = 跑_账本(&加了注释的沙盒(&原), None, Some(first.ledger.clone()));
    assert_eq!((second.judged, second.gens), (0, 0), "请求全 0");
    assert_eq!(second.value, first.value);
    assert_eq!(second.ledger.entries.len(), first.ledger.entries.len(), "续跑不该往账本里加条目");
}

/// 对照：库一字不改，旧账本续跑全命中、重放通过——证明上面几条的红来自注释，不是续跑、重放本身
#[test]
fn 旧账本续跑与重放_库不变_全命中() {
    let (原, first) = 第一趟();
    let second = 跑_账本(&原, None, Some(first.ledger.clone()));
    assert_eq!((second.judged, second.gens), (0, 0));
    assert_eq!(second.value, first.value);
    assert_eq!(second.ledger.entries.len(), first.ledger.entries.len());
    let r = 重放(&原, first.ledger.clone()).unwrap_or_else(|e| panic!("重放失败：{e}"));
    assert_eq!((r.judged, r.gens), (0, 0));
    assert_eq!(r.value, first.value);
}

/// 对照：续跑时动作题题面改了一个字，动作题必须重判
#[test]
fn 旧账本续跑_库改了动作题题面_动作题重判() {
    let (_原, first) = 第一趟();
    let second = 跑_账本(&沙盒::新(改题面), None, Some(first.ledger.clone()));
    assert!(second.judged > 0, "题面真改了：动作题必须重判");
}
