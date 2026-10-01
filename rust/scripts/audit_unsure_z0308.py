#!/usr/bin/env python3
"""Z0308 回查（只读）：扫仓库里全部已跟踪的校准记录，列 select（choice）与 measure（score）记录的
`unsure_rate`、样本上的置换众数，以及按 `cut` 口径的重算值。

背景：`jpp-calib` 的 calib-import 折样本时写死 `perms: 0, mode_share: None`（`truth.rs`），K 元单侧上岗
（`commission.rs` 的 `单侧上岗`）又拿空表当众数；`经验unsure率` 的 select 分支把没有众数的样本一律记成未决
（与 `cut` 的 `Unsure(untested)` 同口径）。于是经导入认证的 select 记录 `unsure_rate` 恒为 1.0，与读数无关。

本脚本不写任何文件，只打印。用法（在仓库任一子目录）：

    python3 地基/rust-jpp/scripts/audit_unsure_z0308.py [--rev HEAD] [--md]

对每条 select / measure 记录给出：
- `存`：记录里的 `unsure_rate` 与 `unsure_rate_delta`；
- `样本众数`：带标注样本里 `mode_share` 非空的条数、其中 ≥ 1 的条数、< 1 的条数；
- `现行重算`：按样本上现有的众数、`cut` 的判据（select：众数空 → untested、< 1 → tie、≥ 1 且 p ≥ hi + δ → pick；
  measure：p ≥ hi + δ → at），δ 取 `unsure_rate_delta`（为空取 0）。它等于 `存` 说明记录与现行规则一致；
- `两序来源`：同目录（或题库条目按 form_hash 对应的评估目录）有没有两序读数（`readings.json` 带 `mode_share`、
  或账本 `Judge` 条目带 `perm`）；
- `真值`：有两序读数时，按标注行 `item` 接上每条的 `mode_share`，用同一判据重算——这是修好后应记的数；
- `分类`：(a) 有两序读数而记 1.0（受影响）；(b) 读数是单序（样本与来源都没有众数，cut 真会出 untested，1.0 就是对的）；
  (c) 样本已带众数（不经导入写入，现行代码重算会变）；measure 另列「measure 一致 / 不一致」。
另外列出：已上岗 select 记录里「众数 < 1 而 p ≥ hi + δ」的样本条数（这些样本在运行期出 tie，却在认证里算已决；
非零说明线本身也受影响，要停下交回主控）。
"""

import argparse
import json
import os
import subprocess
import sys

EPS = 1e-12
# 题库 F3、F4 的两序读数（入库来源，条目说明 1.6）
TWO_ORDER = {
    "e49770913911a79525896988": "地基/评估/27a-置换重跑/F3-提到哪一个",
    "f4886dabf4e089bbdfd9f432": "地基/评估/27a-置换重跑/F4-同一个",
}


def git(*args):
    return subprocess.run(["git", "-c", "core.quotepath=off", *args], capture_output=True, text=True, check=True).stdout


def show(rev, path):
    r = subprocess.run(["git", "show", f"{rev}:{path}"], capture_output=True, text=True)
    return r.stdout if r.returncode == 0 else None


def records_in(obj, path=""):
    if isinstance(obj, dict):
        s = obj.get("samples")
        if isinstance(s, list) and any(isinstance(x, dict) and "phys" in x for x in s):
            yield path, obj
        for k, v in obj.items():
            if k == "samples":
                continue
            yield from records_in(v, f"{path}/{k}")
    elif isinstance(obj, list):
        for i, v in enumerate(obj):
            yield from records_in(v, f"{path}[{i}]")


def parse(txt):
    try:
        return json.loads(txt)
    except json.JSONDecodeError:
        return [json.loads(l) for l in txt.splitlines() if l.strip()]


def decided_up(p, hi, d):
    return p >= hi + d - EPS


def unsure(samples, phys, hi, d):
    n = len(samples)
    if n == 0:
        return None
    if phys == "choice":
        u = sum(
            1
            for s in samples
            if not (s.get("mode_share") is not None and s["mode_share"] >= 1.0 and decided_up(s["p"], hi, d))
        )
    else:
        u = sum(1 for s in samples if not decided_up(s["p"], hi, d))
    return round(u / n * 10000) / 10000


def ledger_choice(txt):
    """账本里 choice 读数：[(p_max, mode_share 或 None)]。"""
    out = []
    try:
        obj = parse(txt)
    except json.JSONDecodeError:
        return out
    for e in obj if isinstance(obj, list) else []:
        j = (e.get("entry") or {}).get("Judge") if isinstance(e, dict) else None
        if not j or "Choice" not in (j.get("answer") or {}):
            continue
        perm = j.get("perm")
        out.append((max(j["answer"]["Choice"]), perm.get("mode_share") if perm else None))
    return out


def candidates(rev, f, rec):
    """读数来源候选：(名字, [(p_max, mode_share)])——同目录、上一级目录的 readings.json 与账本，以及题库 F3/F4 的入库来源。"""
    key = rec.get("key", "")
    h = key.split("\x1f")[-1] if "\x1f" in key else ""
    d = os.path.dirname(f)
    dirs = [d, os.path.dirname(d)]
    if h in TWO_ORDER:
        dirs.append(TWO_ORDER[h])
    seen, out = set(), []
    for c in dirs:
        if c in seen:
            continue
        seen.add(c)
        for x in git("ls-tree", "--name-only", rev, f"{c}/").splitlines():
            if not x.endswith((".json", ".jsonl")) or x == f:
                continue
            t = show(rev, x) or ""
            if os.path.basename(x) == "readings.json":
                rd = json.loads(t)
                out.append((x, [(v["p"], v.get("mode_share")) for v in rd.values() if isinstance(v, dict) and "p" in v]))
            elif '"Judge"' in t and '"Choice"' in t:
                out.append((x, ledger_choice(t)))
    return out


