"""长处探针（examples/strength.py）：三条跑通 + 长处构件的性质 + J-16 反例。全部 FakeClient，$0。"""

from __future__ import annotations

import pytest

import foundation.jv as jv
from foundation.jv.examples import strength as S


def _rt():
    rt = jv.Runtime(client=jv.FakeClient(rule=S.fake_rule))
    return rt


# ---------------------------------------------------------------- 1. 分配复核
def test_allocate_picks_most_uncertain_and_beats_random():
    with _rt() as rt:
        S.calib_all(rt)
        r = S.run_allocate(rt)
        assert r["k"] == 4
        # 最不确定的四段正是四段 fuzzy（下标 2、4、6、8）
        assert sorted(r["按不确定性复核"]) == [2, 4, 6, 8]
        assert r["错误率·分配"] == 0.0
        assert r["错误率·分配"] < r["错误率·随机"] <= r["错误率·不复核"]
        assert r["上界"]["n"] == 12 and r["上界"]["union_bound"] == pytest.approx(12 * 0.2) and r["上界"]["n_unknown"] == 0
        assert rt.stats["calls"] == 12 and len(rt.stats["layers"]) == 1


def test_unsure_bound_unknown_rate_counts_as_one():
    with _rt() as rt:
        rt.calib.put("doc.含数字", hi=0.65, lo=0.35, n=100, status="上岗", set_id="c")      # 无 unsure_rate
        @jv.program()
        def f(xs):
            q = jv.test("含数字？", calib=jv.calib("doc.含数字"))
            rs = jv.judge([jv.state(on=x) for x in xs], q)
            b = jv.unsure_bound(rs)
            jv.consume(jv.cut(rs), unsure=jv.drop)
            return b
        b = f([jv.lit(t) for t, _, _ in S.段落[:3]])
        assert b["n_unknown"] == 3 and b["union_bound"] == 3.0


def test_allocate_rejects_non_readings():
    with _rt() as rt:
        S.calib_all(rt)
        @jv.program()
        def f(xs):
            q = jv.test("含数字？", calib=jv.calib("doc.含数字"))
            es = jv.cut(jv.judge([jv.state(on=x) for x in xs], q))
            jv.consume(es, unsure=jv.drop)
            return jv.allocate(es, 1)                       # 出口不是读数
        with pytest.raises(jv.JvTypeError):
            f([jv.lit(S.段落[0][0])])


# ---------------------------------------------------------------- 2. 代价比线
def test_cost_line_moves_with_cost_matrix():
    with _rt() as rt:
        S.calib_all(rt)
        r = S.run_costline(rt)
        assert r["线·漏退更贵"] < 0.5 < r["线·误退更贵"]              # 线随代价移动
        assert r["出口不同的条数"] >= 3
        assert r["Act 数"][0] > r["Act 数"][1]                          # 漏退更贵 → 退得更多
        assert rt.stats["calls"] == 12 and len(rt.stats["layers"]) == 1  # 两套出口不多花一次调用


def test_cost_line_pure_function_properties():
    s = S._labeled_set(120)
    a = jv.cost_line(s, 1, 10); b = jv.cost_line(s, 10, 1); c = jv.cost_line(s, 1, 1)
    assert a["line"] < c["line"] < b["line"]
    assert a["n"] == 120
    with pytest.raises(ValueError):
        jv.cost_line([], 1, 1)


def test_cost_without_samples_warns_and_keeps_record_line():
    with _rt() as rt:
        rt.calib.put("ticket.退款", hi=0.65, lo=0.35, n=120, status="上岗", set_id="conf-B")    # 无 samples
        @jv.program()
        def f(xs):
            q = jv.test("退款？", calib=jv.calib("ticket.退款"))
            es = jv.cut(jv.judge([jv.state(on=x) for x in xs], q), cost=(1, 10))
            jv.consume(es, unsure=jv.drop)
            return es
        es = f([jv.lit(S.工单[0][0])])
        assert isinstance(es[0], jv.Act) and "cost_line" not in es[0].detail
        assert any("W-cost-unfit" in w for w in rt.stats["warnings"])


