"""按预注册 01 的口径对一次运行打分。只在运行结束后读真值（world/gold），系统运行中从不读。
输入：runs/<run>/result.json  {edges:[{a,b,holds,p_hold,tier_seen,form}], configs:[{id,shape,members,p_hold,stage}], joins:[{id, t_join, t_first_opp, calls}]}
用法：.venv/bin/python host/eval.py runs/<run>/result.json [--subset ids.txt]
"""
import json, sys, collections, os
ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
gold = json.load(open(os.path.join(ROOT, "world/gold/structures.json")))
res = json.load(open(sys.argv[1]))
present = set(res.get("present") or [j["id"] for j in res.get("joins", [])])

def mem(s): return sorted({m["id"] for m in s["members"]})
deep = lambda s: all(f["tier"] != "t0" for f in s["key_facts"] if f["agent"] in mem(s)[:1]) or any(
    all(f["tier"] != "t0" for f in s["key_facts"] if f["agent"] == m) for m in mem(s))

def p_yes(e):
    """决定性那道题说「成立」的读数：出口是 ignore 时报告的是「不成立」那一块的质量，换成 1-p。"""
    p = e.get("p_hold") or 0
    return 1 - p if e.get("decisive_exit") == "ignore" else p

pair_hold = {}
for e in res.get("edges", []):
    k = tuple(sorted((e["a"], e["b"])))
    if e.get("holds"):
        pair_hold[k] = max(pair_hold.get(k, 0), p_yes(e))
all_pairs = {tuple(sorted((e["a"], e["b"]))): e for e in res.get("edges", [])}
cfgs = [c for c in res.get("configs", []) if c.get("stage", "judged") in ("judged", "growing", "plan")]
_by_id = {c["id"]: c for c in res.get("configs", [])}
def _flat(ms, seen=()):
    """构型成员展开到人：嵌套构型（成员里有 cfg:）按它的成员递归展开。10-04 起，之前的嵌套命中被漏算。"""
    out = set()
    for x in ms:
        if str(x).startswith("cfg:") and x in _by_id and x not in seen:
            out |= _flat(_by_id[x]["members"], seen + (x,))
        else:
            out.add(x)
    return out
for c in cfgs:
    c["members"] = sorted(_flat(c["members"]))

def hit(s):
    m = set(mem(s))
    if s["type"] in ("pair", "latent", "relay"):
        a, b = sorted(m)[:2]
        if (a, b) in pair_hold: return True
        return any(m <= set(c["members"]) for c in cfgs)
    need = max(2, int(0.8 * len(m) + 0.999))
    return any(len(m & set(c["members"])) >= need for c in cfgs)

rows = collections.defaultdict(lambda: [0, 0])
detail = []
for s in gold:
    if s["type"] == "hard_neg": continue
    if not set(mem(s)) <= present: continue
    key = (s["type"], "deep" if deep(s) else "t0")
    h = hit(s); rows[key][0] += h; rows[key][1] += 1
    detail.append((s["id"], s["type"], key[1], h))
out = {"recall": {f"{t}/{d}": f"{a}/{n} = {a/n:.2f}" for (t, d), (a, n) in sorted(rows.items())}}
tot = lambda f: (sum(a for k, (a, n) in rows.items() if f(k)), sum(n for k, (a, n) in rows.items() if f(k)))
for name, f in {"pair+latent+relay t0": lambda k: k[0] in ("pair","latent","relay") and k[1]=="t0",
                "pair+latent+relay deep": lambda k: k[0] in ("pair","latent","relay") and k[1]=="deep",
                "chain+ring": lambda k: k[0] in ("chain","ring"), "meta": lambda k: k[0]=="meta",
                "team+star+m2m": lambda k: k[0] in ("team","star","m2m"), "ALL": lambda k: True}.items():
    a, n = tot(f); out[name] = f"{a}/{n} = {a/max(n,1):.2f}"
# 难负例
neg = [s for s in gold if s["type"] == "hard_neg" and set(mem(s)) <= present]
judged_neg = [s for s in neg if tuple(mem(s)[:2]) in all_pairs]
act_neg = [s for s in judged_neg if tuple(mem(s)[:2]) in pair_hold]
out["hard_neg_act_rate"] = f"{len(act_neg)}/{len(judged_neg)} judged (of {len(neg)})"
# AUC：真值两两对 vs 难负例，用 p_hold（未成立记 1-p 的反面 → 用 edge 的 p_hold 原值）
pos_p = [p_yes(all_pairs[k]) for s in gold if s["type"] in ("pair","latent","relay")
         for k in [tuple(mem(s)[:2])] if k in all_pairs]
neg_p = [p_yes(all_pairs[tuple(mem(s)[:2])]) for s in judged_neg]
if pos_p and neg_p:
    auc = sum((p > q) + 0.5 * (p == q) for p in pos_p for q in neg_p) / (len(pos_p) * len(neg_p))
    out["AUC_p_hold"] = round(auc, 3)
j = res.get("joins", [])
if j:
    lat = sorted(x["t_first_opp"] - x["t_join"] for x in j if x.get("t_first_opp"))
    if lat: out["first_opp_p50_s"] = round(lat[len(lat)//2], 2)
    calls = sorted(x.get("calls", 0) for x in j); out["calls_per_join_p50"] = calls[len(calls)//2]
print(json.dumps(out, ensure_ascii=False, indent=1))
_p = sys.argv[1]; _o = os.path.join(os.path.dirname(_p), "eval.json" if os.path.basename(_p) == "result.json" else "eval-" + os.path.basename(_p))
json.dump({"summary": out, "detail": detail}, open(_o, "w"), ensure_ascii=False, indent=1)   # 不覆盖输入
