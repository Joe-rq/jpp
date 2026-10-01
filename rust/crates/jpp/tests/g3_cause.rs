//! G3（步 36）：未决原因封闭化（预注册 `地基/过程记录/工程-G3-原因封闭化.md` §二·3 B-1 至 B-4）。
//!
//! 依据：B197（原因十六种、`unsure(text)` 非成员报 `E-unsure-cause`、`"unsure(<cause>"` 前缀冻结）；`12` §2.13 R15。

use jpp::effects::{CalibStore, FnPort, JudgeResult, Ports};
use jpp::ledger::Ledger;
use jpp::value::{Answer, UnsureCause};
use jpp::{ActionRegistry, Outcome, lower, run, syntax::parse};
use serde_json::json;

fn 定值端口(p: f64) -> Ports<'static> {
    Ports::new().with(FnPort::judge("fixed-0", move |_s, qs| {
        Ok(JudgeResult {
            answers: qs.iter().map(|_| Answer::Noul(p)).collect(),
            tokens: 0,
            cost: 0.0,
            perms: vec![],
            mode_share: vec![],
            confidence: vec![],
        })
    }))
}

fn 跑(src: &str, guard: bool) -> Result<Outcome, String> {
    let mut program = lower(&parse(src).expect("解析")).expect("lower");
    program.entry.guard = guard;
    let mut calib = CalibStore::new();
    calib.put("k", 0.65, 0.35, 100, "上岗", Some(0.05)).unwrap();
    run(
        &program,
        定值端口(0.95),
        &calib,
        &ActionRegistry::new(),
        &mut Ledger::new(),
    )
    .map_err(|e| e.render())
}

/// B-1：字面量非成员由检查器报 `E-unsure-cause`，程序不跑；成员照常
#[test]
fn b1_字面量非成员检查器报错() {
    let e = 跑(
        "budget {calls: 1, cost: 0, depth: 8};\nlet u = unsure(\"材料不够\");\nu\n",
        false,
    )
    .expect_err("非成员原因");
    assert!(e.contains("E-unsure-cause"), "{e}");
    let o = 跑(
        "budget {calls: 1, cost: 0, depth: 8};\nlet u = unsure(\"fail\");\nu\n",
        false,
    )
    .unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(o.returned_unsure, ["unsure(fail)"]);
}

/// B-2：非字面量非成员在运行期报 `E-unsure-cause`
#[test]
fn b2_非字面量非成员运行期报错() {
    let e = 跑(
        "budget {calls: 1, cost: 0, depth: 8};\nlet t = \"材料\" + \"不够\";\nlet u = unsure(t);\nu\n",
        false,
    )
    .expect_err("非成员原因");
    assert!(e.contains("E-unsure-cause"), "{e}");
}

const 缺证据: &str = r#"budget {calls: 2, cost: 1, depth: 8};
let q = test("这份合同有没有违约条款？", "k", {evidence: ["ctx"]});
let e = cut(judge(state(mat("合同摘要")), q));
"#;

/// B-3：标签带细节（冻结前缀不变），原因名只写成员名
#[test]
fn b3_标签带细节_原因名不带() {
    let src = format!("{缺证据}consume(e, \"drop\");\n{{k: exit_kind(e), c: unsure_cause(e)}}\n");
    let o = 跑(&src, false).unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(
        o.value_json(),
        json!({"k": "unsure(insufficient:ctx)", "c": "insufficient"})
    );
}

/// B-4：欠账记号的原因是成员名（与 jpp-cell 的 `cause.name()` 同口径），不带细节
#[test]
fn b4_欠账记号原因是成员名() {
    let src = format!("{缺证据}7\n");
    let o = 跑(&src, true).unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(o.violations.len(), 1);
    let m = &o.violations[0].mark;
    assert_eq!(m.cause, "insufficient");
    assert_eq!(
        UnsureCause::parse(&m.cause),
        Some(UnsureCause::Insufficient)
    );
    assert_eq!(o.violations[0].token, m.token());
}

/// 附录三 F-1、F-2（复核 C1、实测 T1）：G3 之前录的账本（去向事件原因带细节，如 `Refine.cause` 为 `insufficient:ref`）
/// 在本版下审计重放不报 `W-replay-duty`；续跑不重复记 `Refine`。
///
/// 旧账本当场造：用本版跑 `derive-chain` 录一份，把 `Refine.cause` 改回 G3 之前的带细节写法、重新编码（哈希链随之重算）。
/// 原先用 G3 合入前的金样账本作固定夹具，库一改（键带站点）就过期、重放报 `E-replay`（Z0521 全量时撞到），所以改为当场造
#[test]
fn f1_f2_旧账本去向事件原因归一后比对() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let dir = std::env::temp_dir().join(format!("jpp-g3-f1-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let 跑 = |额外: &[&str]| {
        let out = std::process::Command::new(env!("CARGO_BIN_EXE_jpp"))
            .current_dir(&dir)
            .args([
                "run",
                root.join("examples/derive-chain.jpp").to_str().unwrap(),
                "--fixtures",
                root.join("examples/fixtures/derive-chain.json")
                    .to_str()
                    .unwrap(),
                "--output",
                "r.json",
            ])
            .args(额外)
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        let r: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(dir.join("r.json")).unwrap()).unwrap();
        r
    };
    let 告警 = |r: &serde_json::Value| -> Vec<String> {
        r["trace"]["warnings"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|w| w.as_str())
            .filter(|w| w.starts_with("W-replay-duty"))
            .map(String::from)
            .collect()
    };
    // 造旧账本：本版录一份，Refine 的原因改回带细节的写法
    跑(&["--ledger-out", "fresh.json"]);
    let (mut l, t) =
        Ledger::decode(&std::fs::read_to_string(dir.join("fresh.json")).unwrap()).unwrap();
    assert!(t.is_none());
    let mut 改了 = 0;
    for e in l.entries.iter_mut() {
        if let jpp::ledger::Entry::Refine { cause, .. } = e
            && cause == "insufficient"
        {
            *cause = "insufficient:ref".into();
            改了 += 1;
        }
    }
    assert!(改了 > 0, "derive-chain 账本里应有 insufficient 的 Refine");
    let 旧 = dir.join("old.json");
    std::fs::write(&旧, l.encode()).unwrap();
    // F-1：审计重放
    let a = dir.join("a.json");
    std::fs::copy(&旧, &a).unwrap();
    let r = 跑(&["--replay", a.to_str().unwrap()]);
    assert_eq!(告警(&r), Vec::<String>::new());
    assert_eq!(r["cost"]["calls"], 0);
    // F-2：续跑不重复记 Refine
    let b = dir.join("b.json");
    std::fs::copy(&旧, &b).unwrap();
    let r = 跑(&["--resume", b.to_str().unwrap(), "--ledger-out", "out.json"]);
    assert_eq!(告警(&r), Vec::<String>::new());
    let 原 = std::fs::read_to_string(&旧)
        .unwrap()
        .matches("\"Refine\"")
        .count();
    let 后 = std::fs::read_to_string(dir.join("out.json"))
        .unwrap()
        .matches("\"Refine\"")
        .count();
    assert_eq!(后, 原, "续跑不重复记 Refine");
    let _ = std::fs::remove_dir_all(&dir);
}
