"""jv 包：类型纪律、执行模型、pass、账本。全部 FakeClient，$0。"""

from __future__ import annotations

import os
import sys
import warnings

import pytest

sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__)))))

import foundation.jv as jv  # noqa: E402
from foundation.jv.checker import check  # noqa: E402

K = "t.k"


def rt_with(tmp_path=None, client=None, **kw):
    rt = jv.Runtime(client=client or jv.FakeClient(), root=str(tmp_path) if tmp_path else None, **kw)
    rt.calib.put(K, hi=0.65, lo=0.35, n=30, status="上岗")
    rt.calib.put("sel.k", hi=0.5, lo=0.2, n=30, status="上岗")
    rt.calib.put("m.k", hi=0.6, lo=0.3, n=30, status="上岗")
    return rt


def q_test(text="提到截止日期吗", key=K, **kw):
    return jv.test(text, calib=jv.calib(key), **kw)


# ---------------------------------------------------------------- J-01
def test_j01_readings_have_no_arithmetic():
    with rt_with() as rt:
        s = jv.state(on=jv.lit("截止日期 明天"))
        r = jv.judge(s, q_test(), q_test("提到天气吗"))
        with pytest.raises(jv.JvTypeError, match="J-01"):
            float(r[0])
        with pytest.raises(jv.JvTypeError, match="J-01"):
            r[0] > r[1]
        with pytest.raises(jv.JvTypeError, match="J-01"):
            r[0] + 1
        with pytest.raises(jv.JvTypeError, match="J-01"):
            if r[0]:
                pass
        jv.cut(r[0]); jv.cut(r[1])
        rt.exits.clear()


def test_j01_static_match_on_judge():
    def prog():
        e = jv.judge(jv.state(on=jv.lit("x")), q_test())
        match e:
            case jv.Act():
                return 1
    rep = check(prog)
    assert any("J-01" in m and "取 [i] 再 jv.cut" in m for m in rep.errors), rep


# ---------------------------------------------------------------- J-03
def test_j03_literal_line_runtime_and_static():
    with pytest.raises(jv.JvTypeError, match="J-03"):
        jv.test("x", calib="t.k")
    with rt_with():
        r = jv.judge(jv.state(on=jv.lit("x")), q_test())
        with pytest.raises(jv.JvTypeError, match="J-03"):
            jv.cut(r[0], calib="t.k")

    def prog():
        q = jv.test("x", calib="perf.慢了")
    rep = check(prog)
    assert any("J-03" in m and "jv.calib('perf.慢了')" in m for m in rep.errors), rep


# ---------------------------------------------------------------- J-05
def test_j05_unconsumed_unsure_is_error():
    client = jv.FakeClient(rule=lambda t, qid, q: {"type": "noul", "noul": 0.5})
    with rt_with(client=client) as rt:
        @jv.program()
        def p():
            r = jv.judge(jv.state(on=jv.lit("x")), q_test())
            e = jv.cut(r[0])
            return "done"
        with pytest.raises(jv.JvError, match="J-05"):
            p()

        @jv.program()
        def p2():
            r = jv.judge(jv.state(on=jv.lit("x")), q_test())
            match jv.cut(r[0]):
                case jv.Act():
                    return "act"
                case jv.Unsure(c):
                    jv.handle(c, then=jv.drop)
                    return c
        assert p2() == "band"


# ---------------------------------------------------------------- J-11
def test_j11_host_value_into_slot():
    with rt_with():
        with pytest.raises(jv.JvTypeError, match="J-11"):
            jv.state(on="裸字符串")
        with pytest.raises(jv.JvTypeError, match="J-11"):
            jv.state(on=jv.lit("x"), ctx=[{"a": 1}])

    def host(x):
        return x

    def prog():
        s = jv.state(on=host("x"))
    rep = check(prog)
    assert any("J-11" in m and "jv.transform" in m for m in rep.errors), rep


