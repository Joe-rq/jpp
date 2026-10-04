"""预注册 13：同一条事件轨迹、同一个规则判断器、按最终输入算出的真值。两边（J++x、基线）共用本文件。

判断器是确定性规则：读数只依赖状态内容（公开层加这一对已解锁的片段），所以披露、材料更新会改变读数。
是非题读数一律避开 0.4–0.6，补信息默认链在两边都不触发；披露只经宿主事件（真实接入者给出 / 拒绝）。
"""
from __future__ import annotations

import hashlib
import json
import random
import re
from dataclasses import dataclass, field

TOK = re.compile(r"(need|offer):([A-Za-z0-9_]+)")
CAT = "近况细节"


def hx(*xs) -> int:
    return int(hashlib.sha1(json.dumps(xs, sort_keys=True, ensure_ascii=False, default=str).encode()).hexdigest()[:12], 16)


def hi(*key) -> float:
    """成立一侧的读数：0.660–0.949，按内容哈希取，各对不同。"""
    return round(0.66 + (hx(*key) % 290) / 1000, 3)


def lo(*key) -> float:
    """不成立一侧的读数：0.100–0.299。"""
    return round(0.10 + (hx(*key) % 200) / 1000, 3)


# ───────────────────────────── 世界
def frag(kind, tok, tier="t0", src=None):
    return {"text": f"{kind}:{tok}", "tier": tier, "src": src or ("note" if kind == "need" else "skill")}


def world(i, needs=(), offers=(), t1_needs=(), real=False):
    return {"id": i, "display": i, "lang": "zh",
            "signals": [frag("need", t) for t in needs] + [frag("need", t, "t1") for t in t1_needs],
            "offers": [frag("offer", t) for t in offers], "catchers": [], "forbids": [], "projects": [],
            "policy": "有合作苗头时愿意说具体情况", "real": real}


def base_worlds() -> dict:
    W = {}
    for k in range(1, 7):                       # 6 对互补
        W[f"m{k}a"] = world(f"m{k}a", [f"M{k}x"], [f"M{k}y"])
        W[f"m{k}b"] = world(f"m{k}b", [f"M{k}y"], [f"M{k}x"])
    for k in range(1, 4):                       # 3 条三人单向链 a → b → c
        W[f"t{k}a"] = world(f"t{k}a", [f"Z{k}a"], [f"T{k}p"])
        W[f"t{k}b"] = world(f"t{k}b", [f"T{k}p"], [f"T{k}q"])
        W[f"t{k}c"] = world(f"t{k}c", [f"T{k}q"], [f"Z{k}c"])
    for k in (1, 2):                            # 真实接入者：只在 t1 才对得上伙伴
        W[f"r{k}"] = world(f"r{k}", [f"U{k}"], [f"V{k}"], t1_needs=[f"H{k}"], real=True)
        W[f"h{k}"] = world(f"h{k}", [f"W{k}"], [f"H{k}"])
    W["u1"] = world("u1", ["UU0"], ["UU9"], t1_needs=["UU1"])   # 靠材料更新（t1 提为 t0）才对得上
    W["u1p"] = world("u1p", ["UUp"], ["UU1"])
    for k in (1, 2, 3):
        W[f"l{k}"] = world(f"l{k}", [f"L{k}n"], [f"L{k}o"])
    return W


def promote(w, tok):
    w = json.loads(json.dumps(w))
    for f in w["signals"]:
        if f["text"] == f"need:{tok}":
            f["tier"] = "t0"
    return w


def add_offer(w, tok):
    w = json.loads(json.dumps(w))
    w["offers"].append(frag("offer", tok))
    return w


def drop_offer(w, tok):
    w = json.loads(json.dumps(w))
    w["offers"] = [f for f in w["offers"] if f["text"] != f"offer:{tok}"]
    return w


def grant_frags(w):
    return [{"text": f["text"], "tier": "t1", "cat": CAT, "src": "reply"} for f in w["signals"] if f["tier"] == "t1"]


