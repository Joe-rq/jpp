"""B68 修订：指纹口径候选在现有材料上的固定观察（0 费用）。依据文本：地基/附注/2026-09-24-探针首轮裁定.md「B68 修订」。
复刻 jpp-value::stat::material_fingerprint 与 ScopeRanges::from_texts 的分位取法；在 Rust 之外先算出各判据的预期值，
Rust 实现修订口径后由 run_check.py / control.py 复跑对账。输出存 rule_gradient.out.txt。
用法：python3 rule_gradient.py > rule_gradient.out.txt（路径相对本文件；实测目录在公开仓库里没有）。"""
import json, math, pathlib, re, statistics, collections

RJ = pathlib.Path(__file__).resolve().parents[2]  # rust-jpp
CAL = RJ.parents[1] / "实测/校准题式-2026-09-23"
NAMES = ["字符数", "中文比例", "拉丁字母比例", "数字比例", "标点空白比例", "行数", "平均行长"]
SCALE = {0, 5, 6}   # 尺度量：字符数、行数、平均行长
RATIO = {1, 2, 3, 4}


def fp(text):
    n = cjk = latin = digit = punct = 0
    for c in text:
        n += 1
        o = ord(c)
        if 0x4E00 <= o <= 0x9FFF or 0x3400 <= o <= 0x4DBF:
            cjk += 1
        elif c.isnumeric():
            digit += 1
        elif c.isalpha():
            latin += 1
        else:
            punct += 1
    lines = max(len(text.splitlines()), 1)
    r = lambda k: 0.0 if n == 0 else k / n
    return [float(n), r(cjk), r(latin), r(digit), r(punct), float(lines), n / lines]


def rnd(x):  # Rust f64::round：半数远离零
    return int(math.floor(x + 0.5))


def quantile_ranges(fps, q=(0.01, 0.99)):
    out = []
    for k in range(7):
        xs = sorted(f[k] for f in fps)
        lo = xs[min(rnd((len(xs) - 1) * q[0]), len(xs) - 1)]
        hi = xs[min(rnd((len(xs) - 1) * q[1]), len(xs) - 1)]
        out.append((lo, hi))
    return out


def widen(ranges, m=0.10, k=2.0, medians=None, mode="B"):
    """mode B：比例量加绝对边距 m（夹在 [0,1]），尺度量乘除 k。
    mode M：实施者候选——尺度量改为 [中位数/k, 中位数*k]，比例量同 B。
    mode A：不加边距。"""
    res = []
    for i, (lo, hi) in enumerate(ranges):
        if mode == "A":
            res.append((lo, hi))
        elif i in RATIO:
            res.append((max(0.0, lo - m), min(1.0, hi + m)))
        elif mode == "M":
            res.append((medians[i] / k, medians[i] * k))
        else:
            res.append((lo / k, hi * k))
    return res


def outside(ranges, f, hard=None):
    hard = set(range(7)) if hard is None else hard
    for i in sorted(hard):
        lo, hi = ranges[i]
        if f[i] < lo - 1e-12 or f[i] > hi + 1e-12:
            return NAMES[i]
    return None


# ---------- 材料 ----------
cert = [json.loads(l)["text"] for l in open(RJ / "probes/scope/语义R-带材料.jsonl", encoding="utf-8")
        if "text" in json.loads(l)]
cert = sorted(set(cert))
src = (RJ / "examples/topic-relevance.jpp").read_text(encoding="utf-8")
same_a = re.findall(r'^\s*"([^"]+)",?$', src.split("let notes = [")[1].split("];")[0], re.M)
i2 = json.load(open(CAL / "items2.json", encoding="utf-8"))
by = collections.defaultdict(set)
for x in i2:
    by[x["t"]].add(x["state"]["on"])
l1 = sorted(by["L1"])
same_b1 = [t for t in l1 if "重复一遍" not in t]
same_b2 = [t for t in l1 if "重复一遍" in t]
frag_l3 = sorted(by["L3"])
json_l2 = sorted(by["L2"])
folio = [re.search(r'let doc = mat\("(.*?)"\);', (RJ / "probes/folio/folio.jpp").read_text(encoding="utf-8")).group(1)
         .encode().decode("unicode_escape").encode("latin-1").decode("utf-8")]
win = json.load(open(RJ / "probes/winnow/input.json", encoding="utf-8"))
winnow = [c for r in win["results"] for c in r["chunks"]]
al = (RJ / "probes/entity-align/align.jpp").read_text(encoding="utf-8")
align = [" · ".join(m) for m in re.findall(r'name: "([^"]+)", brewery: "([^"]+)", style: "([^"]+)"', al)]
# 合成：认证句拼接（同比例、倍数长度）——多对象状态（J-14）是范围外的典型形态
import random
random.seed(20260924)
def concat(n, cnt=40):
    return ["".join(random.sample(cert, n)) for _ in range(cnt)]
