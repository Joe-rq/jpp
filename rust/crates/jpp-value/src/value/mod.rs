//! 运行期值。`Mat`（材料）与 `Reading`（读数）是不同变体：读数只能经 `cut` 离开（J-01）。
//! 函数值带显式环境链 `Env`，没有 Rust 闭包，可打印、可序列化成名字→值。

use std::cell::{Cell, RefCell};
use std::collections::{BTreeMap, BTreeSet};
use std::rc::Rc;

use serde::{Deserialize, Serialize};
use serde_json::{Value as Json, json};

use jpp_ir::ir::Span;

pub use crate::guard_ev::GuardEv;
// 闭包持有 IR 函数（步 12c：运行时读 IR）
use jpp_ir::ir::Function;

pub use jpp_ir::key::{LineGrade, canon, hash_of};

pub use crate::prov::{Edge, EdgeKind, Provenance, Sources, join as prov_join};
pub use jpp_ir::question_kind::{
    OnShape, OverKind, OverShape, QuestionKind, Request, SlotDecls, SlotShape, question_kind,
};

// 步 36 G3：按类型拆成子模块（只搬不改），`jpp_value::value::` 下的路径照旧
mod env;
/// 按值身份的缓存（Z0882）
pub mod ident_cache;
mod exit;
mod material;
mod question;
mod reading;
mod state;
pub use env::*;
pub use exit::*;
pub use material::*;
pub use question::*;
pub use reading::*;
pub use state::*;

pub use jpp_ir::key::Op;

