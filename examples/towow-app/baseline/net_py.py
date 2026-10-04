"""net.jpx 的普通 Python + asyncio 对照实现（不依赖 jx）。

判断、编码召回、图算法、生成器经 Ports 注入（桩）；其余——依赖追踪、失效传播、撤回、合批、缓存、
补信息链、预算/截止、深度、E-tier、静止判定——全部在本文件手写。接口约定见 Ports。
"""
from __future__ import annotations
import asyncio, hashlib, json
from dataclasses import dataclass
from typing import Any

LACKS = ("时间安排", "预算或报酬", "具体技能或资源", "地点与距离", "意愿与动机", "身份与可信度", "近况细节")
VALUE_REF = "小：一次性的小忙；中：持续几周、双方都有实际收益；大：可能改变一方的生计、事业或生活处境。"
CAT_SRC = {"时间安排": ["calendar", "note", "project"], "预算或报酬": ["message", "note", "project"],
           "具体技能或资源": ["skill", "project", "relation"], "地点与距离": ["bio", "relation", "calendar"],
           "意愿与动机": ["note", "message", "project"], "身份与可信度": ["bio", "skill", "relation"],
           "近况细节": ["note", "message", "calendar"]}
RANK = {"t0": 0, "t1": 1, "t2": 2}
LIMITS = {"chain": 3, "ring": 4, "star": 6, "team": 5, "m2m": 4}


# ───────────── 端口接口（桩实现这些）
@dataclass(frozen=True)
class Q:                     # 一道题；kind: test | select | measure；lacks 非空表示未决时可选「缺哪类」
    text: str
    kind: str
    tag: str
    opts: tuple = ()
    lacks: tuple = ()


@dataclass(frozen=True)
class Reading:               # act: True/False/None(未决)；value: select/measure 的取值；needed: {"cat", "holder"}
    act: bool | None = None
    p: float = 1.0
    value: Any = None
    needed: tuple | None = None   # (cat, holder)
    cause: str | None = None


class Ports:
    async def judge(self, state: dict, qs: list[Q]) -> list[Reading]: raise NotImplementedError
    async def route(self, x: str, node: dict, k: int) -> list[dict]: raise NotImplementedError   # [{peer, routes:[{route,mine,theirs}]}]
    def index_put(self, x: str, node: dict) -> None: pass
    def index_del(self, x: str) -> None: pass
    def graph_local(self, x: str, links: list[dict], limits: dict) -> list[dict]: return []       # [{shape, members, roles}]
    async def route_offers(self, cfg: dict, k: int) -> list[dict]: return []                        # [{id}]
    async def gen_plan(self, cfg: dict, mats: list[dict]) -> dict: return {}


def h(o) -> str: return hashlib.sha1(json.dumps(o, sort_keys=True, ensure_ascii=False, default=str).encode()).hexdigest()
def pair_key(x, y): return (x, y) if x < y else (y, x)
def members_key(ms): return "cfg:" + "+".join(sorted(ms))
def is_cfg(m): return m.startswith("cfg:")
def tier_of(f): return f.get("tier", "t0") if isinstance(f, dict) else "t0"
def tier_le(xs, t): return [x for x in xs or [] if RANK.get(tier_of(x), 9) <= RANK[t]]
def uniq(xs):
    seen, out = set(), []
    for x in xs:
        k = h(x)
        if k not in seen: seen.add(k); out.append(x)
    return out
def act(r): return r is not None and r.act is True
def pick(r): return r.value if r is not None else None
def texts(xs): return [x.get("text") or x.get("can") or x.get("name") or json.dumps(x, ensure_ascii=False) if isinstance(x, dict) else x for x in xs or []]
def frag_text(f): return (f.get("text") or f.get("can") or f.get("hypo") or f.get("name") or json.dumps(f, ensure_ascii=False)) if isinstance(f, dict) else f


