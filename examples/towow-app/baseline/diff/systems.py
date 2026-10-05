"""两个被测系统的驱动：J++x（app/net.jpx 跑在 jx 引擎上）与普通 Python 基线（baseline/net_py.py）。

两边共用：同一个规则判断器（经各自的端口形状）、同一个宿主索引与图算法、同一个假生成器、同一套检测。
本文件只做测量，不改两边的语义；注入在 faults.py。
"""
from __future__ import annotations

import asyncio
import json
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
APP = os.path.dirname(os.path.dirname(HERE))
for p in (APP, os.path.dirname(HERE)):
    if p not in sys.path:
        sys.path.insert(0, p)

from host.graph import graph_local, make_do_actions as graph_actions  # noqa: E402
from host.index import FragmentIndex, make_do_actions as index_actions  # noqa: E402
from jx.sched import HashEncoder, PortAbsent  # noqa: E402

import net_py  # noqa: E402
from diff.trajectory import (Truth, expected_edge, expected_whole, hx, view_of, answer, Side)  # noqa: E402

LIMITS = {"chain": 3, "ring": 4, "star": 6, "team": 5, "m2m": 4}
COMPANION = "材料里最缺哪一类信息"


def is_cfg(x) -> bool:
    return str(x).startswith("cfg:")


def members_of(key) -> list[str]:
    return str(key)[4:].split("+") if is_cfg(key) else [str(key)]


def mentions(obj, who: str) -> bool:
    if isinstance(obj, (list, tuple)):
        return any(mentions(x, who) for x in obj)
    return who in members_of(obj)


# ───────────────────────────── 端口核心：两边同一份记账
class Meter:
    def __init__(self, truth: Truth, fail_calls=()):
        self.T = truth
        self.fail_calls = set(fail_calls)
        self.calls = 0
        self.questions = 0
        self.companion = 0
        self.stale_calls = 0
        self.departed_calls = 0
        self.departed_by = {}          # 含离开者的调用按状态种类分：pair / whole / rerank（他自己的召回）/ rerank-q（题面里的对方）
        self.dup = 0
        self.seen = set()
        self.edge_writes = 0
        self.stale_writes = 0
        self.departed_writes = 0
        self.gen_calls = 0
        self.gen_prompts = set()
        self.failed = 0

    # 发出时看状态：有没有已离开者、有没有不是当前的材料
    def check_state(self, state):
        try:
            v = view_of(state)
        except ValueError:
            return
        T = self.T
        agents = [s for s in v.sides if s.kind == "agent"]
        if any(not T.present(s.id) for s in agents):
            self.departed_calls += 1
            self.departed_by[v.kind] = self.departed_by.get(v.kind, 0) + 1
            return
        if v.kind == "pair" and len(agents) == 2:
            exp = T.pair_sides(*sorted(s.id for s in agents))
            got = sorted(agents, key=lambda s: s.id)
            if any((a.needs, a.offers) != (b.needs, b.offers) for a, b in zip(got, exp)):
                self.stale_calls += 1
        elif v.kind == "whole" and agents and len(agents) == len(v.sides):
            from diff.trajectory import whole_sides
            exp = {s.id: s for s in whole_sides(T, [s.id for s in agents])}
            if any((s.needs, s.offers) != (exp[s.id].needs, exp[s.id].offers) for s in agents):
                self.stale_calls += 1
        elif v.kind == "rerank":
            me = v.sides[0]
            n, o = T.t0(me.id)
            if (me.needs, me.offers) != (n, o):
                self.stale_calls += 1

    def check_question(self, text):
        if "对方：" in text and "读「我」的世界" in text:
            who = text.split("对方：", 1)[1].split("。", 1)[0]
            if who and not who.startswith("（") and who in self.T.ever and not self.T.present(who):
                self.departed_calls += 1
                self.departed_by["rerank-q"] = self.departed_by.get("rerank-q", 0) + 1

    def call(self, state, qs):
        """qs: [(text, op, labels)]。返回规则答案（未转端口格式）。"""
        self.calls += 1
        if self.calls in self.fail_calls:
            self.failed += 1
            raise PortAbsent(f"注入：第 {self.calls} 次调用失败")
        self.check_state(state)
        sk = json.dumps(state, sort_keys=True, ensure_ascii=False, default=str)
        v = view_of(state)
        out = []
        for text, op, labels in qs:
            if COMPANION in text:
                self.companion += 1
            else:
                self.questions += 1
                self.check_question(text)
            k = (sk, text, op, tuple(labels))
            if k in self.seen:
                self.dup += 1
            self.seen.add(k)
            out.append(answer(v, text, op, labels))
        return out

    def on_edge_write(self, a, b, holds, dirv, p):
        self.edge_writes += 1
        if is_cfg(a) or is_cfg(b):
            return
        if not (self.T.present(a) and self.T.present(b)):
            self.departed_writes += 1
            return
        e = expected_edge(self.T, a, b)
        if (bool(holds), dirv, round(float(p or 0), 3)) != (e["holds"], e["dir"], e["p"]):
            self.stale_writes += 1

    def gen(self, cfg):
        self.gen_calls += 1
        self.gen_prompts.add(json.dumps(cfg, sort_keys=True, ensure_ascii=False, default=str))
        return {"title": "方案", "summary": "s", "to_confirm": ["时间安排"], "draft": bool(cfg.get("draft"))}

    def stats(self):
        return {"calls": self.calls, "questions": self.questions, "companion_q": self.companion,
                "stale_calls": self.stale_calls, "departed_calls": self.departed_calls, "departed_by": dict(self.departed_by),
                "dup_sends": self.dup,
                "edge_writes": self.edge_writes, "stale_writes": self.stale_writes, "departed_writes": self.departed_writes,
                "gen_calls": self.gen_calls, "gen_distinct": len(self.gen_prompts), "port_failures": self.failed}


