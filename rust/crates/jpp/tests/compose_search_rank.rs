//! `search` 的打分排序（步 25c-2，B148、B167）：`objective` 是打分题时只排序、不过滤——可行的候选进累积池，
//! 按 `order(读数们, opts.rank)` 取前 width 个；前 width 个里进了新候选才算进展，并列的新候选挤不掉旧的；
//! 打分路径只有 `noshrink` 与 `bound`；打分题不产生出口。闭包端口，不发请求。
//!
//! 依据：B148；B166、B167（15k 的 `order`）；预注册 `地基/过程记录/工程-步25c-2.md` 一·3 (a)–(h)。

use jpp::effects::{CalibStore, EffectError, FnPort, GenResult, JudgeResult, Ports};
use jpp::ledger::Ledger;
use jpp::value::Answer;
use jpp::{ActionRegistry, EntryArgs, Outcome, Session};
use serde_json::{Value as Json, json};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT: AtomicU64 = AtomicU64::new(0);

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn 库() -> CalibStore {
    let mut c = CalibStore::new();
    c.put("k", 0.75, 0.25, 50, "上岗", Some(0.05)).unwrap();
    c
}

/// 打分题的读数表（三档）：档位与期望档位
fn 分(text: &str) -> Vec<f64> {
    match text {
        "好甲" => vec![0.1, 0.2, 0.7],     // 档位 2，期望 1.6
        "好乙" => vec![0.6, 0.4, 0.0],     // 档位 0，期望 0.4
        "好丙" => vec![0.1, 0.8, 0.1],     // 档位 1，期望 1.0
        "好丁" => vec![0.2, 0.7, 0.1],     // 档位 1，期望 0.9
        "好己" => vec![0.8, 0.1, 0.1],     // 档位 0，期望 0.3
        "好高档" => vec![0.45, 0.0, 0.55], // 档位 2，期望 1.1
        "好中档" => vec![0.0, 0.6, 0.4],   // 档位 1，期望 1.4
        "好低档" => vec![0.9, 0.1, 0.0],   // 档位 0，期望 0.1
        _ => vec![0.4, 0.3, 0.3],
    }
}

/// 判断端口：打分题查表；「好记」题——含「记」0.9、其余 0.1；可行域题——含「坏」0.1、其余 0.9
fn 判断端口<'a>() -> FnPort<'a> {
    FnPort::judge("fixed-0", move |s, qs| {
        let text = s.on_text();
        Ok::<_, EffectError>(JudgeResult {
            answers: qs
                .iter()
                .map(|q| {
                    if !q.scale.is_empty() {
                        Answer::Score(分(&text))
                    } else if q.text.contains("好记") {
                        Answer::Noul(if text.contains('记') { 0.9 } else { 0.1 })
                    } else {
                        Answer::Noul(if text.contains('坏') { 0.1 } else { 0.9 })
                    }
                })
                .collect(),
            tokens: 0,
            cost: 0.0,
            mode_share: vec![],
            perms: vec![],
            confidence: vec![],
        })
    })
}

/// 生成端口：按 retry_seq 给一轮；上下文第一项是「需求二」时给第二张表
fn 生成端口<'a>(
    rounds: &'static [&'static [&'static str]],
    rounds2: &'static [&'static [&'static str]],
) -> FnPort<'a> {
    FnPort::generate("fixed-0", move |_p, ctx, _n, retry| {
        let table = if ctx.first().and_then(|x| x.as_str()) == Some("需求二") {
            rounds2
        } else {
            rounds
        };
        Ok(GenResult {
            outputs: table[retry as usize].iter().map(|t| json!(t)).collect(),
            ..Default::default()
        })
    })
}

