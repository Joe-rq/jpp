//! 题与题式（`Question`、`FissionDecl`、`TestLabels`、`Form`、`FormSig`）。步 36 G3 从 `value.rs` 原样搬出（只搬不改）。

use super::*;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct Question {
    pub op: Op,
    pub text: String,
    /// 校准键；线只从校准记录来（I4 / J-03）
    pub calib: String,
    /// measure 的档位
    pub scale: Vec<String>,
    /// J-09 的**决定性证据槽**名（`on` / `ctx` / `ref` / `over`）。
    /// `cut` 判序第一步：这些槽不在状态里就**不信任 p**（`12`:148、:244）。
    #[serde(default)]
    pub evidence: Vec<String>,
    pub hash: String,
    /// B1 的「前提」：题预设为真的命题（Belnap & Steel 1976）。**只作声明**：
    /// 本版不发给判断器、不进题哈希、不改 `cut`；前提不成立的出口位置（insufficient）是 B4 的事。
    #[serde(default)]
    pub presupposition: Option<String>,
    /// B1 的「请求」：对 K 元划分，要选一个还是全部。`None` = 按题型取缺省（见 `request()`）。
    #[serde(default)]
    pub request: Option<String>,
    /// 来自哪个题式（题式哈希）。**不进题哈希**：同题面同题型就是同一道题，
    /// 无论它是手写的还是由题式填出来的。校准键暂不改（B2 待裁），这里只留痕以便日后切换。
    #[serde(default)]
    pub form_hash: Option<String>,
    /// 题式的模板题面（带 `{槽}`）
    #[serde(default)]
    pub template: Option<String>,
    /// 填法：槽名 → 填入的文本
    #[serde(default)]
    pub fill: Option<Vec<(String, String)>>,
    /// 派生题的来源出口账本键（B59，步 17a）。**不进题哈希**，空集不序列化。
    /// 本步只落消费一侧（判断时并入 `parents`）；没有结构通道产生它（`select` 的 `pick` 臂只交出下标），
    /// 生产者是 B45 派生（步 28）与候选 B84。
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub from_key: BTreeSet<String>,
    /// 题式对 `over` 的声明（B76，步 12e-2），由 [`Form::fill`] 带过来。不进题哈希，空不序列化。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub over_kind: Option<OverKind>,
    /// 置换声明（B64，步 15f）：`select` 站点要求判断器按正逆两序各读一次，`cut` 据置换众数一致给
    /// `Pick`。测量声明，**不进题哈希**（换不换置换是同一道题、同一条校准键），为假不序列化。
    #[serde(default, skip_serializing_if = "is_false")]
    pub permute: bool,
    /// 题面 taint（B58，步 17b）：各槽填入值、计算出的题面文本与题式模板的 taint 之 ∨，字面题为 Trusted。
    /// 判断器读到的题面与读到的材料同样可能被注入，读数与出口 taint = 状态 ∨ 题。
    /// **不进题哈希、不进任何账本键**（同一题面就是同一道题）；Trusted 不序列化。
    /// 依据：B58（`12` §2.11 第 4 条；`20` v2 §3.10「文本 → 题面」行）
    #[serde(default, skip_serializing_if = "is_trusted")]
    pub taint: Taint,
    /// 是非题的答案标签（B155，步 15i）：`test(题面, calib, {labels: {yes, no}})`，线上作
    /// `criteria: {"true": yes, "false": no}`。它改变判断器读到的题，所以**有值时**进题哈希
    /// （无值时哈希与步 15i 前逐字节相同）；为空不序列化。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub labels: Option<TestLabels>,
    /// 超窗裂变的声明（步 23b，`{fission: "approx"}`）：作者同意材料超窗时按窗切块、按题的操作合回（近似档）。
    /// **不进题哈希、不进任何账本键或校准键**（切出来的块本来就是不同的状态）；为空不序列化。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fission: Option<FissionDecl>,
    /// 拿不准时可能缺的信息类别（裁定五十一）：只从题式取（`Form::fill` 带过来），默认链的第二级候选。
    /// **不进题哈希、不进任何账本键**；为空不序列化
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub lacks: Vec<String>,
}

