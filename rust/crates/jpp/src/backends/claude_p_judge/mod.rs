//! 判断后端 `claude-p-judge`（V1-4 验证专用；预注册 `地基/规划/验证/预注册-V1-4.md`，提交 `d927ffe24`）。
//!
//! 第二个真实判断器：底层调 `claude -p`，把一次 `submit()` 收到的一个 `EffectCall`（运行时已经按同一刷新点
//! 合好的一批题，可能落在同一状态或按 B155 各带自己的状态）打成一次子进程调用，要求模型严格回一个 JSON
//! 数组，数组第 i 项是第 i 道题的答案，形状按题型分：`test` 回 `{"p": <0..1>}`，`select`/`measure` 回
//! `{"probs": [...]}`（长度分别等于候选数、档位数）。解析后按本模块自带的归一化（不复用 `stub::归一`，那是
//! 私有函数，复用会多碰 `stub/mod.rs`，见 V1-4 预注册「S1 一个目录加一行」的接入成本预测）重新归一。
//!
//! 非阻塞：沿用 `backends/claude_p`（生成器端口）同一套工作线程池设计——`submit` 派工、`poll` 查槽；
//! 起子进程需要串行（macOS 管道 `CLOEXEC` 竞态，同一原因见 `claude_p/mod.rs`），起进程本身很快，不影响并发
//! 执行。子进程参数逐字照抄 `claude_p/mod.rs`（`-p --model <model> --output-format json
//! --no-session-persistence --tools "" --setting-sources "" --strict-mcp-config --disable-slash-commands
//! --system-prompt <SYSTEM>`）——`--setting-sources ""` 尤其不能漏，否则每次判断都会带上本机全局
//! `CLAUDE.md`，把判断题问坏。
//!
//! 画像价格字段（`cost.price_usd_per_input_token`）主臂留未测（V1-4 预注册「预算记账与合批窗口」一节：
//! 如实填会让 `budget.cost = 0.01` 在第一次调用附近就 `Halt`，把其余机制测废），失败的四类
//! （超时/子进程失败/非 JSON/形状不对）统一标 `EffectError::network`，交给运行时按判断力缺席（默认重试、
//! 用尽落 `Unsure(absent)`）处理，不当运行期硬错误。

use crate::effects::Profile;
use jpp_effects::EffectInstance;
use jpp_effects::builtin_ports::{UnansweredPort, UnservedPort};
use jpp_effects::port::{
    EffectCall, EffectError, EffectOut, EffectPort, JudgeResult, Ports, Ticket,
};
use jpp_value::value::{Answer, Op, Question, State};
use serde_json::{Value as Json, json};
use std::io::{Read, Write};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, Mutex};
use std::task::Poll;
use std::time::{Duration, Instant};

/// 注册表条目（`backends::REGISTRY`，`20` v2 §五 S1）。
pub const SPEC: super::BackendSpec = super::BackendSpec {
    name: "claude-p-judge",
    default_model: "judge-claude-p",
    mode_label: "claude -p judge backend (subscription, second live judge for V1-4)",
    transport: true,
    // B127 过渡守卫：校准记录暂无模型分量，只有 jev 能带 --calib（本次验证不测这条）。
    calib: false,
    build,
};

fn build(model: &str, profile: &Profile) -> Result<Box<dyn super::BackendPorts>, String> {
    let cfg = ClaudeJudgeConfig::from_profile(model, profile);
    Ok(Box::new(ClaudeJudgePorts::new(cfg)))
}

/// 交给模型的系统说明：数组进、数组出，形状按题型分。
const SYSTEM: &str = "You are a calibrated judge called from inside a program. \
You receive a JSON array of judging tasks, each with an \"op\", a \"question\", and a \"material\" \
(and, for op \"choice\", a \"candidates\" array; for op \"score\", a \"scale\" array of level names). \
For each task, in the same order, output exactly one JSON object: \
op \"noul\" -> {\"p\": <number 0..1, your probability the answer to the question is yes>}; \
op \"choice\" -> {\"probs\": [<number>, ...]} with exactly one probability per item in \"candidates\", in order; \
op \"score\" -> {\"probs\": [<number>, ...]} with exactly one probability per item in \"scale\", in order. \
Reply with exactly one JSON array of these objects, same length and order as the input array, and nothing \
else: no prose, no explanation, no code fences.";

