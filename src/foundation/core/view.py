"""视野（施工单 §3.1）。今晚只做 self 与 with_parent。"""

from __future__ import annotations

from typing import Any

from foundation.core.canon import H
from foundation.core.item import Item
from foundation.core.params import Params

VIEW_KINDS = ("self", "with_parent")


def _truncate(x: Any, limit: int) -> tuple[Any, bool]:
    if isinstance(x, str) and len(x) > limit:
        return x[:limit], True
    if isinstance(x, dict):
        out, cut = {}, False
        for k, v in x.items():
            v2, c = _truncate(v, limit)
            out[k] = v2
            cut = cut or c
        return out, cut
    return x, False


def build_view(item: Item, table, view_kind: str, params: Params) -> tuple[Any, str, bool]:
    """返回 (视野内容, view_fp, 是否被截断)。"""
    if view_kind == "self":
        content: Any = item.body
    elif view_kind == "with_parent":
        prev = table.get(item.supersedes) if item.supersedes else None
        content = {"current": item.body, "previous": prev.body if prev else None}
    else:
        raise ValueError(f"今晚不支持的视野：{view_kind}（只做 self / with_parent）")
    content, truncated = _truncate(content, params.view_char_limit)
    return content, H(view_kind, content), truncated
