"""Multi-intent discovery across the existing Towow synthetic population.

Build all questions first: J++ fuses questions sharing one receiver's material.
Labels are only read by the evaluator, never by the model request construction.
"""
from collections import Counter
from datetime import datetime, timezone
from importlib.resources import files
import json
import math
from pathlib import Path
import re
from tempfile import TemporaryDirectory
from time import perf_counter

from foundation import jv
from jev_compose import component, execute
from .towow import CallBudget, RecordedClient


def load_population():
    return json.loads(files("jpp").joinpath("data/towow-population.json").read_text(encoding="utf-8"))


def tokens(text):
    latin = re.findall(r"[a-z0-9_+#.]+", text.lower())
    chinese = re.findall(r"[\u4e00-\u9fff]+", text)
    return latin + [part[i:i+2] for part in chinese for i in range(max(1, len(part)-1))]


def lexical_ranking(people, intent):
    """Documented BM25 reference: Chinese bigrams + Latin tokens, k1=1.5, b=.75."""
    docs = [Counter(tokens(p["context"])) for p in people]
    length = [sum(d.values()) for d in docs]
    average = sum(length) / max(1, len(docs))
    query = set(tokens(intent["query"] + " " + (intent.get("context") or "")))
    frequency = {term: sum(term in d for d in docs) for term in query}
    scores = []
    for person, doc, size in zip(people, docs, length):
        score = sum(math.log(1 + (len(docs)-frequency[t]+.5)/(frequency[t]+.5)) * doc[t]*2.5 /
                    (doc[t]+1.5*(.25+.75*size/average)) for t in query if doc[t])
        scores.append((person["id"], score))
    return [key for key, _ in sorted(scores, key=lambda pair: (-pair[1], pair[0]))]


@component("judge independent local contexts", dict, dict, effects=("judge",))
def judge_population(data):
    targets = {q["id"]: {"intent": q["query"], "context": q.get("context")} for q in data["intents"]}
    on = jv.mat({"intents": targets})
    states = [jv.state(on, ctx=[jv.mat({"profile": p["context"]})]) for p in data["people"]]
    pending = []
    for intent in data["intents"]:
        question = jv.test(
            f"只判断 on.intents.{intent['id']} 所描述的意图及其补充上下文。"
            "ctx[0].profile 中是否存在具体技能、经历、资源或兴趣，能与这个意图形成值得进一步了解的关联或互补？"
            "匹配可以来自跨领域能力，不要求使用同样词汇；但必须有档案中的具体依据，不能替此人编造能力、意愿或联系人。"
            "其他 intents 仅是同批独立问题，不是该意图的附加要求。",
            calib=jv.calib("towow.population.relevance"), evidence=("on", "ctx"))
        pending.append((intent["id"], jv.judge(states, question)))
    result = {}
    for key, readings in pending:
        tiers = readings.order()
        answers = []
        for person, row in zip(data["people"], readings):
            decision = jv.cut(row[0])
            if isinstance(decision, jv.Unsure) and decision.cause == "cold":
                decision = jv.handle(decision)
            value = True if isinstance(decision, jv.Act) else False if isinstance(decision, jv.Ignore) else None
            answers.append({"person": person["id"], "value": value, "provisional": True})
        result[key] = {"tiers": [[data["people"][i]["id"] for i in tier] for tier in tiers], "answers": answers}
    return {"population": data, "judgments": result}


@component("semantic groups with lexical ordering", dict, dict)
def rank_candidates(payload):
    """JEV nominates relations; exact lexical ordering resolves broad tied groups.

    No expected-hit list is read here. Keep unknown separate from rejection.
    """
    data = payload["population"]
    rankings = {}
    for intent in data["intents"]:
        lexical = lexical_ranking(data["people"], intent)
        positions = {key: i for i, key in enumerate(lexical)}
        values = {a["person"]: a["value"] for a in payload["judgments"][intent["id"]]["answers"]}
        group = lambda key: 0 if values[key] is True else 1 if values[key] is None else 2
        rankings[intent["id"]] = {"bm25": lexical, "combined": sorted(lexical, key=lambda key: (group(key), positions[key]))}
    return {**payload, "rankings": rankings}


