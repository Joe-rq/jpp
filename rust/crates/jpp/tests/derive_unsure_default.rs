//! Z0514（N8）：chain 里派生不出的未决先走 J-05 默认链（内置 `unsure_default`，与 handle 缺 unsure 臂同一份实现）。
//! 再判成已决的直接成叶；放弃的不进 pending、记 detail.dropped；转交的留在 pending、账本只一条 Handoff；
//! `--guard` 下照旧转交。默认链的选路由「为什么拿不准」定，固定关伴随题（伴随题开着时它的中性读数先判两可、放弃）。预注册：`地基/过程记录/工程-Z0514-链内未决接默认链.md`。

mod derive_support;
use derive_support::*;
use jpp::ledger::{Entry, Ledger};
use jpp::value::{Answer, Question, State};
use jpp::{lower, syntax::parse};
use serde_json::{Value as Json, json};

fn 程序(前: &str) -> String {
    format!(
        r#"budget {{calls: 20, cost: 0, depth: 512}};
{前}
let first = fn(item) {{ {{q: test("这份提案可行吗？", "t-root"), line: {{declare: {{hi: 0.7, lo: 0.3}}}}}} }};
let r = chain([{{on: mat("一份提案")}}], first, 3, {{}});
{{value: map(r.value, fn(v) {{ {{exit: exit_kind(v.exit), ud: has(v, "unsure_default"),
                               path: map(v.path, fn(p) {{ p.by + ":" + exit_kind(p.exit) }})}} }}),
  pending: map(r.pending, fn(p) {{ p.cause }}), dropped: r.detail.dropped, evidence: len(r.evidence)}}
"#
    )
}

/// 「为什么拿不准」那道 K 选一选第 `k` 项（候选类别加「两可」「题不清」）
fn 选(k: usize, s: &State) -> Answer {
    let n = s.over.len();
    let mut v = vec![0.1 / (n - 1) as f64; n];
    v[k] = 0.9;
    Answer::Choice(v)
}

fn 计(l: &Ledger, f: impl Fn(&Entry) -> bool) -> usize {
    l.entries.iter().filter(|e| f(e)).count()
}

#[test]
fn 取来再判成已决_直接成叶() {
    let src = 程序(
        r#"unsure_source({need: ["参照"], fetch: fn(q, need, m) { mat("去年同类提案都按期落地") }});"#,
    );
    // 补进 ctx 的材料到了就判得出（0.9 → act），没有时落在线之间（band）
    let r = 跑_关(
        &src,
        |_t: &str, _q: &Question, s: &State| {
            if s.ctx.is_empty() {
                Answer::Noul(0.5)
            } else {
                Answer::Noul(0.9)
            }
        },
        vec![],
    )
    .unwrap_or_else(|e| panic!("{e}"));
    let v: Json = r.out.value_json();
    assert_eq!(
        v["value"],
        json!([{"exit": "act", "ud": true, "path": ["author:act"]}]),
        "{v}"
    );
    assert_eq!(v["pending"], json!([]), "{v}");
    assert_eq!(v["dropped"], json!([]), "{v}");
    assert_eq!(v["evidence"], json!(2), "原题的键与再判的键：{v}");
    assert_eq!(r.out.unsure_default.len(), 1, "{:?}", r.out.unsure_default);
    assert_eq!(r.out.unsure_default[0]["end"], json!("decided"));
    assert_eq!(计(&r.ledger, |e| matches!(e, Entry::Enrich { .. })), 1);
    assert!(计(&r.ledger, |e| matches!(e, Entry::Refine { .. })) >= 1);
    assert_eq!(
        计(&r.ledger, |e| matches!(
            e,
            Entry::Handoff { .. } | Entry::Drop { .. }
        )),
        0
    );
}

