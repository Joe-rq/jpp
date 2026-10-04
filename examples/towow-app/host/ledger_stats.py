"""账本统计（只读 runs/<run>/ledger.jsonl）：按常驻程序归属尝试数、判断调用数、题数，以及错误、截断。
调用归属：一次 batch 记给其中第一道题的请求单元（与引擎「付钱方是第一个要求者」一致）。
用法：.venv/bin/python -m host.ledger_stats runs/<run>/ledger.jsonl"""
from __future__ import annotations

import collections
import json
import os
import re
import sys


def resident(uid: str) -> str:
    return re.sub(r"[#(].*", "", uid or "?")


def stats(path: str) -> dict:
    attempts = collections.Counter()
    errors = collections.Counter()
    kinds = collections.Counter()
    key_unit: dict[str, str] = {}
    batch_keys: dict = collections.defaultdict(list)
    batches = {}
    for line in open(path, encoding="utf-8"):
        try:
            r = json.loads(line)
        except json.JSONDecodeError:
            continue
        k = r.get("kind")
        kinds[k] += 1
        if k == "attempt":
            attempts[resident(r.get("unit"))] += 1
        elif k == "error":
            errors[resident(r.get("unit"))] += 1
        elif k in ("exit", "unsure_route") and r.get("key"):
            key_unit.setdefault(r["key"], resident(r.get("unit")))
        elif k == "judge":
            batch_keys[r.get("batch")].append(r.get("key"))
        elif k == "batch":
            batches[r.get("id")] = r
    calls = collections.Counter()
    qs = collections.Counter()
    for bid, keys in batch_keys.items():
        owners = [key_unit.get(x, "?") for x in keys]
        calls[owners[0] if owners else "?"] += 1
        for o in owners:
            qs[o] += 1
    return {"ledger_mb": round(os.path.getsize(path) / 1e6, 1), "records": dict(kinds.most_common()),
            "attempts_by_resident": dict(attempts.most_common()), "calls_by_resident": dict(calls.most_common()),
            "questions_by_resident": dict(qs.most_common()), "errors_by_resident": dict(errors.most_common()),
            "batches": len(batches)}


if __name__ == "__main__":
    print(json.dumps(stats(sys.argv[1]), ensure_ascii=False, indent=1))
