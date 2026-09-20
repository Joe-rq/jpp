import pytest
from foundation import jv
from jev_compose import (Component, CompositionError, Partial, checkpoint, continue_with,
                         execute, identity, map_partial, product)
from jev_compose.fixtures import runtime
from jev_compose.partial_example import (CHECKED, advance, combine, direct_workflow, everything,
    promising, requests, validate, workflow)


def test_two_algorithms_use_partial_then_resume_and_keep_prior_work():
    CHECKED.clear()
    run = execute(workflow, requests(), runtime())
    r = run.value
    assert r["used_before_complete"] == {"members": ["A", "B"], "cost": 9}
    assert r["initial"]["pending"] == ["C", "D"]
    assert r["improved"]["best"] == {"members": ["C"], "cost": 2}
    assert r["improved"]["pending"] == ["D"]
    assert r["final"]["pending"] == [] and r["terminal"]
    assert r["original_still_pending"] == ["C", "D"] and r["original_evidence_retained"]
    assert CHECKED == ["A", "B", "C"] and run.stats["effect_requests"]["do"] == 3
    assert run.stats["calls"] == 6
    old = r["initial"]["evidence"][2]
    new = r["improved"]["evidence"][-1]
    assert old["label"] == new["label"] == "C" and old["id"] != new["id"]
    assert old["material_hashes"] != new["material_hashes"]
    assert not old["resolved"] and new["resolved"]
    dynamic = [e for e in run.trace if "generated_structure" in e]
    assert len(dynamic) == 2
    CHECKED.clear()
    direct = execute(direct_workflow, requests(), runtime())
    assert direct.value == run.value and direct.stats["calls"] == run.stats["calls"]
    assert CHECKED == ["A", "B", "C"]


def test_mapping_and_terminal_resume_do_not_execute_strategy():
    seen = []
    strategy = Component("never", tuple, dict, lambda x: seen.append(x) or {})
    packet = Partial(3, (), ("existing",), None)
    mapped = map_partial(Component("double", int, int, lambda x: 2*x))(packet)
    assert mapped.value == 6 and mapped.evidence is packet.evidence
    assert continue_with(strategy, effects=frozenset())(mapped) is mapped
    assert seen == []


def test_alternate_strategies_and_repeated_projection_use_same_protocol():
    CHECKED.clear()
    rt = runtime()
    with rt:
        packet = validate.program()(requests())
        mapped = map_partial(combine)(packet)
        final = continue_with(everything, effects=advance.effects).program()(mapped)
    assert final.value.cheapest["cost"] == 2 and not final.pending
    assert packet.pending == mapped.pending and len(packet.pending) == 2
    assert CHECKED == ["A", "B", "C"]


def test_nested_zero_budget_retains_unresolved_and_performs_no_checks():
    CHECKED.clear()
    nested = workflow.program()
    @jv.program(budget=jv.Budget(calls=0))
    def outer(items):
        return nested(items)
    rt = runtime()
    with rt:
        result = outer(requests())
    assert result["used_before_complete"] is None
    assert not result["terminal"] and CHECKED == [] and rt.client.calls == 0


def test_checkpoint_rejects_strategy_effects_outside_advance_contract():
    v = identity(int)
    pending = Component("remaining", int, tuple, lambda x: ("x",))
    evidence = Component("evidence", int, tuple, lambda x: ())
    advance = Component("step", tuple, int, lambda pair: pair[0])
    packet = checkpoint(1, view=v, pending=pending, evidence=evidence, advance=advance)
    bad = Component("effectful", tuple, dict, lambda x: {}, frozenset({"do"}))
    with pytest.raises(CompositionError, match="capabilities"):
        packet.continuation(bad)


def test_partial_result_can_be_nested_in_product_and_requirement_changed():
    packet = Partial(7, ("pending",), ("source",), None)
    yes = Component("enough", int, bool, lambda x: x >= 5)
    no = Component("more", int, bool, lambda x: x >= 9)
    assert packet.satisfies(yes) and not packet.satisfies(no)
    method = product(identity(Partial), map_partial(Component("square", int, int, lambda x: x*x)))
    output = method(packet)
    assert output[0] is packet and output[1].value == 49 and output[1].pending == packet.pending
