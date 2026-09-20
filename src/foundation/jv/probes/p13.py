"""#13 编译/导入错误归因到修改行（choice + 做：真跑 python 导入）。

材料：一份 diff（4 行改动）+ 导入时的真实 traceback。题：哪一行改动引起了报错。真值：注入错误的那一行。
"""

from __future__ import annotations

import random

import foundation.jv as jv

from ._common import (Probe, Result, Sample, bare_ask, bare_state, exit_to_result, parse_choice, q_choice, register, run_py)

_BASE = """import json

def load(path):
    with open(path) as fh:
        return json.load(fh)

def total(values):
    return sum(values)

def retry(fn, times):
    for _ in range(times):
        try:
            return fn()
        except Exception:
            times = times - 1
    return None

CONFIG = {{"retries": 3, "verbose": False}}
{tail}
"""

_GOOD_LINES = [
    "LIMIT = CONFIG['retries'] * 2",
    "VERBOSE = bool(CONFIG.get('verbose'))",
    "NAMES = ['load', 'total', 'retry']",
    "VERSION = '1.2.0'",
    "DEFAULT_TIMES = 3",
    "EMPTY_TOTAL = total([])",
]
_BAD_LINES = [
    "LIMIT = CONFG['retries'] * 2",          # NameError：拼错
    "EMPTY_TOTAL = totl([])",                # NameError
    "VERSION = '1.2.0",                      # SyntaxError：引号
    "DEFAULT_TIMES = int('three')",          # ValueError
    "NAMES = ['load', 'total', retry()]",    # TypeError：缺参数
    "VERBOSE = CONFIG['verbos']",            # KeyError
]


def make_samples(n: int = 24, seed: int = 0) -> list[Sample]:
    rnd = random.Random(seed)
    out = []
    for i in range(n):
        good = rnd.sample(_GOOD_LINES, 3)
        bad = rnd.choice(_BAD_LINES)
        lines = good + [bad]
        rnd.shuffle(lines)
        k = lines.index(bad)
        src = _BASE.format(tail="\n".join(lines))
        base_n = _BASE.format(tail="").count("\n")
        diff = "\n".join(f"+ L{base_n + j}: {l}" for j, l in enumerate(lines))
        out.append(Sample(id=f"p13-{i:02d}", truth=k, mats={"diff": diff, "src": src, "lines": lines}))
    return out


def _import(src):
    r = run_py({"m.py": src.content}, ["-c", "import m"])
    return {"exit": r["exit"], "stderr": r["stderr"].strip().splitlines()[-6:]}


导入 = jv.Action("import_module", fn=_import, taint_out="untrusted")

哪行 = jv.select("导入这份模块时的报错是由 diff 里哪一行改动引起的？", calib=jv.calib("probe13.哪行"))


@jv.program(budget=jv.Budget(calls=80, cost=0.05, layers=2))
def 归因(items, keys):
    outs = [jv.do(导入, src, iter_seq=i) for i, (diff, src, lines) in enumerate(items)]
    states = [jv.state(on=diff, ctx=[o], over=lines) for (diff, src, lines), o in zip(items, outs)]
    exits = jv.cut(jv.judge(states, 哪行))
    return [exit_to_result(sid, truth, e, "choice") for (sid, truth), e in zip(keys, exits)]   # 出口在程序内收口（J-05）


def builder(samples, rt):
    items = [(jv.lit(s.mats["diff"]), jv.lit(s.mats["src"]), [jv.lit(l) for l in s.mats["lines"]]) for s in samples]
    return 归因(items, [(s.id, s.truth) for s in samples])


def bare(samples, client, tally):
    out = []
    for s in samples:
        tb = _import(jv.lit(s.mats["src"]))
        st = bare_state(on=s.mats["diff"], ctx=[tb], over=s.mats["lines"])
        a = bare_ask(client, st, {"q0": q_choice(哪行.text, s.mats["lines"])}, tally)
        k, p = parse_choice(a["q0"])
        out.append(Result(s.id, s.truth, k, p, "bare"))
    return out


def baseline(samples):
    """启发式：traceback 里的行号 → diff 行；SyntaxError 也带行号。"""
    out = []
    for s in samples:
        tb = _import(jv.lit(s.mats["src"]))
        text = "\n".join(tb["stderr"])
        base_n = _BASE.format(tail="").count("\n")
        k = None
        for j in range(len(s.mats["lines"])):
            if f"line {base_n + j}" in text:
                k = j
        out.append(Result(s.id, s.truth, k, None, "heuristic"))
    return out


register(Probe("p13", "编译错误归因到修改行", "choice", make_samples, builder, bare, baseline,
               note="choice + do；基线是 traceback 行号直接映射"))
