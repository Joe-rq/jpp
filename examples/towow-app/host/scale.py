"""规模实验驱动（预注册 01 的 P3、P5；Fable-B §7 的「真机抽样块」）。

三档 N=500 / 2000 / 10000，每次只跑一档（一个进程、一个编码器）：
  已有人口全部以背景方式写入（world 带 background:true：只发布进索引、被别人召回，自己不召回、不判断），
    500 档  = 480 原住民（world/packs 去掉测试成员）+ 20 测试成员
    2000 档 = 480 + 1500 合成背景 + 20
    10000档 = 480 + 9500 合成背景 + 20
  背景写完后检查判断调用数必须为 0；然后按同一种子逐个接入同一组测试成员，每次等引擎静止，记
  调用数、题数、墙钟、花费、route 时延，以及召回前 32 名对方（运行中不读真值）。
  运行结束后才读 world/gold/structures.json（--gold），算每个测试成员的真值伙伴在前 32 名里有几个。

合成背景人口（`synth_pack`）：signals / offers / catchers / forbids / projects 五类片段各从一个不同的真实原包
整类逐字取来（保持 tier 与片段原文），只换 id（b00001 起）、display、city；id 序号 i 的内容只由 (seed, i) 决定，
所以 2000 档的 1500 个是 10000 档 9500 个的前缀。文件写在 runs/scale/bg/（已 gitignore）。

用法（在应用/通爻网 下；每次最多一个进程加载 bge-m3）：
  python -m host.scale --tier 2000 --ids ids.txt --fixtures --enc hash --limit 3      # 离线自测
  python -m host.scale --tier 10000 --ids ids.txt --live --enc bge --judge-cache runs/judge-cache.sqlite \
        --max-cost 1 --gold world/gold/structures.json --run scale-10000
  python -m host.scale --rescore runs/scale/<run> --gold world/gold/structures.json     # 事后重算真值命中
产物：runs/scale/<run>/summary.json（含每个成员的明细与按档汇总）；ledger.jsonl 等只留本机。
"""
from __future__ import annotations

import argparse
import asyncio
import copy
import hashlib
import json
import os
import random
import resource
import statistics
import sys
import time

APP_DIR = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
sys.path.insert(0, APP_DIR)

from host.simulate import assemble, load_packs, world_of  # noqa: E402

N_RESIDENTS = 480                 # 500 档的原住民数（与测试成员 20 个合起来 500）
TIERS = {500: 0, 2000: 1500, 10000: 9500}      # 档 -> 合成背景数
FRAG_KINDS = ("signals", "offers", "catchers", "forbids", "projects")
BATCH = 200                       # 背景写入分批：每批写完等静止，队列和内存有界


# ---------------------------------------------------------------- 合成背景

def bg_id(i: int) -> str:
    return f"b{i:05d}"


def excl_key(exclude) -> str:
    return "none" if not exclude else "x%d-%s" % (len(exclude), hashlib.md5(",".join(sorted(exclude)).encode()).hexdigest()[:8])


def synth_pack(i: int, pool: dict[str, dict], seed: int, exclude=frozenset()) -> dict:
    """第 i 个合成背景包（i 从 1 起）。五类片段各取自不同原包，原文逐字、tier 原样；只换 id / display / city。
    exclude：不许当片段来源的原包 id（--clean-bg：测试成员及其真值伙伴，防止背景里出现真值伙伴的逐字克隆）。"""
    ids = sorted(x for x in pool if x not in exclude)
    rng = random.Random(f"scale-bg:{seed}:{i}")
    donors = rng.sample(ids, len(FRAG_KINDS))
    city = pool[rng.choice(ids)].get("city")
    aid = bg_id(i)
    src = pool[donors[0]]
    p = {"display": f"{city or ''}的合成背景成员 {aid}", "lang": src.get("lang", "zh")}
    for k, d in zip(FRAG_KINDS, donors):
        p[k] = copy.deepcopy(pool[d].get(k) or [])
    p["policy"] = copy.deepcopy(src.get("policy") or {})
    p.update(id=aid, owner_display=f"背景{aid}", city=city, background=True,
             synthetic={"seed": seed, "excl": excl_key(exclude), "donors": dict(zip(FRAG_KINDS, donors))})
    return p


