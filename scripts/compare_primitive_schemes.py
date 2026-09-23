"""Fields vs types: which factor keeps routing stable under judge confusion?

Two factors vary independently in this experiment:

- **taxonomy**: the original seven types, or our five;
- **router**: type-tag routing (Action/Obligation|Directive -> schedule,
  Policy|Rule -> monitor, else reflection), or the capability router of the
  submitted method, where fields are parsed from the text and routing never
  consults the judged type.

The capability rows run the real ``build_note_compiler`` — the same method the
examples use — with the taxonomy injected via ``primitive_types``.  The tag
rows apply the tag router to the same compiled primitives.  The fixture is
made to answer the *partner* type, simulating a judge that confuses the merged
pair endpoints.

Honest expectation: type-tag routing flips under both taxonomies; capability
routing is stable under both.  Robustness comes from the routing policy, not
from the number of types — so the five-type model is argued on expressiveness
grounds elsewhere, not on this metric.

Synthetic fixtures only; no model client.  Run: python scripts/compare_primitive_schemes.py
"""

from __future__ import annotations

import json
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "src"))
sys.path.insert(0, str(ROOT / "examples"))

from jev_compose import execute
from jev_compose.fixtures import runtime
from primitive_ir_method import (
    PRIMITIVE_TYPES, AnalystNote, build_note_compiler,
)

SEVEN_TYPES = ("Entity", "Resource", "Event", "Obligation", "Action", "Policy", "Ledger")
FIVE_TYPES = PRIMITIVE_TYPES
TAXONOMIES = (("seven", SEVEN_TYPES), ("five", FIVE_TYPES))

TO_FIVE = {
    "Entity": "Participant",
    "Resource": "Object",
    "Event": "Occurrence",
    "Action": "Directive",
    "Obligation": "Directive",
    "Policy": "Rule",
}

# Three confusable pairs, one span each direction.
CORPUS = (
    ("event/action", "Event", "The delegation arrived in Seoul on Tuesday."),
    ("event/action", "Action", "Book the venue before Thursday."),
    ("obligation/policy", "Obligation", "The tenant must pay rent by Friday."),
    ("obligation/policy", "Policy", "If the index dips below baseline, suspend trading."),
    ("entity/resource", "Entity", "The leasing office handles renewals."),
    ("entity/resource", "Resource", "Two conference rooms are available this week."),
)

PARTNER = {"Event": "Action", "Action": "Event",
           "Obligation": "Policy", "Policy": "Obligation",
           "Entity": "Resource", "Resource": "Entity"}

# What a correct routing should produce for the true type.
EXPECTED = {"Event": "reflection", "Action": "schedule",
            "Obligation": "schedule", "Policy": "monitor",
            "Entity": "reflection", "Resource": "reflection"}


def tag_router(primitive_types):
    """Route purely by type tags, given a taxonomy."""
    names = set(primitive_types)
    schedule_types = frozenset({"Action", "Obligation", "Directive"} & names)
    monitor_types = frozenset({"Policy", "Rule"} & names)

    def route(primitives) -> str:
        types = {p.type for p in primitives if p.type is not None}
        if types & schedule_types:
            return "schedule"
        if types & monitor_types:
            return "monitor"
        return "reflection"
    return route


def run_method(primitive_types, intent, text):
    """Compile one span through the real method and return its report."""
    note = AnalystNote(text, ((0, intent),))
    return execute(build_note_compiler(primitive_types=primitive_types),
                   note, runtime()).value


def main():
    rows = []
    for pair, true_type, text in CORPUS:
        confused_type = PARTNER[true_type]
        row = {"pair": pair, "text": text, "true_type": true_type,
               "confused_type": confused_type, "expected": EXPECTED[true_type]}
        for label, types in TAXONOMIES:
            intent_true = true_type if label == "seven" else TO_FIVE[true_type]
            intent_conf = confused_type if label == "seven" else TO_FIVE[confused_type]
            truth_report = run_method(types, intent_true, text)
            conf_report = run_method(types, intent_conf, text)
            route = tag_router(types)
            row[label] = {
                "capability": {"truth": truth_report.card, "confused": conf_report.card,
                               "flip": truth_report.card != conf_report.card},
                "tag": {"truth": route(truth_report.primitives),
                        "confused": route(conf_report.primitives),
                        "flip": route(truth_report.primitives) != route(conf_report.primitives)},
            }
        rows.append(row)

    summary = {}
    for router in ("capability", "tag"):
        for label, _types in TAXONOMIES:
            cells = [r[label][router] for r in rows]
            summary.setdefault(router, {})[label] = {
                "flips": sum(c["flip"] for c in cells),
                "truth_hits": sum(c["truth"] == EXPECTED[r["true_type"]]
                                  for c, r in zip(cells, rows)),
            }
    summary["cases"] = len(rows)
    print(json.dumps({"summary": summary, "rows": rows}, ensure_ascii=False, indent=2))
    return summary


if __name__ == "__main__":
    main()
