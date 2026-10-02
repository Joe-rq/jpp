#!/usr/bin/env python3
"""出题预跑（主控 Z0894，裁定七十第 1 条；推进 B0672）：空跑的放行判据看真机会出的那份题集。

为什么：查题库、闸门第③段、前提探测本身都是判断。空跑用哈希替身时，这些判断的读数是假的，空跑出的题集与真机不同
（第三靶子真机：两臂的另一个谓词空跑有题、真机没成题）。预跑把「出题阶段」放到真机上跑一次、花费封顶，
出题产物（生成、出题阶段的判断）进账本；之后的空跑与真机都带 `--cache <预跑目录>/cache`：
  - 空跑（替身）照预跑的生成走（生成键不含判断器模型），判断走替身；放行时看的是预跑的题集；
  - 真机照预跑的生成与出题阶段的判断走（判断键含判断器模型，预跑与真机同为真机模型，同键命中、花费 0、E4 记来源），
    所以真机的题集与预跑逐个相同。

怎么封顶：用现成的 `--carry-in`（C-3）把这一趟的预算收成 min(程序声明, cap)。预算用完是「停发、不停程序」，
后面的判断记缺席、程序照常结束。上限按入口种类（主会话定）：过程入口（参数里有 `--env`）0.01 美元；批量入口 0.05 美元
（前提全量层属出题阶段，可能比 0.01 多）。预跑花费计入该靶子的总上限，不另开口子。

跑完没有、出题阶段到哪里：见 analyze 的说明。截断不放行（退出码 4）；jpp 报违规照 jpp 的义退出 3，preview.json 记违规条数
与预算越过量。

切缓存：预跑目录下写 `cache/ledger.jsonl`，空跑、真机与 --verify 都带 `--cache <预跑目录>/cache`。范围按入口种类
（preview.json 记 `cache_scope`）：
  - 过程入口只给出题阶段（`plan`）：预跑到封顶为止的逐步判断不进缓存，真机那些步照常花费、照常量时延（第一次快审第 3 条）；
  - 批量入口给整份（`full`）：它会跑到上限才停，边界之后的条目判断也让真机复用，不白花（第二次快审，主会话条件①）；
    这部分照实记在 `after_plan_judge_usd`。库在出题完时留记号让批量入口早停——不做，登后置。

  python3 plan_preview.py --out <预跑目录> [--cap <美元>] [--jpp …] [--model jev-1.13.0] [--gen-model sonnet]
                          [--extract <取返回值的 JSON 路径，缺省 value>] -- <jpp run 的其余参数：程序、--input、--env 等>
  python3 plan_preview.py --out <新目录> --continue-from <截断的预跑目录> -- <同上>
      续跑（主会话 2026-10-02 定）：批量入口出题阶段被截断时，带上一份的 cache/ 续跑一次，已判过的读数全部复用；
      这一趟的上限 = 0.10 − 上一份花费，两份合计不超过 0.10 美元。只续一次（上一份本身是续跑的拒），过程入口不续，
      上一份已经跑完的不续。preview.json 记 continued_from、prev_usd、total_usd。
  python3 plan_preview.py --analyze <预跑目录>     只重读产物、重写 preview.json 与 cache/（判据改了之后用，零花费）
  python3 plan_preview.py --verify <预跑目录> -- <同上>
      复核：带 `--cache <预跑目录>/cache`、预算 0 再跑一趟真机（出题阶段全部命中缓存，之后停发），比对两趟的题哈希逐个相同。
  python3 plan_preview.py --selftest              判据自检（合成的账本与报告，零花费）

凭据：jpp 自己从 ~/.typesafe-key 读；本脚本不读、不写、不传凭据。产物：<预跑目录>/{ledger.jsonl, report.json, carry.json,
preview.json, run.log, cache/ledger.jsonl}。preview.json 是放行要看的那一份：题集（field、题面、题哈希、来源）、花费、
所用上限、是否截断、违规条数、越过量、出题阶段的条目数与判断花费。
"""
from __future__ import annotations

import argparse
import json
import subprocess
import sys
import time
from pathlib import Path

RUST = Path(__file__).resolve().parents[1]


