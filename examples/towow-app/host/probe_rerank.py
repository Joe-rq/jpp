"""预注册 06 的探针：宽召回前 200 名 + 在「我」的状态上一次挂 200 道题重排，看真伙伴能否回到前 32。
候选来自 runs/scale/rank-diag.json（scale_rank 的输出）；合成背景按同一种子、同一排除名单重建，只读文本。
用法：.venv/bin/python -m host.probe_rerank --ids runs/scale/selftest-ids.txt --gold world/gold/structures.json
"""
from __future__ import annotations

import argparse
import asyncio
import json
import os
import re
import sys

APP_DIR = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
sys.path.insert(0, APP_DIR)

from host.scale import TIERS, gold_exclusion, load_packs, write_bg  # noqa: E402
from jx.ports.jev import JevPort, WireQ  # noqa: E402


def txt(f):
    return (f.get("text") or f.get("can") or f.get("name") or "") if isinstance(f, dict) else str(f)


def t0(xs):
    return [f for f in (xs or []) if not isinstance(f, dict) or f.get("tier", "t0") == "t0"]


def me_state(p: dict) -> dict:
    return {"我": {"展示": p.get("display", ""), "在找": [txt(f) for f in t0(p.get("signals"))],
                  "能提供": [txt(f) for f in t0(p.get("offers"))],
                  "接得住的来信": [txt(f) for f in t0(p.get("catchers"))]}}


def cand_q(p: dict) -> str:
    head = re.split(r"[，,。；;]", p.get("display", ""))[0][:40]
    sig = "；".join(txt(f)[:60] for f in t0(p.get("signals"))[:2])
    off = "；".join(txt(f)[:60] for f in t0(p.get("offers"))[:2])
    return (f"对方：{head}。对方在找：{sig}。对方能提供：{off}。"
            "读「我」的世界：我和这位对方之间，可能有一桩实在的合作吗（我帮他、他帮我，或经身边的人转介都算）？")


async def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--ids", required=True)
    ap.add_argument("--gold", required=True)
    ap.add_argument("--seed", type=int, default=0)
    ap.add_argument("--max-cost", type=float, default=0.20)
    ap.add_argument("--out", default="runs/probe-rerank/result.json")
    a = ap.parse_args()
    packs = load_packs(os.path.join(APP_DIR, "world/packs"))
    keep = [x.strip() for x in open(a.ids, encoding="utf-8").read().replace(",", "\n").split() if x.strip()]
    gold_path = a.gold if os.path.isabs(a.gold) else os.path.join(APP_DIR, a.gold)
    exclude = frozenset(gold_exclusion(keep, gold_path))
    partners: dict[str, set[str]] = {}
    for g in json.load(open(gold_path, encoding="utf-8")):
        ids = [m["id"] for m in g["members"]]
        for x in ids:
            partners.setdefault(x, set()).update(y for y in ids if y != x)
    diag = json.load(open(os.path.join(APP_DIR, "runs/scale/rank-diag.json")))
    bg_dir = os.path.join(APP_DIR, "runs", "scale", "bg")
    port = JevPort(concurrency=8)
    out = {}
    for t in ("500", "2000", "10000"):
        write_bg(TIERS[int(t)], packs, a.seed, bg_dir, exclude)
        def pack_of(pid):
            if pid in packs:
                return packs[pid]
            return json.load(open(os.path.join(bg_dir, pid + ".json"), encoding="utf-8"))
        rows = []

        async def one(x):
            cands = [h["peer"] for h in diag[t]["top200"][x]]
            st = me_state(packs[x])
            ps = []
            for i in range(0, len(cands), 100):           # 同一状态，每次 100 题（JEV 单次已验证 100 题）
                chunk = cands[i:i + 100]
                ans = await port.call(st, [WireQ("test", cand_q(pack_of(c))) for c in chunk])
                ps += [float(r["noul"]) for r in ans]
            order = sorted(range(len(cands)), key=lambda i: (-ps[i], i))
            top32 = {cands[i] for i in order[:32]}
            vec32 = set(cands[:32])
            for p in sorted(partners.get(x, ())):
                if p in packs:
                    rows.append({"member": x, "partner": p, "vec_top32": p in vec32, "rerank_top32": p in top32,
                                 "in_top200": p in cands,
                                 "rerank_rank": (1 + order.index(cands.index(p))) if p in cands else None})
        est = len(keep) * 2 * 9000 * 4.2e-8
        if port.stats.cost_usd + est > a.max_cost:
            print("超出花费上限，停止", t); break
        await asyncio.gather(*[one(x) for x in keep])
        out[t] = {"n_pairs": len(rows), "vec_top32": sum(r["vec_top32"] for r in rows),
                  "rerank_top32": sum(r["rerank_top32"] for r in rows), "in_top200": sum(r["in_top200"] for r in rows),
                  "rows": rows}
        print(t, {k: v for k, v in out[t].items() if k != "rows"}, f"cost ${port.stats.cost_usd:.4f}", flush=True)
    await port.close()
    os.makedirs(os.path.dirname(os.path.join(APP_DIR, a.out)), exist_ok=True)
    out["cost_usd"] = round(port.stats.cost_usd, 4)
    out["calls"] = port.stats.calls
    json.dump(out, open(os.path.join(APP_DIR, a.out), "w"), ensure_ascii=False, indent=1)


if __name__ == "__main__":
    asyncio.run(main())
