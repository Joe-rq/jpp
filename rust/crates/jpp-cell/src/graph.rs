//! 单元图引擎（单线程）：源单元、记忆表、反向边、标脏、判断单元与刷新、程序单元与发布、多写者单元的合并。
//!
//! 对照 `12` §2.13：R1 五种单元、R2 四种状态、R3 依赖边与标脏与快照、R4 两层截断（在 [`crate::ctx`]）、
//! R7 多写者与占用、R9 欠账与违规、R10 读者承接与转交。账本与重放是 C3，多程序调度、预算与深度是 C4，
//! 并发尝试是 C10；这里只做单线程语义。原型参照：`地基/骨架原型/乙/src/engine.rs`。

use crate::ctx::{Attempt, Ctx, Frame};
use crate::debt::{DebtTok, DutyEvent, DutyKind, Violation};
use crate::reducer::{self, Cell, MergeKind, Reducer, Writers};
use crate::value::{CellValue, exits_of};
use jpp_ir::cause::UnsureCause;
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::rc::Rc;

// ───────────────────────── 依赖与记忆 ─────────────────────────

/// 一条依赖边：这个单元读了哪个单元的哪一版（R3）。
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Dep {
    /// 源单元的第几版（1 起；0 = 还没写过）
    Src(String, u32),
    /// 代码单元的记忆哈希
    Code(String, String),
    /// 判断单元（内容键）与读到的是什么：答了的不再变；「未问」可能过期（缺席的未问不跨宿主事件，F2）
    Judge(String, JudgeSeen),
    /// 别的程序「定下」的第几版；`None` = 读的时候还没定
    Prog(String, Option<u32>),
    /// 多写者单元的第几版（合并过几次）
    Shared(String, u32),
}

/// 读判断单元时读到的是什么（记进依赖，核依赖时与现在的读法比）。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum JudgeSeen {
    Answered,
    Unasked(UnsureCause),
    Pending,
}

/// 一条「未问」：原因、刷新号、记下时的宿主纪元、是否整场有效。缺席重试用完记下的只在记下时的宿主纪元里有效
/// （宿主事件时去掉并标脏读过它的，原型乙 Z0436 阻断一）；停发（`FlushAnswer::Unasked`）本步按整场有效记，
/// 预算按（题，付钱方）登记、按轮失效归 C4（裁定六十二 (4)）。
#[derive(Clone, Copy, Debug)]
pub(crate) struct UnaskedRec {
    pub(crate) cause: UnsureCause,
    pub(crate) f: u32,
    pub(crate) host_epoch: u32,
    pub(crate) permanent: bool,
}

/// 反向边上的节点。
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) enum Node {
    Src(String),
    Code(String),
    Judge(String),
    ProgSettled(String),
    Shared(String),
}

impl Dep {
    pub(crate) fn node(&self) -> Node {
        match self {
            Dep::Src(k, _) => Node::Src(k.clone()),
            Dep::Code(k, _) => Node::Code(k.clone()),
            Dep::Judge(k, _) => Node::Judge(k.clone()),
            Dep::Prog(p, _) => Node::ProgSettled(p.clone()),
            Dep::Shared(c, _) => Node::Shared(c.clone()),
        }
    }
}

/// 依赖一个节点的东西：代码单元或程序。
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) enum Owner {
    Code(String),
    Prog(String),
}

/// 「还不能算完」：缺判断的答案，或者在等别的程序定下。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Pend;

