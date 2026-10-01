//! G2（步 35）：违规的单次形态与守卫下推迟的不可逆 `do`（预注册 `地基/过程记录/工程-G2-违规单次形态.md`
//! §二·2 V-1 至 V-7、附录一；附录三 W-1 至 W-10，复核修补 Z0564）。
//!
//! 依据：`12` §2.13 R9、R13；裁定五十九第 3 条；裁定六十一 B200；主控 Z0507、Z0508、Z0519。

mod common;

use std::cell::Cell;
use std::rc::Rc;

use jpp::effects::{CalibStore, FnPort, JudgeResult, Ports};
use jpp::ledger::{Entry, Ledger, WithheldCause};
use jpp::value::{Answer, Value};
use jpp::{ActionRegistry, Outcome, TaintOut, run};
use jpp::{lower, syntax::parse};

/// 是非题恒 0.5：没有线时两块并列 → `Unsure(tie)`
fn 并列端口<'a>(calls: &'a Cell<u64>) -> Ports<'a> {
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

/// 同 [`并列端口`]，伴随元题给中性读数（Z0398 起不开 `--guard` 时伴随题默认开着；与 Z0398 改的 17 个测试同一做法），
/// K 选一的题按题型答（默认链「缺哪类信息」那道是 K 选一）。原端口对所有题答 Noul，rebase 到 Z0398 之后报 E-rt-answer
/// 中止——伴随或默认链的题回答形状不符时运行时应降级不中止，是主控板 Z0556 要修的产品风险
fn 并列端口_伴随中性<'a>(calls: &'a Cell<u64>) -> Ports<'a> {
    Ports::new().with(common::伴随中性judge("fixed-0", move |s, qs| {
        calls.set(calls.get() + 1);
        Ok(JudgeResult {
            // 默认链「缺哪类信息」那道是 K 选一（Z0398 后默认有类目），按题型答：K 选一给均匀读数（并列），是非题 0.5
            answers: qs
                .iter()
                .map(|q| match q.op {
                    jpp::value::Op::Select => {
                        Answer::Choice(vec![1.0 / s.over.len().max(1) as f64; s.over.len()])
                    }
                    _ => Answer::Noul(0.5),
                })
                .collect(),
            tokens: 0,
            cost: 0.0,
            mode_share: qs.iter().map(|_| None).collect(),
            perms: qs.iter().map(|_| 0).collect(),
            confidence: vec![],
        })
    }))
}

/// 是非题恒 0.9：没有线时按多数块 → act（守卫放行要可信合取项，材料是程序自己写的）
fn 放行端口<'a>(calls: &'a Cell<u64>) -> Ports<'a> {
    Ports::new().with(FnPort::judge("fixed-0", move |_s, qs| {
        calls.set(calls.get() + 1);
        Ok(JudgeResult {
            answers: qs.iter().map(|_| Answer::Noul(0.9)).collect(),
            tokens: 0,
            cost: 0.0,
            mode_share: qs.iter().map(|_| None).collect(),
            perms: qs.iter().map(|_| 0).collect(),
            confidence: vec![],
        })
    }))
}

/// 题 `k`、`k2` 有上岗线（0.75/0.25）：0.9 → act 且放行；0.5 落在带内 → `Unsure(band)`
fn 库() -> CalibStore {
    let mut c = CalibStore::new();
    for k in ["k", "k2"] {
        common::certified(&mut c, k, 0.75, 0.25, 100);
    }
    c
}

fn 跑(
    src: &str,
    guard: bool,
    ports: Ports<'_>,
    acts: &ActionRegistry,
    l: &mut Ledger,
) -> Result<Outcome, String> {
    let mut program = lower(&parse(src).expect("解析")).expect("lower");
    program.entry.guard = guard;
    run(&program, ports, &库(), acts, l).map_err(|e| e.render())
}

