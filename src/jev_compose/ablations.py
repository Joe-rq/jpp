"""Small finite-domain ablations; observations are synthetic, never real JEV data.

All arms use the existing make_synthesis/feedback constructor and checker. The
controls replace one component or one generator capability without changing the
language core. A fresh runtime and distinct candidates prevent replay from being
mistaken for actual verification work in these runs.
"""
from __future__ import annotations

from dataclasses import dataclass

from foundation import jv

from .core import component, execute
from .examples import (EXPRESSIONS, VERIFY, absolute_value_problem,
                       enumerate_candidates, make_synthesis)
from .fixtures import runtime
from .observation import Observation


@dataclass(frozen=True)
class ExactTrial:
    """Same candidate/report interface, explicitly absent semantic observations."""
    candidate: jv.Mat
    report: jv.Mat
    selection: Observation | None = None
    complexity: Observation | None = None


@component("first_candidate_and_exact_check", tuple, list, effects=("do",))
def inspect_without_semantics(pair):
    state, candidates = pair
    if not candidates:
        return []
    candidate = candidates[0]
    outcome = jv.do(VERIFY, candidate, state.specification, iter_seq=len(state.trials))
    report = jv.on_fail(outcome, alt=jv.mat({"passed": False, "failure": "verifier_failed"}))
    report.content
    return [ExactTrial(candidate, report)]


def enumerate_without_counterexamples(prompt, context, n, retry_seq):
    """Keep the same finite grammar/order and tried set; omit only CE filtering."""
    tried = set(context[-1].content.get("tried", []))
    return [{"expression": expression} for expression in EXPRESSIONS if expression not in tried][:n]


def _run_arm(name, *, semantic_inspector=True, use_counterexamples=True):
    calculation = make_synthesis(limit=16) if semantic_inspector else make_synthesis(
        inspector=inspect_without_semantics, limit=16)
    generator = enumerate_candidates if use_counterexamples else enumerate_without_counterexamples
    rt = runtime(generator=generator)
    initial = absolute_value_problem()
    result = execute(calculation, initial, rt, name=f"ablation_{name}",
                     budget=jv.Budget(calls=128, layers=128))
    state = result.value.state
    reports = [trial.report.content for trial in state.trials]
    checked = [report["expression"] for report in reports]

    # Every candidate is unique in a fresh runtime. The recorded do requests are
    # therefore distinct actual checker executions, not replayed Action results.
    if len(set(checked)) != len(checked) or rt.stats["do"] != len(checked):
        raise AssertionError("Ablation cannot equate unique checker reports with actual verification work")
    domain = initial.specification.content["domain"]
    checked_inputs = sum(len(domain) if report.get("passed")
                         else domain.index(report["counterexample"]["x"]) + 1
                         for report in reports)
    final_report = next((report for report in reports if report.get("passed")), None)
    return {
        "name": name,
        "semantic_inspector": semantic_inspector,
        "counterexamples_filter_generator": use_counterexamples,
        "tried_deduplication": True,
        "limit": 16,
        "iterations": result.value.steps,
        "calls": rt.client.calls,
        "physical_questions": rt.client.questions_asked,
        "actualchecked": len(checked),
        "actualchecked_basis": "Distinct candidate reports from actual jv.do checker executions in a fresh runtime",
        "actual_domain_inputs_executed": checked_inputs,
        "checked_candidates": checked,
        "result": state.solution.content if state.solution is not None else None,
        "reason": result.value.reason,
        "exhausted": state.exhausted,
        "verification": final_report,
        "semantic_observations": [
            {"selection": trial.selection.to_dict() if trial.selection is not None else None,
             "complexity": trial.complexity.to_dict() if trial.complexity is not None else None}
            for trial in state.trials
        ],
        "counterexamples": [report["counterexample"] for report in reports if "counterexample" in report],
    }


def run_ablations() -> dict:
    """Return JSON-serializable measured results for three fixed-observation arms."""
    full = _run_arm("full")
    without_semantics = _run_arm("without_semantic_select_measure", semantic_inspector=False)
    without_counterexamples = _run_arm("without_counterexample_filter", use_counterexamples=False)
    exact_saved = without_semantics["actualchecked"] - full["actualchecked"]
    ce_saved = without_counterexamples["actualchecked"] - full["actualchecked"]
    return {
        "fixture": "Synthetic fixed observations; not real JEV accuracy or a measured JEV speedup",
        "problem": {"goal": "absolute value", "domain": list(range(-4, 5)),
                    "candidate_grammar": list(EXPRESSIONS)},
        "shared": "Same make_synthesis/feedback, exact finite-domain checker, candidates and limit 16",
        "arms": [full, without_semantics, without_counterexamples],
        "contrasts": {
            "checks_saved_by_semantic_inspector_in_this_fixture": exact_saved,
            "extra_model_calls_for_semantic_inspector": full["calls"] - without_semantics["calls"],
            "checks_saved_by_counterexample_filter_in_this_fixture": ce_saved,
        },
        "interpretation": [
            ("The fixed semantic inspector reduced exact candidate checks in this fixture."
             if exact_saved > 0 else
             "The fixed semantic inspector did not reduce exact candidate checks in this fixture; it adds model calls."),
            ("Counterexample filtering reduced exact candidate checks in this fixture."
             if ce_saved > 0 else
             "Counterexample filtering did not reduce exact candidate checks in this fixture."),
            "A passing result proves only the declared nine-input domain, not unbounded integer correctness.",
            "These controls test replaceable algorithm components; they do not isolate a language-specific speedup.",
        ],
    }
