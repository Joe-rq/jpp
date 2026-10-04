"""模拟驱动：把 world/packs/*.json（模拟 agent 的算子包）按种子随机顺序逐个接入网络，跑完导出结果。

驱动只做宿主的事：按节奏写源单元（接入 world[a]、披露变化改写 world[a]、离开删除 world[a]），
从总线记事件，结束时从引擎 peek 各单元导出 result.json。预算、截止、合批、未决去向都在语言里，
这里不给接入事件传 budget / deadline（主会话 2026-10-04 裁定：归语言的 resident 子句）。

用法：
  .venv/bin/python -m host.simulate --n 20 --fixtures            # 最薄链路，离线伪读数，0 美元
  .venv/bin/python -m host.simulate --n 500 --live --disclose 30 --leave 20
选项：--seed 接入顺序种子；--interval 两次接入之间的秒数（0 = 每次接入后等引擎静止再接下一个，
calls 可精确归到该次接入）；--enc bge|hash；--gen 开生成器（--live 默认开）；--run 运行名。
产物：runs/<run>/events.jsonl（events.md 格式，前端回放）、result.json（host/eval.py 的输入）、
      summary.json；原始账本 ledger.jsonl 与引擎事件 engine-events.jsonl 只留本机（.gitignore）。
"""
from __future__ import annotations

import argparse
import asyncio
import glob
import json
import os
os.environ.setdefault("HF_HUB_OFFLINE", "1")   # bge-m3 已在本机缓存；不去 HF Hub 查更新（无网或限流时会卡住启动）
import random
import sys
import time

APP_DIR = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
sys.path.insert(0, APP_DIR)

from host import views  # noqa: E402
from host.index import FragmentIndex  # noqa: E402
from host.server import Host  # noqa: E402


def load_packs(pack_dir: str) -> dict[str, dict]:
    out = {}
    for p in sorted(glob.glob(os.path.join(pack_dir, "*.json"))):
        try:
            d = json.load(open(p, encoding="utf-8"))
        except (OSError, json.JSONDecodeError):
            continue
        aid = str(d.get("id") or os.path.basename(p)[:-5])
        out[aid] = d
    return out


def world_of(aid: str, pack: dict) -> dict:
    """模拟 agent 的 world[a]：算子包原样（t1/t2 片段按 tier 标着，披露程序从这里取）+ 展示字段。"""
    w = {"lang": "zh", "signals": [], "offers": [], "catchers": [], "forbids": [], "projects": [], "policy": {},
         **pack}
    w["id"] = aid
    w["real"] = False
    w["owner"] = {"display_name": pack.get("owner_display"), "city": pack.get("city")}
    w.setdefault("display", pack.get("owner_display") or aid)
    return w


def disclose_change(w: dict, rng: random.Random) -> dict | None:
    """一次披露变化：主人决定把一条 t1 片段改为公开（t0）。没有 t1 片段则返回 None。"""
    cands = [(k, i) for k in ("signals", "offers", "catchers") for i, f in enumerate(w.get(k) or [])
             if isinstance(f, dict) and f.get("tier") == "t1"]
    if not cands:
        return None
    k, i = rng.choice(cands)
    w2 = json.loads(json.dumps(w))
    w2[k][i]["tier"] = "t0"
    return w2


