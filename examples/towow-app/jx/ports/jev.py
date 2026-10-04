"""JEV 端口：只做传输（一次调用 = 一个 state + 多道题）。合批、缓存、预算、缺席都在运行时。

协议：POST https://api.typesafe.ai/v1/systemone
  {model, state:{具名 JSON 槽}, questions:{qN:{type: noul|choice|score, instructions, criteria?}}}
密钥只从 ~/.typesafe-key 读，不进日志。
"""
from __future__ import annotations

import asyncio
import json
import os
import time
from dataclasses import dataclass, field

import aiohttp

URL = "https://api.typesafe.ai/v1/systemone"
PRICE_PER_INPUT_TOKEN = 4.2e-8  # 画像 jev-1.13.0 cost.price_usd_per_input_token


class JevAbsent(Exception):
    """判断器缺席（超时、网络、5xx 退避用尽）。运行时把它变成 Unsure(absent)。"""


@dataclass
class WireQ:
    """线上的一道题。op: test|select|measure。criteria: select 为 {标签: 描述}，measure 为有序档位列表。"""
    op: str
    text: str
    criteria: object = None

    def wire(self) -> dict:
        t = {"test": "noul", "select": "choice", "measure": "score"}[self.op]
        o = {"type": t, "instructions": self.text}
        if self.criteria is not None:
            o["criteria"] = self.criteria
        return o


@dataclass
class JevStats:
    calls: int = 0
    questions: int = 0
    input_tokens: int = 0
    failures: int = 0
    latency_s: list = field(default_factory=list)

    @property
    def cost_usd(self) -> float:
        return self.input_tokens * PRICE_PER_INPUT_TOKEN


class JevPort:
    def __init__(self, model: str = "jev-1.13.0", concurrency: int = 32, timeout_s: float = 20.0):
        self.model = model
        self.sem = asyncio.Semaphore(concurrency)
        self.timeout = aiohttp.ClientTimeout(total=timeout_s)
        self.stats = JevStats()
        self._key = open(os.path.expanduser("~/.typesafe-key")).read().strip()
        self._session: aiohttp.ClientSession | None = None

    async def _sess(self) -> aiohttp.ClientSession:
        if self._session is None or self._session.closed:
            self._session = aiohttp.ClientSession(
                timeout=self.timeout,
                headers={"Authorization": f"Bearer {self._key}", "content-type": "application/json"},
                connector=aiohttp.TCPConnector(limit=64),
            )
        return self._session

    async def close(self):
        if self._session and not self._session.closed:
            await self._session.close()

    async def call(self, state: dict, qs: list[WireQ]) -> list[dict]:
        """返回与 qs 对齐的原始答案（{type, noul|probabilities...}）。失败抛 JevAbsent。"""
        body = {"model": self.model, "state": state,
                "questions": {f"q{i}": q.wire() for i, q in enumerate(qs)}}
        dump = os.environ.get("JX_DUMP_WIRE")          # 诊断：把前 N 个请求体写到文件（不含密钥）
        if dump and self.stats.calls < int(os.environ.get("JX_DUMP_N", "20")):
            with open(dump, "a") as f:
                f.write(json.dumps(body, ensure_ascii=False) + "\n")
        async with self.sem:
            sess = await self._sess()
            last = ""
            for attempt in range(5):
                t0 = time.monotonic()
                try:
                    async with sess.post(URL, data=json.dumps(body, ensure_ascii=False)) as r:
                        if r.status in (429, 500, 502, 503, 529):
                            last = f"HTTP {r.status}"
                            await asyncio.sleep(min(8, 0.5 * 2 ** attempt))
                            continue
                        if r.status != 200:
                            txt = await r.text()
                            self.stats.failures += 1
                            raise JevAbsent(f"HTTP {r.status}: {txt[:300]}")
                        resp = await r.json()
                except (aiohttp.ClientError, asyncio.TimeoutError) as e:
                    last = f"{type(e).__name__}: {e}"
                    await asyncio.sleep(min(8, 0.5 * 2 ** attempt))
                    continue
                dt = time.monotonic() - t0
                self.stats.calls += 1
                self.stats.questions += len(qs)
                self.stats.input_tokens += int(resp.get("usage", {}).get("input_tokens", 0))
                self.stats.latency_s.append(dt)
                ans = resp.get("answers", {})
                out = []
                for i in range(len(qs)):
                    a = ans.get(f"q{i}")
                    if a is None:
                        raise JevAbsent(f"返回体缺 q{i}")
                    out.append(a)
                if out:   # 本次调用自己的用量（并发时不能用全局计数前后相减）
                    out[0] = dict(out[0], _usage_input_tokens=int(resp.get("usage", {}).get("input_tokens", 0)))
                return out
            self.stats.failures += 1
            raise JevAbsent(f"JEV 调用失败：{last}")


def reading_of(op: str, raw: dict, labels: list | None = None) -> dict:
    """把原始答案变成读数：test → {p}; select/measure → {dist: {label: p}}。"""
    if op == "test":
        return {"p": float(raw["noul"])}
    probs = raw.get("probabilities", {})
    if op == "measure":
        # 档位从 0 起编号，与 criteria 列表下标对齐
        n = len(labels or probs)
        return {"dist": [float(probs.get(str(i), 0.0)) for i in range(n)]}
    return {"dist": {k: float(v) for k, v in probs.items()}}
