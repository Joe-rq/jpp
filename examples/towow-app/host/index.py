"""片段索引：`do` 动作 index_put / route / route_offers 的实现（Fable-B §2）。

宿主提供的是精确算法：编码（bge-m3，经 jx/ports/enc.py 的 EncPort，带缓存）+ HNSW 近邻。
这里没有调度、没有判断、没有排队：每个函数对给定参数同步算完就返回。
谁在什么时候调用、调用结果怎样失效重算，由 J++x 程序与引擎决定（net.jpx 的 发布/召回/成长）。

四个子索引，只装 t0 片段（t1/t2 永不进索引，Fable-B §2 第一段）：
    signals   —— 主人最近的状态
    offers    —— 能给出的东西
    catchers  —— 假想来信 hypo（「什么样的来信我接得住」）
    derived   —— 派生片段：进行中的关系、构型的「整体能提供 / 还缺」

route(x, node, k) 的四路（每路每条片段 top-k，再按对方汇总）：
    sig→catcher  我的 signals 查对方 catchers.hypo   （谁接得住我）
    catcher→sig  我的 catchers.hypo 查对方 signals   （我接得住谁）
    offer→sig    我的 offers 查对方 signals          （经典互补，一向）
    sig→offer    我的 signals 查对方 offers          （经典互补，另一向）
    →derived     我的 signals 与 catchers.hypo 查 derived（进行中的关系与构型）

汇总分 score = 对方所有命中里最高的相似度；同分时按 breadth（各路最好命中之和）排。
这是代码聚合（不是判断、不是读数），真正的判断在 两两 程序里由判断器做。
不用「各路之和」作主键：稠密编码器对无关文本也有 0.4–0.5 的底噪，求和会让「每路都弱命中」的对方
压过「一路强命中」的对方（test_index_bge 用真 bge-m3 实测过：烘焙课 0.45+0.45 压过跨语言的 0.62）。
"""
from __future__ import annotations

import threading
from dataclasses import dataclass, field
from typing import Any, Iterable, Protocol

import numpy as np

SUBS = ("signals", "offers", "catchers", "derived")

# 路由表：(路名, 我的哪类片段, 查哪个子索引)
ROUTES = (
    ("sig→catcher", "signals", "catchers"),
    ("catcher→sig", "catchers", "signals"),
    ("offer→sig", "offers", "signals"),
    ("sig→offer", "signals", "offers"),
    ("→derived", "signals", "derived"),
    ("→derived", "catchers", "derived"),
)


class Encoder(Protocol):
    dim: int

    def encode(self, texts: list[str]) -> np.ndarray: ...


MAX_FRAG_CHARS = 600      # 编码前截断：片段本是一句话；构型摘要若异常膨胀，编码代价也有界（bge-m3 上限 8192 token）


def frag_text(f: Any, kind: str) -> str:
    """片段 → 进索引的文本（截到 MAX_FRAG_CHARS）。catcher 只索引假想来信 hypo（确认题与 can 留给判断用）。"""
    return _frag_text(f, kind)[:MAX_FRAG_CHARS]


def _frag_text(f: Any, kind: str) -> str:
    if f is None:
        return ""
    if isinstance(f, str):
        return f.strip()
    if isinstance(f, dict):
        if kind == "catchers":
            return str(f.get("hypo") or f.get("text") or "").strip()
        for key in ("text", "summary", "hypo", "name"):
            v = f.get(key)
            if v:
                return str(v).strip()
        return ""
    return str(f).strip()


def frag_tier(f: Any) -> str:
    if isinstance(f, dict):
        return str(f.get("tier") or "t0")
    return "t0"


def node_frags(node: dict | None, kind: str) -> list[str]:
    """node 里某类 t0 片段的文本（去空、去重、保序）。不是 t0 的片段一律不进索引。"""
    if not node:
        return []
    raw = node.get(kind) or []
    if isinstance(raw, (str, dict)):
        raw = [raw]
    out, seen = [], set()
    for f in raw:
        if frag_tier(f) != "t0":
            continue
        t = frag_text(f, kind)
        if t and t not in seen:
            seen.add(t)
            out.append(t)
    return out


def node_members(node: dict | None, owner: str) -> tuple[str, ...]:
    if not node:
        return (owner,)
    m = node.get("members") or [owner]
    return tuple(str(x) for x in m)


@dataclass
class _Entry:
    owner: str
    sub: str
    text: str


