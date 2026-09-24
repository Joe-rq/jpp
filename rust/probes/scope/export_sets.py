"""B68 修订实现对账：把 rule_gradient.py 的材料集与规则 B（m=0.10, k=2）下的预期判出数导出为 sets.json，
供 Rust 侧 crates/jpp-calib/tests/scope_probe_reconcile.rs 用实现代码复算并逐项对账。"""
import contextlib
import io
import json
import pathlib

HERE = pathlib.Path(__file__).parent
g = {"__file__": str(HERE / "rule_gradient.py"), "__name__": "rule_gradient"}
with contextlib.redirect_stdout(io.StringIO()):
    exec(compile((HERE / "rule_gradient.py").read_text(encoding="utf-8"), "rule_gradient.py", "exec"), g)
R = g["widen"](g["base"], m=0.10, k=2)
out = {"ranges_expected": R, "sets": []}
for name, texts in g["sets"].items():
    n_out = sum(g["outside"](R, g["fp"](t)) is not None for t in texts)
    out["sets"].append({"name": name, "texts": texts, "expected_outside": n_out})
json.dump(out, open(HERE / "sets.json", "w"), ensure_ascii=False, indent=0)
for s in out["sets"]:
    print(f'{s["name"]}: {s["expected_outside"]}/{len(s["texts"])}')
