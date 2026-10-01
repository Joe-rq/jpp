//! C-3（主控 Z0171）：预算随调用传递、只收紧不放宽；重跑与串联的花费记整场，深度接着算。
//!
//! 两条证明：子调用与重跑超整场上限即停（P1–P5）；只收紧（P6–P12）。另钉住不带余额时不变（P13）。
//! 今天没有事件触发的重跑，**测试里用宿主循环代替事件来源**（每轮把上一轮交回的余额交给下一轮）；这是测试夹具，
//! 不是语言里的常驻机制（常驻程序等骨架最终裁定）。
//!
//! 依据：裁定纸面阶段第五节「共 1」（`地基/规划/骨架比较/裁定-纸面阶段-2026-09-29.md`:87）；主控 Z0235 读法
//! （每轮上限 = min(本轮声明, 整场余额)）；B93（停发语义）、B35（重放停在同一处）；预注册
//! `地基/过程记录/工程-C3-预算传递.md` §三（编号 P1–P13 与预注册表一一对应）。

use std::cell::Cell;

use jpp::effects::{CalibStore, FixedPorts, FnPort, JudgeResult, Ports};
use jpp::ledger::{CarryRecord, Ledger};
use jpp::value::{Answer, Value};
use jpp::{ActionRegistry, Budget, BudgetCarry, EntryArgs, Outcome, Session, TaintOut};
use jpp::{lower, syntax::parse};
use serde_json::json;