fn 违规条(l: &Ledger) -> Vec<jpp::ledger::DebtMark> {
    l.entries
        .iter()
        .filter_map(|e| match e {
            Entry::Violation { mark, .. } => Some(mark.clone()),
            _ => None,
        })
        .collect()
}

/// V-1：程序结束时欠一笔（`--guard` 下不走默认链）：返回 Ok、值照带、一笔违规；账本一条 `Violation`，
/// 其记号摘要 = 报告里的 token（主语言算出的记号 = jpp-ledger 摘要）
#[test]
fn v1_程序结束欠一笔_记违规值照带() {
    let src = r#"budget {calls: 4, cost: 0, depth: 16};
let e = cut(judge(state(mat("甲")), test("行吗", "k")));
42
"#;
    let calls = Cell::new(0);
    let mut l = Ledger::new();
    let o =
        跑(src, true, 并列端口(&calls), &ActionRegistry::new(), &mut l).expect("不再是运行期错误");
    assert_eq!(o.value_json(), serde_json::json!(42), "值照带");
    assert_eq!(o.violations.len(), 1);
    let v = &o.violations[0];
    assert_eq!(v.mark.cause, "band");
    assert_eq!(v.mark.frame, jpp::ledger::FrameKind::Program);
    assert_eq!(v.mark.via, jpp::ledger::Via::Cut);
    assert_eq!(v.mark.nth, 1);
    assert!(v.message.contains("程序结束时有未消费的"), "{}", v.message);
    let marks = 违规条(&l);
    assert_eq!(marks.len(), 1);
    assert_eq!(marks[0], v.mark);
    assert_eq!(
        marks[0].token(),
        v.token,
        "主语言算出的记号 = jpp-ledger 摘要"
    );
}

/// V-2：具名函数丢了未决：函数返回处不报错，程序结束时记违规，主人是那个函数、帧是 code，报文说明它在哪丢的
#[test]
fn v2_函数返回前丢的_程序结束记违规() {
    let src = r#"budget {calls: 4, cost: 0, depth: 16};
fn 判(t) { let e = cut(judge(state(mat(t)), test("行吗", "k"))); 1 }
let a = 判("甲");
a
"#;
    let calls = Cell::new(0);
    let mut l = Ledger::new();
    let o =
        跑(src, true, 并列端口(&calls), &ActionRegistry::new(), &mut l).expect("不再是运行期错误");
    assert_eq!(o.value_json(), serde_json::json!(1));
    assert_eq!(o.violations.len(), 1);
    let v = &o.violations[0];
    assert_eq!(v.mark.frame, jpp::ledger::FrameKind::Code);
    // 附录三 W-12：函数帧主人是 `<函数名>#<实参哈希>`
    assert!(v.mark.owner.starts_with("判#"), "{}", v.mark.owner);
    assert!(v.message.contains("判 返回前"), "{}", v.message);
    assert_eq!(违规条(&l).len(), 1);
}

/// V-3：同一笔只记一条；审计重放不重复写
#[test]
fn v3_同一笔只记一条_重放不重复写() {
    let src = r#"budget {calls: 4, cost: 0, depth: 16};
let e = cut(judge(state(mat("甲")), test("行吗", "k")));
let f = cut(judge(state(mat("乙")), test("行吗", "k")));
0
"#;
    let calls = Cell::new(0);
    let mut l = Ledger::new();
    let o = 跑(src, true, 并列端口(&calls), &ActionRegistry::new(), &mut l).expect("跑完");
    assert_eq!(o.violations.len(), 2, "两个站点两笔");
    assert_ne!(o.violations[0].token, o.violations[1].token);
    assert_eq!(违规条(&l).len(), 2);
    let n0 = l.entries.len();
    let mut program = lower(&parse(src).expect("解析")).expect("lower");
    program.entry.guard = true;
    let c2 = Cell::new(0);
    let o2 = jpp::run_replay(
        &program,
        并列端口(&c2),
        &库(),
        &ActionRegistry::new(),
        &mut l,
    )
    .unwrap_or_else(|e| panic!("重放：{}", e.render()));
    assert_eq!(c2.get(), 0);
    assert_eq!(o2.violations.len(), 2, "重放同样记两笔");
    assert_eq!(l.entries.len(), n0, "审计重放不重复写 Violation");
}

