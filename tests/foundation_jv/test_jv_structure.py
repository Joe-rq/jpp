"""`__jv_structure__` 识别端（Claude的评审回复.md §「三处要改」第 3 条）：jv.plan 读组合库挂的结构树，
按 then=求和、branch=predicate+两臂保守上界、opaque=未知符号+告警 合成 PlanReport；不触发任何执行，
不改变既有 @jv.program 的原生 AST 路线。挂载端与结构执行器由 Codex 负责，这里只测识别端。全部 $0。"""

from __future__ import annotations

import os
import sys

import pytest

sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__)))))

import foundation.jv as jv  # noqa: E402


# ---------------------------------------------------------------- 节点构造
def leaf_node(name, fn, effects=(), contract="declared"):
    return {"operation": "leaf", "name": name, "input": "Any", "output": "Any",
            "effects": tuple(effects), "effects_contract": contract,
            "children": (), "function": fn}


def identity_node(name="id"):
    return {"operation": "identity", "name": name, "input": "Any", "output": "Any",
            "effects": (), "effects_contract": "structural", "children": ()}


def then_node(name, left, right):
    return {"operation": "then", "name": name, "input": "Any", "output": "Any",
            "effects": (), "effects_contract": "structural", "children": (left, right)}


def branch_node(name, pred, yes, no):
    return {"operation": "branch", "name": name, "input": "Any", "output": "Any",
            "effects": (), "effects_contract": "structural", "children": (pred, yes, no)}


def opaque_node(name, source_operation, effects=("*",), parameters=None, fn=None):
    return {"operation": "opaque", "name": name, "input": "Any", "output": "Any",
            "effects": tuple(effects), "effects_contract": "unknown", "children": (),
            "source_operation": source_operation, "parameters": parameters or {}, "function": fn}


def product_node(name, *children):
    return {"operation": "product", "name": name, "input": "Any", "output": "Any",
            "effects": (), "effects_contract": "structural", "children": tuple(children)}


def iterate_node(name, step, done, limit):
    return {"operation": "iterate", "name": name, "input": "Any", "output": "Any",
            "effects": (), "effects_contract": "structural", "children": (step, done),
            "parameters": {"limit": limit}}


def bind_node(name, prefix, factory, continuation_effects=()):
    return {"operation": "bind", "name": name, "input": "Any", "output": "Any",
            "effects": (), "effects_contract": "structural", "children": (prefix, factory),
            "parameters": {"continuation_effects": tuple(continuation_effects)}}


def entry_with(root, name="entry"):
    def wrapper():
        raise AssertionError("__jv_structure__ 路径不得触发执行")
    wrapper.__name__ = name
    wrapper.__jv_structure__ = {"version": 1, "root": root}
    return wrapper


# ---------------------------------------------------------------- then：成本相加
def test_structure_then_sums_leaf_costs():
    def leaf_a():
        jv.gen(n=2)
        jv.gen(n=3)

    def leaf_b():
        jv.gen(n=1)

    root = then_node("root", leaf_node("a", leaf_a, ("gen",)), leaf_node("b", leaf_b, ("gen",)))
    rep = jv.plan(entry_with(root))
    assert repr(rep.gen_calls) == "3"
    assert rep.structure is root
    assert not any(w.startswith("W-nosource") for w in rep.warnings)


# ---------------------------------------------------------------- branch：predicate + 两臂上界
def test_structure_branch_predicate_plus_max():
    def pred():
        jv.gen(n=1)

    def yes_fn():
        jv.gen(n=1)
        jv.gen(n=1)
        jv.gen(n=1)

    def no_fn():
        jv.gen(n=1)

    root = branch_node("root", leaf_node("p", pred, ("gen",)),
                        leaf_node("yes", yes_fn, ("gen",)), leaf_node("no", no_fn, ("gen",)))
    rep = jv.plan(entry_with(root))
    assert repr(rep.gen_calls) == "4"          # 1（pred） + max(3, 1)


def test_structure_branch_incomparable_symbols_sum_with_warning():
    def pred_do():
        pass

    def yes_fn():
        for _ in range(1):
            jv.gen(n=1)

    def no_fn():
        for _ in range(1):
            jv.gen(n=1)
            jv.gen(n=1)

    # 用不同符号名让两臂不可直接比较大小（各自受未知循环界 bound 影响）：这里改用两个真正
    # 不可数值比较、且表达式不同的符号，逼 _sym_upper_bound 走「和值取上界」分支。
    def yes_sym():
        while True:
            jv.gen(n=1)
            break

    def no_sym():
        i = 0
        while i < 1:
            jv.gen(n=1)
            jv.gen(n=1)
            i += 1

    root = branch_node("root", leaf_node("p", pred_do, ()),
                        leaf_node("yes", yes_sym, ("gen",)), leaf_node("no", no_sym, ("gen",)))
    rep = jv.plan(entry_with(root))
    assert not rep.gen_calls.is_numeric
    assert any(w.startswith("W-branch-bound") for w in rep.warnings)


