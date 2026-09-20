"""G6 零上下文读者之后的修补（ir-impl-5）：列表型 transform 失败形状、来源链保留、单候选 select 不发调用、
drop 与 escalate 两条记账、通配 case 的静态警告、守卫肯定命题、register_action(inherit) 无 reason。全部 FakeClient，$0。"""

from __future__ import annotations

import os
import sys
import warnings

import pytest

sys.path.insert(0, os.path.join(os.path.dirname(__file__), "..", ".."))
import foundation.jv as jv  # noqa: E402


def rt_with(client=None):
    rt = jv.Runtime(client=client or jv.FakeClient())
    for k in ("t.k", "s.k"):
        rt.calib.put(k, hi=0.6, lo=0.3, n=100, status="上岗")
    return rt


def run(fn, client=None, budget=None, check_static=False):
    with rt_with(client) as rt:
        @jv.program(budget=budget or jv.Budget(calls=20), check_static=check_static)
        def p():
            return fn(rt)
        with warnings.catch_warnings(record=True) as w:
            warnings.simplefilter("always")
            out = p()
        return out, [str(x.message) for x in w], rt


T = lambda: jv.test("是正常的业务邮件吗", calib=jv.calib("t.k"))    # noqa: E731
S = lambda: jv.select("哪个最忠实", calib=jv.calib("s.k"))          # noqa: E731


def unsure_rule(text, qid, q):
    if q["type"] == "noul":
        return {"noul": 0.5}
    return None


# ---------------------------------------------------------------- 1. 列表型 transform 失败 → 空列表（形状不变）
def test_transform_list_annotation_fail_returns_empty_list():
    def 过校验(cands) -> list:
        raise ValueError("校验器挂了")
    def body(rt):
        xs = jv.transform(过校验, [jv.lit("a"), jv.lit("b")])
        return xs, jv.on_fail(xs, ["替代"])
    (out, alt), ws, _ = run(body)
    assert isinstance(out, list) and out == [] and not out
    assert isinstance(out, jv.FailList) and out.fail is not None and "校验器挂了" in out.fail.reason
    assert any(w.startswith("W-transform-fail") and "空列表" in w for w in ws)
    assert alt == ["替代"]


def test_transform_shape_memory_makes_fail_a_list_next_time():
    calls = {"n": 0}
    def 过滤(cands):                                  # 无注解：第一次成功返回列表，第二次抛异常
        calls["n"] += 1
        if calls["n"] == 2:
            raise RuntimeError("第二次挂")
        return [c.content for c in cands]
    def body(rt):
        a = jv.transform(过滤, [jv.lit("x")])
        b = jv.transform(过滤, [jv.lit("y")])
        return a, b
    (a, b), ws, _ = run(body)
    assert [m.content for m in a] == ["x"]
    assert isinstance(b, list) and b == [] and isinstance(b, jv.FailList)


def test_transform_unannotated_first_fail_is_single_fail_mat():
    def 坏(m):
        raise RuntimeError("x")
    def body(rt):
        m = jv.transform(坏, jv.lit("a"))
        return m, jv.on_fail(m, "替代")
    (out, alt), ws, _ = run(body)
    assert isinstance(out, jv.Mat) and "fail" in out.content and alt == "替代"


# ---------------------------------------------------------------- 2. transform 子集 / 重排保留原 Mat 与来源链
def test_transform_subset_keeps_original_mats_and_origin():
    def body(rt):
        cands = jv.gen("改写", ctx=[jv.lit("原文")], n=3, retry_seq=0,
                       generator=lambda prompt, ctx, n, retry_seq: ["北京市海淀区1号", "错的", "上海市浦东区2号"])
        def 过校验(cs):
            return [c.content for c in cs if c.content.endswith("号")]
        kept = jv.transform(过校验, cands)
        return cands, kept
    (cands, kept), _, _ = run(body)
    assert len(kept) == 2
    assert kept[0] is cands[0] and kept[1] is cands[2]              # 就是原来的材料对象
    assert kept[0].origin[:1] == ("gen",)                            # 来源链没断
    assert kept[0].taint == cands[0].taint


def test_transform_reorder_keeps_mats_new_content_is_new_mat():
    def body(rt):
        ms = [jv.lit("a"), jv.lit("b")]
        out = jv.transform(lambda xs: [xs[1].content, xs[0].content, "c"], ms)
        return ms, out
    (ms, out), _, _ = run(body)
    assert out[0] is ms[1] and out[1] is ms[0]
    assert out[2] is not ms[0] and out[2].content == "c" and out[2].origin[:1] == ("transform",)


