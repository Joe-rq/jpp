//! 解释器经单元图求值（步 41，C2b；`12` §2.13 R1、R2、B193、B199）：判断单元、账本求值体的代码单元、程序单元。
//!
//! 单元图开着时（缺省），同运行复用的两张表（判断按缓存键、生成与变换按效应缓存键）由单元存储取代：判断单元的键是
//! `judge_cell_key(CacheKey)`，即今天 `判断缓存键` 算的摘要；生成与变换的输出是求值体为账本条目的代码单元
//! （`effect_cell_key`）。跨运行缓存（`CacheLookup`）是单元存储的持久层，查找次序「本运行的单元 → 持久层」不变。
//! 一趟运行是程序单元的一次尝试：入口开新宿主纪元（Z0581、C2a 复核第 1 条）、开尝试，出口按 R2 发布（只在内存里，
//! 账本事件归 C3）。关掉时（`set_cells(false)`，H1a 对照臂）字段为 `None`，所有接点走 C2 以前的代码，不调 `jpp-cell`。
//! 预注册：`地基/过程记录/工程-C2-单元图求值.md` 第三节、附录一。

use super::*;
use jpp_cell::{
    Attempt, CellGraph, CellValue, CodeStep, Ctx, DebtTok, JudgeSeen, PState, UnsureCause,
};
use jpp_ir::cell_key::{MethodIdentity, code_cell_key};
use jpp_value::value::ident_cache;
use jpp_ir::cell_key::{ProgramIdentity, effect_cell_key, judge_cell_key, program_cell_key};
use jpp_ir::key::CacheKey;
use jpp_ledger::PermMeasure;

/// 一道题第一次有答案时记下的东西（同运行复用要照抄：复用条目的 `reused_from` 是第一次那条的账本键）。
pub(crate) struct 判断答 {
    pub 键: String,
    pub 答: Answer,
    pub 置换: Option<PermMeasure>,
    pub 置信: Option<f64>,
}

/// 单元里存的值（`jpp-cell` 不依赖 `jpp-value`，这里包一层）。
#[derive(Clone)]
pub(crate) enum 单元值 {
    判断(Rc<判断答>),
    /// 生成与变换：第一次那条的账本键
    效应(Rc<str>),
    /// 代码单元的记忆值（C2c）与它的内容哈希。值里的未决不报给单元图：同一趟内欠账由解释器自己的帧与出口记
    /// （G2），单元图的欠账表本步不镜像（第六节缺口第二条，C3 前提）
    值 {
        v: Value,
        哈希: String,
    },
    /// 程序单元的一版：返回值的规范哈希（并上 taint）与值里每个未决出口的记号与原因
    程序 {
        哈希: String,
        未决: Rc<Vec<(Option<DebtTok>, UnsureCause)>>,
    },
}

impl CellValue for 单元值 {
    fn content_hash(&self) -> String {
        match self {
            单元值::判断(j) => hash_of(&[
                "cell/judge-val",
                &serde_json::to_string(&j.答).unwrap_or_default(),
                &serde_json::to_string(&j.置换).unwrap_or_default(),
                &format!("{:?}", j.置信),
            ]),
            单元值::效应(k) => hash_of(&["cell/effect-val", k]),
            单元值::程序 { 哈希, .. } | 单元值::值 { 哈希, .. } => 哈希.clone(),
        }
    }
    fn unsure_exits(&self, out: &mut Vec<(Option<DebtTok>, UnsureCause)>) {
        if let 单元值::程序 { 未决, .. } = self {
            out.extend(未决.iter().cloned());
        }
    }
}

/// 一趟运行的单元图：图、这一趟的尝试状态、程序单元的键、这一趟已记过依赖的判断单元。
pub(crate) struct 单元图 {
    pub g: CellGraph<单元值>,
    a: Option<Attempt<单元值>>,
    pub 程序: Option<String>,
    已记依赖: HashSet<String>,
    /// 代码单元第一次求值的足迹是不是空的（附录二 A2.3）：键 → 空
    足迹空: HashMap<String, bool>,
    /// 闭包的纯性判定缓存：结构哈希加捕获指纹 → 纯
    纯性: HashMap<String, bool>,
    /// 代码单元首次求值往下走了几层（C2c 复核 K1）：键 → 层数
    层数: HashMap<String, u32>,
    计: 代码计数,
}

