//! C2c（步 41）：纯函数调用成为代码单元（B193），同一趟内只在重新求值不留任何外部痕迹时取记忆（附录二 A2.3），
//! 开、关单元图外部行为逐字节相同。预注册：`地基/过程记录/工程-C2-单元图求值.md` 附录二 A2.8（C2c-3）。

use std::cell::Cell;

use jpp::effects::{CalibStore, FnPort, GenResult, JudgeResult, Ports};
use jpp::ledger::Ledger;
use jpp::value::Answer;
use jpp::{ActionRegistry, EntryArgs, Outcome, Session, lower, syntax::parse};
use serde_json::{Value as Json, json};

fn 编译(src: &str) -> jpp::Program {
    lower(&parse(src).unwrap_or_else(|e| panic!("解析：{e:?}"))).expect("lower")
}

fn 端口(判: &Cell<u32>, 生: &Cell<u32>) -> Ports<'static> {
    // 计数器要活过 Ports：泄漏两个小单元（测试进程内）
    let 判: &'static Cell<u32> = Box::leak(Box::new(Cell::new(判.get())));
    let 生: &'static Cell<u32> = Box::leak(Box::new(Cell::new(生.get())));
    Ports::new()
        .with(FnPort::judge("fixed-0", move |_s, qs| {
            判.set(判.get() + 1);
            Ok(JudgeResult {
                answers: qs.iter().map(|_| Answer::Noul(0.5)).collect(),
                tokens: 0,
                cost: 0.0,
                mode_share: vec![],
                perms: vec![],
                confidence: vec![],
            })
        }))
        .with(FnPort::generate("fixed-0", move |_p, _c, _n, _r| {
            生.set(生.get() + 1);
            Ok(GenResult {
                outputs: vec![json!("甲")],
                tokens: 0,
                cost: 0.0,
                failure: None,
                taint_out: None,
            })
        }))
}

fn 跑(src: &str, cells: bool, entry: &EntryArgs) -> (Outcome, Ledger) {
    let calib = CalibStore::new();
    let acts = ActionRegistry::new();
    let (a, b) = (Cell::new(0), Cell::new(0));
    let mut l = Ledger::new();
    let o = Session::new(端口(&a, &b), &calib, &acts)
        .with_companions(jpp::interp::CompanionMode::Off)
        .with_cells(cells)
        .run(
            &Session::compile(&parse(src).expect("解析"), &entry.decl()).expect("compile"),
            entry,
            &mut l,
        )
        .unwrap_or_else(|e| panic!("{}", e.render()));
    (o, l)
}

fn 外部(o: &Outcome, l: &Ledger) -> Json {
    json!({
        "value": o.value_json(),
        "events": format!("{:?}", o.trace.events),
        "warnings": o.trace.warnings,
        "cost": format!("{:?}", o.cost),
        "cache": format!("{:?}", o.cache),
        "layers": format!("{:?}", o.layers),
        "exits": o.exits,
        "questions": o.questions,
        "violations": format!("{:?}", o.violations),
        "returned_unsure": o.returned_unsure,
        "unsure_default": o.unsure_default,
        "ledger": serde_json::to_value(&l.entries).unwrap(),
    })
}

/// 开、关单元图跑同一程序：外部行为逐字节相同；返回开着时的单元统计。
fn 两臂(src: &str) -> jpp::interp::单元统计 {
    两臂_入口(src, &EntryArgs::default())
}

fn 两臂_入口(src: &str, entry: &EntryArgs) -> jpp::interp::单元统计 {
    let (o1, l1) = 跑(src, true, entry);
    let (o2, l2) = 跑(src, false, entry);
    assert_eq!(外部(&o1, &l1), 外部(&o2, &l2), "开、关单元图外部行为不同");
    assert!(o2.cells.is_none());
    o1.cells.expect("单元图开着")
}

