//! 现场稳定性三修（`地基/过程记录/工程-现场稳定性三修.md`；来由 `地基/案例/02-杭州聚餐/缺口.md` 第七、五、八条）：
//! (1) 网络类端口错误在程序没声明 `budget.absent` 时默认重试、用尽落失败值（判断 `Unsure(absent)`、生成 `Fail`），
//! 记账本、不中止；(2) `read_json` 先按程序文件所在目录找、再按当前目录；(3) 生成器回复放宽（n = 1 收单个值、
//! 剥代码块、项数不对只报 `W-gen-count`，多的截断、少的照返）。默认构建下不发任何请求。

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
};
use std::time::{Duration, Instant};

use jpp::backends::claude_p::{ClaudePConfig, ClaudePPort};
use jpp::effects::{
    CalibStore, EffectError, FnPort, JevClient, JevPorts, NoCallPorts, Ports, ReplayPorts,
};
use jpp::ledger::Ledger;
use jpp::{ActionRegistry, Outcome, lower, run, run_replay, syntax::parse};
use serde_json::{Value as Json, json};

static NEXT: AtomicU64 = AtomicU64::new(0);

fn 临时目录(名: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!(
        "jpp-field-{名}-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir_all(&d).unwrap();
    // macOS 的临时目录经符号链接（/var → /private/var）：取真实路径，报文里的「当前目录」按真实路径写
    std::fs::canonicalize(&d).unwrap()
}

// ---------- (1) 判断：网络类错误 ----------

const 判断程序: &str = "budget {calls: 10, cost: 0, depth: 8};\nlet e = cut(judge(state(mat(\"材料\")), test(\"行吗\", \"k\")));\nhandle(e, {act: fn() { \"act\" }, ignore: fn() { \"ignore\" }, unsure: fn(u) { {c: unsure_cause(u), exit: u} }})\n";

fn 线() -> CalibStore {
    let mut c = CalibStore::new();
    c.put("k", 0.65, 0.35, 50, "上岗", Some(0.05)).unwrap();
    c
}

/// 前 `坏` 次请求报 `错`，之后答 0.9；`n` 数请求次数
fn 传输(坏: u64, 错: EffectError, n: Arc<AtomicU64>) -> jpp::effects::Attempt {
    Arc::new(move |_body: &Json| {
        let k = n.fetch_add(1, Ordering::SeqCst) + 1;
        if k <= 坏 {
            return Err(错.clone());
        }
        Ok(json!({"answers": {"q0": {"noul": 0.9}}}))
    })
}

fn 跑判断(坏: u64, 错: EffectError, ledger: &mut Ledger) -> (Result<Outcome, String>, u64) {
    let program = lower(&parse(判断程序).expect("解析")).expect("lower");
    let n = Arc::new(AtomicU64::new(0));
    let mut c = JevPorts::new(JevClient::with_transport("jev-1.13.0", {
        let a = 传输(坏, 错, n.clone());
        Box::new(move |b: &Json| a(b))
    }));
    let o = run(&program, c.ports(), &线(), &ActionRegistry::new(), ledger).map_err(|e| e.render());
    (o, n.load(Ordering::SeqCst))
}

fn 网络错() -> EffectError {
    EffectError::network("Connection Failed: tls connection init failed: unexpected end of file")
}

/// (a) 每次都报网络类错误、程序没声明 absent：重试 2 次后出口 Unsure(absent)，程序返回；账本记缺席；
/// 审计重放 0 调用、值相同；换成正常端口续跑，重新发问，出口 act。
#[test]
fn a_网络错误用尽重试落缺席_程序照常() {
    let mut l = Ledger::new();
    let t0 = Instant::now();
    let (o, 请求) = 跑判断(100, 网络错(), &mut l);
    let o = o.expect("网络类错误不中止程序");
    assert_eq!(o.value_json()["c"], json!("absent"));
    assert_eq!(请求, 3, "首发 + 重试 2 次");
    assert_eq!(o.cost.calls, 3, "每次发出都计一次调用");
    assert!(t0.elapsed() >= Duration::from_secs(3), "退避 1 秒、2 秒");
    let 账 = l.encode();
    assert!(
        账.contains("\"Absent\"") && 账.contains("tls connection init failed"),
        "{账}"
    );
    assert!(
        o.trace.warnings.iter().any(|w| w.starts_with("W-absent")),
        "{:?}",
        o.trace.warnings
    );
    // 审计重放：照记录给出，不发请求
    let program = lower(&parse(判断程序).expect("解析")).expect("lower");
    l.rebuild_index();
    let r = run_replay(
        &program,
        ReplayPorts::ports("jev-1.13.0"),
        &线(),
        &ActionRegistry::new(),
        &mut l,
    )
    .unwrap_or_else(|e| panic!("{}", e.render()));
    assert_eq!(r.value_json(), o.value_json(), "只凭账本重放同值");
    // G5（步 38，裁定五十九第 16 条、六十一主控暂定 (b)）：推翻 PR #48 的「续跑不重发」——缺席不是答案，续跑重发
    // 这道题（判断器已恢复），出口 act；账本同键一条缺席、一条答案（本条原断言「续跑不重发、与首跑同值」）
    let (o2, 请求2) = 跑判断(0, 网络错(), &mut l);
    assert_eq!(
        o2.expect("续跑").value_json(),
        json!("act"),
        "续跑重发、得到答案"
    );
    assert_eq!(请求2, 1, "续跑重发缺席的题 1 次");
    let 缺 = l
        .entries
        .iter()
        .filter(|e| matches!(e, jpp::ledger::Entry::Absent { .. }))
        .count();
    let 答 = l
        .entries
        .iter()
        .filter(|e| matches!(e, jpp::ledger::Entry::Judge { .. }))
        .count();
    assert_eq!((缺, 答), (1, 1), "账本记两次：一次缺席、一次答案");
    // A-4：续跑账本的审计重放零调用、值与续跑相同
    l.rebuild_index();
    let r2 = run_replay(
        &program,
        ReplayPorts::ports("jev-1.13.0"),
        &线(),
        &ActionRegistry::new(),
        &mut l,
    )
    .unwrap_or_else(|e| panic!("{}", e.render()));
    assert_eq!(r2.value_json(), json!("act"));
}

/// (b) 网络类错误只出现一次：重试成功，出口 act，两次都计调用。
#[test]
fn b_网络错误一次_重试成功() {
    let (o, 请求) = 跑判断(1, 网络错(), &mut Ledger::new());
    let o = o.expect("跑完");
    assert_eq!(o.value_json(), json!("act"));
    assert_eq!(请求, 2);
    assert_eq!(o.cost.calls, 2);
}

/// (c) 非网络类端口错误、没声明 absent：照旧 E-rt-client，只发 1 次。
#[test]
fn c_非网络错误照旧中止() {
    let (o, 请求) = 跑判断(100, EffectError("HTTP 401".into()), &mut Ledger::new());
    let e = o.expect_err("非网络类错误照旧是运行期错误");
    assert!(e.contains("E-rt-client") && e.contains("HTTP 401"), "{e}");
    assert_eq!(请求, 1);
}

/// 按序给回复：第 k 次请求取 `seq[k]`，用完后重复最后一个；`n` 数请求次数
fn 序列跑判断(
    seq: Vec<Result<Json, EffectError>>,
    ledger: &mut Ledger,
) -> (Result<Outcome, String>, u64) {
    let program = lower(&parse(判断程序).expect("解析")).expect("lower");
    let n = Arc::new(AtomicU64::new(0));
    let n2 = n.clone();
    let mut c = JevPorts::new(JevClient::with_transport("jev-1.13.0", {
        Box::new(move |_b: &Json| {
            let k = n2.fetch_add(1, Ordering::SeqCst) as usize;
            seq[k.min(seq.len() - 1)].clone()
        })
    }));
    let o = run(&program, c.ports(), &线(), &ActionRegistry::new(), ledger).map_err(|e| e.render());
    (o, n.load(Ordering::SeqCst))
}

/// (h) 公开 PR #48 Codex 意见（flush.rs:571）：首发网络类错误、重试里遇到 HTTP 400，不再重试、不转缺席，
/// 报 E-rt-client，报文是 400 那一次的原文；两次请求都计调用。
#[test]
fn h_隐式重试遇到非网络错误_报客户端错误() {
    let mut l = Ledger::new();
    let (o, 请求) = 序列跑判断(
        vec![
            Err(网络错()),
            Err(EffectError("HTTP 400 Bad Request".into())),
        ],
        &mut l,
    );
    let e = o.expect_err("非网络类错误不转缺席");
    assert!(e.contains("E-rt-client") && e.contains("HTTP 400"), "{e}");
    assert_eq!(请求, 2, "首发 + 一次重试，遇 400 即停，不再重试");
    assert!(
        !l.encode().contains("\"Absent\""),
        "不记缺席账：{}",
        l.encode()
    );
}

/// (i) 同上，重试拿到的回复解析失败（答案形状不对）：同样报 E-rt-client，不转缺席。
#[test]
fn i_隐式重试遇到解析失败_报客户端错误() {
    let mut l = Ledger::new();
    let (o, 请求) = 序列跑判断(vec![Err(网络错()), Ok(json!({"unexpected": true}))], &mut l);
    let e = o.expect_err("解析失败不转缺席");
    assert!(e.contains("E-rt-client"), "{e}");
    assert!(!e.contains("absent"), "{e}");
    assert_eq!(请求, 2);
    assert!(!l.encode().contains("\"Absent\""), "{}", l.encode());
}

/// (g) 传输层超时标成网络类；4xx 不是。
#[test]
fn g_超时是网络类_4xx不是() {
    let 挂: jpp::effects::Attempt = Arc::new(|_b: &Json| {
        std::thread::sleep(Duration::from_millis(500));
        Ok(json!({}))
    });
    let a = jpp::effects::timed_attempt(Some(Duration::from_millis(50)), 挂);
    let e = a(&json!({})).expect_err("超时");
    assert!(e.is_network() && e.0.contains("E-timeout"), "{}", e.0);
    assert!(!EffectError("HTTP 401".into()).is_network());
}

// ---------- (1) 生成：网络类错误 ----------

const 生成程序: &str = r#"budget {calls: 4, cost: 0, depth: 64};
let xs = gen("提 3 个候选名字", [mat("需求")], 3, 0);
if is_fail(xs) { {fail: true} } else { {fail: false} }"#;

/// (d) 生成端口取回时报网络类错误：值是 Fail，程序照常返回，账本记下；审计重放同值、0 调用。
#[test]
fn d_生成网络错误落fail() {
    let program = lower(&parse(生成程序).expect("解析")).expect("lower");
    let mut l = Ledger::new();
    let mut p = FnPort::generate("sonnet", |_p, _c, _n, _r| {
        Err(EffectError::network("connection reset by peer"))
    });
    let mut ports = NoCallPorts::ports();
    ports.replace(Box::new(&mut p));
    let o = run(
        &program,
        ports,
        &CalibStore::new(),
        &ActionRegistry::new(),
        &mut l,
    )
    .unwrap_or_else(|e| panic!("网络类错误不中止：{}", e.render()));
    assert_eq!(o.value_json(), json!({"fail": true}));
    let 账 = l.encode();
    assert!(
        账.lines()
            .any(|x| x.contains(r#""kind":"gen""#) && x.contains("gen network: connection reset")),
        "{账}"
    );
    l.rebuild_index();
    let r = run_replay(
        &program,
        ReplayPorts::ports("fixed-0"),
        &CalibStore::new(),
        &ActionRegistry::new(),
        &mut l,
    )
    .unwrap_or_else(|e| panic!("{}", e.render()));
    assert_eq!(r.value_json(), o.value_json());
    assert_eq!(r.cost.calls, 0);
}

// ---------- (3) 生成器回复放宽 ----------

/// 假 `claude`：读掉 stdin，按 `result` 文本回一次正常结果
fn 假生成器(result: &str) -> PathBuf {
    let d = 临时目录("gen");
    let path = d.join("claude");
    let outer = json!({"type": "result", "is_error": false, "result": result,
                       "usage": {"input_tokens": 10, "output_tokens": 5}});
    std::fs::write(
        &path,
        format!(
            "cat > /dev/null\nprintf '%s' '{}'\n",
            outer.to_string().replace('\'', "'\\''")
        ),
    )
    .unwrap();
    path
}

fn 端口(script: &Path) -> ClaudePPort {
    ClaudePPort::new(ClaudePConfig {
        program: "/bin/sh".into(),
        pre_args: vec![script.to_string_lossy().into_owned()],
        model: "sonnet".into(),
        concurrency: 2,
        timeout: Duration::from_secs(20),
        cost_per_call: 0.0,
        taint_out: Some(jpp::Taint::Untrusted),
    })
}

fn 跑生成(src: &str, result: &str, ledger: &mut Ledger) -> Outcome {
    let program = lower(&parse(src).expect("解析")).expect("lower");
    let mut p = 端口(&假生成器(result));
    let mut ports: Ports<'_> = NoCallPorts::ports();
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

fn 取内容(n: usize) -> String {
    format!(
        "budget {{calls: 4, cost: 0, depth: 64}};\nlet gs = gen(\"出候选\", [mat(\"需求\")], {n}, 0);\nif is_fail(gs) {{ \"fail\" }} else {{ map(gs, fn(g) {{ content(g) }}) }}"
    )
}

fn 有告警(o: &Outcome, 码: &str) -> bool {
    o.trace.warnings.iter().any(|w| w.starts_with(码))
}

/// (e1) n = 1，回一个对象（不套数组）：当一项。
#[test]
fn e1_n为1收单个值() {
    let o = 跑生成(
        &取内容(1),
        r#"{"root": "要不要去", "kids": []}"#,
        &mut Ledger::new(),
    );
    assert_eq!(o.value_json(), json!([{"root": "要不要去", "kids": []}]));
    assert!(!有告警(&o, "W-gen-count"));
}

/// (e2) 说明文字 + 代码块：取代码块内容解析。
#[test]
fn e2_剥代码块() {
    let o = 跑生成(
        &取内容(1),
        "好的，候选如下：\n```json\n[\"湖边茶馆\"]\n```\n以上。",
        &mut Ledger::new(),
    );
    assert_eq!(o.value_json(), json!(["湖边茶馆"]));
}

/// (e3) n = 3 回 4 项：截到 3 项、报 W-gen-count，账本记 3 项，重放同值。
#[test]
fn e3_多了截断并告警() {
    let mut l = Ledger::new();
    let o = 跑生成(&取内容(3), r#"["甲","乙","丙","丁"]"#, &mut l);
    assert_eq!(o.value_json(), json!(["甲", "乙", "丙"]));
    assert!(有告警(&o, "W-gen-count"), "{:?}", o.trace.warnings);
    let 账 = l.encode();
    assert!(
        账.contains(r#""output":["甲","乙","丙"]"#),
        "账本记截断后的输出：{账}"
    );
    let program = lower(&parse(&取内容(3)).expect("解析")).expect("lower");
    l.rebuild_index();
    let r = run_replay(
        &program,
        ReplayPorts::ports("fixed-0"),
        &CalibStore::new(),
        &ActionRegistry::new(),
        &mut l,
    )
    .unwrap_or_else(|e| panic!("{}", e.render()));
    assert_eq!(r.value_json(), o.value_json());
}

/// (e4) n = 3 回 2 项：照返 2 项、报 W-gen-count（端口不再把项数不对当失败）。
#[test]
fn e4_少了照返并告警() {
    let o = 跑生成(&取内容(3), r#"["甲","乙"]"#, &mut Ledger::new());
    assert_eq!(o.value_json(), json!(["甲", "乙"]));
    assert!(有告警(&o, "W-gen-count"), "{:?}", o.trace.warnings);
}

// ---------- (2) read_json 的路径 ----------

const 读程序: &str = "budget {calls: 2, cost: 0, depth: 8};\nlet d = do(\"read_json\", [\"data.json\"], 0);\nif is_fail(d) { {读到: false, 原因: text(d)} } else { {读到: true, a: content(d).a} }\n";

/// 在 `cwd` 下跑 `程序目录/p.jpp`，返回报告里的值
fn 跑读(程序目录: &Path, cwd: &Path) -> Json {
    std::fs::write(程序目录.join("p.jpp"), 读程序).unwrap();
    let out = cwd.join("report.json");
    let o = Command::new(env!("CARGO_BIN_EXE_jpp"))
        .current_dir(cwd)
        .args([
            "run",
            &程序目录.join("p.jpp").display().to_string(),
            "--output",
            &out.display().to_string(),
        ])
        .output()
        .unwrap();
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    let r: Json = serde_json::from_str(&std::fs::read_to_string(&out).unwrap()).unwrap();
    r["value"].clone()
}

/// (f1) 程序目录里有文件、从别的目录跑：读到程序目录的那份。
#[test]
fn f1_先按程序目录找() {
    let (程序目录, cwd) = (临时目录("prog"), 临时目录("cwd"));
    std::fs::write(程序目录.join("data.json"), r#"{"a": "程序目录"}"#).unwrap();
    std::fs::write(cwd.join("data.json"), r#"{"a": "当前目录"}"#).unwrap();
    let v = 跑读(&程序目录, &cwd);
    assert_eq!(v, json!({"读到": true, "a": "程序目录"}));
}

/// (f2) 程序目录没有、当前目录有：读到当前目录那份。
#[test]
fn f2_程序目录没有再按当前目录() {
    let (程序目录, cwd) = (临时目录("prog"), 临时目录("cwd"));
    std::fs::write(cwd.join("data.json"), r#"{"a": "当前目录"}"#).unwrap();
    let v = 跑读(&程序目录, &cwd);
    assert_eq!(v, json!({"读到": true, "a": "当前目录"}));
}

/// (f3) 两处都没有：失败值说明里列出两个完整路径。
#[test]
fn f3_都没有时报两个路径() {
    let (程序目录, cwd) = (临时目录("prog"), 临时目录("cwd"));
    let v = 跑读(&程序目录, &cwd);
    assert_eq!(v["读到"], json!(false));
    let 因 = v["原因"].as_str().unwrap_or_default();
    assert!(
        因.contains(&程序目录.join("data.json").display().to_string())
            && 因.contains(&cwd.join("data.json").display().to_string()),
        "{因}"
    );
}
