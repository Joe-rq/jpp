//! 宿主动作表（B150；`20` v2 §2.1 L6、§五 S14）：`do` 能触发的每个动作在 [`builtin_actions`]
//! 里占一行。
//!
//! 注册（[`register_all`]）、检查期事实表（[`check_table`]，J-08 静态子面与 J-11 用）、帮助文本的
//! 动作清单（[`usage_list`]）都从这张表生成——步 24c「单一来源、避免漂移」的延续。加动作（S14）：
//! 在 `actions/<group>.rs` 写一个 `run` 函数，在表里加一行；需要宿主状态的放进 [`Ctx`]。
//! CLI（bin 目标的 `cli/runner.rs`）只引用这里。
//!
//! **B164（2026-09-26）**：`exec_py`/`check_tests`/`exec_sql` 三个执行器动作的 `reversible`
//! 不再是编译期写死的常量——宿主启动时探测一次操作系统沙箱（`sandbox::kind()`，缓存），
//! `reversible` 由 `sandbox.kind != none` 派生；因此表从 `const` 改成 [`builtin_actions`]
//! （`OnceLock` 缓存，只在第一次访问时按探测结果建表，之后不变）。
//!
//! 过程记录：`地基/过程记录/工程-步C-1.md`（收成一张表）、`工程-步C-1b.md`（挪到 lib 目标）、
//! `工程-比赛R2b.md`（`graph:*` 六个精确算法动作，rebase 后接入本表）、
//! `工程-比赛R2a.md`（`exec_py`/`check_tests`/`embed_topk`/`bm25_topk`，rebase 后接入本表）、
//! `工程-比赛R2a-exec_sql.md`（`exec_sql`）、`工程-执行器动作安全修补.md`（B164：沙箱、
//! 动态 `reversible`、`E-action-no-sandbox`、`exec_sql` 的 `Fail(Denied)`）。

mod env;
mod exec;
mod graph;
mod graph_cycles;
mod io;
mod retrieval;
mod sandbox;
mod subprocess_util;
mod values_util;

use crate::interp::{ActionRegistry, TaintOut};
use crate::value::Value;
use serde_json::Value as Json;
use std::sync::OnceLock;

pub use env::parse_env_flag;
use std::{cell::RefCell, rc::Rc};

/// 动作运行时能碰到的宿主状态：`record_check` 的检查记录（进报告 `local_checks`），与程序文件所在目录。
#[derive(Clone, Default)]
pub struct Ctx {
    pub checks: Rc<RefCell<Vec<Json>>>,
    /// 程序文件所在目录（现场稳定性三修 (2)）：`read_json` 的相对路径先按它找，找不到再按当前目录。
    /// CLI 从程序路径填；库调用方不填时只按当前目录（与改前相同）。
    pub program_dir: Option<std::path::PathBuf>,
    /// 宿主登记的世界（Z0885，`--env <名字>=<命令>`）：名字 → 命令（程序与参数）。`env:step` 按名字找命令；
    /// 没登记的名字给失败值并列出已登记的
    pub envs: Rc<std::collections::BTreeMap<String, Vec<String>>>,
}

/// 画像 `actions` 分表里执行器动作的 `sandbox` 描述（B164）：`kind` 是
/// `"sandbox-exec"|"bwrap"|"none"`（宿主启动时探测得到，`none` 表示两者都没有）；`fs`/`net`/
/// `undo` 是这套隔离方案本身固定的性质，不随探测结果变——写限定在每次调用新建的临时目录
/// （`fs: "tmpdir"`）、网络一律拒绝（`net: false`）、没有额外的撤销动作，调用结束整目录丢弃
/// 就是全部状态清理（`undo: "discard-tmpdir"`）。
pub struct SandboxProfile {
    pub kind: &'static str,
    pub fs: &'static str,
    pub net: bool,
    pub undo: &'static str,
}

