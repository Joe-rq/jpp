"""API 契约表（README §9）逐格测试：每个公开名字七列——输入、输出、空输入、taint、刷新点、消费、失败。
一格一条参数化用例；README 表里标「未定」的格不在这里（列在 设计/G45-猜点分类.md）。全部 FakeClient，$0。"""

from __future__ import annotations

import json
import os
import sys
import warnings

import pytest

sys.path.insert(0, os.path.join(os.path.dirname(__file__), "..", ".."))
import foundation.jv as jv  # noqa: E402


def rt_with(client=None, **kw):
    rt = jv.Runtime(client=client or jv.FakeClient(), **kw)
    for k in ("t.k", "s.k", "m.k"):
        rt.calib.put(k, hi=0.6, lo=0.3, n=100, status="上岗")
    return rt


def boom(text, qid, q):
    raise RuntimeError("boom")


def layers(rt):
    return len(rt.stats["layers"])


def run(fn, client=None, budget=None):
    """在一个程序帧里跑 fn(rt)，收集 warning；返回 (结果, warnings, rt)。"""
    with rt_with(client=client) as rt:
        @jv.program(budget=budget or jv.Budget(calls=20), check_static=False)
        def p():
            return fn(rt)
        with warnings.catch_warnings(record=True) as w:
            warnings.simplefilter("always")
            out = p()
        return out, [str(x.message) for x in w], rt


T = lambda: jv.test("提到截止日期吗", calib=jv.calib("t.k"))          # noqa: E731
S = lambda: jv.select("哪个", calib=jv.calib("s.k"))         # noqa: E731
M = lambda: jv.measure("档", scale=("低", "中", "高"), calib=jv.calib("m.k"))   # noqa: E731


# ------------------------------------------------------------------ 每格一个函数：名字 → 列 → 断言
def lit_output():        assert isinstance(jv.lit("a"), jv.Mat) and isinstance(jv.mat("a"), jv.Mat)
def lit_empty():         assert jv.lit("").content == ""
def lit_taint():         assert jv.lit("a").taint == "trusted" and jv.lit("a").origin == ("lit",)
def lit_norefresh():
    out, _, rt = run(lambda rt: (jv.lit("a"), layers(rt))[1]); assert out == 0
def lit_fail_mat_host():
    _, ws, _ = run(lambda rt: jv.mat({"host": 1})); assert any(w.startswith("W-literal-from-host") for w in ws)

def state_output():      assert isinstance(jv.state(on=jv.lit("a")), jv.State)
def state_empty_on():
    with pytest.raises(jv.JvError): jv.state(on=None)
def state_taint():
    s = jv.state(on=jv.lit("a"), ctx=[jv.Mat(content="u", origin=("gen",), taint="untrusted")])
    assert s.resolved().taint == "untrusted"
def state_fail_host_value():
    with pytest.raises(jv.JvTypeError): jv.state(on="裸字符串")

def q_output():          assert isinstance(T(), jv.Q) and isinstance(S(), jv.Q) and isinstance(M(), jv.Q)
def q_fail_literal_calib():
    with pytest.raises((jv.JvError, TypeError)): jv.test("x", calib="t.k")

def judge_output():
    out, _, _ = run(lambda rt: type(jv.judge(jv.state(on=jv.lit("a")), T())).__name__); assert out == "Readings"
def judge_vec_output():
    out, _, _ = run(lambda rt: type(jv.judge([jv.state(on=jv.lit("a"))], T())).__name__); assert out == "ReadingsVec"
def judge_empty_vec():
    out, _, _ = run(lambda rt: jv.cut(jv.judge([], T()))); assert out == []
def judge_lazy():
    out, _, _ = run(lambda rt: (jv.judge(jv.state(on=jv.lit("a")), T()), layers(rt))[1]); assert out == 0
def judge_fail_client():
    def f(rt):
        e = jv.cut(jv.judge(jv.state(on=jv.lit("a")), T())[0]); jv.consume([e], unsure=jv.drop); return e.cause
    out, _, _ = run(f, client=jv.FakeClient(boom)); assert out == "fail"

def cut_output():
    out, _, _ = run(lambda rt: jv.cut(jv.judge(jv.state(on=jv.lit("a")), T())[0]).kind); assert out in ("act", "ignore", "unsure")
