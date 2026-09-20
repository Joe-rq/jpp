"""效应记录与惰性句柄（§2.2、§2.4–§2.6、§2.8）。

`judge` 登记 JudgeEffect（不发）；`do` 登记 DoEffect 并返回 MatFuture（不发）；`gen`/`ask` 一登记就发，
所以它们没有效应记录类，直接在 runtime 里执行。这里只有数据。
"""

from __future__ import annotations

from dataclasses import dataclass, field
from typing import Any

from .ir import Action, Fail, Mat, Q, State, _Future


class MatFuture(_Future):
    """`do` 的期物。读 .content / 进状态被刷新 / 传给 gen·transform 时解析。"""

    def __init__(self, rt, effect: "DoEffect"):
        self._rt = rt
        self.effect = effect
        self._resolved = None
        self.fail: Fail | None = None

    def __repr__(self):
        return f"MatFuture({self.effect.action.name}@{self.effect.site}, {'done' if self._resolved else 'lazy'})"


@dataclass(eq=False)
class DoEffect:
    action: Action
    args: list                      # MatLike | 普通值
    iter_seq: int
    site: str
    seq: int
    guard: Any = None
    future: MatFuture | None = None
    key: str = ""
    done: bool = False

    def ready(self) -> bool:
        return all(not isinstance(a, _Future) or a._resolved is not None for a in self.args)

    def deps(self) -> list[MatFuture]:
        return [a for a in self.args if isinstance(a, MatFuture) and a._resolved is None]


@dataclass(eq=False)
class JudgeEffect:
    states: list[State]
    qs: list[Q]
    site: str
    seq: int
    rt: Any
    vectorized: bool = False
    run_seq: int = 0
    readings: Any = None            # Readings | ReadingsVec
    done: bool = False
    segment: int = 0                # 直线段号（W-impure 禁融合用）
    frame: Any = None               # 登记时所在的 @jv.program 子账帧（嵌套预算用）

    def ready(self) -> bool:
        return all(s.is_ready() for s in self.states)
