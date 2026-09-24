#!/usr/bin/env python3
"""T1 任务书的验收（步 31-1，B79）：对手写基线与 J++ 程序逐项跑 (a)–(d)，返回逐项结果。

由 `measure_expr.py --check <项目> --tier t1` 调用；也可单独 `python3 scripts/t1_check.py <项目>`。
手写基线在临时目录里的 `baseline/t1/` 副本上运行（(d) ② 要换一份截短的 labels.jsonl）。
J++ 程序：线只从 `labels.jsonl` 经 `jpp calib-import` 来，夹具用去掉校准字面量的 `fixture-t1.json`；
(b) 的预算由本脚本把程序里 `budget {calls: …` 的字面量换成 N 后运行（J++ 的预算写在程序头，不是命令行参数）。
"""
from __future__ import annotations

import json
import os
import re
import shutil
import subprocess
import sys
import tempfile

import certify_ref
from _baseline import ROOT

sys.path.insert(0, str(ROOT / "probes" / "_baseline"))
from accept import PROJECTIONS  # noqa: E402

PROBES = ROOT / "probes"
SDK = PROBES / "_baseline"
JPP = ROOT / "target" / "debug" / "jpp"


def _first_diff(a, b):
    import measure_expr
    return measure_expr.first_diff(a, b)


def _limits(m):
    t = m.get("t1", {})
    return int(t["calls_max"]), int(t["rounds_max"])


def _expect(base):
    t1 = base / "baseline" / "t1"
    free = json.loads((t1 / "expected-t1.json").read_text(encoding="utf-8"))
    bud = json.loads((t1 / "expected-budget.json").read_text(encoding="utf-8"))
    return free, bud


def _truncate_labels(src, dst):
    rows = [json.loads(x) for x in open(src, encoding="utf-8") if x.strip()]
    key = sorted({r["key"] for r in rows})[0]
    kept, n = [], 0
    for r in rows:
        if r["key"] == key:
            n += 1
            if n > 15:
                continue
        kept.append(r)
    with open(dst, "w", encoding="utf-8") as fh:
        for r in kept:
            fh.write(json.dumps(r, ensure_ascii=False) + "\n")
    return key


def _ref_lines(t1dir):
    rows = [json.loads(x) for x in open(t1dir / "labels.jsonl", encoding="utf-8") if x.strip()]
    return certify_ref.certify(rows)


# —— 手写基线 ——

