"""#49 列类型推断（choice）。真值：生成列时的类型。"""

from __future__ import annotations

import random
import re

import foundation.jv as jv

from ._common import Probe, Result, Sample, bare_ask, bare_state, exit_to_result, parse_choice, q_choice, register

_TYPES = ["整数", "小数", "日期", "布尔", "邮箱", "自由文本"]


def _gen(t: int, rnd: random.Random) -> list[str]:
    if t == 0:
        return [str(rnd.randint(-500, 5000)) for _ in range(8)]
    if t == 1:
        return [f"{rnd.uniform(-50, 500):.2f}" for _ in range(8)]
    if t == 2:
        return [f"2026-{rnd.randint(1, 12):02d}-{rnd.randint(1, 28):02d}" for _ in range(8)]
    if t == 3:
        return [rnd.choice(["true", "false", "yes", "no"]) for _ in range(8)]
    if t == 4:
        return [f"{rnd.choice(['li', 'wang', 'zhao', 'chen'])}{rnd.randint(1, 99)}@{rnd.choice(['example.com', 'mail.cn'])}" for _ in range(8)]
    words = ["延期交付", "客户要求改期", "已电话确认", "待补充资料", "现场核验通过", "需要二次报价", "物流异常", "正常"]
    return [rnd.choice(words) + rnd.choice(["", "，见附件", "（张经理）"]) for _ in range(8)]


def make_samples(n: int = 24, seed: int = 0) -> list[Sample]:
    rnd = random.Random(seed)
    out = []
    for i in range(n):
        t = i % len(_TYPES)
        col = _gen(t, rnd)
        out.append(Sample(id=f"p49-{i:02d}", truth=t, mats={"col": "列名: col_" + str(i) + "\n" + "\n".join(col)}))
    return out


类型 = jv.select("这一列的数据类型是什么？", calib=jv.calib("probe49.类型"))


@jv.program(budget=jv.Budget(calls=60, cost=0.05, layers=1))
def 推断(cols, types, keys):
    exits = jv.cut(jv.judge([jv.state(on=c, over=types) for c in cols], 类型))
    return [exit_to_result(sid, truth, e, "choice") for (sid, truth), e in zip(keys, exits)]


def builder(samples, rt):
    return 推断([jv.lit(s.mats["col"]) for s in samples], [jv.lit(t) for t in _TYPES], [(s.id, s.truth) for s in samples])


def bare(samples, client, tally):
    out = []
    for s in samples:
        a = bare_ask(client, bare_state(on=s.mats["col"], over=_TYPES), {"q0": q_choice(类型.text, _TYPES)}, tally)
        k, p = parse_choice(a["q0"])
        out.append(Result(s.id, s.truth, k, p, "bare"))
    return out


def baseline(samples):
    out = []
    for s in samples:
        vals = s.mats["col"].split("\n")[1:]
        def allm(p):
            return all(re.fullmatch(p, v) for v in vals)
        if allm(r"-?\d+"):
            k = 0
        elif allm(r"-?\d+\.\d+"):
            k = 1
        elif allm(r"\d{4}-\d{2}-\d{2}"):
            k = 2
        elif allm(r"(true|false|yes|no)"):
            k = 3
        elif allm(r"[^@\s]+@[^@\s]+\.[a-z]+"):
            k = 4
        else:
            k = 5
        out.append(Result(s.id, s.truth, k, None, "regex"))
    return out


register(Probe("p49", "列类型推断", "choice", make_samples, builder, bare, baseline))