# ---------------------------------------------------------------- opaque：未知符号 + 告警，绝不为 0
def test_structure_opaque_marks_unknown_and_warns():
    root = opaque_node("bind1", "bind", effects=("*",), parameters={"factory_effects": ("gen",)})
    rep = jv.plan(entry_with(root))
    assert not rep.calls.is_numeric
    assert not rep.do_calls.is_numeric
    assert any(w.startswith("W-opaque") for w in rep.warnings)
    assert any(w.startswith("W-dynamic") for w in rep.warnings)


def test_structure_leaf_declared_effect_not_seen_by_ast_is_unknown_not_zero():
    def quiet_leaf():
        return 1                                # 声明 do，但 AST 里根本没有 jv.do 调用

    root = leaf_node("quiet", quiet_leaf, ("do",))
    rep = jv.plan(entry_with(root))
    assert not rep.do_calls.is_numeric           # 未知符号，不是静默的 0
    assert any(w.startswith("W-opaque") for w in rep.warnings)


def test_structure_leaf_no_source_is_unknown_not_zero():
    root = leaf_node("nosource", eval("lambda: jv.gen(n=1)"), ("gen",))   # exec 构造，取不到源码
    rep = jv.plan(entry_with(root))
    assert not rep.gen_calls.is_numeric
    assert any(w.startswith("W-opaque") for w in rep.warnings)


# ---------------------------------------------------------------- 修正：_cost_of_leaf 两处协议问题
def test_structure_leaf_preserves_nonzero_symbolic_estimate():
    """回归：循环 n 次 judge 的非零符号估计不该被「未见到」误判覆盖（is_numeric=False 不等于未见到）。"""
    def leaf_loop_judge(xs):
        for x in xs:
            r = jv.judge(jv.state(on=x), jv.test("q", calib=jv.calib("k.k")))
            jv.consume([jv.cut(r[0])])

    root = leaf_node("loop", leaf_loop_judge, ("judge",))
    rep = jv.plan(entry_with(root))
    assert repr(rep.calls) == "|xs|"
    assert repr(rep.questions) == "|xs|"
    assert not any(w.startswith("W-opaque") for w in rep.warnings)


def test_structure_leaf_no_source_and_empty_effects_marks_full_unknown():
    """无源码时不该只补『声明过的』effect；effects=() 也不能让 9 项资源仍显示为 0。"""
    root = leaf_node("blackbox", eval("lambda: None"), ())                # 无源码，且未声明任何 effect
    rep = jv.plan(entry_with(root))
    for f in ("calls", "questions", "layers", "gen_calls", "gen_latency",
              "do_calls", "do_cost", "asks", "unsure_bound"):
        assert not getattr(rep, f).is_numeric, f
    assert any(w.startswith("W-opaque") for w in rep.warnings)


def test_structure_leaf_noncallable_function_raises_clear_error():
    root = leaf_node("bad", 123, ("do",))                                  # function 不是 callable：坏结构
    with pytest.raises(jv.JvError, match="不可调用"):
        jv.plan(entry_with(root))


# ---------------------------------------------------------------- identity：零成本
def test_structure_identity_is_zero_cost():
    def leaf_fn():
        jv.gen(n=2)

    root = then_node("root", identity_node(), leaf_node("a", leaf_fn, ("gen",)))
    rep = jv.plan(entry_with(root))
    assert repr(rep.gen_calls) == "1"


# ---------------------------------------------------------------- 既有 @jv.program 原生 AST 路线不受影响
def test_normal_program_path_unaffected_by_structure_check():
    @jv.program(budget=jv.Budget())
    def p():
        jv.gen(n=2)

    rep = jv.plan(p)
    assert rep.structure is None
    assert repr(rep.gen_calls) == "1"


# ---------------------------------------------------------------- 未知版本 / 坏结构：清楚报错
def test_structure_unknown_version_raises_clear_error():
    def wrapper():
        pass
    wrapper.__jv_structure__ = {"version": 2, "root": identity_node()}
    with pytest.raises(jv.JvError, match="未知 __jv_structure__ 版本"):
        jv.plan(wrapper)


def test_structure_bad_root_raises_clear_error():
    def wrapper():
        pass
    wrapper.__jv_structure__ = {"version": 1, "root": "not-a-mapping"}
    with pytest.raises(jv.JvError, match="root"):
        jv.plan(wrapper)


