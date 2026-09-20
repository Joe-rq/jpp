"""建造第 4 步（12 §9-4）：21 条程序用构建器写出。

来源：`11-语言规范-v1.md` §9 的 18 条（§9.1–§9.15 十五条探针 + §9.16 G-a/b/c 三条），
加 `设计/G3-零上下文读者第三次.md` 的三条（找矛盾论文对、工单路由、写 docstring）。
每条函数 docstring 第一行写对应编号。假 S 库与假读数规则只为让 FakeClient 跑通，不是目标任务（宪法附则一）。

跑：`python -m foundation.jv.examples.twentyone`，打印每条程序的层数、每层题数、每层调用（§8-10）。
"""

from __future__ import annotations

import json
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))))

import foundation.jv as jv  # noqa: E402


# ================================================================ 假 S 库（transform 用：纯函数，收 Mat 返 python 值）
def _c(m):
    """取材料内容；期物读 .content 即刷新点（§6.0）。非材料原样返回。"""
    return m.content if hasattr(m, "content") else m


def hunks(diff):
    return [h.strip() for h in _c(diff).split("\n@@") if h.strip()]


def log_stage(log):
    """把日志切成行；只留像错误/异常的行。"""
    text = _c(log)
    text = text.get("log", "") if isinstance(text, dict) else text
    return [ln.strip() for ln in text.splitlines() if ln.strip()]


def split(m, n):
    """按 n 字切段。"""
    s = _c(m)
    n = int(_c(n))
    return [s[i:i + n] for i in range(0, len(s), n)] or [s]


def rows_of_table(*runs):
    """多次 pytest json 输出 → 每用例一行：{"test": 名, "results": [...]}。"""
    by = {}
    for r in runs:
        for name, res in _c(r)["tests"].items():
            by.setdefault(name, []).append(res)
    return [{"test": k, "results": v} for k, v in by.items()]


def columns(tbl):
    t = _c(tbl)
    return [{"name": h, "values": [row[i] for row in t["rows"]]} for i, h in enumerate(t["header"])]


def sample(col, n):
    return {"values": _c(col)["values"][: int(_c(n))]}


def header(col):
    return {"header": _c(col)["name"]}


def commits(repo, good, bad):
    return [f"c-{i}" for i in range(int(_c(good)), int(_c(bad)) + 1)]


def apply(repo, patch):
    r = dict(_c(repo))
    r["fails"] = max(0, r.get("fails", 0) - (1 if "fix" in _c(patch) else 0))
    return r


def failures(report):
    c = _c(report)
    head = str(c.get("report", "0")).split()[0] if isinstance(c, dict) else "0"
    return int(head) if head.isdigit() else 0


def ast_functions(repo):
    return [f"def f{i}(x): return x + {i}" for i in range(3)]


def ast_functions_doc(repo):
    return _c(repo)["functions"]                      # [{"name", "sig", "doc"}]


def signature(f):
    return {"sig": _c(f)["sig"]}


def docstring(f):
    return {"doc": _c(f)["doc"]}


def diff_reports(前, 后):
    a, b = _c(前), _c(后)
    return {"新失败": sorted(set(b["failed"]) - set(a["failed"]))}


def tag(m, label):
    return {"tag": _c(label), "body": _c(m)}


def diff_cfg(a, b):
    da, db = _c(a), _c(b)
    return [{"key": k, "a": da.get(k), "b": db.get(k)} for k in sorted(set(da) | set(db)) if da.get(k) != db.get(k)]


def doc_of(d):
    return {"doc": _DOCS.get(_c(d)["key"], "无文档")}


_DOCS = {"port": "服务监听端口；改动需重启且影响可达性", "comment": "仅注释", "timeout": "默认超时，有回退值", "path": "数据目录路径"}


def actions(obs):
    step = 3 - int(_c(obs)["tree"].split()[-1])
    return [f"move-{step + 1}", f"grasp-{step + 1}", f"wait-{step}"]


def pairs_with(n, *items):
    """前 n 个是日志段，其余是规则；配对（transform 的参数只能是 Mat 或纯 JSON 值，列表要摊开传）。"""
    segs, rules = [_c(x) for x in items[:int(n)]], [_c(x) for x in items[int(n):]]
    return [[s, r] for s in segs for r in rules]


def pairs_self(*items):
    xs = [_c(x) for x in items]
    return [[xs[i], xs[j]] for i in range(len(xs)) for j in range(i + 1, len(xs))]


def solve(约束, *菜谱):
    names = [_c(m)["name"] for m in 菜谱]
    return {"排程": [f"{i * 10:02d}:00 {n}" for i, n in enumerate(names)], "说明": f"满足：{_c(约束)}"}


def attach(r, reason):
    return {"简历": _c(r), "理由": _c(reason)}


# ================================================================ 假 S 库（do 用：会触世界的动作）
def _run_make(repo, diff):
    return {"log": "error: foo.c:3: 'x' undeclared\nerror: bar.c:9: too few arguments to 'print_all'"}


