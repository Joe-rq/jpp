"""账本（三表）与料库（§2.10、§5）。

- 账本键（程序局部）：(model_id, 状态结构哈希, q_text_hash, phys, render_version, perm_seed, run_seq, site)
- 缓存键（跨程序）：(状态结构哈希, q_text_hash, phys, render_version, model_id)
- 账本头：budget、profile_hash、model_id、render_version、handler 版本、retry 策略（J-18）
- 料库：只增 Mat 存储 + 来源链，用 core.log.Log 落 JSONL。
"""

from __future__ import annotations

import json
import os
from typing import Any

from foundation.core.canon import H, canon
from foundation.core.ledger import Ledger
from foundation.core.log import Log

from .ir import Mat

HANDLER_VERSION = "h0.1"


def ledger_key(model_id: str, state_hash: str, q_hash: str, phys: str, render_version: str,
               perm_seed: int, run_seq: int, site: str) -> str:
    return H("judge", model_id, state_hash, q_hash, phys, render_version, perm_seed, run_seq, site)


def cache_key(state_hash: str, q_hash: str, phys: str, render_version: str, model_id: str) -> str:
    return H("cache", state_hash, q_hash, phys, render_version, model_id)


def effect_key(kind: str, site: str, *parts: Any) -> str:
    return H(kind, site, *parts)


class Books:
    """账本 + 缓存 + 头，一个目录。"""

    def __init__(self, root: str | None, header: dict):
        self.root = root
        self.header = header
        self.header_warning: str | None = None
        if root:
            os.makedirs(root, exist_ok=True)
            hp = os.path.join(root, "header.json")
            if os.path.exists(hp):
                with open(hp, encoding="utf-8") as fh:
                    old = json.load(fh)
                diff = {k: (old.get(k), header.get(k)) for k in set(old) | set(header) if old.get(k) != header.get(k)}
                if diff:
                    self.header_warning = f"W-header: 账本头不同，不承诺重放一致：{diff}"
            with open(hp, "w", encoding="utf-8") as fh:
                json.dump(header, fh, ensure_ascii=False, sort_keys=True, indent=1)
        self.ledger = Ledger(os.path.join(root, "ledger.json") if root else None)
        self.cache = Ledger(os.path.join(root, "cache.json") if root else None)
        self.effects = Ledger(os.path.join(root, "effects.json") if root else None)   # do/gen/ask/transform

    def save(self) -> None:
        self.ledger.save()
        self.cache.save()
        self.effects.save()


class MatStore:
    """料库：只增。每条 {hash, content, addr, modality, origin, taint, derived_from}。"""

    def __init__(self, path: str | None):
        self.path = path
        self._log = Log(path) if path else None
        self._seen: set[str] = set()
        if self._log:
            for ev in self._log.events():
                if ev.get("t") == "mat":
                    self._seen.add(ev["hash"])

    def add(self, m: Mat, site: str = "") -> str:
        h = m.hash
        if h in self._seen:
            return h
        self._seen.add(h)
        if self._log:
            self._log.emit("mat", hash=h, content=m.content, addr=m.addr, modality=m.modality,
                           origin=list(m.origin), taint=m.taint, derived_from=sorted(m.derived_from),
                           render_version=m.render_version, site=site)
        return h

    def get(self, h: str) -> Mat | None:
        if not self._log:
            return None
        for ev in self._log.events():
            if ev.get("t") == "mat" and ev["hash"] == h:
                return Mat(content=ev["content"], addr=ev["addr"], modality=ev["modality"],
                           origin=tuple(ev["origin"]), taint=ev["taint"],
                           derived_from=frozenset(ev["derived_from"]), render_version=ev["render_version"])
        return None

    def __len__(self):
        return len(self._seen)

    def close(self):
        if self._log:
            self._log.close()
