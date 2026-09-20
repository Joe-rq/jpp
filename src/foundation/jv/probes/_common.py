"""E-PROBE-10 公共件：样本、指标、构建器跑法、裸调臂、无 Jev 基线、注册表与 CLI。

探针只检验任务无关性（宪法附则一）：这里没有任何为某条探针加的语言特判；构建器写不出的地方记在 GAPS.md。

每条探针模块提供一个 `Probe`：
  make_samples(n, seed) -> [Sample]        真值由人工注入（正负平衡）
  builder(samples, rt)  -> [Result]        构建器程序（@jv.program）
  bare(samples, client, tally) -> [Result] 裸调手写版：同题面、同材料、直接 client.ask
  baseline(samples)     -> [Result]|None   无 Jev 基线（启发式 / 全跑）
  kind: noul | choice | score              决定指标（AUC / 准确率）
"""

from __future__ import annotations

import argparse
import json
import os
import shutil
import subprocess
import sys
import tempfile
import time
from dataclasses import dataclass, field
from typing import Any, Callable

sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))))

import foundation.jv as jv  # noqa: E402

RUNS_ROOT = os.path.join(os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__)))), "runs", "jv", "e-probe-10")


@dataclass
class Sample:
    id: str
    truth: Any
    mats: dict[str, Any] = field(default_factory=dict)      # 原值（str / dict），进程序前用 jv.lit 包
    meta: dict[str, Any] = field(default_factory=dict)


@dataclass
class Result:
    id: str
    truth: Any
    pred: Any            # noul: True/False/None；choice: 下标；score: 档位下标
    p: float | None      # noul: P(是)；choice: 众数概率；score: 该档概率
    status: str = ""     # calibrated / provisional / unsure:<cause> / fail


@dataclass
class Probe:
    name: str
    title: str
    kind: str
    make_samples: Callable[..., list[Sample]]
    builder: Callable[[list[Sample], Any], list[Result]]
    bare: Callable[[list[Sample], Any, dict], list[Result]]
    baseline: Callable[[list[Sample]], list[Result] | None] | None = None
    note: str = ""


PROBES: dict[str, Probe] = {}


def register(p: Probe) -> Probe:
    PROBES[p.name] = p
    return p


# ---------------------------------------------------------------- 出口收口（cold 键：handler 库 cold 路径给临时出口）
def settle(e) -> tuple[Any, str]:
    """把一个出口收成 (最终出口或 None, 状态)。cold → 临时出口（provisional，取自 detail）；仍拿不准 → 记账丢弃。
    第一版用 `jv.handle(c)` 取临时出口，6/24 返回 None 且丢了 p（run1 的 unsure:cold 行 p=0.0）——见 GAPS.md #8。"""
    match e:
        case jv.Unsure(c):
            prov = (e.detail or {}).get("provisional") if c == "cold" else None
            jv.handle(e, then=jv.drop)                              # 记账消费（J-05）
            if prov is None:
                return None, f"unsure:{c}"
            if isinstance(prov, jv.Unsure):
                return prov, f"unsure:{prov.cause}"
            return prov, "provisional"
        case _:
            return e, "calibrated"


def exit_to_result(sid: str, truth: Any, e, kind: str) -> Result:
    fin, status = settle(e)
    p = fin.p if fin is not None and fin.p is not None else (e.p if e.p is not None else None)
    if fin is None or isinstance(fin, jv.Unsure):
        return Result(sid, truth, None, p, status)
    if kind == "noul":
        pred = {"act": True, "ignore": False}.get(fin.kind)
    elif kind == "choice":
        pred = getattr(fin, "k", None)
    else:
        pred = getattr(fin, "level", None)
    return Result(sid, truth, pred, p, status)


# ---------------------------------------------------------------- 裸调臂
def bare_state(on, ctx=(), ref=(), over=()):
    """与运行时同一渲染（jv.State.slots），保证裸调臂看到的材料逐字节相同。"""
    s = jv.state(on=jv.lit(on) if not isinstance(on, tuple) else (jv.lit(on[0]), jv.lit(on[1])),
                 ctx=[jv.lit(x) for x in ctx], ref=[jv.lit(x) for x in ref], over=[jv.lit(x) for x in over])
    return s.resolved().slots()


