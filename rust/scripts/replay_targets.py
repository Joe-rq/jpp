#!/usr/bin/env python3
"""三个靶子的账本重放核对（收尾计划第②件「键与格式稳定」：三个靶子的账本各重放一次相同）。

每个靶子取一份真机（或线 A 仅存的）账本，用 `jpp run --replay <账本>` 零花费重放（不调判断器、不调生成器、不碰后端），
把重放报告与原件报告比：
  核心   status、value、returned_unsure、pending 的未决原因、告警编号集合（W-header 另计）、重放新增调用数（须为 0）；
  投影   两份报告去掉重放固有的易变字段（见 VOLATILE）后逐路径比，残差路径数须为 0。
任何一项不同、重放失败（E-replay 等）、原件找不到，该靶子判「不一致」。原件只读。

靶子（每个靶子的账本、程序、输入来源见 TARGETS）：
  lineA     线 A 求职匹配 Q-07 jobbyjev。阶段 0 与阶段 2 的真机账本在提交 add2fc971 清掉了，入库的只剩 Q-07 本体
            改写时的 8 家固定读数账本 `ledger.json`（账本 v3、站点键、无 lib_version，正好是「新二进制读旧账本」）。
            它写于 d312af00f，之后 prog.jpp 又改过，所以程序按那次提交取（git show），不用工作区的。
  hush     第二靶子 hush（issue 分诊）：3r-live（800 条，最后一份真机账本）。账本头 lib_version 是 bb7344c5，对不上任何
            已提交的库（原跑在已删的 linea-t2rerun worktree），目前没有二进制能原样重放它；基线如实记「失败」。
  botcraft 第三靶子 botcraft：第四圈 4.d-live 的 m0（默认；`--botcraft-matches 0,1,2` 取全三局）。

用法（从 地基/rust-jpp 或任意目录）：
  python3 scripts/replay_targets.py                      # 三靶子各一份，用本检出的 target/debug/jpp
  python3 scripts/replay_targets.py --jpp /path/to/jpp   # 指定二进制
  python3 scripts/replay_targets.py --bin hush=/path/to/era/jpp   # 单个靶子换二进制（对照：账本写成时的二进制）
  程序一律取自二进制所在检出（`jpp` 按程序文件位置找 lib/，二进制与 lib 必须同一检出）；不在检出里的二进制用本检出的程序。
  python3 scripts/replay_targets.py --only lineA,botcraft --json out.json
  python3 scripts/replay_targets.py --selftest           # 只测比较逻辑，不需要二进制与原件

路径可用环境变量覆盖：JPP_REPLAY_LINEA_LEDGER / _HUSH_LEDGER / _BOTCRAFT_RUN（目录，内含 m0/ m1/ m2/）。
退出码：全部一致 0；有不一致、失败或原件缺失 1（`--report-only` 时恒 0）。
"""
import argparse
import hashlib
import json
import os
import pathlib
import re
import subprocess
import sys
import tempfile
import time

ROOT = pathlib.Path(__file__).resolve().parent.parent          # 地基/rust-jpp
REPO = ROOT.parent.parent                                       # 仓库根（可能是 worktree）
REPOS_ROOT = pathlib.Path.home() / "个人项目" / "jev"
CMP = "地基/比赛/改写"

# 重放固有、与「值与账本是否相同」无关的报告字段（路径按 [] 归一化后整体匹配）。
# 每条写明理由；新增条目必须写理由，不许为了让某个靶子过而加。
VOLATILE = {
    r"/cost(/.*)?": "重放不花钱不调用：calls/replayed/tokens/usd 本来就不同",
    r"/mode": "live 或 fixed 与 replay 的描述",
    r"/replay": "报告自己标记是否重放",
    r"/confirm(/.*)?": "确认闸只在真机跑出现",
    r"/cache(/.*)?": "跨运行缓存命中统计，重放不查缓存",
    r"/gen_backend(/.*)?": "重放不装生成后端（--gen-model 与 --replay 互斥）",
    r"/plan/cost_usd(/.*)?": "费用预估依赖画像价格，重放不给价格",
    r"/trace/events\[\]/(replayed|cost)": "逐条事件的重放标记与花费",
    r"/trace/warnings(#len|\[\])": "告警文本另按编号集合比（W-header 另计）",
    r"/window_over/group": "窗口超限的分组数取自真机调用分组，重放没有调用分组",
    r"/fixture_description": "固定读数的说明，重放不带夹具",
    r"/exits\[\]/evidence/certified": "校准来源取自夹具/校准目录，只凭账本重放不带（replay_scan 同口径）",
}