const 预算: &str = "budget {calls: 8, cost: 0, depth: 32};\n";

#[test]
fn c2c_3_纯函数同键第二次取记忆() {
    let c = 两臂(&format!(
        "{预算}fn f(x) {{ x + 1 }}\nlet a = f(1);\nlet b = f(1);\nlet d = f(2);\n{{a: a, b: b, d: d}}\n"
    ));
    assert_eq!((c.代码计算, c.代码命中, c.代码单元), (2, 1, 2), "{c:?}");
}

#[test]
fn c2c_3_判断承载的函数同一趟重新求值() {
    let src = format!(
        "{预算}fn g(t) {{ cut(judge(state(mat(t)), test(\"合适吗\", \"k\"))) }}\n\
         let h = fn(e) {{ handle(e, {{act: fn() {{ 1 }}, ignore: fn() {{ 0 }}, unsure: fn(u) {{ consume(u, \"drop\"); 2 }}}}) }};\n\
         let a = h(g(\"甲\"));\nlet b = h(g(\"甲\"));\n{{a: a, b: b}}\n"
    );
    let c = 两臂(&src);
    // g 两次都算（建出口，足迹非空）；h 的实参是出口，不可键
    assert_eq!(c.代码命中, 0, "{c:?}");
    assert!(c.代码计算 >= 2, "{c:?}");
    assert!(c.不可键 >= 2, "{c:?}");
}

#[test]
fn c2c_3_刷新点捎带外面的待发判断时重新求值() {
    // lab 只算值，体内的 if 是刷新点；第二次调用时外面有待发判断，重新求值才会把它带着发一层
    let src = format!(
        "{预算}fn lab(k) {{ if k == \"a\" {{ 1 }} else {{ 2 }} }}\n\
         let x = lab(\"a\");\n\
         let r = judge(state(mat(\"甲\")), test(\"合适吗\", \"k\"));\n\
         let y = lab(\"a\");\n\
         let e = cut(r);\n\
         let z = handle(e, {{act: fn() {{ 1 }}, ignore: fn() {{ 0 }}, unsure: fn(u) {{ consume(u, \"drop\"); 2 }}}});\n\
         {{x: x, y: y, z: z}}\n"
    );
    let c = 两臂(&src);
    assert_eq!(c.代码命中, 0, "外面有待发判断，不取记忆：{c:?}");
    assert_eq!(c.代码计算, 2, "{c:?}");
}

#[test]
fn c2c_3_含生成的函数不成单元() {
    let src = format!(
        "{预算}fn w() {{ gen(\"写一个名字\", [mat(\"需求\")], 1, 0) }}\nlet a = w();\nlet b = w();\n{{n: if is_fail(a) {{ 0 }} else {{ len(a) }}, m: if is_fail(b) {{ 0 }} else {{ len(b) }}}}\n"
    );
    let c = 两臂(&src);
    assert_eq!((c.代码单元, c.代码计算, c.不纯), (0, 0, 2), "{c:?}");
}

#[test]
fn c2c_3_捕获不同键不同() {
    let src = format!(
        "{预算}fn mk(c) {{ fn(x) {{ x + c }} }}\nlet f1 = mk(1);\nlet f2 = mk(2);\nlet a = f1(5);\nlet b = f2(5);\nlet d = f1(5);\n{{a: a, b: b, d: d}}\n"
    );
    let c = 两臂(&src);
    // mk(1)、mk(2)、f1(5)、f2(5) 各算一次；第二次 f1(5) 命中
    assert_eq!((c.代码计算, c.代码命中), (4, 1), "{c:?}");
}