def carry(cost: float) -> dict:
    # C-3 CarryRecord：只收紧花费；调用数、深度给足（程序声明的仍生效，取两者小的）
    return {"calls": 10**9, "cost": cost, "latency_p95": None, "escalate": 10**6, "hop": 0, "round": 0, "depth_cap": 10**6}


def dig(v, path: str):
    for k in path.split("."):
        if k:
            v = v.get(k, {}) if isinstance(v, dict) else {}
    return v


def questions_of(value) -> list[dict]:
    det = value.get("detail", {}) if isinstance(value, dict) else {}
    qs = det.get("questions")
    if qs is None:
        qs = (det.get("plan") or {}).get("questions")
    return [{"field": q.get("field"), "q": q.get("q"), "form": q.get("form"), "from": q.get("from"), "op": q.get("op")}
            for q in (qs or [])]


def entries(ledger: Path) -> tuple[str, list[tuple[str, dict]]]:
    """账本头行与（原文行，条目）列表"""
    lines = ledger.read_text().splitlines()
    out = []
    for l in lines[1:]:
        try:
            out.append((l, json.loads(l)["entry"]))
        except Exception:
            continue
    return (lines[0] if lines else ""), out


def env_step_kind(e: dict) -> str | None:
    """env:step 效应条目：open（开局）/ step（真执行一步）；其余 None。成功与否看输出里有没有 obs"""
    eff = e.get("Effect")
    if not eff:
        return None
    parts = (eff.get("ekey") or {}).get("parts") or []
    if len(parts) < 3 or parts[1] != "env:step":
        return None
    return "open" if '"state":null' in parts[2] else "step"


def run(args, out: Path, cap: float, cache: Path | None) -> dict:
    out.mkdir(parents=True, exist_ok=True)
    (out / "carry.json").write_text(json.dumps(carry(cap)))
    cmd = [args.jpp, "run", *args.rest, "--backend", "live", "--model", args.model,
           "--profile", str(RUST / "profiles" / f"{args.model}.json"),
           "--gen-model", args.gen_model, "--gen-profile", str(RUST / "profiles" / "gen-claude-p.json"),
           "--carry-in", str(out / "carry.json"), "--confirm",
           "--ledger-out", str(out / "ledger.jsonl"), "--output", str(out / "report.json")]
    if cache:
        cmd += ["--cache", str(cache)]
    t0 = time.time()
    with open(out / "run.log", "w") as fh:
        code = subprocess.run(cmd, stdout=fh, stderr=subprocess.STDOUT).returncode
    res = analyze(args, out, cap, code, cmd)
    res["elapsed_s"] = round(time.time() - t0, 1)
    return res