def by_category(w, cat, tier):
    same = [f for f in (w.get("signals", []) + w.get("offers", []) + w.get("projects", []) + w.get("catchers", [])) if tier_of(f) == tier]
    srcs = CAT_SRC.get(cat, [])
    hit = [f for f in same if f.get("cat") == cat or f.get("src") in srcs]
    return [{"text": frag_text(f), "tier": tier, "cat": cat} for f in (hit or same)[:3]]


def tests(text, tag, lacks=()): return Q(text, "test", tag, (), tuple(lacks))
def select(text, opts, tag): return Q(text, "select", tag, tuple(opts))
def measure(text, opts, tag): return Q(text, "measure", tag, tuple(opts))


class Net:
    def __init__(self, ports: Ports, calls=100000, depth=3, concurrency=32, timeout=None):
        self.p, self.cap, self.timeout, self.sem = ports, depth, timeout, asyncio.Semaphore(concurrency)
        self.calls_left, self.q_sent, self.n_calls = calls, 0, 0
        # 单元（状态）
        self.world, self.material, self.node, self.derived = {}, {}, {}, {}
        self.unlocked, self.hits, self.edge, self.adj = {}, {}, {}, {}
        self.cand, self.config, self.plan, self.inbox, self.reply, self.pending = {}, {}, {}, {}, {}, {}
        # 手写的反向依赖表
        self.pairs_of, self.cands_of, self.grow_deps = {}, {}, {}
        # 调度
        self.running, self.rerun, self.busy, self.errors = set(), set(), 0, []
        self.cache, self.queue, self.parked, self.flushing = {}, {}, [], False
        self.idle = asyncio.Event(); self.idle.set()

    # ───────────── 调度：常驻单元、重跑标记、静止
    def mark(self, kind, key):
        u = (kind, key)
        if u in self.running: self.rerun.add(u); return
        self.running.add(u); self.busy += 1; self.idle.clear()
        asyncio.get_running_loop().create_task(self._run(u))

    async def _run(self, u):
        try:
            while True:
                self.rerun.discard(u)
                await getattr(self, "_" + u[0])(u[1])
                if u not in self.rerun: break
        except Exception as e:  # noqa: BLE001
            self.errors.append((u, e))
        finally:
            self.running.discard(u); self.busy -= 1; self._kick(); self._check_idle()

    def stale(self, kind, key): return (kind, key) in self.rerun

    def _check_idle(self):
        if self.busy == 0 and not self.queue and not self.flushing and not self.parked: self.idle.set()

    async def quiet(self):
        await self.idle.wait()
        if self.errors: raise self.errors[0][1]

    # ───────────── 判断：缓存键 (状态内容, 题)、E-tier 闸、按状态合批、并发、预算、截止
    def _etier(self, s):
        for side in s.get("sides", []):
            for f in side["已解锁"]:
                if RANK.get(f["tier"], 9) > 0:
                    for o in s["owners"]:
                        if o != f["owner"] and not any(u["text"] == f["text"] for u in self.unlocked.get((f["owner"], o), [])):
                            raise AssertionError(f"E-tier: {f['owner']} 的 {f['tier']} 片段未向 {o} 解锁")

    async def judge_many(self, items):                     # items: [(state, [Q])]；一次挂起，保证合批
        keyed, needs = [], set()
        for s, qs in items:
            self._etier(s)
            sh = h(s)
            keyed.append((sh, qs))
            for q in qs:
                if (sh, q) not in self.cache:
                    needs.add((sh, q)); self.queue.setdefault(sh, (s, {}))[1][q] = 1
        if needs:
            fut = asyncio.get_running_loop().create_future()
            self.parked.append((needs, fut)); self.busy -= 1
            self._kick()
            await fut
        return [[self.cache[(sh, q)] for q in qs] for sh, qs in keyed]

    async def judge(self, s, qs): return (await self.judge_many([(s, qs)]))[0]

    def _kick(self):
        if self.busy == 0 and self.queue and not self.flushing:
            self.flushing = True
            asyncio.get_running_loop().create_task(self._flush())

    async def _flush(self):
        groups, self.queue = self.queue, {}
        async def one(sh, s, qs):
            async with self.sem:
                if self.calls_left <= 0:
                    rs = [Reading(cause="budget") for _ in qs]
                else:
                    self.calls_left -= len(qs); self.q_sent += len(qs); self.n_calls += 1
                    try: rs = await asyncio.wait_for(self.p.judge(s, list(qs)), self.timeout)
                    except asyncio.TimeoutError: rs = [Reading(cause="deadline") for _ in qs]
                    except Exception as e:  # noqa: BLE001  端口失败 → 未决（缺席类），不拖死合批
                        self.errors.append((("judge", sh), e)); rs = [Reading(cause="absent") for _ in qs]
            for q, r in zip(qs, rs):
                self.cache[(sh, q)] = r
        await asyncio.gather(*[one(sh, s, list(qs)) for sh, (s, qs) in groups.items()])
        still = []
        for needs, fut in self.parked:
            if all(n in self.cache for n in needs): self.busy += 1; fut.set_result(None)
            else: still.append((needs, fut))
        self.parked, self.flushing = still, False
        self._kick(); self._check_idle()

    # ───────────── 宿主事件
    async def join(self, a, world):
        self.world[a] = world; self.mark("pub", a); self.mark("disc", a)

    async def disclose(self, a, patch):
        w = dict(self.world[a])
        for k, v in patch.items(): w[k] = (w.get(k, []) + v) if isinstance(v, list) else v
        self.world[a] = w; self.mark("pub", a); self.mark("disc", a)

    async def leave(self, a):
        if a not in self.world: return
        for d in (self.world, self.material, self.node, self.derived, self.inbox): d.pop(a, None)
        self.p.index_del(a)
        for k in list(self.pairs_of.get(a, ())): self.mark("pair", k)            # 两两发现成员缺席 → 自己撤回
        for ck in list(self.cands_of.get(a, ())): self._kill_cand(ck)
        for key in [k for k in self.unlocked if a in k]: self.unlocked.pop(key)
        for key in [k for k in self.reply if a in k[:2]]: self.reply.pop(key)
        for b, reqs in self.inbox.items(): self.inbox[b] = [r for r in reqs if r["from"] != a]
        self.mark("recall", a)

    async def reply_event(self, b, a, cat, r):             # 真实 agent 的回复，与模拟 agent 写同一处
        self._put_reply(b, a, cat, r)

    # ───────────── 发布：接入即发布可发现表示
    async def _pub(self, a):
        w = self.world.get(a)
        if w is None: return
        dv = sorted((d for d in self.derived.get(a, {}).values() if d), key=h)
        n = {"id": a, "kind": "agent", "display": w["display"], "lang": w.get("lang"),
             "signals": tier_le(w.get("signals"), "t0"), "offers": tier_le(w.get("offers"), "t0"),
             "catchers": tier_le(w.get("catchers"), "t0"), "forbids": w.get("forbids", []), "forbids_ids": w.get("forbids_ids", []),
             "projects": tier_le(w.get("projects"), "t0"), "derived": uniq(dv), "members": [a], "background": w.get("background", False)}
        mat = {**n, "derived": []}
        if self.node.get(a) != n:
            self.node[a] = n; self.p.index_put(a, n)
        if self.material.get(a) != mat:
            self.material[a] = mat; self._material_changed(a)

    def _material_changed(self, x):
        self.mark("recall", x)
        for k in list(self.pairs_of.get(x, ())): self.mark("pair", k)
        for ck in list(self.cands_of.get(x, ())):
            self.mark("whole", ck)
            if ck in self.config: self.mark("plan", ck)
        for k in list(self.grow_deps.get(x, ())): self.mark("grow", k)

    # ───────────── 召回：只在自己的材料变时重跑；命中累积进 hits，新对派生两两
    async def _recall(self, x):
        n = self.node.get(x)
        if n is None: return
        bg = n.get("background") is True
        hits = [] if bg else await self.p.route(x, n, 20)
        width = 8 if n.get("kind") == "config" else 32
        peers = [hh for hh in hits if hh["peer"] != x and hh["peer"] not in n.get("forbids_ids", [])][:width]
        if self.stale("recall", x): return
        for hh in peers:
            k = pair_key(x, hh["peer"])
            rs = [{"route": r["route"], "mine": r["mine"], "theirs": r["theirs"]} for r in hh.get("routes", [])]
            merged = uniq(self.hits.get(k, []) + rs)
            fresh_pair = k not in self.pairs_of.get(x, ())
            for m in k: self.pairs_of.setdefault(m, set()).add(k)
            if merged != self.hits.get(k) or fresh_pair:
                self.hits[k] = merged; self.mark("pair", k)

    # ───────────── 两两：一个状态挂齐全部题；未决 → 补信息链
    def _side(self, n, un):
        return {"display": n["display"] if "display" in n else "", "kind": n.get("kind", "agent"), "signals": texts(n.get("signals")),
                "offers": texts(n.get("offers")), "projects": texts(n.get("projects")),
                "catchers": [c.get("can") or c.get("text") or c for c in n.get("catchers", [])], "members": n.get("members", [n["id"]]),
                "已解锁": [{**f, "owner": n["id"], "tier": tier_of(f)} for f in un or []], "owner": n["id"]}

    def _drop_pair(self, k):
        self.edge.pop(k, None); self.pending.pop(("pair", k), None)
        for x, y in (k, k[::-1]):
            if self.adj.get(x, {}).pop(y, None) is not None: self.mark("cfg", x)
            if self.derived.get(x, {}).pop(y, None) is not None and x in self.world: self.mark("pub", x)
            self.pairs_of.get(x, set()).discard(k)
        self.hits.pop(k, None)

    async def _pair(self, k):
        a, b = k
        if a not in self.material or b not in self.material: return self._drop_pair(k)
        A, B, H = self.material[a], self.material[b], self.hits.get(k, [])
        UA, UB = self.unlocked.get((a, b), []), self.unlocked.get((b, a), [])
        s = {"sides": [self._side(A, UA), self._side(B, UB)], "owners": [a, b], "ref": VALUE_REF}
        seen = [x["mine"] for x in H] + [x["theirs"] for x in H]
        def hit_c(cs): return [c for c in cs if c.get("hypo") in seen] + [c for c in cs if c.get("hypo") not in seen]
        def cq(recv, snd, c): return f"把 {snd} 当作来信的人。{recv} 事先写了一道题来确认来信人是不是自己接得住的那种人，请就 {snd} 的情况回答：{c.get('confirm') or c.get('hypo') or ''}"
        cab = [tests(cq("B", "A", c), "catcher") for c in hit_c(B.get("catchers", []))[:2]]
        cba = [tests(cq("A", "B", c), "catcher") for c in hit_c(A.get("catchers", []))[:2]]
        qs = [tests("读 B 的世界，B（或 B 身边的人）能对 A 现在的状态或需要做点实在的事吗？", "open", LACKS),
              tests("读 A 的世界，A（或 A 身边的人）能对 B 现在的状态或需要做点实在的事吗？", "open", LACKS),
              select("如果他们合作，谁主要帮谁？", ["ab", "ba", "both", "none"], "dir"),
              select("最可能的合作形式是哪一种？", ["direct", "oneway", "relay_a", "relay_b", "third", "none"], "form"),
              measure("如果合作成了，对双方的价值有多大？", ["小", "中", "大"], "value"),
              tests("双方现在的时间、阶段与条件对得上吗？", "timing")] + cab + cba + \
             [tests("他们其实在找同一种人或同一种资源吗？", "neg-same"), tests("对方的问题其实已经解决了吗？", "neg-solved"),
              tests("他们的价值观或底线有冲突吗？", "neg-values")]
        r = await self.judge(s, qs)
        if self.stale("pair", k): return
        form_ok = pick(r[3]) not in ("none", None)
        hab, hba = form_ok and act(r[0]), form_ok and act(r[1])
        e = {"a": a, "b": b, "holds": hab or hba, "dir": r[2], "form": r[3], "value": r[4], "timing": r[5],
             "negs": r[-3:], "catchers": r[6:6 + len(cab) + len(cba)],
             "tier_seen": sorted({"t0"} | {tier_of(f) for f in UA + UB})}
        # 未决 → 默认链：问缺哪类（伴随信息在同批读数里）→ 向持有方要；拒绝则记入 pending
        pend = []
        for rd in r[:4]:
            if rd.act is None and rd.value is None and rd.needed:
                cat, holder = rd.needed; asker = a if holder == b else b
                rep = self.reply.get((holder, asker, cat))
                if rep is None: self._request(holder, asker, cat, "判断这一对是否成立")
                elif rep.get("denied"): pend.append({"needed": f"缺 {cat}，持有者 {holder} 拒绝"})
            elif rd.cause: pend.append({"cause": rd.cause})
        self.pending[("pair", k)] = pend
        self.edge[k] = e
        link = {"a": a, "b": b, "holds": e["holds"], "dir": pick(r[2]), "form": pick(r[3])}
        for x, y, A_, B_ in ((a, b, A, B), (b, a, B, A)):
            if self.adj.setdefault(x, {}).get(y) != {**link, "peer": y}:
                self.adj[x][y] = {**link, "peer": y}; self.mark("cfg", x)
            fr = ({"text": f"{A_['display']} 与 {B_['display']} 可能形成：{pick(r[3])}", "tier": "t0", "src": "derived"} if form_ok else None)
            if self.derived.setdefault(x, {}).get(y) != fr:
                self.derived[x][y] = fr; self.mark("pub", x)

    # ───────────── 补信息：请求 → 持有方披露策略 → 回复 → 补进再判
    def _request(self, holder, asker, cat, purpose):
        req = {"from": asker, "cat": cat, "purpose": purpose, "asker_display": (self.material.get(asker) or {}).get("display", asker)}
        box = self.inbox.setdefault(holder, [])
        if req not in box: box.append(req); self.mark("disc", holder)

    def _put_reply(self, b, a, cat, r):
        if self.reply.get((b, a, cat)) == r: return
        self.reply[(b, a, cat)] = r
        frags = [f for (bb, aa, _), rp in sorted(self.reply.items(), key=lambda kv: kv[0][2]) if (bb, aa) == (b, a) for f in rp.get("frags", [])]
        self.unlocked[(b, a)] = frags                       # 已解锁 = 该对所有已同意回复的并集，回复改了它跟着改
        self.mark("pair", pair_key(a, b))
        for ck in list(self.cands_of.get(b, ())): self.mark("whole", ck)

    async def _disc(self, b):
        w = self.world.get(b)
        if w is None: return
        todo = [] if w.get("real") else uniq(self.inbox.get(b, []))
        items = [({"request": q["purpose"], "category": q["cat"], "asker": q["asker_display"], "policy": w.get("policy"), "sides": [], "owners": []},
                  [tests("把这一类信息给这位对方，落在主人愿意给的范围内、并且会推进这桩合作吗？", "disclose")]) for q in todo]
        rs = await self.judge_many(items) if items else []
        if self.stale("disc", b): return
        for i, q in enumerate(todo):
            before = len([x for x in todo[:i] if x["from"] == q["from"]])
            pe = self.edge.get(pair_key(b, q["from"]))
            tier = "t2" if before > 0 and pe and pe["holds"] else "t1"
            rd = rs[i][0]
            if act(rd): self._put_reply(b, q["from"], q["cat"], {"granted": True, "frags": by_category(w, q["cat"], tier)})
            elif rd.act is False: self._put_reply(b, q["from"], q["cat"], {"denied": True, "cat": q["cat"], "by": b})

    # ───────────── 构型：局部搜索 → 候选只增 → 整体再判
    def _closure(self, m):
        return [x for y in (self.material.get(m) or {"members": []})["members"] for x in self._closure(y)] if is_cfg(m) else [m]

    def _nest(self, m):
        return 1 + max([self._nest(x) for x in (self.material.get(m) or {"members": []})["members"]] or [0]) if is_cfg(m) else 0

    def _invalid(self, c):
        flat = [x for m in c["members"] for x in self._closure(m)]
        return len(set(flat)) < len(flat) or sum(is_cfg(m) for m in c["members"]) > 1 or max(self._nest(m) for m in c["members"]) >= 2

    def _cyclic(self, c):
        return any(any(m != x and m in (self.material.get(x) or {"members": []})["members"] for m in c["members"]) for x in c["members"] if is_cfg(x))

    async def _cfg(self, x):
        es1 = [l for l in self.adj.get(x, {}).values() if l["holds"]]
        nb = uniq([l["peer"] for l in es1])
        es = list({(l["a"], l["b"]): l for l in es1 + [l for y in nb for l in self.adj.get(y, {}).values() if l["holds"]]}.values())
        cands = self.p.graph_local(x, es, LIMITS)
        novel = [c for c in cands if not self._cyclic(c) and not self._invalid(c) and members_key(c["members"]) not in self.cand]
        for c in novel[:6]: self._add_cand(c)

    def _add_cand(self, c):
        ck = members_key(c["members"])
        if ck in self.cand: return
        self.cand[ck] = c
        for m in c["members"]: self.cands_of.setdefault(m, set()).add(ck)
        self.mark("whole", ck)

    def _kill_cand(self, ck):
        c = self.cand.pop(ck, None)
        if c:
            for m in c["members"]: self.cands_of.get(m, set()).discard(ck)
        self.mark("whole", ck)

    def _all_unlocked(self, m, members):
        others = [o for o in members if o != m]
        if not others: return []
        return [f for f in self.unlocked.get((m, others[0]), []) if all(f in self.unlocked.get((m, o), []) for o in others)]

    def _retract_cfg(self, k):
        if self.config.pop(k, None) is None and k not in self.material: return
        for d in (self.material, self.node, self.plan): d.pop(k, None)
        self.p.index_del(k)
        for pk in list(self.pairs_of.get(k, ())): self.mark("pair", pk)
        for ck in list(self.cands_of.get(k, ())): self._kill_cand(ck)       # 以它为成员的构型随之撤销
        self.mark("recall", k); self.mark("plan", k); self.mark("grow", k)

    async def _whole(self, ck):
        c = self.cand.get(ck)
        if c is None or any(m not in self.material for m in c["members"]):
            if c is not None: self._kill_cand(ck)
            return self._retract_cfg(ck)
        ms = c["members"]
        s = {"sides": [self._side(self.material[m], self._all_unlocked(m, ms)) for m in ms], "owners": ms, "shape": c["shape"],
             "roles": c.get("roles"), "ref": VALUE_REF}
        qs = [tests("这几个人按这个形状合作，能成吗？", "hold", LACKS), select("最弱的一环是谁？", ms, "weakest"),
              measure("这个合作整体价值多大？", ["小", "中", "大"], "value")] + \
             [tests(f"去掉 {self.material[m]['display']} 之后，这个合作还成立吗？", "without") for m in ms]
        r = await self.judge(s, qs)
        if self.stale("whole", ck): return
        depth = 1 + max(self._depth(m) for m in ms)
        hd = r[0]
        if hd.act is None and hd.needed:
            cat, holder = hd.needed
            if (holder, ms[0] if holder != ms[0] else ms[1], cat) not in self.reply:
                self._request(holder, ms[0] if holder != ms[0] else ms[1], cat, "判断这个构型是否成立")
        if not act(hd): return self._retract_cfg(ck)
        if depth >= self.cap:
            self.pending[("whole", ck)] = [{"cause": "depth", "needed": f"组合层数到上限 {self.cap}"}]
            return self._retract_cfg(ck)
        cfg = {"id": ck, "kind": "config", "shape": c["shape"], "members": ms, "roles": c.get("roles", {}), "hold": hd, "weakest": r[1],
               "value": r[2], "lacks": [hd.needed[0]] if hd.needed else [], "depth": depth,
               "offers": [{"text": f"（{c['shape']}）" + "、".join(self.material[m]["display"] for m in ms) + " 合起来能提供", "tier": "t0", "src": "config"}],
               "signals": [{"text": "这个组合还缺：" + "、".join([hd.needed[0]] if hd.needed else ["（未知）"]), "tier": "t0", "src": "config"}],
               "derived": [], "display": f"（{c['shape']}）" + "、".join(self.material[m]["display"] for m in ms), "catchers": [], "projects": [], "forbids": []}
        old = self.config.get(ck)
        self.config[ck] = cfg
        if self.node.get(ck) != cfg: self.node[ck] = cfg; self.p.index_put(ck, cfg)
        if self.material.get(ck) != cfg: self.material[ck] = cfg; self._material_changed(ck)
        if old != cfg: self.mark("plan", ck); self.mark("grow", ck)

    def _depth(self, m): return (self.config.get(m) or {}).get("depth", 0) if is_cfg(m) else 0

    # ───────────── 成长、方案：config 定下后实例化
    async def _grow(self, k):
        C = self.config.get(k)
        if C is None: return
        xs = [] if len(C["members"]) >= 5 else (await self.p.route_offers(C, 3))[:3]
        xs = [x for x in xs if x["id"] in self.material]
        for x in xs: self.grow_deps.setdefault(x["id"], set()).add(k)       # 读了候选的材料 → 登记依赖
        sc = {"kind": "config", "shape": C["shape"], "members": [self.material[m]["display"] for m in C["members"]], "roles": C["roles"], "offers": texts(C["offers"]), "lacks": C["lacks"]}
        items = [({"group": sc, "candidate": self._side(self.material[x["id"]], []), "ref": VALUE_REF, "sides": [], "owners": []},
                  [tests("他加入会让这个合作更紧密吗？", "tighter"), select("加入后合作的金额会怎样？", ["up", "same", "down"], "amount"),
                   select("主要在哪个地方变？", C["members"] + [x["id"]], "where"), measure("他加入后合作成的可能性有多高？", ["低", "中", "高"], "p")]) for x in xs]
        rs = await self.judge_many(items) if items else []
        if self.stale("grow", k) or k not in self.config: return
        ok = [(x, r[0]) for x, r in zip(xs, rs) if act(r[0])]
        if ok:
            x, _ = max(ok, key=lambda t: t[1].p)
            c2 = {"shape": C["shape"], "members": C["members"] + [x["id"]], "roles": {**C["roles"], x["id"]: "加入者"}, "grown_from": k}
            if not self._invalid(c2): self._add_cand(c2)

    async def _plan(self, k):
        C = self.config.get(k)
        if C is None: self.plan.pop(k, None); return
        if len(C["members"]) <= 5 and C["hold"].p >= 0.6:
            mats = [self._side(self.material[m], self._all_unlocked(m, C["members"])) for m in C["members"] if m in self.material]
            pl = await self.p.gen_plan(C, mats)
            if not self.stale("plan", k) and k in self.config: self.plan[k] = pl
        else: self.plan.pop(k, None)
