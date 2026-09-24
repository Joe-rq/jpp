#!/usr/bin/env python3
"""验收仪表（步 31-0 最小版，31-0b 改口径）：七项一次跑完，结果写 `地基/评估/仪表/<日期>[-<序号>].json` 与同名简表 `.md`。

依据：`地基/附注/2026-09-24-评估①裁定.md` §六·5（七项定义、数据来源、第一版允许）、§六·4（能力对照）、
§四（B77 重放一致）；`地基/评估/2026-09-24-阶段评估-1.md` §五 建议 10 与附录 A。
Nature 2026-09-24：「有了这个测量以后我们就可以根据结果不断地反馈，不断地修正……按真实的倍率或者真实的水平比较。」
所以每项都写「在参考系里的位置」或「对上一次读数的变化」，暂时取不到数的项写「无数」与原因，不留空。

每个里程碑与每个 S 步后运行（`17` 阶段评估条，主会话落）：

    cd 地基/rust-jpp && python3 scripts/dashboard.py        # 先自行 cargo build -p jpp-cli，保证量的是当前源码

只读仓库、只跑固定观察与本地测试，不发真机调用、不花钱。每次运行写一份新读数：当天第一份为 `<日期>`，
之后为 `<日期>-2`、`<日期>-3`…（不覆盖已有读数；`--overwrite` 改写当天最新一份，`--out <路径前缀>` 写到别处、
供确定性核对）。上一次读数取正要写的那份之外最新的一份，逐项给出变化。

31-0b（`地基/附注/2026-09-24-仪表读数1裁定.md` B78、B80）：表达量比主读数为折行归一行数比，
原始行数比、token 比、调用数比并列；删「字面口径一」；夹具脚本不计入口径二。
"""
from __future__ import annotations

import argparse
import datetime
import glob
import json
import os
import pathlib
import re
import subprocess
import sys

from _baseline import ROOT

sys.path.insert(0, str(ROOT / "scripts"))
import measure_expr  # noqa: E402
import replay_scan  # noqa: E402

try:
    import tomllib
except ModuleNotFoundError:
    import tomli as tomllib

OUT = ROOT.parent / "评估" / "仪表"
JPP = ROOT / "target" / "debug" / "jpp"
TRIAL = ROOT.parent / "评估" / "2026-09-24-试写"


def sh(cmd, **kw):
    return subprocess.run(cmd, cwd=kw.pop("cwd", ROOT), capture_output=True, text=True, **kw)


def cargo_test(pkg: str, test: str, filt: str = "") -> dict:
    cmd = ["cargo", "test", "-p", pkg, "--test", test, "--offline", "-q"] + (["--", filt] if filt else [])
    p = sh(cmd)
    m = re.findall(r"test result: (\w+)\. (\d+) passed; (\d+) failed", p.stdout)
    passed = sum(int(x[1]) for x in m)
    failed = sum(int(x[2]) for x in m)
    return {"cmd": " ".join(cmd), "ok": p.returncode == 0 and failed == 0 and passed > 0,
            "passed": passed, "failed": failed, "tail": (p.stdout + p.stderr).strip()[-300:] if p.returncode else ""}


# —— 1 表达量比 ——

def _tier_rows(t: dict) -> dict:
    per = {}
    for r in t["projects"]:
        p = r["project"]
        chk = t["checks"][p]
        fails = [f"{x['file']}：" + "；".join(f"{k} {v}" for k, v in x.get("items", {}).items() if not v.startswith("通过"))
                 if x.get("items") else x["file"] for x in chk["baselines"] + chk["jpp"] if not x["ok"]]
        per[p] = {k: r.get(k) for k in ("wrap100", "caliber1", "caliber2", "token_ratio", "calls", "layout_flag",
                                        "glue_lines", "shared_glue_lines", "coordination_share",
                                        "runtime_share_baseline", "time_ratio", "status")} | {
            "jpp_counted": (r.get("jpp") or {}).get("counted"),
            "jpp_impls": [(x["file"].split("/")[-1], x["counted"]) for x in r.get("jpp_impls", [])],
            "baseline_counted": r.get("baseline_counted_median"),
            "baselines": [(b["file"].split("/")[-1], b["counted"]) for b in r["baselines"]],
            "baseline_minutes": [b["minutes"] for b in r["baselines"]],
            "baseline_all_pass": all(x["ok"] for x in chk["baselines"]) and bool(chk["baselines"]),
            "acceptance_failures": fails}
    return per


