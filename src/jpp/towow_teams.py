"""Small, composable discovery application over the public synthetic pool.

This is an offline application layer, not a network protocol implementation.
Callers provide the intent, facet operators/questions, router, and combination
rule. A combination keeps its source members and can be passed back as another
node (see :func:`combination_node`) for a later bounded discovery step.
"""
from __future__ import annotations

import argparse
from copy import deepcopy
from datetime import datetime, timezone
import itertools
import json
from pathlib import Path
from time import perf_counter

from foundation import jv
from jev_compose import component, execute

from .towow import CallBudget
from .towow_population import lexical_ranking, load_population
from .towow_real import JournalClient


MAX_QUESTIONS = 20
DEFAULT_CANDIDATE_BUDGET = 16
DEFAULT_COMBINATION_BUDGET = 4
DEFAULT_LEVELS = ("没有具体贡献", "只有泛泛关联", "有具体互补贡献", "有明确的共同实现路径")


def validate_plan(plan, *, seeded=False):
    """Check an authored plan; the app does not infer or enumerate domains."""
    required = ("intent", "facets", "max_candidates")
    missing = [key for key in required if key not in plan]
    if missing:
        raise ValueError("Discovery plan missing: " + ", ".join(missing))
    if not isinstance(plan["intent"].get("query"), str) or not plan["intent"]["query"].strip():
        raise ValueError("plan.intent.query must be nonempty text")
    facets = plan["facets"]
    if not isinstance(facets, list) or len(facets) < (1 if seeded else 2):
        raise ValueError("An unseeded plan needs at least two facets; a seeded continuation may add one")
    ids = [facet.get("id") for facet in facets]
    if any(not isinstance(key, str) or not key for key in ids) or len(set(ids)) != len(ids):
        raise ValueError("Facet ids must be unique, nonempty strings")
    for facet in facets:
        if not all(isinstance(facet.get(key), str) and facet[key].strip()
                   for key in ("id", "label", "search_text", "question")):
            raise ValueError("Each facet needs id, label, search_text, and receiver question")
    limit = int(plan["max_candidates"])
    candidate_budget = limit * len(facets)
    if limit < 1 or candidate_budget > DEFAULT_CANDIDATE_BUDGET:
        raise ValueError("max_candidates is a per-facet cap; facets together may use at most sixteen questions")
    combination = plan.get("combination")
    if combination and "max_combinations" not in plan:
        raise ValueError("A combination rule needs an explicit max_combinations budget")
    combination_budget = int(plan.get("max_combinations", 0)) if combination else 0
    if combination:
        members = combination.get("facets", ids)
        min_facet_members = 1 if seeded else 2
        if len(members) < min_facet_members or len(set(members)) != len(members) or not set(members) <= set(ids):
            raise ValueError("Combination facets must name distinct plan facets and include at least one new facet")
        if not isinstance(combination.get("question"), str) or not combination["question"].strip():
            raise ValueError("A combination rule needs its own question")
        if combination_budget < 0 or combination_budget > DEFAULT_COMBINATION_BUDGET:
            raise ValueError("max_combinations must be at most four")
    if candidate_budget + combination_budget > MAX_QUESTIONS:
        raise ValueError("The plan exceeds the fixed twenty-question application limit")
    return plan


def _lexical_router(nodes, intent, facet):
    """Default replaceable router: profile text BM25 over the supplied pool."""
    query = {"query": intent["query"] + " " + facet["search_text"],
             "context": intent.get("context")}
    return lexical_ranking(nodes, query)


def _combine_profile_context(roles, *mats):
    return "\n\n".join(f"[{role}] {mat.content['profile']}" for role, mat in zip(roles, mats))


def _combination_prompt(intent, reasons, seed_summary):
    return {"intent": intent, "trigger_reasons": reasons,
            "prior_nomination": seed_summary}


