"""Offline, executable tour of J++ composition mechanisms."""
import argparse
import json
from foundation import jv
from jev_compose import execute
from jev_compose.examples import (InquiryState, absolute_value_problem,
    enumerate_candidates, make_inquiry, make_synthesis)
from jev_compose.fixtures import runtime


def demo():
    inquiry = execute(make_inquiry(), InquiryState(jv.mat({"target": 731}), tuple(range(1000))), runtime())
    found = inquiry.value.state
    synthesis = execute(make_synthesis(), absolute_value_problem(), runtime(generator=enumerate_candidates))
    built = synthesis.value.state
    assert found.remaining == (731,)
    assert len(found.observations) <= 10
    assert built.solution is not None
    return {
        "mode": "offline synthetic observations; no model API requests",
        "inquiry": {"candidates": 1000, "identified": found.remaining[0],
                    "questions": len(found.observations)},
        "synthesis": {"expression": built.solution.content["expression"],
                      "trials": len(built.trials), "verified_inputs": 9,
                      "generator": "finite enumeration, not an LLM"},
        "actual_api_cost_usd": 0,
    }


def main(argv=None):
    import sys
    arguments = list(sys.argv[1:] if argv is None else argv)
    if arguments and arguments[0] == "methods":
        from jev_compose.method_example import main as methods_main
        parser = argparse.ArgumentParser(prog="jpp methods", description="Run complete dynamic methods and save plan/trace/results")
        parser.add_argument("--output", default="jpp-methods.json")
        options = parser.parse_args(arguments[1:])
        return methods_main(output=options.output)
    if arguments and arguments[0] == "towow":
        from .towow import main as towow_main
        return towow_main(arguments[1:])
    parser = argparse.ArgumentParser(prog="jpp", description="J++ experimental language")
    parser.add_argument("command", choices=["demo", "methods", "towow"], nargs="?", default="demo")
    parser.parse_args(arguments)
    print(json.dumps(demo(), ensure_ascii=False, indent=2))
