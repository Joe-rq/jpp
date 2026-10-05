"""J++x 判断调度：合批、排序、并发、缓存、预算、时延、缺席降级。

运行时接管的几件事在这里落地：
- 批调度：引擎静止时把所有已登记的题按 state 内容哈希分组，一组一次调用（单次 ≤ max_q 题，超了裂变）。
- 排序：组间按（截止最近 → 接入事件最新 → 价值密度）排，并发上限来自画像。
- 缓存与增量：判断单元键 = (模型, state 内容, 题内容)；可选 sqlite 持久化，跨运行复用。
- 预算：账户树只收紧；用完只停发不停程序，记 Unsure(budget)。
- 时延预算：超时 → Unsure(latency)。
- 判断力缺席：端口失败 → 编码器相似度的降级读数（by: enc），恢复后由引擎让消费过它的单元重判。
"""
from __future__ import annotations

import asyncio
import heapq
import itertools
import json
import math
import os
import sqlite3
from dataclasses import dataclass, field
from typing import Any

import numpy as np

from .core import Question, Reading, canon, h
from .ports.jev import JevAbsent, WireQ, reading_of


class PortAbsent(Exception):
    """任何判断端口的缺席（网络、超时、5xx 用尽）。"""


class AbsentError(Exception):
    """--no-absent 时判断器缺席直接报错。"""


# ---------------------------------------------------------------- 画像

@dataclass
class Profile:
    model: str = "jev-1.13.0"
    price_per_input_token: float = 4.2e-8
    latency_p95_s: float = 1.1
    concurrency: int = 32
    max_q_per_call: int = 100
    max_tokens_per_call: int = 24000    # 估算 token 上限：题长时 100 题会超 JEV 单次上限（10-05 公网：HTTP 400 max_tokens_exceeded）
    timeout_s: float | None = 20.0      # 单次调用的时延上限（超时 → Unsure(latency)）
    near_band: float = 0.1              # 无线是非题读数离 0.5 小于它时触发「先补再判」（画像字段，J-05 读数触发）


# ---------------------------------------------------------------- 预算账户（只收紧）

class Account:
    def __init__(self, name: str, calls: float | None = None, cost: float | None = None, parent: "Account | None" = None):
        self.name = name
        self.cap_calls = math.inf if calls is None else float(calls)
        self.cap_cost = math.inf if cost is None else float(cost)
        self.calls = 0
        self.cost = 0.0
        self.parent = parent

    def chain(self):
        a = self
        while a is not None:
            yield a
            a = a.parent

    def remaining_calls(self) -> float:
        return min(a.cap_calls - a.calls for a in self.chain())

    def remaining_cost(self) -> float:
        return min(a.cap_cost - a.cost for a in self.chain())

    def can(self, calls=1, cost=0.0) -> bool:
        return self.remaining_calls() >= calls and self.remaining_cost() >= cost

    def charge(self, calls=0, cost=0.0):
        for a in self.chain():
            a.calls += calls
            a.cost += cost

    def child(self, name, calls=None, cost=None) -> "Account":
        """派生账户：上限 = min(给定, 父账户剩余)，只收紧。"""
        c = self.remaining_calls() if calls is None else min(float(calls), self.remaining_calls())
        k = self.remaining_cost() if cost is None else min(float(cost), self.remaining_cost())
        return Account(name, c, k, self)


@dataclass
class Chain:
    """一条接入事件链：宿主事件触发的全部尝试与判断记在它名下。"""
    seq: int
    account: Account
    deadline: float | None = None
    cause: str = ""
    sent_questions: int = 0
    reruns: int = 0
    ids: list = field(default_factory=list)
    cascade_used: int = 0          # 这条链上参与级联的尝试已发出的调用数（预注册 16）
    cascade_reserved: int = 0      # 已准入、还没结束的尝试预留的调用数
    cascade_open: dict = field(default_factory=dict)   # 已登记未发出的级联题：state → 题数（一个 state 约一次调用）


# ---------------------------------------------------------------- 可等待物