def gold_subset(pool: list[str], a, rng: random.Random) -> tuple[list[str], dict]:
    """回归子集：驱动（不是系统）按真值挑 K 个成员都有算子包的结构，再混入不少于成员数 × ratio 的随机非成员当干扰。
    系统运行中仍不读真值；挑法写进 summary.json（主会话 2026-10-04 定）。"""
    gold = json.load(open(os.path.join(APP_DIR, "world", "gold", "structures.json"), encoding="utf-8"))
    have = set(pool)
    types = set(a.gold_types.split(",")) if a.gold_types else None
    cands = [g for g in gold if (types is None or g["type"] in types)
             and {m["id"] for m in g["members"]} <= have]
    cands.sort(key=lambda g: g["id"])
    rng.shuffle(cands)
    picked = cands[: a.gold_structures]
    members = sorted({m["id"] for g in picked for m in g["members"]})
    gold_members = {m["id"] for g in gold for m in g["members"]}
    others = [x for x in pool if x not in gold_members] or [x for x in pool if x not in members]
    n_dist = min(len(others), max(len(members), int(round(len(members) * a.distractor_ratio))))
    distract = rng.sample(others, n_dist)
    ids = members + distract
    rng.shuffle(ids)
    note = {"method": "driver picks gold structures whose members all have packs; adds random non-members",
            "types": sorted(types) if types else "all", "structures": [g["id"] for g in picked],
            "structure_types": {g["id"]: g["type"] for g in picked}, "n_members": len(members),
            "n_distractors": len(distract), "distractor_ratio": a.distractor_ratio,
            "distractors_from": "non-members of any gold structure" if others and set(others).isdisjoint(gold_members)
            else "non-members of picked structures"}
    return ids, note


class Tracker:
    """从总线记：每个 agent 第一次出现机会（act 边或含它的构型）的时刻。只是测量，不影响运行。"""

    def __init__(self, clock):
        self.clock = clock
        self.t_join: dict[str, float] = {}
        self.first: dict[str, float] = {}

    def on_event(self, ev: dict):
        if ev.get("type") != "publish":
            return
        cell, val = ev.get("cell"), ev.get("value")
        if not isinstance(val, dict):
            return
        who = []
        if cell == "edge" and views.truthy(val.get("holds")):
            who = [str(x) for x in (ev.get("key") or [])[:2]]
        elif cell == "config":
            who = [str(m) for m in val.get("members") or []]
        t = self.clock()            # 与 t_join 同一个时钟（事件里的 t 是墙钟，曾混用导致首个机会时延算成 1.7e9 秒）
        for a in who:
            if a in self.t_join and a not in self.first:
                self.first[a] = t


def preload_order(pack_dir: str, n: int | None, seed: int) -> tuple[list[str], dict[str, dict]]:
    packs = load_packs(pack_dir)
    ids = sorted(packs)
    random.Random(seed).shuffle(ids)
    return (ids[:n] if n else ids), packs


async def preload(eng, pack_dir: str, *, n: int | None = None, seed: int = 0, interval: float = 0.0,
                  idle_timeout: float = 120.0, background: bool = False, log=print) -> list[str]:
    """服务预载：按种子顺序把模拟 agent 的算子包逐个写进 world[a]（宿主只送事件）。
    interval=0：每次写完等引擎静止再写下一个；>0：固定节奏。已经在 world 里的 id 跳过（重启续载）。
    background=True：预载成「网络里已经有的人」——只发布进索引、被后来者召回并回答补信息请求，彼此之间不判断（不花钱）。"""
    ids, packs = preload_order(pack_dir, n, seed)
    done = []
    t0 = time.time()
    for i, aid in enumerate(ids):
        if eng.read_host("world", [aid]) is None:
            w = world_of(aid, packs[aid])
            if background:
                w["background"] = True
            await eng.put_source("world", [aid], w, cause=f"join:{aid}")
            if interval > 0:
                ts = time.monotonic()
                await eng.idle(timeout=interval)
                rem = interval - (time.monotonic() - ts)
                if rem > 0:
                    await asyncio.sleep(rem)
            else:
                await eng.idle(timeout=idle_timeout)
        done.append(aid)
        if (i + 1) % 25 == 0 or i + 1 == len(ids):
            st = eng.stats()
            log(f"[preload {i + 1}/{len(ids)}] calls={st.get('calls')} cache_hits={st.get('cache_hits')} "
                f"cost=${st.get('cost_usd', 0):.4f} errors={st.get('errors')} {time.time() - t0:.0f}s")
    return done


