"""J++x 引擎：单元图、常驻程序、尝试、两种读、归约器写、占用、派生、静止点合批、事件链、截止、缺席。

语义按 design/Fable-A-语言裁定.md 第一至第四、第七条；与它的出入写在 jx/README.md「与现行 J++ / Fable-A 的差异」。

- 单元：`cell` 声明的族（宿主或程序写）+ 每个常驻程序实例的输出（可被 peek/settled 读）。每个实例有版本；
  值的哈希没变就不升版本（第二层截断）。
- 依赖边在读的那一刻收集（peek / settled / peek_family / members / depends_on 的 do / claim 被拒）。
- 尝试：单元被标脏 → 排一次尝试；尝试开始前若它上次读过的一切版本都没变、触发值也没变，跳过（第一层截断）。
- 静止：所有在跑的「纤程」都在等（判断、生成、执行、子纤程）时，调度器把已登记的题一起发出。
  所有等待只经过 `wait` 一个卡口，计数在可等待物完成时同步加回。
"""
from __future__ import annotations

import asyncio
import heapq
import itertools
import json
import math
import os
import random
import time
from dataclasses import dataclass, field

from . import ast as A
from .bus import Bus
from .core import Exit, Ledger, canon, h
from .interp import FRAMES, Env, Interp, is_pend
from .parser import load as load_program
from .sched import Account, Chain, FixturePort, HashEncoder, Profile, Sched, Waitable
from .values import Closure, Fail, Handle, JxError, Lazy, LazyExit, Pend, Stop, key_of, to_py

DEFAULT_FLAGS = {"no_cells": False, "no_fill": False, "no_batch": False, "no_meta": False,
                 "no_budget_chain": False, "no_absent": False, "no_deadline": False, "no_cascade": False}
MAX_RERUNS_PER_CHAIN = 12


# ================================================================ 时钟

class RealClock:
    virtual = False

    def __init__(self, fn=None):
        self.fn = fn or time.monotonic

    def __call__(self):
        return self.fn()


class VirtualClock:
    virtual = True

    def __init__(self, t=0.0):
        self.t = t

    def __call__(self):
        return self.t

    def advance_to(self, t):
        self.t = max(self.t, t)


# ================================================================ 单元

@dataclass
class Version:
    no: int
    epoch: int
    value: object
    hash: str
    status: str            # settled | running | removed
    depth: int = 0


class Inst:
    __slots__ = ("fam", "key", "versions", "contrib", "contrib_epoch", "claim_owner", "claim_value", "claim_ver",
                 "unit", "contrib_h")

    def __init__(self, fam, key):
        self.fam = fam
        self.key = key
        self.versions: list[Version] = []
        self.contrib: dict[str, list] = {}
        self.contrib_epoch: dict[str, int] = {}
        self.contrib_h: dict[str, list] = {}
        self.claim_owner = None
        self.claim_value = None
        self.claim_ver = 0
        self.unit = None

    @property
    def latest(self) -> Version | None:
        return self.versions[-1] if self.versions else None

    @property
    def ver(self) -> int:
        return self.versions[-1].no if self.versions else 0

    def settled(self) -> Version | None:
        for v in reversed(self.versions):
            if v.status == "settled":
                return v
            if v.status == "removed":
                return None
        return None

    @property
    def alive(self):
        v = self.latest
        return v is not None and v.status != "removed"


class Family:
    def __init__(self, name, kind, decl, reducer="single", reducer_arg=None):
        self.name = name
        self.kind = kind            # cell | resident
        self.decl = decl
        self.reducer = reducer
        self.reducer_arg = reducer_arg
        self.insts: dict[tuple, Inst] = {}
        self.elem_ver: dict = {}
        self.all_ver = 0
        self.member_ver = 0
        self.binders = []           # [(resident, {param: key 位置})]
        self.by_elem: dict = {}     # 键里的元素 → 含它的实例键（peek_family 用，不扫整族）


class Unit:
    def __init__(self, uid, res, args, key, eng):
        self.uid = uid
        self.res = res
        self.args = args
        self.key = key
        self.src_deps: dict = {}     # dep → source kind
        self.owners: set = set()     # 'auto' 或派生它的单元 uid
        self.children: set = set()
        self.contributed: set = set()   # {(fam, key)}
        self.claims: set = set()
        self.last_reads: dict = {}
        self.last_ev = None
        self.ev = None
        self.dirty = False
        self.forced = True
        self.running = None          # 在跑的 Attempt
        self.rerun = False
        self.chain = None
        self.account = None
        self.deadline = None
        self.status = "new"
        self.pending = []
        self.waits: dict = {}        # dep → 截止（绝对时刻或 None）
        self.wait_started: dict = {}
        self.expired: set = set()
        self.chain_runs: dict = {}
        self.retracted = False
        self.runs = 0
        self.skips = 0
        self.error = None
        self.base_depth = 0           # 派生者的触发深度（spawn 时继承）
        self.whys: set = set()        # 这次重算的原因（观测：「为什么重算」要能解释）
        self.chain_acct = None        # (链序号, 账户, 截止)：resident 的 budget/deadline 子句按链派生
        self.acct_root = None         # 继承谁的接入账户（带子句的根程序 uid）
        self.rank_hint = None         # spawn … rank 给的排序值（取最大）
        self.admitted = False         # 已从所在链的级联预算准入，下一次尝试照常跑程序体
        self.cascade_chain = None     # 准入时记账的链
        self.cascade_reserve = 0      # 准入时预留的调用数

    def trigger_depth(self, eng) -> int:
        """深度按触发链（Fable-A 第四条第 3 闸）：触发这个程序的来源单元版本的层数，或派生者传下来的层数。"""
        d = self.base_depth
        for dep in self.src_deps:
            if dep[0] == "i":
                F = eng.fams.get(dep[1])
                inst = F.insts.get(dep[2]) if F else None
                if inst is not None and inst.latest is not None:
                    d = max(d, inst.latest.depth)
        return d


class ParkSignal(BaseException):
    """级联预算（预注册 16）：没准入的级联尝试要发新题，作废这次尝试（BaseException：不被程序里的错误处理吞掉）。"""


class Attempt:
    """一次尝试的上下文（解释器里的 cx）。"""

    def __init__(self, eng, unit: Unit | None, uid: str, chain, account, ev=None):
        self.eng = eng
        self.unit = unit
        self.uid = uid
        self.chain = chain
        self.account = account
        self.ev = ev
        self.reqs = []
        self.pending = []
        self._pkeys = set()
        self.unsure_source = None
        self.reads: dict = {}
        self.read_fams: set = set()
        self.puts: list = []
        self.claims: set = set()
        self.spawned: set = set()
        self.waits: dict = {}
        self.depth = 0
        self.fam_depth: dict = {}      # 族 → 读到的该族版本的最大组合层数
        self.do_memo: dict = {}
        self.lifted: set = set()
        self.trigger_depth = 0
        self.deadline = unit.deadline if unit else None
        self.pool = None               # 准入的级联尝试：调用记到这条链的级联账上（预注册 16）
        self.pool_calls = 0
        self.rank = None               # 没准入的级联尝试的排序值：要发新题时作废并挂起

    def add_pending(self, lx, ex: Exit):
        k = lx.lr.req.key
        if k in self._pkeys:
            return
        self._pkeys.add(k)
        q = lx.lr.q
        self.pending.append({"q": q.text, "key": q.key, "cause": ex.cause, "needed": ex.needed,
                             "ask_to": list(ex.ask_to), "waiting": ex.waiting,
                             "owners": list(lx.lr.state.owners), "lean": ex.lean})

    def pending_handled(self, ex):
        pass


# ================================================================ 引擎

