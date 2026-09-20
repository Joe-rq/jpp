"""Log：唯一真相，JSONL 只追加（施工单 §2.2）。

写 Log 只在主线程做，顺序由心跳决定，与 Jev 返回先后无关。
"""

from __future__ import annotations

import json
import os
from typing import Any, Iterator

from foundation.core.canon import canon


class Log:
    def __init__(self, path: str, append: bool = True):
        self.path = path
        os.makedirs(os.path.dirname(os.path.abspath(path)), exist_ok=True)
        if not append and os.path.exists(path):
            os.remove(path)
        self._seq = 0
        if os.path.exists(path):
            for ev in read_log(path):
                self._seq = max(self._seq, ev.get("seq", 0))
        self._fh = open(path, "a", encoding="utf-8")

    def emit(self, t: str, **fields: Any) -> dict:
        self._seq += 1
        ev = {"seq": self._seq, "t": t}
        ev.update(fields)
        self._fh.write(canon(ev) + "\n")
        self._fh.flush()
        return ev

    def close(self) -> None:
        if not self._fh.closed:
            self._fh.close()

    def __enter__(self) -> "Log":
        return self

    def __exit__(self, *exc) -> None:
        self.close()

    def events(self) -> list[dict]:
        self._fh.flush()
        return list(read_log(self.path))


def read_log(path: str) -> Iterator[dict]:
    if not os.path.exists(path):
        return
    with open(path, encoding="utf-8") as fh:
        for line in fh:
            line = line.strip()
            if line:
                yield json.loads(line)