def analyze(args, out: Path, cap: float, code: int, cmd: list) -> dict:
    """读一份预跑目录（账本、报告、日志）：判出题阶段跑完没有、取题集、切出只含出题阶段的账本；不调用任何东西。

    入口种类（任务无关）：报告的 detail 里有 plan 是过程入口（purpose_drive），否则批量入口（purpose_run 一族）。
    跑完的判据（Z0894 快审第 1 条）：
      - 批量入口：任务题（detail.questions 的 form）至少有一次真读数——出完题才对条目问它，预算一停就再没有读数；
      - 过程入口：循环里至少一次 env:step 真执行成功——预算停在出题阶段时，逐步函数照样追加一行，但世界动作执行失败；
      - 两种入口都算跑完的另一支：预算没停（账本里没有 Stop、日志里没有 W-budget）且 jpp 退出 0——出题阶段全拿到了真读数，
        只是任务题没问到（例如全部条目被前提排除）。
    出题阶段的边界（只供切缓存）：第一次任务题判断所在的那一层（Judge.layer）之前；过程入口另取第一次真执行的 env:step 之前，
    两者取早的（第一步被前提排除时没有任务题判断，但世界动作照样执行）。前提的全量层属出题阶段（主会话定），在边界之前。"""
    log = (out / "run.log").read_text()
    if "requires jpp built with --features live" in log:
        sys.exit("jpp 不带 live 特性：先 cargo build -p jpp --features live（跑过测试会重编成不带 live 的）")
    rep = json.loads((out / "report.json").read_text()) if (out / "report.json").exists() else {}
    value = dig(rep, args.extract)
    qs = questions_of(value)
    forms = {q["form"] for q in qs if q["form"]}
    det = value.get("detail", {}) if isinstance(value, dict) else {}
    process = "plan" in det
    head, es = entries(out / "ledger.jsonl") if (out / "ledger.jsonl").exists() else ("", [])
    js = [e["Judge"] for _, e in es if "Judge" in e]

    def task(e):
        return "Judge" in e and (e["Judge"].get("jkey") or {}).get("q") in forms

    task_idx = next((i for i, (_, e) in enumerate(es) if task(e)), None)
    step_idx = next((i for i, (_, e) in enumerate(es) if env_step_kind(e) == "step"), None)
    step_ok = any(env_step_kind(e) == "step" and "obs" in (e["Effect"].get("output") or {}) for _, e in es)
    stopped = any("Stop" in e for _, e in es) or "W-budget" in log
    usd = rep.get("cost", {}).get("usd", 0.0)
    clean = not stopped and code == 0
    complete = (step_ok if process else task_idx is not None) or clean
    cuts = []
    if task_idx is not None:
        layer = es[task_idx][1]["Judge"].get("layer")
        cuts.append(next(i for i, (_, e) in enumerate(es) if "Judge" in e and e["Judge"].get("layer") == layer))
    if process and step_idx is not None:
        cuts.append(step_idx)
    cut = min(cuts) if cuts else len(es)
    cache_dir = out / "cache"
    cache_dir.mkdir(exist_ok=True)
    # 缓存范围按入口种类（主控 Z0894 第二次快审，任务无关）：批量入口给整份预跑账本——它会跑到上限才停，边界之后的
    # 条目判断也要让真机复用（主会话条件①），不复用就是白花；过程入口只给出题阶段——逐步时延要在真机上量
    scope = "plan" if process else "full"
    keep = es[:cut] if process else es
    (cache_dir / "ledger.jsonl").write_text("\n".join([head] + [l for l, _ in keep]) + "\n")
    return {"exit_code": code, "entry": "process" if process else "batch", "usd": usd, "cap": cap,
            "overshoot_usd": round(max(0.0, usd - cap), 6), "budget_stopped": stopped,
            "violations": len(rep.get("violations") or []), "plan_complete": complete,
            "task_forms_asked": sorted({e["Judge"]["jkey"]["q"] for _, e in es if task(e)}),
            "env_step_ok": step_ok, "questions": qs, "judge_entries": len(js),
            "judge_reused": sum(1 for j in js if j.get("reused_from")),
            "plan_cache": str(cache_dir), "cache_scope": scope, "cache_entries": len(keep), "plan_entries": cut,
            "plan_judge_usd": round(sum(e["Judge"].get("cost", 0.0) for _, e in es[:cut] if "Judge" in e), 6),
            "after_plan_judge_usd": round(sum(e["Judge"].get("cost", 0.0) for _, e in es[cut:] if "Judge" in e), 6),
            "cmd": cmd}


CONTINUE_TOTAL = 0.10  # 批量入口续跑一次后的合计硬上限（主会话 2026-10-02）


def continuation(prev: Path, rest: list) -> dict:
    """续跑的判定（不调用任何东西）：返回 {cap, cache, prev_usd}；不许续的 SystemExit 说明原因"""
    if "--env" in rest:
        sys.exit("过程入口不续跑：上限 0.01 的出题阶段截断，照截断处理")
    pj = prev / "preview.json"
    if not pj.exists():
        sys.exit(f"{prev} 没有 preview.json")
    old = json.loads(pj.read_text())
    if old.get("continued_from"):
        sys.exit(f"{prev} 本身是续跑，只许续一次")
    if old.get("plan_complete"):
        sys.exit(f"{prev} 的出题阶段已经跑完，不用续")
    if old.get("entry") != "batch":
        sys.exit("只有批量入口能续跑")
    prev_usd = float(old.get("usd") or 0.0)
    cap = round(CONTINUE_TOTAL - prev_usd, 6)
    if cap <= 0:
        sys.exit(f"上一份已花 {prev_usd}，合计上限 {CONTINUE_TOTAL} 没有余量")
    if not (prev / "cache" / "ledger.jsonl").exists():
        sys.exit(f"{prev}/cache 没有账本")
    return {"cap": cap, "cache": prev / "cache", "prev_usd": prev_usd}