def write_bg(n: int, pool: dict[str, dict], seed: int, bg_dir: str, exclude=frozenset()) -> list[str]:
    """写出 b00001..b{n}（已有且种子相同的跳过），返回 id 列表。"""
    os.makedirs(bg_dir, exist_ok=True)
    out = []
    for i in range(1, n + 1):
        aid = bg_id(i)
        path = os.path.join(bg_dir, aid + ".json")
        ok = False
        if os.path.exists(path):
            try:
                m = json.load(open(path, encoding="utf-8")).get("synthetic", {})
                ok = m.get("seed") == seed and m.get("excl", "none") == excl_key(exclude)
            except (OSError, json.JSONDecodeError):
                ok = False
        if not ok:
            json.dump(synth_pack(i, pool, seed, exclude), open(path, "w", encoding="utf-8"), ensure_ascii=False)
        out.append(aid)
    return out


# ---------------------------------------------------------------- 真值命中（运行结束后才读）

def pack_texts(p: dict) -> set[str]:
    out = set()
    for k in ("signals", "offers", "catchers", "projects"):
        for f in p.get(k) or []:
            t = f.get("hypo") or f.get("text") or f.get("name") if isinstance(f, dict) else f
            if t:
                out.add(str(t))
    return out


def count_clones(peers: list[str], partner_ids: set[str], packs: dict, seed: int, exclude) -> int:
    """前 32 名里有几个合成背景与真值伙伴共享逐字片段（克隆）。只数 b 开头的合成 id。"""
    pt = set()
    for x in partner_ids:
        if x in packs:
            pt |= pack_texts(packs[x])
    n = 0
    for pid in peers:
        if pid.startswith("b") and pid[1:].isdigit():
            if pack_texts(synth_pack(int(pid[1:]), packs, seed, exclude)) & pt:
                n += 1
    return n


def gold_exclusion(ids: list[str], gold_path: str) -> set[str]:
    """驱动读真值：测试成员 + 它们的真值伙伴（运行中的系统不读）。"""
    gold = json.load(open(gold_path, encoding="utf-8"))
    out = set(ids)
    for g in gold:
        m = [x["id"] for x in g["members"]]
        if out & set(m):
            out |= set(m)
    return out


def score_gold(members: list[dict], gold_path: str, bg_residents: set[str], packs=None, seed=0, exclude=frozenset()) -> dict:
    """members：按接入顺序的明细（含 id、peers32）。每个成员的真值伙伴 = 它所在结构里的其他成员。
    分母取「它接入时已在索引里的伙伴」（原住民 + 先接入的测试成员）：后接入者才找得到先接入者，反过来要靠对方那一侧召回。
    同时给出不限在场的版本（hit_all / partners_all）。"""
    gold = json.load(open(gold_path, encoding="utf-8"))
    partners: dict[str, set[str]] = {}
    for g in gold:
        ids = [m["id"] for m in g["members"]]
        for a in ids:
            partners.setdefault(a, set()).update(x for x in ids if x != a)
    order = [m["id"] for m in members]
    per, hit_p = [], 0
    tot_p = tot_all = hit_all = 0
    for j, m in enumerate(members):
        ps = partners.get(m["id"], set())
        present = bg_residents | set(order[:j])
        in_idx = ps & present
        peers = set(m.get("peers32") or [])
        h, ha = len(peers & in_idx), len(peers & ps)
        row = {"id": m["id"], "partners_present": len(in_idx), "hit_present": h,
               "partners_all": len(ps), "hit_all": ha}
        if packs is not None:
            row["clones_in_top32"] = count_clones(m.get("peers32") or [], ps, packs, seed, exclude)
        per.append(row)
        hit_p += h
        tot_p += len(in_idx)
        hit_all += ha
        tot_all += len(ps)
    cl = sum(r.get("clones_in_top32", 0) for r in per) if packs is not None else None
    return {"gold": gold_path, "clones_in_top32": cl, "recall_at_32_present": round(hit_p / tot_p, 4) if tot_p else None,
            "hit_present": hit_p, "partners_present": tot_p,
            "recall_at_32_all": round(hit_all / tot_all, 4) if tot_all else None,
            "hit_all": hit_all, "partners_all": tot_all, "per_member": per}


