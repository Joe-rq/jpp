//! 步 28：闸门信息值的内置（步 30 起按主会话裁定四十三改名 `gate_info(题列表, 状态列表)`，原名 `info_values`）——B7、B43
//! 的效用族，经 `PlanHooks::gate_info` 由 `jpp-plan::value` 算（运行时不依赖规划器）。验题闸门第④段用它排序。
//! 步 30：夹具的记录改为有证书、按样本造的（主会话裁定四十六：价值是记录混淆矩阵上的互信息；没有证书的记录算无记录），
//! 数按第二版，见过程记录 工程-步30 §7.8、§7.9。

mod value_support;

use jpp::effects::{CalibStore, EffectError, FnPort, JudgeResult, Ports};
use jpp::ledger::Ledger;
use jpp::{ActionRegistry, lower, run, syntax::parse};

fn 跑(src: &str, calib: &CalibStore) -> serde_json::Value {
    let program = lower(&parse(src).expect("解析")).expect("lower");
    let ports = Ports::new()
        .with(FnPort::judge("fixed-0", |_s, _qs| {
            Err::<JudgeResult, _>(EffectError("不该判断".into()))
        }))
        .with(FnPort::generate("fixed-0", |_p, _c, _n, _r| {
            Err(EffectError("不该 gen".into()))
        }))
        .with(FnPort::ask("fixed-0", |_s, _q| {
            Err(EffectError("不该 ask".into()))
        }));
    let mut ledger = Ledger::new();
    run(&program, ports, calib, &ActionRegistry::new(), &mut ledger)
        .unwrap_or_else(|e| panic!("{}", e.render()))
        .value_json()
}

const 两道三选一: &str = r#"
budget {calls: 0, cost: 0, depth: 16};
let st = state(mat("一段材料"), {over: [mat("甲"), mat("乙"), mat("丙")]});
gate_info([select("哪一个是作者？", "ka"), select("哪一个是读者？", "kb"), test("有作者吗？", "kc")], [st, st, state(mat("一段材料"))])
"#;

/// 每个键给「前 j 条样本改为未决」；`None` 为无记录
fn 库(ka: Option<usize>, kb: Option<usize>, kc: Option<usize>) -> CalibStore {
    let v: Vec<(&str, usize, Option<usize>)> = [("ka", ka), ("kb", kb), ("kc", kc)]
        .into_iter()
        .filter_map(|(k, j)| j.map(|j| (k, j, None)))
        .collect();
    value_support::认证库(&v)
}

fn 数(v: &serde_json::Value) -> Vec<f64> {
    v.as_array()
        .unwrap()
        .iter()
        .map(|x| x.as_f64().unwrap())
        .collect()
}

#[test]
fn a_全批都有记录_按各自未决率折扣() {
    // ka：K 选一 3 候选、j = 8（对称近似 1.210568，§10 伪计数同口径后）；kb：j = 40（0.619934）；kc：是非、j = 0（0.822475）
    let v = 数(&跑(两道三选一, &库(Some(8), Some(40), Some(0))));
    assert!((v[0] - 1.210568).abs() < 1e-6, "{v:?}");
    assert!((v[1] - 0.619934).abs() < 1e-6, "{v:?}");
    assert!((v[2] - 0.822475).abs() < 1e-6, "{v:?}");
    assert!(
        v[0] > v[2] && v[2] > v[1],
        "未决率高的 K 选一排到是非题后面：{v:?}"
    );
}

#[test]
/// 裁定四十三：无记录的候选取同题类已认证记录的最低折扣（两条都是 attr 的是非记录，最低为 j = 40 那条的 0.362869），
/// 有记录的按自己的。
/// 原断言「同批一道缺记录就全批不折扣」按裁定四十三作废
fn b_同批有一道缺记录_无记录的取同题类最低折扣() {
    let v = 数(&跑(两道三选一, &库(Some(8), Some(40), None)));
    assert!((v[0] - 1.210568).abs() < 1e-6, "{v:?}");
    assert!((v[1] - 0.619934).abs() < 1e-6, "{v:?}");
    assert!((v[2] - 0.362869).abs() < 1e-6, "{v:?}");
}