#[derive(Clone, Debug)]
pub enum Value {
    Unit,
    /// 宿主标量各带一位 taint（B33，`12` §2.11「宿主内传播」）：含义与材料相同，
    /// trusted = 有人为其内容担保。语法字面量求值即 trusted；从 untrusted 材料读出的叶子
    /// 为 untrusted；内置与运算符的输出取 ∨ 输入；容器不带位，由 `taint_of` 递归取 ∨。
    /// 步 17c（B84）起第二字段是来源标签 `Provenance = (taint, sources)`，taint 分量的规则同 B33。
    Int(i64, Provenance),
    Float(f64, Provenance),
    /// 第三字段是守卫证据（J-08，`20` v2 §3.1；步 16），不参与相等与序列化。
    Bool(bool, Provenance, GuardEv),
    Text(Rc<str>, Provenance),
    List(Rc<Vec<Value>>),
    Record(Rc<Vec<(String, Value)>>),
    Fn(Rc<Closure>),
    Builtin(&'static str),
    Mat(Rc<Mat>),
    State(Rc<State>),
    Question(Rc<Question>),
    /// 题式（带槽的题模板）
    Form(Rc<Form>),
    Reading(Rc<Reading>),
    /// 声明式拟合的结果（B153 (2)，步 20j-4）：不透明句柄，只能进 `cut`（声明线）或同拟合的 `order`。
    /// 拟合出的数不在值上，在运行时的私有表里（与读数的答案同一结构）；程序读不出它（J-01 型面不变）。
    Score(Rc<Score>),
    Exit(Rc<Exit>),
    /// 惰性过桥（B94，步 23c）：`cut` 只把读数与线绑定，出口在第一次被检视时才解析。
    /// 运行时在检视点（内置与构造的实参、`if` 条件、运算、取字段、函数与程序返回）把它换成 `Exit`；
    /// 解析一次、缓存出口，复制出去的各份共享同一个出口（同一份责任）。
    Cut(Rc<PendingCut>),
    /// 惰性生成值（B149，步 15h-2）：`gen` 在调用点只把调用交给生成端口（非阻塞），结果在第一次被检视时
    /// 才取回并记账；检视点与 `Cut` 相同。解析一次、缓存结果，复制出去的各份共享同一个结果。
    Gen(Rc<PendingGen>),
    /// 未决责任 `U(q)`：`handle` 的 unsure 臂收到的就是它。不可伪造（只能由 handle 交付）、
    /// 不能默默变成材料或 JSON 就算销账。与出口共享同一个 `Rc<Exit>`，销账记录是同一份。
    Duty(Rc<Exit>),
    /// `do` 的失败值（J-12）。**失败信息也是外部世界的输出**：它的 taint 取产生它的动作的
    /// 输出 taint（`fail()` 由程序自己写，trusted）。此前没有这一位，装进材料时一律 trusted，
    /// 不可信动作的失败值因此能放行不可逆 do（K-182 / K-203，2026-09-23 修）。
    Fail(Rc<str>, Provenance),
    /// `stop(v)`：有界循环的显式停止
    Stop(Rc<Value>),
}

impl Value {
    /// 可信文本（程序自己造的）
    pub fn text(s: &str) -> Value {
        Value::Text(Rc::from(s), Provenance::trusted())
    }
    pub fn int(i: i64) -> Value {
        Value::Int(i, Provenance::trusted())
    }
    pub fn float(f: f64) -> Value {
        Value::Float(f, Provenance::trusted())
    }
    pub fn bool(b: bool) -> Value {
        Value::Bool(b, Provenance::trusted(), GuardEv::EMPTY)
    }
    /// 这个值携带的来源标签（B84）：标量取自身；容器递归 join；材料取 `(taint, from_key)`；
    /// 出口取 `(taint, {账本键})`；题取 `(trusted, from_key)`；其余 `(trusted, ∅)`。
    /// taint 分量与 [`Value::taint`] 逐值相同（B33 第 1 点）。
    pub fn prov(&self) -> Provenance {
        match self {
            Value::Int(_, p) | Value::Float(_, p) | Value::Bool(_, p, _) | Value::Text(_, p) => {
                p.clone()
            }
            Value::Fail(_, p) => p.clone(),
            Value::Mat(m) => m.prov(),
            // Z0882：大的、子树没有函数与句柄的列表与记录按身份缓存并集（值不可变，结果与重算相同）
            Value::List(l) => ident_cache::来源(self, || {
                ident_cache::记访问();
                l.iter().fold(Provenance::trusted(), |a, x| prov_join(&a, &x.prov()))
            }),
            Value::Record(fs) => ident_cache::来源(self, || {
                ident_cache::记访问();
                fs.iter().fold(Provenance::trusted(), |a, (_, x)| prov_join(&a, &x.prov()))
            }),
            // B92：出口读出是值依赖边
            Value::Exit(e) => {
                Provenance::new(e.taint, Sources::value(&e.ledger_key.borrow(), &e.q_hash))
            }
            // 未解析的出口（B94）：解析后取出口的；解析前取读数的 taint 与同一条值依赖边
            Value::Cut(c) => match c.exit() {
                Some(e) => Value::Exit(e).prov(),
                None => Provenance::new(
                    c.reading.state_taint,
                    Sources::value(&c.reading.ledger_key, &c.reading.q_hash),
                ),
            },
            // 未取回的生成（步 15h-2）：取回后按结果；取回前保守答 untrusted
            Value::Gen(g) => g
                .value()
                .map_or(Provenance::from(Taint::Untrusted), |v| v.prov()),
            // B58（步 17b）：题带题面 taint
            Value::Question(q) => Provenance::new(q.taint, Sources::from_set(q.from_key.clone())),
            // B153 (2)：拟合的 taint = ∨ 各输入读数的状态 taint ∨ extra 的 taint
            Value::Score(s) => Provenance::from(s.taint),
            Value::Form(f) => Provenance::from(f.taint),
            Value::Stop(x) => x.prov(),
            _ => Provenance::trusted(),
        }
    }
    /// 这个值自带的守卫证据：`Bool` 取自身，其他为空（B121-1）。
    pub fn guard_ev(&self) -> GuardEv {
        match self {
            Value::Bool(_, _, g) => *g,
            _ => GuardEv::EMPTY,
        }
    }
    /// 把守卫证据并进值里每个 `Bool` 叶子（递归 `List`、`Record`，其他原样）。
    /// 只由 `handle` 分派调用：臂返回值带分派出口的证据（B121-1）。
    pub fn stamp(self, ev: GuardEv) -> Value {
        if ev.is_empty() {
            return self;
        }
        match self {
            Value::Bool(b, p, g) => Value::Bool(b, p, g.join(ev)),
            Value::List(l) => Value::list(l.iter().cloned().map(|x| x.stamp(ev)).collect()),
            Value::Record(r) => {
                Value::record(r.iter().cloned().map(|(k, v)| (k, v.stamp(ev))).collect())
            }
            other => other,
        }
    }
    /// 把标签 join 进值（B33 `tainted` 的推广）。taint 分量进标量叶子与题、题式的题面 taint（B58，步 17b；
    /// 材料的位不动）；sources 分量进标量叶子、材料的 `from_key`、题的 `from_key`。单位元原样返回。
    pub fn with_prov(self, p: &Provenance) -> Value {
        if p.is_unit() {
            return self;
        }
        match self {
            Value::Int(i, q) => Value::Int(i, prov_join(&q, p)),
            Value::Float(f, q) => Value::Float(f, prov_join(&q, p)),
            Value::Bool(b, q, g) => Value::Bool(b, prov_join(&q, p), g),
            Value::Text(s, q) => Value::Text(s, prov_join(&q, p)),
            Value::List(l) => Value::list(l.iter().cloned().map(|x| x.with_prov(p)).collect()),
            Value::Record(r) => Value::record(
                r.iter()
                    .cloned()
                    .map(|(k, v)| (k, v.with_prov(p)))
                    .collect(),
            ),
            Value::Mat(m) if !p.sources.is_empty() => {
                Value::Mat(Rc::new((*m).clone().with_sources(&p.sources)))
            }
            Value::Question(q) => {
                let mut q = (*q).clone();
                q.from_key.extend(p.sources.iter().cloned());
                q.taint = Taint::join(q.taint, p.taint);
                Value::Question(Rc::new(q))
            }
            Value::Form(f) if p.taint == Taint::Untrusted => {
                let mut f = (*f).clone();
                f.taint = Taint::Untrusted;
                Value::Form(Rc::new(f))
            }
            other => other,
        }
    }
    /// 这个值携带的 taint：标量取自身位，容器递归取 ∨，材料 / 出口 / 失败值取其位；
    /// 其余（函数、题、状态……）是程序自己造的，按 trusted（B33 第 1 点）。
    pub fn taint(&self) -> Taint {
        match self {
            Value::Int(_, t) | Value::Float(_, t) | Value::Bool(_, t, _) | Value::Text(_, t) => {
                t.taint
            }
            Value::Mat(m) => m.taint,
            Value::List(l) => l
                .iter()
                .fold(Taint::Trusted, |t, x| Taint::join(t, x.taint())),
            Value::Record(fs) => fs
                .iter()
                .fold(Taint::Trusted, |t, (_, x)| Taint::join(t, x.taint())),
            Value::Exit(e) => e.taint,
            Value::Cut(c) => c.exit().map_or(c.reading.state_taint, |e| e.taint),
            Value::Gen(g) => g.value().map_or(Taint::Untrusted, |v| v.taint()),
            Value::Stop(x) => x.taint(),
            Value::Fail(_, t) => t.taint,
            Value::Question(q) => q.taint,
            Value::Form(f) => f.taint,
            _ => Taint::Trusted,
        }
    }
    /// 把 `t` ∨ 进所有标量叶子（容器递归）。`t` 为 trusted 时原样返回。
    /// 用于读出规则（从 untrusted 材料读出的全部叶子标 untrusted）与显式数据流（输出 ∨ 输入）。
    pub fn tainted(self, t: Taint) -> Value {
        if t == Taint::Trusted {
            return self;
        }
        // B84：只改 taint 分量，sources 保留
        let set = |q: Provenance| Provenance::new(t, q.sources);
        match self {
            Value::Int(i, q) => Value::Int(i, set(q)),
            Value::Float(f, q) => Value::Float(f, set(q)),
            Value::Bool(b, q, g) => Value::Bool(b, set(q), g),
            Value::Text(s, q) => Value::Text(s, set(q)),
            Value::List(l) => Value::list(l.iter().cloned().map(|x| x.tainted(t)).collect()),
            Value::Record(r) => {
                Value::record(r.iter().cloned().map(|(k, v)| (k, v.tainted(t))).collect())
            }
            other => other,
        }
    }
    pub fn list(v: Vec<Value>) -> Value {
        Value::List(Rc::new(v))
    }
    pub fn record(v: Vec<(String, Value)>) -> Value {
        Value::Record(Rc::new(v))
    }
    pub fn type_name(&self) -> &'static str {
        match self {
            Value::Unit => "Unit",
            Value::Int(_, _) => "Int",
            Value::Float(_, _) => "Float",
            Value::Bool(_, _, _) => "Bool",
            Value::Text(_, _) => "Text",
            Value::List(_) => "List",
            Value::Record(_) => "Record",
            Value::Fn(_) => "Fn",
            Value::Builtin(_) => "Builtin",
            Value::Mat(_) => "Mat",
            Value::State(_) => "State",
            Value::Question(_) => "Question",
            Value::Form(_) => "Form",
            Value::Reading(_) => "Reading",
            Value::Score(_) => "Score",
            Value::Exit(_) | Value::Cut(_) => "Exit",
            Value::Gen(_) => "List",
            Value::Duty(_) => "Unsure",
            Value::Fail(..) => "Fail",
            Value::Stop(_) => "Stop",
        }
    }
    pub fn get(&self, key: &str) -> Option<Value> {
        match self {
            Value::Record(r) => r.iter().find(|(k, _)| k == key).map(|(_, v)| v.clone()),
            _ => None,
        }
    }
    /// 可打印 / 可序列化形式。读数只露元数据不露概率（读数不能被宿主当数用，J-01）。
    pub fn to_json(&self) -> Json {
        match self {
            Value::Unit => Json::Null,
            Value::Int(i, _) => json!(i),
            Value::Float(f, _) => json!(f),
            Value::Bool(b, _, _) => json!(b),
            Value::Text(s, _) => json!(s.as_ref()),
            Value::List(l) => Json::Array(l.iter().map(|v| v.to_json()).collect()),
            Value::Record(r) => {
                let mut m = serde_json::Map::new();
                for (k, v) in r.iter() {
                    m.insert(k.clone(), v.to_json());
                }
                Json::Object(m)
            }
            Value::Fn(c) => {
                json!({"fn": c.name, "params": c.function.parameters.iter().map(|p| p.name.clone()).collect::<Vec<_>>(), "env": env_names(&c.env), "hash": c.hash})
            }
            Value::Builtin(n) => json!({"builtin": n}),
            Value::Mat(m) => {
                json!({"mat": m.hash, "content": m.content, "taint": m.taint, "origin": m.origin})
            }
            Value::State(s) => json!({"state": s.hash, "slots": s.to_json(), "taint": s.taint}),
            Value::Question(q) => {
                json!({"question": q.hash, "op": q.op.phys(), "text": q.text, "calib": q.calib})
            }
            Value::Form(f) => {
                json!({"form": f.hash, "op": f.op.phys(), "template": f.template, "slots": f.slots, "calib": f.calib})
            }
            Value::Reading(r) => {
                json!({"reading": r.ledger_key, "q": r.q_hash, "state": r.state_hash, "op": r.op.phys()})
            }
            // 只露拟合的身份，不露数（B153：Score 不能读出为数）
            Value::Score(s) => json!({"score": s.fit_hash, "inputs": s.inputs}),
            Value::Exit(e) => {
                json!({"exit": e.label(), "id": e.id, "consumed": e.consumed.get(), "q": e.q_hash})
            }
            Value::Cut(c) => match c.exit() {
                Some(e) => Value::Exit(e).to_json(),
                None => json!({"exit": "unresolved", "id": c.id, "q": c.reading.q_hash}),
            },
            Value::Gen(g) => match g.value() {
                Some(v) => v.to_json(),
                None => json!({"gen_pending": g.id}),
            },
            Value::Duty(e) => json!({"unsure": e.cause(), "duty": e.id, "q": e.q_hash}),
            Value::Fail(s, _) => json!({"fail": s.as_ref()}),
            Value::Stop(v) => json!({"stop": v.to_json()}),
        }
    }
    /// 这个值可比吗；`None` = 里面有读数（J-01）。容器要递归看，装进列表或记录不改变这件事。
    pub fn comparable(&self) -> Option<()> {
        match self {
            Value::Reading(_) => None,
            Value::List(l) => l.iter().try_for_each(|x| x.comparable()),
            Value::Record(fs) => fs.iter().try_for_each(|(_, v)| v.comparable()),
            _ => Some(()),
        }
    }

