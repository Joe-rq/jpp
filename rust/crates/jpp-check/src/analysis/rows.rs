//! 效应行的类型（原 `check.rs` 效应标注节的类型部分，只搬不改）。

#![allow(unused_imports)]
use crate::*;

// ---------------------------------------------------------------- 效应标注（E-effect）
//
// 效应行 `ε = 确定集合 ∪ 效应变量`。依据 Codex 答 (b)：
// `map : ∀ A B ε. (Fnω(A ⊸ε B) ⊗ List<A>) ⊸ε List<B>`——高阶函数自己不产生业务效应，
// 执行效应来自传进来的方法。用户**不写** ε：函数体推断确定的那一半，被调者是本函数的方法参数时
// 留一个**效应变量**，到调用点拿实参实例化（rank-1 + 受限泛化，不追求任意阶完全推断）。
//
// 四条纪律：
//
// 1. **创建方法 ≠ 执行方法**（Codex 答 (c) 第 3 条）。只有落在已知高阶位上的方法体才算会发生——
//    `map`/`filter` 的第 2 位、`fold`/`loop` 的第 3 位、`transform` 的第 1 位、`handle` 的臂、
//    当场造当场调。被创建、被返回、被存进记录的 lambda 不算进外层。
// 2. **解析不了的被调者只让推断变成下界，不整条跳过。**「标注少了 X」只要 X 看得见就该报，
//    这个方向漏报是安全的；只有反方向的「标了却看不到」需要完整信息。
// 3. **效应变量按调用点实例化，不取全局并集。** 同一个高阶函数用在两处、一处传纯方法一处传
//    带 judge 的方法，纯的那处不该背 judge。这是效应多态与单态并集的区别所在。
// 4. **但标注的核验取并集。** `!{…}` 是**上界**：它必须盖住这个函数所有调用点上可能发生的效应，
//    所以核 `declared` 时把各调用点实例化出来的效应并起来，诊断落在**定义处**。
//    「按调用点传播」与「按并集核标注」是两件事，分开算。

/// 效应变量 `(函数 id, 参数下标)`。不变式：一个函数行里的变量 id 一定是它自己——
/// 变量往上传时在调用点重新索引，不会出现别人的变量。
pub(crate) type EffVar = (usize, usize);

/// 名字 → 它的方法类型标注给出的效应行（`Some(行)` = 标了；`None` = 标了 Fn 但没给行）
pub(crate) type Known = HashMap<String, Option<Vec<String>>>;

/// 一条效应行
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Row {
    /// 确定会发生的效应（下界）
    pub(crate) concrete: BTreeSet<String>,
    /// 待实例化的效应变量：被调者是本函数的方法参数
    pub(crate) vars: BTreeSet<EffVar>,
    /// 被调者连名字都拿不到（记录字段、同名多个绑定、中间绑定）：未知，且没有可实例化的位置
    pub(crate) opaque: bool,
}

impl Row {
    pub(crate) fn effect(name: &str) -> Row {
        Row {
            concrete: [name.to_string()].into_iter().collect(),
            ..Row::default()
        }
    }
    pub(crate) fn of<'a>(effects: impl IntoIterator<Item = &'a String>) -> Row {
        Row {
            concrete: effects.into_iter().cloned().collect(),
            ..Row::default()
        }
    }
    pub(crate) fn var(owner: usize, param: usize) -> Row {
        Row {
            vars: [(owner, param)].into_iter().collect(),
            ..Row::default()
        }
    }
    pub(crate) fn opaque() -> Row {
        Row {
            opaque: true,
            ..Row::default()
        }
    }
    /// 除了确定集合，还有说不准的部分吗
    pub(crate) fn open(&self) -> bool {
        !self.vars.is_empty() || self.opaque
    }
    pub(crate) fn absorb(&mut self, other: &Row) {
        self.concrete.extend(other.concrete.iter().cloned());
        self.vars.extend(other.vars.iter().copied());
        self.opaque |= other.opaque;
    }
}

/// 正在推断的那个函数：它的 id 与方法参数下标（留效应变量要用）
#[derive(Clone, Default)]
pub(crate) struct Owner {
    pub(crate) id: usize,
    pub(crate) params: HashMap<String, usize>,
}

impl Owner {
    pub(crate) fn without<'a>(&self, names: impl Iterator<Item = &'a str>) -> Owner {
        let mut o = self.clone();
        for n in names {
            o.params.remove(n);
        }
        o
    }
}

/// 一个函数在所有调用点上、由实参实例化出来的效应（核 `!{…}` 上界用）
#[derive(Clone, Debug, Default)]
pub(crate) struct Instantiated {
    pub(crate) effects: BTreeSet<String>,
    /// 有调用点的实参认不出
    pub(crate) incomplete: bool,
    /// 被调用过至少一次（带效应变量却从没被调用过，等于一个变量都没定死）
    pub(crate) called: bool,
}