def exit_code(res: dict) -> int:
    """0 跑完；3 jpp 报了违规（照 jpp 的义，不吞）；4 出题阶段被截断；1 jpp 别的失败"""
    if res["exit_code"] == 3:
        return 3
    if res["exit_code"] != 0:
        return 1
    return 0 if res["plan_complete"] else 4


def selftest():
    """五种情形：批量入口任务题问到了（完成）；过程入口一步世界动作执行成功、任务题没问到（完成）；
    过程入口预算停在出题阶段、逐步函数追加了一行但世界动作失败（截断，快审的零花费探针）；
    批量入口预算停了、任务题没问到（截断）；预算没停、退出 0、任务题没问到（完成：出题阶段全是真读数）"""
    import tempfile

    class A:
        extract = "value"

    def mk(d: Path, value: dict, es: list, usd: float, log: str = ""):
        d.mkdir(parents=True, exist_ok=True)
        (d / "run.log").write_text(log)
        (d / "report.json").write_text(json.dumps({"value": value, "cost": {"usd": usd}}))
        lines = [json.dumps({"version": 5})] + [json.dumps({"entry": e}) for e in es]
        (d / "ledger.jsonl").write_text("\n".join(lines) + "\n")

    def J(q, layer):
        return {"Judge": {"jkey": {"q": q}, "layer": layer, "cost": 0.001}}

    def env(state_null, ok):
        return {"Effect": {"kind": "do", "ekey": {"parts": ["0", "env:step", '{"state":null}' if state_null else '{"state":{}}', "0"]},
                           "output": {"obs": {}} if ok else {"fail": "x"}}}

    stop = {"Stop": {}}
    bq = {"detail": {"questions": [{"field": "f", "form": "Q1"}]}}
    pq = {"value": {"rows": [{"by": "excluded"}]}, "detail": {"plan": {"questions": [{"field": "act", "form": "Q2"}]}}}
    with tempfile.TemporaryDirectory() as t:
        t = Path(t)
        mk(t / "batch", bq, [J("P1", 1), J("Q1", 2), J("Q1", 2), stop], 0.05, "W-budget")
        mk(t / "drive", pq, [env(True, True), J("P1", 1), env(False, True), J("P1", 3), stop], 0.01, "W-budget")
        mk(t / "probe", pq, [env(True, True), J("P1", 1), stop, env(False, False)], 0.0001, "W-budget")
        mk(t / "cut", bq, [J("P1", 1), stop], 0.05, "W-budget")
        mk(t / "clean", bq, [J("P1", 1)], 0.002, "")
        got = {k: analyze(A, t / k, 0.05, 0, []) for k in ("batch", "drive", "probe", "cut", "clean")}
        flags = {k: v["plan_complete"] for k, v in got.items()}
        assert list(flags.values()) == [True, True, False, False, True], flags
        assert got["batch"]["plan_entries"] == 1 and got["drive"]["plan_entries"] == 2, (got["batch"]["plan_entries"], got["drive"]["plan_entries"])
        assert got["batch"]["entry"] == "batch" and got["drive"]["entry"] == "process"
        assert exit_code(got["probe"]) == 4 and exit_code(dict(got["batch"], exit_code=3)) == 3
        # 缓存范围：批量入口整份（4 条全进），过程入口只到出题阶段边界（2 条）
        assert (got["batch"]["cache_scope"], got["batch"]["cache_entries"]) == ("full", 4), got["batch"]["cache_entries"]
        assert (got["drive"]["cache_scope"], got["drive"]["cache_entries"]) == ("plan", 2), got["drive"]["cache_entries"]
        # 续跑：截断的批量入口可续，上限 = 0.10 − 上一份；续跑的不再续；跑完的、过程入口、没余量的都拒
        for k, v in got.items():
            (t / k / "preview.json").write_text(json.dumps(v))
        c = continuation(t / "cut", [])
        assert abs(c["cap"] - 0.05) < 1e-9 and c["prev_usd"] == 0.05, c
        for bad, rest in (("batch", []), ("clean", []), ("cut", ["--env", "x"])):
            try:
                continuation(t / bad, rest)
                raise AssertionError(f"{bad} 应拒")
            except SystemExit:
                pass
        (t / "cut2").mkdir()
        (t / "cut2" / "preview.json").write_text(json.dumps(dict(got["cut"], continued_from=str(t / "cut"))))
        (t / "cut3").mkdir()
        (t / "cut3" / "preview.json").write_text(json.dumps(dict(got["cut"], usd=0.1)))
        for bad in ("cut2", "cut3"):
            try:
                continuation(t / bad, [])
                raise AssertionError(f"{bad} 应拒")
            except SystemExit:
                pass
    print("selftest ok", flags)


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--out")
    ap.add_argument("--verify")
    ap.add_argument("--selftest", action="store_true", help="判据自检（合成的账本与报告，零花费）")
    ap.add_argument("--analyze", help="只重读一份预跑目录、重写 preview.json 与 cache/（不调用任何东西）")
    ap.add_argument("--continue-from", help="续跑：带这份截断的批量入口预跑的 cache/，两份合计 ≤ 0.10 美元（主会话定）")
    ap.add_argument("--cap", type=float, help="花费上限；缺省按入口种类：过程入口（参数里有 --env）0.01，批量入口 0.05（主会话定）")
    ap.add_argument("--jpp", default=str(RUST / "target" / "debug" / "jpp"))
    ap.add_argument("--model", default="jev-1.13.0")
    ap.add_argument("--gen-model", default="sonnet")
    ap.add_argument("--extract", default="value")
    ap.add_argument("rest", nargs=argparse.REMAINDER)
    a = ap.parse_args()
    a.rest = [x for x in a.rest if x != "--"]
    if a.selftest:
        selftest()
        return
    if a.analyze:
        d = Path(a.analyze).resolve()
        old = json.loads((d / "preview.json").read_text())
        res = analyze(a, d, old["cap"], old["exit_code"], old["cmd"])
        res["elapsed_s"] = old.get("elapsed_s")
        res["cap_rule"] = old.get("cap_rule")
        for k in ("continued_from", "prev_usd", "total_usd"):
            if k in old:
                res[k] = old[k]
        (d / "preview.json").write_text(json.dumps(res, ensure_ascii=False, indent=1))
        print(json.dumps({k: v for k, v in res.items() if k != "cmd"}, ensure_ascii=False, indent=1))
        sys.exit(exit_code(res))
    if a.verify:
        src = Path(a.verify).resolve()
        pre = json.loads((src / "preview.json").read_text())
        res = run(a, src / "verify", 0.0, src / "cache")
        same = [q["form"] for q in res["questions"]] == [q["form"] for q in pre["questions"]]
        res["same_forms_as_preview"] = same
        res["preview_forms"] = [q["form"] for q in pre["questions"]]
        # 预算 0 时运行时在发出第一组新调用后才停（「最后一次请求可能越过」，见 --help 的 budget 说明），所以花费不是严格的 0；
        # 出题阶段的判断全部命中缓存（judge_reused），越过的只是循环里的第一组
        (src / "verify" / "verify.json").write_text(json.dumps(res, ensure_ascii=False, indent=1))
        print(json.dumps({k: v for k, v in res.items() if k != "cmd"}, ensure_ascii=False, indent=1))
        sys.exit(0 if same and res["usd"] <= 0.001 else 1)
    out = Path(a.out).resolve()
    if a.continue_from:
        prev = Path(a.continue_from).resolve()
        c = continuation(prev, a.rest)
        if a.cap is not None and a.cap > c["cap"]:
            sys.exit(f"--cap {a.cap} 超过续跑余量 {c['cap']}（合计 ≤ {CONTINUE_TOTAL}）")
        cap = a.cap if a.cap is not None else c["cap"]
        res = run(a, out, cap, c["cache"])
        res["cap_rule"] = f"continue:{CONTINUE_TOTAL}"
        res["continued_from"] = str(prev)
        res["prev_usd"] = c["prev_usd"]
        res["total_usd"] = round(c["prev_usd"] + res["usd"], 6)
    else:
        cap = a.cap if a.cap is not None else (0.01 if "--env" in a.rest else 0.05)
        res = run(a, out, cap, None)
        res["cap_rule"] = "given" if a.cap is not None else ("process:0.01" if "--env" in a.rest else "batch:0.05")
    (out / "preview.json").write_text(json.dumps(res, ensure_ascii=False, indent=1))
    print(json.dumps({k: v for k, v in res.items() if k != "cmd"}, ensure_ascii=False, indent=1))
    sys.exit(exit_code(res))


if __name__ == "__main__":
    main()
