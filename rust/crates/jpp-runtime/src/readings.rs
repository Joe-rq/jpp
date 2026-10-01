//! 读数表（20 §2.3 `readings.rs`）：读答案的唯一入口与刷新点登记。
//!
//! 步 8b：`answer_of` 是运行时读答案的唯一入口（写答案的唯一入口是 `flush.rs::fill_answer`）；
//! 刷新点改为「判据入口 + 结构性刷新点表」：凡结果依赖答案的操作都在读答案前刷新（判据），
//! 下表把现有的每一处 `flush(原因)` 登记下来并标明是判据还是结构性（`12` §2.2 列出的
//! `if`、`handle`/`match`、程序结束、构造入口）。`flush` 在调试构建里核原因已登记，
//! 新增刷新点必须先在这里登记。
//! 依据：`12` §2.2 刷新点判据（「凡结果依赖于答案的操作，皆是刷新点」）；`20` §2.3 `readings.rs`。

use super::*;

impl<'a> Interp<'a> {
    /// 读答案的唯一入口：答案在运行时的私有读数表里（步 11b-3），读数只是句柄。
    /// 调用者必须已经刷新（判据：结果依赖答案的操作先 `flush`）。
    pub(crate) fn answer_of(&self, r: &Reading) -> Option<Answer> {
        self.answers.borrow().answer_of(r)
    }

    /// 标一个读数缺席（G5）：按读数记，同时记在内容键上（没有答案的同键读数照此给出）
    pub(crate) fn 标缺席(&mut self, r: &Reading, k: &str, cause: &str) {
        self.absent_marks.insert(k.to_string(), cause.to_string());
        self.读数缺席.insert(r.id, cause.to_string());
    }

    /// 读数的缺席原因（G5）：按读数记过的取它；否则内容键上的标记只对还没有答案的读数成立——同一内容键后来再问、
    /// 答到了的读数不被先前的缺席改写（裁定五十九第 16 条：缺席不是答案，后续刷新可以再问）
    pub(crate) fn 缺席因(&self, r: &Reading) -> Option<String> {
        if let Some(c) = self.读数缺席.get(&r.id) {
            return Some(c.clone());
        }
        match self.absent_marks.get(&r.ledger_key) {
            Some(c) if self.answer_of(r).is_none() => Some(c.clone()),
            _ => None,
        }
    }

    /// 判断器缺席（原因 `absent`）这一次该记在哪个键上（G5 附录一）：第 1 次是 `absent:<k>`（与改前相同），之后在
    /// 非审计时依次 `absent:<k>#2`、`#3`…（序号按整本账本计）。其余原因、以及审计重放照旧只写第 1 条。
    /// `None` = 不写（已有）
    pub(crate) fn 缺席记录键(&self, k: &str, cause: &str) -> Option<String> {
        let base = format!("absent:{k}");
        if self.账本查(&base).is_none() {
            return Some(base);
        }
        if cause != "absent" || self.audit.on {
            return None;
        }
        (2u32..)
            .map(|n| format!("{base}#{n}"))
            .find(|mk| self.账本查(mk).is_none())
    }

    /// 审计重放（G5 附录二）：这道题第 `m` 次真登记在最后一趟里对应的缺席记录键。「最后一趟」是账本里最后一条
    /// 趟标记（`Attempt`）那一趟：与标记同一追踪段、在标记之前、在上一条标记之后的 `absent` 缺席记录，按 `nth` 配
    pub(crate) fn 重放记录(&self, k: &str, m: u32) -> Option<String> {
        self.重放趟表()?.记录.get(&(k.to_string(), m)).cloned()
    }

    /// 审计重放（G5 附录二）：这道题的答案是不是在最后一条趟标记之后写的（没写标记、被杀的趟留下的），重放不用
    pub(crate) fn 重放答案越界(&self, k: &str) -> bool {
        match (self.重放趟表(), self.ledger.view().position(k)) {
            (Some(t), Some(i)) => i > t.标记,
            _ => false,
        }
    }

    fn 重放趟表(&self) -> Option<&重放趟表> {
        if !self.audit.on {
            return None;
        }
        self.重放趟
            .get_or_init(|| {
                let l = self.ledger.view();
                let 标记: Vec<usize> = l
                    .entries
                    .iter()
                    .enumerate()
                    .filter(|(_, e)| matches!(e, Entry::Attempt { .. }))
                    .map(|(i, _)| i)
                    .collect();
                let (&末, 前) = 标记.split_last()?;
                let 起 = 前.last().map(|i| i + 1).unwrap_or(0);
                let 段 = l.trace_at(末);
                let mut 记录 = HashMap::new();
                for i in 起..末 {
                    if let Entry::Absent {
                        key,
                        cause,
                        nth: Some(m),
                        ..
                    } = &l.entries[i]
                        && cause == "absent"
                        && l.trace_at(i) == 段
                        && let Some(k) = 缺席基键(key)
                    {
                        记录.insert((k.to_string(), *m), key.clone());
                    }
                }
                Some(重放趟表 {
                    标记: 末, 记录
                })
            })
            .as_ref()
    }

