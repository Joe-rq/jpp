"""测试用的笨引擎：只按宿主接口 v1（jx/README.md）记写入、返回读到的值、把写入作为 publish 发到总线。
不跑任何常驻程序、不合批、不调度——那些是语言的事；这里只为了测宿主的接线与展示。
测试里用 sim_put 模拟「网络程序已经发布了某个单元」。"""
from __future__ import annotations

import time

UNION = {"inbox", "unlocked"}


class Bus:
    def __init__(self):
        self.subs = []

    def subscribe(self, cb):
        self.subs.append(cb)

    def emit(self, ev):
        for cb in self.subs:
            cb(ev)


class FakeEngine:
    def __init__(self):
        self.cells: dict[tuple, object] = {}
        self.actions: dict[str, tuple] = {}
        self.writes: list[tuple] = []
        self.bus = Bus()
        self.epoch = 0
        self.started = False
        self.statuses: dict[tuple, dict] = {}

    async def start(self):
        self.started = True

    async def stop(self):
        self.started = False

    def register_action(self, name, fn, cost_usd=0.0, transparent=True, depends_on=None):
        self.actions[name] = (fn, transparent, depends_on)

    def _put(self, cell, key, value, writer):
        k = (cell, tuple(key))
        prev = self.cells.get(k)
        if cell in UNION:
            items = value if isinstance(value, list) else [value]
            new = list(prev or []) + items
        else:
            new = value
        self.cells[k] = new
        self.epoch += 1
        self.writes.append((writer, cell, list(key), value))
        self.bus.emit({"type": "publish", "t": time.time(), "cell": cell, "key": list(key), "version": self.epoch,
                       "status": "settled", "value": new, "prev": prev})
        return self.epoch

    async def put_source(self, cell, key, value, budget=None, deadline_s=None, cause=None):
        return self._put(cell, key, value, "host")

    async def remove_source(self, cell, key):
        k = (cell, tuple(key))
        prev = self.cells.pop(k, None)
        self.epoch += 1
        self.writes.append(("host", cell, list(key), None))
        self.bus.emit({"type": "publish", "t": time.time(), "cell": cell, "key": list(key), "version": self.epoch,
                       "status": "removed", "value": None, "prev": prev})
        return self.epoch

    def sim_put(self, cell, key, value, status="settled"):
        """模拟网络程序发布（写者是程序，不是宿主）。"""
        self.statuses[(cell, tuple(key))] = {"status": status, "pending": []}
        return self._put(cell, key, value, "program")

    def read(self, cell, key, mode="peek"):
        return self.cells.get((cell, tuple(key)))

    def keys(self, cell, contains=None):
        out = [list(k) for (c, k) in self.cells if c == cell]
        if contains is not None:
            out = [k for k in out if contains in k]
        return sorted(out)

    def status(self, cell, key):
        return self.statuses.get((cell, tuple(key)), {"status": "settled", "version": 1, "pending": []})

    # 预载驱动用
    async def idle(self, timeout=None):
        return None

    def read_host(self, cell, key, mode="peek"):
        return self.read(cell, key, mode)

    def stats(self):
        return {"calls": 0, "cache_hits": 0, "cost_usd": 0.0, "errors": 0}
