"""A five-primitive IR method: refine a high-entropy note into judged structures.

- **Five primitives, three axes.**  Participant (who acts), Object (what is
  consumed or delivered), Occurrence (what happened), Directive (what to do),
  Rule (what triggers what).  Pair-wise differences that look like new
  types elsewhere (may vs must obligation, standing vs conditional rule) are
  encoded as fields on the primitive (``Directive.force``, ``Rule.trigger``)
  so widening a type's expressiveness costs nothing.
- **No Ledger primitive.**  Outcome records are runtime evidence (the
  observation chain every primitive already carries), not a class of meaning.
- **Thresholds are a dial, not a constant.**  Each primitive type gets its own
  entropy threshold — acting and triggering demand more precision than
  observing — and ``build_note_compiler`` accepts overrides, the way a budget
  is passed to a run.
- **Routing follows capability, not type tags.**  A structure routes to the
  schedule card when it contains an executable directive (``force=must`` with
  a time anchor), to the monitor card when it contains a conditional rule,
  and to the reflection card otherwise, where every open clarification is
  listed.

Pipeline:

    segment -> judge primitive types -> score entropy / parse fields
            -> assess readiness against per-type thresholds
            -> bind: route the structure into a card packager

The scenario is a finance/diplomacy analyst note.  Type and readiness
judgments are synthetic fixture observations (each span declares the intent
the fixture answers with; genuinely missing quantities stay unresolved);
entropy scoring, field parsing, clarifications and routing are exact local
calculations.  No model client is called and no real calibration is used.
"""

from __future__ import annotations

import hashlib
import json
import re
import sys
from dataclasses import dataclass
from pathlib import Path

PACKAGE_ROOT = Path(__file__).resolve().parent
sys.path.insert(0, str(PACKAGE_ROOT))

from foundation import jv
from jev_compose import (
    Component, CountAnswer, Observation, Request, at_least, batch_observe,
    component, execute,
)
from jev_compose.fixtures import runtime

PRIMITIVE_TYPES = ("Participant", "Object", "Occurrence", "Directive", "Rule")

# The human/machine split dial: execution consequences set the precision bar.
DEFAULT_THRESHOLDS = {
    "Participant": 0.5,   # misidentifying a participant is noise-level
    "Object": 0.3,        # quantities feed exact checks
    "Occurrence": 0.5,    # a misrecorded event is recoverable
    "Directive": 0.25,    # about to act: demand precision
    "Rule": 0.2,          # a trigger fires real consequences
}

_WEEKDAYS = r"(?:mon|tues|wednes|thurs|fri|satur|sun)day"
_MONTHS = (r"jan(?:uary)?|feb(?:ruary)?|mar(?:ch)?|apr(?:il)?|may|jun(?:e)?|"
           r"jul(?:y)?|aug(?:ust)?|sep(?:t(?:ember)?)?|oct(?:ober)?|nov(?:ember)?|dec(?:ember)?")
# Relative time: explicit anchors, bare weekdays, and "by <time expression>"
# deadlines.  "by" alone is deliberately not a marker — "step by step",
# "by the central bank" must not raise entropy.
_RELATIVE_TIME = re.compile(
    rf"\b(after|before|next|this week)\b|\b{_WEEKDAYS}\b"
    rf"|\bby\s+(?:{_WEEKDAYS}|the\s+\d+(?:st|nd|rd|th)?|{_MONTHS}|end of)\b"
    r"|本周|下周|周六|周日|暂时|目前", re.IGNORECASE)
_MISSING_QUANTITY = re.compile(r"^\s*(trim|buy|sell|send|release)\b", re.IGNORECASE)
_VAGUE_NUMERAL = re.compile(r"\b(ninety|triple|double)\b", re.IGNORECASE)
_IMPERATIVE = re.compile(r"^\s*(trim|buy|sell|send|release|deliver|pay|book)\b", re.IGNORECASE)
_UNCONFIRMED = re.compile(r"未确认|还没有确认|待确认|尚待")


@dataclass(frozen=True)
class AnalystNote:
    """Raw high-entropy text plus per-span synthetic judgment intents."""

    text: str
    intents: tuple[tuple[int, str], ...]
    provenance: str = "synthetic_fixture_no_model_call"


@dataclass(frozen=True)
class NoteSpan:
    index: int
    text: str
    intent: str | None


@dataclass(frozen=True)
class JudgedSpan:
    span: NoteSpan
    type: str | None
    type_observation: Observation