# ───────────────────────────── 轨迹
@dataclass
class Ev:
    kind: str            # join | update | grant | deny | leave
    who: str
    world: dict | None = None
    to: str | None = None
    frags: list = field(default_factory=list)
    gap: int = 0         # 事件之后让出事件循环的次数（不等静止）

    def label(self):
        return f"{self.kind}:{self.who}" + (f"→{self.to}" if self.to else "")


def trajectory(seed: int, max_gap: int = 40) -> list[Ev]:
    """约 60 个事件。接入顺序与穿插位置由种子决定；每件事都在它的前提之后。"""
    rng = random.Random(seed)
    W = base_worlds()
    cur = dict(W)
    ev: list[Ev] = []

    def up(who, w):
        cur[who] = w
        ev.append(Ev("update", who, w))

    # 第一段：全部接入，中间插给出、拒绝、提层
    order = list(W)
    rng.shuffle(order)
    side = {"grant:r1": ("r1",), "deny:r2": ("r2",), "promote:u1": ("u1",)}
    pos = {k: rng.randint(order.index(v[0]) + 1, len(order)) for k, v in side.items()}
    for i, who in enumerate(order + [None]):
        for k, p in sorted(pos.items()):
            if p == i:
                if k == "grant:r1":
                    ev.append(Ev("grant", "r1", to="h1", frags=grant_frags(cur["r1"])))
                elif k == "deny:r2":
                    ev.append(Ev("deny", "r2", to="h2"))
                else:
                    up("u1", promote(cur["u1"], "UU1"))
        if who is not None:
            ev.append(Ev("join", who, cur[who]))

    # 第二段：材料更新、给出 / 拒绝、离开（顺序打乱）
    p2 = [lambda: up("m1a", add_offer(cur["m1a"], "M2x")),          # m1a 多出一个构型
          lambda: up("m1a", add_offer(cur["m1a"], "M3x")),          # 再多一个：m1a 的前 2 要换，星形出现
          lambda: up("m5a", drop_offer(cur["m5a"], "M5y")),         # m5 先变单向
          lambda: up("l1", add_offer(cur["l1"], "L2n")),            # 孤立者之间对上
          lambda: ev.append(Ev("leave", "m6a")),
          lambda: ev.append(Ev("leave", "t2b")),
          lambda: ev.append(Ev("leave", "r1")),
          lambda: ev.append(Ev("grant", "r2", to="h2", frags=grant_frags(cur["r2"]))),
          lambda: ev.append(Ev("deny", "r2", to="l1"))]
    rng.shuffle(p2)
    for f in p2:
        f()
    up("m5b", drop_offer(cur["m5b"], "M5x"))                       # m5 两个方向都断：两人构型解体
    # 第三段：再接入（一人换了材料）、再给出、再更新、再离开
    p3 = [lambda: ev.append(Ev("join", "m6a", cur["m6a"])),
          lambda: (cur.__setitem__("t2b", world("t2b", ["T2x"], ["T2q"])), ev.append(Ev("join", "t2b", cur["t2b"]))),
          lambda: ev.append(Ev("join", "r1", cur["r1"])),
          lambda: ev.append(Ev("leave", "h2")),
          lambda: ev.append(Ev("leave", "t3c")),
          lambda: up("l2", add_offer(cur["l2"], "L3n"))]
    rng.shuffle(p3)
    for f in p3:
        f()
    ev.append(Ev("grant", "r1", to="h1", frags=grant_frags(cur["r1"])))
    up("m1a", drop_offer(cur["m1a"], "M3x"))                       # 星形与第三个构型退掉，前 2 换回
    ev.append(Ev("join", "t3c", cur["t3c"]))
    for e in ev:
        e.gap = rng.randint(0, max_gap)
    return ev