    /// 结构相等（函数按哈希）；读数不可比（返回 None，调用方报 J-01）
    pub fn equals(&self, other: &Value) -> Option<bool> {
        Some(match (self, other) {
            (Value::Reading(_), _) | (_, Value::Reading(_)) => return None,
            (Value::Unit, Value::Unit) => true,
            (Value::Int(a, _), Value::Int(b, _)) => a == b,
            (Value::Float(a, _), Value::Float(b, _)) => a == b,
            (Value::Int(a, _), Value::Float(b, _)) | (Value::Float(b, _), Value::Int(a, _)) => {
                (*a as f64) == *b
            }
            (Value::Bool(a, _, _), Value::Bool(b, _, _)) => a == b,
            (Value::Text(a, _), Value::Text(b, _)) => a == b,
            // 容器里的「不可比」要传上来，不能被 `== Some(true)` 悄悄吃成 false：
            // 读数装进列表或记录还是读数，没有可读的值（J-01）。
            (Value::List(a), Value::List(b)) => {
                if a.iter().chain(b.iter()).any(|x| x.comparable().is_none()) {
                    return None;
                }
                a.len() == b.len()
                    && a.iter()
                        .zip(b.iter())
                        .all(|(x, y)| x.equals(y) == Some(true))
            }
            (Value::Record(a), Value::Record(b)) => {
                if a.iter()
                    .chain(b.iter())
                    .any(|(_, v)| v.comparable().is_none())
                {
                    return None;
                }
                a.len() == b.len()
                    && a.iter().all(|(k, v)| {
                        b.iter()
                            .any(|(k2, v2)| k == k2 && v.equals(v2) == Some(true))
                    })
            }
            (Value::Mat(a), Value::Mat(b)) => a.hash == b.hash,
            (Value::State(a), Value::State(b)) => a.hash == b.hash,
            (Value::Question(a), Value::Question(b)) => a.hash == b.hash,
            (Value::Form(a), Value::Form(b)) => a.hash == b.hash,
            (Value::Exit(a), Value::Exit(b)) => a.kind == b.kind,
            // 运行时在运算前已把未解析出口解析掉；这里只剩已解析的
            (Value::Cut(a), _) => Value::Exit(a.exit()?).equals(other)?,
            (_, Value::Cut(b)) => self.equals(&Value::Exit(b.exit()?))?,
            // 运行时在运算前已把生成取回；这里只剩已取回的
            (Value::Gen(a), _) => a.value()?.equals(other)?,
            (_, Value::Gen(b)) => self.equals(&b.value()?)?,
            // 责任按身份比：同一道题的两个未决是两份责任
            (Value::Duty(a), Value::Duty(b)) => a.id == b.id,
            (Value::Fn(a), Value::Fn(b)) => a.hash == b.hash,
            (Value::Builtin(a), Value::Builtin(b)) => a == b,
            (Value::Fail(a, _), Value::Fail(b, _)) => a == b,
            _ => false,
        })
    }
}

