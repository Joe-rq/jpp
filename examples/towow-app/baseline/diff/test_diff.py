"""预注册 13 的差分工具自测：真值与规则自洽，基线在种子 0 上终态无误，注入 I1 能被抓到，J++x 的已知问题钉住。"""
import asyncio
import os
import sys

import pytest

sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))

from diff import systems as S  # noqa: E402
from diff.faults import INJECTIONS  # noqa: E402
from diff.run import drive  # noqa: E402
from diff.trajectory import Truth, base_worlds, expected_edge, expected_whole, trajectory  # noqa: E402


def test_rule_truth_basics():
    T = Truth()
    W = base_worlds()
    for x in ("m1a", "m1b", "r1", "h1", "t1a", "t1b", "t1c"):
        T.world[x] = W[x]
    e = expected_edge(T, "m1a", "m1b")
    assert e["holds"] and e["dir"] == "both" and 0.66 <= e["p"] < 0.95
    assert not expected_edge(T, "h1", "r1")["holds"]                     # r1 的需要只在 t1
    T.unlocked[("r1", "h1")] = [{"text": "need:H1", "tier": "t1"}]
    assert expected_edge(T, "h1", "r1")["holds"]                         # 给出之后对上
    assert expected_whole(T, ["t1a", "t1b", "t1c"])[0]
    assert not expected_whole(T, ["m1a", "t1a", "t1b"])[0]
    ev = trajectory(0)
    assert len(ev) > 50 and all(0 <= e.gap <= 40 for e in ev)


def test_baseline_seed0_final_state_correct():
    r, _ = asyncio.run(drive(S.PySystem, 0))
    assert not r["settle"]["hang"] and r["settle"]["raised"] is None
    assert r["errors"] == {k: 0 for k in r["errors"]}, r["error_detail"]


def test_injection_i1_is_caught():
    r, _ = asyncio.run(drive(None, 0, net_cls=INJECTIONS["I1"]))
    assert r["errors"]["edge_wrong"] > 0                                  # 材料变了两两不重算 → 边与真值不一致


# 预注册 13 查出、预注册 15 修复（离开引起的撤回即时生效）
def test_jpx_seed0_final_state_correct():
    r, _ = asyncio.run(drive(S.JxSystem, 0))
    assert sum(r["errors"].values()) == 0, r["error_detail"]


async def _rejoin_before_quiet(cls, ticks_away):
    from diff.trajectory import Ev, add_offer
    T = Truth()
    sy = cls(T)
    await sy.start()
    W = base_worlds()

    async def ap(e):
        T.apply(e)
        await sy.apply(e)
    await ap(Ev("join", "m1a", W["m1a"]))
    await sy.settle(30)
    await ap(Ev("join", "m1b", W["m1b"]))              # 两两(m1a, m1b) 只由 召回(m1b) 派生
    await sy.settle(30)
    await ap(Ev("leave", "m1a"))
    await ap(Ev("update", "m1b", add_offer(W["m1b"], "Q")))   # m1b 的召回在 m1a 不在时重跑，这一对被撤
    await S.ticks(ticks_away)
    await ap(Ev("join", "m1a", W["m1a"]))              # 全网静止前、以同样材料再接入
    await sy.settle(30)
    return sy.final()["edges"].get(("m1a", "m1b"))


def test_baseline_rejoin_before_quiet_restores_pair():
    e = asyncio.run(_rejoin_before_quiet(S.PySystem, 20))
    assert e is not None and e["holds"]


# 预注册 13 最小复现、预注册 15 修复
def test_jpx_rejoin_before_quiet_restores_pair():
    e = asyncio.run(_rejoin_before_quiet(S.JxSystem, 20))
    assert e is not None and e["holds"]


def test_jpx_rejoin_after_quiet_restores_pair():
    e = asyncio.run(_rejoin_before_quiet(S.JxSystem, 2000))        # 让出足够久、先静止：删除落地，再接入是新版本
    assert e is not None and e["holds"]
