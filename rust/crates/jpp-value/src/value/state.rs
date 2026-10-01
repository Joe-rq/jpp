//! 状态（`State`）。步 36 G3 从 `value.rs` 原样搬出（只搬不改）。

use super::*;

/// 状态 = 具名槽（§2.1）。`on` 恰一个对象（或一对），`over` 是候选。
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct State {
    pub on: Vec<Mat>,
    pub ctx: Vec<Mat>,
    pub r#ref: Vec<Mat>,
    pub over: Vec<Mat>,
    pub taint: Taint,
    pub derived_from: BTreeSet<String>,
    /// 含槽结构的规范化 JSON 的哈希（§2.10）
    pub hash: String,
    pub has_fail: bool,
    /// 来源读数的账本键（B59，步 17a）：各槽材料 `from_key` 的并，加上由元素记录构造状态时的元素出口。
    /// **不进 `hash`**（`StateHash` 不变，旧账本重放不受影响），空集不序列化。
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub parents: BTreeSet<String>,
}

impl State {
    pub fn new(
        on: Vec<Mat>,
        ctx: Vec<Mat>,
        r#ref: Vec<Mat>,
        over: Vec<Mat>,
        has_fail: bool,
    ) -> State {
        let all = on.iter().chain(&ctx).chain(&r#ref).chain(&over);
        let mut taint = Taint::Trusted;
        let mut derived = BTreeSet::new();
        let mut parents = BTreeSet::new();
        for m in all {
            taint = Taint::join(taint, m.taint);
            derived.extend(m.derived_from.iter().cloned());
            parents.extend(m.from_key.iter().cloned());
        }
        let mut s = State {
            on,
            ctx,
            r#ref,
            over,
            taint,
            derived_from: derived,
            hash: String::new(),
            has_fail,
            parents,
        };
        s.hash = hash_of(&["state", &canon(&s.to_json())]);
        s
    }
    /// 并入来源读数的账本键（B59）。空键不记。不改哈希。
    pub fn with_parents<I: IntoIterator<Item = String>>(mut self, keys: I) -> State {
        self.parents
            .extend(keys.into_iter().filter(|k| !k.is_empty()));
        self
    }
    /// 状态的槽形（B76，步 12e-2），供精化题类。`over` 全是字面材料 → 标签，否则 → 计算材料；
    /// 题值渲染成材料（B4）今天没有构造，`question_material` 恒假。
    pub fn slot_shape(&self) -> SlotShape {
        SlotShape {
            on: if self.on.len() == 2 {
                OnShape::Pair
            } else {
                OnShape::One
            },
            over: if self.over.is_empty() {
                OverShape::Empty
            } else if self
                .over
                .iter()
                .all(|m| m.origin.iter().all(|o| o == "literal"))
            {
                OverShape::Labels
            } else {
                OverShape::Materials
            },
            question_material: false,
        }
    }
    /// **夹具形状**的状态 JSON：`on` 恒为列表。
    ///
    /// 与 [`State::to_json`] 的差别只有一处：那个在 `on` 只有一个对象时**摊成对象**
    /// （因为送给模型时那样更自然），**而夹具只收列表**
    /// （`invalid type: map, expected a sequence`）。
    /// 未命中报文说「照抄这两份 JSON」，**打 `to_json` 那句话就是假的**。
    pub fn as_fixture_json(&self) -> Json {
        let m = |v: &Vec<Mat>| Json::Array(v.iter().map(|x| x.content.clone()).collect());
        let mut o = serde_json::Map::new();
        o.insert("on".into(), m(&self.on));
        for (k, v) in [
            ("ctx", &self.ctx),
            ("ref", &self.r#ref),
            ("over", &self.over),
        ] {
            if !v.is_empty() {
                o.insert(k.into(), m(v));
            }
        }
        Json::Object(o)
    }

    /// 被判断对象（`on` 槽）的文本，供认证范围的材料指纹用（B68）：字符串材料取原文，
    /// 其余取紧凑 JSON；多份材料以换行相连。
    pub fn on_text(&self) -> String {
        self.on
            .iter()
            .map(|m| match &m.content {
                Json::String(t) => t.clone(),
                other => other.to_string(),
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// 线上的状态 JSON（B155，步 15i）：与 [`State::to_json`] 同形，只是不含 `over`——候选随题走，
    /// 只作该 `select` 题的 `criteria` 发出；状态只发材料 `on`/`ctx`/`ref`。键与哈希仍取 `to_json`。
    /// 依据：B155（地基/附注/2026-09-26-批6裁定.md §三）
    pub fn wire_json(&self) -> Json {
        let mut j = self.to_json();
        if let Json::Object(o) = &mut j {
            o.remove("over");
        }
        j
    }

    /// 候选标签（B155，步 15i，渲染 `r2` 的一部分）：`over` 里全是恰好 `{label: Text, text: Text}` 两键的
    /// 记录、且标签两两不同时，返回 `(label, text)` 表——线上 `criteria` 键取 `label`、值取 `text`；
    /// 否则 `None`，键用 `c{k}`、值取原内容。标签键的对象按键名排序发出，候选顺序由标签定，换不了序。
    pub fn over_labels(&self) -> Option<Vec<(String, Json)>> {
        if self.over.is_empty() {
            return None;
        }
        let mut out: Vec<(String, Json)> = vec![];
        for m in &self.over {
            let o = m.content.as_object()?;
            let (l, t) = (o.get("label")?.as_str()?, o.get("text")?);
            if o.len() != 2 || !t.is_string() || out.iter().any(|(k, _)| k == l) {
                return None;
            }
            out.push((l.to_string(), t.clone()));
        }
        Some(out)
    }

    /// 材料哈希（B155，步 15i）：`hash(on, ctx, ref)`。刷新与提升按它分组，同材料上候选集不同的题
    /// 合成一次调用；`StateHash`（`hash`，含 `over`）仍是账本键与缓存键的状态分量。
    pub fn mat_hash(&self) -> String {
        hash_of(&["mat", &canon(&self.wire_json())])
    }

    /// 送给模型的状态 JSON（H1：题面按路径引用槽）。B155 起线上改发 [`State::wire_json`]，
    /// 这里仍是状态哈希与夹具观察键的来源。
    pub fn to_json(&self) -> Json {
        let m = |v: &Vec<Mat>| Json::Array(v.iter().map(|x| x.content.clone()).collect());
        let mut o = serde_json::Map::new();
        o.insert(
            "on".into(),
            if self.on.len() == 1 {
                self.on[0].content.clone()
            } else {
                m(&self.on)
            },
        );
        if !self.ctx.is_empty() {
            o.insert("ctx".into(), m(&self.ctx));
        }
        if !self.r#ref.is_empty() {
            o.insert("ref".into(), m(&self.r#ref));
        }
        if !self.over.is_empty() {
            o.insert("over".into(), m(&self.over));
        }
        Json::Object(o)
    }
}