/// 是非题恒 0.9（没有线时按判断器的回答走 → act）；每次调用计数，可选每次睡一会儿（时延预算要有花费）
fn 端口<'a>(calls: &'a Cell<u64>, 睡毫秒: u64) -> Ports<'a> {
    Ports::new().with(FnPort::judge("fixed-0", move |_s, qs| {
        calls.set(calls.get() + 1);
        if 睡毫秒 > 0 {
            std::thread::sleep(std::time::Duration::from_millis(睡毫秒));
        }
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

fn 程序(src: &str) -> jpp::Program {
    lower(&parse(src).expect("解析")).expect("lower")
}

/// `R(k, C)`：k 道是非题各在不同材料上、各自 `cut`（每题一次调用），未决随返回值交出。`额外` 拼进 budget 声明。
fn r(k: usize, calls: u64, 额外: &str) -> String {
    let 题: Vec<String> = (0..k).map(|i| format!("判(\"材料{i}\")")).collect();
    format!(
        "budget {{calls: {calls}, cost: 0, depth: 16{额外}}};\n\
         fn 判(t) {{\n    let e = cut(judge(state(mat(t)), test(\"行吗\", \"k\")));\n    {{k: exit_kind(e), e: e}}\n}}\n\
         let rs = [{}];\n\
         {{v: map(rs, fn(r) {{ r.k }}), pending: map(rs, fn(r) {{ r.e }})}}\n",
        题.join(", ")
    )
}

fn 整场(calls: u64, cost: f64) -> Budget {
    Budget {
        calls,
        cost,
        depth: None,
        escalate: None,
        unsure: None,
        absent: None,
        latency_p95: None,
    }
}

/// 跑一轮：给了余额就带上
fn 跑(
    src: &str,
    carry: Option<BudgetCarry>,
    ledger: &mut Ledger,
    calls: &Cell<u64>,
    睡毫秒: u64,
    acts: &ActionRegistry,
) -> Outcome {
    let calib = CalibStore::new();
    Session::new(端口(calls, 睡毫秒), &calib, acts)
        // 数调用与花费，固定关伴随题（主控 2026-09-30：第二类）
        .with_companions(jpp::interp::CompanionMode::Off)
        .with_carry(carry)
        .run(&程序(src), &EntryArgs::default(), ledger)
        .unwrap_or_else(|e| panic!("应当跑完：{}", e.render()))
}

fn 跑判(src: &str, carry: Option<BudgetCarry>) -> (Outcome, u64, Ledger) {
    let calls = Cell::new(0);
    let mut l = Ledger::new();
    let o = 跑(src, carry, &mut l, &calls, 0, &ActionRegistry::new());
    (o, calls.get(), l)
}

fn 出口(o: &Outcome) -> Vec<String> {
    o.value_json()["v"]
        .as_array()
        .expect("v 是列表")
        .iter()
        .map(|x| x.as_str().unwrap().to_string())
        .collect()
}

fn 数(v: &[String], k: &str) -> usize {
    v.iter().filter(|x| x.as_str() == k).count()
}

fn w_budget(o: &Outcome) -> usize {
    o.trace
        .warnings
        .iter()
        .filter(|w| w.starts_with("W-budget"))
        .count()
}

fn 交回(o: &Outcome) -> BudgetCarry {
    o.carry.clone().expect("带了余额就有交回余额")
}

// ---------- 证明一：子调用与重跑超整场上限即停 ----------

/// P1：子程序被父余额截住（calls）
#[test]
fn p1_子程序被父余额截住_calls() {
    let 根 = BudgetCarry::session(&整场(10, 0.0));
    let (父, n父, _) = 跑判(&r(6, 10, ""), Some(根));
    assert_eq!(n父, 6);
    assert_eq!(数(&出口(&父), "act"), 6);
    let c = 交回(&父);
    assert_eq!((c.calls(), c.depth_at()), (4, 1));

    let (子, n子, _) = 跑判(&r(6, 100, ""), Some(c));
    assert_eq!(n子, 4, "子声明 100，按父余额 4 截");
    assert_eq!(
        出口(&子),
        [
            "act",
            "act",
            "act",
            "act",
            "unsure(budget)",
            "unsure(budget)"
        ]
    );
    let b = 子.budget.as_ref().expect("报预算停机");
    assert!(b.exhausted && b.cause.is_none());
    assert_eq!(b.unsent, 2);
    assert_eq!(w_budget(&子), 1);
    assert!(子.pending.is_empty(), "停发不挂起");
    let c = 交回(&子);
    assert_eq!((c.calls(), c.depth_at()), (0, 2));
    assert_eq!(n父 + n子, 10, "父子合计正好是整场上限");
}

/// P2：子程序被父余额截住（cost，经 `do`；单价 0.25 取二进制精确的数）
#[test]
fn p2_子程序被父余额截住_cost() {
    let mut acts = ActionRegistry::new();
    acts.register("记", 0.25, true, TaintOut::Trusted, |_| {
        Ok(Value::text("好"))
    });
    let src = |n: usize, cost: f64| {
        let 做: Vec<String> = (0..n).map(|i| format!("do(\"记\", [{i}], 0)")).collect();
        format!(
            "budget {{calls: 100, cost: {cost}, depth: 16}};\nlet r = [{}];\nmap(r, fn(x) {{ is_fail(x) }})\n",
            做.join(", ")
        )
    };
    let calls = Cell::new(0);
    let 根 = BudgetCarry::session(&整场(100, 2.0));
    let 父 = 跑(&src(3, 2.0), Some(根), &mut Ledger::new(), &calls, 0, &acts);
    assert_eq!(父.cost.usd, 0.75);
    let c = 交回(&父);
    assert_eq!(c.cost(), 1.25);

    let 子 = 跑(&src(6, 10.0), Some(c), &mut Ledger::new(), &calls, 0, &acts);
    assert_eq!(
        子.value_json(),
        json!([false, false, false, false, false, true]),
        "第 6 次超父余额，产出失败值"
    );
    assert_eq!(子.cost.usd, 1.25);
    assert_eq!(交回(&子).cost(), 0.0);
    assert_eq!(父.cost.usd + 子.cost.usd, 2.0);
}

/// 宿主循环代替事件来源：每轮新账本、把上一轮交回的余额交给下一轮。返回每轮（出口、判断器调用数、结果、账本）
fn 连跑(srcs: &[String], 根: BudgetCarry) -> Vec<(Outcome, u64, Ledger)> {
    let mut c = Some(根);
    let mut out = vec![];
    for s in srcs {
        let (o, n, l) = 跑判(s, c.clone());
        c = o.carry.clone();
        out.push((o, n, l));
    }
    out
}

/// 跑一轮，出错也交回（错误报文、判断器调用数、账本）
fn 试跑(src: &str, carry: Option<BudgetCarry>) -> (Result<Outcome, String>, u64, Ledger) {
    let calls = Cell::new(0);
    let mut l = Ledger::new();
    let calib = CalibStore::new();
    let r = Session::new(端口(&calls, 0), &calib, &ActionRegistry::new())
        // 数调用与花费，固定关伴随题（主控 2026-09-30：第二类）
        .with_companions(jpp::interp::CompanionMode::Off)
        .with_carry(carry)
        .run(&程序(src), &EntryArgs::default(), &mut l)
        .map_err(|e| e.render());
    (r, calls.get(), l)
}

/// P3′：重跑超整场即停（附注：余额 0 的第 4 轮改为计划期 `E-budget-plan`，余额原样交回）
#[test]
fn p3_重跑超整场即停() {
    let 轮 = 连跑(&vec![r(5, 5, ""); 3], BudgetCarry::session(&整场(12, 0.0)));
    let 发: Vec<u64> = 轮.iter().map(|x| x.1).collect();
    assert_eq!(发, [5, 5, 2]);
    let 未决: Vec<usize> = 轮
        .iter()
        .map(|x| 数(&出口(&x.0), "unsure(budget)"))
        .collect();
    assert_eq!(未决, [0, 0, 3]);
    let w: Vec<usize> = 轮.iter().map(|x| w_budget(&x.0)).collect();
    assert_eq!(w, [0, 0, 1]);
    let 余: Vec<u64> = 轮.iter().map(|x| 交回(&x.0).calls()).collect();
    assert_eq!(余, [7, 2, 0]);
    let 进门深度: Vec<u32> = 轮
        .iter()
        .map(|x| x.2.header.as_ref().unwrap().carry.as_ref().unwrap().hop)
        .collect();
    assert_eq!(进门深度, [0, 1, 2]);
    assert!(轮.iter().all(|x| x.0.pending.is_empty()), "都照常返回");
    // 第 4 轮：余额 0，可靠下界 1 次，计划期就拒
    let 第四 = 交回(&轮[2].0);
    let (r4, n4, l4) = 试跑(&r(5, 5, ""), Some(第四.clone()));
    let e = r4.expect_err("余额不够必经下界");
    assert!(e.contains("E-budget-plan"), "{e}");
    assert_eq!(n4, 0);
    let 余4 = 第四.after_round(&l4);
    assert_eq!(余4, 第四, "没开跑，余额原样交回");
    assert_eq!((余4.calls(), 余4.depth_at()), (0, 3));
    assert_eq!(发.iter().sum::<u64>(), 12, "合计正好是整场上限");
}

/// P4′：只凭账本重放停在同一处，交回余额与首跑逐字节相同（含时延余额）；重放取账本头的余额，不看宿主给的
#[test]
fn p4_重放停在同一处() {
    let 根 = BudgetCarry::session(&Budget {
        latency_p95: Some(100.0),
        ..整场(12, 0.0)
    });
    let acts = ActionRegistry::new();
    let calls = Cell::new(0);
    let mut c = Some(根);
    let mut 末 = None;
    for _ in 0..3 {
        let mut l = Ledger::new();
        let o = 跑(&r(5, 5, ""), c.clone(), &mut l, &calls, 2, &acts);
        c = o.carry.clone();
        末 = Some((o, l));
    }
    let (首, 账) = 末.unwrap();
    let 头 = 账.header.as_ref().unwrap().carry.clone().unwrap();
    assert_eq!((头.calls, 头.hop), (2, 2));
    let 首余 = serde_json::to_string(交回(&首).record()).unwrap();
    assert!(
        交回(&首).latency_p95().unwrap() < 100.0,
        "时延余额扣过：{首余}"
    );
    for 宿主给 in [None, Some(BudgetCarry::session(&整场(100, 0.0)))] {
        let calls = Cell::new(0);
        let mut l = 账.clone();
        let calib = CalibStore::new();
        let 重 = Session::new(端口(&calls, 0), &calib, &ActionRegistry::new())
            // 数调用与花费，固定关伴随题（主控 2026-09-30：第二类）
            .with_companions(jpp::interp::CompanionMode::Off)
            .with_carry(宿主给)
            .replay(&程序(&r(5, 5, "")), &EntryArgs::default(), &mut l)
            .unwrap_or_else(|e| panic!("重放应当跑完：{}", e.render()));
        assert_eq!(calls.get(), 0, "重放不发调用");
        assert_eq!(出口(&重), 出口(&首));
        assert_eq!(
            出口(&重),
            [
                "act",
                "act",
                "unsure(budget)",
                "unsure(budget)",
                "unsure(budget)"
            ]
        );
        assert_eq!(重.budget.as_ref().unwrap().unsent, 3);
        assert_eq!(w_budget(&重), 1);
        assert_eq!(
            serde_json::to_string(交回(&重).record()).unwrap(),
            首余,
            "重放的交回余额与首跑逐字节相同"
        );
        assert_eq!((交回(&重).calls(), 交回(&重).depth_at()), (0, 3));
    }
}

/// P5′（G4 步 37，预注册 D-1、D-2；取代 C-3 的「整轮不开跑」）：跳数用完只停发，程序照常求值——判断一道不发、逐题
/// `Unsure(depth)`，每题一条「未问」、整趟一条「停下」、一条 `W-budget`；交回 `hop` 照常加一；审计重放零调用、不重复写
#[test]
fn p5_跳数用完只停发() {
    use jpp::ledger::{Entry, StopCause};
    let 根 = BudgetCarry::session(&Budget {
        depth: Some(2),
        ..整场(100, 0.0)
    });
    let 轮 = 连跑(&vec![r(5, 5, ""); 3], 根);
    let 发: Vec<u64> = 轮.iter().map(|x| x.1).collect();
    assert_eq!(发, [5, 5, 0]);
    let (停, _, l) = &轮[2];
    assert_eq!(
        出口(停),
        vec!["unsure(depth)"; 5],
        "照常求值，逐题 Unsure(depth)"
    );
    let b = 停.budget.as_ref().expect("标停");
    assert_eq!(b.cause.as_deref(), Some("depth"));
    assert_eq!(w_budget(停), 1);
    let 未问 = l
        .entries
        .iter()
        .filter(|e| matches!(e, Entry::Unasked { reason, .. } if *reason == StopCause::Depth))
        .count();
    let 停下 = l
        .entries
        .iter()
        .filter(|e| matches!(e, Entry::Stop { cause, .. } if *cause == StopCause::Depth))
        .count();
    assert_eq!((未问, 停下), (5, 1));
    assert!(
        !l.entries.iter().any(|e| matches!(e, Entry::Absent { .. })),
        "深度停发不记缺席"
    );
    let c = 交回(停);
    assert_eq!(
        (c.calls(), c.depth_at(), c.depth_cap(), c.record().round),
        (90, 3, 2, 0),
        "交回照常 hop + 1、calls 不变（这一趟没发）"
    );
    // 审计重放：零调用、值相同、账本不重复写
    let mut l2 = l.clone();
    let n0 = l2.entries.len();
    let calls = Cell::new(0);
    let calib = CalibStore::new();
    let o2 = Session::new(端口(&calls, 0), &calib, &ActionRegistry::new())
        .with_companions(jpp::interp::CompanionMode::Off)
        .replay(&程序(&r(5, 5, "")), &EntryArgs::default(), &mut l2)
        .unwrap_or_else(|e| panic!("重放：{}", e.render()));
    assert_eq!(calls.get(), 0);
    assert_eq!(o2.value_json(), 停.value_json());
    assert_eq!(l2.entries.len(), n0, "审计重放不重复写未问与停下");
}

// ---------- 证明二：只收紧不放宽 ----------

/// `f(3)` 递归到第 4 层（`f(3)`、`f(2)`、`f(1)`、`f(0)` 各一层）
fn 递归(depth: u32) -> String {
    format!(
        "budget {{calls: 0, cost: 0, depth: {depth}}};\nfn f(n) {{ if n > 0 {{ f(n - 1) }} else {{ 0 }} }}\nf(3)\n"
    )
}

fn 深度余额(at: u32, cap: u32) -> BudgetCarry {
    BudgetCarry::from_record(CarryRecord {
        calls: 0,
        cost: 0.0,
        latency_p95: None,
        escalate: 0,
        hop: at,
        round: 0,
        depth_cap: cap,
    })
}

fn 跑深(src: &str, carry: Option<BudgetCarry>) -> Result<Outcome, String> {
    let calib = CalibStore::new();
    let calls = Cell::new(0);
    Session::new(端口(&calls, 0), &calib, &ActionRegistry::new())
        // 数调用与花费，固定关伴随题（主控 2026-09-30：第二类）
        .with_companions(jpp::interp::CompanionMode::Off)
        .with_carry(carry)
        .run(&程序(src), &EntryArgs::default(), &mut Ledger::new())
        .map_err(|e| e.render())
}

/// P6：一轮之内递归碰上游收紧的深度仍是 J-06，报文分得出上游收紧与程序声明（主控答复第 3 条 (乙)）
#[test]
fn p6_递归碰上游收紧的深度() {
    // G4（步 37，预注册 D-6）：J-06 的调用栈每趟从 0 起算，只管递归；`hop` 不再占递归的层数
    // G4b（步 37b，预注册 §三）：余额的 depth_cap 只比跨程序跳数，不再挡递归
    assert!(
        跑深(&递归(16), Some(深度余额(0, 3))).is_ok(),
        "改前报 J-06「上游收紧到 3」；改后递归 4 层只受声明 16 限"
    );
    assert!(
        跑深(&递归(16), Some(深度余额(1, 5))).is_ok(),
        "递归 4 层，上限 5"
    );
    assert!(
        跑深(&递归(16), Some(深度余额(2, 5))).is_ok(),
        "改前从第 2 层起超 5 报 J-06；改后递归只算自己的 4 层"
    );
    let e = 跑深(&递归(3), None).expect_err("程序声明 3");
    assert!(e.contains("J-06") && e.contains("提高 budget.depth"), "{e}");
    assert!(!e.contains("上游"), "{e}");
    assert!(
        跑深(&递归(4), Some(深度余额(1, 100))).is_ok(),
        "改前从第 1 层起超声明 4 报 J-06；改后递归 4 层不超声明 4"
    );
}

/// P7：只收紧 calls
#[test]
fn p7_只收紧_calls() {
    let (_, n, _) = 跑判(&r(5, 2, ""), Some(BudgetCarry::session(&整场(10, 0.0))));
    assert_eq!(n, 2, "声明 2 < 余额 10：按声明");
    let (_, n, _) = 跑判(&r(5, 100, ""), Some(BudgetCarry::session(&整场(3, 0.0))));
    assert_eq!(n, 3, "声明 100 > 余额 3：按余额");
}

/// P8：只收紧 latency（时延预算是判断调用的累计秒数，不是墙钟截止时间）
#[test]
fn p8_只收紧_latency() {
    let 余额 = |s: f64| {
        BudgetCarry::session(&Budget {
            latency_p95: Some(s),
            ..整场(100, 0.0)
        })
    };
    let calls = Cell::new(0);
    let acts = ActionRegistry::new();
    let o = 跑(
        &r(5, 100, ", latency_p95: 100"),
        Some(余额(0.0)),
        &mut Ledger::new(),
        &calls,
        2,
        &acts,
    );
    // 预注册写的是「第一题 act」，没中：B32 现行规则里，把累计时延推过预算的那一次调用本身也转
    // `Unsure(latency)`（答案记账、不采信，`flush.rs` 超时站点一段）。按余额 0 生效这一点不变：只发出第一次
    assert_eq!(calls.get(), 1, "只发出第一次，之后停发");
    assert_eq!(
        数(&出口(&o), "unsure(latency)"),
        5,
        "声明 100 > 余额 0：按余额"
    );
    calls.set(0);
    let o = 跑(
        &r(5, 100, ""),
        Some(余额(100.0)),
        &mut Ledger::new(),
        &calls,
        2,
        &acts,
    );
    assert_eq!(数(&出口(&o), "act"), 5, "声明不写、余额 100：全发");
}

/// P9：只收紧 escalate
#[test]
fn p9_只收紧_escalate() {
    let src = |e: u64| {
        format!(
            "budget {{calls: 10, cost: 0, depth: 8, escalate: {e}}};\n\
             fn 问(t) {{ handle(ask(state(mat(t)), test(\"行吗\", \"k\")), {{act: fn() {{ 1 }}, ignore: fn() {{ 0 }}, unsure: fn(u) {{ consume(u, \"drop\"); -1 }}}}) }}\n\
             [问(\"甲\"), 问(\"乙\")]\n"
        )
    };
    let mut fp = FixedPorts::new();
    for t in ["甲", "乙"] {
        let s = jpp::value::State::new(
            vec![jpp::value::Mat::literal(json!(t))],
            vec![],
            vec![],
            vec![],
            false,
        );
        fp.fix_ask(
            &s,
            &jpp::value::Question::new(jpp::value::Op::Test, "行吗", "k", vec![]),
            Some(Answer::Noul(0.9)),
        );
    }
    let calib = CalibStore::new();
    let acts = ActionRegistry::new();
    let 余额 = |e: u64| {
        BudgetCarry::session(&Budget {
            escalate: Some(e),
            ..整场(100, 0.0)
        })
    };
    for (声明, 余) in [(5, 1), (1, 5)] {
        let o = Session::new(fp.ports(), &calib, &acts)
            // 数调用与花费，固定关伴随题（主控 2026-09-30：第二类）
            .with_companions(jpp::interp::CompanionMode::Off)
            .with_carry(Some(余额(余)))
            .run(&程序(&src(声明)), &EntryArgs::default(), &mut Ledger::new())
            .unwrap_or_else(|e| panic!("{}", e.render()));
        assert_eq!(o.cost.asks, 1, "声明 {声明}、余额 {余}：只问得了一次");
        assert_eq!(
            o.pending.first().map(|p| p.cause.as_str()),
            Some("budget.escalate"),
            "第二次挂起"
        );
    }
}

/// P10：三代链，余额只减不增，深度逐代 +1
#[test]
fn p10_三代链() {
    let 根 = BudgetCarry::session(&整场(20, 0.0));
    let (甲, _, _) = 跑判(&r(6, 100, ""), Some(根.clone()));
    let c甲 = 交回(&甲);
    let (乙, _, _) = 跑判(&r(5, 100, ""), Some(c甲.clone()));
    let c乙 = 交回(&乙);
    assert_eq!((根.calls(), c甲.calls(), c乙.calls()), (20, 14, 9));
    let (丙, n丙, 账丙) = 跑判(&r(3, 100, ""), Some(c乙.clone()));
    assert_eq!(n丙, 3);
    assert_eq!(
        账丙.header.as_ref().unwrap().carry.as_ref().unwrap().calls,
        9
    );
    assert_eq!(
        (
            根.depth_at(),
            c甲.depth_at(),
            c乙.depth_at(),
            交回(&丙).depth_at()
        ),
        (0, 1, 2, 3)
    );
}

/// P11：接口单调——固定种子的伪随机花费 200 步，每项只减不增、不为负，深度每步 +1、上限不变
#[test]
fn p11_接口单调() {
    let mut c = BudgetCarry::session(&Budget {
        depth: Some(1000),
        escalate: Some(50),
        latency_p95: Some(30.0),
        ..整场(500, 40.0)
    });
    let mut x: u64 = 0x9e37_79b9_7f4a_7c15;
    let mut 下 = || {
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        x
    };
    for _ in 0..200 {
        let n = c.after_spending(
            下() % 7,
            (下() % 1000) as f64 / 400.0,
            (下() % 1000) as f64 / 300.0,
            下() % 2,
        );
        assert!(n.calls() <= c.calls() && n.cost() <= c.cost() && n.escalate() <= c.escalate());
        assert!(n.latency_p95().unwrap() <= c.latency_p95().unwrap());
        assert!(n.cost() >= 0.0 && n.latency_p95().unwrap() >= 0.0);
        assert_eq!((n.depth_at(), n.depth_cap()), (c.depth_at() + 1, 1000));
        c = n;
    }
}

/// P12：Z0235 读法——后一轮可以声明得比前一轮大，生效 = min(本轮声明, 整场余额)
#[test]
fn p12_后一轮可以声明得更大() {
    let 轮 = 连跑(
        &[r(10, 3, ""), r(10, 20, "")],
        BudgetCarry::session(&整场(12, 0.0)),
    );
    assert_eq!((轮[0].1, 轮[1].1), (3, 9));
    assert_eq!(数(&出口(&轮[1].0), "unsure(budget)"), 1);
}

/// P13：不带余额时不变——没有交回余额，账本编码里没有 `carry`
#[test]
fn p13_不带余额不变() {
    let (o, n, l) = 跑判(&r(3, 10, ""), None);
    assert_eq!(n, 3);
    assert!(o.carry.is_none());
    assert!(l.header.as_ref().unwrap().carry.is_none());
    assert!(l.header.as_ref().unwrap().carry_from.is_none());
    assert!(
        !l.entries
            .iter()
            .any(|e| matches!(e, jpp::ledger::Entry::Spent { .. })),
        "不带余额不写 Spent"
    );
    assert!(!l.encode().contains("carry"), "账本逐字节不带余额字段");
}

/// P15：规划器按生效预算——余额已不够必经下界，计划期就拒；不带余额照常
#[test]
fn p15_规划器按生效预算() {
    let (r0, n0, _) = 试跑(&r(5, 5, ""), Some(BudgetCarry::session(&整场(0, 0.0))));
    let e = r0.expect_err("余额 0 < 下界 1");
    assert!(e.contains("E-budget-plan") && e.contains("calls 0"), "{e}");
    assert_eq!(n0, 0);
    let (r1, n1, _) = 试跑(&r(5, 5, ""), None);
    assert!(r1.is_ok());
    assert_eq!(n1, 5);
}

/// 先发 3 次判断，再递归撞 J-06（声明 depth 4）
const 失败轮: &str = "budget {calls: 100, cost: 0, depth: 4};
fn 判(t) {
    let e = cut(judge(state(mat(t)), test(\"行吗\", \"k\")));
    {k: exit_kind(e), e: e}
}
fn f(n) { if n > 0 { f(n - 1) } else { 0 } }
let rs = [判(\"甲\"), 判(\"乙\"), 判(\"丙\")];
let ks = map(rs, fn(r) { r.k });
let z = f(10);
{v: ks, z: z, pending: map(rs, fn(r) { r.e })}
";

/// P16：失败的一轮从账本扣，绝不因失败放宽
#[test]
fn p16_失败的一轮从账本扣() {
    let 根 = BudgetCarry::session(&整场(10, 0.0));
    let (r, n, l) = 试跑(失败轮, Some(根.clone()));
    let e = r.expect_err("递归撞 J-06");
    assert!(e.contains("J-06"), "{e}");
    assert_eq!(n, 3);
    let 花: Vec<_> = l
        .entries
        .iter()
        .filter_map(|e| match e {
            jpp::ledger::Entry::Spent { calls, started, .. } => Some((*calls, *started)),
            _ => None,
        })
        .collect();
    assert_eq!(花, [(3, true)]);
    let c = 根.after_round(&l);
    assert_eq!((c.calls(), c.depth_at()), (7, 1));
}

/// P17：开跑了却没有 `Spent`（例如进程中途被杀）：算不出花了多少就不再给
#[test]
fn p17_没有花费记录就不再给() {
    let 根 = BudgetCarry::session(&Budget {
        escalate: Some(3),
        latency_p95: Some(5.0),
        ..整场(10, 1.0)
    });
    let (_, _, mut l) = 试跑(&r(3, 10, ""), Some(根.clone()));
    l.entries
        .retain(|e| !matches!(e, jpp::ledger::Entry::Spent { .. }));
    let c = 根.after_round(&l);
    assert_eq!(
        (
            c.calls(),
            c.cost(),
            c.latency_p95(),
            c.escalate(),
            c.depth_at()
        ),
        (0, 0.0, Some(0.0), 0, 1)
    );
}