fn 动作表(执行: &Rc<Cell<u32>>) -> ActionRegistry {
    let mut acts = ActionRegistry::new();
    let e2 = 执行.clone();
    acts.register("发邮件", 0.0, false, TaintOut::Trusted, move |_| {
        e2.set(e2.get() + 1);
        Ok(Value::text("已发"))
    });
    acts
}

/// V-4a：守卫下不可逆 do 只作语句、没有违规：程序结束后执行一次；账本依次 Intent、Effect；返回值里交出它的，
/// 看到的是结算后的产出
#[test]
fn v4a_守卫下推迟_无违规结算后执行() {
    let src = r#"budget {calls: 4, cost: 0, depth: 16};
let ok = handle(cut(judge(state(mat("程序自己写的材料")), test("该发吗", "k"))), {act: fn() { true }, ignore: fn() { false }, unsure: fn(u) { consume(u, "drop"); false }});
let r = if ok { do("发邮件", [], 0) } else { "不发" };
{r: r}
"#;
    let 执行 = Rc::new(Cell::new(0));
    let acts = 动作表(&执行);
    let calls = Cell::new(0);
    let mut l = Ledger::new();
    let o = 跑(src, true, 放行端口(&calls), &acts, &mut l).unwrap_or_else(|e| panic!("{e}"));
    assert!(o.violations.is_empty());
    assert_eq!(执行.get(), 1, "结算后执行一次");
    let 种类: Vec<&str> = l
        .entries
        .iter()
        .filter_map(|e| match e {
            Entry::Intent { .. } => Some("Intent"),
            Entry::Effect { .. } => Some("Effect"),
            Entry::Withheld { .. } => Some("Withheld"),
            _ => None,
        })
        .collect();
    assert_eq!(种类, ["Intent", "Effect"]);
    assert!(
        matches!(
            l.entries.iter().find(|e| matches!(e, Entry::Intent { .. })),
            Some(Entry::Intent {
                attempt: Some(_),
                ..
            })
        ),
        "意向带尝试引用"
    );
    assert_eq!(
        o.value_json()["r"]["content"],
        serde_json::json!("已发"),
        "返回值里是结算后的产出（动作输出材料）"
    );
}

/// V-4b：守卫下有违规：不执行，Intent 之后一条 Withheld(violation)，返回值里是 withheld 失败值
#[test]
fn v4b_守卫下推迟_有违规扣下() {
    let src = r#"budget {calls: 4, cost: 0, depth: 16};
let ok = handle(cut(judge(state(mat("程序自己写的材料")), test("该发吗", "k"))), {act: fn() { true }, ignore: fn() { false }, unsure: fn(u) { consume(u, "drop"); false }});
let r = if ok { do("发邮件", [], 0) } else { "不发" };
let 欠 = cut(judge(state(mat("另一份")), test("行吗", "k2")));
{r: r}
"#;
    let 执行 = Rc::new(Cell::new(0));
    let acts = 动作表(&执行);
    // 第一题 0.9 放行，第二题 0.5 并列 → 欠一笔
    let calls = Cell::new(0);
    let ports = Ports::new().with(FnPort::judge("fixed-0", |_s, qs| {
        calls.set(calls.get() + 1);
        Ok(JudgeResult {
            answers: qs
                .iter()
                .map(|q| Answer::Noul(if q.text.contains("该发") { 0.9 } else { 0.5 }))
                .collect(),
            tokens: 0,
            cost: 0.0,
            mode_share: qs.iter().map(|_| None).collect(),
            perms: qs.iter().map(|_| 0).collect(),
            confidence: vec![],
        })
    }));
    let mut l = Ledger::new();
    let o = 跑(src, true, ports, &acts, &mut l).unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(o.violations.len(), 1);
    assert_eq!(执行.get(), 0, "有违规不执行");
    let w: Vec<&Entry> = l
        .entries
        .iter()
        .filter(|e| matches!(e, Entry::Withheld { .. }))
        .collect();
    assert_eq!(w.len(), 1);
    assert!(
        matches!(w[0], Entry::Withheld { cause: WithheldCause::Violation, key, .. } if key.starts_with("intent:"))
    );
    assert!(!l.entries.iter().any(|e| matches!(e, Entry::Effect { .. })));
    let r = o.value_json()["r"].to_string();
    assert!(r.contains("withheld"), "{r}");
    // 附录三 W-9：Violation 在引用它的 Withheld 之前
    assert_eq!(种类(&l), ["Intent", "Violation", "Withheld(violation)"]);
}

