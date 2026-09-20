"""Ledger：问过的问题不再问（施工单 §2.3）。

键 = H(view_fp, question_fp, model_version[, batch_fp])。batch_fp 由 params 开关控制：
E2「并排隔离」若不成立，键必须含批次指纹（红队 A14）。
"""

from __future__ import annotations

import json
import os

from foundation.core.canon import H


def ledger_key(view_fp: str, question_fp: str, model_version: str,
               batch_fp: str | None = None, include_batch: bool = False) -> str:
    if include_batch:
        return H(view_fp, question_fp, model_version, batch_fp)
    return H(view_fp, question_fp, model_version)


class Ledger:
    """JSON 文件存储；进程内保持全量字典，写时落盘。"""

    def __init__(self, path: str | None = None):
        self.path = path
        self._d: dict[str, dict] = {}
        self.hits = 0
        self.misses = 0
        if path and os.path.exists(path):
            with open(path, encoding="utf-8") as fh:
                self._d = json.load(fh)

    def get(self, key: str) -> dict | None:
        v = self._d.get(key)
        if v is None:
            self.misses += 1
        else:
            self.hits += 1
        return v

    def put(self, key: str, answer: dict) -> None:
        self._d[key] = answer

    def save(self) -> None:
        if not self.path:
            return
        os.makedirs(os.path.dirname(os.path.abspath(self.path)), exist_ok=True)
        with open(self.path, "w", encoding="utf-8") as fh:
            json.dump(self._d, fh, sort_keys=True, ensure_ascii=False, separators=(",", ":"))

    def __len__(self) -> int:
        return len(self._d)

    @property
    def hit_rate(self) -> float:
        total = self.hits + self.misses
        return self.hits / total if total else 0.0
