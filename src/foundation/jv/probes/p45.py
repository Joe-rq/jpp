"""#45 性能回归定位提交（choice + 做：真跑基准）。8 个连续提交，其中一个注入慢代码；基准在子进程里计时。"""

from __future__ import annotations

import random

import foundation.jv as jv

from ._common import Probe, Result, Sample, bare_ask, bare_state, exit_to_result, parse_choice, q_choice, register, run_py

_STEPS = [
    ("c0 初版", "def work(xs):\n    return sum(xs)\n"),
    ("c1 过滤负数", "def work(xs):\n    return sum(x for x in xs if x >= 0)\n"),
    ("c2 加入去重", "def work(xs):\n    return sum(set(x for x in xs if x >= 0))\n"),
    ("c3 改用列表推导", "def work(xs):\n    ys = [x for x in xs if x >= 0]\n    return sum(set(ys))\n"),
    ("c4 加统计计数", "def work(xs):\n    ys = [x for x in xs if x >= 0]\n    n = len(ys)\n    return sum(set(ys)) + n * 0\n"),
    ("c5 记录最大值", "def work(xs):\n    ys = [x for x in xs if x >= 0]\n    m = max(ys) if ys else 0\n    return sum(set(ys)) + m * 0\n"),
    ("c6 整理变量名", "def work(values):\n    kept = [v for v in values if v >= 0]\n    top = max(kept) if kept else 0\n    return sum(set(kept)) + top * 0\n"),
    ("c7 加注释", "def work(values):\n    # 只统计非负数\n    kept = [v for v in values if v >= 0]\n    top = max(kept) if kept else 0\n    return sum(set(kept)) + top * 0\n"),
]
_SLOW_LINE = "    {v} = [x for i, x in enumerate({v}) if x not in {v}[:i]]   # 逐个比较去重（O(n²)）\n"


def _slow(src: str) -> str:
    """给每个提交版本注入同一段 O(n²) 去重；对每种写法都产生合法代码（run1 第一版对 c2–c5 产生了语法错，见偏离记录）。"""
    if "kept = [" in src:
        return src.replace("    top = max(kept)", _SLOW_LINE.format(v="kept") + "    top = max(kept)", 1)
    if "ys = [" in src:
        return src.replace("    ys = [x for x in xs if x >= 0]\n", "    ys = [x for x in xs if x >= 0]\n" + _SLOW_LINE.format(v="ys"), 1)
    if "set(x for x in xs if x >= 0)" in src:
        return "def work(xs):\n    ys = [x for x in xs if x >= 0]\n" + _SLOW_LINE.format(v="ys") + "    return sum(set(ys))\n"
    return "def work(xs):\n    ys = [x for x in xs if x >= 0]\n" + _SLOW_LINE.format(v="ys") + "    return sum(ys)\n"


def make_samples(n: int = 24, seed: int = 0) -> list[Sample]:
    rnd = random.Random(seed)
    out = []
    for i in range(n):
        k = 1 + (i % 7)                                  # 慢代码从第 k 个提交起（c1..c7）
        commits = []
        for j, (msg, src) in enumerate(_STEPS):
            if j >= k:
                src = _slow(src)
            commits.append((msg, src))
        out.append(Sample(id=f"p45-{i:02d}", truth=k, mats={"commits": commits}))
    return out


def _bench(src):
    prog = src.content + "\nimport time, random\nxs = [random.randint(0, 400) for _ in range(1500)]\nt0 = time.perf_counter()\nfor _ in range(3): work(xs)\nprint(round((time.perf_counter() - t0) * 1000, 1))\n"
    r = run_py({"b.py": prog}, ["b.py"], timeout=60)
    return {"ms": float(r["stdout"].strip() or -1), "exit": r["exit"]}


基准 = jv.Action("bench", fn=_bench, taint_out="untrusted")

哪个 = jv.select("从各提交的基准耗时看，性能回归是哪个提交引入的？", calib=jv.calib("probe45.哪个"))


def _table(msgs, results):
    return "\n".join(f"{m.content}: {r.content['ms']} ms" for m, r in zip(msgs, results))


@jv.program(budget=jv.Budget(calls=60, cost=0.05, layers=1))
def 定位(items, keys):
    states = []
    for i, (msgs, srcs) in enumerate(items):
        rs = [jv.do(基准, s, iter_seq=i * 10 + j) for j, s in enumerate(srcs)]
        table = jv.transform(_table, msgs, rs)
        states.append(jv.state(on=table, over=msgs))
    exits = jv.cut(jv.judge(states, 哪个))
    return [exit_to_result(sid, truth, e, "choice") for (sid, truth), e in zip(keys, exits)]


def builder(samples, rt):
    items = [([jv.lit(m) for m, _ in s.mats["commits"]], [jv.lit(src) for _, src in s.mats["commits"]]) for s in samples]
    return 定位(items, [(s.id, s.truth) for s in samples])


def _timings(s: Sample):
    return [(m, _bench(jv.lit(src))["ms"]) for m, src in s.mats["commits"]]


def bare(samples, client, tally):
    out = []
    for s in samples:
        t = _timings(s)
        table = "\n".join(f"{m}: {ms} ms" for m, ms in t)
        msgs = [m for m, _ in s.mats["commits"]]
        a = bare_ask(client, bare_state(on=table, over=msgs), {"q0": q_choice(哪个.text, msgs)}, tally)
        k, p = parse_choice(a["q0"])
        out.append(Result(s.id, s.truth, k, p, "bare"))
    return out


def baseline(samples):
    """启发式：相邻提交耗时增幅最大处。"""
    out = []
    for s in samples:
        t = [ms for _, ms in _timings(s)]
        k = max(range(1, len(t)), key=lambda j: t[j] - t[j - 1])
        out.append(Result(s.id, s.truth, k, None, "max_delta"))
    return out


register(Probe("p45", "性能回归定位提交", "choice", make_samples, builder, bare, baseline,
               note="choice + do（子进程基准）；基线 = 最大增幅"))
