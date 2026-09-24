from __future__ import annotations

import argparse
import json
import math
import sys

from jev_interface import judge_batch

ALPHA = 0.1
CONF_C = 0.1
SEED = 20260923
MASK64 = (1 << 64) - 1
NEED = math.ceil(math.log(CONF_C) / math.log(1 - ALPHA))

REL_INSTR = "两条实体描述作为产品是什么关系？"
REL_CRITERIA = [
    "它们描述的是两种不同的产品。",
    "它们描述的是密切相关、可能是也可能不是同一款的产品：变体、特别版，或名字两种理解都说得通。",
    "它们描述的是同一款产品。",
]
NAME_INSTR = "两条实体写的是同一个啤酒名吗？"
BREWERY_INSTR = "两条实体来自同一家酒厂吗？"
STYLE_INSTR = "两条实体描述的是同一种啤酒风格吗？"

LEVEL_OUTCOME = {0: "leave unlinked", 1: "curator queue", 2: "assert sameAs"}


def canon(x) -> str:
    return json.dumps(x, sort_keys=True, ensure_ascii=False)


def splitmix64(x: int) -> int:
    z = (x & MASK64) + 0x9E3779B97F4A7C15
    z &= MASK64
    z = ((z ^ (z >> 30)) * 0xBF58476D1CE4E5B9) & MASK64
    z = ((z ^ (z >> 27)) * 0x94D049BB133111EB) & MASK64
    return (z ^ (z >> 31)) & MASK64


def binom_le(n: int, m: float, k: int) -> float:
    total = 0.0
    for i in range(k + 1):
        total += math.comb(n, i) * (m ** i) * ((1 - m) ** (n - i))
    return total


def clopper_pearson_upper(k: int, n: int) -> float:
    if n == 0 or k >= n:
        return 1.0
    lo, hi = k / n, 1.0
    for _ in range(200):
        mid = (lo + hi) / 2
        p = binom_le(n, mid, k)
        if p > CONF_C:
            lo = mid
        else:
            hi = mid
    return (lo + hi) / 2


def split_half(samples: list) -> tuple[list, list]:
    ordered = sorted(samples, key=lambda t: (t[0], t[1]))
    sel, cert = [], []
    for i, s in enumerate(ordered):
        bit = splitmix64(SEED ^ i) & 1
        (sel if bit == 0 else cert).append(s)
    return sel, cert


def candidate_ps(sel: list) -> list:
    ps = sorted(set(p for p, _ in sel))
    mids = [(ps[i] + ps[i + 1]) / 2 for i in range(len(ps) - 1)]
    return sorted(set(ps + mids))


def compute_isno_line(samples: list, delta: float):
    sel, cert = split_half(samples)
    n_true_sel = sum(1 for _, lb in sel if lb)
    n_false_sel = sum(1 for _, lb in sel if not lb)
    if n_true_sel < NEED or n_false_sel < NEED:
        return None
    cands = candidate_ps(sel)
    upper = []
    for h in cands:
        if h < delta:
            continue
        subset = [(p, lb) for p, lb in sel if p >= h]
        if not subset:
            continue
        k = sum(1 for _, lb in subset if not lb)
        n = len(subset)
        if clopper_pearson_upper(k, n) <= ALPHA:
            upper.append((h, n))
    lower = []
    for l_ in cands:
        if l_ > 1 - delta:
            continue
        subset = [(p, lb) for p, lb in sel if p <= l_]
        if not subset:
            continue
        k = sum(1 for _, lb in subset if lb)
        n = len(subset)
        if clopper_pearson_upper(k, n) <= ALPHA:
            lower.append((l_, n))
    best = None
    best_sum = -1
    for h, nh in upper:
        for l_, nl in lower:
            if l_ + delta <= h - delta:
                s = nh + nl
                if s > best_sum:
                    best_sum = s
                    best = (h, l_)
    if best is None:
        return None
    h, l_ = best
    upper_cert = [(p, lb) for p, lb in cert if p >= h]
    lower_cert = [(p, lb) for p, lb in cert if p <= l_]
    if len(upper_cert) < NEED or len(lower_cert) < NEED:
        return None
    ka = sum(1 for _, lb in upper_cert if not lb)
    kd = sum(1 for _, lb in lower_cert if lb)
    if clopper_pearson_upper(ka, len(upper_cert)) <= ALPHA and \
            clopper_pearson_upper(kd, len(lower_cert)) <= ALPHA:
        hi = min(max(h - delta, 0.0), 1.0)
        lo = min(max(l_ + delta, 0.0), 1.0)
        return {"hi": hi, "lo": lo}
    return None