/// 代码单元的计数（C2c，`单元统计` 带出）。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct 代码计数 {
    命中: u64,
    计算: u64,
    不可键: u64,
    不纯: u64,
}

impl 单元图 {
    pub fn new() -> 单元图 {
        单元图 {
            g: CellGraph::new(),
            a: None,
            程序: None,
            已记依赖: HashSet::new(),
            足迹空: HashMap::new(),
            纯性: HashMap::new(),
            层数: HashMap::new(),
            计: 代码计数::default(),
        }
    }

    /// 在当前帧记一条判断依赖。程序帧里同一单元一趟只记一次；代码单元帧照记（帧各自一份）。没有打开的尝试
    /// （序言求值期间）不记。
    fn 记依赖(&mut self, key: &str, seen: JudgeSeen) {
        let Some(a) = self.a.take() else {
            return;
        };
        let mut c = Ctx::resume(&mut self.g, a);
        if c.in_code_frame() || self.已记依赖.insert(key.to_string()) {
            c.judge_dep(key, seen);
        }
        self.a = Some(c.suspend());
    }

    fn 开代码(&mut self, key: &str, force: bool) -> CodeStep<单元值> {
        let a = self.a.take().expect("有打开的尝试");
        let mut c = Ctx::resume(&mut self.g, a);
        let s = c.begin_code(key, force);
        self.a = Some(c.suspend());
        s
    }

    fn 收代码(&mut self, key: &str, r: Result<单元值, jpp_cell::Pend>) {
        let a = self.a.take().expect("有打开的尝试");
        let mut c = Ctx::resume(&mut self.g, a);
        let _ = c.end_code(key, r);
        self.a = Some(c.suspend());
    }

    /// 单元统计（给测试）：判断单元数、账本单元数、程序单元发布的版本数、宿主纪元、这一趟尝试的宿主纪元。
    pub fn 统计(&self) -> 单元统计 {
        let (版本, 状态, 尝试纪元) = match &self.程序 {
            Some(p) => (
                self.g.published(p).len(),
                self.g.latest(p).map(|x| x.state),
                self.g.attempts(p).last().map(|r| r.host_epoch),
            ),
            None => (0, None, None),
        };
        单元统计 {
            判断单元: self.g.judge_cells(),
            账本单元: self.g.ledger_cells(),
            发布版本: 版本,
            最新状态: 状态.map(PState::name),
            宿主纪元: self.g.host_epoch(),
            尝试宿主纪元: 尝试纪元,
            代码单元: self.g.code_cells(),
            代码命中: self.计.命中,
            代码计算: self.计.计算,
            不可键: self.计.不可键,
            不纯: self.计.不纯,
        }
    }
}

/// 单元图的只读统计（`Outcome.cells`，不进报告 JSON）。
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct 单元统计 {
    pub 判断单元: usize,
    pub 账本单元: usize,
    pub 发布版本: usize,
    pub 最新状态: Option<&'static str>,
    pub 宿主纪元: u32,
    pub 尝试宿主纪元: Option<u32>,
    /// 代码单元（C2c）：记忆表里的单元数、同一趟取记忆的次数、求值次数、实参或捕获算不出哈希的合格调用数、
    /// 函数体不纯的调用数
    pub 代码单元: usize,
    pub 代码命中: u64,
    pub 代码计算: u64,
    pub 不可键: u64,
    pub 不纯: u64,
}

impl<'a> Interp<'a> {
    /// 开关单元图（H1a 对照臂，`21` §六·5）：缺省开；关掉时不调用 `jpp-cell` 的任何接口。
    pub fn set_cells(&mut self, on: bool) {
        self.单元 = if on { Some(单元图::new()) } else { None };
    }

