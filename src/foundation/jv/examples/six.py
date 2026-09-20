"""v0.1 §6.1 六条程序，原样（只把宿主的 S 函数换成假实现，供 FakeClient 跑通）。

跑：`python -m foundation.jv.examples.six`，打印每条程序的层数与每层题数（§8-10 融合率的第一个数）。
"""

from __future__ import annotations

import json
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))))

import foundation.jv as jv  # noqa: E402

# ---------------------------------------------------------------- 假 S 库（do / transform 用）
_BENCH = {}


def _bench(repo, commit):
    """假基准：第 5 个提交之后变慢。"""
    idx = int(str(commit.content).split("-")[-1])
    return {"commit": commit.content, "ms": 100 + (80 if idx >= 5 else 0)}


def _run(cmd, *rest):
    c = cmd.content if isinstance(cmd, jv.Mat) else cmd
    if "pytest" in c:
        repo = rest[0].content if rest else {}
        fails = repo.get("fails", 0) if isinstance(repo, dict) else 0
        return {"cmd": c, "exit": 0 if fails == 0 else 1, "report": f"{fails} failed" if fails else "全部通过 passed"}
    return {"cmd": c, "exit": 0 if "ls" in c else 1, "output": "README.md 列表" if "ls" in c else "command not found"}


def commits(repo, good, bad):
    return [f"c-{i}" for i in range(int(good.content), int(bad.content) + 1)]


def apply(repo, patch):
    r = dict(repo.content)
    r["fails"] = max(0, r.get("fails", 0) - (1 if "fix" in patch.content else 0))
    return r


def failures(report):
    head = str(report.content.get("report", "0")).split()[0] if isinstance(report.content, dict) else "0"
    return int(head) if head.isdigit() else 0


def _sense(scene):
    return {"scene": scene.content, "step": 0}


def _a11y(obs):
    o = obs.content
    return {"tree": f"桌面: 杯子 距离 {max(0, 3 - o.get('step', 0))}", "held": o.get("step", 0) >= 3}


def _act(action, *rest):
    n = int(action.content.split("-")[1])
    return {"scene": "桌面", "step": n}


def actions(obs):
    step = 3 - int(obs.content["tree"].split()[-1])
    return [f"move-{step + 1}", f"grasp-{step + 1}", f"wait-{step}"]


def ast_functions(repo):
    return [f"def f{i}(x): return x + {i}" for i in range(3)]


bench = jv.register_action("bench", fn=_bench, taint_out="trusted", reason="示例假基准：输出由本文件的确定性函数产生，不含用户文本")
run = jv.Action("run", fn=_run, taint_out="untrusted")
sense = jv.register_action("sense", fn=_sense, taint_out="trusted", reason="示例假传感器：输出由本文件的确定性函数产生")
a11y = jv.Action("a11y", fn=_a11y, taint_out="inherit")
act = jv.register_action("act", fn=_act, taint_out="trusted", reason="示例假执行器：输出由本文件的确定性函数产生")


def fake_generator(prompt, ctx, n, retry_seq):
    if "shell" in prompt:
        return ["ls -la", "rm -rf /", "ls", "cat x"][:n]
    if "修改代码" in prompt:
        return [f"fix-{retry_seq}-{i}" if i == 0 else f"noop-{retry_seq}-{i}" for i in range(n)]
    return [f"docstring 版本 {i}：返回 x 加常数" for i in range(n)]


