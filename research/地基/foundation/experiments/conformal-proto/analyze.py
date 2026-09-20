# -*- coding: utf-8 -*-
"""保形弃权域设计的实测脚本。只读，不改任何仓库文件。"""
import json, collections, math

import os, pathlib
ROOT = str(pathlib.Path(__file__).resolve().parents[3]) + "/"  # 地基/
rows = [json.loads(l) for l in open(ROOT+"foundation/experiments/raw/e_cal/readings.jsonl", encoding="utf-8")]
LETTER = {"A":0,"B":1,"C":2,"D":3,"都不是":4}

def label_of(r):
    t, a, y = r["type"], r["ans"], r.get("truth")
    if not y: return None
    if t == "noul":   return (a["p"], 1 if y == "act" else 0)
    if t == "choice": return (a["p"], 1 if int(a["value"]) == LETTER[y] else 0)
    if t == "score":  return (a["p"], 1 if int(a["value"]) + 1 == int(y) else 0)

print("="*72); print("一、可交换性：标注集里到底有几个独立单位")
for t in ("noul","choice","score"):
    rs = [r for r in rows if r["type"]==t and r.get("truth")]
    objs = collections.Counter(r["segments"][0] for r in rs)          # 对象段 = 判断的对象
    ctxs = collections.Counter(r["segments"][1] for r in rs if len(r["segments"])>1)
    m = list(objs.values())
    kish = (sum(m)**2)/sum(x*x for x in m)
    print(f"  {t:6s} 有真值 n={len(rs):3d}  不同对象段={len(objs):3d}  "
          f"对象段重复次数={sorted(m, reverse=True)[:6]}…  Kish n_eff={kish:5.1f}  不同语境段={len(ctxs)}")
allrs=[r for r in rows if r.get("truth")]
print(f"  合计    有真值 n={len(allrs)}  不同对象段={len(set(r['segments'][0] for r in allrs))}  "
      f"全体不同片段={len(set(s for r in rows for s in r['segments']))}")

print("="*72); print("二、有限样本：n 决定了你**最小能认证到多少风险**")
def clopper_pearson_upper(k, n, delta):
    """二项比例的精确上置信界（Clopper–Pearson）：P(Bin(n,p) <= k) = delta 的解"""
    if k >= n: return 1.0
    lo, hi = k/n, 1.0
    for _ in range(200):
        mid = (lo+hi)/2
        cdf = sum(math.comb(n,i)*mid**i*(1-mid)**(n-i) for i in range(k+1))
        if cdf > delta: lo = mid
        else: hi = mid
    return (lo+hi)/2
def beta_quantile(a, b, q):
    lo, hi = 0.0, 1.0
    def cdf(x):  # 正则化不完全 Beta，整数参数用二项和
        n = a+b-1
        return sum(math.comb(n,i)*x**i*(1-x)**(n-i) for i in range(a, n+1))
    for _ in range(200):
        mid=(lo+hi)/2
        if cdf(mid) < q: lo=mid
        else: hi=mid
    return (lo+hi)/2

print(f"  {'n':>4} {'最小可认证 α=1/(n+1)':>22} {'Hoeffding 松弛(δ=0.1)':>22} {'α=0.1 时条件覆盖的 10% 分位':>28}")
for n in (17, 20, 26, 45, 55, 73, 74, 202, 297):
    amin = 1/(n+1)
    hoef = math.sqrt(math.log(1/0.1)/(2*n))
    l = math.floor((n+1)*0.1)
    if l >= 1:
        cov10 = beta_quantile(n+1-l, l, 0.10)
        s = f"{cov10:.3f}"
    else:
        s = "α=0.1 在此 n 上不可认证"
    print(f"  {n:>4} {amin:>22.4f} {hoef:>22.3f} {s:>28}")

print("="*72); print("三、代价线（现有）vs 保形风险控制线（拟加）")
def cost_line(samples, fp, fn):
    pts = sorted((float(p), int(l)) for p,l in samples)
    ps=[p for p,_ in pts]
    cands=[0.0]+[(ps[i]+ps[i+1])/2 for i in range(len(ps)-1)]+[1.0+1e-9]
    best_t=best_c=None
    for t in cands:
        c=sum(fp for p,l in pts if p>=t and l==0)+sum(fn for p,l in pts if p<t and l==1)
        if best_c is None or c<best_c or (c==best_c and t>best_t): best_t,best_c=t,c
    return best_t, best_c
def rcps_line(samples, alpha, delta):
    """RCPS：从最宽的线往紧里走，**取第一个上置信界 ≤ α 的线**。风险 = 放行里的假放行率。"""
    pts=sorted((float(p),int(l)) for p,l in samples)
    ps=sorted(set(p for p,_ in pts))
    cands=[0.0]+[(ps[i]+ps[i+1])/2 for i in range(len(ps)-1)]+[1.0+1e-9]
    for t in cands:                       # 从低到高：线越高越保守
        acc=[(p,l) for p,l in pts if p>=t]
        if not acc: return t, 0, 0, 0.0
        k=sum(1 for _,l in acc if l==0)   # 假放行数
        ucb=clopper_pearson_upper(k, len(acc), delta)
        if ucb<=alpha: return t, k, len(acc), ucb
    return None, None, None, None
for t in ("noul","choice","score"):
    S=[label_of(r) for r in rows if r["type"]==t and r.get("truth")]
    S=[x for x in S if x]
    base=sum(l for _,l in S)/len(S)
    print(f"  --- {t}  n={len(S)}  正例率={base:.2f} ---")
    for fp,fn in ((1,1),(5,1),(1,5)):
        tt,cc=cost_line(S,fp,fn)
        acc=[(p,l) for p,l in S if p>=tt]
        err=(sum(1 for _,l in acc if l==0)/len(acc)) if acc else float('nan')
        print(f"    cost_line(fp={fp},fn={fn}) 线={tt:.3f} 经验代价={cc:.0f} 放行 {len(acc):3d}/{len(S)} 经验假放行率={err:.3f}")
    for alpha in (0.05,0.10,0.20,0.30,0.40):
        tt,k,na,ucb=rcps_line(S,alpha,0.10)
        if tt is None: print(f"    RCPS(α={alpha:.2f},δ=0.10) **无解**：任何线都给不出 ≤{alpha} 的上界（连全弃权也不行）")
        else: print(f"    RCPS(α={alpha:.2f},δ=0.10) 线={tt:.3f} 放行 {na:3d}/{len(S)}（弃权 {100*(1-na/len(S)):.0f}%）假放行 {k} 上界={ucb:.3f}")
