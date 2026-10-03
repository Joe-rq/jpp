//! B0630（裁定三十七；预注册 `地基/规划/B0630-键与格式稳定-预注册.md` §2、§三 K6、K8）：判断键与效应键的站点由源码字节
//! 偏移改为结构化标识 `<定义路径>:<标签>#<序号>`；库内部参数只认 lib 调用点（Z0943、Z0947）。
//!
//! - 文法：顶层定义、嵌套命名函数、`let` 绑定的函数、匿名函数、入口顶层各成一段，序号按同定义同标签计；
//! - 改注释、在别处加定义，站点标识不变；
//! - lib 里改注释（偏移整体后挪）：两趟账本键逐个相同，旧账本只凭账本重放改过注释的 lib 零调用；
//! - `unsure_fetch` 与 `unsure_default` 的第二参：用户程序里报 `E-rt-lib-only`，lib 定义里照常（走到后面的参数检查）。

use serde_json::{Value as Json, json};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT: AtomicU64 = AtomicU64::new(0);

fn 标识(src: &str) -> BTreeSet<String> {
    let p = jpp::lower(&jpp::syntax::parse(src).expect("解析")).expect("降级");
    p.site_keys.iter().map(|(_, e)| e.key.clone()).collect()
}

const 程序: &str = r#"budget {calls: 4, cost: 0.01, depth: 8};
fn f(m) {
    let q = test("甲吗？", "k");
    let r = judge(state(m), q);
    let g = fn(y) { judge(state(y), q) };
    map([m], fn(z) { judge(state(z), q) })
}
fn h(m) {
    fn inner(y) { judge(state(y), test("乙吗？", "k")) }
    inner(m)
}
f(mat("x"))
"#;

#[test]
fn 文法_定义路径与同标签序号() {
    let ks = 标识(程序);
    for k in [
        "f:test#1",
        "f:judge#1",
        "f:state#1",
        "f/g:judge#1",
        "f/g:state#1",
        "f:map#1",
        "f/λ1:judge#1",
        "h/inner:judge#1",
        "h/inner:test#1",
        "h:inner#1",
        "<main>:f#1",
        "<main>:mat#1",
    ] {
        assert!(ks.contains(k), "缺 {k}；实有 {ks:?}");
    }
    assert!(ks.iter().all(|k| !k.starts_with('@')), "{ks:?}");
}

#[test]
fn 改注释与别处加定义_站点标识不变() {
    let a = 标识(程序);
    let 加注释 = 程序.replacen(
        "fn f(m) {",
        "// 一行注释\n// 又一行\nfn f(m) {   // 行尾注释",
        1,
    );
    let 加定义 = 程序.replacen(
        "fn h(m) {",
        "fn 别的(m) { judge(state(m), test(\"丙吗？\", \"k\")) }\nfn h(m) {",
        1,
    );
    assert_eq!(a, 标识(&加注释));
    let b = 标识(&加定义);
    assert!(a.is_subset(&b), "原有站点标识都在：{a:?} ⊄ {b:?}");
    assert!(b.contains("别的:judge#1"));
}

fn 临时目录() -> PathBuf {
    let d = std::env::temp_dir().join(format!(
        "jpp-b0630-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::SeqCst)
    ));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(d.join("lib")).unwrap();
    d
}

fn 跑(dir: &Path, args: &[&str]) -> (i32, Json, String) {
    let out = dir.join(format!(
        "report-{}.json",
        NEXT.fetch_add(1, Ordering::SeqCst)
    ));
    let o = Command::new(env!("CARGO_BIN_EXE_jpp"))
        .current_dir(dir)
        .arg("run")
        .args(args)
        .arg("--output")
        .arg(&out)
        .output()
        .expect("起 jpp");
    let r = std::fs::read_to_string(&out)
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or(Json::Null);
    (
        o.status.code().unwrap_or(-1),
        r,
        String::from_utf8_lossy(&o.stderr).into_owned(),
    )
}

const 库: &str = r#"import "outcome.jpp";
fn 起名(brief) {
    let names = gen("提出 2 个候选名", [brief], 2, 0);
    let t = transform(fn(x) { {v: content(x)} }, brief);
    let r = sieve(names, test("这个名字好吗？", "b0630-k"));
    {chosen: map(accepted(r), fn(e) { e.item }), t: content(t), pending: r.pending}
}
"#;

const 入口: &str = r#"import "lib/name.jpp";
budget {calls: 8, cost: 0.01, depth: 16};
起名(mat("一家书店"))
"#;

fn 夹具() -> Json {
    json!({
        "calibrations": [{"key": "b0630-k", "hi": 0.75, "lo": 0.25, "n": 1, "status": "上岗", "delta": 0.05}],
        "generations": [{"prompt": "提出 2 个候选名", "retry_seq": 0, "output": ["甲书店", "乙书店"]}],
        "observations": [
            {"on": ["甲书店"], "op": "test", "text": "这个名字好吗？", "calib": "b0630-k", "answer": {"Noul": 0.9}},
            {"on": ["乙书店"], "op": "test", "text": "这个名字好吗？", "calib": "b0630-k", "answer": {"Noul": 0.1}}
        ]
    })
}

