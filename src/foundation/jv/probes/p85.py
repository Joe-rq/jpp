"""#85 依赖升级是否破坏（noul + 做：真跑 pytest）。

Jev 只看 v2 变更说明 + 项目对依赖的用法，预测会不会破坏；真值 = 真的把 v2 放进 PYTHONPATH 跑测试。
无 Jev 基线 = 全跑（每个项目都跑）；增益按 E9f 式「固定召回下省掉的跑测次数」量。
"""

from __future__ import annotations

import random

import foundation.jv as jv

from ._common import Probe, Result, Sample, bare_ask, bare_state, exit_to_result, parse_noul, q_noul, recall_saved, register, run_py

_V1 = {"parse": "def parse(s):\n    return [int(x) for x in s.split(',') if x]\n",
       "fmt": "def fmt(xs, sep=','):\n    return sep.join(str(x) for x in xs)\n",
       "clamp": "def clamp(x, lo, hi):\n    return max(lo, min(hi, x))\n",
       "mean": "def mean(xs):\n    return sum(xs) / len(xs) if xs else 0.0\n",
       "slug": "def slug(s):\n    return s.lower().replace(' ', '-')\n"}
_CHANGES = {  # v2 对某个函数的改动：(变更说明, v2 源码, 是否破坏旧用法)
    "parse": [("parse 现在返回元组而不是列表", "def parse(s):\n    return tuple(int(x) for x in s.split(',') if x)\n", True),
              ("parse 内部实现改用 map，行为不变", "def parse(s):\n    return list(map(int, [x for x in s.split(',') if x]))\n", False)],
    "fmt": [("fmt 的 sep 参数改为必填", "def fmt(xs, sep):\n    return sep.join(str(x) for x in xs)\n", True),
            ("fmt 加了类型注解，行为不变", "def fmt(xs, sep=','):\n    return sep.join(str(x) for x in xs)\n", False)],
    "clamp": [("clamp 改名为 clip（旧名删除）", "def clip(x, lo, hi):\n    return max(lo, min(hi, x))\n", True),
              ("clamp 对 lo > hi 抛 ValueError（原来静默）", "def clamp(x, lo, hi):\n    if lo > hi:\n        raise ValueError('lo > hi')\n    return max(lo, min(hi, x))\n", False)],
    "mean": [("mean 对空列表改为抛 ZeroDivisionError", "def mean(xs):\n    return sum(xs) / len(xs)\n", True),
             ("mean 加了 docstring", "def mean(xs):\n    \"\"\"平均值。\"\"\"\n    return sum(xs) / len(xs) if xs else 0.0\n", False)],
    "slug": [("slug 改名为 slugify（旧名删除）", "def slugify(s):\n    return s.lower().replace(' ', '-')\n", True),
             ("slug 现在要求输入是 str，否则 TypeError", "def slug(s):\n    if not isinstance(s, str):\n        raise TypeError('str only')\n    return s.lower().replace(' ', '-')\n", False)],
}
_USAGE = {  # 项目用法与测试（旧行为）
    "parse": ("xs = dep.parse('1,2,3')\nassert xs == [1, 2, 3]\n", "parse('1,2,3') 期望得到列表 [1, 2, 3]"),
    "fmt": ("assert dep.fmt([1, 2]) == '1,2'\n", "fmt([1, 2]) 不传 sep，期望 '1,2'"),
    "clamp": ("assert dep.clamp(5, 0, 3) == 3\n", "调用 clamp(5, 0, 3)"),
    "mean": ("assert dep.mean([]) == 0.0\n", "mean([]) 期望 0.0"),
    "slug": ("assert dep.slug('A B') == 'a-b'\n", "slug('A B') 期望 'a-b'"),
}
_NAMES = list(_V1)


def make_samples(n: int = 24, seed: int = 0) -> list[Sample]:
    rnd = random.Random(seed)
    out = []
    for i in range(n):
        used = rnd.sample(_NAMES, 2)
        changed = rnd.sample(_NAMES, 2)
        want_break = i % 2 == 0
        # 保证正负平衡：want_break 时让至少一个被用的函数发生破坏性改动，否则全是非破坏性改动
        v2 = dict(_V1)
        notes = []
        broke = False
        for name in changed:
            opts = [c for c in _CHANGES[name] if (c[2] if (want_break and name in used and not broke) else not c[2])]
            if not opts:
                opts = [c for c in _CHANGES[name] if not c[2]]
            note, src, is_break = rnd.choice(opts)
            v2[name] = src
            notes.append(f"- {note}")
            broke = broke or (is_break and name in used)
        if want_break and not broke:
            name = used[0]
            note, src, _ = next(c for c in _CHANGES[name] if c[2])
            v2[name] = src
            notes.append(f"- {note}")
            broke = True
        changelog = "dep 2.0 变更说明:\n" + "\n".join(notes)
        usage = "项目对 dep 的用法:\n" + "\n".join(f"- {_USAGE[u][1]}" for u in used)
        test_src = "import dep\n\ndef test_usage():\n" + "".join("    " + l + "\n" for u in used for l in _USAGE[u][0].splitlines())
        out.append(Sample(id=f"p85-{i:02d}", truth=None, mats={"changelog": changelog, "usage": usage},
                          meta={"v2": "\n".join(v2.values()), "test": test_src}))
    # 真值：真的跑（一次性，缓存在 meta）
    for s in out:
        r = run_py({"dep/__init__.py": s.meta["v2"], "test_dep.py": s.meta["test"]},
                   ["-m", "pytest", "-q", "-p", "no:cacheprovider", "test_dep.py"], timeout=60)
        s.truth = r["exit"] != 0
        s.meta["run"] = {"exit": r["exit"], "tail": r["stdout"].strip().splitlines()[-2:]}
    return out


破坏 = jv.test("按变更说明和项目用法判断，升级到这个版本会破坏项目吗？", calib=jv.calib("probe85.破坏"))


@jv.program(budget=jv.Budget(calls=40, cost=0.05, layers=1))
def 预判(items, keys):
    exits = jv.cut(jv.judge([jv.state(on=log, ctx=[use]) for log, use in items], 破坏))
    return [exit_to_result(sid, truth, e, "noul") for (sid, truth), e in zip(keys, exits)]


def builder(samples, rt):
    items = [(jv.lit(s.mats["changelog"]), jv.lit(s.mats["usage"])) for s in samples]
    return 预判(items, [(s.id, s.truth) for s in samples])


def bare(samples, client, tally):
    out = []
    for s in samples:
        a = bare_ask(client, bare_state(on=s.mats["changelog"], ctx=[s.mats["usage"]]), {"q0": q_noul(破坏.text)}, tally)
        pred, p = parse_noul(a["q0"])
        out.append(Result(s.id, s.truth, pred, p, "bare"))
    return out


def baseline(samples):
    """启发式：变更说明里出现「删除 / 必填 / 改为抛 / 返回元组」等词且涉及被用函数名。"""
    kws = ["删除", "必填", "改为抛", "返回元组", "改名"]
    out = []
    for s in samples:
        used = [u for u in _NAMES if u in s.mats["usage"]]
        hit = any(any(k in line for k in kws) and any(u in line for u in used) for line in s.mats["changelog"].splitlines())
        out.append(Result(s.id, s.truth, hit, 1.0 if hit else 0.0, "keywords"))
    return out


register(Probe("p85", "依赖升级是否破坏", "noul", make_samples, builder, bare, baseline,
               note="真值 = 真跑 pytest；增益按固定召回下省掉的跑测次数"))
