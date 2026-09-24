import hashlib
import json

from jev_interface import judge

ERROR_QUESTION = "这段工具输出里是否含有报错、失败或异常信息？"
TOPIC_QUESTION_TMPL = "这段话的内容是否与「{task}」这个话题相关？"


def decide(p, threshold):
    if p >= threshold["hi"] + threshold["delta"]:
        return True
    if p <= threshold["lo"] - threshold["delta"]:
        return False
    return None


def all_kept(chunks, reason):
    return {
        "reason": reason,
        "pruned": [],
        "uncertain": [],
        "text": list(chunks),
        "archive": [],
    }


def process_result(result, task, error_threshold, topic_threshold):
    chunks = result["chunks"]

    error_material = "\n".join(chunks)
    error_q = {"type": "noul", "instructions": ERROR_QUESTION}
    error_answer = judge(error_material, [error_q])[0]
    error_decision = decide(error_answer["noul"], error_threshold)

    if error_decision is True:
        return {"id": result["id"], **all_kept(chunks, "error_present")}
    if error_decision is None:
        return {"id": result["id"], **all_kept(chunks, "error_unsure")}

    topic_instructions = TOPIC_QUESTION_TMPL.format(task=task)
    topic_q = {"type": "noul", "instructions": topic_instructions}
    relevance = []
    for chunk in chunks:
        answer = judge(chunk, [topic_q])[0]
        relevance.append(decide(answer["noul"], topic_threshold))

    candidate_pruned = [i for i, d in enumerate(relevance) if d is False]
    uncertain = [i for i, d in enumerate(relevance) if d is None]

    candidate_chars = sum(len(chunks[i]) for i in candidate_pruned)
    total_chars = sum(len(c) for c in chunks)
    if candidate_chars * 100 < total_chars * 20:
        return {"id": result["id"], **all_kept(chunks, "below_min_prune_ratio")}

    pruned_set = set(candidate_pruned)
    text = []
    archive = []
    for i, chunk in enumerate(chunks):
        if i in pruned_set:
            key = hashlib.sha256(chunk.encode("utf-8")).hexdigest()
            text.append(f"[winnow 已折叠 {len(chunk)} 字 · 展开键 {key}]")
            archive.append({"key": key, "index": i, "text": chunk})
        else:
            text.append(chunk)

    return {
        "id": result["id"],
        "reason": "pruned",
        "pruned": candidate_pruned,
        "uncertain": uncertain,
        "text": text,
        "archive": archive,
    }


def main():
    with open("materials.json", encoding="utf-8") as fh:
        materials = json.load(fh)

    tool_outputs = materials["tool_outputs"]
    task = tool_outputs["task"]
    thresholds = materials["thresholds"]
    error_threshold = thresholds["winnow-error"]
    topic_threshold = thresholds["form-topic"]

    results = [
        process_result(result, task, error_threshold, topic_threshold)
        for result in tool_outputs["results"]
    ]

    print(json.dumps({"task": task, "results": results}, ensure_ascii=False))


if __name__ == "__main__":
    main()
