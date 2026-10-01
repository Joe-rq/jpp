//! 材料与 taint（`Taint`、`Mat`）。步 36 G3 从 `value.rs` 原样搬出（只搬不改）。

use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum Taint {
    Trusted,
    // 反序列化缺字段时按保守那边兜底（与 json_to_effect_value 同一条判据）
    #[default]
    Untrusted,
}

impl Taint {
    pub fn join(a: Taint, b: Taint) -> Taint {
        if a == Taint::Untrusted || b == Taint::Untrusted {
            Taint::Untrusted
        } else {
            Taint::Trusted
        }
    }
}

/// 唯一材料类型（§2）。外部 crate 只能经 `Mat::new` / `Mat::literal` 构造（`#[non_exhaustive]`）；
/// 反序列化经 `raw::MatRaw` 重新走 `Mat::new`（I6）。
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "crate::raw::MatRaw")]
#[non_exhaustive]
pub struct Mat {
    pub content: Json,
    pub addr: String,
    pub modality: String,
    /// 来源链：literal / gen:<key> / do:<key> / ask:<key> / transform:<key> / exit:<q>
    pub origin: Vec<String>,
    pub taint: Taint,
    /// 由哪些题派生（J-02 禁自指）
    pub derived_from: BTreeSet<String>,
    pub hash: String,
    /// 来源出口的账本键（B59，步 17a）：这份材料由哪些判断的出口选出或转成。**不进 `hash`**，
    /// 空集不序列化。本版只在结构通道上打（出口转材料、元素记录转材料、效应输出承接输入），
    /// 经普通值（`content()` 读出、`e.item` 取出）的依赖不计，待候选 B84（值级来源）。
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub from_key: BTreeSet<String>,
    /// `from_key` 里的值依赖边：键 → 题哈希（B92，步 18c）。不在这里的键是选择依赖边。
    /// 并入值依赖边时其题哈希同步并进 `derived_from`，所以 `derived_from` 就是 J-02 的投影
    /// （∪ 从账本解码的效应输出自带的 `derived_from`）。不进 `hash`，空不序列化。
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub value_q: BTreeMap<String, String>,
}

impl Mat {
    pub fn new(
        content: Json,
        addr: &str,
        origin: Vec<String>,
        taint: Taint,
        derived_from: BTreeSet<String>,
    ) -> Mat {
        let hash = hash_of(&["mat", &canon(&content), addr]);
        Mat {
            content,
            addr: addr.to_string(),
            modality: "text".into(),
            origin,
            taint,
            derived_from,
            hash,
            from_key: BTreeSet::new(),
            value_q: BTreeMap::new(),
        }
    }
    /// 材料的来源标签（B84）：`(taint, from_key)`。两个字段分存（报告 JSON 与 `.taint` 读取不变），
    /// 传播只经这里与 [`Value::with_prov`]。
    pub fn prov(&self) -> Provenance {
        let map = self
            .from_key
            .iter()
            .map(|k| {
                let e = match self.value_q.get(k) {
                    Some(q) => Edge {
                        kind: EdgeKind::Value,
                        q: q.clone(),
                    },
                    None => Edge {
                        kind: EdgeKind::Select,
                        q: String::new(),
                    },
                };
                (k.clone(), e)
            })
            .collect();
        Provenance::new(self.taint, Sources::from_map(map))
    }
    /// 并入来源出口的账本键（B59），作选择依赖边（只有键）。空键不记。不改哈希。
    pub fn with_from_key<I: IntoIterator<Item = String>>(mut self, keys: I) -> Mat {
        self.from_key
            .extend(keys.into_iter().filter(|k| !k.is_empty()));
        self
    }
    /// 并入来源边（B92，步 18c）：键进 `from_key`；值依赖边另记题哈希并进 `derived_from`
    /// （J-02 的投影）。同键已有值边不降为选择边。不改哈希。
    pub fn with_sources(mut self, s: &Sources) -> Mat {
        for (k, e) in s.edges() {
            if k.is_empty() {
                continue;
            }
            self.from_key.insert(k.clone());
            if e.kind == EdgeKind::Value {
                self.value_q.insert(k.clone(), e.q.clone());
                if !e.q.is_empty() {
                    self.derived_from.insert(e.q.clone());
                }
            }
        }
        self
    }
    /// 换内容、保来源（步 23b 裂变的块）：地址、来源链、taint、`derived_from`、来源边都照原材料，哈希按新内容重算。
    pub fn with_content(&self, content: Json) -> Mat {
        let mut m = self.clone();
        m.hash = hash_of(&["mat", &canon(&content), &m.addr]);
        m.content = content;
        m
    }
    pub fn literal(content: Json) -> Mat {
        Mat::new(
            content,
            "",
            vec!["literal".into()],
            Taint::Trusted,
            BTreeSet::new(),
        )
    }
    /// 估算 token。与 Python 的 `ir.py:84` **同一个估法**（`int(len(canon)/1.3) + 1`，
    /// 依据是档案 `cost.regression` 的 state_char_coef≈1、1.3 字符/token）——
    /// 两边估法不同，窗口检查就会在不同的地方触发。
    ///
    /// 窗口检查（J-14 / `12`:117）要它：对象槽内单段按 `window.text_slots…usable_lower`，
    /// 槽间按 `window.json_slots`。超窗的后果不是算错，是**读数被语境接管而无人察觉**
    /// ——档案原话「≈1,000 token 带主张语境下翻转 60.7%，读数被语境接管」。
    pub fn tokens(&self) -> usize {
        (canon(&self.content).chars().count() as f64 / 1.3) as usize + 1
    }
    pub fn text(&self) -> String {
        match &self.content {
            Json::String(s) => s.clone(),
            other => other.to_string(),
        }
    }
}
