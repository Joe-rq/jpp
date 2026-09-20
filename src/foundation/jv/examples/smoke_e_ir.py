"""E-IR-SMOKE：工单转部门 5 张工单，真机两遍（第二遍应全部重放）。$ ≤ 0.02。"""

from __future__ import annotations

import json
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))))

import foundation.jv as jv  # noqa: E402

ROOT = os.path.join(os.path.dirname(os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))),
                    "runs", "jv", os.environ.get("SMOKE_TAG", "e-ir-smoke"))

工单 = ["发票抬头开错了，需要重新开一张增值税专用发票", "APP 一直提示登录失败，验证码收不到",
       "收到的商品有破损，想申请退货退款", "想咨询企业采购的合同付款条款和开票流程", "页面加载很慢，经常白屏"]
部门 = ["财务", "技术", "售后"]


@jv.program(budget=jv.Budget(calls=20, cost=0.02, layers=1))
def 工单转部门_smoke(工单流, 部门表):
    去哪 = jv.select("这张工单该转给哪个部门？", calib=jv.calib("ticket.去哪"))
    exits = jv.cut(jv.judge([jv.state(on=t, over=部门表) for t in 工单流], 去哪))
    out = {}
    for t, e in zip(工单流, exits):
        match e:
            case jv.Pick(k): out[t.content] = (部门表[k].content, "calibrated")
            case jv.Unsure(c):
                p = jv.handle(c)                                   # cold → 保守线 + provisional 标（handler 库 §5）
                out[t.content] = (部门表[p.k].content if isinstance(p, jv.Pick) else f"unsure:{p.cause}",
                                  "provisional" if p.provisional else "unsure")
                if isinstance(p, jv.Unsure):
                    jv.handle(p, then=jv.drop)
    return out


def one_run(tag: str) -> dict:
    client = jv.JevClient()
    with jv.Runtime(client=client, root=ROOT) as rt:
        out = 工单转部门_smoke([jv.lit(t) for t in 工单], [jv.lit(d) for d in 部门])
        readings = []
        for e in rt.exits:
            if e.reading is not None and not e.provisional:
                readings.append(json.dumps(e.reading._ans, sort_keys=True, ensure_ascii=False))
        return {"tag": tag, "out": out, "calls": rt.stats["calls"], "questions": rt.stats["questions"],
                "layers": len(rt.stats["layers"]), "ledger_hits": rt.stats["ledger_hits"],
                "cost": round(rt.stats["cost"], 6), "tokens": rt.stats["tokens"], "readings": readings,
                "mode_share": [e.reading._ans.get("mode_share") for e in rt.exits if e.reading is not None and not e.provisional],
                "warnings": rt.stats["warnings"]}


if __name__ == "__main__":
    r1 = one_run("run1")
    r2 = one_run("run2")
    same = sum(a == b for a, b in zip(r1["readings"], r2["readings"]))
    summary = {"run1": {k: v for k, v in r1.items() if k != "readings"},
               "run2": {k: v for k, v in r2.items() if k != "readings"},
               "readings_identical": f"{same}/{len(r1['readings'])}"}
    os.makedirs(ROOT, exist_ok=True)
    with open(os.path.join(ROOT, "summary.json"), "w", encoding="utf-8") as fh:
        json.dump(summary, fh, ensure_ascii=False, indent=1)
    print(json.dumps(summary, ensure_ascii=False, indent=1))
