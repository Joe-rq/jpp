import json

from jev_interface import judge

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


def verdict(p: float, thr: dict) -> str:
    if p >= thr["hi"] + thr["delta"]:
        return "是"
    if p <= thr["lo"] - thr["delta"]:
        return "否"
    return "未决"


def main() -> None:
    with open("materials.json", encoding="utf-8") as fh:
        materials = json.load(fh)
    chats = materials["chats"]
    thresholds = materials["thresholds"]
    refund_thr = thresholds["cs-refund"]
    heated_thr = thresholds["cs-heated"]

    urgent = []
    refund_calm = []
    no_refund = []
    review = []

    for i, chat in enumerate(chats):
        refund_answer = judge(chat, [REFUND_Q])[0]
        refund_verdict = verdict(refund_answer["noul"], refund_thr)
        if refund_verdict == "否":
            no_refund.append(i)
            continue
        if refund_verdict == "未决":
            review.append(i)
            continue
        heated_answer = judge(chat, [HEATED_Q])[0]
        heated_verdict = verdict(heated_answer["noul"], heated_thr)
        if heated_verdict == "是":
            urgent.append(i)
        elif heated_verdict == "否":
            refund_calm.append(i)
        else:
            review.append(i)

    result = {
        "urgent": urgent,
        "refund_calm": refund_calm,
        "no_refund": no_refund,
        "review": review,
        "urgent_count": [len(urgent), len(urgent) + len(review)],
    }
    print(json.dumps(result, ensure_ascii=False))


if __name__ == "__main__":
    main()
