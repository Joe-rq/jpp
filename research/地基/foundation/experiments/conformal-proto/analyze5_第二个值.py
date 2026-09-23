# -*- coding: utf-8 -*-
"""那个「第二个值」：未标注的 95 条与已标注的 202 条像不像。无真值，$0。
预注册见同目录 `预注册-第二个值.md`——**先写了什么结果下答不了，再跑的**。"""
import json, csv, collections, math, pathlib, statistics
ROOT = pathlib.Path(__file__).resolve().parents[3]
rows = [json.loads(l) for l in open(ROOT/"foundation/experiments/raw/e_cal/readings.jsonl", encoding="utf-8")]
conf = {r["id"]: r for r in csv.DictReader(open(ROOT/"foundation/experiments/e_cal_labels_v2_merged.csv", encoding="utf-8"))}
LETTER = {"A":0,"B":1,"C":2,"D":3,"都不是":4}
档 = {"高":2, "中":1, "低":0}

def 组(r):
    c = conf.get(r["id"], {})
    if r.get("truth"): return "已标注"
    return "不一致" if c.get("fable") != c.get("opus") else "一致低置信"
def 对不对(r):
    t, a, y = r["type"], r["ans"], r.get("truth")
    if not y: return None
    if t == "noul":   return (a["p"] >= 0.5) == (y == "act")
    if t == "choice": return int(a["value"]) == LETTER[y]
    return int(a["value"]) + 1 == int(y)
def ks(A, B):
    allv = sorted(A + B)
    return max(abs(sum(1 for x in A if x <= t)/len(A) - sum(1 for x in B if x <= t)/len(B)) for t in allv)
def crit(n1, n2): return 1.36 * math.sqrt(1/n1 + 1/n2)

G = collections.defaultdict(list)
for r in rows: G[组(r)].append(r)
print("=== 三组 ===")
for g in ("已标注","一致低置信","不一致"):
    v = G[g]; ps = [r["ans"]["p"] for r in v]
    print(f"  {g:8s} n={len(v):3d}  读数中位={statistics.median(ps):.3f}  |p−0.5| 中位={statistics.median([abs(p-0.5) for p in ps]):.3f}")

print("\n=== 一、读数分布比对（预注册的判据：KS < 临界 → 答不了）===")
A = [r["ans"]["p"] for r in G["已标注"]]
for g in ("不一致","一致低置信"):
    B = [r["ans"]["p"] for r in G[g]]
    k, c = ks(A, B), crit(len(A), len(B))
    判 = "**答不了**（低于临界，「一样」与「看不出来」在输出上同一个东西）" if k < c else "**判得出：不像**"
    print(f"  已标注(202) vs {g}({len(B)}):  KS={k:.3f}  临界={c:.3f}  → {判}")

print("\n=== 二、在**有真值**的 202 条上测「置信 → Jev 判得对不对」===")
byc = collections.defaultdict(list)
for r in G["已标注"]:
    c = conf.get(r["id"], {})
    lo = min(档.get(c.get("置信_fable","中"),1), 档.get(c.get("置信_opus","中"),1))
    byc[lo].append(对不对(r))
名 = {2:"两方都高", 1:"最低是中", 0:"最低是低"}
for lo in (2,1,0):
    v = byc.get(lo, [])
    if v: print(f"  {名[lo]:8s} n={len(v):3d}  Jev 错误率={1-sum(v)/len(v):.3f}")
高 = byc.get(2,[]); 中低 = byc.get(1,[])+byc.get(0,[])
if 高 and 中低:
    e高, e中低 = 1-sum(高)/len(高), 1-sum(中低)/len(中低)
    print(f"  → 高 {e高:.3f} vs 中低 {e中低:.3f}，差 {e中低-e高:+.3f}")

print("\n=== 三、外推（只对 77 条一致低置信；18 条不一致在构造上没有对照组）===")
n_lab, n_low, n_dis = len(G["已标注"]), len(G["一致低置信"]), len(G["不一致"])
e_lab = 1 - sum(1 for r in G["已标注"] if 对不对(r))/n_lab
if 高 and 中低:
    e_ext = (e_lab*n_lab + e中低*n_low) / (n_lab + n_low)
    print(f"  202 条实测错误率            {e_lab:.3f}")
    print(f"  把 77 条按「中低置信」那一档补进去 {e_ext:.3f}   （+{e_ext-e_lab:.3f}）")
    print(f"  **剩下的 {n_dis} 条不一致不外推**——202/202 全是「一致」，这一档没有对照组")
    print(f"  若那 18 条的错误率是 0.5（相当于掷硬币）：全体 {(e_lab*n_lab+e中低*n_low+0.5*n_dis)/(n_lab+n_low+n_dis):.3f}")
    print(f"  若那 18 条的错误率是 1.0（上界）：      全体 {(e_lab*n_lab+e中低*n_low+1.0*n_dis)/(n_lab+n_low+n_dis):.3f}")