def start_preload(host, pack_dir: str, **kw):
    """给 towow serve 用：返回一个同步回调，挂在 host.after_start 上，在服务的事件循环里起预载任务。"""
    def cb():
        host.preload_task = asyncio.get_running_loop().create_task(
            preload(host.engine, pack_dir, log=lambda m: print(m, file=sys.stderr, flush=True), **kw))
    return cb


def start_restore(host):
    """给 towow serve --public 用：预载完成后恢复存盘的真实接入（驱动侧起任务，宿主只提供 restore_joins）。"""
    def cb():
        host.restore_task = asyncio.get_running_loop().create_task(
            host.restore_joins(getattr(host, "preload_task", None), log=lambda m: print(m, file=sys.stderr, flush=True)))
    return cb


def assemble(a, rdir: str):
    """装配：端口（enc / judge / gen）+ 引擎 + 宿主。返回 (eng, host, judge_cache_path)。simulate.run 与 host.scale 共用。"""
    from jx.engine import Engine

    if a.enc == "bge":
        from jx.ports.enc import EncPort
        enc = EncPort(os.path.join(APP_DIR, "runs", "raw", "enc-cache.sqlite"), device=a.device)
    else:
        from host.tests.conftest import FakeEnc
        enc = FakeEnc()
    ports: dict = {"enc": enc}
    if a.live:
        from jx.ports.jev import JevPort
        ports["judge"] = JevPort()
    if a.gen if a.gen is not None else a.live:
        from jx.ports.gen import GenPort
        g = GenPort(os.path.join(APP_DIR, "runs", "gen-cache.sqlite"))
        if hasattr(g, "gen_json"):
            ports["gen"] = g
        else:
            print("注意：GenPort 还没有 gen_json，方案生成走引擎的离线占位", file=sys.stderr)
    if a.judge_cache and not a.live:
        raise SystemExit("--judge-cache 只能配 --live（伪读数不能进跨运行缓存）")
    jc = a.judge_cache if not a.judge_cache or os.path.isabs(a.judge_cache) else os.path.join(APP_DIR, a.judge_cache)
    eng = Engine.load(os.path.join(APP_DIR, a.program), ports=ports, seed=a.seed, flags={f: True for f in a.flag or []},
                      cache_path=jc or None, max_calls=a.max_calls,
                      ledger_path=os.path.join(rdir, "ledger.jsonl"),
                      events_path=os.path.join(rdir, "engine-events.jsonl"))
    host = Host(eng, FragmentIndex(enc), view_path=os.path.join(rdir, "events.jsonl"),
                view_meta={"source": "jev" if a.live else "fixture", "run": os.path.basename(rdir), "agents": getattr(a, "n", None)})
    return eng, host, jc


