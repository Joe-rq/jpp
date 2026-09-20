"""五只手：mark / put / code / write / ask（施工单 §2.4 / §3.2）。

手只在裁决胜出之后才真正执行——写手很贵，被裁掉的提议不该花钱。
"""

from __future__ import annotations

from dataclasses import dataclass, field
from typing import Any

from foundation.core import registry
from foundation.core.item import Item, mark_body
from foundation.core.outlet import Reading

HAND_TYPES = ("mark", "put", "code", "write", "ask")
# 第六种手不在施工单 §2.4 的五只手里：它是"包"这个观察者的手（§3.9），
# 单元定义的 YAML 永远写不出它——只有 core/pack.py 的 PackObserver 带着它。
PACK_HAND = "pack"


@dataclass
class Proposal:
    unit: Any                       # Unit
    item: Item
    reading: Reading
    hand: dict
    downgraded: bool = False        # draft 单元：act 降级为 would_act 标记
    view: Any = None

    @property
    def hand_type(self) -> str:
        return "mark" if self.downgraded else self.hand.get("type", "mark")

    @property
    def label(self) -> str:
        if self.downgraded:
            return f"would_act:{self.unit.name}"
        return _label_for(self.hand.get("label", self.unit.name), self.reading)

    @property
    def target(self) -> str | None:
        """这条提议会取代谁（裁决用）。mark / ask 不取代任何东西。"""
        t = self.hand_type
        if t in ("put", "write"):
            return self.item.id if self.hand.get("supersedes", True) else None
        if t == "code":
            return self.item.id if self.hand.get("supersedes", True) else None
        # 包不取代自己的 inlet：inlet 被复制进实例分区，原件留在父分区桌面上。
        return None

    def key(self) -> tuple:
        return (self.unit.layer_rank, -self.unit.priority, self.unit.name, self.item.id)

    def to_dict(self) -> dict:
        return {"unit": self.unit.name, "item": self.item.id, "hand": self.hand_type,
                "label": self.label if self.hand_type == "mark" else None,
                "target": self.target, "outlet": self.reading.outlet,
                "p": self.reading.p, "value": self.reading.value,
                "downgraded": self.downgraded}


def _label_for(template: Any, reading: Reading) -> str:
    s = str(template)
    val = reading.value
    if isinstance(reading.detail, dict):
        if "option" in reading.detail:
            val = reading.detail["option"]
        elif "level" in reading.detail:
            val = reading.detail["level"]
    for token in ("{opt}", "{lvl}", "{value}", "{option}", "{level}"):
        s = s.replace(token, str(val))
    return s


@dataclass
class HandOutcome:
    drafts: list[dict] = field(default_factory=list)
    write_used: bool = False
    failed: str = ""
    detail: dict = field(default_factory=dict)


def execute(prop: Proposal, ctx) -> HandOutcome:
    """返回 Item 草稿列表（不含 made_by / scope / beat，由内核填）。"""
    t = prop.hand_type
    item = prop.item
    if t == "mark":
        return HandOutcome(drafts=[{"kind": "mark",
                                    "body": mark_body(prop.label, prop.unit.name),
                                    "about": item.id}])
    if t == "put":
        kind = prop.hand.get("kind", "note")
        tpl = prop.hand.get("body_template", "{body}")
        body = tpl.format(body=item.body) if isinstance(item.body, str) else item.body
        draft = {"kind": kind, "body": body}
        if prop.hand.get("supersedes", True):
            draft["supersedes"] = item.id
        out = HandOutcome(drafts=[draft])
        if prop.hand.get("ask"):
            out.drafts.append({"kind": "ask",
                               "body": {"question": prop.hand["ask"], "unit": prop.unit.name},
                               "about": item.id})
        return out
    if t == "code":
        fn = registry.lookup(prop.hand.get("fn") or f"{prop.unit.name}_hand")
        drafts = fn(item, prop.reading.detail, ctx) or []
        return HandOutcome(drafts=[dict(d) for d in drafts])
    if t == "write":
        instruction = prop.hand.get("instruction", "")
        view = prop.view if prop.view is not None else item.body
        res = ctx.writer.write(instruction, view if isinstance(view, str) else str(view))
        if not res.ok:
            return HandOutcome(write_used=True, failed=res.error or "写手失败",
                               detail={"instruction": instruction})
        kind = prop.hand.get("result_kind", item.kind)
        return HandOutcome(drafts=[{"kind": kind, "body": res.text, "supersedes": item.id}],
                           write_used=True,
                           detail={"instruction": instruction, "replayed": res.replayed,
                                   "text": res.text})
    if t == PACK_HAND:
        # 包与单元同接口：手的内容是"跑一个子分区到安静，把 outlets 与未吸收的
        # unsure 交上来"，返回的仍然是一串草稿，内核照常补 made_by / scope / beat。
        return HandOutcome(drafts=[dict(d) for d in (prop.unit.observe(item, ctx) or [])])
    if t == "ask":
        q = prop.hand.get("question", "请人判断")
        return HandOutcome(drafts=[{"kind": "ask",
                                    "body": {"question": q, "unit": prop.unit.name},
                                    "about": item.id}])
    raise ValueError(f"未知的手：{t}")