pub(crate) type Thunk<V> = Rc<dyn Fn(&mut Ctx<'_, V>) -> Result<V, Pend>>;

/// 代码单元的记忆项。
pub(crate) struct Memo<V> {
    pub(crate) value: V,
    /// 记忆哈希 = 值哈希并上欠账与单元去向的摘要
    pub(crate) hash: String,
    pub(crate) deps: Vec<Dep>,
    /// 求值体；解释器驱动的记忆项为 `None`（C2c），脏了只能由解释器重新求值
    pub(crate) thunk: Option<Thunk<V>>,
    pub(crate) debts: BTreeMap<DebtTok, crate::debt::Debt>,
    pub(crate) duties: BTreeMap<DebtTok, DutyKind>,
    /// 单元里放弃缺席类出口记下的报错（记号 → `E-drop-unobserved`），随记忆项保存（F3）
    pub(crate) errors: BTreeMap<DebtTok, &'static str>,
}

/// 运行计数。
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Stats {
    /// 被标脏的次数（代码单元与程序，按节点计，两次修复之间只计一次）
    pub marked: u64,
    /// 代码单元求值次数（含第一次）
    pub computed: u64,
    /// 其中重算后记忆哈希没变的（出口截断的起点）
    pub recomputed_same: u64,
    /// 脏了、核依赖后没变、不用重算的（输入截断）
    pub verified_clean: u64,
    /// 干净、直接命中的读
    pub hits: u64,
    /// 程序尝试次数
    pub attempts: u64,
}

/// 只给测试用的反证开关：关掉某个机制，对应测试必须失败（预注册第四节 (b)）。默认全开。
#[derive(Clone, Copy, Debug)]
pub(crate) struct Knobs {
    /// 记忆哈希并上欠账摘要（R4 第二层）
    pub(crate) debt_digest: bool,
    /// 读时承接（R10）
    pub(crate) takeover: bool,
    /// 「核依赖后没变」这条返回路径上并入欠账
    pub(crate) inherit_on_verified: bool,
    /// 记号含题与原因（Z0419(1)）
    pub(crate) token_has_question: bool,
    /// 单元以转交交出的记号挡住读者（错的做法）
    pub(crate) handoff_blocks_reader: bool,
}

impl Default for Knobs {
    fn default() -> Knobs {
        Knobs {
            debt_digest: true,
            takeover: true,
            inherit_on_verified: true,
            token_has_question: true,
            handoff_blocks_reader: false,
        }
    }
}

// ───────────────────────── 程序单元 ─────────────────────────

/// 一段程序。每次尝试从头求值（代码单元与判断走记忆），返回有结论的值（`Ok`）或进行中的部分值（`Err`）。
/// 取 `&self`：程序没有跨尝试的可写状态（R7），要跨尝试留住的东西必须是单元。
pub trait Program<V> {
    fn attempt(&self, c: &mut Ctx<'_, V>) -> Result<V, V>;
}

/// 程序单元已发布版本的状态（R2）。「待算」不是状态，是脏标记。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PState {
    InProgress,
    Settled,
    Revised,
    Unsure,
}

impl PState {
    pub fn name(self) -> &'static str {
        match self {
            PState::InProgress => "进行中",
            PState::Settled => "已定",
            PState::Revised => "已修正",
            PState::Unsure => "未决",
        }
    }
    /// 「定下」= 后三种之一
    pub fn settled(self) -> bool {
        self != PState::InProgress
    }
}

/// 程序单元发布过的一版。
#[derive(Clone, Debug)]
pub struct Published<V> {
    pub version: u32,
    pub state: PState,
    pub value: V,
    pub hash: String,
    /// 状态为「未决」时的原因：违规，或值里未销的未决的原因
    pub causes: BTreeSet<UnsureCause>,
    /// 值里所带未销未决的记号集合：对外指纹的一部分（R4；复核 F1）
    pub toks: BTreeSet<DebtTok>,
}

/// 一次尝试的记录（契约性结果与不定输入的内存形态；账本编码归 V5，写入归 C3）。
#[derive(Clone, Debug, Default)]
pub struct AttemptRec {
    pub n: u32,
    pub host_epoch: u32,
    pub flush_epoch: u32,
    /// 开始时的快照：别的程序定下的版本
    pub settled_reads: BTreeMap<String, Option<u32>>,
    /// 开始时的快照：多写者单元的版本
    pub shared_snap: BTreeMap<String, u32>,
    /// 立刻读：程序、版本、状态、值哈希
    pub peeks: Vec<(String, u32, PState, String)>,
    /// 缺的判断（内容键）
    pub pending: Vec<String>,
    /// 发布了新的一版：(版本, 状态)
    pub publish: Option<(u32, PState)>,
    /// 程序级去向事件
    pub duties: Vec<DutyEvent>,
    /// 这次尝试的多写者合并：(单元, 键, 合并后的版本, 事件种类或拒绝原因)
    pub writes: Vec<(String, String, u32, String)>,
    /// 违规（B196）
    pub violations: Vec<Violation>,
    /// 这次尝试读到的单元里交出的去向，按种类计数
    pub cell_duties: BTreeMap<DutyKind, u32>,
    /// 这次尝试读到的单元里记下的报错（`E-drop-unobserved`），按种类计数（F3）
    pub cell_errors: BTreeMap<&'static str, u32>,
    /// 这次尝试有结论（程序返回 `Ok`）
    pub concluded: bool,
}

