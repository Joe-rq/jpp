import json

from jev_interface import judge

COVER_QUESTION = "面试官 b 的专长是否覆盖候选人 a 的技术方向，足以对其进行技术面试？"


def classify(p: float, hi: float, lo: float, delta: float) -> str:
    if p >= hi + delta:
        return "是"
    if p <= lo - delta:
        return "否"
    return "未决"


def main() -> None:
    with open("materials.json", encoding="utf-8") as fh:
        materials = json.load(fh)

    candidates = materials["candidates"]
    interviewers = materials["interviewers"]
    threshold = materials["thresholds"]["iv-cover"]

    pairs = []
    for candidate in candidates:
        for interviewer in interviewers:
            if candidate["level"] in interviewer["levels"]:
                pairs.append((candidate, interviewer))

    verdicts = []
    for candidate, interviewer in pairs:
        material = {"a": candidate, "b": interviewer}
        answers = judge(material, [{"type": "noul", "instructions": COVER_QUESTION}])
        p = answers[0]["noul"]
        verdicts.append(classify(p, threshold["hi"], threshold["lo"], threshold["delta"]))

    plan = []
    review = []
    rejected = []
    assigned_candidates = set()
    interviewer_load = {interviewer["name"]: 0 for interviewer in interviewers}

    for (candidate, interviewer), verdict in zip(pairs, verdicts):
        if verdict == "未决":
            review.append({"candidate": candidate["name"], "interviewer": interviewer["name"]})
        elif verdict == "否":
            rejected.append({"candidate": candidate["name"], "interviewer": interviewer["name"]})
        else:
            if (candidate["name"] not in assigned_candidates
                    and interviewer_load[interviewer["name"]] < 2):
                plan.append({"candidate": candidate["name"], "interviewer": interviewer["name"]})
                assigned_candidates.add(candidate["name"])
                interviewer_load[interviewer["name"]] += 1

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