# 靶子清单。run 给 (ledger, input, report, prog)；缺的原件记「原件缺失」。
TARGETS = ["lineA", "hush", "botcraft"]


def sha256(p: pathlib.Path) -> str:
    return hashlib.sha256(p.read_bytes()).hexdigest()


def first_existing(*cands):
    for c in cands:
        if c and pathlib.Path(c).exists():
            return pathlib.Path(c)
    return None


def git_show(rev: str, rel: str, dest: pathlib.Path):
    r = subprocess.run(["git", "-C", str(REPO), "show", f"{rev}:{rel}"], capture_output=True)
    if r.returncode != 0:
        return None
    dest.write_bytes(r.stdout)
    return dest


def repo_of(jpp: pathlib.Path) -> pathlib.Path:
    """二进制所在检出的仓库根。`jpp` 按程序文件的位置找 lib/，所以程序必须取自与二进制同一个检出，
    否则 lib 版本对不上（2026-10-02 对照实测：era 二进制配 main 的程序路径仍报 E-replay）。"""
    parts = jpp.resolve().parts
    if "target" in parts and parts[parts.index("target") - 1] == "rust-jpp":
        return pathlib.Path(*parts[: parts.index("target") - 2])
    return REPO


def resolve(target: str, tmp: pathlib.Path, matches: list, repo: pathlib.Path = None) -> list:
    """返回该靶子的重放任务列表：[{name, prog, ledger, input, report, extra, note, missing}]。"""
    repo = repo or REPO
    out = []
    if target == "lineA":
        d = repo / CMP / "Q-07-jobbyjev"
        led = first_existing(os.environ.get("JPP_REPLAY_LINEA_LEDGER"), d / "ledger.json")
        prog = git_show("d312af00f", f"{CMP}/Q-07-jobbyjev/prog.jpp", tmp / "q07-prog.jpp")
        rep = git_show("d312af00f", f"{CMP}/Q-07-jobbyjev/report.json", tmp / "q07-report.json")
        out.append({"name": "lineA Q-07 ledger.json（8 家固定读数，v3 旧账本）", "prog": prog, "ledger": led,
                    "input": d / "jpp_input.json", "report": rep, "extra": [],
                    "exempt": {"paths": {"/plan": "原件由 d312af00f 当时的二进制写，报告还没有 plan 段（后加的字段）"},
                               "codes": {"W-declared-line": "首跑才发；重放（含再带 --fixtures 重放）不重发，带夹具核对过"}},
                    "note": "程序取 d312af00f（账本写成时）；阶段 2 真机账本已于 add2fc971 清除"})
    elif target == "hush":
        runs = pathlib.Path(os.environ.get("JPP_REPLAY_HUSH_RUN") or REPOS_ROOT.parent / "jev-runs留存/第二靶子-2026-10-02/runs/3r-live")
        h = repo / CMP / "B1-08-hush/第二靶子"
        out.append({"name": "hush 3r-live（800 条真机）", "prog": h / "prog-arm3.jpp",
                    "ledger": first_existing(os.environ.get("JPP_REPLAY_HUSH_LEDGER"), runs / "ledger.jsonl"),
                    "input": runs / "input-arm3.json", "report": runs / "report.json",
                    "extra": ["--profile", str(repo / "地基/rust-jpp/profiles/jev-1.13.0.json"), "--cells", "off"],
                    "prog_sha": (json.loads((runs / "meta.json").read_text())["prog_sha256"] if (runs / "meta.json").exists() else None),
                    "note": "账本头 lib_version bb7344c5 对不上任何已提交的库（12c79d75f 试过，gen 过、judge 仍 E-replay）"})
    elif target == "botcraft":
        b = repo / CMP / "B3-07-botcraft/第三靶子"
        run = first_existing(os.environ.get("JPP_REPLAY_BOTCRAFT_RUN"),
                             REPOS_ROOT.parent / "jev-runs留存/第三靶子-第二四圈-2026-10-02/runs/4.d-live",
                             REPOS_ROOT / ".claude/worktrees/agent-a83b3333e92d3eda2" / CMP / "B3-07-botcraft/第三靶子/runs/4.d-live",
                             REPOS_ROOT.parent / "jev-runs留存/第三靶子-2026-10-02/runs/4.d-live")
        for m in matches:
            md = (run / f"m{m}") if run else None
            out.append({"name": f"botcraft 4.d-live m{m}", "prog": b / "prog-arm3.jpp",
                        "ledger": (md / "ledger.jsonl") if md else None,
                        "input": (md / "input.json") if md else None, "report": (md / "report.json") if md else None,
                        "extra": ["--profile", str(repo / "地基/rust-jpp/profiles/jev-1.13.0.json"), "--cells", "off"],
                        "prog_sha": (json.loads((run / "meta.json").read_text())["prog_sha256"] if run and (run / "meta.json").exists() else None),
                        "note": "账本写成时 lib 为 d071eb2bb，与 main 的 lib/ 与 crates/ 无差别"})
    return out