#[test]
fn 默认链放弃_不进pending_记dropped() {
    // 「为什么拿不准」选「两可」（通用表三类之后的第 4 项）：末端记账放弃
    let r = 跑_关(
        &程序(""),
        |t: &str, _q: &Question, s: &State| {
            if t.contains("为什么拿不准") {
                选(3, s)
            } else {
                Answer::Noul(0.5)
            }
        },
        vec![],
    )
    .unwrap_or_else(|e| panic!("{e}"));
    let v: Json = r.out.value_json();
    assert_eq!(v["pending"], json!([]), "{v}");
    assert_eq!(
        v["dropped"],
        json!([{"id": "x0", "q": "这份提案可行吗？", "exit": "unsure(band)"}]),
        "{v}"
    );
    assert_eq!(v["value"], json!([]), "{v}");
    assert_eq!(r.out.unsure_default[0]["end"], json!("drop"));
    assert_eq!(计(&r.ledger, |e| matches!(e, Entry::Drop { .. })), 1);
    assert_eq!(
        计(&r.ledger, |e| matches!(e, Entry::Handoff { .. })),
        0,
        "放弃的不再转交"
    );
}

#[test]
fn 默认链转交_留在pending_只记一次handoff() {
    // 没有取法：「为什么拿不准」选「材料」后路 C 转交
    let r = 跑_关(
        &程序(""),
        |t: &str, _q: &Question, s: &State| {
            if t.contains("为什么拿不准") {
                选(0, s)
            } else {
                Answer::Noul(0.5)
            }
        },
        vec![],
    )
    .unwrap_or_else(|e| panic!("{e}"));
    let v: Json = r.out.value_json();
    assert_eq!(v["pending"], json!(["band"]), "{v}");
    assert_eq!(v["dropped"], json!([]), "{v}");
    assert_eq!(r.out.unsure_default[0]["end"], json!("handoff"));
    assert_eq!(r.out.unsure_default[0]["needed"], json!(["材料"]));
    assert_eq!(
        计(&r.ledger, |e| matches!(e, Entry::Handoff { .. })),
        1,
        "程序结束不补第二条"
    );
    assert_eq!(r.calls, 2);
}

#[test]
fn 开guard_照旧转交() {
    let r = 跑_把关(
        &程序(""),
        |t: &str, _q: &Question, s: &State| {
            if t.contains("为什么拿不准") {
                选(0, s)
            } else {
                Answer::Noul(0.5)
            }
        },
        vec![],
    )
    .unwrap_or_else(|e| panic!("{e}"));
    let v: Json = r.out.value_json();
    assert_eq!(v["pending"], json!(["band"]), "{v}");
    assert!(
        r.out.unsure_default.is_empty(),
        "{:?}",
        r.out.unsure_default
    );
    assert_eq!(r.calls, 1);
}

/// 内置本身：已决出口与不可补的原因（fail）原样不动（end: none）；交给它算作者给了去向，检查器不报 J-05、cut 不标默认链站点
#[test]
fn 内置_none与检查器() {
    let src = r#"budget {calls: 4, cost: 0, depth: 64};
let e = cut(judge(state(mat("甲")), test("甲方合适吗？", "k")), {declare: {hi: 0.7, lo: 0.3}});
let d = unsure_default(e);
let u = unsure("fail");
{end: d.end, kind: exit_kind(d.exit), u: unsure_default(u).end}
"#;
    let p = lower(&parse(src).expect("解析")).expect("lower");
    assert!(
        p.unsure_default_sites.is_empty(),
        "{:?}",
        p.unsure_default_sites
    );
    let rep = jpp::check(&p);
    assert!(
        rep.diagnostics.iter().all(|d| d.rule != "J-05"),
        "{:?}",
        rep.diagnostics
    );
    let r = 跑(src, |_t, _q, _s| Answer::Noul(0.9), vec![]).unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(
        r.out.value_json(),
        json!({"end": "none", "kind": "act", "u": "none"})
    );
}

