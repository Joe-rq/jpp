"""建造第 3 步：七个 pass 开关（消融）、jv plan（J-07/J-10 计划期）、select/measure 裂变、保守线从档案、
jv stats、嵌套 @jv.program（程序调用程序：子账预算、作用域内 J-05、返回值进槽）。全部 FakeClient，$0。"""

from __future__ import annotations

import os
import sys
import warnings

import pytest

sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__)))))

import foundation.jv as jv  # noqa: E402
from foundation.jv.checker import check  # noqa: E402
from foundation.jv.plan import Sym  # noqa: E402

K = "t.k"


def rt_with(tmp_path=None, client=None, **kw):
    rt = jv.Runtime(client=client or jv.FakeClient(), root=str(tmp_path) if tmp_path else None, **kw)
    rt.calib.put(K, hi=0.65, lo=0.35, n=30, status="上岗", unsure_rate=0.1)
    rt.calib.put("sel.k", hi=0.5, lo=0.2, n=30, status="上岗")
    rt.calib.put("m.k", hi=0.6, lo=0.3, n=30, status="上岗")
    return rt


def q_test(text="提到截止日期吗", key=K, **kw):
    return jv.test(text, calib=jv.calib(key), **kw)


LONG = "无关内容。" * 300 + "截止日期是明天。" + "无关内容。" * 300      # ≫ 500 token


# ---------------------------------------------------------------- 开关：每个 pass 关掉都能量到
def _two_q_same_state():
    s = jv.state(on=jv.lit("截止日期 明天"))
    r = jv.judge(s, q_test(), q_test("提到天气吗"))
    es = [jv.cut(r[0]), jv.cut(r[1])]
    jv.consume(es)
    return es


def test_switch_fuse_off_one_call_per_question():
    on, off = jv.FakeClient(), jv.FakeClient()
    with rt_with(client=on):
        jv.program()(_two_q_same_state)()
    with rt_with(client=off, passes={"fuse": False}):
        jv.program()(_two_q_same_state)()
    assert (on.calls, on.questions_asked) == (1, 2)
    assert (off.calls, off.questions_asked) == (2, 2)          # 逐题调用


def test_switch_lift_off_each_judge_is_its_own_layer():
    def p():
        r1 = jv.judge(jv.state(on=jv.lit("a 截止日期")), q_test())
        r2 = jv.judge(jv.state(on=jv.lit("a 截止日期")), q_test("天气"))
        jv.consume([jv.cut(r1[0]), jv.cut(r2[0])])
    on, off = jv.FakeClient(), jv.FakeClient()
    with rt_with(client=on) as rt:
        jv.program()(p)()
        assert len(rt.stats["layers"]) == 1 and on.calls == 1     # 同状态两 judge 提升到同层并融合
    with rt_with(client=off, passes={"lift": False}) as rt:
        jv.program()(p)()
        assert len(rt.stats["layers"]) == 2 and off.calls == 2    # 每个 judge 立即刷新


def test_switch_fission_off_no_chunking_and_no_window_warning():
    def p():
        r = jv.judge(jv.state(on=jv.lit(LONG)), q_test())
        e = jv.cut(r[0]); e.__dict__["consumed"] = True
        return r[0]._ans.get("chunks", 0)
    on, off = jv.FakeClient(), jv.FakeClient()
    with rt_with(client=on) as rt:
        assert jv.program()(p)() >= 4 and rt.stats["fission"] == 1
    with rt_with(client=off, passes={"fission": False}) as rt:
        assert jv.program()(p)() == 0 and rt.stats["fission"] == 0 and off.questions_asked == 1


