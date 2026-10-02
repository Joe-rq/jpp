//! 按值身份的缓存（Z0882，`地基/过程记录/工程-C2-单元图求值.md` 附录五）。
//!
//! 值不可变（`Rc<Vec<Value>>`、`Rc<str>`、`Rc<Mat>` …），同一份大值上的派生量（来源并集、帧实参的指纹哈希、单元键的
//! 值哈希）算一次就够。今天它们每取一次都把整份值走一遍，按项的闭包捕获整份材料列表时总量是「调用次数 × 值大小」。
//!
//! 键是 (`Rc` 指针, 种类)；条目里存一个 `Weak`，命中时 `upgrade` 且 `ptr_eq` 才算数，地址复用撞不上，也不钉住值。
//! 只缓存大节点（`List`、`Record` ≥ 2 项（附录六：每步 `append` 出的新列表只走自己与子节点的引用），`Text` ≥ 256 字节，`Mat`、`State`、`Question`、`Form`）。先算一次「子树标记」：
//! 有没有函数值（闭包的捕获环境会长出新名字，`to_json` 与捕获指纹会变）、有没有句柄（出口、惰性出口、生成、责任、
//! 读数、打分：带内部可变格或会被取回）。标记只取决于节点的种类结构，值不可变所以它本身也可缓存。各派生量按自己
//! 依赖的东西定缓存条件：来源并集——没有句柄；帧实参指纹哈希——没有函数值（句柄的帧指纹只取不变的身份字段）；
//! 单元值哈希——有句柄时一定算不出、直接给「算不出」，否则没有函数值才缓存。另有两张表：来源集合（`Sources` 的
//! `Rc<BTreeMap>`，≥ 64 条边）的内容摘要与两两并集。缓存与否不改变任何派生量的值：命中给出的就是重算会得到的那个。
//! 线程局部：解释器单线程（C10 之前）。

use super::*;
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Weak;

/// 子树标记：有没有函数值、有没有句柄。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct 子树标记 {
    pub 有函数: bool,
    pub 有句柄: bool,
}

impl 子树标记 {
    /// 两样都没有：派生量可以缓存。
    pub fn 可缓存(self) -> bool {
        !self.有函数 && !self.有句柄
    }
    fn 并(self, o: 子树标记) -> 子树标记 {
        子树标记 { 有函数: self.有函数 || o.有函数, 有句柄: self.有句柄 || o.有句柄 }
    }
}

enum 弱引用 {
    List(Weak<Vec<Value>>),
    Record(Weak<Vec<(String, Value)>>),
    Text(Weak<str>),
    Mat(Weak<Mat>),
    State(Weak<State>),
    Question(Weak<Question>),
    Form(Weak<Form>),
}

impl 弱引用 {
    /// 这个条目还指着 `v` 吗。
    fn 指着(&self, v: &Value) -> bool {
        match (self, v) {
            (弱引用::List(w), Value::List(l)) => w.upgrade().is_some_and(|x| Rc::ptr_eq(&x, l)),
            (弱引用::Record(w), Value::Record(r)) => w.upgrade().is_some_and(|x| Rc::ptr_eq(&x, r)),
            (弱引用::Text(w), Value::Text(t, _)) => w.upgrade().is_some_and(|x| Rc::ptr_eq(&x, t)),
            (弱引用::Mat(w), Value::Mat(m)) => w.upgrade().is_some_and(|x| Rc::ptr_eq(&x, m)),
            (弱引用::State(w), Value::State(s)) => w.upgrade().is_some_and(|x| Rc::ptr_eq(&x, s)),
            (弱引用::Question(w), Value::Question(q)) => w.upgrade().is_some_and(|x| Rc::ptr_eq(&x, q)),
            (弱引用::Form(w), Value::Form(f)) => w.upgrade().is_some_and(|x| Rc::ptr_eq(&x, f)),
            _ => false,
        }
    }
    fn 活着(&self) -> bool {
        match self {
            弱引用::List(w) => w.strong_count() > 0,
            弱引用::Record(w) => w.strong_count() > 0,
            弱引用::Text(w) => w.strong_count() > 0,
            弱引用::Mat(w) => w.strong_count() > 0,
            弱引用::State(w) => w.strong_count() > 0,
            弱引用::Question(w) => w.strong_count() > 0,
            弱引用::Form(w) => w.strong_count() > 0,
        }
    }
}

