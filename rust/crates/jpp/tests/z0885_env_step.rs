//! Z0885（推进 B0670、B0672）：宿主「跑」最薄一层 = B159 写法二 `env:step` 第一次落实现。
//! 第三靶子预注册 §5.1（`地基/规划/第三靶子-预注册-草稿.md`）；可逆位按裁定六十九（`12` §2.7 B159 附注）。
//!
//! 正面：`.jpp` 经 `--env` 登记的确定性小世界跑 N 步，账本见 N 条 `env:step`（各带世界自报哈希与墙钟），
//! 按账本重放不再启动环境进程（不给 `--env` 也重放成功）、返回值逐字节相同。
//! 反面：环境退出码非 0、输出不合契约、名字没登记，程序都拿到失败值、照常返回；同输入不同输出的环境由重放比对报出。
use serde_json::Value as Json;
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

fn 工作目录(tag: &str) -> PathBuf {
    let d = root().join(format!(
        "target/z0885-{tag}-{}-{}",
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

fn 有_python() -> bool {
    Command::new("python3").arg("-c").arg("pass").output().is_ok_and(|o| o.status.success())
}

/// 确定性小世界：计数器。`state` 为空时按 `reset` 开局；`inc` 加一、`dec` 减一、`stay` 不动；n 到 5 即结束。
/// `mode` 文件控制故意出错的变体（反面用例）：exit = 退出码 3；bad = 输出缺字段；drift = 同输入不同输出。
const 世界: &str = r#"
import sys, json, hashlib, os, time
mode = sys.argv[1] if len(sys.argv) > 1 else "ok"
req = json.loads(sys.stdin.readline())
if mode == "exit":
    sys.stderr.write("boom\n"); sys.exit(3)
st = req.get("state")
if st is None:
    st = {"n": 0, "seed": req.get("reset", 0)}
else:
    a = req.get("action")
    st = {"n": st["n"] + (1 if a == "inc" else -1 if a == "dec" else 0), "seed": st["seed"]}
out = {"state": st, "obs": {"n": st["n"]}, "actions": ["inc", "dec", "stay"], "idle": "stay",
       "done": st["n"] >= 5, "hash": hashlib.sha256(json.dumps(st, sort_keys=True).encode()).hexdigest()[:16]}
if mode == "bad":
    del out["idle"]
if mode == "drift":
    out["obs"]["t"] = time.time()
print(json.dumps(out))
"#;

const 程序: &str = r#"budget {calls: 40, cost: 0, depth: 8};
fn step(acc, i) {
    if acc.w.done { stop(acc) } else {
        let o = do("env:step", [{env: "w", state: acc.w.state, action: "inc"}], 0);
        if is_fail(o) { stop(with(acc, "err", true)) } else {
            let w = content(o);
            {w: w, n: acc.n + 1, hs: append(acc.hs, w.hash), err: false}
        }
    }
}
let o0 = do("env:step", [{env: "w", state: unit, action: unit, reset: 7}], 0);
if is_fail(o0) { {open_failed: true} } else {
    let r = iterate(20, {w: content(o0), n: 0, hs: [], err: false}, step, fn(acc) { 20 - acc.n });
    {reason: r.resume.reason, n: r.value.n, hs: r.value.hs, last: r.value.w.obs, err: r.value.err}
}
"#;

fn 布置(d: &Path) {
    fs::write(d.join("world.py"), 世界).unwrap();
    fs::write(d.join("p.jpp"), 程序).unwrap();
}

fn 跑(d: &Path, mode: &str, extra: &[&str]) -> Output {
    let cmd = format!("w=python3 {} {mode}", d.join("world.py").display());
    let mut a = vec!["run", "p.jpp", "--json", "--env", &cmd];
    a.extend_from_slice(extra);
    jpp(d, &a)
}

fn 值(o: &Output) -> Json {
    let j: Json = serde_json::from_slice(&o.stdout).unwrap_or_else(|e| {
        panic!(
            "stdout 不是 JSON（{e}）：{}{}",
            String::from_utf8_lossy(&o.stdout),
            String::from_utf8_lossy(&o.stderr)
        )
    });
    j.get("value").cloned().unwrap_or(j)
}

fn 账本条目(p: &Path) -> Vec<Json> {
    fs::read_to_string(p)
        .unwrap()
        .lines()
        .skip(1)
        .filter_map(|l| serde_json::from_str::<Json>(l).ok())
        .filter_map(|l| l.get("entry").cloned())
        .collect()
}

fn env_条目(es: &[Json]) -> Vec<Json> {
    es.iter()
        .filter_map(|e| e.get("Effect").cloned())
        .filter(|e| e["kind"] == "do" && e["key"].is_string() && e.get("wall").is_some())
        .collect()
}

/// 正面：跑到 done（5 步），账本 6 条 env:step（开局 1 + 5 步），各带哈希与墙钟；重放不启动进程、值逐字节相同
#[test]
fn 小世界跑到结束_重放不启动进程() {
    if !有_python() {
        eprintln!("跳过：没有 python3");
        return;
    }
    let d = 工作目录("ok");
    布置(&d);
    let o = 跑(&d, "ok", &["--ledger-out", "l.jsonl"]);
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    let v = 值(&o);
    assert_eq!(v["n"], 5, "{v}");
    assert_eq!(v["reason"], "stop", "{v}");
    assert_eq!(v["last"]["n"], 5);
    assert_eq!(v["err"], false);
    let hs = v["hs"].as_array().unwrap();
    assert_eq!(hs.len(), 5);
    let es = 账本条目(&d.join("l.jsonl"));
    let envs = env_条目(&es);
    assert_eq!(envs.len(), 6, "开局 1 + 5 步：{envs:?}");
    for e in &envs {
        let w = e["wall"].as_array().unwrap();
        assert!(w[0].as_f64().unwrap() > 1.0e9 && w[1].as_f64().unwrap() >= w[0].as_f64().unwrap());
        assert!(e["output"]["hash"].is_string());
    }
    // 重放：不给 --env（环境进程无从启动），按账本给出，值逐字节相同
    let r = jpp(&d, &["run", "p.jpp", "--json", "--replay", "l.jsonl"]);
    assert!(r.status.success(), "{}", String::from_utf8_lossy(&r.stderr));
    assert_eq!(值(&r), v);
}

/// 反面：退出码非 0、输出缺字段、名字没登记——程序拿到失败值、照常返回
#[test]
fn 环境出错给失败值() {
    if !有_python() {
        eprintln!("跳过：没有 python3");
        return;
    }
    for mode in ["exit", "bad"] {
        let d = 工作目录(mode);
        布置(&d);
        let o = 跑(&d, mode, &[]);
        assert!(o.status.success(), "{mode}: {}", String::from_utf8_lossy(&o.stderr));
        assert_eq!(值(&o)["open_failed"], true, "{mode}");
    }
    let d = 工作目录("unreg");
    布置(&d);
    let o = jpp(&d, &["run", "p.jpp", "--json", "--env", "other=python3 x.py"]);
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    assert_eq!(值(&o)["open_failed"], true);
}

/// 反面：同输入不同输出的环境（偷留状态或读时钟）——首跑与按缓存重跑的输出不同，重放比对能报出：
/// 同一账本重放给出首跑的值，再首跑一次给出不同的值
#[test]
fn 不守无状态的环境由重放比对报出() {
    if !有_python() {
        eprintln!("跳过：没有 python3");
        return;
    }
    let d = 工作目录("drift");
    布置(&d);
    let a = 跑(&d, "drift", &["--ledger-out", "l.jsonl"]);
    assert!(a.status.success(), "{}", String::from_utf8_lossy(&a.stderr));
    let b = 跑(&d, "drift", &[]);
    let r = jpp(&d, &["run", "p.jpp", "--json", "--replay", "l.jsonl"]);
    assert!(r.status.success(), "{}", String::from_utf8_lossy(&r.stderr));
    assert_eq!(值(&r), 值(&a), "重放给出首跑记下的值");
    assert_ne!(值(&b)["last"], 值(&a)["last"], "同输入第二次首跑输出不同：重放比对报出");
}

/// `--env` 只收 run，名字不重
#[test]
fn 登记开关() {
    let d = 工作目录("flag");
    布置(&d);
    let o = jpp(&d, &["check", "p.jpp", "--env", "w=python3 x.py"]);
    assert!(!o.status.success());
    let o = jpp(&d, &["run", "p.jpp", "--env", "w=python3 x.py", "--env", "w=python3 y.py"]);
    assert!(!o.status.success());
    assert!(String::from_utf8_lossy(&o.stderr).contains("twice"));
}