fn 跑(
    src: &str,
    rounds: &'static [&'static [&'static str]],
    rounds2: &'static [&'static [&'static str]],
) -> Result<Outcome, String> {
    let dir = root().join(format!(
        "target/compose-search-rank-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("p.jpp");
    std::fs::write(&path, src).unwrap();
    let loaded = jpp::syntax::loader::load(&path);
    let _ = std::fs::remove_dir_all(&dir);
    let program = jpp::lower(&loaded.expect("装载").program).expect("lower");
    let calib = 库();
    let acts = ActionRegistry::new();
    let ports = Ports::new()
        .with(判断端口())
        .with(生成端口(rounds, rounds2));
    Session::new(ports, &calib, &acts)
        .run(&program, &EntryArgs::default(), &mut Ledger::new())
        .map_err(|e| e.render())
}

const 头: &str = r#"import "../../lib/compose/search.jpp";
budget {calls: 60, cost: 0, depth: 64};
let brief = mat("需求");
let propose = fn(frontier, i) { gen("提 3 个候选", concat([brief], frontier), 3, i) };
let fits = test("这个候选合适吗？", "k");
let score = measure("这个候选有多好？", ["差", "中", "好"], "k");
"#;

const 读: &str = "{kept: map(r.value, fn(e) { e.item }), found_in: map(r.value, fn(e) { e.round }),
  reason: r.detail.reason, rounds: r.detail.rounds, measures: r.detail.measures,
  duplicates: r.detail.duplicates, pending: r.pending}";

fn 内容(v: &Json) -> Vec<String> {
    v.as_array()
        .unwrap_or_else(|| panic!("不是列表：{v}"))
        .iter()
        .map(|m| m["content"].as_str().unwrap().to_string())
        .collect()
}

/// (a) 缺省按档位排：前 width 个是档位最高的两个，value 按排名；一轮用完，bound
#[test]
fn a_缺省按档位取前_width() {
    const 表: &[&[&str]] = &[&["好乙", "好甲", "好丙"]];
    let src = format!("{头}let r = search([], propose, fits, score, 1, {{width: 2}});\n{读}");
    let v = 跑(&src, 表, 表).unwrap().value_json();
    assert_eq!(内容(&v["kept"]), ["好甲", "好丙"]);
    assert_eq!(v["found_in"], json!([0, 0]));
    assert_eq!(v["reason"], json!("bound"));
}

/// (b) rank: {stat: "expect"} 与缺省给出不同的前沿（档位与期望相反的一组读数）
#[test]
fn b_expect_与档位给出不同前沿() {
    const 表: &[&[&str]] = &[&["好低档", "好高档", "好中档"]];
    let 缺省 = format!("{头}let r = search([], propose, fits, score, 1, {{width: 1}});\n{读}");
    let 期望 = format!(
        "{头}let r = search([], propose, fits, score, 1, {{width: 1, rank: {{stat: \"expect\"}}}});\n{读}"
    );
    assert_eq!(
        内容(&跑(&缺省, 表, 表).unwrap().value_json()["kept"]),
        ["好高档"]
    );
    assert_eq!(
        内容(&跑(&期望, 表, 表).unwrap().value_json()["kept"]),
        ["好中档"]
    );
}

/// (c) 进展：旧好候选重提加一个更好的新候选 → 继续；并列的新候选挤不掉旧的、更差的新候选进不了前 width → noshrink
#[test]
fn c_进展与停滞() {
    const 表: &[&[&str]] = &[
        &["好甲", "好乙"],
        &["好甲", "好丁", "坏戊"],
        &["好己", "好丙"],
    ];
    let src = format!("{头}let r = search([], propose, fits, score, 3, {{width: 2}});\n{读}");
    let v = 跑(&src, 表, 表).unwrap().value_json();
    assert_eq!(v["reason"], json!("noshrink"));
    assert_eq!(v["rounds"], json!(3));
    assert_eq!(
        内容(&v["kept"]),
        ["好甲", "好丁"],
        "档位 1 的好丙与好丁并档，挤不掉先进池的"
    );
    assert_eq!(v["found_in"], json!([0, 1]));
    assert_eq!(v["measures"], json!([4, 3, 2, 2]));
    assert_eq!(v["duplicates"], json!(1));
}

/// (d) 两层嵌套：map 里两份需求各跑一次打分搜索，互不干扰
#[test]
fn d_map_里两次打分搜索() {
    const 表一: &[&[&str]] = &[&["好乙", "好甲"]];
    const 表二: &[&[&str]] = &[&["好丙", "好己"]];
    let src = r#"import "../../lib/compose/search.jpp";
budget {calls: 60, cost: 0, depth: 64};
let fits = test("这个候选合适吗？", "k");
let score = measure("这个候选有多好？", ["差", "中", "好"], "k");
let rs = map([mat("需求一"), mat("需求二")], fn(b) {
    search([], fn(frontier, i) { gen("提 2 个候选", concat([b], frontier), 2, i) }, fits, score, 1, {width: 1})
});
{kept: map(rs, fn(r) { map(r.value, fn(e) { content(e.item) }) }), pending: map(rs, fn(r) { r.pending })}"#;
    let v = 跑(src, 表一, 表二).unwrap().value_json();
    assert_eq!(v["kept"], json!([["好甲"], ["好丙"]]));
}

/// (e) 每轮判断 2 层；打分题不产生出口（报告 exits 只有可行域的行），value 元素出口只合成可行域那道题
#[test]
fn e_层数与出口() {
    const 表: &[&[&str]] = &[&["好乙", "好甲", "坏丙"]];
    let src = format!(
        "{头}let r = search([], propose, fits, score, 1, {{width: 2}});
{{kinds: map(r.value, fn(e) {{ exit_kind(e.exit) }}), n: map(r.value, fn(e) {{ cert(e.exit).n_unknown }}), pending: r.pending}}"
    );
    let o = 跑(&src, 表, 表).unwrap();
    assert_eq!(o.layers.len(), 2, "{:?}", o.layers);
    assert_eq!(o.exits.len(), 3, "只有三次可行域判断的出口：{:?}", o.exits);
    let v = o.value_json();
    assert_eq!(v["kinds"], json!(["act", "act"]));
    assert_eq!(v["n"], json!([1, 1]), "合成出口只有可行域一个分量");
}

/// (f) rank 选项错由 order 报 E-order-options
#[test]
fn f_rank_选项错() {
    const 表: &[&[&str]] = &[&["好乙", "好甲"]];
    let src = format!(
        "{头}let r = search([], propose, fits, score, 1, {{width: 1, rank: {{stat: \"max\", tie: 0.1}}}});\n{读}"
    );
    let e = 跑(&src, 表, 表).expect_err("选项错");
    assert!(e.contains("E-order-options"), "{e}");
}

/// (g) select 题当 objective：走是非题路径，由 sieve 报只收是非题，不静默排序
#[test]
fn g_select_当目标报错() {
    const 表: &[&[&str]] = &[&["好乙", "好甲"]];
    let src = format!(
        "{头}let pick = select(\"哪一个？\", \"k\");\nlet r = search([], propose, fits, pick, 1, {{width: 1}});\n{读}"
    );
    let e = 跑(&src, 表, 表).expect_err("select 不能当目标");
    assert!(e.contains("sieve 只收是非题"), "{e}");
}

/// (h) 是非题目标回归：池满即 stop，按判好的先后取
#[test]
fn h_是非题目标照旧() {
    const 表: &[&[&str]] = &[&["好记甲", "好乙", "好记丙"]];
    let src = format!(
        "{头}let memo = test(\"这个候选好记吗？\", \"k\");\nlet r = search([], propose, fits, memo, 2, {{width: 2}});\n{读}"
    );
    let v = 跑(&src, 表, 表).unwrap().value_json();
    assert_eq!(v["reason"], json!("stop"));
    assert_eq!(内容(&v["kept"]), ["好记甲", "好记丙"]);
}
