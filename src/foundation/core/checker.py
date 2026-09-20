"""最小静态检查（红队 C 表，部分接受）：四项，不阻塞验收。"""

from __future__ import annotations

import os

from foundation.core.params import LAYER_RANK
from foundation.core.probe import load_probes, _counts_ok


def _produced_kinds(u) -> list[str]:
    h = u.hand or {}
    t = h.get("type")
    if t == "mark":
        return ["mark"]
    if t == "put":
        return [h.get("kind", "note")]
    if t == "write":
        return [h.get("result_kind")] if h.get("result_kind") else []
    if t == "ask":
        return ["ask"]
    return []          # code 手产什么要看函数，静态看不出来


def _supersedes(u) -> bool:
    return (u.hand or {}).get("type") in ("put", "write", "code") and \
        (u.hand or {}).get("supersedes", True)


def check_testbed(units, testbed_dir: str) -> list[dict]:
    out: list[dict] = []
    watched = {k for u in units for k in u.watch_kinds}
    produced = {k for u in units for k in _produced_kinds(u)}
    inlets = set()
    from foundation.core.pack import load_packs
    for d in load_packs(testbed_dir).values():
        # 包的收发口就是"这种东西有来处 / 有去处"的静态证据：inlet 算有人产出，
        # outlet 算有人盯着。多个包（pack.yaml + packs/*.yaml）全部算进来。
        inlets |= set(d.inlets)
        produced |= set(d.inlets)
        watched |= set(d.outlets)

    for u in units:
        for k in _produced_kinds(u):
            if k and k not in watched:
                out.append({"level": "warn", "rule": "悬空产出", "unit": u.name,
                            "message": f"{u.name} 产出 {k}，没有任何单元盯着它"})
        for k in u.watch_kinds:
            if k not in produced and k not in ("note.raw", "unsure", "ask", "mark"):
                out.append({"level": "warn", "rule": "无源之水", "unit": u.name,
                            "message": f"{u.name} 盯 {k}，但没有单元产出它，也不是 inlet"})
        if not u.watch_kinds and _supersedes(u):
            out.append({"level": "error", "rule": "不限种类取代", "unit": u.name,
                        "message": f"{u.name} 不限种类却会取代原物，等于对一切动手"})
        # 越权手（语言规范 §5.2 原文：draft 单元配了 mark 以外的手）。
        # 静态可判的那部分 draft：验题文件缺失或数量不够，这样的单元必然停在 draft。
        if (u.hand or {}).get("type") not in (None, "mark"):
            cases = load_probes(u, testbed_dir)
            enough, why = _counts_ok(u, cases)
            if not enough:
                out.append({"level": "error", "rule": "越权手", "unit": u.name,
                            "message": f"{u.name} 必然停在 draft（{why}），却配了 "
                                       f"{(u.hand or {}).get('type')} 手；draft 只许 mark"})

    for i, a in enumerate(units):
        for b in units[i + 1:]:
            same_rank = (LAYER_RANK.get(a.layer, 1), a.priority) == \
                        (LAYER_RANK.get(b.layer, 1), b.priority)
            if not same_rank or not (_supersedes(a) and _supersedes(b)):
                continue
            if set(a.watch_kinds) & set(b.watch_kinds):
                out.append({"level": "error", "rule": "同层同优先级冲突",
                            "unit": f"{a.name}/{b.name}",
                            "message": f"{a.name} 与 {b.name} 同层同优先级且可能取代同一种东西，"
                                       f"撞上就是双方都不执行"})
    return sorted(out, key=lambda f: (f["level"], f["rule"], f.get("unit", "")))
