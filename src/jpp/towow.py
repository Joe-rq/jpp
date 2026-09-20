"""Ten fictional participants; real J++ components, judgments and ledger reuse.

The default client replays published model responses. --live uses JevClient.
No participant is selected by id, an expected label, or a keyword rule.
"""
from __future__ import annotations

import argparse
from concurrent.futures import ThreadPoolExecutor
from copy import deepcopy
from dataclasses import dataclass, field
from datetime import datetime, timezone
from hashlib import sha256
from importlib.resources import files
import json
from pathlib import Path
from tempfile import TemporaryDirectory
from threading import Lock
from time import perf_counter

from foundation import jv
from jev_compose import component, execute
from jev_compose.observation import Request, batch_observe

MODEL = "jev-1.13.0"
QUESTIONS = {
    "direct": "根据 on.signal 的处境，ctx[0].local_context 是否明确记载了接收方本人能提供的一种具体支持、活动或服务，值得让发送者了解？只读给定材料。相似处境、单纯想参加、投资兴趣、第三人的能力不算本人直接提供。无需诊断或推断疾病。",
    "relay": "ctx[0].local_context 是否明确提到一个具体联系人，其能力可能回应 on.signal 的处境，值得进一步读取该联系人的上下文？必须有材料中的具体转介线索，不能猜想人脉。",
    "ready": "ctx[0].local_context 是否明确确认了接收方本人当前可以提供该项支持的时间或可用性？待确认、暂时不能、仅过去的经历都不算确认。",
    "extend": "ctx[0].local_context 中描述的具体兴趣或能力，与 on.candidate.support 描述的服务内容是否相关？只判断内容上的联系，忽略提供者目前有没有空。泛泛的任何项目都能用的技能不算。",
    "activate": "ctx[0].local_context 是否明确表示愿意参与试用、组织试用或讨论 on.candidate.support 所描述的这类服务？只判断接收方的意愿，忽略提供者目前有没有空。",
}


def load_scenario():
    return json.loads(files("jpp").joinpath("data/towow-people.json").read_text(encoding="utf-8"))


def fingerprint(state, questions):
    raw = json.dumps([state, questions], ensure_ascii=False, sort_keys=True, separators=(",", ":"))
    return sha256(raw.encode()).hexdigest()


class CallBudget:
    """Shared across live runs and the optional benchmark, using backend pricing."""
    def __init__(self, limit=0.10):
        self.limit, self.spent, self.reserved = limit, 0.0, 0.0
        self.lock = Lock()

    def reserve(self, amount):
        with self.lock:
            if self.spent + self.reserved + amount > self.limit:
                raise RuntimeError("Towow live-call budget exhausted")
            self.reserved += amount

    def settle(self, reservation, actual):
        with self.lock:
            self.reserved -= reservation
            self.spent += actual


class RecordedClient:
    """Capture only fictional request bodies and responses, never HTTP headers."""
    def __init__(self, *, live=False, recording=None, budget=None):
        self.model_id = MODEL
        self.live = live
        self.backend = jv.JevClient(MODEL) if live else None
        self.budget = budget or CallBudget()
        self.records = []
        self.lock = Lock()
        self.saved = {r["key"]: r for r in (recording or {}).get("requests", [])}

    def ask(self, state, questions):
        key = fingerprint(state, questions)
        start = perf_counter()
        if self.live:
            # Reserve conservatively for input bytes plus service framing. Costs
            # are estimates under the backend's configured per-token price.
            size = len(json.dumps([state, questions], ensure_ascii=False).encode())
            reservation = (size + 8192) * self.backend.params.usd_per_input_token
            self.budget.reserve(reservation)
            try:
                answers, tokens, cost = self.backend.ask(state, questions)
            except Exception:
                self.budget.settle(reservation, reservation)
                raise
            self.budget.settle(reservation, cost)
        else:
            if key not in self.saved:
                raise ValueError("Recorded response missing for this input. Use --live for changed scenarios.")
            row = self.saved[key]
            answers, tokens, cost = deepcopy(row["answers"]), 0, 0.0
        record = {"key": key, "state": deepcopy(state), "questions": deepcopy(questions),
                  "answers": deepcopy(answers), "input_tokens": tokens,
                  "estimated_cost_usd": cost, "elapsed_ms": round((perf_counter() - start) * 1000, 3)}
        with self.lock:
            self.records.append(record)
        return answers, tokens, cost