def _default_combination_builder(nominations, plan, router=None, seed=None, profile_mats=None):
    """Nominate bounded n-member sets from caller-selected facet nominations."""
    rule = plan.get("combination")
    if not rule or not plan.get("max_combinations", 0):
        return []
    by_facet = nominations
    facet_ids = rule.get("facets", [facet["id"] for facet in plan["facets"]])
    pools = [sorted(by_facet[key], key=lambda row: row["order"]) for key in facet_ids]
    proposals = []
    seen = set()
    for rows in itertools.product(*pools):
        member_ids = tuple(row["person"]["id"] for row in rows)
        if len(set(member_ids)) != len(member_ids):
            continue
        identity = tuple(sorted(member_ids))
        if identity in seen:
            continue
        seen.add(identity)
        members = deepcopy(seed.get("members", [])) if seed else []
        reasons = deepcopy(seed.get("trigger_reasons", [])) if seed else []
        seeded_ids = {member["node"]["id"] for member in members if "node" in member}
        if seeded_ids & set(member_ids):
            continue
        for facet_id, row in zip(facet_ids, rows):
            facet = next(f for f in plan["facets"] if f["id"] == facet_id)
            members.append({"node": deepcopy(row["person"]), "role": facet["label"],
                            "facet_id": facet_id,
                            "evidence_refs": deepcopy(row["person"].get("source_ids", [])),
                            "facet_grade": deepcopy(row["grade"]),
                            "facet_order": row["order"]})
            reasons.append({"facet": facet_id, "node": row["person"]["id"],
                            "question": facet["question"], "grade": deepcopy(row["grade"]),
                            "owner_id": facet.get("owner_id"),
                            "reason": "candidate contribution judgment; not a confirmed commitment"})
        roles = [member["role"] for member in members]
        materials = [profile_mats[member["node"]["id"]] for member in members]
        context_mat = jv.transform(_combine_profile_context, roles, *materials)
        context = context_mat.content
        all_ids = sorted(seeded_ids | set(member_ids))
        proposals.append({
            "id": "combination:" + "+".join(all_ids),
            "members": members,
            "context": context,
            "_context_mat": context_mat,
            "trigger_reasons": reasons,
            "intent": deepcopy(plan["intent"]),
            "parent_nomination_ids": ([seed["id"], *seed.get("parent_nomination_ids", [])]
                                      if seed else []),
            "prior_readings": deepcopy(seed.get("prior_readings", [])) if seed else [],
            "combination_order": [row["order"] for row in rows],
        })
        if len(proposals) >= int(plan["max_combinations"]):
            break
    return proposals


def combination_node(proposal):
    """Return an aggregate nomination as a node for another discovery pass."""
    return nomination_as_input(proposal)


@component("route fixed candidate slots", dict, dict)
def route_candidates(payload):
    plan = payload["plan"]
    router = payload.get("router") or _lexical_router
    limit = int(plan["max_candidates"])
    nominations = {}
    people = payload["nodes"]
    people_by_id = {node["id"]: node for node in people}
    for facet in plan["facets"]:
        ranking = router(people, plan["intent"], facet)
        nominations[facet["id"]] = [
            {"person": deepcopy(people_by_id[key]), "lexical_rank": index}
            for index, key in enumerate(ranking[:limit], 1)
        ]
    return {**payload, "nominations": nominations}


def _grade(reading, scale):
    result = jv.cut(reading)
    if isinstance(result, jv.Unsure):
        if result.cause == "cold":
            provisional = jv.handle(result)
            if isinstance(provisional, jv.At):
                level = provisional.level
                return {"level": level, "label": scale[level], "nearest_level": level,
                        "nearest_label": scale[level], "status": "measured",
                        "provisional": True, "cause": "cold"}
            if isinstance(provisional, jv.Unsure):
                cause = provisional.cause
                jv.consume([provisional], unsure=jv.drop)
                return {"level": None, "label": None, "nearest_level": None,
                        "nearest_label": None, "status": "unsure", "cause": cause,
                        "provisional": True}
        detail = result.detail or {}
        nearest = detail.get("nearest_level")
        jv.consume([result], unsure=jv.drop)
        return {"level": None, "label": None, "nearest_level": nearest,
                "nearest_label": scale[nearest] if isinstance(nearest, int) and 0 <= nearest < len(scale) else None,
                "status": "unsure", "cause": result.cause, "provisional": True}
    if isinstance(result, jv.At):
        level = result.level
        return {"level": level, "label": scale[level], "nearest_level": level,
                "nearest_label": scale[level], "status": "measured",
                "provisional": bool(result.provisional)}
    return {"level": None, "label": None, "nearest_level": None, "nearest_label": None,
            "status": getattr(result, "kind", "unresolved"), "provisional": True}


def _order_indices(readings):
    positions, tiers = {}, {}
    for tier_index, indices in enumerate(readings.order()):
        for index in sorted(indices):
            positions[index] = len(positions) + 1
            tiers[index] = tier_index
    return positions, tiers


