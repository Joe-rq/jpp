//! 键与 id（`20` §2.3 `jpp-ir::key`）：题哈希、状态哈希与账本键都经这里的纯函数算出。
//!
//! 步 6（R）：从 `jpp-core` 的 `value.rs` 与 `ledger.rs` 原样搬来，序列化结果不变；
//! `jpp-core` 在原路径重导出。依据：`21` §三·4 步 6。键的结构化（`JudgeKey` 等改为结构体）在步 7。

use serde::{Deserialize, Serialize};
use serde_json::Value as Json;
use sha2::{Digest, Sha256};

pub fn hash_of(parts: &[&str]) -> String {
    let mut h = Sha256::new();
    for p in parts {
        h.update(p.as_bytes());
        h.update(b"\x1f");
    }
    hex::encode(&h.finalize()[..12])
}

mod hex {
    pub fn encode(b: &[u8]) -> String {
        b.iter().map(|x| format!("{x:02x}")).collect()
    }
}

/// 规范化 JSON（键排序）——账本键与缓存键都建在它上面（§2.10）。
pub fn canon(j: &Json) -> String {
    match j {
        Json::Object(m) => {
            let mut keys: Vec<_> = m.keys().collect();
            keys.sort();
            let inner: Vec<String> = keys
                .iter()
                .map(|k| format!("{}:{}", serde_json::to_string(k).unwrap(), canon(&m[*k])))
                .collect();
            format!("{{{}}}", inner.join(","))
        }
        Json::Array(a) => format!("[{}]", a.iter().map(canon).collect::<Vec<_>>().join(",")),
        // 指数写法要与 Python 的 `json.dumps` 一致：它按 repr 出 `4.2e-08`（两位指数），
        // serde_json 出 `4.2e-8`。这个差别会让同一份档案在两边算出不同的哈希，
        // 而 profile_hash 进账本头——两边对不上，跨内核的重放判定就废了。
        Json::Number(n) => {
            let s = n.to_string();
            match s.split_once('e') {
                Some((mant, exp)) => {
                    let (sign, digits) = match exp.strip_prefix('-') {
                        Some(d) => ("-", d),
                        None => ("+", exp.strip_prefix('+').unwrap_or(exp)),
                    };
                    format!("{mant}e{sign}{:0>2}", digits)
                }
                None => s,
            }
        }
        other => other.to_string(),
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Op {
    Test,
    Select,
    Measure,
}

impl Op {
    /// 夹具 JSON 里认的名字（`test` / `select` / `measure`）。
    ///
    /// **与 `phys()`（`noul`/`choice`/`score`）不是一回事**——前者是题式、后者是物理形式。
    /// 未命中报文以前打的是 `phys()`，**于是那句「照抄这两份 JSON」是假的**：
    /// 夹具只认 `test`。
    pub fn fixture_name(&self) -> &'static str {
        match self {
            Op::Test => "test",
            Op::Select => "select",
            Op::Measure => "measure",
        }
    }
    pub fn phys(&self) -> &'static str {
        match self {
            Op::Test => "noul",
            Op::Select => "choice",
            Op::Measure => "score",
        }
    }
}

/// 渲染版本：判断键、缓存键与账本头的分量（B30），线上请求形状一变就升（B48 重认）。
/// `r2`（B155，步 15i）：state JSON 不再含 `over`，候选只作该 `select` 题的 `criteria`（可带标签名），
/// `test` 可带 `criteria: {true, false}`。只凭账本重放按账本头记的版本算键（`jpp-runtime` `Interp.render`）。
pub const RENDER_VERSION: &str = "r2";

/// 账本键。`site` 是**调用点**（`.jpp` 源码里的字节偏移），与 Python 的
/// `foundation/jv/store.py:26` 同一组成分——那边的 `site` 是「第一个不在 jv 包内的栈帧，
/// `文件名:行号`，同程序重放时稳定」，这边用 `Span.start` 干同一件事。
///
/// **缺了它会撞键**：同状态同题的两个不同站点会合成一条记录，于是第二个站点从账本里
/// 命中第一个站点的答案。那不是漏记，是**命中一条本不该命中的记录**——同一程序里问同一道题
/// 两次是两次判断，合成一次，第二次就不再是一次观察，而是复制第一次。
pub fn judge_key(
    model_id: &str,
    state_hash: &str,
    q_hash: &str,
    phys: &str,
    perm_seed: u64,
    run_seq: u64,
    site: usize,
) -> String {
    hash_of(&[
        "judge",
        model_id,
        state_hash,
        q_hash,
        phys,
        RENDER_VERSION,
        &perm_seed.to_string(),
        &run_seq.to_string(),
        &site.to_string(),
    ])
}