@dataclass
class Discovery:
    scenario: dict
    direct: list = field(default_factory=list)
    relays: list = field(default_factory=list)
    candidates: list = field(default_factory=list)
    extensions: list = field(default_factory=list)
    observations: list = field(default_factory=list)
    stages: list = field(default_factory=list)

    def person(self, key):
        return next(p for p in self.scenario["people"] if p["id"] == key)


def make_request(person, kind, on):
    # Only this receiver's local world is supplied, never all ten biographies.
    context = jv.mat({"local_context": person["context"], "person": person["id"]})
    state = jv.state(jv.mat(on), ctx=[context])
    return Request(state, jv.test(QUESTIONS[kind], calib=jv.calib("towow." + kind),
                                 evidence=("on", "ctx")), label=person["id"] + ":" + kind,
                   tag={"person": person["id"], "kind": kind})


def decisions(requests):
    """Use the public cold-calibration handler; keep provisional labels visible."""
    out = []
    for observation in batch_observe(requests):
        decision = observation.decision
        if isinstance(decision, jv.Unsure) and decision.cause == "cold":
            decision = jv.handle(decision)  # existing runtime's explicitly provisional policy
        value = None
        cause = observation.cause
        provisional = True
        if isinstance(decision, jv.Act):
            value, cause, provisional = True, None, decision.provisional
        elif isinstance(decision, jv.Ignore):
            value, cause, provisional = False, None, decision.provisional
        elif isinstance(decision, jv.Unsure):
            cause = decision.cause
        out.append({"id": observation.id, **observation.request.tag, "value": value,
                    "provisional": provisional, "cause": cause,
                    "question": observation.request.question.text,
                    "material_hashes": [m.hash for m in observation.request.state.resolved().all_mats]})
    return out


def checkpoint(flow, name, started):
    stats = jv.stats()
    flow.stages.append({"name": name, "elapsed_ms": round((perf_counter() - started) * 1000, 3),
                       "cumulative_calls": stats["calls"], "direct": deepcopy(flow.direct),
                       "relays": deepcopy(flow.relays), "candidates": deepcopy(flow.candidates),
                       "extensions": deepcopy(flow.extensions)})
    return flow


@component("receiver-local discovery", Discovery, Discovery, effects=("judge",))
def discover(flow):
    start = perf_counter()
    request_list = []
    for person in flow.scenario["people"]:
        if person["visible"] and person["id"] != flow.scenario["sender"]:
            request_list.extend(make_request(person, kind, {"signal": flow.scenario["signal"]})
                                for kind in ("direct", "relay"))
    answers = decisions(request_list)
    flow.observations.extend(answers)
    for answer in answers:
        if answer["value"] is True:
            if answer["kind"] == "direct":
                flow.direct.append({"person": answer["person"], "via": [], "evidence": answer["id"]})
            else:
                for target in flow.person(answer["person"]).get("contacts", []):
                    flow.relays.append({"via": answer["person"], "target": target, "evidence": answer["id"]})
    return checkpoint(flow, "同一意图遇到八个接收方", start)


