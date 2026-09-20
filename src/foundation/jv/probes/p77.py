"""#77 生成命令到执行成功（noul + score + 做 + gen）。

gen 用确定性枚举器（§2.4「gen 的零成本实现」）：每个目标 3 条候选命令（1 对 2 错）；do 在沙箱真跑；
题一：退出码 0 且结果符合目标；题二：完成程度（未完成 / 部分 / 完成）。真值由沙箱里的文件状态判定。
"""

from __future__ import annotations

import foundation.jv as jv

from ._common import (Probe, Result, Sample, bare_ask, bare_state, exit_to_result, parse_noul, parse_score, q_noul, q_score, register, run_sh)

# (目标, 前置文件, 候选命令 [正确, 部分, 错误], 判定函数(files, stdout) -> 0/1/2)
_GOALS = [
    ("创建文件 notes.txt，内容为一行 hello", {}, ["printf 'hello\\n' > notes.txt", "touch notes.txt", "echo hello"],
     lambda f, o: 2 if f.get("notes.txt", "").strip() == "hello" else (1 if "notes.txt" in f else 0)),
    ("统计 data.csv 的行数并打印", {"data.csv": "a\nb\nc\nd\n"}, ["wc -l < data.csv", "wc -c < data.csv", "cat data.csv"],
     lambda f, o: 2 if o.strip() == "4" else (1 if o.strip() else 0)),
    ("把 a.txt 复制为 b.txt", {"a.txt": "x1\n"}, ["cp a.txt b.txt", "cp a.txt c.txt", "mv a.txt b.txt"],
     lambda f, o: 2 if f.get("b.txt") == "x1\n" and "a.txt" in f else (1 if "b.txt" in f else 0)),
    ("列出当前目录里所有 .log 文件名", {"x.log": "", "y.log": "", "z.txt": ""}, ["ls *.log", "ls", "ls *.txt"],
     lambda f, o: 2 if set(o.split()) == {"x.log", "y.log"} else (1 if "x.log" in o else 0)),
    ("把 words.txt 按字母排序写入 sorted.txt", {"words.txt": "pear\napple\nfig\n"}, ["sort words.txt > sorted.txt", "sort words.txt", "cat words.txt > sorted.txt"],
     lambda f, o: 2 if f.get("sorted.txt") == "apple\nfig\npear\n" else (1 if "sorted.txt" in f or o.startswith("apple") else 0)),
    ("在 cfg.ini 里把 debug=false 改成 debug=true", {"cfg.ini": "debug=false\nport=1\n"}, ["sed 's/debug=false/debug=true/' cfg.ini > cfg.tmp && mv cfg.tmp cfg.ini", "grep debug cfg.ini", "sed 's/port=1/port=2/' cfg.ini > cfg.tmp && mv cfg.tmp cfg.ini"],
     lambda f, o: 2 if "debug=true" in f.get("cfg.ini", "") else (1 if "debug" in o else 0)),
    ("删除空文件 empty.txt，保留 keep.txt", {"empty.txt": "", "keep.txt": "k\n"}, ["rm empty.txt", "rm keep.txt", "ls"],
     lambda f, o: 2 if "empty.txt" not in f and "keep.txt" in f else (1 if "keep.txt" in f and "empty.txt" in f else 0)),
    ("打印 data.csv 的第一行", {"data.csv": "h1,h2\n1,2\n"}, ["head -n 1 data.csv", "tail -n 1 data.csv", "head -n 1 nothere.csv"],
     lambda f, o: 2 if o.strip() == "h1,h2" else (1 if o.strip() else 0)),
]


def make_samples(n: int = 24, seed: int = 0) -> list[Sample]:
    out = []
    i = 0
    for g, (goal, setup, cands, judge) in enumerate(_GOALS):
        for c in cands:
            if i >= n:
                break
            r = run_sh(c, setup)
            level = judge(r["files"], r["stdout"]) if r["exit"] == 0 else 0
            out.append(Sample(id=f"p77-{i:02d}", truth=level == 2, mats={"goal": goal, "cmd": c, "setup": setup},
                              meta={"level": level, "goal_idx": g}))
            i += 1
    return out


def _enumerator(prompt: str, ctx, n: int, retry_seq: int):
    """确定性枚举器：按目标文本查表（gen 的零成本实现）。"""
    goal = ctx[0].content
    for g, setup, cands, _ in _GOALS:
        if g == goal:
            return cands[:n]
    return []