/// 欠账记号的摘要（R9、R4；原型乙 Z0419(1) 的六项构成）：帧种类（`program`、`code`）、主人、过桥种类（线上
/// `cut`、`fit`、`cut_score`；单元图内部另有 `claim`）、第几次过桥、题内容键、原因（B197 的名字）。
/// `jpp-ledger::DebtMark::token` 与 `jpp-cell` 的记号都经这一个函数算（主控 Z0519：只留一份）。不上账本。
pub fn debt_mark_token(
    frame: &str,
    owner: &str,
    via: &str,
    nth: u32,
    key: &str,
    cause: &str,
) -> String {
    hash_of(&["debt-mark", frame, owner, via, &nth.to_string(), key, cause])
}

pub fn effect_key(kind: &str, parts: &[&str]) -> String {
    let mut v = vec![kind];
    v.extend_from_slice(parts);
    hash_of(&v)
}

// ---------------------------------------------------------------- 结构化键（步 7）
//
// 账本 v2 记结构化键，不只记哈希：审计时能看出一条记录是哪个模型、哪个状态、哪道题、哪个站点；
// 深度与缓存（步 17、19）按分量取数。`digest()` 与上面两个函数逐字节相同，账本索引与
// 跨内核对照（`tests/cross_kernel.rs`）不变。依据：`20` §2.3 `jpp-ledger` 键、`21` §三·4 步 7。

/// 判断记录的键（`12` §2.10）。`digest()` == [`judge_key`]。
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct JudgeKey {
    pub model_id: String,
    pub state: String,
    pub q: String,
    pub phys: String,
    pub render: String,
    pub perm_seed: u64,
    pub run_seq: u64,
    pub site: usize,
}

impl JudgeKey {
    pub fn new(
        model_id: &str,
        state: &str,
        q: &str,
        phys: &str,
        perm_seed: u64,
        run_seq: u64,
        site: usize,
    ) -> JudgeKey {
        JudgeKey {
            model_id: model_id.into(),
            state: state.into(),
            q: q.into(),
            phys: phys.into(),
            render: RENDER_VERSION.into(),
            perm_seed,
            run_seq,
            site,
        }
    }
    pub fn digest(&self) -> String {
        hash_of(&[
            "judge",
            &self.model_id,
            &self.state,
            &self.q,
            &self.phys,
            &self.render,
            &self.perm_seed.to_string(),
            &self.run_seq.to_string(),
            &self.site.to_string(),
        ])
    }
    /// 去掉调用位置与运行序号后的缓存键（`12` §2.10、B40）。步 19 启用：选择题（物理形式 `choice`）
    /// 按 `perm_seed` 分开（B40「按 `perm_seed` 索引，命中只复用同 seed」），其余不含它。
    pub fn cache_key(&self) -> CacheKey {
        CacheKey {
            model_id: self.model_id.clone(),
            state: self.state.clone(),
            q: self.q.clone(),
            phys: self.phys.clone(),
            render: self.render.clone(),
            perm_seed: (self.phys == "choice").then_some(self.perm_seed),
        }
    }
}

/// 缓存键：同模型、同状态、同题、同物理形式即同一次观察（B40）。步 19 启用。
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CacheKey {
    pub model_id: String,
    pub state: String,
    pub q: String,
    pub phys: String,
    pub render: String,
    /// 只有选择题填（B40，步 19）；为空时摘要与步 7 定义的相同
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub perm_seed: Option<u64>,
}

impl CacheKey {
    pub fn digest(&self) -> String {
        let seed = self.perm_seed.map(|s| s.to_string());
        let mut parts: Vec<&str> = vec![
            "cache",
            &self.model_id,
            &self.state,
            &self.q,
            &self.phys,
            &self.render,
        ];
        if let Some(s) = &seed {
            parts.push(s);
        }
        hash_of(&parts)
    }
}

