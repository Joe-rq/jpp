#!/usr/bin/env python3
"""关闭开关检验（20 §2.4、§11.5；21 §九·1：步 24 起失败模式）。

对源码里每个 `cfg(jpp_disable = "<编号>")` 逐个关闭（RUSTFLAGS=--cfg jpp_disable="<编号>"），
断言对应绕过测试变红；有强制点无开关、或关了不红，都计违规。
违规基数：20 §2.4 不变量登记表的行数减去已建开关且关闭后确实变红的数目。
"""
import re, os, subprocess
from _baseline import ROOT, finish, rust_files

TABLE = ROOT.parent / "20-架构方案-v1.md"
text = TABLE.read_text(encoding="utf-8") if TABLE.exists() else ""
section = text.split("### 2.4", 1)[1].split("### 2.5", 1)[0] if "### 2.4" in text else ""
registered = [l for l in section.splitlines() if re.match(r"\|\s*[A-Z]", l) and not l.startswith("| 不变量")]

switches = sorted({m for f in rust_files() for m in re.findall(r'jpp_disable\s*=\s*"([^"]+)"', f.read_text(encoding="utf-8"))})
verified, detail = 0, []
for s in switches:
    env = dict(os.environ, RUSTFLAGS=f'--cfg jpp_disable="{s}"')
    r = subprocess.run(["cargo", "test", "--workspace", "--offline", "-q"], cwd=ROOT, env=env, capture_output=True)
    if r.returncode != 0:
        verified += 1
    else:
        detail.append(f"开关 {s} 关闭后测试仍全绿")
missing = max(0, len(registered) - verified)
detail.insert(0, f"登记表 {len(registered)} 行；已建开关 {len(switches)}；关闭后变红 {verified}")
finish("kill_switch", missing, detail)