# ---------------------------------------------------------------- J-13
def test_j13_seq_static_and_runtime():
    run = jv.Action("run", fn=lambda *a: "ok")

    def prog():
        for k in range(3):
            jv.do(run, jv.lit("x"))
            jv.gen("p", ctx=[], n=2, retry_seq=0)
    rep = check(prog)
    assert sum("J-13" in m for m in rep.errors) == 2, rep
    with rt_with():
        with pytest.raises(jv.JvError, match="J-13"):
            jv.do(run, jv.lit("x"))
        with pytest.raises(jv.JvError, match="J-13"):
            jv.gen("p", ctx=[], n=2)


# ---------------------------------------------------------------- §6.3 六种模式（J-17）
def test_j17_six_error_patterns():
    run = jv.Action("run", fn=lambda *a: "ok")

    def p1():                                             # 1 match judge
        e = jv.judge(jv.state(on=jv.lit("x")), q_test())
        match e:
            case jv.Act():
                pass

    def p2():                                             # 2 == jv.Act
        e = jv.cut(jv.judge(jv.state(on=jv.lit("x")), q_test())[0])
        if e == jv.Act:
            pass

    def p3():                                             # 3 r1 > r2
        r = jv.judge(jv.state(on=jv.lit("x")), q_test(), q_test("b"))
        if r[0] > r[1]:
            pass

    def p4():                                             # 4 常量序号
        for k in range(2):
            jv.do(run, jv.lit("x"), iter_seq=0)

    def p5():                                             # 5 字符串 calib
        r = jv.judge(jv.state(on=jv.lit("x")), q_test())
        jv.cut(r[0], calib="perf.慢了")

    def p6():                                             # 6 串行依赖
        x = jv.lit("x")
        for c in range(3):
            x = jv.do(run, x, iter_seq=c)

    msgs = {i: check(p).errors + check(p).warnings for i, p in enumerate([p1, p2, p3, p4, p5, p6], 1)}
    assert any("J-01" in m for m in msgs[1])
    assert any("W-cmp-type" in m and "isinstance" in m for m in msgs[2])
    assert any("J-01" in m and ".order()" in m for m in msgs[3])
    assert any("J-13" in m and "it.n" in m for m in msgs[4])
    assert any("J-03" in m for m in msgs[5])
    assert any("W-serial" in m for m in msgs[6])


# ---------------------------------------------------------------- 融合
def test_fusion_same_state_two_questions_one_call():
    client = jv.FakeClient()
    with rt_with(client=client) as rt:
        @jv.program()
        def p():
            s = jv.state(on=jv.lit("截止日期 明天"))
            r = jv.judge(s, q_test(), q_test("提到天气吗"))
            r2 = jv.judge(jv.state(on=jv.lit("截止日期 明天")), q_test("提到合同吗"))   # 同结构哈希 → 同组
            es = [jv.cut(r[0]), jv.cut(r[1]), jv.cut(r2[0])]
            jv.consume(es)
            return [e.kind for e in es]
        out = p()
    assert out == ["act", "ignore", "ignore"]
    assert client.calls == 1 and client.questions_asked == 3
    assert rt.stats["layers"] == [{"reason": "cut", "calls": 1, "questions": 3, "states": 2, "fused_groups": 1}]


def test_no_fuse_switch_gives_one_call_per_state_group():
    client = jv.FakeClient()
    with rt_with(client=client, passes={"fuse": False}) as rt:
        @jv.program()
        def p():
            s = jv.state(on=jv.lit("x"))
            r = jv.judge(s, q_test()); r2 = jv.judge(jv.state(on=jv.lit("x")), q_test("b"))
            jv.consume([jv.cut(r[0]), jv.cut(r2[0])])
        p()
    assert client.calls == 2