def bare_ask(client, state: dict, questions: dict[str, dict], tally: dict) -> dict:
    answers, tokens, cost = client.ask(state, questions)
    tally["calls"] = tally.get("calls", 0) + 1
    tally["questions"] = tally.get("questions", 0) + len(questions)
    tally["tokens"] = tally.get("tokens", 0) + int(tokens)
    tally["cost"] = tally.get("cost", 0.0) + float(cost)
    return answers


def q_noul(text: str) -> dict:
    return {"type": "noul", "instructions": text}


def q_choice(text: str, over: list[str]) -> dict:
    return {"type": "choice", "instructions": text, "criteria": {f"c{i}": o for i, o in enumerate(over)}}


def q_score(text: str, scale: tuple) -> dict:
    return {"type": "score", "instructions": text, "criteria": list(scale)}


def parse_noul(a: dict) -> tuple[bool, float]:
    p = float(a["noul"])
    return p >= 0.5, p


def parse_choice(a: dict) -> tuple[int, float]:
    pr = {str(k): float(v) for k, v in a["probabilities"].items()}
    best = max(pr, key=pr.get)
    return int(best[1:]), pr[best]


def parse_score(a: dict) -> tuple[int, float]:
    pr = {str(k): float(v) for k, v in a["probabilities"].items()}
    best = max(pr, key=pr.get)
    return int(best), pr[best]


# ---------------------------------------------------------------- 假客户端规则（确定性、置换不变；只为 $0 跑通，读数与真值无关）
def fake_rule(text: str, qid: str, q: dict) -> dict | None:
    import hashlib

    def h(x: str) -> float:
        return int(hashlib.sha1(x.encode("utf-8")).hexdigest()[:8], 16) / 0xFFFFFFFF

    t = q["type"]
    if t == "noul":
        return {"type": "noul", "noul": round(0.1 + 0.8 * h(text + q["instructions"]), 3)}
    if t == "choice":
        crit = q["criteria"]
        best = min(crit, key=lambda k: h(str(crit[k]) + text))          # 按候选原文定，置换不变
        probs = {k: (0.9 if k == best else round(0.1 / max(1, len(crit) - 1), 4)) for k in crit}
        return {"type": "choice", "choice": best, "probabilities": probs}
    n = len(q["criteria"])
    lvl = int(h(text + q["instructions"]) * n) % n
    probs = {str(i): (0.9 if i == lvl else round(0.1 / max(1, n - 1), 4)) for i in range(n)}
    return {"type": "score", "score": float(lvl), "probabilities": probs}


# ---------------------------------------------------------------- 指标
def auc(scores: list[float], labels: list[bool]) -> float | None:
    pos = [s for s, l in zip(scores, labels) if l]
    neg = [s for s, l in zip(scores, labels) if not l]
    if not pos or not neg:
        return None
    wins = sum(1.0 if a > b else 0.5 if a == b else 0.0 for a in pos for b in neg)
    return round(wins / (len(pos) * len(neg)), 3)


def metrics(kind: str, results: list[Result]) -> dict:
    rs = [r for r in results if r.pred is not None]
    n = len(results)
    out: dict[str, Any] = {"n": n, "answered": len(rs), "unsure": n - len(rs)}
    if kind == "noul":
        sc = [(r.p if r.p is not None else 0.5) for r in results]
        out["auc"] = auc(sc, [bool(r.truth) for r in results])
        out["acc"] = round(sum(1 for r in rs if bool(r.pred) == bool(r.truth)) / len(rs), 3) if rs else None
    else:
        out["acc"] = round(sum(1 for r in rs if r.pred == r.truth) / len(rs), 3) if rs else None
        if kind == "score" and rs:
            out["mae"] = round(sum(abs(int(r.pred) - int(r.truth)) for r in rs) / len(rs), 3)
    return out


