//! C1 的单元测试：原型乙机制测试的移植（预注册 `地基/过程记录/工程-C1-单元图.md` 第四节，编号 C1-n）。
//!
//! 值类型用本模块的小类型 `TV`；判断器替身默认恒答 0.5（是非题多数块并列 → `Unsure(tie)`）。
//! 原型里的重放断言归 C3，这里改为「种子调度跑到静止一致」与「关掉机制的反证」两类（预注册第四节 (a)(b)）。

use crate::graph::Knobs;
use crate::*;
use jpp_ir::key::hash_of;
use std::collections::BTreeMap;
use std::rc::Rc;

mod c2b;
mod c2c;
mod claim;
mod duty;
mod early;
mod review;
mod takeover;
mod z0419;

// ───────────────────────── 测试值 ─────────────────────────

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum TExit {
    Act,
    Ignore,
    Unsure(UnsureCause, Option<DebtTok>),
}

impl TExit {
    pub(crate) fn name(&self) -> String {
        match self {
            TExit::Act => "act".into(),
            TExit::Ignore => "ignore".into(),
            TExit::Unsure(c, _) => format!("unsure:{}", c.name()),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum TV {
    Null,
    Bool(bool),
    Int(i64),
    Str(String),
    Reading(f64),
    Exit(TExit),
    List(Vec<TV>),
    Map(BTreeMap<String, TV>),
}

impl TV {
    pub(crate) fn s(x: &str) -> TV {
        TV::Str(x.to_string())
    }
    pub(crate) fn map(kv: Vec<(&str, TV)>) -> TV {
        TV::Map(kv.into_iter().map(|(k, v)| (k.to_string(), v)).collect())
    }
    pub(crate) fn strs(xs: &[String]) -> TV {
        TV::List(xs.iter().map(|x| TV::Str(x.clone())).collect())
    }
    pub(crate) fn get(&self, k: &str) -> TV {
        match self {
            TV::Map(m) => m.get(k).cloned().unwrap_or(TV::Null),
            _ => TV::Null,
        }
    }
    /// 规范文本：记号不进（记号只认欠账，不是值的一部分）。
    fn canon(&self) -> String {
        match self {
            TV::Null => "null".into(),
            TV::Bool(b) => b.to_string(),
            TV::Int(i) => i.to_string(),
            TV::Str(s) => format!("{s:?}"),
            TV::Reading(p) => format!("reading({p})"),
            TV::Exit(e) => format!("exit({})", e.name()),
            TV::List(xs) => format!(
                "[{}]",
                xs.iter().map(|x| x.canon()).collect::<Vec<_>>().join(",")
            ),
            TV::Map(m) => format!(
                "{{{}}}",
                m.iter()
                    .map(|(k, v)| format!("{k:?}:{}", v.canon()))
                    .collect::<Vec<_>>()
                    .join(",")
            ),
        }
    }
    /// 从读多写者单元得到的东西换成值（冲突值读者看得见）。
    pub(crate) fn from_shared(r: SharedRead<TV>) -> TV {
        match r {
            SharedRead::Empty => TV::Null,
            SharedRead::One(v) => v,
            SharedRead::Many(xs) => TV::map(vec![(
                "conflict",
                TV::List(
                    xs.into_iter()
                        .map(|(w, v)| TV::map(vec![("writer", TV::Str(w)), ("value", v)]))
                        .collect(),
                ),
            )]),
        }
    }
}

impl CellValue for TV {
    fn content_hash(&self) -> String {
        hash_of(&[&self.canon()])
    }
    fn unsure_exits(&self, out: &mut Vec<(Option<DebtTok>, UnsureCause)>) {
        match self {
            TV::Exit(TExit::Unsure(c, t)) => out.push((t.clone(), *c)),
            TV::List(xs) => xs.iter().for_each(|x| x.unsure_exits(out)),
            TV::Map(m) => m.values().for_each(|x| x.unsure_exits(out)),
            _ => {}
        }
    }
}

// ───────────────────────── 程序与判断的小工具 ─────────────────────────

pub(crate) struct P<F>(F);
impl<F: Fn(&mut Ctx<'_, TV>) -> Result<TV, TV>> Program<TV> for P<F> {
    fn attempt(&self, c: &mut Ctx<'_, TV>) -> Result<TV, TV> {
        (self.0)(c)
    }
}
pub(crate) fn prog(
    f: impl Fn(&mut Ctx<'_, TV>) -> Result<TV, TV> + 'static,
) -> Rc<dyn Program<TV>> {
    Rc::new(P(f))
}

/// 过桥（替身）：是非题按多数块，0.5 为并列；「未问」得那个原因。未决记一笔欠账，出口带记号。还没答返回 `Pend`。
pub(crate) fn cut(c: &mut Ctx<'_, TV>, key: &str) -> Result<TExit, Pend> {
    match c.judge(key) {
        JudgeRead::Answered(TV::Reading(p)) if p > 0.5 => Ok(TExit::Act),
        JudgeRead::Answered(TV::Reading(p)) if p < 0.5 => Ok(TExit::Ignore),
        JudgeRead::Answered(_) => {
            let t = c.owe(BridgeKind::Cut, key, UnsureCause::Tie);
            Ok(TExit::Unsure(UnsureCause::Tie, Some(t)))
        }
        JudgeRead::Unasked(cause) => {
            let t = c.owe(BridgeKind::Cut, key, cause);
            Ok(TExit::Unsure(cause, Some(t)))
        }
        JudgeRead::Pending => Err(Pend),
    }
}

/// 对一个未决出口记去向（不是未决就什么都不做）。
pub(crate) fn duty(c: &mut Ctx<'_, TV>, e: TExit, kind: DutyKind, detail: &str) {
    if let TExit::Unsure(cause, t) = e {
        c.duty(t, cause, kind, detail);
    }
}

pub(crate) fn wait() -> TV {
    TV::s("等")
}

/// 判断器替身：恒答 0.5。
pub(crate) fn half(_k: &str, _r: &[String]) -> FlushAnswer<TV> {
    FlushAnswer::Answer(TV::Reading(0.5))
}

pub(crate) fn host(g: &mut CellGraph<TV>, k: &str, v: TV) {
    g.host_event(vec![(k.to_string(), v)]);
}

/// 调度：`None` 按固定次序（先尝试，没得尝试再刷新）；`Some(种子)` 在可尝试的程序与刷新之间随机选。
pub(crate) fn settle(
    g: &mut CellGraph<TV>,
    seed: Option<u64>,
    decide: &mut dyn FnMut(&str, &[String]) -> FlushAnswer<TV>,
) {
    let Some(seed) = seed else {
        g.quiesce(decide);
        return;
    };
    let mut x = seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1;
    let mut steps = 0;
    loop {
        let mut opts: Vec<Option<String>> = g.runnable().into_iter().map(Some).collect();
        if g.has_pending() {
            opts.push(None);
        }
        if opts.is_empty() {
            break;
        }
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        match opts[(x % opts.len() as u64) as usize].clone() {
            Some(id) => g.attempt(&id),
            None => g.flush(decide),
        }
        steps += 1;
        assert!(steps < 10_000, "调度停不下来");
    }
}

/// 建图、依次施加宿主事件，每件之后跑到静止。
pub(crate) fn run(
    setup: &dyn Fn(&mut CellGraph<TV>),
    knobs: Knobs,
    events: &[(&str, TV)],
    seed: Option<u64>,
    decide: &mut dyn FnMut(&str, &[String]) -> FlushAnswer<TV>,
) -> CellGraph<TV> {
    let mut g = CellGraph::new();
    g.knobs = knobs;
    setup(&mut g);
    for (k, v) in events {
        host(&mut g, k, v.clone());
        settle(&mut g, seed, decide);
    }
    g
}

/// 各程序最后一版的（状态, 值哈希, 原因）：比「静止状态相同」用。
pub(crate) fn finals(g: &CellGraph<TV>) -> BTreeMap<String, (String, String, Vec<String>)> {
    g.programs()
        .iter()
        .filter_map(|id| {
            g.latest(id).map(|p| {
                (
                    id.clone(),
                    (
                        p.state.name().to_string(),
                        p.hash.clone(),
                        p.causes.iter().map(|c| c.name().to_string()).collect(),
                    ),
                )
            })
        })
        .collect()
}

/// 违规的可比形态（去掉程序名与尝试号）：主人、过桥、第几次、原因、题。
pub(crate) fn viol_key(v: &Violation) -> String {
    let d = &v.debt;
    format!(
        "{}·{}#{}（{}）q={} from={}",
        d.owner,
        d.bridge.map(|b| b.tag()).unwrap_or("承接"),
        d.nth,
        d.cause.name(),
        d.question_key,
        d.from.as_deref().unwrap_or("")
    )
}
