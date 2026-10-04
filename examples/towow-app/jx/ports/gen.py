"""生成器端口：`claude -p`（无 API key 时唯一可用的大模型）。产物按键缓存，跨运行复用（B149、B151）。

慢（每次 ~10–15s）且限流，所以只在程序结构需要新东西时调用；运行时把它放在非阻塞端口上，
生成期间判断照常调度（意图汇编 7）。
"""
from __future__ import annotations

import asyncio
import hashlib
import json
import os
import re
import sqlite3
import time

SCRATCH = os.environ.get("JX_GEN_CWD", "/private/tmp")


class GenFail(Exception):
    pass


class GenPort:
    def __init__(self, cache_path: str, model: str = "sonnet", concurrency: int = 6, timeout_s: float = 240):
        self.model = model
        self.sem = asyncio.Semaphore(concurrency)
        self.timeout_s = timeout_s
        os.makedirs(os.path.dirname(cache_path) or ".", exist_ok=True)
        self.db = sqlite3.connect(cache_path, check_same_thread=False)
        self.db.execute("create table if not exists gen(k text primary key, model text, prompt text, out text, t real)")
        self.calls = 0
        self.hits = 0
        self.seconds = 0.0

    def key(self, prompt: str, model: str | None = None) -> str:
        return hashlib.sha256(((model or self.model) + "\x00" + prompt).encode()).hexdigest()

    def cached(self, prompt: str, model: str | None = None):
        row = self.db.execute("select out from gen where k=?", (self.key(prompt, model),)).fetchone()
        return row[0] if row else None

    async def text(self, prompt: str, model: str | None = None) -> str:
        m = model or self.model
        k = self.key(prompt, m)
        row = self.db.execute("select out from gen where k=?", (k,)).fetchone()
        if row:
            self.hits += 1
            return row[0]
        async with self.sem:
            t0 = time.monotonic()
            proc = await asyncio.create_subprocess_exec(
                "claude", "-p", "--model", m, "--output-format", "text",
                stdin=asyncio.subprocess.PIPE, stdout=asyncio.subprocess.PIPE,
                stderr=asyncio.subprocess.PIPE, cwd=SCRATCH)
            try:
                out, err = await asyncio.wait_for(proc.communicate(prompt.encode()), self.timeout_s)
            except asyncio.TimeoutError:
                proc.kill()
                raise GenFail("生成超时")
            self.seconds += time.monotonic() - t0
            self.calls += 1
            if proc.returncode != 0:
                raise GenFail(f"claude -p 退出码 {proc.returncode}: {err.decode()[-300:]}")
            txt = out.decode()
        self.db.execute("insert or replace into gen values(?,?,?,?,?)", (k, m, prompt, txt, time.time()))
        self.db.commit()
        return txt

    async def json(self, prompt: str, model: str | None = None):
        txt = await self.text(prompt + "\n\n只输出一个 JSON 值（可放在 ```json 代码块里），不要其他文字。", model)
        return parse_json_loose(txt)


def parse_json_loose(txt: str):
    m = re.search(r"```(?:json)?\s*(.*?)```", txt, re.S)
    body = m.group(1) if m else txt
    body = body.strip()
    try:
        return json.loads(body)
    except json.JSONDecodeError:
        # 取第一个 { 或 [ 到最后一个对应括号
        for o, c in (("{", "}"), ("[", "]")):
            i, j = body.find(o), body.rfind(c)
            if i != -1 and j > i:
                try:
                    return json.loads(body[i:j + 1])
                except json.JSONDecodeError:
                    pass
        raise GenFail("生成结果不是 JSON：" + txt[:200])


PLAN_PROMPT = """你是一个合作方案的起草人。网络里的判断已经认定下面这几个人（或组合）可能形成合作，形状是「{shape}」。
你只看到他们各自愿意公开、或已经向这组人释放的片段，看不到他们的全部生活。请据此写一份能直接拿去和他们商量的合作方案。

构型：
{config}

成员材料：
{members}

要求：
- 写清谁出什么、谁得到什么、分工、第一步做什么、节奏（第一周/第一个月）、风险与对策。
- 只用材料里有的事实；材料里没有、但方案需要的，列进「还需要确认」，不要编造。
- 不要给出任何概率或置信度数字（置信度由网络的判断给出，不由你写）。
- 用成员所用的语言（中文为主）；称呼成员用材料里的 display。

输出 JSON：{{"title": "一句话标题，≤24 字", "summary": "两三句话的方案摘要", "roles": {{"成员 id": "此人在方案里的角色与贡献"}},
"first_step": "...", "rhythm": ["第一周 ...", "第一个月 ..."], "gains": {{"成员 id": "此人得到什么"}}, "risks": ["..."], "to_confirm": ["..."]}}"""


def _plan_prompt(args):
    import json as _j
    cfg, members = (args + [None, None])[:2] if isinstance(args, list) else (args, [])
    cfg = cfg or {}
    slim = {k: cfg.get(k) for k in ("id", "shape", "members", "roles", "lacks") if isinstance(cfg, dict)}
    return PLAN_PROMPT.format(shape=slim.get("shape"), config=_j.dumps(slim, ensure_ascii=False, default=str)[:3000],
                              members=_j.dumps(members, ensure_ascii=False, default=str)[:9000])


async def _gen_json(self, kind, args):
    if kind == "plan":
        out = await self.json(_plan_prompt(args))
        if not isinstance(out, dict):
            raise GenFail("方案不是 JSON 对象")
        return out
    raise GenFail(f"未知的 gen_json 种类：{kind}")


GenPort.gen_json = _gen_json
