"""宿主动作的标准实现（CLI 与测试用）：embed_topk（编码器召回）、graph_components（连通分量）。
真正的网络宿主（应用/通爻网/host/）注册自己的动作；这里只是让 .jpx 例子离线可跑。"""
from __future__ import annotations

import numpy as np

from .sched import HashEncoder


def install(engine, enc=None):
    enc = enc or engine.enc or HashEncoder()

    def embed_topk(query, corpus, k=5):
        """query: 文字；corpus: [{id, text}]；返回最相似的 k 个 id（确定性：相似度相同按 id）。"""
        if not corpus:
            return []
        texts = [query] + [str(c.get("text", "")) for c in corpus]
        v = enc.encode(texts)
        sims = v[1:] @ v[0]
        order = sorted(range(len(corpus)), key=lambda i: (-float(sims[i]), str(corpus[i].get("id"))))
        return [corpus[i]["id"] for i in order[:int(k)]]

    def graph_components(nodes, edges):
        parent = {n: n for n in nodes}

        def find(x):
            while parent[x] != x:
                parent[x] = parent[parent[x]]
                x = parent[x]
            return x
        for e in edges:
            a, b = e[0], e[1]
            if a in parent and b in parent:
                parent[find(a)] = find(b)
        groups = {}
        for n in nodes:
            groups.setdefault(find(n), []).append(n)
        return sorted((sorted(g) for g in groups.values()), key=lambda g: (-len(g), g))

    engine.register_action("embed_topk", embed_topk, transparent=True)
    engine.register_action("graph_components", graph_components, transparent=True)
