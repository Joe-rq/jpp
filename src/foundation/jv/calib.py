"""校准记录库（§2.3、§5 校准库）。线只从记录来（I4）。

记录 = {key, hi, lo, n, status ∈ {冷, 上岗, 停岗, 待真值}, set_id, delta?, cost_matrix?, source}
状态「冷」= 无标注：cut 给 Unsure(cold)，handler 才能按保守线放行并打标。
"""

from __future__ import annotations

import json
import os
from dataclasses import dataclass, field

STATUSES = ("冷", "上岗", "停岗", "待真值")


@dataclass
class CalibRecord:
    key: str
    hi: float = 0.65
    lo: float = 0.35
    n: int = 0
    status: str = "冷"
    set_id: str = ""
    delta: float | None = None          # 覆盖档案 δ
    cost_matrix: dict | None = None     # {"fp": …, "fn": …}
    source: str = ""
    drift_stat: float = 0.0
    unsure_rate: float | None = None    # 标注集上实测的 unsure 率（J-10 联合上界用；None = 未知，计划期留符号）

    def to_dict(self) -> dict:
        return {k: v for k, v in self.__dict__.items()}

    @staticmethod
    def from_dict(d: dict) -> "CalibRecord":
        return CalibRecord(**{k: d.get(k, getattr(CalibRecord, k, None)) for k in CalibRecord.__dataclass_fields__})


class CalibStore:
    """目录下每键一个 JSON；进程内缓存。"""

    def __init__(self, path: str | None = None):
        self.path = path
        self._d: dict[str, CalibRecord] = {}
        if path and os.path.isdir(path):
            for fn in os.listdir(path):
                if fn.endswith(".json"):
                    with open(os.path.join(path, fn), encoding="utf-8") as fh:
                        rec = CalibRecord.from_dict(json.load(fh))
                        self._d[rec.key] = rec

    def get(self, key: str) -> CalibRecord:
        rec = self._d.get(key)
        if rec is None:
            rec = CalibRecord(key=key, status="冷")
            self._d[key] = rec
        return rec

    def put(self, key: str, **fields) -> CalibRecord:
        """只由校准过程（标注集 / 保形 / 人答）写；程序里不可调用（J-03 的运行期面）。"""
        if "status" in fields and fields["status"] not in STATUSES:
            raise ValueError(f"status 只能是 {STATUSES}")
        if fields.get("status") == "上岗" and int(fields.get("n", 0)) <= 0:
            raise ValueError("上岗记录必须带 n > 0（线只从标注记录来）")
        rec = CalibRecord(key=key, **fields)
        self._d[key] = rec
        if self.path:
            os.makedirs(self.path, exist_ok=True)
            with open(os.path.join(self.path, _safe(key) + ".json"), "w", encoding="utf-8") as fh:
                json.dump(rec.to_dict(), fh, ensure_ascii=False, sort_keys=True, indent=1)
        return rec

    def keys(self) -> list[str]:
        return sorted(self._d)


def _safe(key: str) -> str:
    return "".join(c if c.isalnum() or c in "._-" else "_" for c in key)


class FitRegistry:
    """fit 注册表（§2.9）。只认注册记录；注册约束 1–3 在 register 里核。"""

    def __init__(self):
        self._d: dict[str, dict] = {}

    def register(self, name: str, *, trained_from: str, features: list[tuple[str, str]],
                 n: int, error_rate: float, fn, version: str = "1") -> dict:
        k = len(features)
        if n < max(50, 20 * k):
            raise ValueError(f"J-16: fit {name} 样本 n={n} < max(50, 20×{k})；样本不足退化为投票，不注册。")
        if not trained_from:
            raise ValueError("J-16: fit 必须记 trained_from（训练集 id）")
        rec = {"name": name, "trained_from": trained_from, "features": [tuple(f) for f in features],
               "n": n, "error_rate": error_rate, "fn": fn, "version": version}
        self._d[name] = rec
        return rec

    def get(self, name: str) -> dict | None:
        return self._d.get(name)

    def has(self, name: str) -> bool:
        return name in self._d
