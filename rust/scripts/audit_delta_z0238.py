#!/usr/bin/env python3
"""Z0238 回查（只读）：扫仓库里全部已跟踪的校准记录，列 select / measure 记录的 δ、hi 与来历。

背景：`jpp-calib/src/truth.rs` 的 calib-import 在记录无 δ 时取画像 δ 先验，题型反查把语义操作名
（select / measure）交给只认物理名（noul / choice / score）的 `反查题型`，落到 noul 的 δ。
这条错路径自 `db7000356`（步 15d-2，2026-09-25 10:23）起存在。

本脚本不写任何文件，只打印。用法（在仓库根或任一子目录）：

    python3 地基/rust-jpp/scripts/audit_delta_z0238.py [--rev HEAD] [--md]

判定：
- 记录的题型按带标注样本的 `phys` 定（choice → select，score → measure），noul 不在本表。
- 每张证书用的 δ：`selection.delta`，没有则记录的 `delta`；`selection` 为空（certify 线、代价线）δ 取 0，不受影响。
- 与发行画像 `profiles/jev-1.13.0.json` 的 `delta.*.immediate.p99` 对照：
  等于本题型的先验 → 正确；等于 noul 先验而本题型先验不同 → 错路径；都不等 → 其他（列出，人工看）。
- 来历：`git log` 里触碰该文件的首个与最后一个提交；最后写入的提交是否在 `db7000356` 之后
  （之前的提交里这条错路径还不存在，那时的 δ 取法见步 15d-2 之前的代码）。
"""

import argparse
import json
import re
import subprocess
import sys

BUG_COMMIT = "db7000356"
PROFILE = "地基/rust-jpp/profiles/jev-1.13.0.json"
PHYS_OP = {"choice": "select", "score": "measure"}
EPS = 1e-9


def git(*args):
    return subprocess.run(["git", *args], capture_output=True, text=True, check=True).stdout


def repo_root():
    return git("rev-parse", "--show-toplevel").strip()


def priors(rev):
    d = json.loads(git("show", f"{rev}:{PROFILE}"))["delta"]
    return {
        "noul": d["noul"]["immediate"]["p99"],
        "choice": d["choice_prob_chosen"]["immediate"]["p99"],
        "score": d["score"]["immediate"]["p99"],
    }


def records_in(obj, path=""):
    """递归找「带 samples 列表且样本有 phys 的字典」。"""
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


def rule_delta(cert_key):
    m = re.search(r"δ=Some\(([0-9.eE+-]+)\)", cert_key or "")
    return float(m.group(1)) if m else None


def history(rev, path):
    out = git("log", "--format=%h|%ad|%s", "--date=format:%m-%d %H:%M", rev, "--", path)
    lines = [l for l in out.splitlines() if l]
    return lines


def after_bug(commit):
    r = subprocess.run(["git", "merge-base", "--is-ancestor", BUG_COMMIT, commit])
    return r.returncode == 0


def verdict(used, phys, pr):
    if used is None:
        return "无δ"
    if abs(used - pr[phys]) < EPS:
        return "正确"
    if abs(used - pr["noul"]) < EPS and abs(pr[phys] - pr["noul"]) > EPS:
        return "错路径(noul δ)"
    return "其他"


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--rev", default="HEAD")
    ap.add_argument("--md", action="store_true", help="打印 Markdown 表")
    a = ap.parse_args()
    root = repo_root()
    import os

    os.chdir(root)
    pr = priors(a.rev)
    # 候选：该修订上所有含 "phys" 的 .json（git grep 读对象库，不看工作区）
    files = [
        f.split(":", 1)[1]
        for f in git("grep", "-l", "-z", '"phys"', a.rev, "--", "*.json").split("\0")
        if f
    ]
    rows = []
    scanned = 0
    unparsed = []
    for f in files:
        txt = git("show", f"{a.rev}:{f}")
        try:
            obj = json.loads(txt)
        except json.JSONDecodeError as e:
            # 账本是逐行 JSON（扩展名 .json）：头行的 calib_used 可能内嵌记录，逐行解析
            try:
                obj = [json.loads(l) for l in txt.splitlines() if l.strip()]
            except json.JSONDecodeError:
                unparsed.append((f, str(e)[:60]))
                continue
        scanned += 1
        for rpath, rec in records_in(obj):
            labeled = [s for s in rec["samples"] if isinstance(s, dict) and s.get("label") is not None]
            physes = sorted({s.get("phys") for s in (labeled or rec["samples"]) if isinstance(s, dict)})
            kary = [p for p in physes if p in PHYS_OP]
            if not kary:
                continue
            phys = kary[0]
            certs = rec.get("certs") or {}
            hist = None
            for ck, c in (certs.items() if certs else [(None, None)]):
                sel = (c or {}).get("selection")
                if c is None:
                    used = rec.get("delta")
                    src = "记录(无证书)"
                elif sel is None:
                    used, src = 0.0, "证书无selection(不平移)"
                else:
                    used = sel.get("delta") if sel.get("delta") is not None else rec.get("delta")
                    src = "selection.delta" if sel.get("delta") is not None else "记录delta"
                if hist is None:
                    hist = history(a.rev, f)
                last = hist[0].split("|")[0] if hist else "?"
                rows.append(
                    {
                        "file": f,
                        "rec": rpath or "/",
                        "key": rec.get("key", ""),
                        "phys": phys,
                        "op": PHYS_OP[phys],
                        "status": rec.get("status"),
                        "grade": (c or {}).get("grade"),
                        "hi": (c or {}).get("hi", rec.get("hi")),
                        "rec_hi": rec.get("hi"),
                        "rec_delta": rec.get("delta"),
                        "used": used,
                        "src": src,
                        "rule_delta": rule_delta(ck),
                        "urd": rec.get("unsure_rate_delta"),
                        "prior": pr[phys],
                        "verdict": "不平移" if src.startswith("证书无") else verdict(used, phys, pr),
                        "truth": "truth" in rec,
                        "lsid": rec.get("label_set_id"),
                        "first": hist[-1] if hist else "?",
                        "last": hist[0] if hist else "?",
                        "n_commits": len(hist),
                        "after_bug": after_bug(last) if hist else None,
                    }
                )
    print(f"# 画像 {PROFILE} δ 先验：{pr}", file=sys.stderr)
    print(f"# 含 phys 的 JSON {len(files)} 个，解析 {scanned} 个；select/measure 证书行 {len(rows)} 条", file=sys.stderr)
    for f, e in unparsed:
        print(f"# 解析失败：{f}：{e}", file=sys.stderr)
    if a.md:
        print("| 文件 | 记录位置 | 题型 | 状态 | 等级 | 证书 hi | 用的 δ（来源） | 记录 δ | unsure_rate_δ | 画像先验 | 判定 | 经 calib-import | 最后写入 | 在 db7000356 之后 |")
        print("|---|---|---|---|---|---|---|---|---|---|---|---|---|---|")
        for r in rows:
            print(
                f"| `{r['file']}` | `{r['rec']}` | {r['op']} | {r['status']} | {r['grade']} | {r['hi']} | "
                f"{r['used']}（{r['src']}） | {r['rec_delta']} | {r['urd']} | {r['prior']} | **{r['verdict']}** | "
                f"{'是' if r['truth'] else '否'}（{r['lsid']}） | {r['last']} | {r['after_bug']} |"
            )
    else:
        json.dump(rows, sys.stdout, ensure_ascii=False, indent=1)
        print()


if __name__ == "__main__":
    main()