    /// 本趟由真站点登记的读数编号（G5 附录二）：每道题从 1 起
    pub(crate) fn 记真登记(&mut self, r: &Reading) {
        if r.ledger_key.is_empty() {
            return;
        }
        let n = self.真登记次数.entry(r.ledger_key.clone()).or_insert(0);
        *n += 1;
        let n = *n;
        self.真登记序.insert(r.id, n);
    }

    /// 趟末写一条趟标记（G5 附录二，主控定）：非审计、账本里已有原因为 `absent` 的缺席记录时写；正常结束、挂起、
    /// 出错三条路都写，全复用的趟也写。审计重放复现最后一条标记那一趟
    pub(crate) fn 写趟标记(&mut self) {
        if self.audit.on {
            return;
        }
        let 有缺席 = self
            .ledger
            .view()
            .entries
            .iter()
            .any(|e| matches!(e, Entry::Absent { cause, .. } if cause == "absent"));
        if !有缺席 {
            return;
        }
        let a = self.尝试();
        self.登记记账(Entry::Attempt {
            program: a.program,
            n: a.n,
            host_epoch: 0,
            flush_epoch: 0,
            snapshot: Default::default(),
            peeks: vec![],
            stale: vec![],
        });
    }

    /// 新读数的句柄（本次运行内唯一）
    pub(crate) fn new_reading_id(&self) -> u64 {
        let id = self.next_reading.get();
        self.next_reading.set(id + 1);
        id
    }
}

/// 刷新点的种类
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RefreshKind {
    /// 判据：这个操作的结果依赖答案（读答案前刷新）
    Criterion,
    /// 结构性：`12` §2.2 明列的控制流位置
    Structural,
}

/// 现有的全部刷新点：`flush(原因)` 的原因 → 种类与所在操作。
pub(crate) const REFRESH_POINTS: &[(&str, RefreshKind, &str)] = &[
    (
        "cut",
        RefreshKind::Criterion,
        "bridge.rs cut：构造内部（sieve、literalize 等）当场读答案过线；程序里的 cut 步 23c 起惰性，刷新在检视点",
    ),
    (
        "fit",
        RefreshKind::Criterion,
        "constructs/fit.rs：读答案拟合",
    ),
    ("order", RefreshKind::Criterion, "constructs：按读数排序"),
    (
        "allocate",
        RefreshKind::Criterion,
        "constructs/allocate.rs：按不确定性分配",
    ),
    (
        "unsure_bound",
        RefreshKind::Criterion,
        "constructs/allocate.rs：未决上界",
    ),
    (
        "repeat",
        RefreshKind::Criterion,
        "constructs/repeat.rs：合并重复读数",
    ),
    (
        "sieve",
        RefreshKind::Criterion,
        "constructs/sieve.rs：三路分流读出口",
    ),
    ("content", RefreshKind::Criterion, "eval.rs：宿主读材料内容"),
    (
        "inspect",
        RefreshKind::Criterion,
        "bridge.rs 解析出口：惰性出口被检视（内置与构造的实参、if 条件、运算、取字段、函数与程序返回；B94）",
    ),
    ("if", RefreshKind::Structural, "eval.rs：条件"),
    ("end", RefreshKind::Structural, "outcome.rs：程序结束"),
];

/// 查一个刷新原因是否已登记。
pub(crate) fn refresh_point(reason: &str) -> Option<RefreshKind> {
    REFRESH_POINTS
        .iter()
        .find(|(r, _, _)| *r == reason)
        .map(|(_, k, _)| *k)
}

/// 缺席记录键的内容键部分：`absent:<k>` 或 `absent:<k>#<n>`（n ≥ 2，G5 附录一）
fn 缺席基键(key: &str) -> Option<&str> {
    let rest = key.strip_prefix("absent:")?;
    match rest.rsplit_once('#') {
        Some((k, n)) if n.parse::<u32>().is_ok_and(|n| n >= 2) => Some(k),
        _ => Some(rest),
    }
}

/// 审计重放的最后一趟（G5 附录二）
pub(crate) struct 重放趟表 {
    /// 最后一条趟标记的下标
    标记: usize,
    /// (题, 第几次真登记) → 缺席记录键
    记录: HashMap<(String, u32), String>,
}