def item_expr() -> dict:
    """两档（B79）：T1 是验收 1 的判定档（Nature 2026-09-24 确认），T0 并列报告。两侧都取该档全部实现的中位数。"""
    doc = measure_expr.measure_all(quiet=True)
    out = {"tiers": {}}
    for tier in ("t1", "t0"):
        t = doc[tier]
        out["tiers"][tier] = {"summary": t["summary"], "projects": _tier_rows(t)}
    s1 = doc["t1"]["summary"]
    bad = [p for tier in ("t0", "t1") for p, r in out["tiers"][tier]["projects"].items()
           if r["status"] == "有数" and not r["baseline_all_pass"]]
    if s1.get("status") != "有数":
        return {"status": s1["status"], **out}
    if bad:
        return {"status": "无数：基线验收不一致（" + "、".join(sorted(set(bad))) + "）", **out}
    return {"status": "有数", "value": s1["wrap100_median"], "position": s1["position"], "summary": s1,
            "reference": "T1 折行归一行数比对 9–20×（判定档，Nature 2026-09-24 确认按 T1 判）；T0 对 LMQL 2.7–4.3× 并列",
            "source": "scripts/measure_expr.py（measure_all）；scripts/t1_check.py；probes/*/measure.toml", **out}


# —— 2 真机新题已决出口率 ——

V6 = ROOT.parent / "评估" / "2026-09-24-V6试用线"
# 等级（报告 `exits[].grade`，`20` §3.4 LineGrade 名）→ 仪表的分档
GRADE_BIN = {"Certified": "正式", "Form": "题式", "Trial": "试用", "Class": "借线",
             "Provisional": "其他", "Fixture": "其他", "Cold": "无线"}


def live_runs():
    """真机账本与它的源程序：试写三次（`live-ledger*.jsonl` ↔ 同名目录的 `<目录>.jpp` / `<目录>-flat.jpp`），
    加 V6 目录 `runs.json` 登记的运行。"""
    runs = []
    for f in sorted(glob.glob(str(TRIAL / "*" / "live-ledger*.jsonl"))):
        d = pathlib.Path(f).parent
        src = d / (d.name + ("-flat" if "flat" in pathlib.Path(f).name else "") + ".jpp")
        runs.append((pathlib.Path(f), src))
    reg = V6 / "runs.json"
    if reg.exists():
        for r in json.loads(reg.read_text(encoding="utf-8")):
            runs.append((V6 / r["ledger"], (V6 / r["source"]).resolve()))
    return runs