def ms_mset(xs):
    return sorted(round(p, 9) for p in xs)


def match_source(rev, f, rec, lab):
    """找读数与样本 p 多重集相同的来源；返回 (来源, [(p_max, mode_share)…] 或 None)。"""
    want = ms_mset(s["p"] for s in lab)
    for name, rd in candidates(rev, f, rec):
        if ms_mset(p for p, _ in rd) == want:
            return name, rd
    return "无（未找到读数一致的来源）", None


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--rev", default="HEAD")
    ap.add_argument("--md", action="store_true")
    a = ap.parse_args()
    os.chdir(git("rev-parse", "--show-toplevel").strip())
    files = [
        x.split(":", 1)[1]
        for x in git("grep", "-l", "-z", '"phys"', a.rev, "--", "*.json", "*.jsonl").split("\0")
        if x
    ]
    rows = []
    for f in files:
        txt = show(a.rev, f)
        try:
            obj = parse(txt)
        except json.JSONDecodeError:
            print(f"# 解析失败：{f}", file=sys.stderr)
            continue
        for rpath, rec in records_in(obj):
            lab = [s for s in rec["samples"] if isinstance(s, dict) and s.get("label") is not None and s.get("p") is not None]
            physes = sorted({s.get("phys") for s in lab})
            if physes not in (["choice"], ["score"]):
                continue
            phys = physes[0]
            hi = rec.get("hi")
            d = rec.get("unsure_rate_delta") or 0.0
            ms_n = [s for s in lab if s.get("mode_share") is not None]
            ms_lt1 = [s for s in ms_n if s["mode_share"] < 1.0]
            now = unsure(lab, phys, hi, d) if hi is not None else None
            src = "—"
            truth_val = None
            tie_accepted = None
            if phys == "choice":
                src, rd = match_source(a.rev, f, rec, lab)
                ms_list = None if rd is None else [m for _, m in rd]
                if ms_list is not None and any(m is not None for m in ms_list):
                    if all(m is not None and m >= 1.0 for m in ms_list):
                        # 两序众数全一致：每条都按众数 1 重算
                        truth_val = unsure([{"p": s["p"], "mode_share": 1.0} for s in lab], phys, hi, d)
                    else:
                        truth_val = "众数不全为 1，需按 item 接"
                if rec.get("status") == "上岗":
                    # 样本上、或来源读数上众数 < 1 而 p ≥ hi + δ 的条数（运行期出 tie，认证里却算已决）
                    tie_accepted = sum(1 for s in ms_lt1 if decided_up(s["p"], hi, d)) + sum(
                        1 for p, m in (rd or []) if m is not None and m < 1.0 and decided_up(p, hi, d)
                    )
                if ms_n:
                    cls = "(c) 样本已带众数"
                elif ms_list is not None and any(m is not None for m in ms_list):
                    cls = "(a) 两序读数而记 1.0" if rec.get("unsure_rate") == 1.0 else "(a?) 两序读数"
                elif ms_list is not None:
                    cls = "(b) 单序读数，1.0 与 cut 一致"
                else:
                    cls = "(?) 未找到读数来源，需人工核"
            else:
                cls = "measure 一致" if now == rec.get("unsure_rate") else "measure 不一致"
            rows.append(
                {
                    "file": f,
                    "rec": rpath or "/",
                    "op": "select" if phys == "choice" else "measure",
                    "status": rec.get("status"),
                    "n": len(lab),
                    "hi": hi,
                    "urd": rec.get("unsure_rate_delta"),
                    "stored": rec.get("unsure_rate"),
                    "ms": f"{len(ms_n)}/{len(ms_n) - len(ms_lt1)}/{len(ms_lt1)}",
                    "now": now,
                    "src": src,
                    "truth": truth_val,
                    "tie_accepted": tie_accepted,
                    "class": cls,
                    "via_import": "truth" in rec,
                }
            )
    print(f"# 含 phys 的 JSON {len(files)} 个；select / measure 记录 {len(rows)} 条", file=sys.stderr)
    if a.md:
        print("| # | 文件 | 记录位置 | 题型 | 状态 | 样本 | hi | δ(率) | 存 unsure_rate | 样本众数 有/≥1/<1 | 现行重算 | 两序来源 | 真值（接两序众数） | 已决却 tie | 经导入 | 分类 |")
        print("|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|")
        for i, r in enumerate(rows, 1):
            print(
                f"| {i} | `{r['file']}` | `{r['rec']}` | {r['op']} | {r['status']} | {r['n']} | {r['hi']} | {r['urd']} | "
                f"{r['stored']} | {r['ms']} | {r['now']} | {r['src']} | {r['truth'] if r['truth'] is not None else '—'} | "
                f"{r['tie_accepted'] if r['tie_accepted'] is not None else '—'} | {'是' if r['via_import'] else '否'} | {r['class']} |"
            )
    else:
        json.dump(rows, sys.stdout, ensure_ascii=False, indent=1)
        print()


if __name__ == "__main__":
    main()
