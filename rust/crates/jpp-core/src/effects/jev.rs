//! 真机 JEV 客户端（`live` 特性开 HTTP）。（步 4c 自 `effects.rs` 原样搬出）

use std::collections::HashMap;

use serde_json::{Value as Json, json};

use super::*;
use crate::value::{Answer, Op, Question, State};

/// 真机客户端：与 Python `foundation/clients/eye_client.py` 同协议（POST /v1/systemone）。
/// 密钥只从 `~/.typesafe-key` 读，不写进任何文件或日志。本包不用它发调用；`live` 特性开启 HTTP。
pub struct JevClient {
    /// 发不发置换（`12`:151 的 `Pick` 判据要它）。**默认 false**：它让 select 的调用数 ×2。
    /// `K=2` 且是**正序与逆序**——那是对首位偏置的**极大对抗对**，不是「图便宜的 2」：
    /// 若存在位置偏置，正逆两序最容易让它现形；两个随机置换反而可能都没碰到那个位置。
    pub permute: bool,
    pub model: String,
    /// 每个 input token 的价格，**只从画像读**（`cost.price_usd_per_input_token`；B73、`19` 冲突 #25）。
    /// `None` = 画像没给价格：实际费用未知，这里按 0 计入预算（B42 不拒），由宿主报 `W-cost-unknown`。
    pub usd_per_input_token: Option<f64>,
    pub n_calls: u64,
    pub transport: Box<dyn FnMut(&Json) -> Result<Json, EffectError>>,
}

impl JevClient {
    pub fn with_transport(
        model: &str,
        transport: Box<dyn FnMut(&Json) -> Result<Json, EffectError>>,
    ) -> JevClient {
        JevClient {
            permute: false,
            model: model.to_string(),
            usd_per_input_token: None,
            n_calls: 0,
            transport,
        }
    }
    /// 价格从画像来（B73）：宿主装载画像后把 `Profile::price_per_input_token` 传进来。
    pub fn with_price(mut self, usd_per_input_token: Option<f64>) -> JevClient {
        self.usd_per_input_token = usd_per_input_token;
        self
    }
    #[cfg(feature = "live")]
    pub fn live(model: &str, usd_per_input_token: Option<f64>) -> Result<JevClient, EffectError> {
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
        Ok(JevClient::with_transport(model, Box::new(t)).with_price(usd_per_input_token))
    }