def cut_vec_aligned():
    def f(rt):
        ss = [jv.state(on=jv.lit(f"x{i}")) for i in range(3)]
        es = jv.cut(jv.judge(ss, T())); jv.consume(es, unsure=jv.drop); return len(es)
    out, _, _ = run(f); assert out == 3
def cut_empty():
    out, _, _ = run(lambda rt: jv.cut([])); assert out == []
def cut_taint_inherits():
    def f(rt):
        s = jv.state(on=jv.Mat(content="u", origin=("gen",), taint="untrusted"))
        e = jv.cut(jv.judge(s, T())[0]); jv.consume([e], unsure=jv.drop); return e.taint
    out, _, _ = run(f); assert out == "untrusted"
def cut_refresh():
    out, _, _ = run(lambda rt: (jv.cut(jv.judge(jv.state(on=jv.lit("a")), T())[0]), layers(rt))[1]); assert out == 1
def cut_unsure_must_consume():
    with pytest.raises(jv.JvError, match="J-05"):
        run(lambda rt: jv.cut(jv.judge(jv.state(on=jv.lit("a")), T())[0]).kind, client=jv.FakeClient(boom))
def cut_fail_literal_calib():
    with pytest.raises((jv.JvError, TypeError)):
        run(lambda rt: jv.cut(jv.judge(jv.state(on=jv.lit("a")), T())[0], calib="t.k"))

def order_output():
    def f(rt):
        rs = jv.judge([jv.state(on=jv.lit("a")), jv.state(on=jv.lit("这里提到截止日期"))], T())
        t = rs.order(); jv.consume(jv.cut(rs), unsure=jv.drop); return t
    out, _, _ = run(f); assert isinstance(out, list) and all(isinstance(g, list) for g in out) and sorted(sum(out, [])) == [0, 1]
def order_empty():
    out, _, _ = run(lambda rt: jv.judge([], T()).order()); assert out == []
def order_refresh():
    out, _, _ = run(lambda rt: (jv.judge([jv.state(on=jv.lit("a"))], T()).order(), layers(rt))[1]); assert out == 1
def order_noconsume():
    def f(rt):
        rs = jv.judge([jv.state(on=jv.lit("a"))], T()); rs.order(); return rt.stats["exits"]
    out, _, _ = run(f); assert out == 0
def order_fail_last_tier():
    def f(rt):
        rs = jv.judge([jv.state(on=jv.lit("a")), jv.state(on=jv.lit("b"))], M()); t = rs.order()
        jv.consume(jv.cut(rs), unsure=jv.drop); return t
    out, _, _ = run(f, client=jv.FakeClient(boom)); assert out == [[0, 1]]
def order_single_state_error():
    def f(rt):
        rs = jv.judge(jv.state(on=jv.lit("a")), T())
        with pytest.raises(jv.JvTypeError): rs.order()
        jv.consume([jv.cut(rs)], unsure=jv.drop)
    run(f)

def agg_output():
    out, _, _ = run(lambda rt: type(jv.judge(jv.state(on=jv.lit("a")), T()).agg()).__name__); assert out == "Readings"
def agg_refresh():
    out, _, _ = run(lambda rt: (jv.judge(jv.state(on=jv.lit("a")), T()).agg(), layers(rt))[1]); assert out == 1

def readings_no_arith():
    def f(rt):
        r = jv.judge(jv.state(on=jv.lit("a")), T())[0]
        for op in (lambda: r > r, lambda: r + 1, lambda: float(r), lambda: bool(r)):
            with pytest.raises(jv.JvTypeError): op()
        jv.consume([jv.cut(r)], unsure=jv.drop)
    run(f)

def gen_output():
    out, _, _ = run(lambda rt: jv.gen("p", ctx=[jv.lit("a")], n=3, retry_seq=0, generator=lambda p, c, n, s: ["x", "y"]))
    assert len(out) == 2 and all(isinstance(m, jv.Mat) for m in out)
def gen_n_is_upper_bound():
    out, _, _ = run(lambda rt: len(jv.gen("p", ctx=[], n=5, retry_seq=0, generator=lambda p, c, n, s: ["only"]))); assert out == 1
def gen_empty_ctx_taint_trusted():
    out, _, _ = run(lambda rt: jv.gen("p", ctx=[], n=1, retry_seq=0, generator=lambda p, c, n, s: ["x"])[0].taint); assert out == "trusted"
