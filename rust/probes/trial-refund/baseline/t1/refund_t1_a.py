import argparse
import json
import math
import sys

from jev_interface import judge_batch

ALPHA = 0.1
CONF_C = 0.1
SEED = 20260923
MASK64 = 0xFFFFFFFFFFFFFFFF
NEED = math.ceil(math.log(CONF_C) / math.log(1 - ALPHA))

REFUND_Q = {
    "type": "noul",
    "instructions": "这段客服对话里，顾客是否明确提出要退款或退钱？",
}
HEATED_Q = {
    "type": "noul",
    "instructions": (
        "这段客服对话里，顾客的措辞是否情绪激烈（例如愤怒、威胁投诉或曝光、连续质问）？"
    ),
}


def canon(x):
    return json.dumps(x, sort_keys=True, ensure_ascii=False)


def splitmix64(x):
    x &= MASK64
    z = (x + 0x9E3779B97F4A7C15) & MASK64
    z = ((z ^ (z >> 30)) * 0xBF58476D1CE4E5B9) & MASK64
    z = ((z ^ (z >> 27)) * 0x94D049BB133111EB) & MASK64
    return z ^ (z >> 31)


def binom_cdf_le(n, p, k):
    total = 0.0
    for i in range(k + 1):
        total += math.comb(n, i) * (p ** i) * ((1 - p) ** (n - i))
    return total


def upper_bound(k, n):
    if n == 0 or k >= n:
        return 1.0
    lo, hi = k / n, 1.0
    for _ in range(200):
        mid = (lo + hi) / 2
        if binom_cdf_le(n, mid, k) > CONF_C:
            lo = mid
        else:
            hi = mid
    return (lo + hi) / 2


def load_labels(path):
    by_key = {}
    with open(path, encoding="utf-8") as fh:
        for line in fh:
            line = line.strip()
            if not line:
                continue
            rec = json.loads(line)
            by_key.setdefault(rec["key"], []).append(rec)
    return by_key


def noul_samples(records):
    samples = []
    for r in records:
        op = r.get("op")
        if op is not None and op != "test":
            continue
        samples.append((r["p"], bool(r["label"])))
    return samples


def split_halves(samples):
    ordered = sorted(samples, key=lambda s: (s[0], s[1]))
    sel, cert = [], []
    for i, s in enumerate(ordered):
        h = splitmix64(SEED ^ i)
        if h & 1 == 0:
            sel.append(s)
        else:
            cert.append(s)
    return sel, cert


def candidate_thresholds(sel):
    ps = sorted(set(p for p, _ in sel))
    cands = set(ps)
    for a, b in zip(ps, ps[1:]):
        cands.add((a + b) / 2)
    return sorted(cands)


def certify_key(samples, delta):
    sel, cert = split_halves(samples)
    sel_true = sum(1 for _, lbl in sel if lbl)
    sel_false = len(sel) - sel_true
    if sel_true < NEED or sel_false < NEED:
        return None

    cands = candidate_thresholds(sel)

    upper_candidates = []
    for h in cands:
        if h < delta:
            continue
        subset = [(p, lbl) for p, lbl in sel if p >= h]
        if not subset:
            continue
        n = len(subset)
        k = sum(1 for _, lbl in subset if not lbl)
        if upper_bound(k, n) <= ALPHA:
            upper_candidates.append((h, n))

    lower_candidates = []
    for l in cands:
        if l > 1 - delta:
            continue
        subset = [(p, lbl) for p, lbl in sel if p <= l]
        if not subset:
            continue
        n = len(subset)
        k = sum(1 for _, lbl in subset if lbl)
        if upper_bound(k, n) <= ALPHA:
            lower_candidates.append((l, n))

    best = None
    best_sum = -1
    for h, nh in upper_candidates:
        for l, nl in lower_candidates:
            if l + delta > h - delta:
                continue
            total = nh + nl
            if total > best_sum:
                best_sum = total
                best = (h, l)
    if best is None:
        return None
    h, l = best

    cert_upper = [(p, lbl) for p, lbl in cert if p >= h]
    cert_lower = [(p, lbl) for p, lbl in cert if p <= l]
    if len(cert_upper) < NEED or len(cert_lower) < NEED:
        return None
    ka = sum(1 for _, lbl in cert_upper if not lbl)
    kd = sum(1 for _, lbl in cert_lower if lbl)
    if upper_bound(ka, len(cert_upper)) > ALPHA:
        return None
    if upper_bound(kd, len(cert_lower)) > ALPHA:
        return None

    hi = min(1.0, max(0.0, h - delta))
    lo = min(1.0, max(0.0, l + delta))
    return {"hi": hi, "lo": lo}


def compute_lines(labels_by_key, deltas):
    lines = {}
    for key in sorted(deltas.keys()):
        records = labels_by_key.get(key, [])
        samples = noul_samples(records)
        result = certify_key(samples, deltas[key])
        lines[key] = result if result is not None else {"status": "待真值"}
    return lines


