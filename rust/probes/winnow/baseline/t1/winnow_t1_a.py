"""winnow T1：工具输出逐块筛选，完整职责版（账本重放、预算停机、合批、认证线）。"""
from __future__ import annotations

import argparse
import hashlib
import json
import math
import sys

from jev_interface import judge_batch

ALPHA = 0.1
CONF = 0.1
SEED = 20260923
MASK64 = (1 << 64) - 1
NEED = math.ceil(math.log(CONF) / math.log(1 - ALPHA))

ERROR_KEY = "winnow-error"
TOPIC_KEY = "form-topic"

ERROR_INSTR = "这段工具输出里是否含有报错、失败或异常信息？"


def canon(x) -> str:
    return json.dumps(x, ensure_ascii=False, sort_keys=True)


def splitmix64(x: int) -> int:
    x &= MASK64
    z = (x + 0x9E3779B97F4A7C15) & MASK64
    z = ((z ^ (z >> 30)) * 0xBF58476D1CE4E5B9) & MASK64
    z = ((z ^ (z >> 27)) * 0x94D049BB133111EB) & MASK64
    return (z ^ (z >> 31)) & MASK64


def binom_cdf_le(n: int, k: int, m: float) -> float:
    if m <= 0.0:
        return 1.0
    if m >= 1.0:
        return 1.0 if k >= n else 0.0
    total = 0.0
    for i in range(0, k + 1):
        total += math.comb(n, i) * (m ** i) * ((1 - m) ** (n - i))
    return total


def upper_bound(k: int, n: int) -> float:
    if n == 0 or k >= n:
        return 1.0
    lo, hi = k / n, 1.0
    for _ in range(200):
        mid = (lo + hi) / 2
        if binom_cdf_le(n, k, mid) > CONF:
            lo = mid
        else:
            hi = mid
    return (lo + hi) / 2


def load_labels(path: str) -> dict:
    groups = {}
    op_kinds = {}
    with open(path, encoding="utf-8") as fh:
        for raw in fh:
            raw = raw.strip()
            if not raw:
                continue
            d = json.loads(raw)
            key = d["key"]
            op = d.get("op")
            if op in (None, "test"):
                truth = bool(d["label"])
            else:
                truth = d["pick"] == d["label"]
            groups.setdefault(key, []).append((d["p"], truth))
            kinds = op_kinds.setdefault(key, set())
            kinds.add("two" if op in (None, "test") else "one")
    return groups, op_kinds


def split_halves(samples: list) -> tuple:
    ordered = sorted(samples, key=lambda t: (t[0], t[1]))
    select_half, cert_half = [], []
    for i, sample in enumerate(ordered):
        if splitmix64(SEED ^ i) & 1 == 0:
            select_half.append(sample)
        else:
            cert_half.append(sample)
    return select_half, cert_half


def build_candidates(half: list) -> list:
    ps = sorted(set(p for p, _ in half))
    candidates = set(ps)
    for a, b in zip(ps, ps[1:]):
        candidates.add((a + b) / 2)
    return sorted(candidates)


def two_sided_line(select_half: list, cert_half: list, delta: float):
    n_true = sum(1 for _, t in select_half if t)
    n_false = len(select_half) - n_true
    if n_true < NEED or n_false < NEED:
        return None
    candidates = build_candidates(select_half)
    valid_upper = []
    for h in candidates:
        if h < delta:
            continue
        subset = [(p, t) for p, t in select_half if p >= h]
        if not subset:
            continue
        n = len(subset)
        k = sum(1 for _, t in subset if not t)
        if upper_bound(k, n) <= ALPHA:
            valid_upper.append((h, n))
    valid_lower = []
    for l in candidates:
        if l > 1 - delta:
            continue
        subset = [(p, t) for p, t in select_half if p <= l]
        if not subset:
            continue
        n = len(subset)
        k = sum(1 for _, t in subset if t)
        if upper_bound(k, n) <= ALPHA:
            valid_lower.append((l, n))
    valid_upper.sort(key=lambda x: x[0])
    valid_lower.sort(key=lambda x: x[0])
    best = None
    for h, nh in valid_upper:
        for l, nl in valid_lower:
            if l + delta <= h - delta:
                s = nh + nl
                if best is None or s > best[0]:
                    best = (s, h, l)
    if best is None:
        return None
    _, h, l = best
    upper_cert = [(p, t) for p, t in cert_half if p >= h]
    lower_cert = [(p, t) for p, t in cert_half if p <= l]
    if len(upper_cert) < NEED or len(lower_cert) < NEED:
        return None
    ka = sum(1 for _, t in upper_cert if not t)
    kd = sum(1 for _, t in lower_cert if t)
    if upper_bound(ka, len(upper_cert)) <= ALPHA and upper_bound(kd, len(lower_cert)) <= ALPHA:
        hi = min(1.0, max(0.0, h - delta))
        lo = min(1.0, max(0.0, l + delta))
        return {"hi": hi, "lo": lo}
    return None