/// 效应记录的键（`gen`、`do`、`ask`、`transform`）。`digest()` == [`effect_key`]。
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EffectKey {
    pub kind: String,
    pub parts: Vec<String>,
}

impl EffectKey {
    pub fn new(kind: &str, parts: &[&str]) -> EffectKey {
        EffectKey {
            kind: kind.into(),
            parts: parts.iter().map(|p| p.to_string()).collect(),
        }
    }
    pub fn digest(&self) -> String {
        let parts: Vec<&str> = self.parts.iter().map(String::as_str).collect();
        effect_key(&self.kind, &parts)
    }
    /// 可复用键（步 19；B20、jev-ca 提醒 1、B151）：去掉第一段（调用位置）后的方法身份与输入，
    /// 给了模型再加模型（生成物：同一提示换模型不是同一件产物）。哪些效应复用、带不带模型由注册表
    /// `EffectSpec.reuse` 定，调用方按它传参（`20` A2：注册表外不按效应名分支）。账本键不变，仍带调用位置。
    pub fn cache_digest(&self, model: Option<&str>) -> Option<String> {
        let rest = self.parts.get(1..)?;
        let mut parts: Vec<&str> = vec!["effect-cache", &self.kind];
        parts.extend(model);
        parts.extend(rest.iter().map(String::as_str));
        Some(hash_of(&parts))
    }
}

/// 一条判断记录对应的校准引用：题声明的校准键。实际命中的记录（题键、题式键、模式键）
/// 与其全文记在账本的 `CalibUsed` 条目（账本 v3，步 18a；此前在头行 `calib_used`），
/// 只凭账本重放时据此补回线（出口 = f(读数, 线)）。结构化的 `CalibKey`（元组）在步 20a-2。
///
/// 留位（账本 v3，步 18a；为空不序列化，18a 不填，填值的步不再改账本格式，ET1）：
/// - `key`、`kind`、`fill`（B124，20a-2 填）：B30 元组序列化的校准主键（B116）；精化题类（B120 (a)，取值为
///   `jpp_ir::question_kind::QuestionKind` 的小写英文名：attr、rel、cmp、class、mention、degree、decide、
///   subset、enough）；填法记录（B107）。
/// - `line`、`hi`、`lo`、`site`（B128、B137，20j-1 填）：线的来源（现只有 `"declared"`：作者在 `cut` 上声明的线）、
///   声明的数、`cut` 站点。与 `kind` 分开记，声明线条目也保留题类（B137 订正 B128 的字面 `kind: "declared"`）。
///   注意：已有字段 `declared` 是题声明的校准键，与 `line = "declared"` 名字相近、含义不同。
///
/// 依据：B124（地基/附注/2026-09-25-待补批量裁定-2.md §四）、B128（地基/附注/2026-09-25-作者主权与策略表达裁定.md）、
/// B137（地基/附注/2026-09-25-库层出口合成与待补批3裁定.md §四）
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CalibRef {
    pub declared: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub key: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fill: Option<Vec<(String, String)>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub line: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hi: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lo: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub site: Option<u64>,
}

impl CalibRef {
    /// 只有声明键的引用（18a 起的写法；留位字段由 20a-2 填）。
    pub fn declared(k: &str) -> CalibRef {
        CalibRef {
            declared: k.to_string(),
            ..CalibRef::default()
        }
    }
}

// ── 效应 id 与效应实例（步 9） ──
//
// 键要带效应实例（`20` §2.3 `jpp-ledger` 键、B60），而 `jpp-ir` 零依赖，所以这两个类型定义在这里，
// `jpp-effects` 重导出。本步只定义，不进 `JudgeKey`/`CacheKey`（那会改序列化）；
// 键里仍是 `model_id` 字符串，换成实例在步 15b/20a。变体名只许在 `jpp-effects/src/kinds/`
// 与内置端口里按名取用（A2，`scripts/grep_effect_names.py`）。

/// 五种效应（`12` §2、§2.8）。新增须改依据文本。
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum EffectId {
    Judge,
    Gen,
    Do,
    Ask,
    Transform,
}

