"""Deterministic mechanism tests, deliberately not model quality evaluation."""
from copy import deepcopy
import json

from jpp.towow import QUESTIONS, load_scenario, run_once


class ScriptedClient:
    model_id = "jev-1.13.0"

    def __init__(self, *, no_relays=False, ambiguous=False):
        self.no_relays, self.ambiguous = no_relays, ambiguous
        self.states = []

    def ask(self, state, questions):
        state = json.loads(state) if isinstance(state, str) else state
        self.states.append(deepcopy(state))
        person = state["ctx"][0]["person"]
        result = {}
        for key, q in questions.items():
            kind = next(k for k, v in QUESTIONS.items() if v in q["instructions"])
            yes = False
            if kind == "direct":
                yes = person in ("mei", "zhou")
            elif kind == "relay":
                yes = person == "lan" and not self.no_relays
            elif kind == "ready":
                yes = "现在确认" in state["ctx"][0]["local_context"]
            elif kind == "extend":
                yes = person in ("qiao", "tang", "an", "he")
            elif kind == "activate":
                yes = person in ("qiao", "tang", "an", "he")
            result[key] = {"type": "noul", "noul": 0.5 if self.ambiguous else (0.99 if yes else 0.01)}
        return result, 0, 0.0


def test_relay_candidate_update_and_cache(tmp_path):
    scenario, client = load_scenario(), ScriptedClient()
    first = run_once(scenario, client, tmp_path)
    assert {r["person"] for r in first["direct"]} == {"mei", "zhou"}
    assert first["candidates"][0]["members"] == ["lin", "lan", "zhou"]
    assert {e["person"] for e in first["extensions"]} == {"qiao", "tang", "an", "he"}
    assert not any(e["discuss_now"] for e in first["extensions"])
    assert all(o["provisional"] for o in first["observations"])
    assert first["stats"]["calls"] == 16
    assert first["stats"]["questions"] == 32
    assert all(len(s["ctx"]) == 1 and set(s["ctx"][0]) == {"person", "local_context"} for s in client.states)

    repeat = run_once(scenario, client, tmp_path)
    assert repeat["stats"]["calls"] == 0
    assert repeat["stats"]["ledger_hits"] == 32
    assert repeat["extensions"] == first["extensions"]

    changed = deepcopy(scenario)
    next(p for p in changed["people"] if p["id"] == "zhou")["context"] = changed["update"]["context"]
    updated = run_once(changed, client, tmp_path)
    assert updated["stats"]["calls"] == 8
    assert updated["stats"]["ledger_hits"] == 16
    assert all(e["discuss_now"] for e in updated["extensions"])


def test_no_relay_means_no_hidden_member_or_combination(tmp_path):
    client = ScriptedClient(no_relays=True)
    result = run_once(load_scenario(), client, tmp_path)
    assert not result["relays"] and not result["candidates"] and not result["extensions"]
    assert all(s["ctx"][0]["person"] != "zhou" for s in client.states)


def test_ambiguity_is_preserved_not_a_positive_match(tmp_path):
    result = run_once(load_scenario(), ScriptedClient(ambiguous=True), tmp_path)
    assert not result["direct"] and not result["relays"]
    assert all(o["value"] is None for o in result["observations"])


def test_removing_contact_removes_route(tmp_path):
    scenario = load_scenario()
    next(p for p in scenario["people"] if p["id"] == "lan")["contacts"] = []
    result = run_once(scenario, ScriptedClient(), tmp_path)
    assert not result["candidates"]


def test_uncertain_extension_remains_visible_but_cannot_activate(tmp_path):
    class UncertainExtension(ScriptedClient):
        def ask(self, state, questions):
            answers, tokens, cost = super().ask(state, questions)
            for key, question in questions.items():
                if QUESTIONS["extend"] in question["instructions"]:
                    answers[key]["noul"] = 0.5
            return answers, tokens, cost
    result = run_once(load_scenario(), UncertainExtension(), tmp_path)
    assert result["extensions"]
    assert all(e["relation"] is None and e["discuss_now"] is not True for e in result["extensions"])


def test_public_recording_runs_same_method_without_network(tmp_path, monkeypatch):
    from jpp.towow import run_demo
    from foundation import jv
    def forbidden(*a, **kw):
        raise AssertionError("offline example tried to create a live client")
    monkeypatch.setattr(jv, "JevClient", forbidden)
    report = run_demo(out=tmp_path)
    assert report["warm"]["stats"]["calls"] == 0
    assert report["total_estimated_cost_usd"] == 0
    assert {x["person"] for x in report["cold"]["direct"]} == {"mei", "zhou"}
    assert {x["person"] for x in report["updated"]["extensions"] if x["discuss_now"] is True} == {"tang", "an", "he"}
    assert (tmp_path / "index.html").is_file()