#[test]
fn c2c_3_taint不同键不同() {
    let mut entry = EntryArgs::default();
    entry
        .values
        .push(jpp::interp::EntryValue::new("外来", json!("a")));
    let src = format!(
        "{预算}fn f(x) {{ text(x) }}\nlet a = f(\"a\");\nlet b = f(外来);\nlet d = f(\"a\");\n{{a: a, b: b, d: d}}\n"
    );
    let c = 两臂_入口(&src, &entry);
    // 同一个文本 "a"：可信的与宿主入口给的（不可信）是两个单元；第三次与第一次同键命中
    assert_eq!((c.代码计算, c.代码命中), (2, 1), "{c:?}");
}

#[test]
fn c2c_3_iterate同初值两次_开关相同() {
    let src = format!(
        "{预算}let notes = [mat(\"甲\"), mat(\"乙\")];\n\
         fn step(acc, i) !{{judge}} {{\n\
           let e = cut(judge(state(acc.items[0]), test(\"还要吗\", \"k\")));\n\
           handle(e, {{act: fn() {{ {{items: acc.items, pending: acc.pending}} }}, ignore: fn() {{ stop(acc) }}, unsure: fn(u) {{ {{items: acc.items, pending: append(acc.pending, u)}} }}}})\n\
         }}\n\
         let a = iterate(2, {{items: notes, pending: []}}, step, fn(acc) {{ len(acc.items) }});\n\
         let b = iterate(1, {{items: notes, pending: []}}, step, fn(acc) {{ len(acc.items) }});\n\
         {{a: a.value.pending, b: b.value.pending}}\n"
    );
    两臂(&src);
}

/// C2c 复核 K1：取记忆不能绕过 J-06 的递归深度核对。`depth: 2` 下 `g2(1)` 调 `h(1)` 算过一次；之后 `g3(1)` 再深一层
/// 调到 `g2(1)`，关着单元图在 `h` 处报 J-06，开着也要报同一条（命中条件要求「当前深度 + 首次求值的层数」不超限）。
#[test]
fn k1_取记忆不绕过深度上限() {
    let src = "budget {calls: 8, cost: 0, depth: 2};\nfn h(x) { x + 1 }\nfn g2(x) { h(x) }\nfn g3(x) { g2(x) }\nlet a = g2(1);\nlet b = g3(1);\n{a: a, b: b}\n";
    let 跑结果 = |cells: bool| {
        let calib = CalibStore::new();
        let acts = ActionRegistry::new();
        let (a, b) = (Cell::new(0), Cell::new(0));
        Session::new(端口(&a, &b), &calib, &acts)
            .with_companions(jpp::interp::CompanionMode::Off)
            .with_cells(cells)
            .run(&编译(src), &EntryArgs::default(), &mut Ledger::new())
            .map(|o| o.value_json())
            .map_err(|e| e.render())
    };
    let 开 = 跑结果(true);
    let 关 = 跑结果(false);
    assert!(
        关.as_ref().is_err_and(|e| e.contains("J-06")),
        "关着报 J-06：{关:?}"
    );
    assert_eq!(开, 关, "开、关单元图结果相同");
    // 对照：深度够时开着照常命中（g3(1) 里的 g2(1) 取记忆）
    let 够 = src.replace("depth: 2", "depth: 8");
    let c = 两臂(&够);
    assert_eq!(c.代码命中, 1, "{c:?}");
}

#[test]
fn c2c_3_空程序没有代码单元() {
    // 序言（伴随题与默认链的库）求值期间单元图没有打开的尝试，不建单元
    let calib = CalibStore::new();
    let acts = ActionRegistry::new();
    let (a, b) = (Cell::new(0), Cell::new(0));
    let o = Session::new(端口(&a, &b), &calib, &acts)
        .with_cells(true)
        .run(
            &编译(&format!("{预算}1\n")),
            &EntryArgs::default(),
            &mut Ledger::new(),
        )
        .unwrap_or_else(|e| panic!("{}", e.render()));
    let c = o.cells.unwrap();
    assert_eq!(
        (c.代码单元, c.代码计算, c.不可键, c.不纯),
        (0, 0, 0, 0),
        "{c:?}"
    );
}