/// 动作可撤回性的三个取值（C-7，Z0173）。
///
/// **「可撤回」的定义**：运行完动作后，用户可见的宿主状态能回到运行前，不需要动作之外的手段（备份、
/// 人工、对方系统配合）。用户不可见、可再生的副产物（缓存、调用专属临时目录）不破坏可撤回，但理由里
/// 必须如实写出。出处：J-08 的语义（只有不可逆动作才要放行，`guard.rs::release`）；B55（不可逆动作先写
/// 意向，因为可能已完成而无法确认）；B164（可撤回性由隔离机制成立：写限定在调用专属临时目录，调用结束整
/// 目录丢弃）；主控 2026-09-29 裁定（定义改为「用户可见的宿主状态能回到运行前」）。
///
/// - `Reversible`：定义成立；
/// - `Irreversible`：至少一种成功运行会留下运行时无法撤回的宿主或外部状态；
/// - `DependsOnArgs`：成立与否取决于调用参数或外部系统，静态无法定，`conditions` 写明取决于什么。
///   目前 14 个内置动作没有一个属于此类（`exec_sql` 的授权回调拒绝一切非 SELECT，任何语句都改不了库，
///   所以是可撤回，不是「视语句而定」）；词汇表保留它，给宿主登记的动作（画像 `actions` 分表、B2 的世界动作）用。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Reversibility {
    Reversible,
    Irreversible,
    DependsOnArgs,
}

impl Reversibility {
    pub fn as_str(self) -> &'static str {
        match self {
            Reversibility::Reversible => "reversible",
            Reversibility::Irreversible => "irreversible",
            Reversibility::DependsOnArgs => "depends_on_args",
        }
    }
    /// 与 J-08 布尔的映射：只有 `Reversible` 为真；拿不准（`DependsOnArgs`）按可能不可逆，与今天
    /// 「动作名不是字面量按可能不可逆处理」一致。
    pub fn as_bool(self) -> bool {
        self == Reversibility::Reversible
    }
}

/// 一个动作的可撤回性事实：三值、理由（为什么）、成立条件（在什么前提下成立，可为空）。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UndoFact {
    pub kind: Reversibility,
    pub reason: String,
    pub conditions: Vec<String>,
}

fn yes(reason: &str, conditions: &[&str]) -> UndoFact {
    UndoFact {
        kind: Reversibility::Reversible,
        reason: reason.to_string(),
        conditions: conditions.iter().map(|c| c.to_string()).collect(),
    }
}

fn no(reason: &str, conditions: &[&str]) -> UndoFact {
    UndoFact {
        kind: Reversibility::Irreversible,
        reason: reason.to_string(),
        conditions: conditions.iter().map(|c| c.to_string()).collect(),
    }
}

/// 执行器动作的事实：由沙箱探测派生（B164），与 `executor_reversible()` 同一来源。
fn executor(name: &str) -> UndoFact {
    let kind = sandbox::kind();
    let what = match name {
        "exec_sql" => {
            "只读打开数据库（mode=ro + PRAGMA query_only + 授权回调只放行 SELECT，任何语句都改不了库）"
        }
        "check_tests" => "运行被测代码与断言",
        _ => "运行给定的 Python 代码",
    };
    if kind == sandbox::SandboxKind::None {
        UndoFact {
            kind: Reversibility::Irreversible,
            reason: format!(
                "{what}；本机探测不到操作系统沙箱，以普通子进程运行，代码可写宿主文件、可联网，运行时无法撤回"
            ),
            conditions: vec!["探测不到 sandbox-exec（macOS）或 bwrap（Linux）".into()],
        }
    } else {
        UndoFact {
            kind: Reversibility::Reversible,
            reason: format!(
                "{what}；在调用专属临时目录内运行，操作系统沙箱把写限定在该目录、拒绝一切网络，调用结束整目录丢弃。读宿主文件不隔离；CPU、内存、进程数不限"
            ),
            conditions: vec![format!("沙箱：{}", kind.as_str())],
        }
    }
}

/// 表里的一行。
pub struct HostAction {
    /// `do` 的动作名
    pub name: &'static str,
    /// 帮助文本里的写法
    pub usage: &'static str,
    /// 可逆（J-08：不可逆动作要放行）。执行器动作（`sandbox.is_some()`）的这一位是派生值
    /// （`sandbox.kind != "none"`），不再单独声明；其余动作照常手写。
    /// C-7 之后它与 [`HostAction::undo`] 的三值一致（`Reversible` ⇔ 真），测试逐项断言。
    pub reversible: bool,
    /// 如实的可撤回性事实（C-7，Z0173）：三值、理由、成立条件。只是事实与记录：J-08、`--guard`、
    /// 意向账本仍只读上面的布尔，不读它，不据此加任何默认限制（意图汇编 11a）。
    pub undo: UndoFact,
    /// 输出 taint 声明（B37、`12` §2.11）
    pub taint_out: TaintOut,
    /// 每次执行的固定费用
    pub cost: f64,
    pub run: fn(&Ctx, &[Value]) -> Result<Value, String>,
    /// 只有执行器动作（`exec_py`/`check_tests`/`exec_sql`）有；见 [`SandboxProfile`]。
    pub sandbox: Option<SandboxProfile>,
}

