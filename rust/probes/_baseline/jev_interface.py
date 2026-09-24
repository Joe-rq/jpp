"""判断接口（手写基线用；A-6 计数规则里的「SDK」，两边都不计行数）。

这是手写 Python 基线直接调用的判断器接口，形状与 JEV 的 HTTP 接口
（POST https://api.typesafe.ai/v1/systemone）一一对应：一份材料（状态）上可以一次问多道题，
每道题返回一个概率分布。基线只通过 `judge` 取读数。

两个后端，由环境变量 JEV_BACKEND 选择：
  fixture（默认）  从夹具文件（环境变量 JEV_FIXTURE 指定路径）按「材料 + 题」精确查读数；
                   查不到就报错，不给缺省值（验收测试要求同一份读数得出同一输出）。
  live             真机：POST 到 JEV，钥匙只从 ~/.typesafe-key 读。

用法：

    from jev_interface import judge
    answers = judge(material, [
        {"type": "noul",   "instructions": "这段话是否……？"},
        {"type": "choice", "instructions": "最应归入哪一类？"},          # 候选放在 over
        {"type": "score",  "instructions": "是什么关系？", "criteria": ["档0", "档1", "档2"]},
    ], over=["候选0", "候选1"])
    answers[0]  -> {"noul": 0.93}                                  # 是的概率
    answers[1]  -> {"probabilities": {"c0": 0.9, "c1": 0.1}, "mode_share": 1.0}
                   # mode_share 仅在测过候选顺序置换时出现：1.0 = 换序后答案不变
    answers[2]  -> {"probabilities": {"0": 0.1, "1": 0.2, "2": 0.7}}

一次 `judge` 调用计一次调用（`calls()` 返回累计数），也计一轮（`rounds()`）。

T1 任务书另用一个接口（B79 (c)）：

    from jev_interface import judge_batch
    results = judge_batch([(material1, [q, ...], over1), (material2, [q, ...], None), ...])
    results[i]  -> 与第 i 份请求的 questions 同序的答案列表（同 judge 的返回值）

一次 `judge_batch` 把多份材料各自的题一起提交：计 `len(requests)` 次调用、1 轮。
`judge_batch([])` 不计调用也不计轮。

环境变量 JEV_REPORT_CALLS=1 时，进程退出时把累计调用数与轮数写到标准错误（`JEV_CALLS=<n>`、`JEV_ROUNDS=<n>`
各一行），供 `scripts/measure_expr.py --check` 取调用数比（B80）与 T1 验收；标准输出不受影响。
"""
from __future__ import annotations

import atexit
import json
import os
import sys
import time
import urllib.error
import urllib.request

API_URL = "https://api.typesafe.ai/v1/systemone"
MODEL = "jev-1.13.0"
_OPS = {"noul": "test", "choice": "select", "score": "measure"}
_calls = 0
_rounds = 0
_fixture_index = None


def calls() -> int:
    return _calls


def rounds() -> int:
    return _rounds


if os.environ.get("JEV_REPORT_CALLS") == "1":
    atexit.register(lambda: sys.stderr.write(f"JEV_CALLS={_calls}\nJEV_ROUNDS={_rounds}\n"))


def _canon(x) -> str:
    return json.dumps(x, ensure_ascii=False, sort_keys=True, separators=(",", ":"))


def _load_fixture():
    global _fixture_index
    if _fixture_index is None:
        path = os.environ.get("JEV_FIXTURE")
        if not path:
            raise RuntimeError("fixture 后端要环境变量 JEV_FIXTURE 指向夹具文件")
        with open(path, encoding="utf-8") as fh:
            fx = json.load(fh)
        _fixture_index = {}
        for o in fx["observations"]:
            key = (_canon(o["on"]), _canon(o.get("over", [])), o["op"], o["text"], _canon(o.get("scale", [])))
            _fixture_index[key] = o
    return _fixture_index


def _fixture_answer(material, q: dict, over) -> dict:
    op = _OPS[q["type"]]
    scale = q.get("criteria", []) if q["type"] == "score" else []
    key = (_canon([material]), _canon(over or []), op, q["instructions"], _canon(scale))
    o = _load_fixture().get(key)
    if o is None:
        raise LookupError(f"夹具里没有这道读数：题={q['instructions']!r} 材料={_canon(material)[:80]}…")
    a = o["answer"]
    if "Noul" in a:
        return {"noul": a["Noul"]}
    if "Choice" in a:
        out = {"probabilities": {f"c{k}": p for k, p in enumerate(a["Choice"])}}
        if o.get("mode_share") is not None:
            out["mode_share"] = o["mode_share"]
        return out
    return {"probabilities": {str(k): p for k, p in enumerate(a["Score"])}}


def _live(material, questions: list, over) -> list:
    with open(os.path.expanduser("~/.typesafe-key"), encoding="utf-8") as fh:
        key = fh.read().strip()
    state = {"on": material}
    if over:
        state["over"] = over
    qs = {}
    for i, q in enumerate(questions):
        body = {"type": q["type"], "instructions": q["instructions"]}
        if q["type"] == "choice":
            body["criteria"] = {f"c{k}": c for k, c in enumerate(over or [])}
        elif q["type"] == "score":
            body["criteria"] = q["criteria"]
        qs[f"q{i}"] = body
    data = json.dumps({"state": state, "model": MODEL, "questions": qs}, ensure_ascii=False).encode("utf-8")
    req = urllib.request.Request(API_URL, data=data, headers={
        "Authorization": "Bearer " + key, "Content-Type": "application/json"})
    last = None
    for attempt in range(5):
        try:
            with urllib.request.urlopen(req, timeout=120) as r:
                resp = json.load(r)
            return [resp["answers"][f"q{i}"] for i in range(len(questions))]
        except urllib.error.HTTPError as e:
            last = e
            if e.code in (429, 500, 502, 503, 529):
                time.sleep(2 ** attempt)
                continue
            raise
    raise RuntimeError(f"JEV 调用失败：{last}")


def _one(material, questions: list, over) -> list:
    if os.environ.get("JEV_BACKEND", "fixture") == "live":
        return _live(material, questions, over)
    return [_fixture_answer(material, q, over) for q in questions]


def judge(material, questions: list, over: list | None = None) -> list:
    """对一份材料问一道或多道题，返回与 questions 同序的答案列表。计 1 次调用、1 轮。"""
    global _calls, _rounds
    _calls += 1
    _rounds += 1
    return _one(material, questions, over)


def judge_batch(requests: list) -> list:
    """一次提交多份材料各自的题。requests 的每项是 (material, questions) 或 (material, questions, over)；
    返回与 requests 同序的答案列表的列表。计 len(requests) 次调用、1 轮（空列表不计）。"""
    global _calls, _rounds
    if not requests:
        return []
    _calls += len(requests)
    _rounds += 1
    out = []
    for r in requests:
        material, questions = r[0], r[1]
        over = r[2] if len(r) > 2 else None
        out.append(_one(material, questions, over))
    return out
