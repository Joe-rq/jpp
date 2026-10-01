//! Z0593：段末结算按判断账本键合并违规（预注册 `地基/过程记录/工程-Z0593-违规按判断键合并.md` §二 M-1 至 M-5）。
//!
//! 依据：B162（同一判断的责任只计一次）；裁定五十七；主控板 Z0593；复核 `复核-Z0556-与G2调和.md` 第 3 节 (a)。

use std::cell::Cell;

use jpp::effects::{CalibStore, FnPort, JudgeResult, Ports};
use jpp::ledger::{Entry, Ledger};
use jpp::value::Answer;
use jpp::{ActionRegistry, Outcome, lower, run, run_replay, syntax::parse};

/// 是非题恒 0.5：落在上岗线 0.75/0.25 的带内 → `Unsure(band)`
fn 带内端口(calls: &Cell<u64>) -> Ports<'_> {
    Ports::new().with(FnPort::judge("fixed-0", move |_s, qs| {
        calls.set(calls.get() + 1);
        Ok(JudgeResult {
            answers: qs.iter().map(|_| Answer::Noul(0.5)).collect(),
            tokens: 0,
            cost: 0.0,
            mode_share: qs.iter().map(|_| None).collect(),
            perms: qs.iter().map(|_| 0).collect(),
            confidence: vec![],
        })
    }))
}

fn 库() -> CalibStore {
    let mut c = CalibStore::new();
    c.put("k", 0.75, 0.25, 100, "上岗", Some(0.05)).unwrap();
    c
}

fn 程序(src: &str) -> jpp::Program {
    let mut p = lower(&parse(src).expect("解析")).expect("lower");
    p.entry.guard = true;
    p
}

fn 跑(src: &str, l: &mut Ledger) -> Outcome {
    let calls = Cell::new(0);
    run(
        &程序(src),
        带内端口(&calls),
        &库(),
        &ActionRegistry::new(),
        l,
    )
    .unwrap_or_else(|e| panic!("{}", e.render()))
}

/// 账本里每条 `Violation` 的 `also` 长度
fn 违规条(l: &Ledger) -> Vec<usize> {
    l.entries
        .iter()
        .filter_map(|e| match e {
            Entry::Violation { also, .. } => Some(also.len()),
            _ => None,
        })
        .collect()
}

/// M-5：审计重放与首跑同笔数、同 `also`，零调用，不写账本
fn 重放一致(src: &str, o: &Outcome, l: &mut Ledger) {
    let n = l.entries.len();
    let calls = Cell::new(0);
    let o2 = run_replay(
        &程序(src),
        带内端口(&calls),
        &库(),
        &ActionRegistry::new(),
        l,
    )
    .unwrap_or_else(|e| panic!("重放：{}", e.render()));
    assert_eq!(calls.get(), 0);
    assert_eq!(l.entries.len(), n, "审计重放不写");
    let j = |o: &Outcome| o.violations.iter().map(|v| v.to_json()).collect::<Vec<_>>();
    assert_eq!(j(&o2), j(o));
}

/// M-1（复核 q1 形）：同一道判断切两次，两个出口都没人接：1 笔违规，`also` 1 项（`nth` 1、2）
#[test]
fn m1_同一判断切两次只记一笔() {
    let src = r#"budget {calls: 4, cost: 0, depth: 16};
let r = judge(state(mat("甲")), test("行吗", "k"));
let a = cut(r);
let b = cut(r);
1
"#;
    let mut l = Ledger::new();
    let o = 跑(src, &mut l);
    assert_eq!(o.violations.len(), 1, "{:?}", o.violations);
    let v = &o.violations[0];
    assert_eq!(v.also.len(), 1);
    assert_eq!((v.mark.nth, v.also[0].mark.nth), (1, 2));
    assert_eq!(v.mark.key, v.also[0].mark.key, "同一判断");
    assert!(v.message.contains("另有 1 处"), "{}", v.message);
    assert_eq!(v.to_json()["also"].as_array().map(|a| a.len()), Some(1));
    // 附录一：判断账本键进报告，主行与 also 项同一个
    assert!(v.judge_key.is_some());
    assert_eq!(
        v.to_json()["judge_key"],
        v.to_json()["also"][0]["judge_key"]
    );
    assert_eq!(违规条(&l), [1]);
    重放一致(src, &o, &mut l);
}

