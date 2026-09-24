import json
import os

from jev_interface import judge


def decide_noul(p, threshold):
    hi = threshold["hi"] + threshold["delta"]
    lo = threshold["lo"] - threshold["delta"]
    if p >= hi:
        return True
    if p <= lo:
        return False
    return None


def main():
    here = os.path.dirname(os.path.abspath(__file__))
    with open(os.path.join(here, "materials.json"), encoding="utf-8") as fh:
        materials = json.load(fh)

    chats = materials["chats"]
    thresholds = materials["thresholds"]
    refund_threshold = thresholds["cs-refund"]
    heated_threshold = thresholds["cs-heated"]

    urgent = []
    refund_calm = []
    no_refund = []
    review = []

    for i, chat in enumerate(chats):
        refund_answers = judge(chat, [
            {"type": "noul", "instructions": "这段客服对话里，顾客是否明确提出要退款或退钱？"},
        ])
        refund_p = refund_answers[0]["noul"]
        refund_decision = decide_noul(refund_p, refund_threshold)

        if refund_decision is None:
            review.append(i)
            continue
        if refund_decision is False:
            no_refund.append(i)
            continue

        heated_answers = judge(chat, [
            {"type": "noul", "instructions": "这段客服对话里，顾客的措辞是否情绪激烈（例如愤怒、威胁投诉或曝光、连续质问）？"},
        ])
        heated_p = heated_answers[0]["noul"]
        heated_decision = decide_noul(heated_p, heated_threshold)

        if heated_decision is None:
            review.append(i)
        elif heated_decision is True:
            urgent.append(i)
        else:
            refund_calm.append(i)

    result = {
        "urgent": sorted(urgent),
        "refund_calm": sorted(refund_calm),
        "no_refund": sorted(no_refund),
        "review": sorted(review),
        "urgent_count": [len(urgent), len(urgent) + len(review)],
    }
    print(json.dumps(result, ensure_ascii=False))


if __name__ == "__main__":
    main()
