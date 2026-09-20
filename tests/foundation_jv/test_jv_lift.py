"""ir-impl-6：推测提升与循环向量化（spec.py，pass `speculate` / `vectorize`），J-05「被返回类型消费」。全部 FakeClient，$0。

目的（P5、P3）：同一份材料上的题一次问完、互不依赖的判断同层并发，由机制成立，不靠写法纪律。
"""

from __future__ import annotations

import os
import sys
import warnings

import pytest

sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__)))))

import foundation.jv as jv  # noqa: E402


def rt_with(client=None, passes=None, **kw):
    rt = jv.Runtime(client=client or jv.FakeClient(), passes=passes, **kw)
    for k in ("t.k", "s.k", "m.k"):
        rt.calib.put(k, hi=0.6, lo=0.3, n=100, status="上岗")
    return rt


def run(fn, client=None, passes=None, budget=None):
    with rt_with(client=client, passes=passes) as rt:
        @jv.program(budget=budget or jv.Budget(calls=50), check_static=False)
        def p():
            return fn(rt)
        with warnings.catch_warnings(record=True) as w:
            warnings.simplefilter("always")
            out = p()
        return out, [str(x.message) for x in w], rt


def layers(rt):
    return len(rt.stats["layers"])


T = lambda: jv.test("提到截止日期吗", calib=jv.calib("t.k"))            # noqa: E731
S = lambda: jv.select("哪个", calib=jv.calib("s.k"))                     # noqa: E731


def band_rule(text, qid, q):
    """全部落线附近：test → Unsure(band)。"""
    if q["type"] == "noul":
        return {"type": "noul", "noul": 0.5}
    return None


def mk_acts(m):
    return ["a-截止日期", "b-其他"]


def mk_acts_seen(m, seen):
    return [x for x in ["a-截止日期", "b-其他"] if x not in seen.content]


# ---------------------------------------------------------------- 一、推测提升（直线段内，中间有 transform 与 match）
def choice_rule(text, qid, q):
    """choice 一律选 c0（避开 FakeClient 默认规则在两个置换下的并列）。"""
    if q["type"] == "choice":
        opts = list(q["criteria"])
        return {"type": "choice", "choice": "c0", "probabilities": {o: (0.9 if o == "c0" else 0.1 / max(1, len(opts) - 1)) for o in opts}}
    return None


def rt_c(passes=None, **kw):
    return rt_with(client=jv.FakeClient(rule=choice_rule), passes=passes, **kw)


@jv.program(budget=jv.Budget(calls=50), check_static=False)
def two_sites(观):
    到达, 下一步 = T(), S()
    match jv.cut(jv.judge(jv.state(on=观), 到达)[0]):
        case jv.Act(): pass
        case jv.Ignore(): pass
        case jv.Unsure(c): jv.handle(c, keep=None)
    acts = jv.transform(mk_acts, 观)
    e = jv.cut(jv.judge(jv.state(on=观, over=acts), 下一步))
    jv.consume([e], unsure=jv.drop)
    return e


def _run_prog(prog, *args, passes=None, budget=None, **kw):
    with rt_c(passes=passes, **kw) as rt:
        if budget is not None:
            prog = jv.program(budget=budget, check_static=False)(prog.__jv_fn__)
        with warnings.catch_warnings(record=True) as w:
            warnings.simplefilter("always")
            out = prog(*args)
        return out, [str(x.message) for x in w], rt


def test_lift_second_site_joins_first_layer():
    e, w, rt = _run_prog(two_sites, jv.lit("桌面 截止日期"))
    assert layers(rt) == 1 and rt.client.calls == 2               # 两个状态同层并发；不是同一次调用（状态不同）
    assert rt.stats["spec"]["lift"] == 1 and rt.stats["spec"]["hits"] == 1
    assert any(x.startswith("W-lift") for x in w)
    assert isinstance(e, jv.Pick)


def test_switch_speculate_off_restores_two_layers():
    _, w, rt = _run_prog(two_sites, jv.lit("桌面 截止日期"), passes={"speculate": False, "vectorize": False})
    assert layers(rt) == 2 and rt.client.calls == 2
    assert rt.stats["spec"]["lift"] == 0 and not any(x.startswith("W-lift") for x in w)


def test_lift_does_not_change_ledger_replay(tmp_path):
    with rt_c(root=str(tmp_path)) as rt:
        two_sites(jv.lit("桌面 截止日期"))
        assert rt.stats["calls"] == 2
    with rt_c(root=str(tmp_path)) as rt:
        two_sites(jv.lit("桌面 截止日期"))
        assert rt.stats["calls"] == 0 and rt.stats["ledger_hits"] >= 2 and layers(rt) <= 1


