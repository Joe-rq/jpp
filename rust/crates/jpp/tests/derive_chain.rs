//! 步 28（B0469）：连环题 `chain`——每跳由上一跳出口派生，不写 S、不事先生成题树。闭包端口，不发请求。
//!
//! 场景与预测见 `地基/过程记录/工程-步28.md` §8.3（示例一）：意图汇编 7b 的原题起步，
//! 是非题 act 块挂签名 → 填「方面」→ 选中「资金」按它的签名填「领域」→ 填好的程度题（声明线）→
//! x1 读数在带里走划分细化、x2 缺 `ref` 走前提反面。

mod common;
use jpp::effects::{CalibStore, EffectError, FnPort, JudgeResult, Ports};
use jpp::ledger::{Entry, Ledger};
use jpp::value::Answer;
use jpp::{ActionRegistry, EntryArgs, Outcome, Session};
use serde_json::{Value as Json, json};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT: AtomicU64 = AtomicU64::new(0);

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn 读数(text: &str, scale: bool) -> Answer {
    if scale {
        return Answer::Score(vec![0.05, 0.1, 0.2, 0.4, 0.25]);
    }
    let p = |x: f64| Answer::Noul(x);
    // 派生题面会引用原题，所以先认派生的句式，最后才认原题
    if text.contains("最可能是哪一项") {
        Answer::Choice(vec![0.7, 0.2, 0.1])
    } else if text.contains("填哪一项") {
        Answer::Choice(vec![0.6, 0.3, 0.1])
    } else if text.contains("至少达到「略变小」") {
        p(0.9)
    } else if text.contains("至少达到「基本不变」") {
        p(0.8)
    } else if text.contains("至少达到「略变大」") {
        p(0.7)
    } else if text.contains("至少达到「明显变大」") {
        p(0.3)
    } else if text.contains("已经在合作") {
        p(0.2)
    } else if text.contains("需要分别回答的判断") {
        p(0.1)
    } else if text.contains("这段材料里有没有") {
        p(0.9)
    } else if text.contains("更紧密吗") {
        p(0.8)
    } else {
        panic!("夹具没有这道题：{text}")
    }
}

