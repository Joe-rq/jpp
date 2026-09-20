"""Import owned synthetic fixtures, without executing the source test module."""
import argparse
import ast
from collections import defaultdict
from hashlib import sha256
import json
from pathlib import Path
import subprocess

ROOT = Path(__file__).resolve().parents[1]


SOURCE_REF = "6b4230453ecf7c02e8ae86bedfd97eb695b23a7a"
SOURCE_PATHS = ("apps/S1_hackathon/data/agents.json", "apps/S2_skill_exchange/data/agents.json")


def convert(source, history_repo):
    query_path = source / "test_queries.py"
    raw, provenance = {}, []
    for path in SOURCE_PATHS:
        content = subprocess.check_output(["git", "-C", str(history_repo), "show", SOURCE_REF+":"+path])
        profiles = json.loads(content)
        provenance.append({"path": path, "commit": SOURCE_REF, "sha256": sha256(content).hexdigest(), "records": len(profiles)})
        for alias, profile in profiles.items():
            raw[(path, alias)] = profile
    tree = ast.parse(query_path.read_text(encoding="utf-8"))
    queries = next(ast.literal_eval(n.value) for n in tree.body if isinstance(n, ast.Assign)
                   and any(isinstance(t, ast.Name) and t.id == "TEST_QUERIES" for t in n.targets))
    grouped = defaultdict(list)
    for (path, alias), profile in raw.items():
        grouped[alias].append((path+":"+alias, profile))
    people = [{"id": key, "name": rows[0][1]["name"], "role": rows[0][1]["role"],
               "context": "\n\n".join(dict.fromkeys(json.dumps(profile, ensure_ascii=False, sort_keys=True) for _, profile in rows)),
               "source_ids": [source_id for source_id, _ in rows]} for key, rows in sorted(grouped.items())]
    intents = []
    for i, query in enumerate(queries):
        intents.append({"id": f"q{i+1:02}", **query,
                        "present_expected": [x for x in query["expected_hits"] if x in grouped],
                        "missing_expected": [x for x in query["expected_hits"] if x not in grouped]})
    return {"schema": 1, "source": {"project": "Towow original hackathon and skill-exchange synthetic pools", "synthetic": True,
            "original_records": len(raw), "unique_subjects": len(people),
            "files": provenance,
            "queries_sha256": sha256(query_path.read_bytes()).hexdigest(),
            "transform": "Use both complete original collaboration pools, not the query-selected EXP-008 cache. Merge equal aliases; retain all fields and all 20 old queries. No result-based sampling.",
            "evaluation": "Historical development queries with incomplete expected-hit lists, not blind or exhaustive labels. Recruitment/matchmaking pools excluded by domain before execution."},
            "people": people, "intents": intents}


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("source", type=Path)
    parser.add_argument("history_repo", type=Path)
    args = parser.parse_args()
    data = convert(args.source, args.history_repo)
    destination = ROOT / "src/jpp/data/towow-population.json"
    destination.write_text(json.dumps(data, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
    print(len(data["people"]), "subjects;", len(data["intents"]), "intents")