    /// 单元图开着吗。
    pub fn cells_on(&self) -> bool {
        self.单元.is_some()
    }

    /// 程序入口：开新宿主纪元（每趟一次，Z0581），登记程序单元，开这一趟的尝试。序言求值之后调。
    pub(crate) fn 单元开尝试(&mut self, program: &Program) {
        let Some(u) = self.单元.as_mut() else {
            return;
        };
        let j = serde_json::to_value(program).unwrap_or(Json::Null);
        let entry = hash_of(&["program", &jpp_ir::key::canon(&j)]);
        // 段编号：一次性程序的续跑是同一程序单元的下一次尝试（五十九第 8 条），本步单元图每趟新建，取 0
        let id = program_cell_key(&ProgramIdentity::OneShot { entry, segment: 0 }).id;
        u.g.host_event(vec![]);
        u.g.add_external_program(&id);
        u.a = Some(u.g.open(&id));
        u.程序 = Some(id);
    }

    /// 程序出口：有结论交返回值，挂起交 `None`（进行中，值为空），运行期错误交 `None` 且不算有结论。
    pub(crate) fn 单元结束尝试(&mut self, v: Option<&Value>) {
        let 值 = v.map(|v| self.程序单元值(v));
        let Some(u) = self.单元.as_mut() else {
            return;
        };
        let (Some(a), Some(id)) = (u.a.take(), u.程序.clone()) else {
            return;
        };
        let out = match 值 {
            Some(x) => Ok(x),
            None => Err(单元值::程序 {
                哈希: hash_of(&["cell/prog-val", "进行中"]),
                未决: Rc::new(vec![]),
            }),
        };
        u.g.close(&id, a, out);
    }

    fn 程序单元值(&self, v: &Value) -> 单元值 {
        let mut 未决 = vec![];
        visit_exits(v, &mut |e| {
            if let Some(w) = e.why() {
                未决.push((
                    Some(DebtTok(std::sync::Arc::from(self.记号(e).token()))),
                    w.cause,
                ));
            }
        });
        // G2 单次形态的违规（`12` R9）：这次结论为「未决（violation）」，值照带；`jpp-cell` 的帧本步不记欠账，按值里带着
        // 一项不对应欠账的 `violation` 交给发布判定
        if !self.违规.is_empty() {
            未决.push((None, UnsureCause::Violation));
        }
        let t = format!("{:?}", taint_of(v));
        let 哈希 = hash_of(&["cell/prog-val", &jpp_ir::key::canon(&v.to_json()), &t]);
        单元值::程序 {
            哈希,
            未决: Rc::new(未决),
        }
    }

    /// 一趟运行结束后的单元统计（单元图关着为 `None`）。
    pub(crate) fn 单元统计(&self) -> Option<单元统计> {
        self.单元.as_ref().map(|u| u.统计())
    }

    // ── 判断单元（取代 `ReuseState.judges`） ──

    /// 一道题有了答案：记进判断单元（先到先得）。返回 `false` 表示单元图关着，调用方走旧表。
    pub(crate) fn 单元记判断(
        &mut self,
        ck: &CacheKey,
        key: &str,
        a: &Answer,
        perm: Option<PermMeasure>,
        c: Option<f64>,
    ) -> bool {
        let Some(u) = self.单元.as_mut() else {
            return false;
        };
        let id = judge_cell_key(ck).id;
        let v = 单元值::判断(Rc::new(判断答 {
            键: key.to_string(),
            答: a.clone(),
            置换: perm,
            置信: c,
        }));
        u.g.record_answer(&id, v);
        true
    }

    /// 按缓存键摘要读判断单元：`Some(Some(..))` 命中，`Some(None)` 没有，`None` 单元图关着。
    #[allow(clippy::type_complexity)]
    pub(crate) fn 单元读判断(
        &mut self,
        ck: &CacheKey,
    ) -> Option<Option<(String, Answer, Option<PermMeasure>, Option<f64>)>> {
        let u = self.单元.as_mut()?;
        let id = judge_cell_key(ck).id;
        let hit = match u.g.answer(&id) {
            Some(单元值::判断(j)) => Some((j.键.clone(), j.答.clone(), j.置换, j.置信)),
            _ => None,
        };
        Some(hit)
    }

