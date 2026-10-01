//! 题树 `walk`（批 8 T6，B185）：生成器事先出整棵题树，JEV 沿出口往下判。path 与 all 两种求值的叶、路径、
//! 未决相同，只差调用怎么发；未决沿 unsure 支继续时随返回值交出（J-05）；keep 代码谓词定的节点不发判断；
//! bound 用尽没到叶；grow 与 with_member / without_member 换材料（taint 不被洗掉）；validate_tree 的报文带路径；
//! 拿不准时补信息再判（enrich：select 选最缺的一类 → fetch → 再判同一节点）；K 选一节点与现场模板的树形；
//! 组合封闭：walk 的产物嵌进 map、交给 sieve、compose、接在 search 之后。闭包端口，不发请求。
//!
//! 依据：B185（`地基/附注/2026-09-26-批8裁定-字面化与判断分工.md` §五）；意图汇编 7c；B17（契约值）；
//! B131、B161（compose、cert）；预注册 `地基/过程记录/工程-批8-T6.md` 一·4·7 (a)–(j)、一·补·4 (k)–(n)、
//! 一·补三·3 (r)–(w)、一·补四·1 (x)–(z)、四·3 (L1)–(L4)（Q10：叶元素带路径谱系）、六·1·6 (n1)–(n4)（真实生成器输出）。

mod common;

use jpp::effects::{CalibStore, EffectError, FnPort, GenResult, JudgeResult, Ports};
use jpp::interp::TaintOut;
use jpp::ledger::{Entry, Ledger};
use jpp::value::{Answer, Taint, Value};
use jpp::{ActionRegistry, EntryArgs, Outcome, Session};
use serde_json::{Value as Json, json};
use std::cell::RefCell;
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

/// 每次判断调用记一批：[(题面, 材料文本)]
type 批次 = RefCell<Vec<Vec<(String, String)>>>;

/// 判断端口：是非题的读数由 `table(题面, 材料文本)` 定；select（补信息时问「最缺哪一类」）的读数由
/// `pick(候选数)` 定，声明两序置换已测、众数一致（与夹具的 `perms: 2`、`mode_share: 1.0` 同义）。
/// 每次调用（一批，同一状态上的若干题）记下来
fn 判断端口<'a>(calls: &'a 批次, table: &'a dyn Fn(&str, &str) -> f64) -> FnPort<'a> {
    判断端口_选(calls, table, &首选)
}

/// 选第一个候选：[0.8, 其余均分 0.2]
fn 首选(n: usize) -> Vec<f64> {
    (0..n)
        .map(|i| if i == 0 { 0.8 } else { 0.2 / (n - 1) as f64 })
        .collect()
}

fn 判断端口_选<'a>(
    calls: &'a 批次,
    table: &'a dyn Fn(&str, &str) -> f64,
    pick: &'a dyn Fn(usize) -> Vec<f64>,
) -> FnPort<'a> {
    FnPort::judge("fixed-0", move |s, qs| {
        let text = s.on_text();
        calls
            .borrow_mut()
            .push(qs.iter().map(|q| (q.text.clone(), text.clone())).collect());
        let sel = |q: &jpp::value::Question| q.op == jpp::value::Op::Select;
        Ok::<_, EffectError>(JudgeResult {
            answers: qs
                .iter()
                .map(|q| {
                    if sel(q) {
                        Answer::Choice(pick(s.over.len()))
                    } else {
                        Answer::Noul(table(&q.text, &text))
                    }
                })
                .collect(),
            tokens: 0,
            cost: 0.0,
            mode_share: qs.iter().map(|q| sel(q).then_some(1.0)).collect(),
            perms: qs.iter().map(|q| if sel(q) { 2 } else { 0 }).collect(),
            confidence: vec![],
        })
    })
}

/// 生成端口：固定给出 outputs，输出声明为不可信（B149 缺省）
fn 生成端口(outputs: &'static [&'static str]) -> FnPort<'static> {
    FnPort::generate("fixed-0", move |_p, _ctx, _n, _retry| {
        Ok(GenResult {
            outputs: outputs.iter().map(|t| json!(t)).collect(),
            taint_out: Some(Taint::Untrusted),
            ..Default::default()
        })
    })
}

/// 把程序写进 `target/` 下的临时目录（import 只收相对路径），经装载器装上库再跑
fn 跑(src: &str, ports: Ports<'_>) -> Result<Outcome, String> {
    跑_账(src, ports, &mut Ledger::new())
}

/// 同 [`跑`]，账本由调用方给（看补信息事件用）
fn 跑_账(src: &str, ports: Ports<'_>, ledger: &mut Ledger) -> Result<Outcome, String> {
    let dir = root().join(format!(
        "target/compose-tree-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("p.jpp");
    std::fs::write(&path, src).unwrap();
    let loaded = jpp::syntax::loader::load(&path);
    let _ = std::fs::remove_dir_all(&dir);
    let mut program = jpp::lower(&loaded.expect("装载").program).expect("lower");
    program.entry.guard = true; // 测放行把关本身：开 --guard（意图汇编 11a）
    let calib = 库();
    let acts = ActionRegistry::new();
    Session::new(ports, &calib, &acts)
        .with_companions(jpp::interp::CompanionMode::Off)
        .run(&program, &EntryArgs::default(), ledger)
        .map_err(|e| e.render())
}

/// 小树：Q1 → act: Q2（act/ignore 两叶）；ignore: Q3（act/ignore 两叶）。线为作者声明线 0.7 / 0.3
const 头: &str = r#"import "../../lib/compose/tree.jpp";
import "../../lib/compose/carry.jpp";
budget {calls: 40, cost: 0, depth: 64};
let T = {q: "Q1", act: {q: "Q2", act: {leaf: "L-aa"}, ignore: {leaf: "L-ai"}},
                  ignore: {q: "Q3", act: {leaf: "L-ia"}, ignore: {leaf: "L-ii"}}};
let line = {declare: {hi: 0.7, lo: 0.3}};
let look = fn(r) {
    {leaf: map(r.value, fn(e) { e.leaf }), path: map(r.value, fn(e) { map(e.path, fn(p) { p.q + ":" + p.branch }) }),
     kind: map(r.value, fn(e) { exit_kind(e.exit) }), reason: r.detail.reason, depth: r.detail.depth,
     batches: r.detail.batches, asked: r.detail.asked, code: r.detail.decided_by_code,
     causes: map(r.pending, fn(p) { p.cause }), via: map(r.pending, fn(p) { p.via }), pending: r.pending}
};
"#;

fn 值(o: &Outcome) -> Json {
    o.value_json()
}

/// 题面 → 读数的简单表：给出的题按表，其余 0.9
fn 表(pairs: &'static [(&'static str, f64)]) -> impl Fn(&str, &str) -> f64 {
    move |q, _m| {
        pairs
            .iter()
            .find(|(k, _)| *k == q)
            .map(|(_, p)| *p)
            .unwrap_or(0.9)
    }
}

fn 题面们(calls: &批次) -> Vec<Vec<String>> {
    calls
        .borrow()
        .iter()
        .map(|b| b.iter().map(|(q, _)| q.clone()).collect())
        .collect()
}

