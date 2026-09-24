"""生成 T1 任务书用的 `labels.jsonl`（合成真值，构造检验用，不是模型评测；不计行）。

目标：用这份标注经拆分认证（`certify_ref.py`，与 `jpp calib-import` 同算法）得出的线，
在固定观察的每条读数上给出与 T0 手写阈值相同的出口，于是 T1 与 T0 共用同一份 `expected.json`
（T1 任务书 (d) 验收 ③「线相同则 T0 验收照常通过」取「出口相同」的形式；过程记录 31-1 §二）。

每个键的构造（是非题）：
  清楚的「否」区 [0, l*]：均匀取值，真值为否；
  清楚的「是」区 [h*, 1]：均匀取值，真值为是；
  混杂区：在「非否」最小读数与「非是」最大读数之间等距若干个取值点，每点 6 行起、真假都有，并且按认证的分半规则（与 `certify_ref.split`
  同一规范序与 splitmix64 种子）排定真假，使选线半在每个混杂点上真、假各至少 2 行——
  否则选线半在某点上只有 1 个错（约 40 行上 1 个错的上界仍 ≤ α），线被选进混杂区，认证半就过不了。
  h* 取固定观察里 T0 判「是」的最小读数与判「非是」的最大读数的中点；l* 同理取「否」一侧。
K 选一 / 打分只有上侧：[h*, 1] 上 argmax 正确，其下一半对一半错。
行数与混杂点数固定；随机数种子固定，文件可复现。

用法：python3 make_labels_t1.py        读五个项目的夹具，写 baseline/t1/labels.jsonl
"""
import json
import pathlib
import random
import sys

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))
from certify_ref import SEED, splitmix64  # noqa: E402

HERE = pathlib.Path(__file__).resolve().parent
ROOT = HERE.parent
PROBES = ROOT / "probes"
FIXTURES = {
    "winnow": PROBES / "winnow/fixture.json",
    "folio": PROBES / "folio/fixture.json",
    "entity-align": PROBES / "entity-align/fixture.json",
    "trial-refund": ROOT.parent / "评估/2026-09-24-试写/refund/fixture.json",
    "trial-interview": ROOT.parent / "评估/2026-09-24-试写/interview/fixture.json",
}
DELTA = {"test": 0.05, "select": 0.15, "measure": 0.15}
N_CLEAR = 80       # 每个清楚区的行数（每半约 40，够零错误所需的 22；混杂点一进放行区上界即超 α）
N_MIX = 12         # 混杂区取值点数（每点 6 行）


def p_of(o):
    a = o["answer"]
    return a["Noul"] if "Noul" in a else max(a.get("Choice") or a.get("Score"))


def targets(fx, key, op):
    t0 = {c["key"]: c for c in fx["calibrations"]}[key]
    d = DELTA[op]
    ps = sorted({p_of(o) for o in fx["observations"] if o.get("calib") == key})
    yes = [p for p in ps if p >= t0["hi"] + d]
    not_yes = [p for p in ps if p < t0["hi"] + d]
    h = (min(yes) + max(not_yes)) / 2 if yes and not_yes else t0["hi"] + d
    top = max(not_yes) if not_yes else h - 0.01
    if op != "test":
        return h, None, top, min(0.34, top)
    no = [p for p in ps if p <= t0["lo"] - d]
    not_no = [p for p in ps if p > t0["lo"] - d]
    l = (max(no) + min(not_no)) / 2 if no and not_no else t0["lo"] - d
    bot = min(not_no) if not_no else l + 0.01
    if bot > top:
        # 夹具里这道题没有「未决」读数：是区与否区之间按四分点留出混杂区，两条线之间至少隔 2δ
        lo_p, hi_p = max(no), min(yes)
        l, h = lo_p + 0.25 * (hi_p - lo_p), lo_p + 0.75 * (hi_p - lo_p)
        top, bot = h - 0.01, l + 0.01
    return h, l, top, bot


def rows_for(key, op, h, l, top, bot, rng):
    out = []
    n = 0

    def add(p, ok):
        nonlocal n
        p = round(p, 4)
        if op == "test":
            out.append({"key": key, "item": f"{key}-{n:04d}", "p": p, "label": ok, "source": "computed"})
        else:
            pick = rng.randrange(3)
            out.append({"key": key, "op": op, "item": f"{key}-{n:04d}", "p": p, "pick": pick,
                        "label": pick if ok else (pick + 1) % 3, "source": "computed"})
        n += 1

    for _ in range(N_CLEAR):
        add(h + (1 - h) * rng.random(), True)
    if op == "test":
        for _ in range(N_CLEAR):
            add(l * rng.random(), False)
    # 混杂点两端取在夹具里「非是」的最大读数与「非否」的最小读数上，使选出的线落在这两个读数之外
    step = (top - bot) / (N_MIX - 1)
    mix = sorted({round(bot + step * i, 4) for i in range(N_MIX)})
    # 规范序 = 按 (p, 真值位) 排序，同一 p 上假在前。一个混杂点的下标只取决于比它小的行，
    # 所以按 p 从小到大逐点定：从 6 行起加行，直到存在切分 k 使前 k 行（假）与其余（真）在选线半里各至少 2 行——
    # 放行区只要吞进一个混杂点，选线半上就多至少 2 个错，约 40 行上的二项上界即超 α，线不会选进混杂区。
    base = [r["p"] for r in out]
    below = 0
    plan = []
    for m in mix:
        first = sum(1 for x in base if x < m) + below
        n_row = 6
        while True:
            half = [splitmix64(SEED ^ (first + j)) & 1 for j in range(n_row)]
            ks = [k for k in range(1, n_row) if half[:k].count(0) >= 2 and half[k:].count(0) >= 2]
            if ks:
                break
            n_row += 1
        plan.append((m, n_row, ks[len(ks) // 2]))
        below += n_row
    for m, n_row, k in plan:
        for j in range(n_row):
            add(m, j >= k)
    return out


def main():
    for proj, path in FIXTURES.items():
        fx = json.loads(path.read_text(encoding="utf-8"))
        rng = random.Random(f"t1-labels-{proj}")
        ops = {}
        for o in fx["observations"]:
            ops.setdefault(o["calib"], o["op"])
        rows = []
        for key in sorted(ops):
            rows += rows_for(key, ops[key], *targets(fx, key, ops[key]), rng)
        d = PROBES / proj / "baseline" / "t1"
        d.mkdir(parents=True, exist_ok=True)
        with open(d / "labels.jsonl", "w", encoding="utf-8") as fh:
            for r in rows:
                fh.write(json.dumps(r, ensure_ascii=False) + "\n")
        print(proj, len(rows), "行")


if __name__ == "__main__":
    main()
