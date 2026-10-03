//! C-3 命令行（预注册 P14）：`--carry-in` 读上游余额收紧本轮，`--carry-out` 写交给下一轮的余额，两轮串起来；
//! `--carry-in` 与 `--replay` 同给是用法错误（重放取账本头）。
//! 依据：主控答复第 6 条；过程记录 `地基/过程记录/工程-C3-预算传递.md` §三。

use serde_json::{Value, json};
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};

fn tmp(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("jpp-carry-cli-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&d);
    fs::create_dir_all(&d).unwrap();
    d
}

const 程序: &str = "budget {calls: 100, cost: 0, depth: 16};
fn 判(t) {
    let e = cut(judge(state(mat(t)), test(\"行吗\", \"k\")));
    {k: exit_kind(e), e: e}
}
let rs = [判(\"甲\"), 判(\"乙\"), 判(\"丙\")];
{v: map(rs, fn(r) { r.k }), pending: map(rs, fn(r) { r.e })}
";

fn 夹具() -> Value {
    let obs: Vec<Value> = ["甲", "乙", "丙"]
        .iter()
        .map(|t| json!({"on": [t], "op": "test", "text": "行吗", "calib": "k", "answer": {"Noul": 0.9}}))
        .collect();
    json!({"observations": obs})
}

fn jpp(d: &PathBuf, args: &[&str]) -> std::process::Output {
    // 数调用与花费，`run` 固定关伴随题（B0492 S5，主控 2026-09-30：第二类）
    let 关 = args.first() == Some(&"run");
    Command::new(env!("CARGO_BIN_EXE_jpp"))
        .current_dir(d)
        .args(args)
        .args(if 关 {
            &["--companions", "off"][..]
        } else {
            &[][..]
        })
        .output()
        .unwrap()
}

fn 读(p: PathBuf) -> Value {
    serde_json::from_str(&fs::read_to_string(p).unwrap()).unwrap()
}

#[test]
fn p14_命令行传余额() {
    let d = tmp("chain");
    fs::write(d.join("p.jpp"), 程序).unwrap();
    fs::write(d.join("fx.json"), 夹具().to_string()).unwrap();
    fs::write(
        d.join("in.json"),
        json!({"calls": 2, "cost": 0.0, "latency_p95": null, "escalate": 0, "hop": 0, "round": 0, "depth_cap": 256})
            .to_string(),
    )
    .unwrap();

    let o = jpp(
        &d,
        &[
            "run",
            "p.jpp",
            "--fixtures",
            "fx.json",
            "--output",
            "r1.json",
            "--ledger-out",
            "l1.json",
            "--carry-in",
            "in.json",
            "--carry-out",
            "out.json",
        ],
    );
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    let r1 = 读(d.join("r1.json"));
    assert_eq!(r1["cost"]["calls"], 2);
    assert_eq!(r1["value"]["v"], json!(["act", "act", "unsure(budget)"]));
    assert_eq!(r1["budget"]["unsent"], 1);
    assert!(
        r1["budget"].get("cause").is_none(),
        "预算耗尽的报告不带 cause"
    );
    assert_eq!(r1["carry"]["calls"], 0);
    assert_eq!(r1["carry"]["hop"], 1);
    assert_eq!(
        读(d.join("out.json")),
        r1["carry"],
        "--carry-out 与报告同一份"
    );

    // 第二轮：余额 0，可靠下界 1 次，计划期就拒；--carry-out 仍写出、与进门相同（附注 P14′）
    let o = jpp(
        &d,
        &[
            "run",
            "p.jpp",
            "--fixtures",
            "fx.json",
            "--output",
            "r2.json",
            "--carry-in",
            "out.json",
            "--carry-out",
            "out2.json",
        ],
    );
    assert!(!o.status.success(), "余额 0 应在计划期被拒");
    assert!(String::from_utf8_lossy(&o.stderr).contains("E-budget-plan"));
    assert_eq!(
        fs::read_to_string(d.join("out2.json")).unwrap(),
        fs::read_to_string(d.join("out.json")).unwrap(),
        "没开跑，余额原样交回"
    );

    // 重放取账本头，不收宿主给的余额
    let o = jpp(
        &d,
        &[
            "run",
            "p.jpp",
            "--replay",
            "l1.json",
            "--carry-in",
            "in.json",
        ],
    );
    assert!(!o.status.success(), "--carry-in 加 --replay 应是用法错误");
    assert!(
        String::from_utf8_lossy(&o.stderr).contains("--carry-in cannot be combined with --replay")
    );
    // 只凭账本重放：与首跑同值，--carry-out 与首跑逐字节相同
    let o = jpp(
        &d,
        &[
            "run",
            "p.jpp",
            "--replay",
            "l1.json",
            "--output",
            "r3.json",
            "--carry-out",
            "out3.json",
        ],
    );
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    let r3 = 读(d.join("r3.json"));
    assert_eq!(r3["value"], r1["value"]);
    assert_eq!(
        fs::read_to_string(d.join("out3.json")).unwrap(),
        fs::read_to_string(d.join("out.json")).unwrap(),
        "重放的 --carry-out 与首跑逐字节相同"
    );
    // 没有 --carry-in（也不是重放）就不能 --carry-out
    let o = jpp(
        &d,
        &[
            "run",
            "p.jpp",
            "--fixtures",
            "fx.json",
            "--carry-out",
            "x.json",
        ],
    );
    assert!(!o.status.success());
}

/// P16（命令行）：失败的一轮也写 --carry-out，按账本扣
#[test]
fn p16_失败轮也写交回余额() {
    let d = tmp("fail");
    let src = 程序.replace("depth: 16", "depth: 4").replace(
        "{v: map(rs, fn(r) { r.k }), pending: map(rs, fn(r) { r.e })}",
        "fn f(n) { if n > 0 { f(n - 1) } else { 0 } }\nlet ks = map(rs, fn(r) { r.k });\nlet z = f(10);\n{v: ks, z: z, pending: map(rs, fn(r) { r.e })}",
    );
    fs::write(d.join("p.jpp"), src).unwrap();
    fs::write(d.join("fx.json"), 夹具().to_string()).unwrap();
    fs::write(
        d.join("in.json"),
        json!({"calls": 10, "cost": 0.0, "latency_p95": null, "escalate": 0, "hop": 0, "round": 0, "depth_cap": 256})
            .to_string(),
    )
    .unwrap();
    let o = jpp(
        &d,
        &[
            "run",
            "p.jpp",
            "--fixtures",
            "fx.json",
            "--carry-in",
            "in.json",
            "--carry-out",
            "out.json",
        ],
    );
    assert!(!o.status.success(), "递归撞 J-06");
    assert!(String::from_utf8_lossy(&o.stderr).contains("J-06"));
    let c = 读(d.join("out.json"));
    assert_eq!((c["calls"].as_u64(), c["hop"].as_u64()), (Some(7), Some(1)));
}

/// 五道题、程序声明 calls 100 的版本（P19）
fn 五题(d: &std::path::Path) {
    let 题: Vec<String> = (0..5).map(|i| format!("判(\"材料{i}\")")).collect();
    let src = format!(
        "budget {{calls: 100, cost: 0, depth: 16}};\nfn 判(t) {{\n    let e = cut(judge(state(mat(t)), test(\"行吗\", \"k\")));\n    {{k: exit_kind(e), e: e}}\n}}\nlet rs = [{}];\n{{v: map(rs, fn(r) {{ r.k }}), pending: map(rs, fn(r) {{ r.e }})}}\n",
        题.join(", ")
    );
    fs::write(d.join("p5.jpp"), src).unwrap();
    let obs: Vec<Value> = (0..5)
        .map(|i| json!({"on": [format!("材料{i}")], "op": "test", "text": "行吗", "calib": "k", "answer": {"Noul": 0.9}}))
        .collect();
    fs::write(d.join("fx5.json"), json!({"observations": obs}).to_string()).unwrap();
}

fn 余额文件(d: &std::path::Path, name: &str, calls: u64) {
    fs::write(
        d.join(name),
        json!({"calls": calls, "cost": 0.0, "latency_p95": null, "escalate": 0, "hop": 0, "round": 0, "depth_cap": 256})
            .to_string(),
    )
    .unwrap();
}

/// P19（G1）：带上游余额时 `--explain` 的预算行、拒绝结论与实际运行同源（裁定三十二）
#[test]
fn p19_explain按收紧后的预算() {
    let d = tmp("explain");
    五题(&d);
    余额文件(&d, "c3.json", 3);
    余额文件(&d, "c0.json", 0);
    // JSON：explain.budget 是收紧后的 3，没有拒绝；实际发 3 次、停发 2
    let o = jpp(
        &d,
        &[
            "run",
            "p5.jpp",
            "--fixtures",
            "fx5.json",
            "--json",
            "--explain",
            "--output",
            "r.json",
            "--carry-in",
            "c3.json",
        ],
    );
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    let r = 读(d.join("r.json"));
    assert_eq!(r["explain"]["budget"]["calls"], 3, "{}", r["explain"]);
    assert!(
        r["explain"]["rejected"].is_null(),
        "{}",
        r["explain"]["rejected"]
    );
    assert_eq!(r["cost"]["calls"], 3);
    assert_eq!(r["budget"]["unsent"], 2);
    // 文本：预算行是 3，不出现程序声明的 100
    let o = jpp(
        &d,
        &[
            "run",
            "p5.jpp",
            "--fixtures",
            "fx5.json",
            "--explain",
            "--output",
            "r2.json",
            "--carry-in",
            "c3.json",
        ],
    );
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    let err = String::from_utf8_lossy(&o.stderr);
    assert!(err.contains("预算：calls 3 "), "{err}");
    assert!(!err.contains("calls 100"), "{err}");
    // 余额 0：explain.budget 是 0，拒绝非空，修法改指上游（G2），运行在计划期停下
    let o = jpp(
        &d,
        &[
            "run",
            "p5.jpp",
            "--fixtures",
            "fx5.json",
            "--explain",
            "--carry-in",
            "c0.json",
        ],
    );
    assert!(!o.status.success());
    let err = String::from_utf8_lossy(&o.stderr);
    assert!(err.contains("预算：calls 0 "), "{err}");
    assert!(
        err.contains("拒绝：") && err.contains("上游余额") && err.contains("程序声明 calls 100"),
        "{err}"
    );
    assert!(!err.contains("修法：放宽 budget"), "{err}");
}

/// P30（R1）：`--carry-reauthorize` 要有 `--carry-in`，不能与 `--replay` 同给
#[test]
fn p30_重新授权开关的用法() {
    let d = tmp("reauth");
    fs::write(d.join("p.jpp"), 程序).unwrap();
    fs::write(d.join("fx.json"), 夹具().to_string()).unwrap();
    let o = jpp(
        &d,
        &[
            "run",
            "p.jpp",
            "--fixtures",
            "fx.json",
            "--carry-reauthorize",
        ],
    );
    assert!(!o.status.success());
    assert!(
        String::from_utf8_lossy(&o.stderr).contains("--carry-reauthorize requires --carry-in"),
        "{}",
        String::from_utf8_lossy(&o.stderr)
    );
    余额文件(&d, "c.json", 5);
    let o = jpp(
        &d,
        &[
            "run",
            "p.jpp",
            "--replay",
            "l.json",
            "--carry-in",
            "c.json",
            "--carry-reauthorize",
        ],
    );
    assert!(!o.status.success(), "重放不收宿主余额，也就不收重新授权");
    // R9：失败原因是 `--carry-in` 与 `--replay` 互斥
    assert!(
        String::from_utf8_lossy(&o.stderr).contains("--carry-in cannot be combined with --replay"),
        "{}",
        String::from_utf8_lossy(&o.stderr)
    );
}

/// P34（R10）：`--carry-in` 与费用确认同给——确认门的上界是生效预算的 cost（程序声明与上游余额取小）
#[test]
fn p34_余额收紧确认门的上界() {
    let d = tmp("confirm");
    let 画像 = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../profiles/judge-claude-p-priced.json")
        .canonicalize()
        .unwrap();
    let 画像 = 画像.display().to_string();
    fs::write(
        d.join("p.jpp"),
        "budget {calls: 5, cost: 1};\nlet a = cut(judge(state(mat(\"甲\")), test(\"行吗\", \"k\")));\nconsume([a], \"drop\");\n1\n",
    )
    .unwrap();
    fs::write(
        d.join("c.json"),
        json!({"calls": 5, "cost": 0.05, "latency_p95": null, "escalate": 0, "hop": 0, "round": 0, "depth_cap": 256})
            .to_string(),
    )
    .unwrap();
    // 不带余额：声明 cost 1 > 默认阈值 0.1，要 --confirm
    let o = jpp(
        &d,
        &["run", "p.jpp", "--backend", "stub", "--profile", &画像],
    );
    assert!(!o.status.success());
    let e = String::from_utf8_lossy(&o.stderr);
    assert!(
        e.contains("E-confirm-required") && e.contains("生效费用上限"),
        "{e}"
    );
    // 带余额 cost 0.05：上界 0.05，放行
    let o = jpp(
        &d,
        &[
            "run",
            "p.jpp",
            "--backend",
            "stub",
            "--profile",
            &画像,
            "--carry-in",
            "c.json",
            "--output",
            "r.json",
        ],
    );
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    let r = 读(d.join("r.json"));
    assert_eq!(r["confirm"]["upper_usd"], 0.05, "{}", r["confirm"]);
    assert_eq!(r["confirm"]["verdict"], "within");
    // Z0384（附注五 P34′）：余额比程序声明更紧时，来源写明是余额
    assert_eq!(r["confirm"]["upper_from"], "carry");
}

/// P44（Z0384 小项）：确认门 `carry` 分支的拒绝报文与 `--explain` 文本都写「上游余额收紧后的费用上限」
#[test]
fn p44_确认门余额分支的文案() {
    let d = tmp("confirm-carry");
    let 画像 = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../profiles/judge-claude-p-priced.json")
        .canonicalize()
        .unwrap();
    let 画像 = 画像.display().to_string();
    fs::write(
        d.join("p.jpp"),
        "budget {calls: 5, cost: 1};\nlet a = cut(judge(state(mat(\"甲\")), test(\"行吗\", \"k\")));\nconsume([a], \"drop\");\n1\n",
    )
    .unwrap();
    fs::write(
        d.join("c.json"),
        json!({"calls": 5, "cost": 0.5, "latency_p95": null, "escalate": 0, "hop": 0, "round": 0, "depth_cap": 256})
            .to_string(),
    )
    .unwrap();
    let o = jpp(
        &d,
        &[
            "run",
            "p.jpp",
            "--backend",
            "stub",
            "--profile",
            &画像,
            "--carry-in",
            "c.json",
            "--explain",
        ],
    );
    assert!(!o.status.success(), "余额 cost 0.5 仍超默认阈值 0.1");
    let e = String::from_utf8_lossy(&o.stderr);
    assert!(e.contains("E-confirm-required"), "{e}");
    assert!(
        e.matches("上游余额收紧后的费用上限").count() >= 2,
        "拒绝报文与 --explain 的确认一节都写：{e}"
    );
}