def test_switch_lower_off_select_is_always_k_noul():
    def p():
        s = jv.state(on=jv.lit("要 b"), over=[jv.lit("a"), jv.lit("b"), jv.lit("c")])
        e = jv.cut(jv.judge(s, jv.select("选哪个", calib=jv.calib("sel.k"))))
        e.__dict__["consumed"] = True
        return e
    def rule(text, qid, q):
        if q["type"] == "noul":
            return {"type": "noul", "noul": 0.9 if "c1" in q["instructions"] else 0.05}
        return None
    on, off = jv.FakeClient(rule=rule), jv.FakeClient(rule=rule)
    with rt_with(client=on):
        jv.program()(p)()
        assert on.questions_asked == 2 and on.log[0]["questions"][next(iter(on.log[0]["questions"]))]["type"] == "choice"
    with rt_with(client=off, passes={"lower": False}):
        e = jv.program()(p)()
        assert off.questions_asked == 3 and all(q["type"] == "noul" for q in off.log[0]["questions"].values())
        assert isinstance(e, jv.Pick) and e.k == 1


def test_switch_schedule_off_is_serial_same_calls():
    def p():
        exits = jv.cut(jv.judge([jv.state(on=jv.lit(f"截止日期 {i}")) for i in range(4)], q_test()))
        jv.consume(exits)
    on, off = jv.FakeClient(), jv.FakeClient()
    with rt_with(client=on):
        jv.program()(p)()
    with rt_with(client=off, passes={"schedule": False}):
        jv.program()(p)()
    assert on.calls == off.calls == 4


def test_switch_plan_off_no_budget_enforcement_and_no_plan_warnings():
    def p():
        r1 = jv.judge(jv.state(on=jv.lit("a")), q_test()); e1 = jv.cut(r1[0])
        r2 = jv.judge(jv.state(on=jv.lit("b")), q_test()); e2 = jv.cut(r2[0])
        jv.consume([e1, e2])
        return e2
    on, off = jv.FakeClient(), jv.FakeClient()
    with rt_with(client=on) as rt:
        e2 = jv.program(budget=jv.Budget(calls=1))(p)()
        assert on.calls == 1 and isinstance(e2, jv.Unsure) and e2.cause == "budget"
        assert "plan" in rt.stats
    with rt_with(client=off, passes={"plan": False}) as rt:
        e2 = jv.program(budget=jv.Budget(calls=1))(p)()
        assert off.calls == 2 and not isinstance(e2, jv.Unsure)   # 不核预算：第 2 层照发
        assert "plan" not in rt.stats


def test_switch_ledger_off_second_run_pays_again(tmp_path):
    def p():
        r = jv.judge(jv.state(on=jv.lit("截止日期")), q_test())
        jv.consume([jv.cut(r[0])])
    c = jv.FakeClient()
    with rt_with(tmp_path / "on", client=c):
        jv.program()(p)(); jv.program()(p)()
    assert c.calls == 1                                            # 第二遍重放
    c2 = jv.FakeClient()
    with rt_with(tmp_path / "off", client=c2, passes={"ledger": False}):
        jv.program()(p)(); jv.program()(p)()
    assert c2.calls == 2                                           # 每次都发


# ---------------------------------------------------------------- 裂变：select / measure
def test_fission_select_long_object_chunks_k_noul_then_decides():
    def rule(text, qid, q):
        if q["type"] == "noul":
            return {"type": "noul", "noul": 0.9 if ("c1" in q["instructions"] and "截止日期" in text) else 0.05}
        return None
    client = jv.FakeClient(rule=rule)
    with rt_with(client=client) as rt:
        def p():
            s = jv.state(on=jv.lit(LONG), over=[jv.lit("a"), jv.lit("b")])
            e = jv.cut(jv.judge(s, jv.select("哪个", calib=jv.calib("sel.k"))))
            e.__dict__["consumed"] = True
            return e
        e = jv.program()(p)()
    assert isinstance(e, jv.Pick) and e.k == 1
    assert rt.stats["fission"] == 1 and e.reading._ans["chunks"] >= 4
    assert all(q["type"] == "noul" for l in client.log for q in l["questions"].values())
    assert client.questions_asked == 2 * e.reading._ans["chunks"]