impl AttemptRec {
    /// 按种类数程序级去向事件。
    pub fn duty_counts(&self) -> BTreeMap<&'static str, u32> {
        let mut m = BTreeMap::new();
        for d in &self.duties {
            *m.entry(d.kind).or_default() += 1;
        }
        m
    }
}

pub(crate) struct ProgSlot<V> {
    /// `None` = 解释器驱动的程序（C2b）：由宿主经 `open`/`close` 尝试，引擎不调度它
    prog: Option<Rc<dyn Program<V>>>,
    pub(crate) published: Vec<Published<V>>,
    pub(crate) attempts: Vec<AttemptRec>,
    dirty: bool,
    waiting: BTreeSet<String>,
    /// 上一次尝试的依赖（换依赖边时用，不扫整张反向表）
    deps: Vec<Dep>,
}

impl<V: CellValue> ProgSlot<V> {
    pub(crate) fn latest_settled(&self) -> Option<&Published<V>> {
        self.published.iter().rev().find(|p| p.state.settled())
    }
}

// ───────────────────────── 多写者单元 ─────────────────────────

#[derive(Clone)]
enum WriteOp<V> {
    Put(String, String, V),
    Claim(String, Vec<String>),
}

pub(crate) struct SharedCell<V> {
    reducer: Reducer,
    writers: Writers,
    /// 合并日志（按合并顺序）；版本 = 日志长度
    log: Vec<WriteOp<V>>,
    pub(crate) cell: Cell<V>,
}

impl<V: CellValue> SharedCell<V> {
    /// 第 v 版的状态（按日志前 v 条重放）。
    pub(crate) fn at(&self, v: u32) -> Cell<V> {
        if v as usize == self.log.len() {
            return self.cell.clone();
        }
        let mut c = reducer::declare(self.reducer, self.writers).expect("声明时已核过");
        for op in self.log.iter().take(v as usize) {
            match op {
                WriteOp::Put(w, k, val) => {
                    let _ = c.write(w, k, val.clone());
                }
                WriteOp::Claim(w, g) => {
                    c.claim(w, g);
                }
            }
        }
        c
    }
    pub(crate) fn version(&self) -> u32 {
        self.log.len() as u32
    }
}

/// 尝试里对多写者单元的一次写（提交时按顺序合并）。
pub(crate) enum PendingWrite<V> {
    Put(String, String, V),
    Claim(String, Vec<String>),
}

// ───────────────────────── 刷新 ─────────────────────────

/// 刷新时对一道题的处置（由调用方决定：真判断器、替身、预算停发）。
#[derive(Clone, Debug)]
pub enum FlushAnswer<V> {
    /// 答了：读数
    Answer(V),
    /// 判断器缺席（不是答案，不进答案表；重试 `absent_retry` 次后记「未问（absent）」）
    Absent,
    /// 没有发出（预算、深度、截止）：记「未问」（R11 第 4 条）
    Unasked(UnsureCause),
}

/// 一次刷新：每道题的内容键、要求者（按程序注册顺序；第一个付钱，R11 第 3 条）与处置。
#[derive(Clone, Debug)]
pub struct FlushRec {
    pub f: u32,
    /// (内容键, 要求者, 处置：answer / absent / unasked:<原因>)
    pub questions: Vec<(String, Vec<String>, String)>,
}

// ───────────────────────── 引擎 ─────────────────────────