/// 实际调 `claude -p --model <…>` 的模型档，team-lead 指定用 Sonnet（2026-09-28 批复）。这与
/// `model`（J++ 侧的模型 id，等于画像文件名 `judge-claude-p`）是两回事——后者只用来找画像、报进
/// `EffectInstance`/账本，不能直接传给 `claude -p`（试过：`claude -p --model judge-claude-p`
/// 报 `unrecognized_model`）。
const CLAUDE_CLI_MODEL: &str = "sonnet";

/// 端口配置：可执行文件、模型、并发、超时、单价（主臂未测则 `None`）。
#[derive(Clone, Debug)]
pub struct ClaudeJudgeConfig {
    pub program: PathBuf,
    pub pre_args: Vec<String>,
    /// J++ 侧的模型 id（画像文件名、账本与报告里的 `model`），不是传给 `claude -p` 的值。
    pub model: String,
    pub concurrency: usize,
    /// 单次子进程超时；`None` = 画像没给（`transport.timeout_s` 未测），不设超时，与
    /// `jev.rs::transport_timeout` 同一约定（B73：不回退代码兜底值，未测就是未测，不是「当作
    /// 某个具体秒数」）。本条验证的画像会显式声明这个值，所以实际不落到这一支。
    pub timeout: Option<Duration>,
    /// 每个 input token 的价格；`None` = 未测，记账按 0（`B42`，与 `jev.rs` 同一约定）。
    pub price_usd_per_input_token: Option<f64>,
}

impl ClaudeJudgeConfig {
    /// 并发未测取 1（`20` §3.9 惯例），下面用处再按 4 兜底一次是 V1-4 验证本身的选择（沿用
    /// `gen-claude-p.json` 的宿主策略值，见 V1-4 预注册「画像字段怎么填」，不是代码默认）。
    /// 超时、价格只从画像来，不设代码兜底。
    pub fn from_profile(model: &str, p: &Profile) -> ClaudeJudgeConfig {
        ClaudeJudgeConfig {
            program: PathBuf::from("claude"),
            pre_args: vec![],
            model: model.to_string(),
            concurrency: p.concurrency().unwrap_or(4).max(1) as usize,
            timeout: p.transport_timeout_s().map(Duration::from_secs_f64),
            price_usd_per_input_token: p.price_per_input_token(),
        }
    }
}

type Slot = Arc<Mutex<Option<Result<JudgeResult, EffectError>>>>;

/// 一道题在响应数组里该长什么形状，用来解析与归一化；不含 `&State`/`&Question`（跨线程要 `Send`）。
#[derive(Clone, Copy, Debug)]
enum Shape {
    Test,
    Select(usize),
    Score(usize),
}

fn shape_of(state: &State, q: &Question) -> Shape {
    match q.op {
        Op::Test => Shape::Test,
        Op::Select => Shape::Select(state.over.len()),
        Op::Measure => Shape::Score(q.scale.len()),
    }
}

struct Job {
    prompt: String,
    shapes: Vec<Shape>,
    slot: Slot,
}

/// `judge@claude-p-judge/<model>` 的端口。
pub struct ClaudeJudgePort {
    cfg: Arc<ClaudeJudgeConfig>,
    tx: Option<Sender<Job>>,
    slots: std::collections::HashMap<u64, Slot>,
    next: u64,
}

impl ClaudeJudgePort {
    pub fn new(cfg: ClaudeJudgeConfig) -> ClaudeJudgePort {
        ClaudeJudgePort {
            cfg: Arc::new(cfg),
            tx: None,
            slots: std::collections::HashMap::new(),
            next: 0,
        }
    }

    fn sender(&mut self) -> Sender<Job> {
        if let Some(tx) = &self.tx {
            return tx.clone();
        }
        let (tx, rx) = channel::<Job>();
        let rx: Arc<Mutex<Receiver<Job>>> = Arc::new(Mutex::new(rx));
        for _ in 0..self.cfg.concurrency {
            let (rx, cfg) = (rx.clone(), self.cfg.clone());
            std::thread::spawn(move || {
                loop {
                    let job = match rx.lock().map(|r| r.recv()) {
                        Ok(Ok(job)) => job,
                        _ => break,
                    };
                    let out = run_one(&cfg, &job.prompt, &job.shapes);
                    if let Ok(mut s) = job.slot.lock() {
                        *s = Some(out);
                    }
                }
            });
        }
        self.tx = Some(tx.clone());
        tx
    }
}