def test_fission_measure_long_object_counts_exits():
    def rule(text, qid, q):
        if q["type"] == "score":
            lvl = 2 if "！！" in text else 0
            return {"type": "score", "score": float(lvl), "probabilities": {"0": 0.8 if lvl == 0 else 0.1, "1": 0.1, "2": 0.8 if lvl == 2 else 0.1}}
        return None
    client = jv.FakeClient(rule=rule)
    long = ("平静。" * 400 + "！！") * 3 + "平静。" * 400
    with rt_with(client=client) as rt:
        def p():
            r = jv.judge(jv.state(on=jv.lit(long)), jv.measure("多严重", scale=["低", "中", "高"], calib=jv.calib("m.k")))
            e = jv.cut(r[0]); e.__dict__["consumed"] = True
            return e, r[0]._ans
        e, ans = jv.program()(p)()
    assert rt.stats["fission"] == 1 and ans["chunks"] >= 3 and ans["phys"] == "score"
    assert ans["value"] in (0, 2) and 0 < ans["mode_share"] <= 1 and "band" in (getattr(e, "cause", "") or "") or isinstance(e, jv.At)


# ---------------------------------------------------------------- 保守线只从档案
def test_cold_safety_lines_come_from_profile_not_constants():
    client = jv.FakeClient(rule=lambda t, qid, q: {"type": "noul", "noul": 0.8})
    with rt_with(client=client) as rt:
        assert rt.safety_lines() == (0.75, 0.25)
        r = jv.judge(jv.state(on=jv.lit("x")), q_test(key="cold.key"))
        e = jv.cut(r[0])
        assert isinstance(e, jv.Unsure) and e.cause == "cold" and isinstance(e.detail["provisional"], jv.Act)
        e.__dict__["consumed"] = True
    prof = dict(rt.profile); prof = {k: v for k, v in prof.items() if k != "lines"}
    with jv.Runtime(client=client, profile=prof) as rt2:
        with warnings.catch_warnings(record=True) as w:
            warnings.simplefilter("always")
            assert rt2.safety_lines() == (1.0, 0.0)
        assert any("W-untested" in str(x.message) and "lines.safety_default" in str(x.message) for x in w)
        r = jv.judge(jv.state(on=jv.lit("x")), q_test(key="cold.key"))
        e = jv.cut(r[0])
        assert isinstance(e.detail["provisional"], jv.Unsure)      # 无线 → 不给临时出口
        e.__dict__["consumed"] = True


# ---------------------------------------------------------------- jv plan
def test_sym_polynomial():
    n, m = Sym.var("n"), Sym.var("m")
    e = (Sym(3) * n + 2) * m
    assert repr(e) == "3·m·n + 2·m" and e.degree == 2 and not e.is_numeric
    assert e.subs({"n": 2, "m": 5}).value == 40 and (Sym(4) + 1).is_numeric


def test_plan_symbolic_signature_and_budget_warnings():
    run = jv.Action("run", fn=lambda *a: {"ok": 1}, cost=0.5, latency=2.0)
    慢了 = jv.test("慢了吗", calib=jv.calib(K))

    @jv.program(budget=jv.Budget(calls=3, layers=2))
    def p(xs):
        outs = [jv.do(run, x, iter_seq=i) for i, x in enumerate(xs)]
        for k in range(4):
            r = jv.judge([jv.state(on=o) for o in outs], 慢了)
            jv.consume(jv.cut(r))
        return 1
    with rt_with() as rt:
        rep = jv.plan(p, rt=rt)
    assert repr(rep.calls) == "4·|xs|" and repr(rep.layers) == "4" and repr(rep.do_calls) == "|xs|"   # outs 的规模追到 |xs|
    assert repr(rep.do_cost) == "500·|xs|"
    assert repr(rep.unsure_bound) == "400·|xs|"                       # u=0.1 → 100 千分位 × 4·|xs|
    assert any(w.startswith("W-cost") and "层数上界 4 > budget.layers=2" in w for w in rep.warnings)
    assert "judge 调用 ≈ 4·|xs|" in rep.render()