pub struct CellGraph<V> {
    /// 源单元：键 → [(写入时的宿主纪元, 值)]
    pub(crate) src: BTreeMap<String, Vec<(u32, V)>>,
    pub(crate) host_epoch: u32,
    pub(crate) memo: HashMap<String, Memo<V>>,
    rev: HashMap<Node, BTreeSet<Owner>>,
    pub(crate) dirty: HashSet<String>,
    /// 判断：内容键 → (读数, 在第几次刷新答的)
    pub(crate) answers: HashMap<String, (V, u32)>,
    /// 「未问」：内容键 → (原因, 刷新号)
    pub(crate) unasked: HashMap<String, UnaskedRec>,
    absent_seen: HashMap<String, u32>,
    /// 账本求值体的代码单元（B199 第 2 条：`gen`、`transform` 的输出）：键 → 值，先到先得（C2b）
    ledger_cells: HashMap<String, V>,
    /// 缺席重试次数（B32；原型参数 `absent_retry`）
    pub absent_retry: u32,
    pub(crate) flush_epoch: u32,
    /// 本刷新纪元登记的缺题：内容键 → 要它的程序
    pending: BTreeMap<String, BTreeSet<String>>,
    pub flushes: Vec<FlushRec>,
    pub(crate) progs: BTreeMap<String, ProgSlot<V>>,
    order: Vec<String>,
    pub(crate) shared: BTreeMap<String, SharedCell<V>>,
    pub(crate) stats: Stats,
    pub(crate) knobs: Knobs,
}

impl<V: CellValue> Default for CellGraph<V> {
    fn default() -> Self {
        Self::new()
    }
}

impl<V: CellValue> CellGraph<V> {
    pub fn new() -> CellGraph<V> {
        CellGraph {
            src: BTreeMap::new(),
            host_epoch: 0,
            memo: HashMap::new(),
            rev: HashMap::new(),
            dirty: HashSet::new(),
            answers: HashMap::new(),
            unasked: HashMap::new(),
            absent_seen: HashMap::new(),
            ledger_cells: HashMap::new(),
            absent_retry: 1,
            flush_epoch: 0,
            pending: BTreeMap::new(),
            flushes: vec![],
            progs: BTreeMap::new(),
            order: vec![],
            shared: BTreeMap::new(),
            stats: Stats::default(),
            knobs: Knobs::default(),
        }
    }

    // ── 宿主 ──

    /// 一次宿主事件：写若干源单元，宿主纪元加一，沿反向边标脏（R12：世界的每一次变更都是宿主事件）。
    pub fn host_event(&mut self, writes: Vec<(String, V)>) {
        self.host_epoch += 1;
        let e = self.host_epoch;
        // 缺席的重试计数每个宿主纪元从零起（Z0581，主控认可的读法：与 `rust-jpp` G5 每趟重置同口径）
        self.absent_seen.clear();
        // 缺席的「未问」不跨宿主事件：去掉过期的，标脏读过它的单元与程序（R1「后续刷新可以再问同一内容键」，F2）
        let mut expired: Vec<String> = self
            .unasked
            .iter()
            .filter(|(_, u)| !u.permanent && u.host_epoch < e)
            .map(|(k, _)| k.clone())
            .collect();
        expired.sort();
        for k in expired {
            self.unasked.remove(&k);
            self.mark(Node::Judge(k));
        }
        for (k, v) in writes {
            let h = self.src.entry(k.clone()).or_default();
            if h.last()
                .is_some_and(|x| x.1.content_hash() == v.content_hash())
            {
                continue;
            }
            h.push((e, v));
            self.mark(Node::Src(k));
        }
    }

    pub(crate) fn src_at(&self, key: &str, epoch: u32) -> (u32, Option<V>) {
        match self.src.get(key) {
            None => (0, None),
            Some(h) => {
                let mut ver = 0;
                let mut val = None;
                for (i, (e, v)) in h.iter().enumerate() {
                    if *e <= epoch {
                        ver = i as u32 + 1;
                        val = Some(v.clone());
                    } else {
                        break;
                    }
                }
                (ver, val)
            }
        }
    }

    /// 声明多写者单元。多写者上声明「取最新」直接拒掉（R7）。
    pub fn declare_shared(&mut self, name: &str, r: Reducer, w: Writers) -> Result<(), String> {
        let cell = reducer::declare(r, w)?;
        self.shared.insert(
            name.to_string(),
            SharedCell {
                reducer: r,
                writers: w,
                log: vec![],
                cell,
            },
        );
        Ok(())
    }

    pub fn shared_events(&self, name: &str) -> &[reducer::MergeEvent] {
        &self.shared[name].cell.events
    }

    /// 多写者单元的当前状态（只读，给宿主展示与测试）。
    pub fn shared_now(&self, name: &str) -> &Cell<V> {
        &self.shared[name].cell
    }