struct 条目 {
    弱: 弱引用,
    标记: 子树标记,
    来源: Option<Provenance>,
    指纹哈希: Option<String>,
    单元哈希: Option<Option<String>>,
}

/// 缓存门槛：`List`/`Record` 至少这么多项、`Text` 至少这么多字节才缓存（小节点重算比查表便宜）。附录六：列表与记录
/// 降到 2 项——`iterate` 的累积值每步是新列表，元素是旧的；元素也缓存，新列表的哈希就只走元素引用。
pub const 缓存项数: usize = 2;
pub const 缓存字节: usize = 256;
/// 条目数过这个数时清一次已释放的；清理后门槛取「存活数的两倍」与它的较大者（自适应，附录六：存活条目多时
/// 不会每插一条就扫一遍表）。
const 清理门槛: usize = 1 << 14;

thread_local! {
    static 表: RefCell<HashMap<(usize, u8), 条目>> = RefCell::new(HashMap::new());
    static 慢路径: Cell<u64> = const { Cell::new(0) };
    /// 三张表各自的下一次清理门槛
    static 下次清理: Cell<[usize; 3]> = const { Cell::new([清理门槛; 3]) };
}

/// 表 `i` 的条目数到了它的清理门槛吗；到了就清（`清` 返回清理后的存活数），并把门槛调成存活数的两倍（至少 `清理门槛`）。
fn 按需清理(i: usize, len: usize, 清: impl FnOnce() -> usize) {
    let mut g = 下次清理.with(|c| c.get());
    if len < g[i] {
        return;
    }
    let 活 = 清();
    g[i] = (活 * 2).max(清理门槛);
    下次清理.with(|c| c.set(g));
}

/// 这个节点按身份缓存吗：给 (指针, 种类) 与弱引用。
fn 身份(v: &Value) -> Option<((usize, u8), 弱引用)> {
    Some(match v {
        Value::List(l) if l.len() >= 缓存项数 => ((Rc::as_ptr(l) as *const () as usize, 1), 弱引用::List(Rc::downgrade(l))),
        Value::Record(r) if r.len() >= 缓存项数 => {
            ((Rc::as_ptr(r) as *const () as usize, 2), 弱引用::Record(Rc::downgrade(r)))
        }
        Value::Text(t, _) if t.len() >= 缓存字节 => ((Rc::as_ptr(t) as *const u8 as usize, 3), 弱引用::Text(Rc::downgrade(t))),
        Value::Mat(m) => ((Rc::as_ptr(m) as *const () as usize, 4), 弱引用::Mat(Rc::downgrade(m))),
        Value::State(s) => ((Rc::as_ptr(s) as *const () as usize, 5), 弱引用::State(Rc::downgrade(s))),
        Value::Question(q) => ((Rc::as_ptr(q) as *const () as usize, 6), 弱引用::Question(Rc::downgrade(q))),
        Value::Form(f) => ((Rc::as_ptr(f) as *const () as usize, 7), 弱引用::Form(Rc::downgrade(f))),
        _ => return None,
    })
}

/// 这个值是不是按身份缓存的大节点（单元哈希据此决定父节点里只写它的哈希）。
pub fn 是大节点(v: &Value) -> bool {
    身份(v).is_some()
}

// ───────── 来源集合（`Sources` 里的 `Rc<BTreeMap>`）─────────

type 来源表 = Rc<std::collections::BTreeMap<String, Edge>>;
type 弱来源表 = Weak<std::collections::BTreeMap<String, Edge>>;

/// 来源集合按身份缓存的门槛：至少这么多条边。
const 来源门槛: usize = 64;

/// 并集表的一项：(左弱, 右弱, 结果)
type 并集项 = (弱来源表, 弱来源表, Sources);

thread_local! {
    /// 来源集合的摘要：指针 → (弱引用, 摘要)
    static 集摘要: RefCell<HashMap<usize, (弱来源表, String)>> = RefCell::new(HashMap::new());
    /// 并集：(左指针, 右指针) → (左弱, 右弱, 结果)
    static 并集表: RefCell<HashMap<(usize, usize), 并集项>> = RefCell::new(HashMap::new());
}