async def ticks(n):
    for _ in range(n):
        await asyncio.sleep(0)


def port_ticks(state) -> int:
    return hx(json.dumps(state, sort_keys=True, ensure_ascii=False, default=str)) % 6


# ───────────────────────────── J++x
class JxPort:
    sync = False

    def __init__(self, meter: Meter):
        self.m = meter

    async def call(self, state, wqs):
        ans = self.m.call(state, [(q.text, q.op, list(q.criteria.keys()) if q.op == "select" else
                                   [str(i) for i in range(len(q.criteria or []))]) for q in wqs])
        await ticks(port_ticks(state))
        out = []
        for q, a in zip(wqs, ans):
            if q.op == "test":
                out.append({"type": "noul", "noul": float(a)})
            elif q.op == "select":
                labs = list(q.criteria.keys())
                out.append({"type": "choice", "probabilities": {x: (0.9 if x == a else 0.1 / max(1, len(labs) - 1)) for x in labs}})
            else:
                n = len(q.criteria)
                out.append({"type": "score", "probabilities": {str(i): (0.9 if i == a else 0.1 / max(1, n - 1)) for i in range(n)}})
        return out


class JxGen:
    def __init__(self, meter: Meter):
        self.m = meter

    async def gen_json(self, kind, args):
        cfg = args[0] if isinstance(args, list) and args else {}
        await asyncio.sleep(0)
        return self.m.gen(cfg if isinstance(cfg, dict) else {})


JX_FLAGS: dict = {}          # run.py --jx-flags 设置（如 no_cascade：关掉事件链的级联预算，预注册 16）


