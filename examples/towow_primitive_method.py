"""Towow discovery through the five-primitive lens.

Reuses the typing pipeline from ``primitive_ir_method`` to compile every
participant's profile in the published ten-person scenario
(``jpp/data/towow-people.json``) into primitives, then asks the discovery
question — who offers *personally* a concrete support that answers the signal?

The interesting case is 舟 (zhou): his profile does offer the companion-drive
service (an ``Object``), but availability is unconfirmed, so readiness stays an
unresolved observation — exactly the Towow rule that "to be confirmed" is not a
confirmation.  The scenario ships a published ``update`` for zhou; we resolve
the open observation through ``refine`` instead of recompiling from scratch.

Type, readiness and direct-support judgments are synthetic fixture intents
(the fixture answers the declared intent; genuinely unconfirmed availability
stays unresolved); segmentation, entropy scoring and packaging are exact local
calculations.  No model client is called.
"""

from __future__ import annotations

import json
import re
import sys
from dataclasses import dataclass
from importlib.resources import files
from pathlib import Path

PACKAGE_ROOT = Path(__file__).resolve().parent
sys.path.insert(0, str(PACKAGE_ROOT))

from foundation import jv
from jev_compose import Request, batch_observe, component, execute, refine
from jev_compose.fixtures import runtime
from primitive_ir_method import (
    NoteSpan, build_typing_pipeline,
)

# Synthetic judgment intents per (person, span).  The pair members the
# original seven-primitive scheme splits (Action/Obligation, Policy, Resource)
# appear here already merged: services offered are Objects, participation
# intents are may-directives, boundary statements are standing rules.
FIXTURE_INTENTS = {
    "lin": ((0, "Occurrence"),),
    "mei": ((0, "Object"), (1, "Occurrence"), (2, "Rule")),
    "lan": ((0, "Occurrence"), (1, "Occurrence"), (2, "Directive")),
    "zhou": ((0, "Occurrence"), (1, "Object"), (2, "Occurrence")),
    "qiao": ((0, "Occurrence"), (1, "Rule"), (2, "Rule"), (3, "Rule")),
    "tang": ((0, "Occurrence"), (1, "Rule"), (2, "Occurrence")),
    "an": ((0, "Occurrence"), (1, "Directive"), (2, "Occurrence")),
    "he": ((0, "Occurrence"), (1, "Occurrence")),
    "yu": ((0, "Occurrence"),),
    "bo": ((0, "Occurrence"),),
}

# The discovery question mirrors towow.py's "direct" rule: only a support the
# person themselves offers counts; referrals, organising and pure willingness
# to participate do not.
DIRECT_SUPPORT = {"zhou": True, "mei": True}

DIRECT_QUESTION = ("该参与者的材料是否明确记载本人能提供的、回应 signal 处境的具体支持？"
                   "第三人的能力、单纯想参加、投资兴趣不算。")


@dataclass(frozen=True)
class PersonCard:
    person: str
    name: str
    spans: tuple[NoteSpan, ...]


@dataclass(frozen=True)
class CandidateReport:
    person: str
    name: str
    direct: bool
    spans: tuple[dict, ...]
    unresolved_before: int
    unresolved_after: int
    resolved_false: int
    ready: bool


@dataclass(frozen=True)
class DiscoveryReport:
    signal: str
    candidates: tuple[CandidateReport, ...]
    provenance: str = "synthetic_fixture_no_model_call"


def load_scenario():
    return json.loads(files("jpp").joinpath("data/towow-people.json").read_text(encoding="utf-8"))


def split_spans(text, intents):
    by_index = dict(intents)
    sentences = [part for part in re.split(r"[。；]", text) if part.strip()]
    spans = []
    for index, sentence in enumerate(sentences):
        spans.append(NoteSpan(index, sentence + "。", by_index.get(index)))
    return tuple(spans)