def item_live_decided() -> dict:
    """只凭账本重放每个真机账本（0 调用，校准记录由账本头 `calib_used` 补回），从重放报告的 `exits`
    （步 20f 逐出口记线等级）数已决出口（act / ignore / pick / at），按等级分。出口不进账本（`20` §3.7(1)），
    所以这里是重放重算，不是读账本字段。"""
    per, exits, decided = {}, 0, 0
    by_grade = {k: 0 for k in ("正式", "题式", "试用", "借线", "其他")}
    for led, src in live_runs():
        out = ROOT / "target" / "_dashboard_live.json"
        p = sh([str(JPP), "run", src.name, "--replay", str(led), "--output", str(out)], cwd=src.parent)
        name = os.path.relpath(led, ROOT.parent)
        if p.returncode != 0 or not out.exists():
            per[name] = {"error": (p.stderr or p.stdout).strip()[-300:]}
            continue
        rep = json.loads(out.read_text(encoding="utf-8"))
        out.unlink()
        ex = rep.get("exits", [])
        d = [e for e in ex if not e["exit"].startswith("unsure")]
        g = {}
        for e in d:
            b = GRADE_BIN[e["grade"]]
            g[b] = g.get(b, 0) + 1
            by_grade[b] += 1
        per[name] = {"source": os.path.relpath(src, ROOT.parent), "exits": len(ex), "decided": len(d),
                     "decided_by_grade": g, "replay_calls": rep["cost"]["calls"],
                     "releases": sum(1 for e in d if e.get("releases"))}
        exits += len(ex)
        decided += len(d)
    if not per:
        return {"status": "无数：没有真机账本", "value": 0.0}
    bad = [k for k, v in per.items() if "error" in v or v["replay_calls"]]
    if bad:
        return {"status": "无数：重放失败或重放发了新调用（" + "、".join(bad) + "）", "runs": per}
    if decided == 0:
        return {"status": "无数：真机出口全部未决（无可用的线）", "value": 0.0, "decided": 0, "exits": exits,
                "by_grade": by_grade, "runs": per}
    return {"status": "有数（真机账本重放）", "value": round(decided / exits, 3), "decided": decided, "exits": exits,
            "by_grade": by_grade, "runs": per,
            "note": "分母含全部 cut 出口；by_grade 只数已决出口。试用线出口可路由、不放行不可逆 do（B72）",
            "source": "真机账本 --replay 的报告 exits（步 20f）"}


# —— 3 深度曲线 ——

def item_depth() -> dict:
    hops, parents = [], 0
    probes = [("examples/iterate.jpp", "examples/fixtures/iterate.json", ROOT),
              ("refund.jpp", "fixture.json", TRIAL / "refund")]
    for src, fx, cwd in probes:
        led = ROOT / "target" / "_dashboard_depth.jsonl"
        sh([str(JPP), "run", src, "--fixtures", fx, "--ledger-out", str(led), "--output", str(led) + ".json"], cwd=cwd)
        if led.exists():
            for x in led.read_text(encoding="utf-8").splitlines()[1:]:
                j = json.loads(x).get("entry", {}).get("Judge")
                if j:
                    hops.append(j.get("hop", 0))
                    parents += bool(j.get("parents"))
            led.unlink()
            os.unlink(str(led) + ".json")
    if hops and max(hops) == 0 and parents == 0:
        return {"status": "无数：17a 未落，账本 parents/hop 恒空", "judge_entries": len(hops),
                "note": "iterate（3 层）与试写 refund（链式 2 层）的固定观察账本里 hop 全为 0、parents 全空",
                "reference": "与 00-目标与动机 §四 误差累积（每单元九成准、十跳全对约 35%）对照，17a 后取数"}
    dist = {h: hops.count(h) for h in sorted(set(hops))}
    return {"status": "有数（固定观察）", "hop_distribution": dist,
            "note": "按值级来源计（B84，步 17c）：经普通值、下标、content() 传递的依赖都算，控制流不算。"
                    "逐跳未决率要等出口进账本；真机曲线步 31"}


# —— 4 换画像通过数 ——

def item_profile_swap() -> dict:
    t = cargo_test("jpp-core", "assumptions")
    src = (ROOT / "crates/jpp-core/tests/assumptions.rs").read_text(encoding="utf-8")
    hs = sorted(set(re.findall(r"\bH([1-8])\b", src)))
    n = len(hs) if t["ok"] else 0
    return {"status": "有数", "value": f"{n}/8", "covered": ["H" + h for h in hs] if t["ok"] else [],
            "second_level": "无数：没有第二个判断器客户端（真机换判断器没有步）", "test": t,
            "note": "tests/profile_swap 目录尚不存在；证据是 crates/jpp-core/tests/assumptions.rs（两份只差 arithmetic_capable 的画像，程序不改）"}


# —— 5 等价写法调用比 ——