def gen_taint_joins_ctx():
    u = jv.Mat(content="u", origin=("gen",), taint="untrusted")
    out, _, _ = run(lambda rt: jv.gen("p", ctx=[u], n=1, retry_seq=0, generator=lambda p, c, n, s: ["x"])[0].taint); assert out == "untrusted"
def gen_ctx_elements_are_mats():
    seen = {}
    def g(p, c, n, s): seen["c"] = c; return ["x"]
    run(lambda rt: jv.gen("p", ctx=[jv.lit("a")], n=1, retry_seq=0, generator=g)); assert all(isinstance(m, jv.Mat) for m in seen["c"])
def gen_sends_immediately_no_layer():
    out, _, _ = run(lambda rt: (jv.gen("p", ctx=[], n=1, retry_seq=0, generator=lambda p, c, n, s: ["x"]), layers(rt))[1]); assert out == 0
def gen_fail_empty():
    def g(p, c, n, s): raise ValueError("x")
    out, ws, _ = run(lambda rt: jv.gen("p", ctx=[], n=1, retry_seq=0, generator=g)); assert out == [] and any("W-gen-fail" in w for w in ws)
def gen_missing_retry_seq():
    with pytest.raises(jv.JvError, match="J-13"): run(lambda rt: jv.gen("p", ctx=[], n=1, generator=lambda *a: []))

ACT = jv.register_action("contract_ok", fn=lambda m: "ok", taint_out="inherit", reason="t")
IRR = jv.register_action("contract_irr", fn=lambda m: "done", taint_out="inherit", reversible=False, reason="t")
BAD = jv.register_action("contract_bad", fn=lambda m: (_ for _ in ()).throw(ValueError("do boom")), taint_out="inherit", reason="t")

def do_output_future():
    out, _, _ = run(lambda rt: type(jv.do(ACT, jv.lit("a"), iter_seq=0)).__name__); assert out == "MatFuture"
def do_content_resolves():
    out, _, _ = run(lambda rt: jv.do(ACT, jv.lit("a"), iter_seq=0).content); assert out == "ok"
def do_return_value_materializes_to_mat():
    out, _, _ = run(lambda rt: jv.do(ACT, jv.lit("a"), iter_seq=0)); assert isinstance(out, jv.Mat) and out.content == "ok"
def do_taint_inherit():
    u = jv.Mat(content="u", origin=("gen",), taint="untrusted")
    out, _, _ = run(lambda rt: jv.do(ACT, u, iter_seq=0).taint); assert out == "untrusted"
def do_lazy_until_content():
    out, _, _ = run(lambda rt: (jv.do(ACT, jv.lit("a"), iter_seq=0), rt.stats["do"], layers(rt))[1:]); assert out == (1, 0)
def do_only_flush_is_not_a_layer():
    out, _, _ = run(lambda rt: (jv.do(ACT, jv.lit("a"), iter_seq=0).content, layers(rt))[1]); assert out == 0
def do_fail_is_value():
    out, _, _ = run(lambda rt: jv.on_fail(jv.do(BAD, jv.lit("a"), iter_seq=0), jv.lit("alt")).content); assert out == "alt"
def do_missing_iter_seq():
    with pytest.raises(jv.JvError, match="J-13"): run(lambda rt: jv.do(ACT, jv.lit("a")))
def do_irreversible_needs_trusted_act():
    def f(rt):
        e = jv.cut(jv.judge(jv.state(on=jv.lit("这里提到截止日期")), T())[0]); jv.consume([e], unsure=jv.drop)
        return jv.do(IRR, jv.lit("a"), iter_seq=0, guard=e).content
    out, _, _ = run(f); assert out == "done"
def do_guard_pick_rejected():
    def f(rt):
        e = jv.cut(jv.judge(jv.state(on=jv.lit("x"), over=[jv.lit("x"), jv.lit("y")]), S())); jv.consume([e], unsure=jv.drop)
        return jv.do(IRR, jv.lit("a"), iter_seq=0, guard=e).content
    with pytest.raises(jv.JvError, match="J-08"): run(f)
def do_guard_consumes_exit():
    def f(rt):
        e = jv.cut(jv.judge(jv.state(on=jv.lit("这里提到截止日期")), T())[0])
        jv.do(IRR, jv.lit("a"), iter_seq=0, guard=e).content; return e.consumed
    out, _, _ = run(f); assert out is True

