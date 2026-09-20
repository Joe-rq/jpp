//! 效应适配：`Client`（观察 = 模型调用或固定记录）、校准记录（线只从记录来）。

use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use serde_json::{Value as Json, json};

use crate::value::{Answer, Op, Question, State, canon, hash_of};

#[derive(Debug, Clone)]
pub struct EffectError(pub String);

/// 观察键 = 状态规范 JSON 哈希 + 题（固定观察表按它命中）。
pub fn obs_key(state: &State, q: &Question) -> String {
    hash_of(&[
        "obs",
        &canon(&state.to_json()),
        q.op.phys(),
        &q.text,
        &q.scale.join("\u{1e}"),
    ])
}

pub struct JudgeResult {
    pub answers: Vec<Answer>,
    pub tokens: u64,
    pub cost: f64,
}

pub trait Client {
    fn model_id(&self) -> String;
    /// 一状态多题一次问完（P5）。
    fn judge(&mut self, state: &State, questions: &[&Question])
    -> Result<JudgeResult, EffectError>;
    fn generate(
        &mut self,
        prompt: &str,
        ctx: &[Json],
        n: usize,
        retry_seq: u64,
    ) -> Result<Vec<Json>, EffectError>;
    /// 问人：None = 未答（Pending）。
    fn ask(&mut self, state: &State, q: &Question) -> Result<Option<Answer>, EffectError>;
    fn calls(&self) -> u64;
}

/// 固定观察：只按观察键命中，未命中即错（确定性对照，不是模型）。
#[derive(Default)]
pub struct FixedClient {
    pub table: HashMap<String, Answer>,
    pub gens: HashMap<String, Vec<Json>>,
    pub asks: HashMap<String, Option<Answer>>,
    pub n_calls: u64,
    pub n_questions: u64,
    pub log: Vec<(String, String)>,
}

impl FixedClient {
    pub fn new() -> FixedClient {
        FixedClient::default()
    }
    pub fn observe(&mut self, state: &State, q: &Question, a: Answer) -> String {
        let k = obs_key(state, q);
        self.table.insert(k.clone(), a);
        k
    }
    pub fn observe_key(&mut self, key: &str, a: Answer) {
        self.table.insert(key.to_string(), a);
    }
    pub fn fix_gen(&mut self, prompt: &str, retry_seq: u64, out: Vec<Json>) {
        self.gens.insert(format!("{prompt}\u{1f}{retry_seq}"), out);
    }
    pub fn fix_ask(&mut self, state: &State, q: &Question, a: Option<Answer>) {
        self.asks.insert(obs_key(state, q), a);
    }
}

impl Client for FixedClient {
    fn model_id(&self) -> String {
        "fixed-0".into()
    }
    fn judge(
        &mut self,
        state: &State,
        questions: &[&Question],
    ) -> Result<JudgeResult, EffectError> {
        let mut answers = vec![];
        for q in questions {
            let k = obs_key(state, q);
            let a = self.table.get(&k).cloned().ok_or_else(|| {
                EffectError(format!(
                    "固定观察未命中：题「{}」× 状态 {}（键 {k}）；固定观察不是模型，缺记录即错",
                    q.text,
                    &state.hash[..8]
                ))
            })?;
            self.log.push((k, q.text.clone()));
            answers.push(a);
        }
        self.n_calls += 1;
        self.n_questions += questions.len() as u64;
        Ok(JudgeResult {
            answers,
            tokens: 0,
            cost: 0.0,
        })
    }
    fn generate(
        &mut self,
        prompt: &str,
        _ctx: &[Json],
        _n: usize,
        retry_seq: u64,
    ) -> Result<Vec<Json>, EffectError> {
        self.n_calls += 1;
        self.gens
            .get(&format!("{prompt}\u{1f}{retry_seq}"))
            .cloned()
            .ok_or_else(|| EffectError(format!("固定生成未命中：{prompt} retry_seq={retry_seq}")))
    }
    fn ask(&mut self, state: &State, q: &Question) -> Result<Option<Answer>, EffectError> {
        Ok(self.asks.get(&obs_key(state, q)).cloned().unwrap_or(None))
    }
    fn calls(&self) -> u64 {
        self.n_calls
    }
}