def recall_saved(results: list[Result], target: float = 0.95) -> dict | None:
    """E9f 式：把「阳性」当需要做昂贵步骤的样本，按 p 降序送出，固定召回 ≥ target 时省掉多少步骤。"""
    if not results or any(r.p is None for r in results):
        return None
    labs = [bool(r.truth) for r in results]
    npos = sum(labs)
    if npos == 0:
        return None
    order = sorted(results, key=lambda r: -(r.p or 0))
    got, sent = 0, 0
    for r in order:
        sent += 1
        got += 1 if r.truth else 0
        if got / npos >= target:
            break
    return {"recall": round(got / npos, 3), "sent": sent, "saved": len(results) - sent,
            "saved_ratio": round((len(results) - sent) / len(results), 3)}


# ---------------------------------------------------------------- 沙箱 S（真实执行，本机）
def sandbox_dir(prefix: str = "jvprobe-") -> str:
    return tempfile.mkdtemp(prefix=prefix)


def run_py(files: dict[str, str], argv: list[str], timeout: float = 30.0, env: dict | None = None) -> dict:
    """在临时目录写文件、跑 python，返回 {exit, stdout, stderr, seconds}。真实子进程。"""
    d = sandbox_dir()
    try:
        for name, src in files.items():
            path = os.path.join(d, name)
            os.makedirs(os.path.dirname(path), exist_ok=True)
            with open(path, "w", encoding="utf-8") as fh:
                fh.write(src)
        e = dict(os.environ, PYTHONDONTWRITEBYTECODE="1", **(env or {}))
        t0 = time.perf_counter()
        try:
            r = subprocess.run([sys.executable, *argv], cwd=d, capture_output=True, text=True, timeout=timeout, env=e)
            return {"exit": r.returncode, "stdout": r.stdout[-4000:], "stderr": r.stderr[-4000:],
                    "seconds": round(time.perf_counter() - t0, 3)}
        except subprocess.TimeoutExpired:
            return {"exit": -9, "stdout": "", "stderr": "timeout", "seconds": timeout}
    finally:
        shutil.rmtree(d, ignore_errors=True)


def run_sh(cmd: str, setup: dict[str, str] | None = None, timeout: float = 10.0) -> dict:
    """在临时目录跑一条 shell 命令，返回退出码、输出与命令后目录里的文件清单（真值用）。"""
    d = sandbox_dir()
    try:
        for name, src in (setup or {}).items():
            with open(os.path.join(d, name), "w", encoding="utf-8") as fh:
                fh.write(src)
        try:
            r = subprocess.run(["/bin/sh", "-c", cmd], cwd=d, capture_output=True, text=True, timeout=timeout)
            exit_code, out, err = r.returncode, r.stdout[-2000:], r.stderr[-2000:]
        except subprocess.TimeoutExpired:
            exit_code, out, err = -9, "", "timeout"
        files = {}
        for name in sorted(os.listdir(d)):
            p = os.path.join(d, name)
            if os.path.isfile(p):
                try:
                    with open(p, encoding="utf-8") as fh:
                        files[name] = fh.read()[:500]
                except Exception:
                    files[name] = "<binary>"
        return {"cmd": cmd, "exit": exit_code, "stdout": out, "stderr": err, "files": files}
    finally:
        shutil.rmtree(d, ignore_errors=True)