fn executor_sandbox_profile() -> SandboxProfile {
    SandboxProfile {
        kind: sandbox::kind().as_str(),
        fs: "tmpdir",
        net: false,
        undo: "discard-tmpdir",
    }
}

fn executor_reversible() -> bool {
    sandbox::kind() != sandbox::SandboxKind::None
}

static BUILTIN_ACTIONS_CELL: OnceLock<Vec<HostAction>> = OnceLock::new();

/// 注册的全部动作（顺序即帮助文本里的顺序）。第一次调用时按宿主启动探测到的沙箱结果建表
/// （B164），此后缓存不变——`register_all`/`check_table`/`usage_list` 都从这里取，单一来源。
pub fn builtin_actions() -> &'static [HostAction] {
    BUILTIN_ACTIONS_CELL.get_or_init(|| {
        vec![
            HostAction {
                name: "record_check",
                usage: "record_check",
                reversible: true,
                undo: yes("只把一条记录追加到本次运行的内存检查表（进报告 local_checks），不碰宿主文件与外部系统；进程结束即消失", &[]),
                taint_out: TaintOut::Inherit,
                cost: 0.0,
                run: io::record_check,
                sandbox: None,
            },
            HostAction {
                name: "read_json",
                usage: "read_json(path)",
                reversible: true,
                undo: yes("只读一个文件并解析，不写任何东西", &[]),
                taint_out: TaintOut::Untrusted,
                cost: 0.0,
                run: io::read_json,
                sandbox: None,
            },
            HostAction {
                name: "write_json",
                usage: "write_json(path,value)",
                reversible: false,
                undo: no("创建或覆盖 path 指向的宿主文件；运行时不备份原内容，也没有删除或还原的动作，写完就无法由运行时撤回", &["path 指向已存在文件时原内容丢失；指向新路径时留下新文件"]),
                taint_out: TaintOut::Inherit,
                cost: 0.0,
                run: io::write_json,
                sandbox: None,
            },
            // 比赛 R2b（2026-09-25/26）：六个精确图算法动作，纯函数、可逆、taint 继承、cost 0
            // （附注 §五；实现与设计决定见 `地基/过程记录/工程-比赛R2b.md`）。
            HostAction {
                name: "graph:matching",
                usage: "graph:matching(graph)",
                reversible: true,
                undo: yes("纯函数：只读入参、返回新值，不碰宿主与外部系统", &[]),
                taint_out: TaintOut::Inherit,
                cost: 0.0,
                run: graph::matching,
                sandbox: None,
            },
            HostAction {
                name: "graph:shortest_path",
                usage: "graph:shortest_path(graph)",
                reversible: true,
                undo: yes("纯函数：只读入参、返回新值，不碰宿主与外部系统", &[]),
                taint_out: TaintOut::Inherit,
                cost: 0.0,
                run: graph::shortest_path,
                sandbox: None,
            },
            HostAction {
                name: "graph:max_clique",
                usage: "graph:max_clique(graph)",
                reversible: true,
                undo: yes("纯函数：只读入参、返回新值，不碰宿主与外部系统", &[]),
                taint_out: TaintOut::Inherit,
                cost: 0.0,
                run: graph::max_clique,
                sandbox: None,
            },
            HostAction {
                name: "graph:components",
                usage: "graph:components(graph)",
                reversible: true,
                undo: yes("纯函数：只读入参、返回新值，不碰宿主与外部系统", &[]),
                taint_out: TaintOut::Inherit,
                cost: 0.0,
                run: graph::components,
                sandbox: None,
            },
            HostAction {
                name: "graph:set_cover",
                usage: "graph:set_cover(graph)",
                reversible: true,
                undo: yes("纯函数：只读入参、返回新值，不碰宿主与外部系统", &[]),
                taint_out: TaintOut::Inherit,
                cost: 0.0,
                run: graph::set_cover,
                sandbox: None,
            },
            HostAction {
                name: "graph:max_flow",
                usage: "graph:max_flow(graph)",
                reversible: true,
                undo: yes("纯函数：只读入参、返回新值，不碰宿主与外部系统", &[]),
                taint_out: TaintOut::Inherit,
                cost: 0.0,
                run: graph::max_flow,
                sandbox: None,
            },
            // T0（步 39，Z0454）：S14 扩展点加的第七个图算法动作，找经过指定节点的有向简单环，按节点集合交出
            // （契约与六项隐性知识见 `graph_cycles.rs` 头注与 `地基/过程记录/工程-T0-夹具端口.md`）。
            HostAction {
                name: "graph:cycles",
                usage: "graph:cycles(graph)",
                reversible: true,
                undo: yes("纯函数：只读入参、返回新值，不碰宿主与外部系统", &[]),
                taint_out: TaintOut::Inherit,
                cost: 0.0,
                run: graph_cycles::cycles,
                sandbox: None,
            },
            // 比赛 R2a（2026-09-25/26）：执行器（`exec_py`/`check_tests`，B164 起 `reversible`
            // 由沙箱探测派生）与检索（`embed_topk` 子进程调离线 MiniLM、`bm25_topk` 纯 Rust，
            // 不需要沙箱——不执行任意代码）（附注 §五；实现见 `工程-比赛R2a.md`）。
            HostAction {
                name: "exec_py",
                usage: "exec_py(code,stdin,timeout_s)",
                reversible: executor_reversible(),
                undo: executor("exec_py"),
                taint_out: TaintOut::Untrusted,
                cost: 0.0,
                run: exec::exec_py,
                sandbox: Some(executor_sandbox_profile()),
            },
            HostAction {
                name: "check_tests",
                usage: "check_tests(code,tests,timeout_s)",
                reversible: executor_reversible(),
                undo: executor("check_tests"),
                taint_out: TaintOut::Untrusted,
                cost: 0.0,
                run: exec::check_tests,
                sandbox: Some(executor_sandbox_profile()),
            },
            HostAction {
                name: "embed_topk",
                usage: "embed_topk(texts,query,k)",
                reversible: true,
                undo: yes("读入文本、起 Python 子进程算向量；会写可再生缓存 ~/.cache/jpp-embed，不改用户数据；删掉缓存即回到运行前", &["写 ~/.cache/jpp-embed 下的缓存文件（可再生）"]),
                taint_out: TaintOut::Inherit,
                cost: 0.0,
                run: retrieval::embed_topk,
                sandbox: None,
            },
            HostAction {
                name: "bm25_topk",
                usage: "bm25_topk(query,corpus,k)",
                reversible: true,
                undo: yes("纯 Rust 计算：只读入参、返回新值，不碰宿主与外部系统", &[]),
                taint_out: TaintOut::Inherit,
                cost: 0.0,
                run: retrieval::bm25_topk,
                sandbox: None,
            },
            // exec_sql 是 24e-1 同一块的补齐项（R2a 合入后追加，见
            // `地基/过程记录/工程-比赛R2a-exec_sql.md`）：只读连接 + `PRAGMA query_only` +
            // 授权回调 + B164 起同一套沙箱，四层。
            HostAction {
                name: "exec_sql",
                usage: "exec_sql(db,sql)",
                reversible: executor_reversible(),
                undo: executor("exec_sql"),
                taint_out: TaintOut::Untrusted,
                cost: 0.0,
                run: exec::exec_sql,
                sandbox: Some(executor_sandbox_profile()),
            },
            // Z0885（B159 写法二，裁定六十九）：世界动作。协议无状态且子进程在沙箱内 → 可逆、成本 0；
            // 无沙箱时与执行器同规则登记为不可逆。契约与六项隐性知识见 `env.rs` 头注
            HostAction {
                name: "env:step",
                usage: "env:step({env,state,action,reset?})",
                reversible: env::reversible(),
                undo: env_undo(),
                taint_out: TaintOut::Untrusted,
                cost: 0.0,
                run: env::env_step,
                sandbox: Some(executor_sandbox_profile()),
            },
        ]
    })
}

