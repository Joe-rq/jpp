"""四道保险 + 两个记号（施工单 §3.7）。

每道保险都能用 `--disable-guard <名字>` 关掉（params.disabled_guards），
关掉之后只是不拦，不影响别的逻辑——S3.4 就是靠关掉 return_to_same 才测得到 runaway。
"""

from __future__ import annotations

from dataclasses import dataclass, field

from foundation.core.params import Params

ONCE = "once"
RETURN_TO_SAME = "return_to_same"
RUNAWAY = "runaway"
BEAT_BUDGET = "beat_budget"
COST_BUDGET = "cost_budget"
PACK_DEPTH = "pack_depth"              # 记号：递归深度超限，拒绝实例化并上交（§3.9）
TIE_CONFLICT = "tie_conflict"          # 记号，不是保险
VIEW_TRUNCATED = "view_truncated"      # 记号，不是保险
NAMES = (ONCE, RETURN_TO_SAME, RUNAWAY, BEAT_BUDGET, COST_BUDGET)


@dataclass
class GuardHit:
    name: str
    scope: str
    detail: dict = field(default_factory=dict)


class Guards:
    def __init__(self, params: Params):
        self.params = params
        self.cost_usd = 0.0
        self.write_calls = 0
        self._fp_seen: dict[str, dict[str, int]] = {}     # scope -> 指纹 -> 第几次检查
        self._idx: dict[str, int] = {}                    # scope -> 检查序号
        self._movers: dict[str, dict[int, set[str]]] = {}  # scope -> 序号 -> 动过手的单元
        self._stopped: set[str] = set()                   # 被 runaway 停掉的链上 id
        self.fired: list[GuardHit] = []

    # —— 花费 ——
    def add_cost(self, usd: float) -> None:
        self.cost_usd += float(usd or 0.0)

    def add_write_call(self, n: int = 1) -> None:
        self.write_calls += n

    def check_cost_budget(self, scope: str) -> GuardHit | None:
        if not self.params.guard_on(COST_BUDGET):
            return None
        if self.cost_usd > self.params.cost_usd:
            return self._fire(COST_BUDGET, scope,
                              {"cost_usd": round(self.cost_usd, 6), "limit": self.params.cost_usd})
        if self.write_calls > self.params.write_calls:
            return self._fire(COST_BUDGET, scope,
                              {"write_calls": self.write_calls, "limit": self.params.write_calls})
        return None

    # —— 回到原样 ——
    def note_mover(self, scope: str, unit_name: str) -> None:
        idx = self._idx.get(scope, 0)
        self._movers.setdefault(scope, {}).setdefault(idx, set()).add(unit_name)

    def check_return_to_same(self, scope: str, fingerprint: str) -> GuardHit | None:
        seen = self._fp_seen.setdefault(scope, {})
        idx = self._idx.get(scope, 0) + 1
        self._idx[scope] = idx
        if not self.params.guard_on(RETURN_TO_SAME):
            seen.setdefault(fingerprint, idx)
            return None
        if fingerprint in seen:
            first = seen[fingerprint]
            units = sorted({u for i, us in self._movers.get(scope, {}).items()
                            if first <= i <= idx for u in us})
            return self._fire(RETURN_TO_SAME, scope,
                              {"fingerprint": fingerprint, "units": units,
                               "first_seen_at": first, "again_at": idx})
        seen[fingerprint] = idx
        return None

    # —— 改不停 ——
    def check_runaway(self, scope: str, item_id: str, generation: int) -> GuardHit | None:
        if not self.params.guard_on(RUNAWAY):
            return None
        if generation > self.params.runaway_generations:
            if item_id in self._stopped:
                return None
            self._stopped.add(item_id)
            return self._fire(RUNAWAY, scope,
                              {"item": item_id, "generation": generation,
                               "limit": self.params.runaway_generations})
        return None

    def chain_stopped(self, table, item_id: str) -> bool:
        """被停掉的版本链上的任何一代都不再配对。"""
        if not self._stopped:
            return False
        if item_id in self._stopped:
            return True
        return any(i in self._stopped for i in table.chain(item_id))

    # —— 拍数 ——
    def check_beat_budget(self, scope: str, beats_in_scope: int,
                          budget: int | None = None) -> GuardHit | None:
        """拍数按分区算（红队 A6）：每个包实例有自己的 budget.beats，
        根分区用 params.beats_budget。花费不是这样——花费池全局唯一，见 check_cost_budget。"""
        if not self.params.guard_on(BEAT_BUDGET):
            return None
        limit = self.params.beats_budget if budget is None else int(budget)
        if beats_in_scope > limit:
            return self._fire(BEAT_BUDGET, scope,
                              {"beats": beats_in_scope, "budget": limit})
        return None

    def _fire(self, name: str, scope: str, detail: dict) -> GuardHit:
        hit = GuardHit(name=name, scope=scope, detail=detail)
        self.fired.append(hit)
        return hit
