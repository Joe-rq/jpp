//! 回答与读数（`Answer`、`Reading`、`Score`、惰性出口与生成的句柄、答案表）。步 36 G3 从 `value.rs` 原样搬出（只搬不改）。

use super::*;

/// 模型对一题的回答（校验后的规范形）。
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Answer {
    Noul(f64),
    /// 按 over 下标的概率
    Choice(Vec<f64>),
    /// 按档位下标的概率
    Score(Vec<f64>),
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Reading {
    pub q_hash: String,
    pub state_hash: String,
    pub op: Op,
    pub calib: String,
    /// 读数句柄（步 11b-3）：答案不在读数上，在持有者的答案表里（运行时的私有读数表；
    /// 宿主与测试用 [`AnswerTable`]）。同一次运行内唯一，由登记处分配。
    #[serde(default)]
    pub id: u64,
    /// 状态含 Fail 材料时读数为空（J-12 → Unsure(fail)）
    pub fail: Option<String>,
    pub model_id: String,
    pub ledger_key: String,
    pub over_len: usize,
    pub scale: Vec<String>,
    /// 用了几个置换。**与 `mode_share` 成对**——K 是那个测量身份的一部分。
    #[serde(default)]
    pub perms: std::cell::Cell<usize>,
    /// 置换众数占比（`12`:151）。`None` = 这条路上没测过置换 → `cut` 不给 `Pick`。
    /// 与 `answer` 同样是刷新时才填上的，所以是 `Cell`。
    #[serde(default, skip)]
    pub mode_share: std::cell::Cell<Option<f64>>,
    /// 这道题声明了、而状态里没有的决定性证据槽（J-09）。`cut` 判序第一步据此给
    /// `Unsure(insufficient)`——**在看 p 之前**。登记时就算好，因为那时状态还在手上。
    #[serde(default)]
    pub missing_evidence: Vec<String>,
    /// 被判断的那个状态的 taint。`cut` 据此给出口定 taint
    /// （`12`:150「出口 taint 继承状态 taint」、§2.11「cut 继承」）。
    /// 以前没有这个字段，`cut` 只好一律给 `Trusted`——**状态算好的 taint 被丢掉了**。
    /// 步 17b（B58）起是「状态 ∨ 题面」：判断器读到的题面与材料同样计入，字段名沿用。
    #[serde(default)]
    pub state_taint: Taint,
    /// 题来自哪个题式（件 b 的 `form_hash`）。`cut` 在题键没有上岗记录时据此退到题式键
    /// （B2 待裁，本版只作回退层）。不是由题式填出的题为 `None`。
    #[serde(default)]
    pub form_hash: Option<String>,
    /// 被判断材料（`on` 槽）的指纹（B68）。登记时算好——那时状态还在手上；`cut` 据此核认证范围。
    /// 由材料推得出，不进序列化；`fit`、`repeat` 合成的读数为 `None`（不核范围）。
    #[serde(default, skip)]
    pub fp: Option<[f64; 7]>,
}

/// 声明式拟合的结果（B153 (2)，步 20j-4）：句柄。数在运行时的私有表里，按 `id` 取；这里只有身份与来源。
#[derive(Debug)]
pub struct Score {
    /// 同一次运行内唯一（运行时分配）
    pub id: u64,
    /// `hash(闭包结构哈希, 各输入校准键)`：同一 `fit_hash` 即作者声明的同一把尺子（`order` 据此）
    pub fit_hash: String,
    /// 各输入读数的校准键（按位）
    pub inputs: Vec<String>,
    /// 各输入读数的账本键（按位；谱系放行经它追到输入材料的来源，B72-4）
    pub input_keys: Vec<String>,
    /// ∨ 各输入读数的状态 taint ∨ `extra` 的 taint
    pub taint: Taint,
    /// 并列带宽（`fit({declare, tie})`，缺省 0；`order` 用，不取画像 δ）
    pub tie: f64,
    /// 各输入状态哈希的合成（报告 `exits` 行的 `item`）
    pub state_hash: String,
    /// 输入读数不可用时的未决原因（`fail`、缺席类原因、`insufficient`，带细节；步 36 G3 起是封闭原因）；为 `Some` 时没有数
    pub fail: Option<Why>,
    /// 造它的 `fit` 站点
    pub site: Span,
}

/// 未解析的出口（B94，步 23c）。出口号在 `cut` 时分配（与改前同序），解析出的出口挂回登记它的那一帧。
#[derive(Debug)]
pub struct PendingCut {
    pub reading: Rc<Reading>,
    /// `cut` 的第二位：校准键（Text）
    pub calib: Option<String>,
    /// `cut` 的代价记录 `{cost: [fp, fn]}`（B29）
    pub cost: Option<(f64, f64)>,
    /// `cut` 的 `alpha: a`（B129，步 20j-1）：在 α ≤ a 的证书里选
    pub alpha: Option<f64>,
    /// `cut` 的作者声明线（B128，步 20j-1；`cuts`、`closed` 为步 20j-3）
    pub declare: Option<crate::bridge::DeclaredLine>,
    /// `cut` 的统计量（B153、B154，步 20j-3）；缺省 `max`
    pub stat: crate::stat::Stat,
    /// `cut` 的 `feasible: fn(k) -> Bool`（C-4）：已决 `pick` 之后按代码谓词改选；解析出口时求值
    pub feasible: Option<Rc<Closure>>,
    pub site: Span,
    /// `cut` 时分配的出口号
    pub id: usize,
    /// 登记它的帧（运行时帧栈的下标）
    pub frame: usize,
    pub resolved: RefCell<Option<Rc<Exit>>>,
}

impl PendingCut {
    pub fn exit(&self) -> Option<Rc<Exit>> {
        self.resolved.borrow().clone()
    }
}

/// 未取回的生成（B149，步 15h-2）。`id` 是生成登记序号；`mark` 是登记时读数号计数器的值，用来与判断比
/// 登记先后（账本按登记序，B149）。作业本身（票据、键、提示）留在运行时。
#[derive(Debug)]
pub struct PendingGen {
    pub id: u64,
    pub mark: u64,
    pub site: Span,
    pub resolved: RefCell<Option<Value>>,
}

impl PendingGen {
    pub fn value(&self) -> Option<Value> {
        self.resolved.borrow().clone()
    }
}

impl Reading {
    pub fn set_mode_share(&self, ms: f64, perms: usize) {
        self.mode_share.set(Some(ms));
        self.perms.set(perms);
    }
}

/// 答案源：持有答案表的一方实现它（步 11b-3，`20` §2.3「答案进运行时私有读数表」）。
///
/// **判据（比清单更可靠）：凡结果依赖于答案的操作，都是刷新点**——读答案之前必须先 `flush`。
/// `12` §2.2:129 列了 `cut`/`fit`/`match`/`if`/读内容五项，但清单形式会漏：每加一个读答案的操作
/// 就要记得补一项，而**漏补的失效方式是静默给出错误结果**（`allocate` 漏了刷新时选出 `[0,1,2,3]`
/// 而不是 `[2,4,6,8]`，不报错也不告警）。运行时的实现是 `readings.rs::answer_of`，唯一入口。
pub trait Answers {
    /// 已经填上的答案；`None` = 还没刷新（或失败）。
    fn answer_of(&self, r: &Reading) -> Option<Answer>;
}

/// 按读数句柄存答案的表。运行时持一张私有的；宿主与测试（`strength` 的纯函数入口）用它自带答案。
#[derive(Clone, Debug, Default, PartialEq)]
pub struct AnswerTable(std::collections::BTreeMap<u64, Answer>);

impl AnswerTable {
    pub fn new() -> AnswerTable {
        AnswerTable::default()
    }
    /// 记下一个读数的答案（同一句柄再写即覆盖）
    pub fn insert(&mut self, r: &Reading, a: Answer) {
        self.0.insert(r.id, a);
    }
}

impl Answers for AnswerTable {
    fn answer_of(&self, r: &Reading) -> Option<Answer> {
        self.0.get(&r.id).cloned()
    }
}