cat = {n: concat(n) for n in (2, 3, 5, 10)}

sets = collections.OrderedDict([
    ("认证集自身", cert), ("同风格A·topic-relevance 12 句", same_a),
    ("同风格B1·第二轮 L1 单句", same_b1), ("同风格B2·第二轮 L1 重复句(~2x)", same_b2),
    ("片段·第二轮 L3「目标词：X」", frag_l3),
    ("拼接·2 句", cat[2]), ("拼接·3 句", cat[3]), ("拼接·5 句", cat[5]), ("拼接·10 句", cat[10]),
    ("跨风格·folio 合同", folio), ("跨风格·winnow 工具输出", winnow),
    ("跨风格·第二轮 L2 JSON", json_l2), ("跨风格·entity-align 目录", align),
])
fps = {k: [fp(t) for t in v] for k, v in sets.items()}
base = quantile_ranges(fps["认证集自身"])
med = [statistics.median(f[i] for f in fps["认证集自身"]) for i in range(7)]


def report(ranges, hard=None):
    row = {}
    for k, v in fps.items():
        outs = [outside(ranges, f, hard) for f in v]
        n_out = sum(o is not None for o in outs)
        row[k] = (n_out, len(v), collections.Counter(o for o in outs if o))
    return row


def fmt(ranges):
    return "; ".join(f"{NAMES[i]} [{lo:.3g}, {hi:.3g}]" for i, (lo, hi) in enumerate(ranges))


print("认证集 n =", len(cert), " 中位数：", [round(x, 3) for x in med])
print("分位区间 [0.01,0.99]：", fmt(base))
print("\n=== 规则 A（现行）===")
for k, (o, n, c) in report(widen(base, mode="A")).items():
    print(f"  {k}: {o}/{n} 出区间  {dict(c)}")

print("\n=== 规则 B（修订：比例 ±m，尺度 ×/÷k）梯度 ===")
print("m\\k   " + "  ".join(f"{k:>5}" for k in (1, 1.5, 2, 3, 4)))
for key in sets:
    print(f"-- {key}")
    for m in (0, 0.05, 0.10, 0.15, 0.20):
        cells = []
        for k in (1, 1.5, 2, 3, 4):
            o, n, _ = report(widen(base, m=m, k=k))[key]
            cells.append(f"{o:>2}/{n:<3}")
        print(f"  m={m:<4} " + " ".join(cells))

print("\n=== 规则 B 默认 m=0.10 k=2 ===")
R = widen(base, m=0.10, k=2)
print(fmt(R))
for k, (o, n, c) in report(R).items():
    print(f"  {k}: {o}/{n} 出区间  {dict(c)}")

print("\n=== 实施者候选 M（尺度量 = 中位数 ÷×2；比例 ±0.10）===")
RM = widen(base, m=0.10, k=2, medians=med, mode="M")
print(fmt(RM))
for k, (o, n, c) in report(RM).items():
    print(f"  {k}: {o}/{n} 出区间  {dict(c)}")
# 自洽性反例：宽认证集上中位数法把自己的材料判出去
wide = cert + cat[2] + cat[3] + cat[5]
wfps = [fp(t) for t in wide]
wq = quantile_ranges(wfps)
wmed = [statistics.median(f[i] for f in wfps) for i in range(7)]
selfB = sum(outside(widen(wq, 0.10, 2), f) is not None for f in wfps)
selfM = sum(outside(widen(wq, 0.10, 2, wmed, "M"), f) is not None for f in wfps)
print(f"宽认证集（认证句 + 2/3/5 句拼接，n={len(wide)}，字符数 q01–q99 = [{wq[0][0]:.0f}, {wq[0][1]:.0f}]，中位数 {wmed[0]:.0f}）自判范围外：B {selfB}/{len(wide)}，M {selfM}/{len(wide)}")

print("\n=== 规则 D（只有比例量硬；长度不算）m=0.10 ===")
for k, (o, n, c) in report(widen(base, m=0.10, k=2), hard=RATIO).items():
    print(f"  {k}: {o}/{n} 出区间  {dict(c)}")

print("\n=== 规则 E（硬：比例 + 行数；软：字符数、平均行长）m=0.10 k=2 ===")
for k, (o, n, c) in report(widen(base, m=0.10, k=2), hard=RATIO | {5}).items():
    print(f"  {k}: {o}/{n} 出区间  {dict(c)}")

print("\n=== 各集的指纹范围（min–max）===")
for k, v in fps.items():
    print(f"-- {k} (n={len(v)})")
    for i in range(7):
        xs = [f[i] for f in v]
        print(f"   {NAMES[i]}: [{min(xs):.3g}, {max(xs):.3g}]")