class Engine:
    def __init__(self, prog, libs, ports=None, flags=None, seed=0, ledger_path=None, events_path=None,
                 clock=None, profile: Profile | None = None, max_calls=None, cache_path=None):
        ports = ports or {}
        self.prog, self.libs = prog, libs
        self.flags = {**DEFAULT_FLAGS, **(flags or {})}
        self.seed = seed
        self.rng = random.Random(seed)
        if clock is None:
            self.clock = RealClock()
        elif isinstance(clock, (RealClock, VirtualClock)):
            self.clock = clock
        else:
            self.clock = RealClock(clock)
        self.ledger = Ledger(ledger_path)
        self.bus = Bus(events_path, self.clock if self.clock.virtual else time.time)
        self.prof = profile or Profile()
        self.port = ports.get("judge") or FixturePort()
        self.gen_port = ports.get("gen")
        self.enc = ports.get("enc") or HashEncoder()
        self.sched = Sched(self, self.port, self.prof, self.flags, self.enc, cache_path)
        self.ip = Interp(self)
        self.fams: dict[str, Family] = {}
        self.residents: dict[str, A.ResidentDecl] = {}
        self.units: dict[str, Unit] = {}
        self.readers: dict = {}
        self.actions: dict = {}
        self.epoch = 0
        self.runnable = 0
        self._check_scheduled = False
        self._pump_scheduled = False
        self._dirty: dict[str, None] = {}
        self._changed = asyncio.Event()
        self.timers = []
        self._tseq = itertools.count()
        self.chain_seq = itertools.count(1)
        self.chains_open = []
        self.degraded_units: set = set()
        self._probe_scheduled = False
        self._emitted_routes = set()
        self._warned = set()
        self._deferred: dict = {}           # 撤回造成的删除：等静止时再定（同一键被新写者接上就只是更新版本）
        self.running_attempts = 0
        self.why = {}                       # (常驻程序名, 重算原因) → 次数
        self.attempts_by = {}
        self._judge_seen = {}
        self._disclose_seen = {}
        self.gen_tasks = 0
        self.attempts = 0
        self.skipped = 0
        self.errors = []
        self.started = False
        # 预算
        b = {}
        if prog.budget is not None:
            b = {k: _lit(x) for k, x in prog.budget.items}
        calls = b.get("calls")
        if max_calls is not None:
            calls = min(calls, max_calls) if calls is not None else max_calls
        self.budget = b
        self.account = Account("run", calls, b.get("cost"))
        self.depth_cap = int(b.get("depth") or 3)
        # 事件链的级联预算（预注册 16）：带 rank 的常驻程序在每条宿主事件链里合计至多 calls 次调用，
        # 按排序值准入（≥ line 的先走），用尽的挂起，等别的事件链让它变脏再排队
        cc = b.get("cascade")
        self.cascade = cc if isinstance(cc, dict) and cc.get("calls") is not None else None
        self.parked: dict = {}              # uid → (排序值, 序号, 链序号)：等准入或已挂起的级联单元
        self._pseq = itertools.count(1)
        self.cascade_max: dict = {}         # 常驻程序名 → 单次尝试见过的最多级联调用数（准入预留用）
        self._admit_scheduled = False
        self._resting: dict = {}            # at_rest 程序里被标脏、等链结束的实例（预注册 17）
        self._gen_blocked: dict = {}        # 在等生成器的尝试 → 等着的纤程数（不挡准入与链结束）
        self._gen_memo: dict = {}           # (种类, 参数哈希) → 生成结果：同一份参数只调一次生成器
        self.parks = 0
        self.admits = 0
        if b.get("latency_p95"):
            self.prof.timeout_s = float(b["latency_p95"])
        self.genv = Env(self.ip.globals)
        self._declare()
        # E-tier 闸：程序声明了两键的 unlocked 单元（或 budget 里 tiers: "名"）就开
        tg = b.get("tiers") if isinstance(b.get("tiers"), str) else "unlocked"
        self.tier_gate = tg if (tg in self.fams and self.fams[tg].kind == "cell" and len(self.fams[tg].decl.keys) == 2) else None
        self.ledger.add("head", seed=seed, flags=self.flags, model=self.prof.model, budget=b,
                        program=prog.file, program_hash=h(prog.src), depth_cap=self.depth_cap)

    # ------------------------------------------------------------ 载入
    @classmethod
    def load(cls, path, **kw):
        prog, libs = load_program(path)
        return cls(prog, libs, **kw)

    @classmethod
    def from_source(cls, src, file="<src>", **kw):
        from .parser import parse
        return cls(parse(src, file), [], **kw)

    def _all_stmts(self):
        for p in self.libs + [self.prog]:
            for st in p.stmts:
                yield st

    def _declare(self):
        for st in self._all_stmts():
            if isinstance(st, A.CellDecl):
                if st.name in self.fams:
                    raise JxError(f"单元 `{st.name}` 重复声明", st)
                self.fams[st.name] = Family(st.name, "cell", st, st.reducer, st.reducer_arg)
            elif isinstance(st, A.ResidentDecl):
                if st.name in self.fams:
                    raise JxError(f"`{st.name}` 已被声明为单元或常驻程序", st)
                self.residents[st.name] = st
                self.fams[st.name] = Family(st.name, "resident", st)
            elif isinstance(st, A.FnDecl):
                self.ip.global_fns[st.name] = st.fn
            elif isinstance(st, A.Let) and isinstance(st.e, A.FnLit):
                self.ip.global_fns[st.name] = st.e
        for r in self.residents.values():
            for s in r.sources:
                if s.kind in ("change", "settled"):
                    ref = s.arg
                    if ref.name not in self.fams:
                        raise JxError(f"常驻程序 {r.name} 的来源引用了未声明的单元 `{ref.name}`", ref)
                    pos = {}
                    ok = True
                    for i, k in enumerate(ref.keys):
                        if isinstance(k, A.Var) and k.name in r.params:
                            pos[k.name] = i
                        else:
                            ok = False
                    if ok and r.params and set(pos) == set(r.params):
                        self.fams[ref.name].binders.append((r, pos))

    def has_family(self, name):
        return name in self.fams

    def register_action(self, name, fn, cost_usd=0.0, transparent=True, depends_on=None):
        self.actions[name] = {"fn": fn, "cost": cost_usd, "transparent": transparent,
                              "depends_on": list(depends_on or [])}

    # ------------------------------------------------------------ 生命周期
    async def start(self):
        if self.started:
            return
        self.started = True
        self._changed = asyncio.Event()
        cx = Attempt(self, None, "main", None, self.account)
        w = self.fiber(self._run_toplevel(cx), cx)
        await self.wait(w, None, count=False)
        for r in self.residents.values():
            if not r.params:
                u = self._unit(r, [], "auto", None)
                self._mark(u, None, None, force=True)
        self._kick()

    async def _run_toplevel(self, cx):
        for p in self.libs + [self.prog]:
            for st in p.stmts:
                if isinstance(st, (A.CellDecl, A.ResidentDecl)):
                    continue
                await self.ip.exec_stmt(st, self.genv, cx)
        self._commit_puts(cx, None, "settled")

    async def report(self):
        """在静止后对程序的结果表达式求值（读用 peek）。"""
        if self.prog.final is None:
            return None
        cx = Attempt(self, None, "report", None, self.account)

        async def go():
            v = await self.ip.ev(self.prog.final, Env(self.genv), cx)
            return await self.ip.deep(v, cx)
        w = self.fiber(go(), cx)
        v = await self.wait(w, None, count=False)
        return to_py(v)

    async def idle(self, timeout=None, advance=True, until=None):
        """等到静止：没有脏单元、没有在跑的尝试、没有排队或在飞的判断。虚拟时钟下把时钟拨到下一个定时点继续。"""
        t_end = None if timeout is None else time.monotonic() + timeout
        while True:
            if self.parked and self._admit_ready() and self._admit():
                self._kick()
                await asyncio.sleep(0)
                continue
            if self._resting and self._admit_ready():
                self._start_resting()
                await asyncio.sleep(0)
                continue
            if self._quiet_fast() and self._deferred:
                self._flush_removals()
                await asyncio.sleep(0)
                continue
            if self._quiet():
                due = [x for x in self.timers if not x[3] and (until is None or x[0] <= until)]
                if due and self.clock.virtual and advance:
                    x = min(due)
                    self.timers.remove(x)
                    heapq.heapify(self.timers)
                    self.clock.advance_to(x[0])
                    x[2]()
                    self._kick()
                    await asyncio.sleep(0)
                    continue
                self._close_chains()
                return
            self._changed.clear()
            try:
                if t_end is None:
                    await asyncio.wait_for(self._changed.wait(), 5.0)
                else:
                    rem = t_end - time.monotonic()
                    if rem <= 0:
                        self.ledger.add("idle_timeout", **self.quiet_report())
                        return
                    await asyncio.wait_for(self._changed.wait(), min(rem, 5.0))
            except asyncio.TimeoutError:
                pass

    def quiet_report(self) -> dict:
        """没静下来时，是哪一项不为零（诊断用：10-04 全量在离开阶段停在 0% CPU 却不静止）。"""
        return {"dirty": len(self._dirty), "runnable": self.runnable, "queue": len(self.sched.queue),
                "heap": len(self.sched.heap), "active": self.sched.active, "gen_tasks": self.gen_tasks,
                "running_attempts": self.running_attempts, "deferred": len(self._deferred), "parked": len(self.parked), "resting": len(self._resting),
                "dirty_units": [getattr(u, "uid", str(u)) for u in list(self._dirty)[:5]]}

    def _quiet(self):
        return self._quiet_fast() and not self._deferred and not self._resting

    async def stop(self):
        self._emit_stats()
        if hasattr(self.port, "close"):
            try:
                await self.port.close()
            except Exception:
                pass
        if hasattr(self.port, "save"):
            self.port.save()
        self.ledger.close()
        self.bus.close()

    def changed(self):
        self._changed.set()
        if self._deferred:
            self._changed_soon()
        if self.parked or self._resting:
            self._admit_soon()

    # ------------------------------------------------------------ 卡口：纤程与等待
    def fiber(self, coro, cx) -> Waitable:
        w = Waitable()
        self.runnable += 1

        async def main():
            FRAMES.set([])
            try:
                v = await coro
                self.resolve(w, v)
            except BaseException as e:   # noqa: BLE001
                self.resolve(w, None, exc=e)
            finally:
                self.runnable -= 1
                assert self.runnable >= 0, "runnable 计数为负"
                self._check()
                self.changed()
        asyncio.get_running_loop().create_task(main())
        return w

    async def wait(self, w, cx, count=True):
        if not w.done:
            if w.fut is None:
                w.fut = asyncio.get_running_loop().create_future()
            if count:
                w.blocked += 1
                self.runnable -= 1
                assert self.runnable >= 0, "runnable 计数为负"
                self._check()
            g = w.is_gen and cx is not None and cx.unit is not None
            if g:
                self._gen_blocked[cx] = self._gen_blocked.get(cx, 0) + 1
                self.changed()
            try:
                await w.fut
            finally:
                if g:
                    n = self._gen_blocked.get(cx, 1) - 1
                    if n <= 0:
                        self._gen_blocked.pop(cx, None)
                    else:
                        self._gen_blocked[cx] = n
        if w.exc is not None:
            if isinstance(w.exc, (JxError, Stop, ParkSignal)):    # 并发回调里的作废信号原样往上传（预注册 16）
                raise w.exc
            import traceback
            tb = "".join(traceback.format_exception(w.exc)[-6:])
            raise JxError(f"{type(w.exc).__name__}: {w.exc}\n{tb}")
        return w.value

    def resolve(self, w, value, exc=None):
        if w.done:
            return
        w.done = True
        w.value = value
        w.exc = exc
        self.runnable += w.blocked
        w.blocked = 0
        if w.fut is not None and not w.fut.done():
            w.fut.set_result(None)
        self.changed()

    def _check(self):
        if not self._check_scheduled:
            self._check_scheduled = True
            asyncio.get_running_loop().call_soon(self._quiesce)

    def _quiesce(self):
        self._check_scheduled = False
        if self.runnable == 0 and self.sched.queue and not self.flags["no_batch"]:
            self.sched.flush()
        self.changed()

    async def before_block(self, cx):
        if self.flags["no_batch"] and cx is not None:
            mine = [r for r in cx.reqs if not r.sent and not r.done]
            if mine:
                self.sched.flush(only=mine)

    # ------------------------------------------------------------ 定时器
    def at(self, t, cb, periodic=False):
        if self.clock.virtual:
            heapq.heappush(self.timers, (t, next(self._tseq), cb, periodic))
        else:
            loop = asyncio.get_running_loop()
            loop.call_later(max(0.0, t - self.clock()), lambda: (cb(), self._kick()))

    # ------------------------------------------------------------ 单元与依赖
    def _fam(self, name, node=None) -> Family:
        f = self.fams.get(name)
        if f is None:
            raise JxError(f"未声明的单元 `{name}`", node)
        return f

    def _inst(self, fam: Family, key: tuple, create=True) -> Inst | None:
        i = fam.insts.get(key)
        if i is None and create:
            i = Inst(fam.name, key)
            fam.insts[key] = i
            for e in set(_elems(key)):
                fam.by_elem.setdefault(e, set()).add(key)
        return i

    def dep_ver(self, dep):
        kind = dep[0]
        fam = self.fams.get(dep[1])
        if fam is None:
            return 0
        if kind == "i":
            i = fam.insts.get(dep[2])
            return i.ver if i else 0
        if kind == "f":
            return fam.all_ver if dep[2] is None else fam.elem_ver.get(dep[2], 0)
        if kind == "m":
            return fam.member_ver
        if kind == "c":
            i = fam.insts.get(dep[2])
            return i.claim_ver if i else 0
        return 0

    def _record(self, cx, dep):
        if cx is None:
            return
        cx.reads[dep] = self.dep_ver(dep)
        cx.read_fams.add(dep[1])

    def _notify(self, dep, chain, ev=None, settled=False):
        for uid in list(self.readers.get(dep, ())):
            u = self.units.get(uid)
            if u is None or u.retracted:
                continue
            kind = u.src_deps.get(dep)
            if kind == "change" or (kind == "settled" and settled):
                self._mark(u, chain, ev, why=f"来源:{dep[1]}")
            elif dep in u.waits and settled:
                self._mark(u, chain, None, why=f"等到:{dep[1]}")
            elif dep in u.last_reads and u.last_reads[dep] != self.dep_ver(dep):
                self._mark(u, chain, None, why=f"读过:{dep[1]}")

    def _add_reader(self, dep, uid):
        self.readers.setdefault(dep, set()).add(uid)

    def _del_reader(self, dep, uid):
        s = self.readers.get(dep)
        if s:
            s.discard(uid)

    # ------------------------------------------------------------ 发布
    def _publish(self, inst: Inst, value, status, depth, chain, writer="") -> bool:
        fam = self.fams[inst.fam]
        hv = h(to_py(value)) if status != "removed" else "removed"
        last = inst.latest
        if (last is not None and last.hash == hv and last.status == status and not self.flags["no_cells"]):
            return False
        if last is None and status == "removed":
            return False
        created = last is None or last.status == "removed"
        self.epoch += 1
        v = Version((last.no + 1) if last else 1, self.epoch, value, hv, status, depth)
        inst.versions.append(v)
        if len(inst.versions) > 6:
            # 保留最近的已定版本
            keep = inst.versions[-5:]
            st = inst.settled()
            if st is not None and st not in keep:
                keep = [st] + keep
            inst.versions = keep
        fam.all_ver += 1
        for e in set(_elems(inst.key)):
            fam.elem_ver[e] = fam.elem_ver.get(e, 0) + 1
        if created or status == "removed":
            fam.member_ver += 1
        prev = None
        if last is not None and last.status != "removed":
            prev = last.value
        self.ledger.add("publish", cell=inst.fam, key=_jkey(inst.key), version=v.no, status=status, hash=hv,
                        depth=depth, writer=writer)
        pv = to_py(value) if status != "removed" else None
        extra = {}
        if fam.reducer == "union" and isinstance(pv, list):
            n_prev = len(prev) if isinstance(prev, list) else 0
            extra["added"] = pv[n_prev:]                   # 只增集合：新加的元素（文件里只写这一段）
        self.bus.emit("publish", cell=inst.fam, key=list(_jkey(inst.key)), version=v.no, status=status,
                      value=pv, prev=to_py(prev), **extra)
        settled = status == "settled"
        self._notify(("i", inst.fam, inst.key), chain, value, settled)
        for e in set(_elems(inst.key)):
            self._notify(("f", inst.fam, e), chain, None, settled)
        self._notify(("f", inst.fam, None), chain, None, settled)
        if created or status == "removed":
            self._notify(("m", inst.fam), chain, None, settled)
        if created and status != "removed":
            for res, pos in fam.binders:
                args = [inst.key[pos[p]] for p in res.params]
                u = self._unit(res, args, "auto", chain)
                self._mark(u, chain, value, force=True, why=f"新实例:{inst.fam}")
        if status == "removed":
            for res, pos in fam.binders:
                args = [inst.key[pos[p]] for p in res.params]
                uid = _uid(res.name, key_of(args))
                u = self.units.get(uid)
                if u is not None:
                    u.owners.discard("auto")
                    if not u.owners:
                        self._retract(u, chain)
        return True

    def _fold(self, fam: Family, inst: Inst):
        """按归约器把各写者的贡献合成单元值。写者按 uid 排序，合并顺序进账本。"""
        ws = sorted(w for w, vs in inst.contrib.items() if vs)
        if not ws:
            return None, False
        red = fam.reducer
        if red == "single" or red == "override":
            if red == "single" and len(ws) > 1 and (fam.name, inst.key) not in self._warned:
                self._warned.add((fam.name, inst.key))
                self.ledger.add("warn", what="single 单元有多个写者，取最近提交的", cell=fam.name,
                                key=_jkey(inst.key), writers=[w[:60] for w in ws[:3]], n=len(ws))
            w = max(ws, key=lambda x: (inst.contrib_epoch.get(x, 0), x))
            return inst.contrib[w][-1], True
        if red == "union":
            out, seen = [], set()
            for w in ws:
                vs = inst.contrib[w]
                hs = inst.contrib_h.get(w)
                items = [it for v in vs for it in (v if isinstance(v, list) else [v])]
                if hs is None or len(hs) != len(items):
                    hs = [h(to_py(it)) for it in items]
                    inst.contrib_h[w] = hs
                for it, k in zip(items, hs):
                    if k not in seen:
                        seen.add(k)
                        out.append(it)
            return out, True
        if red == "by_key":
            f = fam.reducer_arg
            d = {}
            for w in sorted(ws, key=lambda x: (inst.contrib_epoch.get(x, 0), x)):
                for v in inst.contrib[w]:
                    for it in (v if isinstance(v, list) else [v]):
                        if isinstance(it, dict) and f in it:
                            d[str(it[f])] = it
            return d, True
        return None, False

    def _apply(self, fam: Family, inst: Inst, status, depth, chain, writer, defer=True):
        val, alive = self._fold(fam, inst)
        k = (fam.name, inst.key)
        if not alive:
            if inst.alive:
                # 预注册 15：宿主删除（离开）引起的撤回当场生效——推迟到静止会让离开者在静止前继续进判断，
                # 且静止前同值再接入会取消删除、不升版本，离开期间撤掉的派生再没人重算（预注册 13，X29-0023/24）
                leaving = str(getattr(chain, "cause", "")).startswith("remove:")
                if defer and writer != "host" and not leaving:
                    self._deferred[k] = (fam, inst, depth, chain, writer)
                    self._changed_soon()
                else:
                    self._publish(inst, None, "removed", depth, chain, writer)
            return
        self._deferred.pop(k, None)
        self._publish(inst, val, status, depth, chain, writer)

    def _flush_removals(self):
        """静止时处理撤回造成的删除：这期间没有新写者接上的才真的删（构型身份 = 成员集合键，重算不抖动）。"""
        items, self._deferred = list(self._deferred.values()), {}
        for fam, inst, depth, chain, writer in items:
            _, alive = self._fold(fam, inst)
            if not alive and inst.alive:
                self._publish(inst, None, "removed", depth, chain, writer)
        self._kick()

    def _quiet_fast(self):
        return (not self._dirty and self.runnable == 0 and not self.sched.queue and not self.sched.heap
                and self.sched.active == 0 and self.gen_tasks == 0 and self.running_attempts == 0)

    def _changed_soon(self):
        try:
            asyncio.get_running_loop().call_soon(self._settle_check)
        except RuntimeError:
            pass

    def _settle_check(self):
        if self._deferred and self._quiet_fast():
            self._flush_removals()

    # ------------------------------------------------------------ 常驻程序实例
    def _unit(self, res, args, owner, chain) -> Unit:
        key = key_of(args)
        uid = _uid(res.name, key)
        u = self.units.get(uid)
        if u is None:
            u = Unit(uid, res, args, key, self)
            u.account = self.account
            self.units[uid] = u
            out = self._inst(self.fams[res.name], key)
            out.unit = uid
            # 来源：按实参求出具体的单元实例
            for s in res.sources:
                if s.kind in ("change", "settled"):
                    try:
                        kv = tuple(_static_eval(k, dict(zip(res.params, args))) for k in s.arg.keys)
                    except Exception:
                        kv = None
                    if kv is None or any(isinstance(x, _Wild) for x in kv):
                        dep = ("f", s.arg.name, None)
                    else:
                        dep = ("i", s.arg.name, kv)
                    u.src_deps[dep] = s.kind
                    self._add_reader(dep, uid)
                elif s.kind == "timer":
                    self._timer_loop(u, float(s.arg))
            self.ledger.add("unit", uid=uid, owner=owner[:60], args=canon(to_py(args))[:200])
        if u.retracted:
            u.retracted = False
        u.owners.add(owner)
        return u

    def _timer_loop(self, u, every):
        def fire():
            if u.retracted or u.uid not in self.units:
                return
            self._mark(u, None, {"t": self.clock()}, force=True)
            self.at(self.clock() + every, fire, periodic=True)
        self.at(self.clock() + every, fire, periodic=True)

    def _mark(self, u: Unit, chain, ev=None, force=False, why="forced"):
        if u.retracted:
            return
        u.whys.add(why)
        if ev is not None:
            u.ev = ev
        if chain is not None:
            u.chain = chain
        if force:
            u.forced = True
        if u.running is not None:
            u.rerun = True
            return
        if not u.dirty:
            u.dirty = True
            self._dirty[u.uid] = None
            self._kick()

    def _kick(self):
        if not self._pump_scheduled:
            self._pump_scheduled = True
            try:
                asyncio.get_running_loop().call_soon(self._pump)
            except RuntimeError:
                self._pump_scheduled = False

    def _pump(self):
        self._pump_scheduled = False
        if not self._dirty:
            self.changed()
            return
        batch = sorted(self._dirty)
        self._dirty.clear()
        self.rng.shuffle(batch)
        # 先来的接入事件链先起尝试（同一条链内按种子打乱）
        pos = {uid: i for i, uid in enumerate(batch)}

        def ck(uid):
            u = self.units.get(uid)
            return (u.chain.seq if (u is not None and u.chain is not None) else 0, pos[uid])
        batch.sort(key=ck)
        # 按依赖序起尝试（无毛刺）：一个脏单元上次读过的单元，若写它的程序也脏着或正在跑，就先等它跑完，
        # 免得用旧值算一遍、上游一发布又重来。全都互相等着（成环）时照常起。
        active = set(batch) | {uid for uid, u in self.units.items() if u.running is not None}
        started, waiting = 0, []
        for uid in batch:
            u = self.units.get(uid)
            if u is None or u.retracted:
                continue
            if u.running is not None:
                u.dirty = False
                u.rerun = True
                continue
            if u.res.at_rest:                 # 链结束时才跑（预注册 17）：先放进等候表，标脏状态保留
                self._resting[uid] = None
                continue
            if not self.flags["no_cells"] and self._upstream_busy(u, active):
                waiting.append(u)
                continue
            u.dirty = False
            self._start(u)
            started += 1
        if waiting:
            if started == 0 and self.running_attempts == 0:
                for u in waiting:             # 成环：照常起
                    u.dirty = False
                    self._start(u)
            else:
                for u in waiting:
                    self._dirty[u.uid] = None
        self.changed()

    def _upstream_busy(self, u, active) -> bool:
        for dep in u.last_reads:
            if dep[0] != "i":
                continue
            F = self.fams.get(dep[1])
            inst = F.insts.get(dep[2]) if F else None
            if inst is None:
                continue
            if inst.unit is not None and inst.unit != u.uid and inst.unit in active:
                return True
            for w in inst.contrib:
                if w != u.uid and w in active:
                    return True
        return False

    def _start(self, u: Unit):
        chain = u.chain
        clause_dl = None
        root = u if _has_clause(u.res) else (self.units.get(u.acct_root) if u.acct_root else None)
        if root is not None and chain is not None:
            # 接入预算子句：根程序被一条事件链触发时，从链账户派生一个子账户（只收紧）、定截止；
            # 由它派生的整条下游（spawn 的程序、补信息请求）在同一条链里共用这个账户与截止
            acct, clause_dl = self._clause_acct(root, chain)
        elif u.account is not None and u.account is not self.account:
            acct = u.account                       # spawn 时给了预算或继承了派生者的账户
        elif chain is not None and not self.flags["no_budget_chain"]:
            acct = chain.account
        else:
            acct = self.account
        cx = Attempt(self, u, u.uid, chain, acct, u.ev)
        cx.trigger_depth = u.trigger_depth(self)
        # 截止：链的截止、子句截止与单元自己的截止取最早
        if not self.flags["no_deadline"]:
            for d in (chain.deadline if chain is not None else None, clause_dl):
                if d is not None:
                    cx.deadline = d if cx.deadline is None else min(cx.deadline, d)
        u.running = cx
        u.status = "running"
        self.running_attempts += 1
        self.fiber(self._attempt(u, cx), cx)

    def _clause_acct(self, root, chain):
        if root.chain_acct is None or root.chain_acct[0] != chain.seq:
            res = root.res
            base = chain.account if not self.flags["no_budget_chain"] else self.account
            acct_c = base
            if res.budget is not None and not self.flags["no_budget_chain"]:
                b = {k: (x.v if isinstance(x, A.Num) else None) for k, x in res.budget.items}
                acct_c = base.child(f"{root.uid}@{chain.seq}", b.get("calls"), b.get("cost"))
            dl = None
            if isinstance(res.deadline, A.Num) and not self.flags["no_deadline"]:
                dl = self.clock() + float(res.deadline.v)
                self.at(dl, self.sched.expire_due)
            root.chain_acct = (chain.seq, acct_c, dl)
        return root.chain_acct[1], root.chain_acct[2]

    async def _attempt(self, u: Unit, cx: Attempt):
        try:
            # 每条事件链里一个单元最多重跑若干次（无进展终止线）
            ck = cx.chain.seq if cx.chain else 0
            u.chain_runs[ck] = u.chain_runs.get(ck, 0) + 1
            if u.chain_runs[ck] > MAX_RERUNS_PER_CHAIN:
                self.ledger.add("noprogress", unit=u.uid, chain=ck, why=sorted(u.whys)[:4])
                u.whys = set()
                u.status = "settled" if not u.waits else "running"
                return
            # 第一层截断：上次读过的版本、触发值都没变 → 不重算
            evh = h(to_py(u.ev))
            if (not self.flags["no_cells"] and not u.forced and u.runs > 0
                    and all(self.dep_ver(d) == v for d, v in u.last_reads.items())
                    and evh == u.last_ev and not u.expired - set(u.last_reads)):
                u.skips += 1
                self.skipped += 1
                self.ledger.add("cutoff", unit=u.uid)
                u.status = "settled" if not u.waits else "running"
                return
            # 级联预算（预注册 16）：带排序值的实例在要发出第一道新题（不是缓存、不是并到在飞的同一道题）时
            # 若还没准入，这次尝试作废（不写、不撤回旧值），单元进所在链的待准入表；引擎静止时按排序值准入，
            # 用尽的留在表里，等别的事件链让它变脏再排队。不发题的尝试（如成员离开后的撤回）照常跑完。
            if self._cascade_on() and not u.admitted and cx.chain is not None:
                cx.rank = await self._rank_of(u)
            if u.admitted:
                cx.pool = u.cascade_chain
            saved = (u.forced, set(u.whys))
            u.forced = False
            u.runs += 1
            self.attempts += 1
            rn = u.res.name
            self.attempts_by[rn] = self.attempts_by.get(rn, 0) + 1
            whys = sorted(u.whys or {"?"})
            for w in whys:
                self.why[(rn, w)] = self.why.get((rn, w), 0) + 1
            u.whys = set()
            if cx.chain:
                cx.chain.reruns += 1
            self.ledger.add("attempt", unit=u.uid, run=u.runs, chain=ck, why=whys[:4])
            env = Env(self.genv)
            for p, a in zip(u.res.params, u.args):
                env.set(p, a)
            val = await self.ip.run_block(u.res.body, env, cx)
            val = await self.ip.deep(val, cx)
            i = 0
            while i < len(cx.puts):        # 强制写入值（可能触发默认链，追加新的写）
                fam, key, v, node, internal = cx.puts[i]
                cx.puts[i] = (fam, key, await self.ip.deep(v, cx), node, internal)
                i += 1
            if u.retracted:
                return
            status = "running" if cx.waits else "settled"
            self._commit(u, cx, val, status, evh)
        except ParkSignal:
            u.forced = saved[0]
            u.whys = saved[1] | u.whys
            u.runs -= 1
            u.chain_runs[ck] -= 1
            rk = cx.rank if cx.rank is not None else await self._rank_of(u)   # 准入后中途碰到上限的也挂起
            self._park(u, cx.chain, rk if rk is not None else 0.0)
        except JxError as e:
            self._fail(u, cx, e)
        except Exception as e:      # noqa: BLE001
            import traceback
            self._fail(u, cx, JxError(f"{type(e).__name__}: {e}\n{traceback.format_exc(limit=4)}"))
        finally:
            self.sched.drop_unforced(cx)
            if u.admitted:                 # 准入只管一次尝试（含被截断的）；下次变脏再排队
                self._release_admission(u)
            u.running = None
            self.running_attempts -= 1
            if u.rerun and not u.retracted:
                u.rerun = False
                self._mark(u, u.chain, None, why="跑时又变")
            if self._dirty:
                self._kick()              # 等着它的下游可以起了
            self.changed()

    # ------------------------------------------------------------ 事件链的级联预算（预注册 16）
    def _cascade_on(self) -> bool:
        return self.cascade is not None and not self.flags["no_cascade"]

    async def _rank_of(self, u):
        """排序值：rank 子句的值（不是 unit 时），否则 spawn … rank 给的值；都没有返回 None（不参与级联）。
        子句里的读不记为依赖（用一次性的上下文）。"""
        v = None
        if u.res.rank is not None:
            cx = Attempt(self, None, f"rank:{u.uid}", None, self.account)
            env = Env(self.genv)
            for p, a in zip(u.res.params, u.args):
                env.set(p, a)
            try:
                v = to_py(await self.ip.deep(await self.ip.ev(u.res.rank, env, cx), cx))
            except JxError as e:
                self.errors.append({"unit": u.uid, "error": f"rank: {e}"})
                self.ledger.add("error", unit=u.uid, error=f"rank: {str(e)[:500]}")
                v = None
        if not (isinstance(v, (int, float)) and not isinstance(v, bool)):
            v = u.rank_hint
        return None if v is None else float(v)

    def _park(self, u, chain, rk):
        prev = self.parked.get(u.uid)
        same = prev is not None and prev[2] == chain.seq
        self.parked[u.uid] = (rk, prev[1] if same else next(self._pseq), chain.seq)
        u.status = "parked"
        if not same:                       # 同一条链上重复挂起不再写账本（驱动按账本判静止）
            self.parks += 1
            self.ledger.add("park", unit=u.uid, chain=chain.seq, rank=round(rk, 4))
        self._admit_soon()

    def _release_admission(self, u):
        ch = u.cascade_chain
        if ch is not None:
            ch.cascade_reserved = max(0, ch.cascade_reserved - u.cascade_reserve)
        u.admitted, u.cascade_chain, u.cascade_reserve = False, None, 0

    def _admit_ready(self) -> bool:
        """静止（不等生成器）：没有脏单元、没有在跑的调用，在跑的尝试都只是在等生成器。"""
        return (not self._dirty and self.runnable == 0 and not self.sched.queue and not self.sched.heap
                and self.sched.active == 0 and self.running_attempts <= len(self._gen_blocked))

    def _start_resting(self):
        """链结束：没有能准入的挂起单元了，at_rest 程序按当时的输入各跑一次（预注册 17）。"""
        items, self._resting = list(self._resting), {}
        for uid in items:
            u = self.units.get(uid)
            if u is None or u.retracted:
                continue
            if u.running is not None:         # 上一次还在等生成器：跑完再来
                u.dirty = False
                u.rerun = True
                continue
            u.dirty = False
            self._start(u)
        self.changed()

    def _admit_soon(self):
        if self._admit_scheduled or not (self.parked or self._resting):
            return
        self._admit_scheduled = True
        try:
            asyncio.get_running_loop().call_soon(self._admit_check)
        except RuntimeError:
            self._admit_scheduled = False

    def _admit_check(self):
        self._admit_scheduled = False
        if not self._admit_ready():
            return
        if self.parked and self._admit():
            self._kick()
        elif self._resting:
            self._start_resting()

    def _admit(self) -> bool:
        """按链准入：排序值 ≥ line 的先走（本链还有线上的就不放线下的），同档按排序值降序、再按登记先后；
        每个预留这个程序以往单次尝试的最多调用数，预留加已花到 calls 为止。"""
        if not self.parked or not self._cascade_on():
            return False
        cap = float(self.cascade["calls"])
        line = self.cascade.get("line")
        groups: dict = {}
        for uid, (rk, seq, _cs) in list(self.parked.items()):
            u = self.units.get(uid)
            if u is None or u.retracted:
                self.parked.pop(uid, None)
                continue
            if u.dirty or u.running is not None or u.chain is None:
                continue
            groups.setdefault(id(u.chain), (u.chain, []))[1].append((rk, seq, u))
        done = False
        for ch, items in groups.values():
            left = cap - ch.cascade_used - ch.cascade_reserved
            if left < 1:
                continue
            above = [it for it in items if line is None or it[0] >= float(line)]
            pool = sorted(above or items, key=lambda it: (-it[0], it[1]))
            for rk, _seq, u in pool:
                if left < 1:
                    break
                est = max(1, int(self.cascade_max.get(u.res.name, 1)))
                if est > left:
                    continue
                left -= est
                ch.cascade_reserved += est
                u.admitted, u.cascade_chain, u.cascade_reserve = True, ch, est
                self.parked.pop(u.uid, None)
                self.admits += 1
                self.ledger.add("admit", unit=u.uid, chain=ch.seq, rank=round(rk, 4), reserve=est)
                self._mark(u, ch, None, why="准入")
                done = True
        return done

    async def resume(self, ids=None, budget=None, cause=None) -> int:
        """宿主事件「续算」：开一条新链，把挂起单元里实参含这些 id 的（不给就是全部）转到这条链上重新排队。"""
        if not self.started:
            await self.start()
        chain = self._new_chain(cause or "resume", budget, None)
        want = None if ids is None else {str(x) for x in ids}
        chain.ids = sorted(want or [])
        n = 0
        for uid in list(self.parked):
            u = self.units.get(uid)
            if u is None or u.retracted:
                continue
            if want is None or (_strs(to_py(u.args)) & want):
                self._mark(u, chain, None, why="续算")
                n += 1
        self.ledger.add("resume", chain=chain.seq, ids=chain.ids[:20], n=n)
        self._kick()
        return n

    def _fail(self, u, cx, e: JxError):
        # 出错的尝试也记下它读过什么：输入变了（例如解锁了更高一层）就重跑
        for d in cx.reads:
            self._add_reader(d, u.uid)
        u.last_reads = dict(cx.reads)
        u.forced = False
        u.status = "error"
        u.error = str(e)
        self.errors.append({"unit": u.uid, "error": str(e)})
        self.ledger.add("error", unit=u.uid, error=str(e)[:2000])
        self.bus.emit("error", unit=u.uid, error=str(e)[:500])

    # ------------------------------------------------------------ 提交
    def _commit_puts(self, cx, u, status):
        """把尝试的写按归约器合入。返回受影响实例。"""
        uid = cx.uid
        by_inst: dict = {}
        depth_of = {}
        out_depth = 0
        for fam, key, v, node, internal in cx.puts:
            F = self._fam(fam, node)
            if F.kind != "cell":
                raise JxError(f"`{fam}` 是常驻程序，不能 put（它的值就是它的程序体的值）", node)
            if F.reducer == "claim":
                raise JxError(f"`{fam}` 的归约器是 claim，只能用 claim 写", node)
            if is_pend(v):
                self.ledger.add("skip", unit=uid, put=fam, why="值未决")
                continue
            d = cx.trigger_depth            # 层数沿触发链携带：node[k] → 召回 → 两两 → adj → 构型 → 整体 → node 整圈都算
            if not internal and fam in cx.read_fams and u is not None:
                # 组合回流：产物写回自己读过的族（构型作为节点）。层数 = 读到的该族最大层数 + 1
                if self.flags["no_meta"]:
                    self.ledger.add("meta_off", unit=uid, cell=fam, key=_jkey(key))
                    continue
                d = cx.trigger_depth + 1
                if d >= self.depth_cap:
                    self.ledger.add("depth", unit=uid, cell=fam, key=_jkey(key), depth=d, cap=self.depth_cap)
                    cx.pending.append({"q": None, "cause": "depth", "needed": f"组合层数到上限 {self.depth_cap}",
                                       "cell": fam, "key": list(_jkey(key))})
                    continue
            depth_of[(fam, key)] = max(depth_of.get((fam, key), 0), d)
            out_depth = max(out_depth, d)
            by_inst.setdefault((fam, key), []).append(v)
        touched = []
        for (fam, key), vs in by_inst.items():
            F = self.fams[fam]
            inst = self._inst(F, key)
            old = inst.contrib.get(uid)
            oldh = inst.contrib_h.get(uid)
            if F.reducer == "union":
                # union 是只增集合：同一写者历次写入累积，不随重算撤回（消息、已解锁片段）
                items = [it for v in vs for it in (v if isinstance(v, list) else [v])]
                hs = [h(to_py(it)) for it in items]
                if old:
                    seen = set(oldh)
                    new = [(x, k) for x, k in zip(items, hs) if k not in seen]
                    vs = list(old) + [x for x, _ in new]
                    newh = list(oldh) + [k for _, k in new]
                else:
                    vs, newh = items, hs
            else:
                newh = [h(to_py(vs))]
            inst.contrib[uid] = vs
            inst.contrib_h[uid] = newh
            inst.contrib_epoch[uid] = self.epoch
            if old is None or oldh != newh or self.flags["no_cells"] or (inst.latest and inst.latest.status != status):
                touched.append((F, inst))
        if u is not None and status == "settled":
            for (fam, key) in list(u.contributed - set(by_inst)):
                F = self.fams[fam]
                inst = F.insts.get(key)
                if F.reducer == "union":
                    continue
                if inst is not None and uid in inst.contrib:
                    del inst.contrib[uid]
                    touched.append((F, inst))
            u.contributed = set(by_inst) | {(f, k) for (f, k) in u.contributed if self.fams[f].reducer == "union"}
        elif u is not None:
            u.contributed |= set(by_inst)
        for F, inst in touched:
            self._apply(F, inst, status, depth_of.get((F.name, inst.key), 0), cx.chain, uid)
        return out_depth

    def _commit(self, u: Unit, cx: Attempt, val, status, evh):
        out_depth = self._commit_puts(cx, u, status)
        if status == "settled":
            for ik in list(u.claims - cx.claims):
                self._release(u, ik, cx.chain)
            u.claims = set(cx.claims)
            for cid in list(u.children - cx.spawned):
                c = self.units.get(cid)
                if c is not None:
                    c.owners.discard(u.uid)
                    if not c.owners:
                        self._retract(c, cx.chain)
            u.children = set(cx.spawned)
        else:
            u.claims |= cx.claims
            u.children |= cx.spawned
        # 依赖边：换成这次读到的
        for d in set(u.last_reads) - set(cx.reads):
            if d not in u.src_deps:
                self._del_reader(d, u.uid)
        for d in cx.reads:
            self._add_reader(d, u.uid)
        u.last_reads = dict(cx.reads)
        u.last_ev = evh
        # 等待：settled 读到未定的单元
        now = self.clock()
        for d in list(u.wait_started):
            if d not in cx.waits:
                del u.wait_started[d]
        for d, dl in cx.waits.items():
            self._add_reader(d, u.uid)
            if d not in u.wait_started:
                u.wait_started[d] = now
                if dl is not None and not self.flags["no_deadline"]:
                    t = now + dl[1] if dl[0] == "rel" else dl[1]
                    self.at(t, self._expire_wait(u, d))
        u.waits = dict(cx.waits)
        u.expired &= set(cx.waits) | set(cx.reads)
        u.pending = list(cx.pending)
        u.status = status
        out = self._inst(self.fams[u.res.name], u.key)
        self._publish(out, val, status, out_depth, cx.chain, u.uid)
        # 读完之后别人又发布了 → 再来一次
        for d, v in cx.reads.items():
            if self.dep_ver(d) != v:
                self._mark(u, cx.chain, None)
                break

    def _expire_wait(self, u, dep):
        def fire():
            if u.retracted or dep not in u.waits:
                return
            fam = self.fams.get(dep[1])
            inst = fam.insts.get(dep[2]) if fam and dep[0] == "i" else None
            if inst is not None and inst.settled() is not None:
                return
            u.expired.add(dep)
            self.ledger.add("deadline", unit=u.uid, wait=list(_jkey(dep[2])) if dep[0] == "i" else None)
            self._mark(u, u.chain, None, force=True, why="等待截止")
        return fire

    def _retract(self, u: Unit, chain):
        if u.retracted:
            return
        u.retracted = True
        self.ledger.add("retract", unit=u.uid)
        self.parked.pop(u.uid, None)
        self._resting.pop(u.uid, None)
        if u.admitted:
            self._release_admission(u)
        for (fam, key) in list(u.contributed):
            F = self.fams[fam]
            inst = F.insts.get(key)
            if F.reducer == "union":
                continue
            if inst is not None and u.uid in inst.contrib:
                del inst.contrib[u.uid]
                self._apply(F, inst, "settled", 0, chain, u.uid)
        u.contributed = set()
        for ik in list(u.claims):
            self._release(u, ik, chain)
        for cid in list(u.children):
            c = self.units.get(cid)
            if c is not None:
                c.owners.discard(u.uid)
                if not c.owners:
                    self._retract(c, chain)
        for d in list(u.last_reads) + list(u.src_deps) + list(u.waits):
            self._del_reader(d, u.uid)
        out = self.fams[u.res.name].insts.get(u.key)
        if out is not None and out.alive:
            self._publish(out, None, "removed", 0, chain, u.uid)
        self._dirty.pop(u.uid, None)
        del self.units[u.uid]

    # ------------------------------------------------------------ 读写原语（解释器调用）
    def read_cell(self, cx, fam, key, mode, node=None, wait_deadline=None):
        F = self._fam(fam, node)
        key = tuple(key)
        dep = ("i", fam, key)
        if mode != "snap":
            self._record(cx, dep)
        inst = F.insts.get(key)
        if mode in ("peek", "snap"):
            v = inst.latest if inst else None
            if v is None or v.status == "removed":
                return None
            _seen_depth(cx, fam, v.depth)
            return v.value
        v = inst.settled() if inst else None
        if v is not None:
            _seen_depth(cx, fam, v.depth)
            return v.value
        if cx is None:
            return None
        u = cx.unit
        if u is not None and dep in u.expired:
            return Pend("deadline", on=[fam, list(_jkey(key))])
        if wait_deadline is not None:
            dl = ("rel", float(wait_deadline))
        elif cx.deadline is not None:
            dl = ("abs", cx.deadline)
        else:
            dl = None
        cx.waits[dep] = dl
        return Pend("waiting", on=[fam, list(_jkey(key))])

    def put(self, cx, fam, key, value, node=None, internal=False):
        F = self._fam(fam, node)
        if len(key) != len(F.decl.keys) and F.kind == "cell":
            raise JxError(f"单元 `{fam}` 要 {len(F.decl.keys)} 个键，给了 {len(key)} 个", node)
        cx.puts.append((fam, tuple(key), value, node, internal))

    def claim(self, cx, fam, key, value, node=None):
        F = self._fam(fam, node)
        if F.reducer != "claim":
            raise JxError(f"`{fam}` 不是占用单元（声明时写 reducer claim）", node)
        key = tuple(key)
        inst = self._inst(F, key)
        dep = ("c", fam, key)
        if inst.claim_owner in (None, cx.uid):
            if inst.claim_owner is None or h(to_py(inst.claim_value)) != h(to_py(value)):
                inst.claim_owner = cx.uid
                inst.claim_value = value
                inst.claim_ver += 1
                self.ledger.add("claim", unit=cx.uid, cell=fam, key=_jkey(key), granted=True)
                self._publish(inst, {"owner": cx.uid, "value": value}, "settled", cx.depth, cx.chain, cx.uid)
            cx.claims.add((fam, key))
            self._record(cx, dep)
            return Exit("act", value=value, conf=1.0)
        self._record(cx, dep)
        self.ledger.add("claim", unit=cx.uid, cell=fam, key=_jkey(key), granted=False, owner=inst.claim_owner)
        return Exit("unsure", cause="claim_conflict", needed=inst.claim_owner)

    def _release(self, u, ik, chain):
        fam, key = ik
        F = self.fams[fam]
        inst = F.insts.get(key)
        if inst is None or inst.claim_owner != u.uid:
            return
        inst.claim_owner = None
        inst.claim_value = None
        inst.claim_ver += 1
        self.ledger.add("release", unit=u.uid, cell=fam, key=_jkey(key))
        self._publish(inst, None, "removed", 0, chain, u.uid)
        self._notify(("c", fam, key), chain, None, True)

    def spawn(self, cx, name, args, budget=None, deadline=None, node=None, rank=None):
        res = self.residents.get(name)
        if res is None:
            raise JxError(f"spawn 的 `{name}` 不是常驻程序", node)
        if len(args) != len(res.params):
            raise JxError(f"常驻程序 {name} 要 {len(res.params)} 个实参，给了 {len(args)} 个", node)
        # 深度按触发链：触发这个程序的单元版本已在组合层数上限前一层，它派生的下游只会产出被丢弃的组合，不派生
        if cx.unit is not None:
            src_d = cx.trigger_depth
            if src_d >= self.depth_cap - 1:
                self.ledger.add("depth", unit=cx.uid, spawn=name, depth=src_d, cap=self.depth_cap)
                return Pend("depth")
        u = self._unit(res, args, cx.uid, cx.chain)
        if isinstance(rank, (int, float)) and not isinstance(rank, bool):     # 派生排序值取最大（预注册 16）
            u.rank_hint = float(rank) if u.rank_hint is None else max(u.rank_hint, float(rank))
        if cx.trigger_depth > u.base_depth:
            u.base_depth = cx.trigger_depth
        new = u.runs == 0 and u.running is None
        if budget and isinstance(budget, dict):
            u.account = (cx.account or self.account).child(f"spawn:{u.uid}", budget.get("calls"), budget.get("cost"))
        elif cx.unit is not None and (_has_clause(cx.unit.res) or cx.unit.acct_root):
            u.acct_root = cx.unit.uid if _has_clause(cx.unit.res) else cx.unit.acct_root   # 继承根程序的接入账户
        elif cx.account is not None and cx.account is not self.account and not self.flags["no_budget_chain"]:
            u.account = cx.account                 # 继承派生者的账户（只收紧）
        if deadline is not None and not self.flags["no_deadline"]:
            dl = self.clock() + float(deadline)
            if cx.deadline is not None:
                dl = min(dl, cx.deadline)
            u.deadline = dl if u.deadline is None else min(u.deadline, dl)
        elif cx.deadline is not None:
            u.deadline = cx.deadline
        cx.spawned.add(u.uid)
        if new and not u.dirty:
            self._mark(u, cx.chain, None, force=True, why="spawn")
        return Handle(name, u.key)

    def read_family(self, cx, fam, elem, node=None):
        F = self._fam(fam, node)
        dep = ("f", fam, elem)
        self._record(cx, dep)
        out = []
        keys = F.insts if elem is None else F.by_elem.get(elem, ())
        for key in sorted(keys, key=lambda k: [str(x) for x in k]):
            inst = F.insts[key]
            v = inst.latest
            if v is None or v.status == "removed":
                continue
            _seen_depth(cx, fam, v.depth)
            out.append(v.value)
        return out

    def family_keys(self, cx, fam, node=None):
        F = self._fam(fam, node)
        self._record(cx, ("m", fam))
        ks = sorted((k for k, i in F.insts.items() if i.alive), key=lambda k: canon(list(k)))
        return [k[0] if len(k) == 1 else list(k) for k in ks]

    def unit_status(self, cx, name, key):
        F = self._fam(name)
        key = tuple(key)
        inst = F.insts.get(key)
        self._record(cx, ("i", name, key))
        u = self.units.get(_uid(name, key)) if F.kind == "resident" else None
        st = {"status": None, "version": inst.ver if inst else 0, "progress": None, "pending": []}
        if inst and inst.latest:
            st["status"] = inst.latest.status
        if u is not None:
            if u.running is not None:
                rs = u.running.reqs
                st["status"] = "running"
                st["progress"] = {"judged": sum(1 for r in rs if r.done), "total": len(rs)}
            st["pending"] = to_py(u.pending)
        return st

    async def do_action(self, cx, name, args, node=None):
        act = self.actions.get(name)
        if act is None:
            raise JxError(f"没有注册的动作 `{name}`（宿主用 engine.register_action 注册）", node)
        for fam in act["depends_on"]:
            if fam in self.fams:
                self._record(cx, ("f", fam, None))
        pargs = to_py(args)
        if not act["transparent"] and cx is not None and cx.unit is not None and cx.unit.retracted:
            # 单元已撤回（如主人离开），它在途的尝试不再做有副作用的动作：不然离开后还会把他写回索引
            # （预注册 16 差分 seed 3 暴露：发布 的尝试在离开前起、离开后才执行 index_put）
            self.ledger.add("do", unit=cx.uid, action=name, skipped="retracted")
            return None
        mk = None
        if act["transparent"]:
            mk = (name, canon(pargs))
            if mk in cx.do_memo:
                return cx.do_memo[mk]
        fn = act["fn"]
        try:
            if asyncio.iscoroutinefunction(fn):
                w = self.fiber(fn(*pargs), cx)
                res = await self.wait(w, cx)
            else:
                res = fn(*pargs)
        except JxError:
            raise
        except Exception as e:      # noqa: BLE001
            self.ledger.add("do", unit=cx.uid, action=name, fail=str(e)[:300])
            return Fail(f"{name}: {e}")
        if act["cost"] and cx.account is not None:
            cx.account.charge(0, act["cost"])
        self.ledger.add("do", unit=cx.uid, action=name, args=h(pargs), result=h(to_py(res)))
        if mk is not None:
            cx.do_memo[mk] = res
        return res

    def gen(self, cx, kind, args, node=None):
        """生成：一登记就发，不阻塞静止判定。同一份参数在这次运行里只调一次生成器（预注册 17）。"""
        mk = (kind, h(to_py(args)))
        if mk in self._gen_memo:
            self.ledger.add("gen_memo", unit=cx.uid, what=kind, args=mk[1])
            return self._gen_memo[mk]
        w = Waitable()
        w.is_gen = True
        port = self.gen_port
        self.gen_tasks += 1

        async def go():
            try:
                if port is None:
                    v = {"synthetic": True, "kind": kind, "args_hash": h(to_py(args))[:12],
                         "text": f"（离线生成占位 {h(to_py(args))[:8]}）"}
                elif kind == "gen_json":
                    v = await port.gen_json(args[0], to_py(args[1:] if len(args) > 2 else (args[1] if len(args) > 1 else [])))
                else:
                    v = await port.text(str(args[0]))
                self.ledger.add("gen", unit=cx.uid, what=kind, args=h(to_py(args)), out=h(to_py(v)))
                self._gen_memo[mk] = v
                self.resolve(w, v)
            except Exception as e:      # noqa: BLE001
                self.ledger.add("gen", unit=cx.uid, what=kind, fail=str(e)[:300])
                self.resolve(w, Fail(str(e)))
            finally:
                self.gen_tasks -= 1
                self._check()
        asyncio.get_running_loop().create_task(go())
        return Lazy(w, "gen")

    # ------------------------------------------------------------ 宿主接口
    def _new_chain(self, cause, budget=None, deadline_s=None):
        if self.flags["no_budget_chain"]:
            acct = self.account
        else:
            acct = self.account.child(f"chain:{cause}", (budget or {}).get("calls"), (budget or {}).get("cost")) \
                if budget else self.account
        dl = None
        if deadline_s is not None and not self.flags["no_deadline"]:
            dl = self.clock() + float(deadline_s)
        ch = Chain(next(self.chain_seq), acct, dl, cause)
        self.chains_open.append(ch)
        if dl is not None:
            self.at(dl, lambda: self.sched.expire(ch))
        return ch

    async def put_source(self, fam, key, value, budget=None, deadline_s=None, cause=None):
        """宿主写源单元（宿主是写者 host）。union 单元是追加一个元素。返回 epoch。"""
        if not self.started:
            await self.start()
        F = self._fam(fam)
        key = tuple(key)
        chain = self._new_chain(cause or f"put:{fam}", budget, deadline_s)
        chain.ids = [k for k in key]
        inst = self._inst(F, key)
        if F.reducer == "union":
            inst.contrib.setdefault("host", []).append(value)
            inst.contrib_h.setdefault("host", []).append(h(to_py(value)))
        else:
            inst.contrib["host"] = [value]
            inst.contrib_h.pop("host", None)
        inst.contrib_epoch["host"] = self.epoch
        self._apply(F, inst, "settled", 0, chain, "host")
        if self.flags["no_cells"]:
            # 关掉单元图：每个事件让全部常驻程序从零重跑
            for u in list(self.units.values()):
                self._mark(u, chain, None, force=True)
        self._kick()
        return self.epoch

    async def remove_source(self, fam, key, budget=None):
        F = self._fam(fam)
        key = tuple(key)
        chain = self._new_chain(f"remove:{fam}", budget, None)
        inst = F.insts.get(key)
        if inst is None:
            return self.epoch
        inst.contrib.pop("host", None)
        self._apply(F, inst, "settled", 0, chain, "host")
        if self.flags["no_cells"]:
            for u in list(self.units.values()):
                self._mark(u, chain, None, force=True)
        self._kick()
        return self.epoch

    async def event(self, name, value, budget=None, deadline_s=None):
        if not self.started:
            await self.start()
        chain = self._new_chain(f"event:{name}", budget, deadline_s)
        for r in self.residents.values():
            if any(s.kind == "event" and s.arg == name for s in r.sources):
                args = []
                for p in r.params:
                    args.append(value.get(p) if isinstance(value, dict) else None)
                u = self._unit(r, args, "auto", chain)
                self._mark(u, chain, value, force=True)
        if self.flags["no_cells"]:
            for u in list(self.units.values()):
                self._mark(u, chain, None, force=True)
        self._kick()
        return self.epoch

    def read_host(self, fam, key, mode="peek"):
        v = self.read_cell(None, fam, tuple(key), mode)
        self.ledger.add("host_read", cell=fam, key=_jkey(tuple(key)), mode=mode)
        return to_py(v) if not is_pend(v) else None

    def read(self, fam, key, mode="peek"):
        """宿主读（接口 v1 (c)）：peek 读当前最新版本，settled 读已定版本；没有则 None。读进账本。"""
        return self.read_host(fam, key, mode)

    def status(self, fam, key):
        return self.unit_status(None, fam, tuple(key))

    def keys(self, fam, contains=None):
        F = self._fam(fam)
        ks = [list(k) for k, i in F.insts.items() if i.alive and (contains is None or contains in _elems(k))]
        return sorted(ks, key=canon)

    # ------------------------------------------------------------ 事件（运行时 → 观察者）
    def emit(self, typ, **kw):
        self.bus.emit(typ, **kw)

    def on_exit(self, cx, lx: LazyExit, ex: Exit):
        st = lx.lr.state
        ow = list(st.owners)
        q = lx.lr.q
        jk = (cx.uid, lx.lr.req.key)
        sig = (ex.kind, str(ex.value), ex.cause)
        if self._judge_seen.get(jk) == sig:
            return                                  # 重算得到同一出口：不再记
        self._judge_seen[jk] = sig
        if ex.kind == "unsure" or ex.from_key or lx.line or ex.grade != "Answer" or ex.needed:
            # 无线、按多数块走的出口可由判断记录推出，不另记；只记作者线、未决、补过再判、降级的
            self.ledger.add("exit", unit=cx.uid, key=lx.lr.req.key, q=q.key, exit=ex.kind, value=to_py(ex.value),
                            cause=ex.cause or None, grade=ex.grade, from_key=ex.from_key or None)
        if q.key.startswith("companion:"):
            return
        self.bus.emit("judge", a=ow[0] if ow else None, b=ow[1] if len(ow) > 1 else None,
                      config=None if len(ow) <= 2 else ow, q=q.key or q.text[:24],
                      p=round(ex.conf, 3), exit=ex.kind if ex.kind in ("act", "ignore", "unsure") else "act",
                      value=to_py(ex.value), cause=ex.cause or None, batch=lx.lr.req.batch)

    def route(self, cx, lx, ex, route, missing, ask_to):
        k = (cx.uid, lx.lr.req.key, route, str(missing))
        self.ledger.add("unsure_route", unit=cx.uid, key=lx.lr.req.key, route=route, missing=missing,
                        ask_to=list(ask_to), cause=ex.cause)
        if k in self._emitted_routes:
            return
        self._emitted_routes.add(k)
        ow = list(lx.lr.state.owners)
        self.bus.emit("unsure_route", a=ow[0] if ow else None, b=ow[1] if len(ow) > 1 else None,
                      config=None if len(ow) <= 2 else ow, q=lx.lr.q.key, missing=missing,
                      ask_to=list(ask_to), route=route, cause=ex.cause,
                      p=round(ex.conf, 3) if ex.conf is not None else None)

    def disclose_sent(self, cx, holder, frm, cat, purpose):
        k = (holder, frm, cat)
        if k in self._disclose_seen:
            return
        self._disclose_seen[k] = "sent"
        self.ledger.add("disclose_request", to=holder, frm=frm, cat=cat, status="sent")
        self.bus.emit("disclose_request", id=f"{holder}|{frm}|{cat}", to=holder, **{"from": frm},
                      category=cat, purpose=purpose, status="sent")

    def disclose_result(self, cx, holder, frm, cat, rv):
        k = (holder, frm, cat)
        status = "denied" if isinstance(rv, dict) and rv.get("denied") else "granted"
        if self._disclose_seen.get(k) == status:
            return
        self._disclose_seen[k] = status
        self.ledger.add("disclose_request", to=holder, frm=frm, cat=cat, status=status)
        self.bus.emit("disclose_request", id=f"{holder}|{frm}|{cat}", to=holder, **{"from": frm},
                      category=cat, purpose=None, status=status)

    def on_recovered(self):
        self.ledger.add("recovered", n=len(self.degraded_units))
        for uid in list(self.degraded_units):
            u = self.units.get(uid)
            if u is not None:
                self._mark(u, u.chain, None, force=True)
        self.degraded_units.clear()

    def schedule_absent_probe(self, after=5.0):
        if self._probe_scheduled:
            return
        self._probe_scheduled = True

        def probe():
            self._probe_scheduled = False
            if self.degraded_units:
                for uid in list(self.degraded_units):
                    u = self.units.get(uid)
                    if u is not None:
                        self._mark(u, u.chain, None, force=True)
                self.degraded_units.clear()
        self.at(self.clock() + after, probe)

    def _close_chains(self):
        for ch in self.chains_open:
            self.bus.emit("invalidate", cause=ch.cause, ids=ch.ids, n_judgments=ch.sent_questions, n_units=ch.reruns)
        self.chains_open = []
        self._emit_stats()

    def _emit_stats(self):
        s = self.sched.stats()
        hits = s["cache_hits"]
        tot = hits + s["questions"]
        self.bus.emit("stats", units=len(self.units), calls=s["calls"], questions=s["questions"],
                      cost_usd=s["cost_usd"], cache_hit=round(hits / tot, 3) if tot else 0.0,
                      attempts=self.attempts, cutoffs=self.skipped, errors=len(self.errors))

    def stats(self) -> dict:
        s = self.sched.stats()
        s.update(attempts=self.attempts, cutoffs=self.skipped, units=len(self.units), errors=len(self.errors),
                 account_calls=self.account.calls, account_cost=round(self.account.cost, 6),
                 parked=len(self.parked), parks=self.parks, admits=self.admits)
        by = {}
        for rn, n in self.attempts_by.items():
            by.setdefault(rn, {})["attempts"] = n
        for rn, b in self.sched.by_resident.items():
            by.setdefault(rn, {}).update(b)
        s["by_resident"] = by
        whys = {}
        for (rn, w), n in sorted(self.why.items(), key=lambda kv: -kv[1]):
            whys.setdefault(rn, {})[w] = n
        s["why_rerun"] = whys
        return s


