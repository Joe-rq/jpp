//! `sieve` 收作者声明线（`{line: {declare: {hi, lo?}}}` 原样交给每个元素的 `cut`），搭配库 `search`、`verify`、
//! `judged_graph`、`judged_bipartite` 的 `opts.line` 透传。B128 当初没接到 `sieve` 上的一处：真机上没有校准记录，
//! 经过 `sieve` 的组合一律 `unsure(cold)`，比赛现场判不出结论。
//!
//! 判断端口是「真机形状的替身」：闭包端口按材料回读数，**校准库为空**（真机上没有记录的样子，与 `cold_run.py`
//! 去掉 `calibrations` 同义）。不发请求。
//!
//! 依据：B128、B129（`地基/附注/2026-09-25-作者主权与策略表达裁定.md`）；意图汇编第 11 条；预注册
//! `地基/过程记录/工程-sieve声明线.md` 一·4 (a)–(j)。
//!
//! 与缺省规则的关系（主会话 2026-09-26 晚：Nature 定翻转缺省值——没有线的 `cut` 按判断器的回答走、J-08 放行改为
//! 宿主可选的 `--guard`，由另一个代理 trust-default 施工）：本文件先合入，断言的是合入时 main 上的行为。凡标了
//! 「【缺省】」的断言依赖「没给线即冷」，翻转后改为「没给线按回答走」；标了「【放行】」的依赖「声明线守不可逆动作
//! 默认要宿主接受」，翻转后改为在 `--guard` 下断言。给了线之后的出口（种类、等级 `Declared`、与逐个 `cut` 等价）
//! 不受翻转影响。
//!
//! 2026-09-26 晚翻转（trust-default，意图汇编 11a、批 9 B187）已按上面的说明改：【缺省】断言改为「没给线按回答走」
//! （act 高、ignore 低、恰好 0.5 并列出 `unsure(tie)`，等级 `Answer`）；【放行】断言与 `W-declared-line` 的计数
//! 在 `--guard` 下断言（本文件的宿主入口都开 `--guard`，(h) 末尾另加不开把关的对照）。

use jpp::effects::{CalibStore, FnPort, JudgeResult, Ports};
use jpp::interp::{ActionRegistry, TaintOut};
use jpp::ledger::Ledger;
use jpp::value::{Answer, Question, State, Taint, Value};
use jpp::{EntryArgs, Outcome, Session};
use serde_json::Value as Json;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT: AtomicU64 = AtomicU64::new(0);

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// 读数：按（题面，材料原文）给
type 表 = fn(&str, &str) -> f64;

/// 默认：材料含「高」0.9、「低」0.1、「中」0.5；目标题上材料含「六」0.6；其余 0.9
fn 常用(q: &str, m: &str) -> f64 {
    if q.contains("目标") && m.contains('六') {
        0.6
    } else if m.contains('高') {
        0.9
    } else if m.contains('低') {
        0.1
    } else if m.contains('中') {
        0.5
    } else {
        0.9
    }
}

/// 边：一对材料里同时含两个名字
fn 边表(_q: &str, m: &str) -> f64 {
    let 有 = |a: char, b: char| m.contains(a) && m.contains(b);
    if 有('甲', '乙') || 有('乙', '丁') {
        0.9
    } else if 有('甲', '丙') || 有('甲', '丁') {
        0.1
    } else if 有('乙', '丙') {
        0.5
    } else {
        0.9
    }
}

fn 端口(t: 表) -> Ports<'static> {
    Ports::new().with(FnPort::judge(
        "live-shaped",
        move |s: &State, qs: &[&Question]| {
            let text = s.on_text();
            Ok(JudgeResult {
                answers: qs.iter().map(|q| Answer::Noul(t(&q.text, &text))).collect(),
                tokens: 0,
                cost: 0.0,
                mode_share: vec![None; qs.len()],
                perms: vec![0; qs.len()],
                confidence: vec![],
            })
        },
    ))
}

/// 一个不可逆动作「发退款」（J-08 用）加图算法动作（`interval` 用）
fn 动作表() -> ActionRegistry {
    let mut a = ActionRegistry::new();
    jpp::actions::register_all(&mut a, &jpp::actions::Ctx::default(), false);
    a.register("发退款", 0.0, false, TaintOut::Trusted, |_| {
        Ok(Value::Text("已退".into(), Taint::Trusted.into()))
    });
    a
}

fn 入口(接受: bool) -> EntryArgs {
    EntryArgs {
        accept: jpp::HostAccept {
            declared_lines: 接受,
        },
        // 【放行】与 W-declared-line 只在放行把关下出现（B187）：本文件开 --guard
        guard: true,
        ..Default::default()
    }
}

/// 不开放行把关（默认）跑：检查、执行，宿主入口为空
fn 跑_默认(src: &str, t: 表) -> Result<Outcome, String> {
    let entry = EntryArgs::default();
    let program = 编译(src, &entry);
    let calib = CalibStore::new();
    let acts = 动作表();
    Session::new(端口(t), &calib, &acts)
        .run(&program, &entry, &mut Ledger::new())
        .map_err(|e| e.render())
}