/// V-5：守卫下同一趟读了不可逆 do 的结果：E-guard-irreversible-midway，动作不执行
#[test]
fn v5_守卫下读推迟动作的结果报错() {
    let src = r#"budget {calls: 4, cost: 0, depth: 16};
let ok = handle(cut(judge(state(mat("程序自己写的材料")), test("该发吗", "k"))), {act: fn() { true }, ignore: fn() { false }, unsure: fn(u) { consume(u, "drop"); false }});
let r = if ok { content(do("发邮件", [], 0)) } else { "不发" };
r
"#;
    let 执行 = Rc::new(Cell::new(0));
    let acts = 动作表(&执行);
    let calls = Cell::new(0);
    let e =
        跑(src, true, 放行端口(&calls), &acts, &mut Ledger::new()).expect_err("读了推迟动作的结果");
    assert!(e.contains("E-guard-irreversible-midway"), "{e}");
    assert_eq!(执行.get(), 0);
}

/// V-6：不开 --guard：不可逆 do 立即执行（读结果照常）。附录三 W-10：函数里丢下的那笔被默认链收走，违规 0 笔
#[test]
fn v6_不开守卫立即执行_默认链收走未决() {
    let src = r#"budget {calls: 4, cost: 0, depth: 16};
let r = content(do("发邮件", [], 0));
fn 判(t) { let e = cut(judge(state(mat(t)), test("行吗", "k"))); 1 }
let a = 判("甲");
{r: r, a: a}
"#;
    let 执行 = Rc::new(Cell::new(0));
    let acts = 动作表(&执行);
    let calls = Cell::new(0);
    let mut l = Ledger::new();
    let o =
        跑(src, false, 并列端口_伴随中性(&calls), &acts, &mut l).unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(执行.get(), 1);
    assert_eq!(o.value_json()["r"], serde_json::json!("已发"));
    // 不开守卫时缺去向的未决在站点当场走默认链（B0492），函数里丢下的那笔被它收走，程序结束时不再欠着
    assert!(o.violations.is_empty(), "{:?}", o.violations);
    assert!(违规条(&l).is_empty());
    assert!(!o.unsure_default.is_empty(), "默认链收走了它");
    assert_eq!(o.value_json()["a"], serde_json::json!(1));
}

/// V-7：写法错照旧是运行期 J-05（Fn¹ 第二次调用）
#[test]
fn v7_写法错仍是运行期j05() {
    let src = r#"budget {calls: 4, cost: 0, depth: 16};
fn mk() { let e = cut(judge(state(mat("甲")), test("行吗", "k"))); fn() { e } }
let k = mk();
let a = k();
let b = k();
[a, b]
"#;
    let calls = Cell::new(0);
    let e = 跑(
        src,
        true,
        并列端口(&calls),
        &ActionRegistry::new(),
        &mut Ledger::new(),
    )
    .expect_err("Fn¹ 调两次");
    assert!(e.contains("J-05"), "{e}");
}