class Waitable:
    __slots__ = ("done", "value", "blocked", "fut", "exc", "is_gen")

    def __init__(self):
        self.is_gen = False      # 生成器的结果：在等它的尝试不挡准入与链结束（预注册 17）
        self.done = False
        self.value = None
        self.exc = None
        self.blocked = 0
        self.fut = None


class Req(Waitable):
    __slots__ = ("key", "state", "q", "wq", "chain", "account", "seq", "owners", "sent", "missing",
                 "degraded", "batch", "density", "site", "reserved", "payer", "deadline", "pool", "pool_cx")

    def __init__(self, key, state, q, chain, account, seq, site=""):
        super().__init__()
        self.key = key
        self.state = state
        self.q = q
        self.wq = WireQ(q.op, q.text, q.criteria())
        self.chain = chain
        self.account = account
        self.seq = seq
        self.owners = set()
        self.sent = False
        self.missing = None        # 未观察到的原因（budget/latency/absent/deadline）
        self.degraded = False
        self.batch = None
        self.density = 1.0
        self.site = site
        self.reserved = None
        self.payer = ""          # 第一个要求者（付钱方，R11.3）
        self.deadline = None
        self.pool = None         # 级联预算记在哪条链上（登记它的尝试是准入的级联尝试时）
        self.pool_cx = None


# ---------------------------------------------------------------- 端口

def _dl(r):
    a = r.chain.deadline if (r.chain is not None) else None
    b = r.deadline
    if a is None:
        return b
    if b is None:
        return a
    return min(a, b)


def fixture_key(state_wire: dict, wq: WireQ) -> str:
    return h({"state": state_wire, "q": wq.wire()})


def synthetic_raw(key: str, wq: WireQ, skew: float = 1.0, skey: str | None = None) -> dict:
    """录好的答案里没有时，按 (state, 题) 哈希给确定性伪读数（标 fixture-synthetic）。
    skew > 1 让是非题读数偏低（p = u^skew），模拟「大多数配对不成立」。
    给了 skey（state 的哈希）时，同一 state 上的是非题读数相关（u = 0.85·u_state + 0.15·u_题），
    像真判断器那样：一对合不合得来，各道题大体同向；单题 act 率约 26%，一对的「任一成立」也约三成。"""
    x = int(key[:16], 16)
    if wq.op == "test":
        u = (x % 997) / 997
        if skey is not None:
            u = 0.85 * ((int(skey[:12], 16) % 991) / 991) + 0.15 * u
        p = u ** skew
        if abs(p - 0.5) < 1e-9:
            p = 0.51
        return {"type": "noul", "noul": round(p, 4), "synthetic": True}
    labels = list(wq.criteria.keys()) if wq.op == "select" else [str(i) for i in range(len(wq.criteria))]
    ws = [(int(h([key, i])[:6], 16) % 9973) + 1 for i in range(len(labels))]
    s = sum(ws)
    return {"type": "choice" if wq.op == "select" else "score",
            "probabilities": {lab: round(w / s, 4) for lab, w in zip(labels, ws)}, "synthetic": True}


class FixturePort:
    """离线判断端口：用录好的答案；缺了给哈希伪读数。同步完成（带种子的测试不抖）。"""
    sync = True

    def __init__(self, answers: dict | None = None, path: str | None = None, skew: float = 2.0):
        self.skew = skew
        self.answers = dict(answers or {})
        if path and os.path.exists(path):
            d = json.load(open(path))
            self.answers.update(d.get("answers", d))
        self.synthetic = 0
        self.recorded = 0
        self.down = False          # 测试用：模拟掉线
        self.latency = None        # 测试用：callable(state, qs) -> 秒

    def call_sync(self, state: dict, qs: list[WireQ]) -> list[dict]:
        if self.down:
            raise PortAbsent("fixture 端口模拟掉线")
        out = []
        for wq in qs:
            k = fixture_key(state, wq)
            a = self.answers.get(k)
            if a is None:
                self.synthetic += 1
                a = synthetic_raw(k, wq, self.skew, h(state))
            else:
                self.recorded += 1
            out.append(a)
        return out

    def simulated_latency(self, state, qs):
        return self.latency(state, qs) if self.latency else 0.0


