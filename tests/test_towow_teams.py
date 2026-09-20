from copy import deepcopy
import json

from jpp.towow import CallBudget, fingerprint
from jpp.towow_real import JournalClient
from jpp.towow_teams import (
    DEFAULT_LEVELS,
    example_q16_plan,
    example_q18_plan,
    nomination_as_input,
    run_discovery,
)


class FakeClient:
    model_id = "fake-offline"
    live = False

    class Budget:
        spent = 0

    budget = Budget()

    def __init__(self):
        self.requests = []

    def ask(self, state, questions):
        self.requests.append((state, questions))
        answer = {"type": "score", "score": len(DEFAULT_LEVELS) - 1,
                  "probabilities": {str(i): 0.97 if i == len(DEFAULT_LEVELS) - 1 else 0.01
                                    for i in range(len(DEFAULT_LEVELS))}}
        return {key: deepcopy(answer) for key in questions}, 0, 0


def _plan(intent, facets, max_combinations=1):
    return {"intent": {"query": intent}, "facets": facets,
            "max_candidates": 1,
            "combination": {"facets": [f["id"] for f in facets],
                            "question": "Do the source-backed contributions form a useful joint proposal?"},
            "max_combinations": max_combinations}


def _facet(key, label, query):
    return {"id": key, "label": label, "search_text": query,
            "question": f"Does this node have concrete evidence for {label}?"}


def test_two_authored_plans_run_as_independent_applications(tmp_path):
    nodes = [
        {"id": "audio-node", "context": "sound audio recording", "source_ids": ["audio-src"]},
        {"id": "visual-node", "context": "shader graphics visual", "source_ids": ["visual-src"]},
        {"id": "craft-node", "context": "wood carving craft", "source_ids": ["craft-src"]},
        {"id": "digital-node", "context": "digital electronics Arduino", "source_ids": ["digital-src"]},
    ]
    plans = [example_q16_plan(), example_q18_plan()]
    assert plans[0]["intent"]["query"] != plans[1]["intent"]["query"]
    for index, plan in enumerate(plans):
        # The app accepts the authored plan as data; no expected-hit field is read.
        facet_ids = [facet["id"] for facet in plan["facets"]]
        mapping = {facet_ids[0]: "audio-node" if index == 0 else "craft-node",
                   facet_ids[1]: "visual-node" if index == 0 else "digital-node"}

        def router(pool, intent, facet):
            return [mapping[facet["id"]]]

        report = run_discovery(plan, FakeClient(), tmp_path / f"root-{index}",
                               people=nodes, router=router)
        assert report["stats"]["questions"] == 3
        assert len(report["proposals"]) == 1
        assert len(report["proposals"][0]["members"]) == 2
        assert report["input"]["intent"]["query"] == plan["intent"]["query"]
        assert all(member["node"]["context"] for member in report["proposals"][0]["members"])


def test_n_party_proposal_is_a_composable_seed_with_context_and_readings(tmp_path):
    facets = [_facet("a", "contribution A", "alpha"),
              _facet("b", "contribution B", "beta")]
    plan = _plan("A goal needing three kinds of contribution", facets)
    nodes = [
        {"id": "member-a", "context": "alpha concrete capability", "source_ids": ["src-a"]},
        {"id": "member-b", "context": "beta concrete capability", "source_ids": ["src-b"]},
        {"id": "member-security", "context": "security audit capability", "source_ids": ["src-security"]},
    ]
    choices = {"a": "member-a", "b": "member-b"}

    def router(pool, intent, facet):
        return [choices[facet["id"]]]

    first = run_discovery(plan, FakeClient(), tmp_path / "first", people=nodes, router=router)
    proposal = first["proposals"][0]
    assert len(proposal["members"]) == 2
    assert proposal["context"].count("concrete capability") == 2
    assert proposal["trigger_reasons"]
    seed = nomination_as_input(proposal)
    assert nomination_as_input(seed) == seed
    assert seed["parent_ids"] == ["member-a", "member-b"]
    assert {row["status"] for row in seed["prior_readings"]} == {"measured"}
    assert all(row["provisional"] for row in seed["prior_readings"])
    assert all(src in seed["context"] for src in ("alpha", "beta"))

    followup = {"intent": {"query": "A changed security follow-up goal"},
                "facets": [_facet("security", "security resource", "security audit")],
                "max_candidates": 1,
                "combination": {"facets": ["security"],
                                "question": "Does the prior group plus this resource support the new goal?"},
                "max_combinations": 1,
                "missing_conditions": ["security resource availability is unknown"],
                "next_question": "Can the security resource review the proposed system?"}

    def followup_router(pool, intent, facet):
        return ["member-security"]

    second = run_discovery(followup, FakeClient(), tmp_path / "second", people=nodes,
                           seed=proposal, router=followup_router)
    assert second["input"]["seed"]["id"] == proposal["id"]
    assert len(second["proposals"][0]["members"]) == 3
    assert [member["node"]["id"] for member in second["proposals"][0]["members"]] == [
        "member-a", "member-b", "member-security"]
    assert all(member["node"]["context"] for member in second["proposals"][0]["members"])
    assert second["proposals"][0]["missing_conditions"] == followup["missing_conditions"]
    assert second["proposals"][0]["next_question"] == followup["next_question"]
    assert second["proposals"][0]["parent_nomination_ids"] == [proposal["id"]]
    assert second["input"]["seed"]["intent"] == plan["intent"]


