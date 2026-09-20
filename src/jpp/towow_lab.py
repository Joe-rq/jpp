"""Executable interventions on the published example, using exact JEV recordings.

The same module runs in CPython and in the browser's Python interpreter. There is
no second discovery implementation in JavaScript and no live model client here.
"""
from copy import deepcopy
from importlib.resources import files
import json
from pathlib import Path
from tempfile import TemporaryDirectory

from .towow import RecordedClient, discover, expand_relays, extend_combinations, load_scenario, run_once


def compose_method(referral=True, composition=True):
    method = discover
    if referral:
        method = method.then(expand_relays)
    if composition:
        method = method.then(extend_combinations)
    return method


def outcome(run):
    """Compare semantic outputs independently of timing, cache and evidence ids."""
    return {
        "support": sorted(d["person"] for d in run["direct"]),
        "combinations": sorted(c["id"] for c in run["candidates"]),
        "extensions": sorted(e["person"] for e in run["extensions"] if e["relation"] is True),
        "discuss": sorted(e["person"] for e in run["extensions"] if e["discuss_now"] is True),
        "unresolved": sorted(e["person"] for e in run["extensions"] if e["relation"] is None),
    }


class LabSession:
    def __init__(self):
        self._root = TemporaryDirectory(prefix="towow-lab-")
        self.recording = json.loads(files("jpp").joinpath("data/towow-recording.json").read_text(encoding="utf-8"))
        self.previous = None

    def close(self):
        self._root.cleanup()

    def run(self, *, referral=True, composition=True, contact=True, available=False, reuse=True):
        options = dict(referral=referral, composition=composition, contact=contact, available=available, reuse=reuse)
        if any(type(v) is not bool for v in options.values()):
            raise ValueError("Experiment switches must be booleans")
        scenario = load_scenario()
        if not contact:
            # Remove addresses, not the text supporting the semantic judgment.
            # The resulting request bodies are still covered by exact recordings.
            for person in scenario["people"]:
                person["contacts"] = []
        if available:
            update = scenario["update"]
            next(p for p in scenario["people"] if p["id"] == update["person"])["context"] = update["context"]
        client = RecordedClient(recording=self.recording)
        run = run_once(scenario, client, Path(self._root.name), method=compose_method(referral, composition),
                       passes={"schedule": False, "ledger": reuse})
        result = {"options": options, "scenario": scenario, "run": run, "outcome": outcome(run),
                  "recording_lookups": len(client.records), "live_model_calls": 0,
                  "mode": "J++ execution with exact recorded JEV responses; sequential browser-compatible scheduling",
                  "requests": client.records}
        if self.previous:
            result["change"] = {key: {"added": sorted(set(value) - set(self.previous[key])),
                                       "removed": sorted(set(self.previous[key]) - set(value))}
                                for key, value in result["outcome"].items()}
        else:
            result["change"] = None
        self.previous = deepcopy(result["outcome"])
        return result


CASES = [
    ("one_round", "仅首轮发现", {"referral": False, "composition": False}),
    ("referral", "加上转介", {"composition": False}),
    ("full", "转介 + 组合", {}),
    ("no_contact", "撤掉联系人地址", {"contact": False}),
    ("available", "司机确认有空", {"available": True}),
]


def compare():
    """Matched materials and recordings; each variant starts with an empty ledger."""
    rows = []
    for key, label, options in CASES:
        session = LabSession()
        try:
            result = session.run(**options)
            rows.append({"id": key, "label": label, **result})
        finally:
            session.close()
    session = LabSession()
    try:
        first = session.run()
        repeat = session.run()
        updated = session.run(available=True)
        no_reuse = session.run(available=True, reuse=False)
    finally:
        session.close()
    return {"schema": 1, "mode": "offline execution of exact recorded model responses", "cases": rows,
            "reuse": {"first": first, "repeat": repeat, "updated": updated, "disabled": no_reuse},
            "scope": "One developed fictional case. Component and dependency interventions, not a model accuracy study or a comparison against all matching algorithms."}


def main():
    import argparse
    parser = argparse.ArgumentParser(description="Run reproducible Towow component interventions without model calls")
    parser.add_argument("--out", type=Path, default=Path("run-data/towow-comparison.json"))
    args = parser.parse_args()
    result = compare()
    args.out.parent.mkdir(parents=True, exist_ok=True)
    args.out.write_text(json.dumps(result, ensure_ascii=False, indent=2), encoding="utf-8")
    for row in result["cases"]:
        print(row["label"], json.dumps(row["outcome"], ensure_ascii=False), "recording lookups:", row["recording_lookups"])
    print("Report:", args.out)


if __name__ == "__main__":
    main()
