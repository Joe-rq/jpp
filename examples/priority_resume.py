"""Independent public-API example: expensive-first, one pending item per step."""
import json
from foundation import jv
from jev_compose import (Partial, Component, Request, Iteration, component, execute,
                         identity, product, iterate, map_partial, continue_with)
from jev_compose.fixtures import runtime
from jev_compose.partial_example import Plans, validate, combine, requests, advance, supplement


@component("expensive_first_one", tuple, dict, effects=("transform",))
def expensive_first(pending):
    if not pending:
        return {}
    selected = max(pending, key=lambda o: o.request.state.resolved().all_mats[0].content["cost"])
    old = selected.request
    mat = old.state.resolved().all_mats[0]
    revised = jv.transform(supplement, mat)
    return {selected.id: Request(jv.state(on=revised), old.question, old.label, old.tag)}


@component("cheap_and_complete", Partial, bool)
def cheap_and_complete(packet):
    return not packet.pending and packet.value.cheapest is not None and packet.value.cheapest["cost"] <= 2


@component("iteration_packet", Iteration, Partial)
def iteration_packet(outcome):
    return outcome.state


resume = continue_with(expensive_first, effects=advance.effects)
finish = iterate(resume, cheap_and_complete, limit=2).then(iteration_packet)


@component("select_caller", Partial, Component)
def select_caller(packet):
    return identity(Partial) if cheap_and_complete(packet) else finish


dynamic_finish = identity(Partial).bind(select_caller, Partial, effects=advance.effects)
solver = validate.then(map_partial(combine))
# Product retains an initial usable packet and demonstrates the changed caller.
method = solver.then(product(identity(Partial), resume.then(product(identity(Partial), dynamic_finish))))


def snapshot(packet):
    return {"cheapest": packet.value.cheapest,
            "pending": [o.request.tag for o in packet.pending],
            "evidence_count": len(packet.evidence)}


if __name__ == "__main__":
    result = execute(method, requests(), runtime())
    before, (after_one, completed) = result.value
    assert before.value.cheapest["cost"] == 9
    assert [o.request.tag for o in before.pending] == ["C", "D"]
    assert [o.request.tag for o in after_one.pending] == ["C"]
    assert after_one.value.cheapest["cost"] == 9
    assert completed.value.cheapest["cost"] == 2 and not completed.pending
    report = {"strategy": "highest-cost unresolved candidate first, one per step",
              "initial": snapshot(before), "after_one": snapshot(after_one),
              "changed_requirement": "cost <= 2 and no pending questions",
              "completed": snapshot(completed), "stats": result.stats,
              "observation_calls": result.stats["calls"],
              "local_checks": result.stats["effect_requests"]["do"],
              "trace": [{k:v for k,v in e.items() if k in ("component", "name", "kind", "steps", "reason")} for e in result.trace]}
    print(json.dumps(report, indent=2, default=str))