def ask_raises_pending():
    def f(rt): return jv.ask(jv.state(on=jv.lit("a")), T())
    with pytest.raises(jv.Pending): run(f)
def ask_answer_replay():
    with rt_with() as rt:
        @jv.program(budget=jv.Budget(calls=5), check_static=False)
        def p(): return jv.ask(jv.state(on=jv.lit("a")), T())
        try:
            p()
        except jv.Pending as e:
            jv.answer(e.key, "act")
        out = p()
        assert isinstance(out, jv.Act) and out.taint == "trusted"
def ask_budget_escalate():
    def f(rt): return jv.ask(jv.state(on=jv.lit("a")), T())
    with pytest.raises(jv.JvError, match="J-07"): run(f, budget=jv.Budget(escalate=0))
def answer_bad_kind():
    with rt_with() as rt:
        with pytest.raises(jv.JvError): jv.answer("k", "bogus")

def transform_output_mat():
    out, _, _ = run(lambda rt: jv.transform(lambda m: m.content.upper(), jv.lit("a"))); assert isinstance(out, jv.Mat) and out.content == "A"
def transform_list_wraps_each():
    out, _, _ = run(lambda rt: jv.transform(lambda m: ["x", "y"], jv.lit("a"))); assert [m.content for m in out] == ["x", "y"]
def transform_empty_list():
    out, _, _ = run(lambda rt: jv.transform(lambda m: [], jv.lit("a"))); assert out == []
def transform_taint_joins_args():
    u = jv.Mat(content="u", origin=("gen",), taint="untrusted")
    out, _, _ = run(lambda rt: jv.transform(lambda a, b: "z", jv.lit("a"), u).taint); assert out == "untrusted"
def transform_receives_mats_and_lists():
    seen = {}
    def f(a, bs): seen["t"] = (type(a).__name__, type(bs).__name__, type(bs[0]).__name__); return "z"
    run(lambda rt: jv.transform(f, jv.lit("a"), [jv.lit("b")])); assert seen["t"] == ("Mat", "list", "Mat")
def transform_refreshes_do_inputs():
    out, _, _ = run(lambda rt: jv.transform(lambda m: m.content + "!", jv.do(ACT, jv.lit("a"), iter_seq=0)).content); assert out == "ok!"
def transform_fail_is_value():
    def bad(m): raise ValueError("x")
    out, ws, _ = run(lambda rt: jv.transform(bad, jv.lit("a"))); assert "fail" in out.content and any("W-transform-fail" in w for w in ws)

def loop_yields_n():
    out, _, _ = run(lambda rt: [it.n for it in jv.loop(bound=3, variant=jv.decreasing(lambda: 0))][:1]); assert out == [0]
def loop_missing_variant():
    with pytest.raises(jv.JvError, match="J-06"): run(lambda rt: list(jv.loop(bound=3)))
def loop_noprogress_is_consumed_unsure():
    def f(rt):
        n = 0
        for it in jv.loop(bound=5, variant=jv.decreasing(lambda: 7)):
            n += 1
        return n, rt.stats["unsure"]
    out, ws, _ = run(f); assert out[0] <= 2 and out[1] >= 1 and any("W-noprogress" in w for w in ws)

def handle_consumes_and_returns():
    def f(rt):
        e = jv.cut(jv.judge(jv.state(on=jv.lit("a")), T())[0]); r = jv.handle(e, then=jv.drop); return e.consumed, r
    out, _, _ = run(f, client=jv.FakeClient(boom)); assert out == (True, None)
def handle_escalate_asks():
    def f(rt):
        e = jv.cut(jv.judge(jv.state(on=jv.lit("a")), T())[0]); return jv.handle(e, then=jv.escalate)
    with pytest.raises(jv.Pending): run(f, client=jv.FakeClient(boom))
def consume_output_list():
    def f(rt):
        es = jv.cut(jv.judge([jv.state(on=jv.lit("a")), jv.state(on=jv.lit("b"))], T()))
        return jv.consume(es, unsure=jv.drop), [e.consumed for e in es]
    out, _, _ = run(f, client=jv.FakeClient(boom)); assert out[0] == [None, None] and out[1] == [True, True]