def _run(cmd, *rest):
    c = _c(cmd)
    if "pytest" in c:
        repo = _c(rest[0]) if rest else {}
        fails = repo.get("fails", 0) if isinstance(repo, dict) else 0
        return {"cmd": c, "exit": 0 if fails == 0 else 1, "report": f"{fails} failed" if fails else "全部通过 passed"}
    return {"cmd": c, "exit": 0 if "ls" in c else 1, "output": "README.md 列表" if "ls" in c else "command not found"}


def _run_pytest_json(repo, i):
    n = int(_c(i))
    return {"tests": {"test_a": "pass", "test_b": "fail" if n % 2 else "pass", "test_c": "pass"}}


def _run_pytest(repo):
    r = _c(repo)
    return {"failed": sorted(r.get("failing", [])), "passed": 10 - len(r.get("failing", []))}


def _upgrade(repo, dep, ver):
    r = dict(_c(repo))
    r["failing"] = sorted(set(r.get("failing", [])) | {"test_dep_api"})
    return r


def _bench(repo, commit):
    idx = int(str(_c(commit)).split("-")[-1])
    return {"commit": _c(commit), "ms": 100 + (80 if idx >= 5 else 0)}


def _sense(scene):
    return {"scene": _c(scene), "step": 0}


def _a11y(obs):
    o = _c(obs)
    return {"tree": f"桌面: 杯子 距离 {max(0, 3 - o.get('step', 0))}", "held": o.get("step", 0) >= 3}


def _act(action, *rest):
    n = int(_c(action).split("-")[1])
    return {"scene": "桌面", "step": n}


def _write_reason(简历, 岗位):
    return f"理由：{_c(简历)['name']} 与岗位「{_c(岗位)}」的技能和年限相符"


run_make = jv.Action("run_make", fn=_run_make, taint_out="untrusted")
run = jv.Action("run", fn=_run, taint_out="untrusted")
run_pytest_json = jv.register_action("run_pytest_json", fn=_run_pytest_json, taint_out="trusted", reason="示例假测试框架：报告由本文件的确定性函数产生，不含被测代码的文本")
run_pytest = jv.register_action("run_pytest", fn=_run_pytest, taint_out="trusted", reason="示例假测试框架：报告由本文件的确定性函数产生，不含被测代码的文本")
upgrade = jv.Action("upgrade", fn=_upgrade, taint_out="inherit")
bench = jv.register_action("bench", fn=_bench, taint_out="trusted", reason="示例假基准：输出由本文件的确定性函数产生，不含用户文本")
sense = jv.register_action("sense", fn=_sense, taint_out="trusted", reason="示例假传感器：输出由本文件的确定性函数产生")
a11y = jv.Action("a11y", fn=_a11y, taint_out="inherit")
act = jv.register_action("act", fn=_act, taint_out="trusted", reason="示例假执行器：输出由本文件的确定性函数产生")
写理由 = jv.Action("写理由", fn=_write_reason, taint_out="untrusted")


def fake_generator(prompt, ctx, n, retry_seq):
    if "shell" in prompt:
        return ["ls -la", "rm -rf /", "ls", "cat x"][:n]
    if "修改代码" in prompt:
        return [f"fix-{retry_seq}-{i}" if i == 0 else f"noop-{retry_seq}-{i}" for i in range(n)]
    return [f"docstring 版本 {i}：返回 x 加常数" for i in range(n)]


# ================================================================ 21 条程序
# ---------------------------------------------------------------- §9.1 E#13
@jv.program(budget=jv.Budget(calls=300, cost=0.05, layers=3))
def 归因编译错误(repo, diff):
    """11 §9.1 E#13 编译错误归因到修改行。"""
    哪块 = jv.select("这条编译错误由哪一块修改引起？", calib=jv.calib("build.哪块"))
    块 = jv.transform(hunks, diff)
    错 = jv.transform(log_stage, jv.do(run_make, repo, diff, iter_seq=0))
    exits = jv.cut(jv.judge([jv.state(on=e, over=块) for e in 错], 哪块))
    out = {}
    for e, x in zip(错, exits):
        match x:
            case jv.Pick(k): out[e.content] = 块[k].content
            case jv.Unsure(c): out[e.content] = jv.handle(c, keep=None)
    return out


# ---------------------------------------------------------------- §9.2 E#17
@jv.program(budget=jv.Budget(calls=100, cost=0.01, layers=1))
def 提交信息一致(msg, diff):
    """11 §9.2 E#17 提交信息与 diff 一致。"""
    出现 = jv.test("这句描述的改动在 diff 里出现了吗？", calib=jv.calib("commit.出现"))
    块 = jv.transform(hunks, diff); 句 = jv.transform(split, msg, 80)
    exits = jv.cut(jv.judge([jv.state(on=s, ctx=块) for s in 句], 出现))
    未见 = [s.content for s, e in zip(句, exits) if isinstance(e, jv.Ignore)]
    疑 = [s.content for s, e in zip(句, exits) if isinstance(e, jv.Unsure)]
    jv.consume(exits, unsure=jv.drop)
    return {"未见": 未见, "unsure": 疑}


