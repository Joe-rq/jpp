"""片段索引离线压测（Fable-B §7 离线块 i 的索引部分；0 美元）。

用法：.venv/bin/python -m host.bench_index [--sizes 500,2000,10000] [--frags 24] [--bge] [--out 摘要.json]

向量是带聚类的合成向量（200 个簇心 + 噪声，1024 维归一化），每 agent 24 条 t0 片段
（signals 8、offers 6、catchers 8、derived 2）；同一进程里从 500 逐级加到 10000，每级测：
建索引耗时、常驻内存、单次 knn_query 时延、一个 node 完整 route() 的 p50/p95、对暴力检索的 recall@20。
--bge 另测真 bge-m3 编码一个 node（24 条新文本，未命中缓存）的时延。
真值命中率@32（P5）需要真实世界数据，这里不测。
"""
from __future__ import annotations

import argparse
import json
import os
import resource
import sys
import time

import numpy as np

os.environ.setdefault("OMP_NUM_THREADS", "2")
sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))

from host.index import SUBS, FragmentIndex  # noqa: E402

DIM = 1024
SPLIT = {"signals": 8, "offers": 6, "catchers": 8, "derived": 2}


class TableEnc:
    """文本 'v<序号>' → 预生成向量。"""
    dim = DIM

    def __init__(self):
        self.vecs: dict[str, np.ndarray] = {}

    def encode(self, texts):
        return np.stack([self.vecs[t] for t in texts])


def rss_mb() -> float:
    r = resource.getrusage(resource.RUSAGE_SELF).ru_maxrss
    return r / 1e6 if sys.platform == "darwin" else r / 1e3


def make_agents(rng, enc: TableEnc, start: int, n: int, centers: np.ndarray, frags: int):
    scale = frags / sum(SPLIT.values())
    out = []
    for i in range(start, start + n):
        node = {"id": f"a{i}", "kind": "agent", "members": [f"a{i}"]}
        topics = rng.choice(len(centers), size=3, replace=False)
        for s, c in SPLIT.items():
            items = []
            for j in range(max(1, round(c * scale))):
                v = centers[rng.choice(topics)] + rng.standard_normal(DIM).astype(np.float32) * 0.035
                v /= np.linalg.norm(v)
                key = f"v{i}.{s}.{j}"
                enc.vecs[key] = v.astype(np.float32)
                items.append({"text": key, "tier": "t0"} if s != "catchers" else {"hypo": key, "tier": "t0"})
            node[s] = items
        out.append(node)
    return out


def brute_recall(ix: FragmentIndex, enc: TableEnc, rng, sub="signals", nq=50, k=20) -> float:
    labs = [lab for lab, e in ix.entries.items() if e.sub == sub]
    M = np.stack([enc.vecs[ix.entries[lab].text] for lab in labs])
    qs = M[rng.choice(len(M), size=nq, replace=False)] + rng.standard_normal((nq, DIM)).astype(np.float32) * 0.02
    qs /= np.linalg.norm(qs, axis=1, keepdims=True)
    got, _ = ix.subs[sub].query(qs.astype(np.float32), k)
    true = np.argsort(-(qs @ M.T), axis=1)[:, :k]
    hit = 0
    for i in range(nq):
        hit += len(set(int(x) for x in got[i]) & set(labs[j] for j in true[i]))
    return hit / (nq * k)


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--sizes", default="500,2000,10000")
    ap.add_argument("--frags", type=int, default=24)
    ap.add_argument("--queries", type=int, default=100)
    ap.add_argument("--bge", action="store_true")
    ap.add_argument("--out", default="")
    a = ap.parse_args()
    rng = np.random.default_rng(42)
    centers = rng.standard_normal((200, DIM)).astype(np.float32)
    centers /= np.linalg.norm(centers, axis=1, keepdims=True)
    enc = TableEnc()
    ix = FragmentIndex(enc, cap=4096, threads=2)
    agents: list[dict] = []
    rows = []
    for N in [int(s) for s in a.sizes.split(",")]:
        new = make_agents(rng, enc, len(agents), N - len(agents), centers, a.frags)
        t0 = time.perf_counter()
        for n in new:
            ix.index_put(n["id"], n)
        build_s = time.perf_counter() - t0
        agents.extend(new)
        # 单次 knn_query（1 条向量，k=20）
        one = []
        for _ in range(200):
            v = enc.vecs[agents[rng.integers(len(agents))]["signals"][0]["text"]][None, :]
            t = time.perf_counter()
            ix.subs["catchers"].query(v, 20)
            one.append((time.perf_counter() - t) * 1e3)
        # 一个 node 完整 route()
        full = []
        for q in rng.choice(len(agents), size=a.queries, replace=False):
            n = agents[int(q)]
            t = time.perf_counter()
            r = ix.route(n["id"], n, 20)
            full.append((time.perf_counter() - t) * 1e3)
        # 接入一个新 agent 的 index_put
        extra = make_agents(rng, enc, 10_000_000 + N, 1, centers, a.frags)[0]
        t = time.perf_counter()
        ix.index_put(extra["id"], extra)
        put_ms = (time.perf_counter() - t) * 1e3
        ix.index_put(extra["id"], None)
        row = {"N": N, "vectors": sum(ix.subs[s].n_active for s in SUBS), "build_s_this_step": round(build_s, 1),
               "rss_peak_mb": round(rss_mb()), "knn1_ms_p50": round(float(np.percentile(one, 50)), 3),
               "knn1_ms_p95": round(float(np.percentile(one, 95)), 3),
               "route_ms_p50": round(float(np.percentile(full, 50)), 2),
               "route_ms_p95": round(float(np.percentile(full, 95)), 2),
               "route_peers_last": len(r), "index_put_ms": round(put_ms, 2),
               "recall_at20_vs_brute": round(brute_recall(ix, enc, rng), 4)}
        rows.append(row)
        print(json.dumps(row, ensure_ascii=False), flush=True)
    res = {"frags_per_agent": a.frags, "dim": DIM, "M": 16, "ef_construction": 100, "threads": 2, "rows": rows}
    if a.bge:
        sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
        from jx.ports.enc import EncPort
        import tempfile
        p = EncPort(os.path.join(tempfile.mkdtemp(), "c.sqlite"))
        p.encode(["预热"])
        texts = [f"压测片段 {time.time_ns()} 第{i}条：最近手头有点紧，想找人一起做点小生意" for i in range(a.frags)]
        t = time.perf_counter()
        p.encode(texts)
        res["bge_m3_encode_one_node_ms"] = round((time.perf_counter() - t) * 1e3)
        t = time.perf_counter()
        p.encode(texts)
        res["bge_m3_encode_one_node_cached_ms"] = round((time.perf_counter() - t) * 1e3, 2)
        print(json.dumps({k: v for k, v in res.items() if k.startswith("bge")}), flush=True)
    if a.out:
        with open(a.out, "w") as f:
            json.dump(res, f, ensure_ascii=False, indent=1)


if __name__ == "__main__":
    main()
