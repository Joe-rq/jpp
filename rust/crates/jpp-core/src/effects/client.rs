//! 客户端：`Client` 抽象、固定观察、拒绝调用的重放客户端。（步 4c 自 `effects.rs` 原样搬出）

use std::collections::HashMap;

use serde_json::{Value as Json, json};

use crate::value::{Answer, Question, State, canon, hash_of};
pub use jpp_effects::{EffectId, EffectInstance};

/// 概率向量里最大的那一档的下标
pub(crate) fn argmax_index(v: &[f64]) -> usize {
    v.iter()
        .enumerate()
        .fold(
            (0usize, f64::MIN),
            |best, (i, p)| if *p > best.1 { (i, *p) } else { best },
        )
        .0
}

#[derive(Debug, Clone)]
pub struct EffectError(pub String);

/// 带上下文的固定生成键（步 4d）：提示、retry_seq 与 ctx 规范 JSON 的哈希。
fn gen_ctx_key(prompt: &str, retry_seq: u64, ctx: &[Json]) -> String {
    format!(
        "{prompt}\u{1f}{retry_seq}\u{1f}{}",
        hash_of(&["gen-ctx", &canon(&Json::Array(ctx.to_vec()))])
    )
}

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
    fn judge(&mut self, state: &State, questions: &[&Question])
    -> Result<JudgeResult, EffectError>;
    fn generate(
        &mut self,
        prompt: &str,
        ctx: &[Json],
        n: usize,
        retry_seq: u64,
    ) -> Result<GenResult, EffectError>;
    /// 问人：None = 未答（Pending）。
    fn ask(&mut self, state: &State, q: &Question) -> Result<Option<Answer>, EffectError>;
    fn calls(&self) -> u64;

    /// 适配到 `jpp-effects` 的新类型（步 9）：这个客户端以哪个效应实例服务 `effect`。
    /// 本版每种效应一个实例，模型即 `model_id()`（B60）。统一端口 `EffectPort` 在步 15b 取代本 trait。
    fn instance(&self, effect: EffectId) -> EffectInstance {
        EffectInstance {
            effect,
            model: self.model_id(),
        }
    }
    /// 这个客户端服务的效应实例（`judge`/`gen`/`ask`；`do` 与 `transform` 不经客户端）。
    fn instances(&self) -> Vec<EffectInstance> {
        jpp_effects::CLIENT_SERVED
            .iter()
            .map(|e| self.instance(*e))
            .collect()
    }
}

/// 固定观察：只按观察键命中，未命中即错（确定性对照，不是模型）。
#[derive(Default)]
pub struct FixedClient {
    pub table: HashMap<String, Answer>,
    pub gens: HashMap<String, Vec<Json>>,
    pub asks: HashMap<String, Option<Answer>>,
    /// 观察键 → 固定下来的置换测量 `(perms, mode_share)`（步 4d，原型 K6）。
    /// 缺省 = 没测过置换，与改前相同：`select` 得不到 `Pick`。
    pub perms: HashMap<String, (usize, f64)>,
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
    /// 带上下文的固定生成（步 4d，原型 K7）：同提示不同 ctx 可以各给一份输出。
    /// 查找时先按 (提示, retry_seq, ctx 哈希)，查不到再退到不带 ctx 的旧键。
    pub fn fix_gen_ctx(&mut self, prompt: &str, retry_seq: u64, ctx: &[Json], out: Vec<Json>) {
        self.gens.insert(gen_ctx_key(prompt, retry_seq, ctx), out);
    }
    /// 给一条观察固定置换测量（步 4d，原型 K6）。
    pub fn fix_perms(&mut self, key: &str, perms: usize, mode_share: f64) {
        self.perms.insert(key.to_string(), (perms, mode_share));
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
        let mut measured = vec![];
        for q in questions {
            let k = obs_key(state, q);
            measured.push(self.perms.get(&k).copied());
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
        // 没有一条固定过置换测量时与改前逐字节相同（空向量）
        let (mode_share, perms) = if measured.iter().all(Option::is_none) {
            (vec![], vec![])
        } else {
            (
                measured.iter().map(|m| m.map(|(_, s)| s)).collect(),
                measured.iter().map(|m| m.map_or(0, |(k, _)| k)).collect(),
            )
        };
        Ok(JudgeResult {
            answers,
            tokens: 0,
            cost: 0.0,
            mode_share,
            perms,
        })
    }
    fn generate(
        &mut self,
        prompt: &str,
        ctx: &[Json],
        _n: usize,
        retry_seq: u64,
    ) -> Result<GenResult, EffectError> {
        self.n_calls += 1;
        let outputs = self
            .gens
            .get(&gen_ctx_key(prompt, retry_seq, ctx))
            .or_else(|| self.gens.get(&format!("{prompt}\u{1f}{retry_seq}")))
            .cloned()
            .ok_or_else(|| {
                EffectError(format!("固定生成未命中：{prompt} retry_seq={retry_seq}"))
            })?;
        Ok(GenResult {
            outputs,
            tokens: 0,
            cost: 0.0,
        })
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
                canon(
                    &json!({"op": q.op.fixture_name(), "text": q.text, "calib": q.calib, "scale": q.scale})
                ),
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
    ) -> Result<GenResult, EffectError> {
        Err(EffectError(format!("重放中不应发 gen：{p}")))
    }
    fn ask(&mut self, _s: &State, _q: &Question) -> Result<Option<Answer>, EffectError> {
        Err(EffectError("重放中不应发 ask".into()))
    }
    fn calls(&self) -> u64 {
        0
    }
}