# —— 比较 ——

def codes(report: dict, skip=()) -> list:
    ws = report.get("trace", {}).get("warnings", []) or []
    cs = {(re.match(r"[A-Z]-[\w-]+", w).group(0) if re.match(r"[A-Z]-[\w-]+", w) else w[:20]) for w in ws}
    return sorted(c for c in cs if c != "W-header" and c not in skip)


def causes(report: dict) -> list:
    return sorted(json.dumps(p.get("cause", p.get("exit")), ensure_ascii=False, sort_keys=True)
                  for p in report.get("pending", []) or [])


def deep_diff(x, y, path, out):
    if type(x) != type(y):
        out.append((path, x, y))
    elif isinstance(x, dict):
        for k in sorted(set(x) | set(y)):
            deep_diff(x.get(k), y.get(k), f"{path}/{k}", out)
    elif isinstance(x, list):
        if len(x) != len(y):
            out.append((path + "#len", len(x), len(y)))
        for i, (u, v) in enumerate(zip(x, y)):
            deep_diff(u, v, f"{path}[{i}]", out)
    elif x != y:
        out.append((path, x, y))


def residual(orig: dict, replay: dict, exempt_paths=None, used=None) -> list:
    """去掉 VOLATILE 与靶子级豁免后的残差。豁免命中的记进 used（输出里列出，不悄悄吞）。"""
    diffs = []
    deep_diff(orig, replay, "", diffs)
    pats = [re.compile(p + r"\Z") for p in VOLATILE]
    ex = [(re.compile(p + r"\Z"), why) for p, why in (exempt_paths or {}).items()]
    res = []
    for p, x, y in diffs:
        norm = re.sub(r"\[\d+\]", "[]", p)
        if any(q.match(norm) for q in pats):
            continue
        hit = next((why for q, why in ex if q.match(norm)), None)
        if hit:
            if used is not None:
                used.add(f"{norm}：{hit}")
            continue
        res.append((norm, x, y))
    return res