def one_sided_line(select_half: list, cert_half: list, delta: float):
    candidates = build_candidates(select_half)
    best = None
    for h in candidates:
        if h < delta:
            continue
        subset = [(p, t) for p, t in select_half if p >= h]
        n = len(subset)
        if n < NEED:
            continue
        k = sum(1 for _, t in subset if not t)
        if upper_bound(k, n) <= ALPHA:
            if best is None or n > best[0] or (n == best[0] and h < best[1]):
                best = (n, h)
    if best is None:
        return None
    _, h = best
    upper_cert = [(p, t) for p, t in cert_half if p >= h]
    if len(upper_cert) < NEED:
        return None
    ka = sum(1 for _, t in upper_cert if not t)
    if upper_bound(ka, len(upper_cert)) <= ALPHA:
        hi = min(1.0, max(0.0, h - delta))
        return {"hi": hi, "lo": 0.0}
    return None


def compute_lines(groups: dict, op_kinds: dict, deltas: dict) -> dict:
    lines = {}
    for key in sorted(groups.keys()):
        samples = groups[key]
        delta = deltas[key]
        select_half, cert_half = split_halves(samples)
        kinds = op_kinds.get(key, set())
        if "one" in kinds:
            line = one_sided_line(select_half, cert_half, delta)
        else:
            line = two_sided_line(select_half, cert_half, delta)
        lines[key] = line
    return lines


def classify(p: float, thr: dict) -> str:
    hi, lo, delta = thr["hi"], thr["lo"], thr["delta"]
    if p >= hi + delta:
        return "是"
    if p <= lo - delta:
        return "否"
    return "未决"


def hash_text(text: str) -> str:
    return hashlib.sha256(text.encode("utf-8")).hexdigest()[:16]


class Dispatcher:
    def __init__(self, ledger_file, replay_index, max_calls):
        self.ledger_file = ledger_file
        self.replay_index = replay_index
        self.max_calls = max_calls
        self.used = 0

    def dispatch(self, requests: list) -> list:
        if not requests:
            return []
        if self.max_calls is None:
            to_send = requests
        else:
            remaining = max(0, self.max_calls - self.used)
            to_send = requests[:remaining]
        if not to_send:
            return []
        if self.replay_index is not None:
            answers_list = []
            for material, questions, over in to_send:
                key = (canon(material), canon(questions), canon(over))
                if key not in self.replay_index:
                    sys.stderr.write(
                        "replay 缺失读数：材料=" + canon(material)[:200]
                        + " 题=" + canon(questions) + " over=" + canon(over) + "\n")
                    sys.exit(1)
                answers_list.append(self.replay_index[key])
        else:
            answers_list = judge_batch([(m, qs, ov) for m, qs, ov in to_send])
        self.used += len(to_send)
        if self.ledger_file is not None:
            for (material, questions, over), answers in zip(to_send, answers_list):
                line_obj = {"material": material, "questions": questions,
                            "over": over, "answers": answers}
                self.ledger_file.write(
                    json.dumps(line_obj, sort_keys=True, ensure_ascii=False) + "\n")
            self.ledger_file.flush()
        return answers_list


def load_replay_index(path: str) -> dict:
    index = {}
    with open(path, encoding="utf-8") as fh:
        for raw in fh:
            raw = raw.strip()
            if not raw:
                continue
            entry = json.loads(raw)
            key = (canon(entry["material"]), canon(entry["questions"]), canon(entry["over"]))
            index[key] = entry["answers"]
    return index