def test_plan_cartesian_product_growing_ctx_untested_and_serial():
    run = jv.Action("run", fn=lambda *a: "o")
    q = jv.test("x", calib=jv.calib("cold.k"))
    sel = jv.select("哪个", calib=jv.calib("sel.k"))

    @jv.program(budget=jv.Budget(unsure=0.1))
    def p(a, b):
        史 = []
        for x in a:
            for y in b:
                r = jv.judge(jv.state(on=x, ctx=[y, *史]), q)
                jv.consume([jv.cut(r[0])])
        for k in range(2):
            o = jv.do(run, jv.lit("x"), iter_seq=k)
            o2 = jv.do(run, o, iter_seq=k)
        e = jv.cut(jv.judge(jv.state(on=jv.lit("t"), over=[jv.lit("u"), jv.lit("v")]), sel))
        jv.consume([e])
    with rt_with() as rt:
        rep = jv.plan(p, rt=rt)
    kinds = {w.split(":")[0] for w in rep.warnings}
    assert {"W-cost", "W-window", "W-untested", "W-unsure-bound", "W-serial"} <= kinds, rep.warnings
    assert rep.calls.degree == 2


def test_plan_runs_inside_program_and_records_stats():
    def p():
        r = jv.judge(jv.state(on=jv.lit("截止日期")), q_test())
        jv.consume([jv.cut(r[0])])
    with rt_with() as rt:
        jv.program(budget=jv.Budget(calls=10))(p)()
        assert rt.stats["plan"]["p"]["calls"] == "1"


# ---------------------------------------------------------------- jv stats
def test_stats_report_fusion_rate_and_layers():
    with rt_with() as rt:
        jv.program()(_two_q_same_state)()
        st = jv.stats()
    assert st["layers"] == 1 and st["questions_per_layer"] == [2] and st["calls_per_layer"] == [1]
    assert st["fusion_rate"] == 2.0 and st["ledger_hits"] == 0 and st["cost"] > 0


# ---------------------------------------------------------------- 嵌套 @jv.program
def test_nested_program_twice_total_calls_and_layers():
    client = jv.FakeClient()

    @jv.program(budget=jv.Budget(calls=5))
    def inner(m):
        r = jv.judge(jv.state(on=m), q_test())
        e = jv.cut(r[0])
        jv.consume([e])
        return e

    @jv.program(budget=jv.Budget(calls=10))
    def outer():
        r0 = jv.judge(jv.state(on=jv.lit("外 截止日期")), q_test())        # 登记，未刷新
        a = inner(jv.lit("内一 截止日期"))                                  # 内层 cut 刷新：外层的 judge 同层融合发出
        b = inner(jv.lit("内二 截止日期"))
        s = jv.state(on=jv.lit("汇总"), ctx=[a, b])                          # 内层返回的 Exit 直接进槽（J-11 合法）
        r = jv.judge(s, q_test("有汇总吗"))
        jv.consume([jv.cut(r0[0]), jv.cut(r[0])])
        return a.kind, b.kind
    with rt_with(client=client) as rt:
        assert outer() == ("act", "act")
        assert rt.stats["calls"] == 4 and len(rt.stats["layers"]) == 3        # 层 1：外 r0 + 内一（2 状态）；层 2：内二；层 3：汇总
        assert rt.stats["layers"][0]["states"] == 2
        frames = rt.stats["frames"]
        assert [f["name"] for f in frames] == ["inner", "inner", "outer"]
        assert frames[0]["calls"] == 2 and frames[1]["calls"] == 1 and frames[2]["calls"] == 4
        assert rt.books is not None and rt.frame is None
    rep = check(outer.__jv_fn__)
    assert not rep.errors and not any("J-11" in w for w in rep.warnings)


def test_nested_inner_over_budget_does_not_blow_outer():
    client = jv.FakeClient()

    @jv.program(budget=jv.Budget(calls=1))
    def inner():
        r1 = jv.judge(jv.state(on=jv.lit("a 截止日期")), q_test()); e1 = jv.cut(r1[0])
        r2 = jv.judge(jv.state(on=jv.lit("b 截止日期")), q_test()); e2 = jv.cut(r2[0])
        jv.consume([e1, e2])
        return e1, e2

    @jv.program(budget=jv.Budget(calls=10))
    def outer():
        e1, e2 = inner()
        r = jv.judge(jv.state(on=jv.lit("外 截止日期")), q_test())
        e3 = jv.cut(r[0]); jv.consume([e3])
        return e1.kind, e2, e3.kind
    with rt_with(client=client) as rt:
        k1, e2, k3 = outer()
    assert k1 == "act" and isinstance(e2, jv.Unsure) and e2.cause == "budget" and k3 == "act"
    assert client.calls == 2                                           # 内层第 2 层被停，外层照发
    assert any("W-budget" in w and "inner" in w for w in rt.stats["warnings"])


