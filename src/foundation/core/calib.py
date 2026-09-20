"""校准集与两条线（施工单 §3.4）。

闸门只管上不上岗，工作点由校准集定（红队 A1）。人答进校准集，不进验题集（A2）。
两条线只看这个单元自己的读数，永远不与别的单元比较（S4.2）。
"""

from __future__ import annotations

import json
import os
from dataclasses import dataclass
from statistics import median

from foundation.core.params import Params

ACT, IGNORE = "act", "ignore"


@dataclass
class Lines:
    hi: float
    lo: float
    calibrated: bool = False
    degenerate: bool = False
    n: int = 0
    act_error: float | None = None      # {p ≥ hi} 里 label=ignore 的比例
    ignore_miss: float | None = None    # {p ≤ lo} 里 label=act 的比例

    def to_dict(self) -> dict:
        return {"hi": self.hi, "lo": self.lo, "calibrated": self.calibrated,
                "degenerate": self.degenerate, "n": self.n,
                "act_error": self.act_error, "ignore_miss": self.ignore_miss}


def calib_path(calib_dir: str, unit_name: str) -> str:
    return os.path.join(calib_dir, f"{unit_name}.jsonl")


def append_calib(calib_dir: str, unit_name: str, record: dict) -> dict:
    os.makedirs(calib_dir, exist_ok=True)
    rec = {"view_fp": record.get("view_fp", ""), "p": float(record.get("p") or 0.0),
           "label": record["label"], "source": record.get("source", "human")}
    with open(calib_path(calib_dir, unit_name), "a", encoding="utf-8") as fh:
        fh.write(json.dumps(rec, sort_keys=True, ensure_ascii=False,
                            separators=(",", ":")) + "\n")
    return rec


def load_calib(calib_dir: str, unit_name: str) -> list[dict]:
    path = calib_path(calib_dir, unit_name)
    if not os.path.exists(path):
        return []
    out = []
    with open(path, encoding="utf-8") as fh:
        for line in fh:
            line = line.strip()
            if line:
                out.append(json.loads(line))
    return out


def _rate(records: list[dict], keep, bad_label: str) -> float:
    region = [r for r in records if keep(float(r["p"]))]
    if not region:                       # 空区间的错误率显式定义为 0
        return 0.0
    return sum(1 for r in region if r["label"] == bad_label) / len(region)


def compute_lines(records: list[dict], params: Params, safety: bool = False) -> Lines:
    """n < 20 用保守默认并标未校准；n ≥ 20 在观测到的 p 上扫描（§3.4）。"""
    n = len(records)
    d_hi = params.safety_default_hi if safety else params.default_hi
    d_lo = params.safety_default_lo if safety else params.default_lo
    if n < params.calib_min_n:
        return Lines(hi=d_hi, lo=d_lo, calibrated=False, degenerate=False, n=n)

    ps = sorted({round(float(r["p"]), 6) for r in records})

    hi = 1.0
    for cand in ps + [1.0]:
        if _rate(records, lambda p, c=cand: p >= c, IGNORE) <= params.eps_act:
            hi = cand
            break
    lo = 0.0
    for cand in sorted(ps + [0.0], reverse=True):
        if _rate(records, lambda p, c=cand: p <= c, ACT) <= params.eps_ignore:
            lo = cand
            break

    degenerate = False
    if hi <= lo:
        m = float(median([float(r["p"]) for r in records]))
        hi = lo = m
        degenerate = True

    return Lines(hi=float(hi), lo=float(lo), calibrated=True, degenerate=degenerate, n=n,
                 act_error=_rate(records, lambda p: p >= hi, IGNORE),
                 ignore_miss=_rate(records, lambda p: p <= lo, ACT))


def should_suspend(records: list[dict], lines: Lines, params: Params,
                   working: Lines | None = None) -> tuple[bool, str]:
    """停岗：校准集 ≥20 且 act 区错误率 > 2ε_act，或 ignore 区漏放率 > 2ε_ignore。

    错误率要按单元**当时真正在用的**两条线算（working）。新扫出来的线按构造必然
    满足 ε，拿新线自查等于永远不会停岗——那条保险就是空的。没给 working 就退回新线。
    """
    if len(records) < params.calib_min_n:
        return False, ""
    ref = working or lines
    act_err = _rate(records, lambda p: p >= ref.hi, IGNORE)
    ign_miss = _rate(records, lambda p: p <= ref.lo, ACT)
    if act_err > 2 * params.eps_act:
        return True, f"act 区错误率 {act_err:.2f} > {2 * params.eps_act:.2f}"
    if ign_miss > 2 * params.eps_ignore:
        return True, f"ignore 区漏放率 {ign_miss:.2f} > {2 * params.eps_ignore:.2f}"
    return False, ""