# ---------------------------------------------------------------- 六条程序（§6.1 原文）
@jv.program(budget=jv.Budget(calls=40, cost=0.02, layers=8, escalate=1))
def 定位回归(repo, good, bad):                                        # 二分：有序列表是宿主的
    慢了 = jv.test("这份基准输出相对基线明显变慢了吗？", calib=jv.calib("perf.慢了"))
    cands = jv.transform(commits, repo, good, bad); base = jv.do(bench, repo, good, iter_seq=0)
    for it in jv.loop(bound=8, variant=jv.decreasing(lambda: len(cands))):
        if len(cands) == 1: break
        mid = cands[(len(cands) - 1) // 2]                                  # 规范原文 len//2 在剩两项且 mid 慢时不缩小（README §4）
        match jv.cut(jv.judge(jv.state(on=jv.do(bench, repo, mid, iter_seq=it.n), ctx=[base]), 慢了)[0]):
            case jv.Act(): cands = cands[: cands.index(mid) + 1]
            case jv.Ignore(): cands = cands[cands.index(mid) + 1 :]
            case jv.Unsure(c): jv.handle(c, then=jv.escalate)
    return cands[0]


@jv.program(budget=jv.Budget(calls=60, cost=0.02, layers=6))
def 生成并执行(目标, cwd):
    成功 = jv.test("退出码为 0 且输出符合目标描述吗？", calib=jv.calib("cmd.成功"))
    史 = []
    for k in range(6):
        cmds = jv.gen("生成一条完成目标的 shell 命令", ctx=[目标, *史], n=4, retry_seq=k)   # 一登记就发
        outs = [jv.do(run, c, cwd, iter_seq=k) for c in cmds]                          # 惰性，同层并发
        exits = jv.cut(jv.judge([jv.state(on=o, ctx=[目标]) for o in outs], 成功))        # 一次刷新：4 状态同层
        if (对 := [c for c, e in zip(cmds, exits) if isinstance(e, jv.Act)]): return 对[0]
        史 += [c for c, e in zip(cmds, exits) if isinstance(e, jv.Ignore)]
        jv.consume(exits, unsure=jv.drop)                                                # J-05：unsure 显式丢弃并记账
    return jv.escalate(史)


@jv.program(budget=jv.Budget(calls=80, cost=0.03, layers=6))
def 生成到全绿(spec, repo):
    最可能 = jv.select("哪个修改最可能让失败用例通过？", calib=jv.calib("fix.最可能"), prior=jv.prior.pass_count)
    全过 = jv.test("测试报告显示全部通过吗？", calib=jv.calib("test.全过"))
    报告 = jv.do(run, jv.lit("pytest -q"), repo, iter_seq=0)
    for it in jv.loop(bound=6, variant=jv.decreasing(lambda: failures(报告))):
        cands = jv.gen("按规格修改代码使失败用例通过", ctx=[spec, 报告], n=4, retry_seq=it.n)
        match jv.cut(jv.judge(jv.state(on=spec, ctx=[报告], over=cands), 最可能)):
            case jv.Pick(k): repo = jv.transform(apply, repo, cands[k]); 报告 = jv.do(run, jv.lit("pytest -q"), repo, iter_seq=it.n + 1)
            case jv.Unsure(c): jv.handle(c, regen=True, then=jv.escalate)
    match jv.cut(jv.judge(jv.state(on=报告), 全过)[0]):
        case jv.Act(): return repo
        case _: return jv.escalate(报告)


@jv.program(budget=jv.Budget(calls=40, cost=0.02, layers=20, escalate=1))
def 取物(目标, 场景):
    到达 = jv.test("目标物已在手中吗？", calib=jv.calib("grasp.到达"))
    下一步 = jv.select("下一步做哪个动作最接近目标？", calib=jv.calib("grasp.下一步"))
    观 = jv.do(a11y, jv.do(sense, 场景, iter_seq=0), iter_seq=0); seen = set()          # 已访集合：宿主 set
    for step in range(1, 21):
        match jv.cut(jv.judge(jv.state(on=观, ctx=[目标]), 到达)[0]):
            case jv.Act(): return 观
            case jv.Unsure(c): jv.handle(c, then=jv.escalate)
        acts = [a for a in jv.transform(actions, 观) if a not in seen]
        match jv.cut(jv.judge(jv.state(on=观, ctx=[目标], over=acts), 下一步)):
            case jv.Pick(k): seen.add(acts[k]); 观 = jv.do(a11y, jv.do(act, acts[k], iter_seq=step), iter_seq=step)
            case jv.Unsure(c): jv.handle(c, then=jv.escalate)
    return jv.escalate(观)


@jv.program(budget=jv.Budget(calls=500, cost=0.05, layers=1, escalate=20))
def 工单转部门(工单流, 部门表):
    去哪 = jv.select("这张工单该转给哪个部门？", calib=jv.calib("ticket.去哪"))          # 静态标签集：over=部门表
    exits = jv.cut(jv.judge([jv.state(on=t, over=部门表) for t in 工单流], 去哪))          # 一层，全部并发
    out = {}
    for t, e in zip(工单流, exits):
        match e:
            case jv.Pick(k): out[t] = 部门表[k]
            case jv.Unsure(c): out[t] = jv.ask(jv.state(on=t, over=部门表), 去哪)        # Pending → 程序挂起，恢复即重放
    jv.on_truth("ticket.去哪", lambda t, dept: dept)                                     # 事后发现转错：真值回填校准集
    return out


@jv.program(budget=jv.Budget(calls=300, cost=0.05, layers=3))
def 写docstring(repo):
    忠实 = jv.select("哪个 docstring 最忠实地描述了这段代码？", calib=jv.calib("doc.忠实"), prior=jv.prior.none)
    通过 = jv.test("测试报告显示全部通过吗？", calib=jv.calib("test.全过"))
    fs = jv.transform(ast_functions, repo)
    过 = jv.cut(jv.judge([jv.state(on=jv.do(run, jv.lit("pytest -q"), repo, f, iter_seq=0)) for f in fs], 通过))   # 一层
    jv.consume(过, unsure=jv.drop)                                                       # 跑不过或拿不准的不改
    out = {}
    for f, e in zip(fs, 过):
        if not isinstance(e, jv.Act): continue
        vers = jv.gen("为这个函数写一句 docstring", ctx=[f], n=3, retry_seq=0)
        match jv.cut(jv.judge(jv.state(on=f, over=vers), 忠实)):
            case jv.Pick(k): out[f] = vers[k]
            case jv.Unsure(c): jv.handle(c, keep=None)
    return out


# ---------------------------------------------------------------- 假读数规则（让六条程序走到有意义的出口）
def fake_rule(text, qid, q):
    ins = q["instructions"]
    if q["type"] == "noul":
        if "变慢" in ins:
            return {"type": "noul", "noul": 0.95 if '"ms": 180' in text else 0.05}
        if "退出码" in ins:
            return {"type": "noul", "noul": 0.95 if '"exit": 0' in text and "README" in text else 0.05}
        if "全部通过" in ins:
            return {"type": "noul", "noul": 0.95 if "passed" in text else 0.05}
        if "手中" in ins:
            return {"type": "noul", "noul": 0.95 if '"held": true' in text else 0.05}
        return {"type": "noul", "noul": 0.5}
    if q["type"] == "choice":
        opts = list(q["criteria"])
        state = json.loads(text) if text.startswith("{") else {}
        over = state.get("over", {})
        # 部门：候选文本出现在工单里；动作：grasp 优先；修改：fix 优先；docstring：版本 0
        for o in sorted(opts):
            v = str(over.get(o, ""))
            if ("grasp" in v) or ("fix" in v) or (v and v in str(state.get("on", ""))) or v.startswith("docstring 版本 0"):
                return {"type": "choice", "choice": o, "probabilities": {k: (0.9 if k == o else 0.1 / max(1, len(opts) - 1)) for k in opts}}
        return {"type": "choice", "choice": opts[0], "probabilities": {k: 1 / len(opts) for k in opts}}
    return None


def calib_all(rt):
    for k in ("perf.慢了", "cmd.成功", "fix.最可能", "test.全过", "grasp.到达", "grasp.下一步", "ticket.去哪", "doc.忠实"):
        rt.calib.put(k, hi=0.6, lo=0.3, n=50, status="上岗", set_id="fake")


def run_all(root: str | None = None, passes: dict | None = None):
    rows = []
    with jv.Runtime(client=jv.FakeClient(rule=fake_rule), root=root, generator=fake_generator, passes=passes) as rt:
        calib_all(rt)
        cases = [
            ("定位回归", lambda: 定位回归(jv.lit({"name": "repo"}), jv.lit("0"), jv.lit("9"))),
            ("生成并执行", lambda: 生成并执行(jv.lit("列出当前目录文件"), jv.lit("/tmp"))),
            ("生成到全绿", lambda: 生成到全绿(jv.lit("规格：函数返回和"), jv.lit({"fails": 2}))),
            ("取物", lambda: 取物(jv.lit("杯子"), jv.lit("桌面"))),
            ("工单转部门", lambda: 工单转部门([jv.lit("发票开错了 财务"), jv.lit("登录不了 技术"), jv.lit("想退货 售后")],
                                        [jv.lit("财务"), jv.lit("技术"), jv.lit("售后")])),
            ("写docstring", lambda: 写docstring(jv.lit({"fails": 0}))),
        ]
        for name, fn in cases:
            try:
                out = fn()
                status = "ok"
            except jv.Pending as e:
                out, status = f"Pending({e.key[:8]})", "pending"
            layers = rt.stats["layers"]
            st = rt.stats_report()
            rows.append({"程序": name, "结果": repr(out)[:60], "状态": status, "层数": len(layers),
                         "每层题数": [l["questions"] for l in layers], "每层调用": [l["calls"] for l in layers],
                         "调用": rt.stats["calls"], "题": rt.stats["questions"], "融合率": st["fusion_rate"],
                         "账本命中": st["ledger_hits"], "钱": st["cost"], "停层": st["stopped_layers"],
                         "警告": rt.stats["warnings"], "plan": rt.stats.get("plan")})
    return rows


def stats_table(rows: list[dict], title: str = "") -> str:
    """`jv stats` 的表（§8-10）。"""
    head = f"| 程序 | 层数 | 每层题数 | 每层调用 | 题 | 调用 | 融合率 | 账本命中 | 停层 | 钱 |"
    lines = [f"### {title}" if title else "", head, "|---|---|---|---|---|---|---|---|---|---|"]
    tq = tc = 0
    for r in rows:
        tq += r["题"]; tc += r["调用"]
        lines.append(f"| {r['程序']} | {r['层数']} | {','.join(map(str, r['每层题数']))} | {','.join(map(str, r['每层调用']))} "
                     f"| {r['题']} | {r['调用']} | {r['融合率']} | {r['账本命中']} | {r['停层']} | {r['钱']:.6f} |")
    lines.append(f"| 合计 | {sum(r['层数'] for r in rows)} | | | {tq} | {tc} | {round(tq / tc, 2) if tc else None} | "
                 f"{sum(r['账本命中'] for r in rows)} | {sum(r['停层'] for r in rows)} | {sum(r['钱'] for r in rows):.6f} |")
    return "\n".join(l for l in lines if l is not None)


PASSES = ("lift", "fuse", "fission", "lower", "schedule", "plan", "ledger", "speculate", "vectorize")


def ablation_table(root: str | None = None) -> str:
    """九个开关逐个关掉，六条示例合计的调用数/层数/题数变化（§4「不做会坏什么」的实测）。ledger 关掉的效果
    要看第二遍：全开时第二遍调用 0（重放），关 ledger 时第二遍照发。"""
    import tempfile
    base = run_all(passes=None)
    out = [f"| 关掉的 pass | 层数 | 题 | 调用 | 融合率 | 停层 | 说明 |", "|---|---|---|---|---|---|---|"]

    def tot(rows):
        q = sum(r["题"] for r in rows); c = sum(r["调用"] for r in rows)
        return sum(r["层数"] for r in rows), q, c, (round(q / c, 2) if c else None), sum(r["停层"] for r in rows)
    L, Q, C, F, S = tot(base)
    out.append(f"| （全开） | {L} | {Q} | {C} | {F} | {S} | 基线 |")
    notes = {"lift": "每个 judge 立即刷新：层数 = 判断数", "fuse": "逐题调用：调用数 = 题数",
             "fission": "超窗对象不切：带偏读数（示例里无超窗对象，故无变化）",
             "lower": "select 一律 K-noul、不置换：题数 ↑ 调用不变", "schedule": "层内串行：调用数不变，只慢",
             "plan": "不核预算：超预算的层照发（停层 0）", "ledger": "见第二遍列",
             "speculate": "刷新点不向前推测同帧的 judge：取物 的「到达」「下一步」被 match 隔成两层（4 → 7 层）",
             "vectorize": "宿主 for 里逐轮 cut 不再合成一层：写docstring 每函数一层（2 → 4 层，第 4 层被预算停）"}
    for p in PASSES:
        rows = run_all(passes={p: False})
        l, q, c, f, s = tot(rows)
        out.append(f"| {p} | {l} | {q} | {c} | {f} | {s} | {notes[p]} |")
    # ledger：第二遍
    with tempfile.TemporaryDirectory() as d:
        run_all(root=d); second = run_all(root=d)
        c2 = sum(r["调用"] for r in second); h2 = sum(r["账本命中"] for r in second)
    with tempfile.TemporaryDirectory() as d:
        run_all(root=d, passes={"ledger": False}); second_nl = run_all(root=d, passes={"ledger": False})
        c2n = sum(r["调用"] for r in second_nl)
    out.append(f"| ledger（第二遍） | | | 全开 {c2} 次（账本命中 {h2}）；关 ledger {c2n} 次 | | | 重放不付费 vs 每次都发 |")
    return "\n".join(out)


if __name__ == "__main__":
    print(stats_table(run_all(), title="六条示例（FakeClient，全开）"))
