"""步 20d-1：给第四轮 R 题式的标注行补上材料文本（`text` 字段），供范围指纹（B68）重导入。

标注行的 item = sha256(材料 on + "\\x1f" + 填法)[:12]（由 items4.json 反查验证：410/410 命中）。
输出 probes/scope/语义R-带材料.jsonl：原行内容不变，只加 text。
"""
import hashlib
import json
import pathlib

ROOT = pathlib.Path(__file__).resolve().parents[4] / "实测" / "校准题式-2026-09-23"
HERE = pathlib.Path(__file__).parent
it = json.load(open(ROOT / "items4.json"))
text_of = {}
for x in it:
    if x["t"] != "S4R":
        continue
    on, fill = x["state"]["on"], x["fill"]
    text_of[hashlib.sha256((on + "\x1f" + fill).encode()).hexdigest()[:12]] = on
rows = [json.loads(l) for l in open(ROOT / "真值通道" / "语义R-模型标注加抽检30.jsonl")]
miss = 0
with open(HERE / "语义R-带材料.jsonl", "w") as f:
    for r in rows:
        t = text_of.get(r["item"])
        if t is None:
            miss += 1
        else:
            r["text"] = t
        f.write(json.dumps(r, ensure_ascii=False) + "\n")
print(json.dumps({"rows": len(rows), "missing_text": miss}))
