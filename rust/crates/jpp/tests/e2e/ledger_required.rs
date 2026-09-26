//! `E-ledger-required`（步 18b，主会话 2026-09-25 裁定）：CLI 跑有不可逆 `do` 的程序，首跑与续接都要
//! `--ledger-out`，否则执行前报错、一个效应都不做；只凭账本重放不要求；没有不可逆 `do` 的程序不受影响。
//!
//! 意图汇编 11a（2026-09-26）起这条只在宿主开 `--guard` 时生效（账本是记录，不是防御）：测机制本身的用例都带
//! `--guard`；默认不停下、账本写到 `<源文件名>.ledger.jsonl` 的对照见 `默认_不给账本文件照常执行_写到默认路径`。

use std::path::{Path, PathBuf};
use std::process::Command;

fn 目录(名: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("jpp-e2e-lr-{名}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn jpp(d: &Path, args: &[&str]) -> (bool, String) {
    let o = Command::new(env!("CARGO_BIN_EXE_jpp"))
        .current_dir(d)
        .args(args)
        .output()
        .unwrap();
    (
        o.status.success(),
        String::from_utf8_lossy(&o.stderr).to_string(),
    )
}

const 写文件: &str =
    "budget {calls: 2, cost: 0};\nis_fail(do(\"write_json\", [\"out.json\", 1], 0))\n";

#[test]
fn 不可逆动作_不给账本文件即停_给了照常() {
    let d = 目录("wj");
    std::fs::write(d.join("p.jpp"), 写文件).unwrap();
    let (ok, err) = jpp(&d, &["run", "p.jpp", "--guard"]);
    assert!(
        !ok && err.contains("E-ledger-required") && err.contains("--ledger-out"),
        "{err}"
    );
    assert!(err.contains("write_json"), "报文写出动作名：{err}");
    assert!(!d.join("out.json").exists(), "执行前停下，动作没做");
    let (ok, err) = jpp(&d, &["run", "p.jpp", "--ledger-out", "l.jsonl", "--guard"]);
    assert!(ok, "{err}");
    assert!(d.join("out.json").exists());
    // 续接同样要求
    let (ok, err) = jpp(&d, &["run", "p.jpp", "--resume", "l.jsonl", "--guard"]);
    assert!(!ok && err.contains("E-ledger-required"), "{err}");
    let (ok, err) = jpp(
        &d,
        &[
            "run",
            "p.jpp",
            "--resume",
            "l.jsonl",
            "--ledger-out",
            "l2.jsonl",
            "--guard",
        ],
    );
    assert!(ok, "{err}");
    // 只凭账本重放不要求
    let (ok, err) = jpp(&d, &["run", "p.jpp", "--replay", "l.jsonl", "--guard"]);
    assert!(ok, "{err}");
    let _ = std::fs::remove_dir_all(&d);
}

/// 意图汇编 11a、B187：不开 `--guard`（默认），有不可逆动作的程序不给 `--ledger-out` 照常执行，账本写到源文件同目录的
/// `p.ledger.jsonl`，stderr 说出路径；这份账本能只凭账本重放，续接从它读、写到 `p.resumed.ledger.jsonl`，不覆盖它。
#[test]
fn 默认_不给账本文件照常执行_写到默认路径() {
    let d = 目录("default");
    std::fs::write(d.join("p.jpp"), 写文件).unwrap();
    let (ok, err) = jpp(&d, &["run", "p.jpp"]);
    assert!(ok, "{err}");
    assert!(!err.contains("E-ledger-required"), "{err}");
    assert!(d.join("out.json").exists(), "动作照常执行");
    assert!(d.join("p.ledger.jsonl").exists(), "账本写到默认路径");
    assert!(err.contains("p.ledger.jsonl"), "提示路径：{err}");
    let (ok, err) = jpp(&d, &["run", "p.jpp", "--replay", "p.ledger.jsonl"]);
    assert!(ok, "{err}");
    let 原账本 = std::fs::read_to_string(d.join("p.ledger.jsonl")).unwrap();
    let (ok, err) = jpp(&d, &["run", "p.jpp", "--resume", "p.ledger.jsonl"]);
    assert!(ok, "{err}");
    assert!(d.join("p.resumed.ledger.jsonl").exists(), "{err}");
    assert_eq!(
        std::fs::read_to_string(d.join("p.ledger.jsonl")).unwrap(),
        原账本,
        "续接的来源不被覆盖"
    );
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn 没有不可逆动作的程序不要求() {
    let d = 目录("rc");
    std::fs::write(
        d.join("p.jpp"),
        "budget {calls: 2, cost: 0};\ncontent(do(\"record_check\", [{a: 1}], 0))\n",
    )
    .unwrap();
    let (ok, err) = jpp(&d, &["run", "p.jpp"]);
    assert!(ok, "{err}");
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn 动作名不是字面量按不可逆处理() {
    let d = 目录("dyn");
    std::fs::write(
        d.join("p.jpp"),
        "budget {calls: 2, cost: 0};\nlet n = \"record_check\";\ncontent(do(n, [{a: 1}], 0))\n",
    )
    .unwrap();
    let (ok, err) = jpp(&d, &["run", "p.jpp", "--guard"]);
    assert!(!ok && err.contains("E-ledger-required"), "{err}");
    let _ = std::fs::remove_dir_all(&d);
}

/// B179 (b)（步 24e-4）：动作名经**形参**转发（`ground(action, ...) { fn(cand) { do(action, …) } }`
/// 这类库函数的真实结构，`lib/compose/ground.jpp` 同形）时，若全程序调用点都给了同一个字面量，
/// 检查器与 CLI 沿这层实参把它当字面量处理——引用它、只传可逆动作名的程序不再被当作「可能不可逆」，
/// 不必要求 `--ledger-out`。
#[test]
fn 引ground同形结构_动作名可逆时不要求账本() {
    let d = 目录("ground-rev");
    std::fs::write(
        d.join("p.jpp"),
        "budget {calls: 2, cost: 0};\n\
         fn ground(action, args_of, render) {\n\
         \x20\x20fn(cand) {\n\
         \x20\x20\x20\x20let out = do(action, args_of(cand), 0);\n\
         \x20\x20\x20\x20if is_fail(out) { out } else { render(cand, out) }\n\
         \x20\x20}\n\
         }\n\
         let runner = ground(\"record_check\", fn(c) { [{a: c}] }, fn(c, o) { content(o) });\n\
         runner(1)\n",
    )
    .unwrap();
    let (ok, err) = jpp(&d, &["run", "p.jpp"]);
    assert!(ok, "{err}");
    let _ = std::fs::remove_dir_all(&d);
}

/// 同一个 `ground` 结构换成不可逆动作名（`write_json`）：仍然要求 `--ledger-out`——沿形参追到的
/// 字面量与直接写字面量受同一条 J-08/`E-ledger-required` 判据。
#[test]
fn 引ground同形结构_动作名不可逆时仍要求账本() {
    let d = 目录("ground-irrev");
    std::fs::write(
        d.join("p.jpp"),
        "budget {calls: 2, cost: 0};\n\
         fn ground(action, args_of, render) {\n\
         \x20\x20fn(cand) {\n\
         \x20\x20\x20\x20let out = do(action, args_of(cand), 0);\n\
         \x20\x20\x20\x20if is_fail(out) { out } else { render(cand, out) }\n\
         \x20\x20}\n\
         }\n\
         let runner = ground(\"write_json\", fn(c) { [\"out.json\", c] }, fn(c, o) { o });\n\
         runner(1)\n",
    )
    .unwrap();
    let (ok, err) = jpp(&d, &["run", "p.jpp", "--guard"]);
    assert!(
        !ok && err.contains("E-ledger-required") && err.contains("write_json"),
        "{err}"
    );
    let (ok, err) = jpp(&d, &["run", "p.jpp", "--ledger-out", "l.jsonl", "--guard"]);
    assert!(ok, "{err}");
    assert!(d.join("out.json").exists());
    let _ = std::fs::remove_dir_all(&d);
}