/// 超窗裂变的声明（步 23b；`11` §5.3「test → exists（或按声明 all）」）。今天只有近似档 `approx`
/// （V8 一致率 0.897，`21` 步 23b 注）；`all` 为真时是非题按全称合回。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FissionDecl {
    #[serde(default, skip_serializing_if = "is_false")]
    pub all: bool,
}

/// 是非题的两个答案标签（B155）：`yes` 对应读数 `p` 的「真」，`no` 对应「假」。
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TestLabels {
    pub yes: String,
    pub no: String,
}

impl TestLabels {
    /// 追加进题哈希、题式哈希的分量（只在有标签时追加）
    fn hash_parts(l: &Option<TestLabels>) -> Vec<&str> {
        match l {
            Some(l) => vec!["labels", l.yes.as_str(), l.no.as_str()],
            None => vec![],
        }
    }
}

fn is_false(b: &bool) -> bool {
    !*b
}

fn is_trusted(t: &Taint) -> bool {
    *t == Taint::Trusted
}

impl Question {
    /// 基础题类（B76，步 12e-2）：不看状态，`question_kind(op, request, 未知槽形, 题式声明)`。
    /// 派生只读、不序列化、不进任何哈希；精化类（看状态槽形）在登记读数时算。
    pub fn kind(&self) -> QuestionKind {
        self.kind_on(&SlotShape::UNKNOWN)
    }
    /// 给定状态槽形时的题类（精化类）。
    pub fn kind_on(&self, shape: &SlotShape) -> QuestionKind {
        let request = self.request.as_deref().and_then(Request::parse);
        let decl = SlotDecls {
            over_kind: self.over_kind,
            accepts: None,
        };
        question_kind(self.op, request, shape, &decl).0
    }
    pub fn new(op: Op, text: &str, calib: &str, scale: Vec<String>) -> Question {
        Question::with_evidence(op, text, calib, scale, vec![])
    }
    /// 带决定性证据槽的题（J-09）。`evidence` 进题的哈希——**声明了证据的题和没声明的
    /// 不是同一道题**，账本键按题哈希走，不能让它们共用一条记录。
    pub fn with_evidence(
        op: Op,
        text: &str,
        calib: &str,
        scale: Vec<String>,
        evidence: Vec<String>,
    ) -> Question {
        let hash = hash_of(&[
            "q",
            op.phys(),
            text,
            &scale.join("\u{1e}"),
            &evidence.join("\u{1e}"),
        ]);
        Question {
            op,
            text: text.to_string(),
            calib: calib.to_string(),
            scale,
            evidence,
            hash,
            presupposition: None,
            request: None,
            form_hash: None,
            template: None,
            fill: None,
            from_key: BTreeSet::new(),
            over_kind: None,
            permute: false,
            taint: Taint::Trusted,
            labels: None,
            fission: None,
            lacks: vec![],
        }
    }

    /// 带答案标签（B155，步 15i）：写上 `labels` 并按「原分量 + 标签分量」重算题哈希。
    /// `None` 时哈希与不带标签的题相同。
    pub fn with_labels(mut self, labels: Option<TestLabels>) -> Question {
        if labels.is_none() {
            return self;
        }
        let scale = self.scale.join("\u{1e}");
        let evidence = self.evidence.join("\u{1e}");
        let mut parts = vec!["q", self.op.phys(), &self.text, &scale, &evidence];
        parts.extend(TestLabels::hash_parts(&labels));
        self.hash = hash_of(&parts);
        self.labels = labels;
        self
    }

