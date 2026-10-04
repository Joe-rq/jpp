"""预注册 12：网络第一次在 1 万人上真跑。单人冷接入的时延、花费、能否静止。

三步，都在 应用/通爻网 下跑：
  python -m host.scale10k prepare --out runs/scale10k/packs-10k            # 490 原包 + 9497 生成背景（--no-genbg：只 490 原包）
  towow serve --port 8795 --judge live --max-cost 4 --preload runs/scale10k/packs-10k --preload-background   # 另起，记 PID
  python -m host.scale10k drive --url http://127.0.0.1:8795 --ledger runs/raw/serve-<时间>.ledger.jsonl \\
         --pid <PID> --expect 9987 --out runs/scale10k/drive-10k.json
  python -m host.scale10k analyze --ledger ... --drive runs/scale10k/drive-10k.json --out runs/scale10k/summary-10k.json

drive 用宿主自带的 MCP 客户端（host.cli.call）一位一位接入：只交原包的 t0 片段与 display，token 只在本进程内存里；
每秒读机会与 /healthz，每 3 秒读收件箱并拒绝全部补信息请求；账本里除 host_read 外 60 秒没有新行算静止（600 秒封顶）。
花费：累计 ≥ --stop-cost 不再接新的人；≥ --kill-cost 按 PID 停服务（只停自己起的那个进程）。
"""
from __future__ import annotations

import argparse
import asyncio
import json
import os
import signal
import statistics
import sys
import time

APP_DIR = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
sys.path.insert(0, APP_DIR)

JOINERS = ["a0125", "a0294", "a0383", "a0401", "a0132", "a0269", "a0390", "a0040", "a0224", "a0255"]
GOLD_TYPES = ("latent", "relay")


def t0_pack(p: dict) -> dict:
    t0 = lambda xs: [f for f in (xs or []) if isinstance(f, str) or (isinstance(f, dict) and f.get("tier", "t0") == "t0")]  # noqa: E731
    return {"display": p.get("display"), "lang": p.get("lang", "zh"), "signals": t0(p.get("signals")),
            "offers": t0(p.get("offers")), "catchers": t0(p.get("catchers")), "forbids": t0(p.get("forbids")),
            "projects": t0(p.get("projects"))}


def gold_partners(joiners: list[str]) -> dict[str, list[str]]:
    gold = json.load(open(os.path.join(APP_DIR, "world", "gold", "structures.json"), encoding="utf-8"))
    out: dict[str, set] = {x: set() for x in joiners}
    for g in gold:
        if g["type"] not in GOLD_TYPES:
            continue
        ids = [m["id"] for m in g["members"]]
        for x in joiners:
            if x in ids:
                out[x].update(y for y in ids if y != x and y not in joiners)
    return {x: sorted(v) for x, v in out.items()}


# ------------------------------------------------------------------ prepare

def prepare(a):
    from host.scale import load_packs
    from host.scale_rank import load_genbg
    packs = load_packs(os.path.join(APP_DIR, "world", "packs"))
    out = a.out if os.path.isabs(a.out) else os.path.join(APP_DIR, a.out)
    os.makedirs(out, exist_ok=True)
    for f in os.listdir(out):
        if f.endswith(".json"):
            os.remove(os.path.join(out, f))
    n = 0
    for aid, p in packs.items():
        if aid in JOINERS:
            continue
        json.dump({**p, "id": aid}, open(os.path.join(out, aid + ".json"), "w", encoding="utf-8"), ensure_ascii=False)
        n += 1
    m = 0
    if not a.no_genbg:
        for gid, p in load_genbg(9500, packs).items():
            json.dump({**p, "id": gid, "owner_display": (p.get("display") or gid)[:20]},
                      open(os.path.join(out, gid + ".json"), "w", encoding="utf-8"), ensure_ascii=False)
            m += 1
    print(json.dumps({"out": out, "original": n, "genbg": m, "total": n + m}, ensure_ascii=False))


# ------------------------------------------------------------------ drive

class LedgerTail:
    """账本尾巴：只读新行，记最后一条非 host_read 行的时刻与种类。"""

    def __init__(self, path: str):
        self.f = open(path, encoding="utf-8")
        self.f.seek(0, 2)
        self.buf = ""
        self.last_t = None
        self.last_kind = None
        self.rows = 0

    def poll(self):
        chunk = self.f.read()
        if not chunk:
            return
        self.buf += chunk
        *lines, self.buf = self.buf.split("\n")
        for ln in lines:
            if not ln.strip():
                continue
            try:
                d = json.loads(ln)
            except json.JSONDecodeError:
                continue
            self.rows += 1
            if d.get("kind") != "host_read":
                self.last_t, self.last_kind = d.get("t"), d.get("kind")