@component("compare against frozen expected aliases", dict, dict)
def evaluate_population(payload):
    data = payload["population"]
    rows = []
    for intent in data["intents"]:
        observed = payload["judgments"][intent["id"]]
        # Stable alias tie-break within runtime's uncertainty tiers; report the tie.
        ranking = [key for tier in observed["tiers"] for key in sorted(tier)]
        baseline = payload["rankings"][intent["id"]]["bm25"]
        combined = payload["rankings"][intent["id"]]["combined"]
        expected = set(intent["present_expected"])
        def metrics(order):
            hits = sorted(expected.intersection(order[:10]))
            return {"top10": order[:10], "hits": hits, "hit_count": len(hits),
                    "meets_original_min": len(hits) >= intent["min_hits"] if intent["expected_hits"] else None,
                    "anti_hits_top5": sorted(set(intent["anti_hits"]).intersection(order[:5]))}
        rows.append({"intent": intent, "jpp": metrics(combined), "semantic_only": metrics(ranking),
                     "bm25": metrics(baseline), **observed})
    eligible = [r for r in rows if r["intent"]["expected_hits"]]
    return {"rows": rows, "summary": {"subjects": len(data["people"]), "intents": len(rows),
            "judgments": len(data["people"])*len(rows), "queries_with_labels": len(eligible),
            "jpp_meets_min": sum(r["jpp"]["meets_original_min"] for r in eligible),
            "bm25_meets_min": sum(r["bm25"]["meets_original_min"] for r in eligible),
            "jpp_hits": sum(r["jpp"]["hit_count"] for r in rows),
            "semantic_only_hits": sum(r["semantic_only"]["hit_count"] for r in rows),
            "bm25_hits": sum(r["bm25"]["hit_count"] for r in rows)}}


POPULATION_METHOD = judge_population.then(rank_candidates).then(evaluate_population, name="towow_population_discovery")


def run_population(client, root, *, browser=False):
    data = load_population()
    runtime = jv.Runtime(client, root=str(root), max_workers=8, passes={"schedule": not browser})
    start = perf_counter()
    result = execute(POPULATION_METHOD, data, runtime, budget=jv.Budget(calls=400, cost=.08))
    return {**result.value, "stats": result.stats, "structure": result.structure,
            "elapsed_ms": round((perf_counter()-start)*1000, 3)}


def main():
    import argparse
    parser = argparse.ArgumentParser()
    parser.add_argument("--live", action="store_true")
    parser.add_argument("--out", type=Path, default=Path("run-data/towow-population"))
    args = parser.parse_args()
    recording = None if args.live else json.loads(files("jpp").joinpath("data/towow-population-recording.json").read_text())
    budget = CallBudget(limit=.08)
    client = RecordedClient(live=args.live, recording=recording, budget=budget)
    args.out.mkdir(parents=True, exist_ok=True)
    try:
        with TemporaryDirectory() as root:
            first = run_population(client, root)
            repeat = run_population(client, root)
        report = {"schema": 1, "created_utc": datetime.now(timezone.utc).isoformat(),
                  "mode": "live JEV" if args.live else "recorded JEV responses", "first": first, "repeat": repeat,
                  "estimated_cost_usd": budget.spent, "call_budget_usd": budget.limit,
                  "model": client.model_id, "data_source": load_population()["source"]}
        (args.out / "report.json").write_text(json.dumps(report, ensure_ascii=False, indent=2), encoding="utf-8")
        print(json.dumps({"summary": first["summary"], "stats": first["stats"], "elapsed_ms": first["elapsed_ms"],
                          "repeat_calls": repeat["stats"]["calls"], "cost": budget.spent}, ensure_ascii=False))
    finally:
        if args.live:
            (args.out / "recording.json").write_text(json.dumps({"model": client.model_id,"requests":client.records},ensure_ascii=False,indent=2),encoding="utf-8")


if __name__ == "__main__":
    main()