    /// B1 的「主体」：判断读状态的哪个槽、几个对象。由题型推出——
    /// 是非与打分读 `on` 里的一个对象（关系题是一对，由状态决定，不由题决定）；K 选一读 `over` 里的 K 个候选。
    pub fn subject(&self) -> &'static str {
        match self.op {
            Op::Select => "over",
            Op::Test | Op::Measure => "on",
        }
    }
    /// B1 的「划分」：是非 = 二元划分，K 选一 = K 元划分，打分 = 有序划分（Groenendijk & Stokhof 1984）。
    pub fn partition(&self) -> &'static str {
        match self.op {
            Op::Test => "binary",
            Op::Select => "k_ary",
            Op::Measure => "ordered",
        }
    }
    /// B1 的「请求」。缺省：是非题 `whether`（问是否），K 选一 `one`（恰选一个），打分 `degree`（取一档）。
    pub fn request(&self) -> String {
        self.request
            .clone()
            .unwrap_or_else(|| default_request(self.op).to_string())
    }
}

pub fn default_request(op: Op) -> &'static str {
    match op {
        Op::Test => "whether",
        Op::Select => "one",
        Op::Measure => "degree",
    }
}

/// 题式：带参数槽的题模板（B1、施工件 b）。`fill` 给每个槽填上文本，得到一道题。
///
/// 题式本身不能被判断——`judge` 只收题。它是题的来源，不是题。
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct Form {
    pub op: Op,
    /// 模板题面，槽写成 `{名字}`
    pub template: String,
    /// 模板里出现的槽名，按首次出现的顺序
    pub slots: Vec<String>,
    pub calib: String,
    pub scale: Vec<String>,
    pub evidence: Vec<String>,
    pub presupposition: Option<String>,
    pub request: Option<String>,
    pub hash: String,
    /// `over` 的声明（B76）：`labels`/`candidates`/`questions`/`actions`。只决定题类，不进 `form_hash`。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub over_kind: Option<OverKind>,
    /// 置换声明（B64，步 15f），由 `fill` 带到题上。不进 `form_hash`，为假不序列化。
    #[serde(default, skip_serializing_if = "is_false")]
    pub permute: bool,
    /// 模板文本的 taint（B58 的解释，步 17b）：`form(计算文本, …)` 的模板也是判断器读到的题面，
    /// `fill` 时并进题。不进 `form_hash`；Trusted 不序列化。
    #[serde(default, skip_serializing_if = "is_trusted")]
    pub taint: Taint,
    /// 是非题式的答案标签（B155，步 15i）：有值时进 `form_hash`，`fill` 带到题上。为空不序列化。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub labels: Option<TestLabels>,
    /// 答案块上声明的签名（主会话裁定十九、B192，步 28）：`form(…, {on: {act?: 签名, ignore?: 签名}})`，
    /// 判出哪一块就按那一块的签名接着派生参数题（出题库 derive 的先选后填）。它决定下一题问什么，
    /// 不改这道题的意思：不进 `form_hash`，不序列化（与 `permute` 同类），跟着题式值走、随 import 跨程序复用。
    #[serde(skip)]
    pub on: Option<FormSig>,
    /// 超窗裂变的声明（步 23b），由 `fill` 带到题上。不进 `form_hash`；为空不序列化。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fission: Option<FissionDecl>,
    /// 拿不准时可能缺的信息类别（裁定五十一，`form(…, {lacks: [类别…]})`），由 `fill` 带到题上。不进 `form_hash`；
    /// 为空不序列化
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub lacks: Vec<String>,
}

/// 题式块签名的载体：签名是 `.jpp` 记录（含题式、可选值），按值保存。相等按指针比（签名不参与题式身份）。
#[derive(Clone, Debug)]
pub struct FormSig(pub std::rc::Rc<Value>);

impl PartialEq for FormSig {
    fn eq(&self, other: &Self) -> bool {
        std::rc::Rc::ptr_eq(&self.0, &other.0)
    }
}

