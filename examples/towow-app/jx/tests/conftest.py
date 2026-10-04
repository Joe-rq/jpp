import asyncio
import json
import os
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
if ROOT not in sys.path:
    sys.path.insert(0, ROOT)

import pytest  # noqa: E402

from jx.engine import Engine, VirtualClock  # noqa: E402
from jx.sched import PortAbsent, synthetic_raw, fixture_key  # noqa: E402


class FnPort:
    """测试用判断端口：答案由函数给（state_wire, WireQ）→ 原始答案；同步完成。"""
    sync = True

    def __init__(self, fn=None, latency=None):
        self.fn = fn
        self.down = False
        self.calls = []
        self.latency = latency

    def call_sync(self, state, qs):
        if self.down:
            raise PortAbsent("模拟掉线")
        self.calls.append((state, [q.text for q in qs]))
        out = []
        for q in qs:
            r = self.fn(state, q) if self.fn else None
            if r is None:
                r = synthetic_raw(fixture_key(state, q), q)
            elif isinstance(r, (int, float)):
                r = {"type": "noul", "noul": float(r)}
            elif isinstance(r, dict) and "type" not in r:
                r = {"type": "choice", "probabilities": r}
            out.append(r)
        return out

    def simulated_latency(self, state, qs):
        return self.latency(state, qs) if self.latency else 0.0


def text_of(state):
    return json.dumps(state, ensure_ascii=False)


def make(src, flags=None, port=None, seed=0, **kw):
    return Engine.from_source(src, "<test>", ports={"judge": port or FnPort()}, flags=flags or {}, seed=seed,
                              clock=VirtualClock(), **kw)


def run(coro):
    return asyncio.run(coro)


@pytest.fixture
def fnport():
    return FnPort
