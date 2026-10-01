//! G1 步 33（Z0460；裁定五十九第三节；预注册见过程记录 5.25）：检查器 J-05 静态口径放宽。读数另有持有者的 `cut` 绑定后
//! 再没被提到——别名、先取下标、闭包捕获、函数参数、同组另一视图只是被提到过——不再整份拒，报 `W-unsure-untracked`，
//! 交运行期按 B162 记账（同一判断任一视图随值交出即解除；都没人接时，G2 起运行期不报 J-05 中止，而是程序结束时
//! 记违规、值照带，CLI 退出码 3）。
//! 能静态确定的照旧：读数独有的走默认链（`N-unsure-default`），同组另一视图有**确定**去向的不计责任（`N-duty-shared`），
//! `ask` 出口绑定后再没被提到仍是 J-05 错。
//!
//! 下面的「差异清单」是原来被检查器拒、现在通过的写法，逐个核检查器与运行期。

mod common;
use jpp::effects::{CalibStore, FnPort, JudgeResult, Ports};
use jpp::ledger::{Entry, Ledger};
use jpp::value::Answer;
use jpp::{ActionRegistry, lower, syntax::parse};

const 判: &str = r#"judge(state(mat("甲")), test("未决吗", "k"))"#;

fn 程序(体: &str) -> String {
    format!(
        "budget {{calls: 8, cost: 0, depth: 16}};\n{}",
        体.replace("{J}", 判)
    )
}

fn 规则(src: &str) -> Vec<String> {
    jpp::check(&lower(&parse(src).expect("解析")).expect("lower"))
        .diagnostics
        .iter()
        .map(|d| d.rule.to_string())
        .collect()
}

/// 运行期（固定关伴随题，判断一律 0.5 并列）：`Ok((去向事件种类, 违规笔数))` 或 `Err(报错)`
fn 运行(src: &str) -> Result<(Vec<&'static str>, usize), String> {
    let program = lower(&parse(src).expect("解析")).expect("lower");
    let ports = Ports::new().with(FnPort::judge("fixed-0", |_s, qs| {
        Ok(JudgeResult {
            answers: qs.iter().map(|_| Answer::Noul(0.5)).collect(),
            tokens: 0,
            cost: 0.0,
            mode_share: vec![],
            perms: vec![],
            confidence: vec![],
        })
    }));
    let mut l = Ledger::new();
    let o = common::run_关(
        &program,
        ports,
        &CalibStore::new(),
        &ActionRegistry::new(),
        &mut l,
    )
    .map_err(|e| e.render())?;
    let ev = l
        .entries
        .iter()
        .filter(|e| e.is_duty_event())
        .map(|e| match e {
            Entry::Handoff { .. } => "Handoff",
            Entry::Drop { .. } => "Drop",
            _ => "其他",
        })
        .collect();
    Ok((ev, o.violations.len()))
}

/// 运行期结果的三种：记转交、程序结束记违规（G2 起，原为运行期 J-05）
#[derive(Debug, PartialEq)]
enum 结果 {
    转交,
    违规,
}

