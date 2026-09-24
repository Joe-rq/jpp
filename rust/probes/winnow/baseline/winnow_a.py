import hashlib
import json

from jev_interface import judge

ERROR_QUESTION = "这段工具输出里是否含有报错、失败或异常信息？"


def decide_noul(p: float, thr: dict):
    if p >= thr["hi"] + thr["delta"]:
        return True
    if p <= thr["lo"] - thr["delta"]:
        return False
    return None


def stub_for(chunk: str, digest: str) -> str:
    return f"[winnow 已折叠 {len(chunk)} 字 · 展开键 {digest}]"


def main():
    with open("materials.json", encoding="utf-8") as fh:
        materials = json.load(fh)

    data = materials["tool_outputs"]
    task = data["task"]
    thresholds = materials["thresholds"]
    error_thr = thresholds["winnow-error"]
    topic_thr = thresholds["form-topic"]
    topic_question = f"这段话的内容是否与「{task}」这个话题相关？"

    results_out = []
    for item in data["results"]:
        item_id = item["id"]
        chunks = item["chunks"]
        joined = "\n".join(chunks)

        error_answer = judge(joined, [{"type": "noul", "instructions": ERROR_QUESTION}])[0]
        error_decision = decide_noul(error_answer["noul"], error_thr)

        if error_decision is True:
            results_out.append({
                "id": item_id,
                "reason": "error_present",
                "pruned": [],
                "uncertain": [],
                "text": list(chunks),
                "archive": [],
            })
            continue

        if error_decision is None:
            results_out.append({
                "id": item_id,
                "reason": "error_unsure",
                "pruned": [],
                "uncertain": [],
                "text": list(chunks),
                "archive": [],
            })
            continue

        # error_decision is False: check each block for topical relevance.
        relevance = []
        for chunk in chunks:
            answer = judge(chunk, [{"type": "noul", "instructions": topic_question}])[0]
            relevance.append(decide_noul(answer["noul"], topic_thr))

        candidates = [i for i, dec in enumerate(relevance) if dec is False]
        uncertain = [i for i, dec in enumerate(relevance) if dec is None]

        total_chars = sum(len(c) for c in chunks)
        candidate_chars = sum(len(chunks[i]) for i in candidates)

        if candidate_chars * 100 < total_chars * 20:
            results_out.append({
                "id": item_id,
                "reason": "below_min_prune_ratio",
                "pruned": [],
                "uncertain": [],
                "text": list(chunks),
                "archive": [],
            })
            continue

        text_out = []
        archive = []
        for i, chunk in enumerate(chunks):
            if i in candidates:
                digest = hashlib.sha256(chunk.encode("utf-8")).hexdigest()[:16]
                text_out.append(stub_for(chunk, digest))
                archive.append({"key": digest, "index": i, "text": chunk})
            else:
                text_out.append(chunk)

        results_out.append({
            "id": item_id,
            "reason": "pruned",
            "pruned": candidates,
            "uncertain": uncertain,
            "text": text_out,
            "archive": archive,
        })

    print(json.dumps({"task": task, "results": results_out}, ensure_ascii=False))


if __name__ == "__main__":
    main()
