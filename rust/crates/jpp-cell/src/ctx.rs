//! 一次尝试的求值上下文：读源单元、定义与读代码单元（两层截断，R4）、问判断、读别的程序（读者承接，R10）、
//! 读写多写者单元与占用（R7）、记欠账与去向（R9）。
//!
//! 帧：程序段本体一帧，每个正在求值的代码单元一帧。欠账记在当前帧；代码单元求出值时，值里带着的欠账算转交
//! （单元去向 `handoff`），其余欠账与去向随记忆项留下，任何读这个单元的地方都并进来（命中记忆时也一样）。

use crate::debt::{
    BridgeKind, Debt, DebtTok, DutyEvent, DutyKind, E_DROP_UNOBSERVED, FrameKind, debt_token,
    with_debt_digest,
};
use crate::graph::{
    AttemptRec, CellGraph, Dep, JudgeSeen, Knobs, Memo, Owner, Pend, PendingWrite, Published,
    Thunk, close_cell_frame,
};
use crate::reducer::SharedRead;
use crate::value::{CellValue, exits_of};
use jpp_ir::cause::UnsureCause;
use jpp_ir::key::hash_of;
use std::collections::{BTreeMap, BTreeSet};
use std::rc::Rc;

/// 一帧：程序段本体或一个代码单元。
#[derive(Debug)]
pub(crate) struct Frame {
    kind: FrameKind,
    owner: String,
    pub(crate) deps: Vec<Dep>,
    /// 这一帧里第几次过桥（记号的成分）
    cuts: u32,
    /// 不带记号的单元去向的序号
    synth: u32,
    pub(crate) debts: BTreeMap<DebtTok, Debt>,
    /// 这一帧（连同它读的单元）交出的单元去向：记号 → 种类
    pub(crate) duties: BTreeMap<DebtTok, DutyKind>,
    /// 这一帧（连同它读的单元）里放弃缺席类出口记下的报错：记号 → `E-drop-unobserved`（F3）
    pub(crate) errors: BTreeMap<DebtTok, &'static str>,
}

impl Frame {
    fn new(kind: FrameKind, owner: &str) -> Frame {
        Frame {
            kind,
            owner: owner.to_string(),
            deps: vec![],
            cuts: 0,
            synth: 0,
            debts: BTreeMap::new(),
            duties: BTreeMap::new(),
            errors: BTreeMap::new(),
        }
    }
}

/// 某个单元去向挡不挡住读者再欠这一笔：转交不挡（值在读者手里，读者从值里接下），其余种类挡（交一次就算交了）。
fn blocks(k: &Knobs, d: Option<&DutyKind>) -> bool {
    match d {
        None => false,
        Some(DutyKind::Handoff) => k.handoff_blocks_reader,
        Some(_) => true,
    }
}

impl Frame {
    /// 并进读到的单元的欠账与单元去向，按记号去重（同一笔经几条路径并进来只算一笔）。
    fn inherit(
        &mut self,
        k: &Knobs,
        debts: &BTreeMap<DebtTok, Debt>,
        duties: &BTreeMap<DebtTok, DutyKind>,
        errors: &BTreeMap<DebtTok, &'static str>,
    ) {
        for (t, e) in errors {
            self.errors.entry(t.clone()).or_insert(*e);
        }
        for (t, d) in debts {
            if !blocks(k, duties.get(t)) && !blocks(k, self.duties.get(t)) {
                self.debts.entry(t.clone()).or_insert_with(|| d.clone());
            }
        }
        for (t, kind) in duties {
            if blocks(k, Some(kind)) {
                self.debts.remove(t);
            }
            self.duties.entry(t.clone()).or_insert(*kind);
        }
    }
}

/// 解释器驱动的代码单元：开始求值时的两种情形（C2c）。
#[derive(Clone, Debug)]
pub enum CodeStep<V> {
    /// 记忆项能用：值
    Hit(V),
    /// 要求值：已压一帧，求完调 `end_code`
    Compute,
}

