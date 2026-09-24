import argparse
import json
import math
import sys

from jev_interface import judge_batch

ALPHA = 0.1
CONF_C = 0.1
SEED = 20260923
NEED = math.ceil(math.log(CONF_C) / math.log(1 - ALPHA))
MASK64 = (1 << 64) - 1

QUESTION_TEXT = "面试官 b 的专长是否覆盖候选人 a 的技术方向，足以对其进行技术面试？"


def splitmix64(x):
    z = (x + 0x9E3779B97F4A7C15) & MASK64
    z = ((z ^ (z >> 30)) * 0xBF58476D1CE4E5B9) & MASK64
    z = ((z ^ (z >> 27)) * 0x94D049BB133111EB) & MASK64
    return (z ^ (z >> 31)) & MASK64


def binom_cdf_le(k, n, m):
    total = 0.0
    for i in range(k + 1):
        total += math.comb(n, i) * (m ** i) * ((1 - m) ** (n - i))
    return total


def clopper_pearson_upper(k, n, c):
    if n == 0 or k >= n:
        return 1.0
    lo, hi = k / n, 1.0
    for _ in range(200):
        mid = (lo + hi) / 2
        p = binom_cdf_le(k, n, mid)
        if p > c:
            lo = mid
        else:
            hi = mid
    return (lo + hi) / 2


def load_materials():
    with open("materials.json", encoding="utf-8") as fh:
        return json.load(fh)


def load_samples():
    groups = {}
    with open("labels.jsonl", encoding="utf-8") as fh:
        for line in fh:
            line = line.strip()
            if not line:
                continue
            row = json.loads(line)
            groups.setdefault(row["key"], []).append((row["p"], bool(row["label"])))
    return groups


def compute_line_test(samples, delta):
    sorted_samples = sorted(samples)
    select_half = []
    cert_half = []
    for rank, sample in enumerate(sorted_samples):
        z = splitmix64(SEED ^ rank)
        (select_half if z & 1 == 0 else cert_half).append(sample)

    select_true = sum(1 for _, label in select_half if label)
    select_false = len(select_half) - select_true
    if select_true < NEED or select_false < NEED:
        return {"status": "待真值"}

    ps = sorted(set(p for p, _ in select_half))
    cand_vals = set(ps)
    for a, b in zip(ps, ps[1:]):
        cand_vals.add((a + b) / 2)
    cand_vals = sorted(cand_vals)

    upper = []
    for h in cand_vals:
        if h < delta:
            continue
        subset = [s for s in select_half if s[0] >= h]
        if not subset:
            continue
        n = len(subset)
        k = sum(1 for _, label in subset if not label)
        if clopper_pearson_upper(k, n, ALPHA) <= ALPHA:
            upper.append((h, n))

    lower = []
    for l in cand_vals:
        if l > 1 - delta:
            continue
        subset = [s for s in select_half if s[0] <= l]
        if not subset:
            continue
        n = len(subset)
        k = sum(1 for _, label in subset if label)
        if clopper_pearson_upper(k, n, ALPHA) <= ALPHA:
            lower.append((l, n))

    best = None
    best_sum = -1
    for h, nh in upper:
        for l, nl in lower:
            if l + delta <= h - delta:
                total = nh + nl
                if total > best_sum:
                    best_sum = total
                    best = (h, l)
    if best is None:
        return {"status": "待真值"}
    h, l = best

    cert_upper = [s for s in cert_half if s[0] >= h]
    cert_lower = [s for s in cert_half if s[0] <= l]
    if len(cert_upper) < NEED or len(cert_lower) < NEED:
        return {"status": "待真值"}
    ka = sum(1 for _, label in cert_upper if not label)
    kd = sum(1 for _, label in cert_lower if label)
    ua = clopper_pearson_upper(ka, len(cert_upper), ALPHA)
    ud = clopper_pearson_upper(kd, len(cert_lower), ALPHA)
    if not (ua <= ALPHA and ud <= ALPHA):
        return {"status": "待真值"}

    hi = min(1.0, max(0.0, h - delta))
    lo = min(1.0, max(0.0, l + delta))
    return {"hi": hi, "lo": lo}


def compute_lines(materials):
    groups = load_samples()
    lines = {}
    pending = []
    for key in sorted(materials["deltas"].keys()):
        delta = materials["deltas"][key]
        samples = groups.get(key, [])
        line = compute_line_test(samples, delta)
        lines[key] = line
        if "status" in line:
            pending.append(key)
    return lines, pending


