#!/usr/bin/env python3
"""cargo fmt --check 与 cargo clippy -D warnings 的计数（21 §九·1 前两项）。

报告模式下只计「需重排的文件数」与「clippy 告警数」并记基线；失败模式下任一增加即失败。
"""
import re, subprocess, sys
from _baseline import ROOT, finish

which = sys.argv[1]
if which == "fmt":
    r = subprocess.run(["cargo", "fmt", "--all", "--", "--check", "-l"], cwd=ROOT, capture_output=True, text=True)
    files = sorted({l.replace(str(ROOT) + "/", "") for l in r.stdout.splitlines() if l.strip()})
    finish("fmt", len(files), files)
else:
    r = subprocess.run(["cargo", "clippy", "--workspace", "--all-targets", "--offline", "--message-format=short"],
                       cwd=ROOT, capture_output=True, text=True)
    warns = sorted({l.replace(str(ROOT) + "/", "") for l in r.stderr.splitlines()
                    if re.search(r": (warning|error): ", l)})
    finish("clippy", len(warns), warns)