# ---------------------------------------------------------------- 3. select 的 over 只剩一个候选：不发调用，Pick(0)
def test_select_single_candidate_is_trivial_pick_without_call():
    def body(rt):
        e = jv.cut(jv.judge(jv.state(on=jv.lit("原文"), over=[jv.lit("唯一候选")]), S()))
        return e, rt.stats["calls"], rt.stats.get("trivial", 0)
    (e, calls, trivial), _, _ = run(body)
    assert isinstance(e, jv.Pick) and e.k == 0 and e.p == 1.0
    assert e.detail.get("trivial") and calls == 0 and trivial == 1


def test_select_single_candidate_trivial_even_when_calib_cold():
    with jv.Runtime(client=jv.FakeClient()) as rt:                  # s.cold 无校准记录
        @jv.program(budget=jv.Budget(calls=5), check_static=False)
        def p():
            return jv.cut(jv.judge(jv.state(on=jv.lit("原文"), over=[jv.lit("唯一")]),
                                   jv.select("哪个", calib=jv.calib("s.cold"))))
        e = p()
    assert isinstance(e, jv.Pick) and e.k == 0


# ---------------------------------------------------------------- 4. drop 与 escalate 两条记账
def _mail_like(rt, how: str):
    exits = jv.cut(jv.judge([jv.state(on=jv.lit(f"m{i}")) for i in range(3)], T()))
    交人 = [i for i, e in enumerate(exits) if isinstance(e, jv.Unsure)]
    if how == "drop-then-escalate":
        jv.consume(exits, unsure=jv.drop)
        return jv.escalate(交人, note="攒起来交人")
    if how == "escalate-with-exits":
        return jv.escalate(交人, note="攒起来交人", exits=[e for e in exits if isinstance(e, jv.Unsure)])
    if how == "drop-only":
        jv.consume(exits, unsure=jv.drop)
        return 交人
    raise AssertionError(how)


def test_drop_then_escalate_warns():
    out, ws, _ = run(lambda rt: _mail_like(rt, "drop-then-escalate"), client=jv.FakeClient(unsure_rule))
    assert isinstance(out, jv.Escalated) and out.payload == [0, 1, 2]
    assert sum(1 for w in ws if w.startswith("W-drop-vs-escalate")) == 1


def test_escalate_with_exits_consumes_and_no_warning():
    out, ws, rt = run(lambda rt: _mail_like(rt, "escalate-with-exits"), client=jv.FakeClient(unsure_rule))
    assert isinstance(out, jv.Escalated) and len(out.exits) == 3
    assert all(e.consumed and e.detail.get("consumed_by") == "escalate" for e in out.exits)
    assert not any(w.startswith("W-drop-vs-escalate") for w in ws)
    assert not any(w.startswith("J-05") for w in ws)


def test_drop_only_no_warning():
    out, ws, _ = run(lambda rt: _mail_like(rt, "drop-only"), client=jv.FakeClient(unsure_rule))
    assert out == [0, 1, 2] and not any(w.startswith("W-drop-vs-escalate") for w in ws)


def test_escalate_payload_of_exits_consumes_them():
    def body(rt):
        exits = jv.cut(jv.judge([jv.state(on=jv.lit("m"))], T()))
        return jv.escalate(exits)
    out, ws, _ = run(body, client=jv.FakeClient(unsure_rule))
    assert len(out.exits) == 1 and out.exits[0].consumed


def test_escalate_exits_rejects_non_exit():
    def body(rt):
        return jv.escalate([1], exits=[jv.lit("x")])
    with pytest.raises(jv.JvTypeError):
        run(body)


def test_handle_then_drop_counts_as_drop():
    def body(rt):
        e = jv.cut(jv.judge(jv.state(on=jv.lit("m")), T())[0])
        jv.handle(e, then=jv.drop)
        return jv.escalate(["m"])
    _, ws, _ = run(body, client=jv.FakeClient(unsure_rule))
    assert any(w.startswith("W-drop-vs-escalate") for w in ws)


# ---------------------------------------------------------------- 5. 通配 case _ 不消费 Unsure：静态警告
def test_wildcard_case_static_warning():
    @jv.program(budget=jv.Budget(calls=5))
    def p(邮件):
        正常 = jv.test("正常吗", calib=jv.calib("t.k"))
        去哪 = jv.select("去哪", calib=jv.calib("s.k"))
        rs = jv.judge([jv.state(on=m, over=[jv.lit("A"), jv.lit("B")]) for m in 邮件], 正常, 去哪)
        安, 夹 = jv.cut([r[0] for r in rs]), jv.cut([r[1] for r in rs])
        交人 = []
        for m, a, f in zip(邮件, 安, 夹):
            match (a, f):
                case (jv.Act(), jv.Pick(k)):
                    pass
                case _:
                    交人.append(m)
        jv.consume(安, unsure=jv.drop)
        return jv.escalate(交人)
    rep = p.check()
    assert not rep.errors
    assert any(w.startswith("W-wildcard-unsure") for w in rep.warnings)


