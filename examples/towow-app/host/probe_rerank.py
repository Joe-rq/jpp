"""预注册 06 的探针：宽召回前 200 名 + 在「我」的状态上一次挂 200 道题重排，看真伙伴能否回到前 32。
候选来自 runs/scale/rank-diag.json（scale_rank 的输出）；合成背景按同一种子、同一排除名单重建，只读文本。
用法：.venv/bin/python -m host.probe_rerank --ids runs/scale/selftest-ids.txt --gold world/gold/structures.json
预注册 09：--pool all 时候选 = 在场全体（原包 + 生成背景）减去自己，不经向量，其余不变。
"""
from __future__ import annotations

import argparse
import asyncio
import json
import os
import re
import sys
import time

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


def cand_q_full(p: dict) -> str:
    """预注册 10 第二段：问法同 cand_q，对方信息换成完整 t0（展示语全文、全部在找 / 能提供、接得住的来信与能怎么接）。"""
    sig = "；".join(txt(f) for f in t0(p.get("signals")))
    off = "；".join(txt(f) for f in t0(p.get("offers")))
    cat = "；".join(f"若有人说「{f.get('hypo', '')}」，能{f.get('can', '')}" for f in t0(p.get("catchers")) if isinstance(f, dict))
    return (f"对方：{p.get('display', '')}。对方在找：{sig}。对方能提供：{off}。对方接得住的来信：{cat or '无'}。"
            "读「我」的世界：我和这位对方之间，可能有一桩实在的合作吗（我帮他、他帮我，或经身边的人转介都算）？")


async def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--ids", required=True)
    ap.add_argument("--gold", required=True)
    ap.add_argument("--seed", type=int, default=0)
    ap.add_argument("--max-cost", type=float, default=0.20)
    ap.add_argument("--out", default="runs/probe-rerank/result.json")
    ap.add_argument("--diag", default="runs/scale/rank-diag.json")
    ap.add_argument("--genbg", action="store_true")
    ap.add_argument("--tiers", default="500,2000,10000")
    ap.add_argument("--pool", choices=["top200", "all"], default="top200", help="all：预注册 09，在场全体都判，不经向量")
    ap.add_argument("--concurrency", type=int, default=8)
    ap.add_argument("--stage2", action="store_true", help="预注册 10：第一段前 200 名用完整 t0 再判一次")
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
    diag = json.load(open(os.path.join(APP_DIR, a.diag))) if a.pool == "top200" else None
    bg_dir = os.path.join(APP_DIR, "runs", "scale", "bg")
    port = JevPort(concurrency=a.concurrency)
    out = {}
    for t in a.tiers.split(","):
        if a.genbg:
            from host.scale_rank import load_genbg
            gb = load_genbg(TIERS[int(t)], packs)
        else:
            write_bg(TIERS[int(t)], packs, a.seed, bg_dir, exclude)
            gb = {}
        def pack_of(pid):
            if pid in packs:
                return packs[pid]
            if pid in gb:
                return gb[pid]
            return json.load(open(os.path.join(bg_dir, pid + ".json"), encoding="utf-8"))
        rows = []

        present = sorted(packs) + sorted(gb)
        walls, walls2, stage1 = [], [], {}

        async def one(x):
            if a.pool == "all":
                cands = [c for c in present if c != x]
            else:
                cands = [h["peer"] for h in diag[t]["top200"][x]]
            st = me_state(packs[x])
            t_start = time.monotonic()
            chunks = [cands[i:i + 100] for i in range(0, len(cands), 100)]   # 同一状态，每次 100 题（JEV 单次已验证 100 题）
            answers = await asyncio.gather(*[port.call(st, [WireQ("test", cand_q(pack_of(c))) for c in ch]) for ch in chunks])
            ps = [float(r["noul"]) for ans in answers for r in ans]
            walls.append(time.monotonic() - t_start)
            order = sorted(range(len(cands)), key=lambda i: (-ps[i], i))
            top32 = {cands[i] for i in order[:32]}
            stage1[x] = [cands[i] for i in order[:500]]
            s2 = None
            if a.stage2:
                c200 = [cands[i] for i in order[:200]]
                t2 = time.monotonic()
                ans2 = await asyncio.gather(*[port.call(st, [WireQ("test", cand_q_full(pack_of(c))) for c in c200[i:i + 100]])
                                              for i in range(0, len(c200), 100)])
                p2 = [float(r["noul"]) for ans in ans2 for r in ans]
                o2 = sorted(range(len(c200)), key=lambda i: (-p2[i], i))
                s2 = {"rank": {c200[i]: k + 1 for k, i in enumerate(o2)}, "wall": time.monotonic() - t2}
                walls2.append(s2["wall"])
            vec32 = set(cands[:32])
            for p in sorted(partners.get(x, ())):
                if p in packs:
                    r2 = s2["rank"].get(p) if s2 else None
                rows.append({"member": x, "partner": p, "vec_top32": p in vec32, "rerank_top32": p in top32,
                             "s1_top200": p in set(stage1[x][:200]), "s2_rank": r2, "s2_top32": bool(r2 and r2 <= 32),
                                 "in_top200": p in cands,
                                 "rerank_rank": (1 + order.index(cands.index(p))) if p in cands else None})
        n_c = len(present) if a.pool == "all" else 200
        est = len(keep) * (n_c + (600 if a.stage2 else 0)) * 0.0000094   # 实测每道重排题约 $0.0000094（预注册 09）；第二段题长约 3 倍
        if port.stats.cost_usd + est > a.max_cost:
            print("超出花费上限，停止", t); break
        await asyncio.gather(*[one(x) for x in keep])
        out[t] = {"n_pairs": len(rows), "vec_top32": sum(r["vec_top32"] for r in rows),
                  "rerank_top32": sum(r["rerank_top32"] for r in rows), "in_top200": sum(r["in_top200"] for r in rows),
                  "pool": a.pool, "n_candidates": n_c, "wall_s_median": sorted(walls)[len(walls) // 2] if walls else None,
                  "wall_s_max": max(walls) if walls else None,
                  "s1_top200": sum(r["s1_top200"] for r in rows), "s2_top32": sum(r["s2_top32"] for r in rows),
                  "s2_wall_s_median": sorted(walls2)[len(walls2) // 2] if walls2 else None, "rows": rows}
        if a.pool == "all":                      # 第一段前 500 名存盘：改第二段时不必重跑第一段
            so = os.path.join(APP_DIR, os.path.splitext(a.out)[0] + f"-stage1-{t}.json")
            os.makedirs(os.path.dirname(so), exist_ok=True)
            json.dump(stage1, open(so, "w"), ensure_ascii=False)
        print(t, {k: v for k, v in out[t].items() if k != "rows"}, f"cost ${port.stats.cost_usd:.4f}", flush=True)
    await port.close()
    os.makedirs(os.path.dirname(os.path.join(APP_DIR, a.out)), exist_ok=True)
    out["cost_usd"] = round(port.stats.cost_usd, 4)
    out["calls"] = port.stats.calls
    json.dump(out, open(os.path.join(APP_DIR, a.out), "w"), ensure_ascii=False, indent=1)


if __name__ == "__main__":
    asyncio.run(main())
