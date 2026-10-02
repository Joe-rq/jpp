//! Z0613（复核 `复核-Z0593.md` 第五节；过程记录 `工程-Z0613-提升下标越界.md`）：同一份材料上两道判断、后面跟六条以上语句，
//! 运行期曾在提升（`lift_followers`）里下标越界 panic。根因：伴随题序言 `lib/unsure.jpp` 另行降级，节点号与用户程序重叠，
//! 求值序言时拿用户程序的提升计划（下标是用户程序块的）去索引序言的块。修法：求值序言用空计划；提升核对计划与块对得上，
//! 对不上报 `E-rt-plan`。

use std::cell::RefCell;

use jpp::effects::{CalibStore, FnPort, JudgeResult, Ports};
use jpp::interp::CompanionMode;
use jpp::ledger::Ledger;
use jpp::value::{Answer, Value};
use jpp::{ActionRegistry, EntryArgs, Session, lower, syntax::parse};

/// 复核第五节的程序：两道同材料判断，后跟 `n` 条 `let`；`同题` 为真时两道题面相同
fn 程序(n: usize, 同题: bool) -> jpp::Program {
    let 第二题 = if 同题 { "未决吗" } else { "好吗" };
    let mut src = format!(
        "budget {{calls: 8, cost: 1, depth: 16}};\nlet x = judge(state(mat(\"甲\")), test(\"未决吗\", \"k\"));\nlet y = judge(state(mat(\"甲\")), test(\"{第二题}\", \"k\"));\n"
    );
    for i in 1..=n {
        src.push_str(&format!("let k{i} = {i};\n"));
    }
    src.push_str("1\n");
    lower(&parse(&src).expect("解析")).expect("lower")
}

fn 端口(calls: &RefCell<u64>) -> Ports<'_> {
    Ports::new().with(FnPort::judge("fixed-0", move |_s, qs| {
        *calls.borrow_mut() += 1;
        Ok(JudgeResult {
            answers: qs.iter().map(|_| Answer::Noul(0.5)).collect(),
            tokens: 0,
            cost: 0.0,
            mode_share: vec![],
            perms: vec![],
            confidence: vec![],
        })
    }))
}

/// 经 `Session` 跑（它总装序言）；返回判断端口被调的次数
fn 跑(n: usize, 同题: bool, 伴随: CompanionMode) -> u64 {
    let p = 程序(n, 同题);
    let calls = RefCell::new(0);
    let calib = CalibStore::new();
    let actions = ActionRegistry::new();
    let mut l = Ledger::new();
    let o = Session::new(端口(&calls), &calib, &actions)
        .with_companions(伴随)
        .run(&p, &EntryArgs::default(), &mut l)
        .unwrap_or_else(|e| panic!("n={n} 同题={同题} {伴随:?}：{}", e.render()));
    assert!(
        matches!(o.value, Some(Value::Int(1, _))),
        "n={n} 同题={同题} {伴随:?}：{:?}",
        o.value
    );
    calls.into_inner()
}

/// 4、5、6、10 条语句 × 两题同与不同 × 伴随题关与开：都跑完、返回 1，不 panic
#[test]
fn 两道同材料判断后跟多条语句_不崩() {
    for 伴随 in [CompanionMode::Off, CompanionMode::Same] {
        for 同题 in [true, false] {
            let mut 次数 = vec![];
            for n in [4, 5, 6, 10] {
                次数.push((n, 跑(n, 同题, 伴随)));
            }
            // 调用次数与后面跟几条语句无关
            assert!(
                次数.iter().all(|(_, c)| *c == 次数[0].1),
                "同题={同题} {伴随:?}：{次数:?}"
            );
            // 伴随题关着时，提升与融合对用户程序照常生效：两道同状态的判断一次发出
            if 伴随 == CompanionMode::Off {
                assert!(次数.iter().all(|(_, c)| *c == 1), "同题={同题}：{次数:?}");
            }
        }
    }
}

/// 防御：提升计划与正在求值的块对不上（这里拿后跟 6 条语句的程序算出的计划去跑只有两道判断的程序，节点号前缀相同）
/// 时报 `E-rt-plan`，不 panic，也不静默跳过
#[test]
fn 提升计划与块对不上_报_e_rt_plan() {
    let 长 = 程序(6, false);
    let 短 = 程序(0, false);
    let plan = jpp::interp::plan_with(
        &长,
        &jpp::interp::Passes::default(),
        None,
        &jpp::interp::PlanCtx::unknown(),
    );
    let calls = RefCell::new(0);
    let calib = CalibStore::new();
    let actions = ActionRegistry::new();
    let mut l = Ledger::new();
    let r = jpp_runtime::Interp::new(端口(&calls), &mut l, &calib, &actions, 短.budget.clone())
        .run(&短, plan, &jpp_plan::Hooks);
    let e = r.expect_err("计划与块对不上应报错");
    assert_eq!(e.rule.as_deref(), Some("E-rt-plan"), "{}", e.render());
}