    /// 只查判断单元有没有（推测前用，不记依赖）。`None` 单元图关着。
    pub(crate) fn 单元有判断(&self, ck: &CacheKey) -> Option<bool> {
        let u = self.单元.as_ref()?;
        Some(u.g.answer(&judge_cell_key(ck).id).is_some())
    }

    // ── 账本求值体的代码单元（取代 `ReuseState.effects`） ──

    /// 生成或变换的一条结果进了账本：记进账本单元（先到先得）。返回 `false` 表示单元图关着。
    pub(crate) fn 单元记效应(&mut self, d: &str, key: &str) -> bool {
        let Some(u) = self.单元.as_mut() else {
            return false;
        };
        u.g.record_ledger_cell(&effect_cell_key(d).id, 单元值::效应(Rc::from(key)));
        true
    }

    /// 按效应缓存键摘要读账本单元：`Some(Some(第一次那条的账本键))` 命中，`Some(None)` 没有，`None` 单元图关着。
    pub(crate) fn 单元读效应(&self, d: &str) -> Option<Option<String>> {
        let u = self.单元.as_ref()?;
        Some(match u.g.ledger_cell(&effect_cell_key(d).id) {
            Some(单元值::效应(k)) => Some(k.to_string()),
            _ => None,
        })
    }

    /// 解释器的一次刷新结束：单元图的刷新纪元加一。
    pub(crate) fn 单元刷新(&mut self) {
        if let Some(u) = self.单元.as_mut() {
            u.g.advance_flush();
        }
    }
}

// ───────────────────────── 代码单元（C2c，附录二） ─────────────────────────

impl<'a> Interp<'a> {
    /// 判断登记时在当前帧记判断依赖（C2c 起记在登记处：答了的 `Answered`，进待发的 `Pending`）。
    pub(crate) fn 单元记题依赖(&mut self, key: &str, 已答: bool) {
        if self.单元.is_none() {
            return;
        }
        let Some((_, ck)) = self.判断缓存键_pub(key) else {
            return;
        };
        let id = judge_cell_key(&ck).id;
        let seen = if 已答 {
            JudgeSeen::Answered
        } else {
            JudgeSeen::Pending
        };
        if let Some(u) = self.单元.as_mut() {
            u.记依赖(&id, seen);
        }
    }

    /// 足迹（附录二 A2.3）：求值前后比较，任何一项变了就是重新求值会留下外部痕迹。
    fn 足迹快照(&self) -> [u64; 14] {
        [
            self.trace.events.len() as u64,
            self.trace.warnings.len() as u64,
            self.next_exit as u64,
            self.next_reading.get(),
            self.ledger.view().len() as u64,
            self.layers.len() as u64,
            self.pending.len() as u64,
            u64::from(self.有未交生成()),
            self.exit_grades.len() as u64,
            self.questions.len() as u64,
            self.默认链记录.len() as u64,
            self.违规.len() as u64,
            self.去向计数.len() as u64,
            self.丢失处.len() as u64,
        ]
    }