# ───────────────────────────── 宿主一侧的真值（随事件更新）
class Truth:
    def __init__(self):
        self.world: dict = {}             # 在场者 → 当前 world
        self.unlocked: dict = {}          # (给出者, 对方) → 片段（宿主写入，只增，离开时删）
        self.left: set = set()
        self.ever: set = set()

    def apply(self, e: Ev):
        if e.kind in ("join", "update"):
            self.world[e.who] = e.world
            self.ever.add(e.who)
            self.left.discard(e.who)
        elif e.kind == "grant":
            cur = self.unlocked.setdefault((e.who, e.to), [])
            for f in e.frags:
                if f not in cur:
                    cur.append(f)
        elif e.kind == "leave":
            self.world.pop(e.who, None)
            self.left.add(e.who)
            for k in [k for k in self.unlocked if e.who in k]:
                self.unlocked.pop(k)

    def present(self, x) -> bool:
        return x in self.world

    def t0(self, x):
        w = self.world[x]
        return ({m for f in w["signals"] if f["tier"] == "t0" for m in toks(f["text"], "need")},
                {m for f in w["offers"] if f["tier"] == "t0" for m in toks(f["text"], "offer")})

    def side(self, x, unlocked_texts):
        n, o = self.t0(x)
        for t in unlocked_texts:
            n |= toks(t, "need")
            o |= toks(t, "offer")
        return Side(x, "agent", frozenset(n), frozenset(o))

    def pair_sides(self, a, b):
        ua = [f["text"] for f in self.unlocked.get((a, b), [])]
        ub = [f["text"] for f in self.unlocked.get((b, a), [])]
        return self.side(a, ua), self.side(b, ub)

    def all_unlocked(self, m, members):
        others = [o for o in members if o != m]
        if not others:
            return []
        first = self.unlocked.get((m, others[0]), [])
        return [f for f in first if all(f in self.unlocked.get((m, o), []) for o in others)]


def toks(text, kind):
    return {m.group(2) for m in TOK.finditer(str(text)) if m.group(1) == kind}


# ───────────────────────────── 规则判断器（作用在两边共同的「视图」上）
@dataclass(frozen=True)
class Side:
    id: str
    kind: str
    needs: frozenset
    offers: frozenset


@dataclass
class View:
    kind: str                    # pair | whole | rerank | disclose | grow
    sides: list
    policy: str = ""


def side_of(d: dict, extra_texts=()) -> Side:
    texts = list(d.get("signals") or []) + list(d.get("offers") or [])
    unl = [f.get("text", "") if isinstance(f, dict) else str(f) for f in d.get("已解锁") or []]
    n, o = set(), set()
    for t in [str(x) for x in texts] + unl + list(extra_texts):
        n |= toks(t, "need")
        o |= toks(t, "offer")
    ms = d.get("members") or [d.get("display")]
    kind = d.get("kind", "agent")
    return Side(str(ms[0]) if kind == "agent" else "+".join(map(str, ms)), kind, frozenset(n), frozenset(o))


def view_of(state: dict) -> View:
    on = state.get("on", state)
    if "我" in on:
        me = on["我"]
        return View("rerank", [Side(str(me.get("展示")), "agent",
                                    frozenset(set().union(*[toks(t, "need") for t in me.get("在找") or []] or [set()])),
                                    frozenset(set().union(*[toks(t, "offer") for t in me.get("能提供") or []] or [set()])))])
    if "request" in on:
        return View("disclose", [], policy=json.dumps(state.get("ctx", state.get("policy")), ensure_ascii=False))
    if "group" in on:
        return View("grow", [])
    if "a" in on and "b" in on:
        return View("pair", [side_of(on["a"]), side_of(on["b"])])
    if isinstance(on.get("members"), list) and "shape" in on:
        return View("whole", [side_of(m) for m in on["members"]])
    sides = state.get("sides") or []
    if len(sides) == 2 and "shape" not in state:
        return View("pair", [side_of(s) for s in sides])
    if "shape" in state:
        return View("whole", [side_of(s) for s in sides])
    raise ValueError(f"认不出的状态：{list(on)[:6]}")


def helps(giver: Side, taker: Side) -> frozenset:
    if giver.kind != "agent" or taker.kind != "agent":
        return frozenset()
    return giver.offers & taker.needs


