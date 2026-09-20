//! 检查器的失败用例：每条都断言规则号与出错位置。
//!
//! 只收静态确定的那一半。运行期那一半（J-02 禁自指、J-05 的 `consumed` 标记、J-07 预算）由
//! `interp.rs` 在执行时把关，这里不重复。

mod common;

use common::*;
use jpp_core::ast::{Span, Statement};
use jpp_core::check::Severity;
use jpp_core::{Type, check};

/// `program` 在这些测试里常被局部变量盖住，用别名再取一次
use common::program as build;

fn let_span(s: &Statement) -> Span {
    match s {
        Statement::Let { span, .. } => *span,
        Statement::Function { span, .. } => *span,
        Statement::Expression(e) => e.span,
    }
}

/// J-01：读数不是材料，不能进状态槽。
#[test]
fn 读数不能进状态槽() {
    let reading = name("r");
    let at = reading.span;
    let program = program(
        Some(budget(1, 8)),
        vec![
            bind("q", call("test", vec![text("这个对吗？"), text("k")])),
            bind("m", call("mat", vec![rec(vec![("x", int(1))])])),
            bind(
                "r",
                call("judge", vec![call("state", vec![name("m")]), name("q")]),
            ),
        ],
        call("state", vec![reading]),
    );
    let report = check(&program);
    let d = report
        .find("J-01")
        .unwrap_or_else(|| panic!("应当报 J-01：\n{}", report.render()));
    assert_eq!(d.severity, Severity::Error);
    assert_eq!(d.span, at, "出错位置指着那个读数");
    assert!(d.message.contains("修法"), "报文要带修法：{}", d.message);
}

/// J-01 的另一面：读数不可比、不可算。
#[test]
fn 读数不能做比较() {
    let left = name("r");
    let at = left.span;
    let program = program(
        Some(budget(1, 8)),
        vec![
            bind("q", call("test", vec![text("这个对吗？"), text("k")])),
            bind(
                "r",
                call(
                    "judge",
                    vec![call("state", vec![call("mat", vec![int(1)])]), name("q")],
                ),
            ),
        ],
        bin(">", left, dec(0.5)),
    );
    let report = check(&program);
    let d = report
        .find("J-01")
        .unwrap_or_else(|| panic!("应当报 J-01：\n{}", report.render()));
    assert_eq!(d.span, at);
}

/// J-05：出口绑定之后再没被提到 = 静默丢弃。
#[test]
fn 未消费的出口是错() {
    let binding = bind("e", call("unsure", vec![text("材料不够")]));
    let at = let_span(&binding);
    let program = program(Some(budget(1, 8)), vec![binding], int(0));
    let report = check(&program);
    let d = report
        .find("J-05")
        .unwrap_or_else(|| panic!("应当报 J-05：\n{}", report.render()));
    assert_eq!(d.severity, Severity::Error);
    assert_eq!(d.span, at, "出错位置指着那个绑定");
    assert!(d.message.contains("consume"), "报文要给出路：{}", d.message);
}

/// J-05：函数把出口带出去，返回类型却没提 Exit——调用者不知道要消费它。
#[test]
fn 带出出口要标返回类型() {
    let program = program(
        Some(budget(1, 8)),
        vec![func_ret(
            "peek",
            &["m", "q"],
            Some(&["judge"]),
            Type::Named("Record".into()),
            body(
                vec![bind(
                    "e",
                    call(
                        "cut",
                        vec![call(
                            "judge",
                            vec![call("state", vec![name("m")]), name("q")],
                        )],
                    ),
                )],
                name("e"),
            ),
        )],
        int(0),
    );
    let report = check(&program);
    let d = report
        .find("J-05")
        .unwrap_or_else(|| panic!("应当报 J-05：\n{}", report.render()));
    assert!(d.message.contains("Exit"), "{}", d.message);

    // 标了 -> Exit 就合法
    let ok = program_returning_exit();
    assert!(check(&ok).is_ok(), "{}", check(&ok).render());
}

fn program_returning_exit() -> jpp_core::Program {
    program(
        Some(budget(1, 8)),
        vec![func_ret(
            "peek",
            &["m", "q"],
            Some(&["judge"]),
            Type::Named("Exit".into()),
            body(
                vec![bind(
                    "e",
                    call(
                        "cut",
                        vec![call(
                            "judge",
                            vec![call("state", vec![name("m")]), name("q")],
                        )],
                    ),
                )],
                name("e"),
            ),
        )],
        int(0),
    )
}