def population_cards() -> tuple[PersonCard, ...]:
    data = load_scenario()
    cards = []
    for person in data["people"]:
        intents = FIXTURE_INTENTS[person["id"]]
        cards.append(PersonCard(person["id"], person["name"], split_spans(person["context"], intents)))
    return tuple(cards)


def signal_spans() -> tuple[NoteSpan, ...]:
    intents = ((0, "Occurrence"), (1, "Occurrence"), (2, "Occurrence"))
    return split_spans(load_scenario()["signal"], intents)


@component("compile_population", tuple, tuple, effects=("judge",))
def compile_population(cards):
    """Compile every participant's profile into primitives."""
    typing = build_typing_pipeline()
    return tuple((card.person, card.name, typing(card.spans)) for card in cards)


@component("discover_and_package", tuple, DiscoveryReport, effects=("judge",))
def discover_and_package(compiled):
    """Judge direct support; resolve unconfirmed availability via the update."""
    data = load_scenario()
    update = data["update"]
    requests = []
    for person, name, primitives in compiled:
        on = jv.lit({"flag": DIRECT_SUPPORT.get(person, False)}, addr=f"direct/{person}")
        question = jv.test(DIRECT_QUESTION, calib=jv.calib("demo.flag"))
        requests.append(Request(jv.state(on=on), question, f"{person}/direct"))
    direct_observations = batch_observe(requests)

    candidates = []
    for (person, name, primitives), obs in zip(compiled, direct_observations):
        unresolved = [p for p in primitives if not p.readiness.resolved]
        before = len(unresolved)
        if person == update["person"] and unresolved:
            replacements = {}
            for primitive in unresolved:
                index = primitive.span.index
                label = f"{person}/readiness/{index}/updated"
                on = jv.lit({"flag": True}, addr=f"readiness/{person}/{index}/updated")
                question = jv.test(f"{person} span {index} 的可用性是否已被明确确认？",
                                   calib=jv.calib("demo.flag"))
                request = Request(jv.state(on=on), question, label)
                replacements[primitive.readiness.id] = request
            updated = refine(tuple(p.readiness for p in primitives), replacements)
            resolved = dict(zip((p.readiness.id for p in primitives), updated))
            primitives = tuple(
                type(primitive)(primitive.span, primitive.type, primitive.entropy,
                                primitive.confidence, primitive.flags, primitive.force,
                                primitive.trigger, primitive.type_observation,
                                resolved[primitive.readiness.id])
                for primitive in primitives
            )
        after = sum(not p.readiness.resolved for p in primitives)
        resolved_false = sum(p.readiness.resolved and p.readiness.value is False
                             for p in primitives)
        is_direct = bool(obs.resolved and obs.value)
        candidates.append(CandidateReport(
            person=person, name=name, direct=is_direct,
            spans=tuple({"type": p.type, "entropy": p.entropy, "flags": p.flags}
                        for p in primitives),
            unresolved_before=before, unresolved_after=after,
            resolved_false=resolved_false,
            ready=is_direct and after == 0 and resolved_false == 0,
        ))
    return DiscoveryReport(signal=data["signal"], candidates=tuple(candidates))


def build_towow_method():
    return compile_population.then(discover_and_package, name="compile_and_discover")


def report_dict(report: DiscoveryReport) -> dict:
    return {
        "signal": report.signal,
        "candidates": [candidate.__dict__ for candidate in report.candidates],
        "ready": [c.person for c in report.candidates if c.ready],
    }


def main():
    run = execute(build_towow_method(), population_cards(), runtime())
    print(json.dumps(report_dict(run.value), ensure_ascii=False, indent=2))
    signal = execute(build_typing_pipeline(), signal_spans(), runtime()).value
    print(json.dumps({"signal_types": [p.type for p in signal],
                      "signal_mean_entropy": round(sum(p.entropy for p in signal) / len(signal), 3)},
                     ensure_ascii=False, indent=2))


if __name__ == "__main__":
    main()