def compute_score_line(samples: list, delta: float):
    sel, cert = split_half(samples)
    cands = candidate_ps(sel)
    best_h = None
    best_n = -1
    for h in cands:
        if h < delta:
            continue
        subset = [(p, c) for p, c in sel if p >= h]
        n = len(subset)
        if n < NEED:
            continue
        k = sum(1 for _, c in subset if not c)
        if clopper_pearson_upper(k, n) <= ALPHA:
            if n > best_n:
                best_n = n
                best_h = h
    if best_h is None:
        return None
    cert_subset = [(p, c) for p, c in cert if p >= best_h]
    if len(cert_subset) < NEED:
        return None
    k = sum(1 for _, c in cert_subset if not c)
    if clopper_pearson_upper(k, len(cert_subset)) <= ALPHA:
        hi = min(max(best_h - delta, 0.0), 1.0)
        return {"hi": hi, "lo": 0.0}
    return None


def load_labels(path: str) -> dict:
    groups: dict = {}
    with open(path, encoding="utf-8") as fh:
        for line in fh:
            line = line.strip()
            if not line:
                continue
            row = json.loads(line)
            groups.setdefault(row["key"], []).append(row)
    return groups


def compute_lines(labels_by_key: dict, deltas: dict) -> dict:
    lines = {}
    for key in sorted(deltas):
        rows = labels_by_key.get(key, [])
        delta = deltas[key]
        op = rows[0].get("op") if rows else None
        if op in ("select", "measure"):
            samples = [(row["p"], row["pick"] == row["label"]) for row in rows]
            lines[key] = compute_score_line(samples, delta)
        else:
            samples = [(row["p"], bool(row["label"])) for row in rows]
            lines[key] = compute_isno_line(samples, delta)
    return lines


def build_pairs(catalog_a: list, catalog_b: list) -> list:
    pairs = []
    for a in catalog_a:
        for b in catalog_b:
            if abs(a["abv"] - b["abv"]) <= 1.5:
                pairs.append((a, b))
    return pairs


def build_questions() -> list:
    return [
        {"type": "score", "instructions": REL_INSTR, "criteria": REL_CRITERIA},
        {"type": "noul", "instructions": NAME_INSTR},
        {"type": "noul", "instructions": BREWERY_INSTR},
        {"type": "noul", "instructions": STYLE_INSTR},
    ]


def read_isno(answer: dict, thresh: dict) -> str:
    p = answer["noul"]
    if p >= thresh["hi"] + thresh["delta"]:
        return "是"
    if p <= thresh["lo"] - thresh["delta"]:
        return "否"
    return "未决"


def read_level(answer: dict, thresh: dict):
    probs = answer["probabilities"]
    best_idx, best_p = None, -1.0
    for idx_str in ("0", "1", "2"):
        v = probs[idx_str]
        if v > best_p:
            best_p = v
            best_idx = int(idx_str)
    if best_p >= thresh["hi"] + thresh["delta"]:
        return best_idx
    return None


def load_replay_index(path: str) -> dict:
    index = {}
    with open(path, encoding="utf-8") as fh:
        for line in fh:
            line = line.strip()
            if not line:
                continue
            obj = json.loads(line)
            key = (canon(obj["material"]), canon(obj["questions"]), canon(obj.get("over")))
            index[key] = obj["answers"]
    return index


