#!/usr/bin/env python3
"""「只有 flush 能填答案」的 CI 强制点（20 §2.3 jpp-value、§11.1；21 §九·1：步 8b 起失败模式）。

目标：`Readings::fill` 的调用点只许在 jpp-runtime/src/flush.rs。现行代码里读数的填写是
`Reading::fill`（interp.rs）；本脚本计 `.fill(` 在 flush.rs 以外的调用点。
"""
import re
from _baseline import ROOT, finish, rust_files

hits = []
for f in rust_files():
    if f.name == "flush.rs":
        continue
    for i, line in enumerate(f.read_text(encoding="utf-8").splitlines(), 1):
        s = line.split("//")[0]
        if re.search(r"\.fill\(", s) and "fn fill" not in s:
            hits.append(f"{f.relative_to(ROOT)}:{i}")
finish("grep_fill", len(hits), hits)