def compare(orig: dict, replay: dict, exempt=None) -> dict:
    """value_diff：status、value、returned_unsure、pending 原因、重放新增调用（「值一致」）；
    report_diff：告警编号集合与去易变字段后的逐路径残差（「报告一致」）。exempt 是靶子级豁免，命中的列在 exempted。"""
    exempt = exempt or {}
    used = set()
    v = []
    for k in ("status", "value", "returned_unsure"):
        if orig.get(k) != replay.get(k):
            v.append(k)
    if causes(orig) != causes(replay):
        v.append("pending.cause")
    new_calls = replay.get("cost", {}).get("calls", 0)
    if new_calls:
        v.append(f"重放新增调用 {new_calls}")
    r = []
    skip = exempt.get("codes", {})
    co, cr = codes(orig, skip), codes(replay, skip)
    if co != cr:
        r.append(f"告警编号 {co} → {cr}")
    for c, why in skip.items():
        if (c in codes(orig)) != (c in codes(replay)):
            used.add(f"{c}：{why}")
    res = residual(orig, replay, exempt.get("paths"), used)
    if res:
        r.append(f"投影残差 {len(res)} 处：{', '.join(sorted({p for p, _, _ in res})[:4])}")
    return {"value_diff": v, "report_diff": r, "exempted": sorted(used),
            "samples": [(p, str(x)[:80], str(y)[:80]) for p, x, y in res[:5]]}


def w_header(report: dict) -> list:
    return [w[:140] for w in report.get("trace", {}).get("warnings", []) if w.startswith("W-header")]


def run_one(job: dict, jpp: pathlib.Path, tmp: pathlib.Path) -> dict:
    row = {"target": job["name"], "ledger": str(job["ledger"]) if job["ledger"] else None, "note": job.get("note", "")}
    for k in ("prog", "ledger", "input", "report"):
        if not job[k] or not pathlib.Path(job[k]).exists():
            row.update(status="原件缺失", replay_ok=False, equal=False, diff=[f"缺 {k}：{job[k]}"])
            return row
    if job.get("prog_sha") and sha256(pathlib.Path(job["prog"])) != job["prog_sha"]:
        row["note"] += "；程序 sha 与账本旁 meta 不同"
    rep = tmp / (re.sub(r"\W+", "_", job["name"]) + ".replay.json")
    cmd = [str(jpp), "run", str(job["prog"]), "--input", str(job["input"]), "--replay", str(job["ledger"]),
           "--output", str(rep), *job["extra"]]
    t0 = time.time()
    p = subprocess.run(cmd, capture_output=True, text=True)
    row["elapsed_s"] = round(time.time() - t0, 1)
    if not rep.exists():
        err = [l for l in p.stderr.splitlines() if re.search(r"E-[\w-]+", l)]
        row.update(status="重放失败", replay_ok=False, equal=False, exit=p.returncode,
                   diff=[(err[0] if err else p.stderr.strip()[-200:])[:300]])
        return row
    orig = json.loads(pathlib.Path(job["report"]).read_text(encoding="utf-8"))
    replay = json.loads(rep.read_text(encoding="utf-8"))
    c = compare(orig, replay, job.get("exempt"))
    row.update(replay_ok=True, exit=p.returncode, w_header=w_header(replay), **c)
    row["value_equal"] = not c["value_diff"]
    row["equal"] = not c["value_diff"] and not c["report_diff"]
    row["status"] = "一致" if row["equal"] else "不一致"
    row["diff"] = c["value_diff"] + c["report_diff"]
    return row


def render(rows: list, jpp_desc: str) -> str:
    out = [f"二进制：{jpp_desc}", "", "| 靶子 | 账本 | 重放成功 | 值一致 | 报告一致 | 不一致处 |", "|---|---|---|---|---|---|"]
    for r in rows:
        led = pathlib.Path(r["ledger"]).name if r.get("ledger") else "—"
        ok = "是" if r.get("replay_ok") else "否"
        veq = "是" if r.get("value_equal") else ("—" if not r.get("replay_ok") else "否")
        req = "是" if r.get("equal") else ("—" if not r.get("replay_ok") else "否")
        diff = "；".join(r.get("diff", [])) or "无"
        out.append(f"| {r['target']} | {led} | {ok} | {veq} | {req} | {diff} |")
    return "\n".join(out)