class JxSystem:
    name = "jpx"

    def __init__(self, truth: Truth, fail_calls=()):
        from jx.engine import Engine, VirtualClock
        self.T = truth
        self.m = Meter(truth, fail_calls)
        enc = HashEncoder()
        self.eng = Engine.load(os.path.join(APP, "app", "net.jpx"), ports={"judge": JxPort(self.m), "enc": enc},
                               clock=VirtualClock(), seed=0, flags=dict(JX_FLAGS))
        self.ix = FragmentIndex(enc, cap=1024, threads=1)
        for name, fn in index_actions(self.ix).items():
            self.eng.register_action(name, fn, transparent=(name != "index_put"))
        for name, fn in graph_actions().items():
            self.eng.register_action(name, fn, transparent=True)
        self.eng.gen_port = JxGen(self.m)
        self.eng.bus.subscribe(self._on_bus)

    def _on_bus(self, ev):
        if ev.get("type") == "publish" and ev.get("cell") == "edge" and ev.get("status") != "removed":
            v = ev.get("value") or {}
            d = v.get("decisive") or {}
            self.m.on_edge_write(v.get("a"), v.get("b"), v.get("holds"), (v.get("dir") or {}).get("value"), d.get("p"))

    async def start(self):
        await self.eng.start()

    async def apply(self, e):
        E = self.eng
        if e.kind in ("join", "update"):
            await E.put_source("world", [e.who], e.world, cause=f"{e.kind}:{e.who}")
        elif e.kind == "grant":
            await E.put_source("unlocked", [e.who, e.to], e.frags)
            await E.put_source("reply", [e.who, e.to, "近况细节"], {"granted": True, "frags": e.frags, "by": e.who})
        elif e.kind == "deny":
            await E.put_source("reply", [e.who, e.to, "近况细节"], {"denied": True, "cat": "近况细节", "by": e.who, "text": None})
        elif e.kind == "leave":                      # 同 host/server.py 的 leave
            await E.remove_source("world", [e.who])
            self.ix.remove(e.who)
            for fam in ("unlocked", "reply"):
                for k in list(E.keys(fam, contains=e.who)):
                    await E.remove_source(fam, list(k))

    async def settle(self, timeout):
        await self.eng.idle(timeout=timeout)
        return {"hang": not self.eng._quiet(), "raised": None, "engine_errors": len(self.eng.errors),
                "cutoffs": self.eng.skipped}

    def _vals(self, fam):
        return {tuple(k): self.eng.read(fam, k) for k in self.eng.keys(fam)}

    def departed_visible(self, departed):
        return sum(1 for (a, b), v in self._vals("edge").items()
                   if v and v.get("holds") and (set(members_of(a) + members_of(b)) & departed))

    def final(self):
        edges = {}
        for (a, b), v in self._vals("edge").items():
            if v is None:
                continue
            d = v.get("decisive") or {}
            edges[(a, b)] = {"holds": bool(v.get("holds")), "dir": (v.get("dir") or {}).get("value"),
                             "form": (v.get("form") or {}).get("value"), "p": round(float(d.get("p") or 0), 3),
                             "exit": d.get("exit")}
        configs = {}
        for (k,), v in self._vals("config").items():
            hold = v.get("hold") or {}
            configs[k] = {"members": list(v["members"]), "shape": v["shape"], "p": round(float(hold.get("p") or 0), 3)}
        plans = {k: bool((v or {}).get("draft")) for (k,), v in self._vals("plan").items()}
        best = {m: {kk: e.get("p") for kk, e in (v or {}).items()} for (m,), v in self._vals("best").items()}
        fams = {f: self._vals(f) for f in ("world", "material", "node", "edge", "adj", "derived", "config", "plan", "best",
                                           "unlocked", "reply", "inbox", "cand", "hits")}
        return {"edges": edges, "configs": configs, "plans": plans, "best": best, "fams": fams,
                "index": {o: n for o, n in self.ix.nodes.items()}}


# ───────────────────────────── 基线
class PyPorts(net_py.Ports):
    def __init__(self, meter: Meter, ix: FragmentIndex):
        self.m, self.ix = meter, ix

    async def judge(self, state, qs):
        try:
            ans = self.m.call(state, [(q.text, q.kind, list(q.opts) if q.kind == "select" else [str(i) for i in range(len(q.opts))])
                                      for q in qs])
        except PortAbsent as e:
            raise RuntimeError(str(e))
        await ticks(port_ticks(state))
        out = []
        for q, a in zip(qs, ans):
            if q.kind == "test":
                out.append(net_py.Reading(act=a > 0.5, p=float(a)))
            elif q.kind == "select":
                out.append(net_py.Reading(value=a, p=0.9))
            else:
                out.append(net_py.Reading(value=q.opts[a], p=0.9))
        return out

    async def route(self, x, node, k): return self.ix.route(x, node, k)
    def present(self, x): return self.ix.present(x)
    def index_put(self, x, node): self.ix.index_put(x, node)
    def index_del(self, x): self.ix.remove(x)
    def graph_local(self, x, links, limits): return graph_local(x, links, limits)
    async def route_offers(self, cfg, k): return self.ix.route_offers(cfg, k)

    async def gen_plan(self, cfg, mats):
        await asyncio.sleep(0)
        return self.m.gen(cfg)


class WatchEdges(dict):
    def __init__(self, meter):
        super().__init__()
        self.m = meter

    def __setitem__(self, k, e):
        super().__setitem__(k, e)
        self.m.on_edge_write(k[0], k[1], e["holds"], net_py.pick(e["dir"]), e["decisive"]["p"])


