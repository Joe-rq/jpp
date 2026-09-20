"""裁决（施工单 §3.6）。

只比较单元的层与优先级，绝不比较读数（S4.2：这里不存在任何 p 的跨单元比较）。
"""

from __future__ import annotations

from dataclasses import dataclass, field

from foundation.core.hand import Proposal


@dataclass
class Arbitration:
    winners: list[Proposal] = field(default_factory=list)
    records: list[dict] = field(default_factory=list)   # 写 Log 的 arbitration 事件
    ties: list[list[Proposal]] = field(default_factory=list)


def _conflict_groups(proposals: list[Proposal]) -> list[list[Proposal]]:
    """冲突 = 两个提议 supersede 同一 item；或两个 mark 在彼此的 exclusive_with 里。"""
    groups: list[list[Proposal]] = []
    by_target: dict[str, list[Proposal]] = {}
    for p in proposals:
        if p.target:
            by_target.setdefault(p.target, []).append(p)
    for target in sorted(by_target):
        g = by_target[target]
        if len(g) > 1:
            groups.append(sorted(g, key=lambda p: p.key()))

    marks = [p for p in proposals if p.hand_type == "mark" and not p.downgraded]
    for i in range(len(marks)):
        for j in range(i + 1, len(marks)):
            a, b = marks[i], marks[j]
            if a.item.id != b.item.id:
                continue
            ex_a = set(a.unit.exclusive_with)
            ex_b = set(b.unit.exclusive_with)
            if (b.label in ex_a or b.unit.name in ex_a
                    or a.label in ex_b or a.unit.name in ex_b):
                groups.append(sorted([a, b], key=lambda p: p.key()))
    return groups


def arbitrate(proposals: list[Proposal]) -> Arbitration:
    out = Arbitration()
    groups = _conflict_groups(proposals)
    losers: set[int] = set()
    tied: set[int] = set()

    for g in groups:
        best = min(g, key=lambda p: (p.unit.layer_rank, -p.unit.priority))
        rank = (best.unit.layer_rank, -best.unit.priority)
        same = [p for p in g if (p.unit.layer_rank, -p.unit.priority) == rank]
        if len(same) > 1:                      # 同层同优先级：双方都不执行
            for p in same:
                tied.add(id(p))
            for p in g:
                if id(p) not in tied:
                    losers.add(id(p))
            out.ties.append(sorted(same, key=lambda p: p.key()))
            out.records.append({"winner": None, "reason": "tie",
                                "tied": [p.unit.name for p in sorted(same, key=lambda p: p.key())],
                                "losers": [p.unit.name for p in g if id(p) in losers],
                                "target": g[0].target, "item": g[0].item.id})
        else:
            for p in g:
                if p is not best:
                    losers.add(id(p))
            reason = ("layer" if any(p.unit.layer_rank != best.unit.layer_rank for p in g)
                      else "priority")
            out.records.append({"winner": best.unit.name, "reason": reason,
                                "losers": [p.unit.name for p in g if p is not best],
                                "target": g[0].target, "item": g[0].item.id})

    out.winners = sorted([p for p in proposals if id(p) not in losers and id(p) not in tied],
                         key=lambda p: p.key())
    return out
