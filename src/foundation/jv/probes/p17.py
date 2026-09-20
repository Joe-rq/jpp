"""#17 提交信息与 diff 一致（noul）。真值：正例是同一模板生成的 (diff, message)，负例把 message 换成别的模板的。"""

from __future__ import annotations

import random

import foundation.jv as jv

from ._common import Probe, Result, Sample, bare_ask, bare_state, exit_to_result, parse_noul, q_noul, register

_TEMPLATES = [
    ("fix: 修正 total 对空列表的返回值", "-    return sum(values) / len(values)\n+    return sum(values) / len(values) if values else 0.0"),
    ("feat: load 支持自定义编码", "-def load(path):\n-    with open(path) as fh:\n+def load(path, encoding='utf-8'):\n+    with open(path, encoding=encoding) as fh:"),
    ("refactor: retry 改用 for-else", "-    while times > 0:\n-        times -= 1\n+    for _ in range(times):\n+        pass\n+    else:\n+        return None"),
    ("chore: 加日志输出", "+import logging\n+log = logging.getLogger(__name__)\n+    log.info('loaded %s', path)"),
    ("fix: 配置默认 retries 从 3 改为 5", "-CONFIG = {'retries': 3}\n+CONFIG = {'retries': 5}"),
    ("docs: 补 total 的 docstring", "+    \"\"\"求和；空列表返回 0。\"\"\""),
    ("perf: 缓存 load 的结果", "+from functools import lru_cache\n+@lru_cache(maxsize=32)\n def load(path):"),
    ("test: 增加 retry 的失败用例", "+def test_retry_gives_up():\n+    assert retry(lambda: 1 / 0, 2) is None"),
    ("fix: 修正 VERSION 字符串", "-VERSION = '1.2'\n+VERSION = '1.2.0'"),
    ("feat: 新增 median 函数", "+def median(values):\n+    s = sorted(values)\n+    return s[len(s) // 2]"),
    ("refactor: 删除未用的 NAMES", "-NAMES = ['load', 'total', 'retry']"),
    ("fix: retry 捕获的异常改为 ValueError", "-        except Exception:\n+        except ValueError:"),
]


def make_samples(n: int = 24, seed: int = 0) -> list[Sample]:
    rnd = random.Random(seed)
    out = []
    for i in range(n):
        t = i % len(_TEMPLATES)
        msg, diff = _TEMPLATES[t]
        pos = (i // len(_TEMPLATES) + i) % 2 == 0
        if not pos:
            other = rnd.choice([j for j in range(len(_TEMPLATES)) if j != t])
            msg = _TEMPLATES[other][0]
        out.append(Sample(id=f"p17-{i:02d}", truth=pos, mats={"diff": diff, "msg": msg}))
    npos = sum(1 for s in out if s.truth)
    assert abs(npos - n / 2) <= 1, npos
    return out


一致 = jv.test("这条提交信息如实描述了这个 diff 的改动吗？", calib=jv.calib("probe17.一致"))


@jv.program(budget=jv.Budget(calls=40, cost=0.05, layers=1))
def 核对(items, keys):
    exits = jv.cut(jv.judge([jv.state(on=diff, ctx=[msg]) for diff, msg in items], 一致))
    return [exit_to_result(sid, truth, e, "noul") for (sid, truth), e in zip(keys, exits)]


def builder(samples, rt):
    items = [(jv.lit(s.mats["diff"]), jv.lit(s.mats["msg"])) for s in samples]
    return 核对(items, [(s.id, s.truth) for s in samples])


def bare(samples, client, tally):
    out = []
    for s in samples:
        a = bare_ask(client, bare_state(on=s.mats["diff"], ctx=[s.mats["msg"]]), {"q0": q_noul(一致.text)}, tally)
        pred, p = parse_noul(a["q0"])
        out.append(Result(s.id, s.truth, pred, p, "bare"))
    return out


def _tokens(x: str) -> set[str]:
    import re
    return {t.lower() for t in re.findall(r"[A-Za-z_]{3,}|[一-鿿]{2}", x)}


def baseline(samples):
    """启发式：信息与 diff 的词重叠率。"""
    out = []
    for s in samples:
        a, b = _tokens(s.mats["msg"]), _tokens(s.mats["diff"])
        j = len(a & b) / max(1, len(a | b))
        out.append(Result(s.id, s.truth, j >= 0.1, round(j, 3), "heuristic"))
    return out


register(Probe("p17", "提交信息与 diff 一致", "noul", make_samples, builder, bare, baseline))