class PySystem:
    name = "py"
    net_cls = net_py.Net

    def __init__(self, truth: Truth, fail_calls=()):
        self.T = truth
        self.m = Meter(truth, fail_calls)
        self.ix = FragmentIndex(HashEncoder(), cap=1024, threads=1)
        self.net = self.net_cls(PyPorts(self.m, self.ix))
        self.net.edge = WatchEdges(self.m)

    async def start(self):
        pass

    async def apply(self, e):
        N = self.net
        if e.kind in ("join", "update"):
            await N.join(e.who, e.world)               # 基线的 join 就是写 world（同 put_source world）
        elif e.kind == "grant":
            await N.reply_event(e.who, e.to, "近况细节", {"granted": True, "frags": e.frags})
        elif e.kind == "deny":
            await N.reply_event(e.who, e.to, "近况细节", {"denied": True, "cat": "近况细节", "by": e.who})
        elif e.kind == "leave":
            await N.leave(e.who)

    async def settle(self, timeout):
        try:
            await asyncio.wait_for(self.net.idle.wait(), timeout)
        except asyncio.TimeoutError:
            return {"hang": True, "raised": None, "engine_errors": len(self.net.errors), "cutoffs": 0}
        raised = None
        try:
            await self.net.quiet()
        except Exception as e:  # noqa: BLE001  quiet() 把记下的第一个异常抛出来
            raised = f"{type(e).__name__}: {e}"[:200]
        return {"hang": False, "raised": raised, "engine_errors": len(self.net.errors), "cutoffs": 0}

    def departed_visible(self, departed):
        return sum(1 for (a, b), e in self.net.edge.items()
                   if e.get("holds") and (set(members_of(a) + members_of(b)) & departed))

    def final(self):
        N = self.net
        edges = {k: {"holds": bool(e["holds"]), "dir": net_py.pick(e["dir"]), "form": net_py.pick(e["form"]),
                     "p": round(float(e["decisive"]["p"]), 3), "exit": e["decisive"]["exit"]} for k, e in N.edge.items()}
        configs = {}
        for k, c in N.config.items():
            h = c["hold"]
            p = h["p"] if isinstance(h, dict) else h.p
            configs[k] = {"members": list(c["members"]), "shape": c["shape"], "p": round(float(p), 3)}
        plans = {k: bool((v or {}).get("draft")) for k, v in N.plan.items()}
        fams = {"world": N.world, "material": N.material, "node": N.node, "edge": dict(N.edge), "adj": N.adj,
                "derived": N.derived, "config": N.config, "plan": N.plan, "best": N.best, "unlocked": N.unlocked,
                "reply": N.reply, "inbox": N.inbox, "cand": N.cand, "hits": N.hits}
        fams = {f: {(k if isinstance(k, tuple) else (k,)): v for k, v in d.items()} for f, d in fams.items()}
        return {"edges": edges, "configs": configs, "plans": plans, "best": dict(N.best), "fams": fams,
                "index": {o: n for o, n in self.ix.nodes.items()}}


# ───────────────────────────── 终态检测（两边同一套）
STRICT = ("world", "material", "node", "edge", "adj", "derived", "config", "plan", "best", "unlocked", "reply")
UNION = ("inbox", "cand", "hits")


def residue(fin, departed):
    """含已离开者的条目：键里有他、按对方覆盖的单元里有他、best 里有含他的构型、索引里有他。"""
    out = {}
    for fam, d in fin["fams"].items():
        n = 0
        for k, v in d.items():
            if any(mentions(x, w) for x in k for w in departed):
                if v not in (None, {}, []):
                    n += 1
                continue
            if fam in ("adj", "derived", "best") and isinstance(v, dict):
                n += sum(1 for kk in v if any(mentions(kk, w) for w in departed))
            if fam == "inbox" and isinstance(v, list):
                n += sum(1 for r in v if isinstance(r, dict) and r.get("from") in departed)
        if n:
            out[fam] = n
    ix = sum(1 for o, n in fin["index"].items() if any(mentions(o, w) for w in departed)
             or any(m in departed for m in (n or {}).get("members") or []))
    if ix:
        out["index"] = ix
    strict = {k: v for k, v in out.items() if k not in UNION}
    union = {k: v for k, v in out.items() if k in UNION}
    return strict, union


