//! IR 与表层共用的叶子类型：源位置、预算、形参与作者写的类型标注。
//!
//! 步 12d 从核心语法树（原 `ast.rs`）搬来；核心语法树删除后，它们是 IR 的一部分。

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Span {
    pub start: usize,
    pub end: usize,
}

impl Span {
    pub fn new(start: usize, end: usize) -> Self {
        Self { start, end }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Budget {
    /// 模型调用次数上限
    pub calls: u64,
    /// 美元上限
    pub cost: f64,
    /// 嵌套调用深度上限（递归也算），None 取默认 256
    #[serde(default)]
    pub depth: Option<u32>,
    /// `ask` 次数上限，None 取 0
    #[serde(default)]
    pub escalate: Option<u64>,
    /// **J-10 的 unsure 预算**（`12`:591「超 `budget.unsure` 即**报**」）。
    /// `None` = 不设限，**不是 0**——设成 0 是「一条 unsure 都不许有」，是个很强的断言。
    ///
    /// **它是这四格里唯一只「报」不「停」的**：`calls`/`cost`/`escalate` 超了都 `Halt`，
    /// **这条不停**。**那是条文写的（「即报」），不是实现偷懒**——别顺手「修正」成 Halt。
    #[serde(default)]
    pub unsure: Option<f64>,
    /// **判断力缺席策略**（B32）：判断器调用失败（连接、超时、无凭据）时怎么办。
    /// `None` = 未声明：沿用旧行为（客户端错误即运行期错误）。
    #[serde(default)]
    pub absent: Option<AbsentPolicy>,
    /// **时延预算**（B32，秒）：本趟判断调用的累计耗时上限。超出后的判断站点转 `Unsure(latency)`；
    /// 静态估计（层数 × 画像 p95）超出时检查阶段报错。`None` = 不设限。
    #[serde(default)]
    pub latency_p95: Option<f64>,
}

/// B32：`budget {absent: {retry, backoff, then, breaker}}`。
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AbsentPolicy {
    /// 失败后重试次数
    pub retry: u32,
    /// 首次重试前等待秒数，之后每次翻倍
    pub backoff: f64,
    /// 重试用尽后：`escalate`（程序挂起，cause=absent，待判断器恢复后续跑）/
    /// `conservative`（该调用的题出口为 `Unsure(absent)`，程序继续）/ `fail`（运行期错误）
    pub then: String,
    /// 连续缺席多少次后熔断：之后的判断不再发出，直接按 `then` 处理
    pub breaker: u32,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Parameter {
    pub name: String,
    pub annotation: Option<Type>,
    pub span: Span,
}

/// 作者写的类型标注（步 12d 起由降级解析：类型名成为 [`TypeName`]，检查器按变体判，不再比字符串）。
///
/// 只是作者写下的标注，不是推断出的类型（`B71`：类型与效应推断是 L3 的事）。
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Type {
    Named(TypeName),
    Applied(TypeName, Vec<Type>),
    /// 只有参数与结果的旧形式：效应行未知。前端目前 lower 出的就是这个。
    Function(Vec<Type>, Box<Type>),
    /// 方法类型：效应行与责任捕获跟着**类型**走，所以方法经参数、记录字段、返回值传递时契约不丢。
    Method(MethodType),
}

/// `Fn^κ(params ⊸ε ret)`。`effects` 是效应行（None = 未知/待推断，Some 是上界）；
/// `captures_responsibility` 区分 `Fn¹`（捕获了未决责任，不可随意丢弃或重复调用）与 `Fnω`。
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MethodType {
    pub params: Vec<Type>,
    pub ret: Box<Type>,
    #[serde(default)]
    pub effects: Option<Vec<String>>,
    #[serde(default)]
    pub captures_responsibility: bool,
}

impl MethodType {
    /// `Fnω`：可重复调用、可丢弃；高阶操作（map / filter）只收这种
    pub fn omega(params: Vec<Type>, ret: Type, effects: &[&str]) -> MethodType {
        MethodType {
            params,
            ret: Box::new(ret),
            effects: Some(effects.iter().map(|e| e.to_string()).collect()),
            captures_responsibility: false,
        }
    }
    /// `Fn¹`：捕获了未决责任
    pub fn linear(params: Vec<Type>, ret: Type, effects: &[&str]) -> MethodType {
        MethodType {
            captures_responsibility: true,
            ..MethodType::omega(params, ret, effects)
        }
    }
}

/// 类型名。内核与检查器认得的名字各有一个变体，其余（用户记录名、`Mat`、`State` 等由运行期把关的
/// 名字）留原文。序列化为作者写的原文（`Float` 与 `Decimal` 各自保留），IR 打印因此与类型化之前相同。
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TypeName {
    Int,
    Decimal,
    /// `Decimal` 的别名
    Float,
    Bool,
    Text,
    List,
    Record,
    Unit,
    /// 旧式方法类型名（无效应行）
    Fn,
    Method,
    Question,
    Form,
    Outcome,
    Exit,
    Other(String),
}

impl TypeName {
    /// 降级用：作者写的名字 → 类型名
    pub fn parse(name: &str) -> TypeName {
        match name {
            "Int" => TypeName::Int,
            "Decimal" => TypeName::Decimal,
            "Float" => TypeName::Float,
            "Bool" => TypeName::Bool,
            "Text" => TypeName::Text,
            "List" => TypeName::List,
            "Record" => TypeName::Record,
            "Unit" => TypeName::Unit,
            "Fn" => TypeName::Fn,
            "Method" => TypeName::Method,
            "Question" => TypeName::Question,
            "Form" => TypeName::Form,
            "Outcome" => TypeName::Outcome,
            "Exit" => TypeName::Exit,
            other => TypeName::Other(other.to_string()),
        }
    }
    /// 作者写的原文
    pub fn as_str(&self) -> &str {
        match self {
            TypeName::Int => "Int",
            TypeName::Decimal => "Decimal",
            TypeName::Float => "Float",
            TypeName::Bool => "Bool",
            TypeName::Text => "Text",
            TypeName::List => "List",
            TypeName::Record => "Record",
            TypeName::Unit => "Unit",
            TypeName::Fn => "Fn",
            TypeName::Method => "Method",
            TypeName::Question => "Question",
            TypeName::Form => "Form",
            TypeName::Outcome => "Outcome",
            TypeName::Exit => "Exit",
            TypeName::Other(s) => s,
        }
    }
}

impl Serialize for TypeName {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for TypeName {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<TypeName, D::Error> {
        Ok(TypeName::parse(&String::deserialize(d)?))
    }
}

impl Type {
    /// 类型是否提到某个类型名（J-05 的返回类型消费看 `Exit`）
    pub fn mentions(&self, name: &str) -> bool {
        match self {
            Type::Named(n) => n.as_str() == name,
            Type::Applied(n, args) => n.as_str() == name || args.iter().any(|a| a.mentions(name)),
            Type::Function(ps, r) => ps.iter().any(|p| p.mentions(name)) || r.mentions(name),
            Type::Method(m) => m.params.iter().any(|p| p.mentions(name)) || m.ret.mentions(name),
        }
    }

    /// 这个类型是方法吗？是的话给出它的效应行（None = 未知）与是否捕获责任。
    pub fn as_method(&self) -> Option<(Option<&[String]>, bool)> {
        match self {
            Type::Method(m) => Some((m.effects.as_deref(), m.captures_responsibility)),
            Type::Function(_, _) => Some((None, false)),
            Type::Named(TypeName::Fn | TypeName::Method)
            | Type::Applied(TypeName::Fn | TypeName::Method, _) => Some((None, false)),
            _ => None,
        }
    }
}
