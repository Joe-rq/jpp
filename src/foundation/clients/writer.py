"""写手（施工单 §3.10 / 红队 C 表）：后端是 `claude -p --model haiku`，本机订阅零 API 费。

record 真跑，replay 从旧 Log 的 write_call 事件取。超时 90 秒记失败，由心跳转成上交。
"""

from __future__ import annotations

import subprocess
import threading
from dataclasses import dataclass

from foundation.core.canon import H
from foundation.core.params import Params


def write_key(instruction: str, view) -> str:
    return H("write", instruction, view)


@dataclass
class WriteResult:
    ok: bool
    text: str = ""
    error: str = ""
    replayed: bool = False
    key: str = ""


class WriterReplayMiss(Exception):
    pass


class Writer:
    def __init__(self, mode: str = "record", params: Params | None = None,
                 replay_events: list[dict] | None = None, runner=None):
        if mode not in ("record", "replay"):
            raise ValueError("mode 只能是 record | replay")
        from foundation.core.params import DEFAULTS
        self.mode = mode
        self.params = params or DEFAULTS
        self.runner = runner                     # 测试注入：(instruction, view) -> WriteResult
        self.calls = 0
        self._lock = threading.Lock()
        self._replay: dict[str, list[dict]] = {}
        for ev in replay_events or []:
            if ev.get("t") == "write_call" and ev.get("write_fp"):
                self._replay.setdefault(ev["write_fp"], []).append(ev)

    def write(self, instruction: str, view) -> WriteResult:
        key = write_key(instruction, view)
        with self._lock:
            self.calls += 1
        if self.mode == "replay":
            bucket = self._replay.get(key)
            if not bucket:
                raise WriterReplayMiss(f"replay 取不到写手调用：{str(instruction)[:40]}…")
            ev = bucket.pop(0)
            return WriteResult(ok=bool(ev.get("ok")), text=ev.get("text") or "",
                               error=ev.get("error") or "", replayed=True, key=key)
        if self.runner is not None:
            res = self.runner(instruction, view)
            if isinstance(res, WriteResult):
                res.key = key
                return res
            return WriteResult(ok=True, text=str(res), key=key)
        return self._run(instruction, view, key)

    def _run(self, instruction: str, view, key: str) -> WriteResult:
        prompt = f"{instruction}\n\n---\n{view if isinstance(view, str) else str(view)}"
        try:
            proc = subprocess.run(
                ["claude", "-p", "--model", self.params.writer_model,
                 "--output-format", "text"],
                input=prompt, capture_output=True, text=True,
                timeout=self.params.writer_timeout_s)
        except subprocess.TimeoutExpired:
            return WriteResult(ok=False, error=f"写手超时 {self.params.writer_timeout_s}s", key=key)
        except FileNotFoundError:
            return WriteResult(ok=False, error="找不到 claude 可执行文件", key=key)
        if proc.returncode != 0:
            return WriteResult(ok=False, error=(proc.stderr or "")[:400].strip(), key=key)
        text = (proc.stdout or "").strip()
        if not text:
            return WriteResult(ok=False, error="写手返回空", key=key)
        return WriteResult(ok=True, text=text, key=key)