/// `env:step` 的可撤回性事实（裁定六十九，`12` §2.7 B159 附注）
fn env_undo() -> UndoFact {
    if env::reversible() {
        yes(
            "世界状态整份进出、环境进程两次调用之间不留状态（协议无状态），子进程在操作系统沙箱里跑（写限定在调用专属临时目录、断网）；同一输入必得同一输出，回到任一步只需拿那一步的状态重来",
            &["环境遵守无状态协议（重放比对能报出同输入不同输出的环境）"],
        )
    } else {
        no(
            "本机探测不到操作系统沙箱，登记的环境命令以普通子进程运行，可写宿主文件、可联网，运行时无法撤回",
            &["探测不到 sandbox-exec（macOS）或 bwrap（Linux）"],
        )
    }
}

/// 把表里的动作全部注册进 `registry`。只凭账本重放（`replay_only`）时动作一律不执行：
/// 账本里有记录的由运行时照记录给出，走不到这里；走到这里就是缺记录。
pub fn register_all(registry: &mut ActionRegistry, ctx: &Ctx, replay_only: bool) {
    for a in builtin_actions() {
        let (name, run, ctx) = (a.name, a.run, ctx.clone());
        registry.register(a.name, a.cost, a.reversible, a.taint_out, move |args| {
            if replay_only {
                return Err(format!("replay has no completed record for {name}"));
            }
            run(&ctx, args)
        });
        // C-7：事实同时进注册表，`.jpp` 的 `action_fact(name)` 读它
        registry.describe_undo(
            a.name,
            crate::interp::ActionUndo {
                reversibility: a.undo.kind.as_str().to_string(),
                reason: a.undo.reason.clone(),
                conditions: a.undo.conditions.clone(),
            },
        );
    }
}