/// V-8（附录二）：`--guard` 下 handle 缺 unsure 臂、出口确为未决：不当场报错，这笔往下传；没有被交出就在程序结束记违规
#[test]
fn v8_守卫下缺unsure臂_未交出即记违规() {
    let src = r#"budget {calls: 4, cost: 0, depth: 16};
let x = handle(cut(judge(state(mat("甲")), test("行吗", "k"))), {act: fn() { 1 }, ignore: fn() { 0 }});
7
"#;
    let calls = Cell::new(0);
    let mut l = Ledger::new();
    let o = 跑(src, true, 并列端口(&calls), &ActionRegistry::new(), &mut l)
        .unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(o.value_json(), serde_json::json!(7));
    assert_eq!(o.violations.len(), 1);
    // 出口已决时照常选臂，不报错
    let o2 = 跑(
        src,
        true,
        放行端口(&calls),
        &ActionRegistry::new(),
        &mut Ledger::new(),
    )
    .unwrap_or_else(|e| panic!("{e}"));
    assert!(o2.violations.is_empty());
}

/// V-1 CLI：违规时进程退出码 3，stderr 逐笔 J-05 行加一行汇总
#[test]
fn v1_cli_退出码3() {
    let dir = std::env::temp_dir().join(format!("jpp-g2-cli-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let out = std::process::Command::new(env!("CARGO_BIN_EXE_jpp"))
        .current_dir(&dir)
        .args([
            "run",
            root.join("examples/errors/outcome-dropped.jpp")
                .to_str()
                .unwrap(),
            "--fixtures",
            root.join("examples/fixtures/outcome-dropped.json")
                .to_str()
                .unwrap(),
            "--guard",
            "--output",
            "report.json",
        ])
        .output()
        .unwrap();
    assert_eq!(
        out.status.code(),
        Some(3),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(
        err.contains("J-05: 程序结束时有未消费的") && err.contains("违规 1 笔"),
        "{err}"
    );
    let report: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(dir.join("report.json")).unwrap()).unwrap();
    assert_eq!(report["status"], "violation");
    assert_eq!(report["violations"].as_array().unwrap().len(), 1);
    let _ = std::fs::remove_dir_all(&dir);
}

// ───────────── 附录三（复核修补，Z0564）─────────────

/// 账本里与推迟动作有关的条目种类，按账本顺序
fn 种类(l: &Ledger) -> Vec<String> {
    l.entries
        .iter()
        .filter_map(|e| match e {
            Entry::Intent { .. } => Some("Intent".to_string()),
            Entry::Effect { kind, .. } if kind == "do" => Some("Effect".to_string()),
            Entry::Violation { .. } => Some("Violation".to_string()),
            Entry::Withheld { cause, .. } => Some(format!(
                "Withheld({})",
                serde_json::to_value(cause).unwrap().as_str().unwrap()
            )),
            _ => None,
        })
        .collect()
}

/// 判断恒 0.9（放行），问人按 `答` 回答或不答（挂起）
fn 放行问人端口<'a>(calls: &'a Cell<u64>, 答: bool) -> Ports<'a> {
    放行端口(calls).with(FnPort::ask("fixed-0", move |_s, _q| {
        Ok(答.then_some(Answer::Noul(0.9)))
    }))
}

fn 守卫程序(src: &str) -> jpp::Program {
    let mut p = lower(&parse(src).expect("解析")).expect("lower");
    p.entry.guard = true;
    p
}