#[cfg(test)]
mod form_tests {
    use super::*;

    #[test]
    fn 填法必须恰好填满槽_同题面即同一道题() {
        let f = Form::new(
            Op::Test,
            "{a} 是否早于 {b}？",
            "k",
            vec![],
            vec![],
            None,
            None,
        )
        .unwrap();
        assert_eq!(f.slots, vec!["a", "b"]);
        let fill = |xs: &[(&str, &str)]| {
            f.fill(
                &xs.iter()
                    .map(|(k, v)| (k.to_string(), v.to_string()))
                    .collect::<Vec<_>>(),
            )
        };
        assert!(fill(&[("a", "周一")]).unwrap_err().contains("槽 b 没有填"));
        assert!(
            fill(&[("a", "周一"), ("b", "周二"), ("c", "x")])
                .unwrap_err()
                .contains("没有槽 c")
        );
        let q = fill(&[("b", "周二"), ("a", "周一")]).unwrap();
        assert_eq!(q.text, "周一 是否早于 周二？");
        assert_eq!(q.fill.as_ref().unwrap()[0].0, "a");
        // 题式来源不进题哈希：同题面同题型就是同一道题（账本键与校准键不因写法不同而分裂）
        assert_eq!(
            q.hash,
            Question::new(Op::Test, "周一 是否早于 周二？", "k", vec![]).hash
        );
        assert!(Form::slots_of("未闭合 {a").is_err());
    }