    /// 这次调用的代码单元键；不成单元（单元图关着、没有打开的尝试、拟合闭包里、不纯、不可键）为 `None`。
    fn 代码单元键(&mut self, c: &Rc<Closure>, args: &[Value]) -> Option<String> {
        let open = self.单元.as_ref().is_some_and(|u| u.a.is_some());
        if !open || self.拟合中 > 0 {
            return None;
        }
        let Some(captured) = self.单元捕获指纹(&c.env, &referenced_names(&c.function), 3) else {
            self.单元.as_mut().unwrap().计.不可键 += 1;
            return None;
        };
        // 纯性：函数体（按捕获环境解析名字）与实参里的每个函数值
        let pk = format!("{}\u{1f}{captured}", c.hash);
        let 体纯 = match self.单元.as_ref().unwrap().纯性.get(&pk) {
            Some(p) => *p,
            None => {
                let p =
                    jpp_ir::purity::cell_pure(&c.function, &crate::plan_view::RtEnv(c.env.clone()));
                self.单元.as_mut().unwrap().纯性.insert(pk, p);
                p
            }
        };
        let 实参纯 = jpp_ir::purity::fn_args_pure(
            &args
                .iter()
                .map(crate::plan_view::summary)
                .collect::<Vec<_>>(),
        );
        if !体纯 || !实参纯 {
            self.单元.as_mut().unwrap().计.不纯 += 1;
            return None;
        }
        let mut hs = Vec::with_capacity(args.len());
        for a in args {
            match self.值哈希(a) {
                Some(h) => hs.push(h),
                None => {
                    self.单元.as_mut().unwrap().计.不可键 += 1;
                    return None;
                }
            }
        }
        let m = MethodIdentity {
            source_hash: c.hash.clone(),
            captured,
            lib_version: self.复用.lib_version.clone(),
        };
        let refs: Vec<&str> = hs.iter().map(String::as_str).collect();
        Some(code_cell_key(&m, &refs).id)
    }

    /// 值的内容哈希（实参与记忆值；附录二 A2.2）：见 [`值哈希_用`]；函数值的捕获指纹按 [`Interp::单元捕获指纹`]（深度 3）。
    fn 值哈希(&self, v: &Value) -> Option<String> {
        值哈希_用(v, &|c| self.单元捕获指纹(&c.env, &referenced_names(&c.function), 3))
    }

    /// 代码单元键的捕获指纹（附录四）：遍历的名字、跳过与「不可键」的条件与 `env_fingerprint` 逐条相同（内置跳过；
    /// 未取回的生成、读数、出口、惰性出口、责任、状态、题 → 不可键；函数值递归），每个值用按身份缓存的 [`值哈希_用`]；
    /// 里面嵌着算不出哈希的东西（如列表里的读数）时退回规范 JSON（与 `env_fingerprint` 同口径，慢但罕见）。
    /// `env_fingerprint` 本身不改：它还给 `transform` 的效应键用，改它会动账本键。
    fn 单元捕获指纹(&self, env: &Env, names: &BTreeSet<String>, depth: u32) -> Option<String> {
        let mut parts: Vec<String> = vec![];
        for n in names {
            let Some(v) = env_lookup(env, n) else {
                continue;
            };
            match &v {
                Value::Builtin(_) => continue,
                Value::Fn(c) => {
                    if depth == 0 {
                        return None;
                    }
                    let inner = self.单元捕获指纹(&c.env, &referenced_names(&c.function), depth - 1)?;
                    parts.push(format!("{n}=fn:{}:{inner}", c.hash));
                }
                Value::Gen(g) if g.value().is_none() => return None,
                Value::Reading(_)
                | Value::Exit(_)
                | Value::Cut(_)
                | Value::Duty(_)
                | Value::State(_)
                | Value::Question(_) => return None,
                other => {
                    let 内容 = match other {
                        // 取回了的生成按结果算（与 `env_fingerprint` 的 `to_json` 同义）
                        Value::Gen(g) => g.value().and_then(|x| self.值哈希(&x)),
                        x => self.值哈希(x),
                    };
                    let h = 内容.unwrap_or_else(|| hash_of(&["cell/json", &canon(&other.to_json())]));
                    parts.push(format!("{n}={h}"));
                }
            }
        }
        Some(hash_of(&parts.iter().map(|s| s.as_str()).collect::<Vec<_>>()))
    }

