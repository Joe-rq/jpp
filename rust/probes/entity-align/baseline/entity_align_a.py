import json
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from jev_interface import judge

LINK_CRITERIA = [
    "它们描述的是两种不同的产品。",
    "它们描述的是密切相关、可能是也可能不是同一款的产品：变体、特别版，或名字两种理解都说得通。",
    "它们描述的是同一款产品。",
]
LINK_OUTCOMES = ["leave unlinked", "curator queue", "assert sameAs"]
LINK_TALLY_KEYS = ["unlinked", "curator", "sameAs"]


def decide_bool(p, thr):
    if p >= thr["hi"] + thr["delta"]:
        return "是"
    if p <= thr["lo"] - thr["delta"]:
        return "否"
    return "未决"


def decide_score(probabilities, thr):
    values = [probabilities[str(i)] for i in range(len(LINK_CRITERIA))]
    best_i = 0
    best_v = values[0]
    for i in range(1, len(values)):
        if values[i] > best_v:
            best_v = values[i]
            best_i = i
    if best_v >= thr["hi"] + thr["delta"]:
        return best_i
    return None


def main():
    with open("materials.json", encoding="utf-8") as fh:
        materials = json.load(fh)

    catalog_a = materials["catalog_a"]
    catalog_b = materials["catalog_b"]
    thresholds = materials["thresholds"]

    pairs = []
    for a in catalog_a:
        for b in catalog_b:
            if abs(a["abv"] - b["abv"]) <= 1.5:
                pairs.append((a, b))

    tally = {"sameAs": 0, "curator": 0, "unlinked": 0}
    routed = []

    for a, b in pairs:
        material = {"a": a, "b": b}
        answers = judge(material, [
            {"type": "score", "instructions": "两条实体描述作为产品是什么关系？", "criteria": LINK_CRITERIA},
            {"type": "noul", "instructions": "两条实体写的是同一个啤酒名吗？"},
            {"type": "noul", "instructions": "两条实体来自同一家酒厂吗？"},
            {"type": "noul", "instructions": "两条实体描述的是同一种啤酒风格吗？"},
        ])

        link_idx = decide_score(answers[0]["probabilities"], thresholds["align-link"])
        if link_idx is None:
            outcome = LINK_OUTCOMES[1]
            tally_key = LINK_TALLY_KEYS[1]
        else:
            outcome = LINK_OUTCOMES[link_idx]
            tally_key = LINK_TALLY_KEYS[link_idx]
        tally[tally_key] += 1

        name_hint = decide_bool(answers[1]["noul"], thresholds["align-name"])
        brewery_hint = decide_bool(answers[2]["noul"], thresholds["align-brewery"])
        style_hint = decide_bool(answers[3]["noul"], thresholds["align-style"])

        routed.append({
            "pair": {"a": a["name"], "b": b["name"]},
            "outcome": outcome,
            "hints": {"name": name_hint, "brewery": brewery_hint, "style": style_hint},
        })

    result = {
        "candidates": len(pairs),
        "tally": tally,
        "routed": routed,
    }
    print(json.dumps(result, ensure_ascii=False))


if __name__ == "__main__":
    main()
