"""从真实运行记录导出宣传片画面用的数据（只读原始文件）。

- data/net.js：通爻网络真机录制（docs/demos/towow-net/run/events.jsonl）的节点、传播、逐节点判断活动与累计判断数。
"""
import collections
import json
import math
import random
from pathlib import Path

HERE = Path(__file__).resolve().parent
REPO = HERE.parents[1]
BINS = 60


def net():
    ev = [json.loads(l) for l in open(REPO / "docs/demos/towow-net/run/events.jsonl")]
    meta = json.load(open(REPO / "docs/demos/towow-net/run/meta.json"))
    T = meta["summary"]["elapsed_ms"]
    nodes, idx = [], {}
    for e in ev:
        if e["type"] == "node_join":
            n = e["node"]
            idx[n["id"]] = len(nodes)
            nodes.append(n.get("kind", "?"))
    routes, intents = [], []
    act = collections.defaultdict(lambda: [0] * BINS)
    cum = [0] * BINS
    usd = [0.0] * BINS
    b = lambda t: min(BINS - 1, int(t / T * BINS))
    src = {e["id"]: e["from"] for e in ev if e["type"] == "intent"}
    for e in ev:
        t = e.get("t", 0)
        fr = e.get("from") or src.get(e.get("about"))
        if e["type"] == "route" and fr in idx:
            tos = [idx[x] for x in e.get("to", []) if x in idx]
            if tos:
                routes.append([round(t / T, 4), idx[fr], tos])
        elif e["type"] == "intent" and e.get("from") in idx:
            intents.append([round(t / T, 4), idx[e["from"]]])
        elif e["type"] == "judge":
            cum[b(t)] += 1
            usd[b(t)] += e.get("usd") or 0
            if e.get("node") in idx:
                act[idx[e["node"]]][b(t)] += 1
    pairs = collections.Counter()
    for _, f, tos in routes:
        for x in tos:
            pairs[(min(f, x), max(f, x))] += 1
    # 简单力导布局，固定种子
    random.seed(3)
    n = len(nodes)
    P = [[random.uniform(-1, 1), random.uniform(-1, 1)] for _ in range(n)]
    edges = list(pairs)
    steps = 200
    for it in range(steps):
        F = [[0.0, 0.0] for _ in range(n)]
        for i in range(n):
            xi, yi = P[i]
            for j in range(i + 1, n):
                dx, dy = xi - P[j][0], yi - P[j][1]
                f = 0.0009 / (dx * dx + dy * dy + 1e-4)
                F[i][0] += dx * f; F[i][1] += dy * f
                F[j][0] -= dx * f; F[j][1] -= dy * f
        for a, c in edges:
            dx, dy = P[c][0] - P[a][0], P[c][1] - P[a][1]
            F[a][0] += dx * .02; F[a][1] += dy * .02
            F[c][0] -= dx * .02; F[c][1] -= dy * .02
        s = .5 * (1 - it / steps) + .05
        for i in range(n):
            F[i][0] -= P[i][0] * .01; F[i][1] -= P[i][1] * .01
            P[i][0] += max(-.05, min(.05, F[i][0] * s))
            P[i][1] += max(-.05, min(.05, F[i][1] * s))
    xs, ys = [p[0] for p in P], [p[1] for p in P]
    pos = [[round((x - min(xs)) / (max(xs) - min(xs)), 4), round((y - min(ys)) / (max(ys) - min(ys)), 4)] for x, y in P]
    kinds = sorted(set(nodes))
    out = {"elapsed_ms": T, "summary": meta["summary"] | {"per_prog": None}, "programs": len(meta["programs"]),
           "pos": pos, "kind": [kinds.index(k) for k in nodes], "kinds": kinds,
           "routes": routes, "intents": intents, "act": [act[i] for i in range(n)],
           "cum": cum, "usd": [round(u, 6) for u in usd], "edges": [list(e) for e in edges]}
    (HERE / "data").mkdir(exist_ok=True)
    (HERE / "data/net.js").write_text("window.NET=" + json.dumps(out, separators=(",", ":"), ensure_ascii=False) + ";\n")
    print("net:", n, "nodes", len(routes), "routes", len(intents), "intents", sum(cum), "judgments", round(sum(usd), 4), "usd", kinds)


if __name__ == "__main__":
    net()
