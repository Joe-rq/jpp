from __future__ import annotations
from dataclasses import replace
from pathlib import Path
import json
import sys

ROOT = Path(__file__).resolve().parents[1]
sys.path[:0] = [str(ROOT / "src"), str(ROOT)]

import pytest
from foundation import jv
from jev_compose import (Component, CompositionError, Iteration, Request, at_least, batch_observe,
                         branch, component, execute, identity, observe, product,
                         question_from_dict, question_to_dict, refine)
from jev_compose.examples import (InquiryState, absolute_value_problem, balanced, enumerate_candidates,
                                  evaluate_expression, identified_target, make_inquiry, make_synthesis,
                                  make_solver_from_strategy, sequential)
from jev_compose.fixtures import runtime


def flag_request(value=None, *, address="record", label="flag"):
    material = jv.mat({"unknown": True} if value is None else {"flag": value}, addr=address)
    return Request(jv.state(on=material), jv.test("标记是否成立？", calib=jv.calib("demo.flag")), label)


def test_construction_and_question_roundtrip_do_not_execute():
    rt = runtime()
    questions = [jv.test("标记成立吗", calib=jv.calib("demo.flag")),
                 jv.select("选哪个", calib=jv.calib("demo.pick")),
                 jv.measure("复杂度", scale=("低", "高"), calib=jv.calib("demo.complexity"))]
    with rt:
        for q in questions:
            saved = json.loads(json.dumps(question_to_dict(q)))
            assert question_to_dict(question_from_dict(saved)) == question_to_dict(q)
        calculate = make_inquiry().then(identified_target)
        assert calculate.describe()["effects"] == ["judge", "transform"]
        assert rt.client.calls == 0


def test_incompatible_composition_fails_at_construction():
    with pytest.raises(CompositionError, match="Cannot connect"):
        identity(int).then(identity(str))


def test_kernel_checker_still_checks_effectful_leaf_source():
    with pytest.raises(CompositionError, match="J-11"):
        @component("invalid_material", int, object, effects=("judge",))
        def invalid(n):
            return jv.state(on="untracked literal")


def test_association_and_identity_preserve_observations():
    from jev_compose import Observation
    left = identity(Request).then(observe).then(identity(Observation))
    right = identity(Request).then(observe.then(identity(Observation)))
    a, b = execute(left, flag_request(True), runtime()), execute(right, flag_request(True), runtime())
    assert a.value.to_dict() == b.value.to_dict()
    assert (a.stats["calls"], b.stats["calls"]) == (1, 1)


def test_native_and_library_nested_programs_keep_budget_and_result():
    leaf = observe.program(name="leaf", budget=jv.Budget(calls=3))

    @component("middle", Request, object, effects=("judge",))
    def middle(request):
        return leaf(request)

    nested = middle.program(name="middle_scope")

    @component("outer", Request, object, effects=("judge",))
    def outer(request):
        return nested(request)

    result = execute(outer, flag_request(True), runtime(), budget=jv.Budget(calls=1))
    assert result.value.resolved and result.value.value is True
    assert result.stats["calls"] == 1


def test_independent_questions_share_one_kernel_call():
    first = flag_request(True)
    second = Request(first.state, jv.test("这个 flag 为真吗？", calib=jv.calib("demo.flag")))
    result = execute(batch_observe, [first, second], runtime())
    assert [o.value for o in result.value] == [True, True]
    assert result.stats["calls"] == 1
    assert result.stats["questions"] == 2


def test_uncertainty_is_local_and_only_requested_unknown_is_refined():
    requests = [flag_request(True, address="a"), flag_request(False, address="b"),
                flag_request(None, address="c"), flag_request(True, address="d")]
    @component("partial_and_continue", list, tuple, effects=("judge",))
    def solve(items):
        observations = batch_observe(items)
        enough = at_least(observations, 2)
        not_yet = at_least(observations, 3)
        unknown = observations[2]
        updated = refine(observations, {unknown.id: flag_request(True, address="c-revealed")})
        return observations, enough, not_yet, updated, at_least(updated, 3)
    result = execute(solve, requests, runtime())
    old, enough, unknown, new, complete = result.value
    assert enough.value is True and (enough.lower, enough.upper) == (2, 3)
    assert unknown.value is None and complete.value is True
    assert all(new[i] is old[i] for i in (0, 1, 3))
    assert new[2].id != old[2].id
    # The existing kernel can reuse identical known flag observations across records.
    assert result.stats["calls"] == 3


