#!/usr/bin/env python3
"""第二批 select 条目（C22、C25、C26）的重认夹具：给 fixture_from_ledger.py 导出的观察补上两序置换测量
（`perms`、`mode_share`，Z0308），再合并进 crates/jpp/tests/fixtures/bank-recert/fixtures.json。

做法同 fixture_add_perm_z0308.py：观察的 `on[0]` 与 `over` 等于材料（`input*.json` 的 text 与 over），
`answer.Choice` 与该材料读数（`readings*.json` 的 dist）逐位相同，任何一条对不上或一条对上多条，报错退出、不写。

用法：python3 scripts/fixture_add_perm_batch2.py <导出的夹具.json> <组目录> <input 文件名> <readings 文件名> <标签> [<标签> ...]
  例：... c25.json 题库/第二批/C25 input.json readings.json "C25 next_step"
写回前先把补好的观察追加到夹具文件；描述里记一句来源。
"""
import json
import sys
from pathlib import Path

RJ = Path(__file__).resolve().parents[1]
FIX = RJ / "crates/jpp/tests/fixtures/bank-recert/fixtures.json"
src, gdir, inp, rdn, label = sys.argv[1], Path(sys.argv[2]), sys.argv[3], sys.argv[4], sys.argv[5]
obs = json.loads(Path(src).read_text(encoding="utf-8"))["observations"]
mats = json.loads((gdir / inp).read_text(encoding="utf-8"))["items"]
rd = json.loads((gdir / rdn).read_text(encoding="utf-8"))
used = set()
for o in obs:
    hits = [m for m in mats if m["text"] == o["on"][0] and m["over"] == o["over"] and rd[m["id"]]["dist"] == o["answer"]["Choice"]]
    if len(hits) != 1:
        sys.exit(f"观察 {o['on'][0]!r} 对上 {len(hits)} 条材料")
    i = hits[0]["id"]
    if i in used:
        sys.exit(f"材料 {i} 被两条观察对上")
    used.add(i)
    o["perms"], o["mode_share"] = rd[i]["perms"], rd[i]["mode_share"]
fx = json.loads(FIX.read_text(encoding="utf-8"))
fx["observations"].extend(obs)
fx["description"] += f"第二批 {label} 的 {len(obs)} 条（select，带两序置换测量 perms、mode_share）来自 {gdir.name}/ 的真机账本，用条目的 recert/read.jpp 经 scripts/fixture_from_ledger.py 导出、scripts/fixture_add_perm_batch2.py 补置换后合并。"
FIX.write_text(json.dumps(fx, ensure_ascii=False, indent=1) + "\n", encoding="utf-8")
print(label, "补置换并追加", len(obs), "条；夹具共", len(fx["observations"]), "条")
