"""预注册 02 的探针：同一组题，状态只差「是否带双方 t1」，看难负例能不能被分开。
用法：.venv/bin/python host/probe_t1.py runs/full-500/edges-final.json --out runs/probe-t1/result.json
名单只取全量里判过的真值两两对与难负例；真值标签只在评分时读。
"""
import argparse, asyncio, json, os, sys
ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
sys.path.insert(0, ROOT)
from jx.ports.jev import JevPort, WireQ, PRICE_PER_INPUT_TOKEN

TIERS = {"t0": 0, "t1": 1, "t2": 2, "never": 9}
QS = [WireQ("test", "读 B 的世界，B（或 B 身边的人）能对 A 现在的状态或需要做点实在的事吗？"),
      WireQ("test", "读 A 的世界，A（或 A 身边的人）能对 B 现在的状态或需要做点实在的事吗？"),
      WireQ("select", "最可能的合作形式是哪一种？", {"direct": "直接互补", "oneway": "单向帮助", "relay_a": "经 A 身边的人转介",
            "relay_b": "经 B 身边的人转介", "third": "还需要第三方才成立", "none": "不成立"}),
      WireQ("test", "双方现在的时间、阶段与条件对得上吗？"),
      WireQ("test", "他们其实在找同一种人或同一种资源吗？"),
      WireQ("test", "对方的问题其实已经解决了吗？"),
      WireQ("test", "他们的价值观或底线有冲突吗？")]


def side(pack, upto):
    ok = lambda f: TIERS.get(f.get("tier", "t0"), 9) <= TIERS[upto]
    tx = lambda xs: [f.get("text") or f.get("can") or f.get("name") for f in xs if ok(f)]
    s = {"display": pack["display"], "signals": tx(pack.get("signals", [])), "offers": tx(pack.get("offers", [])),
         "projects": tx(pack.get("projects", [])), "catchers": tx(pack.get("catchers", []))}
    return s


def p_yes(a):
    return float(a["noul"])


async def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--out", default="runs/probe-t1/result.json")
    ap.add_argument("--max-cost", type=float, default=0.10)
    ap.add_argument("--random-only", type=int, default=0, help="预注册 04：只跑随机 N 对（种子 0，与 probe_catcher 同一组），只跑 t0")
    a = ap.parse_args()
    gold = json.load(open(os.path.join(ROOT, "world/gold/structures.json")))
    judged = set(json.load(open(os.path.join(ROOT, "runs/full-500-v1/judged-pairs.json"))))
    mem = lambda s: sorted({m["id"] for m in s["members"]})
    pairs = []
    for s in gold:
        lab = "gold" if s["type"] in ("pair", "latent", "relay") else "neg" if s["type"] == "hard_neg" else None
        k = "|".join(mem(s)[:2])
        if lab and k in judged:
            pairs.append((k, lab, s["id"]))
    tiers = ("t0", "t1")
    if a.random_only:
        import random
        gp = set()
        for s in gold:
            ms = mem(s)
            gp |= {f"{ms[i]}|{ms[j]}" for i in range(len(ms)) for j in range(i + 1, len(ms))}
        rnd = [k for k in json.load(open(os.path.join(ROOT, "runs/full-500-v1/judged-pairs.json"))) if k not in gp and "cfg" not in k]
        random.Random(0).shuffle(rnd)
        pairs, tiers = [(k, "random", None) for k in rnd[:a.random_only]], ("t0",)
    port = JevPort(concurrency=16)
    ref = "小：一次性的小忙；中：持续几周、双方都有实际收益；大：可能改变一方的生计、事业或生活处境。"
    rows = []

    async def one(k, lab, sid, tier):
        x, y = k.split("|")
        A = json.load(open(os.path.join(ROOT, f"world/packs/{x}.json")))
        B = json.load(open(os.path.join(ROOT, f"world/packs/{y}.json")))
        ans = await port.call({"a": side(A, tier), "b": side(B, tier), "ref": ref}, QS)
        form = max(ans[2]["probabilities"].items(), key=lambda kv: float(kv[1]))[0]
        r = {"pair": k, "label": lab, "sid": sid, "tier": tier, "open_ab": p_yes(ans[0]), "open_ba": p_yes(ans[1]),
             "form": form, "timing": p_yes(ans[3]), "neg_same": p_yes(ans[4]), "neg_solved": p_yes(ans[5]),
             "neg_values": p_yes(ans[6])}
        r["holds"] = (r["open_ab"] > 0.5 or r["open_ba"] > 0.5) and form != "none"
        rows.append(r)

    est = len(pairs) * 2 * 3000 * PRICE_PER_INPUT_TOKEN
    print(f"{len(pairs)} 对 × 2 条件，估计 ${est:.3f}", flush=True)
    if est > a.max_cost:
        sys.exit("超出花费上限")
    await asyncio.gather(*[one(k, l, s, t) for k, l, s in pairs for t in tiers])
    await port.close()
    os.makedirs(os.path.dirname(a.out), exist_ok=True)
    summ = {}
    for t in tiers:
        for lab in ("gold", "neg", "random"):
            rs = [r for r in rows if r["tier"] == t and r["label"] == lab]
            summ[f"{t}/{lab}"] = f"{sum(r['holds'] for r in rs)}/{len(rs)}"
        P = [max(r["open_ab"], r["open_ba"]) for r in rows if r["tier"] == t and r["label"] == "gold"]
        N = [max(r["open_ab"], r["open_ba"]) for r in rows if r["tier"] == t and r["label"] == "neg"]
        summ[f"{t}/auc"] = round(sum((p > n) + 0.5 * (p == n) for p in P for n in N) / max(1, len(P) * len(N)), 3)
    summ["cost_usd"] = round(port.stats.cost_usd, 4)
    summ["calls"] = port.stats.calls
    json.dump({"summary": summ, "rows": rows}, open(a.out, "w"), ensure_ascii=False, indent=1)
    print(json.dumps(summ, ensure_ascii=False, indent=1))


if __name__ == "__main__":
    asyncio.run(main())
