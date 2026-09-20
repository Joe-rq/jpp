"""G5 零上下文读者（构建器二）写的三条程序：会议纪要派待办 / 合同条款风险标红 / 报错信息改写并写回。

只读了 README、six.py、seven.py 与 __init__.py 的签名。猜点编号与 `设计/G5-零上下文读者-构建器二.md` 对齐。
跑：`cd 地基 && .venv/bin/python -m foundation.jv.examples.fresh5`
"""

from __future__ import annotations

import json
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))))

import foundation.jv as jv  # noqa: E402

# ================================================================ 程序一：会议纪要派待办
@jv.program(budget=jv.Budget(calls=40, cost=0.02, layers=3, escalate=10))
def 纪要派待办(纪要, 人员表):
    可执行 = jv.test("这是不是一条可执行的待办？", calib=jv.calib("todo.可执行"))
    派给谁 = jv.select("这条待办该派给谁？", calib=jv.calib("todo.派给谁"))
    条目 = jv.gen("从会议纪要里提取待办条目，一条一句", ctx=[纪要], n=6, retry_seq=0)      # 猜 1、2
    出口 = jv.cut(jv.judge([jv.state(on=t, ctx=[纪要]) for t in 条目], 可执行))         # 一层：全部条目并发
    待派, 待问 = [], []
    for t, e in zip(条目, 出口):
        match e:
            case jv.Act(): 待派.append(t)
            case jv.Ignore(): pass
            case jv.Unsure(): 待问.append(t)                                              # 猜 3：命中即消费，先攒着
    选 = jv.cut(jv.judge([jv.state(on=t, over=人员表) for t in 待派], 派给谁))            # 第二层：猜 4
    派单 = {}
    for t, e in zip(待派, 选):
        match e:
            case jv.Pick(k): 派单[t.content] = 人员表[k].content
            case jv.Unsure(): 待问.append(t)
    return {"派单": 派单, "问人": jv.escalate([t.content for t in 待问])}                  # 猜 5：攒起来一次交人


# ================================================================ 程序二：合同条款风险标红
def 拆条款(合同):
    return [ln.strip() for ln in str(合同.content).splitlines() if ln.strip()]


@jv.program(budget=jv.Budget(calls=40, cost=0.02, layers=3))
def 条款标红(合同, 模板):
    风险 = jv.measure("这条条款对我方的风险档位？", scale=("低", "中", "高"), calib=jv.calib("contract.风险"))
    冲突 = jv.test("这条条款与我方模板条款冲突吗？", calib=jv.calib("contract.冲突"))
    条款 = jv.transform(拆条款, 合同)                                                    # 猜 6：拆分是宿主函数 → transform
    rs = jv.judge([jv.state(on=c, ref=[模板]) for c in 条款], 风险)                      # 猜 7：模板进 ref
    档 = jv.cut(rs)
    高, 中 = [], []
    for i, (c, e) in enumerate(zip(条款, 档)):
        match e:
            case jv.At(2): 高.append((i, c))
            case jv.At(1): 中.append(i)
            case jv.At(0): pass
            case jv.Unsure(): 中.append(i)                                              # 猜 9：拿不准按中风险人看
    门 = jv.cut(jv.judge([jv.state(on=c, ref=[模板]) for _, c in 高], 冲突))            # 第二层：高风险才问冲突
    标红 = []
    for (i, c), g in zip(高, 门):
        match g:
            case jv.Act(): 标红.append({"条款": i + 1, "理由": jv.gen("摘录这条条款与模板冲突之处", ctx=[c, 模板], n=1, retry_seq=i)[0].content})   # 猜 10
            case jv.Unsure(): 标红.append({"条款": i + 1, "理由": "拿不准，需人看"})
    序 = [i for 组 in rs.order() for i in 组]                                            # 猜 11、12：.order() 实测返回「下标分组列表」，先展平
    中序 = [i + 1 for i in 序 if i in 中]
    return {"标红": 标红, "中风险从高到低": 中序}


# ================================================================ 程序三：报错信息改写并写回
_FILE: dict[str, str] = {}


def _write_back(新, 原, *rest):
    _FILE[原.content] = 新.content
    return {"written": 原.content}


写回 = jv.register_action("write_back", fn=_write_back, taint_out="inherit", reversible=False,
                          reason="写回本地文件，只写入已守卫放行的改写", cost=0.0)


def 错误码(报错):
    return str(报错.content).split(":")[0].strip()


def 保留错误码(报错, 候选):                                                              # 猜 13：transform 的参数可以是材料列表
    code = 错误码(报错)
    return [c.content for c in 候选 if code in str(c.content)]