async def run(a) -> dict:
    from jx.engine import Engine

    packs = load_packs(os.path.join(APP_DIR, a.packs))
    ids = sorted(packs)
    rng = random.Random(a.seed)
    rng.shuffle(ids)
    subset_note = None
    if a.gold_structures:
        ids, subset_note = gold_subset(ids, a, rng)
    elif a.ids:                                   # 指定接入名单（如主会话从真值挑的成员齐全的结构）；顺序仍按种子打乱
        keep = {x.strip() for x in open(a.ids, encoding="utf-8").read().replace(",", "\n").split() if x.strip()}
        missing = keep - set(ids)
        if missing:
            print(f"注意：{len(missing)} 个指定 id 没有算子包：{sorted(missing)[:10]}", file=sys.stderr)
        ids = [x for x in ids if x in keep]
    if not a.gold_structures:
        ids = ids[: a.n]
    if len(ids) < a.n and not a.gold_structures:
        print(f"注意：只有 {len(ids)} 个算子包，少于 --n {a.n}", file=sys.stderr)

    run_name = a.run or f"sim-{'live' if a.live else 'fx'}-n{len(ids)}-s{a.seed}-{time.strftime('%m%d-%H%M%S')}"
    rdir = os.path.join(APP_DIR, "runs", run_name)
    os.makedirs(rdir, exist_ok=True)

    eng, host, jc = assemble(a, rdir)
    tr = Tracker(eng.clock)
    eng.bus.subscribe(tr.on_event)
    await eng.start()

    calls = lambda: eng.stats().get("calls", 0)        # noqa: E731
    joins, present = [], []
    stopped_at = None
    t0 = time.time()
    for i, aid in enumerate(ids):
        c0 = calls()
        tr.t_join[aid] = eng.clock()
        await eng.put_source("world", [aid], world_of(aid, packs[aid]), cause=f"join:{aid}")
        present.append(aid)
        if a.interval > 0:                       # 固定节奏接入：引擎早静止也等满间隔（驱动供时钟）
            ts = time.monotonic()
            await eng.idle(timeout=a.interval)
            rem = a.interval - (time.monotonic() - ts)
            if rem > 0:
                await asyncio.sleep(rem)
        else:
            await eng.idle(timeout=a.idle_timeout)
        joins.append({"id": aid, "t_join": tr.t_join[aid], "calls": calls() - c0})
        if a.max_cost and eng.stats().get("cost_usd", 0) >= a.max_cost:     # 实验花费上限：驱动停止送新事件
            print(f"花费达到上限 ${a.max_cost}，停止接入（已接入 {len(present)}）", file=sys.stderr)
            stopped_at = len(present)
            break
        if (i + 1) % 10 == 0 or i + 1 == len(ids):
            st = eng.stats()
            print(f"[{i + 1}/{len(ids)}] calls={st.get('calls')} q={st.get('questions')} "
                  f"cost=${st.get('cost_usd', 0):.4f} errors={st.get('errors')} {time.time() - t0:.0f}s", flush=True)
    await eng.idle(timeout=a.idle_timeout)
    # 接入阶段一结束就先落一份结果（后面的披露/离开阶段若被打断，接入结果不丢）
    for j in joins:
        j["t_first_opp"] = tr.first.get(j["id"])
    json.dump({**views.result_export(host.eng, present, joins), "phase": "after_joins", "engine": eng.stats()},
              open(os.path.join(rdir, "result-joins.json"), "w"), ensure_ascii=False, indent=1, default=str)
    print("接入阶段结果已写 result-joins.json", flush=True)

    # 披露变化与离开（各自之后等静止）；每次记增量调用数（预注册 P9：一次披露只重判读到它的判断）
    disclosed, left = [], []
    deltas = []
    for _ in range(a.disclose):
        cand = [x for x in present if x not in disclosed]
        rng.shuffle(cand)
        for aid in cand:
            w2 = disclose_change(eng.read_host("world", [aid]) or world_of(aid, packs[aid]), rng)
            if w2:
                c0 = eng.stats().get("calls", 0)
                await eng.put_source("world", [aid], w2, cause=f"disclose:{aid}")
                disclosed.append(aid)
                await eng.idle(timeout=a.idle_timeout)
                deltas.append({"event": "disclose", "id": aid, "calls": eng.stats().get("calls", 0) - c0, "quiet": eng._quiet()})
                break
    for aid in rng.sample(present, min(a.leave, len(present))):
        c0 = eng.stats().get("calls", 0)
        await host.leave(aid)
        present.remove(aid)
        left.append(aid)
        await eng.idle(timeout=a.idle_timeout)
        deltas.append({"event": "leave", "id": aid, "calls": eng.stats().get("calls", 0) - c0, "quiet": eng._quiet()})

    for j in joins:
        j["t_first_opp"] = tr.first.get(j["id"])
    result = views.result_export(host.eng, present, joins)
    json.dump(result, open(os.path.join(rdir, "result.json"), "w"), ensure_ascii=False, indent=1)
    st = eng.stats()
    lat = sorted(j["t_first_opp"] - j["t_join"] for j in joins if j["t_first_opp"] is not None)
    cj = sorted(j["calls"] for j in joins)
    summary = {"run": run_name, "mode": "live" if a.live else "fixtures", "seed": a.seed, "n": len(ids),
               "subset": subset_note or ({"method": "ids file", "file": a.ids} if a.ids else
                                         {"method": "random", "n": a.n}),
               "note": None if a.live else "fixtures：判断全是伪读数，召回无意义，只看链路与调用数",
               "stopped_by_cost_at": stopped_at, "judge_cache": jc or None,
               "disclosed": disclosed, "left": left, "event_deltas": deltas, "wall_s": round(time.time() - t0, 1), "engine": st,
               "edges": len(result["edges"]), "edges_holds": sum(e["holds"] for e in result["edges"]),
               "configs": len(result["configs"]),
               "configs_by_shape": {s: sum(c["shape"] == s for c in result["configs"])
                                    for s in sorted({c["shape"] for c in result["configs"]})},
               "joins_with_opp": len(lat), "first_opp_p50_s": round(lat[len(lat) // 2], 3) if lat else None,
               "calls_per_join_p50": cj[len(cj) // 2] if cj else None,
               "engine_errors": [str(e)[:300] for e in eng.errors[:5]], "n_engine_errors": len(eng.errors)}
    json.dump(summary, open(os.path.join(rdir, "summary.json"), "w"), ensure_ascii=False, indent=1, default=str)
    await eng.stop()
    return summary


def main(argv=None):
    ap = argparse.ArgumentParser(prog="host.simulate")
    ap.add_argument("--n", type=int, default=20)
    ap.add_argument("--seed", type=int, default=0)
    m = ap.add_mutually_exclusive_group()
    m.add_argument("--fixtures", action="store_true", help="离线伪读数（默认）")
    m.add_argument("--live", action="store_true", help="真 JEV（花钱）")
    ap.add_argument("--gen", action=argparse.BooleanOptionalAction, default=None)
    ap.add_argument("--interval", type=float, default=0.0)
    ap.add_argument("--idle-timeout", type=float, default=120.0)
    ap.add_argument("--disclose", type=int, default=0)
    ap.add_argument("--leave", type=int, default=0)
    ap.add_argument("--enc", choices=["bge", "hash"], default="bge")
    ap.add_argument("--device", default="mps")
    ap.add_argument("--packs", default="world/packs")
    ap.add_argument("--program", default="app/net.jpx")
    ap.add_argument("--flag", action="append", help="消融开关，如 --flag no_batch")
    ap.add_argument("--run", default="")
    ap.add_argument("--judge-cache", default="", help="判断缓存 sqlite（与 serve 同一个库），只配 --live")
    ap.add_argument("--max-cost", type=float, default=0.0, help="实验花费上限（美元）：到了驱动就停止接入")
    ap.add_argument("--max-calls", type=int, default=None, help="传给引擎的整场调用上限（只收紧）")
    ap.add_argument("--gold-structures", type=int, default=0, help="回归子集：挑 K 个成员齐全的真值结构")
    ap.add_argument("--gold-types", default="", help="限定结构类型，如 pair,latent,relay,chain")
    ap.add_argument("--distractor-ratio", type=float, default=1.0, help="干扰非成员数 / 成员数，≥1")
    ap.add_argument("--ids", default="", help="接入名单文件（空白或逗号分隔的 agent id）；驱动不读真值，名单由调用方给")
    a = ap.parse_args(argv)
    if a.gold_structures and a.distractor_ratio < 1:
        ap.error("--distractor-ratio 至少 1（干扰数不少于成员数）")
    s = asyncio.run(run(a))
    print(json.dumps({k: v for k, v in s.items() if k != "engine"}, ensure_ascii=False, indent=1, default=str))


if __name__ == "__main__":
    main()