def build_pairs(materials):
    pairs = []
    for candidate in materials["candidates"]:
        for interviewer in materials["interviewers"]:
            if candidate["level"] in interviewer["levels"]:
                pairs.append((candidate, interviewer))
    return pairs


def make_question():
    return {"type": "noul", "instructions": QUESTION_TEXT}


def canon(x):
    return json.dumps(x, ensure_ascii=False, sort_keys=True)


def load_ledger_map(path):
    idx = {}
    with open(path, encoding="utf-8") as fh:
        for line in fh:
            line = line.strip()
            if not line:
                continue
            row = json.loads(line)
            key = (canon(row["material"]), canon(row["questions"]), canon(row.get("over")))
            idx[key] = row["answers"]
    return idx


def replay_lookup(idx, material, questions, over):
    key = (canon(material), canon(questions), canon(over))
    if key not in idx:
        sys.stderr.write(
            f"账本里找不到这道判断：题={questions!r} 材料={material!r}\n"
        )
        sys.exit(1)
    return idx[key]


def run_asks(requests, replay_path, ledger_path):
    if replay_path:
        idx = load_ledger_map(replay_path)
        return [replay_lookup(idx, m, q, o) for m, q, o in requests]

    answers_list = judge_batch(requests) if requests else []
    if ledger_path:
        with open(ledger_path, "a", encoding="utf-8") as fh:
            for (material, questions, over), answers in zip(requests, answers_list):
                row = {
                    "material": material,
                    "questions": questions,
                    "over": over,
                    "answers": answers,
                }
                fh.write(json.dumps(row, sort_keys=True, ensure_ascii=False) + "\n")
    return answers_list


def classify(p, hi, lo, delta):
    if p >= hi + delta:
        return "是"
    if p <= lo - delta:
        return "否"
    return "未决"


def assemble_output(pairs, included_pairs, answers_list, hi, lo, delta, materials):
    plan = []
    review = []
    rejected = []
    assigned = set()
    interviewer_counts = {}

    for (candidate, interviewer), answers in zip(included_pairs, answers_list):
        p = answers[0]["noul"]
        verdict = classify(p, hi, lo, delta)
        entry = {"candidate": candidate["name"], "interviewer": interviewer["name"]}
        if verdict == "是":
            if (candidate["name"] not in assigned
                    and interviewer_counts.get(interviewer["name"], 0) < 2):
                plan.append(entry)
                assigned.add(candidate["name"])
                interviewer_counts[interviewer["name"]] = (
                    interviewer_counts.get(interviewer["name"], 0) + 1
                )
        elif verdict == "未决":
            review.append(entry)
        else:
            rejected.append(entry)

    unplaced = [c["name"] for c in materials["candidates"] if c["name"] not in assigned]
    unobserved_pairs = pairs[len(included_pairs):]
    unobserved = [
        {"candidate": c["name"], "interviewer": iv["name"]} for c, iv in unobserved_pairs
    ]

    return {
        "pairs_asked": len(pairs),
        "plan": plan,
        "unplaced": unplaced,
        "review": review,
        "rejected": rejected,
        "unobserved": unobserved,
    }


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--ledger-out")
    parser.add_argument("--replay")
    parser.add_argument("--max-calls", type=int, default=None)
    parser.add_argument("--print-lines", action="store_true")
    args = parser.parse_args()

    materials = load_materials()
    lines, pending = compute_lines(materials)

    if args.print_lines:
        output = {}
        for key, line in lines.items():
            output[key] = {"status": "待真值"} if "status" in line else line
        print(json.dumps(output, ensure_ascii=False))
        return

    if pending:
        print(json.dumps({"status": "待真值", "key": sorted(pending)[0]}, ensure_ascii=False))
        return

    delta = materials["deltas"]["iv-cover"]
    hi = lines["iv-cover"]["hi"]
    lo = lines["iv-cover"]["lo"]

    pairs = build_pairs(materials)
    question = make_question()

    if args.max_calls is not None:
        included_pairs = pairs[: args.max_calls]
    else:
        included_pairs = pairs

    requests = [
        ({"a": candidate, "b": interviewer}, [question], None)
        for candidate, interviewer in included_pairs
    ]
    answers_list = run_asks(requests, args.replay, args.ledger_out)

    result = assemble_output(pairs, included_pairs, answers_list, hi, lo, delta, materials)
    print(json.dumps(result, ensure_ascii=False))


if __name__ == "__main__":
    main()