def test_shared_unknown_references_get_one_observation_on_refine():
    @component("shared", Request, tuple, effects=("judge",))
    def shared(request):
        unknown = observe(request)
        return refine((unknown, unknown), {unknown.id: flag_request(True, address="revealed")})
    result = execute(shared, flag_request(None), runtime())
    assert result.value[0] is result.value[1]
    assert result.stats["calls"] == 2


def test_branch_executes_only_selected_arm():
    @component("choose_left", Request, bool)
    def choose_left(request):
        return True
    @component("unselected", Request, type(None), effects=("judge",))
    def unselected(request):
        raise AssertionError("Unselected branch executed")
    @component("selected", Request, type(None), effects=("judge",))
    def selected(request):
        observe(request)
        return None
    result = execute(branch(choose_left, selected, unselected), flag_request(True), runtime())
    assert result.stats["calls"] == 1


def test_strategy_factory_returns_a_solver_that_is_reused_twice():
    @component("choose_solver", InquiryState, Component)
    def choose_solver(state):
        return make_inquiry(balanced, limit=8)
    dynamic = make_solver_from_strategy(choose_solver).then(identified_target)
    nested = identity(InquiryState).then(dynamic)
    source = InquiryState(jv.mat({"target": 93}), tuple(range(128)))
    result = execute(product(nested, nested, name="reused_solver"), source, runtime())
    assert result.value == (93, 93)
    assert any(e["operation"] == "bind" for e in result.trace)
    assert result.stats["calls"] == 7   # second solver uses the same kernel's observation cache


def test_same_inquiry_constructor_accepts_two_partition_strategies():
    source = InquiryState(jv.mat({"target": 93}), tuple(range(128)))
    a = execute(make_inquiry(balanced, limit=128), source, runtime())
    b = execute(make_inquiry(sequential, limit=128), source, runtime())
    assert a.value.state.remaining == b.value.state.remaining == (93,)
    assert (a.stats["calls"], b.stats["calls"]) == (7, 94)


def test_budget_returns_explicit_partial_candidates_not_a_false_solution():
    source = InquiryState(jv.mat({"target": 93}), tuple(range(128)))
    result = execute(make_inquiry(), source, runtime(), budget=jv.Budget(calls=2))
    assert result.value.state.paused
    assert len(result.value.state.remaining) > 1
    assert result.value.state.observations[-1].cause == "budget"
    assert result.stats["calls"] == 2


def test_expression_feedback_checks_real_behavior_and_reuses_counterexamples():
    result = execute(make_synthesis(), absolute_value_problem(), runtime(generator=enumerate_candidates))
    state = result.value.state
    assert state.solution is not None
    expression = state.solution.content["expression"]
    assert all(evaluate_expression(expression, x) == abs(x) for x in range(-4, 5))
    assert len(state.trials) == 3
    assert all(not t.report.content["passed"] for t in state.trials[:-1])
    assert state.trials[-1].report.content["passed"]
    for index, trial in enumerate(state.trials):
        for previous in state.trials[:index]:
            counterexample = previous.report.content["counterexample"]
            assert evaluate_expression(trial.candidate.content["expression"], counterexample["x"]) == counterexample["expected"]
    assert {t.selection.request.question.op for t in state.trials} == {"select"}
    assert {t.complexity.request.question.op for t in state.trials} == {"measure"}
    assert result.stats["effect_requests"]["do"] == 3


def test_two_algorithms_share_iterate_and_accept_replacement_problem():
    inquiry = make_inquiry()
    synthesis = make_synthesis()
    assert inquiry.operation == synthesis.operation == "iterate"
    problem = absolute_value_problem()
    spec = dict(problem.specification.content)
    spec["goal"] = "返回 x 的平方"
    spec["expected"] = [x * x for x in spec["domain"]]
    changed = replace(problem, specification=jv.mat(spec))
    result = execute(synthesis, changed, runtime(generator=enumerate_candidates))
    expression = result.value.state.solution.content["expression"]
    assert all(evaluate_expression(expression, x) == x * x for x in range(-4, 5))


def test_persistent_replay_reuses_judgments_generation_and_actual_checks(tmp_path):
    generated = []
    def generator(prompt, context, n, retry_seq):
        generated.append(retry_seq)
        return enumerate_candidates(prompt, context, n, retry_seq)
    first = execute(make_synthesis(), absolute_value_problem(), runtime(root=tmp_path, generator=generator))
    first_gen_count = len(generated)
    second = execute(make_synthesis(), absolute_value_problem(), runtime(root=tmp_path, generator=generator))
    assert first.value.state.solution.content == second.value.state.solution.content
    assert second.stats["calls"] == 0 and len(generated) == first_gen_count