def whole_holds(sides) -> bool:
    if any(s.kind != "agent" for s in sides) or len(sides) < 2:
        return False
    adj = {s.id: set() for s in sides}
    for a in sides:
        for b in sides:
            if a.id != b.id and (helps(a, b) or helps(b, a)):
                adj[a.id].add(b.id)
    seen, todo = set(), [sides[0].id]
    while todo:
        x = todo.pop()
        if x not in seen:
            seen.add(x)
            todo.extend(adj[x] - seen)
    return len(seen) == len(sides)


def answer(v: View, text: str, op: str, labels: list):
    """返回规则答案：是非题 → p；选择题 → 标签；打分题 → 档位下标。"""
    if "材料里最缺哪一类信息" in text:                     # J++x 每批带的伴随题：读数都离 0.5 远，不会被用到
        return "none" if "none" in labels else labels[-1]
    if v.kind == "rerank":
        me = v.sides[0]
        seg = lambda name: text.split(name, 1)[1].split("。", 1)[0] if name in text else ""   # noqa: E731
        their_n, their_o = toks(seg("对方在找："), "need"), toks(seg("对方能提供："), "offer")
        return 0.9 if (me.needs & their_o or me.offers & their_n) else 0.2
    if v.kind == "disclose":
        return 0.1 if "不给" in v.policy else 0.9
    if v.kind == "grow":
        if op == "test":
            return 0.1
        if op == "select":
            return "same" if "same" in labels else labels[0]
        return 0
    if v.kind == "pair":
        A, B = v.sides
        ab, ba = helps(B, A), helps(A, B)
        if "读 B 的世界" in text:
            return hi(A.id, B.id, "ab", sorted(ab)) if ab else lo(A.id, B.id, "ab")
        if "读 A 的世界" in text:
            return hi(A.id, B.id, "ba", sorted(ba)) if ba else lo(A.id, B.id, "ba")
        if "谁主要帮谁" in text:
            return "both" if ab and ba else "ab" if ab else "ba" if ba else "none"
        if "合作形式" in text:
            return "direct" if ab or ba else "none"
        if op == "measure":
            return 1
        if "时间、阶段与条件" in text:
            return 0.7
        if op == "test":
            return 0.1                                    # 三道负例题、确认题
        raise ValueError(f"两两里认不出的题：{text[:30]}")
    if v.kind == "whole":
        if "能成吗" in text:
            ids = sorted(s.id for s in v.sides)
            return hi(ids, sorted(sorted(s.needs | s.offers) for s in v.sides)) if whole_holds(v.sides) else lo(ids)
        if "最弱的一环" in text:
            return labels[0]
        if op == "measure":
            return 1
        if op == "test":
            return 0.1                                    # 「去掉某人还成立吗」
    raise ValueError(f"认不出的题：{v.kind} {text[:30]}")


# ───────────────────────────── 真值（按当前输入，用同一条规则）
def expected_edge(T: Truth, a: str, b: str) -> dict:
    A, B = T.pair_sides(a, b)
    ab, ba = helps(B, A), helps(A, B)
    pab = hi(A.id, B.id, "ab", sorted(ab)) if ab else lo(A.id, B.id, "ab")
    pba = hi(A.id, B.id, "ba", sorted(ba)) if ba else lo(A.id, B.id, "ba")
    ex = [("ab", pab > 0.5, pab), ("ba", pba > 0.5, pba)]
    acts = [t for t in ex if t[1]]
    conf = lambda t: t[2] if t[1] else 1 - t[2]           # noqa: E731
    best = max(acts or ex, key=conf)
    return {"holds": bool(ab or ba), "dir": "both" if ab and ba else "ab" if ab else "ba" if ba else "none",
            "form": "direct" if ab or ba else "none", "p": round(conf(best), 3), "exit": "act" if best[1] else "ignore"}


def whole_sides(T: Truth, members):
    return [T.side(m, [f["text"] for f in T.all_unlocked(m, members)]) for m in members]


def expected_whole(T: Truth, members) -> tuple[bool, float]:
    sides = whole_sides(T, members)
    ids = sorted(s.id for s in sides)
    if whole_holds(sides):
        return True, hi(ids, sorted(sorted(s.needs | s.offers) for s in sides))
    return False, lo(ids)