fn 同一(w: &弱来源表, r: &来源表) -> bool {
    w.upgrade().is_some_and(|x| Rc::ptr_eq(&x, r))
}

/// 来源集合的内容摘要（单元值哈希写来源标签时用；内容相同则摘要相同）。大集合按身份缓存。
pub fn 来源集摘要(s: &Sources) -> String {
    let Some(m) = &s.0 else {
        return "∅".into();
    };
    let 算 = || {
        记访问();
        hash_of(&["sources", &format!("{:?}", **m)])
    };
    if m.len() < 来源门槛 {
        return 算();
    }
    let k = Rc::as_ptr(m) as usize;
    if let Some(h) = 集摘要.with(|t| t.borrow().get(&k).filter(|(w, _)| 同一(w, m)).map(|(_, h)| h.clone())) {
        return h;
    }
    let h = 算();
    集摘要.with(|t| {
        let mut t = t.borrow_mut();
        let n = t.len();
        按需清理(1, n, || {
            t.retain(|_, (w, _)| w.strong_count() > 0);
            t.len()
        });
        t.insert(k, (Rc::downgrade(m), h.clone()));
    });
    h
}

/// 两个来源集合的并集，大集合按 (左, 右) 身份记住结果（`Sources::union` 用）。
pub(crate) fn 并集(a: &来源表, b: &来源表, 算: impl FnOnce() -> Sources) -> Sources {
    if a.len() + b.len() < 来源门槛 {
        return 算();
    }
    let k = (Rc::as_ptr(a) as usize, Rc::as_ptr(b) as usize);
    if let Some(r) = 并集表.with(|t| {
        t.borrow().get(&k).filter(|(wa, wb, _)| 同一(wa, a) && 同一(wb, b)).map(|(_, _, r)| r.clone())
    }) {
        return r;
    }
    let r = 算();
    并集表.with(|t| {
        let mut t = t.borrow_mut();
        let n = t.len();
        按需清理(2, n, || {
            t.retain(|_, (wa, wb, _)| wa.strong_count() > 0 && wb.strong_count() > 0);
            t.len()
        });
        t.insert(k, (Rc::downgrade(a), Rc::downgrade(b), r.clone()));
    });
    r
}

/// 慢路径访问节点数加一（三种派生量的重算每访问一个节点调一次）。
pub fn 记访问() {
    慢路径.with(|c| c.set(c.get() + 1));
}

/// 只给容器节点（列表、记录、`stop`）与按身份缓存的大节点记访问：叶子随父节点一起走，按父节点计（附录六：计数要量的是
/// 「每次重算走过多少个容器」，新列表逐个读一遍旧元素的引用是 `append` 本身就要付的线性代价，不算重复劳动）。
pub fn 记访问_容器(v: &Value) {
    if matches!(v, Value::List(_) | Value::Record(_) | Value::Stop(_)) || 是大节点(v) {
        记访问();
    }
}

/// 本线程累计的慢路径访问节点数（回归测试取前后差）。
pub fn 慢路径访问数() -> u64 {
    慢路径.with(|c| c.get())
}

/// 在缓存里找 `v` 的条目并对它做 `f`；没有（或已失效）就先建一个（算标记）。不是缓存节点给 `None`。
fn 用条目<T>(v: &Value, f: impl FnOnce(&mut 条目) -> T) -> Option<T> {
    let (k, 弱) = 身份(v)?;
    let 已有 = 表.with(|t| t.borrow().get(&k).is_some_and(|e| e.弱.指着(v)));
    if !已有 {
        // 标记要在借表之外算（会递归进子节点的条目）
        let 标记 = 算标记(v);
        表.with(|t| {
            let mut t = t.borrow_mut();
            let n = t.len();
            按需清理(0, n, || {
                t.retain(|_, e| e.弱.活着());
                t.len()
            });
            t.insert(k, 条目 { 弱, 标记, 来源: None, 指纹哈希: None, 单元哈希: None });
        });
    }
    表.with(|t| t.borrow_mut().get_mut(&k).map(f))
}