    // ── 程序 ──

    pub fn add_program(&mut self, id: &str, prog: Rc<dyn Program<V>>) {
        assert!(
            !self.progs.contains_key(id),
            "E-program-dup：程序 {id} 重名（B193 第 4 条）"
        );
        self.order.push(id.to_string());
        self.progs.insert(
            id.to_string(),
            ProgSlot {
                prog: Some(prog),
                published: vec![],
                attempts: vec![],
                dirty: true,
                waiting: BTreeSet::new(),
                deps: vec![],
            },
        );
    }

    /// 现在可以尝试的程序：被标脏、且不在等判断（按注册顺序）。
    pub fn runnable(&self) -> Vec<String> {
        self.order
            .iter()
            .filter(|id| {
                let s = &self.progs[*id];
                s.prog.is_some() && s.dirty && s.waiting.is_empty()
            })
            .cloned()
            .collect()
    }

    pub fn has_pending(&self) -> bool {
        !self.pending.is_empty()
    }

    pub fn published(&self, id: &str) -> &[Published<V>] {
        &self.progs[id].published
    }
    pub fn latest(&self, id: &str) -> Option<&Published<V>> {
        self.progs[id].published.last()
    }
    pub fn attempts(&self, id: &str) -> &[AttemptRec] {
        &self.progs[id].attempts
    }
    pub fn stats(&self) -> &Stats {
        &self.stats
    }
    pub fn programs(&self) -> &[String] {
        &self.order
    }
    /// 记忆表里的代码单元数。
    pub fn code_cells(&self) -> usize {
        self.memo.len()
    }

    /// 标脏：从一个节点沿反向边往下，依赖它的代码单元记脏并继续往下传，程序记为要再尝试。不重算、不发判断（R3）。
    pub(crate) fn mark(&mut self, node: Node) {
        let mut stack = vec![node];
        while let Some(n) = stack.pop() {
            let owners: Vec<Owner> = self
                .rev
                .get(&n)
                .map(|s| s.iter().cloned().collect())
                .unwrap_or_default();
            for o in owners {
                match o {
                    Owner::Prog(p) => {
                        if let Some(slot) = self.progs.get_mut(&p)
                            && !slot.dirty
                        {
                            slot.dirty = true;
                            self.stats.marked += 1;
                        }
                    }
                    Owner::Code(k) => {
                        if self.dirty.insert(k.clone()) {
                            self.stats.marked += 1;
                            stack.push(Node::Code(k));
                        }
                    }
                }
            }
        }
    }

    pub(crate) fn set_deps(&mut self, owner: Owner, old: &[Dep], new: &[Dep]) {
        for d in old {
            if let Some(s) = self.rev.get_mut(&d.node()) {
                s.remove(&owner);
            }
        }
        for d in new {
            self.rev.entry(d.node()).or_default().insert(owner.clone());
        }
    }

    /// 尝试一次。
    pub fn attempt(&mut self, id: &str) {
        let prog = self.progs[id]
            .prog
            .clone()
            .expect("解释器驱动的程序不由引擎尝试");
        let a = self.open(id);
        let mut c = Ctx::resume(self, a);
        let out = prog.attempt(&mut c);
        let a = c.suspend();
        self.close(id, a, out);
    }

    /// 开一次尝试：记纪元与快照（R3）。别的程序定下的版本、多写者单元的版本；单线程下尝试期间没有别的写落地，
    /// 快照天然一致。解释器驱动时由宿主持有返回的尝试状态，结束时交给 [`CellGraph::close`]（C2b）。
    pub fn open(&mut self, id: &str) -> Attempt<V> {
        let slot = &self.progs[id];
        let mut rec = AttemptRec {
            n: slot.attempts.len() as u32 + 1,
            host_epoch: self.host_epoch,
            flush_epoch: self.flush_epoch,
            ..Default::default()
        };
        rec.settled_reads = self
            .order
            .iter()
            .filter(|p| *p != id)
            .map(|p| (p.clone(), self.progs[p].latest_settled().map(|x| x.version)))
            .collect();
        rec.shared_snap = self
            .shared
            .iter()
            .map(|(k, c)| (k.clone(), c.version()))
            .collect();
        Ctx::new(self, id, rec).suspend()
    }