/// 效应实例：同一效应可有多个带画像的实例（B60）。本版每种效应只一个实例，`model` 即客户端的 `model_id`。
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EffectInstance {
    pub effect: EffectId,
    pub model: String,
}

// ── 节点与站点 id（步 12a） ──

/// IR 节点 id：降级时按先序编号，同一程序内唯一（`20` §2.3 `jpp-ir`）。
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct NodeId(pub u32);

/// 站点 id：效应节点、语言形式、内核构造、高阶宿主调用、`if` 各占一个（推测与向量化的触发点）。
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct SiteId(pub u32);

// 步 14a 自 `jpp-calib::calib::record` 原样搬来（运行时写样本要用，运行时不依赖 `jpp-calib`）。
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

// ── 线等级（步 20a-1） ──

/// **线的认证等级**（`20` v2 §3.4 等级表；`附注/2026-09-24-评估①裁定.md` §五 B75 一致表、§十第 12(a) 条）。
///
/// 等级只回答「线是怎么认证的」。「这次使用有没有失去保证」由出口上的正交位回答：
/// `scope_out`（B68）、`suspend_candidate`（B25）、`delta_unknown` / `scope_unknown`（B104）、
/// `untested`（J-15）。正交位可与任何等级叠加；写成等级就丢了「它本来怎么认证的」（补遗 12(a)）。
///
/// 取代 `Exit` 上原来的三个等级位：`fixture_line`（B29）、`class_line`（B75）、`trial_line`（B72）。
/// `Provisional`（B19 修订的临时上岗）在步 20a-1 之前的运行时会放行，与等级表不符（步 20f 登记），
/// 从本步起按等级表不放行。
///
/// 多个条件同时成立时取靠前者：`Cold` > `Fixture` > `Class` > `Trial` > `Provisional` > `Form` >
/// `Certified`。这个优先序步 20f 起已在报告 `exits` 表里用，本步不改。
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum LineGrade {
    /// 没用上线、也没有判断器的回答可走（停岗、缺席、失败、证据不足；作者要求证书线而没有证书；
    /// 非 `max` 统计量或拟合分数上没写声明线）
    Cold,
    /// 夹具线：宿主 `put` 写入，或没有认证证书（B29）
    Fixture,
    /// 类键借来的线（B34、B75）；`class_release` 升格本版未实现
    Class,
    /// 试用 α 认证，或有效 α 超过证书 α（B72、B89）
    Trial,
    /// 临时上岗（B19 修订）
    Provisional,
    /// 题式键，正式 α（B30 主键）
    Form,
    /// 题键，正式 α（B24）
    Certified,
    /// 作者声明线（B128，步 20j-1）：`cut(r, {declare: {hi, lo?}})` 按作者写的数切，不进记录、不产证书。
    /// 路由可用；放行不可逆 `do` 须宿主另作接受（`Exit::host_accepts_declared`，20j-2 置位）。
    /// 与上面七档不在同一条优先序上：它不来自记录，只由声明分支给出。
    Declared,
    /// 判断器自己的回答（意图汇编 11a，2026-09-26）：没有记录的线、作者也没写线时，`cut` 按判断器的回答走——
    /// 是非题 p > 0.5 为 act、p < 0.5 为 ignore，select / measure 取概率最大的候选或档位，恰好并列出
    /// `Unsure(tie)`。与声明线一样不来自记录，不在上面七档的优先序上。不作放行证据：只在宿主开 `--guard` 时有意义。
    Answer,
}

impl LineGrade {
    /// 变体名：报告 `exits` 表的 `grade` 字段，与步 20f 起的字符串逐字相同。
    pub fn name(self) -> &'static str {
        match self {
            LineGrade::Cold => "Cold",
            LineGrade::Fixture => "Fixture",
            LineGrade::Class => "Class",
            LineGrade::Trial => "Trial",
            LineGrade::Provisional => "Provisional",
            LineGrade::Form => "Form",
            LineGrade::Certified => "Certified",
            LineGrade::Declared => "Declared",
            LineGrade::Answer => "Answer",
        }
    }
    /// 等级这一项放不放行不可逆 `do`：只有主键记录（题键、题式键）经正式 α 认证才放行（B75 放行原则）。
    /// 完整判定还要看正交位，只在 `jpp_value::value::Exit::releases` 一处合成。
    pub fn releases(self) -> bool {
        matches!(self, LineGrade::Certified | LineGrade::Form)
    }
}