/// 模拟「推迟动作执行完、写 `Effect` 之前进程被杀」：账本编码后去掉最后一条 `do` 的 `Effect` 及其后各行，再解码
/// （文件账本重载同一条路径）
fn 杀在效果之前(l: &Ledger) -> Ledger {
    let i = l
        .entries
        .iter()
        .rposition(|e| matches!(e, Entry::Effect { kind, .. } if kind == "do"))
        .expect("有 Effect");
    let 全 = l.encode();
    let 行: Vec<&str> = 全.lines().collect();
    let mut s = 行[..1 + i].join("\n");
    s.push('\n');
    let (d, t) = Ledger::decode(&s).expect("解码");
    assert!(t.is_none());
    assert_eq!(d.entries.len(), i);
    d
}

/// 第三趟：最后一条意向之后既无 Withheld 也无 Effect，给 unknown_outcome、不执行；审计重放同样
fn 第三趟结果未知(
    src: &str,
    l: &Ledger,
    执行: &Rc<Cell<u32>>,
    ports: impl Fn() -> Ports<'static>,
) {
    let acts = 动作表(执行);
    let before = 执行.get();
    let mut l3 = 杀在效果之前(l);
    assert!(
        !l3.intent_withheld(&intent_key(&l3)),
        "最后一条意向之后没有 Withheld"
    );
    let o = jpp::run(&守卫程序(src), ports(), &库(), &acts, &mut l3)
        .unwrap_or_else(|e| panic!("第三趟：{}", e.render()));
    assert_eq!(执行.get(), before, "不重复执行");
    let r = o.value_json()["r"].to_string();
    assert!(r.contains("unknown_outcome"), "{r}");
    assert!(
        o.trace
            .warnings
            .iter()
            .any(|w| w.starts_with("W-unknown-outcome")),
        "{:?}",
        o.trace.warnings
    );
    // 审计重放（另一份重载的账本）结果一致
    let mut l4 = 杀在效果之前(l);
    let o4 = jpp::run_replay(&守卫程序(src), ports(), &库(), &acts, &mut l4)
        .unwrap_or_else(|e| panic!("重放：{}", e.render()));
    assert_eq!(执行.get(), before);
    assert!(o4.value_json()["r"].to_string().contains("unknown_outcome"));
}

fn intent_key(l: &Ledger) -> String {
    l.entries
        .iter()
        .find_map(|e| match e {
            Entry::Intent { key, .. } => Some(key.clone()),
            _ => None,
        })
        .expect("有意向")
}

fn 静态放行端口() -> Ports<'static> {
    let c: &'static Cell<u64> = Box::leak(Box::new(Cell::new(0)));
    放行问人端口(c, true)
}

const 先做后问: &str = r#"budget {calls: 10, cost: 0, depth: 16, escalate: 2};
let ok = handle(cut(judge(state(mat("程序自己写的材料")), test("该发吗", "k"))), {act: fn() { true }, ignore: fn() { false }, unsure: fn(u) { consume(u, "drop"); false }});
let r = if ok { do("发邮件", [], 0) } else { "不发" };
let h = handle(ask(state(mat("问")), test("行吗", "k")), {act: fn() { 1 }, ignore: fn() { 0 }, unsure: fn(u) { consume(u, "drop"); -1 }});
{r: r, h: h}
"#;