# ---------------------------------------------------------------- §9.3 E#22
@jv.program(budget=jv.Budget(calls=200, cost=0.02, layers=2, escalate=20))
def 找flaky(repo, n):
    """11 §9.3 E#22 识别 flaky 测试。"""
    不一致 = jv.test("同一用例在多次重跑里结果不一致吗？", calib=jv.calib("flaky.不一致"))
    结果 = [jv.do(run_pytest_json, repo, jv.lit(i), iter_seq=i) for i in range(n)]      # 同层并发
    rows = jv.transform(rows_of_table, *结果)
    exits = jv.cut(jv.judge([jv.state(on=r) for r in rows], 不一致))
    flaky = [r.content["test"] for r, e in zip(rows, exits) if isinstance(e, jv.Act)]
    疑 = [r.content["test"] for r, e in zip(rows, exits) if isinstance(e, jv.Unsure)]
    jv.consume(exits, unsure=jv.drop)
    if 疑: jv.escalate(疑, note="flaky 拿不准")
    return flaky


# ---------------------------------------------------------------- §9.4 E#42
@jv.program(budget=jv.Budget(calls=500, cost=0.05, layers=1))
def 分类日志(log):
    """11 §9.4 E#42 日志异常行分类（静态标签集 over）。"""
    类别 = jv.select("这行属于哪一类？", calib=jv.calib("log.类别"))
    标签 = [jv.lit("超时"), jv.lit("拒绝"), jv.lit("崩溃"), jv.lit("正常")]
    rows = jv.transform(log_stage, log)
    exits = jv.cut(jv.judge([jv.state(on=r, over=标签) for r in rows], 类别))
    out = {}
    for r, e in zip(rows, exits):
        match e:
            case jv.Pick(k): out[r.content] = 标签[k].content
            case jv.Unsure(c): out[r.content] = jv.handle(c, keep=None)
    return out