def consume_empty():
    out, _, _ = run(lambda rt: jv.consume([], unsure=jv.drop)); assert out == []
def consume_act_passthrough():
    def f(rt):
        es = jv.cut(jv.judge([jv.state(on=jv.lit("这里提到截止日期"))], T())); return jv.consume(es, unsure=jv.drop)[0].kind
    out, _, _ = run(f); assert out in ("act", "ignore")

def escalate_output():
    out, _, _ = run(lambda rt: jv.escalate([1, 2], note="n")); assert isinstance(out, jv.Escalated) and out.payload == [1, 2] and out.note == "n"
def escalate_counts_budget():
    with pytest.raises(jv.JvError, match="J-07"): run(lambda rt: (jv.escalate(1), jv.escalate(2)), budget=jv.Budget(escalate=1))
def escalate_no_layer():
    out, _, _ = run(lambda rt: (jv.escalate([1]), layers(rt))[1]); assert out == 0
def escalated_in_return_passes_j05():
    out, _, _ = run(lambda rt: {"x": jv.escalate(["a"])}); assert isinstance(out["x"], jv.Escalated)

def on_fail_passthrough():
    out, _, _ = run(lambda rt: jv.on_fail(jv.lit("ok"), jv.lit("alt")).content); assert out == "ok"

def budget_defaults_unbounded():
    b = jv.Budget(); assert (b.calls, b.cost, b.layers, b.escalate, b.unsure) == (None,) * 5
def budget_layers_counts_judge_layers():
    def f(rt):
        jv.cut(jv.judge(jv.state(on=jv.lit("a")), T())[0]); jv.do(ACT, jv.lit("a"), iter_seq=0).content
        e = jv.cut(jv.judge(jv.state(on=jv.lit("b")), T())[0]); jv.consume([e], unsure=jv.drop); return layers(rt)
    out, _, _ = run(f); assert out == 2
def budget_layers_exceeded_marks_unsure_budget():
    def f(rt):
        jv.cut(jv.judge(jv.state(on=jv.lit("a")), T())[0])
        e = jv.cut(jv.judge(jv.state(on=jv.lit("b")), T())[0]); jv.consume([e], unsure=jv.drop); return e.cause
    out, _, _ = run(f, budget=jv.Budget(layers=1)); assert out == "budget"

def program_static_error_raises():
    @jv.program(budget=jv.Budget(calls=5))
    def p(): return jv.cut(jv.judge(jv.state(on=jv.lit("a")), T())[0], calib="t.k")
    with rt_with():
        with pytest.raises(jv.JvError): p()
def program_returns_materialized():
    out, _, _ = run(lambda rt: [jv.do(ACT, jv.lit("a"), iter_seq=0)]); assert isinstance(out[0], jv.Mat)

def fake_client_text_keys():
    seen = {}
    def rule(text, qid, q): seen["t"] = json.loads(text); seen["q"] = q; return None
    def f(rt):
        s = jv.state(on=jv.lit("A"), ctx=[jv.lit("C")], ref=[jv.lit("R")], over=[jv.lit("o1"), jv.lit("o2")])
        e = jv.cut(jv.judge(s, S())); jv.consume([e], unsure=jv.drop)
    run(f, client=jv.FakeClient(rule)); assert set(seen["t"]) == {"on", "ctx", "ref", "over"} and set(seen["q"]) >= {"type", "instructions"}
def fake_client_bad_body_is_error():
    def rule(text, qid, q): return {"type": "noul"}
    with pytest.raises(jv.JvError): run(lambda rt: jv.cut(jv.judge(jv.state(on=jv.lit("a")), T())[0]), client=jv.FakeClient(rule))

def register_action_trusted_needs_reason():
    with pytest.raises(jv.JvError): jv.register_action("x_contract", fn=lambda m: 1, taint_out="trusted")
def action_self_trusted_warns():
    a = jv.Action(name="selfy_contract", fn=lambda m: "ok", taint_out="trusted")
    _, ws, _ = run(lambda rt: jv.do(a, jv.lit("a"), iter_seq=0).content); assert any("W-self-trusted" in w for w in ws)

def exit_attrs():
    e = jv.Pick(2); u = jv.Unsure("band"); a = jv.At(1)
    assert e.k == 2 and a.level == 1 and u.cause == "band" and e.kind == "pick" and hasattr(e, "p") and hasattr(e, "detail") and e.consumed is False
