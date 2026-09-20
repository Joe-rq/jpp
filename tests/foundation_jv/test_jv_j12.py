"""ir-impl-4：J-12（失败是值，程序不崩）、J-05 消费收窄（裸 isinstance 不消费 Unsure）、W-literal-from-host、
G5 猜 23 回归、measure band 归档提示、守卫收到 Pick 的报文。全部 FakeClient，$0。"""

from __future__ import annotations

import os
import sys
import warnings

import pytest

sys.path.insert(0, os.path.join(os.path.dirname(__file__), "..", ".."))
import foundation.jv as jv  # noqa: E402
from foundation.jv.checker import check  # noqa: E402


def rt_with(client=None, **kw):
    rt = jv.Runtime(client=client or jv.FakeClient(), **kw)
    for k in ("t.k", "s.k", "m.k"):
        rt.calib.put(k, hi=0.6, lo=0.3, n=100, status="上岗")
    return rt


def boom_rule(text, qid, q):
    raise RuntimeError("假规则炸了")


def catch(fn):
    with warnings.catch_warnings(record=True) as w:
        warnings.simplefilter("always")
        out = fn()
    return out, [str(x.message) for x in w]


# ---------------------------------------------------------------- J-12：五条失败路径都是值
def test_client_failure_test_question_is_unsure_fail_not_crash():
    with rt_with(client=jv.FakeClient(boom_rule)):
        @jv.program(budget=jv.Budget(calls=5))
        def f():
            e = jv.cut(jv.judge(jv.state(on=jv.lit("a")), jv.test("是吗", calib=jv.calib("t.k")))[0])
            match e:
                case jv.Unsure(c):
                    return c
            return "no"
        out, ws = catch(f)
    assert out == "fail"
    assert any(w.startswith("W-call-fail") for w in ws)


def test_client_failure_measure_then_order_regression_g5_23():
    """G5 猜 23：假规则抛异常后 cut/order 抛 TypeError float(None)。现在：Unsure(fail) + 失败读数排最后一档。"""
    with rt_with(client=jv.FakeClient(boom_rule)):
        @jv.program(budget=jv.Budget(calls=5))
        def f():
            q = jv.measure("档？", scale=("低", "中", "高"), calib=jv.calib("m.k"))
            rs = jv.judge([jv.state(on=jv.lit("a")), jv.state(on=jv.lit("b"))], q)
            es = jv.cut(rs)
            jv.consume(es, unsure=jv.drop)
            return [e.cause for e in es], rs.order()
        (causes, tiers), _ = catch(f)
    assert causes == ["fail", "fail"]
    assert tiers == [[0, 1]]                      # 失败读数一档，不抛 TypeError


def test_client_failure_select_is_unsure_fail():
    with rt_with(client=jv.FakeClient(boom_rule)):
        @jv.program(budget=jv.Budget(calls=5))
        def f():
            q = jv.select("哪个？", calib=jv.calib("s.k"))
            e = jv.cut(jv.judge(jv.state(on=jv.lit("a"), over=[jv.lit("x"), jv.lit("y")]), q))
            jv.consume([e], unsure=jv.drop)
            return e.cause
        out, _ = catch(f)
    assert out == "fail"


def test_transform_exception_is_fail_material():
    def bad(m):
        raise ValueError("tx boom")
    with rt_with():
        @jv.program(budget=jv.Budget(calls=5))
        def f():
            m = jv.transform(bad, jv.lit("a"))
            alt = jv.on_fail(m, jv.lit("替代"))
            return m, alt
        (m, alt), ws = catch(f)
    assert isinstance(m, jv.Mat) and "fail" in m.content and "ValueError" in m.content["fail"]
    assert alt.content == "替代"
    assert any(w.startswith("W-transform-fail") for w in ws)


def test_transform_fail_material_in_state_gives_unsure_fail():
    def bad(m):
        raise ValueError("tx boom")
    with rt_with():
        @jv.program(budget=jv.Budget(calls=5))
        def f():
            m = jv.transform(bad, jv.lit("a"))
            e = jv.cut(jv.judge(jv.state(on=m), jv.test("是吗", calib=jv.calib("t.k")))[0])
            jv.consume([e], unsure=jv.drop)
            return e.cause
        out, _ = catch(f)
    assert out == "fail"


def test_gen_exception_is_empty_list():
    def g(prompt, ctx, n, seq):
        raise ValueError("gen boom")
    with rt_with():
        @jv.program(budget=jv.Budget(calls=5))
        def f():
            return jv.gen("x", ctx=[jv.lit("a")], n=2, retry_seq=0, generator=g)
        out, ws = catch(f)
    assert out == [] and any(w.startswith("W-gen-fail") for w in ws)