/// 一个动作的事实 JSON：`{reversibility, reason, conditions}`（与 `.jpp` 里 `action_fact(name)` 同形）。
pub fn action_fact_json(a: &HostAction) -> Json {
    serde_json::json!({
        "reversibility": a.undo.kind.as_str(),
        "reason": a.undo.reason,
        "conditions": a.undo.conditions,
    })
}

/// 整张事实表的 JSON（C-7）：动作名 → 事实。`names` 给出时只取这些名字（未登记的忽略）；`None` 取全部。
/// 报告的 `action_facts` 一节由它生成。
pub fn action_facts_json(names: Option<&[String]>) -> Json {
    let mut m = serde_json::Map::new();
    for a in builtin_actions() {
        if names.is_none_or(|ns| ns.iter().any(|n| n == a.name)) {
            m.insert(a.name.to_string(), action_fact_json(a));
        }
    }
    Json::Object(m)
}

/// 把整张事实表按「事实是否随宿主变」拆成两份（Z0901）：`(不随宿主变, 随宿主变)`，各是 动作名 → 事实。
/// 随宿主变的是表里带沙箱画像的动作（`exec_py`、`check_tests`、`exec_sql`：可逆与否、成立条件由本机的沙箱探测派生）；
/// 其余动作的事实只由动作本身决定。`names` 同 [`action_facts_json`]。报告把前一份放 `action_facts`、
/// 后一份放 `host.action_facts`，金样比较排除整个 `host`。
pub fn action_facts_split(names: Option<&[String]>) -> (Json, Json) {
    let (mut plain, mut host) = (serde_json::Map::new(), serde_json::Map::new());
    for a in builtin_actions() {
        if names.is_none_or(|ns| ns.iter().any(|n| n == a.name)) {
            let m = if a.sandbox.is_some() {
                &mut host
            } else {
                &mut plain
            };
            m.insert(a.name.to_string(), action_fact_json(a));
        }
    }
    (Json::Object(plain), Json::Object(host))
}

/// 宿主的沙箱种类（`"sandbox-exec"`、`"bwrap"`、`"none"`），报告 `host.sandbox` 用。
pub fn host_sandbox_kind() -> &'static str {
    sandbox::kind().as_str()
}

/// 已知动作的事实表（步 24c）：不依赖用户输入或实际 `ActionRegistry`（`check` 不构造它），
/// `check` 与 `run` 的预跑诊断都能随时拿到——J-08 静态子面据此对可逆动作不报、
/// 对不可逆动作报 error，而不是没有表时一律降成 `W-guard-untrusted`；`no_sandbox`
/// （B164）供 `W-action-no-sandbox` 用，与调用点有没有守卫无关（B187：没有沙箱只告警，执行器照跑）。
pub fn check_table() -> crate::check::ActionTable {
    let mut t = crate::check::ActionTable::default();
    for a in builtin_actions() {
        t.actions.insert(
            a.name.to_string(),
            crate::check::ActionFacts {
                reversible: a.reversible,
                output_untrusted: a.taint_out == TaintOut::Untrusted,
                no_sandbox: a
                    .sandbox
                    .as_ref()
                    .is_some_and(|s| s.kind == sandbox::SandboxKind::None.as_str()),
            },
        );
    }
    t
}