# ================================================================ 工具

class _Wild:
    pass


def _static_eval(n, env):
    """来源里的键表达式：只允许名字、字段、下标、字面量（在实例化时按实参求值）。"""
    if isinstance(n, A.Var):
        if n.name in env:
            return env[n.name]
        return _Wild()
    if isinstance(n, A.Str) or isinstance(n, A.Num):
        return n.v
    if isinstance(n, A.Field):
        o = _static_eval(n.obj, env)
        if isinstance(o, _Wild):
            return o
        return o[n.name]
    if isinstance(n, A.Index):
        o = _static_eval(n.obj, env)
        i = _static_eval(n.idx, env)
        if isinstance(o, _Wild) or isinstance(i, _Wild):
            return _Wild()
        return o[i]
    raise ValueError("来源的键只能是名字、字段、下标或字面量")


def _lit(x):
    """程序头 budget 记录里的字面量（可嵌一层记录，如 cascade: {calls: 60, line: 0.65}）。"""
    if isinstance(x, (A.Num, A.Str)):
        return x.v
    if isinstance(x, A.RecordLit):
        return {k: _lit(v) for k, v in x.items}
    return None


def _strs(x) -> set:
    out = set()
    if isinstance(x, str):
        out.add(x)
    elif isinstance(x, dict):
        for v in x.values():
            out |= _strs(v)
    elif isinstance(x, (list, tuple)):
        for v in x:
            out |= _strs(v)
    return out


def _has_clause(res):
    return res.budget is not None or res.deadline is not None


def _seen_depth(cx, fam, d):
    if cx is not None and d:
        if d > cx.fam_depth.get(fam, 0):
            cx.fam_depth[fam] = d


def _elems(key):
    out = []
    for k in key:
        if isinstance(k, (str, int, float)):
            out.append(k)
    return out


def _jkey(key):
    return [k for k in key]


def _uid(name, key):
    body = ",".join(str(k) for k in key)
    if len(body) > 48:                       # 实参是大记录时用内容哈希（账本里不重复整份实参）
        return f"{name}#{h(list(key))[:16]}"
    return f"{name}({body})"