class RecordingPort:
    """包住真机端口，把答案按 fixture 键录下来，供零花费复跑。"""
    sync = False

    sync = False

    def __init__(self, inner, path: str):
        self.inner = inner
        self.path = path
        self.answers = {}
        if os.path.exists(path):
            self.answers = json.load(open(path)).get("answers", {})

    @property
    def stats(self):
        return self.inner.stats

    async def call(self, state: dict, qs: list[WireQ]) -> list[dict]:
        out = await self.inner.call(state, qs)
        for wq, a in zip(qs, out):
            self.answers[fixture_key(state, wq)] = a
        return out

    def save(self):
        os.makedirs(os.path.dirname(os.path.abspath(self.path)), exist_ok=True)
        json.dump({"answers": self.answers}, open(self.path, "w"), ensure_ascii=False)

    async def close(self):
        if hasattr(self.inner, "close"):
            await self.inner.close()


class HashEncoder:
    """内置编码器：字符三元组哈希到 512 维、归一化。缺席降级与离线 embed_topk 用；真编码器用 jx/ports/enc.py。"""
    dim = 512

    def encode(self, texts: list[str]) -> np.ndarray:
        out = np.zeros((len(texts), self.dim), np.float32)
        for i, t in enumerate(texts):
            t = f"  {t}  "
            for j in range(len(t) - 2):
                g = t[j:j + 3]
                out[i, int(h(g)[:6], 16) % self.dim] += 1.0
            n = np.linalg.norm(out[i])
            if n > 0:
                out[i] /= n
        return out


def _text(x) -> str:
    return x if isinstance(x, str) else canon(x)


def degraded_reading(enc, state_wire: dict, q: Question) -> Reading:
    """编码器相似度给的粗读数（by: enc）。只用于判断器缺席时，出口会是 Unsure(absent) 带倾向。"""
    on = state_wire.get("on")
    if isinstance(on, dict) and len(on) >= 2:
        vals = list(on.values())
        a, b = _text(vals[0]), _text(vals[1])
    elif isinstance(on, list) and len(on) >= 2:
        a, b = _text(on[0]), _text(on[1])
    else:
        a, b = _text(state_wire), q.text
    if q.op == "test":
        v = enc.encode([a, b])
        sim = float(v[0] @ v[1])
        p = max(0.02, min(0.98, 0.5 + (sim - 0.5) * 0.8))
        return Reading(q, p=round(p, 4), by="enc")
    if q.op == "select":
        labels = [k for k, _ in q.labels]
        v = enc.encode([_text(state_wire)] + [d for _, d in q.labels])
        sims = v[1:] @ v[0]
        e = np.exp((sims - sims.max()) / 0.05)
        ps = e / e.sum()
        return Reading(q, dist=tuple((lab, round(float(p), 4)) for lab, p in zip(labels, ps)), by="enc")
    v = enc.encode([a, b])
    sim = float(v[0] @ v[1])
    n = len(q.labels)
    c = max(0, min(n - 1, round(sim * (n - 1))))
    ps = [0.6 if i == c else 0.4 / max(1, n - 1) for i in range(n)]
    return Reading(q, dist=tuple(ps), by="enc")


def first_payer(part):
    return min(part, key=lambda r: r.seq).payer


def _top(rd):
    t = rd.top()
    if rd.q.op == "select":
        return [t[0], round(t[1], 4)] if t else None
    return [t, round(rd.dist[t], 4)] if t is not None else None


def to_reading(q: Question, raw: dict, sid: str) -> Reading:
    r = reading_of(q.op, raw, list(q.labels) if q.op == "measure" else None)
    if q.op == "test":
        return Reading(q, p=r["p"], state_id=sid)
    if q.op == "select":
        d = r["dist"]
        return Reading(q, dist=tuple((lab, float(d.get(lab, 0.0))) for lab, _ in q.labels), state_id=sid)
    return Reading(q, dist=tuple(r["dist"]), state_id=sid)


# ---------------------------------------------------------------- 调度器