def test_match_with_unsure_case_no_wildcard_warning():
    @jv.program(budget=jv.Budget(calls=5))
    def p(m):
        e = jv.cut(jv.judge(jv.state(on=m), jv.test("正常吗", calib=jv.calib("t.k")))[0])
        match e:
            case jv.Act():
                return 1
            case jv.Unsure(c):
                return jv.handle(c, keep=0)
            case _:
                return 0
    rep = p.check()
    assert not any(w.startswith("W-wildcard-unsure") for w in rep.warnings)


# ---------------------------------------------------------------- 6. 守卫：否定题的 Ignore 不放行；inherit 无 reason 合法
def test_guard_rejects_ignore_from_negated_question():
    分流 = jv.register_action("file_email_g6", fn=lambda m: {"filed": m.content}, taint_out="inherit",
                             reversible=False, reason="")          # inherit 不需要 reason
    assert 分流.registered and 分流.reason == ""
    def body(rt):
        e = jv.cut(jv.judge(jv.state(on=jv.lit("m")), jv.test("可疑吗", calib=jv.calib("t.k")))[0])
        assert isinstance(e, jv.Ignore)
        return jv.do(分流, jv.lit("m"), iter_seq=0, guard=e)
    with pytest.raises(jv.JvError) as ei:
        run(body, client=jv.FakeClient(lambda t, qid, q: {"noul": 0.1} if q["type"] == "noul" else None))
    assert "J-08" in str(ei.value)


# ---------------------------------------------------------------- 7. G6 读者的三条仍跑通
def test_fresh6_still_runs():
    import subprocess
    r = subprocess.run([sys.executable, "-m", "foundation.jv.examples.fresh6"],
                       cwd=os.path.join(os.path.dirname(__file__), "..", ".."), capture_output=True, text=True, timeout=120)
    assert r.returncode == 0, r.stderr[-800:]
    assert '"程序": "地址标准化"' in r.stdout


# ---------------------------------------------------------------- 8. J-09 evidence 含 on 槽（Codex towow 测试踩到：Mat 禁 bool 导致 cut 崩）
def test_evidence_on_slot_does_not_crash_and_yields_act():
    q = jv.test("符合目标吗", calib=jv.calib("t.k"), evidence=("on", "ctx"))
    def body(rt):
        return jv.cut(jv.judge(jv.state(on=jv.lit("输出"), ctx=[jv.lit("目标")]), q)[0])
    e, _, _ = run(body, client=jv.FakeClient(lambda t, qid, qq: {"noul": 0.9} if qq["type"] == "noul" else None))
    assert isinstance(e, jv.Act)


def test_evidence_missing_ctx_is_insufficient():
    q = jv.test("符合目标吗", calib=jv.calib("t.k"), evidence=("on", "ctx"))
    def body(rt):
        e = jv.cut(jv.judge(jv.state(on=jv.lit("输出")), q)[0])
        jv.handle(e, keep=None)
        return e
    e, _, _ = run(body, client=jv.FakeClient(lambda t, qid, qq: {"noul": 0.9} if qq["type"] == "noul" else None))
    assert isinstance(e, jv.Unsure) and e.cause == "insufficient" and e.detail["missing"] == "ctx"


# ---------------------------------------------------------------- 3b. 单候选平凡出口绑档案字段 select_sums_to_one（H2）
def test_select_single_candidate_not_trivial_when_profile_says_sums_not_one():
    base = jv.Runtime(client=jv.FakeClient()).profile
    with jv.Runtime(client=jv.FakeClient(), profile={**base, "select_sums_to_one": False}) as rt:
        rt.calib.put("s.k", hi=0.6, lo=0.3, n=100, status="上岗")

        @jv.program(budget=jv.Budget(calls=20), check_static=False)
        def p():
            q = jv.select("哪个？", calib=jv.calib("s.k"))
            e = jv.cut(jv.judge(jv.state(on=jv.mat("x"), over=[jv.mat("a")]), q))
            jv.consume([e], unsure=jv.drop)
            return rt.stats["calls"], rt.stats.get("trivial", 0)
        calls, trivial = p()
    assert trivial == 0 and calls >= 1