/// W-1：守卫下 do 之后 ask 未答而挂起：不执行，Intent 之后 Withheld(suspended)；续接答了：执行一次，本趟有自己的意向，
/// 返回值里是产出、不是 unknown_outcome。W-3：续接后执行完、写 Effect 前被杀，第三趟 unknown_outcome、不重复执行
#[test]
fn w1_w3_挂起后续接执行一次_执行后被杀不重复执行() {
    let 执行 = Rc::new(Cell::new(0));
    let acts = 动作表(&执行);
    let calls = Cell::new(0);
    let mut l = Ledger::new();
    let o = 跑(先做后问, true, 放行问人端口(&calls, false), &acts, &mut l)
        .unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(o.pending.first().map(|p| p.cause.as_str()), Some("ask"));
    assert_eq!(执行.get(), 0, "挂起时不执行");
    assert_eq!(种类(&l), ["Intent", "Withheld(suspended)"]);
    // 续接：答了
    let o = 跑(先做后问, true, 放行问人端口(&calls, true), &acts, &mut l)
        .unwrap_or_else(|e| panic!("{e}"));
    assert!(o.pending.is_empty(), "{:?}", o.pending);
    assert_eq!(执行.get(), 1, "续接后执行一次");
    assert_eq!(
        种类(&l),
        ["Intent", "Withheld(suspended)", "Intent", "Effect"]
    );
    let 意向: Vec<&Entry> = l
        .entries
        .iter()
        .filter(|e| matches!(e, Entry::Intent { .. }))
        .collect();
    assert!(
        matches!((意向[0], 意向[1]), (Entry::Intent { attempt: Some(a), .. }, Entry::Intent { attempt: Some(b), .. }) if a != b),
        "两趟各有自己的意向：{意向:?}"
    );
    assert_eq!(o.value_json()["r"]["content"], serde_json::json!("已发"));
    assert_eq!(o.value_json()["h"], serde_json::json!(1));
    // W-3
    第三趟结果未知(先做后问, &l, &执行, 静态放行端口);
}

const 先做后错: &str = r#"budget {calls: 10, cost: 0, depth: 16};
let ok = handle(cut(judge(state(mat("程序自己写的材料")), test("该发吗", "k"))), {act: fn() { true }, ignore: fn() { false }, unsure: fn(u) { consume(u, "drop"); false }});
let r = if ok { do("发邮件", [], 0) } else { "不发" };
let x = [1, 2][I];
{r: r, x: x}
"#;

/// W-2：守卫下 do 之后运行期出错：不执行，Intent 之后 Withheld(error)；修好（do 的站点与实参不变）再跑同一账本：执行一次
#[test]
fn w2_出错修好后再跑执行一次() {
    let 执行 = Rc::new(Cell::new(0));
    let acts = 动作表(&执行);
    let calls = Cell::new(0);
    let mut l = Ledger::new();
    let 坏 = 先做后错.replace("[I]", "[5]");
    let e = 跑(&坏, true, 放行端口(&calls), &acts, &mut l).expect_err("下标越界");
    assert_eq!(执行.get(), 0, "出错时不执行：{e}");
    assert_eq!(种类(&l), ["Intent", "Withheld(error)"]);
    let 好 = 先做后错.replace("[I]", "[0]");
    let o = 跑(&好, true, 放行端口(&calls), &acts, &mut l).unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(执行.get(), 1, "修好后执行一次");
    assert_eq!(种类(&l), ["Intent", "Withheld(error)", "Intent", "Effect"]);
    assert_eq!(o.value_json()["r"]["content"], serde_json::json!("已发"));
}

