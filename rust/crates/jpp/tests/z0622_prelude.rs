//! Z0622（过程记录 5.32）：伴随题序言与用户程序共用从 0 起的节点号与源码偏移。序言只许常量定义（题式、列表、let）；
//! 出现效应、cut 等就在运行入口报 `E-prelude`、不求值，免得序言的判断与用户程序同偏移同材料同题的判断串键。

use jpp::effects::{CalibStore, FnPort, JudgeResult, Ports};
use jpp::interp::{ActionRegistry, Interp};
use jpp::ledger::Ledger;
use jpp::value::Answer;
use jpp::{lower, syntax::parse};
use std::cell::RefCell;

const 同前缀: &str = "budget {calls: 4, cost: 0, depth: 16};\nlet e = cut(judge(state(mat(\"甲\")), test(\"行吗\", \"k\")));\n";

fn 跑(序言: &str, 程序: &str) -> (Result<serde_json::Value, String>, usize) {
    let p = lower(&parse(序言).expect("解析序言")).expect("lower 序言");
    let prog = lower(&parse(程序).expect("解析")).expect("lower");
    let 调 = RefCell::new(0usize);
    let ports = Ports::new().with(FnPort::judge("fixed-0", |_s, qs| {
        *调.borrow_mut() += 1;
        Ok(JudgeResult {
            answers: qs.iter().map(|_| Answer::Noul(0.9)).collect(),
            tokens: 0,
            cost: 0.0,
            mode_share: vec![],
            perms: vec![],
            confidence: vec![],
        })
    }));
    let (calib, acts) = (CalibStore::new(), ActionRegistry::new());
    let mut l = Ledger::new();
    let r = Interp::new(ports, &mut l, &calib, &acts, prog.budget.clone())
        .with_prelude(p)
        .run(&prog)
        .map(|o| o.value_json())
        .map_err(|e| format!("[{}] {}", e.rule.clone().unwrap_or_default(), e.message));
    let n = *调.borrow();
    (r, n)
}

/// 自造带判断的序言，与用户程序同偏移、同材料、同题：运行入口报 E-prelude，序言的判断不求值，不会与用户判断串键
#[test]
fn 带判断的序言被拒_不串键() {
    let 序言 = format!("{同前缀}0\n");
    let 程序 = format!("{同前缀}exit_kind(e)\n");
    let (r, 调) = 跑(&序言, &程序);
    let e = r.expect_err("序言带判断应被拒");
    assert!(e.contains("E-prelude") && e.contains("cut"), "{e}");
    assert_eq!(调, 0, "序言与用户程序的判断都没有发");
}

/// 只含题式与 let 的自造序言照常可用；用户程序照常判
#[test]
fn 常量序言照常() {
    let 序言 = "budget {calls: 0, cost: 0};\nlet f = form(\"test\", \"题「{q}」说得够具体吗？\", {calib: \"unsure-companion-x\"});\nlet unsure_companions = [f];\n0\n";
    let 程序 = format!("{同前缀}exit_kind(e)\n");
    let (r, 调) = 跑(序言, &程序);
    assert_eq!(r.expect("照常"), serde_json::json!("act"));
    assert!(调 >= 1);
}

/// 标准序言 lib/unsure.jpp 通过这道核
#[test]
fn 标准序言通过核() {
    let prog = lower(&parse("budget {calls: 0, cost: 0};\n1\n").unwrap()).unwrap();
    let (calib, acts) = (CalibStore::new(), ActionRegistry::new());
    let mut l = Ledger::new();
    let r = Interp::new(Ports::new(), &mut l, &calib, &acts, prog.budget.clone())
        .with_prelude(jpp::session::unsure_prelude())
        .run(&prog);
    assert!(r.is_ok(), "{:?}", r.err().map(|e| e.message));
}

/// Z0622 补漏（过程记录 5.34）：序言调 unsure_source 会改写用户程序默认链的候选类别，报 E-prelude、不求值
#[test]
fn 序言调改全局状态的内置被拒() {
    let 序言 = "budget {calls: 0, cost: 0};\nunsure_source({need: [\"序言注入的类别\"]});\n0\n";
    let 程序 = format!("{同前缀}exit_kind(e)\n");
    let (r, 调) = 跑(序言, &程序);
    let e = r.expect_err("序言调 unsure_source 应被拒");
    assert!(
        e.contains("E-prelude") && e.contains("unsure_source"),
        "{e}"
    );
    assert_eq!(调, 0);
}

/// 起别名再调、或当实参传，同样拒（查的是名字引用，不只看调用位置）；有输出的 print 也拒
#[test]
fn 别名与输出内置也被拒() {
    for (序言, 名) in [
        (
            "budget {calls: 0, cost: 0};\nlet s = unsure_source;\ns({need: [\"x\"]});\n0\n",
            "unsure_source",
        ),
        (
            "budget {calls: 0, cost: 0};\nlet xs = map([{need: [\"x\"]}], unsure_source);\n0\n",
            "unsure_source",
        ),
        ("budget {calls: 0, cost: 0};\nprint(\"hi\");\n0\n", "print"),
    ] {
        let (r, 调) = 跑(序言, &format!("{同前缀}exit_kind(e)\n"));
        let e = r.expect_err(序言);
        assert!(e.contains("E-prelude") && e.contains(名), "{序言} → {e}");
        assert_eq!(调, 0);
    }
}

/// 只用纯数据内置（len、map、join）的自造序言照常可用
#[test]
fn 纯数据内置照常() {
    let 序言 = "budget {calls: 0, cost: 0};\nlet unsure_lacks = map([\"背景\", \"预算\"], fn(x) { join([x, \"信息\"], \"\") });\nlet n = len(unsure_lacks);\n0\n";
    let (r, 调) = 跑(序言, &format!("{同前缀}exit_kind(e)\n"));
    assert_eq!(r.expect("照常"), serde_json::json!("act"));
    assert!(调 >= 1);
}