print("\n=== 四、去混淆：置信与题型相关吗？（分题型各测一次）===")
print(f"  {'题型':6s}{'两方都高 n/错误率':>22}{'最低是中低 n/错误率':>24}{'差':>10}")
for t in ("noul","choice","score"):
    v = [r for r in G["已标注"] if r["type"]==t]
    hi=[对不对(r) for r in v if min(档.get(conf[r['id']].get('置信_fable','中'),1), 档.get(conf[r['id']].get('置信_opus','中'),1))==2]
    lo=[对不对(r) for r in v if min(档.get(conf[r['id']].get('置信_fable','中'),1), 档.get(conf[r['id']].get('置信_opus','中'),1))<2]
    if hi and lo:
        a,b = 1-sum(hi)/len(hi), 1-sum(lo)/len(lo)
        print(f"  {t:6s}{f'{len(hi)} / {a:.3f}':>22}{f'{len(lo)} / {b:.3f}':>24}{b-a:>+10.3f}")
    else:
        print(f"  {t:6s}  一档为空，分不开（hi={len(hi)} lo={len(lo)}）")
print("\n  未标注 95 条的题型分布：", collections.Counter(r['type'] for r in rows if not r.get('truth')))
print("  已标注 202 条的题型分布：", collections.Counter(r['type'] for r in rows if r.get('truth')))

print("\n=== 五、分题型外推（合并那个 +0.063 是被题型构成拖出来的，作废）===")
print(f"  {'题型':6s}{'已标注错误率':>14}{'一致低置信 n':>14}{'该型代理差':>12}{'外推后':>10}{'变化':>9}")
tot_a=tot_b=0
for t in ("noul","choice","score"):
    v=[r for r in G["已标注"] if r["type"]==t]
    e=1-sum(1 for r in v if 对不对(r))/len(v)
    lo=[对不对(r) for r in v if min(档.get(conf[r['id']].get('置信_fable','中'),1), 档.get(conf[r['id']].get('置信_opus','中'),1))<2]
    hi=[对不对(r) for r in v if min(档.get(conf[r['id']].get('置信_fable','中'),1), 档.get(conf[r['id']].get('置信_opus','中'),1))==2]
    e_lo = 1-sum(lo)/len(lo)
    n_low=len([r for r in G["一致低置信"] if r["type"]==t])
    ext=(e*len(v)+e_lo*n_low)/(len(v)+n_low)
    tot_a+=e*len(v); tot_b+=e*len(v)+e_lo*n_low
    print(f"  {t:6s}{e:>14.3f}{n_low:>14}{1-sum(lo)/len(lo)-(1-sum(hi)/len(hi)):>+12.3f}{ext:>10.3f}{ext-e:>+9.3f}")
N_lab=sum(1 for r in G["已标注"]); N_low=len(G["一致低置信"])
print(f"  {'合计':6s}{tot_a/N_lab:>14.3f}{N_low:>14}{'':>12}{tot_b/(N_lab+N_low):>10.3f}{tot_b/(N_lab+N_low)-tot_a/N_lab:>+9.3f}")
print("\n  **score 那一档代理是反的（-0.062），所以它的外推等于不动**——而 score 恰是标注覆盖最低的那型（0.55）。")

print("\n=== 六、那个外推是**下界**不是估计 ===")
import csv as _csv
_rs=list(_csv.DictReader(open(ROOT/"foundation/experiments/e_cal_labels_v2_merged.csv", encoding="utf-8")))
def _g(r):
    if r["真值"]: return "已标注"
    return "不一致" if r["fable"]!=r["opus"] else "一致低置信"
a=[r for r in _rs if _g(r)=="已标注"]; b=[r for r in _rs if _g(r)=="一致低置信"]
print(f"  已标注 202 条里**至少一方置信高**的：{sum(1 for r in a if '高' in (r['置信_fable'],r['置信_opus']))}/202")
print(f"  一致低置信 77 条里有高的：           {sum(1 for r in b if '高' in (r['置信_fable'],r['置信_opus']))}/77")
print("  → 拿「已标注里最低是中低」那 55 条的错误率去补 77 条，**补低了**：")
print("    那 55 条**每一条都还有一方是高**，而 77 条**一条都没有**。")
print("    所以 +0.090 / +0.059 是**下界**，前提是置信单调预测错误（noul/choice 成立，score 不成立）。")
