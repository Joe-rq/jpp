"""Independent installed-package author exercise: detect inconsistent test runs."""
from dataclasses import dataclass, asdict, is_dataclass, fields
from collections.abc import Mapping
import json
from pathlib import Path
import foundation
import jev_compose
from foundation import jv
from jev_compose import Component, component, execute, identity, product
from jev_compose.fixtures import runtime


@dataclass(frozen=True)
class Run:
    case: str
    outcome: str


@dataclass(frozen=True)
class Batch:
    runs: tuple[Run, ...]


@component("flaky_cases", Batch, list[str])
def flaky_cases(batch):
    outcomes = {}
    for run in batch.runs:
        outcomes.setdefault(run.case, set()).add(run.outcome)
    return sorted(case for case, values in outcomes.items() if len(values) > 1)


@component("case_count", Batch, int)
def case_count(batch):
    return len(set(run.case for run in batch.runs))


@component("summarize", tuple[list[str], int], dict)
def summarize(parts):
    flaky, total = parts
    return {"flaky_cases": flaky, "total_cases": total,
            "flaky_count": len(flaky), "consistent_count": total - len(flaky)}


def build_report(detector: Component) -> Component:
    """Receive a complete method as a value, embed it, return a new method."""
    return product(detector, case_count).then(summarize)


@component("select_report", Batch, Component)
def select_report(batch):
    return build_report(flaky_cases)


@component("release_decision", dict, str)
def release_decision(report):
    return "review inconsistent cases" if report["flaky_count"] else "no inconsistency observed"


def json_default(value):
    if is_dataclass(value):
        return {field.name: getattr(value, field.name) for field in fields(value)}
    if isinstance(value, Mapping):
        return dict(value)
    if isinstance(value, (set, frozenset)):
        return sorted(value)
    if hasattr(value, "to_dict"):
        return value.to_dict()
    raise TypeError(f"Unsupported output type: {type(value).__name__}")


def main():
    # Complete detector is passed to build_report, returned, and reused as a child.
    report_method = build_report(flaky_cases)
    # A Component factory returns the same composed method for dynamic execution.
    dynamic_report = identity(Batch).bind(select_report, dict, effects=frozenset())
    final_method = product(report_method, dynamic_report.then(release_decision))
    examples = {
        "mixed": Batch((Run("login", "pass"), Run("login", "fail"),
                        Run("checkout", "pass"), Run("checkout", "pass"),
                        Run("search", "fail"), Run("search", "fail"))),
        "all_consistent": Batch((Run("login", "pass"), Run("login", "pass"))),
        "empty": Batch(()),
    }
    records = {}
    for name, batch in examples.items():
        result = execute(final_method, batch, runtime())
        records[name] = {"input": asdict(batch), "value": result.value,
                         "stats": result.stats,
                         "trace": [{key: value for key, value in event.items()
                                    if key != "generated_structure"} for event in result.trace]}
    assert records["mixed"]["value"] == (
        {"flaky_cases": ["login"], "total_cases": 3, "flaky_count": 1, "consistent_count": 2},
        "review inconsistent cases")
    assert records["all_consistent"]["value"][0]["flaky_cases"] == []
    assert records["empty"]["value"][0]["total_cases"] == 0
    payload = {"imports": {"jev_compose": jev_compose.__file__, "foundation": foundation.__file__},
               "description": final_method.describe(),
               "plan": str(jv.plan(final_method.program())), "runs": records}
    Path(__file__).with_name("output.json").write_text(
        json.dumps(payload, ensure_ascii=False, indent=2, default=json_default) + "\n")
    print(json.dumps({name: record["value"] for name, record in records.items()}, ensure_ascii=False))


if __name__ == "__main__":
    main()