class Sched:
    def __init__(self, engine, port, profile: Profile, flags: dict, enc=None, cache_path: str | None = None):
        self.eng = engine
        self.port = port
        self.prof = profile
        self.flags = flags
        self.enc = enc or HashEncoder()
        # 缓存键带端口身份：离线伪读数与真机读数不能互相冒充（同一模型名）
        inner = getattr(port, "inner", port)
        self.port_tag = getattr(inner, "cache_tag", None) or ("fixture" if isinstance(inner, FixturePort) else
                                                               ("live" if inner.__class__.__name__ == "JevPort" else
                                                                inner.__class__.__name__))
        self.queue: list[Req] = []           # 已登记、未发出
        self.live: dict[str, Req] = {}       # 同运行内按键去重（未发出或在飞）
        self.cache: dict[str, Reading] = {}
        self.heap = []
        self.active = 0
        self.seq = itertools.count(1)
        self.batch_ids = itertools.count(1)
        self.absent = False
        self.db = None
        if cache_path:
            os.makedirs(os.path.dirname(os.path.abspath(cache_path)), exist_ok=True)
            self.db = sqlite3.connect(cache_path)
            self.db.execute("create table if not exists judged(k text primary key, raw text)")
        # 统计
        self.calls = 0
        self.questions = 0
        self.cost = 0.0
        self.cache_hits = 0
        self.dedup_hits = 0
        self.degraded_n = 0
        self.budget_skips = 0
        self.latencies = []
        self.flushes = 0
        self.by_resident = {}
        self.cascade_calls = 0

    # ------------------------------------------------------------ 登记
    def key(self, state, q) -> str:
        return f"{self.prof.model}|{self.port_tag}|{state.sid}|{q.ident()}"

    def register(self, cx, state, q: Question, site="") -> Req:
        k = self.key(state, q)
        use_cache = not self.flags.get("no_cells")
        if use_cache:
            r = self.cache.get(k)
            if r is None and self.db is not None:
                row = self.db.execute("select raw from judged where k=?", (k,)).fetchone()
                if row:
                    r = to_reading(q, json.loads(row[0]), state.sid)
                    self.cache[k] = r
            if r is not None:
                req = Req(k, state, q, cx.chain, cx.account, next(self.seq), site)
                req.done, req.value = True, r
                self.cache_hits += 1
                return req
        old = self.live.get(k)
        if old is not None and (not old.done or (use_cache and not old.missing and not old.degraded)):
            old.owners.add(cx)
            cx.reqs.append(old)
            self.dedup_hits += 1
            return old
        pool = getattr(cx, "pool", None)
        if getattr(cx, "rank", None) is not None and pool is None:
            from .engine import ParkSignal          # 级联预算：没准入的级联尝试不发新题（预注册 16）
            raise ParkSignal()
        if pool is not None:
            # 准入的尝试要开一次新调用（新的 state）时，本链级联账（已发出 + 已登记未发出）到顶就作废挂起：上限是硬的
            sid = state.sid
            if sid not in pool.cascade_open and pool.cascade_used + len(pool.cascade_open) >= float(self.eng.cascade["calls"]):
                from .engine import ParkSignal
                raise ParkSignal()
            pool.cascade_open[sid] = pool.cascade_open.get(sid, 0) + 1
        req = Req(k, state, q, cx.chain, cx.account, next(self.seq), site)
        req.owners.add(cx)
        req.payer = cx.uid
        req.deadline = cx.deadline
        req.pool = getattr(cx, "pool", None)
        req.pool_cx = cx if req.pool is not None else None
        self.live[k] = req
        self.queue.append(req)
        cx.reqs.append(req)
        return req

    def drop_unforced(self, cx):
        """尝试结束时，只属于它、没被检视、还没发出的题不发（惰性：没人要就不问）。"""
        keep = []
        for r in self.queue:
            if cx in r.owners:
                r.owners.discard(cx)
            if r.owners or r.blocked:
                keep.append(r)
            else:
                self.live.pop(r.key, None)
                self._unopen(r)
                r.done, r.missing = True, "dropped"
        self.queue = keep

    @staticmethod
    def _unopen(r):
        """级联题离开排队（发出、丢弃、截止）：从本链「已登记未发出」里减掉。"""
        p = r.pool
        if p is None:
            return
        sid = r.state.sid
        n = p.cascade_open.get(sid, 0) - 1
        if n <= 0:
            p.cascade_open.pop(sid, None)
        else:
            p.cascade_open[sid] = n

    def _count_pool(self, part):
        """级联预算按发出的调用记（预注册 16）；同时记每个常驻程序单次尝试最多发了几次（准入预留用）。"""
        first = min(part, key=lambda r: r.seq)
        if first.pool is None:
            return
        first.pool.cascade_used += 1
        self.cascade_calls += 1
        pc = first.pool_cx
        if pc is not None:
            pc.pool_calls += 1
            if pc.unit is not None:
                rn = pc.unit.res.name
                self.eng.cascade_max[rn] = max(self.eng.cascade_max.get(rn, 1), pc.pool_calls)

    # ------------------------------------------------------------ 发出
    def flush(self, only=None):
        """把已登记的题按 state 分组发出。only: 只发这些（--no-batch 时某个尝试自己的刷新）。"""
        if only is None:
            reqs, self.queue = self.queue, []
        else:
            ids = {id(r) for r in only}
            reqs = [r for r in self.queue if id(r) in ids]
            self.queue = [r for r in self.queue if id(r) not in ids]
        reqs = [r for r in reqs if not r.done and not r.sent]
        for r in reqs:
            self._unopen(r)
        if not reqs:
            return
        self.flushes += 1
        groups: dict[str, list[Req]] = {}
        for r in reqs:
            groups.setdefault(r.state.sid, []).append(r)
        no_chain = self.flags.get("no_budget_chain")
        for sid, rs in groups.items():
            rs.sort(key=lambda r: r.seq)
            mq, mt = self.prof.max_q_per_call, self.prof.max_tokens_per_call
            parts, cur = [], []
            for r in rs:                   # 按题数与估算 token 两条上限切分
                if cur and (len(cur) >= mq or self._split_tokens(r.state.wire(), cur + [r]) > mt):
                    parts.append(cur)
                    cur = []
                cur.append(r)
            if cur:
                parts.append(cur)
            for part in parts:
                for r in part:
                    r.sent = True
                self._count_pool(part)
                first = min(part, key=lambda r: r.seq)
                if no_chain:
                    prio = (first.seq,)
                else:
                    dl = min((_dl(r) for r in part if _dl(r) is not None), default=math.inf)
                    newest = max((r.chain.seq for r in part if r.chain), default=0)
                    density = len(part) / max(1.0, len(canon(first.state.wire())) / 400)
                    prio = (dl, -newest, -density, first.seq)
                heapq.heappush(self.heap, (prio, next(self.seq), part))
        self._pump()

    def _pump(self):
        while self.heap and self.active < self.prof.concurrency:
            _, _, part = heapq.heappop(self.heap)
            self.active += 1
            if getattr(self.port, "sync", False):
                try:
                    self._run_sync(part)
                finally:
                    self.active -= 1
            else:
                asyncio.get_running_loop().create_task(self._run_async(part))

    def _precheck(self, part) -> str | None:
        """发出前：截止过了？预算够吗？返回未观察原因或 None。"""
        now = self.eng.clock()
        first = min(part, key=lambda r: r.seq)
        if not self.flags.get("no_deadline"):
            dls = [_dl(r) for r in part]
            if all(d is not None for d in dls) and dls and max(dls) < now:
                return "deadline"
        acct = first.account
        est = self._est_tokens(first.state.wire(), part) * self.prof.price_per_input_token
        if acct is not None and not acct.can(1, est):
            return "budget"
        if acct is not None:
            acct.charge(1, est)          # 发出即预留（并发在飞的调用也不会超账），回来后按实花费校正
        part[0].reserved = est
        return None

    @staticmethod
    def _split_tokens(state_wire, part) -> int:
        """切批用的保守估算：中文约一字一 token 以上（10-05 实测 100 道短题 1.87 万 token，按字数 /1.6 只估到一半）。"""
        return int((len(canon(state_wire)) + sum(len(canon(r.wq.wire())) for r in part)) * 1.2) + 20

    @staticmethod
    def _est_tokens(state_wire, part) -> int:
        return int((len(canon(state_wire)) + sum(len(canon(r.wq.wire())) for r in part)) / 1.6) + 20

    def _charge(self, part, tokens):
        first = min(part, key=lambda r: r.seq)
        cost = tokens * self.prof.price_per_input_token
        if first.account is not None:
            first.account.charge(0, cost - (getattr(part[0], "reserved", 0.0) or 0.0))
        self.calls += 1
        self.questions += len(part)
        self.cost += cost
        if first.chain is not None:
            first.chain.sent_questions += len(part)
        res = (first.payer or "main").split("(")[0].split("#")[0]
        b = self.by_resident.setdefault(res, {"calls": 0, "questions": 0})
        b["calls"] += 1
        b["questions"] += len(part)
        return cost

    def _run_sync(self, part):
        miss = self._precheck(part)
        if miss:
            return self._finish_missing(part, miss)
        st = part[0].state.wire()
        lat = self.port.simulated_latency(st, [r.wq for r in part]) if hasattr(self.port, "simulated_latency") else 0.0
        if self.prof.timeout_s is not None and lat > self.prof.timeout_s:
            self._charge(part, self._est_tokens(st, part))
            return self._finish_missing(part, "latency")
        try:
            raws = self.port.call_sync(st, [r.wq for r in part])
        except (PortAbsent, JevAbsent) as e:
            return self._absent(part, str(e))
        self._finish(part, raws, self._est_tokens(st, part), lat)

    async def _run_async(self, part):
        try:
            miss = self._precheck(part)
            if miss:
                return self._finish_missing(part, miss)
            st = part[0].state.wire()
            t0 = self.eng.clock()
            before = getattr(getattr(self.port, "stats", None), "input_tokens", None)
            try:
                coro = self.port.call(st, [r.wq for r in part])
                raws = await (asyncio.wait_for(coro, self.prof.timeout_s) if self.prof.timeout_s else coro)
            except asyncio.TimeoutError:
                self._charge(part, self._est_tokens(st, part))
                return self._finish_missing(part, "latency")
            except (PortAbsent, JevAbsent) as e:
                if "max_tokens_exceeded" in str(e) and len(part) > 1:    # 估算偏低：对半拆开重发，不当缺席
                    half = len(part) // 2
                    first = min(part, key=lambda r: r.seq)
                    if first.account is not None:       # 退回发出时的预留，两半重发时各自再预留
                        first.account.charge(-1, -(getattr(part[0], "reserved", 0.0) or 0.0))
                    part[0].reserved = 0.0
                    for sub in (part[:half], part[half:]):
                        heapq.heappush(self.heap, ((-math.inf,), next(self.seq), sub))
                    self.splits = getattr(self, "splits", 0) + 1
                    return None
                return self._absent(part, str(e))
            after = getattr(getattr(self.port, "stats", None), "input_tokens", None)
            own = raws[0].get("_usage_input_tokens") if raws and isinstance(raws[0], dict) else None
            if own is not None:
                tokens = own                      # 本次调用自己的用量
            else:
                tokens = (after - before) if (before is not None and after is not None and after > before) else self._est_tokens(st, part)
            self._finish(part, raws, tokens, self.eng.clock() - t0)
        except Exception as e:     # 不让调度器静默死掉
            for r in part:
                if not r.done:
                    self.eng.resolve(r, None, exc=e)
        finally:
            self.active -= 1
            self._pump()
            self.eng.changed()

    def _finish(self, part, raws, tokens, latency):
        cost = self._charge(part, tokens)
        bid = next(self.batch_ids)
        self.latencies.append(latency)
        if self.absent:
            self.absent = False
            self.eng.on_recovered()
        owners = set()
        for r in part:
            owners |= {cx.uid for cx in r.owners}
        sid = part[0].state.sid
        f0 = min(part, key=lambda r: r.seq)
        self.eng.ledger.add("batch", id=bid, sid=sid, n=len(part), tokens=tokens, cost=round(cost, 8),
                            latency_ms=round(latency * 1000, 1), payer=first_payer(part), n_owners=len(owners),
                            chain=f0.chain.seq if f0.chain is not None else None,
                            pool=f0.pool.seq if f0.pool is not None else None)      # 记在哪条链的级联账上（预注册 16）
        self.eng.emit("batch", id=bid, n_states=1, n_questions=len(part), latency_ms=round(latency * 1000, 1),
                      merged_from=len(owners))
        for r, raw in zip(part, raws):
            rd = to_reading(r.q, raw, sid)
            r.batch = bid
            if not self.flags.get("no_cells"):
                self.cache[r.key] = rd
                if self.db is not None and not raw.get("synthetic"):
                    self.db.execute("insert or replace into judged values(?,?)", (r.key, json.dumps(raw)))
            syn = bool(raw.get("synthetic"))
            self.eng.ledger.add("judge", key=r.key, q=r.q.key, batch=bid, p=None if rd.p is None else round(rd.p, 4),
                                top=None if r.q.op == "test" else _top(rd), by="fixture-synthetic" if syn else "jev")
            self.eng.resolve(r, rd)
        if self.db is not None:
            self.db.commit()

    def _finish_missing(self, part, cause):
        if cause == "budget":
            self.budget_skips += len(part)
        self.eng.ledger.add("unobserved", cause=cause, n=len(part), sid=part[0].state.sid)
        for r in part:
            r.missing = cause
            self.live.pop(r.key, None)
            self.eng.resolve(r, None)

    def _absent(self, part, msg):
        self.absent = True
        first = min(part, key=lambda r: r.seq)
        if first.account is not None and getattr(part[0], "reserved", None) is not None:
            first.account.charge(-1, -part[0].reserved)      # 没调成：退回预留
            part[0].reserved = None
        self.eng.ledger.add("absent", msg=msg[:200], n=len(part))
        if self.flags.get("no_absent"):
            err = AbsentError(f"判断器缺席（--no-absent）：{msg}")
            for r in part:
                self.live.pop(r.key, None)
                self.eng.resolve(r, None, exc=err)
            return
        st = part[0].state.wire()
        for r in part:
            rd = degraded_reading(self.enc, st, r.q)
            r.degraded = True
            self.degraded_n += 1
            self.live.pop(r.key, None)      # 降级读数不进缓存，恢复后重判
            for cx in r.owners:
                self.eng.degraded_units.add(cx.uid)
            self.eng.resolve(r, rd)
        self.eng.schedule_absent_probe()

    # ------------------------------------------------------------ 截止
    def expire_due(self):
        """截止到了：还没发出、截止已过的题转 Unsure(deadline)。"""
        now = self.eng.clock()
        dead = [r for r in self.queue if _dl(r) is not None and _dl(r) <= now]
        if not dead:
            return
        for r in dead:
            self._unopen(r)
        ids = {id(r) for r in dead}
        self.queue = [r for r in self.queue if id(r) not in ids]
        self._finish_missing(dead, "deadline")

    def expire(self, chain):
        """截止到了：这条链还没发出的题转 Unsure(deadline)。"""
        dead = [r for r in self.queue if r.chain is chain]
        if not dead:
            return
        for r in dead:
            self._unopen(r)
        self.queue = [r for r in self.queue if r.chain is not chain]
        self._finish_missing(dead, "deadline")

    def stats(self) -> dict:
        lat = sorted(self.latencies)
        return {"calls": self.calls, "questions": self.questions, "cost_usd": round(self.cost, 6),
                "cache_hits": self.cache_hits, "dedup_hits": self.dedup_hits, "degraded": self.degraded_n,
                "budget_skips": self.budget_skips, "flushes": self.flushes, "cascade_calls": self.cascade_calls,
                "p50_ms": round(lat[len(lat) // 2] * 1000, 1) if lat else None,
                "p95_ms": round(lat[int(len(lat) * 0.95)] * 1000, 1) if lat else None}