fn 算标记(v: &Value) -> 子树标记 {
    记访问_容器(v);
    match v {
        Value::List(l) => l.iter().fold(子树标记::default(), |a, x| a.并(标记(x))),
        Value::Record(r) => r.iter().fold(子树标记::default(), |a, (_, x)| a.并(标记(x))),
        Value::Stop(x) => 标记(x),
        Value::Fn(_) => 子树标记 { 有函数: true, 有句柄: false },
        Value::Exit(_) | Value::Cut(_) | Value::Gen(_) | Value::Duty(_) | Value::Reading(_) | Value::Score(_) => {
            子树标记 { 有函数: false, 有句柄: true }
        }
        _ => 子树标记::default(),
    }
}

/// 子树标记（大节点走缓存）。
pub fn 标记(v: &Value) -> 子树标记 {
    match 用条目(v, |e| e.标记) {
        Some(m) => m,
        None => 算标记(v),
    }
}

/// 取一个可缓存的派生量：`v` 是大节点、子树没有函数与句柄时查缓存（没有就 `算` 一次并记下），否则直接 `算`。
fn 取<T: Clone>(
    v: &Value,
    可缓存: impl Fn(子树标记) -> bool,
    读: impl Fn(&条目) -> Option<T>,
    写: impl Fn(&mut 条目, T),
    算: impl FnOnce() -> T,
) -> T {
    if 身份(v).is_none() || !可缓存(标记(v)) {
        return 算();
    }
    if let Some(x) = 用条目(v, |e| 读(e)).flatten() {
        return x;
    }
    let x = 算();
    用条目(v, |e| 写(e, x.clone()));
    x
}

/// 缓存的来源标签（`Value::prov` 用）。
/// 缓存条件：子树没有句柄（出口的账本键、惰性出口与生成的取回都会改变来源；函数值的来源恒为可信，不影响）。
pub fn 来源(v: &Value, 算: impl FnOnce() -> Provenance) -> Provenance {
    取(v, |m| !m.有句柄, |e| e.来源.clone(), |e, x| e.来源 = Some(x), 算)
}

/// 缓存的帧实参指纹哈希（G2 `实参哈希` 用）。
/// 缓存条件：子树没有函数值（闭包的 `to_json` 带捕获环境的名字，环境会长出新名字）。句柄的帧指纹只取身份
/// （生成号、出口号与题、读数的账本键与题等不变字段），不随取回或消费变，可以缓存。
pub fn 指纹哈希(v: &Value, 算: impl FnOnce() -> String) -> String {
    取(v, |m| !m.有函数, |e| e.指纹哈希.clone(), |e, x| e.指纹哈希 = Some(x), 算)
}

/// 缓存的单元值哈希（单元键用；`None` = 算不出）。
/// 子树有句柄时单元值哈希一定算不出（句柄不可键），直接给 `None`，不走子树；否则在子树没有函数值时缓存。
pub fn 单元哈希(v: &Value, 算: impl FnOnce() -> Option<String>) -> Option<String> {
    if 身份(v).is_some() && 标记(v).有句柄 {
        return None;
    }
    取(v, |m| !m.有函数, |e| e.单元哈希.clone(), |e, x| e.单元哈希 = Some(x), 算)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn 大表(n: usize) -> Value {
        Value::list((0..n).map(|i| Value::Int(i as i64, Taint::Untrusted.into())).collect())
    }

    #[test]
    fn 同一份大值只算一次_小值不缓存() {
        let v = 大表(100);
        let mut n = 0;
        for _ in 0..5 {
            let _ = 指纹哈希(&v, || {
                n += 1;
                "h".into()
            });
        }
        assert_eq!(n, 1, "同一 Rc 只算一次");
        let 同内容另一份 = 大表(100);
        let mut m = 0;
        let _ = 指纹哈希(&同内容另一份, || {
            m += 1;
            "h".into()
        });
        assert_eq!(m, 1, "另一份值另算（按身份不按内容）");
        let 小 = 大表(1);
        let mut k = 0;
        for _ in 0..3 {
            let _ = 指纹哈希(&小, || {
                k += 1;
                "h".into()
            });
        }
        assert_eq!(k, 3, "小节点不缓存");
    }

    #[test]
    fn 值释放后地址复用不撞缓存() {
        let mut 算过 = 0;
        for _ in 0..50 {
            let v = 大表(64);
            let _ = 指纹哈希(&v, || {
                算过 += 1;
                "h".into()
            });
            drop(v);
        }
        assert_eq!(算过, 50, "每一份新值都重算（旧条目的弱引用已失效）");
    }
}