# ---------------------------------------------------------------- §9.5 E#45（= 12 §6.1 第一条）
@jv.program(budget=jv.Budget(calls=40, cost=0.02, layers=8, escalate=1))
def 定位回归(repo, good, bad):
    """11 §9.5 E#45 性能回归定位提交（12 §6.1 原文）。"""
    慢了 = jv.test("这份基准输出相对基线明显变慢了吗？", calib=jv.calib("perf.慢了"))
    cands = jv.transform(commits, repo, good, bad); base = jv.do(bench, repo, good, iter_seq=0)
    for it in jv.loop(bound=8, variant=jv.decreasing(lambda: len(cands))):
        if len(cands) == 1: break
        mid = cands[len(cands) // 2]
        match jv.cut(jv.judge(jv.state(on=jv.do(bench, repo, mid, iter_seq=it.n), ctx=[base]), 慢了)[0]):
            case jv.Act(): cands = cands[: cands.index(mid) + 1]
            case jv.Ignore(): cands = cands[cands.index(mid) + 1 :]
            case jv.Unsure(c): jv.handle(c, then=jv.escalate)
    return cands[0]


# ---------------------------------------------------------------- §9.6 E#49
@jv.program(budget=jv.Budget(calls=100, cost=0.01, layers=2))
def 推断列类型(tbl):
    """11 §9.6 E#49 列类型推断。"""
    类型 = jv.select("这列的样例值属于哪一类？", calib=jv.calib("col.类型"))
    标签 = [jv.lit("日期"), jv.lit("金额"), jv.lit("ID"), jv.lit("自由文本")]
    cols = jv.transform(columns, tbl)
    states = [jv.state(on=jv.transform(sample, c, 20), ctx=[jv.transform(header, c)], over=标签) for c in cols]
    exits = jv.cut(jv.judge(states, 类型))
    out = {}
    for c, e in zip(cols, exits):
        match e:
            case jv.Pick(k): out[c.content["name"]] = 标签[k].content
            case jv.Unsure(cause): out[c.content["name"]] = jv.handle(cause, keep=None)
    return out


# ---------------------------------------------------------------- §9.7 E#77（= 12 §6.1 第二条）
@jv.program(budget=jv.Budget(calls=60, cost=0.02, layers=6))
def 生成并执行(目标, cwd):
    """11 §9.7 E#77 生成命令到执行成功（12 §6.1 原文）。"""
    成功 = jv.test("退出码为 0 且输出符合目标描述吗？", calib=jv.calib("cmd.成功"))
    史 = []
    for k in range(6):
        cmds = jv.gen("生成一条完成目标的 shell 命令", ctx=[目标, *史], n=4, retry_seq=k)
        outs = [jv.do(run, c, cwd, iter_seq=k) for c in cmds]
        exits = jv.cut(jv.judge([jv.state(on=o, ctx=[目标]) for o in outs], 成功))
        if (对 := [c for c, e in zip(cmds, exits) if isinstance(e, jv.Act)]): return 对[0]
        史 += [c for c, e in zip(cmds, exits) if isinstance(e, jv.Ignore)]
        jv.consume(exits, unsure=jv.drop)
    return jv.escalate(史)


# ---------------------------------------------------------------- §9.8 E#81
@jv.program(budget=jv.Budget(calls=300, cost=0.03, layers=1))
def 文档签名不一致(repo):
    """11 §9.8 E#81 文档与函数签名不一致。"""
    不同 = jv.test("文档写的参数与签名不同吗？", calib=jv.calib("doc.不同"))
    fs = jv.transform(ast_functions_doc, repo)
    exits = jv.cut(jv.judge([jv.state(on=jv.transform(signature, f), ctx=[jv.transform(docstring, f)]) for f in fs], 不同))
    不符 = [f.content["name"] for f, e in zip(fs, exits) if isinstance(e, jv.Act)]
    疑 = [f.content["name"] for f, e in zip(fs, exits) if isinstance(e, jv.Unsure)]
    jv.consume(exits, unsure=jv.drop)
    return {"不符": 不符, "unsure": 疑}


# ---------------------------------------------------------------- §9.9 E#85
@jv.program(budget=jv.Budget(calls=2, cost=0.01, layers=1, escalate=1))
def 升级是否破坏(repo, dep, ver):
    """11 §9.9 E#85 依赖升级是否破坏。"""
    破坏 = jv.test("有此前通过的测试现在失败了吗？", calib=jv.calib("dep.破坏"))
    前 = jv.do(run_pytest, repo, iter_seq=0)
    后 = jv.do(run_pytest, jv.do(upgrade, repo, dep, ver, iter_seq=0), iter_seq=1)
    match jv.cut(jv.judge(jv.state(on=后, ctx=[前]), 破坏)[0]):
        case jv.Act(): return jv.transform(tag, jv.transform(diff_reports, 前, 后), jv.lit("破坏"))
        case jv.Ignore(): return jv.transform(tag, 后, jv.lit("安全"))
        case jv.Unsure(c): return jv.handle(c, then=jv.escalate)


# ---------------------------------------------------------------- §9.10 E#87
@jv.program(budget=jv.Budget(calls=200, cost=0.02, layers=2))
def 配置漂移(a, b):
    """11 §9.10 E#87 配置漂移检测（test 分流 + measure 排序）。"""
    改变行为 = jv.test("这处差异会改变运行行为吗？", calib=jv.calib("cfg.改变行为"))
    严重 = jv.measure("这条差异的严重程度", scale=["低", "中", "高"], anchors=jv.anchors("漂移锚"), calib=jv.calib("cfg.严重"))
    ds = jv.transform(diff_cfg, a, b)
    exits = jv.cut(jv.judge([jv.state(on=d, ctx=[jv.transform(doc_of, d)]) for d in ds], 改变行为))
    变 = [d for d, e in zip(ds, exits) if isinstance(e, jv.Act)]
    疑 = [d.content["key"] for d, e in zip(ds, exits) if isinstance(e, jv.Unsure)]
    jv.consume(exits, unsure=jv.drop)
    v = jv.judge([jv.state(on=d) for d in 变], 严重)
    档 = jv.consume(jv.cut(v), unsure=jv.drop)
    排 = [(变[i].content["key"], 档[i].level if 档[i] is not None else None) for tier in v.order() for i in tier]
    return {"变": 排, "unsure": 疑}


# ---------------------------------------------------------------- §9.11 D#7（= 12 §6.1 第三条）
@jv.program(budget=jv.Budget(calls=80, cost=0.03, layers=6))
def 生成到全绿(spec, repo):
    """11 §9.11 D#7 生成到测试全绿（12 §6.1 原文）。"""
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


# ---------------------------------------------------------------- §9.12 D#23
@jv.program(budget=jv.Budget(calls=1000, cost=0.1, layers=1))
def 术中审计(传感器流):
    """11 §9.12 D#23 手术机器人实时控制（离线审计形式；时延锁不解）。"""
    偏离 = jv.test("这一帧的器械位置偏离计划路径吗？", calib=jv.calib("surg.偏离"))
    帧 = jv.transform(split, 传感器流, 200)
    exits = jv.cut(jv.judge([jv.state(on=f) for f in 帧], 偏离))
    偏 = [i for i, e in enumerate(exits) if isinstance(e, jv.Act)]
    疑 = [i for i, e in enumerate(exits) if isinstance(e, jv.Unsure)]
    jv.consume(exits, unsure=jv.drop)
    return {"偏": 偏, "unsure": 疑}


# ---------------------------------------------------------------- §9.13 D#41（= 12 §6.1 第四条）
@jv.program(budget=jv.Budget(calls=40, cost=0.02, layers=20, escalate=1))
def 取物(目标, 场景):
    """11 §9.13 D#41 任务级规划（12 §6.1 原文）。"""
    到达 = jv.test("目标物已在手中吗？", calib=jv.calib("grasp.到达"))
    下一步 = jv.select("下一步做哪个动作最接近目标？", calib=jv.calib("grasp.下一步"))
    观 = jv.do(a11y, jv.do(sense, 场景, iter_seq=0), iter_seq=0); seen = set()
    for step in range(1, 21):
        match jv.cut(jv.judge(jv.state(on=观, ctx=[目标]), 到达)[0]):
            case jv.Act(): return 观
            case jv.Unsure(c): jv.handle(c, then=jv.escalate)
        acts = [a for a in jv.transform(actions, 观) if a not in seen]
        match jv.cut(jv.judge(jv.state(on=观, ctx=[目标], over=acts), 下一步)):
            case jv.Pick(k): seen.add(acts[k]); 观 = jv.do(a11y, jv.do(act, acts[k], iter_seq=step), iter_seq=step)
            case jv.Unsure(c): jv.handle(c, then=jv.escalate)
    return jv.escalate(观)


# ---------------------------------------------------------------- §9.14 D#66
@jv.program(budget=jv.Budget(calls=500, cost=0.05, layers=1))
def 入侵迹象(log, 规则):
    """11 §9.14 D#66 日志入侵迹象（配对：日志段 × 规则）。"""
    符合 = jv.test("这段日志符合这条规则的字面描述吗？", calib=jv.calib("ids.符合"))
    段 = jv.transform(log_stage, log)
    ps = jv.transform(pairs_with, len(段), *段, *规则)
    exits = jv.cut(jv.judge([jv.state(on=p[0], ctx=[p[1]]) for p in ps], 符合))
    out = {}
    for p, e in zip(ps, exits):
        match e:
            case jv.Act(): out[(p[0].content, p[1].content)] = True
            case jv.Ignore(): out[(p[0].content, p[1].content)] = False
            case jv.Unsure(c): out[(p[0].content, p[1].content)] = jv.handle(c, keep=None)
    return out


# ---------------------------------------------------------------- §9.15 D#88
@jv.program(budget=jv.Budget(calls=1, cost=0.001, layers=1, escalate=1))
def 备餐排程(菜谱, 约束):
    """11 §9.15 D#88 备餐排程（无增益：求解器出排程，Jev 只核一次）。"""
    可行 = jv.test("这份排程说明满足了约束里写的全部条件吗？", calib=jv.calib("meal.可行"))
    排程 = jv.transform(solve, 约束, *菜谱)
    match jv.cut(jv.judge(jv.state(on=排程, ctx=[约束]), 可行)[0]):
        case jv.Act(): return 排程
        case jv.Ignore(): return jv.escalate(排程)
        case jv.Unsure(c): return jv.handle(c, then=jv.escalate)


# ---------------------------------------------------------------- §9.16 G-a
@jv.program(budget=jv.Budget(calls=200, cost=0.02, layers=2))
def 甲方风险条款(合同):
    """11 §9.16 G-a 合同不利条款按严重程度排序。"""
    不利 = jv.test("这条条款对甲方不利吗？", calib=jv.calib("contract.不利"))
    严重 = jv.measure("这条条款对甲方不利的严重程度", scale=["低", "中", "高"], anchors=jv.anchors("风险锚"), calib=jv.calib("contract.严重"))
    条 = jv.transform(split, 合同, 200)
    exits = jv.cut(jv.judge([jv.state(on=c) for c in 条], 不利))
    不利条 = [c for c, e in zip(条, exits) if isinstance(e, jv.Act)]
    疑 = [c.content for c, e in zip(条, exits) if isinstance(e, jv.Unsure)]
    jv.consume(exits, unsure=jv.drop)
    v = jv.judge([jv.state(on=c) for c in 不利条], 严重)
    档 = jv.consume(jv.cut(v), unsure=jv.drop)
    排 = [(不利条[i].content, 档[i].level if 档[i] is not None else None) for tier in v.order() for i in tier]
    return {"不利": 排, "unsure": 疑}


# ---------------------------------------------------------------- §9.16 G-b
@jv.program(budget=jv.Budget(calls=10, cost=0.01, layers=2, escalate=1))
def 客服挽留(对话, 话术库):
    """11 §9.16 G-b 客服流失判断并选话术。"""
    流失 = jv.test("这段对话显示客户有明显的流失倾向吗？", calib=jv.calib("cs.流失"))
    最合适 = jv.select("针对这位客户当前的状态，哪条话术最合适？", calib=jv.calib("cs.最合适"))
    match jv.cut(jv.judge(jv.state(on=对话), 流失)[0]):
        case jv.Act():
            match jv.cut(jv.judge(jv.state(on=对话, over=话术库), 最合适)):
                case jv.Pick(k): return 话术库[k]
                case jv.Unsure(c): return jv.handle(c, then=jv.escalate)
        case jv.Ignore(): return jv.lit("无需挽留")
        case jv.Unsure(c): return jv.handle(c, then=jv.escalate)


# ---------------------------------------------------------------- §9.16 G-c
@jv.program(budget=jv.Budget(calls=60, cost=0.02, layers=2))
def 挑简历(岗位, 简历库):
    """11 §9.16 G-c 简历挑三份并写理由。"""
    匹配度 = jv.measure("这份简历与岗位描述的匹配程度", scale=["低", "中", "高"], anchors=jv.anchors("匹配锚"), calib=jv.calib("cv.匹配度"))
    v = jv.judge([jv.state(on=r, ctx=[岗位]) for r in 简历库], 匹配度)
    jv.consume(jv.cut(v), unsure=jv.drop)
    top3 = [i for tier in v.order() for i in tier][:3]
    return [jv.transform(attach, 简历库[i], jv.do(写理由, 简历库[i], 岗位, iter_seq=i)) for i in top3]


# ---------------------------------------------------------------- G3-a
@jv.program(budget=jv.Budget(calls=200, cost=0.02, layers=1))
def 找矛盾论文对(论文集):
    """G3 (2)a 找互相引用同一数据集但结论相反的论文对（两题同状态一层融合）。"""
    同数据集 = jv.test("这两篇论文引用的是同一个数据集吗？", calib=jv.calib("paper.同数据集"))
    结论相反 = jv.test("这两篇论文的结论相反吗？", calib=jv.calib("paper.结论相反"))
    ps = jv.transform(pairs_self, *论文集)
    v = jv.judge([jv.state(on=(p[0], p[1])) for p in ps], 同数据集, 结论相反)
    矛盾, 疑 = [], []
    for p, rs in zip(ps, v):
        a, b = jv.cut(rs[0]), jv.cut(rs[1])
        if isinstance(a, jv.Act) and isinstance(b, jv.Act): 矛盾.append((p[0].content["id"], p[1].content["id"]))
        elif isinstance(a, jv.Unsure) or isinstance(b, jv.Unsure): 疑.append((p[0].content["id"], p[1].content["id"]))
        jv.consume([a, b], unsure=jv.drop)
    return {"矛盾": 矛盾, "unsure": 疑}


# ---------------------------------------------------------------- G3-b
@jv.program(budget=jv.Budget(calls=300, cost=0.03, layers=1, escalate=50))
def 工单路由(工单流, 部门集):
    """G3 (2)b 客服工单路由，转错升级给人。"""
    该部门 = jv.select("这张工单最应该转给哪个部门处理？", calib=jv.calib("ticket.该部门"))
    exits = jv.cut(jv.judge([jv.state(on=w, over=部门集) for w in 工单流], 该部门))
    out = {}
    for w, e in zip(工单流, exits):
        match e:
            case jv.Pick(k): out[w.content] = 部门集[k].content
            case jv.Unsure(c): out[w.content] = jv.handle(c, then=jv.escalate)
    jv.on_truth("ticket.该部门", lambda w, dept: dept)
    return out


# ---------------------------------------------------------------- G3-c（= 12 §6.1 第六条）
@jv.program(budget=jv.Budget(calls=300, cost=0.05, layers=4))
def 写docstring(repo):
    """G3 (2)c Python 仓库函数写 docstring，三版选最忠实，测试不过不改（12 §6.1 原文；layers 按实测改 4）。"""
    忠实 = jv.select("哪个 docstring 最忠实地描述了这段代码？", calib=jv.calib("doc.忠实"), prior=jv.prior.none)
    通过 = jv.test("测试报告显示全部通过吗？", calib=jv.calib("test.全过"))
    fs = jv.transform(ast_functions, repo)
    过 = jv.cut(jv.judge([jv.state(on=jv.do(run, jv.lit("pytest -q"), repo, f, iter_seq=0)) for f in fs], 通过))
    jv.consume(过, unsure=jv.drop)
    out = {}
    for f, e in zip(fs, 过):
        if not isinstance(e, jv.Act): continue
        vers = jv.gen("为这个函数写一句 docstring", ctx=[f], n=3, retry_seq=0)
        match jv.cut(jv.judge(jv.state(on=f, over=vers), 忠实)):
            case jv.Pick(k): out[f.content] = vers[k].content
            case jv.Unsure(c): jv.handle(c, keep=None)
    return out


PROGRAMS = [归因编译错误, 提交信息一致, 找flaky, 分类日志, 定位回归, 推断列类型, 生成并执行, 文档签名不一致, 升级是否破坏,
            配置漂移, 生成到全绿, 术中审计, 取物, 入侵迹象, 备餐排程, 甲方风险条款, 客服挽留, 挑简历,
            找矛盾论文对, 工单路由, 写docstring]

CALIB_KEYS = ["build.哪块", "commit.出现", "flaky.不一致", "log.类别", "perf.慢了", "col.类型", "cmd.成功", "doc.不同",
              "dep.破坏", "cfg.改变行为", "cfg.严重", "fix.最可能", "test.全过", "surg.偏离", "grasp.到达", "grasp.下一步",
              "ids.符合", "meal.可行", "contract.不利", "contract.严重", "cs.流失", "cs.最合适", "cv.匹配度",
              "paper.同数据集", "paper.结论相反", "ticket.该部门", "doc.忠实"]


# ================================================================ 假读数规则（按题面关键词分派；只为让程序走到有意义的出口）
def _state(text):
    try:
        return json.loads(text) if text.startswith("{") else {"on": text}
    except json.JSONDecodeError:
        return {"on": text}


def _noul(v):
    return {"type": "noul", "noul": 0.95 if v else 0.05}


def _choice(opts, pick):
    pick = pick if pick in opts else opts[0]
    return {"type": "choice", "choice": pick,
            "probabilities": {k: (0.9 if k == pick else 0.1 / max(1, len(opts) - 1)) for k in opts}}


def _score(lvl, n=3):
    return {"type": "score", "score": float(lvl),
            "probabilities": {str(i): (0.9 if i == lvl else 0.1 / (n - 1)) for i in range(n)}}


def _s(x):
    return x if isinstance(x, str) else json.dumps(x, ensure_ascii=False)


def fake_rule(text, qid, q):
    ins = q["instructions"]
    st = _state(text)
    on, ctx, over = st.get("on"), st.get("ctx", []), st.get("over", {})
    ons, ctxs = _s(on), _s(ctx)
    if q["type"] == "noul":
        if "在 diff 里出现" in ins: return _noul(any(w in ctxs for w in ("重试", "超时")) and ("重试" in ons or "超时" in ons))
        if "结果不一致" in ins: return _noul("pass" in ons and "fail" in ons)
        if "变慢" in ins: return _noul('"ms": 180' in ons)
        if "退出码" in ins: return _noul('"exit": 0' in ons and "README" in ons)
        if "参数与签名不同" in ins:
            sig = on.get("sig", "") if isinstance(on, dict) else ""
            doc = ctx[0].get("doc", "") if ctx and isinstance(ctx[0], dict) else ""
            return _noul(any(p and p not in doc for p in sig.strip("()").split(",")))
        if "现在失败" in ins:
            new = set(on.get("failed", [])) - set(ctx[0].get("failed", [])) if isinstance(on, dict) and ctx else set()
            return _noul(bool(new))
        if "改变运行行为" in ins: return _noul(on.get("key") in ("port", "path", "timeout"))
        if "全部通过" in ins: return _noul("passed" in ons)
        if "偏离计划" in ins: return _noul("偏移" in ons)
        if "手中" in ins: return _noul('"held": true' in ons)
        if "符合这条规则" in ins:
            kw = {"暴力登录": "Failed password", "扫描": "port scan", "提权": "sudo"}.get(_s(ctx[0]) if ctx else "", "∅")
            return _noul(kw in ons)
        if "全部条件" in ins: return _noul("满足" in ons)
        if "对甲方不利" in ins: return _noul(any(w in ons for w in ("无限", "赔偿", "不得")))
        if "流失倾向" in ins: return _noul("不想用了" in ons or "退订" in ons)
        if "同一个数据集" in ins: return _noul(isinstance(on, dict) and on["a"].get("dataset") == on["b"].get("dataset"))
        if "结论相反" in ins: return _noul(isinstance(on, dict) and on["a"].get("conclusion") != on["b"].get("conclusion"))
        return _noul(False)
    if q["type"] == "choice":
        crit = q["criteria"]; opts = list(crit)
        inv = {_s(v): k for k, v in crit.items()}
        if "哪一块修改" in ins:
            f = ons.split(":")[1] if ":" in ons else ""
            return _choice(opts, next((k for k, v in crit.items() if f and f.strip() in _s(v)), opts[0]))
        if "属于哪一类" in ins and "超时" in inv:
            lab = "超时" if "timeout" in ons else "拒绝" if "denied" in ons else "崩溃" if "crash" in ons else "正常"
            return _choice(opts, inv.get(lab, opts[0]))
        if "样例值属于哪一类" in ins:
            vals = on.get("values", []) if isinstance(on, dict) else []
            v0 = _s(vals[0]) if vals else ""
            lab = "日期" if "-" in v0 and v0[:4].isdigit() else "金额" if v0.replace(".", "").isdigit() else "ID" if v0.startswith("id_") else "自由文本"
            return _choice(opts, inv.get(lab, opts[0]))
        if "哪个动作" in ins: return _choice(opts, next((k for k, v in crit.items() if "grasp" in _s(v)), opts[0]))
        if "哪个修改" in ins: return _choice(opts, next((k for k, v in crit.items() if "fix" in _s(v)), opts[0]))
        if "哪条话术" in ins: return _choice(opts, next((k for k, v in crit.items() if "优惠" in _s(v)), opts[0]))
        if "哪个部门" in ins: return _choice(opts, next((k for k, v in crit.items() if _s(v) in ons), opts[0]))
        if "docstring" in ins: return _choice(opts, next((k for k, v in crit.items() if "版本 0" in _s(v)), opts[0]))
        return _choice(opts, opts[0])
    if q["type"] == "score":
        if "差异的严重程度" in ins:
            k = on.get("key", "") if isinstance(on, dict) else ""
            return _score(2 if k in ("port", "path") else 1 if k == "timeout" else 0)
        if "不利的严重程度" in ins: return _score(2 if "无限" in ons else 1 if "赔偿" in ons else 0)
        if "匹配程度" in ins:
            y = on.get("years", 0) if isinstance(on, dict) else 0
            return _score(2 if y >= 5 else 1 if y >= 2 else 0)
        return _score(0)
    return None


def calib_all(rt):
    for k in CALIB_KEYS:
        rt.calib.put(k, hi=0.6, lo=0.3, n=50, status="上岗", set_id="fake")


# ================================================================ 输入数据（假）
def inputs():
    L = jv.lit
    diff = L("@@ -1,3 +1,3 @@ foo.c: int x = 1; 加了重试逻辑\n@@ -7,2 +7,4 @@ bar.c: print_all(a, b) 改了超时参数")
    return {
        "归因编译错误": (L({"name": "repo"}), diff),
        "提交信息一致": (L("加了重试逻辑；改了超时参数；顺手修了排版；更新了版权年份"), diff),
        "找flaky": (L({"name": "repo"}), 4),
        "分类日志": (L("10:01 request timeout after 30s\n10:02 access denied for user bob\n10:03 worker crash: segfault\n10:04 ok 200"),),
        "定位回归": (L({"name": "repo"}), L("0"), L("9")),
        "推断列类型": (L({"header": ["created", "amount", "user", "note"],
                          "rows": [["2026-01-02", "12.5", "id_1", "第一条备注"], ["2026-02-03", "7.0", "id_2", "第二条"]]}),),
        "生成并执行": (L("列出当前目录文件"), L("/tmp")),
        "文档签名不一致": (L({"functions": [{"name": "f", "sig": "(a, b)", "doc": "参数 a 与 b"},
                                             {"name": "g", "sig": "(x, y, z)", "doc": "参数 x 与 y"}]}),),
        "升级是否破坏": (L({"name": "repo", "failing": []}), L("requests"), L("3.0")),
        "配置漂移": (L({"port": 8080, "comment": "old", "timeout": 30, "path": "/data"}),
                     L({"port": 9090, "comment": "new", "timeout": 60, "path": "/data"})),
        "生成到全绿": (L("规格：函数返回和"), L({"fails": 2})),
        "术中审计": (L("帧1 正常 位于路径上。" * 8 + "帧2 偏移 3mm 超出容差。" * 8 + "帧3 正常。" * 8),),
        "取物": (L("杯子"), L("桌面")),
        "入侵迹象": (L("Failed password for root from 1.2.3.4\nport scan detected from 5.6.7.8\nuser alice logged in"),
                     [L("暴力登录"), L("扫描"), L("提权")]),
        "备餐排程": ([L({"name": "汤"}), L({"name": "主菜"}), L({"name": "甜点"})], L("汤先于主菜；甜点最后")),
        "甲方风险条款": (L("第一条 乙方承担无限赔偿责任" + "。" * 190 + "第二条 双方友好协商" + "。" * 190 + "第三条 甲方不得单方解约并须赔偿" + "。" * 180),),
        "客服挽留": (L("客户：我不想用了，太贵。"), [L("送您一张优惠券"), L("给您转接技术"), L("好的再见")]),
        "挑简历": (L("Python 后端 5 年"), [L({"name": "甲", "years": 6}), L({"name": "乙", "years": 1}),
                                           L({"name": "丙", "years": 3}), L({"name": "丁", "years": 8})]),
        "找矛盾论文对": ([L({"id": "P1", "dataset": "MNIST", "conclusion": "有效"}), L({"id": "P2", "dataset": "MNIST", "conclusion": "无效"}),
                          L({"id": "P3", "dataset": "CIFAR", "conclusion": "有效"})],),
        "工单路由": ([L("发票开错了 财务"), L("登录不了 技术"), L("想退货 售后")], [L("财务"), L("技术"), L("售后")]),
        "写docstring": (L({"fails": 0}),),
    }


def run_all(root: str | None = None, programs=None):
    rows = []
    data = inputs()
    with jv.Runtime(client=jv.FakeClient(rule=fake_rule), root=root, generator=fake_generator) as rt:
        calib_all(rt)
        for prog in (programs or PROGRAMS):
            name = prog.__name__
            args = data[name]
            try:
                out = prog(*args)
                status = "ok"
            except jv.Pending as e:
                out, status = f"Pending({e.key[:8]})", "pending"
            layers = rt.stats["layers"]
            rows.append({"程序": name, "结果": repr(out)[:80], "状态": status, "层数": len(layers),
                         "每层题数": [l["questions"] for l in layers], "每层调用": [l["calls"] for l in layers],
                         "调用": rt.stats["calls"], "题": rt.stats["questions"], "警告": rt.stats["warnings"]})
    return rows


if __name__ == "__main__":
    for r in run_all():
        print(json.dumps(r, ensure_ascii=False))
