"""G4 零上下文读者：只看 README / six.py / __init__.py 签名写的三条程序（可运行版）。

跑：`cd 地基 && .venv/bin/python -m foundation.jv.examples.fresh4`
猜点编号与 `设计/G4-零上下文读者-构建器.md` 一致。
"""

from __future__ import annotations

import json
import os
import sys
import tempfile

sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))))

import foundation.jv as jv  # noqa: E402

# ---------------------------------------------------------------- 假 S 库
_ALERTS = []


def _alert(seg, *rest):                       # 不可逆动作：发告警（这里只是记一笔）
    _ALERTS.append(seg.content)
    return {"sent": seg.content}


_PAGE = {"姓名": "", "邮箱": ""}                 # 假网页的可变状态


def _read_page(page, *rest):                  # 读可访问性树 → 文本
    return "表单 " + " ".join(f"{k}:{v or '(空)'}" for k, v in _PAGE.items()) + " 按钮:提交"


def _operate(action, page, *rest):            # 点击 / 输入
    a = action.content if isinstance(action, jv.Mat) else action
    if a.startswith("输入 "):
        k, v = a[3:].split("=", 1)
        _PAGE[k] = v
    return {"did": a}


def _submit(page, *rest):                     # 不可逆：提交
    return {"submitted": dict(_PAGE)}


def 候选动作(观, 目标):
    tree, goal = 观.content, 目标.content
    want = dict(kv.split("=", 1) for kv in goal.split(";"))
    acts = [f"输入 {k}={v}" for k, v in want.items() if f"{k}:(空)" in tree]
    return acts + ["点击 提交按钮"] if acts else ["提交"]


告警 = jv.Action("alert", fn=_alert, taint_out="trusted")
读页 = jv.Action("read_page", fn=_read_page, taint_out="trusted")   # 猜 14：把自家浏览器读到的树当 trusted
操作 = jv.Action("operate", fn=_operate, taint_out="trusted")
提交 = jv.Action("submit", fn=_submit, taint_out="trusted")


# ---------------------------------------------------------------- 程序 1：客户名单合并去重
@jv.program(budget=jv.Budget(calls=50, cost=0.02, layers=2))
def 名单合并(名单甲, 名单乙):
    同人 = jv.test("这两条客户记录指的是同一个人吗？", calib=jv.calib("crm.同人"))
    对子 = [(i, j) for i in range(len(名单甲)) for j in range(len(名单乙))]
    exits = jv.cut(jv.judge([jv.state(on=名单甲[i], ctx=[名单乙[j]]) for i, j in 对子], 同人))   # 一层全并发
    并掉 = set()
    for (i, j), e in zip(对子, exits):
        match e:
            case jv.Act(): 并掉.add(j)
            case jv.Ignore(): pass
            case jv.Unsure(c):
                答 = jv.ask(jv.state(on=名单甲[i], ctx=[名单乙[j]]), 同人)          # 交给人：Pending 挂起
                if isinstance(答, jv.Act): 并掉.add(j)
    return [m.content for m in 名单甲] + [m.content for j, m in enumerate(名单乙) if j not in 并掉]


# ---------------------------------------------------------------- 程序 2：日志异常分级
@jv.program(budget=jv.Budget(calls=50, cost=0.02, layers=2))
def 日志分级(日志段):
    严重 = jv.measure("这段日志的严重程度是？", scale=("低", "中", "高"), calib=jv.calib("log.严重"))
    exits = jv.cut(jv.judge([jv.state(on=s) for s in 日志段], 严重))
    汇总, 已发 = [], []
    for i, (s, e) in enumerate(zip(日志段, exits)):
        match e:
            case jv.At(2): 已发.append(jv.do(告警, s, iter_seq=i, guard=e))    # 不可逆：带守卫；猜 11：At 带档位下标不是标签
            case jv.At(1): 汇总.append(s.content)
            case jv.At(0): pass
            case jv.Unsure(c): 汇总.append(s.content); jv.handle(c, keep=None)    # 拿不准的不发，进汇总
    return {"告警": [f.content for f in 已发], "汇总": 汇总}