def test_do_exception_is_fail_material():
    def bad(m):
        raise ValueError("do boom")
    with rt_with():
        act = jv.register_action("bad_j12", fn=bad, taint_out="inherit", reason="test")
        @jv.program(budget=jv.Budget(calls=5))
        def f():
            m = jv.do(act, jv.lit("a"), iter_seq=0)
            return jv.on_fail(m, jv.lit("替代")).content, m.content
        (alt, c), _ = catch(f)
    assert alt == "替代" and "fail" in c


# ---------------------------------------------------------------- J-05 消费收窄
def test_bare_isinstance_does_not_consume_unsure():
    with rt_with(client=jv.FakeClient(boom_rule)):
        @jv.program(budget=jv.Budget(calls=5))
        def f():
            e = jv.cut(jv.judge(jv.state(on=jv.lit("a")), jv.test("是吗", calib=jv.calib("t.k")))[0])
            assert isinstance(e, jv.Unsure)            # 只看一眼
            return e.cause
        with pytest.raises(jv.JvError, match="J-05"):
            catch(f)


def test_match_consume_and_handle_still_consume_unsure():
    with rt_with(client=jv.FakeClient(boom_rule)):
        @jv.program(budget=jv.Budget(calls=5))
        def f():
            es = jv.cut(jv.judge([jv.state(on=jv.lit("a")), jv.state(on=jv.lit("b")), jv.state(on=jv.lit("c"))],
                                 jv.test("是吗", calib=jv.calib("t.k"))))
            match es[0]:
                case jv.Unsure():                      # 不绑 cause 也算消费
                    pass
            jv.handle(es[1], then=jv.drop)
            jv.consume([es[2]], unsure=jv.drop)
            return [e.consumed for e in es]
        out, _ = catch(f)
    assert out == [True, True, True]


def test_isinstance_on_act_still_marks_consumed_but_not_required():
    with rt_with():
        @jv.program(budget=jv.Budget(calls=5))
        def f():
            e = jv.cut(jv.judge(jv.state(on=jv.lit("截止日期 有")), jv.test("提到截止日期吗", calib=jv.calib("t.k")))[0])
            isinstance(e, jv.Act)
            return e.kind, e.consumed
        out, _ = catch(f)
    assert out[0] in ("act", "ignore")             # Act/Ignore 不需消费；程序正常返回


# ---------------------------------------------------------------- W-literal-from-host（jv.mat 洗白宿主计算，J-11 补丁提议）
def test_static_w_literal_from_host():
    def host():
        return "x"

    @jv.program(budget=jv.Budget(calls=5))
    def f(doc):
        a = jv.mat("字面量")                 # 合法
        b = jv.mat(doc)                     # 程序参数：合法
        c = jv.mat(host())                  # 宿主计算：warn
        return a, b, c
    rep = check(f.__jv_fn__)
    ws = [w for w in rep.warnings if "W-literal-from-host" in w]
    assert len(ws) == 1 and "jv.transform" in ws[0]


def test_runtime_w_literal_from_host_in_frame():
    with rt_with():
        @jv.program(budget=jv.Budget(calls=5), check_static=False)
        def f():
            jv.mat("标量")                    # 不报
            jv.mat({"k": [1, 2]})             # 帧内非标量：报
            return 1
        _, ws = catch(f)
    assert sum(w.startswith("W-literal-from-host") for w in ws) == 1
    with rt_with():
        _, ws = catch(lambda: jv.mat({"k": 1}))   # 帧外不报
    assert not any(w.startswith("W-literal-from-host") for w in ws)


# ---------------------------------------------------------------- 语义未定处按提议实现的两条
def test_measure_band_carries_nearest_level():
    with rt_with():
        @jv.program(budget=jv.Budget(calls=5))
        def f():
            jv.current().calib.put("m.k", hi=0.9, lo=0.1, n=100, status="上岗")
            q = jv.measure("档？", scale=("低", "中", "高"), calib=jv.calib("m.k"))
            e = jv.cut(jv.judge(jv.state(on=jv.lit("a！")), q)[0])
            jv.consume([e], unsure=jv.drop)
            return e.cause, e.detail.get("nearest_level")
        out, _ = catch(f)
    assert out == ("band", 1)


def test_guard_with_pick_has_explicit_message():
    with rt_with():
        act = jv.register_action("irr_j12", fn=lambda m: "ok", taint_out="inherit", reversible=False, reason="t")
        @jv.program(budget=jv.Budget(calls=5))
        def f():
            q = jv.select("哪个？", calib=jv.calib("s.k"))
            e = jv.cut(jv.judge(jv.state(on=jv.lit("x"), over=[jv.lit("x"), jv.lit("y")]), q))
            return jv.do(act, jv.lit("a"), iter_seq=0, guard=e).content
        with pytest.raises(jv.JvError, match="J-08") as ei:
            catch(f)
    assert "Pick/At" in str(ei.value) or "Unsure 不能放行" in str(ei.value)