# ---------------------------------------------------------------- 裂变
def test_fission_splits_long_object_and_aggregates_exists():
    seen = []

    def rule(text, qid, q):
        seen.append(len(text))
        return {"type": "noul", "noul": 0.9 if "截止日期" in text else 0.05}
    client = jv.FakeClient(rule=rule)
    with rt_with(client=client) as rt:
        long = "无关内容。" * 300 + "截止日期是明天。" + "无关内容。" * 300      # ≫ 500 token
        @jv.program()
        def p():
            r = jv.judge(jv.state(on=jv.lit(long)), q_test())
            e = jv.cut(r[0]); e.__dict__["consumed"] = True
            return e.kind, r[0]._ans["chunks"]
        kind, chunks = p()
    assert kind == "act" and chunks >= 4 and rt.stats["fission"] == 1
    assert client.questions_asked == chunks and client.calls <= chunks   # 每块单独发；内容相同的块融合


# ---------------------------------------------------------------- 下沉
def test_lowering_short_candidates_choice_two_perms_mode():
    orders = []

    def rule(text, qid, q):
        if q["type"] == "choice":
            orders.append(list(q["criteria"]))
            return {"type": "choice", "choice": "c2", "probabilities": {"c0": 0.05, "c1": 0.05, "c2": 0.9}}
    client = jv.FakeClient(rule=rule)
    with rt_with(client=client) as rt:
        @jv.program()
        def p():
            cands = [jv.lit("a"), jv.lit("b"), jv.lit("c")]
            r = jv.judge(jv.state(on=jv.lit("哪个是 c"), over=cands), jv.select("哪个", calib=jv.calib("sel.k")))
            e = jv.cut(r)
            e.__dict__["consumed"] = True
            return e
        e = p()
    assert isinstance(e, jv.Pick) and e.k == 2
    assert client.calls == 1 and client.questions_asked == 2                 # 两个置换融合进同一次调用
    assert orders[0] == ["c0", "c1", "c2"] and orders[1] == ["c2", "c1", "c0"]


def test_lowering_perm_disagree_is_tie():
    def rule(text, qid, q):
        if q["type"] == "choice":
            first = list(q["criteria"])[0]
            return {"type": "choice", "choice": first, "probabilities": {k: (0.8 if k == first else 0.1) for k in q["criteria"]}}
    with rt_with(client=jv.FakeClient(rule=rule)):
        r = jv.judge(jv.state(on=jv.lit("x"), over=[jv.lit("a"), jv.lit("b")]), jv.select("哪个", calib=jv.calib("sel.k")))
        e = jv.cut(r)
        assert isinstance(e, jv.Unsure) and e.cause == "tie"


def test_lowering_long_candidates_k_noul():
    client = jv.FakeClient(rule=lambda t, qid, q: {"type": "noul", "noul": 0.9 if "c1" in q["instructions"] else 0.1})
    with rt_with(client=client) as rt:
        long = "x" * 600                                                       # ≥300 token 档，K=6 > K_max 4 → K-noul
        r = jv.judge(jv.state(on=jv.lit("q"), over=[jv.lit(long + str(i)) for i in range(6)]),
                     jv.select("哪个", calib=jv.calib("sel.k")))
        e = jv.cut(r)
        assert isinstance(e, jv.Pick) and e.k == 1
        assert client.questions_asked == 6 and client.calls == 1 and r[0]._ans["knoul"]


def test_lowering_untested_band_conservative_with_warning():
    with rt_with() as rt:
        mid = "y" * 250                                                       # 120–250 token 档：档案未测
        with warnings.catch_warnings(record=True) as w:
            warnings.simplefilter("always")
            r = jv.judge(jv.state(on=jv.lit("q"), over=[jv.lit(mid + str(i)) for i in range(3)]),
                         jv.select("哪个", calib=jv.calib("sel.k")))
            jv.cut(r)
        assert any("W-untested" in str(x.message) for x in w)
        assert r[0]._ans["knoul"]


def test_measure_lowers_to_score_and_at():
    with rt_with() as rt:
        q = jv.measure("多急", scale=["低", "中", "高"], calib=jv.calib("m.k"))
        r = jv.judge(jv.state(on=jv.lit("急！！")), q)
        e = jv.cut(r[0])
        assert isinstance(e, jv.At) and e.level == 2