# ---------------------------------------------------------------- 跑一条探针
def run_probe(probe: Probe, client_factory: Callable[[], Any], root: str | None, n: int, seed: int,
              replay: bool = True, skip_bare: bool = False) -> dict:
    samples = probe.make_samples(n, seed)
    pos = sum(1 for s in samples if bool(s.truth) is True) if probe.kind == "noul" else None
    out: dict[str, Any] = {"probe": probe.name, "title": probe.title, "kind": probe.kind, "n": len(samples),
                           "positives": pos}
    # 构建器
    with jv.Runtime(client=client_factory(), root=root) as rt:
        res_b = probe.builder(samples, rt)
        rep = rt.stats_report()
        out["builder"] = {"metrics": metrics(probe.kind, res_b), "layers": rep["layers"],
                          "questions_per_layer": rep.get("questions_per_layer"), "questions": rep["questions"],
                          "calls": rep["calls"], "fusion_rate": rep["fusion_rate"], "tokens": rep["tokens"],
                          "cost": rep["cost"], "ledger_hits": rep["ledger_hits"],
                          "warnings": sorted({w.split(":")[0] for w in rt.stats["warnings"]}),
                          "saved": recall_saved(res_b) if probe.kind == "noul" else None}
        out["builder_results"] = [r.__dict__ for r in res_b]
    if replay and root:
        with jv.Runtime(client=client_factory(), root=root) as rt2:
            probe.builder(samples, rt2)
            rep2 = rt2.stats_report()
            out["replay"] = {"calls": rep2["calls"], "ledger_hits": rep2["ledger_hits"], "cost": rep2["cost"]}
    # 裸调
    tally: dict = {}
    if skip_bare:
        out["bare"] = {}
        return out
    res_bare = probe.bare(samples, client_factory(), tally)
    out["bare"] = {"metrics": metrics(probe.kind, res_bare), **{k: (round(v, 6) if isinstance(v, float) else v) for k, v in tally.items()},
                   "saved": recall_saved(res_bare) if probe.kind == "noul" else None}
    # 无 Jev 基线
    if probe.baseline:
        res_base = probe.baseline(samples)
        out["baseline"] = {"metrics": metrics(probe.kind, res_base),
                           "saved": recall_saved(res_base) if probe.kind == "noul" else None} if res_base else None
    return out


def main(argv: list[str] | None = None) -> dict:
    ap = argparse.ArgumentParser(description="E-PROBE-10：十条可运行探针")
    ap.add_argument("--real", action="store_true", help="真机 JevClient（先预注册；总上限 $0.30，单条 $0.05）")
    ap.add_argument("--only", default="", help="逗号分隔的探针名，如 p13,p17")
    ap.add_argument("--tag", default="fake")
    ap.add_argument("--n", type=int, default=24)
    ap.add_argument("--seed", type=int, default=0)
    ap.add_argument("--cap-probe", type=float, default=0.05)
    ap.add_argument("--cap-total", type=float, default=0.30)
    ap.add_argument("--reuse-bare", default="", help="从该 tag 的 summary.json 复用裸调臂与基线（构建器走账本重放，不再付费）")
    a = ap.parse_args(argv)
    reuse = {}
    if a.reuse_bare:
        with open(os.path.join(RUNS_ROOT, a.reuse_bare, "summary.json"), encoding="utf-8") as fh:
            reuse = json.load(fh)["probes"]
    from foundation.jv.probes import ALL  # noqa: F401  注册十条
    names = [x for x in a.only.split(",") if x] or list(PROBES)
    summary: dict[str, Any] = {"tag": a.tag, "real": a.real, "n": a.n, "probes": {}, "total_cost": 0.0}
    root_base = os.path.join(RUNS_ROOT, a.tag)
    for name in names:
        p = PROBES[name]
        factory = (lambda: jv.JevClient()) if a.real else (lambda: jv.FakeClient(rule=fake_rule))
        root = os.path.join(root_base, name) if a.real else None
        r = run_probe(p, factory, root, a.n, a.seed, replay=a.real, skip_bare=name in reuse)
        if name in reuse:
            r["bare"] = reuse[name]["bare"]; r["baseline"] = reuse[name].get("baseline"); r["bare_reused_from"] = a.reuse_bare
        cost = float(r["builder"]["cost"]) + float(r["bare"].get("cost", 0.0)) + float((r.get("replay") or {}).get("cost", 0.0))
        r["cost_total"] = round(cost, 6)
        summary["probes"][name] = r
        summary["total_cost"] = round(summary["total_cost"] + cost, 6)
        line = {k: v for k, v in r.items() if k != "builder_results"}
        print(json.dumps(line, ensure_ascii=False))
        if a.real and (cost > a.cap_probe or summary["total_cost"] > a.cap_total):
            summary["stopped"] = f"预算上限：{name} ${cost:.4f} / 累计 ${summary['total_cost']:.4f}"
            print(summary["stopped"])
            break
    if a.real:
        os.makedirs(root_base, exist_ok=True)
        with open(os.path.join(root_base, "summary.json"), "w", encoding="utf-8") as fh:
            json.dump(summary, fh, ensure_ascii=False, indent=1)
    print(f"total_cost={summary['total_cost']}")
    return summary