    /// 结束一次尝试并提交（欠账结算、发布，R2、R9）。
    pub fn close(&mut self, id: &str, a: Attempt<V>, out: Result<V, V>) {
        let (rec, parts) = crate::ctx::finish_attempt(a);
        self.commit(id, rec, out, parts);
    }

    /// 登记一段解释器驱动的程序（C2b）：引擎不调度它，尝试由宿主经 [`CellGraph::open`]、[`CellGraph::close`] 做。
    pub fn add_external_program(&mut self, id: &str) {
        assert!(
            !self.progs.contains_key(id),
            "E-program-dup：程序 {id} 重名（B193 第 4 条）"
        );
        self.order.push(id.to_string());
        self.progs.insert(
            id.to_string(),
            ProgSlot {
                prog: None,
                published: vec![],
                attempts: vec![],
                dirty: false,
                waiting: BTreeSet::new(),
                deps: vec![],
            },
        );
    }

    // ── 解释器驱动的判断单元与账本单元（C2b） ──

    /// 记一道题的答案（先到先得；已有返回 `false`）。按当前刷新纪元记：引擎驱动的读法（`Ctx::judge`）照旧要求答案
    /// 在本次尝试的刷新纪元之前写下。
    pub fn record_answer(&mut self, key: &str, v: V) -> bool {
        if self.answers.contains_key(key) {
            return false;
        }
        self.answers.insert(key.to_string(), (v, self.flush_epoch));
        true
    }

    /// 读一道题的答案，不看刷新纪元（解释器在自己的刷新点上落账后立即可复用，与 `rust-jpp` 的同运行复用相同）。
    pub fn answer(&self, key: &str) -> Option<&V> {
        self.answers.get(key).map(|(v, _)| v)
    }

    /// 解释器的一次刷新结束：刷新纪元加一。
    pub fn advance_flush(&mut self) {
        self.flush_epoch += 1;
    }

    /// 判断单元数（已答的不同内容键）。
    pub fn judge_cells(&self) -> usize {
        self.answers.len()
    }

    /// 记一个账本求值体的代码单元（先到先得；已有返回 `false`）。
    pub fn record_ledger_cell(&mut self, key: &str, v: V) -> bool {
        if self.ledger_cells.contains_key(key) {
            return false;
        }
        self.ledger_cells.insert(key.to_string(), v);
        true
    }

    /// 读一个账本求值体的代码单元。
    pub fn ledger_cell(&self, key: &str) -> Option<&V> {
        self.ledger_cells.get(key)
    }

    /// 账本求值体的代码单元数。
    pub fn ledger_cells(&self) -> usize {
        self.ledger_cells.len()
    }

    /// 当前宿主纪元。
    pub fn host_epoch(&self) -> u32 {
        self.host_epoch
    }

    /// 当前刷新纪元。
    pub fn flush_epoch(&self) -> u32 {
        self.flush_epoch
    }