# ---------------------------------------------------------------- 账本重放
def test_ledger_replay_second_run_zero_calls(tmp_path):
    def build(client):
        rt = rt_with(tmp_path, client=client)

        @jv.program(budget=jv.Budget(calls=10))
        def p():
            r = jv.judge(jv.state(on=jv.lit("截止日期 明天")), q_test(), q_test("提到天气吗"))
            es = [jv.cut(r[0]), jv.cut(r[1])]
            jv.consume(es)
            return [e.kind for e in es], [e.p for e in es]
        return rt, p
    c1 = jv.FakeClient()
    rt1, p1 = build(c1)
    with rt1:
        out1 = p1()
    c2 = jv.FakeClient(rule=lambda *a: {"type": "noul", "noul": 0.5})     # 若真调，读数会不同
    rt2, p2 = build(c2)
    with rt2:
        out2 = p2()
    assert out1 == out2 and c1.calls == 1 and c2.calls == 0 and rt2.stats["ledger_hits"] == 2


def test_j18_header_change_warns(tmp_path):
    with rt_with(tmp_path) as rt:
        @jv.program(budget=jv.Budget(calls=5))
        def p():
            return 1
        p()
    with rt_with(tmp_path) as rt:
        @jv.program(budget=jv.Budget(calls=6))
        def p():
            return 1
        with warnings.catch_warnings(record=True) as w:
            warnings.simplefilter("always")
            p()
        assert any("W-header" in str(x.message) for x in w)


# ---------------------------------------------------------------- transform
def test_transform_impure_detected_and_fusion_disabled(tmp_path):
    counter = {"n": 0}

    def impure(m):
        counter["n"] += 1
        return f"{m.content}#{counter['n']}"
    client = jv.FakeClient()
    with rt_with(tmp_path, client=client) as rt:
        @jv.program()
        def p():
            outs = []
            with warnings.catch_warnings(record=True) as w:
                warnings.simplefilter("always")
                for _ in range(2):                                   # 同一调用点、同输入
                    outs.append(jv.transform(impure, jv.lit("x")))
            a, b = outs
            assert any("W-impure" in str(x.message) for x in w)
            r1 = jv.judge(jv.state(on=jv.lit("同")), q_test())
            r2 = jv.judge(jv.state(on=jv.lit("同")), q_test("b"))
            jv.consume([jv.cut(r1[0]), jv.cut(r2[0])])
            return a.content, b.content
        a, b = p()
    assert a == "x#1" and b == "x#2"
    assert client.calls == 2                       # 本直线段禁融合
    assert rt.stats["transform"] == 2


def test_transform_output_is_mat_with_taint_and_lists():
    with rt_with():
        u = jv.Mat(content="不可信", origin=("gen",), taint="untrusted")
        outs = jv.transform(lambda m: [m.content + "1", m.content + "2"], u)
        assert [o.content for o in outs] == ["不可信1", "不可信2"] and all(o.taint == "untrusted" for o in outs)
        assert outs[0].origin[0] == "transform"


# ---------------------------------------------------------------- Pending 恢复即重放
def test_pending_then_resume_replays(tmp_path):
    client = jv.FakeClient(rule=lambda t, qid, q: {"type": "noul", "noul": 0.5})

    def build(c):
        rt = rt_with(tmp_path, client=c)

        @jv.program(budget=jv.Budget(escalate=2))
        def p():
            s = jv.state(on=jv.lit("含糊"))
            r = jv.judge(s, q_test())
            match jv.cut(r[0]):
                case jv.Act():
                    return "act"
                case jv.Unsure(c):
                    e = jv.handle(c, then=jv.escalate)     # band → 重跑仍 band → ask → Pending
                    return "human:" + e.kind
        return rt, p
    rt1, p1 = build(client)
    with rt1:
        with pytest.raises(jv.Pending) as ei:
            p1()
        key = ei.value.key
    assert client.calls == 2                                   # 首判 + band 重跑
    rt1.books.effects  # 已落盘
    c2 = jv.FakeClient(rule=lambda *a: {"type": "noul", "noul": 0.5})
    rt2, p2 = build(c2)
    with rt2:
        rt2.begin("p", jv.Budget(escalate=2)); rt2.answer(key, "act")
        assert p2() == "human:act"
    assert c2.calls == 0                                       # 全部重放，不付费


