"""Validated candidates -> feasible combinations -> caller-directed continuation."""
from __future__ import annotations
from dataclasses import dataclass
from itertools import combinations
import json
from pathlib import Path

from foundation import jv
from .core import component, execute
from .partial import Partial, checkpoint, map_partial, continue_with
from .observation import Request, batch_observe, refine
from .fixtures import runtime

CHECKED = []


def check_candidate(material):
    data = material.content
    CHECKED.append(data["name"])
    return {"name": data["name"], "cost": data["cost"], "skills": data["skills"],
            "valid": data["cost"] > 0 and bool(data["skills"])}


VERIFY = jv.register_action("composition.partial.verify", fn=check_candidate,
    taint_out="trusted", reason="Local exact candidate schema check; no external side effect")


@dataclass(frozen=True)
class CandidateState:
    observations: tuple
    checked: tuple = ()
    history: tuple = ()


def check_new(observations, checked):
    done = {m.content["name"] for m in checked}
    out = list(checked)
    for observation in observations:
        if observation.resolved and observation.value is True and observation.request.tag not in done:
            source = observation.request.state.resolved().all_mats[0]
            outcome = jv.do(VERIFY, source, iter_seq=len(out))
            checked_material = jv.on_fail(outcome, alt=jv.mat("verification failed"))
            if not isinstance(checked_material.content, dict):
                raise RuntimeError("Local candidate verification failed")
            out.append(checked_material)
            done.add(observation.request.tag)
    return tuple(out)


@component("accepted_candidates", CandidateState, tuple)
def current(state):
    return tuple(m for m in state.checked if m.content["valid"])


@component("unresolved_questions", CandidateState, tuple)
def unresolved(state):
    return tuple(o for o in state.observations if not o.resolved)


@component("all_observation_evidence", CandidateState, tuple)
def evidence(state):
    return state.history


@component("advance_candidate_checks", tuple, CandidateState, effects=("judge", "do", "transform"))
def advance(pair):
    state, strategy = pair
    replacements = strategy(unresolved(state))
    updated = refine(state.observations, replacements)
    history = state.history + tuple(new for old, new in zip(state.observations, updated) if new is not old)
    return CandidateState(updated, check_new(updated, state.checked), history)


def expose(state):
    return checkpoint(state, view=current, pending=unresolved, evidence=evidence, advance=advance)


@component("validate_candidates", list, Partial, effects=("judge", "do"))
def validate(requests):
    observations = tuple(batch_observe(requests))
    return expose(CandidateState(observations, check_new(observations, ()), observations))


@dataclass(frozen=True)
class Plans:
    alternatives: tuple

    @property
    def cheapest(self):
        return min(self.alternatives, key=lambda p: p["cost"]) if self.alternatives else None


@component("exact_constraint_combinations", tuple, Plans)
def combine(candidates):
    rows = [m.content for m in candidates]
    plans = []
    for count in range(1, len(rows) + 1):
        for group in combinations(rows, count):
            cost = sum(c["cost"] for c in group)
            skills = set().union(*(set(c["skills"]) for c in group))
            if cost <= 10 and {"web", "database"} <= skills:
                plans.append({"members": [c["name"] for c in group], "cost": cost})
    return Plans(tuple(plans))


def supplement(material):
    # Fixture: a new evidence revision resolves C positively and D negatively.
    data = material.content
    return {**data, "unknown": False, "flag": data["name"] == "C", "revision": 1}


def selected_updates(items, cheap_only):
    updates = {}
    for item in items:
        old = item.request.state.resolved().all_mats[0]
        if cheap_only and old.content["cost"] > 2:
            continue
        new = jv.transform(supplement, old)
        updates[item.id] = Request(jv.state(on=new), item.request.question, item.request.label, item.request.tag)
    return updates


@component("only_promising_unresolved", tuple, dict, effects=("transform",))
def promising(items):
    return selected_updates(items, True)


@component("all_remaining_unresolved", tuple, dict, effects=("transform",))
def everything(items):
    return selected_updates(items, False)


