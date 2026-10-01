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
