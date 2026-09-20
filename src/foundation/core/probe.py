"""验题闸门：上岗的必要条件（施工单 §3.3）。

闸门只管"能不能上岗"，不产出两条线（红队 A1）。缝不足 → 停留 draft，不得手调任何数。
"""

from __future__ import annotations

import os
from concurrent.futures import ThreadPoolExecutor
from dataclasses import dataclass, field
from typing import Any

import yaml

from foundation.core import registry
from foundation.core.canon import H
from foundation.core.ledger import ledger_key
from foundation.core.outlet import read_code, score_level
from foundation.core.params import Params
from foundation.core.unit import Unit


@dataclass
class ProbeCase:
    view: Any
    expect: Any
    source: str = "builder"


@dataclass
class ProbeResult:
    unit: str
    n_probes: int = 0
    enough: bool = False
    gap: float | None = None
    passed: bool = False
    note: str = ""
    rows: list[dict] = field(default_factory=list)

    def to_dict(self) -> dict:
        return {"unit": self.unit, "n_probes": self.n_probes, "enough": self.enough,
                "gap": self.gap, "passed": self.passed, "note": self.note, "rows": self.rows}


def load_probes(unit: Unit, testbed_dir: str) -> list[ProbeCase]:
    path = unit.probes_path or os.path.join("probes", f"{unit.name}.yaml")
    if not os.path.isabs(path):
        path = os.path.join(testbed_dir, path)
    if not os.path.exists(path):
        return []
    with open(path, encoding="utf-8") as fh:
        raw = yaml.safe_load(fh) or []
    if isinstance(raw, dict):                 # 宽容：{probes: [...]} 也收
        raw = raw.get("probes") or raw.get("cases") or []
    out = []
    for r in raw:
        if not isinstance(r, dict):
            continue
        out.append(ProbeCase(view=r.get("view"), expect=r.get("expect"),
                             source=r.get("source", "builder")))
    return out


def _expect_level(expect: Any, levels: list[str]) -> int | None:
    if isinstance(expect, bool):
        return None
    if isinstance(expect, int):
        return expect
    if isinstance(expect, str):
        if expect.isdigit():
            return int(expect)
        for i, desc in enumerate(levels):     # 宽容：允许写档位描述原文
            if str(desc).strip() == expect.strip():
                return i
    return None


def _counts_ok(unit: Unit, cases: list[ProbeCase]) -> tuple[bool, str]:
    prim = unit.primitive
    if prim == "noul" or unit.eye_type == "code":
        n_act = sum(1 for c in cases if c.expect == "act")
        n_ign = sum(1 for c in cases if c.expect == "ignore")
        if n_act < 3 or n_ign < 3:
            return False, f"验题不足：act {n_act}/3, ignore {n_ign}/3"
        return True, ""
    if prim == "choice":
        for opt in unit.jev_eye.options:
            n = sum(1 for c in cases if c.expect == opt)
            if n < 2:
                return False, f"验题不足：选项「{opt}」只有 {n}/2 道"
        return True, ""
    if prim == "score":
        levels = unit.jev_eye.levels
        for i in range(len(levels)):
            n = sum(1 for c in cases if _expect_level(c.expect, levels) == i)
            if n < 2:
                return False, f"验题不足：档位 {i} 只有 {n}/2 道"
        return True, ""
    return True, ""