/// 本机能不能真的跑执行器动作（`exec_py`/`check_tests`/`exec_sql`）——供集成测试复用同一套
/// 探测逻辑（主会话复核追加：公开仓库 CI 上沙箱工具可能存在但跑不起来，`sandbox::probe()`
/// 已经把「跑一次冒烟测试」算进探测结果，这里只是把答案暴露成公开只读函数，不是另一套逻辑）。
/// 集成测试（`crates/jpp/tests/*.rs`）在需要真沙箱跑通的用例开头调用它，探测不到就跳过、
/// 打印原因，不判失败；测 `JPP_FORCE_NO_SANDBOX` 本身（无沙箱路径）的用例不需要它。
pub fn sandbox_available() -> bool {
    sandbox::kind() != sandbox::SandboxKind::None
}

/// 帮助文本里「Registered actions: …」的清单。
pub fn usage_list() -> String {
    builtin_actions()
        .iter()
        .map(|a| a.usage)
        .collect::<Vec<_>>()
        .join(", ")
}

/// 核心的 JSON 适配器把整数读成 Int（i64）；表示不了的无符号大整数在这里拒绝。
pub fn validate_numbers(value: &Json) -> Result<(), String> {
    match value {
        Json::Number(n) if n.is_u64() && n.as_i64().is_none() => {
            Err("JSON integer exceeds J++ Int range".into())
        }
        Json::Array(items) => items.iter().try_for_each(validate_numbers),
        Json::Object(items) => items.values().try_for_each(validate_numbers),
        _ => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 表内名字不重复且写法以名字开头() {
        let mut seen = std::collections::BTreeSet::new();
        for a in builtin_actions() {
            assert!(seen.insert(a.name), "动作 {} 重复登记", a.name);
            assert!(a.usage.starts_with(a.name), "{} 的写法 {}", a.name, a.usage);
        }
        assert_eq!(check_table().actions.len(), builtin_actions().len());
    }

    /// B164：执行器动作的 `reversible` 与 `sandbox.kind` 一致派生；本机（macOS）预期探测到
    /// `sandbox-exec`，三个执行器动作应该是可逆的、`sandbox` 字段齐全。
    #[test]
    fn 执行器动作的reversible由sandbox_kind派生() {
        for name in ["exec_py", "check_tests", "exec_sql"] {
            let a = builtin_actions()
                .iter()
                .find(|a| a.name == name)
                .unwrap_or_else(|| panic!("{name} 应在表里"));
            let sb = a
                .sandbox
                .as_ref()
                .unwrap_or_else(|| panic!("{name} 应有 sandbox 画像"));
            assert_eq!(
                a.reversible,
                sb.kind != sandbox::SandboxKind::None.as_str(),
                "{name}: reversible 应等于 kind != \"none\""
            );
            assert_eq!(sb.fs, "tmpdir");
            assert!(!sb.net);
            assert_eq!(sb.undo, "discard-tmpdir");
        }
    }

    /// C-7：三值与 J-08 布尔一致，逐项有理由。
    #[test]
    fn 三值与布尔一致且逐项有理由() {
        for a in builtin_actions() {
            assert_eq!(
                a.undo.kind.as_bool(),
                a.reversible,
                "{}: 三值与布尔不一致",
                a.name
            );
            assert!(!a.undo.reason.trim().is_empty(), "{}: 理由不能空", a.name);
        }
    }

    /// `no_sandbox` 与 `reversible` 的一致性：探测到沙箱时两者都该是「正常」（可逆、不报无沙箱）；
    /// 探测不到时两者都该翻转（不可逆、告警无沙箱）——不管本机实际探测结果是哪种，这条关系恒成立。
    /// B187：不可逆只在 `--guard` 下有后果（须守卫）；默认执行器照跑，只多写一份账本。
    #[test]
    fn no_sandbox与reversible互为反面() {
        let t = check_table();
        for name in ["exec_py", "check_tests", "exec_sql"] {
            let facts = t.actions[name];
            assert_eq!(
                facts.no_sandbox, !facts.reversible,
                "{name}: no_sandbox 应与 reversible 相反"
            );
        }
    }
}
