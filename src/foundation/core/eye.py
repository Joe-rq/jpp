"""眼：jev（noul / choice / score）与 code（施工单 §2.4 / §3.2）。"""

from __future__ import annotations

from dataclasses import dataclass, field
from typing import Any

from foundation.core.canon import H

PRIMITIVES = ("noul", "choice", "score")


@dataclass(frozen=True)
class JevEye:
    primitive: str
    instructions: Any
    criteria: Any = None

    def question(self) -> dict:
        q: dict[str, Any] = {"type": self.primitive, "instructions": self.instructions}
        if self.criteria is not None:
            q["criteria"] = self.criteria
        return q

    @property
    def options(self) -> list[str]:
        if self.primitive == "choice" and isinstance(self.criteria, dict):
            return list(self.criteria.keys())
        return []

    @property
    def levels(self) -> list[str]:
        if self.primitive == "score" and isinstance(self.criteria, (list, tuple)):
            return list(self.criteria)
        return []

    def fp(self) -> str:
        """question_fp = H(primitive, instructions, criteria, options/levels)（§2.3）。"""
        return H(self.primitive, self.instructions, self.criteria, self.options or self.levels)


@dataclass(frozen=True)
class CodeEye:
    fn_name: str

    def fp(self) -> str:
        return H("code", self.fn_name)
