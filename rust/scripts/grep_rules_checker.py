#!/usr/bin/env python3
"""「规则只收 &Cx」的 CI 强制点（B71，地基/附注/2026-09-24-施工中裁定.md；21 步 12b-5）。

目标：`jpp-check/src/rules/` 目录的代码不引用分析器类型 `Checker`（注释不算）。规则只能经共享借用的
`Cx` 读 IR、`check` 的输入与分析结果视图，改不到分析状态、看不见彼此的诊断。基线为 0，只许保持。
"""
import re
from _baseline import ROOT, finish

hits = []
for f in sorted((ROOT / "crates" / "jpp-check" / "src" / "rules").rglob("*.rs")):
    for i, line in enumerate(f.read_text(encoding="utf-8").splitlines(), 1):
        s = line.split("//")[0]
        if re.search(r"\bChecker\b", s):
            hits.append(f"{f.relative_to(ROOT)}:{i}")
finish("grep_rules_checker", len(hits), hits)