def load_replay(path):
    index = {}
    with open(path, encoding="utf-8") as fh:
        for line in fh:
            line = line.strip()
            if not line:
                continue
            rec = json.loads(line)
            key = (canon(rec["material"]), canon(rec["questions"]), canon(rec.get("over")))
            index[key] = rec["answers"]
    return index


def run_batch(requests, ledger_fh, replay_index):
    if replay_index is not None:
        answers_list = []
        for material, questions, over in requests:
            key = (canon(material), canon(questions), canon(over))
            if key not in replay_index:
                sys.stderr.write(
                    f"replay 取不到读数：题={[q['instructions'] for q in questions]!r} "
                    f"材料={canon(material)[:80]}…\n"
                )
                sys.exit(1)
            answers_list.append(replay_index[key])
        return answers_list
    if not requests:
        return []
    results = judge_batch(requests)
    if ledger_fh is not None:
        for (material, questions, over), answers in zip(requests, results):
            entry = {"material": material, "questions": questions, "over": over, "answers": answers}
            ledger_fh.write(json.dumps(entry, sort_keys=True, ensure_ascii=False) + "\n")
    return results


def decide(p, hi, lo, delta):
    if p >= hi + delta:
        return "是"
    if p <= lo - delta:
        return "否"
    return "未决"


def parse_args():
    parser = argparse.ArgumentParser()
    parser.add_argument("--ledger-out")
    parser.add_argument("--replay")
    parser.add_argument("--max-calls", type=int)
    parser.add_argument("--print-lines", action="store_true")
    return parser.parse_args()


def main():
    args = parse_args()

    with open("materials.json", encoding="utf-8") as fh:
        materials = json.load(fh)
    chats = materials["chats"]
    deltas = materials["deltas"]

    labels_by_key = load_labels("labels.jsonl")
    lines = compute_lines(labels_by_key, deltas)

    if args.print_lines:
        print(json.dumps(lines, ensure_ascii=False))
        return

    bad = sorted(k for k, v in lines.items() if v.get("status") == "待真值")
    if bad:
        print(json.dumps({"status": "待真值", "key": bad[0]}, ensure_ascii=False))
        return

    replay_index = load_replay(args.replay) if args.replay else None

    ledger_fh = open(args.ledger_out, "a", encoding="utf-8") if args.ledger_out else None

    remaining = args.max_calls

    def take(items):
        nonlocal remaining
        if remaining is None:
            return items, []
        n = max(0, remaining)
        taken = items[:n]
        left = items[len(taken):]
        remaining -= len(taken)
        return taken, left

    n_chats = len(chats)
    layer1_idx = list(range(n_chats))
    layer1_take, layer1_skip = take(layer1_idx)

    requests1 = [(chats[i], [REFUND_Q], None) for i in layer1_take]
    results1 = run_batch(requests1, ledger_fh, replay_index)

    refund_hi = lines["cs-refund"]["hi"]
    refund_lo = lines["cs-refund"]["lo"]
    refund_delta = deltas["cs-refund"]

    refund_decision = {}
    for i, res in zip(layer1_take, results1):
        p = res[0]["noul"]
        refund_decision[i] = decide(p, refund_hi, refund_lo, refund_delta)

    unobserved = set(layer1_skip)

    layer2_candidates = [i for i in layer1_take if refund_decision[i] == "是"]
    layer2_take, layer2_skip = take(layer2_candidates)
    unobserved.update(layer2_skip)

    requests2 = [(chats[i], [HEATED_Q], None) for i in layer2_take]
    results2 = run_batch(requests2, ledger_fh, replay_index)

    heated_hi = lines["cs-heated"]["hi"]
    heated_lo = lines["cs-heated"]["lo"]
    heated_delta = deltas["cs-heated"]

    heated_decision = {}
    for i, res in zip(layer2_take, results2):
        p = res[0]["noul"]
        heated_decision[i] = decide(p, heated_hi, heated_lo, heated_delta)

    urgent, refund_calm, no_refund, review = [], [], [], []
    for i in range(n_chats):
        if i in unobserved:
            continue
        rd = refund_decision[i]
        if rd == "否":
            no_refund.append(i)
        elif rd == "未决":
            review.append(i)
        else:
            hd = heated_decision[i]
            if hd == "是":
                urgent.append(i)
            elif hd == "否":
                refund_calm.append(i)
            else:
                review.append(i)

    unobserved_sorted = sorted(unobserved)
    lower = len(urgent)
    upper = len(urgent) + len(review) + len(unobserved_sorted)

    output = {
        "urgent": urgent,
        "refund_calm": refund_calm,
        "no_refund": no_refund,
        "review": review,
        "urgent_count": [lower, upper],
        "unobserved": unobserved_sorted,
    }
    print(json.dumps(output, ensure_ascii=False))

    if ledger_fh is not None:
        ledger_fh.close()


if __name__ == "__main__":
    main()