def test_journal_ledger_reuses_identical_inputs_and_invalidates_changed_profile(tmp_path):
    plan = _plan("alpha beta shared goal", [
        _facet("alpha", "alpha role", "alpha"), _facet("beta", "beta role", "beta")])
    nodes = [
        {"id": "a", "context": "alpha base profile", "source_ids": ["source-a"]},
        {"id": "b", "context": "beta base profile", "source_ids": ["source-b"]},
        {"id": "c", "context": "other profile", "source_ids": ["source-c"]},
    ]
    def router(pool, intent, facet):
        return ["a" if facet["id"] == "alpha" else "b"]

    client = FakeClient()
    root = tmp_path / "ledger"
    first = run_discovery(plan, client, root, people=nodes, router=router)
    json.dumps(first, ensure_ascii=False)
    count_after_first = len(client.requests)
    warm = run_discovery(plan, client, root, people=nodes, router=router)
    assert len(client.requests) == count_after_first
    assert warm["stats"]["calls"] == 0
    assert warm["stats"]["ledger_hits"] >= 3

    changed = deepcopy(nodes)
    changed[0]["context"] += " changed source evidence"
    revised = run_discovery(plan, client, root, people=changed, router=router)
    assert len(client.requests) == count_after_first + 2  # alpha nomination + dependent combination
    assert revised["stats"]["ledger_hits"] >= 1
    assert revised["proposals"][0]["members"][0]["node"]["context"].endswith("changed source evidence")


def test_existing_journal_client_replays_exact_requests_without_network(tmp_path):
    plan = _plan("audio visual goal", [
        _facet("audio", "audio role", "audio"), _facet("visual", "visual role", "visual")])
    nodes = [
        {"id": "a", "context": "audio profile", "source_ids": ["src-a"]},
        {"id": "b", "context": "visual profile", "source_ids": ["src-b"]},
    ]
    def router(pool, intent, facet):
        return ["a" if facet["id"] == "audio" else "b"]

    live_shape = FakeClient()
    expected = run_discovery(plan, live_shape, tmp_path / "shape", people=nodes, router=router)
    answer = {"type": "score", "score": len(DEFAULT_LEVELS) - 1,
              "probabilities": {str(i): 0.97 if i == len(DEFAULT_LEVELS) - 1 else 0.01
                                for i in range(len(DEFAULT_LEVELS))}}
    journal_path = tmp_path / "recording.jsonl"
    with journal_path.open("w", encoding="utf-8") as stream:
        for state, questions in live_shape.requests:
            record = {"key": fingerprint(state, questions), "state": state,
                      "questions": questions,
                      "answers": {key: deepcopy(answer) for key in questions},
                      "input_tokens": 0, "estimated_cost_usd": 0.012, "elapsed_ms": 1}
            stream.write(json.dumps(record, ensure_ascii=False) + "\n")
    client = JournalClient(journal_path, live=False, budget=CallBudget(limit=1.0))
    replayed = run_discovery(plan, client, tmp_path / "replay", people=nodes, router=router)
    assert replayed["proposals"][0]["members"] == expected["proposals"][0]["members"]
    assert replayed["stats"]["new_estimated_cost_usd"] == 0
    assert replayed["stats"]["questions"] == expected["stats"]["questions"]