# ---------------------------------------------------------------- 程序 3：网页表单自动填写
@jv.program(budget=jv.Budget(calls=60, cost=0.03, layers=12, escalate=1))
def 填表(目标, 页面):
    下一步 = jv.select("为完成目标，下一步该做哪个动作？", calib=jv.calib("form.下一步"))
    全填 = jv.test("页面显示目标要求的所有字段都已填好了吗？", calib=jv.calib("form.全填"))
    观 = jv.do(读页, 页面, iter_seq=0)
    for it in jv.loop(bound=8, variant=jv.decreasing(lambda: 观.content.count("(空)"))):   # 猜 12：variant 必填
        动作 = jv.transform(候选动作, 观, 目标)
        match jv.cut(jv.judge(jv.state(on=观, ctx=[目标], over=动作), 下一步)):
            case jv.Pick(k) if "提交" in 动作[k].content:                   # 猜 13：transform 返回的列表元素是 Mat
                好 = jv.cut(jv.judge(jv.state(on=观, ctx=[目标]), 全填)[0])
                if isinstance(好, jv.Act): return jv.do(提交, 页面, iter_seq=it.n, guard=好).content   # 只有可信 Act 才放行；猜 14/19
                jv.consume([好], unsure=jv.drop)                                             # 没填全/拿不准：不提交，继续
            case jv.Pick(k): 观 = jv.do(读页, jv.do(操作, 动作[k], 页面, iter_seq=it.n), iter_seq=it.n)
            case jv.Unsure(c): jv.handle(c, then=jv.escalate)
    return jv.escalate(观)


# ---------------------------------------------------------------- 假读数规则
def fake_rule(text, qid, q):
    ins = q["instructions"]
    state = json.loads(text) if text.startswith("{") else {}
    if q["type"] == "noul":
        if "同一个人" in ins:
            a, b = str(state.get("on", "")), str((state.get("ctx") or [""])[0])
            ea, eb = a.split("<")[-1].rstrip(">"), b.split("<")[-1].rstrip(">")
            na, nb = a.split()[0], b.split()[0]
            return {"type": "noul", "noul": 0.95 if ea == eb else (0.5 if na == nb else 0.05)}
        if "已填好" in ins:
            return {"type": "noul", "noul": 0.95 if "(空)" not in str(state.get("on", "")) else 0.05}
        return {"type": "noul", "noul": 0.5}
    if q["type"] == "choice":
        opts = list(q["criteria"]); over = state.get("over", {}); on = str(state.get("on", ""))
        for o in sorted(opts):
            v = str(over.get(o, ""))
            if v.startswith("输入 ") and f"{v[3:].split('=')[0]}:(空)" in on:
                return {"type": "choice", "choice": o, "probabilities": {k: (0.9 if k == o else 0.1 / max(1, len(opts) - 1)) for k in opts}}
        o = opts[-1]
        return {"type": "choice", "choice": o, "probabilities": {k: (0.9 if k == o else 0.1 / max(1, len(opts) - 1)) for k in opts}}
    if q["type"] == "score":                                             # 猜 8：measure 下沉后的题型叫 score
        on = str(state.get("on", ""))
        idx = 2 if ("FATAL" in on or "OOM" in on) else (1 if ("WARN" in on or "retry" in on) else 0)
        n = len(q["criteria"])                                           # 猜 9/10：score 是 0..n-1 的档位下标，probabilities 按下标键
        return {"type": "score", "score": float(idx), "probabilities": {i: (0.9 if i == idx else 0.05) for i in range(n)}}
    return None


def calib_all(rt):
    for k in ("crm.同人", "log.严重", "form.下一步", "form.全填"):
        rt.calib.put(k, hi=0.6, lo=0.3, n=50, status="上岗", set_id="fake")


def run_all(root: str | None = None):
    rows = []
    root = root or tempfile.mkdtemp(prefix="fresh4-")
    with jv.Runtime(client=jv.FakeClient(rule=fake_rule), root=root) as rt:
        calib_all(rt)
        甲 = [jv.lit("张三 华为 <zs@huawei.com>"), jv.lit("李四 腾讯 <ls@tencent.com>")]
        乙 = [jv.lit("张三 华为技术 <zs@huawei.com>"), jv.lit("李四 阿里 <lisi@alibaba.com>"), jv.lit("王五 字节 <ww@bytedance.com>")]
        日志 = [jv.lit("INFO 启动完成"), jv.lit("WARN 连接池 retry 3 次"), jv.lit("FATAL OOM killed worker"), jv.lit("DEBUG 心跳")]
        cases = [
            ("名单合并", lambda: 名单合并(甲, 乙)),
            ("日志分级", lambda: 日志分级(日志)),
            ("填表", lambda: 填表(jv.lit("姓名=张三;邮箱=zs@huawei.com"), jv.lit("https://example/form"))),
        ]
        for name, fn in cases:
            try:
                out = fn(); status = "ok"
            except jv.Pending as e:                                       # 猜 5：人答后重放要带 root 的运行时
                rt.answer(e.key, "act")                                    # 假装人答「是同一个人」
                out, status = fn(), f"pending→answered({e.key[:8]})"
            except Exception as e:  # noqa: BLE001
                out, status = f"{type(e).__name__}: {e}", "error"
            layers = rt.stats["layers"]
            rows.append({"程序": name, "结果": repr(out)[:120], "状态": status, "层数": len(layers),
                         "每层题数": [l["questions"] for l in layers], "调用": rt.stats["calls"], "警告": rt.stats["warnings"]})
    return rows


if __name__ == "__main__":
    for r in run_all():
        print(json.dumps(r, ensure_ascii=False))
