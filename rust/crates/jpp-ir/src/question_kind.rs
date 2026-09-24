//! 题类推断（B76）：题类由 `question_kind(op, request, 状态槽形, 题式槽声明)` 推出，这里是唯一推断处。
//!
//! 放 `jpp-ir` 是因为检查器与运行时都要调它，而 `jpp-check` 不能依赖 `jpp-value`（`20` §2.2 第 4 条）。
//! 题类是 `op`、`request`、槽形、槽声明的函数，不携带新信息，所以不进 `q_hash`、`form_hash` 与账本。
//! 基础类（不依赖状态）在 [`SlotShape::UNKNOWN`] 上算；精化类（状态槽形可见时）用实际槽形算，
//! 两者不一致时以精化类为准，都留在报告（B76 (4)）。
//!
//! 映射表（`附注/2026-09-24-评估①裁定.md` §六「B76」）：
//!
//! | `op` | `request` | 槽形 | 声明 | 题类 |
//! |---|---|---|---|---|
//! | test | whether | `on` 单对象 | 无 | 属性 |
//! | test | whether | `on` 单对象 | `accepts`（B23） | 提及 |
//! | test | whether | `on` 一对 | — | 关系 |
//! | test | whether | `ctx`/`ref` 含由题值渲染的材料（B4） | — | 充分性 |
//! | test / select | all | — | — | 子集 |
//! | select | one | `over` 字面文本 | `over_kind = labels`（缺省） | 归类 |
//! | select | one | `over` 计算出的材料 | `over_kind = candidates` | 比较 |
//! | select | one | `over` 题值 | `over_kind ∈ {questions, actions}` | 决定 |
//! | measure | degree | — | — | 程度 |
//!
//! 不唯一的格由题式槽声明定，缺声明按缺省并记 [`KindSource::Inferred`]；声明与可见结构矛盾时
//! [`kind_conflict`] 给出理由，检查器报 `E-kind-conflict`。

use serde::{Deserialize, Serialize};

use crate::key::Op;

/// 九题类（`11` §2 七种加 `DECISIONS.md` 升格的子集、充分性）。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum QuestionKind {
    /// 属性：单对象是否具有某性质
    Attr,
    /// 关系：一对对象之间是否成立某关系
    Rel,
    /// 比较：在计算出的候选材料里挑
    Cmp,
    /// 归类：在程序里写定的标签里挑
    Class,
    /// 提及：材料的文字里是否出现某实体（由题式的 `accepts` 声明区分，B23）
    Mention,
    /// 程度：有序分档
    Degree,
    /// 决定：在题或动作之间挑下一步
    Decide,
    /// 子集：属性在集合上的向量化（`request = all`）
    Subset,
    /// 充分性：材料够不够回答另一道题（题面作材料，B4）
    Enough,
}

impl QuestionKind {
    pub const ALL: [QuestionKind; 9] = [
        QuestionKind::Attr,
        QuestionKind::Rel,
        QuestionKind::Cmp,
        QuestionKind::Class,
        QuestionKind::Mention,
        QuestionKind::Degree,
        QuestionKind::Decide,
        QuestionKind::Subset,
        QuestionKind::Enough,
    ];

    /// 术语表中文名
    pub fn label(self) -> &'static str {
        match self {
            QuestionKind::Attr => "属性",
            QuestionKind::Rel => "关系",
            QuestionKind::Cmp => "比较",
            QuestionKind::Class => "归类",
            QuestionKind::Mention => "提及",
            QuestionKind::Degree => "程度",
            QuestionKind::Decide => "决定",
            QuestionKind::Subset => "子集",
            QuestionKind::Enough => "充分性",
        }
    }
}

/// 题类从哪里来：由结构或缺省推出，还是由题式槽声明定下。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum KindSource {
    Inferred,
    Declared,
}

/// 题的请求（B1 题的五件之一）。缺省由 `op` 定，与 `jpp-value::default_request` 同表。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Request {
    Whether,
    One,
    Degree,
    All,
}

impl Request {
    pub fn parse(s: &str) -> Option<Request> {
        match s {
            "whether" => Some(Request::Whether),
            "one" => Some(Request::One),
            "degree" => Some(Request::Degree),
            "all" => Some(Request::All),
            _ => None,
        }
    }
    pub fn default_for(op: Op) -> Request {
        match op {
            Op::Test => Request::Whether,
            Op::Select => Request::One,
            Op::Measure => Request::Degree,
        }
    }
}

/// `on` 槽的形状。`Unknown`：看不见（基础类，或静态上不确定）。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OnShape {
    Unknown,
    One,
    Pair,
}

/// `over` 槽的形状。`Unknown`：看不见，或形状不确定（混合、名字引用）。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OverShape {
    Unknown,
    /// 没有 `over`
    Empty,
    /// 程序里写定的字面文本
    Labels,
    /// 计算出的材料
    Materials,
    /// 题值
    Questions,
}

/// 判断时状态的槽形。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SlotShape {
    pub on: OnShape,
    pub over: OverShape,
    /// `ctx` 或 `ref` 含由题值渲染的材料（B4，材料 origin = question）
    pub question_material: bool,
}