def exit_eq_class_warns_false():
    with warnings.catch_warnings(record=True) as w:
        warnings.simplefilter("always"); r = (jv.Act() == jv.Act)
    assert r is False and any("W-cmp-type" in str(x.message) for x in w)
def exit_as_mat_derived_from():
    def f(rt):
        e = jv.cut(jv.judge(jv.state(on=jv.lit("a")), T())[0]); jv.consume([e], unsure=jv.drop); return e.as_mat()
    out, _, _ = run(f); assert isinstance(out, jv.Mat) and out.derived_from
def unsure_bad_cause():
    with pytest.raises(jv.JvError): jv.Unsure("whatever")

def mat_eq_by_content():   assert jv.lit("a") == jv.lit("a") and hash(jv.lit("a")) == hash(jv.lit("a")) and jv.lit("a") != jv.lit("b")
def mat_no_truth_len_in():
    m = jv.lit("abc")
    for op in (lambda: bool(m), lambda: len(m), lambda: "a" in m):
        with pytest.raises(jv.JvTypeError): op()

def allocate_output():
    def f(rt):
        rs = jv.judge([jv.state(on=jv.lit(f"x{i}")) for i in range(4)], T()); ks = jv.allocate(rs, 2)
        jv.consume(jv.cut(rs), unsure=jv.drop); return ks
    out, _, _ = run(f); assert len(out) == 2 and all(isinstance(k, int) for k in out)
def allocate_empty():
    out, _, _ = run(lambda rt: jv.allocate(jv.judge([], T()), 2)); assert out == []
def allocate_bad_k():
    with pytest.raises(jv.JvError): run(lambda rt: jv.allocate(jv.judge([jv.state(on=jv.lit("a"))], T()), -1))
def unsure_bound_output():
    def f(rt):
        rs = jv.judge([jv.state(on=jv.lit("a"))], T()); d = jv.unsure_bound(rs); jv.consume(jv.cut(rs), unsure=jv.drop); return d
    out, _, _ = run(f); assert isinstance(out, dict)
def budget_accessor():
    out, _, _ = run(lambda rt: jv.budget().calls, budget=jv.Budget(calls=7)); assert out == 7
def stats_keys():
    out, _, _ = run(lambda rt: set(jv.stats())); assert {"calls", "questions", "layers", "cost"} <= out

def fit_needs_registered():
    def f(rt):
        rs = jv.judge(jv.state(on=jv.lit("a")), T(), jv.test("B 吗", calib=jv.calib("t.k")))
        with pytest.raises(jv.JvError): jv.fit(jv.fitref("没注册"), rs[0], rs[1])
        jv.consume([jv.cut(rs[0]), jv.cut(rs[1])], unsure=jv.drop)
    run(f)


