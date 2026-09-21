# -*- coding: utf-8 -*-
import json, collections, math, random
import os, pathlib
ROOT = str(pathlib.Path(__file__).resolve().parents[3]) + "/"  # 地基/
rows=[json.loads(l) for l in open(ROOT+"foundation/experiments/raw/e_cal/readings.jsonl",encoding="utf-8")]
LETTER={"A":0,"B":1,"C":2,"D":3,"都不是":4}
def lab(r):
    t,a,y=r["type"],r["ans"],r.get("truth")
    if not y: return None
    if t=="noul":   return (a["p"], 1 if y=="act" else 0, r["segments"][0])
    if t=="choice": return (a["p"], 1 if int(a["value"])==LETTER[y] else 0, r["segments"][0])
    return (a["p"], 1 if int(a["value"])+1==int(y) else 0, r["segments"][0])
def cp_upper(k,n,delta):
    if n==0: return 1.0
    if k>=n: return 1.0
    lo,hi=k/n,1.0
    for _ in range(200):
        mid=(lo+hi)/2
        cdf=sum(math.comb(n,i)*mid**i*(1-mid)**(n-i) for i in range(k+1))
        if cdf>delta: lo=mid
        else: hi=mid
    return (lo+hi)/2

print("="*76); print("四、放行区能拿到的**最好**上界：零错时 n 要多大")
print("  要在 δ=0.10 上把风险上界压到 α，且放行区一条都不错，所需放行条数 n = ln(δ)/ln(1-α)：")
for a in (0.05,0.10,0.20,0.30):
    print(f"    α={a:.2f} → n ≥ {math.ceil(math.log(0.10)/math.log(1-a))}")
print()
for t in ("noul","choice","score"):
    S=[x for x in (lab(r) for r in rows if r["type"]==t) if x]
    ps=sorted(set(p for p,_,_ in S))
    best=None
    for th in ps:
        acc=[(p,l) for p,l,_ in S if p>=th]
        k=sum(1 for _,l in acc if l==0)
        if k==0 and acc and (best is None or len(acc)>best[1]): best=(th,len(acc))
    if best:
        th,na=best
        print(f"  {t:6s} n={len(S):3d}  **零错放行区最大** = {na} 条（线 {th:.2f}），该区的精确上界 = {cp_upper(0,na,0.10):.3f}")
    else:
        print(f"  {t:6s} n={len(S):3d}  **没有任何零错放行区**")

print("="*76); print("五、把「一条读数」换成「一个对象段」：簇级保形的代价")
random.seed(7)
for t in ("noul","choice","score"):
    S=[x for x in (lab(r) for r in rows if r["type"]==t) if x]
    by=collections.defaultdict(list)
    for p,l,seg in S: by[seg].append((p,l))
    # 簇级：每簇取一条（多次重采样看线的稳定性）
    lines=[]
    for _ in range(400):
        sub=[random.choice(v) for v in by.values()]
        ps=sorted(set(p for p,_ in sub)); found=None
        for th in [0.0]+[(ps[i]+ps[i+1])/2 for i in range(len(ps)-1)]+[1.0+1e-9]:
            acc=[(p,l) for p,l in sub if p>=th]
            if not acc: break
            if cp_upper(sum(1 for _,l in acc if l==0), len(acc), 0.10)<=0.30: found=th; break
        lines.append(found)
    ok=[x for x in lines if x is not None]
    print(f"  {t:6s} 簇数={len(by):3d}（vs 条数 {len(S)}）  α=0.30 有解的重采样比例={len(ok)/len(lines):.0%}"
          + (f"  线的分布 min/中位/max = {min(ok):.2f}/{sorted(ok)[len(ok)//2]:.2f}/{max(ok):.2f}" if ok else ""))

print("="*76); print("六、δ 非单调：档案里的翻转，以及它是不是噪声")
prof=json.load(open(ROOT+"foundation/profile/profiles/jev-1.13.0.json",encoding="utf-8"))
for k,v in prof["delta"].items():
    i,g=v["immediate"],v["after_gap"]
    print(f"  {k:22s} n {i['n']:>4}→{g['n']:>4}  p95 {i['p95']:.4f}→{g['p95']:.4f} {'↑' if g['p95']>i['p95'] else '↓'}"
          f"   p99 {i['p99']:.4f}→{g['p99']:.4f} {'↑' if g['p99']>i['p99'] else '↓'}"
          f"   max {i['max']:.3f}→{g['max']:.3f} {'↑' if g['max']>i['max'] else '↓'}"
          + ("   **同一栏里 p95 与 p99 方向相反**" if (g['p95']>i['p95'])!=(g['p99']>i['p99']) else ""))
print()
print("  p99 在 n 上的位置（第 ⌈0.99n⌉ 顺序统计量，即「倒数第几个」）：")
for k,v in prof["delta"].items():
    for nm in ("immediate","after_gap"):
        n=v[nm]["n"]; rank=math.ceil(0.99*n)
        print(f"    {k:22s} {nm:10s} n={n:>4}  p99 = 第 {rank} / {n} 个（离最大值差 {n-rank} 个样本）")