@component("expand nominated relays", Discovery, Discovery, effects=("judge",))
def expand_relays(flow):
    start = perf_counter()
    targets = sorted({r["target"] for r in flow.relays})
    requests = [make_request(flow.person(target), kind, {"signal": flow.scenario["signal"]})
                for target in targets for kind in ("direct", "ready")]
    answers = decisions(requests)
    flow.observations.extend(answers)
    lookup = {(a["person"], a["kind"]): a for a in answers}
    for target in targets:
        support, ready = lookup[target, "direct"], lookup[target, "ready"]
        if support["value"] is not True:
            continue
        via = sorted({r["via"] for r in flow.relays if r["target"] == target})
        flow.direct.append({"person": target, "via": via, "evidence": support["id"]})
        # A generic candidate material, assembled from nominated members' source text.
        # No generated business idea or preassigned match label is smuggled into it.
        flow.candidates.append({"id": "candidate:" + target,
            "members": [flow.scenario["sender"], *via, target],
            "signal": flow.scenario["signal"], "support": flow.person(target)["context"],
            "availability": ready["value"], "provisional": True,
            "basis": [support["id"], ready["id"]]})
    return checkpoint(flow, "转介触达未在初始广播中的主体，形成中间构型", start)


@component("combinations meet new contexts", Discovery, Discovery, effects=("judge",))
def extend_combinations(flow):
    start = perf_counter()
    requests = []
    for candidate in flow.candidates:
        for person in flow.scenario["people"]:
            if person["id"] not in candidate["members"]:
                for kind in ("extend", "activate"):
                    request = make_request(person, kind, {"candidate": candidate})
                    request.tag["candidate"] = candidate["id"]
                    requests.append(request)
    answers = decisions(requests)
    flow.observations.extend(answers)
    lookup = {(a["candidate"], a["person"], a["kind"]): a for a in answers}
    for a in answers:
        if a["kind"] == "extend" and a["value"] is not False:
            ready = lookup[a["candidate"], a["person"], "activate"]
            candidate = next(c for c in flow.candidates if c["id"] == a["candidate"])
            # Availability is already a typed observation: exact conjunction
            # belongs in ordinary code, not a repeated model judgment.
            conditions = [a["value"], ready["value"], candidate["availability"]]
            discuss = False if False in conditions else (True if all(v is True for v in conditions) else None)
            flow.extensions.append({"candidate": a["candidate"], "person": a["person"],
                                    "relation": a["value"], "willing": ready["value"],
                                    "discuss_now": discuss, "provisional": True,
                                    "context": flow.person(a["person"])["context"],
                                    "evidence": a["id"]})
    return checkpoint(flow, "中间构型作为材料，再发现互补主体", start)


METHOD = discover.then(expand_relays).then(extend_combinations, name="towow_discovery")


def run_once(scenario, client, root, *, workers=8):
    rt = jv.Runtime(client, root=str(root), max_workers=workers)
    start = perf_counter()
    result = execute(METHOD, Discovery(deepcopy(scenario)), rt,
                     budget=jv.Budget(calls=40, cost=0.02))
    return {"elapsed_ms": round((perf_counter() - start) * 1000, 3),
            "stats": result.stats, "direct": result.value.direct, "relays": result.value.relays,
            "candidates": result.value.candidates, "extensions": result.value.extensions,
            "observations": result.value.observations, "stages": result.value.stages,
            "structure": result.structure}


def benchmark_requests(records, budget):
    """Real sequential/parallel calls on the exact same initial physical requests."""
    results = {}
    for name, workers in (("sequential", 1), ("parallel", 8)):
        client = RecordedClient(live=True, budget=budget)
        start = perf_counter()
        with ThreadPoolExecutor(max_workers=workers) as pool:
            list(pool.map(lambda r: client.ask(r["state"], r["questions"]), records))
        results[name] = {"elapsed_ms": round((perf_counter() - start) * 1000, 3),
                         "calls": len(client.records), "requests": client.records}
    results["speedup"] = round(results["sequential"]["elapsed_ms"] / results["parallel"]["elapsed_ms"], 3)
    results["scope"] = "Same initial request bodies; one sequential run then one parallel run; provider/network caches uncontrolled."
    return results


