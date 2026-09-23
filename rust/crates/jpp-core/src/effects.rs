//! 效应适配：`Client`（观察 = 模型调用或固定记录）、校准记录（线只从记录来）。

use std::collections::HashMap;
use std::rc::Rc;

use serde::{Deserialize, Serialize};
use serde_json::{Value as Json, json};

use crate::value::{Answer, Op, Question, State, canon, hash_of};

/// 概率向量里最大的那一档的下标
fn argmax_index(v: &[f64]) -> usize {
    v.iter().enumerate().fold((0usize, f64::MIN), |best, (i, p)| if *p > best.1 { (i, *p) } else { best }).0
}

#[derive(Debug, Clone)]
pub struct EffectError(pub String);

/// 观察键 = 状态规范 JSON 哈希 + 题（固定观察表按它命中）。
pub fn obs_key(state: &State, q: &Question) -> String {
    hash_of(&["obs", &canon(&state.to_json()), q.op.phys(), &q.text, &q.scale.join("\u{1e}")])
}

/// `gen` 的返回：**带实际费用与 token**。以前只返回 `Vec<Json>`，于是 `budget.cost`
/// 对 gen 整条路失效——一个只 gen 不 judge 的程序花多少钱都不会被拦住。
/// 形状与 `JudgeResult` 对齐，理由是同一条纪律（`13` §5：后端返回即记事实，再决定下一步）。
pub struct GenResult {
    pub outputs: Vec<Json>,
    pub tokens: u64,
    pub cost: f64,
}

pub struct JudgeResult {
    pub answers: Vec<Answer>,
    pub tokens: u64,
    pub cost: f64,
    /// 每条答案用了几个置换。**`mode_share` 不许裸记**——K 是这个测量**身份的一部分**：
    /// K=2 的 1.0 与 K=15 的 1.0 是两个不同的测量。裸记的话改 K 就会把不同 K 下的值
    /// 合进同一格、第二个覆盖第一个，**而它长得像一次观察**（与 `literal_mode` 缺维、
    /// `judge_key` 缺 `site` 同族）。
    ///
    /// 这条的证据就是那个被写错三次的数：最终能定下来**靠的正是原始数据里的 `perms: 2`**
    /// ——没有它，「测出来的 1.0」与「写死的 1.0」到今天还是不可判的。
    pub perms: Vec<usize>,
    /// 每条答案的**置换众数占比**（`12`:151：`select` 的 `Pick` 要求置换众数一致）。
    ///
    /// 缺省 `None` = **这条路上没测过置换**，`cut` 因此不给 `Pick`。**出口不是 `Unsure(tie)`**
    /// ——`tie` 只留给「测了，不一致」；没测过走 J-15 那一位（`Unsure("untested")` +
    /// `Exit.untested = Some("permutation")`），见 `value.rs` 的 `Exit::untested`。
    /// K-noul 路径就是这个情形：它把一道 select 拆成 K 道独立的 noul 再取 argmax，
    /// **那条路上根本没有「置换」这回事**，候选顺序不参与。
    ///
    /// 放在 `JudgeResult` 而不是给 `Answer` 加变体：后者会逼每一处 `match Answer` 都改，
    /// 而那些地方（渲染、验证形状、合并）跟置换无关——**改动面比它该有的大**。
    #[allow(clippy::type_complexity)]
    pub mode_share: Vec<Option<f64>>,
}