# ---------------------------------------------------------------- 预算（层边界）
def test_budget_layer_boundary_stops_and_marks_unsure_budget():
    client = jv.FakeClient()
    with rt_with(client=client) as rt:
        @jv.program(budget=jv.Budget(calls=1))
        def p():
            r1 = jv.judge(jv.state(on=jv.lit("a")), q_test())
            e1 = jv.cut(r1[0])                                # 第 1 层：1 次调用
            r2 = jv.judge(jv.state(on=jv.lit("b")), q_test())
            e2 = jv.cut(r2[0])                                # 第 2 层：超 calls=1 → 不发
            jv.consume([e1, e2])
            return e1.kind, e2
        k1, e2 = p()
    assert client.calls == 1 and isinstance(e2, jv.Unsure) and e2.cause == "budget"
    assert rt.stats["layers"][1]["stopped"]


# ---------------------------------------------------------------- 其余纪律
def test_j08_irreversible_do_needs_trusted_guard():
    deploy = jv.Action("deploy", fn=lambda *a: "ok", reversible=False)
    with rt_with():
        with pytest.raises(jv.JvError, match="J-08"):
            jv.do(deploy, jv.lit("x"), iter_seq=0)
        r = jv.judge(jv.state(on=jv.Mat(content="截止日期", origin=("gen",), taint="untrusted")), q_test())
        e = jv.cut(r[0])
        assert isinstance(e, jv.Act) and e.taint == "untrusted"
        with pytest.raises(jv.JvError, match="J-08"):
            jv.do(deploy, jv.lit("x"), iter_seq=0, guard=[e])           # untrusted 的 Act 不能单独放行
        r2 = jv.judge(jv.state(on=jv.lit("截止日期")), q_test())
        e2 = jv.cut(r2[0])
        f = jv.do(deploy, jv.lit("x"), iter_seq=0, guard=[e, e2])
        assert f.content == "ok"


def test_j09_insufficient_before_trusting_p():
    with rt_with():
        q = q_test(evidence=("ctx",))
        r = jv.judge(jv.state(on=jv.lit("截止日期")), q)
        e = jv.cut(r[0])
        assert isinstance(e, jv.Unsure) and e.cause == "insufficient"
        r2 = jv.judge(jv.state(on=jv.lit("截止日期"), ctx=[jv.lit("证据")]), q)
        assert isinstance(jv.cut(r2[0]), jv.Act)


def test_j02_self_reference_runtime():
    with rt_with():
        q = q_test()
        e = jv.cut(jv.judge(jv.state(on=jv.lit("截止日期")), q)[0])
        r = jv.judge(jv.state(on=jv.lit("截止日期"), ctx=[e]), q)
        with pytest.raises(jv.JvError, match="J-02"):
            jv.cut(r[0])
        q2 = q_test("另一题", key=K)
        assert jv.cut(jv.judge(jv.state(on=jv.lit("截止日期"), ctx=[e]), q2)[0]) is not None


def test_cold_calib_gives_unsure_cold_and_provisional_handler():
    with rt_with() as rt:
        r = jv.judge(jv.state(on=jv.lit("截止日期")), q_test(key="never.calibrated"))
        e = jv.cut(r[0])
        assert isinstance(e, jv.Unsure) and e.cause == "cold"
        p = jv.handle(e)
        assert isinstance(p, jv.Act) and p.provisional


def test_loop_stops_on_noprogress_and_bound():
    with rt_with() as rt:
        xs = [1, 2, 3]
        ns = []
        for it in jv.loop(bound=10, variant=jv.decreasing(lambda: len(xs))):
            ns.append(it.n)
            if it.n == 0:
                xs.pop()
        assert ns == [0, 1]                    # 第 2 轮变式不再下降 → noprogress 停
        assert rt.stats["noprogress"]
        with pytest.raises(jv.JvError, match="J-06"):
            jv.loop(bound=3)


