"""Item：桌面上不可修改的一件东西（施工单 §2.1）。"""

from __future__ import annotations

from dataclasses import dataclass, field
from typing import Any

from foundation.core.canon import H


@dataclass(frozen=True)
class Item:
    kind: str
    body: Any                       # str | dict
    about: str | None = None
    supersedes: str | None = None
    made_by: str = "external"
    scope: str = "/"
    beat: int = 0                   # 不进 id

    @property
    def id(self) -> str:
        return H(self.kind, self.body, self.about, self.supersedes, self.made_by, self.scope)

    def to_dict(self) -> dict:
        return {
            "id": self.id,
            "kind": self.kind,
            "body": self.body,
            "about": self.about,
            "supersedes": self.supersedes,
            "made_by": self.made_by,
            "scope": self.scope,
            "beat": self.beat,
        }

    @staticmethod
    def from_dict(d: dict) -> "Item":
        return Item(
            kind=d["kind"],
            body=d["body"],
            about=d.get("about"),
            supersedes=d.get("supersedes"),
            made_by=d.get("made_by", "external"),
            scope=d.get("scope", "/"),
            beat=d.get("beat", 0),
        )


def draft_to_item(draft: dict, made_by: str, scope: str, beat: int) -> Item:
    """把手产出的草稿补全为 Item：来源 / 分区 / 拍号由内核填。"""
    return Item(
        kind=draft["kind"],
        body=draft["body"],
        about=draft.get("about"),
        supersedes=draft.get("supersedes"),
        made_by=made_by,
        scope=scope,
        beat=beat,
    )


MARK_KIND = "mark"


def mark_body(label: str, unit: str) -> dict:
    """标记 body 固定为 {"label": str, "unit": str}（§2.1）。"""
    return {"label": label, "unit": unit}
