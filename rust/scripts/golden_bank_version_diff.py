#!/usr/bin/env python3
"""核对重录的金样：与某个修订相比，差异是否只在账本头的 bank_version 与随之改变的哈希链（只读）。

用法（仓库内任一目录）：
    python3 地基/rust-jpp/scripts/golden_bank_version_diff.py [--rev HEAD] [--old 1] [--new 4]

对 `tests/golden/` 下工作区与 `--rev` 不同的每个文件：
- 只允许是 `ledger.json`（逐行 JSON 账本）；其他文件有差异即报。
- 逐行解析，行数必须相同；每行比较全部叶子路径：
  - 头行 `header.compared.bank_version` 必须由 `--old` 变为 `--new`；
  - 每个条目行的 `prev`（哈希链）允许变；
  - 其余任何叶子不同即报。
`--z0308`（Z0308 题库 F3、F4 重认）另允许：头行 `calib_hash`、`CalibUsed.hash`，以及 `CalibUsed.record` 的
`delta` null→数、`unsure_rate`，和每条样本的 `perms` 0→2、`mode_share` null→1.0（两序置换测量随样本进记录）。
各项计数随摘要打印。

退出码：0 = 只含预期差异；1 = 有别的差异。
"""
import argparse
import json
import subprocess
import sys


def git(*a):
    return subprocess.run(["git", *a], capture_output=True, text=True, check=True).stdout


def leaves(x, path=()):
    if isinstance(x, dict):
        for k, v in x.items():
            yield from leaves(v, path + (k,))
    elif isinstance(x, list):
        for i, v in enumerate(x):
            yield from leaves(v, path + (i,))
    else:
        yield path, x


def z0308_ok(p, i, old, new):
    """Z0308 预注册写明的字段变化（`工程-Z0308-select未决率.md` 3.3、3.4）。"""
    if i == 0:
        return p == ("header", "compared", "calib_hash")
    if p[:2] != ("entry", "CalibUsed"):
        return False
    q = p[2:]
    if q == ("hash",) or q == ("record", "unsure_rate"):
        return True
    if q == ("record", "delta"):
        return old is None and isinstance(new, float)
    if len(q) == 4 and q[:2] == ("record", "samples") and isinstance(q[2], int):
        return (q[3], old, new) in (("perms", 0, 2), ("mode_share", None, 1.0))
    return False


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--rev", default="HEAD")
    ap.add_argument("--old", default="1")
    ap.add_argument("--new", default="4")
    ap.add_argument("--z0308", action="store_true", help="另允许 F3、F4 重认带来的记录字段变化")
    a = ap.parse_args()
    root = git("rev-parse", "--show-toplevel").strip()
    import os

    os.chdir(root)
    files = [f for f in git("diff", "--name-only", "-z", a.rev, "--", "地基/rust-jpp/tests/golden/").split("\0") if f]
    bad = []
    summary = []
    for f in files:
        if not f.endswith("/ledger.json"):
            bad.append(f"{f}：不是账本文件却有差异")
            continue
        old = [json.loads(l) for l in git("show", f"{a.rev}:{f}").splitlines() if l.strip()]
        new = [json.loads(l) for l in open(f, encoding="utf-8").read().splitlines() if l.strip()]
        if len(old) != len(new):
            bad.append(f"{f}：行数 {len(old)} → {len(new)}")
            continue
        n_prev = 0
        bv = None
        z: dict = {}
        for i, (o, n) in enumerate(zip(old, new)):
            lo, ln = dict(leaves(o)), dict(leaves(n))
            for p in sorted(set(lo) | set(ln), key=str):
                if lo.get(p) == ln.get(p):
                    continue
                if p == ("header", "compared", "bank_version") and i == 0:
                    if (lo.get(p), ln.get(p)) == (a.old, a.new):
                        bv = f"{lo.get(p)}→{ln.get(p)}"
                        continue
                if p == ("prev",) and i > 0:
                    n_prev += 1
                    continue
                if a.z0308 and z0308_ok(p, i, lo.get(p), ln.get(p)):
                    k = ".".join(str(x) for x in p if not isinstance(x, int))
                    z[k] = z.get(k, 0) + 1
                    continue
                bad.append(f"{f} 第 {i + 1} 行 {'.'.join(map(str, p))}：{lo.get(p)!r} → {ln.get(p)!r}")
        summary.append(
            f"{f}：{len(new)} 行；bank_version {bv}；prev 变 {n_prev} 处"
            + (f"；Z0308 字段 {z}" if z else "")
        )
    for s in summary:
        print(s)
    for b in bad:
        print("意外差异：" + b)
    print(f"共 {len(files)} 个文件有差异；意外差异 {len(bad)} 处")
    sys.exit(1 if bad else 0)


if __name__ == "__main__":
    main()
