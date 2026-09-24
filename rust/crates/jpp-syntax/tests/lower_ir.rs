//! 降级直接产出 IR（步 12d；原 `jpp-ir/tests/ir_wellformed.rs`，步 12a）：`wellformed` 能变红
//! （每条结构不变量一个反例），`print` 稳定，站点表与节点对应，遮蔽与缺预算在降级处处理。
use jpp_ir::ir::{self, NameClass, NameTable};
use jpp_ir::key::EffectId;
use jpp_syntax::{lower, parse};

struct Names;
impl NameTable for Names {
    fn classify(&self, n: &str) -> NameClass {
        match n {
            "judge" => NameClass::Effect(EffectId::Judge),
            "state" => NameClass::State,
            "handle" => NameClass::Handle,
            "map" => NameClass::HigherOrder,
            "loop" => NameClass::Loop,
            _ => NameClass::Plain,
        }
    }
    fn slots(&self, _: EffectId) -> Vec<&'static str> {
        vec!["state", "questions"]
    }
}

fn ir_of(body: &str) -> ir::Program {
    let src = format!("budget {{calls: 1, cost: 0}};\n{body}");
    lower(&parse(&src).expect("解析"), &Names).expect("降级")
}

#[test]
fn 良构程序通过且打印稳定() {
    let ir = ir_of("let s = state(\"x\");\nmap([], fn(q) { judge(s, q) })");
    ir::wellformed(&ir, &Names).unwrap();
    assert_eq!(ir::print(&ir, None), ir::print(&ir, None));
    // 三个站点：state、map、judge；judge 在 map 的函数里，外层站点是 map
    assert_eq!(ir.sites.sites.len(), 3);
    let judge = &ir.sites.sites[2];
    assert_eq!(judge.enclosing, Some(ir.sites.sites[1].id));
    assert!(judge.function.is_some());
}

#[test]
fn 遮蔽的名字按用户名字处理() {
    let ir = ir_of("let judge = 1;\njudge(1)");
    assert!(ir.sites.sites.is_empty());
}

#[test]
fn 缺预算在降级处报() {
    let e = lower(&parse("1").unwrap(), &Names).unwrap_err();
    assert!(e[0].message.starts_with("J-07a: "), "{e:?}");
    let e = lower(&parse("budget {calls: 1}; 0").unwrap(), &Names).unwrap_err();
    assert!(e[0].message.starts_with("J-07a: "), "{e:?}");
}

#[test]
fn 槽名不符变红() {
    let ir = ir_of("judge(1)");
    let e = ir::wellformed(&ir, &Names).unwrap_err();
    assert_eq!(e[0].code, "I-slots");
}

#[test]
fn handle缺unsure臂变红() {
    let ir = ir_of("handle(0, {act: 1})");
    let e = ir::wellformed(&ir, &Names).unwrap_err();
    assert_eq!(e[0].code, "I-handle");
}

#[test]
fn 站点表与节点不对应变红() {
    let mut ir = ir_of("state(\"x\")");
    ir.sites.sites[0].node = jpp_ir::key::NodeId(999);
    let e = ir::wellformed(&ir, &Names).unwrap_err();
    assert_eq!(e[0].code, "I-site");
    let mut ir2 = ir_of("state(\"x\")");
    ir2.sites.sites.push(ir2.sites.sites[0].clone());
    assert!(ir::wellformed(&ir2, &Names).is_err());
}

#[test]
fn 预算负数变红() {
    // 表层的预算解析拒绝负数，这里直接改 IR，核 `wellformed` 自己也拦得住
    let mut ir = ir_of("0");
    ir.budget.cost = -1.0;
    assert_eq!(ir::wellformed(&ir, &Names).unwrap_err()[0].code, "I-budget");
}
