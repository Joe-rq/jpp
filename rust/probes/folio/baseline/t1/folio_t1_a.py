"""folio T1：法律本体逐层下探分类，完整职责版（账本重放、预算停机、合批、认证线）。"""
from __future__ import annotations

import argparse
import json
import math
import sys

from jev_interface import calls, judge

ALPHA = 0.1
CONF_C = 0.1
SEED = 20260923
MASK64 = (1 << 64) - 1
NEED = math.ceil(math.log(CONF_C) / math.log(1 - ALPHA))

FOLIO_LEVEL_INSTRUCTIONS = "这份法律文书最应归入下列哪一类？"


def _canon(x) -> str:
    return json.dumps(x, ensure_ascii=False, sort_keys=True)


def splitmix64(x: int) -> int:
    x &= MASK64
    z = (x + 0x9E3779B97F4A7C15) & MASK64
    z = ((z ^ (z >> 30)) * 0xBF58476D1CE4E5B9) & MASK64
    z = ((z ^ (z >> 27)) * 0x94D049BB133111EB) & MASK64
    return z ^ (z >> 31)


def binom_cdf(k: int, n: int, p: float) -> float:
    total = 0.0
    for i in range(k + 1):
        total += math.comb(n, i) * (p ** i) * ((1 - p) ** (n - i))
    return total


def upper_bound(k: int, n: int) -> float:
    if n == 0 or k >= n:
        return 1.0
    lo, hi = k / n, 1.0
    for _ in range(200):
        mid = (lo + hi) / 2
        if binom_cdf(k, n, mid) > CONF_C:
            lo = mid
        else:
            hi = mid
    return (lo + hi) / 2


def split_halves(samples):
    ordered = sorted(samples, key=lambda s: (s[0], s[1]))
    sel, cert = [], []
    for i, s in enumerate(ordered):
        z = splitmix64(SEED ^ i)
        (sel if (z & 1) == 0 else cert).append(s)
    return sel, cert


def build_candidates(sel):
    ps = sorted({p for p, _ in sel})
    cands = set(ps)
    for a, b in zip(ps, ps[1:]):
        cands.add((a + b) / 2)
    return sorted(cands)


def compute_two_sided(samples, delta):
    sel, cert = split_halves(samples)
    n_true = sum(1 for _, t in sel if t)
    n_false = sum(1 for _, t in sel if not t)
    if n_true < NEED or n_false < NEED:
        return None
    cands = build_candidates(sel)
    upper = []
    for h in cands:
        if h < delta:
            continue
        subset = [t for p, t in sel if p >= h]
        n = len(subset)
        if n == 0:
            continue
        k = sum(1 for t in subset if not t)
        if upper_bound(k, n) <= ALPHA:
            upper.append((h, n))
    lower = []
    for l in cands:
        if l > 1 - delta:
            continue
        subset = [t for p, t in sel if p <= l]
        n = len(subset)
        if n == 0:
            continue
        k = sum(1 for t in subset if t)
        if upper_bound(k, n) <= ALPHA:
            lower.append((l, n))
    best = None
    for h, nh in upper:
        for l, nl in lower:
            if l + delta <= h - delta:
                total = nh + nl
                if best is None or total > best[0]:
                    best = (total, h, l)
    if best is None:
        return None
    _, h, l = best
    ka_subset = [t for p, t in cert if p >= h]
    na = len(ka_subset)
    kd_subset = [t for p, t in cert if p <= l]
    nd = len(kd_subset)
    if na < NEED or nd < NEED:
        return None
    ka = sum(1 for t in ka_subset if not t)
    kd = sum(1 for t in kd_subset if t)
    if upper_bound(ka, na) > ALPHA or upper_bound(kd, nd) > ALPHA:
        return None
    hi = min(1.0, max(0.0, h - delta))
    lo = min(1.0, max(0.0, l + delta))
    return {"hi": hi, "lo": lo, "delta": delta}


def compute_one_sided(samples, delta):
    sel, cert = split_halves(samples)
    cands = build_candidates(sel)
    best = None
    for h in cands:
        if h < delta:
            continue
        subset = [t for p, t in sel if p >= h]
        n = len(subset)
        if n < NEED:
            continue
        k = sum(1 for t in subset if not t)
        if upper_bound(k, n) <= ALPHA:
            if best is None or n > best[0]:
                best = (n, h)
    if best is None:
        return None
    _, h = best
    cert_subset = [t for p, t in cert if p >= h]
    na = len(cert_subset)
    if na < NEED:
        return None
    ka = sum(1 for t in cert_subset if not t)
    if upper_bound(ka, na) > ALPHA:
        return None
    hi = min(1.0, max(0.0, h - delta))
    return {"hi": hi, "lo": 0.0, "delta": delta}


def load_labels(path):
    by_key = {}
    with open(path, encoding="utf-8") as fh:
        for line in fh:
            line = line.strip()
            if not line:
                continue
            row = json.loads(line)
            key = row["key"]
            op = row.get("op")
            if op in (None, "test"):
                truth = bool(row["label"])
            else:
                truth = row["pick"] == row["label"]
            by_key.setdefault(key, []).append((row["p"], truth, op))
    return by_key


def compute_lines(labels_path, deltas):
    by_key = load_labels(labels_path)
    lines = {}
    for key in by_key:
        samples = by_key[key]
        op = samples[0][2]
        delta = deltas[key]
        pairs = [(p, t) for p, t, _ in samples]
        if op in (None, "test"):
            lines[key] = compute_two_sided(pairs, delta)
        else:
            lines[key] = compute_one_sided(pairs, delta)
    return lines