/// W-4：违规一路同形：扣下之后去掉欠账再跑，执行一次、有自己的意向；执行后被杀，第三趟 unknown_outcome
#[test]
fn w4_违规扣下后再跑执行一次_执行后被杀不重复执行() {
    let 有欠 = r#"budget {calls: 10, cost: 0, depth: 16};
let ok = handle(cut(judge(state(mat("程序自己写的材料")), test("该发吗", "k"))), {act: fn() { true }, ignore: fn() { false }, unsure: fn(u) { consume(u, "drop"); false }});
let r = if ok { do("发邮件", [], 0) } else { "不发" };
let 欠 = cut(judge(state(mat("另一份")), test("行吗", "k2")));
{r: r}
"#;
    let 不欠 = 有欠.replace(
        r#"cut(judge(state(mat("另一份")), test("行吗", "k2")))"#,
        "0",
    );
    let 执行 = Rc::new(Cell::new(0));
    let acts = 动作表(&执行);
    let 端口 = |calls: &'static Cell<u64>| {
        Ports::new().with(FnPort::judge("fixed-0", move |_s, qs| {
            calls.set(calls.get() + 1);
            Ok(JudgeResult {
                answers: qs
                    .iter()
                    .map(|q| Answer::Noul(if q.text.contains("该发") { 0.9 } else { 0.5 }))
                    .collect(),
                tokens: 0,
                cost: 0.0,
                mode_share: qs.iter().map(|_| None).collect(),
                perms: qs.iter().map(|_| 0).collect(),
                confidence: vec![],
            })
        }))
    };
    let calls: &'static Cell<u64> = Box::leak(Box::new(Cell::new(0)));
    let mut l = Ledger::new();
    let o = 跑(有欠, true, 端口(calls), &acts, &mut l).unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(o.violations.len(), 1);
    assert_eq!(执行.get(), 0);
    let o = 跑(&不欠, true, 端口(calls), &acts, &mut l).unwrap_or_else(|e| panic!("{e}"));
    assert!(o.violations.is_empty());
    assert_eq!(执行.get(), 1, "去掉欠账后执行一次");
    assert_eq!(
        种类(&l),
        [
            "Intent",
            "Violation",
            "Withheld(violation)",
            "Intent",
            "Effect"
        ]
    );
    第三趟结果未知(&不欠, &l, &执行, move || 端口(calls));
}

/// W-6：守卫下账本停在意向（之后既无 Withheld 也无 Effect，进程被杀）：续跑 unknown_outcome、不执行（原有行为保持）
#[test]
fn w6_只有意向没有结果仍是结果未知() {
    let src = r#"budget {calls: 4, cost: 0, depth: 16};
let ok = handle(cut(judge(state(mat("程序自己写的材料")), test("该发吗", "k"))), {act: fn() { true }, ignore: fn() { false }, unsure: fn(u) { consume(u, "drop"); false }});
let r = if ok { do("发邮件", [], 0) } else { "不发" };
{r: r}
"#;
    let 执行 = Rc::new(Cell::new(0));
    let acts = 动作表(&执行);
    let calls = Cell::new(0);
    let mut l = Ledger::new();
    跑(src, true, 放行端口(&calls), &acts, &mut l).unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(执行.get(), 1);
    assert_eq!(种类(&l), ["Intent", "Effect"]);
    第三趟结果未知(src, &l, &执行, 静态放行端口);
}

/// W-8（复核 P6）：同一函数不同实参的两次调用各丢一笔：两笔记号不同，主人都以 `判#` 开头；审计重放记号相同
#[test]
fn w8_同一函数不同实参两笔记号不同() {
    let src = r#"budget {calls: 4, cost: 0, depth: 16};
fn 判(t) { let e = cut(judge(state(mat(t)), test("行吗", "k"))); 1 }
let a = 判("甲");
let b = 判("乙");
[a, b]
"#;
    let calls = Cell::new(0);
    let mut l = Ledger::new();
    let o = 跑(src, true, 并列端口(&calls), &ActionRegistry::new(), &mut l)
        .unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(o.violations.len(), 2);
    let (x, y) = (&o.violations[0], &o.violations[1]);
    assert!(x.mark.owner.starts_with("判#") && y.mark.owner.starts_with("判#"));
    assert_ne!(x.mark.owner, y.mark.owner);
    assert_ne!(x.token, y.token, "两笔记号不同");
    let marks = 违规条(&l);
    assert_eq!(marks.len(), 2);
    assert_ne!(marks[0], marks[1]);
    let c2 = Cell::new(0);
    let o2 = jpp::run_replay(
        &守卫程序(src),
        并列端口(&c2),
        &库(),
        &ActionRegistry::new(),
        &mut l,
    )
    .unwrap_or_else(|e| panic!("重放：{}", e.render()));
    let t1: Vec<&String> = o.violations.iter().map(|v| &v.token).collect();
    let t2: Vec<&String> = o2.violations.iter().map(|v| &v.token).collect();
    assert_eq!(t1, t2, "审计重放记号相同");
}