fn 跑(src: &str) -> (Outcome, Ledger, usize) {
    let dir = root().join(format!(
        "target/derive-chain-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("p.jpp");
    std::fs::write(&path, src).unwrap();
    let loaded = jpp::syntax::loader::load(&path);
    let _ = std::fs::remove_dir_all(&dir);
    let program =
        jpp::lower(&loaded.unwrap_or_else(|e| panic!("装载：{e:?}")).program).expect("lower");
    let calib = CalibStore::new();
    let acts = ActionRegistry::new();
    let calls = std::cell::RefCell::new(0usize);
    let ports = Ports::new()
        .with(common::伴随中性judge("fixed-0", |_s, qs| {
            *calls.borrow_mut() += 1;
            Ok::<_, EffectError>(JudgeResult {
                answers: qs
                    .iter()
                    .map(|q| 读数(&q.text, !q.scale.is_empty()))
                    .collect(),
                tokens: 0,
                cost: 0.0,
                mode_share: vec![],
                perms: vec![],
                confidence: vec![],
            })
        }))
        .with(FnPort::generate("fixed-0", |_p, _c, _n, _r| {
            Err(EffectError("本测试不唤出".into()))
        }))
        .with(FnPort::ask("fixed-0", |_s, _q| {
            Err(EffectError("不该 ask".into()))
        }));
    let mut ledger = Ledger::new();
    let out = Session::new(ports, &calib, &acts)
        .with_companions(jpp::interp::CompanionMode::Off) // 数调用与跳数，整文件固定关伴随题（主控 2026-09-30：第二类）
        .run(&program, &EntryArgs::default(), &mut ledger)
        .unwrap_or_else(|e| panic!("程序应当跑完：{}", e.render()));
    let n = *calls.borrow();
    (out, ledger, n)
}

const 程序: &str = r#"import "../../lib/derive/chain.jpp";
budget {calls: 40, cost: 0, depth: 512};
let money = form("measure", "c 加入后，a 与 b 在{领域}上的合作金额会怎样变化？",
                 {calib: "chain-money", scale: ["明显变小", "略变小", "基本不变", "略变大", "明显变大"],
                  evidence: ["ref"], presupposition: "a 与 b 已经在合作"});
let money_sig = {form: money, params: [{slot: "领域", choices: ["硬件", "软件", "渠道"]}], line: {declare: {hi: 0.6}}};
let aspect_sig = {params: [{slot: "变化最大的方面", choices: [{name: "资金", sig: money_sig}, "分工", "进度"]}]};
let root = form("test", "c 的加入会让 a 与 b 的合作更紧密吗？", {calib: "chain-root", on: {act: aspect_sig}});
let first = fn(item) { {form: root} };
let items = [{on: mat("a 做硬件，b 做渠道；c 做软件，打算加入。"), ref: mat("a 与 b 合作三年，去年合作金额 200 万。")},
             {on: mat("a 做硬件，b 做渠道；c 做软件，想要加入。")}];
let r = chain(items, first, 6, {});
{value: map(r.value, fn(v) { {by: v.by, refined: if has(v, "refined") { v.refined } else { unit },
                              conclusion: if has(v, "conclusion") { v.conclusion } else { unit },
                              path: map(v.path, fn(p) { p.by + ":" + exit_kind(p.exit) })} }),
 pending: len(r.pending), hops: r.detail.hops, per_hop: r.detail.per_hop,
 rejected: len(r.detail.rejected), unasked: len(r.detail.unasked), version: r.detail.version}
"#;

fn 判断条目(l: &Ledger) -> Vec<(String, Vec<String>, u32)> {
    l.entries
        .iter()
        .filter_map(|e| match e {
            Entry::Judge {
                key, parents, hop, ..
            } => Some((key.clone(), parents.clone(), *hop)),
            _ => None,
        })
        .collect()
}

#[test]
fn a_五跳连环_每跳由上一跳出口派生() {
    let (out, ledger, calls) = 跑(程序);
    let v: Json = out.value_json();
    eprintln!("{}", serde_json::to_string_pretty(&v).unwrap());
    // 预注册 §10.2：调用 29（诊断每候选两道，各在自己的状态上）、跳数 5、候选 11 全过、pending 0
    assert_eq!(calls, 29, "调用数");
    assert_eq!(v["hops"], json!(5));
    assert_eq!(v["pending"], json!(0));
    assert_eq!(v["rejected"], json!(0));
    let js = 判断条目(&ledger);
    assert_eq!(
        js.len(),
        32,
        "判断条目：作者题 2 +「藏两判」8（同一题面只问一次）+「在不在」11 + 派生 11"
    );
    let max_hop = js.iter().map(|j| j.2).max().unwrap();
    assert_eq!(max_hop, 5, "最深的派生题在第 5 跳");
    // 闸门第①段每个候选一条 transform 条目（裁定二十）
    let effects = ledger
        .entries
        .iter()
        .filter(|e| matches!(e, Entry::Effect { .. }))
        .count();
    assert_eq!(effects, 11, "闸门 transform 条目");
    // 结论：x1 细化到「略变大」，原出口仍是 band；x2 前提不成立
    let vals = v["value"].as_array().unwrap();
    assert!(
        vals.iter().any(|x| x["refined"] == json!("略变大")
            && x["path"].as_array().unwrap().last().unwrap() == &json!("fill:unsure(band)")),
        "{v}"
    );
    assert!(
        vals.iter().any(|x| x["conclusion"] == json!("前提不成立")),
        "{v}"
    );
}

#[test]
fn b_账本沿_parents_从第五跳回溯到第一跳() {
    let (_out, ledger, _) = 跑(程序);
    let js = 判断条目(&ledger);
    let by_key: std::collections::HashMap<&str, &(String, Vec<String>, u32)> =
        js.iter().map(|j| (j.0.as_str(), j)).collect();
    // 谱系不串条目（回归：用 range(0, len(整批)) 的下标按位置配对时，B84 把整批的来源并进取出的值，
    // 一个条目的题挂上了另一个条目的来源出口）。派生题面引用来源题面，祖先会逐跳累积（值依赖，合法），
    // 但同一条链每跳只有一个出口：父条目里同一跳出现两个，就是串了条目
    let hop_of: std::collections::HashMap<&str, u32> =
        js.iter().map(|j| (j.0.as_str(), j.2)).collect();
    for j in &js {
        let mut hs: Vec<u32> =
            j.1.iter()
                .filter_map(|p| hop_of.get(p.as_str()).copied())
                .collect();
        let n = hs.len();
        hs.sort();
        hs.dedup();
        assert_eq!(
            hs.len(),
            n,
            "条目 {} 的父条目里同一跳出现不止一个（串了条目）：{:?}",
            j.0,
            j.1
        );
    }
    // 更直接的判定：沿全部父条目（传递）走到第一跳，每条派生出的判断条目只落到一道作者题上——
    // 恰好同跳的串条目上一条抓得到，跨跳串进别的条目的链只有这一条抓得到
    fn 第一跳祖先<'a>(
        k: &'a str,
        by_key: &std::collections::HashMap<&'a str, &'a (String, Vec<String>, u32)>,
        out: &mut std::collections::BTreeSet<&'a str>,
    ) {
        let j = by_key[k];
        if j.2 == 1 {
            out.insert(j.0.as_str());
            return;
        }
        for p in &j.1 {
            if by_key.contains_key(p.as_str()) {
                第一跳祖先(p.as_str(), by_key, out);
            }
        }
    }
    for j in js.iter().filter(|j| j.2 >= 2) {
        let mut roots = std::collections::BTreeSet::new();
        第一跳祖先(j.0.as_str(), &by_key, &mut roots);
        assert_eq!(
            roots.len(),
            1,
            "条目 {}（hop {}）的第一跳祖先不止一道作者题（串了条目）：{roots:?}",
            j.0,
            j.2
        );
    }
    for start in js.iter().filter(|j| j.2 == 5) {
        let mut cur = start;
        let mut len = 1;
        // hop = 1 + 父条目的最大 hop：沿 hop 最大的父条目走，每步恰好减一
        while let Some(p) = cur
            .1
            .iter()
            .filter_map(|p| by_key.get(p.as_str()))
            .max_by_key(|j| j.2)
        {
            assert_eq!(p.2 + 1, cur.2, "每步 hop 减一");
            cur = p;
            len += 1;
        }
        assert_eq!(cur.2, 1, "回溯到第一跳");
        assert_eq!(len, 5, "链长 5");
    }
}