@component("judge receiver-owned facet questions", dict, dict, effects=("judge",))
def judge_candidates(payload):
    plan = payload["plan"]
    judged = {}
    for facet in plan["facets"]:
        rows = payload["nominations"][facet["id"]]
        scale = tuple(facet.get("scale", plan.get("scale", DEFAULT_LEVELS)))
        states = [jv.state(
            on=payload["facet_on_mats"][facet["id"]],
            ctx=[payload["profile_mats"][row["person"]["id"]]],
        ) for row in rows]
        question = jv.measure(facet["question"], scale=scale,
                              calib=jv.calib(facet.get("calibration", "towow.discovery." + facet["id"])),
                              evidence=("on", "ctx"))
        readings = jv.judge(states, question)
        positions, tiers = _order_indices(readings)
        judged[facet["id"]] = [{
            **row,
            "grade": _grade(reading[0], scale),
            "order": positions.get(index, index + 1),
            "tier": tiers.get(index),
            "evidence_refs": deepcopy(row["person"].get("source_ids", [])),
        } for index, (row, reading) in enumerate(zip(rows, readings))]
    builder = payload.get("combination_builder") or _default_combination_builder
    proposals = builder(judged, plan, payload.get("router"), payload.get("seed"),
                        payload.get("profile_mats"))
    proposals = proposals[:int(plan.get("max_combinations", 0))] if plan.get("combination") else []
    return {**payload, "candidates": judged, "proposals": proposals}


@component("judge nominated combinations", dict, dict, effects=("judge",))
def judge_combinations(payload):
    proposals = payload["proposals"]
    rule = payload["plan"].get("combination")
    if not rule or not proposals:
        return {**payload, "proposals": [] if not rule else proposals}
    scale = tuple(rule.get("scale", payload["plan"].get("scale", DEFAULT_LEVELS)))
    states = [jv.state(
        on=jv.transform(_combination_prompt, payload["plan"]["intent"],
                         proposal["trigger_reasons"],
                         {key: payload["seed"].get(key) for key in
                          ("id", "intent", "prior_readings", "parent_nomination_ids")}
                         if payload.get("seed") else None),
        ctx=[proposal["_context_mat"]],
    ) for proposal in proposals]
    question = jv.measure(rule["question"], scale=scale,
                          calib=jv.calib(rule.get("calibration", "towow.discovery.combination")),
                          evidence=("on", "ctx"))
    readings = jv.judge(states, question)
    positions, tiers = _order_indices(readings)
    missing = payload["plan"].get("missing_conditions", rule.get("missing_conditions", [
        "成员是否愿意参与尚未判断", "成员各自可投入的时间尚未核实", "行动范围与验收方式尚未确定"]))
    next_question = payload["plan"].get("next_question", rule.get("next_question", "成员是否愿意共同推进这个方案，最小可验证行动是什么？"))
    output = []
    for index, (proposal, reading) in enumerate(zip(proposals, readings)):
        grade = _grade(reading[0], scale)
        output.append({**proposal, "grade": grade,
                       "order": positions.get(index, index + 1), "tier": tiers.get(index),
                       "missing_conditions": deepcopy(missing), "next_question": next_question,
                       "provisional": True})
    output.sort(key=lambda row: row["order"])
    return {**payload, "proposals": output}


DISCOVERY_METHOD = route_candidates.then(judge_candidates).then(
    judge_combinations, name="bounded_composable_discovery")