class Ledger:
    def __init__(self, ledger_out, replay_path):
        self.ledger_out = ledger_out
        self.replay_index = None
        if replay_path:
            self.replay_index = {}
            with open(replay_path, encoding="utf-8") as fh:
                for line in fh:
                    line = line.strip()
                    if not line:
                        continue
                    row = json.loads(line)
                    key = (_canon(row["material"]), _canon(row["questions"]),
                           _canon(row.get("over")))
                    self.replay_index[key] = row["answers"]

    def ask(self, material, questions, over=None):
        if self.replay_index is not None:
            key = (_canon(material), _canon(questions), _canon(over))
            if key not in self.replay_index:
                texts = "、".join(q.get("instructions", "") for q in questions)
                sys.stderr.write(
                    "账本里没有这次判断：题=%r 材料=%s\n" % (texts, _canon(material)[:80])
                )
                sys.exit(1)
            return self.replay_index[key]
        answers = judge(material, questions, over)
        if self.ledger_out is not None:
            row = {"material": material, "questions": questions, "over": over,
                   "answers": answers}
            with open(self.ledger_out, "a", encoding="utf-8") as fh:
                fh.write(_canon(row) + "\n")
        return answers


def pick_choice(answer, candidates, hi, delta):
    probs = answer["probabilities"]
    best_idx, best_p = None, -1.0
    for i in range(len(candidates)):
        p = probs[f"c{i}"]
        if p > best_p:
            best_p, best_idx = p, i
    mode_share = answer.get("mode_share")
    if mode_share is not None and mode_share == 1.0 and best_p >= hi + delta:
        return candidates[best_idx]
    return None


def judge_noul(p, hi, lo, delta):
    if p >= hi + delta:
        return True
    if p <= lo - delta:
        return False
    return None


def run_single_path(document, tree, root, line, ledger, max_calls):
    hi, delta = line["hi"], line["delta"]
    current = root
    path = []
    for _ in range(6):
        children = tree.get(current)
        if not children:
            return {"leaf": current, "path": path, "stopped": "leaf", "unobserved": []}
        if max_calls is not None and calls() >= max_calls:
            return {"leaf": current, "path": path, "stopped": "budget",
                     "unobserved": list(children)}
        q = {"type": "choice", "instructions": FOLIO_LEVEL_INSTRUCTIONS}
        answers = ledger.ask(document, [q], over=children)
        picked = pick_choice(answers[0], children, hi, delta)
        if picked is None:
            return {"leaf": current, "path": path, "stopped": "unsure", "unobserved": []}
        current = picked
        path.append(picked)
    return {"leaf": current, "path": path, "stopped": "depth", "unobserved": []}


def run_multi_path(document, tree, root, line, ledger, max_calls):
    hi, lo, delta = line["hi"], line["lo"], line["delta"]
    frontier = [root]
    leaves, dead_ends = [], []
    layers, undecided = 0, 0
    unobserved = []
    for _ in range(6):
        internal = [c for c in frontier if c in tree]
        reached_leaves = [c for c in frontier if c not in tree]
        if not internal:
            leaves.extend(reached_leaves)
            break
        pairs = [(cat, child) for cat in internal for child in tree[cat]]
        if max_calls is not None and calls() >= max_calls:
            unobserved = [child for _, child in pairs]
            break
        qs = [{"type": "noul",
               "instructions": f"这段话的内容是否与「{child}」这个话题相关？"}
              for _, child in pairs]
        answers = ledger.ask(document, qs)
        outcomes = []
        for (cat, child), ans in zip(pairs, answers):
            outcome = judge_noul(ans["noul"], hi, lo, delta)
            outcomes.append((cat, child, outcome))
            if outcome is None:
                undecided += 1
        next_frontier = [child for _, child, outcome in outcomes if outcome is True]
        for cat in internal:
            cat_outcomes = [outcome for c, _, outcome in outcomes if c == cat]
            if not any(o is True for o in cat_outcomes):
                dead_ends.append(cat)
        leaves.extend(reached_leaves)
        layers += 1
        frontier = next_frontier
    return {"leaves": leaves, "dead_ends": dead_ends, "layers": layers,
            "undecided": undecided, "unobserved": unobserved}


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--ledger-out")
    parser.add_argument("--replay")
    parser.add_argument("--max-calls", type=int, default=None)
    parser.add_argument("--print-lines", action="store_true")
    args = parser.parse_args()

    with open("materials.json", encoding="utf-8") as fh:
        materials = json.load(fh)
    root = materials["root"]
    tree = materials["tree"]
    document = materials["document"]
    deltas = materials["deltas"]

    lines = compute_lines("labels.jsonl", deltas)

    if args.print_lines:
        out = {}
        for key, line in lines.items():
            if line is None:
                out[key] = {"status": "待真值"}
            else:
                out[key] = {"hi": line["hi"], "lo": line["lo"]}
        print(json.dumps(out, ensure_ascii=False))
        return

    missing = sorted(key for key, line in lines.items() if line is None)
    if missing:
        print(json.dumps({"status": "待真值", "key": missing[0]}, ensure_ascii=False))
        return

    ledger = Ledger(args.ledger_out, args.replay)

    single_path = run_single_path(
        document, tree, root, lines["folio-level"], ledger, args.max_calls
    )
    multi_path = run_multi_path(
        document, tree, root, lines["form-topic"], ledger, args.max_calls
    )

    result = {"single_path": single_path, "multi_path": multi_path}
    print(json.dumps(result, ensure_ascii=False))


if __name__ == "__main__":
    main()
