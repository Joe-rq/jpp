"""Public-API integration checks against the frozen J V kernel.

All model observations and calibration records here are synthetic fixtures;
these tests make no network calls and do not measure real JEV accuracy.
"""

from pathlib import Path
import sys

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT))
sys.path.insert(0, str(ROOT / "src"))

from foundation import jv


FIXTURE_PROFILE = {
    "model_version": "composition-contract-synthetic-v1",
    "delta": {"noul": {"immediate": {"p99": 0.01}}},
}


def synthetic_observation(state_text, question_id, question):
    """Deliberately fixed observations, independent of any real model."""
    return {"type": "noul", "noul": 0.5 if "uncertain-fixture" in state_text else 0.95}


def configured_runtime(root):
    rt = jv.Runtime(
        client=jv.FakeClient(rule=synthetic_observation),
        profile=FIXTURE_PROFILE,
        root=str(root),
    )
    rt.calib.put(
        "composition.contract.synthetic", hi=0.8, lo=0.2, n=50,
        status="上岗", set_id="synthetic-fixture-not-model-calibration",
    )
    return rt


def fixture_question():
    return jv.test("Synthetic fixture accepts this input?", calib=jv.calib("composition.contract.synthetic"))


@jv.program(budget=jv.Budget(calls=2))
def observation_component(material, question):
    outcome = jv.cut(jv.judge(jv.state(on=material), question)[0])
    match outcome:
        case jv.Act() | jv.Ignore():
            return outcome
        case jv.Unsure():
            # Explicitly preserve the original unresolved observation for callers.
            return outcome


@jv.program(budget=jv.Budget(calls=4))
def invoke_component(component, material, question):
    return component(material, question)


@jv.program(budget=jv.Budget(calls=4))
def nested_with_outer_observations(component, question):
    before = jv.judge(jv.state(on=jv.lit("outer-before")), question)
    middle = invoke_component(component, jv.lit("inner-input"), question)
    first = jv.cut(before[0])
    last = jv.cut(jv.judge(jv.state(on=jv.lit("outer-after")), question)[0])
    jv.consume([first, last], unsure=lambda outcome: outcome)
    return first, middle, last


@jv.program(budget=jv.Budget(calls=2))
def nested_with_outer_limit(component, question):
    before = jv.judge(jv.state(on=jv.lit("outer-before")), question)
    middle = invoke_component(component, jv.lit("inner-input"), question)
    first = jv.cut(before[0])
    last = jv.cut(jv.judge(jv.state(on=jv.lit("outer-after")), question)[0])
    jv.consume([first, last], unsure=lambda outcome: outcome)
    return first, middle, last


@jv.program(budget=jv.Budget(calls=4))
def nested_preserve_unknown(component, material, question):
    return invoke_component(component, material, question)


@jv.program(budget=jv.Budget(calls=2))
def local_action_component(action, material, question):
    actual = jv.do(action, material, iter_seq=0)
    outcome = jv.cut(jv.judge(jv.state(on=actual), question)[0])
    jv.consume([outcome], unsure=lambda pending: pending)
    return actual.content, outcome


@jv.program(budget=jv.Budget(calls=4))
def replayable_action_parent(component, action, question):
    return component(action, jv.lit("local-only-action"), question)


def test_component_runs_independently(tmp_path):
    with configured_runtime(tmp_path) as rt:
        outcome = observation_component(jv.lit("standalone"), fixture_question())
        assert isinstance(outcome, jv.Act)
        assert rt.stats["calls"] == 1
        assert rt.stats["questions"] == 1


def test_parameterized_two_level_nesting_keeps_outer_pending_work_and_counts(tmp_path):
    with configured_runtime(tmp_path) as rt:
        outcomes = nested_with_outer_observations(observation_component, fixture_question())
        assert all(isinstance(outcome, jv.Act) for outcome in outcomes)
        assert rt.stats["calls"] == 3
        assert rt.stats["questions"] == 3
        assert rt.client.questions_asked == 3
        assert rt.budget.calls == 4


def test_nested_calls_cannot_reset_outer_budget(tmp_path):
    with configured_runtime(tmp_path) as rt:
        first, middle, last = nested_with_outer_limit(observation_component, fixture_question())
        assert isinstance(first, jv.Act)
        assert isinstance(middle, jv.Act)
        assert isinstance(last, jv.Unsure)
        assert last.cause == "budget"
        assert rt.stats["calls"] == 2
        assert rt.client.calls == 2
        assert rt.budget.calls == 2


def test_unknown_is_preserved_as_an_observation_through_two_levels(tmp_path):
    with configured_runtime(tmp_path) as rt:
        question = fixture_question()
        outcome = nested_preserve_unknown(observation_component, jv.lit("uncertain-fixture"), question)
        assert isinstance(outcome, jv.Unsure)
        assert outcome.cause == "band"
        assert outcome.q_hash == question.text_hash
        assert outcome.reading is not None
        assert outcome.consumed is True
        assert rt.stats["calls"] == 1
        assert rt.stats["unsure"] == 1


def test_replay_in_new_runtime_does_not_repeat_local_action(tmp_path):
    invocations = []

    def local_counter(material):
        invocations.append(material.content)
        return {"recorded": material.content, "count": len(invocations)}

    action = jv.Action("composition.contract.local-counter", fn=local_counter, taint_out="trusted")
    with configured_runtime(tmp_path) as first_rt:
        first_value, first_outcome = replayable_action_parent(local_action_component, action, fixture_question())
        assert isinstance(first_outcome, jv.Act)
        assert first_rt.client.calls == 1

    with configured_runtime(tmp_path) as replay_rt:
        replay_value, replay_outcome = replayable_action_parent(local_action_component, action, fixture_question())
        assert isinstance(replay_outcome, jv.Act)
        assert replay_rt.client.calls == 0

    assert invocations == ["local-only-action"]
    assert replay_value == first_value == {"recorded": "local-only-action", "count": 1}