# ---------------------------------------------------------------- 驱动

def _pct(xs: list[float], q: float) -> float | None:
    if not xs:
        return None
    xs = sorted(xs)
    return round(xs[min(len(xs) - 1, int(q * len(xs)))], 3)


def _agg(members: list[dict]) -> dict:
    calls = [m["calls"] for m in members]
    qs = [m["questions"] for m in members]
    wall = [m["wall_s"] for m in members]
    rms = [x for m in members for x in m["route_ms"]]
    return {"n_members": len(members),
            "calls_mean": round(statistics.mean(calls), 2) if calls else None, "calls_p50": _pct(calls, .5),
            "questions_mean": round(statistics.mean(qs), 2) if qs else None,
            "wall_s_p50": _pct(wall, .5), "wall_s_p95": _pct(wall, .95),
            "cost_usd_total": round(sum(m["cost_usd"] for m in members), 6),
            "route_ms_p50": _pct(rms, .5), "route_ms_p95": _pct(rms, .95), "n_route_calls": len(rms),
            "n_not_settled": sum(not m.get("settled", True) for m in members)}


async def run(a) -> dict:
    t_start = time.time()
    packs = load_packs(os.path.join(APP_DIR, a.packs))
    keep = [x.strip() for x in open(a.ids, encoding="utf-8").read().replace(",", "\n").split() if x.strip()]
    keep = list(dict.fromkeys(keep))
    missing = [x for x in keep if x not in packs]
    if missing:
        raise SystemExit(f"--ids 里 {len(missing)} 个 id 没有算子包：{missing[:10]}")
    tests = sorted(keep)
    random.Random(a.seed).shuffle(tests)               # 同一种子 → 同一接入顺序，三档一致
    if a.limit:
        tests = tests[: a.limit]
    residents = sorted(set(packs) - set(keep))          # 原住民：除全部测试成员以外
    n_synth = TIERS[a.tier]
    if len(residents) != N_RESIDENTS:
        print(f"注意：原住民 {len(residents)} 个（预期 {N_RESIDENTS}，--ids 给了 {len(keep)} 个），档内总数会偏离 --tier", file=sys.stderr)

    bg_dir = os.path.join(APP_DIR, "runs", "scale", "bg")
    gold_abs = a.gold if (not a.gold or os.path.isabs(a.gold)) else os.path.join(APP_DIR, a.gold)
    if a.clean_bg and not gold_abs:
        raise SystemExit("--clean-bg 需要 --gold（驱动读真值挑背景来源的排除名单）")
    exclude = frozenset(gold_exclusion(keep, gold_abs)) if a.clean_bg else frozenset()
    synth_ids = write_bg(n_synth, packs, a.seed, bg_dir, exclude)
    synth_ids_path = {i: os.path.join(bg_dir, i + ".json") for i in synth_ids}

    run_name = a.run or f"scale-{a.tier}-{'live' if a.live else 'fx'}-s{a.seed}-{time.strftime('%m%d-%H%M%S')}"
    rdir = os.path.join(APP_DIR, "runs", "scale", run_name)
    os.makedirs(rdir, exist_ok=True)
    eng, host, jc = assemble(a, rdir)
    ix = host.index

    # 记录 route / index_put（实例上包一层；Host 的 route 动作在调用时才取 ix.route，所以能截到）
    rec = {"route_ms": [], "peers32": None, "n_route": 0, "put_ms": []}
    watch: set[str] = set()
    orig_route, orig_put = ix.route, ix.index_put

    def route(x, node, k=20):
        t = time.perf_counter()
        res = orig_route(x, node, k)
        dt = (time.perf_counter() - t) * 1000
        if str(x) in watch:                              # 只记测试成员（背景不召回）
            rec["route_ms"].append(round(dt, 2))
            rec["peers32"] = [h["peer"] for h in res[:32]]
            rec["n_route"] += 1
        return res

    def put(owner, node):
        t = time.perf_counter()
        r = orig_put(owner, node)
        if str(owner) in watch:
            rec["put_ms"].append(round((time.perf_counter() - t) * 1000, 2))
        return r

    ix.route, ix.index_put = route, put
    await eng.start()
    st = lambda: eng.stats()                             # noqa: E731

    # ---- 1. 背景写入：只发布与 index_put，不触发判断
    def bg_world(aid: str) -> dict:
        w = world_of(aid, packs[aid] if aid in packs else json.load(open(synth_ids_path[aid], encoding="utf-8")))
        w["background"] = True
        return w

    bg_order = residents + synth_ids
    random.Random(a.seed + 1).shuffle(bg_order)          # 原住民与合成混着进，索引形成顺序不带偏
    tb = time.time()
    for i in range(0, len(bg_order), BATCH):
        for aid in bg_order[i: i + BATCH]:
            await eng.put_source("world", [aid], bg_world(aid), cause=f"bg:{aid}")
        await eng.idle(timeout=a.idle_timeout * 5)
        print(f"[bg {min(i + BATCH, len(bg_order))}/{len(bg_order)}] calls={st().get('calls')} "
              f"vectors={ix.stats()['vectors']} {time.time() - tb:.0f}s", flush=True)
    s0 = st()
    bg = {"n": len(bg_order), "n_residents": len(residents), "n_synth": len(synth_ids),
          "wall_s": round(time.time() - tb, 1), "calls": s0.get("calls", 0), "questions": s0.get("questions", 0),
          "cost_usd": s0.get("cost_usd", 0), "route_calls": ix.stats()["routes"], "index": ix.stats(),
          "zero_judgment": s0.get("calls", 0) == 0 and ix.stats()["routes"] == 0,
          "rss_mb": round(resource.getrusage(resource.RUSAGE_SELF).ru_maxrss / 2 ** 20, 1)}
    print(f"背景写入完成：{bg}", flush=True)

    members, stopped_at = [], None
    if bg["zero_judgment"]:
        # ---- 2. 逐个接入测试成员
        for j, aid in enumerate(tests):
            watch.clear()
            watch.add(aid)
            rec.update(route_ms=[], peers32=None, n_route=0, put_ms=[])
            s1 = st()
            t = time.time()
            await eng.put_source("world", [aid], world_of(aid, packs[aid]), cause=f"join:{aid}")
            await eng.idle(timeout=a.idle_timeout)
            settled = bool(eng._quiet())               # False = 等到 --idle-timeout 仍未静止，该成员的数字是下界
            s2 = st()
            members.append({"id": aid, "order": j, "calls": s2.get("calls", 0) - s1.get("calls", 0),
                            "questions": s2.get("questions", 0) - s1.get("questions", 0),
                            "cost_usd": round(s2.get("cost_usd", 0) - s1.get("cost_usd", 0), 6),
                            "wall_s": round(time.time() - t, 2), "settled": settled, "route_ms": list(rec["route_ms"]),
                            "put_ms": list(rec["put_ms"]), "peers32": rec["peers32"] or []})
            print(f"[{j + 1}/{len(tests)}] {aid} calls={members[-1]['calls']} q={members[-1]['questions']} "
                  f"{members[-1]['wall_s']}s settled={settled} total_cost=${s2.get('cost_usd', 0):.4f}", flush=True)
            if a.max_cost and s2.get("cost_usd", 0) >= a.max_cost:
                print(f"花费达到上限 ${a.max_cost}，停止接入（已接入 {len(members)}）", file=sys.stderr)
                stopped_at = len(members)
                break
    else:
        print("背景写入触发了判断或召回，停止（见 summary.bg）。", file=sys.stderr)

    summary = {"run": run_name, "tier": a.tier, "mode": "live" if a.live else "fixtures", "enc": a.enc, "seed": a.seed,
               "ids_file": a.ids, "judge_cache": jc or None, "stopped_by_cost_at": stopped_at,
               "note": None if a.live else "fixtures：判断全是伪读数，只看链路、调用数与时延；召回数字只在 --enc bge 下有意义",
               "clean_bg": a.clean_bg, "bg_excluded": sorted(exclude), "bg": bg, "members": members, "agg": _agg(members) if members else None,
               "total_n": len(bg_order) + len(members), "engine": st(), "wall_s": round(time.time() - t_start, 1),
               "peak_rss_mb": round(resource.getrusage(resource.RUSAGE_SELF).ru_maxrss / 2 ** 20, 1),
               "engine_errors": [str(e)[:300] for e in eng.errors[:5]], "n_engine_errors": len(eng.errors)}
    if a.gold and members:
        summary["gold"] = score_gold(members, gold_abs, set(residents), packs, a.seed, exclude)
    json.dump(summary, open(os.path.join(rdir, "summary.json"), "w"), ensure_ascii=False, indent=1, default=str)
    await eng.stop()
    host._pool.shutdown(wait=False)
    return summary


