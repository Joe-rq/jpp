//! B0472（推进主会话 B0672「线 A 全程只经 CLI」）：CLI 给目的、材料条目与料库。
//! `--purpose <text>`、`--mat <name>=<file>`（可重复）、`--mat-store <dir>`。预注册见
//! `地基/过程记录/工程-B0472-CLI材料与目的.md` 第一节；依据 B105、B106（`地基/附注/2026-09-25-B105-B106裁定.md`）。
use serde_json::{Value as Json, json};
use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Output},
    sync::atomic::{AtomicUsize, Ordering},
};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap()
}

static NEXT: AtomicUsize = AtomicUsize::new(0);

/// 工作目录放在 `target/` 下，程序里的 `import "../../lib/…"` 能找到库
fn 工作目录(tag: &str) -> PathBuf {
    let d = root().join(format!(
        "target/b0472-{tag}-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    let _ = fs::remove_dir_all(&d);
    fs::create_dir_all(&d).unwrap();
    d
}

fn jpp(cwd: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_jpp"))
        .current_dir(cwd)
        .args(args)
        .output()
        .unwrap()
}

fn 成功(o: &Output) {
    assert!(
        o.status.success(),
        "失败：{}{}",
        String::from_utf8_lossy(&o.stdout),
        String::from_utf8_lossy(&o.stderr)
    );
}

fn 读(p: &Path) -> Json {
    serde_json::from_str(&fs::read_to_string(p).unwrap()).unwrap()
}

/// 账本头里的 `entry_hash`（逐行落盘的账本，头在第一行）
fn 账本入口哈希(p: &Path) -> Json {
    let text = fs::read_to_string(p).unwrap();
    let 头: Json = serde_json::from_str(text.lines().next().unwrap()).unwrap();
    fn 找(v: &Json) -> Option<Json> {
        match v {
            Json::Object(m) => m
                .get("entry_hash")
                .cloned()
                .or_else(|| m.values().find_map(找)),
            Json::Array(a) => a.iter().find_map(找),
            _ => None,
        }
    }
    找(&头).expect("账本头有 entry_hash")
}

const 简历: &str = "Backend engineer, 6 years of Rust and Go.\n";
const 题: &str = "Is this resume for a backend role?";

fn 写入口件(d: &Path) -> Json {
    let co = json!({"items": [{"id": 1, "name": "甲"}, {"id": 2, "name": "乙"}]});
    fs::write(d.join("resume.md"), 简历).unwrap();
    fs::write(d.join("co.json"), co.to_string()).unwrap();
    fs::write(
        d.join("a.jpp"),
        format!(
            "budget {{calls: 2, cost: 0}};\nlet e = cut(judge(state(resume), test(\"{题}\", \"k\")));\n\
             {{purpose: purpose, resume: content(resume), co: content(co), taint: resume.taint, kind: exit_kind(e)}}\n"
        ),
    )
    .unwrap();
    fs::write(
        d.join("fx.json"),
        json!({"observations": [{"on": [简历], "op": "test", "text": 题, "calib": "k", "answer": {"Noul": 0.9}}]})
            .to_string(),
    )
    .unwrap();
    co
}

fn 跑a(d: &Path, 目的: &str, 账本: &str) -> Output {
    jpp(
        d,
        &[
            "run",
            "a.jpp",
            "--fixtures",
            "fx.json",
            "--companions",
            "off",
            "--purpose",
            目的,
            "--mat",
            "resume=resume.md",
            "--mat",
            "co=co.json",
            "--output",
            "r.json",
            "--ledger-out",
            账本,
        ],
    )
}

/// (a) 目的与两份材料条目从 CLI 进程序；账本头 `entry_hash` 与库 API 按同样三样算的逐字相等；换目的就变
#[test]
fn a_目的与材料条目进程序_entry_hash与库一致() {
    let d = 工作目录("a");
    let co = 写入口件(&d);
    let 目的 = "Find companies most likely to interview me.";
    成功(&跑a(&d, 目的, "l1.jsonl"));
    let v = &读(&d.join("r.json"))["value"];
    assert_eq!(v["purpose"], 目的, "{v}");
    assert_eq!(v["resume"], 简历, "非 .json 按文本读：{v}");
    assert_eq!(v["co"], co, ".json 按 JSON 读：{v}");
    assert_eq!(v["taint"], "untrusted", "CLI 材料条目缺省不可信：{v}");
    assert_eq!(v["kind"], "act", "{v}");

    let 期望 = jpp::EntryArgs {
        purpose: Some(目的.into()),
        materials: vec![
            jpp::EntryMat::untrusted("resume", json!(简历)),
            jpp::EntryMat::untrusted("co", co.clone()),
        ],
        ..Default::default()
    }
    .hash()
    .expect("有入口");
    let h1 = 账本入口哈希(&d.join("l1.jsonl"));
    assert_eq!(h1, json!(期望));

    成功(&跑a(&d, "Another purpose.", "l2.jsonl"));
    assert_ne!(
        账本入口哈希(&d.join("l2.jsonl")),
        h1,
        "换目的 entry_hash 变"
    );
    let _ = fs::remove_dir_all(&d);
}

/// (b) `check` 带同样开关：程序里的 `purpose`、`resume`、`co` 认得；不带开关时同一程序报错
#[test]
fn b_check_认得入口名字() {
    let d = 工作目录("b");
    写入口件(&d);
    成功(&jpp(
        &d,
        &[
            "check",
            "a.jpp",
            "--purpose",
            "x",
            "--mat",
            "resume=resume.md",
            "--mat",
            "co=co.json",
        ],
    ));
    let o = jpp(&d, &["check", "a.jpp"]);
    assert!(!o.status.success(), "不给入口时程序里的名字没定义");
    let err = String::from_utf8_lossy(&o.stderr);
    assert!(
        err.contains("E-name") && err.contains("resume") && err.contains("purpose"),
        "{err}"
    );
    let _ = fs::remove_dir_all(&d);
}

const P1: &str = "Does this record mention a specific amount?";
const P2: &str = "Is this record from a contract?";
const P3: &str = "Does this record mention a payment deadline?";

/// 每份材料对每道前提题的读数：`A` 留（0.99）、`I` 淘（0.01）
fn 读数(id: i64, q: &str) -> f64 {
    let 表: [char; 4] = match q {
        P1 => ['A', 'A', 'A', 'I'],
        P2 => ['A', 'I', 'A', 'A'],
        _ => ['A', 'A', 'I', 'A'],
    };
    if 表[(id - 1) as usize] == 'A' {
        0.99
    } else {
        0.01
    }
}

fn 写选料件(d: &Path) {
    let items: Vec<Json> = (1..=4)
        .map(|i| json!({"id": i, "text": format!("记录 {i}")}))
        .collect();
    fs::write(d.join("pool.json"), json!({"items": items}).to_string()).unwrap();
    let obs: Vec<Json> = items
        .iter()
        .flat_map(|it| {
            [P1, P2, P3].map(|q| {
                json!({"on": [it], "op": "test", "text": q, "calib": "select-premise",
                       "answer": {"Noul": 读数(it["id"].as_i64().unwrap(), q)}})
            })
        })
        .collect();
    fs::write(d.join("fx.json"), json!({"observations": obs}).to_string()).unwrap();
    for (名, 链) in [("first", [P1, P2]), ("second", [P2, P3])] {
        let 链: Vec<String> = 链
            .iter()
            .map(|q| format!("{{q: \"{q}\", line: L}}"))
            .collect();
        fs::write(
            d.join(format!("{名}.jpp")),
            format!(
                "import \"../../lib/skeletons/select.jpp\";\nbudget {{calls: 40, cost: 0, depth: 256}};\n\
                 let pool = map(content(co).items, fn(x) {{ mat(x) }});\nlet L = {{declare: {{hi: 0.9, lo: 0.1}}}};\n\
                 let r = select(purpose, pool, [{}], {{}});\n\
                 {{ids: map(r.value, fn(e) {{ content(e.item).id }}), pool_hashes: map(pool, fn(m) {{ m.hash }}),\n\
                  per_layer: r.detail.per_layer, stored: r.detail.stored}}\n",
                链.join(", ")
            ),
        )
        .unwrap();
    }
}

fn 跑选料(d: &Path, 程序: &str, 目的: &str, 料库: Option<&str>) -> Json {
    let src = format!("{程序}.jpp");
    let mut args = vec![
        "run",
        &src,
        "--fixtures",
        "fx.json",
        "--companions",
        "off",
        "--purpose",
        目的,
        "--mat",
        "co=pool.json",
        "--output",
        "r.json",
    ];
    if let Some(s) = 料库 {
        args.extend(["--mat-store", s]);
    }
    成功(&jpp(d, &args));
    读(&d.join("r.json"))["value"].clone()
}

fn 问数(v: &Json) -> u64 {
    v["per_layer"]
        .as_array()
        .unwrap()
        .iter()
        .map(|l| l["asked"].as_u64().unwrap())
        .sum()
}

fn 复用数(v: &Json) -> u64 {
    v["per_layer"]
        .as_array()
        .unwrap()
        .iter()
        .map(|l| l["reused"].as_u64().unwrap())
        .sum()
}

/// (c) 线 A 的路：池子来自 `--mat`、在程序里拆开，两次运行共用 `--mat-store`、目的不同、前提链有一道相同。
/// 第一次标记落盘；第二次那道相同前提复用标记、少发题。不给 `--mat-store` 时不存、不复用
#[test]
fn c_料库跨运行复用_池子来自入口材料() {
    let d = 工作目录("c");
    写选料件(&d);
    let a = 跑选料(&d, "first", "Purpose A.", Some("store"));
    assert_eq!(a["stored"], true, "{a}");
    assert!(d.join("store/mat-store.json").exists());
    assert!(d.join("store/marks").is_dir());
    assert_eq!(a["ids"], json!([1, 3]), "{a}");
    assert_eq!(问数(&a), 7, "P1 问 4、P2 问存活的 3：{a}");

    let b = 跑选料(&d, "second", "Purpose B.", Some("store"));
    assert_eq!(
        b["pool_hashes"], a["pool_hashes"],
        "由入口材料内容 mat() 出的材料，哈希两次相同"
    );
    assert_eq!(
        b["per_layer"][0]["reused"], 3,
        "P2 在 1、2、3 上的标记复用：{b}"
    );
    assert_eq!(问数(&b), 4, "P2 只问 4 号、P3 问存活的 3 份：{b}");
    assert_eq!(b["ids"], json!([1, 4]), "{b}");

    let 无 = 跑选料(&d, "second", "Purpose B.", None);
    assert_eq!(无["stored"], false, "{无}");
    assert_eq!(复用数(&无), 0, "{无}");
    assert_eq!(问数(&无), 7, "{无}");
    assert_eq!(无["ids"], b["ids"]);
    let _ = fs::remove_dir_all(&d);
}

/// (d) 用法错误在解析时报（退出码 2）
#[test]
fn d_用法错误() {
    let d = 工作目录("d");
    写入口件(&d);
    for (args, 报文) in [
        (
            vec!["run", "a.jpp", "--purpose", "x", "--purpose", "y"],
            "--purpose was supplied twice",
        ),
        (
            vec!["run", "a.jpp", "--mat", "resume"],
            "--mat expects <name>=<file>",
        ),
        (
            vec!["run", "a.jpp", "--mat", "1x=resume.md"],
            "is not an identifier",
        ),
        (
            vec!["run", "a.jpp", "--mat", "r=resume.md", "--mat", "r=co.json"],
            "'r' was supplied twice",
        ),
        (
            vec!["run", "a.jpp", "--mat", "input=resume.md"],
            "'input' is reserved",
        ),
        (
            vec!["check", "a.jpp", "--mat", "purpose=resume.md"],
            "'purpose' is reserved",
        ),
        (
            vec!["check", "a.jpp", "--mat-store", "s"],
            "--mat-store is accepted only by run",
        ),
        (
            vec!["parse", "a.jpp", "--purpose", "x"],
            "accepted only by check and run",
        ),
    ] {
        let o = jpp(&d, &args);
        assert_eq!(o.status.code(), Some(2), "{args:?}");
        let err = String::from_utf8_lossy(&o.stderr);
        assert!(err.contains(报文), "{args:?} → {err}");
    }
    let _ = fs::remove_dir_all(&d);
}
