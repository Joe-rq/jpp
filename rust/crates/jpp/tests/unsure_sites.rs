//! B0492 S4：检查器按数据流标注。handle 缺 unsure 臂的值流进 if 条件、效应实参、块结果时各一条提示；
//! 「无作者去向」的 cut 与契约值站点写进 IR（`Program.unsure_default_sites`，供 S2c）。预注册见过程记录 5.5。

use jpp::{lower, syntax::parse};
use serde_json::Value as Json;
use std::path::Path;

fn 编(src: &str) -> jpp::ir::Program {
    lower(&parse(src).expect("解析")).expect("lower")
}

fn 提示(src: &str) -> Vec<String> {
    let r = jpp::check(&编(src));
    r.diagnostics
        .iter()
        .filter(|d| d.rule == "N-unsure-default")
        .map(|d| d.message.clone())
        .collect()
}

const 前: &str = r#"budget {calls: 8, cost: 0, depth: 16};
let v = handle(cut(judge(state(mat("甲")), test("甲方合适吗？", "k"))), {act: fn() { true }, ignore: fn() { false }});
"#;

#[test]
fn 流进if条件() {
    let hs = 提示(&format!("{前}if v {{ 1 }} else {{ 2 }}\n"));
    assert!(hs.iter().any(|m| m.contains("两支都不走")), "{hs:?}");
}

#[test]
fn 流进效应实参() {
    let hs = 提示(&format!(
        "{前}cut(judge(state(mat(v)), test(\"乙呢？\", \"k\")))\n"
    ));
    assert!(hs.iter().any(|m| m.contains("效应不发出")), "{hs:?}");
}

#[test]
fn 流进块结果() {
    let hs = 提示(&format!("{前}{{r: v}}\n"));
    assert!(hs.iter().any(|m| m.contains("随块结果交出")), "{hs:?}");
}

#[test]
fn 写了unsure臂不提示流向() {
    let src = r#"budget {calls: 8, cost: 0, depth: 16};
let v = handle(cut(judge(state(mat("甲")), test("甲方合适吗？", "k"))), {act: fn() { true }, ignore: fn() { false }, unsure: fn(u) { {exit: u} }});
{r: v}
"#;
    assert!(提示(src).is_empty());
}

fn 站点(src: &str) -> Vec<usize> {
    编(src).unsure_default_sites.into_iter().collect()
}

fn 起点(src: &str, pat: &str) -> usize {
    src.find(pat).expect("找得到")
}

#[test]
fn 出口没提到或只读_标() {
    for 后 in [
        "1",
        "exit_kind(e)",
        "if exit_kind(e) == \"act\" { 1 } else { 0 }",
        "e == e",
        "e.kind",
    ] {
        let src = format!(
            "budget {{calls: 8, cost: 0, depth: 16}};\nlet e = cut(judge(state(mat(\"甲\")), test(\"q\", \"k\")));\n{后}\n"
        );
        assert_eq!(站点(&src), vec![起点(&src, "cut(")], "{后}");
    }
}

#[test]
fn 出口有去向_不标() {
    for 后 in [
        "handle(e, {act: fn() { 1 }, ignore: fn() { 0 }, unsure: fn(u) { {exit: u} }})",
        "e",
        "{exit: e}",
        "[e]",
        "consume(e, \"drop\")",
        "f(e)",
    ] {
        let src = format!(
            "budget {{calls: 8, cost: 0, depth: 16}};\nfn f(x) -> Exit {{ x }}\nlet e = cut(judge(state(mat(\"甲\")), test(\"q\", \"k\")));\n{后}\n"
        );
        assert!(站点(&src).is_empty(), "{后}：{:?}", 站点(&src));
    }
}

#[test]
fn 契约值只窄引用_标_否则不标() {
    let 标 = format!(
        "budget {{calls: 8, cost: 0, depth: 16}};\nlet o = sieve([\"甲\", \"乙\"], test(\"q\", \"k\"));\nlen(accepted(o))\n"
    );
    assert_eq!(站点(&标), vec![起点(&标, "sieve(")]);
    for 后 in ["o", "o.pending", "{a: accepted(o), p: o.pending}"] {
        let src = format!(
            "budget {{calls: 8, cost: 0, depth: 16}};\nlet o = sieve([\"甲\", \"乙\"], test(\"q\", \"k\"));\n{后}\n"
        );
        assert!(站点(&src).is_empty(), "{后}");
    }
}