/// J-05 的静态穷尽面：臂表缺 unsure 去向。
#[test]
fn handle缺unsure臂是错() {
    let arms = rec(vec![("act", int(1)), ("ignore", int(0))]);
    let at = arms.span;
    let program = program(
        Some(budget(1, 8)),
        vec![
            bind("q", call("test", vec![text("这个对吗？"), text("k")])),
            bind(
                "e",
                call(
                    "cut",
                    vec![call(
                        "judge",
                        vec![call("state", vec![call("mat", vec![int(1)])]), name("q")],
                    )],
                ),
            ),
        ],
        call("handle", vec![name("e"), arms]),
    );
    let report = check(&program);
    let d = report
        .find("J-05")
        .unwrap_or_else(|| panic!("应当报 J-05：\n{}", report.render()));
    assert_eq!(d.span, at);
}

/// E5：有界循环必带 bound，而且 bound 要是正整数。
#[test]
fn 循环缺bound是错() {
    let zero = int(0);
    let at = zero.span;
    let program = program(
        Some(budget(1, 8)),
        vec![],
        call(
            "loop",
            vec![
                zero,
                int(1),
                lambda(&["acc", "i"], body(vec![], name("acc"))),
            ],
        ),
    );
    let report = check(&program);
    let d = report
        .find("E5")
        .unwrap_or_else(|| panic!("应当报 E5：\n{}", report.render()));
    assert_eq!(d.severity, Severity::Error);
    assert_eq!(d.span, at);

    // 参数个数不对也是 E5（bound 整个缺了）
    let two_args = build(
        Some(budget(1, 8)),
        vec![],
        call(
            "loop",
            vec![int(1), lambda(&["acc", "i"], body(vec![], name("acc")))],
        ),
    );
    assert!(check(&two_args).find("E5").is_some());
}

/// E12：预算必填。
#[test]
fn 缺预算是错() {
    let program = program(None, vec![], int(1));
    let at = program.span;
    let report = check(&program);
    let d = report
        .find("E12")
        .unwrap_or_else(|| panic!("应当报 E12：\n{}", report.render()));
    assert_eq!(d.span, at);
    assert!(d.message.contains("budget"), "{}", d.message);
}

/// E10：程序里有 ask 而 budget 没给 escalate。
#[test]
fn ask没有升级预算是错() {
    let program = program(
        Some(budget(1, 8)),
        vec![
            bind("q", call("test", vec![text("这个对吗？"), text("k")])),
            bind(
                "e",
                call(
                    "ask",
                    vec![call("state", vec![call("mat", vec![int(1)])]), name("q")],
                ),
            ),
        ],
        call("consume", vec![name("e"), text("drop")]),
    );
    assert!(
        check(&program).find("E10").is_some(),
        "{}",
        check(&program).render()
    );
}

/// E7：`for … yield`（map / filter）的体内是纯映射，不能含 loop。
#[test]
fn 映射体内不能有循环() {
    let inner = call(
        "loop",
        vec![
            int(2),
            name("x"),
            lambda(&["a", "i"], body(vec![], name("a"))),
        ],
    );
    let at = inner.span;
    let program = program(
        Some(budget(1, 8)),
        vec![],
        call(
            "map",
            vec![
                list(vec![int(1), int(2)]),
                lambda(&["x"], body(vec![], inner)),
            ],
        ),
    );
    let report = check(&program);
    let d = report
        .find("E7")
        .unwrap_or_else(|| panic!("应当报 E7：\n{}", report.render()));
    assert_eq!(d.span, at);
}

/// J-13：循环里的常量序号会让第二轮起被当成重放。
#[test]
fn 循环里的常量序号是错() {
    let program = program(
        Some(budget(1, 8)),
        vec![],
        call(
            "loop",
            vec![
                int(3),
                int(0),
                lambda(
                    &["acc", "i"],
                    body(
                        vec![],
                        call("do", vec![text("act"), list(vec![int(1)]), int(0)]),
                    ),
                ),
            ],
        ),
    );
    assert!(
        check(&program).find("J-13").is_some(),
        "{}",
        check(&program).render()
    );

    // 序号随轮次变就没问题
    let good = program_with_varying_seq();
    assert!(check(&good).is_ok(), "{}", check(&good).render());
}

