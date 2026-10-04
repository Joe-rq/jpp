"""注入（预注册 13，I1–I6）：在基线上去掉一处手写机制，看检测能不能抓到。只覆盖方法，不改 net_py.py。"""
from __future__ import annotations

import sys

import net_py


def _caller():
    return sys._getframe(2).f_code.co_name


class I1NoPairInvalidation(net_py.Net):
    """材料变了不再让读它的两两重算。"""
    def _material_changed(self, x):
        self.mark("recall", x)
        for ck in list(self.cands_of.get(x, ())):
            self.mark("whole", ck)
            if ck in self.config: self.mark("plan", ck)
        for k in list(self.grow_deps.get(x, ())): self.mark("grow", k)


class I2NoLeaveCascade(net_py.Net):
    """离开不级联撤候选与构型。"""
    async def leave(self, a):
        if a not in self.world: return
        for d in (self.world, self.material, self.node, self.derived, self.inbox, self.best): d.pop(a, None)
        self.p.index_del(a)
        for k in list(self.pairs_of.get(a, ())): self.mark("pair", k)
        for key in [k for k in self.unlocked if a in k]: self.unlocked.pop(key)
        for key in [k for k in self.reply if a in k[:2]]: self.reply.pop(key)
        for b, reqs in self.inbox.items(): self.inbox[b] = [r for r in reqs if r["from"] != a]
        self.mark("recall", a)


class I3NoEdgeToPairConfig(net_py.Net):
    """不登记「edge → 两人构型」：两两写完边不再让对应的两人构型重算（方案门这次需求新加的依赖）。"""
    def mark(self, kind, key):
        if kind == "whole" and _caller() in ("_pair", "_drop_pair"):
            return
        super().mark(kind, key)


class I4NoBestToPlan(net_py.Net):
    """不登记「best[m] → 方案」：某人的前 2 变了，含他的其他方案不重查。"""
    def _set_best(self, m, k, p):
        b = self.best.get(m, {})
        if b.get(k) == p: return
        if p is None: b.pop(k, None)
        else: self.best.setdefault(m, b)[k] = p


class I5NoStaleCheck(net_py.Net):
    """两两提交前不查 stale()。"""
    def stale(self, kind, key):
        if kind == "pair":
            return False
        return super().stale(kind, key)


class I6BusyOffByOne(net_py.Net):
    """静止计数错一处：judge_many 挂起时不减 busy。"""
    async def judge_many(self, items):
        keyed, needs = [], set()
        for s, qs in items:
            self._etier(s)
            sh = net_py.h(s)
            keyed.append((sh, qs))
            for q in qs:
                if (sh, q) not in self.cache:
                    needs.add((sh, q)); self.queue.setdefault(sh, (s, {}))[1][q] = 1
        if needs:
            fut = __import__("asyncio").get_running_loop().create_future()
            self.parked.append((needs, fut))
            self._kick()
            await fut
        return [[self.cache[(sh, q)] for q in qs] for sh, qs in keyed]


INJECTIONS = {"I1": I1NoPairInvalidation, "I2": I2NoLeaveCascade, "I3": I3NoEdgeToPairConfig,
              "I4": I4NoBestToPlan, "I5": I5NoStaleCheck, "I6": I6BusyOffByOne}
