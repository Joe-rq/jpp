"""局部构型候选：`do` 动作 graph_local(x, edges, limits)（Fable-B §5）。

结果取决于参数（x、edges、limits，含 limits["nodes"]）；只有 team 在程序没传 nodes 时从索引兜底读 node，其余是纯函数；
不读引擎、不读索引、不判断。图算法只用 act 边定图，**读数不进权重**（Fable-B §5 替代 b：
读数跨题不可比），所以剪枝与排序都按确定的代码键（形状、人数、成员 id），不按读数。

── 边的方向（唯一一处映射，测试 test_graph.py::test_direction_mapping 钉住）──
边记录 e 的 a、b 是记录自身的两端（net.jpx 里 a < b），dir 相对它们而言：
    "ab"  = 「B 帮 A」 → 弧 b→a
    "ba"  = 「A 帮 B」 → 弧 a→b
    "both"= 「互相帮」 → 两条弧，且这一对算互惠
    "none"/未决/缺省 → 无弧（只有无向关系，pair/relay 仍可出）
另接受 events.md 的写法 "a>b"（a 给 b，弧 a→b）、"b>a"。dir 可以是 str、dict（取 value/kind）
或 jx.core.Exit（取 .value；非 pick 出口视为无方向）。弧 u→v 读作「u 供给 v」。

── limits 的含义 ──
    chain: 有向路径的最多**边数**（默认 3，即 3–4 人；最少 2 条边）
    ring:  有向环的最长**环长**（默认 4；最短 3），且任两名成员之间没有互惠 act 边
    star:  外围最多**人数**（默认 6；中心至少 3 条同向边）
    team:  团队最多**总人数**（默认 5，含发起人；至少 2 名成员补角色）
    m2m:   biclique 每侧最多**人数**（默认 4；每侧至少 2）
    deg:   每个点展开的出/入度上限（默认 8，按对方 id 取前若干）
    max:   返回的候选总数上限（默认 60）
    nodes: {id: node}，team 用：发起人的 projects[].roles_needed 与成员的 offers；
           没给的 id 由宿主注入的 node_lookup 从索引（index_put 收到的 node）兜底查。
           兜底读的依赖引擎看不见：成员 offers 变了，要等 x 自己的构型程序重跑才会反映。
    team_min_sim: 角色名与 offers 的最低相似度（默认 0.15，字符二元组重合系数）

── 局部 ──
只用 x 两跳内的点。传进来的边只有 x 的一跳时（net.jpx 当前的 peek_family(edge, x)），
能出的只有：x 居中的三人链、以 x 为中心的星、x 发起的团队、pair、relay；
四人链、环、m2m、以邻居为中心的星需要邻居之间的边（两跳）。

返回 [{shape, members(排序去重), roles{member: role}, key, ...}]，同 (shape, members) 只出一次；
chain/ring 另带 order（成员顺序），star 带 center，m2m 带 sides，team 带 project。
"""
from __future__ import annotations

import itertools
import re
from typing import Any, Callable

import networkx as nx

SHAPE_ORDER = ("m2m", "team", "ring", "chain", "star", "relay", "pair")
DEFAULTS = {"chain": 3, "ring": 4, "star": 6, "team": 5, "m2m": 4, "deg": 8, "max": 60,
            "team_min_sim": 0.15}


# ---------------------------------------------------------------- 值的规范化

def _plain(v: Any) -> Any:
    """Exit / dict / str → 可比较的标签。"""
    if v is None:
        return None
    if isinstance(v, (str, bool, int, float)):
        return v
    if isinstance(v, dict):
        if "value" in v and v.get("value") is not None:
            return v["value"]
        return v.get("kind") or v.get("exit")
    kind = getattr(v, "kind", None)
    if kind is not None:
        val = getattr(v, "value", None)
        return val if val is not None else kind
    return str(v)


def _truthy(v: Any) -> bool:
    if v is None:
        return False
    if isinstance(v, bool):
        return v
    if isinstance(v, dict):
        if "act" in v:
            return bool(v["act"])
        return (v.get("kind") or v.get("exit")) == "act"
    kind = getattr(v, "kind", None)
    if kind is not None:
        return kind == "act"
    act = getattr(v, "act", None)
    if act is not None:
        return bool(act)
    return bool(v)


def _get(e: Any, key: str, default=None):
    if isinstance(e, dict):
        return e.get(key, default)
    return getattr(e, key, default)


def arcs_of(e: Any) -> tuple[list[tuple[str, str]], bool]:
    """一条边记录 → (弧列表 [(供给方, 接受方)], 是否互惠)。见模块说明的方向表。"""
    a, b = str(_get(e, "a")), str(_get(e, "b"))
    d = _plain(_get(e, "dir"))
    d = str(d).strip().lower() if d is not None else "none"
    if d in ("ab", "b>a", "b->a"):
        return [(b, a)], False
    if d in ("ba", "a>b", "a->b"):
        return [(a, b)], False
    if d == "both":
        return [(a, b), (b, a)], True
    return [], False