    fn request_body_permuted(
        model: &str,
        state: &State,
        questions: &[&Question],
        perm: &[usize],
    ) -> Json {
        let mut body = JevClient::request_body(model, state, questions);
        // 按 perm 重排 select 的候选表：键仍是 c0..cK，值换成置换后的候选原文
        if let Some(qs) = body.get_mut("questions").and_then(|q| q.as_object_mut()) {
            for (i, q) in questions.iter().enumerate() {
                if q.op != Op::Select {
                    continue;
                }
                let crit: serde_json::Map<String, Json> = perm
                    .iter()
                    .enumerate()
                    .filter_map(|(发出位, 原下标)| {
                        state
                            .over
                            .get(*原下标)
                            .map(|m| (format!("c{发出位}"), m.content.clone()))
                    })
                    .collect();
                if let Some(o) = qs.get_mut(&format!("q{i}")).and_then(|x| x.as_object_mut()) {
                    o.insert("criteria".into(), Json::Object(crit));
                }
            }
        }
        body
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
        // `select` 要发**两个置换**（候选正序与逆序），按众数占比算 `mode_share`
        // ——`12`:151「`Pick` 要求置换众数一致」的**数据来源**。与 Python 同口径：
        // `runtime.py:949` 的 `perms = [正序, 逆序]`、`:1069` 的 `cnt / len(picks)`。
        //
        // 此前这里硬写 `mode_share: vec![]`，于是用真模型跑**每一道 select 都切成
        // `Unsure(tie)`**，而全绿的测试一盏灯都不亮——绿灯全是替身给的。
        // （那个 `tie` 现在已经改成 J-15 那一位；这一段留着记的是**怎么发现的**。）
        // **默认不开**（总控裁定三）：置换是两次不同的调用，不是同一状态多问一题——
        // 它实实在在让 select 的调用数 ×2。**这笔钱不能默认替作者花掉，更不能悄悄花。**
        //
        // 不开的后果是**有痕迹的失败关闭**：`mode_share == None` → J-15 那一位亮 →
        // 取保守项（不给 `Pick`）+ `W-untested`。作者看见告警，知道「这一格要更强的出口
        // 就得付这笔钱」，然后自己决定。反过来默认开的代价是：每一道 select 都悄悄花了双倍，
        // 而作者不知道自己买了什么。**兜底往拒绝那边倒**——「不给强出口」是拒绝。
        let 要置换 =
            self.permute && questions.iter().any(|q| q.op == Op::Select) && state.over.len() > 1;
        let perms: Vec<Vec<usize>> = if 要置换 {
            let 正 = (0..state.over.len()).collect::<Vec<_>>();
            let mut 逆 = 正.clone();
            逆.reverse();
            vec![正, 逆]
        } else {
            vec![(0..state.over.len()).collect()]
        };

        let mut 每次答案: Vec<Vec<Answer>> = vec![];
        let mut tokens = 0u64;
        for perm in &perms {
            let body = JevClient::request_body_permuted(&self.model, state, questions, perm);
            let resp = (self.transport)(&body)?;
            let mut answers = JevClient::parse_answers(&resp, state, questions)?;
            // 返回的概率按**这次发出去的顺序**索引，要还原回原始候选下标
            for a in &mut answers {
                if let Answer::Choice(v) = a {
                    let mut 还原 = vec![0.0; v.len()];
                    for (发出位, 原下标) in perm.iter().enumerate() {
                        if 发出位 < v.len() && *原下标 < 还原.len() {
                            还原[*原下标] = v[发出位];
                        }
                    }
                    *v = 还原;
                }
            }
            tokens += resp
                .get("usage")
                .and_then(|u| u.get("input_tokens"))
                .and_then(|t| t.as_u64())
                .unwrap_or(0);
            每次答案.push(answers);
            self.n_calls += 1;
        }

        // 逐题合并：select 取众数（并记占比），其余取第一次
        let mut answers = vec![];
        let mut mode_share = vec![];
        for (i, q) in questions.iter().enumerate() {
            if q.op == Op::Select && 每次答案.len() > 1 {
                let picks: Vec<usize> = 每次答案
                    .iter()
                    .filter_map(|一次| 一次.get(i))
                    .filter_map(|a| {
                        if let Answer::Choice(v) = a {
                            Some(argmax_index(v))
                        } else {
                            None
                        }
                    })
                    .collect();
                let mut 票 = HashMap::new();
                for k in &picks {
                    *票.entry(*k).or_insert(0usize) += 1;
                }
                let (众数, 次数) = 票.into_iter().max_by_key(|(_, n)| *n).unwrap_or((0, 0));
                // 概率取各次的均值（与 Python `p_ = sum(...) / len(probs_all)` 同）
                let mut 均值 = vec![0.0; state.over.len()];
                let mut n = 0.0;
                for 一次 in &每次答案 {
                    if let Some(Answer::Choice(v)) = 一次.get(i) {
                        for (k, x) in v.iter().enumerate() {
                            if k < 均值.len() {
                                均值[k] += x;
                            }
                        }
                        n += 1.0;
                    }
                }
                if n > 0.0 {
                    均值.iter_mut().for_each(|x| *x /= n);
                }
                let _ = 众数;
                answers.push(Answer::Choice(均值));
                mode_share.push(Some(次数 as f64 / picks.len().max(1) as f64));
            } else {
                answers.push(每次答案[0][i].clone());
                // 非 select：没有「置换」这回事，是 None 不是 0
                mode_share.push(None);
            }
        }
        let perms_used = perms.len();
        Ok(JudgeResult {
            answers,
            tokens,
            cost: self
                .usd_per_input_token
                .map(|p| tokens as f64 * p)
                .unwrap_or_default(),
            mode_share,
            perms: questions.iter().map(|_| perms_used).collect(),
        })
    }
    fn generate(
        &mut self,
        _p: &str,
        _c: &[Json],
        _n: usize,
        _r: u64,
    ) -> Result<GenResult, EffectError> {
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
