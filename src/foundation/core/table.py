"""Table：从 Log 折叠出的桌面（施工单 §2.2 / §2.4）。没有任何修改接口。"""

from __future__ import annotations

from typing import Iterable

from foundation.core.item import Item
from foundation.core.canon import H


class Table:
    """只读视图。唯一的构造方式是折叠 Log 里的 item 事件。"""

    def __init__(self, items: Iterable[Item] = ()):
        self._items: dict[str, Item] = {}
        self._order: list[str] = []
        for it in items:
            self._absorb(it)

    # —— 构造（内部）——
    def _absorb(self, item: Item) -> None:
        iid = item.id
        if iid not in self._items:          # 同 id 幂等：beat 不进 id
            self._items[iid] = item
            self._order.append(iid)

    @staticmethod
    def from_log(events: Iterable[dict]) -> "Table":
        t = Table()
        for ev in events:
            if ev.get("t") == "item":
                t._absorb(Item.from_dict(ev["item"]))
        return t

    # —— 只读查询 ——
    def get(self, iid: str) -> Item | None:
        return self._items.get(iid)

    def all(self) -> list[Item]:
        return [self._items[i] for i in self._order]

    def ids(self) -> set[str]:
        return set(self._items)

    def superseded(self) -> set[str]:
        """被"活着的"东西取代掉的 id。"""
        dead: set[str] = set()
        for it in self._items.values():
            if it.supersedes:
                dead.add(it.supersedes)
        return dead

    def alive(self, scope: str | None = None) -> list[Item]:
        """没有别的 alive item 的 supersedes 指向它。

        版本链上每一代只被下一代指一次，所以"被任何人指过"等价于"不是链头"。
        """
        dead = self.superseded()
        out = [self._items[i] for i in self._order if i not in dead]
        if scope is not None:
            out = [it for it in out if it.scope == scope]
        return out

    def alive_ids(self, scope: str | None = None) -> set[str]:
        return {it.id for it in self.alive(scope)}

    def marks_on(self, iid: str, scope: str | None = None) -> list[Item]:
        return [it for it in self.alive(scope) if it.kind == "mark" and it.about == iid]

    def generation(self, iid: str) -> int:
        """版本链长度：本条是第几代（起点为 1）。"""
        n, seen, cur = 1, {iid}, self._items.get(iid)
        while cur is not None and cur.supersedes and cur.supersedes not in seen:
            seen.add(cur.supersedes)
            n += 1
            cur = self._items.get(cur.supersedes)
        return n

    def chain(self, iid: str) -> list[str]:
        out, seen, cur = [], set(), self._items.get(iid)
        while cur is not None and cur.id not in seen:
            out.append(cur.id)
            seen.add(cur.id)
            cur = self._items.get(cur.supersedes) if cur.supersedes else None
        return out

    def fingerprint(self, scope: str) -> str:
        """回到原样的指纹：活着的 (kind, body) 多重集合哈希，不含任何来源字段（红队 A3）。"""
        from foundation.core.canon import canon
        pairs = sorted(canon([it.kind, it.body]) for it in self.alive(scope))
        return H(pairs)
