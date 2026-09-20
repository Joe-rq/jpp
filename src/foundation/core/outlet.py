"""读数 → 出口（施工单 §3.2）。三分：act / ignore / unsure。

这里只看一个单元自己的读数和自己的两条线，绝不跨单元比较（S4.2）。
"""

from __future__ import annotations

from dataclasses import dataclass
from typing import Any

ACT, IGNORE, UNSURE = "act", "ignore", "unsure"


@dataclass(frozen=True)
class Reading:
    value: Any               # noul 概率 / choice 选项 / score 数值
    p: float                 # 选中项概率（choice/score）或 noul 概率
    outlet: str
    detail: dict


def score_level(score: float, n_levels: int) -> int:
    """四舍五入到档位索引，避开 Python 的银行家舍入；再夹到合法范围。"""
    import math
    lvl = int(math.floor(float(score) + 0.5))
    return max(0, min(n_levels - 1, lvl))


def _three_way(p: float, hi: float, lo: float, delta: float = 0.0) -> str:
    """δ 是迟滞：线附近 ±δ 的读数不算数，一律拿不准（E1 不过 → 红队 §133）。"""
    if p >= hi + delta:
        return ACT
    if p <= lo - delta:
        return IGNORE
    return UNSURE


def read_answer(primitive: str, answer: dict, hi: float, lo: float,
                delta: float = 0.0) -> Reading:
    if primitive == "noul":
        p = float(answer["noul"])
        return Reading(value=p, p=p, outlet=_three_way(p, hi, lo, delta),
                       detail={"noul": p, "delta": delta})
    if primitive == "choice":
        probs = {k: float(v) for k, v in answer["probabilities"].items()}
        opt = answer.get("choice") or max(sorted(probs), key=lambda k: probs[k])
        p = probs.get(opt, 0.0)
        return Reading(value=opt, p=p, outlet=_three_way(p, hi, lo, delta),
                       detail={"option": opt, "probabilities": probs, "delta": delta,
                               "confidence": answer.get("confidence")})
    if primitive == "score":
        probs = {str(k): float(v) for k, v in answer["probabilities"].items()}
        n = len(probs) or len(answer.get("legend", {})) or 1
        lvl = score_level(answer["score"], n)
        p = probs.get(str(lvl), 0.0)
        return Reading(value=lvl, p=p, outlet=_three_way(p, hi, lo, delta),
                       detail={"level": lvl, "score": float(answer["score"]), "delta": delta,
                               "probabilities": probs, "confidence": answer.get("confidence")})
    raise ValueError(f"未知 primitive：{primitive}")


def read_code(result: Any) -> Reading:
    """code 眼直接返回出口，或 (出口, detail)。"""
    detail: dict = {}
    if isinstance(result, tuple):
        outlet, detail = result[0], (result[1] or {})
    else:
        outlet = result
    if outlet is True:
        outlet = ACT
    elif outlet is False or outlet is None:
        outlet = IGNORE
    if outlet not in (ACT, IGNORE, UNSURE):
        raise ValueError(f"code 眼返回了非法出口：{outlet!r}")
    return Reading(value=outlet, p=1.0 if outlet == ACT else 0.0, outlet=outlet, detail=dict(detail))