/// 把程序写进 `target/` 下的临时目录（import 只收相对路径），经装载器装上库，按宿主入口编译
fn 编译(src: &str, entry: &EntryArgs) -> jpp::Program {
    let dir = root().join(format!(
        "target/sieve-line-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("p.jpp");
    std::fs::write(&path, src).unwrap();
    let loaded = jpp::syntax::loader::load(&path);
    let _ = std::fs::remove_dir_all(&dir);
    Session::compile(&loaded.expect("装载").program, &entry.decl()).expect("compile")
}

/// 先检查再执行（`Session::run`）；空校准库
fn 跑(src: &str, t: 表, 接受: bool) -> Result<Outcome, String> {
    let entry = 入口(接受);
    let program = 编译(src, &entry);
    let calib = CalibStore::new();
    let acts = 动作表();
    Session::new(端口(t), &calib, &acts)
        .run(&program, &entry, &mut Ledger::new())
        .map_err(|e| e.render())
}

/// 跳过静态检查、只看运行期（`Interp` 直接跑）
fn 运行期跑(src: &str, t: 表, 接受: bool) -> Result<Json, String> {
    let entry = 入口(接受);
    let program = 编译(src, &entry);
    let calib = CalibStore::new();
    let acts = 动作表();
    let mut l = Ledger::new();
    jpp::interp::Interp::new(端口(t), &mut l, &calib, &acts, program.budget.clone())
        .with_entry(entry.clone())
        .run(&program)
        .map(|o| o.value_json())
        .map_err(|e| format!("[{}] {}", e.rule.clone().unwrap_or_default(), e.message))
}

/// 检查期诊断（CLI 的动作表：`write_json` 不可逆、`record_check` 可逆），按规则号筛
fn 静态(src: &str, 接受: bool, 规则: &str) -> Vec<String> {
    let 表 = jpp::actions::check_table();
    Session::explain_with_actions(&编译(src, &入口(接受)), None, &表)
        .diagnostics
        .into_iter()
        .filter(|d| d.rule == 规则)
        .map(|d| d.message)
        .collect()
}

fn 文字(v: &Json) -> Vec<String> {
    v.as_array()
        .unwrap_or_else(|| panic!("不是列表：{v}"))
        .iter()
        .map(|x| {
            x.as_str()
                .map(String::from)
                .unwrap_or_else(|| x.to_string())
        })
        .collect()
}

fn 等级(o: &Outcome) -> Vec<String> {
    o.exits
        .iter()
        .filter_map(|r| r["grade"].as_str().map(String::from))
        .collect()
}

fn 声明告警(o: &Outcome) -> usize {
    o.trace
        .warnings
        .iter()
        .filter(|w| w.starts_with("W-declared-line"))
        .count()
}

const 线: &str = "{line: {declare: {hi: 0.7, lo: 0.3}}}";

// ---------------------------------------------------------------- (a) 单题

const 单题: &str = r#"import "../../lib/outcome.jpp";
budget {calls: 8, cost: 0, depth: 32};
let q = test("这条材料说的是真的吗", "k-live");
let o = sieve([mat("高"), mat("低"), mat("中")], q OPTS);
{acc: map(o.value, fn(e) { content(e.item) }), ign: map(o.detail.ignore, fn(e) { content(e.item) }),
 causes: map(o.pending, fn(p) { p.cause }), pending: o.pending}
"#;

/// (a) 不给线：三个元素全部冷（`value` 空、`pending` 3）；给声明线：act / ignore / band 各一，报告三行 `Declared`，
/// `W-declared-line` 一条
#[test]
fn a_单题不给线全冷_给线按线切() {
    // 【缺省】没给线按回答走（B187）：act 高、ignore 低、中间 0.5 恰好并列出 unsure(tie)，等级 Answer
    let o = 跑(&单题.replace("OPTS", ""), 常用, false).expect("不给线照常跑");
    let v = o.value_json();
    assert_eq!(文字(&v["acc"]), vec!["高"], "{v}");
    assert_eq!(文字(&v["ign"]), vec!["低"], "{v}");
    assert_eq!(文字(&v["causes"]), vec!["tie"], "{v}");
    assert!(等级(&o).iter().all(|g| g == "Answer"), "{:?}", o.exits);

    let o = 跑(&单题.replace("OPTS", &format!(", {线}")), 常用, false).expect("给线");
    let v = o.value_json();
    assert_eq!(文字(&v["acc"]), vec!["高"], "{v}");
    assert_eq!(文字(&v["ign"]), vec!["低"], "{v}");
    assert_eq!(文字(&v["causes"]), vec!["band"], "{v}");
    assert_eq!(等级(&o), vec!["Declared"; 3], "{:?}", o.exits);
    assert_eq!(声明告警(&o), 1, "{:?}", o.trace.warnings);
    // 空选项记录与不给相同
    let o = 跑(&单题.replace("OPTS", ", {}"), 常用, false).expect("空选项");
    assert_eq!(文字(&o.value_json()["acc"]), vec!["高"]);
}

// ---------------------------------------------------------------- (b) 多题、题式加填法

/// (b) 多题与题式加填法：同一条线用在全部题上，等级 `Declared`；不给线全冷
#[test]
fn b_多题与题式填法一致() {
    let 多题 = r#"import "../../lib/outcome.jpp";
budget {calls: 8, cost: 0, depth: 32};
let o = sieve([mat("高"), mat("低")], [test("甲问", "k1"), test("乙问", "k2")] OPTS);
{acc: map(o.value, fn(e) { content(e.item) }), ign: map(o.detail.ignore, fn(e) { content(e.item) }),
 n: len(o.pending), pending: o.pending}
"#;
    let o = 跑(&多题.replace("OPTS", &format!(", {线}")), 常用, false).expect("多题给线");
    let v = o.value_json();
    assert_eq!(文字(&v["acc"]), vec!["高", "高"], "{v}");
    assert_eq!(文字(&v["ign"]), vec!["低", "低"], "{v}");
    assert_eq!(v["n"], 0);
    assert_eq!(等级(&o), vec!["Declared"; 4]);
    // 【缺省】没给线按回答走：高两题都 act、低两题都 ignore，没有未决
    let o = 跑(&多题.replace("OPTS", ""), 常用, false).expect("多题不给线");
    assert_eq!(o.value_json()["n"], 0);
    assert_eq!(文字(&o.value_json()["acc"]), vec!["高", "高"]);

    let 题式 = r#"import "../../lib/outcome.jpp";
budget {calls: 8, cost: 0, depth: 32};
let f = form("test", "这段话是否提到了{city}？", {calib: "form-live"});
let o = sieve([mat("高"), mat("低")], f, [{city: "成都"}, {city: "北京"}] OPTS);
{acc: map(o.value, fn(e) { content(e.item) }), ign: map(o.detail.ignore, fn(e) { content(e.item) }),
 fills: map(o.value, fn(e) { e.fill.city }), n: len(o.pending), pending: o.pending}
"#;
    let o = 跑(&题式.replace("OPTS", &format!(", {线}")), 常用, false).expect("题式给线");
    let v = o.value_json();
    assert_eq!(文字(&v["acc"]), vec!["高", "高"], "{v}");
    assert_eq!(文字(&v["fills"]), vec!["成都", "北京"], "{v}");
    assert_eq!(文字(&v["ign"]), vec!["低", "低"], "{v}");
    assert_eq!(v["n"], 0);
    assert_eq!(等级(&o), vec!["Declared"; 4]);
    // 【缺省】没给线按回答走
    let o = 跑(&题式.replace("OPTS", ""), 常用, false).expect("题式不给线");
    assert_eq!(o.value_json()["n"], 0);
    assert_eq!(文字(&o.value_json()["ign"]), vec!["低", "低"]);
}

// ---------------------------------------------------------------- (c) 与逐个 cut 等价

/// (c) `sieve(xs, q, {line: L})` 的出口与逐个 `cut(judge(state(x), q), L)` 逐项相同（种类、等级、放行、材料、键、线）
#[test]
fn c_与逐个cut等价() {
    let src = r#"budget {calls: 8, cost: 0, depth: 32};
let L = {declare: {hi: 0.7, lo: 0.3}};
let xs = [mat("高"), mat("低"), mat("中")];
let q = test("这条材料说的是真的吗", "k-live");
let o = sieve(xs, q, {line: L});
let es = map(xs, fn(x) { cut(judge(state(x), q), L) });
{kinds: map(es, fn(e) { exit_kind(e) }), es: es, pending: o.pending, acc: o.value, ign: o.detail.ignore}
"#;
    let o = 跑(src, 常用, false).expect("跑");
    assert_eq!(o.exits.len(), 6, "{:?}", o.exits);
    let 取 = |r: &Json| {
        (
            r["exit"].clone(),
            r["grade"].clone(),
            r["releases"].clone(),
            r["item"].clone(),
            r["key"].clone(),
            r["declared"]["hi"].clone(),
            r["declared"]["lo"].clone(),
        )
    };
    let (筛, 切) = o.exits.split_at(3);
    // sieve 的元素按材料主序切，与 map 逐个切同序
    for (a, b) in 筛.iter().zip(切) {
        assert_eq!(取(a), 取(b), "\nsieve：{a}\ncut：{b}");
    }
    assert_eq!(
        文字(&o.value_json()["kinds"]),
        vec!["act", "ignore", "unsure(band)"]
    );
}

// ---------------------------------------------------------------- (d) 报错

/// (d) 选项位或 `line` 是数字：`J-03`（检查期与运行期），修法给出声明线写法；未知键 `E-rt-arg`；`line` 里的互斥与
/// 越界与 `cut` 相同（`E-cut-options`）
#[test]
fn d_选项报错() {
    let 程序 = |opts: &str| {
        format!(
            r#"budget {{calls: 8, cost: 0, depth: 32}};
let o = sieve([mat("高")], test("真的吗", "k"), {opts});
{{pending: o.pending, acc: o.value}}
"#
        )
    };
    for opts in ["{line: 0.7}", "0.7"] {
        let src = 程序(opts);
        let e = 跑(&src, 常用, false).map(|_| ()).expect_err("检查期拒");
        assert!(
            e.contains("J-03") && e.contains("{line: {declare: {hi: 0.7}}}"),
            "{opts}：{e}"
        );
        let e = 运行期跑(&src, 常用, false).expect_err("运行期拒");
        assert!(
            e.starts_with("[J-03]") && e.contains("{line: {declare: {hi: 0.7}}}"),
            "{opts}：{e}"
        );
    }
    let e = 运行期跑(&程序("{lin: {declare: {hi: 0.7}}}"), 常用, false).expect_err("未知键");
    assert!(e.starts_with("[E-rt-arg]") && e.contains("line"), "{e}");
    let e = 运行期跑(
        &程序("{line: {declare: {hi: 0.7}, cost: [1, 2]}}"),
        常用,
        false,
    )
    .expect_err("互斥");
    assert!(e.starts_with("[E-cut-options]"), "{e}");
    let e =
        运行期跑(&程序("{line: {declare: {hi: 0.3, lo: 0.7}}}"), 常用, false).expect_err("lo > hi");
    assert!(e.starts_with("[E-cut-options]"), "{e}");
    let e = 运行期跑(&程序("{line: {decl: {hi: 0.7}}}"), 常用, false).expect_err("line 里未知字段");
    assert!(e.starts_with("[E-rt-arg]"), "{e}");
}

// ---------------------------------------------------------------- (e) search

const 搜索: &str = r#"import "../../lib/compose/search.jpp";
budget {calls: 20, cost: 0, depth: 64};
let feas = test("这个候选可行吗", "k-feas");
let obj = test("这个候选达到目标吗", "k-obj");
let propose = fn(frontier, i) { CANDS };
let r = search([], propose, feas, OBJ, 2, OPTS);
{kept: map(r.value, fn(e) { content(e.item) }), kinds: map(r.value, fn(e) { exit_kind(e.exit) }),
 reason: r.detail.reason, causes: map(r.pending, fn(p) { p.cause }), pending: r.pending}
"#;

fn 搜(cands: &str, obj: &str, opts: &str) -> Result<Outcome, String> {
    跑(
        &搜索
            .replace("CANDS", cands)
            .replace("OBJ", obj)
            .replace("OPTS", opts),
        常用,
        false,
    )
}

/// (e) `search`：不给线全冷、`value` 空；给 `line` 凑够 width 停在 stop；`objective_line` 单独作用于目标题；
/// `objective_line` 配 `unit` 目标报 `E-search-options`
#[test]
fn e_search的线() {
    let 三个 = r#"[mat("高甲"), mat("低乙"), mat("中丙")]"#;
    // 【缺省】没给线按回答走：高甲可行且达到目标，凑够 width 1 即停；中丙 0.5 恰好并列留在未决
    let o = 搜(三个, "obj", "{width: 1}").expect("不给线");
    let v = o.value_json();
    assert_eq!(文字(&v["kept"]), vec!["高甲"], "{v}");
    assert_eq!(文字(&v["causes"]), vec!["tie"], "{v}");

    let o = 搜(
        三个,
        "obj",
        &format!("{{width: 1, line: {}}}", "{declare: {hi: 0.7, lo: 0.3}}"),
    )
    .expect("给线");
    let v = o.value_json();
    assert_eq!(文字(&v["kept"]), vec!["高甲"], "{v}");
    assert_eq!(文字(&v["kinds"]), vec!["act"], "{v}");
    assert_eq!(v["reason"], "stop", "{v}");
    assert_eq!(文字(&v["causes"]), vec!["band"], "{v}");
    assert!(!等级(&o).iter().any(|g| g == "Cold"), "{:?}", o.exits);

    // 目标读数 0.6：只给 line（hi 0.7）时目标未决；另给 objective_line（hi 0.5）时判好
    let 六 = r#"[mat("高六")]"#;
    let o = 搜(六, "obj", "{width: 1, line: {declare: {hi: 0.7, lo: 0.3}}}").expect("只给 line");
    let v = o.value_json();
    assert!(文字(&v["kept"]).is_empty(), "{v}");
    assert_eq!(文字(&v["causes"]), vec!["band"], "{v}");
    let o = 搜(
        六,
        "obj",
        "{width: 1, line: {declare: {hi: 0.7, lo: 0.3}}, objective_line: {declare: {hi: 0.5}}}",
    )
    .expect("另给 objective_line");
    assert_eq!(文字(&o.value_json()["kept"]), vec!["高六"]);

    let e = 搜(
        三个,
        "unit",
        "{width: 1, objective_line: {declare: {hi: 0.5}}}",
    )
    .map(|_| ())
    .expect_err("unit 目标给 objective_line");
    assert!(e.contains("E-search-options"), "{e}");
}

// ---------------------------------------------------------------- (f) verify

/// (f) `verify`：不给线全冷；给线出 act / ignore / band
#[test]
fn f_verify的线() {
    let src = r#"import "../../lib/compose/ground.jpp";
budget {calls: 8, cost: 0, depth: 64};
let o = verify([mat("高"), mat("低"), mat("中")], fn(c) { c }, test("这个结果对吗", "k-verify"), OPTS);
{acc: map(o.value, fn(e) { content(e.item) }), ign: map(o.detail.ignore, fn(e) { content(e.item) }),
 causes: map(o.pending, fn(p) { p.cause }), pending: o.pending}
"#;
    // 【缺省】没给线按回答走
    let o = 跑(&src.replace("OPTS", "{}"), 常用, false).expect("不给线");
    let v = o.value_json();
    assert_eq!(文字(&v["acc"]), vec!["高"], "{v}");
    assert_eq!(文字(&v["ign"]), vec!["低"], "{v}");
    assert_eq!(文字(&v["causes"]), vec!["tie"], "{v}");
    let o = 跑(&src.replace("OPTS", 线), 常用, false).expect("给线");
    let v = o.value_json();
    assert_eq!(文字(&v["acc"]), vec!["高"], "{v}");
    assert_eq!(文字(&v["ign"]), vec!["低"], "{v}");
    assert_eq!(文字(&v["causes"]), vec!["band"], "{v}");
}

// ---------------------------------------------------------------- (g) 判出来的图

/// (g) `judged_graph` / `judged_bipartite`：不给线全部边未决（cold）；给线分成已决有、已决无、未决（band），
/// `interval` 在已决边上配出对
#[test]
fn g_判出来的图的线() {
    let 单集 = r#"import "../../lib/compose/graph.jpp";
budget {calls: 20, cost: 0, depth: 64};
let g = judged_graph(["甲", "乙", "丙"], test("a 与 b 能合作吗", "k-edge"), OPTS);
let t = interval(g, "matching", {});
{edges: map(g.edges, fn(e) { e.ends }), rejected: map(g.rejected, fn(e) { e.ends }),
 open: map(g.pending, fn(e) { e.ends }), causes: map(g.pending, fn(p) { p.cause }),
 assigned: map(t.lo.value, fn(p) { p.members }), pending: concat(t.hi.pending, g.pending)}
"#;
    // 【缺省】没给线按回答走：甲乙 0.9 有边、甲丙 0.1 无边、乙丙 0.5 恰好并列未决
    let o = 跑(&单集.replace("OPTS", "{}"), 边表, false).expect("不给线");
    let v = o.value_json();
    assert_eq!(v["edges"], serde_json::json!([[0, 1]]), "{v}");
    assert_eq!(v["rejected"], serde_json::json!([[0, 2]]), "{v}");
    assert_eq!(文字(&v["causes"]), vec!["tie"], "{v}");
    let o = 跑(&单集.replace("OPTS", 线), 边表, false).expect("给线");
    let v = o.value_json();
    assert_eq!(v["edges"], serde_json::json!([[0, 1]]), "{v}");
    assert_eq!(v["rejected"], serde_json::json!([[0, 2]]), "{v}");
    assert_eq!(v["open"], serde_json::json!([[1, 2]]), "{v}");
    assert_eq!(文字(&v["causes"]), vec!["band"], "{v}");
    assert_eq!(v["assigned"], serde_json::json!([[0, 1]]), "{v}");

    let 二部 = r#"import "../../lib/compose/graph.jpp";
budget {calls: 20, cost: 0, depth: 64};
let g = judged_bipartite(["甲", "乙"], ["丙", "丁"], test("a 与 b 能合作吗", "k-edge"), OPTS);
{edges: map(g.edges, fn(e) { e.ends }), rejected: map(g.rejected, fn(e) { e.ends }),
 open: map(g.pending, fn(e) { e.ends }), causes: map(g.pending, fn(p) { p.cause }), pending: g.pending}
"#;
    // 【缺省】没给线按回答走
    let o = 跑(&二部.replace("OPTS", "{}"), 边表, false).expect("二部不给线");
    let v = o.value_json();
    assert_eq!(v["edges"], serde_json::json!([[1, 3]]), "{v}");
    assert_eq!(v["rejected"], serde_json::json!([[0, 2], [0, 3]]), "{v}");
    assert_eq!(文字(&v["causes"]), vec!["tie"], "{v}");
    let o = 跑(&二部.replace("OPTS", 线), 边表, false).expect("二部给线");
    let v = o.value_json();
    assert_eq!(v["edges"], serde_json::json!([[1, 3]]), "{v}");
    assert_eq!(v["rejected"], serde_json::json!([[0, 2], [0, 3]]), "{v}");
    assert_eq!(v["open"], serde_json::json!([[1, 2]]), "{v}");
}

// ---------------------------------------------------------------- (h) 运行期 J-08

const 筛后守动作: &str = r#"budget {calls: 4, cost: 0, depth: 16};
let o = sieve([mat("高")], test("要退款吗", "k"), {line: {declare: {hi: 0.7, lo: 0.3}}});
{r: handle(o.value[0].exit, {
    act: fn() { do("发退款", [], 0) },
    ignore: fn() { "不退" },
    unsure: fn(u) { consume(u, "drop"); "转人工" }}), pending: o.pending}
"#;

/// (h) `sieve` 声明线的接受流出口守不可逆动作：不带接受，检查期（静态子面）与运行期都拒；带接受执行。
/// 经库（`judged_graph` 的已决边）：静态看不透，运行期不带接受拒、带接受放行
#[test]
fn h_声明线守不可逆动作要宿主接受() {
    // 【放行】声明线守不可逆动作默认要宿主接受（翻转后改在 --guard 下断言）
    let e = 跑(筛后守动作, 常用, false)
        .map(|_| ())
        .expect_err("检查期拒");
    // 检查期拒在静态子面（报文以「（静态子面）」开头），不是走到运行期才拒
    assert!(
        e.contains("J-08")
            && e.contains("静态子面")
            && e.contains("--release-on-declared")
            && e.contains("hi=0.7 lo=0.3"),
        "{e}"
    );
    let e = 运行期跑(筛后守动作, 常用, false).expect_err("运行期拒");
    assert!(
        e.starts_with("[J-08]") && e.contains("宿主未声明接受作者线放行"),
        "{e}"
    );
    let o = 跑(筛后守动作, 常用, true).expect("带接受放行");
    assert_eq!(o.value_json()["r"]["content"], "已退", "{}", o.value_json());

    let 经库 = r#"import "../../lib/compose/graph.jpp";
budget {calls: 8, cost: 0, depth: 64};
let g = judged_graph(["甲", "乙"], test("a 与 b 能合作吗", "k-edge"), {line: {declare: {hi: 0.7, lo: 0.3}}});
{r: handle(g.edges[0].exit, {
    act: fn() { do("发退款", [], 0) },
    ignore: fn() { "不退" },
    unsure: fn(u) { consume(u, "drop"); "转人工" }}), pending: g.pending}
"#;
    // 【放行】同上
    let e = 跑(经库, 边表, false).map(|_| ()).expect_err("运行期拒");
    // 出口在库函数的帧里切出，函数返回后 guard.rs 在当前帧找不到它的声明线说明，报文只有通用的 B128 一句
    // （已知限制，交效应轨：guard.rs 的补句按帧找出口）
    assert!(
        e.contains("J-08") && e.contains("宿主未接受的作者声明线（B128）"),
        "{e}"
    );
    let o = 跑(经库, 边表, true).expect("带接受放行");
    assert_eq!(o.value_json()["r"]["content"], "已退", "{}", o.value_json());
    // 不开放行把关（默认，意图汇编 11a）：声明线出口直接驱动不可逆动作，不需要任何开关
    for (src, t) in [(筛后守动作, 常用 as 表), (经库, 边表 as 表)] {
        let o = 跑_默认(src, t).expect("默认不拦");
        assert_eq!(o.value_json()["r"]["content"], "已退", "{}", o.value_json());
    }
}

// ---------------------------------------------------------------- (i) 检查期 J-08 静态子面

/// (i) 静态子面：`sieve` 字面声明线的 `o.value[i].exit` 直接守不可逆 `do` 报 J-08（经 `let` 同样）；带接受不报；
/// 不写线的 `sieve` 不报；可逆动作不报；材料是不可信入口时报文说材料
#[test]
fn i_静态子面看得到sieve的声明线() {
    let 直接 = r#"budget {calls: 4, cost: 0, depth: 16};
let o = sieve([mat("甲")], test("要退款吗", "k"), {line: {declare: {hi: 0.7}}});
{r: handle(o.value[0].exit, {
    act: fn() { do("write_json", ["o.json", 1], 0) },
    ignore: fn() { 0 },
    unsure: fn(u) { consume(u, "drop"); 0 }}), pending: o.pending}
"#;
    // 【放行】静态子面同上（翻转后在 --guard 下断言）
    let m = 静态(直接, false, "J-08");
    assert_eq!(m.len(), 1, "{m:?}");
    assert!(
        m[0].contains("--release-on-declared") && m[0].contains("hi=0.7 lo=0.7"),
        "{}",
        m[0]
    );
    assert!(静态(直接, true, "J-08").is_empty());
    let 经let = 直接.replace(
        "{r: handle(o.value[0].exit, {",
        "let e = o.value[0];\n{r: handle(e.exit, {",
    );
    assert_eq!(静态(&经let, false, "J-08").len(), 1);
    let 不写线 = 直接.replace(", {line: {declare: {hi: 0.7}}}", "");
    assert!(静态(&不写线, false, "J-08").is_empty());
    let 可逆 = 直接.replace(
        r#"do("write_json", ["o.json", 1], 0)"#,
        r#"do("record_check", ["c", true], 0)"#,
    );
    assert!(静态(&可逆, false, "J-08").is_empty());
    // 材料是不可信入口：报文说材料（开关救不了它）
    let mut entry = 入口(true);
    entry
        .materials
        .push(jpp::EntryMat::untrusted("料", serde_json::json!("甲")));
    let 不可信 = 直接.replace(r#"[mat("甲")]"#, "[料]");
    let 表 = jpp::actions::check_table();
    let m: Vec<String> = Session::explain_with_actions(&编译(&不可信, &entry), None, &表)
        .diagnostics
        .into_iter()
        .filter(|d| d.rule == "J-08")
        .map(|d| d.message)
        .collect();
    assert_eq!(m.len(), 1, "{m:?}");
    assert!(m[0].contains("宿主入口 料"), "{}", m[0]);
    // 一可信一不可信：sieve 每个元素各一个状态，可信元素的出口在宿主接受时照样放行，静态不报（零假拒绝）
    let 混合 = 直接.replace(r#"[mat("甲")]"#, r#"[料, mat("乙")]"#);
    let n = Session::explain_with_actions(&编译(&混合, &entry), None, &表)
        .diagnostics
        .into_iter()
        .filter(|d| d.rule == "J-08")
        .count();
    assert_eq!(n, 0);
}

// ---------------------------------------------------------------- (j) 读数种类

/// (j) 三参形状（题 + 选项）与两参形状一样推为契约值：字段拼错报 `E-field`
#[test]
fn j_三参形状仍是契约值() {
    for opts in ["", ", {line: {declare: {hi: 0.7}}}"] {
        let src = format!(
            r#"budget {{calls: 4, cost: 0, depth: 16}};
let o = sieve([mat("甲")], test("真的吗", "k"){opts});
{{x: o.valeu, pending: o.pending}}
"#
        );
        let m = 静态(&src, false, "E-field");
        assert_eq!(m.len(), 1, "{opts}：{m:?}");
    }
}

// ---------------------------------------------------------------- (k)–(n) literalize 收线（附录预注册 §四）

/// 重问的题（题面含「直接」）按材料里的「真」「假」给 0.9 / 0.1；其余同 [`常用`]（「中」0.5，外层落在带内）
fn 重问表(q: &str, m: &str) -> f64 {
    if q.contains("直接") {
        if m.contains('真') {
            0.9
        } else if m.contains('假') {
            0.1
        } else {
            0.5
        }
    } else {
        常用(q, m)
    }
}

/// 外层声明线判出 band，unsure 臂里 `literalize` 换更字面的题重问；OPTS 是第四参
const 重问: &str = r#"budget {calls: 8, cost: 0, depth: 32};
let q1 = test("这条说的是真的吗", "k1");
let q2 = test("这条材料里直接写了是真的吗", "k2");
let m = mat("中MAT");
handle(cut(judge(state(m), q1), {declare: {hi: 0.7, lo: 0.3}}), {
    act: fn() { {kind: "外层是"} },
    ignore: fn() { {kind: "外层否"} },
    unsure: fn(u) { let e = literalize(u, state(m), q2 OPTS); {kind: exit_kind(e), exit: e} }})
"#;

fn 重问程序(材料: &str, opts: &str) -> String {
    重问.replace("MAT", 材料).replace("OPTS", opts)
}

/// (k) 重问给了线：0.9 出 act、0.1 出 ignore，两行报告都是 `Declared`；不给线时重问按缺省规则
#[test]
fn k_literalize给线按线切() {
    let o = 跑(&重问程序("真", &format!(", {线}")), 重问表, false).expect("给线");
    assert_eq!(o.value_json()["kind"], "act", "{}", o.value_json());
    assert_eq!(等级(&o), vec!["Declared"; 2], "{:?}", o.exits);
    let o = 跑(&重问程序("假", &format!(", {线}")), 重问表, false).expect("给线");
    assert_eq!(o.value_json()["kind"], "ignore", "{}", o.value_json());
    // 【缺省】不给线：重问按缺省规则（B187 起按判断器的回答走：0.9 → act，等级 Answer）
    let o = 跑(&重问程序("真", ""), 重问表, false).expect("不给线");
    assert_eq!(o.value_json()["kind"], "act", "{}", o.value_json());
    assert_eq!(等级(&o), vec!["Declared", "Answer"], "{:?}", o.exits);
    // 空选项与不给相同
    let o = 跑(&重问程序("真", ", {}"), 重问表, false).expect("空选项");
    assert_eq!(o.value_json()["kind"], "act", "{}", o.value_json());
}

/// (l) 报错与 `sieve` 的选项相同
#[test]
fn l_literalize选项报错() {
    for opts in [", {line: 0.7}", ", 0.7"] {
        let src = 重问程序("真", opts);
        let e = 跑(&src, 重问表, false).map(|_| ()).expect_err("检查期拒");
        assert!(
            e.contains("J-03") && e.contains("{line: {declare: {hi: 0.7}}}"),
            "{opts}：{e}"
        );
        let e = 运行期跑(&src, 重问表, false).expect_err("运行期拒");
        assert!(
            e.starts_with("[J-03]") && e.contains("{line: {declare: {hi: 0.7}}}"),
            "{opts}：{e}"
        );
    }
    let e = 运行期跑(
        &重问程序("真", ", {lin: {declare: {hi: 0.7}}}"),
        重问表,
        false,
    )
    .expect_err("未知键");
    assert!(e.starts_with("[E-rt-arg]") && e.contains("line"), "{e}");
    // select 题的声明线只收 hi
    let 选 = 重问程序("真", "")
        .replace(
            r#"test("这条材料里直接写了是真的吗", "k2")"#,
            r#"select("直接写的是哪一个", "k2")"#,
        )
        .replace(
            "literalize(u, state(m), q2 )",
            r#"literalize(u, state(m, {over: [mat("甲"), mat("乙")]}), q2, {line: {declare: {hi: 0.7, lo: 0.3}}})"#,
        );
    let e = 运行期跑(&选, 重问表, false).expect_err("select 给 lo");
    assert!(e.starts_with("[E-cut-options]"), "{e}");
}

/// 外层与重问都用声明线（重问 hi=0.8 lo=0.2），重问出口的 act 臂守不可逆动作
const 重问守动作: &str = r#"budget {calls: 8, cost: 0, depth: 32};
let q1 = test("这条说的是真的吗", "k1");
let q2 = test("这条材料里直接写了是真的吗", "k2");
let m = mat("中真");
handle(cut(judge(state(m), q1), {declare: {hi: 0.7, lo: 0.3}}), {
    act: fn() { "外层是" },
    ignore: fn() { "外层否" },
    unsure: fn(u) { handle(literalize(u, state(m), q2 OPTS), {
        act: fn() { do("ACTION", ARGS, 0) },
        ignore: fn() { "不退" },
        unsure: fn(v) { consume(v, "drop"); "转人工" }}) }})
"#;

fn 守动作(opts: &str, 动作: &str, 实参: &str) -> String {
    重问守动作
        .replace("OPTS", opts)
        .replace("ACTION", 动作)
        .replace("ARGS", 实参)
}

/// (m) 运行期 J-08：不带接受，检查期（静态子面）与运行期都拒。带接受仍拒，拦在谱系放行：`literalize` 重问的题
/// 带着那个未决出口的键（B84），重问出口的谱系里有一个未决出口，B72-4 不放行——与线无关，认证线同样如此
/// （预注册写的「带接受放行」不成立，见过程记录 §五）
#[test]
fn m_literalize声明线守不可逆动作() {
    let src = 守动作(", {line: {declare: {hi: 0.8, lo: 0.2}}}", "发退款", "[]");
    // 【放行】默认要宿主接受（翻转后改在 --guard 下断言）
    let e = 跑(&src, 重问表, false).map(|_| ()).expect_err("检查期拒");
    assert!(
        e.contains("J-08") && e.contains("静态子面") && e.contains("hi=0.8 lo=0.2"),
        "{e}"
    );
    let e = 运行期跑(&src, 重问表, false).expect_err("运行期拒");
    assert!(e.starts_with("[J-08]"), "{e}");
    // 【放行】批 9 B187 把谱系放行挪到 --guard 下，翻转后这一条随之改
    let e = 跑(&src, 重问表, true)
        .map(|_| ())
        .expect_err("带接受仍拒（谱系）");
    assert!(
        e.contains("J-08") && e.contains("未决出口选出") && e.contains("B72-4"),
        "{e}"
    );
    // 不开放行把关（默认，意图汇编 11a）：重问出口的 act 臂直接执行不可逆动作
    let o = 跑_默认(&src, 重问表).expect("默认不拦");
    assert_eq!(o.value_json()["content"], "已退", "{}", o.value_json());
}

/// (n) 检查期（CLI 动作表）：报文含重问站点的线；只有外层是声明线、重问不写线时不报（重问那层看不透）
#[test]
fn n_静态子面看得到literalize的声明线() {
    let src = 守动作(
        ", {line: {declare: {hi: 0.8, lo: 0.2}}}",
        "write_json",
        r#"["o.json", 1]"#,
    );
    // 【放行】同上
    let m = 静态(&src, false, "J-08");
    assert_eq!(m.len(), 1, "{m:?}");
    assert!(
        m[0].contains("hi=0.8 lo=0.2") && m[0].contains("hi=0.7 lo=0.3"),
        "{}",
        m[0]
    );
    assert!(静态(&src, true, "J-08").is_empty());
    let 不写线 = 守动作("", "write_json", r#"["o.json", 1]"#);
    assert!(静态(&不写线, false, "J-08").is_empty());
}