/// 问判断的结果。
#[derive(Clone, Debug)]
pub enum JudgeRead<V> {
    /// 答了：读数
    Answered(V),
    /// 没问（预算、深度、截止，或缺席重试后仍缺席）：过桥得这个原因的未决
    Unasked(UnsureCause),
    /// 还没答：已登记到下一个刷新点
    Pending,
}

/// 占用的结果。
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ClaimResult {
    /// 申请已写，等合并
    Waiting,
    Granted,
    /// 被拒：占着冲突成员的写者，与引擎发的 `claim_conflict` 未决出口的记号（已在当前帧记一笔欠账，Q2）
    Rejected {
        held_by: Vec<String>,
        tok: DebtTok,
    },
}

/// 尝试结束时交回引擎提交的东西。
pub(crate) struct Finished<V> {
    pub(crate) top: Frame,
    pub(crate) handed: BTreeSet<DebtTok>,
    pub(crate) declared: BTreeMap<DebtTok, String>,
    pub(crate) duties: Vec<DutyEvent>,
    pub(crate) pending: BTreeSet<String>,
    pub(crate) writes: Vec<PendingWrite<V>>,
    pub(crate) peeks: Vec<(String, u32, crate::graph::PState, String)>,
}

/// 一次尝试的状态（不含对单元图的借用）：引擎驱动时活在 [`Ctx`] 里；解释器驱动时由解释器持有，要读写单元图时
/// 经 [`Ctx::resume`] 借回、用完 [`Ctx::suspend`] 交还（C2b）。
pub struct Attempt<V> {
    pub(crate) rec: AttemptRec,
    prog: String,
    host_epoch: u32,
    flush_epoch: u32,
    settled: BTreeMap<String, Option<u32>>,
    shared_snap: BTreeMap<String, u32>,
    frames: Vec<Frame>,
    pending: BTreeSet<String>,
    duties: Vec<DutyEvent>,
    writes: Vec<PendingWrite<V>>,
    peeks: Vec<(String, u32, crate::graph::PState, String)>,
    /// 程序段本体里已经交出去的记号：再读到同一个未决不再记成欠账（R10 第 6 条）
    handed: BTreeSet<DebtTok>,
    /// 程序段本体里声明要转交的记号与说明：有结论时返回值里带着才销账（R10 第 5 条）
    declared: BTreeMap<DebtTok, String>,
}

impl<V> Attempt<V> {
    /// 这次尝试属于哪段程序。
    pub fn program(&self) -> &str {
        &self.prog
    }
    /// 这次尝试的记录（开始时的快照与纪元）。
    pub fn record(&self) -> &AttemptRec {
        &self.rec
    }
}

/// 一次尝试的求值上下文：尝试状态加对单元图的借用。
pub struct Ctx<'g, V> {
    g: &'g mut CellGraph<V>,
    a: Attempt<V>,
}

