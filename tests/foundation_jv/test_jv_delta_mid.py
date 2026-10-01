"""Z0361 / Z0389：Python 内核的画像 δ 与 Rust 同口径——取中段、缺 mid 报 E-delta-mid、没有 δ 不编数（裁定四十四、四十五、五十六）。

预注册：地基/过程记录/工程-Z0334-δ分层.md §二十二。
"""

from __future__ import annotations

import os
import sys

import pytest

sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__)))))

import foundation.jv as jv  # noqa: E402
from foundation.jv.runtime import load_profile  # noqa: E402

K = "dm.k"
COLS = ("noul", "choice_prob_chosen", "score")


def 只有尾段() -> dict:
    p = load_profile()
    p["delta"] = {c: {k: v for k, v in p["delta"][c].items() if k != "mid"} for c in COLS}
    return p


def 没有delta() -> dict:
    return {k: v for k, v in load_profile().items() if k != "delta"}


def rt_with(profile, reading: float):
    client = jv.FakeClient(rule=lambda t, qid, q: {"type": "noul", "noul": reading})
    rt = jv.Runtime(client=client, profile=profile)
    rt.calib.put(K, hi=0.60, lo=0.10, n=60, status="上岗")          # 记录不写 δ：走画像兜底
    return rt


def q_test():
    return jv.test("这张工单需要人工跟进吗", calib=jv.calib(K))


def test_delta_for取中段():
    rt = jv.Runtime(client=jv.FakeClient())
    assert (rt.delta_for("noul"), rt.delta_for("choice"), rt.delta_for("score")) == (0.1281, 0.0971, 0.0821)


def test_只有尾段报缺mid():
    rt = jv.Runtime(client=jv.FakeClient(), profile=只有尾段())
    with pytest.raises(jv.JvError, match="E-delta-mid"):
        rt.delta_for("noul")
    p = 只有尾段()
    p["delta"]["score"]["mid"] = {"immediate": {"p99": 0.0821}}         # 只有一列中段也算缺
    with pytest.raises(jv.JvError, match="E-delta-mid"):
        jv.Runtime(client=jv.FakeClient(), profile=p).delta_for("score")


def test_没有delta返回None不编数():
    rt = jv.Runtime(client=jv.FakeClient(), profile=没有delta())
    assert rt.delta_for("noul") is None and rt.delta_for("choice") is None and rt.delta_for("score") is None


def test_没有delta照线切带位且不作放行守卫():
    deploy = jv.Action("deploy", fn=lambda *a: "ok", reversible=False)
    with rt_with(没有delta(), 0.62):
        e = jv.cut(jv.judge(jv.state(on=jv.lit("客户第三次来信")), q_test())[0])
        assert isinstance(e, jv.Act), e                                  # 0.62 过 0.60：照线切、没加带
        assert e.detail.get("delta_unknown") is True
        with pytest.raises(jv.JvError, match="J-08"):
            jv.do(deploy, jv.lit("x"), iter_seq=0, guard=[e])
    with rt_with(load_profile(), 0.62):                                  # 对照：发行画像取中段加带，0.62 在带内
        e = jv.cut(jv.judge(jv.state(on=jv.lit("客户第三次来信")), q_test())[0])
        assert isinstance(e, jv.Unsure) and e.cause == "band"
        assert not e.detail.get("delta_unknown")
        jv.consume([e], unsure=jv.drop)


def test_cut画像只有尾段报缺mid():
    with rt_with(只有尾段(), 0.62):
        with pytest.raises(jv.JvError, match="E-delta-mid"):
            jv.cut(jv.judge(jv.state(on=jv.lit("客户第三次来信")), q_test())[0])


def _两条读数(profile):
    client = jv.FakeClient(rule=lambda t, qid, q: {"type": "noul", "noul": 0.70 if "甲" in t else 0.62})
    rt = jv.Runtime(client=client, profile=profile)
    rt.calib.put(K, hi=0.60, lo=0.10, n=60, status="上岗")
    with rt:
        rs = jv.judge([jv.state(on=jv.lit("甲")), jv.state(on=jv.lit("乙"))], q_test())
        t = rs.order()
        jv.consume(jv.cut(rs), unsure=jv.drop)
    return t


def test_order没有delta不并档_有中段并档():
    assert _两条读数(没有delta()) == [[0], [1]]                         # 差 0.08，不编数，不并档
    assert _两条读数(load_profile()) == [[0, 1]]                         # 中段 0.1281 内，并成一档


# ---------------------------------------------------------------- 跨内核用法对齐（Z0334 §二十四第 4 件，以 Rust 为准）

def test_order先取记录delta():
    """记录有 δ（0.05）时先取记录的，差 0.08 分两档（与 Rust `并档容差` 同）；原来 Python 只取画像 δ，会按 0.1281 并档"""
    client = jv.FakeClient(rule=lambda t, qid, q: {"type": "noul", "noul": 0.70 if "甲" in t else 0.62})
    rt = jv.Runtime(client=client, profile=load_profile())
    rt.calib.put(K, hi=0.60, lo=0.10, n=60, status="上岗", delta=0.05)
    with rt:
        rs = jv.judge([jv.state(on=jv.lit("甲")), jv.state(on=jv.lit("乙"))], q_test())
        t = rs.order()
        jv.consume(jv.cut(rs), unsure=jv.drop)
    assert t == [[0], [1]]


def test_allocate记录没有delta算不出不进榜():
    """与 Rust `strength::uncertainty` 同：δ 只从记录取，记录没有 δ 算不出，不进榜、告警点名（原来 Python 退到画像 δ）"""
    import warnings
    client = jv.FakeClient(rule=lambda t, qid, q: {"type": "noul", "noul": 0.5})
    rt = jv.Runtime(client=client, profile=load_profile())
    rt.calib.put(K, hi=0.60, lo=0.10, n=60, status="上岗")               # 没有 δ
    rt.calib.put("dm.k2", hi=0.60, lo=0.10, n=60, status="上岗", delta=0.1281)
    with rt:
        a = jv.judge([jv.state(on=jv.lit("甲")), jv.state(on=jv.lit("乙"))], q_test())
        b = jv.judge(jv.state(on=jv.lit("丙")), jv.test("这张工单需要人工跟进吗", calib=jv.calib("dm.k2")))
        rs = [a[0][0], a[1][0], b[0]]
        with warnings.catch_warnings(record=True) as w:
            warnings.simplefilter("always")
            picked = jv.allocate(rs, 3)
        jv.consume([jv.cut(r) for r in rs], unsure=jv.drop)
    assert picked == [2], picked
    assert any("W-untested" in str(x.message) and "[0, 1]" in str(x.message) for x in w)