def process_result(res: dict, task: str, thresholds: dict, dispatcher: Dispatcher) -> dict:
    rid = res["id"]
    chunks = res["chunks"]
    k = len(chunks)
    error_material = "\n".join(chunks)
    error_q = {"type": "noul", "instructions": ERROR_INSTR}
    rel_instr = f"这段话的内容是否与「{task}」这个话题相关？"
    rel_q = {"type": "noul", "instructions": rel_instr}
    requests = [(error_material, [error_q], None)]
    for chunk in chunks:
        requests.append((chunk, [rel_q], None))
    answers_list = dispatcher.dispatch(requests)
    m = len(answers_list)
    if m == 0:
        return {
            "id": rid, "reason": "error_unobserved", "pruned": [], "uncertain": [],
            "text": list(chunks), "archive": [], "unobserved": list(range(k)),
        }
    error_p = answers_list[0][0]["noul"]
    error_cls = classify(error_p, thresholds[ERROR_KEY])
    observed_chunk_count = m - 1
    unobserved = list(range(observed_chunk_count, k))
    if error_cls == "是":
        return {
            "id": rid, "reason": "error_present", "pruned": [], "uncertain": [],
            "text": list(chunks), "archive": [], "unobserved": unobserved,
        }
    if error_cls == "未决":
        return {
            "id": rid, "reason": "error_unsure", "pruned": [], "uncertain": [],
            "text": list(chunks), "archive": [], "unobserved": unobserved,
        }
    candidate_idx = []
    uncertain_idx = []
    for j in range(observed_chunk_count):
        chunk_p = answers_list[1 + j][0]["noul"]
        cls = classify(chunk_p, thresholds[TOPIC_KEY])
        if cls == "否":
            candidate_idx.append(j)
        elif cls == "未决":
            uncertain_idx.append(j)
    candidate_chars = sum(len(chunks[j]) for j in candidate_idx)
    total_chars = sum(len(c) for c in chunks)
    if candidate_chars * 100 < total_chars * 20:
        return {
            "id": rid, "reason": "below_min_prune_ratio", "pruned": [], "uncertain": [],
            "text": list(chunks), "archive": [], "unobserved": unobserved,
        }
    pruned = sorted(candidate_idx)
    uncertain = sorted(uncertain_idx)
    candidate_set = set(candidate_idx)
    text = []
    archive = []
    for j, chunk in enumerate(chunks):
        if j in candidate_set:
            h = hash_text(chunk)
            text.append(f"[winnow 已折叠 {len(chunk)} 字 · 展开键 {h}]")
            archive.append({"key": h, "index": j, "text": chunk})
        else:
            text.append(chunk)
    return {
        "id": rid, "reason": "pruned", "pruned": pruned, "uncertain": uncertain,
        "text": text, "archive": archive, "unobserved": unobserved,
    }


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--ledger-out")
    parser.add_argument("--replay")
    parser.add_argument("--max-calls", type=int, default=None)
    parser.add_argument("--print-lines", action="store_true")
    args = parser.parse_args()

    with open("materials.json", encoding="utf-8") as fh:
        materials = json.load(fh)
    task = materials["tool_outputs"]["task"]
    results_in = materials["tool_outputs"]["results"]
    deltas = materials["deltas"]

    groups, op_kinds = load_labels("labels.jsonl")
    lines = compute_lines(groups, op_kinds, deltas)

    if args.print_lines:
        out = {}
        for key in sorted(lines.keys()):
            line = lines[key]
            out[key] = {"hi": line["hi"], "lo": line["lo"]} if line else {"status": "待真值"}
        print(json.dumps(out, ensure_ascii=False))
        return

    undetermined = sorted(key for key, line in lines.items() if line is None)
    if undetermined:
        print(json.dumps({"status": "待真值", "key": undetermined[0]}, ensure_ascii=False))
        return

    thresholds = {}
    for key, line in lines.items():
        thresholds[key] = {"hi": line["hi"], "lo": line["lo"], "delta": deltas[key]}

    ledger_file = None
    if args.ledger_out:
        ledger_file = open(args.ledger_out, "w", encoding="utf-8")
    replay_index = None
    if args.replay:
        replay_index = load_replay_index(args.replay)

    dispatcher = Dispatcher(ledger_file, replay_index, args.max_calls)
    try:
        results_out = []
        for res in results_in:
            results_out.append(process_result(res, task, thresholds, dispatcher))
        output = {"task": task, "results": results_out}
        print(json.dumps(output, ensure_ascii=False))
    finally:
        if ledger_file is not None:
            ledger_file.close()


if __name__ == "__main__":
    main()