fn program_with_varying_seq() -> jpp_core::Program {
    program(
        Some(budget(1, 8)),
        vec![],
        call(
            "loop",
            vec![
                int(3),
                int(0),
                lambda(
                    &["acc", "i"],
                    body(
                        vec![],
                        call("do", vec![text("act"), list(vec![int(1)]), name("i")]),
                    ),
                ),
            ],
        ),
    )
}

/// J-03：线不可字面，calib 位只收校准记录的键。
#[test]
fn 线不可字面() {
    let literal = dec(0.8);
    let at = literal.span;
    let program = program(
        Some(budget(1, 8)),
        vec![],
        call("test", vec![text("这个对吗？"), literal]),
    );
    let report = check(&program);
    let d = report
        .find("J-03")
        .unwrap_or_else(|| panic!("应当报 J-03：\n{}", report.render()));
    assert_eq!(d.span, at);
}

/// J-14：状态的 on 槽恰一个判断对象（关系用一对）。
#[test]
fn 状态只判一个对象() {
    let program = program(
        Some(budget(1, 8)),
        vec![],
        call("state", vec![list(vec![int(1), int(2), int(3)])]),
    );
    assert!(
        check(&program).find("J-14").is_some(),
        "{}",
        check(&program).render()
    );
}

/// 效应标注要盖住实际发生的效应（core 本地码 E-effect）。
#[test]
fn 效应标注不能少() {
    let program = program(
        Some(budget(1, 8)),
        vec![func_eff(
            "peek",
            &["m", "q"],
            &[],
            body(
                vec![],
                call(
                    "cut",
                    vec![call(
                        "judge",
                        vec![call("state", vec![name("m")]), name("q")],
                    )],
                ),
            ),
        )],
        int(0),
    );
    let report = check(&program);
    let d = report
        .find("E-effect")
        .unwrap_or_else(|| panic!("应当报 E-effect：\n{}", report.render()));
    assert!(d.message.contains("judge"), "{}", d.message);

    // 被调者是参数（方法值）时静态判不了效应，不报——宁可漏也不误杀
    let higher_order = build(
        Some(budget(1, 8)),
        vec![func_eff(
            "apply",
            &["f", "x"],
            &[],
            body(vec![], call_of(name("f"), vec![name("x")])),
        )],
        int(0),
    );
    assert!(
        check(&higher_order).find("E-effect").is_none(),
        "{}",
        check(&higher_order).render()
    );
}

/// 未定义的名字（core 本地码 E-name）。递归与互相引用不算未定义。
#[test]
fn 名字要有定义() {
    let missing = name("没这个东西");
    let at = missing.span;
    let program = program(Some(budget(1, 8)), vec![], missing);
    let report = check(&program);
    let d = report
        .find("E-name")
        .unwrap_or_else(|| panic!("应当报 E-name：\n{}", report.render()));
    assert_eq!(d.span, at);

    // 自递归与后定义的互相引用都是合法的：解释器里同一个块的绑定共享环境节点
    let recursive = build(
        Some(budget(1, 8)),
        vec![
            func(
                "down",
                &["n"],
                body(
                    vec![],
                    if_(
                        bin("<=", name("n"), int(0)),
                        int(0),
                        call("up", vec![bin("-", name("n"), int(1))]),
                    ),
                ),
            ),
            func("up", &["n"], body(vec![], call("down", vec![name("n")]))),
        ],
        call("down", vec![int(3)]),
    );
    assert!(check(&recursive).is_ok(), "{}", check(&recursive).render());
}

/// 盖住内置名字只是提示，不拦程序。
#[test]
fn 盖住内置名只是提示() {
    let program = program(
        Some(budget(1, 8)),
        vec![func("has", &["r", "k"], body(vec![], boolean(true)))],
        call("has", vec![rec(vec![]), text("k")]),
    );
    let report = check(&program);
    assert!(report.is_ok(), "盖名不该拦程序：{}", report.render());
    assert!(
        report.find("W-shadow").is_some(),
        "但要提示：{}",
        report.render()
    );
}