class _Sub:
    """一个 HNSW 子索引（内积空间，向量已归一化，相似度 = 余弦）。支持删除与槽位复用。"""

    def __init__(self, dim: int, cap: int, M: int, ef_construction: int, threads: int):
        import hnswlib

        self.ix = hnswlib.Index(space="ip", dim=dim)
        self.ix.init_index(max_elements=cap, ef_construction=ef_construction, M=M,
                           allow_replace_deleted=True)
        self.ix.set_num_threads(threads)
        self.cap = cap
        self.n_active = 0

    def add(self, vecs: np.ndarray, labels: list[int]):
        if not labels:
            return
        need = self.ix.get_current_count() + len(labels)
        if need > self.cap:
            self.cap = max(self.cap * 2, need + 1024)
            self.ix.resize_index(self.cap)
        self.ix.add_items(vecs, np.asarray(labels, dtype=np.int64), replace_deleted=True)
        self.n_active += len(labels)

    def delete(self, labels: Iterable[int]):
        for lab in labels:
            self.ix.mark_deleted(int(lab))
            self.n_active -= 1

    def query(self, vecs: np.ndarray, k: int):
        """有删除标记时 HNSW 可能凑不满 k 个（hnswlib 报 contiguous 2D array 错）：
        先把 ef 提到 2k，仍不够就把 k 减半重试，直到 1。"""
        k = min(k, self.n_active)
        if k <= 0 or len(vecs) == 0:
            return None, None
        ef = max(64, 2 * k)
        while k >= 1:
            self.ix.set_ef(ef)
            try:
                return self.ix.knn_query(vecs, k=k)
            except RuntimeError:
                ef = max(ef, 4 * k)
                k //= 2
        return None, None