def test_structure_unknown_operation_raises_clear_error():
    def wrapper():
        pass
    wrapper.__jv_structure__ = {"version": 1, "root": {"operation": "mystery", "name": "x", "children": ()}}
    with pytest.raises(jv.JvError, match="operation"):
        jv.plan(wrapper)


def test_structure_then_wrong_children_count_raises_clear_error():
    def wrapper():
        pass
    bad = dict(then_node("t", identity_node(), identity_node()))
    bad["children"] = (identity_node(),)
    wrapper.__jv_structure__ = {"version": 1, "root": bad}
    with pytest.raises(jv.JvError, match="then 节点"):
        jv.plan(wrapper)


def test_structure_branch_wrong_children_count_raises_clear_error():
    def wrapper():
        pass
    bad = dict(branch_node("b", identity_node(), identity_node(), identity_node()))
    bad["children"] = (identity_node(), identity_node())
    wrapper.__jv_structure__ = {"version": 1, "root": bad}
    with pytest.raises(jv.JvError, match="branch 节点"):
        jv.plan(wrapper)


# ---------------------------------------------------------------- product：非空 children，资源相加
def test_structure_product_sums_all_children():
    def a():
        jv.gen(n=1)
        jv.gen(n=1)

    def b():
        jv.gen(n=1)

    def c():
        jv.gen(n=1)

    root = product_node("root", leaf_node("a", a, ("gen",)), leaf_node("b", b, ("gen",)), leaf_node("c", c, ("gen",)))
    rep = jv.plan(entry_with(root))
    assert repr(rep.gen_calls) == "4"


def test_structure_product_empty_children_raises_clear_error():
    root = product_node("root")
    with pytest.raises(jv.JvError, match="product 节点"):
        jv.plan(entry_with(root))


# ---------------------------------------------------------------- iterate：先查 done，N*step + (N+1)*done
def test_structure_iterate_cost_is_n_step_plus_n_plus_1_done():
    def step_fn():
        jv.gen(n=1)

    def done_fn():
        jv.gen(n=1)

    root = iterate_node("loop", leaf_node("step", step_fn, ("gen",)), leaf_node("done", done_fn, ("gen",)), limit=3)
    rep = jv.plan(entry_with(root))
    assert repr(rep.gen_calls) == "7"          # 3 步 × 1 + 4 次 done 检查 × 1


def test_structure_iterate_limit_zero_is_done_only():
    def step_fn():
        jv.gen(n=1)

    def done_fn():
        jv.gen(n=1)

    root = iterate_node("loop0", leaf_node("step", step_fn, ("gen",)), leaf_node("done", done_fn, ("gen",)), limit=0)
    rep = jv.plan(entry_with(root))
    assert repr(rep.gen_calls) == "1"          # N=0：只查一次 done，不进 step


def test_structure_iterate_negative_limit_raises_clear_error():
    root = iterate_node("bad", identity_node(), identity_node(), limit=-1)
    with pytest.raises(jv.JvError, match="非负整数"):
        jv.plan(entry_with(root))


def test_structure_iterate_non_int_limit_raises_clear_error():
    root = iterate_node("bad", identity_node(), identity_node(), limit="3")
    with pytest.raises(jv.JvError, match="非负整数"):
        jv.plan(entry_with(root))


def test_structure_iterate_wrong_children_count_raises_clear_error():
    bad = dict(iterate_node("loop", identity_node(), identity_node(), limit=1))
    bad["children"] = (identity_node(),)
    with pytest.raises(jv.JvError, match="iterate 节点"):
        jv.plan(entry_with(bad))


# ---------------------------------------------------------------- bind：静态绝不调用 factory，continuation 未知
def test_structure_bind_never_calls_factory_and_continuation_is_unknown():
    def prefix_fn():
        jv.gen(n=1)

    def factory_fn():
        raise AssertionError("bind 的 factory 绝不能在计划期被调用")

    root = bind_node("root", leaf_node("prefix", prefix_fn, ("gen",)), leaf_node("factory", factory_fn, ()))
    rep = jv.plan(entry_with(root))            # factory_fn 若被真的调用，会在这里直接抛出上面的 AssertionError
    assert not rep.gen_calls.is_numeric        # continuation 未知，污染了原本纯数值的 prefix 估计
    assert not rep.do_calls.is_numeric
    assert any(w.startswith("W-dynamic") for w in rep.warnings)


def test_structure_bind_wrong_children_count_raises_clear_error():
    bad = dict(bind_node("b", identity_node(), identity_node()))
    bad["children"] = (identity_node(),)
    with pytest.raises(jv.JvError, match="bind 节点"):
        jv.plan(entry_with(bad))