    /// 对闭包的一次调用，经单元图（附录二 A2.3、A2.5）：成单元的调用按键记忆；同一趟内只有第一次求值足迹为空、
    /// 且此刻没有待发判断与未交生成时取记忆，否则重新求值并写回。不成单元的照原路径。
    pub(crate) fn call_closure(&mut self, c: &Rc<Closure>, args: Vec<Value>, sp: Span) -> R<Value> {
        let Some(key) = self.代码单元键(c, &args) else {
            return self.call_closure_body(c, args, sp);
        };
        // C2c 复核 K1：取记忆不经过体内各层的 J-06 深度核对，所以还要「当前深度 + 首次求值往下走的层数」不超过
        // 递归上限；超了就重算，重算时照常报 J-06（与关着单元图逐字相同）
        let 层数 = self.单元.as_ref().unwrap().层数.get(&key).copied();
        let 可取 = self.单元.as_ref().unwrap().足迹空.get(&key) == Some(&true)
            && self.pending.is_empty()
            && !self.有未交生成()
            && 层数.is_some_and(|k| self.depth + k <= self.递归上限());
        match self.单元.as_mut().unwrap().开代码(&key, !可取) {
            CodeStep::Hit(单元值::值 { v, .. }) => {
                self.单元.as_mut().unwrap().计.命中 += 1;
                // 命中也算「走到过这么深」：外层的层数按它抬
                let k = 层数.unwrap_or(0);
                self.深度峰 = self.深度峰.max(self.depth + k);
                return Ok(v);
            }
            CodeStep::Hit(_) => unreachable!("代码单元键只存 `值`"),
            CodeStep::Compute => {}
        }
        let 进入 = self.depth;
        let 外层峰 = std::mem::replace(&mut self.深度峰, 进入);
        let 前 = self.足迹快照();
        let r = self.call_closure_body(c, args, sp);
        let 后 = self.足迹快照();
        let 本层数 = self.深度峰 - 进入;
        self.深度峰 = 外层峰.max(self.深度峰);
        let 记 = match &r {
            Ok(v) => {
                let 哈希 = self.值哈希(v).unwrap_or_else(|| {
                    // 记忆值算不出哈希（带着出口、读数等，足迹必非空，不会被取用）：取一个只等于自己的哈希
                    let n = self.单元.as_ref().unwrap().计.计算;
                    hash_of(&["cell/val-unhashable", &key, &n.to_string()])
                });
                Ok(单元值::值 {
                    v: v.clone(), 哈希
                })
            }
            Err(_) => Err(jpp_cell::Pend),
        };
        let u = self.单元.as_mut().unwrap();
        u.计.计算 += 1;
        if 记.is_ok() {
            u.足迹空.insert(key.clone(), 前 == 后);
            u.层数.insert(key.clone(), 本层数);
        }
        u.收代码(&key, 记);
        r
    }
}