@jv.program(budget=jv.Budget(calls=40, cost=0.02, layers=4))
def 改写报错(报错们):
    最清楚 = jv.select("哪个改写最清楚？", calib=jv.calib("err.最清楚"))
    没走样 = jv.test("这条改写保留了原报错的意思与错误码吗？", calib=jv.calib("err.没走样"))
    候选表 = []
    for i, e in enumerate(报错们):
        cands = jv.gen("把这条命令行报错改写得更清楚，保留错误码", ctx=[e], n=3, retry_seq=i)   # 猜 14：retry_seq 用循环变量
        有码 = jv.transform(保留错误码, e, cands)                                          # 猜 15：返回 [Mat]，可能为空
        候选表.append((e, 有码))
    可选 = [(e, cs) for e, cs in 候选表 if cs]                                             # 猜 16：over 为空不敢交给 select
    选 = jv.cut(jv.judge([jv.state(on=e, over=cs) for e, cs in 可选], 最清楚))            # 一层
    中选 = [(e, cs[k]) for (e, cs), x in zip(可选, 选) if isinstance(x, jv.Pick) and (k := x.k) is not None]   # 猜 17
    jv.consume(选, unsure=jv.drop)
    门 = jv.cut(jv.judge([jv.state(on=新, ctx=[原]) for 原, 新 in 中选], 没走样))          # 第二层：守卫题
    写了, 没写 = [], []
    for i, ((原, 新), g) in enumerate(zip(中选, 门)):
        if isinstance(g, jv.Act): 写了.append(jv.do(写回, 新, 原, iter_seq=i, guard=g))   # 猜 18：guard 传 Act；iter_seq 用循环变量
        else: 没写.append(原.content)
    jv.consume(门, unsure=jv.drop)
    return {"写了": 写了, "没写": 没写}


# ================================================================ 假 S 函数：生成器 + 读数规则
def fake_generator(prompt, ctx, n, retry_seq):
    src = [c.content if isinstance(c, jv.Mat) else c for c in (ctx or [])]                   # 猜 19：ctx 元素是 Mat
    if "待办" in prompt:
        return ["张三本周内提交后端接口文档", "王五下周二前交登录页设计稿", "大家这周辛苦了",
                "李四评估数据库迁移方案（是否要做待定）", "赵六把会议室投影修一下"][:n]
    if "摘录" in prompt:
        return [f"条款写「{str(src[0])[:12]}…」，模板要求有限责任"][:n]
    if "改写" in prompt:
        code = str(src[0]).split(":")[0].strip()
        return [f"{code}: 无法连接到服务器，请检查地址与端口是否正确", "连接被拒绝", f"{code}: 拒绝连接"][:n]
    return []


def fake_rule(text, qid, q):
    st = json.loads(text) if text.startswith("{") else {}
    on = str(st.get("on", ""))
    ins = q["instructions"]
    if q["type"] == "noul":
        if "可执行" in ins:
            return {"type": "noul", "noul": 0.5 if "待定" in on else (0.05 if "辛苦" in on else 0.95)}
        if "冲突" in ins:
            return {"type": "noul", "noul": 0.95 if "无限" in on else 0.05}
        if "保留" in ins:
            return {"type": "noul", "noul": 0.5 if "W300" in on else 0.95}
        return {"type": "noul", "noul": 0.5}
    if q["type"] == "score":
        idx = 2 if ("无限" in on or "违约金" in on) else (1 if ("自动续约" in on or "30 日" in on or "单方" in on) else 0)
        p = {"自动续约": 0.8, "单方": 0.85}.get(next((k for k in ("自动续约", "单方") if k in on), ""), 0.9)   # 三条「中」概率不同，好看 order
        n = len(q["criteria"])
        return {"type": "score", "score": float(idx), "probabilities": {str(i): (p if i == idx else (1 - p) / (n - 1)) for i in range(n)}}
    if q["type"] == "choice":
        opts = list(q["criteria"]); over = st.get("over", {})
        for o in sorted(opts):
            v = str(over.get(o, ""))
            if (v.split("：")[0] in on) or ("请检查" in v):
                return {"type": "choice", "choice": o, "probabilities": {k: (0.9 if k == o else 0.1 / max(1, len(opts) - 1)) for k in opts}}
        return {"type": "choice", "choice": opts[0], "probabilities": {k: 1 / len(opts) for k in opts}}
    return None


def calib_all(rt):
    for k in ("todo.可执行", "todo.派给谁", "contract.风险", "contract.冲突", "err.最清楚", "err.没走样"):
        rt.calib.put(k, hi=0.6, lo=0.3, n=50, status="上岗", set_id="fake")


def run_all():
    rows = []
    with jv.Runtime(client=jv.FakeClient(rule=fake_rule), generator=fake_generator) as rt:
        calib_all(rt)
        cases = [
            ("纪要派待办", lambda: 纪要派待办(jv.lit("讨论了接口文档、登录页与数据库迁移；投影仪坏了。"),
                                          [jv.lit("张三：后端开发"), jv.lit("王五：前端设计"), jv.lit("李四：数据库"), jv.lit("赵六：行政")])),
            ("条款标红", lambda: 条款标红(jv.lit("乙方承担无限责任\n合同到期自动续约一年\n付款期 30 日\n甲方可单方解除\n争议提交北京仲裁"),
                                      jv.lit("模板：乙方责任以合同总额为限；不自动续约"))),
            ("改写报错", lambda: 改写报错([jv.lit("E1042: connection refused"), jv.lit("E2001: file not found"), jv.lit("W300: deprecated flag")])),
        ]
        for name, fn in cases:
            try:
                out = fn(); status = "ok"
            except jv.Pending as e:
                out, status = f"Pending({e.key[:8]})", "pending"
            except Exception as e:  # noqa: BLE001 —— 零上下文读者：跑不通就原样记录
                out, status = f"{type(e).__name__}: {e}", "error"
            layers = rt.stats["layers"]
            rows.append({"程序": name, "状态": status, "结果": repr(out)[:300], "层数": len(layers),
                         "每层题数": [l["questions"] for l in layers], "调用": rt.stats["calls"],
                         "警告": rt.stats["warnings"]})
    return rows


if __name__ == "__main__":
    for r in run_all():
        print(json.dumps(r, ensure_ascii=False, default=str))
    print("文件写回：", _FILE)