# ---------------------------------------------------------------- 二、不提升：中间语句改写了后一站点要读的名字 / 依赖 do
def test_no_lift_when_name_mutated_between_sites():
    @jv.program(budget=jv.Budget(calls=50), check_static=False)
    def p(观):
        到达, 下一步 = T(), S()
        seen = jv.lit([])
        match jv.cut(jv.judge(jv.state(on=观), 到达)[0]):
            case jv.Act(): seen = jv.lit(["a-截止日期"])
            case jv.Unsure(c): jv.handle(c, keep=None)
        acts = jv.transform(mk_acts_seen, 观, seen)
        e = jv.cut(jv.judge(jv.state(on=观, over=acts), 下一步))
        jv.consume([e], unsure=jv.drop)
    _, _, rt = _run_prog(p, jv.lit("桌面 截止日期"))
    assert layers(rt) == 2 and rt.stats["spec"]["lift"] == 0


def test_no_lift_across_do_dependency():
    """后一站点的状态依赖中间的 do：期物未解析 → 不推测（do 永远不推测）。"""
    act = jv.register_action("lift_probe", fn=lambda m: {"x": m.content}, taint_out="inherit", reason="")

    @jv.program(budget=jv.Budget(calls=50), check_static=False)
    def p(观):
        match jv.cut(jv.judge(jv.state(on=观), T())[0]):
            case jv.Unsure(c): jv.handle(c, keep=None)
            case _: pass
        观2 = jv.do(act, 观, iter_seq=0)
        e = jv.cut(jv.judge(jv.state(on=观2), T())[0])
        jv.consume([e], unsure=jv.drop)
    _, _, rt = _run_prog(p, jv.lit("桌面 截止日期"))
    assert layers(rt) == 2 and rt.stats["spec"]["lift"] == 0 and rt.stats["do"] == 1


# ---------------------------------------------------------------- 三、推错：前一站点 return，后一站点没到 → 记 unused 并告警
def test_speculated_but_unused_is_counted_and_warned():
    @jv.program(budget=jv.Budget(calls=50), check_static=False)
    def p(观):
        到达, 下一步 = T(), S()
        match jv.cut(jv.judge(jv.state(on=观), 到达)[0]):
            case jv.Act(): return "done"
            case jv.Unsure(c): jv.handle(c, keep=None)
        acts = jv.transform(mk_acts, 观)
        e = jv.cut(jv.judge(jv.state(on=观, over=acts), 下一步))
        jv.consume([e], unsure=jv.drop)
    out, w, rt = _run_prog(p, jv.lit("桌面 截止日期"))
    assert out == "done"
    assert rt.stats["spec"]["lift"] == 1 and rt.stats["spec"]["hits"] == 0 and rt.stats["spec"]["unused"] == 1
    assert any(x.startswith("W-spec-unused") for x in w)


# ---------------------------------------------------------------- 四、循环向量化
def _gen_counter():
    calls = {"n": 0}

    def g(prompt, ctx, n, retry_seq):
        calls["n"] += 1
        return [f"v{i} {ctx[0].content}" for i in range(n)]
    return g, calls


@jv.program(budget=jv.Budget(calls=50), check_static=False)
def loop_gen(fs):
    q = S()
    out = {}
    for f in fs:
        vers = jv.gen("写一句", ctx=[f], n=2, retry_seq=0)
        match jv.cut(jv.judge(jv.state(on=f, over=vers), q)):
            case jv.Pick(k): out[f.content] = k
            case jv.Unsure(c): jv.handle(c, keep=None)
    return out


def test_vectorize_loop_one_layer_and_gen_not_repeated():
    g, calls = _gen_counter()
    fs = [jv.lit(f"函数 {i}") for i in range(4)]
    out, _, rt = _run_prog(loop_gen, fs, generator=g)
    assert len(out) == 4 and layers(rt) == 1
    assert rt.stats["spec"]["vectorize"] == 3 and rt.stats["spec"]["hits"] == 3
    assert calls["n"] == 4                                        # 沙盒里的 gen 记账，真站点命中账本：生成器只跑 4 次


def test_switch_vectorize_off_one_layer_per_iteration():
    g, calls = _gen_counter()
    fs = [jv.lit(f"函数 {i}") for i in range(4)]
    out, _, rt = _run_prog(loop_gen, fs, generator=g, passes={"vectorize": False})
    assert len(out) == 4 and layers(rt) == 4 and rt.stats["spec"]["vectorize"] == 0 and calls["n"] == 4


