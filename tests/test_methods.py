"""Behavioral checks for complete and dynamically constructed methods."""
import pytest
from foundation import jv
from jev_compose.core import Component, CompositionError, execute, identity, iterate, product
from jev_compose.fixtures import runtime
from jev_compose.examples import enumerate_candidates, make_synthesis
from jev_compose.ablations import inspect_without_semantics
from jev_compose.method_example import (FACTORY_INPUTS, construct_method, problem,
    program, report_solution)


def leaf(name, fn, input_type=int, output_type=int):
    return Component(name, input_type, output_type, fn)


def test_product_preserves_order_and_replacement_rebuilds_execution():
    seen = []
    a = leaf("a", lambda x: seen.append("a") or x + 1)
    b = leaf("b", lambda x: seen.append("b") or x * 2)
    original = product(a, b)
    changed = original.replace_at((1,), a)
    assert original(3) == (4, 6) and seen == ["a", "b"]
    seen.clear()
    assert changed(3) == (4, 4) and seen == ["a", "a"]
    assert original.structure()["root"]["children"][1]["name"] == "b"


@pytest.mark.parametrize("limit,start,stop,steps,reason,checks", [
    (0, 0, 2, 0, "limit", 1), (0, 2, 2, 0, "done", 1),
    (3, 0, 2, 2, "done", 3), (2, 0, 5, 2, "limit", 3)])
def test_iterate_checks_before_steps_and_includes_final_check(limit,start,stop,steps,reason,checks):
    seen = []
    step = leaf("advance", lambda x: x + 1)
    done = leaf("done", lambda x: seen.append(x) or x >= stop, output_type=bool)
    method = iterate(step, done, limit=limit)
    result = method(start)
    assert (result.steps, result.reason, len(seen)) == (steps, reason, checks)
    changed = method.replace_at((0,), leaf("jump", lambda x: x + 2))
    assert changed.children[0].name == "jump" and method.children[0].name == "advance"


def test_dynamic_structure_is_per_execution_and_nested_ranges_match():
    FACTORY_INPUTS.clear()
    calculation = program()
    before = calculation.describe()
    planned = jv.plan(calculation.program())
    assert not planned.calls.is_numeric and FACTORY_INPUTS == []
    runs = [execute(calculation, problem(domain), runtime(generator=enumerate_candidates))
            for domain in ([0], list(range(-4, 5)))]
    assert FACTORY_INPUTS == [1, 9] and calculation.describe() == before
    events = [[e for e in run.trace if e["operation"] == "bind"] for run in runs]
    assert len(events[0]) == 1 and len(events[1]) == 4
    assert events[0][0]["generated_description"] != events[1][0]["generated_description"]
    assert runs[0].value[1] == "x"
    assert runs[1].value[1] == "x if x >= 0 else -x"
    for run, dynamic in zip(runs, events):
        assert len({e["call_id"] for e in run.trace}) == len(run.trace)
        for event in dynamic:
            start, end = event["continuation_trace_start"], event["continuation_trace_end"]
            assert event["call_id"] < start < end <= len(run.trace)
            assert run.trace[start]["component"] == event["generated_structure"]["root"]["name"]
            assert "result" in event


def test_returning_method_as_data_does_not_execute_it():
    state = problem([0])
    run = execute(construct_method, state, runtime(generator=enumerate_candidates))
    assert isinstance(run.value, Component)
    assert run.stats["calls"] == 0 and run.stats["effect_requests"]["do"] == 0
    assert all(e["operation"] != "iterate" for e in run.trace)


def test_full_method_replacement_changes_calls_and_preserves_original():
    original = make_synthesis().then(report_solution)
    changed = original.replace_at((0, 0, 0, 1, 1), inspect_without_semantics)
    runs = [execute(method, problem(list(range(-4, 5))), runtime(generator=enumerate_candidates))
            for method in (original, changed, original)]
    assert runs[0].stats["calls"] > 0
    assert runs[1].stats["calls"] == 0
    assert runs[2].stats["calls"] == runs[0].stats["calls"]
    assert all(r.value["reason"] == "done" for r in runs)
    assert all(r.value["solution"] == runs[0].value["solution"] for r in runs)
    assert jv.plan(changed.program()).calls == 0


def test_dynamic_method_obeys_outer_budget():
    calculation = program()
    nested = calculation.program(name="nested_dynamic")
    @jv.program(budget=jv.Budget(calls=0))
    def outer(state):
        return nested(state)
    rt = runtime(generator=enumerate_candidates)
    with rt:
        result = outer(problem(list(range(-4, 5))))
    assert rt.client.calls == 0
    assert result[0]["reason"] == "done"  # exact checks still determine completion


def test_replacing_bind_factory_preserves_continuation_contract():
    method = identity(int).bind(lambda x: identity(int), int,
        effects=frozenset(), factory_effects=frozenset())
    incompatible = leaf("wrong", lambda x: identity(str), output_type=Component)
    changed = method.replace_at((1,), incompatible)
    with pytest.raises(CompositionError, match="result type"):
        changed(1)
    assert method(1) == 1


@pytest.mark.parametrize("composite", [False, True])
def test_component_factory_accepts_capability_superset_without_hiding_structure(composite):
    entered = []
    factory = leaf("factory", lambda x: entered.append(x) or identity(int), output_type=Component)
    if composite:
        factory = identity(int).then(factory)
    method = identity(int).bind(factory, int, effects=frozenset(),
                               factory_effects=frozenset({"judge"}))
    assert method.children[1] is factory
    node = method.structure()["root"]
    assert node["children"][1]["operation"] == ("then" if composite else "leaf")
    assert node["parameters"]["factory_effects"] == ("judge",)
    planned = jv.plan(method.program())
    assert entered == [] and not planned.calls.is_numeric
    assert "factory" in str(planned.calls)
    assert method(7) == 7 and entered == [7]
    rebuilt = method.replace_at((0,), identity(int))
    assert rebuilt.children[1] is factory
    assert rebuilt.effects == method.effects == frozenset({"judge"})
    assert rebuilt.structure()["root"]["parameters"]["factory_effects"] == ("judge",)
    jv.plan(rebuilt.program())
    assert entered == [7]