def item_equiv() -> dict:
    os.environ.pop("JPP_CI_UPDATE_BASELINE", None)
    import equiv_pairs
    got = {}
    equiv_pairs.report_ratio = lambda name, ratio, detail: got.__setitem__(name.replace("equiv_pairs_", ""), round(ratio, 2))
    equiv_pairs.CLI = [str(JPP)]
    equiv_pairs.main()
    return {"status": "有数", "value": got, "reference": "13b 后失败线 1.5；第一版允许 14 / 2 / 1.4",
            "source": "scripts/equiv_pairs.py（tests/equiv_pairs/manifest.json）"}


# —— 6 能力对照 ——

def item_capabilities(equiv: dict, replay: dict, swap: dict) -> dict:
    with open(ROOT / "probes" / "measure.toml", "rb") as fh:
        caps = tomllib.load(fh)["capabilities"]
    golden = cargo_test("jpp-cli", "golden")
    failopen = cargo_test("jpp-core", "failopen")
    judged = {
        "A3": (golden["ok"] and replay.get("diff_groups") == 0,
               f"golden {'通过' if golden['ok'] else '失败'}；只凭账本重放差异 {replay.get('diff_groups')} 组"),
        "A4": (failopen["ok"], f"failopen {failopen['passed']} 通过 / {failopen['failed']} 失败"),
        # 步 13b（1f51319e）后慢写法也被合并成一次批发，比值 14 → 1；判据随之改为「≤ 13b 失败线 1.5」
        # （31-0b 过程记录 §三：旧判据「> 1」在 13b 后把合并成功判成未通过）
        "A5": (isinstance(equiv.get("value"), dict) and 0 < equiv["value"].get("sieve_batch", 0) <= 1.5,
               f"sieve_batch 慢写法 / 快写法 = {equiv.get('value', {}).get('sieve_batch')}（≤ 1.5：两种写法都合并成一次批发）"),
        "A7": (swap.get("value") == "8/8", f"换画像 {swap.get('value')}（8/8 才算通过）"),
        "B5": (golden["ok"], "金样 error-question-field-typo 报 W-diag-mention-scope 与 E-field"),
    }
    rows, passed = {}, 0
    for k, c in caps.items():
        if c["status"] == "measured" and k in judged:
            ok, ev = judged[k]
            passed += ok
            rows[k] = {"name": c["name"], "result": "通过" if ok else "未通过", "evidence": ev}
        else:
            rows[k] = {"name": c["name"], "result": "未测", "why": c.get("why", "")}
    return {"status": "有数", "value": f"{passed}/12", "items": rows,
            "tests": {"golden": golden, "failopen": failopen},
            "note": "只判 J++ 一侧能否做到；基线侧的实现方式与代价在 probes/measure.toml，步 31 逐项对基线测"}


# —— 7 重放一致 ——

def item_replay(scan: dict) -> dict:
    return {"status": "有数", "value": {"groups": scan["groups"], "w_header_groups": scan["w_header_groups"],
                                        "diff_groups": scan["diff_groups"], "first_run_failures": len(scan["errors"])},
            "reference": "目标 0；第一版允许 11（修复记录 §六，组的构成不同，见 note）",
            "note": "scripts/replay_scan.py 复建修复记录 §六 的扫查；本仓库组的构成：金样 24（去掉预期报错与续跑）、"
                    "探针 4、winnow 与 folio 各三种 scope 校准目录 6、topic-relevance 三种 scope 校准目录 3、续接后账本 1（B83，步 7c），合 35 组；"
                    "比原记录多出 winnow-batched 与 winnow×scope 三组，所以 W-header 组数不与 11 直接相减",
            "w_header": [r["name"] for r in scan["rows"] if r.get("w_header")]}


def reading_key(stem: str):
    """`2026-09-24` → (日期, 1)；`2026-09-24-2` → (日期, 2)。序号按整数比较。"""
    m = re.fullmatch(r"(\d{4}-\d{2}-\d{2})(?:-(\d+))?", stem)
    return (m.group(1), int(m.group(2) or 1)) if m else None


def readings():
    return sorted((reading_key(f.stem), f) for f in OUT.glob("*.json") if reading_key(f.stem))


