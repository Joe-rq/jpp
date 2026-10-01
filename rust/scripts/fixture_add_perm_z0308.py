#!/usr/bin/env python3
"""Z0308：给题库重认夹具里 F3、F4 的 select 观察补上两序置换测量（`perms`、`mode_share`）。

数据来源：`地基/评估/27a-置换重跑/F3-提到哪一个/readings.json` 与 `F4-同一个/readings.json`——题库 F3、F4
入库时用的两序真机读数（B64，每道题正序、逆序各发一次），由同目录真机账本 `live-ledger.jsonl` 的 `Judge.perm`
导出，每条带 `dist`、`perms`、`mode_share`。`scripts/fixture_from_ledger.py` 导出夹具时没带 `perm`，所以夹具里的
F3、F4 观察缺这两个字段；固定观察重认时账本没有置换测量，标注行也就带不上众数。

对应方法：夹具观察的 `on[0]` 等于材料（同目录 `materials.json`）的 `text`，F4 另要 `over` 相同；
再核观察的 `answer.Choice` 与该材料读数的 `dist` 逐位相同。任何一条对不上、或一条对上多条材料，报错退出、不写。

用法（在 `地基/rust-jpp` 下）：
    python3 scripts/fixture_add_perm_z0308.py [--check]
`--check` 只核对不写。写回保持夹具原有缩进与字段顺序（新字段追加在观察末尾）。
"""
import argparse
import json
import sys
from pathlib import Path

RJ = Path(__file__).resolve().parents[1]
FIX = RJ / "crates/jpp/tests/fixtures/bank-recert/fixtures.json"
SRC = {
    "cls-which-mentioned": RJ.parent / "评估/27a-置换重跑/F3-提到哪一个",
    "cmp-same-referent": RJ.parent / "评估/27a-置换重跑/F4-同一个",
}


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--check", action="store_true")
    a = ap.parse_args()
    raw = FIX.read_text(encoding="utf-8")
    fx = json.loads(raw)
    n = 0
    for calib, d in SRC.items():
        mats = json.loads((d / "materials.json").read_text(encoding="utf-8"))["items"]
        rd = json.loads((d / "readings.json").read_text(encoding="utf-8"))
        obs = [o for o in fx["observations"] if o["op"] == "select" and o["calib"] == calib]
        if len(obs) != len(rd):
            sys.exit(f"{calib}: 夹具观察 {len(obs)} 条，两序读数 {len(rd)} 条，对不上")
        used = set()
        for o in obs:
            hits = [
                m for m in mats
                if m["text"] == o["on"][0] and ("over" not in m or m["over"] == o["over"])
                and rd[m["id"]]["dist"] == o["answer"]["Choice"]
            ]
            if len(hits) != 1:
                sys.exit(f"{calib}: 观察 {o['on'][0]!r} 对上 {len(hits)} 条材料")
            i = hits[0]["id"]
            if i in used:
                sys.exit(f"{calib}: 材料 {i} 被两条观察对上")
            used.add(i)
            r = rd[i]
            for k in ("perms", "mode_share"):
                if k in o and o[k] != r[k]:
                    sys.exit(f"{calib}: 材料 {i} 夹具已有 {k}={o[k]}，与两序读数 {r[k]} 不同")
            o["perms"], o["mode_share"] = r["perms"], r["mode_share"]
            n += 1
    note = "F3、F4 的 select 观察带两序置换测量 perms、mode_share（来自 评估/27a-置换重跑/F*/readings.json，scripts/fixture_add_perm_z0308.py 补，Z0308）。"
    if note not in fx["description"]:
        fx["description"] += note
    print(f"补置换测量 {n} 条观察", file=sys.stderr)
    if not a.check:
        FIX.write_text(json.dumps(fx, ensure_ascii=False, indent=1 if raw.startswith("{\n ") else None) + "\n", encoding="utf-8")


if __name__ == "__main__":
    main()