/// 值的内容哈希（附录二 A2.2）：规范文本并上来源标签与守卫证据；读数、打分、出口、惰性出口、未取回的生成、未决责任
/// 算不出（`None`）。不读惰性值。函数值的捕获指纹由调用方给（`指纹` 返回 `None` 即算不出）。
pub(crate) fn 值哈希_用(
    v: &Value,
    指纹: &dyn Fn(&Rc<Closure>) -> Option<String>,
) -> Option<String> {
    fn go(
        v: &Value,
        指纹: &dyn Fn(&Rc<Closure>) -> Option<String>,
        out: &mut String,
    ) -> Option<()> {
        // Z0882（附录四、五）：大节点在父节点里只写它自己的哈希，按身份缓存（子树有函数或句柄的不缓存、照算）。
        // 写不写成哈希只取决于节点的种类与大小，即只取决于内容，所以内容相同则哈希相同、与是否命中缓存无关
        if ident_cache::是大节点(v) {
            let h = ident_cache::单元哈希(v, || {
                let mut s = String::new();
                go_inline(v, 指纹, &mut s)?;
                Some(hash_of(&["cell/node", &s]))
            });
            out.push('#');
            out.push_str(&h?);
            return Some(());
        }
        go_inline(v, 指纹, out)
    }
    fn go_inline(
        v: &Value,
        指纹: &dyn Fn(&Rc<Closure>) -> Option<String>,
        out: &mut String,
    ) -> Option<()> {
        use std::fmt::Write;
        ident_cache::记访问_容器(v);
        match v {
            Value::Unit => out.push('u'),
            // 来源标签写成 taint 加来源集合的摘要（大集合按身份缓存，Z0882）：内容相同则相同
            Value::Int(i, p) => write!(out, "i{i}|{}", 来源文(p)).ok()?,
            Value::Float(x, p) => write!(out, "f{}|{}", x.to_bits(), 来源文(p)).ok()?,
            Value::Bool(b, p, g) => write!(out, "b{b}|{}|{g:?}", 来源文(p)).ok()?,
            Value::Text(t, p) => write!(out, "t{t:?}|{}", 来源文(p)).ok()?,
            Value::List(l) => {
                out.push('[');
                for x in l.iter() {
                    go(x, 指纹, out)?;
                    out.push(',');
                }
                out.push(']');
            }
            Value::Record(r) => {
                out.push('{');
                for (k, x) in r.iter() {
                    write!(out, "{k:?}:").ok()?;
                    go(x, 指纹, out)?;
                    out.push(',');
                }
                out.push('}');
            }
            Value::Fn(c) => {
                let cap = 指纹(c)?;
                write!(out, "fn{}|{cap}", c.hash).ok()?
            }
            Value::Builtin(n) => write!(out, "B{n}").ok()?,
            Value::Mat(m) => write!(out, "m{m:?}").ok()?,
            Value::State(s) => write!(out, "s{s:?}").ok()?,
            Value::Question(q) => write!(out, "q{q:?}").ok()?,
            Value::Form(f) => write!(out, "F{f:?}").ok()?,
            Value::Fail(t, p) => write!(out, "x{t:?}|{}", 来源文(p)).ok()?,
            Value::Stop(x) => {
                out.push('S');
                go(x, 指纹, out)?;
            }
            Value::Reading(_)
            | Value::Score(_)
            | Value::Exit(_)
            | Value::Cut(_)
            | Value::Gen(_)
            | Value::Duty(_) => {
                return None;
            }
        }
        Some(())
    }
    let mut s = String::new();
    go(v, 指纹, &mut s)?;
    Some(hash_of(&["cell/val", &s]))
}

/// 来源标签的文字（单元值哈希用）：taint 加来源集合的摘要。
fn 来源文(p: &jpp_value::prov::Provenance) -> String {
    format!("{:?}#{}", p.taint, ident_cache::来源集摘要(&p.sources))
}

#[cfg(test)]
mod tests {
    use super::*;
    use jpp_value::guard_ev::GuardEv;

    /// C2c 复核 K3：两个只差守卫证据的布尔，值哈希不同、代码单元键不同（守卫证据不参与值的相等与序列化，但决定
    /// J-08 放行，不进键就会让记忆值带着第一次调用的守卫证据）。
    #[test]
    fn k3_守卫证据不同_值哈希与代码单元键都不同() {
        let p = jpp_value::prov::Provenance::from(Taint::Trusted);
        let a = Value::Bool(true, p.clone(), GuardEv::EMPTY);
        let b = Value::Bool(
            true,
            p.clone(),
            GuardEv {
                from_releasing_judgement: true,
                via_ask: false,
            },
        );
        let c = Value::Bool(
            true,
            p,
            GuardEv {
                from_releasing_judgement: false,
                via_ask: true,
            },
        );
        let 无指纹 = |_: &Rc<Closure>| -> Option<String> { None };
        let (ha, hb, hc) = (
            值哈希_用(&a, &无指纹).unwrap(),
            值哈希_用(&b, &无指纹).unwrap(),
            值哈希_用(&c, &无指纹).unwrap(),
        );
        assert!(ha != hb && hb != hc && ha != hc, "值哈希");
        assert_eq!(ha, 值哈希_用(&a.clone(), &无指纹).unwrap(), "同值同哈希");
        let m = MethodIdentity {
            source_hash: "f".into(),
            captured: String::new(),
            lib_version: None,
        };
        let k = |h: &str| code_cell_key(&m, &[h]).id;
        assert!(
            k(&ha) != k(&hb) && k(&hb) != k(&hc) && k(&ha) != k(&hc),
            "代码单元键"
        );
    }
}