def form_of(e: Any) -> str:
    f = _plain(_get(e, "form"))
    return str(f).strip().lower() if f is not None else ""


def holds_of(e: Any) -> bool:
    h = _get(e, "holds", None)
    return True if h is None else _truthy(h)


# ---------------------------------------------------------------- 角色相似度（team 用）

def _grams(s: str) -> set[str]:
    s = s.lower()
    words = set(re.findall(r"[a-z0-9]+", s))
    han = re.sub(r"[^一-鿿]", "", s)
    bi = {han[i:i + 2] for i in range(len(han) - 1)} | set(han)
    return words | bi


def bigram_sim(a: str, b: str) -> float:
    """重合系数 |A∩B| / min(|A|,|B|)，A、B 为汉字单字与二元组 + 英文词。确定、无模型。"""
    A, B = _grams(a), _grams(b)
    if not A or not B:
        return 0.0
    return len(A & B) / min(len(A), len(B))


def _texts(xs) -> list[str]:
    out = []
    for f in xs or []:
        if isinstance(f, str):
            out.append(f)
        elif isinstance(f, dict):
            t = f.get("text") or f.get("summary") or f.get("name")
            if t:
                out.append(str(t))
    return out


# ---------------------------------------------------------------- 主函数

def graph_local(x: str, edges: list, limits: dict | None = None, *,
                sim: Callable[[str, str], float] | None = None,
                node_lookup: Callable[[str], dict | None] | None = None) -> list[dict]:
    x = str(x)
    L = {**DEFAULTS, **(limits or {})}
    given = L.get("nodes") or {}

    class _Nodes(dict):                 # 先用参数给的 nodes；没给的从索引兜底（宿主注册时注入 node_lookup）
        def get(self, k, default=None):
            if k in given:
                return given[k]
            v = node_lookup(k) if node_lookup else None
            return v if v is not None else default

    nodes = _Nodes()
    sim = sim or bigram_sim
    deg = int(L["deg"])

    # 1. act 边建图
    U = nx.Graph()             # 无向：有 act 关系
    D = nx.DiGraph()           # 有向：u 供给 v
    mutual: set[frozenset] = set()
    pair_edges: dict[frozenset, Any] = {}
    for e in edges or []:
        if not holds_of(e):
            continue
        a, b = str(_get(e, "a")), str(_get(e, "b"))
        if a == b or a == "None" or b == "None":
            continue
        U.add_edge(a, b)
        pair_edges.setdefault(frozenset((a, b)), e)
        arcs, mut = arcs_of(e)
        if mut:
            mutual.add(frozenset((a, b)))
        for u, v in arcs:
            D.add_edge(u, v)
    for (u, v) in list(D.edges()):
        if D.has_edge(v, u):
            mutual.add(frozenset((u, v)))
    if x not in U:
        return []

    # 2. 两跳局部，度上限按对方 id 确定地截
    def nbrs(n):
        return sorted(U.neighbors(n))[:deg] if n in U else []

    hop1 = nbrs(x)
    local = {x, *hop1}
    for n in hop1:
        local.update(nbrs(n))
    Dl = D.subgraph(local).copy()
    one_way = nx.DiGraph()
    one_way.add_nodes_from(local)
    one_way.add_edges_from((u, v) for u, v in Dl.edges() if frozenset((u, v)) not in mutual)

    def outs(n, G=Dl):
        return sorted(G.successors(n))[:deg] if n in G else []

    def ins(n, G=Dl):
        return sorted(G.predecessors(n))[:deg] if n in G else []

    out: dict[tuple, dict] = {}

    def emit(shape, members, roles, **extra):
        ms = sorted(set(members))
        key = (shape, tuple(ms))
        if key in out or x not in ms:
            return
        out[key] = {"shape": shape, "members": ms, "roles": {m: roles.get(m, "") for m in ms},
                    "key": shape + ":" + "|".join(ms), **extra}

    # 3. chain：单向弧上的简单有向路径，2..limit 条边，经过 x
    max_e = int(L["chain"])
    if max_e >= 2:
        def walk(path):
            if len(path) - 1 >= 2 and x in path:
                n = len(path)
                roles = {m: ("起点" if i == 0 else "终点" if i == n - 1 else f"中段{i}") +
                         (f"→{path[i + 1]}" if i < n - 1 else "") for i, m in enumerate(path)}
                emit("chain", path, roles, order=list(path))
            if len(path) - 1 >= max_e:
                return
            for v in outs(path[-1], one_way):
                if v not in path:
                    walk(path + [v])
        for s in sorted(local):
            walk([s])

    # 4. ring：有向环 3..limit，经过 x，任两名成员之间无互惠
    max_r = int(L["ring"])
    if max_r >= 3:
        for cyc in nx.simple_cycles(one_way, length_bound=max_r):
            if len(cyc) < 3 or x not in cyc:
                continue
            if any(frozenset(p) in mutual for p in itertools.combinations(cyc, 2)):
                continue
            i0 = cyc.index(min(cyc))
            cyc = cyc[i0:] + cyc[:i0]
            roles = {m: f"供给→{cyc[(i + 1) % len(cyc)]}" for i, m in enumerate(cyc)}
            emit("ring", cyc, roles, order=list(cyc))

    # 5. star：中心 ≥3 条同向边（中心是 x 或 x 的邻居），外围 ≤limit
    max_p = int(L["star"])
    for c in [x, *hop1]:
        for direction, per in (("out", outs(c)), ("in", ins(c))):
            if len(per) < 3:
                continue
            if c != x:
                if x not in per:
                    continue
                per = [x] + [p for p in per if p != x]
            per = per[:max_p]
            if len(per) < 3:
                continue
            roles = {c: "中心（供给）" if direction == "out" else "中心（接受）"}
            roles.update({p: "外围（接受）" if direction == "out" else "外围（供给）" for p in per})
            emit("star", [c, *per], roles, center=c, direction=direction)

    # 6. m2m：biclique L→R，每侧 2..limit，经过 x
    max_s = int(L["m2m"])
    if max_s >= 2:
        senders = sorted(n for n in local if len(outs(n)) >= 2)
        seen_bc = set()
        for u, v in itertools.combinations(senders, 2):
            R = sorted(set(outs(u)) & set(outs(v)) - {u, v})
            if len(R) < 2:
                continue
            R = R[:max_s]
            Lside = [u, v]
            for w in senders:
                if w in Lside or w in R or len(Lside) >= max_s:
                    continue
                if set(R) <= set(outs(w)):
                    Lside.append(w)
            sig = (frozenset(Lside), frozenset(R))
            if sig in seen_bc or x not in (set(Lside) | set(R)):
                continue
            seen_bc.add(sig)
            roles = {m: "供给侧" for m in Lside}
            roles.update({m: "接受侧" for m in R})
            emit("m2m", Lside + R, roles, sides=[sorted(Lside), sorted(R)])

    # 7. team：发起人的 projects.roles_needed 对 act 邻居的 offers（角色名相似，贪心一人一角）
    max_t = int(L["team"])
    min_sim = float(L["team_min_sim"])
    for i in [x, *hop1]:
        ni = nodes.get(i) or {}
        for proj in ni.get("projects") or []:
            roles_needed = [str(r) for r in (proj.get("roles_needed") or []) if r]
            if not roles_needed:
                continue
            cands = [c for c in nbrs(i) if c != i]
            scored = []
            for ri, role in enumerate(roles_needed):
                for c in cands:
                    offers = _texts((nodes.get(c) or {}).get("offers"))
                    s = max((sim(role, t) for t in offers), default=0.0)
                    if s >= min_sim:
                        scored.append((-s, ri, c))
            scored.sort()
            taken_r, taken_c, roles = set(), set(), {i: "发起人：" + str(proj.get("name") or "项目")}
            for negs, ri, c in scored:
                if ri in taken_r or c in taken_c or 1 + len(taken_c) >= max_t:
                    continue
                taken_r.add(ri)
                taken_c.add(c)
                roles[c] = roles_needed[ri]
            if len(taken_c) >= 2:
                emit("team", [i, *taken_c], roles, project=str(proj.get("name") or ""),
                     missing=[r for k, r in enumerate(roles_needed) if k not in taken_r])

    # 8. relay 与 pair：x 的每条 act 边
    for n in hop1:
        e = pair_edges.get(frozenset((x, n)))
        if e is None:
            continue
        a, b = str(_get(e, "a")), str(_get(e, "b"))
        f = form_of(e)
        if f.startswith("relay"):
            via = a if f == "relay_a" else b if f == "relay_b" else ""
            other = b if via == a else a
            roles = {via: "转介方（经身边的人）", other: "受益方"} if via else {a: "转介", b: "转介"}
            emit("relay", [a, b], roles, via=via)
        else:
            arcs, mut = arcs_of(e)
            if mut:
                roles = {a: "互补", b: "互补"}
            elif arcs:
                roles = {arcs[0][0]: "提供方", arcs[0][1]: "受益方"}
            else:
                roles = {a: "", b: ""}
            emit("pair", [a, b], roles, form=f)

    res = sorted(out.values(), key=lambda c: (SHAPE_ORDER.index(c["shape"]), -len(c["members"]), c["key"]))
    return res[: int(L["max"])]


def make_do_actions(sim: Callable[[str, str], float] | None = None,
                    node_lookup: Callable[[str], dict | None] | None = None) -> dict:
    return {"graph_local": lambda x, edges, limits=None: graph_local(x, edges, limits, sim=sim,
                                                                      node_lookup=node_lookup)}
