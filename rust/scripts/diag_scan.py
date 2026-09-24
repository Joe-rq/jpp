#!/usr/bin/env python3
"""对全部示例跑 `jpp check`，列出 B13 诊断层（W-diag-*）提示。步 5 预注册差异清单用。"""
import pathlib
import re
import subprocess
import sys

root = pathlib.Path(__file__).resolve().parent.parent
jpp = root / "target" / "debug" / "jpp"
files = sorted((root / "examples").glob("*.jpp")) + sorted((root / "examples" / "errors").glob("*.jpp"))
total = 0
for f in files:
    out = subprocess.run([str(jpp), "check", str(f)], capture_output=True, text=True, cwd=root)
    lines = [l for l in (out.stderr + out.stdout).splitlines() if "W-diag-" in l]
    error_level = [l for l in (out.stderr + out.stdout).splitlines() if re.search(r"E-diag-", l)]
    if lines or error_level:
        print(f"{f.relative_to(root)}:")
        for l in lines + error_level:
            print("   ", l.split(": ", 1)[1][:160] if ": " in l else l[:160])
        total += len(lines) + len(error_level)
print(f"合计 {total} 条")
sys.exit(0)