/// (c) 读折扣要进账本（价值函数线复核）：两个 K 选一候选的题式键上各有校准记录，width 1 按折扣后的信息值取
/// 一道并发出；只凭账本重放（不给校准库，按账本 CalibUsed 补回）时排名不变、发出同一道，不报 E-replay。
#[test]
fn c_折扣用到的校准记录进账本_只凭账本重放排名不变() {
    use jpp::value::{Answer, Form, Op};
    use jpp::{EntryArgs, Session};
    let src = r#"import "../../lib/derive/chain.jpp";
budget {calls: 5, cost: 0, depth: 512};
let n = derive_node("x0", {on: mat("甲乙丙的分工记录")}, test("分工清楚吗？", "t0"), {});
let mk = fn(k, t) { derive_child(n, unit, k, fill(derive_form("select", t, {}, {}), {}), {by: "elicit", over: ["甲", "乙", "丙"]}) };
let g = derive_gate([mk(0, "谁负责交付？"), mk(1, "谁负责付款？")], {judge_diag: false, width: 1});
let es = map(g.pass, fn(c) { cut(judge(c.st, c.q)) });
map(derive_idx(g.pass), fn(k) { {q: g.pass[k].q.text, exit: es[k]} })
"#;
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let dir = root.join(format!("target/derive-iv-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("p.jpp");
    std::fs::write(&path, src).unwrap();
    let loaded = jpp::syntax::loader::load(&path);
    let _ = std::fs::remove_dir_all(&dir);
    let program = lower(&loaded.expect("装载").program).expect("lower");
    // 两个候选的题式键：「谁负责付款？」未决率低，折扣后胜出（不折扣时打平，按原顺序取「谁负责交付？」）
    let key = |t: &str| {
        let h = Form::new(Op::Select, t, "x", vec![], vec![], None, None)
            .unwrap()
            .hash;
        format!("\u{1f}form\u{1f}{h}")
    };
    let (k1, k2) = (key("谁负责交付？"), key("谁负责付款？"));
    // 「谁负责交付？」j = 48（0.474926），「谁负责付款？」j = 8（1.210568）
    let calib = value_support::认证库(&[(k1.as_str(), 48, None), (k2.as_str(), 8, None)]);
    let 端口 = |判: bool| {
        Ports::new()
            .with(FnPort::judge("fixed-0", move |_s, qs| {
                assert!(判, "重放不该判断");
                Ok::<_, EffectError>(JudgeResult {
                    answers: qs
                        .iter()
                        .map(|_| Answer::Choice(vec![0.1, 0.8, 0.1]))
                        .collect(),
                    tokens: 0,
                    cost: 0.0,
                    mode_share: vec![],
                    perms: vec![],
                    confidence: vec![],
                })
            }))
            .with(FnPort::generate("fixed-0", |_p, _c, _n, _r| {
                Err(EffectError("不该 gen".into()))
            }))
            .with(FnPort::ask("fixed-0", |_s, _q| {
                Err(EffectError("不该 ask".into()))
            }))
    };
    let acts = ActionRegistry::new();
    let mut ledger = Ledger::new();
    let first = Session::new(端口(true), &calib, &acts)
        .run(&program, &EntryArgs::default(), &mut ledger)
        .unwrap_or_else(|e| panic!("{}", e.render()))
        .value_json();
    assert_eq!(first[0]["q"], serde_json::json!("谁负责付款？"), "{first}");
    let used: Vec<&String> = ledger.calib_used.keys().collect();
    assert!(
        used.contains(&&key("谁负责交付？")) && used.contains(&&key("谁负责付款？")),
        "{used:?}"
    );
    // 只凭账本重放：不给校准库，按账本的 CalibUsed 补回（宿主与 CLI `--replay` 同一入口）
    let mut restored = CalibStore::new();
    Session::restore_calib(&mut restored, &ledger).unwrap();
    let mut l2 = ledger.clone();
    let again = Session::new(端口(false), &restored, &acts)
        .replay(&program, &EntryArgs::default(), &mut l2)
        .unwrap_or_else(|e| panic!("只凭账本重放：{}", e.render()))
        .value_json();
    assert_eq!(again, first);
}