    /// B155（步 15i）：线上状态不含 `over`；材料哈希只随 `on`/`ctx`/`ref` 变；`StateHash` 仍含 `over`、
    /// 算法不变（钉一个现值，账本键与夹具命中靠它）。
    #[test]
    fn 线上形状与材料哈希() {
        let m = |x: &str| Mat::literal(json!(x));
        let a = State::new(
            vec![m("材料")],
            vec![m("语境")],
            vec![],
            vec![m("甲"), m("乙")],
            false,
        );
        let b = State::new(
            vec![m("材料")],
            vec![m("语境")],
            vec![],
            vec![m("丙")],
            false,
        );
        let c = State::new(vec![m("材料")], vec![m("语境")], vec![], vec![], false);
        let d = State::new(
            vec![m("别的")],
            vec![m("语境")],
            vec![],
            vec![m("甲"), m("乙")],
            false,
        );
        assert_eq!(a.wire_json(), json!({"on": "材料", "ctx": ["语境"]}));
        assert_eq!(a.to_json()["over"], json!(["甲", "乙"]));
        assert_eq!(a.mat_hash(), b.mat_hash());
        assert_eq!(a.mat_hash(), c.mat_hash());
        assert_ne!(a.mat_hash(), d.mat_hash());
        assert_ne!(a.hash, b.hash, "StateHash 仍含 over");
        assert_eq!(a.hash, hash_of(&["state", &canon(&a.to_json())]));
        assert_eq!(
            c.hash,
            hash_of(&["state", &canon(&c.wire_json())]),
            "无 over 时两者同形"
        );
    }
}
