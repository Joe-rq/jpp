//! 同键在飞的生成合并（公开 PR #37 评审 P1）：同一刷新前同一个 `gen` 站点被同样的输入走到多次时，共享一个
//! 待取回的生成——只发一次、只计一次，所有同键位置拿到同一输出；只凭账本重放逐字段相同。
//! 假 `claude -p` 脚本（由 `/bin/sh` 执行）每被调一次往计数文件追加一行，并把自己的进程号写进输出，不发任何请求。
//!
//! 依据：B149、B160；预注册订正 `地基/过程记录/工程-步15h-2.md` 三 (a)–(c)。

use jpp::backends::claude_p::{ClaudePConfig, ClaudePPort};
use jpp::effects::{CalibStore, NoCallPorts, ReplayPorts};
use jpp::ledger::Ledger;
use jpp::{ActionRegistry, Outcome, lower, run, run_replay, syntax::parse};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

static NEXT: AtomicU64 = AtomicU64::new(0);

/// 假 `claude`：追加一行到同目录 `calls`，输出 `["进程 <pid>"]`（并发调用时计数会竞争，进程号每次不同）
fn 计数脚本() -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "jpp-gen-same-key-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("claude");
    let body = concat!(
        "d=$(dirname \"$0\")\n",
        "cat > /dev/null\n",
        "echo x >> \"$d/calls\"\n",
        "printf '{\"type\":\"result\",\"is_error\":false,\"result\":\"[\\\\\"进程 %s\\\\\"]\",",
        "\"usage\":{\"input_tokens\":10,\"output_tokens\":5}}' \"$$\"\n",
    );
    std::fs::write(&path, body).unwrap();
    path
}

fn 调用次数(script: &Path) -> usize {
    std::fs::read_to_string(script.with_file_name("calls"))
        .map(|s| s.lines().count())
        .unwrap_or(0)
}

fn 端口(script: &Path) -> ClaudePPort {
    ClaudePPort::new(ClaudePConfig {
        program: "/bin/sh".into(),
        pre_args: vec![script.to_string_lossy().into_owned()],
        model: "sonnet".into(),
        concurrency: 4,
        timeout: Duration::from_secs(20),
        cost_per_call: 0.0,
        taint_out: Some(jpp::Taint::Untrusted),
    })
}

fn 首跑(src: &str, script: &Path, ledger: &mut Ledger) -> Outcome {
    let program = lower(&parse(src).expect("parse")).expect("lower");
    let mut p = 端口(script);
    let mut ports = NoCallPorts::ports();
    ports.replace(Box::new(&mut p));
    run(
        &program,
        ports,
        &CalibStore::new(),
        &ActionRegistry::new(),
        ledger,
    )
    .unwrap_or_else(|e| panic!("{}", e.render()))
}

fn 重放(src: &str, ledger: &mut Ledger) -> Outcome {
    let program = lower(&parse(src).expect("parse")).expect("lower");
    ledger.rebuild_index();
    run_replay(
        &program,
        ReplayPorts::ports("fixed-0"),
        &CalibStore::new(),
        &ActionRegistry::new(),
        ledger,
    )
    .unwrap_or_else(|e| panic!("{}", e.render()))
}

/// (a) map 遍历含重复值的列表：同键位置共享一次生成，脚本只被调 2 次；重放零调用、值逐字段相同
#[test]
fn a_同键只发一次_重放一致() {
    let src = r#"budget {calls: 8, cost: 0, depth: 64};
let gs = map(["甲", "甲", "乙", "甲"], fn(t) { gen("提候选", [mat(t)], 1, 0) });
map(gs, fn(g) { content(g[0]) })"#;
    let script = 计数脚本();
    let mut ledger = Ledger::new();
    let o = 首跑(src, &script, &mut ledger);
    let v = o.value_json();
    assert_eq!(调用次数(&script), 2, "甲、乙各一次");
    assert_eq!(v[0], v[1]);
    assert_eq!(v[0], v[3]);
    assert_ne!(v[0], v[2]);
    assert_eq!(o.cost.calls, 2);
    assert_eq!(o.cost.replayed, 2, "两个同键位置记为不付费");
    let gens = ledger
        .encode()
        .lines()
        .filter(|l| l.contains(r#""kind":"gen""#))
        .count();
    assert_eq!(gens, 2, "账本每键一条");
    let again = 重放(src, &mut ledger);
    assert_eq!(again.value_json(), v, "只凭账本重放逐字段相同");
    assert_eq!(again.cost.calls, 0);
}

/// (b) 同键出现在两层：第一次读取、收层之后再走到同键，走账本命中，不再调用。
/// 两个站点同输入：账本键不同，缓存键相同（步 19，B40），第一个取回之后第二个按缓存键复用
#[test]
fn b_收层之后同键走账本() {
    let src = r#"budget {calls: 8, cost: 0, depth: 64};
let a = gen("提候选", [mat("甲")], 1, 0);
let x = content(a[0]);
let b = gen("提候选", [mat("甲")], 1, 0);
[x, content(b[0])]"#;
    // 两次 gen 是两个站点，账本键不同；同一站点跨层要靠 map
    let src_same_site = r#"budget {calls: 8, cost: 0, depth: 64};
let f = fn(t) { gen("提候选", [mat(t)], 1, 0) };
let a = f("甲");
let x = content(a[0]);
let b = f("甲");
[x, content(b[0])]"#;
    let script = 计数脚本();
    let o = 首跑(src_same_site, &script, &mut Ledger::new());
    let v = o.value_json();
    assert_eq!(调用次数(&script), 1);
    assert_eq!(v[0], v[1]);
    // 步 19（B40）：生成的缓存键不含调用位置（生成器模型、提示、上下文哈希、n、retry_seq）。`content(a[0])`
    // 先把第一个站点的生成取回，第二个站点同输入按缓存键复用：不调用，值相同，账本写一条复用条目
    let script2 = 计数脚本();
    let mut l2 = Ledger::new();
    let o2 = 首跑(src, &script2, &mut l2);
    assert_eq!(
        调用次数(&script2),
        1,
        "两个站点缓存键相同，第二个复用第一个"
    );
    assert_eq!(o2.value_json()[0], o2.value_json()[1]);
    let reused = l2
        .encode()
        .lines()
        .filter(|l| l.contains(r#""kind":"gen""#) && l.contains(r#""reused_from""#))
        .count();
    assert_eq!(reused, 1, "第二个站点记一条复用条目");
    let again = 重放(src, &mut l2);
    assert_eq!(
        again.value_json(),
        o2.value_json(),
        "只凭账本重放逐字段相同"
    );
    assert_eq!(again.cost.calls, 0);
}

/// (c) 不同 retry_seq 不合并（键不同）
#[test]
fn c_不同_retry_seq_不合并() {
    let src = r#"budget {calls: 8, cost: 0, depth: 64};
let gs = map([0, 1, 0], fn(i) { gen("提候选", [mat("甲")], 1, i) });
map(gs, fn(g) { content(g[0]) })"#;
    let script = 计数脚本();
    let o = 首跑(src, &script, &mut Ledger::new());
    let v = o.value_json();
    assert_eq!(调用次数(&script), 2);
    assert_eq!(v[0], v[2]);
    assert_ne!(v[0], v[1]);
    assert_eq!(v.as_array().unwrap().len(), 3);
}
