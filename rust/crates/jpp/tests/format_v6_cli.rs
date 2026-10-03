//! B0630 格式版本与兼容读取的命令行一面（预注册 `规划/B0630-键与格式稳定-预注册.md` §四 F2、F3、§2.3；k-fmt）。
//!
//! 钉的是：新二进制 `--replay` 读旧版本（v3、v4）账本与没有 `key_version` 的账本不崩、报告与原账本重放一致；
//! 版本比本二进制新的账本报 `E-ledger-newer`、退出码非 0、无 panic；头里 `key_version` 不认识报 `E-key-version`
//! （重放与续接都是）；`--cache` 目录里混放各版本账本时都进索引、读不了的计数跳过。
//!
//! 不依赖 `LEDGER_VERSION` 与键法当前值的具体取值：把账本头重写成各版本再重新串链（`rewrite`），随键法切换变的
//! 断言按 `KEY_VERSION_CURRENT` 分支。
use jpp_ledger::key_version::{KEY_VERSION_CURRENT, KeyVersion};
use jpp_ledger::{Header, LEDGER_VERSION, Ledger, encode_head, line_hash};
use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Output},
};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap()
}

fn tmp(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("jpp-fmtcli-{tag}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&d);
    fs::create_dir_all(&d).unwrap();
    d
}

fn jpp(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_jpp"))
        .current_dir(root())
        .args(args)
        .output()
        .unwrap()
}

fn err(o: &Output) -> String {
    String::from_utf8_lossy(&o.stderr).into_owned()
}

/// 读一份账本，头按 `f` 改、头行 `version` 写成 `version`，条目按新头重新串链。
fn rewrite(text: &str, version: u32, f: impl FnOnce(&mut Header)) -> String {
    let (mut l, _) = Ledger::decode(text).unwrap();
    f(l.header.as_mut().expect("账本有头"));
    let head = encode_head(&l.header).replacen(
        &format!("{{\"version\":{LEDGER_VERSION},"),
        &format!("{{\"version\":{version},"),
        1,
    );
    let mut out = format!("{head}\n");
    for line in l.encode_from(0, &line_hash(&head)).0 {
        out.push_str(&line);
        out.push('\n');
    }
    out
}

/// 固定观察跑 `sieve` 首跑，返回（目录、账本文本、账本路径、报告路径）
fn 首跑(tag: &str) -> (PathBuf, String, String, PathBuf) {
    let d = tmp(tag);
    let ledger = d.join("first.ledger").display().to_string();
    let report = d.join("first.json");
    let o = jpp(&[
        "run",
        "examples/sieve.jpp",
        "--fixtures",
        "examples/fixtures/sieve.json",
        "--ledger-out",
        &ledger,
        "--output",
        &report.display().to_string(),
    ]);
    assert!(o.status.success(), "{}", err(&o));
    let text = fs::read_to_string(&ledger).unwrap();
    (d, text, ledger, report)
}

fn 重放(d: &Path, ledger: &str, name: &str) -> (Output, PathBuf) {
    let report = d.join(name);
    let o = jpp(&[
        "run",
        "examples/sieve.jpp",
        "--fixtures",
        "examples/fixtures/sieve.json",
        "--replay",
        ledger,
        "--output",
        &report.display().to_string(),
    ]);
    (o, report)
}

fn 写(d: &Path, name: &str, text: &str) -> String {
    let p = d.join(name);
    fs::write(&p, text).unwrap();
    p.display().to_string()
}

#[test]
fn 首跑写的头_旧键法不带key_version() {
    let (_d, text, _, _) = 首跑("head");
    let head = text.lines().next().unwrap();
    assert!(
        head.starts_with(&format!("{{\"version\":{LEDGER_VERSION},")),
        "{head}"
    );
    assert_eq!(
        head.contains("\"key_version\":\"1\""),
        KEY_VERSION_CURRENT == KeyVersion::Structured,
        "{head}"
    );
    assert_eq!(
        head.contains("key_version"),
        KEY_VERSION_CURRENT != KeyVersion::Offset,
        "旧键法不写字段（账本逐字节不变）：{head}"
    );
}

#[test]
fn 旧版本账本重放不崩_报告与当前版本重放一致() {
    let (d, text, ledger, _) = 首跑("old");
    let (base, r0) = 重放(&d, &ledger, "base.json");
    assert!(base.status.success(), "{}", err(&base));
    for v in [3u32, 4] {
        let p = 写(&d, &format!("v{v}.ledger"), &rewrite(&text, v, |_| {}));
        let (o, r) = 重放(&d, &p, &format!("v{v}.json"));
        assert!(o.status.success(), "v{v}：{}", err(&o));
        assert!(!err(&o).contains("panicked"), "{}", err(&o));
        assert_eq!(
            fs::read(&r0).unwrap(),
            fs::read(&r).unwrap(),
            "v{v} 账本重放的报告与当前版本逐字节相同"
        );
    }
}

#[test]
fn 比本二进制新的格式_报更新_退出码非0_不panic() {
    let (d, text, _, _) = 首跑("newer");
    let n = LEDGER_VERSION + 1;
    let p = 写(&d, "newer.ledger", &rewrite(&text, n, |_| {}));
    for mode in ["--replay", "--resume"] {
        let report = d.join("n.json").display().to_string();
        let o = jpp(&[
            "run",
            "examples/sieve.jpp",
            "--fixtures",
            "examples/fixtures/sieve.json",
            mode,
            &p,
            "--output",
            &report,
        ]);
        assert!(!o.status.success(), "{mode} 要失败");
        let e = err(&o);
        assert!(e.contains("E-ledger-newer"), "{mode}：{e}");
        assert!(e.contains(&format!("v{n}")), "{e}");
        assert!(!e.contains("panicked"), "{e}");
        assert_ne!(o.status.code(), None, "不是被信号杀的");
    }
}

#[test]
fn 头里key_version不认识_重放与续接都报key_version错() {
    let (d, text, _, _) = 首跑("kv9");
    let p = 写(
        &d,
        "kv9.ledger",
        &rewrite(&text, LEDGER_VERSION, |h| {
            h.compared.key_version = Some("9".into());
        }),
    );
    for mode in ["--replay", "--resume"] {
        let report = d.join("k9.json").display().to_string();
        let o = jpp(&[
            "run",
            "examples/sieve.jpp",
            "--fixtures",
            "examples/fixtures/sieve.json",
            mode,
            &p,
            "--output",
            &report,
        ]);
        assert!(!o.status.success(), "{mode} 要失败");
        let e = err(&o);
        assert!(
            e.contains("E-key-version") && e.contains("\"9\""),
            "{mode}：{e}"
        );
        assert!(!e.contains("panicked"), "{e}");
    }
}

#[test]
fn 旧键法账本_续接按当前键法_重放照常() {
    // 首跑账本没有 key_version = 旧键法
    let (d, text, _, _) = 首跑("offset");
    let old = rewrite(&text, LEDGER_VERSION, |h| h.compared.key_version = None);
    let p = 写(&d, "offset.ledger", &old);
    if KEY_VERSION_CURRENT == KeyVersion::Offset {
        let (o, _) = 重放(&d, &p, "o.json");
        assert!(o.status.success(), "{}", err(&o));
    } else {
        // 键法已是结构化（B0630 合入后）：本二进制写不出偏移键的账本，旧键法重放用改键前录下的真账本
        // （`tests/replay/k0/`：改键前的 sieve 金样账本，v5、无 key_version，源码按录制时原样存着）
        let report = d.join("k0.json").display().to_string();
        let o = jpp(&[
            "run",
            "tests/replay/k0/src/examples/sieve.jpp",
            "--fixtures",
            "examples/fixtures/sieve.json",
            "--replay",
            "tests/replay/k0/sieve.ledger.json",
            "--output",
            &report,
        ]);
        assert!(o.status.success(), "{}", err(&o));
        let r: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(&report).unwrap()).unwrap();
        let first: serde_json::Value = serde_json::from_str(
            &fs::read_to_string(root().join("tests/replay/k0/sieve.report.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(r["cost"]["calls"], serde_json::json!(0), "{r}");
        assert_eq!(r["value"], first["value"], "旧键法重放与录制时的值相同");
    }
    let report = d.join("r.json").display().to_string();
    let o = jpp(&[
        "run",
        "examples/sieve.jpp",
        "--fixtures",
        "examples/fixtures/sieve.json",
        "--resume",
        &p,
        "--output",
        &report,
    ]);
    if KEY_VERSION_CURRENT == KeyVersion::Offset {
        assert!(o.status.success(), "{}", err(&o));
    } else {
        assert!(!o.status.success());
        let e = err(&o);
        assert!(
            e.contains("E-key-version") && e.contains("--replay") && e.contains("--cache"),
            "{e}"
        );
    }
}

/// F3：`--cache` 目录里 v3、v4、当前版本账本同时在，加一个比本二进制新的账本与一个垃圾文件：
/// 索引 = 三份可读账本去重后的并集（与只放当前版本一份时的条数相同），读不了的两个计数跳过，不崩。
#[test]
fn cache目录混放各版本账本_都进索引() {
    let (d, text, _, _) = 首跑("cache");
    let 解析 = |e: &str| -> (usize, String) {
        let l = e
            .lines()
            .find(|l| l.starts_with("缓存：从 "))
            .unwrap_or_else(|| panic!("{e}"));
        let n = l
            .trim_start_matches("缓存：从 ")
            .split(' ')
            .next()
            .unwrap()
            .parse()
            .unwrap();
        (n, l.to_string())
    };
    let 跑 = |dir: &Path| -> String {
        let report = d.join("c.json").display().to_string();
        let o = jpp(&[
            "run",
            "examples/sieve.jpp",
            "--fixtures",
            "examples/fixtures/sieve.json",
            "--cache",
            &dir.display().to_string(),
            "--output",
            &report,
        ]);
        assert!(o.status.success(), "{}", err(&o));
        let e = err(&o);
        assert!(!e.contains("panicked"), "{e}");
        e
    };
    // 对照：目录里只有当前版本一份
    let one = d.join("cache-one");
    fs::create_dir_all(&one).unwrap();
    写(&one, "cur.ledger", &text);
    let (n1, l1) = 解析(&跑(&one));
    assert_eq!(n1, 1, "{l1}");
    // 混放
    let mix = d.join("cache-mix");
    fs::create_dir_all(&mix).unwrap();
    写(&mix, "a-v3.ledger", &rewrite(&text, 3, |_| {}));
    写(&mix, "b-v4.ledger", &rewrite(&text, 4, |_| {}));
    写(&mix, "c-cur.ledger", &text);
    写(
        &mix,
        "d-newer.ledger",
        &rewrite(&text, LEDGER_VERSION + 1, |_| {}),
    );
    写(&mix, "e-junk.txt", "not a ledger\n");
    let (n3, l3) = 解析(&跑(&mix));
    assert_eq!(n3, 3, "三份可读账本都进索引：{l3}");
    assert!(
        l3.ends_with("跳过 2 个文件"),
        "新版本与垃圾各计一次跳过：{l3}"
    );
    // 判断、生成、变换条数 = 只放一份时（同内容去重后的并集）
    let 条数 = |l: &str| {
        l.split_once("（")
            .unwrap()
            .1
            .split_once("）")
            .unwrap()
            .0
            .to_string()
    };
    assert_eq!(条数(&l1), 条数(&l3), "{l1} / {l3}");
}
