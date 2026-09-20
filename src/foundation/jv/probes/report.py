"""把 runs/jv/e-probe-10/<tag>/summary.json 变成 前提结论 用的表（只打印，不写文件）。"""

from __future__ import annotations

import json
import os
import sys

from ._common import RUNS_ROOT


def main(tag: str = "run1") -> None:
    with open(os.path.join(RUNS_ROOT, tag, "summary.json"), encoding="utf-8") as fh:
        s = json.load(fh)
    rows = []
    for name, r in s["probes"].items():
        b, bare, base, rep = r["builder"], r["bare"], r.get("baseline") or {}, r.get("replay") or {}
        kind = r["kind"]

        def m(x):
            mm = (x or {}).get("metrics") or {}
            if kind == "noul":
                return f"AUC {mm.get('auc')} / acc {mm.get('acc')}"
            if kind == "score":
                return f"acc {mm.get('acc')} / MAE {mm.get('mae')}"
            return f"acc {mm.get('acc')}"

        def sv(x):
            v = (x or {}).get("saved")
            return f"{v['saved']}/{v['sent'] + v['saved']}（召回 {v['recall']}）" if v else "—"

        unsure = b["metrics"]["unsure"]
        rows.append(f"| {name} {r['title']} | {kind} | {m(b)}（unsure {unsure}） | {m(bare)} | {m(base) if base else '—'} | "
                    f"{b['layers']} / {b['questions']} / {b['calls']} / {b['fusion_rate']} | {bare.get('calls')} / {bare.get('questions')} | "
                    f"{rep.get('calls', '—')} / {rep.get('ledger_hits', '—')} | ${b['cost']:.4f} / ${bare.get('cost', 0):.4f} | "
                    f"{sv(b)} / {sv(base) if base else '—'} |")
    print("| 探针 | 题型 | 构建器 | 裸调 | 无 Jev 基线 | 构建器 层/题/调用/融合率 | 裸调 调用/题 | 重放 调用/命中 | 钱 构建器/裸调 | 省掉（构建器 / 基线） |")
    print("|---|---|---|---|---|---|---|---|---|---|")
    print("\n".join(rows))
    print(f"\n总花费 ${s['total_cost']:.4f}；stopped={s.get('stopped')}")
    # 逐题 provisional / unsure 分布
    for name, r in s["probes"].items():
        st = {}
        for x in r["builder_results"]:
            st[x["status"]] = st.get(x["status"], 0) + 1
        print(name, st)


if __name__ == "__main__":
    main(sys.argv[1] if len(sys.argv) > 1 else "run1")