/// 预注册：50 个能跑通的金样程序标 0 个站点；报错金样里 error-outcome-dropped 标 1 个。S2c 起加了证明示例
/// unsure-default（一个 handle 都不写，sieve 只取 accepted），它标 1 个
#[test]
fn 金样程序的站点数() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap();
    let m: Json =
        serde_json::from_slice(&std::fs::read(root.join("tests/golden/manifest.json")).unwrap())
            .unwrap();
    let mut 非零 = vec![];
    for c in m["cases"].as_array().unwrap() {
        let Ok(loaded) = jpp::syntax::loader::load(&root.join(c["source"].as_str().unwrap()))
        else {
            continue;
        };
        let Ok(p) = lower(&loaded.program) else {
            continue;
        };
        if !p.unsure_default_sites.is_empty() {
            非零.push((
                c["name"].as_str().unwrap().to_string(),
                p.unsure_default_sites.len(),
            ));
        }
    }
    assert_eq!(
        非零,
        vec![
            ("error-outcome-dropped".to_string(), 1),
            ("unsure-default".to_string(), 1)
        ]
    );
}

/// 同一读数切两次（B162：同一判断的两份未决是一份责任的两个视图）：一处有去向时任一个都不标，免得一处走链替另一处的
/// 持有者解除责任。两处都没有去向（结果是 `1`）时 5.18 起两处都标，见 `同一判断直接绑定的一组cut按b162分组`
#[test]
fn 同一读数切两次_不标() {
    for 后 in ["{a: a}"] {
        let src = format!(
            "budget {{calls: 8, cost: 0, depth: 16}};\nlet r = judge(state(mat(\"甲\")), test(\"q\", \"k\"));\nlet a = cut(r);\nlet b = cut(r);\n{后}\n"
        );
        assert!(站点(&src).is_empty(), "{后}：{:?}", 站点(&src));
    }
}

/// 主控复核 2026-09-30（B162 站点漏洞）：嵌套函数里的第二个 `cut(r)`、两次 `cut(rs[0])` 都不标
#[test]
fn 复核_嵌套函数与非裸名读数_不标() {
    let p2 = "budget {calls: 8, cost: 0, depth: 16};\nlet r = judge(state(mat(\"甲\")), test(\"未决吗\", \"k\"));\nlet a = cut(r);\nlet f = fn() { let b = cut(r); 1 };\nlet z = f();\n{a: a, z: z}\n";
    assert!(站点(p2).is_empty(), "p2：{:?}", 站点(p2));
    let p3 = "budget {calls: 8, cost: 0, depth: 16};\nlet rs = [judge(state(mat(\"甲\")), test(\"未决吗\", \"k\"))];\nlet a = cut(rs[0]);\nlet b = cut(rs[0]);\n{a: a}\n";
    assert!(站点(p3).is_empty(), "p3：{:?}", 站点(p3));
}

/// 程序自己的 unsure_companions 不是题式列表：检查器提示
#[test]
fn 非题式的unsure_companions_提示() {
    let src =
        "budget {calls: 8, cost: 0, depth: 16};\nlet unsure_companions = [\"不是题式\"];\n1\n";
    let r = jpp::check(&编(src));
    assert!(r.find("N-unsure-companions").is_some(), "{}", r.render());
    let ok = "budget {calls: 8, cost: 0, depth: 16};\nlet unsure_companions = [form(\"test\", \"题「{q}」清楚吗？\", {calib: \"unsure-companion-x\"})];\n1\n";
    assert!(jpp::check(&编(ok)).find("N-unsure-companions").is_none());
}

/// 复查 2026-09-30 阻断项：只在读数名是 `let <名> = judge(…)` 直接绑定、且全程序只出现这一次时才标；
/// 函数参数（p5）、`let` 别名（p6 `let r2 = r`、p7 `let r = rs[0]`）一律不标
#[test]
fn 别名与参数传来的读数_不标() {
    let j = r#"judge(state(mat("甲")), test("q", "k"))"#;
    let 头 = "budget {calls: 8, cost: 0, depth: 16};\n";
    for (名, src) in [
        (
            "p5",
            format!(
                "{头}let r0 = {j};\nlet a = cut(r0);\nlet g = fn(r) {{ let b = cut(r); 1 }};\nlet z = g(r0);\n{{a: a, z: z}}\n"
            ),
        ),
        (
            "p6",
            format!("{头}let r = {j};\nlet a = cut(r);\nlet r2 = r;\nlet b = cut(r2);\n{{a: a}}\n"),
        ),
        (
            "p7",
            format!(
                "{头}let rs = [{j}];\nlet a = cut(rs[0]);\nlet r = rs[0];\nlet b = cut(r);\n{{a: a}}\n"
            ),
        ),
        (
            "参数只用一次",
            format!("{头}let g = fn(r) {{ let b = cut(r); 1 }};\ng({j})\n"),
        ),
    ] {
        assert!(站点(&src).is_empty(), "{名}：{:?}", 站点(&src));
    }
    // 直接绑定、只出现一次：照旧标
    let src = format!("{头}let r = {j};\nlet b = cut(r);\n1\n");
    assert_eq!(站点(&src), vec![起点(&src, "cut(")]);
}

