"""预注册 13 的差分运行：同一条轨迹分别喂给 J++x 与基线，不等静止，最后等静止，出终态检测与过程代价。

    cd 应用/通爻网
    OMP_NUM_THREADS=2 .venv/bin/python -m baseline.diff.run --seeds 0-4 --out <scratchpad>/diff.json

不花钱（规则判断器、假生成器、哈希编码器）。结果摘要打印到标准输出，完整记录写 --out。
"""
from __future__ import annotations

import argparse
import asyncio
import json
import os
import sys
import time

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.dirname(HERE))

from diff.trajectory import Truth, trajectory  # noqa: E402
from diff import systems as S  # noqa: E402
from diff.faults import INJECTIONS  # noqa: E402

TIMEOUT = 120.0


async def drive(sys_cls, seed, fail_calls=(), net_cls=None, timeout=TIMEOUT, max_gap=40):
    T = Truth()
    if net_cls is not None:
        sys_cls = type("Injected", (S.PySystem,), {"net_cls": net_cls})
    sy = sys_cls(T, fail_calls)
    await sy.start()
    seen_departed = 0
    t0 = time.monotonic()
    for e in trajectory(seed, max_gap):
        T.apply(e)                                  # 宿主一侧的真值随事件更新（系统还在算）
        await sy.apply(e)
        await S.ticks(e.gap)                        # 不等静止
        dep = set(T.left) - set(T.world)
        if dep:
            seen_departed += sy.departed_visible(dep)
    st = await sy.settle(timeout)
    fin = sy.final()
    n, errs, strict, union = S.check(fin, T)
    return {"system": sy.name, "seed": seed, "settle": st, "wall_s": round(time.monotonic() - t0, 2),
            "errors": n, "error_detail": {k: v[:5] for k, v in errs.items() if v}, "residue": strict, "union_residue": union,
            "missing_holding": sum(1 for x in errs["edge_missing"] if x.endswith("应成立")),
            "departed_visible": seen_departed, "meter": sy.m.stats(),
            "final": {"edges_holding": sum(1 for v in fin["edges"].values() if v["holds"]),
                      "configs": len(fin["configs"]), "plans": len(fin["plans"]),
                      "multi": sorted(k for k, v in fin["configs"].items() if len(v["members"]) > 2)}}, fin


def run1(*args, **kw):
    """每次运行一个独立的事件循环：挂死的注入留下的任务不影响下一次。"""
    loop = asyncio.new_event_loop()
    try:
        return loop.run_until_complete(drive(*args, **kw))
    finally:
        for t in asyncio.all_tasks(loop):
            t.cancel()
        loop.run_until_complete(asyncio.sleep(0))
        loop.close()


def bad(r):
    return sum(r["errors"].values()) + (1 if r["settle"]["hang"] else 0)


def main_sync(a):
    seeds = _range(a.seeds)
    out = {"S1": [], "S2": [], "inject": {}}
    for sd in seeds:
        rj, fj = run1(S.JxSystem, sd)
        rp, fp = run1(S.PySystem, sd)
        cmp = S.compare(fj, fp)
        out["S1"].append({"seed": sd, "jpx": rj, "py": rp, "diff": cmp})
        print(f"S1 seed {sd}: 一致={cmp['same']} 形状差={len(cmp['shapes'])} | jpx 错={bad(rj)} 挂={rj['settle']['hang']} "
              f"调用={rj['meter']['calls']} 题={rj['meter']['questions']} 陈旧调用={rj['meter']['stale_calls']}+{rj['meter']['departed_calls']} "
              f"重复={rj['meter']['dup_sends']} 写陈旧={rj['meter']['stale_writes']}+{rj['meter']['departed_writes']} 在途可见={rj['departed_visible']} "
              f"| py 错={bad(rp)} 挂={rp['settle']['hang']} 调用={rp['meter']['calls']} 题={rp['meter']['questions']} "
              f"陈旧调用={rp['meter']['stale_calls']}+{rp['meter']['departed_calls']} 重复={rp['meter']['dup_sends']} "
              f"写陈旧={rp['meter']['stale_writes']}+{rp['meter']['departed_writes']} 在途可见={rp['departed_visible']}", flush=True)
        if not a.skip_s2:
            fail = (40, 41, 42)
            rj2, _ = run1(S.JxSystem, sd, fail)
            rp2, _ = run1(S.PySystem, sd, fail)
            out["S2"].append({"seed": sd, "jpx": rj2, "py": rp2})
            print(f"S2 seed {sd}: jpx 错={bad(rj2)} {rj2['errors']} | py 错={bad(rp2)} {rp2['errors']} 抛出={rp2['settle']['raised']}", flush=True)
        for name, cls in INJECTIONS.items():
            if a.inject and name not in a.inject.split(","):
                continue
            ri, _ = run1(None, sd, net_cls=cls, timeout=a.inject_timeout)
            out["inject"].setdefault(name, []).append(ri)
            print(f"  {name} seed {sd}: 错={ri['errors']} 挂={ri['settle']['hang']} 写陈旧={ri['meter']['stale_writes']}", flush=True)
    if a.out:
        os.makedirs(os.path.dirname(os.path.abspath(a.out)), exist_ok=True)
        json.dump(out, open(a.out, "w"), ensure_ascii=False, indent=1, default=str)
    summarize(out)
    return out