impl EffectPort for ClaudeJudgePort {
    fn instance(&self) -> EffectInstance {
        EffectInstance {
            effect: judge_effect(),
            model: self.cfg.model.clone(),
        }
    }

    fn submit(&mut self, calls: Vec<EffectCall>) -> Result<Vec<Ticket>, EffectError> {
        let tx = self.sender();
        let mut tickets = Vec::with_capacity(calls.len());
        for call in calls {
            let Some(items) = call.input.judge_items() else {
                return Err(EffectError(
                    "claude-p-judge 端口只服务判断输入（状态加题）".into(),
                ));
            };
            let prompt = render_prompt(&items);
            let shapes = items.iter().map(|(s, q)| shape_of(s, q)).collect();
            let slot: Slot = Arc::new(Mutex::new(None));
            tx.send(Job {
                prompt,
                shapes,
                slot: slot.clone(),
            })
            .map_err(|_| EffectError("claude-p-judge 工作线程已退出".into()))?;
            let t = self.next;
            self.next += 1;
            self.slots.insert(t, slot);
            tickets.push(Ticket(t));
        }
        Ok(tickets)
    }

    fn poll(&mut self, t: &Ticket) -> Poll<Result<EffectOut, EffectError>> {
        let Some(slot) = self.slots.get(&t.0) else {
            return Poll::Ready(Err(EffectError(format!(
                "claude-p-judge：票据 {} 不存在",
                t.0
            ))));
        };
        let done = slot.lock().ok().and_then(|mut s| s.take());
        match done {
            Some(r) => {
                self.slots.remove(&t.0);
                Poll::Ready(r.map(EffectOut::Readings))
            }
            None => Poll::Pending,
        }
    }
}

/// 本端口服务的效应：注册表里产出读数的那一个（`judge`）。
fn judge_effect() -> jpp_ir::key::EffectId {
    jpp_effects::find(|s| s.produces_reading).expect("注册表里有判断")
}

/// 一道题在提示里的 JSON 描述：材料（`state.wire_json()`，与 jev 同一渲染，去掉 `over`）、
/// select 的候选内容、measure 的档位名。
fn render_item(idx: usize, state: &State, q: &Question) -> Json {
    let mut o = serde_json::Map::new();
    o.insert("index".into(), json!(idx));
    o.insert("op".into(), json!(q.op.phys()));
    o.insert("question".into(), json!(q.text));
    o.insert("material".into(), state.wire_json());
    match q.op {
        Op::Select => {
            let candidates: Vec<Json> = state.over.iter().map(|m| m.content.clone()).collect();
            o.insert("candidates".into(), Json::Array(candidates));
        }
        Op::Measure => {
            o.insert("scale".into(), json!(q.scale));
        }
        Op::Test => {}
    }
    Json::Object(o)
}

fn render_prompt(items: &[(&State, &Question)]) -> String {
    let tasks: Vec<Json> = items
        .iter()
        .enumerate()
        .map(|(i, (s, q))| render_item(i, s, q))
        .collect();
    json!(tasks).to_string()
}

/// 起子进程要串行（同 `claude_p/mod.rs`：macOS 没有 `pipe2`，管道先建后设 `CLOEXEC`，两条线程同时起
/// 子进程时会互相继承对方的管道端）。
static SPAWN: Mutex<()> = Mutex::new(());