def run_demo(*, live=False, out=None, benchmark=False, scenario=None):
    if benchmark and not live:
        raise ValueError("--benchmark requires --live")
    scenario = scenario or load_scenario()
    recording = None if live else json.loads(files("jpp").joinpath("data/towow-recording.json").read_text(encoding="utf-8"))
    budget = CallBudget()
    client = RecordedClient(live=live, recording=recording, budget=budget)
    with TemporaryDirectory(prefix="jpp-towow-") as root:
        cold = run_once(scenario, client, root)
        cold_records = list(client.records)
        warm = run_once(scenario, client, root)
        changed = deepcopy(scenario)
        update = changed["update"]
        for person in changed["people"]:
            if person["id"] == update["person"]:
                person["context"] = update["context"]
        revised = run_once(changed, client, root)
    report = {"schema": 1, "created_utc": datetime.now(timezone.utc).isoformat(),
              "mode": "live JEV" if live else "recorded JEV responses; zero network calls",
              "model": MODEL, "scenario": scenario, "cold": cold, "warm": warm, "updated": revised,
              "limitations": "Fictional participants; provisional uncalibrated nominations; finite rounds; one-process simulation, not a distributed or billion-node test."}
    if benchmark:
        first_calls = cold["stages"][0]["cumulative_calls"]
        report["benchmark"] = benchmark_requests(cold_records[:first_calls], budget)
    report["total_estimated_cost_usd"] = round(budget.spent, 8)
    report["call_budget_usd"] = budget.limit
    recorded = {"model": MODEL, "created_utc": report["created_utc"], "requests": client.records}
    if out:
        out = Path(out)
        out.mkdir(parents=True, exist_ok=True)
        (out / "report.json").write_text(json.dumps(report, ensure_ascii=False, indent=2), encoding="utf-8")
        if live:
            (out / "recording.json").write_text(json.dumps(recorded, ensure_ascii=False, indent=2), encoding="utf-8")
        from .towow_view import write_view
        write_view(report, out / "index.html")
    return report


def summary(report):
    names = {p["id"]: p["name"] for p in report["scenario"]["people"]}
    return {"mode": report["mode"], "model": report["model"],
        "direct": [names[p["person"]] for p in report["cold"]["direct"]],
        "relay": [names[r["via"]] + " → " + names[r["target"]] for r in report["cold"]["relays"]],
        "candidate_extensions": [names[e["person"]] for e in report["cold"]["extensions"] if e["relation"] is True],
        "unresolved_extensions": [names[e["person"]] for e in report["cold"]["extensions"] if e["relation"] is None],
        "discussion_after_update": [names[e["person"]] for e in report["updated"]["extensions"] if e["discuss_now"] is True],
        "runs": {key: {"elapsed_ms": report[key]["elapsed_ms"], "calls": report[key]["stats"]["calls"],
                       "ledger_hits": report[key]["stats"]["ledger_hits"],
                       "estimated_cost_usd": report[key]["stats"]["cost"]} for key in ("cold", "warm", "updated")},
        "benchmark": {k: v for k, v in report.get("benchmark", {}).items() if k in ("speedup", "scope")},
        "all_nominations_provisional": True}


def main(argv=None):
    parser = argparse.ArgumentParser(description="Towow ten-person J++ demonstration")
    parser.add_argument("--live", action="store_true", help="Call JEV using ~/.typesafe-key; incurs API charges")
    parser.add_argument("--benchmark", action="store_true", help="Extra live calls comparing identical initial requests")
    parser.add_argument("--scenario", type=Path, help="Custom scenario JSON; requires live responses or a matching recording")
    parser.add_argument("--out", type=Path, default=Path("run-data/towow"))
    args = parser.parse_args(argv)
    scenario = json.loads(args.scenario.read_text(encoding="utf-8")) if args.scenario else None
    report = run_demo(live=args.live, benchmark=args.benchmark, out=args.out, scenario=scenario)
    print(json.dumps(summary(report), ensure_ascii=False, indent=2))
    print("View:", args.out / "index.html")


if __name__ == "__main__":
    main()