def rescore(rdir: str, gold: str) -> dict:
    p = os.path.join(rdir, "summary.json")
    s = json.load(open(p, encoding="utf-8"))
    packs = load_packs(os.path.join(APP_DIR, "world/packs"))
    ids = {m["id"] for m in s["members"]}
    keep = {x.strip() for x in open(s["ids_file"], encoding="utf-8").read().replace(",", "\n").split() if x.strip()}
    s["gold"] = score_gold(s["members"], gold, set(packs) - keep - ids, packs, s["seed"], frozenset(s.get("bg_excluded") or []))
    json.dump(s, open(p, "w"), ensure_ascii=False, indent=1, default=str)
    return s["gold"]


def main(argv=None):
    ap = argparse.ArgumentParser(prog="host.scale")
    ap.add_argument("--tier", type=int, choices=sorted(TIERS), default=500)
    ap.add_argument("--ids", default="", help="测试成员名单文件（空白或逗号分隔），由调用方从真值结构挑；运行中不读真值")
    m = ap.add_mutually_exclusive_group()
    m.add_argument("--fixtures", action="store_true", help="离线伪读数（默认）")
    m.add_argument("--live", action="store_true", help="真 JEV（花钱）")
    ap.add_argument("--enc", choices=["bge", "hash"], default="bge")
    ap.add_argument("--judge-cache", default="", help="判断缓存 sqlite，只配 --live")
    ap.add_argument("--max-cost", type=float, default=0.0, help="花费上限（美元）：到了就停止接入")
    ap.add_argument("--max-calls", type=int, default=None)
    ap.add_argument("--run", default="")
    ap.add_argument("--gold", default="", help="运行结束后读真值算命中@32（world/gold/structures.json）")
    ap.add_argument("--limit", type=int, default=0, help="只接入前 K 个测试成员（离线自测）")
    ap.add_argument("--seed", type=int, default=0)
    ap.add_argument("--idle-timeout", type=float, default=120.0)
    ap.add_argument("--device", default="mps")
    ap.add_argument("--packs", default="world/packs")
    ap.add_argument("--program", default="app/net.jpx")
    ap.add_argument("--clean-bg", action="store_true", help="合成背景不取自测试成员及其真值伙伴的原包（驱动读 --gold 挑名单）")
    ap.add_argument("--rescore", default="", help="对已有运行目录补算真值命中（需 --gold）")
    a = ap.parse_args(argv)
    if a.rescore:
        if not a.gold:
            ap.error("--rescore 需要 --gold")
        print(json.dumps({k: v for k, v in rescore(a.rescore, a.gold).items() if k != "per_member"}, ensure_ascii=False))
        return
    if not a.ids:
        ap.error("需要 --ids")
    a.gen, a.flag, a.n = None, [], 0          # 与 simulate.assemble 对齐：--live 默认开生成器
    s = asyncio.run(run(a))
    print(json.dumps({k: v for k, v in s.items() if k not in ("engine", "members")}, ensure_ascii=False, indent=1, default=str))


if __name__ == "__main__":
    main()
