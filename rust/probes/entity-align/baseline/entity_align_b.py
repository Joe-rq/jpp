import json

from jev_interface import judge

RELATION_INSTRUCTIONS = "两条实体描述作为产品是什么关系？"
RELATION_CRITERIA = [
    "它们描述的是两种不同的产品。",
    "它们描述的是密切相关、可能是也可能不是同一款的产品：变体、特别版，或名字两种理解都说得通。",
    "它们描述的是同一款产品。",
]
NAME_INSTRUCTIONS = "两条实体写的是同一个啤酒名吗？"
BREWERY_INSTRUCTIONS = "两条实体来自同一家酒厂吗？"
STYLE_INSTRUCTIONS = "两条实体描述的是同一种啤酒风格吗？"

RELATION_OUTCOMES = ["leave unlinked", "curator queue", "assert sameAs"]
TALLY_KEYS = ["unlinked", "curator", "sameAs"]


def load_materials():
    with open("materials.json", encoding="utf-8") as fh:
        return json.load(fh)


def build_pairs(catalog_a, catalog_b):
    pairs = []
    for a in catalog_a:
        for b in catalog_b:
            if abs(a["abv"] - b["abv"]) <= 1.5:
                pairs.append((a, b))
    return pairs


def route_score(probabilities, threshold):
    ranked = sorted(probabilities.items(), key=lambda kv: int(kv[0]))
    best_idx, best_p = None, -1.0
    for key, p in ranked:
        if p > best_p:
            best_p = p
            best_idx = int(key)
    if best_p >= threshold["hi"] + threshold["delta"]:
        return best_idx
    return None


def route_noul(p, threshold):
    if p >= threshold["hi"] + threshold["delta"]:
        return "是"
    if p <= threshold["lo"] - threshold["delta"]:
        return "否"
    return "未决"


def main():
    materials = load_materials()
    catalog_a = materials["catalog_a"]
    catalog_b = materials["catalog_b"]
    thresholds = materials["thresholds"]

    pairs = build_pairs(catalog_a, catalog_b)

    tally = {"sameAs": 0, "curator": 0, "unlinked": 0}
    routed = []

    for a, b in pairs:
        material = {"a": a, "b": b}
        answers = judge(material, [
            {
                "type": "score",
                "instructions": RELATION_INSTRUCTIONS,
                "criteria": RELATION_CRITERIA,
            },
            {"type": "noul", "instructions": NAME_INSTRUCTIONS},
            {"type": "noul", "instructions": BREWERY_INSTRUCTIONS},
            {"type": "noul", "instructions": STYLE_INSTRUCTIONS},
        ])
        relation_answer, name_answer, brewery_answer, style_answer = answers

        tier = route_score(relation_answer["probabilities"], thresholds["align-link"])
        if tier is None:
            outcome_idx = 1
        else:
            outcome_idx = tier
        outcome = RELATION_OUTCOMES[outcome_idx]
        tally[TALLY_KEYS[outcome_idx]] += 1

        hints = {
            "name": route_noul(name_answer["noul"], thresholds["align-name"]),
            "brewery": route_noul(brewery_answer["noul"], thresholds["align-brewery"]),
            "style": route_noul(style_answer["noul"], thresholds["align-style"]),
        }

        routed.append({
            "pair": {"a": a["name"], "b": b["name"]},
            "outcome": outcome,
            "hints": hints,
        })

    result = {
        "candidates": len(pairs),
        "tally": tally,
        "routed": routed,
    }
    print(json.dumps(result, ensure_ascii=False))


if __name__ == "__main__":
    main()