@component("one_feasible_plan_is_enough", Plans, bool)
def enough(plans):
    return plans.cheapest is not None


@component("require_cost_at_most_two", Plans, bool)
def cheaper(plans):
    return plans.cheapest is not None and plans.cheapest["cost"] <= 2


def requests():
    rows = [("A", 4, ["web"], True), ("B", 5, ["database"], True),
            ("C", 2, ["web", "database"], None), ("D", 20, ["web"], None)]
    return [Request(jv.state(on=jv.mat({"name": name, "cost": cost, "skills": skills,
                                      "flag": flag, "unknown": flag is None, "revision": 0})),
                    jv.test("候选符合当前任务需要吗？", calib=jv.calib("demo.flag")), name, name)
            for name, cost, skills, flag in rows]


def snapshot(packet):
    return {"best": packet.value.cheapest, "pending": [o.request.tag for o in packet.pending],
            "evidence": [o.to_dict() for o in packet.evidence], "checks_so_far": list(CHECKED)}


@component("partial_results_workflow", list, dict, effects=("judge", "do", "transform"))
def workflow(items):
    solver = validate.then(map_partial(combine))
    initial = solver(items)
    used = initial.value.cheapest if initial.satisfies(enough) else None
    first = snapshot(initial)
    improved = continue_with(promising, effects=advance.effects)(initial)
    second = snapshot(improved)
    final = continue_with(everything, effects=advance.effects)(improved)
    return {"used_before_complete": used, "initial": first, "improved": second,
            "final": snapshot(final), "stronger_requirement_met": improved.satisfies(cheaper),
            "original_still_pending": [o.request.tag for o in initial.pending],
            "original_evidence_retained": all(o is final.evidence[i] for i, o in enumerate(initial.evidence)),
            "terminal": final.continuation is None}


def main(output="partial-report.json"):
    CHECKED.clear()
    rt = runtime()
    run = execute(workflow, requests(), rt)
    data = {"fixture": "Fixed observations and actual local checks; not real-model quality",
            **run.value, "calls": run.stats["calls"], "actions": run.stats["effect_requests"]["do"],
            "dynamic": [{k: v for k, v in e.items() if k not in ("generated_structure", "result")}
                        for e in run.trace if e["operation"] == "bind"]}
    CHECKED.clear()
    direct = execute(direct_workflow, requests(), runtime())
    data["direct_equivalent"] = (direct.value == run.value and direct.stats["calls"] == run.stats["calls"]
                                 and direct.stats["effect_requests"]["do"] == run.stats["effect_requests"]["do"])
    data["direct_calls"] = direct.stats["calls"]
    Path(output).write_text(json.dumps(data, ensure_ascii=False, indent=2) + "\n")
    summary = {stage: {k: data[stage][k] for k in ("best", "pending", "checks_so_far")}
               for stage in ("initial", "improved", "final")}
    summary.update({k: data[k] for k in ("used_before_complete", "calls", "actions", "direct_equivalent")})
    print(json.dumps(summary, ensure_ascii=False, indent=2))


@component("direct_partial_control", list, dict, effects=("judge", "do", "transform"))
def direct_workflow(items):
    """Same algorithm bodies, manually carry state and reapply the result view."""
    observations = tuple(batch_observe(items))
    state = CandidateState(observations, check_new(observations, ()), observations)
    def capture(state):
        return {"best": combine(current(state)).cheapest,
                "pending": [o.request.tag for o in unresolved(state)],
                "evidence": [o.to_dict() for o in state.history], "checks_so_far": list(CHECKED)}
    first = capture(state)
    used = combine(current(state)).cheapest if enough(combine(current(state))) else None
    improved = advance((state, promising))
    second = capture(improved)
    final = advance((improved, everything))
    return {"used_before_complete": used, "initial": first, "improved": second, "final": capture(final),
            "stronger_requirement_met": cheaper(combine(current(improved))),
            "original_still_pending": [o.request.tag for o in unresolved(state)],
            "original_evidence_retained": all(o is final.history[i] for i, o in enumerate(state.history)),
            "terminal": not unresolved(final)}