/// 账本里全部条目的键（判断、效应、校准引用）
fn 键集(ledger: &Path) -> Vec<String> {
    let t = std::fs::read_to_string(ledger).expect("账本");
    let mut ks = vec![];
    for (i, l) in t.lines().enumerate() {
        if i == 0 || l.trim().is_empty() {
            continue;
        }
        let e: Json = serde_json::from_str(l).unwrap();
        let e = &e["entry"];
        for kind in ["Judge", "Effect", "CalibUsed"] {
            if let Some(k) = e[kind]["key"].as_str() {
                ks.push(format!("{kind}:{k}"));
            }
        }
    }
    ks
}

#[test]
fn lib只改注释_账本键逐个相同_旧账本重放零调用() {
    let d = 临时目录();
    // `accepted` 在标准库 `lib/outcome.jpp` 里：复制一份到临时 lib 目录
    std::fs::copy(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../lib/outcome.jpp"),
        d.join("lib/outcome.jpp"),
    )
    .unwrap();
    std::fs::write(d.join("lib/name.jpp"), 库).unwrap();
    std::fs::write(d.join("main.jpp"), 入口).unwrap();
    std::fs::write(d.join("fx.json"), 夹具().to_string()).unwrap();
    let (rc, r1, err) = 跑(
        &d,
        &[
            "main.jpp",
            "--fixtures",
            "fx.json",
            "--ledger-out",
            "a.jsonl",
        ],
    );
    assert_eq!(rc, 0, "{err}");
    let 头 = std::fs::read_to_string(d.join("a.jsonl")).unwrap();
    let 头: Json = serde_json::from_str(头.lines().next().unwrap()).unwrap();
    assert_eq!(头["version"], json!(6));
    assert_eq!(头["header"]["compared"]["key_version"], json!("1"));
    let a = 键集(&d.join("a.jsonl"));
    assert!(
        a.iter().any(|k| k.starts_with("Judge:")) && a.iter().any(|k| k.starts_with("Effect:"))
    );

    // lib 顶部插三行注释：lib 里全部站点的字节偏移后挪
    std::fs::write(
        d.join("lib/name.jpp"),
        format!("// 注释一\n// 注释二\n// 注释三\n{库}"),
    )
    .unwrap();
    let o = std::fs::read_to_string(d.join("lib/outcome.jpp")).unwrap();
    std::fs::write(d.join("lib/outcome.jpp"), format!("// 注释\n{o}")).unwrap();
    let (rc, r2, err) = 跑(
        &d,
        &[
            "main.jpp",
            "--fixtures",
            "fx.json",
            "--ledger-out",
            "b.jsonl",
        ],
    );
    assert_eq!(rc, 0, "{err}");
    assert_eq!(a, 键集(&d.join("b.jsonl")), "账本键逐个相同");
    assert_eq!(r1["value"], r2["value"]);

    // 改注释前的账本只凭账本重放改注释后的 lib：零调用，值不变
    let (rc, r3, err) = 跑(&d, &["main.jpp", "--replay", "a.jsonl"]);
    assert_eq!(rc, 0, "{err}");
    assert_eq!(r3["cost"]["calls"], json!(0), "{r3}");
    assert_eq!(r1["value"], r3["value"]);
    assert!(r3.get("site_key_fallback").is_none(), "没有回退：{r3}");
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn 库内部参数_用户程序报错_lib定义里照常() {
    let d = 临时目录();
    std::fs::write(
        d.join("lib/inner.jpp"),
        "fn 库取(q, m) { unsure_fetch(q, \"语境\", m) }\nfn 库链(e) { unsure_default(e, {end: \"top\"}) }\n",
    )
    .unwrap();
    let 写 = |name: &str, body: &str| {
        std::fs::write(
            d.join(name),
            format!(
                "import \"lib/inner.jpp\";\nbudget {{calls: 1, cost: 0.01, depth: 8}};\n{body}\n"
            ),
        )
        .unwrap();
    };
    写(
        "u1.jpp",
        "unsure_fetch(test(\"甲吗？\", \"k\"), \"语境\", mat(\"x\"))",
    );
    写("u2.jpp", "unsure_default(1, {end: \"top\"})");
    写("l1.jpp", "库取(test(\"甲吗？\", \"k\"), mat(\"x\"))");
    写("l2.jpp", "库链(1)");
    for f in ["u1.jpp", "u2.jpp"] {
        let (rc, _, err) = 跑(&d, &[f]);
        assert_ne!(rc, 0, "{f}");
        assert!(err.contains("E-rt-lib-only"), "{f}：{err}");
    }
    // lib 里的 unsure_fetch 过了站点核对：没注册 fetch，回失败值，程序照常结束
    let (rc, _, err) = 跑(&d, &["l1.jpp"]);
    assert!(!err.contains("E-rt-lib-only"), "{err}");
    assert_eq!(rc, 0, "{err}");
    // lib 里带第二参的 unsure_default 过了站点核对，停在后面的实参检查（1 不是出口）
    let (_, _, err) = 跑(&d, &["l2.jpp"]);
    assert!(!err.contains("E-rt-lib-only"), "{err}");
    assert!(err.contains("E-rt-arg"), "{err}");
    let _ = std::fs::remove_dir_all(&d);
}
