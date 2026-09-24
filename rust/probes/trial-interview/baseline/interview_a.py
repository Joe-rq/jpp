import json

from jev_interface import judge


def classify(p, thr):
    if p >= thr["hi"] + thr["delta"]:
        return "是"
    if p <= thr["lo"] - thr["delta"]:
        return "否"
    return "未决"


def main():
    with open("materials.json", encoding="utf-8") as fh:
        materials = json.load(fh)

    candidates = materials["candidates"]
    interviewers = materials["interviewers"]
    thr = materials["thresholds"]["iv-cover"]

    pairs = []
    for c in candidates:
        for iv in interviewers:
            if c["level"] in iv["levels"]:
                pairs.append((c, iv))

    verdicts = []
    for c, iv in pairs:
        material = {"a": c, "b": iv}
        question = {
            "type": "noul",
            "instructions": "面试官 b 的专长是否覆盖候选人 a 的技术方向，足以对其进行技术面试？",
        }
        answer = judge(material, [question])[0]
        verdicts.append(classify(answer["noul"], thr))

    plan = []
    review = []
    rejected = []
    assigned_candidates = set()
    interviewer_load = {iv["name"]: 0 for iv in interviewers}

    for (c, iv), verdict in zip(pairs, verdicts):
        if verdict == "未决":
            review.append({"candidate": c["name"], "interviewer": iv["name"]})
        elif verdict == "否":
            rejected.append({"candidate": c["name"], "interviewer": iv["name"]})

    for (c, iv), verdict in zip(pairs, verdicts):
        if verdict != "是":
            continue
        if c["name"] in assigned_candidates:
            continue
        if interviewer_load[iv["name"]] >= 2:
            continue
        plan.append({"candidate": c["name"], "interviewer": iv["name"]})
        assigned_candidates.add(c["name"])
        interviewer_load[iv["name"]] += 1

    unplaced = [c["name"] for c in candidates if c["name"] not in assigned_candidates]

    result = {
        "pairs_asked": len(pairs),
        "plan": plan,
        "unplaced": unplaced,
        "review": review,
        "rejected": rejected,
    }
    print(json.dumps(result, ensure_ascii=False))


if __name__ == "__main__":
    main()