CELLS = [
    ("lit/mat", "输出", lit_output), ("lit/mat", "空输入", lit_empty), ("lit/mat", "taint", lit_taint),
    ("lit/mat", "刷新", lit_norefresh), ("lit/mat", "失败", lit_fail_mat_host),
    ("state", "输出", state_output), ("state", "空输入", state_empty_on), ("state", "taint", state_taint), ("state", "失败", state_fail_host_value),
    ("test/select/measure", "输出", q_output), ("test/select/measure", "失败", q_fail_literal_calib),
    ("judge", "输出", judge_output), ("judge", "输出向量", judge_vec_output), ("judge", "空输入", judge_empty_vec),
    ("judge", "刷新", judge_lazy), ("judge", "失败", judge_fail_client),
    ("cut", "输出", cut_output), ("cut", "向量对齐", cut_vec_aligned), ("cut", "空输入", cut_empty), ("cut", "taint", cut_taint_inherits),
    ("cut", "刷新", cut_refresh), ("cut", "消费", cut_unsure_must_consume), ("cut", "失败", cut_fail_literal_calib),
    ("Readings.order", "输出", order_output), ("Readings.order", "空输入", order_empty), ("Readings.order", "刷新", order_refresh),
    ("Readings.order", "消费", order_noconsume), ("Readings.order", "失败", order_fail_last_tier), ("Readings.order", "单状态", order_single_state_error),
    ("Readings.agg", "输出", agg_output), ("Readings.agg", "刷新", agg_refresh), ("Readings", "无算术", readings_no_arith),
    ("gen", "输出", gen_output), ("gen", "n 上限", gen_n_is_upper_bound), ("gen", "空输入", gen_empty_ctx_taint_trusted),
    ("gen", "taint", gen_taint_joins_ctx), ("gen", "ctx 元素", gen_ctx_elements_are_mats), ("gen", "刷新", gen_sends_immediately_no_layer),
    ("gen", "失败", gen_fail_empty), ("gen", "缺序号", gen_missing_retry_seq),
    ("do", "输出", do_output_future), ("do", ".content", do_content_resolves), ("do", "返回值", do_return_value_materializes_to_mat),
    ("do", "taint", do_taint_inherit), ("do", "刷新", do_lazy_until_content), ("do", "不计层", do_only_flush_is_not_a_layer),
    ("do", "失败", do_fail_is_value), ("do", "缺序号", do_missing_iter_seq), ("do", "守卫", do_irreversible_needs_trusted_act),
    ("do", "守卫 Pick", do_guard_pick_rejected), ("do", "守卫消费", do_guard_consumes_exit),
    ("ask", "Pending", ask_raises_pending), ("ask", "answer 重放", ask_answer_replay), ("ask", "预算", ask_budget_escalate), ("answer", "失败", answer_bad_kind),
    ("transform", "输出", transform_output_mat), ("transform", "列表", transform_list_wraps_each), ("transform", "空输入", transform_empty_list),
    ("transform", "taint", transform_taint_joins_args), ("transform", "实参", transform_receives_mats_and_lists),
    ("transform", "刷新", transform_refreshes_do_inputs), ("transform", "失败", transform_fail_is_value),
    ("loop", "输出", loop_yields_n), ("loop", "失败", loop_missing_variant), ("loop", "无进展", loop_noprogress_is_consumed_unsure),
    ("handle", "消费", handle_consumes_and_returns), ("handle", "escalate", handle_escalate_asks),
    ("consume", "输出", consume_output_list), ("consume", "空输入", consume_empty), ("consume", "Act 透传", consume_act_passthrough),
    ("escalate", "输出", escalate_output), ("escalate", "预算", escalate_counts_budget), ("escalate", "刷新", escalate_no_layer),
    ("escalate", "返回值", escalated_in_return_passes_j05),
    ("on_fail", "透传", on_fail_passthrough),
    ("Budget", "默认", budget_defaults_unbounded), ("Budget", "layers", budget_layers_counts_judge_layers), ("Budget", "超层", budget_layers_exceeded_marks_unsure_budget),
    ("program", "静态错", program_static_error_raises), ("program", "返回值", program_returns_materialized),
    ("FakeClient", "text 键", fake_client_text_keys), ("FakeClient", "失败", fake_client_bad_body_is_error),
    ("register_action", "失败", register_action_trusted_needs_reason), ("Action", "自封可信", action_self_trusted_warns),
    ("Exit", "属性", exit_attrs), ("Exit", "==类", exit_eq_class_warns_false), ("Exit", "as_mat", exit_as_mat_derived_from), ("Unsure", "cause", unsure_bad_cause),
    ("Mat", "相等", mat_eq_by_content), ("Mat", "真值/长度/in", mat_no_truth_len_in),
    ("allocate", "输出", allocate_output), ("allocate", "空输入", allocate_empty), ("allocate", "失败", allocate_bad_k),
    ("unsure_bound", "输出", unsure_bound_output), ("budget()", "输出", budget_accessor), ("stats", "输出", stats_keys),
    ("fit", "失败", fit_needs_registered),
]


@pytest.mark.parametrize("name,col,fn", CELLS, ids=[f"{n}:{c}" for n, c, _ in CELLS])
def test_contract_cell(name, col, fn):
    fn()


def test_contract_table_covers_public_api():
    """README §9 表的行 ⊇ 契约测试涉及的名字（表少写了名字就在这里挂）。"""
    readme = open(os.path.join(os.path.dirname(__file__), "..", "..", "src", "foundation", "jv", "README.md"), encoding="utf-8").read()
    sec = readme.split("## 9. API 契约表")[1].split("\n## ")[0]
    for n in {c[0] for c in CELLS}:
        assert n.split("/")[0].split(".")[0] in sec, f"README §9 缺 {n}"
