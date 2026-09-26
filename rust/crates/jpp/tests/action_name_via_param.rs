//! B179 (b)（步 24e-4）：`do` 的动作名实参是形参名时，沿「函数 → 词法外层函数」链追一层——若该
//! 形参所属的**具名**函数在全程序所有调用点上都给了同一个字面动作名，视为该字面量；调用点实参
//! 不是字面量（哪怕它本身又是别的函数的形参）当场停，不递归。`do_sites`、`E-action-no-sandbox`、
//! J-11、J-08 静态子面、CLI `irreversible_action_in` 同一来源（CLI 的 `--ledger-out` 行为见
//! `tests/e2e/ledger_required.rs`）。
//!
//! 依据：B179 (b)（`地基/附注/2026-09-26-批7裁定.md` §十二）；预注册
//! `地基/过程记录/工程-步24e-4.md`。测试文件路径与 `21` 步 24e-4 原文写的
//! `crates/jpp-check/tests/action_name_via_param.rs` 有出入——`jpp-check` 至今没有独立的
//! `tests/` 目录，本仓库对检查器诊断与 `do_sites` 的集成测试一贯放在 `jpp` 门面 crate 下（如
//! `b153_stat.rs`、`bypass_j11_unregistered_action.rs`），这里沿用既有约定，不新开先例。

use jpp::check::{ActionFacts, ActionTable, check_with_calib_actions, do_sites};
use jpp::effects::CalibStore;
use jpp::{lower, syntax::parse};

fn compile(src: &str) -> jpp::Program {
    // 本文件拿沙箱诊断当探针看追名结果：B187（批 9）起 `E-action-no-sandbox` 退役，同一规则报 `W-action-no-sandbox`
    lower(&parse(src).expect("解析")).expect("降级")
}

/// `danger_action`：可逆但无沙箱——`E-action-no-sandbox` 无条件报，不需要另外搭 J-08 的守卫场景。
fn 动作表() -> ActionTable {
    let mut t = ActionTable::default();
    t.actions.insert(
        "danger_action".into(),
        ActionFacts {
            reversible: true,
            output_untrusted: false,
            no_sandbox: true,
        },
    );
    t
}

/// (a) 单调用点字面量追到：`do_sites` 解析出字面动作名；`E-action-no-sandbox` 落在调用点
/// （`f("danger_action")` 那一行），报文写「经 `f` 的形参 `action` 传入」。
#[test]
fn a_单调用点追到() {
    let src = "budget {calls: 2, cost: 0};\n\
               fn f(action) { fn(x) { do(action, [x], 0) } }\n\
               let g = f(\"danger_action\");\n\
               g(1)\n";
    let p = compile(src);
    assert_eq!(do_sites(&p), vec![Some("danger_action".to_string())]);
    let r = check_with_calib_actions(&p, &CalibStore::new(), &动作表());
    let d = r
        .find("W-action-no-sandbox")
        .unwrap_or_else(|| panic!("{}", r.render()));
    assert!(
        d.message.contains("经 `f` 的形参 `action` 传入"),
        "{}",
        d.message
    );
}

/// (b) 两个调用点给了不同字面量：追不到，`do_sites` 与 `E-action-no-sandbox` 都不报。
#[test]
fn b_两个调用点不同字面量不追() {
    let src = "budget {calls: 2, cost: 0};\n\
               fn f(action) { fn(x) { do(action, [x], 0) } }\n\
               let g1 = f(\"danger_action\");\n\
               let g2 = f(\"record_check\");\n\
               {a: g1(1), b: g2(1)}\n";
    let p = compile(src);
    assert_eq!(do_sites(&p), vec![None]);
    let r = check_with_calib_actions(&p, &CalibStore::new(), &动作表());
    assert!(r.find("W-action-no-sandbox").is_none(), "{}", r.render());
}

/// (c) 形参再传给另一函数（两层）：`wrapper` 的调用点是字面量，但 `f` 的调用点实参是 `wrapper`
/// 自己的形参、不是字面量——一层为止，不递归到 `wrapper` 的调用点。
#[test]
fn c_两层转发不追() {
    let src = "budget {calls: 2, cost: 0};\n\
               fn f(action) { fn(x) { do(action, [x], 0) } }\n\
               fn wrapper(act) { f(act) }\n\
               let g = wrapper(\"danger_action\");\n\
               g(1)\n";
    let p = compile(src);
    assert_eq!(do_sites(&p), vec![None]);
    let r = check_with_calib_actions(&p, &CalibStore::new(), &动作表());
    assert!(r.find("W-action-no-sandbox").is_none(), "{}", r.render());
}

/// (d) `ground`（`lib/compose/ground.jpp` 同形结构：动作名是 `ground` 自己的形参，`do` 站点在
/// `ground` 返回的匿名闭包里）：`do_sites` 对 `ground("write_json", …)` 解析出 `write_json`。
#[test]
fn d_ground同形结构追到() {
    let src = "budget {calls: 2, cost: 0};\n\
               fn ground(action, args_of, render) {\n\
               \x20\x20fn(cand) {\n\
               \x20\x20\x20\x20let out = do(action, args_of(cand), 0);\n\
               \x20\x20\x20\x20if is_fail(out) { out } else { render(cand, out) }\n\
               \x20\x20}\n\
               }\n\
               let runner = ground(\"write_json\", fn(c) { [c, 1] }, fn(c, o) { o });\n\
               runner(1)\n";
    let p = compile(src);
    assert_eq!(do_sites(&p), vec![Some("write_json".to_string())]);
}
