import json

from jpp.towow_population import load_population, run_population, lexical_ranking, rank_candidates


def test_complete_pools_and_model_request_boundary(tmp_path):
    data = load_population()
    assert len(data["people"]) == 216
    assert len({p["id"] for p in data["people"]}) == 216
    assert len(data["intents"]) == 20
    assert sum(len(p["source_ids"]) for p in data["people"]) == 225
    class Probe:
        model_id = "jev-1.13.0"
        def __init__(self):
            self.requests = []
        def ask(self, state, questions):
            self.requests.append((state, questions))
            assert set(state) == {"on", "ctx"}
            assert set(state["ctx"][0]) == {"profile"}
            assert all(set(q) == {"intent", "context"} for q in state["on"]["intents"].values())
            return {k: {"type": "noul", "noul": .5} for k in questions}, 0, 0
    client = Probe()
    report = run_population(client, tmp_path)
    assert len(client.requests) == 216
    assert all(len(q) == 20 for _, q in client.requests)
    assert report["stats"]["questions"] == 4320
    assert all(a["value"] is None for row in report["rows"] for a in row["answers"])
    repeat = run_population(client, tmp_path)
    assert repeat["stats"]["calls"] == 0
    assert repeat["stats"]["ledger_hits"] == 4320


def test_bm25_uses_query_and_context_with_stable_ties():
    people = [{"id":"b","context":"陶艺 手工"},{"id":"a","context":"Rust Kubernetes 系统"},{"id":"c","context":"Rust Kubernetes 系统"}]
    assert lexical_ranking(people, {"query":"部署","context":"Kubernetes"}) == ["a", "c", "b"]


def test_candidate_grouping_retains_unknown_and_ignores_evaluation_labels():
    payload = {"population": {"people": [{"id":"a","context":"Kubernetes Kubernetes"},
                 {"id":"b","context":"部署系统"},{"id":"c","context":"容器服务"}],
                 "intents": [{"id":"q","query":"Kubernetes","expected_hits":["a"]}]},
               "judgments": {"q": {"answers": [{"person":"a","value":False},
                 {"person":"b","value":True},{"person":"c","value":None}]}}}
    ranked = rank_candidates(payload)["rankings"]["q"]
    assert ranked["bm25"][0] == "a"
    assert ranked["combined"] == ["b", "c", "a"]
    payload["population"]["intents"][0]["expected_hits"] = ["c"]
    assert rank_candidates(payload)["rankings"]["q"] == ranked


def test_published_answers_reproduce_all_three_rankings_without_network(tmp_path, monkeypatch):
    from foundation import jv
    from importlib.resources import files
    from jpp.towow import RecordedClient
    def forbidden(*args, **kwargs):
        raise AssertionError("Population replay tried to create a live client")
    monkeypatch.setattr(jv, "JevClient", forbidden)
    saved = json.loads(files("jpp").joinpath("data/towow-population-recording.json").read_text())
    report = run_population(RecordedClient(recording=saved), tmp_path)
    assert report["summary"]["jpp_hits"] == 56
    assert report["summary"]["semantic_only_hits"] == 33
    assert report["summary"]["bm25_hits"] == 43
    assert report["stats"]["cost"] == 0
