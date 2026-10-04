"""预注册 03 的探针：确认题只问限定句 vs 连同设想来信一起问。
用法：.venv/bin/python host/probe_catcher.py [--out runs/probe-catcher/result.json]
"""
import argparse, asyncio, json, os, random, sys
ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
sys.path.insert(0, ROOT)
from jx.ports.jev import JevPort, WireQ, PRICE_PER_INPUT_TOKEN
from host.probe_t1 import side


def q_old(recv, sender, c):
    return (f"把 {sender} 当作来信的人。{recv} 事先写了一道题来确认来信人是不是自己接得住的那种人，"
            f"请就 {sender} 的情况回答：{c.get('confirm') or c.get('hypo') or ''}")


def q_new(recv, sender, c):
    return (f"把 {sender} 当作来信的人。{recv} 设想过会有人这样来找他：「{c.get('hypo') or ''}」。"
            f"{sender} 现在真有这样的需要、会发出这样的来信吗？并且：{c.get('confirm') or ''}")


def t0_catchers(p, n=2):
    return [c for c in p.get("catchers", []) if c.get("tier", "t0") == "t0" and c.get("hypo")][:n]


async def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--out", default="runs/probe-catcher/result.json")
    ap.add_argument("--max-cost", type=float, default=0.10)
    a = ap.parse_args()
    gold = json.load(open(os.path.join(ROOT, "world/gold/structures.json")))
    judged = json.load(open(os.path.join(ROOT, "runs/full-500-v1/judged-pairs.json")))
    mem = lambda s: sorted({m["id"] for m in s["members"]})
    in_gold = {m for s in gold for m in [tuple(mem(s))] for _ in [0]}
    gold_members_pairs = set()
    for s in gold:
        ms = mem(s)
        for i in range(len(ms)):
            for j in range(i + 1, len(ms)):
                gold_members_pairs.add(f"{ms[i]}|{ms[j]}")
    pairs = []
    for s in gold:
        lab = "gold" if s["type"] in ("pair", "latent", "relay") else "neg" if s["type"] == "hard_neg" else None
        k = "|".join(mem(s)[:2])
        if lab and k in set(judged):
            pairs.append((k, lab))
    rnd = [k for k in judged if k not in gold_members_pairs and "cfg" not in k]
    random.Random(0).shuffle(rnd)
    pairs += [(k, "random") for k in rnd[:60]]
    port = JevPort(concurrency=16)
    rows = []

    async def one(k, lab):
        x, y = k.split("|")
        A = json.load(open(os.path.join(ROOT, f"world/packs/{x}.json")))
        B = json.load(open(os.path.join(ROOT, f"world/packs/{y}.json")))
        st = {"a": side(A, "t0"), "b": side(B, "t0")}
        # B 的确认题问 A（A 是来信人），A 的确认题问 B
        cs = [("B", "A", c) for c in t0_catchers(B)] + [("A", "B", c) for c in t0_catchers(A)]
        if not cs:
            return
        for wording, f in (("old", q_old), ("new", q_new)):
            ans = await port.call(st, [WireQ("test", f(r, s, c)) for r, s, c in cs])
            ps = [float(x["noul"]) for x in ans]
            rows.append({"pair": k, "label": lab, "wording": wording, "ps": ps, "any_act": any(p > 0.5 for p in ps)})

    est = len(pairs) * 2 * 2500 * PRICE_PER_INPUT_TOKEN
    print(f"{len(pairs)} 对 × 2 种题面，估计 ${est:.3f}", flush=True)
    if est > a.max_cost:
        sys.exit("超出花费上限")
    await asyncio.gather(*[one(k, l) for k, l in pairs])
    await port.close()
    summ = {}
    for w in ("old", "new"):
        for lab in ("gold", "neg", "random"):
            rs = [r for r in rows if r["wording"] == w and r["label"] == lab]
            summ[f"{w}/{lab}"] = f"{sum(r['any_act'] for r in rs)}/{len(rs)} = {sum(r['any_act'] for r in rs) / max(1, len(rs)):.2f}"
    summ["cost_usd"] = round(port.stats.cost_usd, 4)
    summ["calls"] = port.stats.calls
    os.makedirs(os.path.dirname(a.out), exist_ok=True)
    json.dump({"summary": summ, "rows": rows}, open(a.out, "w"), ensure_ascii=False, indent=1)
    print(json.dumps(summ, ensure_ascii=False, indent=1))


asyncio.run(main())
