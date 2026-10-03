//! Z0882（C2 回炉，附录四、五）：闭包捕获整份大值、以整份大值为实参调函数、对整份大值调内置时，单元键、帧实参哈希、
//! 来源并集都不能随「调用次数 × 值大小」涨。第二靶子 `purpose_run` 的写法：`map(derive_idx(items0), fn(j) { … items0[j] … })`。
//! 预注册：`地基/过程记录/工程-C2-单元图求值.md` 附录四、五（P3′ 基准、P4′ 回归）。

use std::time::Instant;

use jpp::effects::{CalibStore, Ports};
use jpp::ledger::Ledger;
use jpp::{ActionRegistry, EntryArgs, EntryValue, Outcome, Session};
use serde_json::json;

/// purpose_run 形状：材料列表（每项带一份材料）；按项的闭包捕获整份列表；每项以整份列表为实参调一次用户函数；
/// 每项对整份列表调一次内置。
const 程序: &str = "budget {calls: 0, cost: 0, depth: 64};
fn pick(xs, j) { xs[j].m }
fn run(items0) {
  let lens = map(range(0, len(items0)), fn(j) { len(content(pick(items0, j))) });
  let keep = filter(range(0, len(items0)), fn(j) { lens[j] > 0 && len(items0) > 0 });
  len(keep)
}
let items0 = map(items, fn(x) { {id: x.id, m: mat(x.text)} });
run(items0)
";

/// N 条记录、每条约 7 KB 文本（N = 800 约 5.6 MB）。
fn 入口(n: usize) -> EntryArgs {
    let items: Vec<_> = (0..n)
        .map(|i| json!({"id": i, "text": format!("第{i}条：{}", "问题描述".repeat(600))}))
        .collect();
    EntryArgs {
        values: vec![EntryValue::new("items", json!(items))],
        ..Default::default()
    }
}

fn 跑(n: usize, cells: bool) -> (Outcome, f64) {
    let entry = 入口(n);
    let program =
        Session::compile(&jpp::syntax::parse(程序).expect("解析"), &entry.decl()).expect("compile");
    let calib = CalibStore::new();
    let acts = ActionRegistry::new();
    let t = Instant::now();
    let o = Session::new(Ports::new(), &calib, &acts)
        .with_companions(jpp::interp::CompanionMode::Off)
        .with_cells(cells)
        .run(&program, &entry, &mut Ledger::new())
        .unwrap_or_else(|e| panic!("{}", e.render()));
    assert_eq!(o.value_json(), json!(n));
    (o, t.elapsed().as_secs_f64())
}

/// P3′ 基准（墙钟，只报告不断言）：`cargo test -p jpp --test c2_large_capture -- --ignored --nocapture`。
/// `C2_BENCH_N` 给逗号分隔的 N（缺省 100,200,400,800）。
#[test]
#[ignore]
fn p3_基准_大捕获墙钟() {
    let ns: Vec<usize> = std::env::var("C2_BENCH_N")
        .unwrap_or_else(|_| "100,200,400,800".into())
        .split(',')
        .map(|s| s.trim().parse().unwrap())
        .collect();
    for n in ns {
        let (_, off) = 跑(n, false);
        let (_, on) = 跑(n, true);
        eprintln!("N={n} 关 {off:.2}s 开 {on:.2}s");
    }
}

/// P4′ 回归（计数，不靠墙钟）：慢路径访问节点数（来源并集、帧实参指纹哈希、单元值哈希三处的重算实际访问的节点）
/// 随 N 线性涨。N = 100 与 N = 400 之比，开、关单元图都 ≤ 5（修前每次调用都把整份列表走一遍，比约 16）。
#[test]
fn p4_大捕获慢路径线性() {
    use jpp::value::ident_cache::慢路径访问数;
    for cells in [false, true] {
        let 量 = |n: usize| {
            let 前 = 慢路径访问数();
            let _ = 跑(n, cells);
            慢路径访问数() - 前
        };
        let (a, b) = (量(100), 量(400));
        assert!(a > 0);
        let 比 = b as f64 / a as f64;
        assert!(
            比 <= 5.0,
            "单元图{}：慢路径 N=100 {a}、N=400 {b}，比 {比:.1} 超过 5（应线性）",
            if cells { "开" } else { "关" }
        );
    }
}

/// P8（附录六）：`iterate` 的累积值逐步变长（每步 `append` 一行），单元图开着时每步只该哈希新节点。步数 50 与 200
/// 的慢路径访问数之比，开、关单元图都 ≤ 5（修前开着每步把整份累积值逐元素重算，比约 16）。
#[test]
fn p8_iterate累积值慢路径线性() {
    use jpp::value::ident_cache::慢路径访问数;
    let 跑步 = |n: usize, cells: bool| {
        let src = format!(
            "budget {{calls: 0, cost: 0, depth: 64}};
fn step(acc, i) {{ {{rows: append(acc.rows, {{i: i, t: text(i), pend: [i]}}), keys: append(acc.keys, text(i)), n: acc.n + 1}} }}
let r = iterate({n}, {{rows: [], keys: [], n: 0}}, step, fn(acc) {{ {n} - len(acc.rows) }});
len(r.value.rows)
"
        );
        let program = jpp::lower(&jpp::syntax::parse(&src).expect("解析")).expect("lower");
        let calib = CalibStore::new();
        let acts = ActionRegistry::new();
        let 前 = 慢路径访问数();
        let o = Session::new(Ports::new(), &calib, &acts)
            .with_companions(jpp::interp::CompanionMode::Off)
            .with_cells(cells)
            .run(&program, &EntryArgs::default(), &mut Ledger::new())
            .unwrap_or_else(|e| panic!("{}", e.render()));
        assert!(
            o.value_json().as_i64().is_some_and(|x| x >= 1),
            "{:?}",
            o.value_json()
        );
        if cells {
            assert!(
                o.cells.as_ref().is_some_and(|c| c.代码计算 > 0),
                "step 成了代码单元"
            );
        }
        慢路径访问数() - 前
    };
    for cells in [false, true] {
        let (a, b) = (跑步(50, cells), 跑步(200, cells));
        let 比 = b as f64 / a.max(1) as f64;
        assert!(
            比 <= 5.0,
            "单元图{}：慢路径 50 步 {a}、200 步 {b}，比 {比:.1} 超过 5（应线性）",
            if cells { "开" } else { "关" }
        );
    }
}
