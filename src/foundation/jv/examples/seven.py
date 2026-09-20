"""第七条示例：日志异常分级（measure 题的用法；G4 零上下文读者的第二个任务）。

三档有序（低 / 中 / 高）用 `jv.measure`；出口是 `jv.At(k)`，k 是 scale 的 0 起下标；
「高」走不可逆动作「发告警」，守卫是同一状态上一道 trusted 的 test 出口；「中」攒起来汇总；「低」丢弃。
跑：`python -m foundation.jv.examples.seven`。
"""

from __future__ import annotations

import json
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))))

import foundation.jv as jv  # noqa: E402

_SENT: list[str] = []


def _alert(seg, *rest):
    _SENT.append(seg.content)
    return {"sent": seg.content}


# 不可逆动作：发告警。它自己的输出是否可信与守卫无关；守卫要的是「放行它的那个 Act 来自 trusted 状态」。
告警 = jv.register_action("alert", fn=_alert, taint_out="inherit", reversible=False,
                         reason="", cost=0.001)          # cost：每发一条告警计入预算的钱

LEVELS = ("低", "中", "高")


@jv.program(budget=jv.Budget(calls=20, cost=0.02, layers=3, escalate=2))
def 日志分级(日志段):
    严重 = jv.measure("这段日志的严重程度是？", scale=LEVELS, calib=jv.calib("log.严重"))
    需告警 = jv.test("这段日志表明服务已经不可用了吗？", calib=jv.calib("log.不可用"))
    # 两题同一状态：登记在同一直线段里，一次刷新同层融合（每段一次调用两道题）
    rs = jv.judge([jv.state(on=s) for s in 日志段], 严重, 需告警)
    档 = jv.cut([r[0] for r in rs])                      # 每段的 At(k) / Unsure
    门 = jv.cut([r[1] for r in rs])                      # 每段的 Act / Ignore / Unsure（守卫用）
    汇总, 已发 = [], []
    for i, (s, e, g) in enumerate(zip(日志段, 档, 门)):
        match e:
            case jv.At(2):                               # 「高」：k=2 是 LEVELS 的下标
                if isinstance(g, jv.Act):                # 守卫：trusted 状态上的 Act 才放行不可逆动作
                    已发.append(jv.do(告警, s, iter_seq=i, guard=g))
                else:
                    汇总.append(s.content)               # 严重但门没开：进汇总，人看
            case jv.At(1): 汇总.append(s.content)
            case jv.At(0): pass
            case jv.Unsure(c): 汇总.append(s.content); jv.handle(c, keep=None)
    jv.consume(门, unsure=jv.drop)                       # 门题的 Unsure 也要消费（J-05）
    return {"告警": 已发, "汇总": 汇总}                    # 期物直接返回：程序返回时解析成 Mat


def fake_rule(text, qid, q):
    state = json.loads(text) if text.startswith("{") else {}
    on = str(state.get("on", ""))
    if q["type"] == "score":
        idx = 2 if ("FATAL" in on or "OOM" in on) else (1 if ("WARN" in on or "retry" in on) else 0)
        n = len(q["criteria"])
        return {"type": "score", "score": float(idx), "probabilities": {str(i): (0.9 if i == idx else 0.05) for i in range(n)}}
    if q["type"] == "noul":
        return {"type": "noul", "noul": 0.95 if ("killed" in on or "down" in on) else 0.05}
    return None


def calib_all(rt):
    rt.calib.put("log.严重", hi=0.6, lo=0.3, n=50, status="上岗", set_id="fake")
    rt.calib.put("log.不可用", hi=0.6, lo=0.3, n=50, status="上岗", set_id="fake")


def run_seven(root: str | None = None, passes: dict | None = None) -> dict:
    with jv.Runtime(client=jv.FakeClient(rule=fake_rule), root=root, passes=passes) as rt:
        calib_all(rt)
        日志 = [jv.lit("INFO 启动完成"), jv.lit("WARN 连接池 retry 3 次"), jv.lit("FATAL OOM killed worker"), jv.lit("DEBUG 心跳")]
        out = 日志分级(日志)
        layers = rt.stats["layers"]
        st = rt.stats_report()
        return {"程序": "日志分级", "结果": repr(out)[:80], "状态": "ok", "层数": len(layers),
                "每层题数": [l["questions"] for l in layers], "每层调用": [l["calls"] for l in layers],
                "调用": rt.stats["calls"], "题": rt.stats["questions"], "融合率": st["fusion_rate"],
                "账本命中": st["ledger_hits"], "钱": st["cost"], "停层": st["stopped_layers"],
                "警告": rt.stats["warnings"], "out": out}


if __name__ == "__main__":
    r = run_seven()
    print(json.dumps({k: v for k, v in r.items() if k != "out"}, ensure_ascii=False, default=str))