/// (a) path 模式：act / ignore 下行、到叶；每判一个节点一次调用；声明线按写的数切
#[test]
fn a_path_下行到叶() {
    let calls = RefCell::new(vec![]);
    let t = 表(&[("Q1", 0.86), ("Q2", 0.12)]);
    let ports = Ports::new().with(判断端口(&calls, &t));
    let src = format!(
        "{头}look(walk(T, mat({{a: \"甲\", b: \"乙\"}}), 3, {{line: line, eval: \"path\"}}))"
    );
    let o = 跑(&src, ports).unwrap();
    let v = 值(&o);
    assert_eq!(v["leaf"], json!(["L-ai"]));
    assert_eq!(v["path"], json!([["Q1:act", "Q2:ignore"]]));
    // 叶出口 = compose(路径出口, "all")，强 Kleene 合取（B131）：路径上走过 ignore 支，合成为 ignore——
    // 它读作「路径上每个节点都判了是」，不读作「每一步都已决」；每一步是否已决看 pending 是否为空
    assert_eq!(
        v["kind"],
        json!(["ignore"]),
        "走过 ignore 支的叶出口是 ignore"
    );
    assert_eq!(v["causes"], json!([]), "路径每一步都已决：pending 为空");
    assert_eq!(v["reason"], json!("leaf"));
    assert_eq!(v["depth"], json!(2));
    assert_eq!(v["batches"], json!(2));
    assert_eq!(v["asked"], json!(2));
    assert_eq!(
        题面们(&calls),
        vec![vec!["Q1"], vec!["Q2"]],
        "每判一个节点一次调用"
    );
    assert!(o.returned_unsure.is_empty());

    // 不给线：cut(r) 按题的 calib 查记录；键 k 上岗（0.75 / 0.25，δ 0.05），0.86 走 act、0.12 走 ignore
    let calls = RefCell::new(vec![]);
    let ports = Ports::new().with(判断端口(&calls, &t));
    let src = format!(
        "{头}look(walk(T, mat({{a: \"甲\", b: \"乙\"}}), 3, {{calib: \"k\", eval: \"path\"}}))"
    );
    let v = 值(&跑(&src, ports).unwrap());
    assert_eq!(v["leaf"], json!(["L-ai"]));
}

/// (k) 缺省 all 模式：同一材料上的整组节点一次调用，叶、路径与 path 模式相同
#[test]
fn k_all_缺省_同材料一批() {
    let calls = RefCell::new(vec![]);
    let t = 表(&[("Q1", 0.86), ("Q2", 0.12)]);
    let ports = Ports::new().with(判断端口(&calls, &t));
    let src = format!("{头}look(walk(T, mat({{a: \"甲\", b: \"乙\"}}), 3, {{line: line}}))");
    let o = 跑(&src, ports).unwrap();
    let v = 值(&o);
    assert_eq!(v["leaf"], json!(["L-ai"]));
    assert_eq!(v["path"], json!([["Q1:act", "Q2:ignore"]]));
    assert_eq!(v["batches"], json!(1));
    assert_eq!(v["asked"], json!(3), "没走到的 Q3 也发了");
    assert_eq!(题面们(&calls), vec![vec!["Q1", "Q2", "Q3"]]);
}

/// 合作题树（与 examples/tree-collab.jpp 同形，题面缩短）：N2、N5 经 grow 并入 c、d
const 合作树: &str = r#"let members = {c: "丙", d: "丁"};
let grow = fn(m, node) { if has(node, "add") { with_member(m, node.add, members[node.add]) } else { m } };
let C = {q: "N1", act: {q: "N2", add: "c",
                        act: {q: "N3", act: {q: "N4", act: {leaf: "三方"}, ignore: {leaf: "来源另查"},
                                                        unsure: {q: "N5", add: "d", act: {leaf: "四方"}, ignore: {leaf: "三方为宜"}}},
                                       ignore: {leaf: "不变大"}},
                        ignore: {leaf: "两人即可"}},
                  ignore: {q: "N6", act: {leaf: "先共享数据"}, ignore: {leaf: "不合作"}}};
"#;

/// (k) 合作题树：path 5 批、all 3 批（{a,b} 上 N1、N6；{a,b,c} 上 N2、N3、N4；{a,b,c,d} 上 N5），叶、路径、未决相同
#[test]
fn k_合作树_path_五批_all_三批() {
    let t = 表(&[
        ("N1", 0.86),
        ("N2", 0.82),
        ("N3", 0.78),
        ("N4", 0.52),
        ("N5", 0.80),
        ("N6", 0.20),
    ]);
    let mut 结果 = vec![];
    for how in ["path", "all"] {
        let calls = RefCell::new(vec![]);
        let ports = Ports::new().with(判断端口(&calls, &t));
        let src = format!(
            "{头}{合作树}look(walk(C, mat({{a: \"甲\", b: \"乙\"}}), 5, {{line: line, grow: grow, eval: \"{how}\"}}))"
        );
        let o = 跑(&src, ports).unwrap();
        let v = 值(&o);
        assert_eq!(v["leaf"], json!(["四方"]), "{how}");
        assert_eq!(
            v["path"],
            json!([["N1:act", "N2:act", "N3:act", "N4:unsure", "N5:act"]]),
            "{how}"
        );
        assert_eq!(v["kind"], json!(["unsure(band)"]), "{how}");
        assert_eq!(v["causes"], json!(["band"]), "{how}");
        assert_eq!(v["via"], json!([["walk#3"]]), "{how}");
        assert_eq!(o.returned_unsure, ["unsure(band)"], "{how}");
        结果.push((how, 题面们(&calls), calls.borrow().clone()));
    }
    assert_eq!(
        结果[0].1,
        vec![vec!["N1"], vec!["N2"], vec!["N3"], vec!["N4"], vec!["N5"]]
    );
    assert_eq!(
        结果[1].1,
        vec![vec!["N1", "N6"], vec!["N2", "N3", "N4"], vec!["N5"]]
    );
    // 材料：第二批在 {a,b,c} 上，第三批在 {a,b,c,d} 上
    let all = &结果[1].2;
    assert!(!all[0][0].1.contains('丙'));
    assert!(all[1][0].1.contains('丙') && !all[1][0].1.contains('丁'));
    assert!(all[2][0].1.contains('丁'));
}