def target_stem(today: str, overwrite: bool) -> str:
    seqs = [k[1] for k, _ in readings() if k[0] == today]
    if overwrite and seqs:
        n = max(seqs)
    else:
        n = max(seqs) + 1 if seqs else 1
    return today if n == 1 else f"{today}-{n}"


def previous(exclude_stem: str | None = None):
    files = [f for _, f in readings() if f.stem != exclude_stem]
    return json.loads(files[-1].read_text(encoding="utf-8")) if files else None


def headline(k: str, v: dict) -> str:
    if k == "expr" and v.get("status") == "有数" and "tiers" in v:
        def one(s, band):
            calls = " / ".join(f"{p} {r}" for p, r in s["calls_ratio"].items())
            return (f"折行归一中位 {s['wrap100_median']}×（{s['position']}，参考带 {band}×）；原始行数 {s['caliber1_median']}×、"
                    f"token {s['token_ratio_median']}×、口径二 {s['caliber2_median']}×；调用数比 {calls}")
        t1, t0 = v["tiers"]["t1"]["summary"], v["tiers"]["t0"]["summary"]
        return (f"**T1** {one(t1, '9–20')}；**T0** {one(t0, '2.7–4.3')}；{t1['projects']} 个项目，两侧多实现中位")
    if k == "expr" and v.get("status") == "有数":
        s = v["summary"]
        if "wrap100_median" not in s:  # 31-0b 之前的读数格式
            return (f"口径一中位：行数 {s['caliber1_median']}×、token {s['token_ratio_median']}×（{s['position']}，参考带 9–20×；"
                    f"行数对排版敏感，两数并看）；口径二 {s['caliber2_median']}×（参考带 3.5–4.6×）；{s['projects']} 个项目，弱受控")
        calls = " / ".join(f"{p} {r}" for p, r in s["calls_ratio"].items())
        return (f"折行归一中位 {s['wrap100_median']}×（{s['position']}，参考带 9–20×）；并列：原始行数 {s['caliber1_median']}×、"
                f"token {s['token_ratio_median']}×、口径二 {s['caliber2_median']}×（参考带 3.5–4.6×）；"
                f"调用数比 {calls}；{s['projects']} 个项目，T0，弱受控")
    if k == "live_decided":
        return f"{v.get('value', '—')}（{v.get('decided', '—')}/{v.get('exits', '—')}）"
    if k == "depth":
        d = v.get("hop_distribution")
        return ("hop " + "、".join(f"{h}:{n}" for h, n in d.items()) + "") if d else "—"
    if k == "equiv":
        return " / ".join(f"{a} {b}" for a, b in v["value"].items())
    if k == "replay":
        x = v["value"]
        return f"W-header {x['w_header_groups']}/{x['groups']} 组；差异 {x['diff_groups']} 组"
    return str(v.get("value", "—"))


NAMES = {"expr": "表达量比", "live_decided": "真机新题已决出口率", "depth": "深度曲线", "profile_swap": "换画像通过数",
         "equiv": "等价写法调用比", "capabilities": "能力对照", "replay": "重放一致"}