impl SlotShape {
    /// 状态不可见：用它算出的是基础类。
    pub const UNKNOWN: SlotShape = SlotShape {
        on: OnShape::Unknown,
        over: OverShape::Unknown,
        question_material: false,
    };
}

/// 题式对 `over` 的声明（`Form.over_kind`，B76）。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum OverKind {
    Labels,
    Candidates,
    Questions,
    Actions,
}

impl OverKind {
    pub fn parse(s: &str) -> Option<OverKind> {
        match s {
            "labels" => Some(OverKind::Labels),
            "candidates" => Some(OverKind::Candidates),
            "questions" => Some(OverKind::Questions),
            "actions" => Some(OverKind::Actions),
            _ => None,
        }
    }
    pub fn name(self) -> &'static str {
        match self {
            OverKind::Labels => "labels",
            OverKind::Candidates => "candidates",
            OverKind::Questions => "questions",
            OverKind::Actions => "actions",
        }
    }
}

/// 实体槽接受的填法类型（B23；步 8 的 `Form.slots.accepts` 落地前没有载体）。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Accepts {
    Concrete,
    Category,
    Abstract,
}

/// 题式槽声明。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SlotDecls {
    pub over_kind: Option<OverKind>,
    pub accepts: Option<Accepts>,
}

/// 声明与可见结构矛盾（`E-kind-conflict`）。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KindConflict {
    pub reason: String,
}

/// 题类推断（B76）。先按请求，再按声明，最后按结构与缺省。
pub fn question_kind(
    op: Op,
    request: Option<Request>,
    shape: &SlotShape,
    decl: &SlotDecls,
) -> (QuestionKind, KindSource) {
    use QuestionKind as K;
    let request = request.unwrap_or(Request::default_for(op));
    if request == Request::All && op != Op::Measure {
        return (K::Subset, KindSource::Inferred);
    }
    match op {
        Op::Measure => (K::Degree, KindSource::Inferred),
        Op::Test => {
            if shape.on == OnShape::Pair {
                (K::Rel, KindSource::Inferred)
            } else if shape.question_material {
                (K::Enough, KindSource::Inferred)
            } else if decl.accepts.is_some() {
                (K::Mention, KindSource::Declared)
            } else {
                (K::Attr, KindSource::Inferred)
            }
        }
        Op::Select => match (decl.over_kind, shape.over) {
            (Some(OverKind::Labels), _) => (K::Class, KindSource::Declared),
            (Some(OverKind::Candidates), _) => (K::Cmp, KindSource::Declared),
            (Some(OverKind::Questions | OverKind::Actions), _) => (K::Decide, KindSource::Declared),
            (None, OverShape::Materials) => (K::Cmp, KindSource::Inferred),
            (None, OverShape::Questions) => (K::Decide, KindSource::Inferred),
            (None, _) => (K::Class, KindSource::Inferred),
        },
    }
}

/// 声明与可见结构是否矛盾。结构看不见（`Unknown`）时不算矛盾：宁可漏报。
pub fn kind_conflict(op: Op, shape: &SlotShape, decl: &SlotDecls) -> Option<KindConflict> {
    let conflict = |reason: String| Some(KindConflict { reason });
    if let Some(ok) = decl.over_kind {
        if op != Op::Select {
            return conflict(format!(
                "over_kind = {} 只用于 select 题式，这里是 {}",
                ok.name(),
                op.fixture_name()
            ));
        }
        let fits = match shape.over {
            OverShape::Unknown | OverShape::Empty => true,
            OverShape::Labels => matches!(ok, OverKind::Labels | OverKind::Actions),
            OverShape::Materials => ok == OverKind::Candidates,
            OverShape::Questions => ok == OverKind::Questions,
        };
        if !fits {
            let seen = match shape.over {
                OverShape::Labels => "字面文本",
                OverShape::Materials => "计算出的材料",
                OverShape::Questions => "题值",
                OverShape::Unknown | OverShape::Empty => "",
            };
            return conflict(format!(
                "题式声明 over_kind = {}，状态的 over 却是{seen}",
                ok.name()
            ));
        }
    }
    if decl.accepts.is_some() && op == Op::Test && shape.on == OnShape::Pair {
        return conflict("题式声明了实体槽 accepts（提及类，单对象），状态的 on 却是一对".into());
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 基础类按op缺省() {
        let d = SlotDecls::default();
        let u = SlotShape::UNKNOWN;
        assert_eq!(question_kind(Op::Test, None, &u, &d).0, QuestionKind::Attr);
        assert_eq!(
            question_kind(Op::Select, None, &u, &d).0,
            QuestionKind::Class
        );
        assert_eq!(
            question_kind(Op::Measure, None, &u, &d).0,
            QuestionKind::Degree
        );
    }

    #[test]
    fn 缺省请求与值模型同表() {
        assert_eq!(
            Request::default_for(Op::Test),
            Request::parse("whether").unwrap()
        );
        assert_eq!(
            Request::default_for(Op::Select),
            Request::parse("one").unwrap()
        );
        assert_eq!(
            Request::default_for(Op::Measure),
            Request::parse("degree").unwrap()
        );
    }
}