@dataclass(frozen=True)
class Primitive:
    span: NoteSpan
    type: str | None
    entropy: float
    confidence: float
    flags: tuple[str, ...]
    force: str | None          # Directive only: may | must
    trigger: str | None        # Rule only: standing | conditional
    type_observation: Observation
    readiness: Observation | None = None


@dataclass(frozen=True)
class NoteReport:
    card: str
    focus: tuple[str, ...]
    primitives: tuple[Primitive, ...]
    clarifications: tuple[str, ...]
    ready: CountAnswer
    overall_entropy: float
    overall_confidence: float
    trace_id: str


@component("segment_note", AnalystNote, tuple)
def segment_note(note):
    """Split the note into spans and attach each span's synthetic intent."""
    intents = dict(note.intents)
    spans = []
    for index, sentence in enumerate(note.text.split(". ")):
        text = sentence if sentence.endswith(".") else sentence + "."
        spans.append(NoteSpan(index, text, intents.get(index)))
    return tuple(spans)


def build_type_judge(primitive_types=PRIMITIVE_TYPES):
    @component("judge_primitive_types", tuple, tuple, effects=("judge",))
    def judge_primitive_types(spans):
        """Ask, per span, which of the primitives its meaning belongs to."""
        requests = []
        for span in spans:
            index = span.index
            intent = span.intent
            text = span.text
            on = jv.lit({"index": index, "text": text, "intent": intent}, addr=f"span/{index}")
            over = []
            for primitive_type in primitive_types:
                priority = 0 if primitive_type == intent else 1
                candidate_addr = f"type/{index}/{primitive_type}"
                over.append(jv.lit({"type": primitive_type, "priority": priority}, addr=candidate_addr))
            question = jv.select(f"span {index} 属于哪种原语类型？", calib=jv.calib("demo.pick"))
            requests.append(Request(jv.state(on=on, over=over), question, f"span/{index}/type"))
        observations = batch_observe(requests)
        return tuple(
            JudgedSpan(span, primitive_types[o.value] if o.resolved else None, o)
            for span, o in zip(spans, observations)
        )
    return judge_primitive_types


judge_primitive_types = build_type_judge()


def scoring_flags(text):
    flags = []
    if _RELATIVE_TIME.search(text):
        flags.append("relative_time")
    if _MISSING_QUANTITY.match(text) and not re.search(r"\d", text):
        flags.append("missing_quantity")
    if _VAGUE_NUMERAL.search(text):
        flags.append("vague_numeral")
    if _UNCONFIRMED.search(text):
        flags.append("unconfirmed")
    return tuple(flags)


def force_of(text):
    lowered = text.lower()
    if _IMPERATIVE.match(text) or "must" in lowered or "shall" in lowered:
        return "must"
    return "may"


def trigger_of(text):
    lowered = text.lower()
    if " if " in f" {lowered} " or "若" in text or "如果" in text:
        return "conditional"
    return "standing"


@component("score_primitives", tuple, tuple)
def score_primitives(judged):
    """Exact entropy/confidence from field completeness; parse force/trigger."""
    scored = []
    for item in judged:
        if item.type is None:
            scored.append(Primitive(item.span, None, 1.0, 0.0, ("unresolved_type",),
                                    None, None, item.type_observation))
            continue
        flags = scoring_flags(item.span.text)
        entropy = 0.05 + 0.15 * ("relative_time" in flags) \
            + 0.25 * ("missing_quantity" in flags) + 0.10 * ("vague_numeral" in flags) \
            + 0.20 * ("unconfirmed" in flags)
        confidence = min(0.99, 0.98 - 0.5 * (entropy - 0.05))
        # Fields are intrinsic to the text, not to the judged type; parsing
        # them unconditionally is what makes capability routing type-independent.
        force = force_of(item.span.text)
        trigger = trigger_of(item.span.text)
        scored.append(Primitive(item.span, item.type, round(entropy, 2),
                                round(confidence, 2), flags, force, trigger,
                                item.type_observation))
    return tuple(scored)


def clarification_for(primitive):
    reasons = []
    if "unresolved_type" in primitive.flags:
        reasons.append("type unresolved; no primitive could be judged for this span")
    if "missing_quantity" in primitive.flags:
        reasons.append("missing quantity; state an exact amount instead of guessing")
    if "relative_time" in primitive.flags:
        reasons.append("relative time; anchor it to a concrete date")
    if "vague_numeral" in primitive.flags:
        reasons.append("vague numeral; pin down the exact figure")
    if "unconfirmed" in primitive.flags:
        reasons.append("unconfirmed availability; wait for an explicit confirmation")
    quoted = primitive.span.text if len(primitive.span.text) <= 60 else primitive.span.text[:57] + "..."
    return f"span {primitive.span.index}: '{quoted}' — " + "; ".join(reasons)