impl Form {
    /// 带答案标签（B155，步 15i）：写上 `labels` 并按「原分量 + 标签分量」重算题式哈希。
    /// `None` 时哈希与不带标签的题式相同。
    pub fn with_labels(mut self, labels: Option<TestLabels>) -> Form {
        if labels.is_none() {
            return self;
        }
        let scale = self.scale.join("\u{1e}");
        let evidence = self.evidence.join("\u{1e}");
        let mut parts = vec![
            "form",
            self.op.phys(),
            &self.template,
            &scale,
            &evidence,
            self.presupposition.as_deref().unwrap_or(""),
            self.request.as_deref().unwrap_or(""),
        ];
        parts.extend(TestLabels::hash_parts(&labels));
        self.hash = hash_of(&parts);
        self.labels = labels;
        self
    }
    /// 解析模板里的 `{槽}`。`{{` / `}}` 不作转义——模板里不支持字面花括号，出现未闭合的 `{` 报错。
    pub fn slots_of(template: &str) -> Result<Vec<String>, String> {
        let mut out: Vec<String> = vec![];
        let mut rest = template;
        while let Some(i) = rest.find('{') {
            let after = &rest[i + 1..];
            let Some(j) = after.find('}') else {
                return Err(format!("模板「{template}」里有未闭合的 {{"));
            };
            let name = after[..j].trim();
            if name.is_empty() {
                return Err(format!("模板「{template}」里有空槽 {{}}"));
            }
            if !out.iter().any(|x| x == name) {
                out.push(name.to_string());
            }
            rest = &after[j + 1..];
        }
        Ok(out)
    }
    pub fn new(
        op: Op,
        template: &str,
        calib: &str,
        scale: Vec<String>,
        evidence: Vec<String>,
        presupposition: Option<String>,
        request: Option<String>,
    ) -> Result<Form, String> {
        let slots = Form::slots_of(template)?;
        let hash = hash_of(&[
            "form",
            op.phys(),
            template,
            &scale.join("\u{1e}"),
            &evidence.join("\u{1e}"),
            presupposition.as_deref().unwrap_or(""),
            request.as_deref().unwrap_or(""),
        ]);
        Ok(Form {
            op,
            template: template.to_string(),
            slots,
            calib: calib.to_string(),
            scale,
            evidence,
            presupposition,
            request,
            hash,
            over_kind: None,
            permute: false,
            taint: Taint::Trusted,
            labels: None,
            on: None,
            fission: None,
            lacks: vec![],
        })
    }
    /// 按填法得到一道题。槽必须恰好填满：缺槽、多槽都是错——多出来的键多半是拼错的槽名。
    pub fn fill(&self, fill: &[(String, String)]) -> Result<Question, String> {
        for s in &self.slots {
            if !fill.iter().any(|(k, _)| k == s) {
                return Err(format!("题式「{}」的槽 {s} 没有填", self.template));
            }
        }
        for (k, _) in fill {
            if !self.slots.iter().any(|s| s == k) {
                return Err(format!(
                    "题式「{}」没有槽 {k}（它的槽是 {}）",
                    self.template,
                    self.slots.join("、")
                ));
            }
        }
        let mut text = self.template.clone();
        for (k, v) in fill {
            text = text.replace(&format!("{{{k}}}"), v);
        }
        let mut q = Question::with_evidence(
            self.op,
            &text,
            &self.calib,
            self.scale.clone(),
            self.evidence.clone(),
        )
        // B155：答案标签是题的一部分，进题哈希（无标签时不变）
        .with_labels(self.labels.clone());
        q.presupposition = self.presupposition.clone();
        q.request = self.request.clone();
        q.form_hash = Some(self.hash.clone());
        q.over_kind = self.over_kind;
        q.permute = self.permute;
        q.fission = self.fission;
        q.lacks = self.lacks.clone();
        q.template = Some(self.template.clone());
        // B58（步 17b）：模板的 taint 带到题上；填入值的 taint 由调用处（`fill` 内置）并入
        q.taint = self.taint;
        q.fill = Some(
            self.slots
                .iter()
                .filter_map(|s| fill.iter().find(|(k, _)| k == s).cloned())
                .collect(),
        );
        Ok(q)
    }
}
