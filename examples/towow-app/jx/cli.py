"""命令行：
  python -m jx check prog.jpx
  python -m jx run prog.jpx [--fixtures f.json | --live [--record f.json]] [--world w.json] [--events e.jsonl]
                            [--ledger-out p] [--events-out p] [--seed N] [--max-calls N] [--cache p.sqlite]
                            [--no-cells --no-fill --no-batch --no-meta --no-budget-chain --no-absent --no-deadline]
"""
from __future__ import annotations

import argparse
import asyncio
import json
import os
import sys
import time

from .lexer import JxSyntaxError
from .values import JxError

FLAG_NAMES = ["no_cells", "no_fill", "no_batch", "no_meta", "no_budget_chain", "no_absent", "no_deadline"]


def _parser():
    ap = argparse.ArgumentParser(prog="jx")
    sub = ap.add_subparsers(dest="cmd", required=True)
    c = sub.add_parser("check")
    c.add_argument("prog")
    r = sub.add_parser("run")
    r.add_argument("prog")
    r.add_argument("--fixtures")
    r.add_argument("--live", action="store_true")
    r.add_argument("--record")
    r.add_argument("--world", help="宿主先写入的源单元：{族名: {键: 值}} 或 [{cell, key, value}]")
    r.add_argument("--events", help="之后逐条送入的宿主事件 .jsonl：{op: put|remove|event, cell, key, value, budget?, deadline_s?}")
    r.add_argument("--ledger-out")
    r.add_argument("--events-out")
    r.add_argument("--seed", type=int, default=0)
    r.add_argument("--max-calls", type=int)
    r.add_argument("--cache")
    r.add_argument("--enc", choices=["hash", "bge"], default="hash")
    r.add_argument("--quiet", action="store_true")
    for f in FLAG_NAMES:
        r.add_argument("--" + f.replace("_", "-"), action="store_true")
    return ap


def check(path) -> int:
    from .check import check_program
    try:
        diags = check_program(path)
    except (JxSyntaxError, JxError) as e:
        print(e, file=sys.stderr)
        return 1
    for d in diags:
        print(d, file=sys.stderr)
    errs = [d for d in diags if "错误" in d]
    if not errs:
        print(f"{path}: 通过（{len(diags)} 条提示）")
    return 1 if errs else 0


def _world_items(path):
    d = json.load(open(path))
    if isinstance(d, list):
        return [(x["cell"], x["key"], x["value"]) for x in d]
    out = []
    for fam, m in d.items():
        for k, v in m.items():
            out.append((fam, k.split("|"), v))
    return out


async def run(a) -> int:
    from . import hostlib
    from .engine import Engine, VirtualClock
    from .sched import FixturePort, RecordingPort
    ports = {}
    if a.live:
        from .ports.jev import JevPort
        port = JevPort()
        if a.record:
            port = RecordingPort(port, a.record)
        ports["judge"] = port
        clock = None
    else:
        ports["judge"] = FixturePort(path=a.fixtures)
        clock = VirtualClock()
    if a.enc == "bge":
        from .ports.enc import EncPort
        ports["enc"] = EncPort(os.path.expanduser("~/.cache/jx-enc.sqlite"))
    flags = {f: getattr(a, f) for f in FLAG_NAMES}
    eng = Engine.load(a.prog, ports=ports, flags=flags, seed=a.seed, ledger_path=a.ledger_out,
                      events_path=a.events_out, clock=clock, max_calls=a.max_calls, cache_path=a.cache)
    hostlib.install(eng)
    t0 = time.monotonic()
    await eng.start()
    phases = []

    def snap(label):
        s = eng.stats()
        prev = phases[-1]["cum"] if phases else {"calls": 0, "questions": 0, "attempts": 0, "cutoffs": 0}
        phases.append({"phase": label, "calls": s["calls"] - prev["calls"],
                       "questions": s["questions"] - prev["questions"],
                       "attempts": s["attempts"] - prev["attempts"], "cutoffs": s["cutoffs"] - prev["cutoffs"],
                       "cum": {k: s[k] for k in ("calls", "questions", "attempts", "cutoffs")}})
    if a.world:
        for fam, key, val in _world_items(a.world):
            await eng.put_source(fam, key, val)
    await eng.idle()
    snap("world")
    if a.events:
        for line in open(a.events):
            line = line.strip()
            if not line or line.startswith("//"):
                continue
            e = json.loads(line)
            op = e.get("op", "put")
            if op == "put":
                await eng.put_source(e["cell"], e["key"], e["value"], budget=e.get("budget"), deadline_s=e.get("deadline_s"))
            elif op == "remove":
                await eng.remove_source(e["cell"], e["key"])
            elif op == "event":
                await eng.event(e["name"], e.get("value"), budget=e.get("budget"), deadline_s=e.get("deadline_s"))
            await eng.idle()
            snap(e.get("label") or f"{op}:{e.get('cell') or e.get('name')}:{e.get('key')}")
    result = await eng.report()
    s = eng.stats()
    out = {"result": result, "stats": s, "phases": [{k: v for k, v in p.items() if k != "cum"} for p in phases],
           "errors": eng.errors[:20], "seconds": round(time.monotonic() - t0, 2)}
    port = ports["judge"]
    if hasattr(port, "synthetic"):
        out["fixture"] = {"recorded": port.recorded, "synthetic": port.synthetic}
    await eng.stop()
    if a.quiet:
        out.pop("result")
    print(json.dumps(out, ensure_ascii=False, indent=1, default=str))
    return 1 if eng.errors else 0


def main(argv=None):
    a = _parser().parse_args(argv)
    if a.cmd == "check":
        sys.exit(check(a.prog))
    try:
        sys.exit(asyncio.run(run(a)))
    except (JxSyntaxError, JxError) as e:
        print(e, file=sys.stderr)
        sys.exit(1)
