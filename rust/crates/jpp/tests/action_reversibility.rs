//! C-7（主控 Z0173）：宿主动作可撤回性如实标注——事实表逐项有值，`.jpp`（`action_fact`）与报告
//! （`action_facts`）读得到；J-08 放行门与检查器不读它。
//!
//! 预注册（代码之前登记的预测）：`地基/过程记录/工程-C7C8-动作表.md` §5.1、§5.2（P7-1 至 P7-7）。
//! 「可撤回」的定义与出处见 `actions/mod.rs` 的 [`jpp::actions::Reversibility`] 头注。

use std::path::{Path, PathBuf};
use std::process::Command;

use jpp::actions::{Reversibility, builtin_actions};
use jpp::effects::{CalibStore, Ports};
use jpp::interp::ActionRegistry;
use jpp::ledger::Ledger;
use jpp::{lower, run, syntax::parse};

fn 内置名() -> Vec<&'static str> {
    builtin_actions().iter().map(|a| a.name).collect()
}

fn 取(name: &str) -> &'static jpp::actions::HostAction {
    builtin_actions()
        .iter()
        .find(|a| a.name == name)
        .unwrap_or_else(|| panic!("{name} 应在表里"))
}

/// P7-1：16 行逐项有三值、有非空理由；本机有沙箱时分布 15 : 1 : 0（T0 加 `graph:cycles`，14 → 15；Z0885 加
/// `env:step`，15 → 16，可逆位按裁定六十九随沙箱派生）。
#[test]
fn 事实表逐项有值且分布如预注册() {
    assert_eq!(builtin_actions().len(), 16);
    for a in builtin_actions() {
        assert!(!a.undo.reason.trim().is_empty(), "{}: 理由不能空", a.name);
    }
    let 数 = |k: Reversibility| {
        builtin_actions()
            .iter()
            .filter(|a| a.undo.kind == k)
            .count()
    };
    if jpp::actions::sandbox_available() {
        assert_eq!(
            (
                数(Reversibility::Reversible),
                数(Reversibility::Irreversible),
                数(Reversibility::DependsOnArgs)
            ),
            (15, 1, 0)
        );
        assert_eq!(取("write_json").undo.kind, Reversibility::Irreversible);
    } else {
        // P7-2 在本机没有沙箱时的同一断言：三个执行器与 env:step 翻成不可逆
        assert_eq!(
            (
                数(Reversibility::Reversible),
                数(Reversibility::Irreversible),
                数(Reversibility::DependsOnArgs)
            ),
            (11, 5, 0)
        );
    }
}

/// P7-3：三值与 J-08 布尔一致，`check_table()` 的布尔逐项等于表里的布尔。
#[test]
fn 三值与布尔一致_检查表布尔不变() {
    let t = jpp::actions::check_table();
    for a in builtin_actions() {
        assert_eq!(a.undo.kind.as_bool(), a.reversible, "{}", a.name);
        assert_eq!(t.actions[a.name].reversible, a.reversible, "{}", a.name);
    }
    // 三值到布尔的映射规则：只有 Reversible 为真
    assert!(Reversibility::Reversible.as_bool());
    assert!(!Reversibility::Irreversible.as_bool());
    assert!(!Reversibility::DependsOnArgs.as_bool());
}

/// P7-4：理由如实写出关键事实。
#[test]
fn 理由如实写出关键事实() {
    let e = 取("embed_topk");
    assert_eq!(e.undo.kind, Reversibility::Reversible);
    assert!(
        e.undo.reason.contains("~/.cache/jpp-embed"),
        "{}",
        e.undo.reason
    );
    assert!(e.undo.reason.contains("不改用户数据"), "{}", e.undo.reason);
    assert!(e.undo.conditions.iter().any(|c| c.contains("jpp-embed")));
    let w = 取("write_json");
    assert!(w.undo.reason.contains("覆盖"), "{}", w.undo.reason);
    assert!(
        !w.undo.conditions.is_empty(),
        "覆盖与新建两种情形要写成条件"
    );
    for n in ["graph:matching", "graph:max_clique", "bm25_topk"] {
        assert!(取(n).undo.reason.contains("不碰宿主"), "{n}");
    }
    for n in ["exec_py", "check_tests", "exec_sql"] {
        let a = 取(n);
        if jpp::actions::sandbox_available() {
            assert!(a.undo.reason.contains("临时目录"), "{n}: {}", a.undo.reason);
            assert!(a.undo.reason.contains("读宿主文件不隔离"), "{n}");
            assert!(a.undo.conditions[0].starts_with("沙箱："), "{n}");
        } else {
            assert!(a.undo.reason.contains("探测不到"), "{n}: {}", a.undo.reason);
        }
    }
}