def test_nested_outer_over_budget_stops_inner_layer_too():
    client = jv.FakeClient()

    @jv.program(budget=jv.Budget(calls=5))
    def inner():
        r = jv.judge(jv.state(on=jv.lit("a 截止日期")), q_test()); e = jv.cut(r[0]); jv.consume([e]); return e

    @jv.program(budget=jv.Budget(calls=1))
    def outer():
        e1 = inner(); e2 = inner()
        return e1.kind, e2
    with rt_with(client=client):
        k1, e2 = outer()
    assert k1 == "act" and isinstance(e2, jv.Unsure) and e2.cause == "budget" and client.calls == 1


def test_nested_three_levels_and_scoped_j05():
    client = jv.FakeClient(rule=lambda t, qid, q: {"type": "noul", "noul": 0.5})

    @jv.program()
    def leaf():
        r = jv.judge(jv.state(on=jv.lit("x")), q_test())
        e = jv.cut(r[0])                                              # band → Unsure，不消费
        return e

    @jv.program()
    def mid():
        return leaf()

    @jv.program()
    def top():
        return mid()
    with rt_with(client=client) as rt:
        with pytest.raises(jv.JvError, match="J-05: 程序 leaf "):
            top()
        assert rt.frame is None                                       # 异常路径三层帧全部弹出

    @jv.program()
    def leaf_ok():
        r = jv.judge(jv.state(on=jv.lit("x 截止日期")), q_test())
        e = jv.cut(r[0]); jv.consume([e]); return e

    @jv.program()
    def mid_ok():
        return leaf_ok()

    @jv.program()
    def top_ok():
        e = mid_ok()
        return jv.state(on=e).on.kind
    with rt_with() as rt:
        assert top_ok() == "act"
        assert [f["depth"] for f in rt.stats["frames"]] == [2, 1, 0]


def test_nested_program_passed_as_parameter_is_dynamic_warning_not_error():
    @jv.program()
    def worker(m):
        r = jv.judge(jv.state(on=m), q_test()); e = jv.cut(r[0]); jv.consume([e]); return e

    @jv.program()
    def outer(f):
        s = jv.state(on=jv.lit("t 合同"), ctx=[f(jv.lit("a 截止日期"))])
        r = jv.judge(s, q_test("提到合同吗")); jv.consume([jv.cut(r[0])]); return "ok"   # 换题：J-02 禁自指
    rep = check(outer.__jv_fn__)
    assert not rep.errors and sum(1 for w in rep.warnings if w.startswith("W-dynamic")) == 1
    with rt_with():
        assert outer(worker) == "ok"

    def host(x):
        return x

    @jv.program()
    def bad(f):
        s = jv.state(on=jv.lit("t"), ctx=[f(jv.lit("a"))])
        r = jv.judge(s, q_test()); jv.consume([jv.cut(r[0])])
    with rt_with():
        with pytest.raises(jv.JvTypeError, match="J-11"):             # 运行期核：宿主函数的返回值不是材料
            bad(lambda m: {"raw": m.content})


def test_plan_includes_nested_program_signature():
    q = jv.test("x", calib=jv.calib(K))

    @jv.program()
    def inner(m):
        jv.consume([jv.cut(jv.judge(jv.state(on=m), q)[0])])

    @jv.program()
    def outer(xs):
        for x in xs:
            inner(x)
    with rt_with() as rt:
        rep = jv.plan(outer, rt=rt)
    assert repr(rep.calls) == "|xs|" and repr(rep.layers) == "|xs|"
    assert any(r.effect == "program" for r in rep.rows)


