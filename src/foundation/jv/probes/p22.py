"""#22 识别 flaky 测试（noul + 做：真跑 pytest 两次）。真值：注入随机失败。

Jev 看到测试源码 + 两次报告；无 Jev 基线只看两次报告是否不一致（两次一致的 flaky 会漏）。
"""

from __future__ import annotations

import random

import foundation.jv as jv

from ._common import Probe, Result, Sample, bare_ask, bare_state, exit_to_result, parse_noul, q_noul, register, run_py

_FLAKY = [
    "import random\n\ndef test_latency():\n    t = random.random() * 100\n    assert t < 50\n",
    "import time\n\ndef test_clock():\n    ns = time.time_ns()\n    assert ns % 2 == 0\n",
    "import random\n\ndef test_sample_order():\n    xs = random.sample(range(10), 3)\n    assert xs == sorted(xs)\n",
    "import os\n\ndef test_pid_parity():\n    assert os.getpid() % 2 == 1\n",
    "import random\n\ndef test_retry_budget():\n    fails = sum(random.random() < 0.5 for _ in range(3))\n    assert fails <= 1\n",
    "import time\n\ndef test_ts_digit():\n    assert str(time.time_ns())[-1] in '01234'\n",
]
_STABLE_PASS = [
    "def test_sum():\n    assert sum([1, 2, 3]) == 6\n",
    "def test_sorted():\n    assert sorted([3, 1, 2]) == [1, 2, 3]\n",
    "import json\n\ndef test_json():\n    assert json.loads('{\"a\": 1}') == {'a': 1}\n",
]
_STABLE_FAIL = [
    "def test_sum_wrong():\n    assert sum([1, 2, 3]) == 7\n",
    "def test_upper():\n    assert 'abc'.upper() == 'abc'\n",
    "import json\n\ndef test_json_bad():\n    assert json.loads('[1, 2]') == [2, 1]\n",
]


def make_samples(n: int = 24, seed: int = 0) -> list[Sample]:
    rnd = random.Random(seed)
    out = []
    for i in range(n):
        if i % 2 == 0:
            src, truth = _FLAKY[(i // 2) % len(_FLAKY)], True
        else:
            pool = _STABLE_PASS if (i // 2) % 2 == 0 else _STABLE_FAIL
            src, truth = pool[(i // 4) % len(pool)], False
        src = f"# case {i}\n" + src
        out.append(Sample(id=f"p22-{i:02d}", truth=truth, mats={"src": src}))
    return out


def _pytest(src, run_idx):
    r = run_py({"test_x.py": src.content}, ["-m", "pytest", "-q", "-p", "no:cacheprovider", "test_x.py"], timeout=60)
    tail = [l for l in r["stdout"].splitlines() if l.strip()][-3:]
    return {"run": run_idx.content, "exit": r["exit"], "tail": tail}


跑测试 = jv.register_action("pytest_twice", fn=_pytest, taint_out="trusted",
                           reason="仓库自带 pytest，报告只含通过/失败摘要，不含用户文本")

不稳 = jv.test("这个测试是 flaky（结果随运行而变）的吗？", calib=jv.calib("probe22.不稳"))


@jv.program(budget=jv.Budget(calls=40, cost=0.05, layers=1))
def 识别(items, keys):
    states = []
    for i, (src, r0, r1) in enumerate(items):
        a = jv.do(跑测试, src, r0, iter_seq=i)
        b = jv.do(跑测试, src, r1, iter_seq=i)
        states.append(jv.state(on=src, ctx=[a, b]))
    exits = jv.cut(jv.judge(states, 不稳))
    return [exit_to_result(sid, truth, e, "noul") for (sid, truth), e in zip(keys, exits)]


def builder(samples, rt):
    items = [(jv.lit(s.mats["src"]), jv.lit("第一次"), jv.lit("第二次")) for s in samples]
    return 识别(items, [(s.id, s.truth) for s in samples])


def _two_runs(src: str):
    return _pytest(jv.lit(src), jv.lit("第一次")), _pytest(jv.lit(src), jv.lit("第二次"))


def bare(samples, client, tally):
    out = []
    for s in samples:
        a, b = _two_runs(s.mats["src"])
        ans = bare_ask(client, bare_state(on=s.mats["src"], ctx=[a, b]), {"q0": q_noul(不稳.text)}, tally)
        pred, p = parse_noul(ans["q0"])
        out.append(Result(s.id, s.truth, pred, p, "bare"))
    return out


def baseline(samples):
    """全跑两次：两次退出码不同即 flaky。"""
    out = []
    for s in samples:
        a, b = _two_runs(s.mats["src"])
        differ = a["exit"] != b["exit"]
        out.append(Result(s.id, s.truth, differ, 1.0 if differ else 0.0, "rerun2"))
    return out


register(Probe("p22", "识别 flaky 测试", "noul", make_samples, builder, bare, baseline,
               note="noul + do（pytest ×2）；基线 = 两次是否不一致"))