fn run_one(
    cfg: &ClaudeJudgeConfig,
    prompt: &str,
    shapes: &[Shape],
) -> Result<JudgeResult, EffectError> {
    let guard = SPAWN.lock().unwrap_or_else(|e| e.into_inner());
    let spawned = Command::new(&cfg.program)
        .args(&cfg.pre_args)
        .args([
            "-p",
            "--model",
            CLAUDE_CLI_MODEL,
            "--output-format",
            "json",
            "--no-session-persistence",
            "--tools",
            "",
            "--setting-sources",
            "",
            "--strict-mcp-config",
            "--disable-slash-commands",
            "--system-prompt",
            SYSTEM,
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn();
    drop(guard);
    let mut child = match spawned {
        Ok(c) => c,
        Err(e) => {
            return Err(EffectError::network(format!(
                "failed: 起不了 {}：{e}",
                cfg.program.display()
            )));
        }
    };
    if let Some(mut stdin) = child.stdin.take() {
        let _ = stdin.write_all(prompt.as_bytes());
    }
    let reader = |p: Option<Box<dyn Read + Send>>| {
        std::thread::spawn(move || {
            let mut buf = String::new();
            if let Some(mut p) = p {
                let _ = p.read_to_string(&mut buf);
            }
            buf
        })
    };
    let out = reader(
        child
            .stdout
            .take()
            .map(|p| Box::new(p) as Box<dyn Read + Send>),
    );
    let err = reader(
        child
            .stderr
            .take()
            .map(|p| Box::new(p) as Box<dyn Read + Send>),
    );
    let start = Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(s)) => break s,
            Ok(None) if cfg.timeout.is_some_and(|t| start.elapsed() >= t) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(EffectError::network(format!(
                    "timeout: {:.0} 秒未返回",
                    cfg.timeout.unwrap().as_secs_f64()
                )));
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(20)),
            Err(e) => return Err(EffectError::network(format!("failed: {e}"))),
        }
    };
    let stdout = out.join().unwrap_or_default();
    let stderr = err.join().unwrap_or_default();
    if !status.success() {
        let tail: String = stderr
            .chars()
            .rev()
            .take(200)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect();
        return Err(EffectError::network(format!(
            "failed: 退出码 {status}：{tail}"
        )));
    }
    parse(&stdout, shapes, cfg)
}

/// 回复里第一个代码块的内容（同 `claude_p/mod.rs` 的宽容解析：前后可有说明文字）。
fn code_block(text: &str) -> Option<&str> {
    let rest = &text[text.find("```")? + 3..];
    let tag = rest
        .chars()
        .take_while(|c| c.is_ascii_alphanumeric())
        .count();
    let rest = if tag > 0 && rest[tag..].starts_with(char::is_whitespace) {
        &rest[tag..]
    } else {
        rest
    };
    let end = rest.find("```").unwrap_or(rest.len());
    Some(rest[..end].trim())
}

/// 一组权重归一成概率（全为 0 时均分）。不复用 `stub::归一`（私有，复用会多碰 `stub/mod.rs`）。
fn normalize(w: Vec<f64>) -> Vec<f64> {
    let s: f64 = w.iter().map(|x| x.max(0.0)).sum();
    if s > 0.0 {
        w.iter().map(|x| x.max(0.0) / s).collect()
    } else {
        let n = w.len().max(1) as f64;
        w.iter().map(|_| 1.0 / n).collect()
    }
}