def _run(cmd, setup):
    r = run_sh(cmd.content, setup.content)
    return {"cmd": r["cmd"], "exit": r["exit"], "stdout": r["stdout"], "stderr": r["stderr"], "files": r["files"]}


沙箱 = jv.Action("sandbox_sh", fn=_run, taint_out="untrusted")

成功 = jv.test("命令退出码为 0 且执行结果符合目标吗？", calib=jv.calib("probe77.成功"))
程度 = jv.measure("从执行结果看，目标完成到什么程度？", scale=("未完成", "部分完成", "完成"), calib=jv.calib("probe77.程度"))


@jv.program(budget=jv.Budget(calls=60, cost=0.05, layers=1))
def 生成执行(goals, setups, keys):
    """keys[(g, j)] = (sid, 成功真值, 程度真值)；枚举出的候选若不在样本里（n 截断）则记账丢弃。"""
    states, order = [], []
    for g, (goal, setup) in enumerate(zip(goals, setups)):
        cmds = jv.gen("生成一条完成目标的 shell 命令", ctx=[goal], n=3, retry_seq=g, generator=_enumerator)
        for j, c in enumerate(cmds):
            states.append(jv.state(on=jv.do(沙箱, c, setup, iter_seq=g * 10 + j), ctx=[goal]))
            order.append((g, j))
    rs = jv.judge(states, 成功, 程度)
    ex1, ex2 = jv.cut([r[0] for r in rs]), jv.cut([r[1] for r in rs])
    r1, r2 = [], []
    for o, e1, e2 in zip(order, ex1, ex2):
        if o in keys:
            sid, t1, t2 = keys[o]
            r1.append(exit_to_result(sid, t1, e1, "noul"))
            r2.append(exit_to_result(sid, t2, e2, "score"))
        else:
            jv.consume([e1, e2], unsure=jv.drop)
    return r1, r2


def _run_builder(samples):
    goals = sorted({s.meta["goal_idx"] for s in samples})
    keys = {(s.meta["goal_idx"], _GOALS[s.meta["goal_idx"]][2].index(s.mats["cmd"])): (s.id, s.truth, s.meta["level"]) for s in samples}
    return 生成执行([jv.lit(_GOALS[g][0]) for g in goals], [jv.lit(_GOALS[g][1]) for g in goals], keys)


def builder(samples, rt):
    return _run_builder(samples)[0]


def builder_score(samples, rt):
    return _run_builder(samples)[1]


def _bare_common(samples, client, tally, which):
    out = []
    for s in samples:
        r = _run(jv.lit(s.mats["cmd"]), jv.lit(s.mats["setup"]))
        a = bare_ask(client, bare_state(on=r, ctx=[s.mats["goal"]]), {"q0": q_noul(成功.text), "q1": q_score(程度.text, 程度.scale)}, tally)
        if which == "noul":
            pred, p = parse_noul(a["q0"])
            out.append(Result(s.id, s.truth, pred, p, "bare"))
        else:
            lvl, p = parse_score(a["q1"])
            out.append(Result(s.id, s.meta["level"], lvl, p, "bare"))
    return out


def bare(samples, client, tally):
    return _bare_common(samples, client, tally, "noul")


def bare_score(samples, client, tally):
    return _bare_common(samples, client, tally, "score")


def baseline(samples):
    """启发式：退出码 0 即成功。"""
    out = []
    for s in samples:
        r = _run(jv.lit(s.mats["cmd"]), jv.lit(s.mats["setup"]))
        out.append(Result(s.id, s.truth, r["exit"] == 0, 1.0 if r["exit"] == 0 else 0.0, "exit0"))
    return out


def baseline_score(samples):
    out = []
    for s in samples:
        r = _run(jv.lit(s.mats["cmd"]), jv.lit(s.mats["setup"]))
        out.append(Result(s.id, s.meta["level"], 2 if r["exit"] == 0 else 0, None, "exit0"))
    return out


register(Probe("p77", "生成命令到执行成功", "noul", make_samples, builder, bare, baseline, note="gen（枚举器）+ do（沙箱 sh）+ noul + score"))
register(Probe("p77s", "生成命令：完成程度（score 变式）", "score", make_samples, builder_score, bare_score, baseline_score))