    fn commit(
        &mut self,
        id: &str,
        mut rec: AttemptRec,
        out: Result<V, V>,
        p: crate::ctx::Finished<V>,
    ) {
        self.stats.attempts += 1;
        let crate::ctx::Finished {
            mut top,
            handed,
            declared,
            duties,
            pending,
            writes,
            peeks,
        } = p;
        let mut toks: BTreeSet<DebtTok> = BTreeSet::new();
        // 程序段本体里交出过的记号，之后经代码单元的记忆项又并回来的，不再算欠（R10 第 6 条）
        for t in &handed {
            top.debts.remove(t);
        }
        // 多写者单元的写：尝试结束时按顺序合并
        let mut touched = BTreeSet::new();
        for w in writes {
            let (name, key) = match &w {
                PendingWrite::Put(n, k, _) => (n.clone(), k.clone()),
                PendingWrite::Claim(n, _) => (n.clone(), id.to_string()),
            };
            let sc = self
                .shared
                .get_mut(&name)
                .unwrap_or_else(|| panic!("没声明的多写者单元 {name}"));
            let before = sc.cell.events.len();
            let r = match &w {
                PendingWrite::Put(_, k, v) => sc.cell.write(id, k, v.clone()),
                PendingWrite::Claim(_, g) => Ok(sc.cell.claim(id, g)),
            };
            match r {
                Ok(true) => {
                    sc.log.push(match w {
                        PendingWrite::Put(_, k, v) => WriteOp::Put(id.to_string(), k, v),
                        PendingWrite::Claim(_, g) => WriteOp::Claim(id.to_string(), g),
                    });
                    let kind = sc.cell.events[before..]
                        .last()
                        .map(|e| e.kind)
                        .unwrap_or(MergeKind::Merge);
                    rec.writes
                        .push((name.clone(), key, sc.version(), kind.name().to_string()));
                    touched.insert(name);
                }
                Ok(false) => {}
                Err(e) => {
                    rec.writes
                        .push((name.clone(), key, sc.version(), format!("rejected:{e}")))
                }
            }
        }
        // 依赖边：程序节点（旧边存在程序单元上，不扫整张反向表）
        let old = std::mem::take(&mut self.progs.get_mut(id).unwrap().deps);
        self.set_deps(Owner::Prog(id.to_string()), &old, &top.deps);
        self.progs.get_mut(id).unwrap().deps = top.deps.clone();
        // B194（Z0510 第一条）：等到读碰到上游未定（记过 `Prog(_, None)`），这次尝试到此为止——程序交来有结论也
        // 降为进行中，不发布有结论的版本；上游定下后读者被标脏、下一次从头来
        let out = match out {
            Ok(v) if top.deps.iter().any(|d| matches!(d, Dep::Prog(_, None))) => Err(v),
            o => o,
        };
        // 欠账结算：只在有结论时结算；进行中的尝试下次从头算（R9）
        rec.concluded = out.is_ok();
        let mut duties = duties;
        let mut causes: BTreeSet<UnsureCause> = BTreeSet::new();
        if let Ok(v) = &out {
            for (tok, cause) in exits_of(v) {
                match tok {
                    // 值里带着、本帧欠着：随返回值转交（R10 第 5 条：值里真带着才成立）
                    Some(t) if top.debts.contains_key(&t) => {
                        top.debts.remove(&t);
                        let what = declared
                            .get(&t)
                            .cloned()
                            .unwrap_or_else(|| "写进返回值，随返回值转交".into());
                        duties.push(DutyEvent {
                            kind: DutyKind::Handoff.event_type(),
                            tok: Some(t.clone()),
                            detail: what,
                        });
                        causes.insert(cause);
                        toks.insert(t);
                    }
                    // 本次尝试已经交出过的（程序段里升级、放弃等，或读到的单元里以转交以外的去向交出），不算未销
                    Some(t)
                        if handed.contains(&t)
                            || top.duties.get(&t).is_some_and(|k| *k != DutyKind::Handoff) => {}
                    // 同一值里同一记号出现多次，或读了没接（不可能），都按未销计
                    _ => {
                        causes.insert(cause);
                        if let Some(t) = tok {
                            toks.insert(t);
                        }
                    }
                }
            }
            rec.violations = top
                .debts
                .values()
                .map(|d| Violation {
                    program: id.to_string(),
                    attempt: rec.n,
                    debt: d.clone(),
                })
                .collect();
        }
        for k in top.duties.values() {
            *rec.cell_duties.entry(*k).or_default() += 1;
        }
        for e in top.errors.values() {
            *rec.cell_errors.entry(*e).or_default() += 1;
        }
        rec.duties = duties;
        rec.peeks = peeks;
        rec.pending = pending.iter().cloned().collect();
        for k in &pending {
            self.pending
                .entry(k.clone())
                .or_default()
                .insert(id.to_string());
        }
        // 发布（R2）
        let (concluded, value) = match out {
            Ok(v) => (true, v),
            Err(v) => (false, v),
        };
        let violated = concluded && !rec.violations.is_empty();
        if violated {
            causes = BTreeSet::from([UnsureCause::Violation]);
        }
        let slot = self.progs.get_mut(id).unwrap();
        let ever_settled = slot.published.iter().any(|p| p.state.settled());
        let state = if !concluded {
            PState::InProgress
        } else if violated || !causes.is_empty() {
            PState::Unsure
        } else if ever_settled {
            PState::Revised
        } else {
            PState::Settled
        };
        if state != PState::Unsure {
            causes.clear();
            toks.clear();
        }
        let hash = value.content_hash();
        let changed = match slot.published.last() {
            None => true,
            Some(p) => {
                p.hash != hash
                    || p.state.settled() != state.settled()
                    || (p.state == PState::Unsure) != (state == PState::Unsure)
                    || p.causes != causes
                    // 对外指纹含值里所带未销未决的记号（R4；复核 F1）：同一处换了题而值与原因不变时也要发新版，
                    // 否则读者接的是旧版里的旧记号
                    || p.toks != toks
            }
        };
        let settled_before = slot.latest_settled().map(|p| p.version);
        if changed {
            let v = slot.published.len() as u32 + 1;
            slot.published.push(Published {
                version: v,
                state,
                value,
                hash,
                causes,
                toks,
            });
            rec.publish = Some((v, state));
        }
        let settled_after = slot.latest_settled().map(|p| p.version);
        slot.waiting = pending;
        slot.dirty = false;
        slot.attempts.push(rec);
        if settled_before != settled_after {
            self.mark(Node::ProgSettled(id.to_string()));
        }
        for name in touched {
            self.mark(Node::Shared(name));
        }
    }