fn 跑源(src: &str, actions: &ActionRegistry) -> serde_json::Value {
    let program = lower(&parse(src).expect("解析")).expect("lower");
    let mut ledger = Ledger::new();
    let o = run(
        &program,
        Ports::new(),
        &CalibStore::new(),
        actions,
        &mut ledger,
    )
    .unwrap_or_else(|e| panic!("应当跑完：{}", e.render()));
    o.value_json()
}

fn 注册全表() -> ActionRegistry {
    let mut a = ActionRegistry::new();
    jpp::actions::register_all(&mut a, &jpp::actions::Ctx::default(), false);
    a
}

/// P7-5：`.jpp` 里 `action_fact(name)` 对表里每个名字返回 `{reversibility, reason, conditions}`，与 Rust 表逐项一致。
#[test]
fn jpp里读到每个动作的事实且与表一致() {
    let actions = 注册全表();
    for a in builtin_actions() {
        let v = 跑源(
            &format!(
                "budget {{calls: 1, cost: 0}};\naction_fact(\"{}\")\n",
                a.name
            ),
            &actions,
        );
        assert_eq!(v, jpp::actions::action_fact_json(a), "{}", a.name);
        assert_eq!(v["reversibility"], a.undo.kind.as_str());
    }
}

/// P7-5：未登记的名字返回失败值，程序不中止，作者可以 `is_fail` 判。
#[test]
fn 未登记名返回失败值() {
    let v = 跑源(
        "budget {calls: 1, cost: 0};\nis_fail(action_fact(\"没有这个动作\"))\n",
        &注册全表(),
    );
    assert_eq!(v, serde_json::json!(true));
}

/// 宿主自己登记、没附事实的动作：从布尔推两值，理由如实写「登记时未附」。
#[test]
fn 宿主登记的动作没附事实时按布尔推() {
    let mut a = ActionRegistry::new();
    a.register(
        "发邮件",
        0.0,
        false,
        jpp::interp::TaintOut::Trusted,
        |_| Ok(jpp::value::Value::text("已发")),
    );
    let v = 跑源("budget {calls: 1, cost: 0};\naction_fact(\"发邮件\")\n", &a);
    assert_eq!(v["reversibility"], "irreversible");
    assert!(v["reason"].as_str().unwrap().contains("未附"));
    assert_eq!(v["conditions"], serde_json::json!([]));
    // 宿主用 describe_undo 补一份「视参数而定」的事实
    assert!(a.describe_undo(
        "发邮件",
        jpp::interp::ActionUndo {
            reversibility: "depends_on_args".into(),
            reason: "收件人在撤回窗口内的可撤回".into(),
            conditions: vec!["参数 to 属于内部域名".into()],
        }
    ));
    assert!(!a.describe_undo(
        "没登记",
        jpp::interp::ActionUndo {
            reversibility: "reversible".into(),
            reason: String::new(),
            conditions: vec![],
        }
    ));
    let v = 跑源("budget {calls: 1, cost: 0};\naction_fact(\"发邮件\")\n", &a);
    assert_eq!(v["reversibility"], "depends_on_args");
    assert_eq!(v["conditions"], serde_json::json!(["参数 to 属于内部域名"]));
}

fn 目录(名: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("jpp-c7-{名}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn jpp跑(d: &Path, 文件: &str) -> serde_json::Value {
    let o = Command::new(env!("CARGO_BIN_EXE_jpp"))
        .current_dir(d)
        .args(["run", 文件, "--output", "r.json"])
        .output()
        .unwrap();
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    serde_json::from_slice(&std::fs::read(d.join("r.json")).unwrap()).unwrap()
}

/// P7-6：报告只列字面 `do` 用到的动作；动作名非字面量时列全表；没有 `do` 的程序没有这一节。
#[test]
fn 报告有action_facts一节且只在有do时出现() {
    let d = 目录("report");
    std::fs::write(
        d.join("a.jpp"),
        "budget {calls: 2, cost: 0};\nlet g = {edges: [{u: \"a\", v: \"b\"}], nodes: [\"a\", \"b\"]};\ncontent(do(\"graph:components\", [g], 0))\n",
    )
    .unwrap();
    let r = jpp跑(&d, "a.jpp");
    let f = r["action_facts"].as_object().expect("有 action_facts 一节");
    assert_eq!(f.len(), 1);
    assert_eq!(f["graph:components"]["reversibility"], "reversible");
    assert!(
        f["graph:components"]["reason"]
            .as_str()
            .unwrap()
            .contains("纯函数")
    );

    std::fs::write(d.join("n.jpp"), "budget {calls: 1, cost: 0};\n1 + 2\n").unwrap();
    let r = jpp跑(&d, "n.jpp");
    assert!(r.get("action_facts").is_none(), "没有 do 的程序报告不变");

    std::fs::write(
        d.join("v.jpp"),
        "budget {calls: 2, cost: 0};\nfn f(name) { do(name, [1], 0) }\nis_fail(f(\"read_json\"))\n",
    )
    .unwrap();
    let r = jpp跑(&d, "v.jpp");
    // 动作名经形参追到唯一字面量时仍按字面量列；这里只关心它是表里的子集且含被用到的那个
    let f = r["action_facts"].as_object().expect("有 action_facts 一节");
    assert!(f.contains_key("read_json"));
    let _ = std::fs::remove_dir_all(&d);
}

/// P7-7 的机制面：J-08 放行门、检查器、意向账本不读三值字段（三值只被 `action_fact` 与报告读）。
#[test]
fn 放行门与检查器不读三值字段() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut 文件 = vec![
        root.join("crates/jpp-runtime/src/guard.rs"),
        root.join("crates/jpp-runtime/src/effects_exec.rs"),
        root.join("crates/jpp/src/cli/runner.rs"),
    ];
    for e in std::fs::read_dir(root.join("crates/jpp-check/src/rules")).unwrap() {
        文件.push(e.unwrap().path());
    }
    for f in 文件 {
        let s = std::fs::read_to_string(&f).unwrap();
        for 词 in ["ActionUndo", ".undo.", ".undo)"] {
            // runner.rs 只允许经 action_facts_json 露出，不直接读 .undo
            assert!(!s.contains(词), "{} 不该读三值字段（{词}）", f.display());
        }
    }
    let _ = 内置名();
}

