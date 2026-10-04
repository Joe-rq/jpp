"""事件总线：运行时 → 观察者（进程内订阅 + 可写 .jsonl）。格式见 design/events.md。"""
from __future__ import annotations

import json
import os


class Bus:
    def __init__(self, path: str | None = None, clock=None):
        self.subs = []
        self.clock = clock
        self.f = None
        self.n = 0
        if path:
            os.makedirs(os.path.dirname(os.path.abspath(path)), exist_ok=True)
            self.f = open(path, "a", buffering=1)

    def subscribe(self, cb):
        self.subs.append(cb)
        return lambda: self.subs.remove(cb)

    def emit(self, typ: str, **kw):
        ev = {"t": round(self.clock(), 3) if self.clock else 0.0, "type": typ, **kw}
        self.n += 1
        if self.f:
            drop = ("prev", "value") if "added" in ev else ("prev",)       # 文件里不重复上一版本；只增集合只写新加的
            fev = {k: v for k, v in ev.items() if k not in drop}
            self.f.write(json.dumps(fev, ensure_ascii=False, default=str) + "\n")
        for cb in list(self.subs):
            try:
                cb(ev)
            except Exception:  # 观察者出错不影响运行时
                pass

    def close(self):
        if self.f:
            self.f.close()
            self.f = None