/// (b) 未决沿 unsure 支继续：pending 一个元素（band，via walk#0），叶出口未决，程序正常返回
#[test]
fn b_未决沿_unsure_支继续() {
    let calls = RefCell::new(vec![]);
    let t = 表(&[("Q1", 0.5)]);
    let ports = Ports::new().with(判断端口(&calls, &t));
    let src = format!(
        "{头}let U = {{q: \"Q1\", act: {{leaf: \"A\"}}, ignore: {{leaf: \"I\"}}, unsure: {{q: \"Q4\", act: {{leaf: \"UA\"}}, ignore: {{leaf: \"UI\"}}}}}};
look(walk(U, mat(\"材料\"), 3, {{line: line}}))"
    );
    let o = 跑(&src, ports).unwrap();
    let v = 值(&o);
    assert_eq!(v["leaf"], json!(["UA"]));
    assert_eq!(v["path"], json!([["Q1:unsure", "Q4:act"]]));
    assert_eq!(v["kind"], json!(["unsure(band)"]), "合成吸收路径上的未决");
    assert_eq!(v["causes"], json!(["band"]));
    assert_eq!(v["via"], json!([["walk#0"]]));
    assert_eq!(o.returned_unsure, ["unsure(band)"]);
}

/// (c) 未决而没有 unsure 子节点：停在该节点，value 一个元素、leaf 为 unit，reason unsure
#[test]
fn c_未决无子节点_停下() {
    let calls = RefCell::new(vec![]);
    let t = 表(&[("Q2", 0.5)]);
    let ports = Ports::new().with(判断端口(&calls, &t));
    let src = format!("{头}look(walk(T, mat(\"材料\"), 3, {{line: line}}))");
    let o = 跑(&src, ports).unwrap();
    let v = 值(&o);
    assert_eq!(v["leaf"], json!([null]));
    assert_eq!(v["path"], json!([["Q1:act", "Q2:unsure"]]));
    assert_eq!(v["reason"], json!("unsure"));
    assert_eq!(v["causes"], json!(["band"]));
}

/// (d)(l) keep 为假：该节点不发判断，走 ignore 支，path 该项 exit 为 unit，decided_by_code 一项；
/// all 模式下它不进批次，它的 act 支（Q2）也不进
#[test]
fn d_keep_代码定的节点不发判断() {
    for how in ["path", "all"] {
        let calls = RefCell::new(vec![]);
        let t = 表(&[]);
        let ports = Ports::new().with(判断端口(&calls, &t));
        let src = format!(
            "{头}let r = walk(T, mat(\"材料\"), 3, {{line: line, eval: \"{how}\", keep: fn(m, node) {{ node.q != \"Q1\" }}}});
{{look: look(r), first: map(r.value, fn(e) {{ to_json(e.path[0].exit) }})}}"
        );
        let o = 跑(&src, ports).unwrap();
        let v = 值(&o);
        assert_eq!(v["look"]["leaf"], json!(["L-ia"]), "{how}");
        assert_eq!(v["look"]["path"], json!([["Q1:ignore", "Q3:act"]]), "{how}");
        assert_eq!(v["look"]["code"], json!([{"q": "Q1", "step": 0}]), "{how}");
        assert_eq!(v["first"], json!(["null"]), "{how}：代码定的节点没有出口");
        assert_eq!(题面们(&calls), vec![vec!["Q3"]], "{how}：Q1、Q2 都没发");
    }
}

/// (e) bound：树比 depth 深，value 为空，reason bound，已判节点的未决照样在 pending
#[test]
fn e_bound_没到叶() {
    for how in ["path", "all"] {
        let calls = RefCell::new(vec![]);
        let t = 表(&[("Q1", 0.5)]);
        let ports = Ports::new().with(判断端口(&calls, &t));
        let src = format!(
            "{头}let U = {{q: \"Q1\", act: {{leaf: \"A\"}}, ignore: {{leaf: \"I\"}}, unsure: {{q: \"Q4\", act: {{leaf: \"UA\"}}, ignore: {{leaf: \"UI\"}}}}}};
look(walk(U, mat(\"材料\"), 1, {{line: line, eval: \"{how}\"}}))"
        );
        let o = 跑(&src, ports).unwrap();
        let v = 值(&o);
        assert_eq!(v["leaf"], json!([]), "{how}");
        assert_eq!(v["reason"], json!("bound"), "{how}");
        assert_eq!(v["causes"], json!(["band"]), "{how}");
        // (m) all 模式的组按剩余 depth 截断：depth 1 时只发 Q1，不发 Q4
        assert_eq!(题面们(&calls), vec![vec!["Q1"]], "{how}");
        assert_eq!(o.returned_unsure, ["unsure(band)"], "{how}");
    }
}

/// (f) 材料：grow + with_member 后判断看到新成员；without_member 去掉字段；可信材料并入不可信成员（gen 输出）
/// 后产物 taint 为 untrusted
#[test]
fn f_材料_加减成员与_taint() {
    let calls = RefCell::new(vec![]);
    let t = 表(&[]);
    let ports = Ports::new()
        .with(判断端口(&calls, &t))
        .with(生成端口(&["生成的成员"]));
    let src = format!(
        "{头}let G = {{q: \"Q1\", act: {{q: \"Q2\", add: \"c\", act: {{leaf: \"好\"}}, ignore: {{leaf: \"坏\"}}}}, ignore: {{leaf: \"不\"}}}};
let base = mat({{a: \"甲\", b: \"乙\"}});
let r = walk(G, base, 3, {{line: line, eval: \"path\", grow: fn(m, node) {{ if has(node, \"add\") {{ with_member(m, node.add, \"丙\") }} else {{ m }} }}}});
let g = gen(\"给一个成员\", [], 1, 0);
let mixed = with_member(base, \"c\", g[0]);
{{look: look(r), item: map(r.value, fn(e) {{ content(e.item) }}),
  dropped: content(without_member(mat({{a: \"甲\", b: \"乙\", c: \"丙\"}}), \"b\")),
  mixed: content(mixed), mixed_taint: mixed.taint, base_taint: base.taint,
  kept_taint: without_member(mixed, \"c\").taint}}"
    );
    let o = 跑(&src, ports).unwrap();
    let v = 值(&o);
    assert_eq!(v["look"]["leaf"], json!(["好"]));
    assert_eq!(v["item"], json!([{"a": "甲", "b": "乙", "c": "丙"}]));
    let c = calls.borrow();
    assert_eq!(c.len(), 2, "材料变了，Q2 另起一次调用");
    assert!(!c[0][0].1.contains('丙'), "{:?}", c[0]);
    assert!(c[1][0].1.contains('丙'), "{:?}", c[1]);
    assert_eq!(v["dropped"], json!({"a": "甲", "c": "丙"}));
    assert_eq!(v["mixed"], json!({"a": "甲", "b": "乙", "c": "生成的成员"}));
    assert_eq!(v["base_taint"], json!("trusted"));
    assert_eq!(
        v["mixed_taint"],
        json!("untrusted"),
        "不可信成员不被洗成可信"
    );
    assert_eq!(
        v["kept_taint"],
        json!("untrusted"),
        "去掉成员不把整份材料洗回可信（保守）"
    );
}

/// (g) validate_tree：合格原样返回；缺 q、q 不是文字、缺 act、子节点不是记录、超深各返回 Fail，报文带路径；
/// 输入是 Fail 时原样返回
#[test]
fn g_validate_tree_报文带路径() {
    let src = format!(
        "{头}let t = fn(x) {{ let r = validate_tree(x, 2); if is_fail(r) {{ text(r) }} else {{ \"ok\" }} }};
[t(T), t({{act: {{leaf: 1}}, ignore: {{leaf: 2}}}}), t({{q: 5, act: {{leaf: 1}}, ignore: {{leaf: 2}}}}),
 t({{q: \"x\", ignore: {{leaf: 1}}}}), t({{q: \"x\", act: \"是\", ignore: {{leaf: 1}}}}),
 text(validate_tree(T, 1)), is_fail(validate_tree(parse_json(\"{{坏\"), 3))]"
    );
    let o = 跑(&src, Ports::new()).unwrap();
    let v = 值(&o);
    let s = |i: usize| v[i].as_str().unwrap().to_string();
    assert_eq!(s(0), "ok");
    assert!(s(1).contains("根：节点缺题面 q"), "{}", s(1));
    assert!(s(2).contains("根：节点缺题面 q"), "{}", s(2));
    assert!(s(3).contains("根：节点缺 act 或 ignore"), "{}", s(3));
    assert!(s(4).contains("根.act：不是记录"), "{}", s(4));
    assert!(s(5).contains("根.act：超过深度上限"), "{}", s(5));
    assert_eq!(v[6], json!(true));
}

/// (h) 形状不合的树交给 walk：reason invalid、detail.failed 是 Fail、不发判断
#[test]
fn h_形状不合_不发判断() {
    let calls = RefCell::new(vec![]);
    let t = 表(&[]);
    let ports = Ports::new().with(判断端口(&calls, &t));
    let src = format!(
        "{头}let r = walk({{q: \"Q1\", act: {{leaf: 1}}}}, mat(\"材料\"), 3, {{line: line}});
{{look: look(r), failed: is_fail(r.detail.failed), why: text(r.detail.failed)}}"
    );
    let o = 跑(&src, ports).unwrap();
    let v = 值(&o);
    assert_eq!(v["look"]["reason"], json!("invalid"));
    assert_eq!(v["look"]["leaf"], json!([]));
    assert_eq!(v["failed"], json!(true));
    assert!(v["why"].as_str().unwrap().contains("缺 act 或 ignore"));
    assert!(calls.borrow().is_empty());
}

/// (j) eval 给 all、path 以外的值报错，报文含 E-walk-options
#[test]
fn j_eval_只收_all_与_path() {
    let calls = RefCell::new(vec![]);
    let t = 表(&[]);
    let ports = Ports::new().with(判断端口(&calls, &t));
    let src = format!("{头}look(walk(T, mat(\"材料\"), 3, {{line: line, eval: \"bfs\"}}))");
    let e = 跑(&src, ports).unwrap_err();
    assert!(e.contains("E-walk-options"), "{e}");
}

/// (n) all 模式里没走到的节点读数落在线之间：只有读数、没有出口，不产生 J-05；pending 只含路径上的未决
#[test]
fn n_没走到的节点带内不产生责任() {
    let calls = RefCell::new(vec![]);
    let t = 表(&[("Q1", 0.9), ("Q2", 0.9), ("Q3", 0.5)]);
    let ports = Ports::new().with(判断端口(&calls, &t));
    let src = format!("{头}look(walk(T, mat(\"材料\"), 3, {{line: line}}))");
    let o = 跑(&src, ports).unwrap();
    let v = 值(&o);
    assert_eq!(v["leaf"], json!(["L-aa"]));
    assert_eq!(v["causes"], json!([]));
    assert_eq!(v["asked"], json!(3));
    assert!(o.returned_unsure.is_empty());
}

// ---------- (r)–(w) 拿不准时补信息再判（意图汇编 7c） ----------

/// 补信息的小树：Q1 列了三类可能缺的信息；fetch 把「补来的<缺项>」补进材料
const 补树: &str = r#"let E = {q: "Q1", need: ["缺甲", "缺乙", "缺丙"], act: {leaf: "A"}, ignore: {leaf: "I"}};
let fetched = fn(node, need, m) { "补来的" + need };
let look2 = fn(r) {
    {look: look(r), enriched: map(r.value, fn(e) { map(e.path, fn(p) { p.enriched }) }),
     detail: r.detail.enriched, item: map(r.value, fn(e) { content(e.item) })}
};
"#;

/// (r)(w) 补一次后判定：select 选中第一类，fetch 取来补进材料，重判看到补进的字段，act 到叶；首判的未决被取代，
/// 不在 pending 里。path 与 all 两种模式结果相同
#[test]
fn r_补信息后判定() {
    for how in ["path", "all"] {
        let calls = RefCell::new(vec![]);
        let t = |q: &str, m: &str| -> f64 {
            if q == "Q1" && m.contains("补来的缺甲") {
                0.9
            } else {
                0.5
            }
        };
        let ports = Ports::new().with(判断端口(&calls, &t));
        let src = format!(
            "{头}{补树}look2(walk(E, mat({{a: \"甲\"}}), 3, {{line: line, eval: \"{how}\",
                              enrich: {{rounds: 2, line: {{declare: {{hi: 0.5}}}}, fetch: fetched}}}}))"
        );
        let o = 跑(&src, ports).unwrap();
        let v = 值(&o);
        assert_eq!(v["look"]["leaf"], json!(["A"]), "{how}");
        assert_eq!(v["look"]["path"], json!([["Q1:act"]]), "{how}");
        assert_eq!(v["enriched"], json!([[["缺甲"]]]), "{how}");
        assert_eq!(
            v["detail"],
            json!([{"q": "Q1", "need": "缺甲", "step": 0}]),
            "{how}"
        );
        assert_eq!(
            v["item"],
            json!([{"a": "甲", "缺甲": "补来的缺甲"}]),
            "{how}"
        );
        assert_eq!(
            v["look"]["causes"],
            json!([]),
            "{how}：首判的未决被重判取代"
        );
        assert!(o.returned_unsure.is_empty(), "{how}");
        let qs = 题面们(&calls);
        assert_eq!(qs.len(), 3, "{how}：首判、select、重判：{qs:?}");
        assert!(qs[1][0].contains("最缺"), "{how}：{qs:?}");
        assert!(calls.borrow()[2][0].1.contains("补来的缺甲"), "{how}");
    }
}

/// (s) 补了 rounds 次仍未决：最后一次的出口进 pending；第二轮的候选剔除了补过的那一类
#[test]
fn s_补够轮数仍未决() {
    let calls = RefCell::new(vec![]);
    let t = 表(&[("Q1", 0.5)]);
    let ports = Ports::new().with(判断端口(&calls, &t));
    let src = format!(
        "{头}{补树}look2(walk(E, mat({{a: \"甲\"}}), 3, {{line: line, enrich: {{rounds: 2, line: {{declare: {{hi: 0.5}}}}, fetch: fetched}}}}))"
    );
    let o = 跑(&src, ports).unwrap();
    let v = 值(&o);
    assert_eq!(v["enriched"], json!([[["缺甲", "缺乙"]]]));
    assert_eq!(v["look"]["reason"], json!("unsure"));
    assert_eq!(v["look"]["causes"], json!(["band"]));
    assert_eq!(v["look"]["via"], json!([["walk#0"]]));
    assert_eq!(
        v["item"],
        json!([{"a": "甲", "缺甲": "补来的缺甲", "缺乙": "补来的缺乙"}])
    );
    assert_eq!(o.returned_unsure, ["unsure(band)"]);
}

/// (t) select 选不出最缺哪一类：不 fetch，select 的未决与节点的未决都进 pending
#[test]
fn t_select_未决不补() {
    let calls = RefCell::new(vec![]);
    let t = 表(&[("Q1", 0.5)]);
    let 平 = |n: usize| vec![1.0 / n as f64; n];
    let ports = Ports::new().with(判断端口_选(&calls, &t, &平));
    let src = format!(
        "{头}{补树}look2(walk(E, mat({{a: \"甲\"}}), 3, {{line: line, enrich: {{rounds: 2, line: {{declare: {{hi: 0.5}}}}, fetch: fetched}}}}))"
    );
    let o = 跑(&src, ports).unwrap();
    let v = 值(&o);
    assert_eq!(v["enriched"], json!([[[]]]));
    assert_eq!(v["look"]["via"], json!([["walk#0:need"], ["walk#0"]]));
    assert_eq!(o.returned_unsure.len(), 2);
    assert_eq!(calls.borrow().len(), 2, "首判与 select，没有重判");
}

/// (u) fetch 返回 Fail：停止补，节点的未决进 pending
#[test]
fn u_fetch_失败停止补() {
    let calls = RefCell::new(vec![]);
    let t = 表(&[("Q1", 0.5)]);
    let ports = Ports::new().with(判断端口(&calls, &t));
    let src = format!(
        "{头}{补树}look2(walk(E, mat({{a: \"甲\"}}), 3, {{line: line, enrich: {{rounds: 2, line: {{declare: {{hi: 0.5}}}},
                              fetch: fn(node, need, m) {{ fail(\"查不到\") }}}}}}))"
    );
    let o = 跑(&src, ports).unwrap();
    let v = 值(&o);
    assert_eq!(v["enriched"], json!([[[]]]));
    assert_eq!(v["look"]["causes"], json!(["band"]));
    assert_eq!(v["look"]["via"], json!([["walk#0"]]));
    assert_eq!(calls.borrow().len(), 2, "首判与 select，fetch 失败后不再判");
}

/// (u′) 复查 2026-09-30 小项 4：fetch 失败的那一轮在账本记 `Enrich{got: false}`，责任不动（不记 Refine），
/// 节点的未决随 pending 交出，记 Handoff
#[test]
fn u2_fetch_失败记一轮取不到() {
    let calls = RefCell::new(vec![]);
    let t = 表(&[("Q1", 0.5)]);
    let ports = Ports::new().with(判断端口(&calls, &t));
    let src = format!(
        "{头}{补树}look2(walk(E, mat({{a: \"甲\"}}), 3, {{line: line, enrich: {{rounds: 2, line: {{declare: {{hi: 0.5}}}},
                              fetch: fn(node, need, m) {{ fail(\"查不到\") }}}}}}))"
    );
    let mut l = Ledger::new();
    跑_账(&src, ports, &mut l).unwrap();
    let ev: Vec<&Entry> = l.entries.iter().filter(|e| e.is_duty_event()).collect();
    let 取不到: Vec<&Entry> = ev
        .iter()
        .copied()
        .filter(|e| matches!(e, Entry::Enrich { got: false, .. }))
        .collect();
    assert_eq!(取不到.len(), 1, "{ev:?}");
    assert!(
        matches!(取不到[0], Entry::Enrich { need, round: 1, .. } if need.starts_with("缺")),
        "{ev:?}"
    );
    assert!(
        !ev.iter()
            .any(|e| matches!(e, Entry::Refine { .. } | Entry::Drop { .. })),
        "{ev:?}"
    );
    assert!(
        ev.iter().any(|e| matches!(e, Entry::Handoff { .. })),
        "{ev:?}"
    );
}

/// (v) 文字材料的缺省 merge：「缺项：取来的」接在末尾
#[test]
fn v_文字材料接在末尾() {
    let calls = RefCell::new(vec![]);
    let t = |q: &str, m: &str| -> f64 {
        if q == "Q1" && m.contains("缺甲：补来的缺甲") {
            0.9
        } else {
            0.5
        }
    };
    let ports = Ports::new().with(判断端口(&calls, &t));
    let src = format!(
        "{头}{补树}look2(walk(E, mat(\"甲的情况\"), 3, {{line: line, enrich: {{rounds: 1, line: {{declare: {{hi: 0.5}}}}, fetch: fetched}}}}))"
    );
    let o = 跑(&src, ports).unwrap();
    let v = 值(&o);
    assert_eq!(v["look"]["leaf"], json!(["A"]));
    assert_eq!(v["item"], json!(["甲的情况\n缺甲：补来的缺甲"]));
}

// ---------- (x)–(z) K 选一节点、现场模板的树形、叶出口在有未决时一律未决（预注册一·补四） ----------

/// K 选一节点：金额「变大 / 变小 / 不变」，picks 与 over 等长；未决走 unsure 子节点
const 选树: &str = r#"let X = {kind: "select", q: "S1", over: ["变大", "变小", "不变"],
         picks: [{leaf: "大"}, {q: "Q2", act: {leaf: "小-A"}, ignore: {leaf: "小-I"}}, {leaf: "平"}],
         unsure: {leaf: "说不清"}};
"#;

/// (x) pick(k) 走 picks[k]；选不出时未决进 pending、走 unsure 子节点。path 与 all 相同
#[test]
fn x_k选一节点() {
    for how in ["path", "all"] {
        let calls = RefCell::new(vec![]);
        let t = 表(&[]);
        let ports = Ports::new().with(判断端口(&calls, &t));
        let src =
            format!("{头}{选树}look(walk(X, mat(\"材料\"), 3, {{line: line, eval: \"{how}\"}}))");
        let o = 跑(&src, ports).unwrap();
        let v = 值(&o);
        assert_eq!(v["leaf"], json!(["大"]), "{how}");
        assert_eq!(v["path"], json!([["S1:pick:变大"]]), "{how}");
        assert_eq!(v["causes"], json!([]), "{how}");

        let calls = RefCell::new(vec![]);
        let 平 = |n: usize| vec![1.0 / n as f64; n];
        let ports = Ports::new().with(判断端口_选(&calls, &t, &平));
        let o = 跑(&src, ports).unwrap();
        let v = 值(&o);
        assert_eq!(v["leaf"], json!(["说不清"]), "{how}");
        assert_eq!(v["path"], json!([["S1:unsure"]]), "{how}");
        assert_eq!(v["via"], json!([["walk#0"]]), "{how}");
        assert_eq!(v["kind"], json!(["unsure(band)"]), "{how}");
        assert_eq!(o.returned_unsure.len(), 1, "{how}");
    }
}

/// (y) 现场模板形状的树（kind、missing、over / picks）经 walk 跑通；missing 当作候选缺项用上
#[test]
fn y_现场模板形状() {
    let calls = RefCell::new(vec![]);
    let t = |q: &str, m: &str| -> f64 {
        if q == "T1" && !m.contains("补来的") {
            0.5
        } else {
            0.9
        }
    };
    let ports = Ports::new().with(判断端口(&calls, &t));
    let src = format!(
        "{头}let Y = {{kind: \"test\", q: \"T1\", missing: [\"缺甲\", \"缺乙\"],
         act: {{kind: \"select\", q: \"S\", over: [\"变大\", \"变小\"], picks: [{{leaf: {{amount: \"变大\"}}}}, {{leaf: {{amount: \"变小\"}}}}],
                unsure: {{leaf: {{amount: \"拿不准\"}}}}}},
         ignore: {{leaf: {{cooperation: \"不适合\"}}}}, unsure: {{leaf: {{cooperation: \"补过仍拿不准\"}}}}}};
let v = validate_tree(Y, 3);
let r = walk(v, mat({{a: \"甲\", b: \"乙\"}}), 3, {{line: line, enrich: {{rounds: 2, line: {{declare: {{hi: 0.5}}}},
                                                   fetch: fn(node, need, m) {{ \"补来的\" + need }}}}}});
{{ok: !is_fail(v), look: look(r), enriched: map(r.detail.enriched, fn(x) {{ x.need }})}}"
    );
    let o = 跑(&src, ports).unwrap();
    let v = 值(&o);
    assert_eq!(v["ok"], json!(true));
    assert_eq!(v["look"]["leaf"], json!([{"amount": "变大"}]));
    assert_eq!(v["look"]["path"], json!([["T1:act", "S:pick:变大"]]));
    assert_eq!(v["enriched"], json!(["缺甲"]));
    assert_eq!(v["look"]["causes"], json!([]));
}

/// (z) 路径 [ignore, unsure]：叶出口是未决（合成时去掉 ignore），不再是 ignore
#[test]
fn z_有未决时叶出口未决() {
    let calls = RefCell::new(vec![]);
    let t = 表(&[("Q1", 0.1), ("Q3", 0.5)]);
    let ports = Ports::new().with(判断端口(&calls, &t));
    let src = format!("{头}look(walk(T, mat(\"材料\"), 3, {{line: line}}))");
    let o = 跑(&src, ports).unwrap();
    let v = 值(&o);
    assert_eq!(v["path"], json!([["Q1:ignore", "Q3:unsure"]]));
    assert_eq!(v["reason"], json!("unsure"));
    assert_eq!(v["kind"], json!(["unsure(band)"]));
    assert_eq!(v["causes"], json!(["band"]));
}

/// (g2) validate_tree 查 K 选一节点：缺 over、picks 与 over 不等长各返回 Fail
#[test]
fn g2_validate_tree_k选一() {
    let src = format!(
        "{头}let t = fn(x) {{ let r = validate_tree(x, 2); if is_fail(r) {{ text(r) }} else {{ \"ok\" }} }};
[t({{kind: \"select\", q: \"S\", picks: [{{leaf: 1}}]}}),
 t({{q: \"S\", over: [\"甲\", \"乙\"], picks: [{{leaf: 1}}]}}),
 t({{q: \"S\", over: [\"甲\", \"乙\"], picks: [{{leaf: 1}}, {{q: \"x\"}}]}}),
 t({{q: \"S\", over: [\"甲\"], picks: [{{leaf: 1}}]}})]"
    );
    let o = 跑(&src, Ports::new()).unwrap();
    let v = 值(&o);
    let s = |i: usize| v[i].as_str().unwrap().to_string();
    assert!(s(0).contains("缺候选列表 over"), "{}", s(0));
    assert!(s(1).contains("与 over 等长的 picks"), "{}", s(1));
    assert!(s(2).contains("根.pick1：节点缺 act 或 ignore"), "{}", s(2));
    assert_eq!(s(3), "ok");
}

// ---------- (i) 组合封闭：walk 的产物是契约值，能再组合 ----------

/// (i1) map 里对两组人各走一遍，两次的 pending 经 carry 并进外层（via 两层）
#[test]
fn i1_map_里对多组人各走一遍() {
    let calls = RefCell::new(vec![]);
    // 甲乙这组 Q1 走 act；丙丁这组 Q1 落带内
    let t = |q: &str, m: &str| -> f64 {
        if q == "Q1" && m.contains('丙') {
            0.5
        } else if q == "Q2" {
            0.1
        } else {
            0.9
        }
    };
    let ports = Ports::new().with(判断端口(&calls, &t));
    let src = format!(
        "{头}let U = {{q: \"Q1\", act: {{q: \"Q2\", act: {{leaf: \"合\"}}, ignore: {{leaf: \"合但不扩\"}}}}, ignore: {{leaf: \"不合\"}},
          unsure: {{q: \"Q3\", act: {{leaf: \"先谈\"}}, ignore: {{leaf: \"不谈\"}}}}}};
let groups = [mat({{a: \"甲\", b: \"乙\"}}), mat({{a: \"丙\", b: \"丁\"}})];
let rs = map(groups, fn(g) {{ walk(U, g, 3, {{line: line}}) }});
let all = carry(carry([], rs[0].pending, \"组#0\"), rs[1].pending, \"组#1\");
{{leaves: map(rs, fn(r) {{ map(r.value, fn(e) {{ e.leaf }}) }}), via: map(all, fn(p) {{ p.via }}), pending: all}}"
    );
    let o = 跑(&src, ports).unwrap();
    let v = 值(&o);
    assert_eq!(v["leaves"], json!([["合但不扩"], ["先谈"]]));
    assert_eq!(v["via"], json!([["walk#0", "组#1"]]));
    assert_eq!(o.returned_unsure, ["unsure(band)"]);
    assert_eq!(calls.borrow().len(), 2, "每组人一份材料、一批");
}

/// (i2) sieve(walk 的产物, 另一道题)：叶元素的 item（到叶时的材料）再判一次，trail 接上 walk 的合成出口；
/// walk 的 pending 随契约值并进 sieve 的 pending
#[test]
fn i2_walk_的产物交给_sieve() {
    let calls = RefCell::new(vec![]);
    let t = 表(&[("Q1", 0.5)]);
    let ports = Ports::new().with(判断端口(&calls, &t));
    let src = format!(
        "{头}let U = {{q: \"Q1\", act: {{leaf: \"A\"}}, ignore: {{leaf: \"I\"}}, unsure: {{q: \"Q4\", act: {{leaf: \"UA\"}}, ignore: {{leaf: \"UI\"}}}}}};
let r = walk(U, mat(\"材料\"), 3, {{line: line}});
let s = sieve(r, test(\"Q9\", \"k\"));
{{kept: map(s.value, fn(e) {{ content(e.item) }}), leaf: map(s.value, fn(e) {{ e.leaf }}),
  trail: map(s.value, fn(e) {{ map(e.trail, fn(x) {{ exit_kind(x) }}) }}),
  causes: map(s.pending, fn(p) {{ p.cause }}), pending: s.pending}}"
    );
    let o = 跑(&src, ports).unwrap();
    let v = 值(&o);
    assert_eq!(v["kept"], json!(["材料"]));
    assert_eq!(v["leaf"], json!(["UA"]), "sieve 保留输入元素的字段");
    assert_eq!(
        v["trail"],
        json!([["unsure(band)"]]),
        "trail 接上 walk 的合成出口"
    );
    assert_eq!(
        v["causes"],
        json!(["band"]),
        "walk 的未决并进 sieve 的 pending"
    );
    let last = calls.borrow().last().unwrap().clone();
    assert_eq!(last, vec![("Q9".to_string(), "材料".to_string())]);
}

/// (i3) compose 与 cert：几次 walk 的叶出口合成；全 act 为 act，混进一次带未决的为未决
#[test]
fn i3_walk_的出口再合成() {
    let calls = RefCell::new(vec![]);
    let t = |q: &str, m: &str| -> f64 { if q == "Q2" && m == "三" { 0.5 } else { 0.9 } };
    let ports = Ports::new().with(判断端口(&calls, &t));
    let src = format!(
        "{头}let rs = map([mat(\"一\"), mat(\"二\"), mat(\"三\")], fn(m) {{ walk(T, m, 3, {{line: line}}) }});
let e = map(rs, fn(r) {{ r.value[0].exit }});
let two = compose([e[0], e[1]], \"all\");
let three = compose(e, \"all\");
{{two: exit_kind(two), three: exit_kind(three), n_unknown: cert(two).n_unknown,
  pending: concat(concat(rs[0].pending, rs[1].pending), rs[2].pending)}}"
    );
    let o = 跑(&src, ports).unwrap();
    let v = 值(&o);
    assert_eq!(v["two"], json!("act"));
    assert_eq!(v["three"], json!("unsure(band)"));
    assert_eq!(v["n_unknown"], json!(4), "声明线分量一律按 1 计（B161）");
}

/// (i4) search 的产物交给 walk：对 search 选出的每个候选各走一遍题树
#[test]
fn i4_search_之后接_walk() {
    let calls = RefCell::new(vec![]);
    let t = |q: &str, m: &str| -> f64 {
        if q == "候选合适吗？" {
            if m.starts_with('好') { 0.9 } else { 0.1 }
        } else if q == "Q1" && m == "好乙" {
            0.1
        } else {
            0.9
        }
    };
    let ports = Ports::new()
        .with(判断端口(&calls, &t))
        .with(生成端口(&["好甲", "好乙", "坏丙"]));
    let src = format!(
        "import \"../../lib/compose/search.jpp\";\n{头}let propose = fn(frontier, i) {{ gen(\"提 3 个候选\", frontier, 3, i) }};
let s = search([], propose, test(\"候选合适吗？\", \"k\"), unit, 1, {{width: 2}});
let ws = map(s.value, fn(e) {{ walk(T, e.item, 3, {{line: line}}) }});
{{kept: map(s.value, fn(e) {{ content(e.item) }}), leaves: map(ws, fn(r) {{ map(r.value, fn(e) {{ e.leaf }}) }}),
  pending: concat(s.pending, concat(ws[0].pending, ws[1].pending))}}"
    );
    let o = 跑(&src, ports).unwrap();
    let v = 值(&o);
    assert_eq!(v["kept"], json!(["好甲", "好乙"]));
    assert_eq!(v["leaves"], json!([["L-aa"], ["L-ia"]]));
}

// ---------- (L1)–(L4) Q10：叶元素带路径谱系，下游再判、再守不可逆 do 时 J-08 看得到路径 ----------

/// 正式线「复核」键 k2（测试合成证书，见 tests/common）；登记不可逆动作「发邮件」。宿主是否接受作者声明线由 `接受` 定
fn 会话跑(src: &str, ports: Ports<'_>, 接受: bool) -> Result<Outcome, String> {
    let dir = root().join(format!(
        "target/compose-tree-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("p.jpp");
    std::fs::write(&path, src).unwrap();
    let loaded = jpp::syntax::loader::load(&path);
    let _ = std::fs::remove_dir_all(&dir);
    let mut program = jpp::lower(&loaded.expect("装载").program).expect("lower");
    program.entry.guard = true; // 测放行把关本身：开 --guard（意图汇编 11a）
    let mut calib = CalibStore::new();
    common::certified(&mut calib, "k2", 0.8, 0.2, 50);
    let mut acts = ActionRegistry::new();
    acts.register("发邮件", 0.0, false, TaintOut::Trusted, |_| {
        Ok(Value::Text("已发".into(), Taint::Trusted.into()))
    });
    let entry = EntryArgs {
        accept: jpp::HostAccept {
            declared_lines: 接受,
        },
        ..Default::default()
    };
    Session::new(ports, &calib, &acts)
        .with_companions(jpp::interp::CompanionMode::Off)
        .run(&program, &entry, &mut Ledger::new())
        .map_err(|e| e.render())
}

/// 走完题树后对叶材料再判一道正式线的题，在 act 臂里发不可逆动作
const 守: &str = r#"let s = sieve(r, test("复核：这份材料可以发出去吗？", "k2"));
let sent = map(s.value, fn(e) {
    handle(e.exit, {act: fn() { (do("发邮件", [], 0)) }, ignore: fn() { "不发" },
                    unsure: fn(u) { consume(u, "drop"); "不发" }})
});
{sent: sent, pending: s.pending}
"#;

const 字面树: &str = r#"import "../../lib/compose/tree.jpp";
budget {calls: 8, cost: 0, depth: 64};
let T = {q: "a 和 b 适合合作吗？", act: {leaf: "合作"}, ignore: {leaf: "不合作"}};
let r = walk(T, mat({a: "甲", b: "乙"}), 2, {line: {declare: {hi: 0.7, lo: 0.3}}});
"#;

/// (L1) 路径用了声明线、宿主没接受：对叶材料再判一道正式线的题去守不可逆 do，J-08 拦下（谱系放行，B72-4）
#[test]
fn l1_声明线路径_宿主未接受_下游被拦() {
    let calls = RefCell::new(vec![]);
    let t = 表(&[]);
    let ports = Ports::new().with(判断端口(&calls, &t));
    let e = 会话跑(&format!("{字面树}{守}"), ports, false)
        .expect_err("路径上的声明线宿主没接受，下游那道正式线的判断不能单独放行");
    assert!(e.contains("J-08"), "{e}");
    assert!(e.contains("Declared") && e.contains("B72-4"), "{e}");
}

/// (L2) 同 L1，宿主接受声明线：动作照发（挂谱系不误伤接受了声明线的宿主）
#[test]
fn l2_声明线路径_宿主接受_照发() {
    let calls = RefCell::new(vec![]);
    let t = 表(&[]);
    let ports = Ports::new().with(判断端口(&calls, &t));
    let o = 会话跑(&format!("{字面树}{守}"), ports, true).unwrap();
    assert_eq!(值(&o)["sent"][0]["content"], json!("已发"));
}

/// 生成器给出的题树（题面不可信，B58）
const 生成的树: &[&str] =
    &[r#"{"q": "a 和 b 适合合作吗？", "act": {"leaf": "合作"}, "ignore": {"leaf": "不合作"}}"#];

const 生成树程序: &str = r#"import "../../lib/compose/tree.jpp";
budget {calls: 8, cost: 0, depth: 64};
let g = gen("出一棵题树", [], 1, 0);
let T = validate_tree(parse_json(content(g[0])), 2);
let r = walk(T, mat({a: "甲", b: "乙"}), 2, {line: {declare: {hi: 0.7, lo: 0.3}}});
"#;

/// (L3) 题树由生成器给出（题面不可信，B58）、宿主接受声明线：下游被拦（叶材料带上题面的 taint）
#[test]
fn l3_不可信题面_下游被拦() {
    let calls = RefCell::new(vec![]);
    let t = 表(&[]);
    let ports = Ports::new()
        .with(判断端口(&calls, &t))
        .with(生成端口(生成的树));
    let e = 会话跑(&format!("{生成树程序}{守}"), ports, true)
        .expect_err("题面不可信的路径选出的叶材料，下游判断不能单独放行");
    assert!(e.contains("J-08"), "{e}");
}

/// (L4) 对照：同一材料不经 walk，直接再判那道正式线的题去守动作，照发
#[test]
fn l4_不经_walk_照发() {
    let calls = RefCell::new(vec![]);
    let t = 表(&[]);
    let ports = Ports::new().with(判断端口(&calls, &t));
    let src = format!(
        "import \"../../lib/compose/tree.jpp\";\nbudget {{calls: 8, cost: 0, depth: 64}};\nlet r = [mat({{a: \"甲\", b: \"乙\"}})];\n{守}"
    );
    let o = 会话跑(&src, ports, false).unwrap();
    assert_eq!(值(&o)["sent"][0]["content"], json!("已发"));
}

// ---------- (n1)–(n4) 真实生成器的输出：形状规范、词表、parse_tree、add_members（冒烟 3，过程记录六） ----------

/// (n1) validate_tree_with：add 文字规范成列表、当事人与词表外名字剔除；need 按词表筛（比对忽略空格，写回词表原文）、
/// missing 读作 need；剔除的都记在 dropped；形状错报路径
#[test]
fn n1_validate_tree_with_规范与词表() {
    let src = format!(
        "{头}let t = {{q: \"Q\", add: \"c\", missing: \"缺甲\", ignore: {{leaf: 3}},
          act: {{q: \"Q2\", add: [\"a\", \"c\", \"e\"], need: [\"缺 乙\", \"缺丙\"], act: {{leaf: 1}}, ignore: {{leaf: 2}}}}}};
let v = validate_tree_with(t, 3, {{need: [\"缺甲\", \"缺乙\"], members: [\"c\", \"d\"]}});
let e = fn(x) {{ let r = validate_tree_with(x, 3, {{}}); if is_fail(r) {{ text(r) }} else {{ \"ok\" }} }};
{{add: v.tree.add, need: v.tree.need, add2: v.tree.act.add, need2: v.tree.act.need, dropped: v.dropped,
  plain: validate_tree(t, 3).act.add,
  errs: [e({{q: \"Q\", add: [1], act: {{leaf: 1}}, ignore: {{leaf: 2}}}}), e({{q: \"Q\", need: 5, act: {{leaf: 1}}, ignore: {{leaf: 2}}}}),
         e({{q: \"Q\", kind: \"x\", act: {{leaf: 1}}, ignore: {{leaf: 2}}}}), e({{q: \"Q\", act: {{q: \"Q2\", need: [\"甲\", 2], act: {{leaf: 1}}, ignore: {{leaf: 2}}}}, ignore: {{leaf: 2}}}})]}}"
    );
    let v = 值(&跑(&src, Ports::new()).unwrap());
    assert_eq!(v["add"], json!(["c"]));
    assert_eq!(v["need"], json!(["缺甲"]));
    assert_eq!(v["add2"], json!(["c"]));
    assert_eq!(v["need2"], json!(["缺乙"]), "比对忽略空格，写回词表原文");
    assert_eq!(
        v["dropped"],
        json!([{"at": "根.act", "field": "add", "value": "a"}, {"at": "根.act", "field": "add", "value": "e"},
               {"at": "根.act", "field": "need", "value": "缺丙"}])
    );
    assert_eq!(
        v["plain"],
        json!(["a", "c", "e"]),
        "不带词表只规范形状、不剔除"
    );
    let s = |i: usize| v["errs"][i].as_str().unwrap().to_string();
    assert!(s(0).contains("根：add 的每一项都要是文字"), "{}", s(0));
    assert!(s(1).contains("根：need 要是文字或文字列表"), "{}", s(1));
    assert!(s(2).contains("根：kind 只能是 test 或 select"), "{}", s(2));
    assert!(s(3).contains("根.act：need 的每一项都要是文字"), "{}", s(3));
}

/// (n2) add_members：已在材料里的、成员表里没有的跳过；文字与列表都收
#[test]
fn n2_add_members_跳过当事人与生人() {
    let src = format!(
        "{头}let ms = {{c: \"丙\", d: \"丁\"}};
let m = mat({{a: \"甲\", b: \"乙\"}});
{{list: content(add_members(m, [\"a\", \"c\", \"e\"], ms)), one: content(add_members(m, \"d\", ms))}}"
    );
    let v = 值(&跑(&src, Ports::new()).unwrap());
    assert_eq!(v["list"], json!({"a": "甲", "b": "乙", "c": "丙"}));
    assert_eq!(v["one"], json!({"a": "甲", "b": "乙", "d": "丁"}));
}

/// (n3) parse_tree：带代码块标记与说明文字的文字取出 JSON；已是记录原样返回；没有 JSON 返回 Fail
#[test]
fn n3_parse_tree() {
    let src = format!(
        "{头}{{fenced: parse_tree(\"好的，题树如下：\\n```json\\n{{\\\"q\\\": \\\"Q\\\", \\\"act\\\": {{\\\"leaf\\\": 1}}, \\\"ignore\\\": {{\\\"leaf\\\": 2}}}}\\n```\").q,
  record: parse_tree({{q: \"R\", act: {{leaf: 1}}, ignore: {{leaf: 2}}}}).q, none: is_fail(parse_tree(\"没有 JSON\"))}}"
    );
    let v = 值(&跑(&src, Ports::new()).unwrap());
    assert_eq!(v["fenced"], json!("Q"));
    assert_eq!(v["record"], json!("R"));
    assert_eq!(v["none"], json!(true));
}

/// 冒烟 3 真机生成器的原始输出（smoke3 账本 gen 输出原文）
const 冒烟3: &str = include_str!("compose_tree_smoke3.txt");

/// (n4) 冒烟 3 的真机题树经 parse_tree + validate_tree_with 规范后，walk 走到叶；dropped 里有根节点的 a、b 与全部
/// 自由写的 need（示例词表里一个都没有），成员名按表规范成列表后 grow 照常并入 c
#[test]
fn n4_冒烟3_真机题树跑通() {
    let calls = RefCell::new(vec![]);
    let t = 表(&[]);
    let 输出: &'static [&'static str] = Box::leak(Box::new([冒烟3]));
    let ports = Ports::new().with(判断端口(&calls, &t)).with(生成端口(输出));
    let src = format!(
        "{头}let ms = {{c: \"丙\", d: \"丁\"}};
let g = gen(\"出一棵题树\", [], 1, 0);
let v = validate_tree_with(parse_tree(content(g[0])), 6, {{need: [\"c 的配送覆盖范围\", \"d 的采购规模\"], members: [\"c\", \"d\"]}});
let r = walk(v.tree, mat({{a: \"甲\", b: \"乙\"}}), 6, {{line: line, grow: fn(m, node) {{ if has(node, \"add\") {{ add_members(m, node.add, ms) }} else {{ m }} }}}});
{{look: look(r), dropped: v.dropped, item: map(r.value, fn(e) {{ content(e.item) }})}}"
    );
    let o = 跑(&src, ports).unwrap();
    let v = 值(&o);
    assert_eq!(v["look"]["reason"], json!("leaf"));
    let dropped = v["dropped"].as_array().unwrap();
    assert!(dropped.contains(&json!({"at": "根", "field": "add", "value": "a"})));
    assert!(dropped.contains(&json!({"at": "根", "field": "add", "value": "b"})));
    let needs = dropped.iter().filter(|d| d["field"] == "need").count();
    assert_eq!(needs, 21, "真机题树里 21 条 need 全是自由写的类别");
    assert_eq!(
        v["item"][0]["c"],
        json!("丙"),
        "全部读 0.9 走 act 支，c 按 add 并入：{v}"
    );
}
