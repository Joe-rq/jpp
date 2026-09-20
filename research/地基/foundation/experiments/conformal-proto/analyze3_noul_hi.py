# -*- coding: utf-8 -*-
"""E-NOUL-HI 预注册的全部数字（只读）。跑法：python3 analyze3_noul_hi.py"""
import json, collections, math, pathlib, statistics
ROOT = str(pathlib.Path(__file__).resolve().parents[3]) + "/"
rows = [json.loads(l) for l in open(ROOT+"foundation/experiments/raw/e_cal/readings.jsonl", encoding="utf-8")]
N = [r for r in rows if r["type"] == "noul"]
lab = [r for r in N if r.get("truth")]; unl = [r for r in N if not r.get("truth")]

def cp(k, n, d=0.10):
    if n == 0 or k >= n: return 1.0
    lo, hi = k/n, 1.0
    for _ in range(200):
        m = (lo+hi)/2
        c = sum(math.comb(n, i)*m**i*(1-m)**(n-i) for i in range(k+1))
        lo, hi = (m, hi) if c > d else (lo, m)
    return (lo+hi)/2

print(f"noul 100 条：已标注 {len(lab)}（全部 模型双标）、未标注 {len(unl)}（待人抽检）")
print(f"已标注 202 条总体里 fable==opus 的比例 = {sum(1 for r in rows if r.get('truth') and r['fable']==r['opus'])/202:.3f}")
print(f"未标注 95 条里 fable==opus 的比例 = {sum(1 for r in rows if not r.get('truth') and r['fable']==r['opus'])/95:.3f}")
print(f"\n{'线':>7}{'放行区':>8}{'已标注':>8}{'错':>5}{'未标注':>8}{'对象段':>8}")
for th in (0.70, 0.75, 0.775, 0.80, 0.90):
    a = [r for r in N if r["ans"]["p"] >= th]; l = [r for r in a if r.get("truth")]
    e = sum(1 for r in l if r["truth"] != "act")
    print(f"{th:>7.3f}{len(a):>8}{len(l):>8}{e:>5}{len(a)-len(l):>8}{len(set(r['segments'][0] for r in a)):>8}")
print(f"\n全部 100 条标完，p≥0.775 仍只有 {len([r for r in N if r['ans']['p']>=0.775])} 条——22 条不在这批材料里")
print(f"按 7/100 出现率，凑 22 条高读数样本需约 {math.ceil(22/0.07)} 条新读数（须落在新片段上）")
print(f"放行区 6 个对象段 → 簇级最小可认证 α = 1/(6+1) = {1/7:.3f}，α=0.10 无论如何认证不到")
e73 = sum(1 for r in lab if (r["ans"]["p"] >= 0.5) != (r["truth"] == "act"))
print(f"\n基线：已标注 73 条上 p≥0.5 当判断的错误率 = {e73}/73 = {e73/73:.3f}")
print(f"读数中位：模型双标 {statistics.median([r['ans']['p'] for r in lab]):.3f} / 待人抽检 {statistics.median([r['ans']['p'] for r in unl]):.3f}")
print(f"未标注 27 条：fable==opus {sum(1 for r in unl if r['fable']==r['opus'])} 条、不一致 {sum(1 for r in unl if r['fable']!=r['opus'])} 条")
hi1 = [r for r in unl if r["ans"]["p"] >= 0.775]
for r in hi1:
    print(f"\n放行区唯一未标注的：{r['id']} p={r['ans']['p']} fable={r['fable']} opus={r['opus']}")
    print(f"  对象段：{r['segments'][0][:60]}")
print(f"\n7 条全标后：零错 → 上界 {cp(0,7):.3f}（仍认证不到 α=0.10）；错 1 条 → 上界 {cp(1,7):.3f}（比今天的 0.319 更差）")