/// P7-2：宿主探测不到沙箱（`JPP_FORCE_NO_SANDBOX`，只给测试用）时，报告里的执行器动作是不可逆，条件写探测不到；
/// 有沙箱的本机对照组是可逆并写出沙箱名。事实随探测结果如实变化，程序一个字不改。
#[test]
fn 无沙箱时执行器事实翻成不可逆() {
    let d = 目录("nosb");
    std::fs::write(
        d.join("p.jpp"),
        "budget {calls: 2, cost: 0};\nis_fail(do(\"exec_py\", [\"print(1)\", \"\", 5], 0))\n",
    )
    .unwrap();
    let 跑 = |无沙箱: bool| {
        let mut c = Command::new(env!("CARGO_BIN_EXE_jpp"));
        c.current_dir(&d)
            .args(["run", "p.jpp", "--output", "r.json"]);
        if 无沙箱 {
            c.env("JPP_FORCE_NO_SANDBOX", "1");
        }
        let o = c.output().unwrap();
        assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
        let r: serde_json::Value =
            serde_json::from_slice(&std::fs::read(d.join("r.json")).unwrap()).unwrap();
        r["host"]["action_facts"]["exec_py"].clone()
    };
    let 无 = 跑(true);
    assert_eq!(无["reversibility"], "irreversible");
    assert!(无["conditions"][0].as_str().unwrap().contains("探测不到"));
    if jpp::actions::sandbox_available() {
        let 有 = 跑(false);
        assert_eq!(有["reversibility"], "reversible");
        assert!(有["conditions"][0].as_str().unwrap().starts_with("沙箱："));
    }
    let _ = std::fs::remove_dir_all(&d);
}

/// Z0901：事实随宿主沙箱变的动作（执行器三个）进报告的 `host` 块，不在 `action_facts` 里；`host.sandbox` 写沙箱种类；
/// 只用纯动作的程序没有 `host`。
#[test]
fn 执行器事实进host块且纯动作程序没有host() {
    let d = 目录("hostblock");
    std::fs::write(
        d.join("p.jpp"),
        "budget {calls: 2, cost: 0};\nis_fail(do(\"exec_py\", [\"print(1)\", \"\", 5], 0))\n",
    )
    .unwrap();
    let r = jpp跑(&d, "p.jpp");
    assert!(r["action_facts"].as_object().unwrap().is_empty(), "{r}");
    assert_eq!(r["host"]["sandbox"], jpp::actions::host_sandbox_kind());
    assert!(r["host"]["action_facts"]["exec_py"]["reversibility"].is_string());
    assert_eq!(r["host"]["action_facts"].as_object().unwrap().len(), 1);

    std::fs::write(
        d.join("g.jpp"),
        "budget {calls: 2, cost: 0};\nlet g = {edges: [{u: \"a\", v: \"b\"}], nodes: [\"a\", \"b\"]};\ncontent(do(\"graph:components\", [g], 0))\n",
    )
    .unwrap();
    let r = jpp跑(&d, "g.jpp");
    assert!(r.get("host").is_none(), "纯动作程序没有 host：{r}");
    assert!(r["action_facts"].get("graph:components").is_some());
    let _ = std::fs::remove_dir_all(&d);
}
