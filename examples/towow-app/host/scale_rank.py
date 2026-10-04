"""零花费诊断：真值伙伴在宽召回里的名次（对 --clean-bg 背景的 500/2000/10000 三档）。

对 20 个测试成员各做一次 route(k=500)（四路合并去重，排序与线上一致：score、breadth），
报每个真值伙伴的名次：33–200 名 = 加一道重排就能救回；1000 名开外或没被取回 = 向量召回本身不行。
索引里放全部背景 + 全部 20 个测试成员（伙伴都在场，共 25 个有序对）。只用 bge 编码，一次一档、一个进程。
用法：python -m host.scale_rank --ids runs/scale/selftest-ids.txt --gold world/gold/structures.json
"""
from __future__ import annotations

import argparse
import json
import os
import statistics
import sys

APP_DIR = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
sys.path.insert(0, APP_DIR)
os.environ.setdefault("HF_HUB_OFFLINE", "1")

from host.index import FragmentIndex  # noqa: E402
from host.scale import TIERS, gold_exclusion, load_packs, write_bg  # noqa: E402


def node_of(aid: str, p: dict) -> dict:
    t0 = lambda xs: [f for f in (xs or []) if not isinstance(f, dict) or f.get("tier", "t0") == "t0"]  # noqa: E731
    return {"id": aid, "kind": "agent", "signals": t0(p.get("signals")), "offers": t0(p.get("offers")),
            "catchers": t0(p.get("catchers")), "forbids": p.get("forbids"), "projects": t0(p.get("projects")),
            "derived": [], "members": [aid]}


def load_genbg(n: int, packs: dict) -> dict:
    """预注册 08 的背景：按批次顺序取前 n 个生成的人（g00001…），只留 t0；
    丢掉与任何原包逐字相同的片段，人与人之间逐字重复的片段只留第一次出现。"""
    gdir = os.path.join(APP_DIR, "runs", "scale", "genbg")
    seen = set()
    for p in packs.values():
        for k in ("signals", "offers", "catchers"):
            for f in p.get(k) or []:
                seen.add(str((f.get("hypo") or f.get("text")) if isinstance(f, dict) else f))
    out, i = {}, 0
    for fn in sorted(os.listdir(gdir)):
        if not fn.startswith("batch"):
            continue
        for q in json.load(open(os.path.join(gdir, fn), encoding="utf-8")):
            if len(out) >= n:
                return out
            if not isinstance(q, dict):
                continue
            i += 1
            pk = {"display": str(q.get("display", "")), "lang": q.get("lang", "zh")}
            for k in ("signals", "offers"):
                keep = []
                for t in q.get(k) or []:
                    t = str(t)
                    if t not in seen:
                        seen.add(t)
                        keep.append({"text": t, "tier": "t0"})
                pk[k] = keep
            cs = []
            for c in q.get("catchers") or []:
                if isinstance(c, dict) and c.get("hypo") and str(c["hypo"]) not in seen:
                    seen.add(str(c["hypo"]))
                    cs.append({**c, "tier": "t0"})
            pk["catchers"] = cs
            out[f"g{i:05d}"] = pk
    return out


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--ids", required=True)
    ap.add_argument("--gold", required=True)
    ap.add_argument("--tiers", default="500,2000,10000")
    ap.add_argument("--k", type=int, default=500)
    ap.add_argument("--device", default="mps")
    ap.add_argument("--seed", type=int, default=0)
    ap.add_argument("--genbg", action="store_true", help="预注册 08：背景用 claude 生成的不复用片段人口（runs/scale/genbg）")
    ap.add_argument("--out", default="runs/scale/rank-diag.json")
    a = ap.parse_args()
    from jx.ports.enc import EncPort
    enc = EncPort(os.path.join(APP_DIR, "runs", "raw", "enc-cache.sqlite"), device=a.device)

    packs = load_packs(os.path.join(APP_DIR, "world/packs"))
    keep = [x.strip() for x in open(a.ids, encoding="utf-8").read().replace(",", "\n").split() if x.strip()]
    gold_path = a.gold if os.path.isabs(a.gold) else os.path.join(APP_DIR, a.gold)
    exclude = frozenset(gold_exclusion(keep, gold_path))
    partners: dict[str, set[str]] = {}
    for g in json.load(open(gold_path, encoding="utf-8")):
        ids = [m["id"] for m in g["members"]]
        for x in ids:
            partners.setdefault(x, set()).update(y for y in ids if y != x)
    residents = sorted(set(packs) - set(keep))
    bg_dir = os.path.join(APP_DIR, "runs", "scale", "bg")
    out = {}
    for t in [int(x) for x in a.tiers.split(",")]:
        ix = FragmentIndex(enc)
        for aid in residents:
            ix.index_put(aid, node_of(aid, packs[aid]))
        if a.genbg:
            gb = load_genbg(TIERS[t], packs)
            for aid, p in gb.items():
                ix.index_put(aid, node_of(aid, p))
        else:
            synth = write_bg(TIERS[t], packs, a.seed, bg_dir, exclude)
            for aid in synth:
                ix.index_put(aid, node_of(aid, json.load(open(os.path.join(bg_dir, aid + ".json"), encoding="utf-8"))))
        for aid in keep:
            ix.index_put(aid, node_of(aid, packs[aid]))
        rows, top200, samples = [], {}, []
        for x in keep:
            res = ix.route(x, node_of(x, packs[x]), a.k)
            rank = {h["peer"]: i + 1 for i, h in enumerate(res)}
            s32 = res[31]["score"] if len(res) >= 32 else None
            top200[x] = [{"peer": h["peer"], "score": h["score"]} for h in res[:200]]
            for h in res[:32]:       # 抽样：挤进前 32 的合成背景，与测试成员匹配上的那条片段原文
                if h["peer"][:1] in ("b", "g") and h["peer"][1:].isdigit() and len(samples) < 40:
                    r0 = h["routes"][0]
                    samples.append({"member": x, "peer": h["peer"], "score": h["score"], "route": r0["route"],
                                    "mine": r0["mine"], "theirs": r0["theirs"]})
            for p in sorted(partners.get(x, ())):
                if p not in packs:
                    continue
                h = res[rank[p] - 1] if p in rank else None
                rows.append({"member": x, "partner": p, "rank": rank.get(p), "score": h and h["score"],
                             "score_at_32": s32, "n_retrieved": len(res),
                             "best_route": h and h["routes"][0]["route"]})
        rk = [r["rank"] for r in rows]
        got = sorted(r for r in rk if r)
        out[t] = {"n_pairs": len(rows), "retrieved": len(got), "rank_median": statistics.median(got) if got else None,
                  "rank_worst": max(got) if got else None, "not_retrieved": len(rk) - len(got),
                  "in_top32": sum(1 for r in got if r <= 32), "in_33_200": sum(1 for r in got if 33 <= r <= 200),
                  "in_top200": sum(1 for r in got if r <= 200), "frac_top200": round(sum(1 for r in got if r <= 200) / len(rows), 3),
                  "beyond_1000_or_missing": sum(1 for r in rk if r is None or r > 1000), "k": a.k, "rows": rows, "top200": top200, "synthetic_samples": samples}
        print(t, {k: v for k, v in out[t].items() if k not in ("rows", "top200", "synthetic_samples")}, flush=True)
        del ix
    json.dump(out, open(os.path.join(APP_DIR, a.out), "w"), ensure_ascii=False, indent=1)


if __name__ == "__main__":
    main()