def clarifications_for(primitives, thresholds):
    """Everything past its precision bar, plus spans whose type is unknown."""
    return tuple(
        clarification_for(p) for p in primitives
        if p.type is None or p.entropy > thresholds.get(p.type, 0.4)
    )


def schedule_items(primitives):
    """Spans whose text carries an executable directive with a time anchor."""
    return tuple(
        f"schedule: {p.span.text}"
        for p in primitives
        if p.force == "must" and "relative_time" in p.flags
    )


def monitor_items(primitives):
    """Spans whose text states a conditional rule to watch."""
    return tuple(
        f"monitor: {p.span.text}"
        for p in primitives
        if p.trigger == "conditional"
    )


def reflection_items(primitives, thresholds):
    return tuple(f"open: {c}" for c in clarifications_for(primitives, thresholds))


def choose_card(primitives) -> str:
    """Route by what the structure can support, not by dominant type."""
    if schedule_items(primitives):
        return "schedule"
    if monitor_items(primitives):
        return "monitor"
    return "reflection"


def build_assess_readiness(thresholds):
    @component("assess_readiness", tuple, tuple, effects=("judge",))
    def assess_readiness(primitives):
        """Resolve readiness where text suffices; retain the rest unresolved."""
        requests = []
        for primitive in primitives:
            index = primitive.span.index
            label = f"span/{index}/readiness"
            question = jv.test(
                f"span {index} 的信息缺口能否在不新增假设的情况下补齐？",
                calib=jv.calib("demo.flag"),
            )
            missing = "missing_quantity" in primitive.flags \
                or "unconfirmed" in primitive.flags or primitive.type is None
            if missing:
                on = jv.lit({"unknown": True}, addr=f"readiness/{index}")
            else:
                ready = primitive.entropy <= thresholds.get(primitive.type, 0.4)
                on = jv.lit({"flag": ready}, addr=f"readiness/{index}")
            requests.append(Request(jv.state(on=on), question, label))
        observations = batch_observe(requests)
        return tuple(
            Primitive(p.span, p.type, p.entropy, p.confidence, p.flags, p.force,
                      p.trigger, p.type_observation, o)
            for p, o in zip(primitives, observations)
        )
    return assess_readiness


def trace_id_for(primitives):
    joined = ".".join(p.span.text for p in primitives)
    return "NOTE-" + hashlib.sha1(joined.encode()).hexdigest()[:8]


def check_postconditions(report: NoteReport, thresholds) -> None:
    """Clarify-don't-guess, checked against the exact scores and thresholds."""
    assert report.primitives, "at least one primitive"
    assert report.overall_entropy <= 0.6, "overall entropy within executable range"
    for primitive in report.primitives:
        limit = thresholds.get(primitive.type, 0.4)
        if primitive.entropy > limit:
            assert any(c.startswith(f"span {primitive.span.index}:")
                       for c in report.clarifications), \
                f"span {primitive.span.index} exceeds its threshold without a clarification"


def build_typing_pipeline(thresholds: dict | None = None,
                          primitive_types=None) -> Component:
    """judge -> score -> assess over NoteSpan tuples, without card routing.

    Reusable for corpora that segment their own text (e.g. the Towow example)
    or discriminate with a different primitive set (e.g. the seven-type
    comparison experiment).
    """
    thresholds = {**DEFAULT_THRESHOLDS, **(thresholds or {})}
    judge = build_type_judge(primitive_types) if primitive_types else judge_primitive_types
    return judge.then(score_primitives, name="score_and_parse") \
        .then(build_assess_readiness(thresholds), name="attach_readiness")