def run_discovery(plan, client, root, *, people=None, seed=None, router=None,
                  combination_builder=None, budget_total=0.08):
    """Run a bounded discovery plan over nodes, returning evidence-backed nominations.

    `router(nodes, intent, facet)` and `combination_builder(judged, plan, router, seed, profile_mats)`
    are replaceable application functions. They see node material and plan only;
    evaluation labels are not an argument. The returned aggregate `context` and
    `members` can be projected with :func:`combination_node` into a later call.
    """
    validate_plan(plan, seeded=seed is not None)
    nodes = deepcopy(people if people is not None else load_population()["people"])
    seed_node = nomination_as_input(seed) if seed is not None else None
    material_nodes = list(nodes)
    if seed_node is not None:
        material_nodes.extend(member["node"] for member in seed_node["members"] if "node" in member)
    profile_mats = {node["id"]: jv.mat({"profile": node["context"], "node_id": node["id"]})
                    for node in material_nodes}
    facet_on_mats = {facet["id"]: jv.mat({
        "intent": plan["intent"]["query"],
        "intent_context": plan["intent"].get("context"),
        "seed_context": seed_node["context"] if seed_node else None,
        "facet": facet["label"], "owner_id": facet.get("owner_id"),
    }) for facet in plan["facets"]}
    candidate_questions = min(DEFAULT_CANDIDATE_BUDGET,
                              len(plan["facets"]) * int(plan["max_candidates"]))
    combination_questions = int(plan.get("max_combinations", 0)) if plan.get("combination") else 0
    question_limit = candidate_questions + combination_questions
    if question_limit > MAX_QUESTIONS:
        raise ValueError("Discovery run exceeds the fixed twenty-question application limit")
    payload = {"plan": deepcopy(plan), "nodes": nodes, "seed": deepcopy(seed_node),
               "profile_mats": profile_mats, "facet_on_mats": facet_on_mats,
               "router": router, "combination_builder": combination_builder}
    runtime = jv.Runtime(client, root=str(root), max_workers=8)
    started = perf_counter()
    result = execute(DISCOVERY_METHOD, payload, runtime,
                     budget=jv.Budget(calls=MAX_QUESTIONS, cost=budget_total))
    if "W-call-fail" in result.stats.get("warnings", []):
        raise RuntimeError("A JEV request failed. The incomplete discovery run is not a valid result; resume or replay from the journal.")
    value = result.value
    candidate_questions = sum(len(rows) for rows in value["candidates"].values())
    actual_questions = candidate_questions + len(value["proposals"])
    public_proposals = [{key: item for key, item in proposal.items() if not key.startswith("_")}
                        for proposal in value["proposals"]]
    return {
        "schema": 1,
        "mode": "live JEV" if getattr(client, "live", False) else "exact journal replay",
        "model": getattr(client, "model_id", None),
        "input": {"intent": value["plan"]["intent"], "facets": value["plan"]["facets"],
                  "owner_id": value["plan"].get("owner_id"), "seed": value.get("seed")},
        "candidates": value["candidates"],
        "proposals": public_proposals,
        "stats": {"questions": actual_questions, "calls": result.stats["calls"],
                  "ledger_hits": result.stats.get("ledger_hits", 0),
                  "elapsed_ms": round((perf_counter() - started) * 1000, 3),
                  "new_estimated_cost_usd": result.stats.get("cost", 0),
                  "total_estimated_cost_usd": getattr(getattr(client, "budget", None), "spent", 0)},
        "limits": {"candidate_questions": candidate_questions,
                   "combination_questions": len(value["proposals"]),
                   "total_questions": actual_questions, "max_questions": MAX_QUESTIONS},
        "scope": "A bounded application composition. Nominations and combinations are provisional; this does not implement network-scale propagation, private relay discovery, or prove willingness.",
    }


def nomination_as_input(proposal):
    """Project a proposal to a composable node, keeping source text and tentative readings."""
    if "parent_ids" in proposal and "prior_readings" in proposal and "grade" not in proposal:
        return deepcopy(proposal)
    members = deepcopy(proposal.get("members", []))
    context = "\n\n".join(
        f"[{member.get('role', 'member')}] {member['node']['context']}"
        for member in members if "node" in member and "context" in member["node"])
    return {
        "id": proposal["id"],
        "context": context,
        "members": members,
        "parent_ids": sorted({member["node"]["id"] for member in members if "node" in member}),
        "intent": deepcopy(proposal.get("intent")),
        "parent_nomination_ids": deepcopy(proposal.get("parent_nomination_ids", [])),
        "trigger_reasons": deepcopy(proposal.get("trigger_reasons", [])),
        "prior_readings": (deepcopy(proposal.get("prior_readings", [])) + [{"facet_id": member.get("facet_id"),
                             "grade": deepcopy(member.get("facet_grade")),
                             "status": (member.get("facet_grade") or {}).get("status", "unsure"),
                             "provisional": True} for member in members] +
                           ([{"facet_id": "combination", "grade": deepcopy(proposal["grade"]),
                              "status": proposal["grade"].get("status", "unsure"),
                              "provisional": True}] if proposal.get("grade") is not None else [])),
        "source_ids": sorted({source for member in members
                              for source in member.get("evidence_refs", [])}),
    }


def _example_intent(data, key):
    rows = [row for row in data["intents"] if row.get("id") == key]
    if len(rows) != 1:
        raise ValueError(f"Public synthetic pool does not contain exactly one {key} intent")
    return {name: deepcopy(rows[0].get(name)) for name in ("id", "query", "context", "level")}


