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

pub const RENDER_VERSION: &str = "r1";

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
    /// 去掉调用位置与运行序号后的缓存键（`12` §2.10、B40）。步 19 启用；步 7 只定义。
    pub fn cache_key(&self) -> CacheKey {
        CacheKey {
            model_id: self.model_id.clone(),
            state: self.state.clone(),
            q: self.q.clone(),
            phys: self.phys.clone(),
            render: self.render.clone(),
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
}

impl CacheKey {
    pub fn digest(&self) -> String {
        hash_of(&[
            "cache",
            &self.model_id,
            &self.state,
            &self.q,
            &self.phys,
            &self.render,
        ])
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
}

/// 一条判断记录对应的校准引用：题声明的校准键。实际命中的记录（题键、题式键、模式键）
/// 与其全文记在账本头行的 `calib_used`，只凭账本重放时据此补回线（出口 = f(读数, 线)）。
/// 结构化的 `CalibKey`（元组）在步 20a。
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CalibRef {
    pub declared: String,
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