def test_vectorized_judge_one_layer_and_order():
    client = jv.FakeClient(rule=lambda t, qid, q: {"type": "noul", "noul": 0.9 if "a" in t else (0.6 if "b" in t else 0.1)})
    with rt_with(client=client) as rt:
        ss = [jv.state(on=jv.lit(x)) for x in ("a", "b", "c")]
        v = jv.judge(ss, q_test("x"))
        es = jv.cut(v)
        assert [e.kind for e in es] == ["act", "unsure", "ignore"]
        assert client.calls == 3 and len(rt.stats["layers"]) == 1
        assert v.order() == [[0], [1], [2]]
        jv.consume(es)


def test_fail_flows_to_unsure_fail_and_on_fail():
    def boom(m):
        raise RuntimeError("炸")
    run = jv.Action("boom", fn=boom)
    with rt_with():
        f = jv.do(run, jv.lit("x"), iter_seq=0)
        r = jv.judge(jv.state(on=f), q_test())
        e = jv.cut(r[0])
        assert isinstance(e, jv.Unsure) and e.cause == "fail"
        alt = jv.on_fail(jv.do(run, jv.lit("y"), iter_seq=1), jv.lit("替代"))
        assert alt.content == "替代"


def test_fit_registry_constraints_and_bridge():
    with rt_with() as rt:
        with pytest.raises(ValueError, match="J-16"):
            rt.fits.register("f", trained_from="set-A", features=[(K, "test")], n=10, error_rate=0.1, fn=lambda p: p)
        rt.fits.register("f", trained_from="set-A", features=[(K, "test"), (K, "test")], n=100, error_rate=0.1,
                         fn=lambda a, b: (a + b) / 2)
        r = jv.judge(jv.state(on=jv.lit("截止日期")), q_test(), q_test("b"))
        with pytest.raises(jv.JvError, match="J-16"):
            jv.fit(jv.fitref("nope"), r[0], r[1])
        s = jv.fit(jv.fitref("f"), r[0], r[1])
        rt.calib.put("fit.k", hi=0.4, lo=0.2, n=30, status="上岗", set_id="set-B")
        e = jv.cut(s, calib=jv.calib("fit.k"))
        assert isinstance(e, jv.Act)
        rt.calib.put("fit.same", hi=0.4, lo=0.2, n=30, status="上岗", set_id="set-A")
        with pytest.raises(jv.JvError, match="J-16"):
            jv.cut(s, calib=jv.calib("fit.same"))
        jv.consume([e])


def test_match_and_isinstance_consume_exits_exact_type():
    with rt_with() as rt:
        e = jv.cut(jv.judge(jv.state(on=jv.lit("截止日期")), q_test())[0])
        assert not e.__dict__["consumed"]
        match e:
            case jv.Act():
                pass
        assert e.__dict__["consumed"] and jv.Exit._last_matched is e
        u = jv.Unsure("band")
        # J-05：裸 isinstance 对 Unsure 只是看一眼，不消费（ir-impl-4）；match 才消费
        assert isinstance(u, jv.Unsure) and not u.__dict__["consumed"] and u.kind == "unsure"
        match u:
            case jv.Unsure(c):
                assert c == "band"
        assert u.__dict__["consumed"]
        assert repr(u).startswith("Unsure('band')")


def test_vectorized_cold_handle_keeps_alignment():
    import json

    def rule(text, qid, q):
        st = json.loads(text); k = int(st["on"][-1]) % 3
        return {"type": "choice", "choice": f"c{k}", "probabilities": {o: (1.0 if o == f"c{k}" else 0.0) for o in q["criteria"]}}
    with rt_with(client=jv.FakeClient(rule=rule)) as rt:
        q = jv.select("哪个", calib=jv.calib("cold.k"))
        es = jv.cut(jv.judge([jv.state(on=jv.lit(f"t{i}"), over=[jv.lit(d) for d in "甲乙丙"]) for i in range(5)], q))
        got = []
        for e in es:
            match e:
                case jv.Unsure(c):
                    got.append(jv.handle(c).k)
        assert got == [0, 1, 2, 0, 1]
