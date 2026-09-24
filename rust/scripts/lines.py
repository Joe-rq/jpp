#!/usr/bin/env python3
"""单文件行数上限 1,500（20 §11；21 §九·1：步 4a/4b/4c 后对已拆文件改为失败模式）。函数 ≤150 行由 clippy.toml 管。"""
from _baseline import ROOT, finish, rust_files

LIMIT = 1500
over = []
for f in rust_files():
    n = sum(1 for _ in f.open(encoding="utf-8"))
    if n > LIMIT:
        over.append(f"{f.relative_to(ROOT)}：{n} 行")
finish("lines", len(over), over)