fn 归(r: Result<(Vec<&'static str>, usize), String>, 名: &str) -> 结果 {
    match r {
        Ok((ev, 0)) if ev == ["Handoff"] => 结果::转交,
        Ok((ev, n)) if n > 0 && !ev.contains(&"Drop") => 结果::违规,
        other => panic!("{名}：意外的运行期结果 {other:?}"),
    }
}

/// 差异清单：原来检查器报 J-05 整份拒，现在只报 W-unsure-untracked；运行期结果照 B162 与 G2（步 35）的违规单次形态。
/// p2、p5 在 G1 施工时运行期是 J-05（B162 帧返回规则），G2 起函数返回不再报错、欠账挂到调用者帧，同键的 a 随返回值
/// 交出，记转交；q10 原为运行期 J-05，G2 起程序结束记违规
#[test]
fn 差异清单_由拒改为通过加告警() {
    use 结果::*;
    let 例: [(&str, &str, 结果); 9] = [
        (
            "p2 嵌套函数里第二个 cut(r)",
            "let r = {J};\nlet a = cut(r);\nlet f = fn() { let b = cut(r); 1 };\nlet z = f();\n{a: a, z: z}\n",
            转交,
        ),
        (
            "p3 两次 cut(rs[0])",
            "let rs = [{J}];\nlet a = cut(rs[0]);\nlet b = cut(rs[0]);\n{a: a}\n",
            转交,
        ),
        (
            "p5 读数作函数参数",
            "let r0 = {J};\nlet a = cut(r0);\nlet g = fn(r) { let b = cut(r); 1 };\nlet z = g(r0);\n{a: a, z: z}\n",
            转交,
        ),
        (
            "p6 let 改名",
            "let r = {J};\nlet a = cut(r);\nlet r2 = r;\nlet b = cut(r2);\n{a: a}\n",
            转交,
        ),
        (
            "p7 先取下标",
            "let rs = [{J}];\nlet a = cut(rs[0]);\nlet r = rs[0];\nlet b = cut(r);\n{a: a}\n",
            转交,
        ),
        (
            "p10 读数经函数返回",
            "let mk = fn() { {J} };\nlet a = cut(mk());\nlet r = mk();\nlet b = cut(r);\n{a: a}\n",
            转交,
        ),
        (
            "p12 闭包捕获",
            "let r = {J};\nlet g = fn() { r };\nlet a = cut(g());\nlet b = cut(r);\n{a: a}\n",
            转交,
        ),
        (
            "q10 裸 cut 不算组员",
            "let r = {J};\nlet a = cut(r);\ncut(r);\n1\n",
            违规,
        ),
        (
            "q11 组员嵌在 if 支里",
            "let r = {J};\nlet a = cut(r);\nlet c = if true { let b = cut(r); 1 } else { 0 };\n{a: a, c: c}\n",
            转交,
        ),
    ];
    for (名, 体, 期望) in 例 {
        let src = 程序(体);
        let rs = 规则(&src);
        assert!(
            !rs.iter().any(|r| r == "J-05"),
            "{名}：检查器不再报 J-05：{rs:?}"
        );
        assert!(rs.iter().any(|r| r == "W-unsure-untracked"), "{名}：{rs:?}");
        assert_eq!(归(运行(&src), 名), 期望, "{名}");
    }
}

/// Z0448 q1–q4：a 只是被提到过（放进丢掉的列表、print、不执行的分支、交给不理参数的函数），静态看不清它交没交出；
/// 同组的 b 不再按「另一视图已有去向」豁免（N-duty-shared），改报 W-unsure-untracked。运行期 a 没人接：G1 施工时是
/// J-05，G2 起程序结束记违规
#[test]
fn q1到q4_同组另一视图只被提到_报untracked() {
    for (名, 用a) in [
        ("q1 丢掉的列表", "let z = [a];\n1"),
        ("q2 print", "print(a);\n1"),
        ("q3 不执行的分支", "if false { a } else { 0 }"),
        ("q4 不理参数的函数", "let f = fn(e) { 1 };\nf(a)"),
    ] {
        let src = 程序(&format!(
            "let r = {{J}};\nlet a = cut(r);\nlet b = cut(r);\n{用a}\n"
        ));
        let rs = 规则(&src);
        assert!(!rs.iter().any(|r| r == "N-duty-shared"), "{名}：{rs:?}");
        assert!(rs.iter().any(|r| r == "W-unsure-untracked"), "{名}：{rs:?}");
        assert!(!rs.iter().any(|r| r == "J-05"), "{名}：{rs:?}");
        assert_eq!(归(运行(&src), 名), 结果::违规, "{名}");
    }
}

/// 确定的去向照旧 N-duty-shared：直接返回、在返回的记录与列表字面量里、交给 handle / consume
#[test]
fn 确定去向照旧同组豁免() {
    for (名, 用a) in [
        ("直接返回", "a"),
        ("返回的记录", "{x: a, y: 1}"),
        ("返回的列表里的记录", "[{x: a}]"),
        (
            "handle",
            "handle(a, {act: fn() { 1 }, ignore: fn() { 0 }, unsure: fn(u) { {exit: u} }})",
        ),
        ("consume", "consume(a, \"drop\");\n1"),
    ] {
        let src = 程序(&format!(
            "let r = {{J}};\nlet a = cut(r);\nlet b = cut(r);\n{用a}\n"
        ));
        let rs = 规则(&src);
        assert_eq!(
            rs.iter().filter(|r| *r == "N-duty-shared").count(),
            1,
            "{名}：{rs:?}"
        );
        assert!(
            !rs.iter().any(|r| r == "W-unsure-untracked" || r == "J-05"),
            "{名}：{rs:?}"
        );
    }
}

/// 能静态确定的照旧：ask 出口绑定后再没被提到仍是 J-05 错；读数独有的 cut 照旧走默认链
#[test]
fn 能静态确定的照旧() {
    let rs = 规则(&程序(
        "let h = ask(state(mat(\"甲\")), test(\"行吗\", \"k\"));\n1\n",
    ));
    assert!(rs.iter().any(|r| r == "J-05"), "{rs:?}");
    let rs = 规则(&程序("let e = cut({J});\n1\n"));
    assert!(
        rs.iter().any(|r| r == "N-unsure-default") && !rs.iter().any(|r| r == "W-unsure-untracked"),
        "{rs:?}"
    );
}