def test_vectorize_respects_guard_continue():
    """守卫 `if …: continue` 在沙盒里求值：被跳过的轮次不推测。"""
    @jv.program(budget=jv.Budget(calls=50), check_static=False)
    def p(fs):
        q = S()
        out = []
        for f in fs:
            if "跳过" in f.content: continue
            vers = jv.transform(mk_acts, f)
            match jv.cut(jv.judge(jv.state(on=f, over=vers), q)):
                case jv.Pick(k): out.append(k)
                case jv.Unsure(c): jv.handle(c, keep=None)
        return out
    out, _, rt = _run_prog(p, [jv.lit("函数 0"), jv.lit("函数 跳过"), jv.lit("函数 2")])
    assert len(out) == 2 and layers(rt) == 1 and rt.stats["spec"]["vectorize"] == 1


def test_no_vectorize_loop_carried_dependency():
    """体内改写了前置段读的名字（acc）→ 不向量化，每轮一层；静态原因记进 stats。"""
    @jv.program(budget=jv.Budget(calls=50), check_static=False)
    def p(fs):
        q = T()
        acc = jv.lit("起点")
        for f in fs:
            match jv.cut(jv.judge(jv.state(on=f, ctx=[acc]), q)[0]):
                case jv.Act(): acc = f
                case jv.Ignore(): pass
                case jv.Unsure(c): jv.handle(c, keep=None)
    _, _, rt = _run_prog(p, [jv.lit("函数 0"), jv.lit("函数 1"), jv.lit("函数 2")])
    assert layers(rt) == 3 and rt.stats["spec"]["vectorize"] == 0
    assert any(s.startswith("vectorize-skip:loop-carried") for s in rt.stats["spec"]["skipped"])


def test_no_vectorize_when_body_has_do():
    act = jv.register_action("vec_probe", fn=lambda m: {"x": m.content}, taint_out="inherit", reason="")

    @jv.program(budget=jv.Budget(calls=50), check_static=False)
    def p(fs):
        q = T()
        for i, f in enumerate(fs):
            e = jv.cut(jv.judge(jv.state(on=f), q)[0])
            jv.consume([e], unsure=jv.drop)
            jv.do(act, f, iter_seq=i).content
    _, _, rt = _run_prog(p, [jv.lit("函数 0"), jv.lit("函数 1")])
    assert layers(rt) == 2 and rt.stats["spec"]["vectorize"] == 0
    assert any(s.startswith("vectorize-skip:") and "do" in s for s in rt.stats["spec"]["skipped"])


# ---------------------------------------------------------------- 四b、零成本零副作用：含 gen 的站点只在无条件可达时推测
def test_gen_site_in_branch_is_not_speculated():
    """分支体里的站点状态含 jv.gen：能不能走到还不知道，gen 花钱 → 不推测；生成器只在分支真走到时跑。"""
    g, calls = _gen_counter()

    @jv.program(budget=jv.Budget(calls=50), check_static=False)
    def p(观):
        到达, 下一步 = T(), S()
        match jv.cut(jv.judge(jv.state(on=观), 到达)[0]):
            case jv.Act():
                vers = jv.gen("写一句", ctx=[观], n=2, retry_seq=0)
                e = jv.cut(jv.judge(jv.state(on=观, over=vers), 下一步))
                jv.consume([e], unsure=jv.drop)
                return "act"
            case jv.Ignore(): return "ignore"
            case jv.Unsure(c): jv.handle(c, keep=None)
    out, _, rt = _run_prog(p, jv.lit("桌面 无关"), generator=g)          # Ignore 分支：gen 根本不该跑
    assert out == "ignore" and calls["n"] == 0 and rt.stats["spec"]["lift"] == 0
    assert "gen-in-branch" in rt.stats["spec"]["skipped"]
    out, _, rt = _run_prog(p, jv.lit("桌面 截止日期"), generator=g)      # Act 分支：真站点自己发，多一层
    assert out == "act" and calls["n"] == 1 and rt.stats["spec"]["lift"] == 0 and layers(rt) == 2