pub trait Client {
    fn model_id(&self) -> String;
    /// 一状态多题一次问完（P5）。
    fn judge(&mut self, state: &State, questions: &[&Question]) -> Result<JudgeResult, EffectError>;
    fn generate(&mut self, prompt: &str, ctx: &[Json], n: usize, retry_seq: u64) -> Result<GenResult, EffectError>;
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
    fn judge(&mut self, state: &State, questions: &[&Question]) -> Result<JudgeResult, EffectError> {
        let mut answers = vec![];
        for q in questions {
            let k = obs_key(state, q);
            let a = self.table.get(&k).cloned().ok_or_else(|| {
                // **把自己算出来的那份状态与题打出来。**
                //
                // 以前只给哈希。于是作者猜 `measure` 的档位字段名猜了四次
                // （`bands`/`levels`/`grades`/`options` 全不中，最后是 `scale`），
                // **每次拿到同一句话、同一个哈希，没有任何梯度**。
                // **那份 JSON 就在手边，只是没打出来**——这是整个任务里浪费时间最多的单点，
                // 而且是免费可修的：「说不准」与「说不出」是两回事，这里属于后者。
                EffectError(format!(
                    "固定观察未命中：题「{}」× 状态 {}（键 {k}）；固定观察不是模型，缺记录即错。内核这次算出来的状态 = {}；题 = {}。夹具里要有一条 observation，其 on/ctx/ref/over 与上面的状态逐字段相同，op/text/calib/scale/evidence 与上面的题相同",
                    q.text,
                    state.hash.chars().take(8).collect::<String>(),
                    canon(&state.as_fixture_json()),
                    canon(&json!({"op": q.op.fixture_name(), "text": q.text, "calib": q.calib, "scale": q.scale})),
                ))
            })?;
            self.log.push((k, q.text.clone()));
            answers.push(a);
        }
        self.n_calls += 1;
        self.n_questions += questions.len() as u64;
        Ok(JudgeResult { answers, tokens: 0, cost: 0.0, mode_share: vec![], perms: vec![] })
    }
    fn generate(&mut self, prompt: &str, _ctx: &[Json], _n: usize, retry_seq: u64) -> Result<GenResult, EffectError> {
        self.n_calls += 1;
        let outputs = self.gens.get(&format!("{prompt}\u{1f}{retry_seq}")).cloned().ok_or_else(|| EffectError(format!("固定生成未命中：{prompt} retry_seq={retry_seq}")))?;
        Ok(GenResult { outputs, tokens: 0, cost: 0.0 })
    }
    fn ask(&mut self, state: &State, q: &Question) -> Result<Option<Answer>, EffectError> {
        let k = obs_key(state, q);
        match self.asks.get(&k) {
            // 登记过：有答案就给，明说「还没答」就挂起——**那是真的在等人**
            Some(a) => Ok(a.clone()),
            // **一条 responses 都没给 = 这是第一趟，本来就没人答**，静默挂起
            None if self.asks.is_empty() => Ok(None),
            // **给了 responses 却没命中**——以前这里是 `unwrap_or(None)`，
            // 于是它与「人还没答」在唯一的输出上**逐字段一模一样**。
            // 「人还没答」与「你的 responses 没命中」是两件事。
            None => Err(EffectError(format!(
                "responses 未命中：题「{}」× 状态 {}（键 {k}）；夹具给了 {} 条 responses，没有一条对得上。\
                 内核这次算出来的状态 = {}；题 = {}。responses 里要有一条，其 on/ctx/ref/over 与上面的状态逐字段相同，op/text/calib/scale 与上面的题相同",
                q.text,
                state.hash.chars().take(8).collect::<String>(),
                self.asks.len(),
                canon(&state.as_fixture_json()),
                canon(&json!({"op": q.op.fixture_name(), "text": q.text, "calib": q.calib, "scale": q.scale})),
            ))),
        }
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
        Err(EffectError(format!("重放中不应发调用（题 {}）", q.iter().map(|x| x.text.as_str()).collect::<Vec<_>>().join("|"))))
    }
    fn generate(&mut self, p: &str, _c: &[Json], _n: usize, _r: u64) -> Result<GenResult, EffectError> {
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
    /// 发不发置换（`12`:151 的 `Pick` 判据要它）。**默认 false**：它让 select 的调用数 ×2。
    /// `K=2` 且是**正序与逆序**——那是对首位偏置的**极大对抗对**，不是「图便宜的 2」：
    /// 若存在位置偏置，正逆两序最容易让它现形；两个随机置换反而可能都没碰到那个位置。
    pub permute: bool,
    pub model: String,
    pub usd_per_input_token: f64,
    pub n_calls: u64,
    pub transport: Box<dyn FnMut(&Json) -> Result<Json, EffectError>>,
}

impl JevClient {
    pub fn with_transport(model: &str, transport: Box<dyn FnMut(&Json) -> Result<Json, EffectError>>) -> JevClient {
        JevClient { permute: false, model: model.to_string(), usd_per_input_token: 4.2e-8, n_calls: 0, transport }
    }
    #[cfg(feature = "live")]
    pub fn live(model: &str) -> Result<JevClient, EffectError> {
        let key = std::fs::read_to_string(std::env::var("HOME").map(|h| format!("{h}/.typesafe-key")).unwrap_or_default())
            .map_err(|e| EffectError(format!("读不到 ~/.typesafe-key：{e}")))?;
        let key = key.trim().to_string();
        let t = move |body: &Json| -> Result<Json, EffectError> {
            let mut last = String::new();
            for attempt in 0..5u32 {
                let r = ureq::post("https://api.typesafe.ai/v1/systemone").set("Authorization", &format!("Bearer {key}")).send_json(body.clone());
                match r {
                    Ok(resp) => return resp.into_json::<Json>().map_err(|e| EffectError(e.to_string())),
                    Err(ureq::Error::Status(code, _)) if matches!(code, 429 | 500 | 502 | 503 | 529) => {
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

    fn request_body_permuted(model: &str, state: &State, questions: &[&Question], perm: &[usize]) -> Json {
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
                    .filter_map(|(发出位, 原下标)| state.over.get(*原下标).map(|m| (format!("c{发出位}"), m.content.clone())))
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
                    let crit: serde_json::Map<String, Json> = state.over.iter().enumerate().map(|(k, m)| (format!("c{k}"), m.content.clone())).collect();
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
    pub fn parse_answers(resp: &Json, state: &State, questions: &[&Question]) -> Result<Vec<Answer>, EffectError> {
        let answers = resp.get("answers").and_then(|a| a.as_object()).ok_or_else(|| EffectError("返回体缺 answers".into()))?;
        let mut out = vec![];
        for (i, q) in questions.iter().enumerate() {
            let a = answers.get(&format!("q{i}")).ok_or_else(|| EffectError(format!("Jev 没有回答 q{i}")))?;
            match q.op {
                Op::Test => {
                    let p = a.get("noul").and_then(|v| v.as_f64()).filter(|p| (0.0..=1.0).contains(p)).ok_or_else(|| EffectError(format!("noul 题 q{i} 返回体要 {{noul: p}}，收到 {a}")))?;
                    out.push(Answer::Noul(p));
                }
                Op::Select => {
                    let probs = a.get("probabilities").and_then(|v| v.as_object()).ok_or_else(|| EffectError(format!("choice 题 q{i} 缺 probabilities")))?;
                    let mut v = vec![0.0; state.over.len()];
                    for (k, p) in probs {
                        let idx: usize = k.trim_start_matches('c').parse().map_err(|_| EffectError(format!("choice 键 {k} 不是候选键")))?;
                        if idx >= v.len() {
                            return Err(EffectError(format!("choice 键 {k} 越界")));
                        }
                        v[idx] = p.as_f64().unwrap_or(0.0);
                    }
                    out.push(Answer::Choice(v));
                }
                Op::Measure => {
                    let probs = a.get("probabilities").and_then(|v| v.as_object()).ok_or_else(|| EffectError(format!("score 题 q{i} 缺 probabilities")))?;
                    let mut v = vec![0.0; q.scale.len()];
                    for (k, p) in probs {
                        let idx: usize = k.parse().map_err(|_| EffectError(format!("score 键 {k} 不是档位下标")))?;
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
    fn judge(&mut self, state: &State, questions: &[&Question]) -> Result<JudgeResult, EffectError> {
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
        let 要置换 = self.permute && questions.iter().any(|q| q.op == Op::Select) && state.over.len() > 1;
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
            tokens += resp.get("usage").and_then(|u| u.get("input_tokens")).and_then(|t| t.as_u64()).unwrap_or(0);
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
                    .filter_map(|a| if let Answer::Choice(v) = a { Some(argmax_index(v)) } else { None })
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
        Ok(JudgeResult { answers, tokens, cost: tokens as f64 * self.usd_per_input_token, mode_share, perms: questions.iter().map(|_| perms_used).collect() })
    }
    fn generate(&mut self, _p: &str, _c: &[Json], _n: usize, _r: u64) -> Result<GenResult, EffectError> {
        Err(EffectError("JevClient 不生成：gen 用生成器客户端或枚举器".into()))
    }
    fn ask(&mut self, _s: &State, _q: &Question) -> Result<Option<Answer>, EffectError> {
        Ok(None)
    }
    fn calls(&self) -> u64 {
        self.n_calls
    }
}

/// `fit` 注册表（`12`:319「每个 `FitRef` 的训练集 id、特征键、指纹种类、错误率、版本；
/// 注册约束见 §2.9」，:314「**只能训练产生**」）。
///
/// `fit` 是第七种形式之外的**桥**：它把跨题的多个读数合成**一个仍然是读数的东西**，
/// 因而仍要过线。作者不用它也能合并两道题（`cut` 出两个出口再写 `if`），
/// 但那样一来**合并这一步的不确定性就消失了**——两个 `act` 合出来的结论看着和一个 `act`
/// 一样确定，而它其实经过了一个没有校准过的函数。
pub struct FitRecord {
    /// 特征：`(校准键, 指纹种类)`，**逐项**要与输入读数相同（J-04）
    pub features: Vec<(String, String)>,
    /// 训练样本数（J-16：`n ≥ max(50, 20×特征数)`）
    pub n: u64,
    /// 训练集 id（J-16：训练集 ≠ 保形集）
    pub trained_from: String,
    #[allow(clippy::type_complexity)]
    pub f: Rc<dyn Fn(&[f64]) -> f64>,
}

#[derive(Default)]
pub struct FitRegistry {
    pub fits: HashMap<String, Rc<FitRecord>>,
}

impl FitRegistry {
    pub fn new() -> FitRegistry {
        FitRegistry::default()
    }
    /// 不核约束的登记（测试与内部用）
    pub fn register(&mut self, name: &str, features: &[(&str, &str)], n: u64, trained_from: &str, f: impl Fn(&[f64]) -> f64 + 'static) {
        let features = features.iter().map(|(a, b)| (a.to_string(), b.to_string())).collect();
        self.fits.insert(name.to_string(), Rc::new(FitRecord { features, n, trained_from: trained_from.into(), f: Rc::new(f) }));
    }
    /// 核 J-16 的样本数约束再登记：`n ≥ max(50, 20×特征数)`。
    /// **样本不够就不该用它下结论**——这条在登记时拦，比在调用时拦早。
    pub fn register_checked(
        &mut self,
        name: &str,
        features: &[(&str, &str)],
        n: u64,
        trained_from: &str,
        f: impl Fn(&[f64]) -> f64 + 'static,
    ) -> Result<(), String> {
        let need = std::cmp::max(50, 20 * features.len() as u64);
        if n < need {
            return Err(format!("J-16: fit {name} 的训练样本 n={n} 不足 max(50, 20×{}) = {need}", features.len()));
        }
        self.register(name, features, n, trained_from, f);
        Ok(())
    }
}

/// 字面模式（`12`:136 的 `calib_key` 第五维）：`literal_mode ∈ {判执行输出, 判代码字面,
/// 判文档段落, …}`。
///
/// **它是校准键的一维，不是标签。** 同一道题问「这段代码字面上写了什么」和
/// 「这份文档这一段说了什么」，模型的可靠性完全不同——线自然也不同。
/// 缺这一维的后果与 `judge_key` 缺 `site` **同族**：不同模式下的值合进同一格、
/// 第二个覆盖第一个，**而它长得像一次观察**。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum LiteralMode {
    /// 未分档：老接口写进来的就是这一档，键就是裸键名（老账本读得回来）
    #[default]
    Unspecified,
    /// 判执行输出
    ExecOutput,
    /// 判代码字面
    CodeLiteral,
    /// 判文档段落
    DocSection,
}

impl LiteralMode {
    /// 键里的后缀。默认档**不加后缀**——这是老账本还读得回来的原因。
    pub fn suffix(&self) -> &'static str {
        match self {
            LiteralMode::Unspecified => "",
            LiteralMode::ExecOutput => "\u{1f}exec",
            LiteralMode::CodeLiteral => "\u{1f}code",
            LiteralMode::DocSection => "\u{1f}doc",
        }
    }
}

/// 一条运行期观察（`12`:347 标为「未定」的**运行期写入口**的载荷）。
///
/// **与 Python `CalibRecord.samples` 的 `[[p, label]]` 同族，但带上了记账要的几样。**
/// 总控立的规矩在这里落地：**`mode_share` 不许裸记——必须和 `perms` 一起**进账本和
/// 校准记录。一个没有 `perms` 的一致率不是测量结果，是一个孤零零的小数。
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Sample {
    /// 标量概率。**`None` = 这个物理形式没有定义好的标量 `p`**（select / measure）：
    /// 强给它一个标量就是造一个不同尺的数，而「不同尺不可比」是本项目自己的判据。
    pub p: Option<f64>,
    /// 真值。**`None` = 还没有**——`cut` 切出来的读数本身从不携带真值。
    /// 这一位是 `n` 与 `observations` 分家的全部理由。
    pub label: Option<u8>,
    /// 用了几个置换（0 = 没测）
    pub perms: usize,
    /// 置换众数占比；与 `perms` 成对
    pub mode_share: Option<f64>,
    /// 字面模式（`12`:136 第五维）
    pub mode: LiteralMode,
    /// 物理形式：noul / choice / score
    pub phys: String,
    /// 这条观察属于哪个**簇**（通常是对象段）。**可交换性在我们这里不是被时间打破的，
    /// 是被材料复用打破的**——同一段落的多条读数不是多次独立观察。
    /// `None` = 这条没有簇 id，**于是它只能参与「按条」的认证**；
    /// 声明按对象段却没有簇 id 是**错，不是降级**。
    #[serde(default)]
    pub cluster: Option<String>,
}

/// 记录里的证据是哪来的。**它是算出来的，不是填出来的。**
///
/// **为什么不做成一个可写字段**：Python 的 `CalibRecord.source: str = ""` 全仓
/// **零引用**——没人写、也没人读。一个自由字符串正是「给没有类型的东西补来源」那个
/// 形状：替身会把它填成让检查恰好通过的值，而纪律就在替身上成立、在真机上失效。
/// 算出来的答案填不错。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Provenance {
    /// 既没有线也没有证据
    空,
    /// 宣称了 `n` 条，却一条证据也没有——**以前分不出来的正是这一种**
    宿主手填,
    /// `n` 与实际标注条数一致
    程序积累,
    /// **只判过、没标注过**：有观察，一条标注也没有。
    ///
    /// 以前它掉进「程序积累」——因为 `n == labeled()` 在两边都是 0 时**偶然为真**，
    /// **而门正按这个判**。「算出来的答案填不错」的前提是**那个算法对**。
    /// 它也不是「混合」：混合读起来像「两种都有一些」，而这里一种都没有。
    只有观察,
    /// 两者都有且对不上
    混合,
}

/// **标签是怎么选出来的。**
///
/// 判据与 `cluster_unit` 同源：**不记「簇是怎么分的」，`n` 这个数就没有意义；
/// 不记「标签是怎么选的」，那张证书也没有意义。**
/// **一张在「两模型都同意」的子集上认证出来的证书，和一张在全体上认证出来的，
/// 不是同一个测量**——它们以前占同一个格子，只不过第二个还没被算出来。
///
/// **为什么是带载荷的枚举而不是自由字符串。** `provenance()` 那次的理由是
/// 「算出来的答案填不错」，**但这里算不出来**——标签怎么选的是系统之外的事实。
/// 枚举给的是自由字符串给不了的那一样：**`全体` 是一个要有人明确声明的断言，
/// 不是一个谁都会漂进去的默认。** 代价落在**有一种结构上全新的标签来源的人**身上，
/// 他必须来改 core——**而那正是该改的地方，因为一种新的结构改变了证书的含义**。
/// 具体判据写在 `选择子集` 的载荷里，不用改 core。
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub enum LabelSource {
    /// 标注覆盖了总体，没有选择。**这是一个断言，要有人明确说出口。**
    全体,
    /// 标注只覆盖了一个**被选出来**的子集。
    选择子集 {
        /// 按什么选的（如「两个模型都同意」）
        判据: String,
        /// 这个选择判据与「对不对」的相关性。
        /// **`None` = 没测，不是「不相关」**——与 `Tri::未测` 同一条：
        /// **一个没测过相关性的选择子集，比一个声明了 0.0 的更可疑，不是更不可疑。**
        与对错相关: Option<f64>,
    },
    /// **没人说过。** 缺失值**不许默认成「全体」**——那是替不确定说了确定，
    /// 而且方向是**乐观的那一侧**。
    #[default]
    未声明,
}

impl LabelSource {
    /// 这个来源声明本身可不可疑。**`未声明` 与「选了子集却没测相关性」都算**。
    pub fn 可疑(&self) -> bool {
        matches!(self, LabelSource::未声明 | LabelSource::选择子集 { 与对错相关: None, .. })
    }
    /// 进证书地址用的短名
    pub fn addr(&self) -> String {
        match self {
            LabelSource::全体 => "全体".into(),
            LabelSource::选择子集 { 判据, 与对错相关 } => match 与对错相关 {
                Some(r) => format!("子集({判据},r={r:.4})"),
                None => format!("子集({判据},r=未测)"),
            },
            LabelSource::未声明 => "未声明".into(),
        }
    }
}

/// 存进记录的证书。
///
/// **`conf_delta` 不是 `CalibRecord.delta`。** 前者是二项上界的置信水平，
/// 后者是带宽 δ（喂 `delta_for`）。**两个不同的保证不共用一个名字**——
/// 「共用机制的前提是要保证的东西相同，不是听起来像同一类」。
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Cert {
    /// 风险目标：放行区里的假放行率上界
    pub alpha: f64,
    /// 二项上界的置信水平（**不是带宽 δ**）
    pub conf_delta: f64,
    /// 认证住的线
    pub hi: f64,
    pub n_accepted: usize,
    pub n_errors: usize,
    /// 实测上界，必须 ≤ `alpha`
    pub ucb: f64,
    /// **按什么分的簇**。不记它，`n` 这个数本身就没有意义——
    /// 它会让「73 条」读起来像 73 次独立观察，而实际独立单位可能是 19。
    pub cluster_unit: String,
    /// 簇级认证用了几次重采样、判定规则是什么。**`None` = 按条认证，没有重采样。**
    ///
    /// **这条规则是未标定的**：原型只报了「200 次里有解几次」，没有定「几次算过」。
    /// 我取「全过才算过」（说不准往拒绝那边倒），**记下来是为了它可审**，
    /// 不是因为它被标定过。
    #[serde(default)]
    pub resample: Option<(usize, String)>,
    /// **这条线是哪个代价矩阵定的**。`None` = 线由证书自己找（无代价矩阵）。
    /// 它进地址：**换代价矩阵 = 换一个测量，不是覆盖同一个**。
    #[serde(default)]
    pub cost: Option<(f64, f64)>,
    /// **这张证书界定的是哪一侧**。今天只有一个取值，而它必须是**数据不是注释**。
    ///
    /// 保形只界定**放行那一侧**的风险。另一侧既没有界、也没有出口
    /// （`commission` 置 `lo = 0.0`，于是 `p <= lo → Ignore` 实际不可达），
    /// **而 J-05 仍然强制作者为那条永不执行的分支写代码**。
    ///
    /// 置 `lo = 0.0` 的原注释说「说不准往拒绝那边倒」，**而那句话把「拒绝」默认
    /// 等同于「不给 `Act`」**。`Act` 与 `Ignore` 在出口代数里是**对称的两个判定**，
    /// 哪一个安全取决于程序怎么用——写 `if 不安全(x) { 拦下 }` 时，
    /// **`Ignore` 才是放行的那个答案**。
    ///
    /// **它今天不进地址**：只有一个取值，进去也分不出任何东西，只会把现有地址全改一遍。
    /// **哪天有双侧证书，它必须进地址**——那时两侧界不同就是两个测量。
    #[serde(default = "单侧声明")]
    pub bounded_side: String,
    /// **这张证书是在什么样的标签上算出来的**（冻结在认证那一刻）。
    ///
    /// 它**进地址**：换标签来源 = 换一个测量。不进就是后一张盖掉前一张，
    /// **那正是刚修好的覆盖那个坑换了一根轴重演**。
    ///
    /// 与 `bounded_side` **不进地址**的分界：那个今天只有一个取值，
    /// 进去分不出任何东西；这个有好几个，**不同规则同一条理由**。
    #[serde(default)]
    pub label_source: LabelSource,
    /// **这张证书是在哪一份标注集上算出来的**——`(p, label)` 对的规范形指纹。
    ///
    /// **它是算出来的，不是填的。** 与 `label_source` **必须声明**恰好相反，
    /// 而两者是同一条理由的两侧：**算得出来的不许填**（填得错），
    /// **算不出来的必须有人明确说**（标签怎么选的是系统之外的事实）。
    ///
    /// **它进地址**：两份不同的标注集是两个测量，哪怕 α、簇单位、`label_source` 全同。
    /// 与 `label_source` 的分工——**那个说「怎么选的」，这个说「是哪一份」，
    /// 互相替代不了。**
    #[serde(default)]
    pub label_fp: String,
}

/// 标注集指纹：`(p, label)` 对的**规范形**，排序后取 `canon` 再 sha256 前 16 位。
/// **对写入顺序不敏感**——同一批对、不同顺序，指纹相同。
fn 标注集指纹(样本: &[&Sample]) -> String {
    let mut pairs: Vec<(String, u8)> = 样本
        .iter()
        .filter_map(|s| Some((format!("{:.10}", s.p?), s.label?)))
        .collect();
    pairs.sort();
    hash16(&json!(pairs))
}

/// `null` → 空表。见 `CalibRecord::samples` 的注释。
fn null当空表<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Vec<Sample>, D::Error> {
    Ok(Option::<Vec<Sample>>::deserialize(d)?.unwrap_or_default())
}

fn 单侧声明() -> String {
    "证书只界定 p ≥ hi 一侧的假放行；p ≤ lo 一侧没有界，且 lo = 0 使该出口实际不可达".to_string()
}

/// 校准记录（§2.3）：线只从这里来。状态「冷」→ cut 给 Unsure(cold)。
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CalibRecord {
    pub key: String,
    pub hi: f64,
    pub lo: f64,
    pub n: u64,
    pub status: String,
    /// 覆盖档案 δ；None = 用档案里这种题式的 δ
    #[serde(default)]
    pub delta: Option<f64>,
    /// 标注集上实测的 unsure 率（J-10 的联合上界用）；None = 未知，按 1 计最保守
    #[serde(default)]
    pub unsure_rate: Option<f64>,
    /// 保形集 id（J-16：fit 的训练集 ≠ 保形集）
    #[serde(default)]
    pub set_id: String,
    /// **标注集 id**。必须给出且 ≠ `set_id`——否则**代价线与保形线同源**，
    /// 那是 J-16 同一条纪律：拿训练数据给自己打分，过线的那条线不再是独立证据。
    #[serde(default)]
    pub label_set_id: String,
    /// 标注集的**去处**（给人看的定位，不是检查依据）。
    /// **检查只信指纹**——去处解析不了是「这台机器上没有那份数据」，不是「检查失败」。
    #[serde(default)]
    pub label_locator: String,
    /// **声明的**标注集指纹。非空时 `commission` 会核它与实际用到的那批对不对得上。
    #[serde(default)]
    pub label_fp: String,
    /// **这条记录的标签是怎么选出来的**（见 [`LabelSource`]）。
    /// 证书在认证那一刻把它冻进 `Cert.label_source`——**后来改声明不追改已发的证书**，
    /// 与「线重算之后已经发出的出口不改」同一条。
    #[serde(default)]
    pub label_source: LabelSource,
    /// **这条线凭什么上岗**（`12` §2.3「保形分位数（含代价矩阵的风险控制）」）。
    ///
    /// `None` 有两种读法，靠 [`CalibRecord::provenance`] 区分：宿主手填的线**本来就没有证书**
    /// （I4：线来自程序之外，宿主为它负责），而程序积累的证据**没证书就上不了岗**。
    ///
    /// **两条记录可以有一模一样的 `hi`/`lo`，背后却是两张不同的证书**——一张 α=0.10
    /// 一张 α=0.40，一张按簇取一张按条取。所以它必须跟着进 `calib_hash`：
    /// **只哈希线，就覆盖了线、没覆盖线的凭据。**
    /// **按 `(α, conf_delta, 簇单位)` 寻址**，不是一格覆盖一格。
    ///
    /// 实测过的坑：同一个键认证两次，α=0.60 线 0.195 → α=0.80 线 **0.000**
    /// （**从「过线才放行」变成「全放行」**），而被覆盖过的记录与只认证过一次的记录
    /// **逐字段相同**。**哈希是封条，不是地址**——它答「变了没有」，不答「这是哪一批
    /// 材料、哪个 α 上的」。**要防的不是篡改，是误用与合并**：没有东西被改，
    /// 是**两个不同的测量占了同一个格子**。
    ///
    /// 限定进了地址，取值不同就是不同的键，于是**并存而不是覆盖**。
    #[serde(default)]
    pub certs: std::collections::BTreeMap<String, Cert>,
    /// 运行期积累的观察（与 Python `CalibRecord.samples` 同位）。
    ///
    /// **Python 那边写 `null` 表示「没有标注集」**，这里映射成空表。
    /// 之所以敢合并这两者：**Python 自己的消费方 `runtime.py:1149` 是 `if not rec.samples`,
    /// `None` 与 `[]` 走同一条路**——合并的是一个本来就没有行为差别的区分。
    /// **带标注的那些才是 `n` 的来源**；无标注的只是观察。
    #[serde(default, deserialize_with = "null当空表")]
    pub samples: Vec<Sample>,
}

impl Cert {
    /// 这张证书的**地址**：限定进键，取值不同 → 键不同 → 并存。
    pub fn addr(&self) -> String {
        let c = match self.cost {
            Some((fp, fn_)) => format!("\u{1f}cost({fp},{fn_})"),
            None => String::new(),
        };
        // **不用 `{:.4}`**：那会让第五位小数不同的两张证书拿到同一个地址，
        // 而 `certs` 是 `BTreeMap<addr, Cert>`——**后写的静默覆盖先写的**，
        // 正是这套寻址要消除的那件事。今天 α 全是调用方写的字面量（实测全集：
        // 0.01 / 0.10 / 0.35 / 0.40 / 0.45 / 0.60 / 0.80），**所以撞不到**；
        // 但 α 是公开 API 的参数，**「调用方传一个算出来的 α」是完全正常的事**。
        // `{:?}` 给 f64 的往返精度，一行换掉一颗按取值决定生死的雷。
        //
        // **`cluster_unit` 与 `label_source.addr()` 里都有自由文本**
        // （后者含 `判据: String`，现值「两个模型都同意」）。**文本里若含 `\u{1f}` 就撞地址。**
        // 所以自由文本那两段先哈成定长，**分隔符就再也不可能出现在段内**。
        format!(
            "α={:?}\u{1f}δ={:?}\u{1f}{}{c}\u{1f}{}\u{1f}fp={}",
            self.alpha,
            self.conf_delta,
            hash16(&json!(self.cluster_unit)),
            hash16(&json!(self.label_source.addr())),
            self.label_fp
        )
    }
}

impl CalibRecord {
    /// **选中的那张证书：α 最小的一张。**
    ///
    /// α 越小 = 风险目标越严 = 线越高 = 放行越少 = **越保守**。取最小的那张，
    /// 于是**后认一个更松的 α 永远不会把线放宽**——覆盖那个坑从规则上就不存在了。
    /// 并列时按地址取第一个，保证同一份记录每次选出同一张。
    pub fn 选中的证书(&self) -> Option<&Cert> {
        self.certs.values().min_by(|a, b| {
            // 先按 α：越小 = 风险目标越严 = 越保守
            a.alpha.partial_cmp(&b.alpha).unwrap_or(std::cmp::Ordering::Equal)
                // **同 α 时取线更高的那张。** 代价矩阵进来之后这一格才有内容：
                // 一张 `fn` 重的证书（线 0.385、放行 63）和一张 `fp` 重的（线 0.780、放行 6）
                // 可以在同一个 α 上都认得住，**按地址字符串挑就可能挑中宽松的那张**。
                // 线更高 = 放行更少 = 往拒绝那边倒。
                .then_with(|| b.hi.partial_cmp(&a.hi).unwrap_or(std::cmp::Ordering::Equal))
                // 最后按地址定死，保证同一份记录每次选出同一张
                .then_with(|| a.addr().cmp(&b.addr()))
        })
    }
    /// 积累了多少条观察（**含无标注的**）
    pub fn observations(&self) -> usize {
        self.samples.len()
    }
    /// 其中有真值的有多少条——**只有这些能撑起一条线**
    pub fn labeled(&self) -> usize {
        self.samples.iter().filter(|s| s.label.is_some()).count()
    }
    /// **这条线让哪些出口种类不可达**（`"act"` / `"ignore"`），**算出来的不是填的**。
    ///
    /// `p <= lo` 在 `lo = 0` 时只有 `p` 恰为 0 才成立，`p >= hi` 在 `hi = 1` 时同理
    /// ——那一侧的出口实际上没有了，**而 J-05 仍然强制作者为它写一臂**。
    pub fn 不可达出口(&self) -> Vec<&'static str> {
        let mut v = vec![];
        if self.hi >= 1.0 {
            v.push("act");
        }
        if self.lo <= 0.0 {
            v.push("ignore");
        }
        v
    }
    /// 证据来源：**算出来的**。见 `Provenance`。
    pub fn provenance(&self) -> Provenance {
        // **按 `labeled()` 显式分档**，不靠一个在 0 上偶然成立的等式。
        match (self.n, self.labeled(), self.observations()) {
            (0, _, 0) => Provenance::空,
            (n, _, 0) if n > 0 => Provenance::宿主手填,
            // **`只有观察` 要求 `n == 0`。**
            //
            // 原来写的是 `(_, 0, obs) if obs > 0`——于是「宿主手填了 n=73 的线、
            // 程序又跑出 1 条无标注观察」被判成 `只有观察`，**把宿主那 73 条说没了**。
            // **这一格固定观察测不到**：测试要么只 `put`、要么只 `absorb`，
            // **而真机的工作流天然是两者叠加**（有线的键上跑程序）。
            // E-JPP-LIVE 第一次真机跑完折证据时撞出来的。
            (0, 0, obs) if obs > 0 => Provenance::只有观察,
            (n, l, _) if n as usize == l => Provenance::程序积累,
            _ => Provenance::混合,
        }
    }
}

/// 一个**类假设档案字段**的三态（`12` §1.2 绑字段、§1.3 末行「任一字段『未测』」）。
///
/// **不是 `Option<bool>`**：`未测` 是 §1.3 亲口规定过的一个**合法状态**，有自己的降级
/// 规则（按 J-15 取该假设为真、报 W-untested）。写成 `Option` 会招来 `unwrap_or(false)`
/// ——**那正是替不确定做了确定的默认**。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Tri {
    真,
    假,
    /// **档案里有这份档案，但这个字段没测过。** 与「根本没加载档案」不是一回事——
    /// 前者是一份测量报告说「这项没测」，后者是连报告都没有。
    #[default]
    未测,
}

impl Tri {
    pub fn from_json(j: &Json, field: &str) -> Tri {
        match j.get(field).and_then(|v| v.as_bool()) {
            Some(true) => Tri::真,
            Some(false) => Tri::假,
            None => Tri::未测,
        }
    }
}

/// 档案字段（§1「凡是数字都是档案字段」）：这些数不是代码常数，是档案里测出来的。
/// 缺省值取 Python 内核代码里的同名兜底：冷键无线（hi=1, lo=0，一切都在带内 = 最保守），
/// δ 取 noul 0.05 / choice 0.15 / score 0.15。接线时应当从档案覆盖它们。
#[derive(Clone, Debug, PartialEq)]
pub struct Profile {
    /// 冷键（非上岗）用的保守线
    pub safety: (f64, f64),
    /// 每种题式的 δ：noul / choice / score
    pub delta: (f64, f64, f64),
    /// 对象槽内**单段**材料的可用窗口（`window.text_slots.claim_bearing_ctx.usable_lower`）
    pub text_window: usize,
    /// 槽间干扰的窗口（`window.json_slots…flip_frac_by_ctx_tokens` 的最大档）
    pub json_ctx_window: usize,
    /// 行为承载子集的摘要：只覆盖会进入执行的字段。`profile_hash` 不同时，它相同
    /// 就说明**只改了说明文字，行为没变**。**它不放行任何东西**，只回答「哪一种不同」。
    pub behavior_hash: Option<String>,
    /// **H5 的档案字段**（`12` §1.2）：模型会不会算术。
    ///
    /// 注意方向：**`arithmetic_capable: true` 表示 H5 _不_ 成立**。
    /// §1.3 的降级：为真时「J-01 的『算术在宿主』降 warn；`fit` 仍是唯一跨题合成（I3 不变）」。
    ///
    /// **今天真档案里没有这个字段**，所以读出来是 `未测`——这正是 §1.2 八条里缺的那几条之一，
    /// 而缺字段**不该由内核替它编一个值**。
    pub arithmetic_capable: Tri,
    /// 这份档案的哈希，进账本头（`12` §J-18：头不同即不承诺重放一致）。
    /// **`None` = 本次运行没有加载档案**，线与 δ 是代码兜底值——这个痕迹必须留下，
    /// 否则「用了兜底值」和「档案恰好等于兜底值」在账本上分不开。
    pub hash: Option<String>,
}

impl Default for Profile {
    /// 代码兜底值。**它不是一份档案**，所以 `hash` 是 `None`——这是它与「加载到的档案」
    /// 唯一的区别，也是账本头上看得出来的那个区别。
    /// 兜底值本身合法（测试与不接档案的调用方要用），**不合法的是 `load` 悄悄回退到它**。
    /// 取值与 Python 内核同名兜底一致（`runtime.py` 的 `delta_for` / `safety_lines` 的 except 分支）。
    fn default() -> Profile {
        Profile { safety: (1.0, 0.0), delta: (0.05, 0.15, 0.15), text_window: 1000, json_ctx_window: 1800, behavior_hash: None, hash: None, arithmetic_capable: Tri::未测 }
    }
}

impl Profile {
    /// 从档案 JSON 读线与 δ（`12` §1「凡是数字都是档案字段」）。
    ///
    /// **读不到就报错，不悄悄回退到兜底值**——那正是「替不确定说确定」。调用方要兜底，
    /// 得自己显式写 `Profile::default()`，那时 `hash` 是 `None`，账本头看得见。
    ///
    /// 取值路径与 Python 一致，不是抄结果：`lines.safety_default.{hi,lo}`、
    /// `delta.{noul,choice_prob_chosen,score}.immediate.p99`。
    pub fn load(path: &std::path::Path) -> Result<Profile, String> {
        let text = std::fs::read_to_string(path).map_err(|e| format!("读不到档案 {}：{e}", path.display()))?;
        let j: Json = serde_json::from_str(&text).map_err(|e| format!("档案不是合法 JSON：{e}"))?;
        Profile::from_json(&j)
    }

    pub fn from_json(j: &Json) -> Result<Profile, String> {
        let 取 = |路径: &[&str]| -> Option<f64> {
            let mut cur = j;
            for k in 路径 {
                cur = cur.get(k)?;
            }
            cur.as_f64()
        };
        let hi = 取(&["lines", "safety_default", "hi"]).ok_or("档案缺 lines.safety_default.hi")?;
        let lo = 取(&["lines", "safety_default", "lo"]).ok_or("档案缺 lines.safety_default.lo")?;
        let dn = 取(&["delta", "noul", "immediate", "p99"]).ok_or("档案缺 delta.noul.immediate.p99")?;
        // choice 的 δ 取「被选中那档的概率」那一列（与 Python `delta_for` 的映射一致）
        let dc = 取(&["delta", "choice_prob_chosen", "immediate", "p99"]).ok_or("档案缺 delta.choice_prob_chosen.immediate.p99")?;
        let ds = 取(&["delta", "score", "immediate", "p99"]).ok_or("档案缺 delta.score.immediate.p99")?;
        // 窗口也是档案字段（`12` §1「凡是数字都是档案字段」），**与线、δ 同一条**。
        //
        // 这里原本是 `.unwrap_or(1000)` / `.unwrap_or(1800)`——同一个函数里对线严、对窗口松，
        // 而那两个兜底值正是**在替不确定说确定**（同一形状的第六个实例，且是今夜写的代码）。
        // 改成与线一样缺了就报错：**档案缺字段是档案的问题，不该由内核替它编一个数**。
        // 要兜底的调用方显式写 `Profile::default()`，那时 `hash` 是 `None`、账本头看得见。
        let text_window = 取(&["window", "text_slots", "claim_bearing_ctx", "usable_lower"])
            .map(|v| v as usize)
            .ok_or("档案缺 window.text_slots.claim_bearing_ctx.usable_lower")?;
        let json_ctx_window = j
            .get("window")
            .and_then(|w| w.get("json_slots"))
            .and_then(|s| s.get("claim_bearing_ctx"))
            .and_then(|c| c.get("flip_frac_by_ctx_tokens"))
            .and_then(|m| m.as_object())
            .and_then(|m| m.keys().filter_map(|k| k.trim_start_matches('~').parse::<usize>().ok()).max())
            .ok_or("档案缺 window.json_slots.claim_bearing_ctx.flip_frac_by_ctx_tokens")?;
        Ok(Profile { safety: (hi, lo), delta: (dn, dc, ds), text_window, json_ctx_window, behavior_hash: Some(behavior_hash(j)), hash: Some(profile_hash(j)), arithmetic_capable: Tri::from_json(j, "arithmetic_capable") })
    }
}

/// **行为承载子集摘要**：只覆盖会进入执行的字段，不覆盖 `note`/`source`/`value` 那类说明。
///
/// 为什么要它（总控 2026-09-21 §2.10 修订）：`profile_hash` 覆盖整份档案是对的——头不同
/// 只说「不承诺重放一致」，倒向拒绝那一侧。**但它只给一个比特，而现场有两个**：
/// 一晚上两次纯文档更正把对照基准打红、**行为一个字节没变**。那会造出
/// 「**改对文档要付代价**」的反向激励，而同一个数被写错三次正是在这个激励下发生的。
///
/// **它不放行任何东西**：`profile_hash` 不同时该怎么判还怎么判。它只让看账本的人知道
/// 这次不同属于哪一种：**线和 δ 也变了，还是只改了说明。**
pub fn behavior_hash(j: &Json) -> String {
    // 会进入执行的字段。加字段时要同步这里——漏加的后果是「行为变了但摘要没变」，
    // 比多加一个字段糟得多，所以宁可多收。
    const 行为字段: &[&str] = &["lines", "delta", "window", "k_limit", "position_bias", "anchors", "concurrency", "cost", "select_sums_to_one", "fixed_output_types"];
    let mut sub = serde_json::Map::new();
    for k in 行为字段 {
        if let Some(v) = j.get(*k) {
            sub.insert((*k).to_string(), strip_prose(v));
        }
    }
    hash16(&Json::Object(sub))
}

/// 去掉说明性字段：它们改了不影响执行
fn strip_prose(j: &Json) -> Json {
    const 说明字段: &[&str] = &["note", "source", "by", "reason", "status", "notes", "comment"];
    match j {
        Json::Object(m) => Json::Object(m.iter().filter(|(k, _)| !说明字段.contains(&k.as_str())).map(|(k, v)| (k.clone(), strip_prose(v))).collect()),
        Json::Array(a) => Json::Array(a.iter().map(strip_prose).collect()),
        other => other.clone(),
    }
}

fn hash16(j: &Json) -> String {
    use sha2::{Digest, Sha256};
    let mut h = Sha256::new();
    h.update(canon(j).as_bytes());
    h.finalize().iter().map(|b| format!("{b:02x}")).collect::<String>()[..16].to_string()
}

/// **校准库的哈希**（进账本头，J-18）。
///
/// 为什么要它：**出口 = f(读数, 线)。读数进了账本，线没有。** 头里的 `profile_hash`
/// 记的是**档案里的缺省线**，而 `cut` 用的是**按键的线**——同一份账本换一批校准记录重放，
/// 读数一样、出口可以不一样，**而账本上看不出**。运行期写入口接上之后，
/// 校准记录在两次运行之间会长，所以这不再是理论上的可变。
///
/// **用排除法，不用列举法。** 覆盖整条记录，只减去一张封闭的「说明字段」清单。
/// 反面教材就在下面几行：[`behavior_hash`] 是列举法，**它自己的注释承认了失效方式**
/// ——「加字段时要同步这里——漏加的后果是『行为变了但摘要没变』」。列举法在有人加新字段时
/// **会说出一个假的「相同」**，而假的「相同」倒向放行那一侧。
///
/// **空库也有哈希，不是 `None`。** 头里的 `None` 只该有一个意思：**这份账本早于这个字段**。
/// 让空库也占 `None`，两种情形在账本上就分不开。
pub fn calib_hash(store: &CalibStore) -> String {
    /// 说明性字段：改了不影响执行，所以不进哈希。
    /// **今天这张表是空的**——`CalibRecord` 每个字段都承载行为（`hi`/`lo`/`n`/`status` 进 `cut`，
    /// `delta` 进 `delta_for`，`unsure_rate` 进 J-10，`set_id` 进 J-16，`samples` 是证据本身）。
    /// **空表不是摆设**：它是加 `note` 那类字段时该动的那一处，
    /// 有了它，新字段的默认归宿是「进哈希」而不是「被忘掉」。
    const 说明字段: &[&str] = &[];

    // BTreeMap：哈希不随写入顺序变，否则同一批线会因写入顺序不同报假 W-header
    let mut sorted: std::collections::BTreeMap<&str, Json> = Default::default();
    for (k, rec) in &store.records {
        let mut j = serde_json::to_value(rec).unwrap_or(Json::Null);
        if let Json::Object(m) = &mut j {
            m.retain(|f, _| !说明字段.contains(&f.as_str()));
        }
        sorted.insert(k.as_str(), j);
    }
    hash16(&Json::Array(sorted.into_values().collect()))
}

/// 与 Python 的 `H(profile)` 同值：`sha256(canon([profile]))` 取前 16 个十六进制字符。
/// **必须同值**——它进账本头，两边算不出同一个数，跨内核的重放判定就对不上。
pub fn profile_hash(j: &Json) -> String {
    use sha2::{Digest, Sha256};
    let wrapped = Json::Array(vec![j.clone()]);
    let mut h = Sha256::new();
    h.update(canon(&wrapped).as_bytes());
    h.finalize().iter().map(|b| format!("{b:02x}")).collect::<String>()[..16].to_string()
}

use crate::conformal::Certificate;

/// `commission` 失败的两种，**不能混成一种**。
///
/// 「认证跑完了、结论是拒绝」带着 `n_needed`——**它告诉作者这条路有终点**。
/// 「根本没跑成」（α 越界、没有标注样本、声明了簇却没有簇 id）**没有终点可报**，
/// 报一个假的 `n_needed` 比不报更糟：作者会照着那个数去凑样本，而问题根本不在样本数上。
#[derive(Clone, Debug, PartialEq)]
pub enum Refusal {
    /// 认证跑了，这批数据撑不起这个 α
    认证不过(Certificate),
    /// 认证没跑：入参或记录状态不对
    跑不成(String),
}

impl Refusal {
    /// 还差多少条零错放行才认得动。**「跑不成」没有这个数**——不编一个出来。
    pub fn n_needed(&self) -> Option<usize> {
        match self {
            Refusal::认证不过(Certificate::Refused { n_needed, .. }) => Some(*n_needed),
            _ => None,
        }
    }
}

/// 四个合法状态（与 Python `calib.py:13` 同）
pub const STATUSES: [&str; 4] = ["冷", "上岗", "停岗", "待真值"];

#[derive(Clone, Debug, Default)]
pub struct CalibStore {
    pub records: HashMap<String, CalibRecord>,
    pub profile: Profile,
}

impl CalibStore {
    pub fn new() -> CalibStore {
        CalibStore::default()
    }
    /// 只由校准过程写；程序里不可调用（J-03 的运行期面）。
    ///
    /// **宿主能在这里写线，这是设计意图**——`12`:64 I4 写着「线只从校准记录来（标注集、
    /// 保形、代价），**程序里**不可写线」，拦的是程序，不是校准过程。**但正因如此，
    /// 来源必须能分辨**：否则「线来自校准」这句话在审计上是空的。见 `Provenance`。
    pub fn put(&mut self, key: &str, hi: f64, lo: f64, n: u64, status: &str) -> Result<(), String> {
        if !STATUSES.contains(&status) {
            return Err(format!("status 只能是 {STATUSES:?}，收到 {status:?}"));
        }
        if status == "上岗" && n == 0 {
            return Err("上岗记录必须带 n > 0（线只从标注记录来）".into());
        }
        if !(0.0..=1.0).contains(&hi) || !(0.0..=1.0).contains(&lo) || lo > hi {
            return Err("线必须满足 0 ≤ lo ≤ hi ≤ 1".into());
        }
        // **证书门**：**带标注的**证据要让键上岗，必须过认证——手写一条线不算凭据。
        //
        // **取的量是 `labeled()`，不是 `samples.len()`。** 一条只有无标注观察的记录
        // **没有积累任何「关于线的证据」，它只是判过几次**；拿它挡 `put`，是把观察当成了证据。
        // 而运行期写入口推的每一条样本 `label` 都是 `None`（真值通道还不存在），
        // 所以按 `samples.len()` 拦会**把「跑程序 → 人写线 → 上岗」这条最常规的路彻底锁死**。
        if status == "上岗" {
            if let Some(old) = self.records.get(key) {
                // **收紧永远放行，放宽才要凭据。**
                //
                // 这一格是被自己的门夹出来的：原来拦住之后记录保持原状，**旧线继续放行，
                // 而人已经不能收紧它了**——那是失败开放。我第一版改成「拦住就停岗」，
                // 结果更糟：`commission` 明写着不经由它复岗，于是那个键**永久死掉**，
                // **正是这一包在修的那一类锁**。
                //
                // 真正的判据是方向——**但「方向」要按出口种类算，不按 `Act` 一种算**。
                //
                // 原来写的是 `hi >= old.hi && lo <= old.lo`，理由是「带更宽 = `Unsure` 更多
                // = 往拒绝那边倒」。**那句把「拒绝」默认等同于「不给 `Act`」**——
                // 而 `Act` 与 `Ignore` 是**对称的两个判定**：写 `if 不安全(x) { 拦下 }` 时，
                // **`Ignore` 才是放行的那个答案**。**语言不知道哪一侧对这个程序才是安全的那一侧。**
                //
                // **一个改动若使某个出口种类变得不可达，它就不是「收紧」，不论方向。**
                // 这个活口子最难看的地方是：`lo → 0` 免凭据，**而那正是 `commission`
                // 自己做的操作**——它有证书所以照办，手写的没有。
                let 新塌 : Vec<&str> = {
                    let 旧 = old.不可达出口();
                    let 新 = CalibRecord { hi, lo, ..old.clone() }.不可达出口();
                    新.into_iter().filter(|k| !旧.contains(k)).collect()
                };
                let 收紧 = old.status == "上岗" && hi >= old.hi && lo <= old.lo && 新塌.is_empty();
                if old.labeled() > 0 && !收紧 {
                    let 塌 = if 新塌.is_empty() {
                        String::new()
                    } else {
                        format!("。**这次改动会让出口 {} 变得不可达——那不是收紧，不论方向**", 新塌.join(" / "))
                    };
                    return Err(format!(
                        "键 {key} 上有 {} 条**带标注**的证据，手写的线不是凭据：走 commission(key, alpha, conf_delta, cluster_unit) 让证书定线。\
                         （收紧现有线——`hi` 不降、`lo` 不升、**且不让任何一种出口变得不可达**——不需要凭据，随时可写）{塌}",
                        old.labeled()
                    ));
                }
            }
        }
        // **写线不抹证据**：积累起来的观察不是这次写线的人的东西。
        // 与「`Ledger::put` 只增不改」同一条纪律——抹掉了，`provenance` 就再也分不出混合。
        let samples = self.records.get(key).map(|r| r.samples.clone()).unwrap_or_default();
        let certs = self.records.get(key).map(|r| r.certs.clone()).unwrap_or_default();
        let lsid = self.records.get(key).map(|r| r.label_set_id.clone()).unwrap_or_default();
        let lsrc = self.records.get(key).map(|r| r.label_source.clone()).unwrap_or_default();
        let lloc = self.records.get(key).map(|r| r.label_locator.clone()).unwrap_or_default();
        let lfp = self.records.get(key).map(|r| r.label_fp.clone()).unwrap_or_default();
        self.records.insert(key.to_string(), CalibRecord { key: key.into(), hi, lo, n, status: status.into(), delta: None, unsure_rate: None, set_id: String::new(), label_set_id: lsid, label_locator: lloc, label_fp: lfp, label_source: lsrc, samples, certs });
        Ok(())
    }

    /// **运行期写入口**（`12`:347 两样「未定」里的第二样）：把程序跑出来的一条观察折进记录。
    ///
    /// **它不写线，也不能让记录上岗。** 无标注的观察只把冷记录推到 `待真值`——
    /// 四个状态里本来就为这件事留了那一格。**`n` 只随带标注的样本长**，因为
    /// `put` 自己的报错写着「线只从**标注**记录来」。
    pub fn absorb(&mut self, key: &str, s: Sample) -> Result<(), String> {
        if let Some(p) = s.p {
            if !(0.0..=1.0).contains(&p) {
                return Err(format!("样本 p 必须在 0..=1，收到 {p}"));
            }
        }
        if let Some(l) = s.label {
            if l > 1 {
                return Err(format!("label ∈ {{0, 1}}，收到 {l}"));
            }
        }
        let rec = self.records.entry(key.to_string()).or_insert_with(|| CalibRecord {
            key: key.into(), hi: 0.65, lo: 0.35, n: 0, status: "冷".into(),
            delta: None, unsure_rate: None, set_id: String::new(), label_set_id: String::new(), label_locator: String::new(), label_fp: String::new(), label_source: LabelSource::default(), samples: vec![], certs: Default::default(),
        });
        let 有标注 = s.label.is_some();
        rec.samples.push(s);
        if 有标注 {
            rec.n += 1;
        }
        // **停岗不因为来了新观察就复岗**：停岗是人下的判断，不是样本数的函数。
        if rec.status == "冷" {
            rec.status = "待真值".into();
        }
        Ok(())
    }
    /// **上岗的正门**：拿这条键积累来的标注样本跑保形认证，**认过才上岗，线由证书定**。
    ///
    /// Experimental host policy: selection and pointwise bounds reuse samples.
    /// A returned `Cert` is not yet a general finite-sample risk guarantee.
    ///
    /// `cluster_unit` **必须由调用方声明**，不许从数据推断。推断出来的默认会造出一张
    /// 写着「按条核过」的证书，**而真相是没人说过簇是什么**——那正是「给没有类型的东西
    /// 补来源」那个形状。声明「对象段」而样本没有簇 id 是**错，不是降级**。
    ///
    /// 认证不过时返回 `Certificate::Refused`，**带着 `n_needed`**——
    /// 它是唯一告诉作者「这条路有终点」的东西。**记录不动，留在 `待真值`**：
    /// 不阻塞、但也不放行。
    pub fn commission(&mut self, key: &str, alpha: f64, conf_delta: f64, cluster_unit: &str) -> Result<Cert, Refusal> {
        self.commission_inner(key, alpha, conf_delta, cluster_unit, None)
    }

    /// **代价矩阵定线、证书定能不能上岗**（那条裁定的两半合起来）。
    ///
    /// The two declared dataset IDs are checked, but this implementation still
    /// computes selection and validation from the same stored samples. Distinct
    /// IDs alone do not establish independent holdout validation.
    ///
    /// `certify` 自己会去找一条最宽的、仍被认证住的线；**给了代价矩阵就不找了**——
    /// 线由 `cost_line` 在标注集上按 `fp·#误放行 + fn·#漏放行` 最小定出来，
    /// 证书只回答**这条线在这批数据上的假放行上界够不够 α**。
    /// 这是那条裁定的字面实现：**代价决定线定在哪，证书决定能不能上岗，不是二选一。**
    pub fn commission_costed(&mut self, key: &str, alpha: f64, conf_delta: f64, cluster_unit: &str, cost: (f64, f64)) -> Result<Cert, Refusal> {
        self.commission_inner(key, alpha, conf_delta, cluster_unit, Some(cost))
    }

    fn commission_inner(&mut self, key: &str, alpha: f64, conf_delta: f64, cluster_unit: &str, cost: Option<(f64, f64)>) -> Result<Cert, Refusal> {
        // **先卡区间再造证书**：α=0 会让 `n_needed_zero_error` 得到 inf，
        // 而 `serde_json::to_value` 在 NaN/inf 上失败 → `calib_hash` 把整条记录哈成 Null，
        // **两条坏记录于是哈出同一个值**。区间在进哈希之前卡住。
        let bad = |why: &str| Refusal::跑不成(why.to_string());
        if !(alpha > 0.0 && alpha < 1.0) || !(conf_delta > 0.0 && conf_delta < 1.0) {
            return Err(bad("alpha 与 conf_delta 必须在开区间 (0, 1)"));
        }
        let rec = match self.records.get(key) {
            Some(r) => r,
            None => return Err(bad("没有这条记录")),
        };
        if rec.status == "停岗" {
            // 停岗是人下的判断，不是样本数的函数——**重新认证不是复岗的通道**
            return Err(bad("停岗的键不经由 commission 复岗"));
        }
        let rec_lsrc = rec.label_source.clone();
        let 带标注: Vec<&Sample> = rec.samples.iter().filter(|s| s.label.is_some() && s.p.is_some()).collect();
        // **一条记录只能有一个题型。** 代价路径一直查这个，而这条非代价路径从来没查过——
        // **不同尺不可比**是本项目自己的判据，认证路径上漏了一处。
        //
        // 这一条也是 `LabelSource::选择子集.与对错相关` **能是一个标量的前提**：
        // 相关性按题型分叉（noul +0.527 / choice +0.418 / **score −0.062，反向**），
        // 一条记录一个值之所以够用，**正是因为一条记录就是一个题型**。
        // 在这之前那是约定不是保证——`absorb` 收任何 `phys` 字符串。
        {
            let mut 见到: std::collections::BTreeSet<&str> = Default::default();
            for s in &带标注 {
                见到.insert(s.phys.as_str());
            }
            if 见到.len() > 1 {
                return Err(Refusal::跑不成(format!(
                    "键 {key} 的标注样本混了 {} 种物理形式（{}）：不同尺不可比，一条记录只能认一个题型的线",
                    见到.len(),
                    见到.into_iter().collect::<Vec<_>>().join(" / ")
                )));
            }
        }
        if 带标注.is_empty() {
            return Err(bad("没有带标注的样本：线只从标注记录来"));
        }
        // **指纹算的是「用到的那些 `(p, label)` 对的规范形」，不是源文件字节。**
        // 源文件会被重新导出、重新排序——按字节算会在数据没变时乱跳，
        // **而乱跳的检查会教会人绕过它**。
        let label_fp = 标注集指纹(&带标注);
        if !rec.label_fp.is_empty() && rec.label_fp != label_fp {
            return Err(Refusal::跑不成(format!(
                "键 {key} 声明的标注集是 {:?}（指纹 {}），而这次用到的那批指纹是 {label_fp}：对不上就不认证",
                rec.label_set_id, rec.label_fp
            )));
        }
        let 按条: Vec<(f64, bool)> = 带标注.iter().map(|s| (s.p.expect("已滤"), s.label == Some(1))).collect();

        // **代价线分支**：线不由证书自己找，由代价矩阵在标注集上定。
        if let Some((fp, fn_)) = cost {
            // Python `runtime.py:1147` 直接 raise：select / measure 的代价线**未定**。
            // 让它悄悄什么也不做，比报错糟。
            if 带标注.iter().any(|s| s.phys != "noul") {
                return Err(Refusal::跑不成(format!(
                    "cut(cost=) 只对 test 题有定义（select / measure 的代价线未定）；键 {key} 上有非 noul 的样本"
                )));
            }
            // J-16：代价线与保形线不能同源。**这一条 Python 也拦**（`runtime.py:1151`）。
            let (lsid, sid) = (rec.label_set_id.clone(), rec.set_id.clone());
            if lsid.is_empty() || lsid == sid {
                return Err(Refusal::跑不成(format!(
                    "J-16: 校准键 {key} 的标注集 id（{lsid:?}）必须给出且 ≠ 保形集 id（{sid:?}）；代价线与保形线不能同源"
                )));
            }
            let cl = match crate::conformal::cost_line(&按条, fp, fn_) {
                Ok(c) => c,
                Err(e) => return Err(Refusal::跑不成(e)),
            };
            // 证书只回答「这条线够不够 α」，不再自己找线
            let ucb = crate::conformal::binomial_upper(cl.n_false_accept, cl.n_accepted, conf_delta);
            if cl.n_accepted == 0 || ucb > alpha {
                return Err(Refusal::认证不过(Certificate::Refused {
                    best_ucb: ucb,
                    best_hi: cl.line,
                    best_n_accepted: cl.n_accepted,
                    n_needed: crate::conformal::n_needed_zero_error(alpha, conf_delta),
                }));
            }
            let cert = Cert { alpha, conf_delta, hi: cl.line, n_accepted: cl.n_accepted, n_errors: cl.n_false_accept, ucb, cluster_unit: cluster_unit.into(), resample: None, cost, bounded_side: 单侧声明(), label_source: rec_lsrc.clone(), label_fp: label_fp.clone() };
            let r = self.records.get_mut(key).expect("刚读过");
            r.certs.insert(cert.addr(), cert.clone());
            let 选中 = r.选中的证书().cloned().expect("刚插进去");
            r.hi = 选中.hi;
            r.lo = 0.0;
            r.status = "上岗".into();
            return Ok(cert);
        }
        let (cert_result, resample) = if cluster_unit == "条" {
            (crate::conformal::certify(&按条, alpha, conf_delta), None)
        } else {
            // 声明了簇，就必须每条都带簇 id
            if 带标注.iter().any(|s| s.cluster.is_none()) {
                return Err(bad("声明了簇单位，但有样本没有簇 id：这是错，不是降级"));
            }
            let 三元: Vec<(f64, bool, String)> = 带标注.iter()
                .map(|s| (s.p.expect("已滤"), s.label == Some(1), s.cluster.clone().expect("已核")))
                .collect();
            // **全过才算过**（说不准往拒绝那边倒）。这条规则**未标定**——原型只报了
            // 「200 次里有解几次」，没有定「几次算过」。记进证书是为了它可审。
            const R: usize = 200;
            let mut 最差: Option<Certificate> = None;
            let mut 全过 = true;
            let mut 成的: Option<Certificate> = None;
            for seed in 0..R as u64 {
                let c = crate::conformal::certify(&crate::conformal::cluster_subsample(&三元, seed), alpha, conf_delta);
                if c.is_refused() {
                    全过 = false;
                    最差 = Some(c);
                    break;
                }
                if 成的.is_none() {
                    成的 = Some(c);
                }
            }
            let out = if 全过 { 成的.expect("R > 0") } else { 最差.expect("刚设的") };
            (out, Some((R, "全过才算过（未标定）".to_string())))
        };

        match cert_result {
            Certificate::Refused { .. } => Err(Refusal::认证不过(cert_result)),
            Certificate::Line { hi, n_accepted, n_errors, ucb } => {
                let cert = Cert { alpha, conf_delta, hi, n_accepted, n_errors, ucb, cluster_unit: cluster_unit.into(), resample, cost, bounded_side: 单侧声明(), label_source: rec_lsrc.clone(), label_fp: label_fp.clone() };
                let r = self.records.get_mut(key).expect("刚读过");
                // 同一个地址是**更新那一格**；不同地址是**新增一格**
                r.certs.insert(cert.addr(), cert.clone());
                // `hi`/`lo` 是**选择规则算出来的视图**，不是第二处真相——
                // 它和 `line_source` 报的那张必须出自同一个函数，否则两处会各说各的。
                let 选中 = r.选中的证书().cloned().expect("刚插进去");
                r.hi = 选中.hi;
                // **线由证书定，不由调用方写。**
                //
                // `lo = 0.0`：**证书只管放行那一侧**（损失 = 放行区里的假放行），
                // 弃权那一侧它一个字也没说。没有凭据就不放行任何 `Ignore`，
                // 于是带宽最大、`Unsure` 最多——**说不准往拒绝那边倒**。
                // 保留记录原有的 `lo` 是不行的：`hi` 可能落到它下面（实测 hi=0.295 而
                // 缺省 lo=0.35），那样带就翻了，而 `put` 的 `lo ≤ hi` 也会被自己人违反。
                r.lo = 0.0;
                r.status = "上岗".into();
                Ok(cert)
            }
        }
    }

    /// **这个键的无标签漂移报告**（`12`:396「漂移监控（无标签：读数分布偏移 + 保形覆盖跌落告警）」）。
    ///
    /// **参照分布 = 带标注的那些**（线是在它们上面定的），
    /// **近期分布 = 运行期积累的无标注观察**（`absorb` 从 `Outcome.evidence` 收来的）。
    /// **数据已经在记录里，不需要新的数据源。**
    ///
    /// **一侧为空返回 `None`，不是「漂移为 0」**——与 `binomial_upper` 的 `n == 0 → 1.0`
    /// 同一条：**算不出来不是一个值**。
    pub fn drift_of(&self, key: &str) -> Option<crate::conformal::DriftReport> {
        let rec = self.records.get(key)?;
        let (参照, 近期): (Vec<f64>, Vec<f64>) = (
            rec.samples.iter().filter(|s| s.label.is_some()).filter_map(|s| s.p).collect(),
            rec.samples.iter().filter(|s| s.label.is_none()).filter_map(|s| s.p).collect(),
        );
        if 参照.is_empty() || 近期.is_empty() {
            return None;
        }
        Some(crate::conformal::drift(&参照, &近期, 10))
    }

    /// 这个键选中的那张证书（见 [`CalibRecord::选中的证书`]）。
    pub fn 选中的证书(&self, key: &str) -> Option<Cert> {
        self.records.get(key).and_then(|r| r.选中的证书().cloned())
    }

    /// 标注集上实测的 unsure 率（J-10）。只有上岗记录的这个值会被 `unsure_bound` 采信。
    pub fn set_unsure_rate(&mut self, key: &str, rate: f64) -> Result<(), String> {
        if !(0.0..=1.0).contains(&rate) {
            return Err("unsure_rate 必须在 0..=1".into());
        }
        let r = self.records.get_mut(key).ok_or_else(|| format!("没有校准记录 {key}"))?;
        r.unsure_rate = Some(rate);
        Ok(())
    }
    /// **从一个目录装载校准记录**（每键一个 JSON，与 Python `CalibStore(path)` 同格式）。
    ///
    /// 在这之前 `CalibStore` **没有任何落盘/装载**，CLI 也从不构造它——
    /// **所以 E-CAL 那三条真记录，内核一次也没读过。**
    ///
    /// **不许静默丢字段。** 与 `Profile::load` 缺字段就报错同一条纪律：
    /// 文件里有而内核没地方放的字段，要么报错、要么**具名地**「知道但不映射」。
    /// 无声吞掉一个字段，和把限定写进注释是同一件事——**那个数会照常丢掉**。
    pub fn load(dir: &std::path::Path) -> Result<CalibStore, String> {
        /// 知道、但**故意不映射**的字段。**这张表是具名的**：
        /// `source` 是自由散文（Python 全仓零引用，E-CAL 把 `label_source` 埋在里面）；
        /// `cost_matrix` / `drift_stat` 今天 Rust 侧没有对应承载，**而留一个没有
        /// 生产者也没有消费者的字段是另一个错**（见 INTERFACE 四·十六）。
        const 知道但不映射: &[&str] = &["source", "cost_matrix", "drift_stat"];
        let mut store = CalibStore::new();
        let mut names: Vec<_> = std::fs::read_dir(dir)
            .map_err(|e| format!("读不到校准目录 {}：{e}", dir.display()))?
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| p.extension().map(|x| x == "json").unwrap_or(false))
            .collect();
        names.sort();
        for path in names {
            let text = std::fs::read_to_string(&path).map_err(|e| format!("读不到 {}：{e}", path.display()))?;
            let j: Json = serde_json::from_str(&text).map_err(|e| format!("{} 不是合法 JSON：{e}", path.display()))?;
            let obj = j.as_object().ok_or_else(|| format!("{} 不是对象", path.display()))?;
            // **认得的字段是算出来的，不是写出来的。**
            //
            // 这里原来是一张**手写的列举表**——我给 `calib_hash` 选了排除法、
            // 给这里留了列举法，**而它在几小时内就漂了**：
            // `label_locator` / `label_fp` 是结构体字段，**却不在那张表里**，
            // 于是 `--calib-out` 写出来的目录**喂不回 `--calib`**（实测报「不认得的字段 label_fp」）。
            // **写的和读的对不上，而两边各自都有测试。**
            //
            // 改成从**结构体自己**问：序列化一条默认记录，它的键就是认得的全集。
            // **算得出来的不许填**——这条规则我今晚用了三次，唯独在这里没用。
            let 映射: Vec<String> = match serde_json::to_value(CalibRecord {
                key: String::new(), hi: 0.0, lo: 0.0, n: 0, status: "冷".into(),
                delta: None, unsure_rate: None, set_id: String::new(),
                label_set_id: String::new(), label_locator: String::new(), label_fp: String::new(),
                label_source: LabelSource::default(), samples: vec![], certs: Default::default(),
            }) {
                Ok(Json::Object(m)) => m.keys().cloned().collect(),
                _ => return Err("内部错误：CalibRecord 序列化不出对象".into()),
            };
            for k in obj.keys() {
                if !映射.iter().any(|m| m == k) && !知道但不映射.contains(&k.as_str()) {
                    return Err(format!("{} 有内核不认得的字段 {k:?}：装载不猜，也不无声吞掉", path.display()));
                }
            }
            let rec: CalibRecord = serde_json::from_value(j.clone())
                .map_err(|e| format!("{} 读不成校准记录：{e}", path.display()))?;
            if !STATUSES.contains(&rec.status.as_str()) {
                return Err(format!("{} 的 status {:?} 不在 {STATUSES:?} 里", path.display(), rec.status));
            }
            store.records.insert(rec.key.clone(), rec);
        }
        Ok(store)
    }

    /// **把记录落盘**（与 [`CalibStore::load`] 对称：每键一个 JSON）。
    ///
    /// **只写记录，不写 `profile`**——档案是**输入**，从 `--profile` 来，
    /// 把它写进校准目录会造出第二份真相。
    ///
    /// 文件名对键做与 Python `calib.py::_safe` 同样的转义：非字母数字与 `._-` 一律换 `_`。
    pub fn save(&self, dir: &std::path::Path) -> Result<(), String> {
        std::fs::create_dir_all(dir).map_err(|e| format!("建不了 {}：{e}", dir.display()))?;
        for (k, rec) in &self.records {
            let safe: String = k.chars().map(|c| if c.is_alphanumeric() || "._-".contains(c) { c } else { '_' }).collect();
            let j = serde_json::to_string_pretty(rec).map_err(|e| format!("{k} 序列化不了：{e}"))?;
            std::fs::write(dir.join(format!("{safe}.json")), j).map_err(|e| format!("写不了 {k}：{e}"))?;
        }
        Ok(())
    }

    /// 声明这条记录用的是哪一份标注集：`id`（J-16 用）、`locator`（给人看的去处）、
    /// `fingerprint`（**检查只信这个**）。
    pub fn declare_label_set(&mut self, key: &str, id: &str, locator: &str, fingerprint: &str) -> Result<(), String> {
        let r = self.records.get_mut(key).ok_or_else(|| format!("没有校准记录 {key}"))?;
        r.label_set_id = id.to_string();
        r.label_locator = locator.to_string();
        r.label_fp = fingerprint.to_string();
        Ok(())
    }

    /// 声明这条记录的标签是怎么选出来的（见 [`LabelSource`]）
    pub fn set_label_source(&mut self, key: &str, src: LabelSource) -> Result<(), String> {
        let r = self.records.get_mut(key).ok_or_else(|| format!("没有校准记录 {key}"))?;
        r.label_source = src;
        Ok(())
    }
    /// 标注集 id（J-16：代价线与保形线不能同源）
    pub fn set_label_set_id(&mut self, key: &str, id: &str) -> Result<(), String> {
        let r = self.records.get_mut(key).ok_or_else(|| format!("没有校准记录 {key}"))?;
        r.label_set_id = id.to_string();
        Ok(())
    }
    /// 保形集 id（J-16 用）
    pub fn set_set_id(&mut self, key: &str, id: &str) -> Result<(), String> {
        let r = self.records.get_mut(key).ok_or_else(|| format!("没有校准记录 {key}"))?;
        r.set_id = id.to_string();
        Ok(())
    }
    /// 这条记录自己的 δ，覆盖档案默认
    pub fn set_delta(&mut self, key: &str, delta: f64) -> Result<(), String> {
        let r = self.records.get_mut(key).ok_or_else(|| format!("没有校准记录 {key}"))?;
        r.delta = Some(delta);
        Ok(())
    }
    /// **模式级键**（`12`:136「题级样本不够时用模式级校准做先验收缩」）。
    ///
    /// 五元组里去掉 `q_text_hash` 就是模式键。**但不能只剩 `literal_mode`**：那样
    /// 一个模式一格，会把 noul 与 choice 的线并到一起——**不同尺不可比**是本项目自己的
    /// 判据，`delta_for` 也早就按 `op.phys()` 分。所以模式键是 `(phys, literal_mode)`。
    ///
    /// 落在 `\u{1f}` 开头的保留命名空间里，**与任何题级键名都撞不上**。
    pub fn mode_key(phys: &str, mode: LiteralMode) -> String {
        format!("\u{1f}mode\u{1f}{phys}{}", mode.suffix())
    }

    /// 带模式的键。默认档就是裸键名。
    pub fn keyed(key: &str, mode: LiteralMode) -> String {
        format!("{key}{}", mode.suffix())
    }
    /// 带模式地写一条记录（`12`:136 的第五维）
    pub fn put_moded(&mut self, key: &str, mode: LiteralMode, hi: f64, lo: f64, n: u64, status: &str) -> Result<(), String> {
        self.put(&CalibStore::keyed(key, mode), hi, lo, n, status)
    }
    /// 带模式地读一条记录。**没写过的那一档仍是冷的——不继承别的模式的线。**
    pub fn get_moded(&self, key: &str, mode: LiteralMode) -> CalibRecord {
        self.get(&CalibStore::keyed(key, mode))
    }
    pub fn get(&self, key: &str) -> CalibRecord {
        self.records
            .get(key)
            .cloned()
            .unwrap_or(CalibRecord { key: key.into(), hi: 0.65, lo: 0.35, n: 0, status: "冷".into(), delta: None, unsure_rate: None, set_id: String::new(), label_set_id: String::new(), label_locator: String::new(), label_fp: String::new(), label_source: LabelSource::default(), samples: vec![], certs: Default::default() })
    }
    /// 这种题式的 δ：记录自带的优先，否则用档案的
    pub fn delta_for(&self, rec: &CalibRecord, op: Op) -> f64 {
        rec.delta.unwrap_or(match op {
            Op::Test => self.profile.delta.0,
            Op::Select => self.profile.delta.1,
            Op::Measure => self.profile.delta.2,
        })
    }
    /// 判断用的线：上岗记录用自己的，其余一律用档案的保守线（与 Python `_uncertainty` 同口径）
    pub fn lines_for(&self, rec: &CalibRecord) -> (f64, f64) {
        if rec.status == "上岗" { (rec.hi, rec.lo) } else { self.profile.safety }
    }
}
