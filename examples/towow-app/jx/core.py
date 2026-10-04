"""J++x 内核值：题、状态、读数、桥（cut）、三值出口、账本。

与现行 J++ 对齐的语义（context/02 §2）：
- 读数是分布，不是值；只能经 cut 离开（J-01）。跨题不可加。
- 无线时按多数块走（B187），恰好并列出 Unsure(tie)；作者声明线 declare 按写的数切，中间 Unsure(band)。
- 出口：Act | Ignore | Pick(k) | At(level) | Unsure(cause)。
"""
from __future__ import annotations

import hashlib
import json
import os
import threading
import time
from dataclasses import dataclass, field
from typing import Any


def canon(x: Any) -> str:
    return json.dumps(x, ensure_ascii=False, sort_keys=True, separators=(",", ":"))


def h(x: Any) -> str:
    return hashlib.sha256(canon(x).encode()).hexdigest()[:24]


# ---------------------------------------------------------------- 题

@dataclass(frozen=True)
class Question:
    """题是值。op: test(是非) | select(K 选一) | measure(打分)。
    labels: select 的候选 {标签: 描述}（有序保留）；measure 的有序档位。key: 题式键（校准/账本用）。"""
    op: str
    text: str
    labels: tuple = ()
    key: str = ""
    lacks: tuple = ()  # 拿不准时可能缺的信息类别（J-05 默认链的候选来源之一）

    def criteria(self):
        if self.op == "select":
            return {k: v for k, v in self.labels}
        if self.op == "measure":
            return list(self.labels)
        return None

    def ident(self) -> str:
        return h({"op": self.op, "text": self.text, "labels": list(self.labels)})


def test(text: str, key: str = "", lacks=()) -> Question:
    return Question("test", text, (), key, tuple(lacks))


def select(text: str, options: dict | list, key: str = "", lacks=()) -> Question:
    if isinstance(options, list):
        options = {f"c{i}": o for i, o in enumerate(options)}
    return Question("select", text, tuple(options.items()), key, tuple(lacks))


def measure(text: str, levels: list, key: str = "", lacks=()) -> Question:
    return Question("measure", text, tuple(levels), key, tuple(lacks))


# ---------------------------------------------------------------- 读数与出口

@dataclass(frozen=True)
class Reading:
    q: Question
    p: float | None = None          # test
    dist: tuple = ()                # select: ((label, p),...) ; measure: (p0, p1, ...)
    by: str = "jev"                 # jev | enc(缺席降级) | cache
    state_id: str = ""

    def top(self):
        if self.q.op == "test":
            return self.p
        if self.q.op == "select":
            return max(self.dist, key=lambda kv: kv[1]) if self.dist else (None, 0.0)
        return max(range(len(self.dist)), key=lambda i: self.dist[i]) if self.dist else None

    def expect(self) -> float | None:
        """打分题的期望档位（0..1 归一）。只在同题同锚内比较（B28 order）。"""
        if self.q.op != "measure" or not self.dist:
            return None
        n = len(self.dist) - 1 or 1
        return sum(i * p for i, p in enumerate(self.dist)) / n


@dataclass(frozen=True)
class Exit:
    kind: str                 # act | ignore | pick | at | unsure
    value: Any = None         # pick: label ; at: level index
    cause: str = ""           # unsure 原因（封闭枚举的子集）
    grade: str = "Answer"     # Answer | Declared | Degraded
    conf: float = 0.0         # 出口所在块的概率质量（给人看的置信度；不参与跨题算术）
    reading: Reading | None = None
    lean: Any = None          # 缺席降级时编码器读数倾向的出口（act/ignore/标签/档位），出口本身是 Unsure(absent)
    needed: Any = None        # 默认链问出的缺的信息类别 / 「缺 X，持有者 b 拒绝」
    ask_to: tuple = ()        # 向谁要过
    waiting: bool = False     # 补信息请求已发出、回复还没定下
    from_key: str = ""        # 补信息重判时指回原判断

    @property
    def unsure(self) -> bool:
        return self.kind == "unsure"


UNSURE_CAUSES = {"band", "tie", "insufficient", "absent", "budget", "latency", "deadline",
                 "depth", "noprogress", "denied", "fail", "claim_conflict", "waiting"}
ABSENCE_CAUSES = {"absent", "budget", "latency", "deadline", "depth"}   # 缺席类：没观察到，不能放弃（B95）


def cut(r: Reading | None, line: dict | None = None, *, cause_if_none="absent") -> Exit:
    """桥。line=None：按多数块（B187）。line={'hi':..,'lo':..}：作者声明线（Declared）。"""
    if r is None:
        return Exit("unsure", cause=cause_if_none)
    if r.by == "enc":
        # 判断器缺席时的编码器读数：出口是 Unsure(absent)，倾向放在 lean（Fable-A 第七条「标 Unsure(absent) 的粗召回」）
        base = cut(Reading(r.q, r.p, r.dist, "jev", r.state_id), line)
        return Exit("unsure", cause="absent", grade="Degraded", conf=base.conf, reading=r,
                    lean=(base.value if base.kind in ("pick", "at") else base.kind))
    if line and "declare" in line:
        line = line["declare"]
    grade = "Declared" if line else "Answer"
    q = r.q
    if q.op == "test":
        p = r.p
        if line:
            if p >= line.get("hi", 0.5):
                return Exit("act", grade=grade, conf=p, reading=r)
            if p <= line.get("lo", 0.5):
                return Exit("ignore", grade=grade, conf=1 - p, reading=r)
            return Exit("unsure", cause="band", grade=grade, conf=max(p, 1 - p), reading=r)
        if p > 0.5:
            return Exit("act", grade=grade, conf=p, reading=r)
        if p < 0.5:
            return Exit("ignore", grade=grade, conf=1 - p, reading=r)
        return Exit("unsure", cause="tie", grade=grade, conf=0.5, reading=r)
    if q.op == "select":
        items = sorted(r.dist, key=lambda kv: -kv[1])
        if not items:
            return Exit("unsure", cause="insufficient", reading=r)
        if len(items) > 1 and abs(items[0][1] - items[1][1]) < 1e-9:
            return Exit("unsure", cause="tie", grade=grade, conf=items[0][1], reading=r)
        lab, p = items[0]
        if line and p < line.get("min", 0):
            return Exit("unsure", cause="band", grade=grade, conf=p, reading=r)
        return Exit("pick", value=lab, grade=grade, conf=p, reading=r)
    # measure
    d = list(r.dist)
    if not d:
        return Exit("unsure", cause="insufficient", reading=r)
    m = max(d)
    tops = [i for i, p in enumerate(d) if abs(p - m) < 1e-9]
    if len(tops) > 1:
        return Exit("unsure", cause="tie", grade=grade, conf=m, reading=r)
    return Exit("at", value=tops[0], grade=grade, conf=m, reading=r)


# ---------------------------------------------------------------- 账本

class Ledger:
    """追加日志，唯一真相来源。每次判断/生成/执行/补信息/缺席各记一条；逐行落盘。"""

    def __init__(self, path: str | None):
        self.path = path
        self.lock = threading.Lock()
        self.n = 0
        self.f = open(path, "a", buffering=1) if path else None

    def add(self, kind: str, **kw):
        with self.lock:
            self.n += 1
            if self.f:
                self.f.write(canon({"i": self.n, "t": round(time.time(), 3), "kind": kind, **kw}) + "\n")
        return self.n

    def close(self):
        if self.f:
            self.f.close()