    /// 刷新：把登记的缺题逐道交给 `decide` 处置，刷新纪元加一，唤醒等这些题的程序。
    /// 缺席不是答案：记次数，超过 `absent_retry` 次记「未问（absent）」；程序被唤醒后再登记，下一次刷新再问（R1、B32）。
    pub fn flush(&mut self, decide: &mut dyn FnMut(&str, &[String]) -> FlushAnswer<V>) {
        let f = self.flush_epoch;
        let pend = std::mem::take(&mut self.pending);
        let mut questions = vec![];
        let mut resolved: BTreeSet<String> = BTreeSet::new();
        for (k, reqs) in pend {
            if self.answers.contains_key(&k) || self.unasked.contains_key(&k) {
                resolved.insert(k);
                continue;
            }
            let mut rs: Vec<String> = reqs.into_iter().collect();
            rs.sort_by_key(|r| self.order.iter().position(|x| x == r).unwrap_or(usize::MAX));
            let what = match decide(&k, &rs) {
                FlushAnswer::Answer(v) => {
                    self.answers.insert(k.clone(), (v, f));
                    "answer".to_string()
                }
                FlushAnswer::Absent => {
                    let n = self.absent_seen.entry(k.clone()).or_default();
                    *n += 1;
                    if *n > self.absent_retry {
                        let he = self.host_epoch;
                        self.unasked.insert(
                            k.clone(),
                            UnaskedRec {
                                cause: UnsureCause::Absent,
                                f,
                                host_epoch: he,
                                permanent: false,
                            },
                        );
                    }
                    "absent".to_string()
                }
                FlushAnswer::Unasked(c) => {
                    let he = self.host_epoch;
                    self.unasked.insert(
                        k.clone(),
                        UnaskedRec {
                            cause: c,
                            f,
                            host_epoch: he,
                            permanent: true,
                        },
                    );
                    format!("unasked:{}", c.name())
                }
            };
            resolved.insert(k.clone());
            questions.push((k, rs, what));
        }
        self.flushes.push(FlushRec { f, questions });
        self.flush_epoch += 1;
        for slot in self.progs.values_mut() {
            if slot.waiting.is_empty() {
                continue;
            }
            let hit = slot.waiting.iter().any(|k| resolved.contains(k));
            slot.waiting.retain(|k| !resolved.contains(k));
            if hit {
                slot.dirty = true;
            }
        }
    }

    /// 跑到静止：先尝试（按注册顺序），没得尝试再刷新。多程序的带种子调度是 C4。
    pub fn quiesce(&mut self, decide: &mut dyn FnMut(&str, &[String]) -> FlushAnswer<V>) {
        loop {
            if let Some(id) = self.runnable().first().cloned() {
                self.attempt(&id);
            } else if self.has_pending() {
                self.flush(decide);
            } else {
                break;
            }
        }
    }
}

/// 帧的结束：值里带着、本帧欠着的记号算转交（单元级记 `handoff` 去向）。给 [`crate::ctx`] 用。
pub(crate) fn close_cell_frame<V: CellValue>(f: &mut Frame, v: &V) {
    for (tok, _) in exits_of(v) {
        if let Some(t) = tok
            && f.debts.remove(&t).is_some()
        {
            f.duties.insert(t, DutyKind::Handoff);
        }
    }
}