class FragmentIndex:
    """四个子索引 + owner→条目 的簿记。线程安全（一把锁），每次调用同步完成。"""

    def __init__(self, encoder: Encoder, *, cap: int = 4096, M: int = 16, ef_construction: int = 100,
                 threads: int = 2, layout_seed: int = 7):
        self.enc = encoder
        self.dim = int(encoder.dim)
        self.subs = {s: _Sub(self.dim, cap, M, ef_construction, threads) for s in SUBS}
        self.entries: dict[int, _Entry] = {}
        self.by_owner: dict[str, dict[str, list[int]]] = {}
        self.members: dict[str, tuple[str, ...]] = {}
        self.nodes: dict[str, dict] = {}          # index_put 收到的 node（graph_local 的 team 兜底用）
        self.mean: dict[str, np.ndarray] = {}
        self.next_label = 0
        self.lock = threading.RLock()
        rng = np.random.default_rng(layout_seed)
        self._proj = rng.standard_normal((self.dim, 3)).astype(np.float32) / np.sqrt(3.0)
        self.n_put = 0
        self.n_route = 0

    # ------------------------------------------------------------ 增删

    def index_put(self, owner: str, node: dict | None) -> dict:
        """建 / 更新 owner 的条目：旧条目全部删掉，按 node 里的 t0 片段重建。
        node 为空或 None 等于删除（agent 离开）；派生片段撤销 = 带新 derived 重新 put。"""
        owner = str(owner)
        with self.lock:
            self._drop(owner)
            if not node or node.get("left") or node.get("gone"):
                return {"owner": owner, "n": 0, "removed": True}
            texts, subs = [], []
            for s in SUBS:
                for t in node_frags(node, s):
                    texts.append(t)
                    subs.append(s)
            self.members[owner] = node_members(node, owner)
            self.nodes[owner] = node
            if not texts:
                self.by_owner[owner] = {s: [] for s in SUBS}
                return {"owner": owner, "n": 0}
            vecs = self.enc.encode(texts)
            labs: dict[str, list[int]] = {s: [] for s in SUBS}
            for s in SUBS:
                idx = [i for i, ss in enumerate(subs) if ss == s]
                if not idx:
                    continue
                ls = []
                for i in idx:
                    lab = self.next_label
                    self.next_label += 1
                    self.entries[lab] = _Entry(owner, s, texts[i])
                    ls.append(lab)
                self.subs[s].add(vecs[idx], ls)
                labs[s] = ls
            self.by_owner[owner] = labs
            m = vecs.mean(axis=0)
            self.mean[owner] = m / (np.linalg.norm(m) or 1.0)
            self.n_put += 1
            return {"owner": owner, "n": len(texts), "by_sub": {s: len(v) for s, v in labs.items()}}

    def remove(self, owner: str) -> dict:
        return self.index_put(owner, None)

    def _drop(self, owner: str):
        old = self.by_owner.pop(owner, None)
        self.members.pop(owner, None)
        self.nodes.pop(owner, None)
        self.mean.pop(owner, None)
        if not old:
            return
        for s, labs in old.items():
            self.subs[s].delete(labs)
            for lab in labs:
                self.entries.pop(lab, None)

    # ------------------------------------------------------------ 查询

    def _excluded(self, x: str, peer: str, extra: set[str]) -> bool:
        if peer == x or peer in extra:
            return True
        if x in self.members.get(peer, ()):             # 含我在内的构型不是我的对方
            return True
        return peer in self.members.get(x, ()) and peer != x   # 构型查询时，自己的成员不是对方

    def _search(self, x: str, queries: list[tuple[str, str, str]], k: int, exclude: set[str]):
        """queries: [(路名, 我的文本, 目标子索引)] → {peer: {(route, mine): (sim, theirs)}}。
        每个目标子索引一次批量 knn_query；为排除自己与自己所在构型多取一些。"""
        hits: dict[str, dict[tuple[str, str], tuple[float, str]]] = {}
        if not queries:
            return hits
        uniq = list(dict.fromkeys(q[1] for q in queries))
        vecs = self.enc.encode(uniq)
        vrow = {t: i for i, t in enumerate(uniq)}
        by_target: dict[str, list[tuple[str, str]]] = {}
        for route, mine, target in queries:
            by_target.setdefault(target, []).append((route, mine))
        for target, qs in by_target.items():
            sub = self.subs[target]
            own = len(self.by_owner.get(x, {}).get(target, []))
            k_eff = k + own + max(8, k // 2)
            rows = [vrow[m] for _, m in qs]
            labels, dists = sub.query(vecs[rows], k_eff)
            if labels is None:
                continue
            for qi, (route, mine) in enumerate(qs):
                got = 0
                for lab, dist in zip(labels[qi], dists[qi]):
                    e = self.entries.get(int(lab))
                    if e is None or self._excluded(x, e.owner, exclude):
                        continue
                    sim = float(1.0 - dist)
                    key = (route, mine)
                    cur = hits.setdefault(e.owner, {}).get(key)
                    if cur is None or sim > cur[0]:
                        hits[e.owner][key] = (sim, e.text)
                    got += 1
                    if got >= k:
                        break
        return hits

    @staticmethod
    def _aggregate(hits, id_key: str = "peer") -> list[dict]:
        out = []
        for peer, d in hits.items():
            routes = sorted(({"route": r, "mine": m, "theirs": t, "sim": round(s, 4)}
                             for (r, m), (s, t) in d.items()), key=lambda z: -z["sim"])
            best_per_route: dict[str, float] = {}
            for z in routes:
                best_per_route[z["route"]] = max(best_per_route.get(z["route"], -1.0), z["sim"])
            score = round(max(best_per_route.values()), 4)
            breadth = round(sum(best_per_route.values()), 4)
            item = {"peer": peer, "score": score, "breadth": breadth, "routes": routes}
            if id_key != "peer":
                item[id_key] = peer
            out.append(item)
        out.sort(key=lambda z: (-z["score"], -z["breadth"], z["peer"]))
        return out

    def route(self, x: str, node: dict | None, k: int = 20) -> list[dict]:
        """Fable-B §2 四路查询。返回 [{peer, score, breadth, routes:[{route, mine, theirs, sim}]}]，按分数降序，排除自己。"""
        x = str(x)
        k = int(k)
        with self.lock:
            self.n_route += 1
            qs = []
            for route, mine_kind, target in ROUTES:
                for t in node_frags(node, mine_kind):
                    qs.append((route, t, target))
            own = {str(m) for m in (node or {}).get("members") or []} - {x}   # 构型查询时排除自己的成员
            return self._aggregate(self._search(x, qs, k, own))

    def route_offers(self, config_node: dict | None, k: int = 5) -> list[dict]:
        """构型的「还缺」查 offers 子索引：还缺 = 构型的 signals + lacks 的类别文字。
        返回 [{id, peer, score, routes}]，排除构型本身与它的成员。"""
        if not config_node:
            return []
        cid = str(config_node.get("id", ""))
        members = set(str(m) for m in (config_node.get("members") or []))
        texts = node_frags(config_node, "signals")
        for lk in config_node.get("lacks") or []:
            t = lk.get("cat") if isinstance(lk, dict) else lk
            if t:
                texts.append(str(t))
        with self.lock:
            qs = [("lack→offer", t, "offers") for t in dict.fromkeys(texts)]
            out = self._aggregate(self._search(cid, qs, int(k), members), id_key="id")
            return out[: int(k)]

    # ------------------------------------------------------------ 展示用

    def vec3(self, owner: str, radius: float = 100.0) -> list[float] | None:
        """语义布局坐标（events.md node_join.vec3）：均值向量的固定随机投影。纯展示。"""
        m = self.mean.get(str(owner))
        if m is None:
            return None
        p = m @ self._proj
        return [round(float(v) * radius * 3.0, 2) for v in p]

    def stats(self) -> dict:
        with self.lock:
            return {"owners": len(self.by_owner), "vectors": {s: self.subs[s].n_active for s in SUBS},
                    "puts": self.n_put, "routes": self.n_route}


def make_do_actions(index: FragmentIndex) -> dict:
    """给引擎注册的 do 动作表。"""
    return {
        "index_put": lambda owner, node: index.index_put(owner, node),
        "route": lambda x, node, k=20: index.route(x, node, k),
        "route_offers": lambda cfg, k=5: index.route_offers(cfg, k),
    }