# ---------------------------------------------------------------- programs-21 报的包缺陷 D1–D3 + 定位回归 off-by-one + noprogress
def test_d1_reading_element_as_exit_raises_j01_at_runtime():
    with rt_with() as rt:
        @jv.program(check_static=False)
        def f():
            v = jv.judge([jv.state(on=jv.lit("a"))], q_test())
            out = []
            for e in v:
                match e:
                    case jv.Act():
                        out.append(1)
            return out
        with pytest.raises(jv.JvTypeError, match="J-01.*jv.cut"):
            f()
        assert rt.frame is None
        r = jv.judge(jv.state(on=jv.lit("a")), q_test())
        with pytest.raises(jv.JvTypeError, match="J-01"):
            isinstance(r[0], jv.Act)
        with pytest.raises(jv.JvTypeError, match="J-01"):
            isinstance(r, jv.Unsure)


def test_d1_static_unpacked_reading_vector_is_caught():
    def p1(cmds):
        v = jv.judge([jv.state(on=c) for c in cmds], q_test())
        for c, e in zip(cmds, v):
            match e:
                case jv.Act():
                    return c

    def p2(cmds):
        v = jv.judge([jv.state(on=c) for c in cmds], q_test())
        return [c for c, e in zip(cmds, v) if isinstance(e, jv.Act)]

    def p3(cmds):
        v = jv.judge([jv.state(on=c) for c in cmds], q_test())
        for i, e in enumerate(v):
            if isinstance(e, jv.Ignore):
                return i

    def p4(cmds):
        v = jv.judge([jv.state(on=c) for c in cmds], q_test())
        match v[0]:
            case jv.Act():
                return 1

    def ok(cmds):                                                   # 正确写法：cut 向量再解包，不报
        es = jv.cut(jv.judge([jv.state(on=c) for c in cmds], q_test()))
        for c, e in zip(cmds, es):
            match e:
                case jv.Act():
                    return c
    for p in (p1, p2, p3, p4):
        rep = check(p)
        assert any("J-01" in m and "jv.cut" in m for m in rep.errors), (p.__name__, rep)
    assert not check(ok).errors


def test_d2_transform_accepts_material_lists():
    with rt_with() as rt:
        @jv.program()
        def f():
            xs = jv.transform(lambda m: ["a", "b"], jv.lit("x"))
            n = jv.transform(lambda ys: len(ys), xs)
            s = jv.state(on=n, ctx=xs)
            r = jv.judge(s, q_test("有 2 吗"))
            jv.consume([jv.cut(r[0])])
            return n.content, [m.content for m in xs]
        assert f() == (2, ["a", "b"])
        assert rt.stats["transform"] == 2


def test_d3_plan_cut_in_loop_of_outer_vector_is_one_layer():
    q = jv.test("x", calib=jv.calib(K))

    @jv.program(budget=jv.Budget(layers=1))
    def p(ps):
        v = jv.judge([jv.state(on=x) for x in ps], q)
        out = []
        for x, r in zip(ps, v):
            e = jv.cut(r[0])
            jv.consume([e]); out.append(e)
        return out
    with rt_with() as rt:
        rep = jv.plan(p, rt=rt)
    assert repr(rep.layers) == "1" and not any(w.startswith("W-cost") for w in rep.warnings), rep.warnings


def test_locate_regression_off_by_one_fixed_and_noprogress_is_explicit():
    from foundation.jv.examples import six
    rows = {r["程序"]: r for r in six.run_all()}
    assert "'c-5'" in rows["定位回归"]["结果"], rows["定位回归"]              # 第 5 个提交起变慢（修前返回 c-6）
    with rt_with() as rt:
        @jv.program()
        def p():
            n = [3]
            lp = jv.loop(bound=5, variant=jv.decreasing(lambda: n[0]))
            for it in lp:
                pass                                                 # 计量不降 → 第 2 轮无进展
            return lp
        lp = p()
    assert lp.stopped_by == "noprogress" and isinstance(lp.stopped_exit, jv.Unsure) and lp.stopped_exit.cause == "noprogress"
    assert lp.stopped_exit.consumed and rt.stats["unsure"] == 1
    assert any(w.startswith("W-noprogress") for w in rt.stats["warnings"])