fn parse(
    stdout: &str,
    shapes: &[Shape],
    cfg: &ClaudeJudgeConfig,
) -> Result<JudgeResult, EffectError> {
    let Ok(outer) = serde_json::from_str::<Json>(stdout.trim()) else {
        return Err(EffectError::network("malformed: 外层不是 JSON"));
    };
    // 输入 token 三类之和：服务端缓存系统提示词后，`input_tokens` 只剩未缓存的零头（V1-4 价格小臂
    // 实测 2 对 779），只读它会把价格记低两个数量级。三类都按同一个输入单价计，是费用上界。
    let usage = &outer["usage"];
    let input_tokens = usage["input_tokens"].as_u64().unwrap_or(0)
        + usage["cache_read_input_tokens"].as_u64().unwrap_or(0)
        + usage["cache_creation_input_tokens"].as_u64().unwrap_or(0);
    let output_tokens = outer["usage"]["output_tokens"].as_u64().unwrap_or(0);
    let tokens = input_tokens + output_tokens;
    if outer["is_error"].as_bool() == Some(true) {
        let msg = outer["result"].as_str().unwrap_or("is_error");
        return Err(EffectError::network(format!("failed: {msg}")));
    }
    let text = outer["result"].as_str().unwrap_or("").trim();
    let body = code_block(text).unwrap_or(text);
    let Ok(Json::Array(items)) = serde_json::from_str::<Json>(body) else {
        return Err(EffectError::network("malformed: result 不是 JSON 数组"));
    };
    if items.is_empty() || items.len() != shapes.len() {
        return Err(EffectError::network(format!(
            "malformed: 数组长度 {} 与题数 {} 不符",
            items.len(),
            shapes.len()
        )));
    }
    let mut answers = Vec::with_capacity(shapes.len());
    for (item, shape) in items.iter().zip(shapes.iter()) {
        let answer = match shape {
            Shape::Test => {
                let Some(p) = item["p"].as_f64() else {
                    return Err(EffectError::network("malformed: noul 缺 p"));
                };
                Answer::Noul(p.clamp(0.0, 1.0))
            }
            Shape::Select(n) => {
                let Some(probs) = item["probs"].as_array() else {
                    return Err(EffectError::network("malformed: choice 缺 probs"));
                };
                if probs.len() != *n {
                    return Err(EffectError::network(format!(
                        "malformed: choice probs 长度 {} 与候选数 {n} 不符",
                        probs.len()
                    )));
                }
                let v: Vec<f64> = probs.iter().map(|p| p.as_f64().unwrap_or(0.0)).collect();
                Answer::Choice(normalize(v))
            }
            Shape::Score(n) => {
                let Some(probs) = item["probs"].as_array() else {
                    return Err(EffectError::network("malformed: score 缺 probs"));
                };
                if probs.len() != *n {
                    return Err(EffectError::network(format!(
                        "malformed: score probs 长度 {} 与档位数 {n} 不符",
                        probs.len()
                    )));
                }
                let v: Vec<f64> = probs.iter().map(|p| p.as_f64().unwrap_or(0.0)).collect();
                Answer::Score(normalize(v))
            }
        };
        answers.push(answer);
    }
    let n = answers.len();
    // 每次判断的实付费用只按 input token 计（`cost.price_usd_per_input_token`，B73；输出 token
    // 未计价是画像 schema 本身的缺口，见 V1-4 预注册「候选画像缺口」）；价格未测（主臂）记 0。
    let cost = cfg
        .price_usd_per_input_token
        .map(|p| p * input_tokens as f64)
        .unwrap_or(0.0);
    Ok(JudgeResult {
        answers,
        tokens,
        cost,
        mode_share: vec![None; n],
        perms: vec![0; n],
        confidence: vec![None; n],
    })
}

/// 后端端口表：判断走并发子进程端口；`ask`/`gen` 用占位实现（照抄 `jev.rs`/`stub/mod.rs` 同一写法，
/// 见 V1-4 预注册「跑两臂的协议」：两臂的非判断端口要一致，差异只能来自判断端口本身）。
pub struct ClaudeJudgePorts {
    judge: ClaudeJudgePort,
    rest: Vec<Box<dyn EffectPort>>,
}

/// 本后端不生成：`gen` 实例的报错（与 `stub` 的 `STUB_NO_GEN` 同用途）。
const NO_GEN: &str = "claude-p-judge 判断后端不生成：gen 用生成器端口或夹具";

impl ClaudeJudgePorts {
    pub fn new(cfg: ClaudeJudgeConfig) -> ClaudeJudgePorts {
        let model = cfg.model.clone();
        let rest = jpp_effects::PORTED
            .iter()
            .map(|e| (*e, jpp_effects::spec(*e)))
            .filter(|(_, s)| !s.produces_reading)
            .map(|(effect, s)| {
                let instance = EffectInstance {
                    effect,
                    model: model.clone(),
                };
                if s.output_shape == jpp_effects::OutputShape::Answer {
                    Box::new(UnansweredPort::new(instance)) as Box<dyn EffectPort>
                } else {
                    Box::new(UnservedPort::new(instance, NO_GEN))
                }
            })
            .collect();
        ClaudeJudgePorts {
            judge: ClaudeJudgePort::new(cfg),
            rest,
        }
    }
    pub fn model_id(&self) -> String {
        self.judge.cfg.model.clone()
    }
    pub fn ports(&mut self) -> Ports<'_> {
        let mut p = Ports::new().with(&mut self.judge);
        for r in self.rest.iter_mut() {
            p = p.with(&mut **r);
        }
        p
    }
}

impl super::BackendPorts for ClaudeJudgePorts {
    fn ports(&mut self) -> Ports<'_> {
        ClaudeJudgePorts::ports(self)
    }
    fn model_id(&self) -> String {
        ClaudeJudgePorts::model_id(self)
    }
}