def test_cost_line_requires_disjoint_label_and_conformal_sets():
    with _rt() as rt:
        rt.calib.put("ticket.退款", hi=0.65, lo=0.35, n=120, status="上岗", set_id="same",
                     samples=S._labeled_set(120), label_set_id="same")
        @jv.program()
        def f(xs):
            q = jv.test("退款？", calib=jv.calib("ticket.退款"))
            return jv.cut(jv.judge([jv.state(on=x) for x in xs], q), cost=(1, 10))
        with pytest.raises(jv.JvError) as ei:
            f([jv.lit(S.工单[0][0])])
        assert "J-16" in str(ei.value)


def test_cost_only_defined_for_test_questions():
    with _rt() as rt:
        rt.calib.put("k.sel", hi=0.6, lo=0.3, n=120, status="上岗", set_id="a",
                     samples=S._labeled_set(120), label_set_id="b")
        @jv.program()
        def f(x):
            q = jv.select("哪个？", calib=jv.calib("k.sel"))
            return jv.cut(jv.judge(jv.state(on=x, over=[jv.lit("a"), jv.lit("b")]), q), cost=(1, 2))
        with pytest.raises(jv.JvError):
            f(jv.lit("x"))


# ---------------------------------------------------------------- 3. fit 桥
def test_fit_program_runs_and_merges_only_green_and_local():
    with _rt() as rt:
        S.calib_all(rt)
        r = S.run_fit(rt)
        assert r["合入"] == ["diff-1", "diff-5"]
        assert rt.stats["calls"] == 5 and len(rt.stats["layers"]) == 1      # 每状态两题一次调用


def test_fit_register_rejects_small_n():
    reg = jv.FitRegistry()
    with pytest.raises(ValueError) as ei:
        reg.register("x", trained_from="t", n=40, error_rate=0.1, fn=lambda a, b: a * b,
                     features=[("test.全过", "test"), ("diff.局部", "test")])
    assert "J-16" in str(ei.value)


def test_fit_rejects_fingerprint_mismatch():
    with _rt() as rt:
        S.calib_all(rt)
        @jv.program()
        def f(c):
            全过 = jv.test("测试报告显示全部通过吗？", calib=jv.calib("test.全过"))
            别的 = jv.test("含数字？", calib=jv.calib("doc.含数字"))          # 指纹与注册特征不同
            r = jv.judge(jv.state(on=c), 全过, 别的)
            return jv.cut(jv.fit(jv.fitref("绿且局部"), r[0], r[1]), calib=jv.calib("fit.绿且局部"))
        with pytest.raises(jv.JvError) as ei:
            f(jv.lit(S.变更[0][0]))
        assert "J-04" in str(ei.value)


def test_fit_rejects_same_train_and_conformal_set():
    with _rt() as rt:
        S.calib_all(rt)
        rt.calib.put("fit.绿且局部", hi=0.6, lo=0.3, n=80, status="上岗", set_id="train-A")   # 与 trained_from 相同
        with pytest.raises(jv.JvError) as ei:
            S.run_fit(rt)
        assert "J-16" in str(ei.value)


def test_cross_question_arithmetic_outside_fit_is_j01():
    with _rt() as rt:
        S.calib_all(rt)
        @jv.program()
        def f(c):
            全过 = jv.test("测试报告显示全部通过吗？", calib=jv.calib("test.全过"))
            局部 = jv.test("diff 只改了被测函数吗？", calib=jv.calib("diff.局部"))
            r = jv.judge(jv.state(on=c), 全过, 局部)
            return r[0] * r[1]
        with pytest.raises((jv.JvTypeError, jv.JvError)):
            f(jv.lit(S.变更[0][0]))


def test_run_all_three_rows():
    rows = S.run_all()
    assert [r["程序"] for r in rows] == ["分配复核", "代价比线", "自动合入"]
    assert all(r["层数"] == 1 for r in rows)