def selftest():
    a = {"status": "ok", "value": 1, "cost": {"calls": 5}, "mode": "live", "replay": False,
         "trace": {"events": [{"replayed": False, "cost": 1.0, "key": "k"}], "warnings": ["W-window: x"]},
         "exits": [{"exit": "act", "evidence": {"certified": {"g": 1}}}]}
    b = json.loads(json.dumps(a))
    b["cost"] = {"calls": 0}
    b["mode"], b["replay"] = "replay", True
    b["trace"]["events"][0].update(replayed=True, cost=0.0)
    b["trace"]["warnings"] = ["W-header: x", "W-window: y"]
    b["exits"][0]["evidence"]["certified"] = None
    c = compare(a, b)
    assert c["value_diff"] == [] and c["report_diff"] == [], c
    b["value"] = 2
    b["exits"][0]["exit"] = "ignore"
    c = compare(a, b)
    assert c["value_diff"] == ["value"] and c["report_diff"] and "/exits[]/exit" in c["report_diff"][0], c
    b["value"] = 1
    b["exits"][0]["exit"] = "act"
    b["cost"]["calls"] = 3
    assert "重放新增调用 3" in compare(a, b)["value_diff"]
    b["cost"]["calls"] = 0
    b["trace"]["warnings"].append("W-foo: z")
    assert compare(a, b)["report_diff"] and not compare(a, b, {"codes": {"W-foo": "t"}})["report_diff"]
    b["extra"] = 1
    c = compare(a, b, {"codes": {"W-foo": "t"}, "paths": {"/extra": "t"}})
    assert c["report_diff"] == [] and any(e.startswith("/extra") for e in c["exempted"]), c
    print("[replay_targets --selftest] 通过")


def main():
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("--jpp", default=str(ROOT / "target/debug/jpp"))
    ap.add_argument("--bin", action="append", default=[], metavar="靶子=路径", help="单个靶子换二进制，可重复")
    ap.add_argument("--only", default=",".join(TARGETS))
    ap.add_argument("--botcraft-matches", default="0")
    ap.add_argument("--json")
    ap.add_argument("--report-only", action="store_true")
    ap.add_argument("--selftest", action="store_true")
    a = ap.parse_args()
    if a.selftest:
        return selftest()
    bins = dict(x.split("=", 1) for x in a.bin)
    tmp = pathlib.Path(tempfile.mkdtemp(prefix="jpp_replay_targets_"))
    rows = []
    for t in a.only.split(","):
        jpp = pathlib.Path(bins.get(t, a.jpp))
        if not jpp.exists():
            sys.exit(f"二进制不存在：{jpp}（先 scripts/cargoq build -p jpp）")
        for job in resolve(t, tmp, [int(x) for x in a.botcraft_matches.split(",")], repo_of(jpp)):
            row = run_one(job, jpp, tmp)
            row["binary"] = str(jpp)
            rows.append(row)
    head = subprocess.run(["git", "-C", str(REPO), "rev-parse", "--short", "HEAD"], capture_output=True, text=True).stdout.strip()
    desc = f"{a.jpp}（源码 HEAD {head}）" + (f"；覆盖 {a.bin}" if a.bin else "")
    print(render(rows, desc))
    for r in rows:
        if r.get("w_header"):
            print(f"\n{r['target']} 的 W-header：" + " / ".join(r["w_header"]))
        for e in r.get("exempted", []):
            print(f"\n{r['target']} 已豁免的差异：{e}")
    if a.json:
        pathlib.Path(a.json).write_text(json.dumps({"head": head, "rows": rows}, ensure_ascii=False, indent=1) + "\n", encoding="utf-8")
    bad = [r for r in rows if not r.get("equal")]
    print(f"\n[replay_targets] {len(rows)} 份账本，一致 {len(rows) - len(bad)}，不一致/失败/缺失 {len(bad)}")
    sys.exit(0 if a.report_only or not bad else 1)


if __name__ == "__main__":
    main()