def build_note_compiler(thresholds: dict | None = None,
                        primitive_types=None) -> Component:
    """The whole pipeline; thresholds are the dial passed in at construction."""
    thresholds = {**DEFAULT_THRESHOLDS, **(thresholds or {})}

    def package_report(card, primitives):
        focus = {"schedule": schedule_items, "monitor": monitor_items,
                 "reflection": lambda ps: reflection_items(ps, thresholds)}[card](primitives)
        ready = at_least(tuple(p.readiness for p in primitives), len(primitives))
        n = len(primitives)
        return NoteReport(
            card=card,
            focus=tuple(focus),
            primitives=primitives,
            clarifications=clarifications_for(primitives, thresholds),
            ready=ready,
            overall_entropy=round(sum(p.entropy for p in primitives) / n, 3),
            overall_confidence=round(sum(p.confidence for p in primitives) / n, 3),
            trace_id=trace_id_for(primitives),
        )

    @component("pack_schedule_card", tuple, NoteReport)
    def pack_schedule_card(primitives):
        return package_report("schedule", primitives)

    @component("pack_monitor_card", tuple, NoteReport)
    def pack_monitor_card(primitives):
        return package_report("monitor", primitives)

    @component("pack_reflection_card", tuple, NoteReport)
    def pack_reflection_card(primitives):
        return package_report("reflection", primitives)

    packagers = {"schedule": pack_schedule_card, "monitor": pack_monitor_card,
                 "reflection": pack_reflection_card}

    def choose_packager(primitives) -> Component:
        return packagers[choose_card(primitives)]

    assessed = segment_note.then(build_typing_pipeline(thresholds, primitive_types),
                                 name="type_score_assess")
    return assessed.bind(choose_packager, NoteReport, effects=frozenset(),
                         factory_effects=frozenset(), name="route_to_structure_card")


def sample_note() -> AnalystNote:
    text = (
        "Treasury yields climbed after the central bank signalled a slower easing path. "
        "The energy ministry said checkpoint fees at the strait will be reviewed next quarter. "
        "If Brent settles above ninety dollars for two straight weeks, "
        "the cabinet will consider releasing strategic reserves. "
        "Trim our airline exposure before the OPEC meeting on Thursday."
    )
    intents = ((0, "Occurrence"), (1, "Occurrence"), (2, "Rule"), (3, "Directive"))
    return AnalystNote(text, intents)


def diplomacy_note() -> AnalystNote:
    text = (
        "Border officials confirmed the transit corridor reopened after last month's drills. "
        "The delegation will arrive next week to review the checkpoint fee schedule."
    )
    return AnalystNote(text, ((0, "Occurrence"), (1, "Occurrence")))


def policy_note() -> AnalystNote:
    text = (
        "If tanker insurance premiums stay above two percent for a month, "
        "the consortium will cap spot bookings."
    )
    return AnalystNote(text, ((0, "Rule"),))


def observation_dict(observation):
    if observation is None:
        return None
    return {"label": observation.request.label, "resolved": observation.resolved,
            "value": observation.value, "cause": observation.cause,
            "exit": observation.decision.kind}


def primitive_dict(primitive):
    traced = observation_dict(primitive.type_observation)
    if primitive.type is not None:
        traced["value"] = primitive.type  # select exit is an index; show the type name
    return {"id": f"PRM-{primitive.type or 'Unresolved'}-{primitive.span.index:03d}",
            "span": primitive.span.index, "text": primitive.span.text,
            "type": primitive.type, "force": primitive.force, "trigger": primitive.trigger,
            "entropy": primitive.entropy, "confidence": primitive.confidence,
            "flags": list(primitive.flags),
            "trace": [traced],
            "readiness": observation_dict(primitive.readiness)}


def report_dict(report: NoteReport) -> dict:
    return {
        "card": report.card,
        "focus": list(report.focus),
        "primitives": [primitive_dict(p) for p in report.primitives],
        "clarifications": list(report.clarifications),
        "ready": {"value": report.ready.value, "lower": report.ready.lower,
                  "upper": report.ready.upper,
                  "unresolved": [observation_dict(o) for o in report.ready.unresolved]},
        "meta": {"overall_entropy": report.overall_entropy,
                 "overall_confidence": report.overall_confidence,
                 "trace_id": report.trace_id},
    }


def main():
    thresholds = dict(DEFAULT_THRESHOLDS)
    run = execute(build_note_compiler(thresholds), sample_note(), runtime())
    check_postconditions(run.value, thresholds)
    print(json.dumps(report_dict(run.value), ensure_ascii=False, indent=2))
    routing = [event for event in run.trace if "generated_structure" in event]
    print(json.dumps({"routing_events": [
        {"component": event["component"], "chosen_card": event["generated_description"]["name"]}
        for event in routing
    ], "thresholds": thresholds, "stats": run.stats}, ensure_ascii=False, indent=2))


if __name__ == "__main__":
    main()
