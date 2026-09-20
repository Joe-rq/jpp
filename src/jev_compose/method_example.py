"""Complete method construction program; fixed observations, real local checks."""
from __future__ import annotations

import json
from pathlib import Path

from foundation import jv
from .core import Component, Iteration, component, execute, identity, product
from .examples import (SynthesisState, enumerate_candidates, inspect_expressions,
                       make_synthesis)
from .ablations import inspect_without_semantics
from .fixtures import runtime

FACTORY_INPUTS = []


@component("construct_candidate_checker", tuple, Component)
def construct_checker(pair):
    _, candidates = pair
    if len(candidates) > 1:
        return inspect_expressions
    return inspect_without_semantics.then(identity(list), name="single_exact_check")


@component("report_solution", Iteration, dict)
def report_solution(result):
    state = result.state
    return {"solution": state.solution.content if state.solution is not None else None,
            "steps": result.steps, "reason": result.reason,
            "checked": [t.report.content["expression"] for t in state.trials]}


@component("construct_complete_method", SynthesisState, Component)
def construct_method(state):
    size = len(state.specification.content["domain"])
    FACTORY_INPUTS.append(size)
    inspector = inspect_without_semantics
    if size > 1:
        inspector = identity(tuple).bind(construct_checker, list,
            effects=frozenset({"judge", "do"}), name="choose_checker_each_round")
    # An entire method is a value handed to the enclosing solver constructor.
    return make_synthesis(inspector=inspector).then(report_solution, name="solve_and_report")


@component("read_solution_text", dict, str)
def solution_text(report):
    return report["solution"]["expression"] if report["solution"] else "unresolved"


def program():
    solve = identity(SynthesisState).bind(construct_method, dict,
        effects=frozenset({"judge", "do", "gen", "transform"}), name="construct_and_run_method")
    return solve.then(product(identity(dict), solution_text), name="solve_then_reuse_report")


def problem(domain):
    return SynthesisState(jv.mat({"goal": "absolute value", "domain": domain,
                                  "expected": [abs(x) for x in domain]}))


def plan_summary(plan):
    return {key: str(getattr(plan, key)) for key in
            ("calls", "questions", "layers", "do_calls", "gen_calls")}


def run_case(calculation, domain):
    run = execute(calculation, problem(domain), runtime(generator=enumerate_candidates))
    dynamic = []
    for event in run.trace:
        if "generated_structure" in event:
            dynamic.append({"call_id": event["call_id"], "component": event["component"],
                "structure": event["generated_description"],
                "trace_range": [event["continuation_trace_start"], event["continuation_trace_end"]],
                "result_type": type(event["result"]).__name__})
    return {"domain": domain, "output": run.value, "calls": run.stats["calls"],
            "actions": run.stats["effect_requests"]["do"], "dynamic": dynamic}


def main():
    FACTORY_INPUTS.clear()
    calculation = program()
    plan = jv.plan(calculation.program())
    assert FACTORY_INPUTS == [], "Planning must not construct methods"
    cases = [run_case(calculation, [0]), run_case(calculation, list(range(-4, 5)))]
    original = make_synthesis().then(report_solution)
    # This path traverses then / iterate / then / product / then.
    changed = original.replace_at((0, 0, 0, 1, 1), inspect_without_semantics)
    replacement = {"original": run_case(original, list(range(-4, 5))),
                   "changed": run_case(changed, list(range(-4, 5))),
                   "original_again": run_case(original, list(range(-4, 5))),
                   "original_plan": plan_summary(jv.plan(original.program())),
                   "changed_plan": plan_summary(jv.plan(changed.program()))}
    data = {"fixture": "Fixed synthetic observations; local finite-domain checks",
            "static_plan": plan_summary(plan), "factory_inputs": FACTORY_INPUTS,
            "cases": cases, "replacement": replacement}
    output = Path(__file__).resolve().parents[1] / "out" / "method-construction.json"
    output.write_text(json.dumps(data, ensure_ascii=False, indent=2) + "\n")
    print(json.dumps({"cases": [{k: c[k] for k in ("domain", "output", "calls", "actions")}
                                for c in cases],
                      "replacement_calls": {k: replacement[k]["calls"] for k in
                          ("original", "changed", "original_again")},
                      "evidence": str(output)}, ensure_ascii=False, indent=2))


if __name__ == "__main__":
    main()