def test_gen_site_in_straight_line_is_speculated_and_generator_runs_once():
    """直线段内含 gen 的站点一定会执行 → 可以提前执行：同层，生成器只跑一遍（真站点命中效应账本）。"""
    g, calls = _gen_counter()

    @jv.program(budget=jv.Budget(calls=50), check_static=False)
    def p(观):
        到达, 下一步 = T(), S()
        match jv.cut(jv.judge(jv.state(on=观), 到达)[0]):
            case jv.Unsure(c): jv.handle(c, keep=None)
            case _: pass
        vers = jv.gen("写一句", ctx=[观], n=2, retry_seq=0)
        e = jv.cut(jv.judge(jv.state(on=观, over=vers), 下一步))
        jv.consume([e], unsure=jv.drop)
        return e
    out, _, rt = _run_prog(p, jv.lit("桌面 截止日期"), generator=g)
    assert layers(rt) == 1 and rt.stats["spec"]["lift"] == 1 and rt.stats["spec"]["hits"] == 1
    assert calls["n"] == 1 and "gen-in-branch" not in rt.stats["spec"]["skipped"]


# ---------------------------------------------------------------- 五、预算：层边界超预算先丢推测，真站点照发
def test_budget_drops_speculation_before_real_sites():
    e, w, rt = _run_prog(two_sites, jv.lit("桌面 截止日期"), budget=jv.Budget(calls=1))
    assert rt.stats["spec"]["dropped"] == 1
    first = rt.stats["layers"][0]
    assert first["calls"] == 1 and not first.get("stopped")        # 第一层只发真站点
    assert layers(rt) == 2                                        # 后一站点自己一层（超预算 → Unsure(budget)，已 consume）


# ---------------------------------------------------------------- 六、六条示例在不改代码的情况下降到理论层数
def test_six_examples_reach_theoretical_layers():
    from foundation.jv.examples import six
    rows = {r["程序"]: r for r in six.run_all()}
    assert rows["取物"]["层数"] == 4 and rows["写docstring"]["层数"] == 2 and rows["写docstring"]["停层"] == 0
    rows_off = {r["程序"]: r for r in six.run_all(passes={"speculate": False, "vectorize": False})}
    assert rows_off["取物"]["层数"] == 7 and rows_off["写docstring"]["层数"] == 4


# ---------------------------------------------------------------- 七、J-05「被返回类型消费」：注解含 Unsure 才能原样返回
def _unsure_exit(rt):
    return jv.cut(jv.judge(jv.state(on=jv.lit("a")), T())[0])


def test_return_annotation_hands_unsure_to_caller_who_must_consume():
    with rt_with(client=jv.FakeClient(rule=band_rule)) as rt:
        @jv.program(budget=jv.Budget(calls=5), check_static=False)
        def inner() -> jv.Exit:
            return _unsure_exit(rt)

        @jv.program(budget=jv.Budget(calls=5), check_static=False)
        def outer_bad():
            e = inner()
            return e.cause                                        # 没处理它

        with pytest.raises(jv.JvError, match=r"J-05: 程序 outer_bad"):
            outer_bad()


def test_return_annotation_handed_unsure_consumed_by_caller_passes():
    with rt_with(client=jv.FakeClient(rule=band_rule)) as rt:
        @jv.program(budget=jv.Budget(calls=5), check_static=False)
        def inner() -> jv.Exit | jv.Unsure:
            return _unsure_exit(rt)

        @jv.program(budget=jv.Budget(calls=5), check_static=False)
        def outer():
            match inner():
                case jv.Unsure(c): return c
                case _: return "ok"
        assert outer() == "band"


def test_return_without_annotation_is_j05_with_fix():
    with rt_with(client=jv.FakeClient(rule=band_rule)) as rt:
        @jv.program(budget=jv.Budget(calls=5), check_static=False)
        def inner():
            return _unsure_exit(rt)

        @jv.program(budget=jv.Budget(calls=5), check_static=False)
        def outer():
            match inner():
                case jv.Unsure(c): return c
                case _: return "ok"
        with pytest.raises(jv.JvError, match=r"J-05: 程序 inner .*返回注解"):
            outer()


def test_outermost_annotated_may_return_unsure_and_ledger_records_it():
    with rt_with(client=jv.FakeClient(rule=band_rule)) as rt:
        @jv.program(budget=jv.Budget(calls=5), check_static=False)
        def p() -> list[jv.Exit]:
            return [_unsure_exit(rt)]
        out = p()
        assert isinstance(out[0], jv.Unsure) and out[0].detail.get("consumed_by") is None
        assert out[0].__dict__.get("consumed_by") == "return_type"
        assert rt.stats["returned_unsure"] == 1


def test_unsure_not_in_result_is_still_j05_even_if_annotated():
    with rt_with(client=jv.FakeClient(rule=band_rule)) as rt:
        @jv.program(budget=jv.Budget(calls=5), check_static=False)
        def p() -> jv.Exit:
            _unsure_exit(rt)
            return jv.Act()
        with pytest.raises(jv.JvError, match="J-05"):
            p()