/// 拒绝一切调用：重放验证用。
pub struct NoCallClient;
impl Client for NoCallClient {
    fn model_id(&self) -> String {
        "fixed-0".into()
    }
    fn judge(&mut self, _s: &State, q: &[&Question]) -> Result<JudgeResult, EffectError> {
        Err(EffectError(format!(
            "重放中不应发调用（题 {}）",
            q.iter()
                .map(|x| x.text.as_str())
                .collect::<Vec<_>>()
                .join("|")
        )))
    }
    fn generate(
        &mut self,
        p: &str,
        _c: &[Json],
        _n: usize,
        _r: u64,
    ) -> Result<Vec<Json>, EffectError> {
        Err(EffectError(format!("重放中不应发 gen：{p}")))
    }
    fn ask(&mut self, _s: &State, _q: &Question) -> Result<Option<Answer>, EffectError> {
        Err(EffectError("重放中不应发 ask".into()))
    }
    fn calls(&self) -> u64 {
        0
    }
}

/// 真机客户端：与 Python `foundation/clients/eye_client.py` 同协议（POST /v1/systemone）。
/// 密钥只从 `~/.typesafe-key` 读，不写进任何文件或日志。本包不用它发调用；`live` 特性开启 HTTP。
pub struct JevClient {
    pub model: String,
    pub usd_per_input_token: f64,
    pub n_calls: u64,
    pub transport: Box<dyn FnMut(&Json) -> Result<Json, EffectError>>,
}

impl JevClient {
    pub fn with_transport(
        model: &str,
        transport: Box<dyn FnMut(&Json) -> Result<Json, EffectError>>,
    ) -> JevClient {
        JevClient {
            model: model.to_string(),
            usd_per_input_token: 4.2e-8,
            n_calls: 0,
            transport,
        }
    }
    #[cfg(feature = "live")]
    pub fn live(model: &str) -> Result<JevClient, EffectError> {
        let key = std::fs::read_to_string(
            std::env::var("HOME")
                .map(|h| format!("{h}/.typesafe-key"))
                .unwrap_or_default(),
        )
        .map_err(|e| EffectError(format!("读不到 ~/.typesafe-key：{e}")))?;
        let key = key.trim().to_string();
        let t = move |body: &Json| -> Result<Json, EffectError> {
            let mut last = String::new();
            for attempt in 0..5u32 {
                let r = ureq::post("https://api.typesafe.ai/v1/systemone")
                    .set("Authorization", &format!("Bearer {key}"))
                    .send_json(body.clone());
                match r {
                    Ok(resp) => {
                        return resp
                            .into_json::<Json>()
                            .map_err(|e| EffectError(e.to_string()));
                    }
                    Err(ureq::Error::Status(code, _))
                        if matches!(code, 429 | 500 | 502 | 503 | 529) =>
                    {
                        last = format!("HTTP {code}");
                        std::thread::sleep(std::time::Duration::from_secs(1 << attempt));
                    }
                    Err(e) => return Err(EffectError(e.to_string())),
                }
            }
            Err(EffectError(format!("Jev 调用失败：{last}")))
        };
        Ok(JevClient::with_transport(model, Box::new(t)))
    }

    pub fn request_body(model: &str, state: &State, questions: &[&Question]) -> Json {
        let mut qs = serde_json::Map::new();
        for (i, q) in questions.iter().enumerate() {
            let qid = format!("q{i}");
            let mut o = serde_json::Map::new();
            o.insert("type".into(), json!(q.op.phys()));
            o.insert("instructions".into(), json!(q.text));
            match q.op {
                Op::Select => {
                    let crit: serde_json::Map<String, Json> = state
                        .over
                        .iter()
                        .enumerate()
                        .map(|(k, m)| (format!("c{k}"), m.content.clone()))
                        .collect();
                    o.insert("criteria".into(), Json::Object(crit));
                }
                Op::Measure => {
                    o.insert("criteria".into(), json!(q.scale));
                }
                Op::Test => {}
            }
            qs.insert(qid, Json::Object(o));
        }
        json!({"state": state.to_json(), "model": model, "questions": Json::Object(qs)})
    }