def expected_configs(T: Truth):
    """按最终输入：每个在场者跑同一个 graph_local，成立的候选。"""
    present = sorted(T.world)
    links = {}
    for i, a in enumerate(present):
        for b in present[i + 1:]:
            e = expected_edge(T, a, b)
            if e["holds"]:
                links[(a, b)] = {"a": a, "b": b, "holds": True, "dir": e["dir"], "form": e["form"]}
    out = {}
    for x in present:
        es1 = [l for k, l in links.items() if x in k]
        nb = {l["a"] if l["b"] == x else l["b"] for l in es1}
        es = list({(l["a"], l["b"]): l for l in es1 + [l for k, l in links.items() if set(k) & nb]}.values())
        for c in graph_local(x, es, LIMITS):
            ms = c["members"]
            if c["shape"] == "pair":
                e = expected_edge(T, *sorted(ms))
                ok, p = e["holds"], e["p"]
            else:
                ok, p = expected_whole(T, ms)
            if ok:
                out["cfg:" + "+".join(sorted(ms))] = {"members": sorted(ms), "shape": c["shape"], "p": p}
    return out


def check(fin, T: Truth):
    present = sorted(T.world)
    departed = set(T.left) - set(present)
    errs = {"edge_wrong": [], "edge_missing": [], "config_invalid": [], "config_missing": [], "plan_wrong": []}
    for i, a in enumerate(present):
        for b in present[i + 1:]:
            e = expected_edge(T, a, b)
            got = fin["edges"].get((a, b))
            if got is None:
                errs["edge_missing"].append(f"{a}|{b}" + (" 应成立" if e["holds"] else ""))
            elif (got["holds"], got["dir"], got["form"], got["p"], got["exit"]) != (e["holds"], e["dir"], e["form"], e["p"], e["exit"]):
                errs["edge_wrong"].append(f"{a}|{b} 得 {got} 应 {e}")
    for k, c in fin["configs"].items():
        ms = c["members"]
        if any(m not in T.world for m in ms):
            errs["config_invalid"].append(f"{k} 有成员不在场")
            continue
        if c["shape"] == "pair" and len(ms) == 2:
            e = expected_edge(T, *sorted(ms))
            ok, p = e["holds"], e["p"]
        else:
            ok, p = expected_whole(T, ms)
        if not ok or round(p, 3) != c["p"]:
            errs["config_invalid"].append(f"{k} 应 {'成立' if ok else '不成立'} p={p} 得 p={c['p']}")
    exp = expected_configs(T)
    for k in exp:
        if k not in fin["configs"]:
            errs["config_missing"].append(k)
    # 方案门：按系统自己的终态构型算每位成员的前 2
    best = {}
    for k, c in fin["configs"].items():
        for m in c["members"]:
            best.setdefault(m, []).append((c["p"], k))
    want = {}
    for k, c in fin["configs"].items():
        ms = c["members"]
        if len(ms) <= 5 and all(k in [kk for _, kk in sorted(best[m], key=lambda t: -t[0])[:2]] for m in ms):
            want[k] = any(not T.all_unlocked(m, ms) for m in ms)
    for k in set(want) | set(fin["plans"]):
        if want.get(k) != fin["plans"].get(k):
            errs["plan_wrong"].append(f"{k} 应 {want.get(k)} 得 {fin['plans'].get(k)}")
    strict, union = residue(fin, departed)
    n = {k: len(v) for k, v in errs.items()}
    n["residue"] = sum(strict.values())
    return n, errs, strict, union


def compare(f1, f2):
    """两边终态逐项比：agent 之间的边、构型（成员键、读数；形状单列）、方案（键、草案）。"""
    def ag(edges):
        return {k: v for k, v in edges.items() if not (is_cfg(k[0]) or is_cfg(k[1]))}
    e1, e2 = ag(f1["edges"]), ag(f2["edges"])
    d_edges = sorted(f"{k}" for k in set(e1) | set(e2) if e1.get(k) != e2.get(k))
    c1 = {k: v["p"] for k, v in f1["configs"].items()}
    c2 = {k: v["p"] for k, v in f2["configs"].items()}
    d_cfg = sorted(k for k in set(c1) | set(c2) if c1.get(k) != c2.get(k))
    d_shape = sorted(k for k in set(c1) & set(c2) if f1["configs"][k]["shape"] != f2["configs"][k]["shape"])
    d_plan = sorted(k for k in set(f1["plans"]) | set(f2["plans"]) if f1["plans"].get(k) != f2["plans"].get(k))
    return {"edges": d_edges, "configs": d_cfg, "shapes": d_shape, "plans": d_plan,
            "same": not (d_edges or d_cfg or d_plan)}
