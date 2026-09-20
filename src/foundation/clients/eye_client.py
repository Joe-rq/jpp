"""Jev 眼客户端（施工单 §3.10）。

record 真调 HTTP（POST /v1/systemone，model 固定 jev-1.13.0，429/529 指数退避）；
replay 只从旧 Log 的 ask 事件取，取不到就报错。
`ask` 被心跳放在线程池里调用，所以它自己不写 Log、不碰账本——写 Log 与写账本都在主线程。
"""

from __future__ import annotations

import json
import os
import threading
import time
import urllib.error
import urllib.request
from dataclasses import dataclass, field
from typing import Any, Callable

from foundation.core.canon import H
from foundation.core.params import Params

API_URL = "https://api.typesafe.ai/v1/systemone"
KEY_FILE = "~/.typesafe-key"


def read_key() -> str:
    with open(os.path.expanduser(KEY_FILE), encoding="utf-8") as fh:
        return fh.read().strip()


def ask_key(view_fp: str, qfps: dict[str, str]) -> str:
    """replay 的匹配键：视野指纹 + 这一批问题的指纹集合。"""
    return H("ask", view_fp, sorted(qfps.values()))


@dataclass
class AskResult:
    answers: dict[str, dict]
    request: dict
    response: dict
    input_tokens: int = 0
    cost_usd: float = 0.0
    replayed: bool = False


class ReplayMiss(Exception):
    pass


class EyeClient:
    def __init__(self, mode: str = "record", params: Params | None = None,
                 replay_events: list[dict] | None = None,
                 transport: Callable[[dict], dict] | None = None):
        if mode not in ("record", "replay"):
            raise ValueError("mode 只能是 record | replay")
        from foundation.core.params import DEFAULTS
        self.mode = mode
        self.params = params or DEFAULTS
        self.transport = transport
        self._lock = threading.Lock()
        self.calls = 0
        self.input_tokens = 0
        self.cost_usd = 0.0
        self._replay: dict[str, list[dict]] = {}
        for ev in replay_events or []:
            if ev.get("t") == "ask" and not ev.get("cache_hit") and ev.get("response"):
                qfps = ev.get("question_fps") or {}
                if isinstance(qfps, dict):
                    self._replay.setdefault(ask_key(ev["view_fp"], qfps), []).append(ev)

    def ask(self, view_fp: str, view: Any, questions: dict[str, dict],
            qfps: dict[str, str]) -> AskResult:
        body = {"state": view, "model": self.params.model_version,
                "questions": {k: questions[k] for k in sorted(questions)}}
        if self.mode == "replay":
            key = ask_key(view_fp, qfps)
            with self._lock:
                bucket = self._replay.get(key)
                if not bucket:
                    raise ReplayMiss(f"replay 取不到这次调用：view_fp={view_fp} "
                                     f"问题={sorted(questions)}")
                ev = bucket.pop(0)
            resp = ev["response"]
            res = AskResult(answers=dict(resp.get("answers") or {}),
                            request=ev.get("request") or body, response=resp,
                            input_tokens=int(ev.get("input_tokens") or 0),
                            cost_usd=float(ev.get("cost") or 0.0), replayed=True)
        else:
            resp = (self.transport or _http_post)(body)
            tok = int((resp.get("usage") or {}).get("input_tokens", 0))
            res = AskResult(answers=dict(resp.get("answers") or {}), request=body,
                            response=resp, input_tokens=tok,
                            cost_usd=round(tok * self.params.usd_per_input_token, 8))
        missing = [q for q in questions if q not in res.answers]
        if missing:
            raise RuntimeError(f"Jev 没有回答这些问题：{missing}")
        with self._lock:
            self.calls += 1
            self.input_tokens += res.input_tokens
            self.cost_usd += res.cost_usd
        return res


def _http_post(body: dict, timeout: int = 120) -> dict:
    data = json.dumps(body, ensure_ascii=False).encode("utf-8")
    req = urllib.request.Request(
        API_URL, data=data,
        headers={"Authorization": "Bearer " + read_key(), "Content-Type": "application/json"})
    last: Exception | None = None
    for attempt in range(5):
        try:
            with urllib.request.urlopen(req, timeout=timeout) as r:
                return json.load(r)
        except urllib.error.HTTPError as e:
            last = e
            if e.code in (429, 500, 502, 503, 529):
                time.sleep(2 ** attempt)
                continue
            raise
        except Exception as e:
            last = e
            time.sleep(2 ** attempt)
    raise RuntimeError(f"Jev 调用失败：{last}")