def _dep(r):
    d = r["meter"].get("departed_by", {})
    return f"{d.get('pair', 0) + d.get('whole', 0)}/{d.get('rerank', 0)}/{d.get('rerank-q', 0)}"


def summarize(out):
    """汇总成预注册结果节用的表（每列五个种子，逗号分隔）。"""
    def col(rows, f):
        return ",".join(str(f(r)) for r in rows)
    S1 = out["S1"]
    j, p = [r["jpx"] for r in S1], [r["py"] for r in S1]
    m = lambda k: (lambda r: r["meter"][k])                                    # noqa: E731
    print("\n| 项 | J++x | 基线 |\n|---|---|---|")
    rows = [("终态两边一致", col(S1, lambda r: "是" if r["diff"]["same"] else "否"), ""),
            ("终态错误（边错/漏边/构型无效/构型漏/方案/残留）",
             col(j, lambda r: "/".join(str(v) for v in r["errors"].values())), col(p, lambda r: "/".join(str(v) for v in r["errors"].values()))),
            ("其中漏掉的成立边", col(j, lambda r: r["missing_holding"]), col(p, lambda r: r["missing_holding"])),
            ("挂死", col(j, lambda r: int(r["settle"]["hang"])), col(p, lambda r: int(r["settle"]["hang"]))),
            ("union 单元里的离开者（cand/hits/inbox）", col(j, lambda r: sum(r["union_residue"].values())), col(p, lambda r: sum(r["union_residue"].values()))),
            ("判断调用", col(j, m("calls")), col(p, m("calls"))),
            ("题数（不含伴随题）", col(j, m("questions")), col(p, m("questions"))),
            ("伴随题", col(j, m("companion_q")), col(p, m("companion_q"))),
            ("陈旧调用（旧材料 + 含离开者）", col(j, lambda r: f"{r['meter']['stale_calls']}+{r['meter']['departed_calls']}"),
             col(p, lambda r: f"{r['meter']['stale_calls']}+{r['meter']['departed_calls']}")),
            ("其中含离开者：两两与整体 / 他自己的召回 / 召回题面里的对方",
             col(j, lambda r: _dep(r)), col(p, lambda r: _dep(r))),
            ("重复发送", col(j, m("dup_sends")), col(p, m("dup_sends"))),
            ("写 edge 次数", col(j, m("edge_writes")), col(p, m("edge_writes"))),
            ("写入时陈旧（旧输入 + 写给离开者）", col(j, lambda r: f"{r['meter']['stale_writes']}+{r['meter']['departed_writes']}"),
             col(p, lambda r: f"{r['meter']['stale_writes']}+{r['meter']['departed_writes']}")),
            ("离开者在途可见（逐事件累计）", col(j, lambda r: r["departed_visible"]), col(p, lambda r: r["departed_visible"])),
            ("生成器调用（不同内容）", col(j, lambda r: f"{r['meter']['gen_calls']}({r['meter']['gen_distinct']})"),
             col(p, lambda r: f"{r['meter']['gen_calls']}({r['meter']['gen_distinct']})")),
            ("终态成立边 / 构型 / 方案", col(j, lambda r: f"{r['final']['edges_holding']}/{r['final']['configs']}/{r['final']['plans']}"),
             col(p, lambda r: f"{r['final']['edges_holding']}/{r['final']['configs']}/{r['final']['plans']}"))]
    for name, a, b in rows:
        print(f"| {name} | {a} | {b} |")
    if out["S2"]:
        print("\nS2（第 40–42 次调用失败）")
        for r in out["S2"]:
            print(f"seed {r['seed']}: jpx {r['jpx']['errors']} | py {r['py']['errors']} 抛出={r['py']['settle']['raised']}")
    if out["inject"]:
        print("\n| 注入 | 抓到的种子 | 检测项（各种子） |\n|---|---|---|")
        for name, rs in out["inject"].items():
            hit = sum(1 for r in rs if bad(r) > 0)
            det = ";".join(("挂死" if r["settle"]["hang"] else ",".join(f"{k}={v}" for k, v in r["errors"].items() if v) or "无")
                           for r in rs)
            print(f"| {name} | {hit}/{len(rs)} | {det} |")


def _range(s):
    if "-" in s:
        x, y = s.split("-")
        return list(range(int(x), int(y) + 1))
    return [int(v) for v in s.split(",")]


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--seeds", default="0-4")
    ap.add_argument("--out", default="")
    ap.add_argument("--inject", default="", help="只跑这些注入，逗号分隔；空 = 全部")
    ap.add_argument("--inject-timeout", type=float, default=20.0, help="注入运行的静止上限（I6 预期挂死，不必等满 120 秒）")
    ap.add_argument("--skip-s2", action="store_true")
    ap.add_argument("--from", dest="src", default="", help="只汇总已有的 --out 文件")
    a = ap.parse_args()
    if a.src:
        summarize(json.load(open(a.src)))
    else:
        main_sync(a)


if __name__ == "__main__":
    main()