/// 说明性断言（Jpp 2026-10-02）：`unsure_default(e, {end: "top"})` 的现状——末端按最大项行动，回 `end: "top"` 与最大项
/// 下标。**这个行为不在契约内，只是锁住现状、不作承诺**：`{end: "top"}` 只供 lib 的过程入口（`lib/derive/drive.jpp`）用，
/// INTERFACE.md 不写。B0630 起只认 lib 里的调用点（Z0943）：用户程序里调报 `E-rt-lib-only`，同一段放在 lib 目录里照常
#[test]
fn 不在契约内_锁住现状_end_top_只认lib调用点() {
    let src = r#"budget {calls: 8, cost: 0, depth: 128};
let e = cut(judge(state(mat("甲"), {over: [mat("a"), mat("b")]}), select("哪个", "k")));
let d = unsure_default(e, {end: "top"});
{kind0: exit_kind(e), end: d.end, top: d.top, top_of: d.top_of}
"#;
    let 读 = |t: &str, _q: &Question, _s: &State| {
        if t == "哪个" {
            Answer::Choice(vec![0.5, 0.5])
        } else {
            Answer::Noul(0.5)
        }
    };
    let e = 跑(src, 读, vec![]).err().expect("用户调用点不认");
    assert!(e.contains("E-rt-lib-only"), "{e}");
    let r = 跑_库(src, 读, vec![]).unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(
        r.out.value_json(),
        json!({"kind0": "unsure(tie)", "end": "top", "top": 0, "top_of": 2})
    );
}

/// 说明性断言（Z0913 复核，照 end: top 那条）：`unsure_default(e, {end: "top", tree: {…}})` 的现状（B0630 起只认 lib 调用点，
/// 用户程序里调报 `E-rt-lib-only`）——
/// 题式 lacks 里写了「中间判断」时，默认链能走到它（为什么拿不准选中它）、问子题、按回答缩小候选再问，选中项按值映回原候选（orig）。**这个行为不在契约内，只是锁住现状、不作承诺**：
/// tree 规格只供 lib 的过程入口（`lib/derive/drive.jpp`）用，INTERFACE.md 不写这个参数；B0630 之后只认 lib 里的调用点
/// （Z0939/Z0943）
#[test]
fn 不在契约内_锁住现状_tree_只认lib调用点() {
    let src = r#"budget {calls: 16, cost: 0, depth: 256};
let st = state(mat("甲"), {over: [mat("a"), mat("b"), mat("c")]});
let e = cut(judge(st, fill(form("select", "哪个", {calib: "k", lacks: ["材料", "中间判断"]}), {})));
let tree = {s: select("先判哪件事", "ks"), s_over: ["事一", "事二"],
            narrow: form("test", "做「{动作}」是在做「{要紧的事}」吗？", {calib: "kn"}), ctx_tpl: "{q}：{s}", mode: "narrow"};
let d = unsure_default(e, {end: "top", tree: tree});
{kind0: exit_kind(e), end: d.end, kind: exit_kind(d.exit), orig: if has(d, "orig") { d.orig } else { -1 }}
"#;
    // 固定关伴随题：伴随题的中性读数会先判两可停下，走不到「为什么」
    let 读 = |t: &str, _q: &Question, s: &State| {
        if t == "哪个" {
            // 首问三项并列；缩小到 [a, c] 后再问选 c
            if s.over.len() == 3 {
                Answer::Choice(vec![0.4, 0.4, 0.2])
            } else {
                Answer::Choice(vec![0.1, 0.9])
            }
        } else if t.contains("为什么拿不准") {
            let k = s
                .over
                .iter()
                .position(|m| m.content.as_str() == Some("中间判断"))
                .unwrap_or(0);
            let mut v = vec![0.0; s.over.len()];
            v[k] = 1.0;
            Answer::Choice(v)
        } else if t == "先判哪件事" {
            Answer::Choice(vec![0.9, 0.1])
        } else if t.contains("是在做") {
            Answer::Noul(if t.contains("「b」") { 0.0 } else { 0.9 })
        } else {
            Answer::Noul(0.5)
        }
    };
    let e = 跑_关(src, 读, vec![]).err().expect("用户调用点不认");
    assert!(e.contains("E-rt-lib-only"), "{e}");
    let r = 跑_关_库(src, 读, vec![]).unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(
        r.out.value_json(),
        json!({"kind0": "unsure(tie)", "end": "decided", "kind": "pick(1)", "orig": 2})
    );
}

