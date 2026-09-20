"""Question values and partial answers through the public jv observation boundary."""
from __future__ import annotations
from dataclasses import dataclass, field
import hashlib
import json
from typing import Any

from foundation import jv
from .core import component


def question_to_dict(q) -> dict:
    return {"op": q.op, "text": q.text, "calib": q.calib.key, "scale": list(q.scale),
            "anchors": q.anchors, "evidence": list(q.evidence), "prior": q.prior,
            "phys": q.phys, "agg": q.agg}


def question_from_dict(d: dict):
    args = {"calib": jv.calib(d["calib"]), "evidence": tuple(d.get("evidence", ()))}
    if d["op"] == "test":
        return jv.test(d["text"], agg=d.get("agg", "exists"), **args)
    if d["op"] == "select":
        return jv.select(d["text"], prior=d.get("prior", "none"), phys=d.get("phys"), **args)
    if d["op"] == "measure":
        return jv.measure(d["text"], scale=d["scale"], anchors=d.get("anchors"), **args)
    raise ValueError(f"Unknown question kind: {d['op']}")


@dataclass(frozen=True)
class Request:
    state: Any
    question: Any
    label: str = ""
    tag: Any = field(default=None, compare=False, repr=False)

    def fingerprint(self, model_id: str) -> str:
        state = self.state.resolved()
        payload = {"model": model_id, "state": state.structure_hash,
                   "materials": [m.hash for m in state.all_mats],
                   "question": question_to_dict(self.question)}
        return hashlib.sha256(json.dumps(payload, sort_keys=True, ensure_ascii=False).encode()).hexdigest()


@dataclass(frozen=True)
class Observation:
    """A captured outcome; ``id`` identifies the request, not an execution attempt.

    The same request can produce a known outcome in one scope and an unresolved
    outcome in another (for example, with no budget). Each remains its own value.
    """
    id: str
    request: Request = field(repr=False, compare=False)
    resolved: bool
    value: Any
    cause: str | None
    decision: Any = field(repr=False, compare=False)
    model_id: str = ""

    def to_dict(self) -> dict:
        return {"id": self.id, "label": self.request.label,
                "question": question_to_dict(self.request.question),
                "resolved": self.resolved, "value": self.value, "cause": self.cause,
                "model_id": self.model_id, "exit": self.decision.kind,
                "uncertainty_policy": "retain" if not self.resolved else None,
                "provisional": self.decision.provisional,
                "material_hashes": [m.hash for m in self.request.state.resolved().all_mats]}


def _capture(request, decision) -> Observation:
    # observe's explicit policy is to retain an Unsure as a first-class partial
    # result (including the original exit), never turn it into a guessed value.
    # `match` is the public responsibility-transfer boundary in the current jv.
    match decision:
        case jv.Unsure(c):
            known, value, cause = False, None, c
        case jv.Act():
            known, value, cause = True, True, None
        case jv.Ignore():
            known, value, cause = True, False, None
        case jv.Pick(k):
            known, value, cause = True, k, None
        case jv.At(level):
            known, value, cause = True, level, None
        case _:
            raise TypeError(f"Unknown jv exit: {type(decision).__name__}")
    model_id = jv.current().model_id
    return Observation(request.fingerprint(model_id), request, known, value, cause, decision, model_id)


@component("observe", Request, Observation, effects=("judge",))
def observe(request):
    readings = jv.judge(request.state, request.question)
    decision = jv.cut(readings[0])
    return _capture(request, decision)


@component("batch_observe", list, list, effects=("judge",))
def batch_observe(requests):
    # Enqueue before any cut: jv owns fusion, cost, physical lowering and replay.
    queued = [jv.judge(r.state, r.question) for r in requests]
    return [_capture(r, jv.cut(handle[0])) for r, handle in zip(requests, queued)]


@dataclass(frozen=True)
class CountAnswer:
    value: bool | None
    lower: int
    upper: int
    unresolved: tuple[Observation, ...]


def at_least(observations, count: int) -> CountAnswer:
    """Exact bounds conditional on accepted boolean exits; no independence assumption."""
    items = tuple(observations)
    if any(o.request.question.op != "test" for o in items):
        raise TypeError("at_least accepts test observations only")
    lower = sum(o.resolved and o.value is True for o in items)
    unknown = tuple(o for o in items if not o.resolved)
    upper = lower + len(unknown)
    answer = True if lower >= count else (False if upper < count else None)
    return CountAnswer(answer, lower, upper, unknown)


def refine(observations, replacements: dict[str, Request]) -> tuple[Observation, ...]:
    """Re-observe unresolved occurrences selected by their request IDs.

    Keep every known observation unchanged, even if it has the same request ID
    as an unresolved occurrence. Unresolved occurrences sharing a selected ID
    receive one shared replacement. Evidence/question changes alter request IDs;
    IDs do not distinguish separate execution attempts of the same request.
    """
    items = tuple(observations)
    unresolved = {o.id for o in items if not o.resolved}
    extra = set(replacements) - unresolved
    if extra:
        raise ValueError("refine accepts replacements for existing unresolved identities only")
    ids = list(replacements)
    values = batch_observe([replacements[i] for i in ids])
    updated = dict(zip(ids, values))
    return tuple(o if o.resolved else updated.get(o.id, o) for o in items)
