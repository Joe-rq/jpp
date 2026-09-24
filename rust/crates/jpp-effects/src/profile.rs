//! 能力画像（档案）与档案哈希。（步 4c 自 `effects.rs` 原样搬出；步 9 自 `jpp-core::effects::profile` 原样搬入，
//! 字段暂与现状一致，按效应实例分表在步 15d。`calib_hash` 依赖校准库，留在 `jpp-core`，随步 11 进 `jpp-calib`。）

use serde::{Deserialize, Serialize};
use serde_json::Value as Json;

use jpp_value::value::canon;

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
    /// **每次判断调用的 p95 时延（秒）**（`concurrency.latency_s.p95`），B32 静态时延估计用。
    /// `None` = 档案没给或没加载档案：估计不了，检查报 `W-untested`。
    pub latency_p95: Option<f64>,
    /// **每个 input token 的价格（美元）**（`cost.price_usd_per_input_token`；B73、`19` 冲突 #25）。
    /// 价格只住画像，代码里不写死。`None` = 画像没给或没加载画像：费用记 `Unknown`、报 `W-cost-unknown`。
    pub price_per_input_token: Option<f64>,
}

impl Default for Profile {
    /// 代码兜底值。**它不是一份档案**，所以 `hash` 是 `None`——这是它与「加载到的档案」
    /// 唯一的区别，也是账本头上看得出来的那个区别。
    /// 兜底值本身合法（测试与不接档案的调用方要用），**不合法的是 `load` 悄悄回退到它**。
    /// 取值与 Python 内核同名兜底一致（`runtime.py` 的 `delta_for` / `safety_lines` 的 except 分支）。
    fn default() -> Profile {
        Profile {
            safety: (1.0, 0.0),
            delta: (0.05, 0.15, 0.15),
            text_window: 1000,
            json_ctx_window: 1800,
            behavior_hash: None,
            hash: None,
            arithmetic_capable: Tri::未测,
            latency_p95: None,
            price_per_input_token: None,
        }
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
        let text = std::fs::read_to_string(path)
            .map_err(|e| format!("读不到档案 {}：{e}", path.display()))?;
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
        let dn =
            取(&["delta", "noul", "immediate", "p99"]).ok_or("档案缺 delta.noul.immediate.p99")?;
        // choice 的 δ 取「被选中那档的概率」那一列（与 Python `delta_for` 的映射一致）
        let dc = 取(&["delta", "choice_prob_chosen", "immediate", "p99"])
            .ok_or("档案缺 delta.choice_prob_chosen.immediate.p99")?;
        let ds = 取(&["delta", "score", "immediate", "p99"])
            .ok_or("档案缺 delta.score.immediate.p99")?;
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
            .and_then(|m| {
                m.keys()
                    .filter_map(|k| k.trim_start_matches('~').parse::<usize>().ok())
                    .max()
            })
            .ok_or("档案缺 window.json_slots.claim_bearing_ctx.flip_frac_by_ctx_tokens")?;
        Ok(Profile {
            safety: (hi, lo),
            delta: (dn, dc, ds),
            text_window,
            json_ctx_window,
            behavior_hash: Some(behavior_hash(j)),
            hash: Some(profile_hash(j)),
            arithmetic_capable: Tri::from_json(j, "arithmetic_capable"),
            latency_p95: j
                .get("concurrency")
                .and_then(|c| c.get("latency_s"))
                .and_then(|l| l.get("p95"))
                .and_then(|x| x.as_f64()),
            price_per_input_token: j
                .get("cost")
                .and_then(|c| c.get("price_usd_per_input_token"))
                .and_then(|x| x.as_f64()),
        })
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
    const 行为字段: &[&str] = &[
        "lines",
        "delta",
        "window",
        "k_limit",
        "position_bias",
        "anchors",
        "concurrency",
        "cost",
        "select_sums_to_one",
        "fixed_output_types",
    ];
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
    const 说明字段: &[&str] = &[
        "note", "source", "by", "reason", "status", "notes", "comment",
    ];
    match j {
        Json::Object(m) => Json::Object(
            m.iter()
                .filter(|(k, _)| !说明字段.contains(&k.as_str()))
                .map(|(k, v)| (k.clone(), strip_prose(v)))
                .collect(),
        ),
        Json::Array(a) => Json::Array(a.iter().map(strip_prose).collect()),
        other => other.clone(),
    }
}

/// 规范 JSON 的 sha256 前 16 个十六进制字符（校准记录、证书地址也用它）。
pub fn hash16(j: &Json) -> String {
    use sha2::{Digest, Sha256};
    let mut h = Sha256::new();
    h.update(canon(j).as_bytes());
    h.finalize()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect::<String>()[..16]
        .to_string()
}

/// 与 Python 的 `H(profile)` 同值：`sha256(canon([profile]))` 取前 16 个十六进制字符。
/// **必须同值**——它进账本头，两边算不出同一个数，跨内核的重放判定就对不上。
pub fn profile_hash(j: &Json) -> String {
    use sha2::{Digest, Sha256};
    let wrapped = Json::Array(vec![j.clone()]);
    let mut h = Sha256::new();
    h.update(canon(&wrapped).as_bytes());
    h.finalize()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect::<String>()[..16]
        .to_string()
}