// ── 追踪上下文（C-2，账本 v4 的行外壳字段 `trace`） ──
//
// 依据：`地基/规划/骨架候选.md` 2.0.2 C-2；研究 15 的 OpenTelemetry：一条跨程序调用链共用一个追踪编号，每一段
// （一趟程序运行）有自己的段编号与父段编号，事后按父子关系拼成树。与 OTel 的不同处：编号由哈希推导、不用随机数，
// 同一条链重跑、审计重放得到同样的编号，账本才能逐字节重放（`21` 步 0 门禁）。追踪上下文只装身份；预算与截止时间
// 是另一件事（C-3，研究 15 的 Baggage 与 Trace Context 分开传），不放进这里。
// 命名：`jpp_ledger::Trace` / `TraceEvent` 已是「执行轨迹」，这里一律带 `Ctx`。

/// 取 `parts`（以 `\x1f` 分隔）的 SHA-256 前 `nbytes` 字节的十六进制。
fn hash_hex(parts: &[&str], nbytes: usize) -> String {
    let mut h = Sha256::new();
    for p in parts {
        h.update(p.as_bytes());
        h.update(b"\x1f");
    }
    hex::encode(&h.finalize()[..nbytes])
}

/// 编号必须是给定长度的小写十六进制且不全为零（W3C Trace Context 的要求）；不合格就拒收，不兜底。
fn check_hex_id(what: &str, s: &str, len: usize) -> Result<(), String> {
    if s.len() != len
        || !s
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return Err(format!(
            "E-trace-id: {what}要 {len} 位小写十六进制，收到 {s:?}"
        ));
    }
    if s.bytes().all(|b| b == b'0') {
        return Err(format!("E-trace-id: {what}不能全为零"));
    }
    Ok(())
}

/// 追踪编号：一整条调用链共用一个；32 位小写十六进制（与 W3C `traceparent` 的 trace-id 同形）。
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct TraceId(String);

/// 段编号：一趟程序运行一个；16 位小写十六进制（与 `traceparent` 的 parent-id 同形）。
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct SpanId(String);

macro_rules! id_impl {
    ($t:ident, $what:literal, $len:literal) => {
        impl $t {
            pub fn parse(s: &str) -> Result<$t, String> {
                check_hex_id($what, s, $len)?;
                Ok($t(s.to_string()))
            }
            pub fn as_str(&self) -> &str {
                &self.0
            }
        }
        impl TryFrom<String> for $t {
            type Error = String;
            fn try_from(s: String) -> Result<$t, String> {
                check_hex_id($what, &s, $len)?;
                Ok($t(s))
            }
        }
        impl From<$t> for String {
            fn from(v: $t) -> String {
                v.0
            }
        }
        impl std::fmt::Display for $t {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str(&self.0)
            }
        }
    };
}
id_impl!(TraceId, "追踪编号", 32);
id_impl!(SpanId, "段编号", 16);

/// 一段的追踪上下文：`trace` 整条链共用，`span` 是本段，`parent` 是调起本段的那一段（链的起点没有）。
/// 账本行外壳的 `trace` 字段就是它（键序 `trace`、`span`、`parent`，没有父段不写 `parent`）。
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TraceCtx {
    pub trace: TraceId,
    pub span: SpanId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent: Option<SpanId>,
}

impl TraceCtx {
    /// 链的起点：追踪编号由 `seed` 推导，本段（根段）由（追踪编号、`label`）推导，没有父段。
    /// 同样的 `seed` 与 `label` 永远得到同样的上下文。
    pub fn start(seed: &str, label: &str) -> TraceCtx {
        let trace = hash_hex(&["jpp-trace", seed], 16);
        let span = hash_hex(&["jpp-span", &trace, "", label], 8);
        TraceCtx {
            trace: TraceId(trace),
            span: SpanId(span),
            parent: None,
        }
    }