def main():
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("--overwrite", action="store_true", help="改写当天最新一份读数，不新开序号")
    ap.add_argument("--out", help="写到这个路径前缀（不加扩展名），不进 评估/仪表/；供确定性核对")
    a = ap.parse_args()
    # 先按当前源码重建：S 步之后仪表必须量新代码，不量旧二进制
    b = sh(["cargo", "build", "-p", "jpp-cli", "--offline", "-q"])
    if b.returncode != 0 or not JPP.exists():
        sys.exit("cargo build -p jpp-cli 失败：" + b.stderr[-800:])
    today = datetime.date.today().isoformat()
    stem = target_stem(today, a.overwrite)
    prev = previous(exclude_stem=stem if not a.out else None)
    scan = replay_scan.scan()
    equiv = item_equiv()
    swap = item_profile_swap()
    items = {"expr": item_expr(), "live_decided": item_live_decided(), "depth": item_depth(), "profile_swap": swap,
             "equiv": equiv, "capabilities": item_capabilities(equiv, scan, swap), "replay": item_replay(scan)}
    head = sh(["git", "rev-parse", "--short", "HEAD"]).stdout.strip()
    doc = {"date": today, "reading": stem, "time": datetime.datetime.now().astimezone().isoformat(timespec="minutes"), "commit": head,
           "spec": "地基/附注/2026-09-24-评估①裁定.md §六·5", "items": items,
           "has_number": sum(1 for v in items.values() if v["status"].startswith("有数")),
           "previous": prev and {"date": prev["date"], "commit": prev["commit"],
                                 "headlines": {k: headline(k, v) for k, v in prev["items"].items()}}}
    base = pathlib.Path(a.out) if a.out else OUT / stem
    base.parent.mkdir(parents=True, exist_ok=True)
    base.with_suffix(".json").write_text(json.dumps(doc, ensure_ascii=False, indent=1) + "\n", encoding="utf-8")
    lines = [f"# 验收仪表 {stem}", "",
             f"提交 `{head}`；口径 `{doc['spec']}`；由 `地基/rust-jpp/scripts/dashboard.py` 生成，全文见同名 `.json`。"
             f"七项中 {doc['has_number']} 项有数。", "",
             "| 项 | 状态 | 读数 | 上一次 |", "|---|---|---|---|"]
    for k, v in items.items():
        pv = doc["previous"]["headlines"].get(k, "—") if doc["previous"] else "（首次）"
        lines.append(f"| {NAMES[k]} | {v['status']} | {headline(k, v)} | {pv} |")
    e = items["expr"]
    for tier, title in (("t1", "T1 完整职责任务书（判定档，对 9–20×）"), ("t0", "T0 最小任务书（并列，对 LMQL 2.7–4.3×）")):
        tt = e.get("tiers", {}).get(tier)
        if not tt:
            continue
        lines += ["", f"**表达量比逐项目 · {title}**（两侧取该档全部实现的中位数；行数 = 非注释非空、去数据行；"
                  "折行归一 = 100 列、CJK 双宽；口径二 J++ 侧只加生产必需胶水，夹具脚本不计，B78）", "",
                  "| 项目 | J++ 实现（行） | 基线实现（行） | 折行归一 | 原始行数 | token | 调用数比 | 口径二 | 排版差异 |",
                  "|---|---|---|---|---|---|---|---|---|"]
        for p, r in tt["projects"].items():
            c = r["calls"]
            lines.append(f"| {p} | {'、'.join(f'{a} {b}' for a, b in r['jpp_impls'])} | "
                         f"{'、'.join(f'{a} {b}' for a, b in r['baselines'])} | {r['wrap100']} | {r['caliber1']} | "
                         f"{r['token_ratio']} | {c['ratio'] if c else '无数'} | {r['caliber2']} | "
                         f"{'是' if r['layout_flag'] else '否'} |")
        s = tt["summary"]
        notes = []
        if s.get("status") == "有数":
            notes.append(f"中位：折行归一 {s['wrap100_median']}×（{s['position']}），原始行数 {s['caliber1_median']}×，"
                         f"token {s['token_ratio_median']}×，口径二 {s['caliber2_median']}×。")
            if s.get("overturn_b80_2"):
                notes.append("B80 推翻条件 (2)（折行归一与 token 相差 > 2×）：" + "、".join(s["overturn_b80_2"]) + "。")
        fails = [f"{p}：{f}" for p, r in tt["projects"].items() for f in r["acceptance_failures"]]
        if fails:
            notes.append("验收未过的实现（照实报，不从计数里剔除）：" + "；".join(fails) + "。")
        lines += [""] + notes
    c = items["capabilities"]["items"]
    lines += ["", "**能力对照**：" + "；".join(f"{k} {v['name']} {v['result']}" for k, v in c.items())]
    base.with_suffix(".md").write_text("\n".join(lines) + "\n", encoding="utf-8")
    print("\n".join(lines))


if __name__ == "__main__":
    main()