def run_probes(units: list[Unit], testbed_dir: str, client, params: Params,
               ledger=None, log=None) -> dict[str, ProbeResult]:
    """跑全部单元的验题。jev 眼按 (unit, 验题) 逐条问，先查账本。"""
    results: dict[str, ProbeResult] = {}
    jobs: list[tuple[Unit, int, ProbeCase]] = []
    for unit in sorted(units, key=lambda u: u.name):
        cases = load_probes(unit, testbed_dir)
        res = ProbeResult(unit=unit.name, n_probes=len(cases))
        results[unit.name] = res
        if not cases:
            res.note = "没有验题文件"
            continue
        ok, why = _counts_ok(unit, cases)
        res.enough = ok
        if not ok:
            res.note = why
        if unit.eye_type == "code":
            fn = registry.lookup(unit.code_eye.fn_name)
            for i, c in enumerate(cases):
                r = read_code(fn(c.view, None, None))
                res.rows.append({"i": i, "expect": c.expect, "outlet": r.outlet,
                                 "p": r.p, "match": r.outlet == c.expect})
        else:
            for i, c in enumerate(cases):
                jobs.append((unit, i, c))

    # jev 眼：并发调用（每条验题一次，含账本）
    def work(job):
        unit, i, c = job
        qid = unit.name
        qfp = unit.jev_eye.fp()
        view_fp = H(unit.view, c.view)
        key = ledger_key(view_fp, qfp, params.model_version,
                         include_batch=params.ledger_key_includes_batch)
        cached = ledger.get(key) if ledger is not None else None
        if cached is not None:
            return unit, i, c, cached, True, None
        r = client.ask(view_fp, c.view, {qid: unit.jev_eye.question()}, {qid: qfp})
        return unit, i, c, r.answers[qid], False, r

    if jobs:
        with ThreadPoolExecutor(max_workers=max(1, params.max_concurrency)) as ex:
            done = list(ex.map(work, jobs))
        for unit, i, c, answer, hit, r in done:      # 顺序按 jobs，与返回先后无关
            if ledger is not None and not hit:
                ledger.put(ledger_key(H(unit.view, c.view), unit.jev_eye.fp(),
                                      params.model_version,
                                      include_batch=params.ledger_key_includes_batch), answer)
            if log is not None and not hit:
                log.emit("ask", view_fp=H(unit.view, c.view),
                         question_fps={unit.name: unit.jev_eye.fp()}, cache_hit=False,
                         request=r.request, response=r.response, cost=r.cost_usd,
                         input_tokens=r.input_tokens, phase="probe")
            res = results[unit.name]
            prim = unit.primitive
            row: dict[str, Any] = {"i": i, "expect": c.expect}
            if prim == "noul":
                row["p"] = float(answer["noul"])
            elif prim == "choice":
                probs = {k: float(v) for k, v in answer["probabilities"].items()}
                row["choice"] = answer.get("choice")
                row["p"] = probs.get(row["choice"], 0.0)
                row["probabilities"] = probs
                row["match"] = row["choice"] == c.expect
            elif prim == "score":
                levels = unit.jev_eye.levels
                exp = _expect_level(c.expect, levels)
                row["score"] = float(answer["score"])
                row["level"] = score_level(answer["score"], max(1, len(levels)))
                row["expect_level"] = exp
                row["match"] = exp is not None and abs(row["score"] - exp) <= params.score_tolerance
            res.rows.append(row)

    for unit in units:
        _judge(unit, results[unit.name], params)
    return results


def _judge(unit: Unit, res: ProbeResult, params: Params) -> None:
    rows = sorted(res.rows, key=lambda r: r["i"])
    res.rows = rows
    if not rows:
        res.passed = False
        res.note = res.note or "没有验题"
        return
    if not res.enough:
        res.passed = False
        return
    prim = unit.primitive
    if unit.eye_type == "code":
        res.passed = all(r["match"] for r in rows)
        res.gap = 1.0 if res.passed else 0.0
        if not res.passed:
            res.note = "code 眼验题不匹配"
        return
    if prim == "noul":
        acts = [r["p"] for r in rows if r["expect"] == "act"]
        igns = [r["p"] for r in rows if r["expect"] == "ignore"]
        if not acts or not igns:
            res.passed, res.note = False, "验题不足"
            return
        res.gap = min(acts) - max(igns)
        thr = unit.gap_threshold(params)
        res.passed = res.gap >= thr
        if not res.passed:
            res.note = f"缝不足：gap={res.gap:.3f} < {thr:.2f}"
        return
    if prim == "choice":
        res.passed = all(r.get("match") for r in rows)
        gaps = []
        for r in rows:
            probs = dict(r.get("probabilities") or {})
            pe = probs.pop(r["expect"], 0.0)
            second = max(probs.values()) if probs else 0.0
            gaps.append(pe - second)
        res.gap = min(gaps) if gaps else None
        if not res.passed:
            res.note = "有验题 argmax 与 expect 不符"
        return
    if prim == "score":
        res.passed = all(r.get("match") for r in rows)
        devs = [abs(r["score"] - r["expect_level"]) for r in rows
                if r.get("expect_level") is not None]
        res.gap = (params.score_tolerance - max(devs)) if devs else None
        if not res.passed:
            res.note = f"有验题偏离 > {params.score_tolerance}"
