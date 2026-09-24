#!/usr/bin/env python3
"""依赖表核对（20 §2.2 第 1 条；21 §九·1：步 6 起对已存在的 crate 改为失败模式）。

读 cargo metadata，逐个工作区 crate 比对允许的依赖集合；多一条即违规。
目标架构的 crate 尚未建出时，现存 crate（jpp-core 等）不在表里，只报告其依赖，不计违规。
"""
import json, subprocess
from _baseline import ROOT, finish

# 20 §2.2 第 1 条（终审后：jpp-stat 并入 jpp-value，jpp-lower 并入 jpp-syntax）。
ALLOWED = {
    "jpp-ir": set(),
    "jpp-syntax": {"jpp-ir"},
    "jpp-value": {"jpp-ir"},
    "jpp-effects": {"jpp-ir", "jpp-value"},
    "jpp-ledger": {"jpp-ir", "jpp-value", "jpp-effects"},
    "jpp-calib": {"jpp-ir", "jpp-value", "jpp-effects"},
    "jpp-check": {"jpp-ir", "jpp-effects"},
    "jpp-plan": {"jpp-ir", "jpp-effects"},
    "jpp-runtime": {"jpp-ir", "jpp-value", "jpp-effects", "jpp-ledger"},
    "jpp-store": {"jpp-ir", "jpp-value", "jpp-effects", "jpp-ledger", "jpp-calib"},
    "jpp-backend-jev": {"jpp-ir", "jpp-value", "jpp-effects"},
    "jpp-lib": {"jpp-ir", "jpp-value", "jpp-effects", "jpp-check"},
    "jpp-cli": {"jpp"},
}

meta = json.loads(subprocess.run(["cargo", "metadata", "--format-version", "1", "--no-deps", "--offline"],
                                 cwd=ROOT, capture_output=True, text=True, check=True).stdout)
members = {p["name"]: p for p in meta["packages"]}
violations, notes = [], []
for name, p in sorted(members.items()):
    deps = {d["name"] for d in p["dependencies"] if d["name"].startswith("jpp") and d.get("kind") in (None, "normal")}
    if name == "jpp":
        continue  # 外观依赖全部
    if name not in ALLOWED:
        notes.append(f"{name}（现存 crate，不在目标表）→ {sorted(deps)}")
        continue
    for d in sorted(deps - ALLOWED[name]):
        violations.append(f"{name} → {d} 不在 20 §2.2 依赖表")
for n in notes:
    print(f"  · {n}")
finish("deps", len(violations), violations)
