"""唯一的规范化与哈希入口（施工单 §2）。"""

from __future__ import annotations

import hashlib
import json
from typing import Any


def canon(obj: Any) -> str:
    """规范 JSON：sort_keys=True, ensure_ascii=False, separators=(",", ":")。"""
    return json.dumps(obj, sort_keys=True, ensure_ascii=False, separators=(",", ":"))


def H(*parts: Any) -> str:
    """sha256 的前 16 个十六进制字符，输入先走 canon（列表形式，避免拼接歧义）。"""
    return hashlib.sha256(canon(list(parts)).encode("utf-8")).hexdigest()[:16]