/// 复查 2026-09-30 小项 1 与 G1 步 33（裁定五十九，过程记录 5.25）：站点不标的 cut（p2 里函数中的第二个 `cut(r)`）
/// 不降成 N-unsure-default（运行期不走链）；静态看不清责任由谁接，报 W-unsure-untracked、不拒，运行期照 B162 判
/// （p2 运行期是 J-05）。标了的站点才降为 N-unsure-default
#[test]
fn 站点不标时检查器报untracked() {
    let p2 = "budget {calls: 8, cost: 0, depth: 16};\nlet r = judge(state(mat(\"甲\")), test(\"q\", \"k\"));\nlet a = cut(r);\nlet f = fn() { let b = cut(r); 1 };\nlet z = f();\n{a: a, z: z}\n";
    assert!(站点(p2).is_empty());
    let rep = jpp::check(&编(p2));
    assert!(rep.find("J-05").is_none(), "{}", rep.render());
    assert!(rep.find("W-unsure-untracked").is_some(), "{}", rep.render());
    assert!(rep.find("N-unsure-default").is_none(), "{}", rep.render());
    let 标 = "budget {calls: 8, cost: 0, depth: 16};\nlet e = cut(judge(state(mat(\"甲\")), test(\"q\", \"k\")));\nexit_kind(e)\n";
    let rep = jpp::check(&编(标));
    assert!(rep.find("J-05").is_none(), "{}", rep.render());
}

/// 二次复查 p1 与过程记录 5.18（主控 2026-09-30 路 2）：同一判断直接绑定的一组 cut。组里一处有作者去向，其余没被提到的
/// 不标站点、不报 J-05，报 `N-duty-shared`；组里都没有去向，几处都标、各报 `N-unsure-default`。p10、p12 不算同组，
/// G1 步 33 起报 `W-unsure-untracked`（原为 J-05）
#[test]
fn 同一判断直接绑定的一组cut按b162分组() {
    let 头 = "budget {calls: 8, cost: 0, depth: 16};\nlet r = judge(state(mat(\"甲\")), test(\"q\", \"k\"));\n";
    let 规则 = |src: &str| -> Vec<String> {
        jpp::check(&编(src))
            .diagnostics
            .iter()
            .map(|d| d.rule.to_string())
            .collect()
    };
    // p1：a 返回，b 没被提到
    let p1 = format!("{头}let a = cut(r);\nlet b = cut(r);\n{{a: a}}\n");
    assert!(站点(&p1).is_empty(), "{:?}", 站点(&p1));
    let rs = 规则(&p1);
    assert!(!rs.iter().any(|r| r == "J-05"), "{rs:?}");
    assert_eq!(
        rs.iter().filter(|r| *r == "N-duty-shared").count(),
        1,
        "{rs:?}"
    );
    // 块结果就是 cut(r) 也算有去向
    let 结果 = format!("{头}let b = cut(r);\ncut(r)\n");
    assert!(站点(&结果).is_empty());
    assert!(!规则(&结果).iter().any(|r| r == "J-05"));
    // 组里都没有去向：两处都标
    let 都无 = format!("{头}let a = cut(r);\nlet b = cut(r);\n1\n");
    assert_eq!(站点(&都无).len(), 2, "{:?}", 站点(&都无));
    let rs = 规则(&都无);
    assert!(!rs.iter().any(|r| r == "J-05"), "{rs:?}");
    assert_eq!(
        rs.iter().filter(|r| *r == "N-unsure-default").count(),
        2,
        "{rs:?}"
    );
    // 有一次出现不在同组位置（嵌进 if 支）：不算同组，按另有持有者，报 W-unsure-untracked
    let 未管 = |rs: &[String]| {
        !rs.iter().any(|r| r == "J-05") && rs.iter().any(|r| r == "W-unsure-untracked")
    };
    let 嵌 = format!(
        "{头}let a = cut(r);\nlet b = cut(r);\nlet c = if true {{ cut(r) }} else {{ a }};\n{{a: a}}\n"
    );
    assert!(未管(&规则(&嵌)), "{:?}", 规则(&嵌));
    // p10：读数经函数返回，绑定不是 judge(…)
    let p10 = "budget {calls: 8, cost: 0, depth: 16};\nlet mk = fn() { judge(state(mat(\"甲\")), test(\"q\", \"k\")) };\nlet a = cut(mk());\nlet r = mk();\nlet b = cut(r);\n{a: a}\n";
    assert!(未管(&规则(p10)), "{:?}", 规则(p10));
    // p12：闭包捕获 r
    let p12 = format!("{头}let g = fn() {{ r }};\nlet a = cut(g());\nlet b = cut(r);\n{{a: a}}\n");
    assert!(站点(&p12).is_empty());
    assert!(未管(&规则(&p12)), "{:?}", 规则(&p12));
}