impl<'g, V: CellValue> Ctx<'g, V> {
    pub(crate) fn new(g: &'g mut CellGraph<V>, prog: &str, rec: AttemptRec) -> Ctx<'g, V> {
        let a = Attempt {
            prog: prog.to_string(),
            host_epoch: rec.host_epoch,
            flush_epoch: rec.flush_epoch,
            settled: rec.settled_reads.clone(),
            shared_snap: rec.shared_snap.clone(),
            frames: vec![Frame::new(FrameKind::Program, prog)],
            pending: BTreeSet::new(),
            duties: vec![],
            writes: vec![],
            peeks: vec![],
            handed: BTreeSet::new(),
            declared: BTreeMap::new(),
            rec,
        };
        Ctx { g, a }
    }

    /// 借回单元图，继续一次由解释器持有的尝试（C2b）。
    pub fn resume(g: &'g mut CellGraph<V>, a: Attempt<V>) -> Ctx<'g, V> {
        Ctx { g, a }
    }

    /// 交还尝试状态（C2b）。
    pub fn suspend(self) -> Attempt<V> {
        self.a
    }

    /// 这次尝试属于哪段程序。
    pub fn program(&self) -> &str {
        &self.a.prog
    }

    fn knobs(&self) -> Knobs {
        self.g.knobs
    }

    fn dep(&mut self, d: Dep) {
        self.a.frames.last_mut().unwrap().deps.push(d);
    }

    // ── 源单元 ──

    /// 读源单元（按这次尝试的宿主纪元），记依赖。还没写过为 `None`。
    pub fn source(&mut self, key: &str) -> Option<V> {
        let (ver, v) = self.g.src_at(key, self.a.host_epoch);
        self.dep(Dep::Src(key.to_string(), ver));
        v
    }

    // ── 判断单元 ──

    /// 问判断（按内容键）：答过给读数；「未问」给原因；否则登记到下一个刷新点。
    pub fn judge(&mut self, key: &str) -> JudgeRead<V> {
        let r = self.judge_now(key);
        let seen = match &r {
            JudgeRead::Answered(_) => JudgeSeen::Answered,
            JudgeRead::Unasked(c) => JudgeSeen::Unasked(*c),
            JudgeRead::Pending => JudgeSeen::Pending,
        };
        self.dep(Dep::Judge(key.to_string(), seen));
        if matches!(r, JudgeRead::Pending) {
            self.a.pending.insert(key.to_string());
        }
        r
    }

    /// 在当前帧记一条判断依赖（解释器驱动：判断的登记与刷新在解释器里做，这里只记「读了哪个判断单元、读到什么」，C2b）。
    pub fn judge_dep(&mut self, key: &str, seen: JudgeSeen) {
        self.dep(Dep::Judge(key.to_string(), seen));
    }

    /// 按这次尝试的刷新纪元读判断单元（不记依赖、不登记）。
    fn judge_now(&self, key: &str) -> JudgeRead<V> {
        if let Some((v, f)) = self.g.answers.get(key)
            && *f < self.a.flush_epoch
        {
            return JudgeRead::Answered(v.clone());
        }
        if let Some(u) = self.g.unasked.get(key)
            && u.f < self.a.flush_epoch
        {
            return JudgeRead::Unasked(u.cause);
        }
        JudgeRead::Pending
    }

    // ── 欠账与去向 ──

    /// 记一笔欠账（R9）：解释器在 `cut`、`fit`、分数过线得到未决时调用；返回记号，出口带着它。
    pub fn owe(&mut self, bridge: BridgeKind, question_key: &str, cause: UnsureCause) -> DebtTok {
        let with_q = self.knobs().token_has_question;
        let f = self.a.frames.last_mut().unwrap();
        f.cuts += 1;
        let tok = debt_token(
            f.kind,
            &f.owner,
            bridge,
            f.cuts,
            question_key,
            cause,
            with_q,
        );
        f.debts.insert(
            tok.clone(),
            Debt {
                tok: tok.clone(),
                owner: f.owner.clone(),
                bridge: Some(bridge),
                nth: f.cuts,
                cause,
                question_key: question_key.to_string(),
                from: None,
            },
        );
        tok
    }

    /// 交出一个未决出口的责任（R9、R13）。带记号的去向只销同记号的那一笔；不带记号的只记事件。
    /// 转交只是声明：帧结束时产物值里带着才成立（R10 第 5 条）。缺席类原因的出口不能放弃：记 `E-drop-unobserved`，不销账。
    pub fn duty(&mut self, tok: Option<DebtTok>, cause: UnsureCause, kind: DutyKind, detail: &str) {
        let top = self.a.frames.len() == 1;
        if kind == DutyKind::Handoff
            && let Some(t) = &tok
        {
            if top {
                self.a.declared.insert(t.clone(), detail.to_string());
            }
            return;
        }
        if kind == DutyKind::DropAccounted && cause.is_absent_class() {
            if top {
                self.a.duties.push(DutyEvent {
                    kind: E_DROP_UNOBSERVED,
                    tok,
                    detail: format!("{}：{detail}", cause.name()),
                });
            } else {
                // 单元里的报错记在帧上，随记忆项保存，命中记忆时照样并回读者（F3）
                let f = self.a.frames.last_mut().unwrap();
                let key = tok.unwrap_or_else(|| {
                    f.synth += 1;
                    DebtTok::synthetic(&["error", f.kind.tag(), &f.owner, &f.synth.to_string()])
                });
                f.errors.insert(key, E_DROP_UNOBSERVED);
            }
            return;
        }
        if top {
            if let Some(t) = &tok {
                self.a.frames[0].debts.remove(t);
                self.a.handed.insert(t.clone());
            }
            self.a.duties.push(DutyEvent {
                kind: kind.event_type(),
                tok,
                detail: detail.to_string(),
            });
        } else {
            let f = self.a.frames.last_mut().unwrap();
            let key = match tok {
                Some(t) => t,
                None => {
                    f.synth += 1;
                    DebtTok::synthetic(&["duty", f.kind.tag(), &f.owner, &f.synth.to_string()])
                }
            };
            f.debts.remove(&key);
            f.duties.insert(key, kind);
        }
    }

    /// 读者承接（R10）：读到的值里每个带记号的未决，在当前帧记一笔同记号的欠账；各读者各自一份；
    /// 这一帧已经交出过的不再记。
    fn take_over(&mut self, from: &str, v: &V) {
        let exits = exits_of(v);
        self.take_over_exits(|| from.to_string(), exits);
    }

    /// 同上，来源的说明按需才拼（热路径上多数值不带未决）。
    fn take_over_exits(
        &mut self,
        from: impl FnOnce() -> String,
        exits: Vec<(Option<DebtTok>, UnsureCause)>,
    ) {
        let k = self.knobs();
        if !k.takeover || exits.is_empty() {
            return;
        }
        let from = from();
        let top = self.a.frames.len() == 1;
        for (tok, cause) in exits {
            let Some(t) = tok else { continue };
            let f = self.a.frames.last_mut().unwrap();
            if blocks(&k, f.duties.get(&t)) || (top && self.a.handed.contains(&t)) {
                continue;
            }
            let owner = f.owner.clone();
            f.debts.entry(t.clone()).or_insert_with(|| Debt {
                tok: t,
                owner,
                bridge: None,
                nth: 0,
                cause,
                question_key: String::new(),
                from: Some(from.clone()),
            });
        }
    }

    // ── 程序单元 ──

    fn snap(&self, prog: &str) -> Option<u32> {
        *self
            .a
            .settled
            .get(prog)
            .unwrap_or_else(|| panic!("程序 {prog} 不在这次尝试的快照里"))
    }

    /// 等到读：别的程序最新一个定下的版本（按这次尝试的快照）。没定下返回 `None`（记下在等它，R5、B194）。
    /// 读的是整个值，值里带记号的未决全部接下。
    pub fn settled(&mut self, prog: &str) -> Option<V> {
        self.settled_project(prog, |v| v.clone())
    }

    /// 等到读，只取投影出的那部分；只接这一部分里带记号的未决（R10 第 1 条：按元素承接）。依赖照旧按程序版本记。
    pub fn settled_project(&mut self, prog: &str, f: impl FnOnce(&V) -> V) -> Option<V> {
        let ver = self.snap(prog);
        self.dep(Dep::Prog(prog.to_string(), ver));
        let v = ver?;
        let out = f(&self.g.progs[prog].published[v as usize - 1].value);
        self.take_over(&format!("{prog} v{v}"), &out);
        Some(out)
    }

    /// 等到读，只要版本号。
    pub fn settled_version(&mut self, prog: &str) -> Option<u32> {
        let ver = self.snap(prog);
        self.dep(Dep::Prog(prog.to_string(), ver));
        ver
    }

    /// 立刻读：当前最新的版本（可能进行中），版本记进尝试记录（重放在 C3 按它取）。本步照原型不记依赖，
    /// 读过进行中版本的读者在上游定下后被标脏归 C5。读到的未决照样接下（R10 第 7 条）。
    pub fn peek(&mut self, prog: &str) -> Option<Published<V>> {
        self.forbid_in_code("立刻读");
        let p = self.g.progs[prog].published.last().cloned()?;
        self.a
            .peeks
            .push((prog.to_string(), p.version, p.state, p.hash.clone()));
        self.take_over(&format!("{prog} v{}（立刻读）", p.version), &p.value);
        Some(p)
    }

    // ── 多写者单元 ──

    fn shared_version(&mut self, cell: &str) -> u32 {
        let v = *self
            .a
            .shared_snap
            .get(cell)
            .unwrap_or_else(|| panic!("多写者单元 {cell} 不在快照里"));
        self.dep(Dep::Shared(cell.to_string(), v));
        v
    }

    /// 读多写者单元的一个键：读这次尝试开始时快照里的那一版；冲突值读者看得见。读到的未决照样接下（R10 第 4 条）。
    pub fn read_shared(&mut self, cell: &str, key: &str) -> SharedRead<V> {
        let v = self.shared_version(cell);
        let r = self.g.shared[cell].at(v).read(key);
        let from = format!("多写者单元 {cell}[{key}] v{v}");
        for x in r.values() {
            let x = x.clone();
            self.take_over(&from, &x);
        }
        r
    }

    /// 写多写者单元：只经声明的归约器，尝试结束时合并。
    pub fn write_shared(&mut self, cell: &str, key: &str, v: V) {
        self.forbid_in_code("写多写者单元");
        self.a
            .writes
            .push(PendingWrite::Put(cell.to_string(), key.to_string(), v));
    }

    /// 占用（R7）：在占用单元 `cell` 上替这段程序申请整组 `group`。结果未知（没申请过、要的组变了、或被拒而冲突已经
    /// 不在）就写请求、返回「等结果」；读到的正是这一组的结果就返回它。被拒时引擎在当前帧记一笔 `claim_conflict`
    /// 的欠账（Q2），程序要写出、放弃并记账或另交，否则有结论时记违规。
    pub fn claim(&mut self, cell: &str, group: &[String]) -> ClaimResult {
        self.forbid_in_code("占用");
        let v = self.shared_version(cell);
        let state = self.g.shared[cell].at(v).claim_of(&self.a.prog);
        let mut want = group.to_vec();
        want.sort();
        want.dedup();
        let known = match &state {
            None => want.is_empty(),
            Some(s) => s.members == want && (s.granted || !s.still_held_by.is_empty()),
        };
        if !known {
            self.a
                .writes
                .push(PendingWrite::Claim(cell.to_string(), want));
            return ClaimResult::Waiting;
        }
        match state {
            Some(s) if !s.granted => {
                let mut parts: Vec<&str> = vec!["claim", cell];
                parts.extend(want.iter().map(|x| x.as_str()));
                let q = hash_of(&parts);
                let tok = self.owe(BridgeKind::Claim, &q, UnsureCause::ClaimConflict);
                ClaimResult::Rejected {
                    held_by: s.still_held_by,
                    tok,
                }
            }
            _ => ClaimResult::Granted,
        }
    }

    // ── 代码单元 ──

    /// 代码单元：按键记忆，读时收集依赖，脏了才核、核出变了才重算（R4）。`key` 在主语言里是
    /// `jpp_ir::cell_key::code_cell_key` 的 `id`（C2 接）。
    pub fn code(
        &mut self,
        key: &str,
        f: impl Fn(&mut Ctx<'_, V>) -> Result<V, Pend> + 'static,
    ) -> Result<V, Pend> {
        let thunk: Thunk<V> = Rc::new(f);
        let (v, h) = self.ensure(key, Some(thunk))?;
        self.dep(Dep::Code(key.to_string(), h));
        Ok(v)
    }

    /// 读到一个记忆项：把它的欠账与单元去向并进当前帧，再从它的值里接下以转交交出的未决。
    fn receive(&mut self, key: &str) {
        let k = self.knobs();
        let m = &self.g.memo[key];
        if !m.debts.is_empty() || !m.duties.is_empty() || !m.errors.is_empty() {
            let (debts, duties, errors) = (m.debts.clone(), m.duties.clone(), m.errors.clone());
            self.a
                .frames
                .last_mut()
                .unwrap()
                .inherit(&k, &debts, &duties, &errors);
        }
        let exits = exits_of(&self.g.memo[key].value);
        self.take_over_exits(|| format!("代码单元 {key}"), exits);
    }

    /// 记忆项能直接用吗：干净命中，或脏了而核依赖都没变（第一层截断）。能用就把欠账与去向并进当前帧并给值。
    fn memo_usable(&mut self, key: &str) -> Option<(V, String)> {
        let k = self.knobs();
        let m = self.g.memo.get(key)?;
        let (v, h) = (m.value.clone(), m.hash.clone());
        if !self.g.dirty.contains(key) {
            self.g.stats.hits += 1;
            self.receive(key);
            return Some((v, h));
        }
        // 第一层截断：脏了先核依赖，都没变就不重算
        let deps = m.deps.clone();
        if self.deps_unchanged(&deps) {
            self.g.dirty.remove(key);
            self.g.stats.verified_clean += 1;
            if k.inherit_on_verified {
                self.receive(key);
            }
            return Some((v, h));
        }
        None
    }

    fn ensure(&mut self, key: &str, thunk: Option<Thunk<V>>) -> Result<(V, String), Pend> {
        if let Some(x) = self.memo_usable(key) {
            return Ok(x);
        }
        let th = match thunk {
            Some(t) => t,
            // 解释器驱动的记忆项没有求值体（C2c）：引擎修不了它，核依赖时按「变了」处理（从严）
            None => match self
                .g
                .memo
                .get(key)
                .map(|m| m.thunk.clone())
                .expect("没有定义过的代码单元")
            {
                Some(t) => t,
                None => return Err(Pend),
            },
        };
        self.a.frames.push(Frame::new(FrameKind::Cell, key));
        let r = th(self);
        self.finish_code(key, r, Some(th))
    }

    // ── 解释器驱动的代码单元（C2c） ──

    /// 开始求一个代码单元：`force` 为假且记忆项能用（干净命中或核依赖没变）给 `Hit`，并记依赖；否则压一帧给
    /// `Compute`，解释器求值后调 [`Ctx::end_code`]。`force` 为真时一律重算（同一趟内足迹非空的单元，附录二 A2.3）。
    pub fn begin_code(&mut self, key: &str, force: bool) -> CodeStep<V> {
        if !force && let Some((v, h)) = self.memo_usable(key) {
            self.dep(Dep::Code(key.to_string(), h));
            return CodeStep::Hit(v);
        }
        self.a.frames.push(Frame::new(FrameKind::Cell, key));
        CodeStep::Compute
    }

    /// 结束 [`Ctx::begin_code`] 给了 `Compute` 的那一帧：有值就记忆（记忆项没有求值体），并在上一层记依赖；
    /// `Pend`（缺东西或出错）不记忆，依赖与欠账并到上一层。
    pub fn end_code(&mut self, key: &str, r: Result<V, Pend>) -> Result<V, Pend> {
        let (v, h) = self.finish_code(key, r, None)?;
        self.dep(Dep::Code(key.to_string(), h));
        Ok(v)
    }

    /// 当前帧是不是代码单元帧（解释器据此决定判断依赖记在哪一帧）。
    pub fn in_code_frame(&self) -> bool {
        self.a.frames.len() > 1
    }

    /// 代码单元帧里不许占用、写多写者单元、立刻读（Z0510 第二条：它们不进记忆，命中时不会重做）。解释器的纯性
    /// 判定保证走不到，这里是引擎侧的断言。
    fn forbid_in_code(&self, what: &str) {
        if self.in_code_frame() {
            panic!(
                "E-cell-impure：代码单元里不能{what}（Z0510；纯性判定应已把这个函数排除出单元）"
            );
        }
    }

    /// 弹出代码单元帧并收尾：转交、记忆哈希、依赖边、记忆项。
    fn finish_code(
        &mut self,
        key: &str,
        r: Result<V, Pend>,
        th: Option<Thunk<V>>,
    ) -> Result<(V, String), Pend> {
        let k = self.knobs();
        let mut fr = self.a.frames.pop().unwrap();
        match r {
            Err(p) => {
                // 缺东西：不记忆；依赖并到上一层（它们变的时候上层被标脏），欠账也并上去（从严）
                let top = self.a.frames.last_mut().unwrap();
                top.deps.extend(fr.deps);
                top.inherit(&k, &fr.debts, &fr.duties, &fr.errors);
                Err(p)
            }
            Ok(v) => {
                close_cell_frame(&mut fr, &v);
                let vh = v.content_hash();
                // 第二层截断的依据：记忆哈希并上欠账与单元去向的摘要，子单元的责任变了上层不被挡住
                let h = if k.debt_digest {
                    with_debt_digest(vh, &fr.debts, &fr.duties, &fr.errors)
                } else {
                    vh
                };
                self.g.stats.computed += 1;
                let old = self.g.memo.get(key);
                if old.is_some_and(|m| m.hash == h) {
                    self.g.stats.recomputed_same += 1;
                }
                let old_deps = old.map(|m| m.deps.clone()).unwrap_or_default();
                self.g
                    .set_deps(Owner::Code(key.to_string()), &old_deps, &fr.deps);
                self.g.memo.insert(
                    key.to_string(),
                    Memo {
                        value: v.clone(),
                        hash: h.clone(),
                        deps: fr.deps,
                        thunk: th,
                        debts: fr.debts,
                        duties: fr.duties,
                        errors: fr.errors,
                    },
                );
                self.g.dirty.remove(key);
                self.receive(key);
                Ok((v, h))
            }
        }
    }

    fn deps_unchanged(&mut self, deps: &[Dep]) -> bool {
        for d in deps {
            let same = match d {
                Dep::Src(key, v) => self.g.src_at(key, self.a.host_epoch).0 == *v,
                // 读到的与现在的读法比：答了的不再变；缺席的未问过了宿主事件就不在了（F2）。重放归 C3
                Dep::Judge(key, seen) => {
                    let now = match self.judge_now(key) {
                        JudgeRead::Answered(_) => JudgeSeen::Answered,
                        JudgeRead::Unasked(c) => JudgeSeen::Unasked(c),
                        JudgeRead::Pending => JudgeSeen::Pending,
                    };
                    now == *seen
                }
                Dep::Prog(p, v) => self.snap(p) == *v,
                Dep::Shared(c, v) => self.a.shared_snap.get(c) == Some(v),
                // 核依赖时临时求的单元在临时帧里求，欠账不并进读者（读者真读它时才并）
                Dep::Code(key, h) => {
                    self.a.frames.push(Frame::new(FrameKind::Cell, "核依赖"));
                    let r = self.ensure(key, None);
                    self.a.frames.pop();
                    matches!(r, Ok((_, h2)) if h2 == *h)
                }
            };
            if !same {
                return false;
            }
        }
        true
    }
}

/// 尝试结束：拆出记录与交回引擎提交的东西。
pub(crate) fn finish_attempt<V>(mut a: Attempt<V>) -> (AttemptRec, Finished<V>) {
    let top = a.frames.pop().expect("程序段本体的帧");
    assert!(a.frames.is_empty(), "代码单元的帧没有弹出");
    (
        a.rec,
        Finished {
            top,
            handed: a.handed,
            declared: a.declared,
            duties: a.duties,
            pending: a.pending,
            writes: a.writes,
            peeks: a.peeks,
        },
    )
}