    /// 返回体校验：键不合即错，不静默变 Unsure（与 Python `validate_answers` 同纪律）。
    pub fn parse_answers(
        resp: &Json,
        state: &State,
        questions: &[&Question],
    ) -> Result<Vec<Answer>, EffectError> {
        let answers = resp
            .get("answers")
            .and_then(|a| a.as_object())
            .ok_or_else(|| EffectError("返回体缺 answers".into()))?;
        let mut out = vec![];
        for (i, q) in questions.iter().enumerate() {
            let a = answers
                .get(&format!("q{i}"))
                .ok_or_else(|| EffectError(format!("Jev 没有回答 q{i}")))?;
            match q.op {
                Op::Test => {
                    let p = a
                        .get("noul")
                        .and_then(|v| v.as_f64())
                        .filter(|p| (0.0..=1.0).contains(p))
                        .ok_or_else(|| {
                            EffectError(format!("noul 题 q{i} 返回体要 {{noul: p}}，收到 {a}"))
                        })?;
                    out.push(Answer::Noul(p));
                }
                Op::Select => {
                    let probs = a
                        .get("probabilities")
                        .and_then(|v| v.as_object())
                        .ok_or_else(|| EffectError(format!("choice 题 q{i} 缺 probabilities")))?;
                    let mut v = vec![0.0; state.over.len()];
                    for (k, p) in probs {
                        let idx: usize = k
                            .trim_start_matches('c')
                            .parse()
                            .map_err(|_| EffectError(format!("choice 键 {k} 不是候选键")))?;
                        if idx >= v.len() {
                            return Err(EffectError(format!("choice 键 {k} 越界")));
                        }
                        v[idx] = p.as_f64().unwrap_or(0.0);
                    }
                    out.push(Answer::Choice(v));
                }
                Op::Measure => {
                    let probs = a
                        .get("probabilities")
                        .and_then(|v| v.as_object())
                        .ok_or_else(|| EffectError(format!("score 题 q{i} 缺 probabilities")))?;
                    let mut v = vec![0.0; q.scale.len()];
                    for (k, p) in probs {
                        let idx: usize = k
                            .parse()
                            .map_err(|_| EffectError(format!("score 键 {k} 不是档位下标")))?;
                        if idx >= v.len() {
                            return Err(EffectError(format!("score 键 {k} 越界")));
                        }
                        v[idx] = p.as_f64().unwrap_or(0.0);
                    }
                    out.push(Answer::Score(v));
                }
            }
        }
        Ok(out)
    }
}

impl Client for JevClient {
    fn model_id(&self) -> String {
        self.model.clone()
    }
    fn judge(
        &mut self,
        state: &State,
        questions: &[&Question],
    ) -> Result<JudgeResult, EffectError> {
        let body = JevClient::request_body(&self.model, state, questions);
        let resp = (self.transport)(&body)?;
        let answers = JevClient::parse_answers(&resp, state, questions)?;
        let tokens = resp
            .get("usage")
            .and_then(|u| u.get("input_tokens"))
            .and_then(|t| t.as_u64())
            .unwrap_or(0);
        self.n_calls += 1;
        Ok(JudgeResult {
            answers,
            tokens,
            cost: tokens as f64 * self.usd_per_input_token,
        })
    }
    fn generate(
        &mut self,
        _p: &str,
        _c: &[Json],
        _n: usize,
        _r: u64,
    ) -> Result<Vec<Json>, EffectError> {
        Err(EffectError(
            "JevClient 不生成：gen 用生成器客户端或枚举器".into(),
        ))
    }
    fn ask(&mut self, _s: &State, _q: &Question) -> Result<Option<Answer>, EffectError> {
        Ok(None)
    }
    fn calls(&self) -> u64 {
        self.n_calls
    }
}

/// 校准记录（§2.3）：线只从这里来。状态「冷」→ cut 给 Unsure(cold)。
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CalibRecord {
    pub key: String,
    pub hi: f64,
    pub lo: f64,
    pub n: u64,
    pub status: String,
    pub delta: f64,
}

#[derive(Clone, Debug, Default)]
pub struct CalibStore {
    pub records: HashMap<String, CalibRecord>,
}

impl CalibStore {
    pub fn new() -> CalibStore {
        CalibStore::default()
    }
    /// 只由校准过程写；程序里不可调用（J-03 的运行期面）。
    pub fn put(&mut self, key: &str, hi: f64, lo: f64, n: u64, status: &str) -> Result<(), String> {
        if status == "上岗" && n == 0 {
            return Err("上岗记录必须带 n > 0（线只从标注记录来）".into());
        }
        if !(0.0..=1.0).contains(&hi) || !(0.0..=1.0).contains(&lo) || lo > hi {
            return Err("线必须满足 0 ≤ lo ≤ hi ≤ 1".into());
        }
        self.records.insert(
            key.to_string(),
            CalibRecord {
                key: key.into(),
                hi,
                lo,
                n,
                status: status.into(),
                delta: 0.05,
            },
        );
        Ok(())
    }
    pub fn get(&self, key: &str) -> CalibRecord {
        self.records.get(key).cloned().unwrap_or(CalibRecord {
            key: key.into(),
            hi: 0.65,
            lo: 0.35,
            n: 0,
            status: "冷".into(),
            delta: 0.05,
        })
    }
}