def rss_mb(pid: int | None) -> float | None:
    if not pid:
        return None
    import subprocess
    try:
        out = subprocess.run(["ps", "-o", "rss=", "-p", str(pid)], capture_output=True, text=True).stdout.strip()
        return round(int(out) / 1024, 1) if out else None
    except (OSError, ValueError):
        return None


async def healthz(url: str) -> dict:
    import httpx
    async with httpx.AsyncClient(timeout=10) as c:
        return (await c.get(url.rstrip("/") + "/healthz")).json()


async def wait_preload(url: str, expect: int, pid: int | None, log) -> dict:
    t0 = time.time()
    peak = 0.0
    last = -1
    while True:
        try:
            h = await healthz(url)
        except Exception:
            h = None
        r = rss_mb(pid) or 0.0
        peak = max(peak, r)
        if h:
            n = h.get("agents", 0)
            if n != last and (n // 500 != last // 500 or n >= expect):
                log(f"preload {n}/{expect} {time.time() - t0:.0f}s rss={r}MB")
            last = n
            if n >= expect:
                await asyncio.sleep(5)
                return {"agents": n, "wait_s": round(time.time() - t0, 1), "rss_mb_peak_during_preload": peak,
                        "rss_mb_after": rss_mb(pid)}
        await asyncio.sleep(2)


async def one_join(url: str, aid: str, pack: dict, tail: LedgerTail, a, log) -> dict:
    from host.cli import call
    h0 = await healthz(url)
    c0 = h0["budget"]["cost_used_usd"]
    name, host_agent = f"万人测试 {aid}", "scale10k-probe"
    tail.poll()
    t_call = time.time()
    res = await call(url, "towow_join", {"pack": pack, "agent_name": name, "host_agent": host_agent, "token": ""})
    t_ret = time.time()
    if res.get("error") or not res.get("token"):
        return {"id": aid, "error": res.get("error") or "no token"}
    uid, tok = res["agent_id"], res["token"]
    rec = {"id": aid, "uid": uid, "t_call": t_call, "join_return_s": round(t_ret - t_call, 3),
           "t0_fragments": res.get("t0_fragments"), "network_at_join": res.get("network"), "cost_before": c0}
    first_opp = first_plan = None
    denied, polls, rss_peak = 0, 0, 0.0
    quiet_at = None
    killed = False
    last_inbox = 0.0
    opps = {}
    while True:
        await asyncio.sleep(1.0)
        now = time.time()
        polls += 1
        try:
            opps = await call(url, "towow_opportunities", {"agent_id": uid, "token": tok})
        except Exception as e:     # noqa: BLE001
            log(f"  opps error {e!r}")
            opps = {}
        items = opps.get("opportunities") or []
        if items and first_opp is None:
            first_opp = now - t_call
            log(f"  {aid} 第一个机会 {first_opp:.1f}s（{len(items)} 个）")
        if first_plan is None and any(o.get("plan") for o in items):
            first_plan = now - t_call
            log(f"  {aid} 第一份方案 {first_plan:.1f}s")
        if now - last_inbox >= 3.0:
            last_inbox = now
            try:
                ib = await call(url, "towow_inbox", {"agent_id": uid, "token": tok})
                for q in ib.get("requests") or []:
                    await call(url, "towow_respond", {"agent_id": uid, "token": tok, "request_id": q["request_id"],
                                                      "grant": False, "text": ""})
                    denied += 1
            except Exception as e:     # noqa: BLE001
                log(f"  inbox error {e!r}")
        try:
            h = await healthz(url)
            cost = h["budget"]["cost_used_usd"]
        except Exception:
            h, cost = None, None
        rss_peak = max(rss_peak, rss_mb(a.pid) or 0.0)
        if cost is not None and cost >= a.kill_cost and a.pid:
            log(f"  花费 ${cost} ≥ ${a.kill_cost}，停服务 PID {a.pid}")
            os.kill(a.pid, signal.SIGTERM)
            killed = True
            break
        tail.poll()
        if tail.last_t and now - t_call >= 30 and now - tail.last_t >= a.quiet_s:
            quiet_at = tail.last_t - t_call
            break
        if now - t_call >= a.max_wait:
            break
    h1 = await healthz(url) if not killed else {}
    plans = [o for o in (opps.get("opportunities") or []) if o.get("plan")]
    rec.update({"first_opp_s": first_opp and round(first_opp, 2), "first_plan_s": first_plan and round(first_plan, 2),
                "quiet": quiet_at is not None, "last_activity_s": round(quiet_at, 2) if quiet_at is not None else None,
                "last_kind": tail.last_kind, "observed_s": round(time.time() - t_call, 1),
                "opps_end": len(opps.get("opportunities") or []), "plans_end": len(plans),
                "plans_draft": sum(1 for o in plans if (o.get("plan") or {}).get("draft")),
                "plan_conf": [((o.get("confidence") or {}).get("p")) for o in plans],
                "opps_with": [[w.get("id") for w in o.get("with") or []] for o in opps.get("opportunities") or []],
                "discovery": opps.get("discovery"), "denied_requests": denied, "polls": polls,
                "cost_after": (h1.get("budget") or {}).get("cost_used_usd"), "rss_mb_peak": rss_peak, "killed": killed,
                "skipped_for_budget": (h1.get("budget") or {}).get("skipped_for_budget"),
                "quiet_within_600s": quiet_at is not None and quiet_at <= 600})
    if rec["cost_after"] is not None:
        rec["cost"] = round(rec["cost_after"] - c0, 4)
    return rec


async def drive(a):
    log = lambda m: print(time.strftime("%H:%M:%S"), m, flush=True)     # noqa: E731
    from host.scale import load_packs
    packs = load_packs(os.path.join(APP_DIR, "world", "packs"))
    joiners = a.joiners.split(",") if a.joiners else JOINERS
    pre = await wait_preload(a.url, a.expect, a.pid, log)
    log(f"预载完成 {pre}")
    h = await healthz(a.url)
    out = {"url_port": a.url.rsplit(":", 1)[-1], "preload": pre, "healthz_before": h, "joins": [],
           "gold_partners": gold_partners(joiners), "started": time.time()}
    tail = LedgerTail(a.ledger)
    path = a.out if os.path.isabs(a.out) else os.path.join(APP_DIR, a.out)
    for aid in joiners:
        hh = await healthz(a.url)
        if hh["budget"]["cost_used_usd"] >= a.stop_cost:
            log(f"累计花费 ${hh['budget']['cost_used_usd']} ≥ ${a.stop_cost}，不再接入")
            out["stopped_by_cost"] = True
            break
        log(f"接入 {aid}（在场 {hh.get('agents')}）")
        rec = await one_join(a.url, aid, t0_pack(packs[aid]), tail, a, log)
        log(f"  {json.dumps({k: rec.get(k) for k in ('join_return_s', 'first_opp_s', 'first_plan_s', 'quiet', 'last_activity_s', 'opps_end', 'plans_end', 'cost')}, ensure_ascii=False)}")
        out["joins"].append(rec)
        json.dump(out, open(path, "w", encoding="utf-8"), ensure_ascii=False, indent=1)
        if rec.get("killed") or rec.get("error"):
            break
        if not rec.get("quiet"):       # 到封顶还不静止：不接下一位（窗口重叠会让按接入切的账没有意义）
            log("  到观察封顶仍不静止，停止接入")
            out["stopped_not_quiet"] = aid
            break
    out["finished"] = time.time()
    with_suppress = None
    try:
        out["healthz_after"] = await healthz(a.url)
    except Exception as e:     # noqa: BLE001
        with_suppress = repr(e)
    out["healthz_after_error"] = with_suppress
    json.dump(out, open(path, "w", encoding="utf-8"), ensure_ascii=False, indent=1)
    log(f"写 {path}")


# ------------------------------------------------------------------ analyze

def analyze(a):
    """从账本按接入窗口切：第一段（rerank）/ 第二段（rerank2）时间、按程序分的调用与花费、未观察题、召回前 32 里的真伙伴。"""
    d = json.load(open(a.drive, encoding="utf-8"))
    rows = [json.loads(ln) for ln in open(a.ledger, encoding="utf-8") if ln.strip()]
    pubs = {}
    for r in rows:
        if r.get("kind") == "publish" and r.get("cell") == "world" and r.get("writer") == "host":
            k = str((r.get("key") or [None])[0])
            pubs.setdefault(k, r["t"])
    batches = {r["id"]: r for r in rows if r.get("kind") == "batch"}
    qkind: dict[int, str] = {}
    for r in rows:
        if r.get("kind") == "judge" and r.get("batch") in batches and r["batch"] not in qkind:
            qkind[r["batch"]] = r.get("q")
    preload_end = None
    joins = d["joins"]
    starts = [pubs.get(j.get("uid")) for j in joins]
    out = []
    for i, j in enumerate(joins):
        uid, tp = j.get("uid"), starts[i]
        if not uid or tp is None:
            out.append({"id": j["id"], "error": "no publish"})
            continue
        tn = starts[i + 1] if i + 1 < len(starts) and starts[i + 1] else float("inf")
        win = [r for r in rows if tp <= r.get("t", 0) < tn]
        bs = [r for r in win if r.get("kind") == "batch"]
        by_prog: dict[str, list] = {}
        for b in bs:
            name = b["payer"].split("(")[0].split("#")[0]
            if name == "召回":     # 接入者自己的两段召回，与构型节点的召回（向量 40 → 8）分开记
                name = "召回·接入者" if str(b["payer"]).startswith(f"召回({uid}") else "召回·构型节点"
            by_prog.setdefault(name, []).append(b)
        st = {}
        for q in ("rerank", "rerank2"):
            L = [b for b in bs if qkind.get(b["id"]) == q and str(b.get("payer", "")).startswith(f"召回({uid}")]
            if L:
                s0 = min(b["t"] - b["latency_ms"] / 1000 for b in L)
                st[q] = {"calls": len(L), "questions": sum(b["n"] for b in L), "cost": round(sum(b["cost"] for b in L), 5),
                         "first_sent_s": round(s0 - tp, 2), "last_back_s": round(max(b["t"] for b in L) - tp, 2),
                         "q_per_call": round(sum(b["n"] for b in L) / len(L), 1),
                         "latency_ms_p50": sorted(b["latency_ms"] for b in L)[len(L) // 2]}
        unobs = [r for r in win if r.get("kind") in ("unobserved", "absent")]
        unobs_budget = sum(int(r.get("n", 1)) for r in unobs if r.get("cause") == "budget")
        peers = []
        for r in win:
            if r.get("kind") == "unit" and r.get("owner") == f"召回({uid})" and str(r.get("uid", "")).startswith("两两"):
                try:
                    k = json.loads(r["args"])[0]
                    peers.append(k["b"] if k["a"] == uid else k["a"])
                except (ValueError, KeyError, IndexError, TypeError):
                    pass
        gp = d["gold_partners"].get(j["id"], [])
        held_with = {x for w in j.get("opps_with") or [] for x in w}
        first_edge = next((r["t"] - tp for r in win if r.get("kind") == "publish" and r.get("cell") == "edge"), None)
        last_judge = max((b["t"] for b in bs), default=None)
        out.append({"id": j["id"], "uid": uid, "join_return_s": j.get("join_return_s"),
                    "stage1": st.get("rerank"), "stage2": st.get("rerank2"),
                    "stage1_pre_dispatch_s": st.get("rerank", {}).get("first_sent_s"),
                    "stage1_jev_s": (round(st["rerank"]["last_back_s"] - st["rerank"]["first_sent_s"], 2) if "rerank" in st else None),
                    "stage2_wall_s": (round(st["rerank2"]["last_back_s"] - st["rerank"]["last_back_s"], 2)
                                      if "rerank" in st and "rerank2" in st else None),
                    "first_edge_s": first_edge and round(first_edge, 2), "first_opp_s": j.get("first_opp_s"),
                    "first_plan_s": j.get("first_plan_s"), "quiet": j.get("quiet"), "last_activity_s": j.get("last_activity_s"),
                    "last_judge_s": last_judge and round(last_judge - tp, 1),
                    "cost_healthz": j.get("cost"), "cost_ledger": round(sum(b["cost"] for b in bs), 5),
                    "calls": len(bs), "calls_by_program": {k: len(v) for k, v in by_prog.items()},
                    "cost_by_program": {k: round(sum(b["cost"] for b in v), 5) for k, v in by_prog.items()},
                    "calls_after_recall": sum(len(v) for k, v in by_prog.items() if k != "召回·接入者"),
                    "unobserved_questions": sum(int(r.get("n", 1)) for r in unobs),
                    "unobserved_budget_questions": unobs_budget, "skipped_for_budget_after": j.get("skipped_for_budget"),
                    "quiet_within_600s": bool(j.get("quiet")) and (j.get("last_activity_s") or 1e9) <= 600,
                    "stage1_unobserved": sum(int(r.get("n", 1)) for r in unobs if str(r.get("sid")) in
                                             {str(b.get("sid")) for b in bs if str(b.get("payer", "")).startswith(f"召回({uid}")}),
                    "unobserved_rows": [{k: r.get(k) for k in ("kind", "cause", "n", "msg")} for r in unobs[:5]],
                    "top32": len(peers), "gold": gp, "gold_in_top32": [p for p in gp if p in peers],
                    "gold_held": [p for p in gp if p in held_with],
                    "opps_end": j.get("opps_end"), "plans_end": j.get("plans_end"), "plans_draft": j.get("plans_draft"),
                    "denied_requests": j.get("denied_requests"), "network_at_join": j.get("network_at_join")})
    ok = [o for o in out if "error" not in o]

    def med(k, sub=None):
        xs = [(o.get(k) or {}).get(sub) if sub else o.get(k) for o in ok]
        xs = [x for x in xs if isinstance(x, (int, float))]
        return round(statistics.median(xs), 4) if xs else None

    summ = {"n_joins": len(ok), "preload": d.get("preload"),
            "median": {k: med(k) for k in ("join_return_s", "stage1_pre_dispatch_s", "stage1_jev_s", "stage2_wall_s",
                                           "first_edge_s", "first_opp_s", "first_plan_s", "last_activity_s", "last_judge_s",
                                           "cost_healthz", "cost_ledger", "calls", "calls_after_recall", "opps_end")},
            "median_stage1": {k: med("stage1", k) for k in ("calls", "questions", "cost", "q_per_call", "latency_ms_p50")},
            "median_stage2": {k: med("stage2", k) for k in ("calls", "questions", "cost")},
            "quiet": f"{sum(1 for o in ok if o.get('quiet'))}/{len(ok)}",
            "with_plan": f"{sum(1 for o in ok if o.get('first_plan_s'))}/{len(ok)}",
            "gold_in_top32": f"{sum(len(o['gold_in_top32']) for o in ok)}/{sum(len(o['gold']) for o in ok)}",
            "gold_held": f"{sum(len(o['gold_held']) for o in ok)}/{sum(len(o['gold']) for o in ok)}",
            "degraded": [o["id"] for o in ok if o["unobserved_budget_questions"] > 0
                         or (o.get("stage1") and o["stage1_unobserved"] > 0.02 * o["stage1"]["questions"])],
            "quiet_within_600s": f"{sum(1 for o in ok if o.get('quiet_within_600s'))}/{len(ok)}",
            "total_cost_healthz": round(sum(o.get("cost_healthz") or 0 for o in ok), 4),
            "total_cost_ledger": round(sum(o.get("cost_ledger") or 0 for o in ok), 4),
            "preload_end": preload_end, "joins": out}
    path = a.out if os.path.isabs(a.out) else os.path.join(APP_DIR, a.out)
    json.dump(summ, open(path, "w", encoding="utf-8"), ensure_ascii=False, indent=1)
    print(json.dumps({k: v for k, v in summ.items() if k != "joins"}, ensure_ascii=False, indent=1))


def main():
    ap = argparse.ArgumentParser(prog="host.scale10k")
    sp = ap.add_subparsers(dest="cmd", required=True)
    p = sp.add_parser("prepare")
    p.add_argument("--out", required=True)
    p.add_argument("--no-genbg", action="store_true")
    d = sp.add_parser("drive")
    d.add_argument("--url", default="http://127.0.0.1:8795")
    d.add_argument("--ledger", required=True)
    d.add_argument("--pid", type=int, default=None)
    d.add_argument("--expect", type=int, required=True, help="预载完成时的在场人数")
    d.add_argument("--joiners", default="")
    d.add_argument("--quiet-s", type=float, default=60.0)
    d.add_argument("--max-wait", type=float, default=600.0)
    d.add_argument("--stop-cost", type=float, default=3.5)
    d.add_argument("--kill-cost", type=float, default=4.0)
    d.add_argument("--out", required=True)
    z = sp.add_parser("analyze")
    z.add_argument("--ledger", required=True)
    z.add_argument("--drive", required=True)
    z.add_argument("--out", required=True)
    a = ap.parse_args()
    if a.cmd == "prepare":
        prepare(a)
    elif a.cmd == "drive":
        if a.url.rstrip("/").endswith(":8794"):
            raise SystemExit("8794 是公网服务，本实验用 8795")
        asyncio.run(drive(a))
    else:
        analyze(a)


if __name__ == "__main__":
    main()