    /// 从调用者（`self`）的上下文推导被调用段：同一追踪编号，父段 = 调用者的段，本段由
    /// （追踪编号、调用者的段、`label`）推导。同一调用者、同一 `label` 得到同一段；要分成两个节点就换 `label`。
    pub fn enter(&self, label: &str) -> TraceCtx {
        let span = hash_hex(
            &["jpp-span", self.trace.as_str(), self.span.as_str(), label],
            8,
        );
        TraceCtx {
            trace: self.trace.clone(),
            span: SpanId(span),
            parent: Some(self.span.clone()),
        }
    }

    /// 传给被调用程序的文本形式（W3C `traceparent`：`00-<追踪编号>-<段编号>-01`）。
    /// 传的是**调用者**的段：被调用方收到后用 [`TraceCtx::enter`] 推导自己的段。
    pub fn to_traceparent(&self) -> String {
        format!("00-{}-{}-01", self.trace, self.span)
    }

    /// 读 `traceparent`：版本要是 `00`，编号长度与字符合格且不全为零，标志两位十六进制。
    /// 返回的上下文是**调用者**的（没有父段：调用者自己的父段 `traceparent` 里没有）。
    pub fn from_traceparent(s: &str) -> Result<TraceCtx, String> {
        let parts: Vec<&str> = s.split('-').collect();
        let [ver, trace, span, flags] = parts.as_slice() else {
            return Err(format!(
                "E-trace-id: traceparent 要四段（00-追踪编号-段编号-标志），收到 {s:?}"
            ));
        };
        if *ver != "00" {
            return Err(format!("E-trace-id: traceparent 版本要 00，收到 {ver:?}"));
        }
        if flags.len() != 2 || !flags.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err(format!(
                "E-trace-id: traceparent 标志要两位十六进制，收到 {flags:?}"
            ));
        }
        Ok(TraceCtx {
            trace: TraceId::parse(trace)?,
            span: SpanId::parse(span)?,
            parent: None,
        })
    }
}

#[cfg(test)]
mod cache_key_tests {
    //! 步 19（B40、B20）：缓存键不含调用位置；选择题按 `perm_seed` 分开；`do` 不复用。
    use super::*;

    #[test]
    fn 非选择题的摘要与步7相同_选择题按seed分开() {
        let a = JudgeKey::new("m", "s", "q", "noul", 1, 0, 10).cache_key();
        let b = JudgeKey::new("m", "s", "q", "noul", 2, 5, 99).cache_key();
        assert_eq!(
            a.digest(),
            b.digest(),
            "是非题不看 seed、调用位置、运行序号"
        );
        assert_eq!(
            a.digest(),
            hash_of(&["cache", "m", "s", "q", "noul", RENDER_VERSION]),
            "非选择题的摘要与步 7 的定义相同"
        );
        let c1 = JudgeKey::new("m", "s", "q", "choice", 1, 0, 10).cache_key();
        let c1b = JudgeKey::new("m", "s", "q", "choice", 1, 0, 99).cache_key();
        let c2 = JudgeKey::new("m", "s", "q", "choice", 2, 0, 10).cache_key();
        assert_eq!(c1.digest(), c1b.digest(), "同 seed 复用");
        assert_ne!(c1.digest(), c2.digest(), "选择题 seed 不同不复用");
    }

    #[test]
    fn 效应缓存键去掉调用位置_给模型才带模型() {
        let g1 = EffectKey::new("gen", &["10", "提示", "h1", "3", "0"]);
        let g2 = EffectKey::new("gen", &["99", "提示", "h1", "3", "0"]);
        assert_eq!(g1.cache_digest(Some("甲")), g2.cache_digest(Some("甲")));
        assert_ne!(g1.cache_digest(Some("甲")), g1.cache_digest(Some("乙")));
        assert_ne!(g1.digest(), g2.digest(), "账本键仍带调用位置");
        let t1 = EffectKey::new("transform", &["10", "f", "cap", "a"]);
        let t2 = EffectKey::new("transform", &["20", "f", "cap", "a"]);
        assert_eq!(
            t1.cache_digest(None),
            t2.cache_digest(None),
            "不给模型：只看方法身份与输入"
        );
    }
}
