"""Pair each live reading in live-read2.ledger.jsonl with its constructed truth; write labels.jsonl for calib-import."""
import json
items = json.load(open("items.json", encoding="utf-8"))
k2i = {x["k"]: x["i"] for x in json.load(open("live-read2.json"))["value"]["keys"]}
out, seen = [], set()
for line in open("live-read2.ledger.jsonl"):
    e = json.loads(line).get("entry", {}).get("Judge")
    if not e or e["key"] not in k2i or k2i[e["key"]] in seen:
        continue
    i = k2i[e["key"]]
    seen.add(i)
    out.append({"key": "winnow-error", "item": items[i]["id"], "p": e["answer"]["Noul"],
                "label": items[i]["label"], "source": "computed"})
open("labels.jsonl", "w").write("\n".join(json.dumps(o, ensure_ascii=False) for o in out) + "\n")
print(len(out), "labels")