def example_q16_plan(data=None):
    """An authored sound-to-visual plan; not a universal decomposition rule."""
    data = data or load_population()
    return {
        "intent": _example_intent(data, "q16"),
        "facets": [
            {"id": "audio", "label": "音频输入与处理",
             "search_text": "声音 音频 音乐 采样 实时处理 合成 Max/MSP 声音艺术",
             "question": "档案是否记载了本人具备可用于这个意图的具体音频输入、声音设计或音频处理能力？指出依据；单纯兴趣或想学习不算。"},
            {"id": "visual", "label": "视觉输出与图形",
             "search_text": "画面 视觉 图形 Shader 实时视觉特效 WebGL Three.js Processing",
             "question": "档案是否记载了本人具备可用于这个意图的具体视觉输出、图形编程或实时视觉效果能力？指出依据；单纯兴趣或想学习不算。"},
        ],
        "max_candidates": 8,
        "scale": DEFAULT_LEVELS,
        "combination": {
            "facets": ["audio", "visual"],
            "question": "结合意图与这两份档案，判断成员能否形成有具体依据的互补合作构型。不能把泛泛兴趣相加，也不能编造意愿、时间或已发生合作。",
            "next_question": "你们是否愿意共同做一个最小原型？各自何时有空，先验证哪一种输入与输出？",
        },
        "max_combinations": 4,
    }


def example_q18_plan(data=None):
    """A second authored plan with different facets, for interface examples."""
    data = data or load_population()
    return {
        "intent": _example_intent(data, "q18"),
        "facets": [
            {"id": "craft", "label": "传统工艺或手作",
             "search_text": "传统 手艺 工艺 木工 陶艺 皮革 雕刻 纺织",
             "question": "档案是否记载了本人具备与这个意图有关的具体传统工艺或手作经验？指出依据；泛泛兴趣不算。"},
            {"id": "digital", "label": "数字技术",
             "search_text": "数字 技术 编程 电子 Arduino Shader Processing 生成艺术",
             "question": "档案是否记载了本人具备可用于这个意图的具体数字技术或制作能力？指出依据；泛泛兴趣不算。"},
        ],
        "max_candidates": 8,
        "scale": DEFAULT_LEVELS,
        "combination": {
            "facets": ["craft", "digital"],
            "question": "两份档案是否共同提供把传统工艺与数字技术结合的具体能力或路径？只根据材料判断。",
            "next_question": "你们愿意尝试哪种最小的传统工艺与数字技术原型？",
        },
        "max_combinations": 4,
    }


def main(argv=None):
    parser = argparse.ArgumentParser(description="Run an authored bounded discovery plan")
    parser.add_argument("--plan", type=Path, help="JSON plan; defaults to the authored public q16 example")
    parser.add_argument("--people", type=Path, help="JSON profile list or object with a people list")
    parser.add_argument("--seed", type=Path, help="JSON proposal object from a prior discovery report")
    parser.add_argument("--journal", type=Path, default=Path("run-data/towow-teams/recording.jsonl"))
    parser.add_argument("--live", action="store_true", help="Call JEV; default is exact journal replay")
    parser.add_argument("--budget-total", type=float, default=0.08,
                        help="Maximum estimated spend across the full journal, in USD")
    parser.add_argument("--out", type=Path, default=Path("run-data/towow-teams/report.json"))
    parser.add_argument("--no-combinations", action="store_true", help="Ablate the combination stage")
    args = parser.parse_args(argv)
    if args.budget_total <= 0:
        parser.error("--budget-total must be positive")
    data = load_population()
    if args.plan:
        plan = json.loads(args.plan.read_text(encoding="utf-8"))
    else:
        plan = example_q16_plan(data)
    people = data["people"]
    if args.people:
        loaded_people = json.loads(args.people.read_text(encoding="utf-8"))
        people = loaded_people["people"] if isinstance(loaded_people, dict) else loaded_people
    seed = json.loads(args.seed.read_text(encoding="utf-8")) if args.seed else None
    if args.no_combinations:
        plan["combination"] = None
    validate_plan(plan, seeded=seed is not None)
    args.journal.parent.mkdir(parents=True, exist_ok=True)
    args.out.parent.mkdir(parents=True, exist_ok=True)
    budget = CallBudget(limit=args.budget_total)
    client = JournalClient(args.journal, live=args.live, budget=budget)
    report = run_discovery(plan, client, args.out.parent / "ledger", people=people,
                           seed=seed, budget_total=args.budget_total)
    report.update({"created_utc": datetime.now(timezone.utc).isoformat(),
                   "journal": str(args.journal),
                   "estimated_total_cost_usd": client.budget.spent if args.live else client.prior_cost,
                   "estimated_new_cost_usd": client.budget.spent-client.prior_cost if args.live else 0})
    args.out.write_text(json.dumps(report, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
    print(json.dumps({"questions": report["stats"]["questions"], "calls": report["stats"]["calls"],
                      "ledger_hits": report["stats"]["ledger_hits"],
                      "estimated_new_cost_usd": report["estimated_new_cost_usd"],
                      "out": str(args.out)}, ensure_ascii=False))


if __name__ == "__main__":
    main()