/// 说明性断言（第三靶子第四圈，照 end: top、tree 那两条）：库内部的 `unsure_fetch(题, 类别, 材料)` 的现状（B0630 起只认 lib
/// 调用点，用户程序里调报 `E-rt-lib-only`）——
/// 只调程序经 `unsure_source` 注册的 `fetch`：没注册时回 fail；取到材料照原样回；取到别的值包成 `{类别: 值}` 的材料；
/// fetch 回 fail 照回 fail。**这个行为不在契约内，只是锁住现状、不作承诺**：`unsure_fetch` 只供 lib 的过程入口
/// （`lib/derive/drive.jpp` 的完成条件派生）用，INTERFACE.md 不写；B0630 起只认 lib 里的调用点（Z0947）
#[test]
fn 不在契约内_锁住现状_unsure_fetch_只认lib调用点() {
    let src = r#"budget {calls: 4, cost: 0, depth: 128};
let a = unsure_fetch("题", "语境", mat("甲"));
unsure_source({fetch: fn(q, need, m) { if need == "语境" { mat("规则原文") } else if need == "参照" { "一段文字" } else { fail("没有") } }});
let b = unsure_fetch("题", "语境", mat("甲"));
let c = unsure_fetch("题", "参照", mat("甲"));
let d = unsure_fetch("题", "材料", mat("甲"));
{a: is_fail(a), b: content(b), c: content(c), d: is_fail(d)}
"#;
    let e = 跑_关(src, |_t, _q, _s| Answer::Noul(0.5), vec![])
        .err()
        .expect("用户调用点不认");
    assert!(e.contains("E-rt-lib-only"), "{e}");
    let r = 跑_关_库(src, |_t, _q, _s| Answer::Noul(0.5), vec![]).unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(
        r.out.value_json(),
        json!({"a": true, "b": "规则原文", "c": {"参照": "一段文字"}, "d": true})
    );
}

/// 说明性断言（第四圈 4.e，照上一条）：tree 规格带 `fallback: "ctx"` 时，缩小后为空（三个候选都带外否）不落到末端，
/// 带 S 的回答在原集上再问、已决，选中项就是原集下标；不带 fallback 时同样的读数落到末端按最大项。
/// **不在契约内，只是锁住现状**：fallback 只供 lib 的过程入口的对照臂用，INTERFACE.md 不写；B0630 起只认 lib 调用点
#[test]
fn 不在契约内_锁住现状_tree_fallback_只认lib调用点() {
    let 源 = |fb: &str| {
        format!(
            r#"budget {{calls: 16, cost: 0, depth: 256}};
let st = state(mat("甲"), {{over: [mat("a"), mat("b"), mat("c")]}});
let e = cut(judge(st, fill(form("select", "哪个", {{calib: "k", lacks: ["材料", "中间判断"]}}), {{}})));
let tree = {{s: select("先判哪件事", "ks"), s_over: ["事一", "事二"],
            narrow: form("test", "做「{{动作}}」是在做「{{要紧的事}}」吗？", {{calib: "kn"}}), ctx_tpl: "{{q}}：{{s}}", mode: "narrow"{fb}}};
let d = unsure_default(e, {{end: "top", tree: tree}});
{{end: d.end, kind: exit_kind(d.exit)}}
"#
        )
    };
    let 读 = |t: &str, _q: &Question, s: &State| {
        if t == "哪个" {
            // 首问三项并列；带了子题回答（ctx 非空）再问选 c
            if s.ctx.is_empty() {
                Answer::Choice(vec![0.4, 0.4, 0.2])
            } else {
                Answer::Choice(vec![0.1, 0.1, 0.8])
            }
        } else if t.contains("为什么拿不准") {
            let k = s
                .over
                .iter()
                .position(|m| m.content.as_str() == Some("中间判断"))
                .unwrap_or(0);
            let mut v = vec![0.0; s.over.len()];
            v[k] = 1.0;
            Answer::Choice(v)
        } else if t == "先判哪件事" {
            Answer::Choice(vec![0.9, 0.1])
        } else if t.contains("是在做") {
            Answer::Noul(0.0)
        } else {
            Answer::Noul(0.5)
        }
    };
    let e = 跑_关(&源(", fallback: \"ctx\""), 读, vec![])
        .err()
        .expect("用户调用点不认");
    assert!(e.contains("E-rt-lib-only"), "{e}");
    let ctx = 跑_关_库(&源(", fallback: \"ctx\""), 读, vec![]).unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(
        ctx.out.value_json(),
        json!({"end": "decided", "kind": "pick(2)"})
    );
    let end = 跑_关_库(&源(""), 读, vec![]).unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(
        end.out.value_json(),
        json!({"end": "top", "kind": "unsure(tie)"})
    );
}