/// M-2（复核 s1 形）：`map` 的闭包里切两次，外面再切一次：1 笔违规，`also` 2 项
#[test]
fn m2_闭包里与外面同一判断只记一笔() {
    let src = r#"budget {calls: 4, cost: 0, depth: 16};
let r = judge(state(mat("甲")), test("行吗", "k"));
let r2 = r;
let xs = map([1, 2], fn(i) { let b = cut(r2); 1 });
let a = cut(r);
1
"#;
    let mut l = Ledger::new();
    let o = 跑(src, &mut l);
    assert_eq!(o.violations.len(), 1, "{:?}", o.violations);
    assert_eq!(o.violations[0].also.len(), 2, "{:?}", o.violations[0].also);
    assert_eq!(违规条(&l), [2]);
    重放一致(src, &o, &mut l);
}

/// M-3：同一函数、同样实参调两次：同键、同记号、同站点，1 笔违规，`also` 为空，报告与改前同形
#[test]
fn m3_同记号同站点不重复列() {
    let src = r#"budget {calls: 4, cost: 0, depth: 16};
fn 判(t) { let e = cut(judge(state(mat(t)), test("行吗", "k"))); 1 }
let a = 判("甲");
let b = 判("甲");
1
"#;
    let mut l = Ledger::new();
    let o = 跑(src, &mut l);
    assert_eq!(o.violations.len(), 1, "{:?}", o.violations);
    assert!(o.violations[0].also.is_empty());
    assert!(o.violations[0].to_json().get("also").is_none());
    assert!(!o.violations[0].message.contains("另有"));
    assert_eq!(违规条(&l), [0]);
}

/// M-4：不同判断（不同材料）照旧各记一笔
#[test]
fn m4_不同判断各记一笔() {
    let src = r#"budget {calls: 4, cost: 0, depth: 16};
let a = cut(judge(state(mat("甲")), test("行吗", "k")));
let b = cut(judge(state(mat("乙")), test("行吗", "k")));
1
"#;
    let mut l = Ledger::new();
    let o = 跑(src, &mut l);
    assert_eq!(o.violations.len(), 2);
    assert!(o.violations.iter().all(|v| v.also.is_empty()));
}

/// 附录一 N-1（复核 e7、e8）：甲、乙两个判断各有一个视图没人接，另有吸收了二者的合成出口没人接。两种书写顺序都是
/// 2 笔，`judge_key` 分别是甲、乙的键，每笔列两处视图（自己的别名出口、合成出口）
#[test]
fn n1_合成出口不把两个判断并成一笔() {
    for 顺序 in [
        "let ax = cut(x);\nlet ay = cut(y);\nlet c = compose([cut(x), cut(y)], \"all\");\n",
        "let c = compose([cut(x), cut(y)], \"all\");\nlet ax = cut(x);\nlet ay = cut(y);\n",
    ] {
        let src = format!(
            "budget {{calls: 4, cost: 0, depth: 16}};\nlet x = judge(state(mat(\"甲\")), test(\"行吗\", \"k\"));\nlet y = judge(state(mat(\"乙\")), test(\"行吗\", \"k\"));\n{顺序}1\n"
        );
        let mut l = Ledger::new();
        let o = 跑(&src, &mut l);
        assert_eq!(o.violations.len(), 2, "{顺序}{:?}", o.violations);
        let 键: Vec<String> = o
            .violations
            .iter()
            .map(|v| v.judge_key.clone().expect("有判断键"))
            .collect();
        assert_ne!(键[0], 键[1], "两个判断各一笔");
        for v in &o.violations {
            // 每笔两处视图：自己的别名出口与合成出口（合成的分量已被吸收，不另算）；先到的是主记号
            assert_eq!(v.also.len(), 1, "{:?}", v.also);
            let j = v.to_json();
            assert_eq!(j["judge_key"], j["also"][0]["judge_key"]);
        }
        重放一致(&src, &o, &mut l);
    }
}