def issue_requests(to_send: list, replay_index, ledger_fh) -> list:
    if not to_send:
        return []
    if replay_index is not None:
        answers_list = []
        for material, questions, over in to_send:
            key = (canon(material), canon(questions), canon(over))
            if key not in replay_index:
                sys.stderr.write(
                    f"replay 缺失读数：材料={material!r} 问题="
                    f"{[q['instructions'] for q in questions]!r}\n"
                )
                sys.exit(1)
            answers_list.append(replay_index[key])
        return answers_list
    batch_reqs = [(m, q) for m, q, _ in to_send]
    results = judge_batch(batch_reqs)
    if ledger_fh is not None:
        for (material, questions, over), answers in zip(to_send, results):
            line = {
                "material": material,
                "questions": questions,
                "over": over,
                "answers": answers,
            }
            ledger_fh.write(json.dumps(line, sort_keys=True, ensure_ascii=False) + "\n")
    return results


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--ledger-out")
    parser.add_argument("--replay")
    parser.add_argument("--max-calls", type=int, default=None)
    parser.add_argument("--print-lines", action="store_true")
    args = parser.parse_args()

    with open("materials.json", encoding="utf-8") as fh:
        materials = json.load(fh)
    labels_by_key = load_labels("labels.jsonl")
    lines = compute_lines(labels_by_key, materials["deltas"])

    if args.print_lines:
        out = {}
        for key in sorted(materials["deltas"]):
            if lines[key] is None:
                out[key] = {"status": "待真值"}
            else:
                out[key] = lines[key]
        print(json.dumps(out, ensure_ascii=False))
        return

    pending = sorted(k for k in lines if lines[k] is None)
    if pending:
        print(json.dumps({"status": "待真值", "key": pending[0]}, ensure_ascii=False))
        return

    thresholds = {
        key: {"hi": lines[key]["hi"], "lo": lines[key]["lo"], "delta": materials["deltas"][key]}
        for key in lines
    }

    pairs = build_pairs(materials["catalog_a"], materials["catalog_b"])
    questions = build_questions()

    n_send = len(pairs) if args.max_calls is None else min(len(pairs), args.max_calls)
    to_send = [({"a": a, "b": b}, questions, None) for a, b in pairs[:n_send]]

    replay_index = load_replay_index(args.replay) if args.replay else None
    ledger_fh = open(args.ledger_out, "a", encoding="utf-8") if args.ledger_out else None
    try:
        results = issue_requests(to_send, replay_index, ledger_fh)
    finally:
        if ledger_fh is not None:
            ledger_fh.close()

    tally = {"sameAs": 0, "curator": 0, "unlinked": 0}
    routed = []
    for (a, b), answers in zip(pairs[:n_send], results):
        rel_answer, name_answer, brewery_answer, style_answer = answers
        level = read_level(rel_answer, thresholds["align-link"])
        outcome = LEVEL_OUTCOME[level] if level is not None else "curator queue"
        if outcome == "assert sameAs":
            tally["sameAs"] += 1
        elif outcome == "curator queue":
            tally["curator"] += 1
        else:
            tally["unlinked"] += 1
        hints = {
            "name": read_isno(name_answer, thresholds["align-name"]),
            "brewery": read_isno(brewery_answer, thresholds["align-brewery"]),
            "style": read_isno(style_answer, thresholds["align-style"]),
        }
        pair = {"a": a["name"], "b": b["name"]}
        routed.append({"pair": pair, "outcome": outcome, "hints": hints})

    unobserved = [{"a": a["name"], "b": b["name"]} for a, b in pairs[n_send:]]

    result = {
        "candidates": len(pairs),
        "tally": tally,
        "routed": routed,
        "unobserved": unobserved,
    }
    print(json.dumps(result, ensure_ascii=False))


if __name__ == "__main__":
    main()