def check_baseline(project: str, file: str, quiet=False) -> dict:
    import measure_expr
    base, m = measure_expr.load(project)
    src_t1 = base / "baseline" / "t1"
    free, bud = _expect(base)
    cmax, rmax = _limits(m)
    fx = (base / m["t1"]["fixture"]).resolve()
    out = {"file": file, "items": {}}
    tmp = pathlib_tmp()
    try:
        for f in ("materials.json", "labels.jsonl"):
            shutil.copy(src_t1 / f, tmp / f)
        prog = (base / "baseline" / file).resolve()
        env = dict(os.environ, JEV_BACKEND="fixture", JEV_FIXTURE=str(fx), PYTHONPATH=str(SDK), JEV_REPORT_CALLS="1")

        def run(*args):
            p = subprocess.run([sys.executable, str(prog), *args], cwd=tmp, env=env, capture_output=True, text=True)
            c = re.findall(r"^JEV_CALLS=(\d+)$", p.stderr, re.M)
            r = re.findall(r"^JEV_ROUNDS=(\d+)$", p.stderr, re.M)
            try:
                val = json.loads(p.stdout)
            except json.JSONDecodeError:
                val = None
            return {"code": p.returncode, "stdout": p.stdout, "val": val, "calls": int(c[-1]) if c else 0,
                    "rounds": int(r[-1]) if r else 0, "err": p.stderr[-600:]}

        led = tmp / "ledger.jsonl"
        r1 = run("--ledger-out", str(led))
        d = _first_diff(free, r1["val"]) if r1["val"] is not None else f"stdout 不是 JSON：{r1['err']}"
        out["func"] = r1["calls"]
        out["items"]["功能"] = d or "通过"
        ok_a1 = d is None and led.exists()
        r2 = run("--replay", str(led)) if led.exists() else None
        a2 = r2 is not None and r2["calls"] == 0 and r2["stdout"] == r1["stdout"] and r2["code"] == 0
        a3 = False
        if led.exists():
            lines = led.read_text(encoding="utf-8").splitlines()
            cut = tmp / "ledger-cut.jsonl"
            cut.write_text("\n".join(lines[:len(lines) // 2] + lines[len(lines) // 2 + 1:]) + "\n", encoding="utf-8")
            a3 = run("--replay", str(cut))["code"] != 0
        out["items"]["(a) 账本与重放"] = "通过" if (ok_a1 and a2 and a3) else \
            f"不通过（①{'✓' if ok_a1 else '✗'} ②{'✓' if a2 else '✗'} ③{'✓' if a3 else '✗'}）"
        rb = run("--max-calls", str(bud["N"]))
        db = _first_diff(bud["expected"], rb["val"]) if rb["val"] is not None else "stdout 不是 JSON"
        b1 = rb["calls"] == bud["N"] and db is None
        rb2 = run("--max-calls", "1000")
        b2 = rb2["val"] is not None and _first_diff(free, rb2["val"]) is None
        out["items"]["(b) 预算停机"] = "通过" if (b1 and b2) else \
            f"不通过（① calls {rb['calls']}/{bud['N']} {db or ''} ②{'✓' if b2 else '✗'}）"
        c_ok = r1["calls"] <= cmax and r1["rounds"] <= rmax
        out["calls"], out["rounds"] = r1["calls"], r1["rounds"]
        out["items"]["(c) 合批与轮数"] = ("通过" if c_ok else "不通过") + \
            f"（调用 {r1['calls']} ≤ {cmax}，轮 {r1['rounds']} ≤ {rmax}）"
        rl = run("--print-lines")
        ref = _ref_lines(src_t1)
        d1 = rl["val"] is not None and all(
            isinstance(rl["val"].get(k), dict) and (("hi" in v) == ("hi" in rl["val"][k])) and
            ("hi" not in v or (abs(v["hi"] - rl["val"][k]["hi"]) <= 1e-9 and abs(v["lo"] - rl["val"][k]["lo"]) <= 1e-9))
            for k, v in ref.items())
        key = _truncate_labels(src_t1 / "labels.jsonl", tmp / "labels.jsonl")
        rt = run()
        d2 = rt["val"] == {"status": "待真值", "key": key} and rt["calls"] == 0
        shutil.copy(src_t1 / "labels.jsonl", tmp / "labels.jsonl")
        out["items"]["(d) 认证线"] = "通过" if (d1 and d2 and d is None) else \
            f"不通过（①{'✓' if d1 else '✗'} ②{'✓' if d2 else '✗'}（{json.dumps(rt['val'], ensure_ascii=False)[:80]}） ③{'✓' if d is None else '✗'}）"
    finally:
        shutil.rmtree(tmp, ignore_errors=True)
    out["ok"] = all(v == "通过" or v.startswith("通过") for v in out["items"].values())
    if not quiet:
        for k, v in out["items"].items():
            print(f"[{project}] {file} {k}：{v}")
    return out


def pathlib_tmp():
    import pathlib
    return pathlib.Path(tempfile.mkdtemp(prefix="t1check-", dir=str(ROOT / "target")))


# —— J++ 程序 ——

def _project(m, impl, value):
    if value is None:  # 程序停在 Pending（例如预算在 judge 上用完）：没有返回值
        return None
    pj = impl.get("projection")
    if not pj:
        return value
    return PROJECTIONS[pj](value)


def check_jpp(project: str, impl: dict, quiet=False) -> dict:
    """impl：measure.toml 的一项 J++ 实现（source、run_dir、projection 可选）。"""
    import measure_expr
    base, m = measure_expr.load(project)
    src_t1 = base / "baseline" / "t1"
    free, bud = _expect(base)
    cmax, rmax = _limits(m)
    fx = (base / m["t1"]["fixture"]).resolve()
    src = (base / impl["source"]).resolve()
    cwd = (base / impl.get("run_dir", os.path.relpath(src.parent, base))).resolve()
    tmp = pathlib_tmp()
    out = {"file": os.path.relpath(src, ROOT), "items": {}}
    try:
        cal = tmp / "calib"
        p = subprocess.run([str(JPP), "calib-import", str(src_t1 / "labels.jsonl"), "--calib-out", str(cal)],
                           capture_output=True, text=True)
        imp = json.loads(p.stdout) if p.returncode == 0 else []

        def run(source, *args):
            o = tmp / f"r{len(list(tmp.glob('r*.json')))}.json"
            q = subprocess.run([str(JPP), "run", os.path.relpath(source, cwd), *args, "--output", str(o)],
                               cwd=cwd, capture_output=True, text=True)
            d = json.loads(o.read_text(encoding="utf-8")) if o.exists() else None
            return {"code": q.returncode, "rep": d, "err": q.stderr[-600:]}

        led = tmp / "ledger.jsonl"
        r1 = run(src, "--fixtures", str(fx), "--calib", str(cal), "--ledger-out", str(led))
        v1 = _project(m, impl, r1["rep"]["value"]) if r1["rep"] else None
        d = _first_diff(free, v1) if v1 is not None else f"运行失败：{r1['err']}"
        out["items"]["功能"] = d or "通过"
        calls = r1["rep"]["cost"]["calls"] if r1["rep"] else None
        layers = set()
        if led.exists():
            for x in led.read_text(encoding="utf-8").splitlines()[1:]:
                j = json.loads(x).get("entry", {}).get("Judge")
                if j:
                    layers.add(j.get("layer"))
        out["calls"], out["rounds"] = calls, len(layers)
        r2 = run(src, "--replay", str(led)) if led.exists() else None
        a2 = bool(r2 and r2["rep"] and r2["rep"]["cost"]["calls"] == 0 and r2["rep"]["value"] == r1["rep"]["value"])
        a3 = False
        if led.exists():
            lines = led.read_text(encoding="utf-8").splitlines()
            idx = [i for i, x in enumerate(lines) if '"Judge"' in x]
            if idx:
                cut = tmp / "ledger-cut.jsonl"
                k = idx[len(idx) // 2]
                cut.write_text("\n".join(lines[:k] + lines[k + 1:]) + "\n", encoding="utf-8")
                a3 = run(src, "--replay", str(cut))["code"] != 0
        out["items"]["(a) 账本与重放"] = "通过" if (d is None and a2 and a3) else \
            f"不通过（①{'✓' if d is None else '✗'} ②{'✓' if a2 else '✗'} ③{'✓' if a3 else '✗'}）"
        text = src.read_text(encoding="utf-8")

        def with_budget(n):
            t = re.sub(r"(budget\s*\{\s*calls:\s*)\d+", lambda mm: mm.group(1) + str(n), text, count=1)
            f = src.parent / f".{src.stem}.budget{n}.jpp"
            f.write_text(t, encoding="utf-8")
            return f

        fb = with_budget(bud["N"])
        rb = run(fb, "--fixtures", str(fx), "--calib", str(cal))
        fb.unlink()
        vb = _project(m, impl, rb["rep"]["value"]) if rb["rep"] else None
        db = _first_diff(bud["expected"], vb) if vb is not None else \
            f"没有返回值（status {rb['rep'] and rb['rep'].get('status')}）"
        b1 = rb["rep"] is not None and rb["rep"]["cost"]["calls"] == bud["N"] and db is None
        fb2 = with_budget(1000)
        rb2 = run(fb2, "--fixtures", str(fx), "--calib", str(cal))
        fb2.unlink()
        b2 = rb2["rep"] is not None and _first_diff(free, _project(m, impl, rb2["rep"]["value"])) is None
        out["items"]["(b) 预算停机"] = "通过" if (b1 and b2) else \
            f"不通过（① calls {rb['rep']['cost']['calls'] if rb['rep'] else '—'}/{bud['N']} {db or ''} ②{'✓' if b2 else '✗'}）"
        c_ok = calls is not None and calls <= cmax and len(layers) <= rmax
        out["items"]["(c) 合批与轮数"] = ("通过" if c_ok else "不通过") + f"（调用 {calls} ≤ {cmax}，层 {len(layers)} ≤ {rmax}）"
        ref = _ref_lines(src_t1)
        got = {r.get("key"): r for r in imp} if isinstance(imp, list) else {}
        d1 = all(k in got and (got[k].get("status") == "上岗") == ("hi" in v) and
                 ("hi" not in v or (abs(got[k]["hi"] - v["hi"]) <= 1e-9 and abs(got[k]["lo"] - v["lo"]) <= 1e-9))
                 for k, v in ref.items())
        cal2 = tmp / "calib2"
        tl = tmp / "labels-cut.jsonl"
        key = _truncate_labels(src_t1 / "labels.jsonl", tl)
        p2 = subprocess.run([str(JPP), "calib-import", str(tl), "--calib-out", str(cal2)], capture_output=True, text=True)
        rec = cal2 / f"{key}.json"
        d2 = rec.exists() and json.loads(rec.read_text(encoding="utf-8")).get("status") != "上岗"
        fxt = json.loads(fx.read_text(encoding="utf-8"))
        no_lit = not fxt.get("calibrations")
        out["items"]["(d) 认证线"] = "通过" if (d1 and d2 and d is None and no_lit) else \
            f"不通过（calib-import 覆盖全部键 {'✓' if d1 else '✗'}；截短后不上岗 {'✓' if d2 else '✗'}；夹具无校准字面量 {'✓' if no_lit else '✗'}）"
    finally:
        shutil.rmtree(tmp, ignore_errors=True)
    out["ok"] = all(v.startswith("通过") for v in out["items"].values())
    if not quiet:
        for k, v in out["items"].items():
            print(f"[{project}] {out['file']} {k}：{v}")
    return out


if __name__ == "__main__":
    import measure_expr
    for p in sys.argv[1:] or measure_expr.projects():
        measure_expr.check_tier(p, "t1")
