"""预注册 07 的探针：成立题改成三选一（真能帮上 / 看似相关但不成 / 无关），看难负例能否被分到「看似相关」。
用法：.venv/bin/python -m host.probe_choice [--out runs/probe-choice/result.json]
"""
import argparse, asyncio, json, os, random, sys
ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
sys.path.insert(0, ROOT)
from jx.ports.jev import JevPort, WireQ
from host.probe_t1 import side

Q = WireQ("select", "这两个人之间是哪一种？", {
    "help": "其中一方（或他身边的人）真能给另一方现在要的东西",
    "looks": "看起来相关（同行、同城、同一话题），但细看给不了对方现在要的东西，或时间地点、条件对不上，或对方已经解决，或底线相冲",
    "none": "基本无关"})


def pairs_of():
    gold = json.load(open(os.path.join(ROOT, "world/gold/structures.json")))
    judged = json.load(open(os.path.join(ROOT, "runs/full-500-v1/judged-pairs.json")))
    js = set(judged)
    mem = lambda s: sorted({m["id"] for m in s["members"]})
    out, gp = [], set()
    for s in gold:
        ms = mem(s)
        gp |= {f"{ms[i]}|{ms[j]}" for i in range(len(ms)) for j in range(i + 1, len(ms))}
        lab = "gold" if s["type"] in ("pair", "latent", "relay") else "neg" if s["type"] == "hard_neg" else None
        k = "|".join(ms[:2])
        if lab and k in js:
            out.append((k, lab))
    rnd = [k for k in judged if k not in gp and "cfg" not in k]
    random.Random(0).shuffle(rnd)
    return out + [(k, "random") for k in rnd[:60]]


async def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--out", default="runs/probe-choice/result.json")
    a = ap.parse_args()
    pairs = pairs_of()
    port = JevPort(concurrency=16)
    rows = []

    async def one(k, lab, tier):
        x, y = k.split("|")
        A = json.load(open(os.path.join(ROOT, f"world/packs/{x}.json")))
        B = json.load(open(os.path.join(ROOT, f"world/packs/{y}.json")))
        ans = await port.call({"a": side(A, tier), "b": side(B, tier)}, [Q])
        pr = {k2: float(v) for k2, v in ans[0]["probabilities"].items()}
        top = max(pr, key=pr.get)
        rows.append({"pair": k, "label": lab, "tier": tier, "probs": pr, "top": top, "holds": top == "help"})

    await asyncio.gather(*[one(k, l, t) for k, l in pairs for t in ("t0", "t1") if not (l == "random" and t == "t1")])
    await port.close()
    summ = {}
    for t in ("t0", "t1"):
        for lab in ("gold", "neg", "random"):
            rs = [r for r in rows if r["tier"] == t and r["label"] == lab]
            if rs:
                summ[f"{t}/{lab}"] = f"{sum(r['holds'] for r in rs)}/{len(rs)} = {sum(r['holds'] for r in rs) / len(rs):.2f}"
        P = [r["probs"].get("help", 0) for r in rows if r["tier"] == t and r["label"] == "gold"]
        N = [r["probs"].get("help", 0) for r in rows if r["tier"] == t and r["label"] == "neg"]
        summ[f"{t}/auc_help"] = round(sum((p > n) + 0.5 * (p == n) for p in P for n in N) / max(1, len(P) * len(N)), 3)
    summ["cost_usd"] = round(port.stats.cost_usd, 4)
    summ["calls"] = port.stats.calls
    os.makedirs(os.path.dirname(os.path.join(ROOT, a.out)), exist_ok=True)
    json.dump({"summary": summ, "rows": rows}, open(os.path.join(ROOT, a.out), "w"), ensure_ascii=False, indent=1)
    print(json.dumps(summ, ensure_ascii=False, indent=1))


if __name__ == "__main__":
    asyncio.run(main())
