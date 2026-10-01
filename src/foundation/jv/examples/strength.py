"""长处探针：三条程序，各用一种「发挥不确定性长处」的构件（清单 G4；v0.1 §2.2 / §2.3 / §2.9 / J-10 / §7「判断力花在哪」）。

裸调用（拿到 p 就 if p > 0.5）写不出这三条的共同原因：它把读数当成值。这里读数是带校准线的随机变量，
三条程序分别用它的三种性质——离线的距离（分配复核）、线随代价移动（代价比线）、跨题只能经注册桥合成（fit）。
全部 FakeClient，$0。跑：`python -m foundation.jv.examples.strength`。
"""

from __future__ import annotations

import json
import os
import random
import sys

sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))))

import foundation.jv as jv  # noqa: E402

# =============================================================== 1. 按不确定性分配复核预算
# 十二段文档：四段明显含数字事实、四段明显不含、四段模棱两可（真值各半）。读数 p 由假规则按段落标记给出。
段落 = [
    ("clear-yes 2025 年营收 12.4 亿元，同比增长 18%。", 0.93, 1),
    ("clear-no 我们相信产品会越来越好。", 0.05, 0),
    ("fuzzy-a 客户数量较上季度有所增加。", 0.52, 0),
    ("clear-yes 服务器 CPU 峰值 87%，持续 14 分钟。", 0.91, 1),
    ("fuzzy-b 交付周期缩短了大约一半。", 0.58, 1),
    ("clear-no 团队士气高涨。", 0.04, 0),
    ("fuzzy-c 多数用户在首周内完成了注册。", 0.47, 1),
    ("clear-yes 版本 3.2 修复了 27 个缺陷。", 0.95, 1),
    ("fuzzy-d 成本略有上升。", 0.43, 0),
    ("clear-no 这是一个令人兴奋的方向。", 0.06, 0),
    ("clear-yes 平均响应时间 230 毫秒。", 0.9, 1),
    ("clear-no 感谢所有参与者。", 0.03, 0),
]
真值 = {t: y for t, _, y in 段落}


@jv.program(budget=jv.Budget(calls=20, cost=0.01, layers=2, escalate=4))
def 分配复核(段们):
    """长处：判断向量 + J-10 unsure 上界 + `jv.allocate`（§7「判断力花在哪」）。
    裸调用写不出：它没有「离线多远」这个量，只能随机抽或全抽复核。"""
    含数字 = jv.test("这段文字包含可核对的数字事实吗？", calib=jv.calib("doc.含数字"))
    rs = jv.judge([jv.state(on=s) for s in 段们], 含数字)          # 一层：12 个状态并发
    上界 = jv.unsure_bound(rs)                                     # J-10：整批 unsure 期望数的联合界（估计，不花钱）
    k = jv.budget().escalate                                       # 复核名额来自预算，不是常数
    复核 = jv.allocate(rs, k)                                       # 最不确定的 k 段（离校准线最近）
    出口 = jv.cut(rs)
    jv.consume(出口, unsure=jv.drop)                               # 未被复核的 unsure 按丢弃记账（J-05）
    return {"复核": 复核, "出口": 出口, "上界": 上界}


def _err(exits, reviewed: set, texts) -> float:
    """假真值下的错误率：被复核的按人答（0 错）；其余 Act≠真值或 Unsure 计错。"""
    n_err = 0
    for i, (e, t) in enumerate(zip(exits, texts)):
        if i in reviewed:
            continue
        pred = 1 if isinstance(e, jv.Act) else (0 if isinstance(e, jv.Ignore) else None)
        if pred is None or pred != 真值[t.content]:
            n_err += 1
    return round(n_err / len(exits), 3)


def run_allocate(rt) -> dict:
    texts = [jv.lit(t) for t, _, _ in 段落]
    out = 分配复核(texts)
    k = len(out["复核"])
    rnd = random.Random(0).sample(range(len(texts)), k)              # 对照臂：随机抽同样多的段落复核
    return {"程序": "分配复核", "k": k, "上界": out["上界"],
            "按不确定性复核": out["复核"], "错误率·分配": _err(out["出口"], set(out["复核"]), texts),
            "随机复核": rnd, "错误率·随机": _err(out["出口"], set(rnd), texts),
            "错误率·不复核": _err(out["出口"], set(), texts)}


# =============================================================== 2. 代价比线
工单 = [(f"ticket-{i:02d} " + s, p) for i, (s, p) in enumerate([
    ("商品到手已碎裂，附照片。", 0.9), ("下单后 30 分钟想取消。", 0.55), ("收到的颜色与页面不符。", 0.7),
    ("用了两周觉得不好用。", 0.35), ("快递显示签收但没收到。", 0.62), ("发票抬头写错。", 0.08),
    ("尺码偏小想换。", 0.45), ("重复扣款两次。", 0.95), ("赠品没有随单。", 0.28), ("包装有拆封痕迹。", 0.5),
    ("物流超过承诺时效三天。", 0.4), ("问一下退货流程。", 0.12),
])]
P_工单 = dict(工单)


@jv.program(budget=jv.Budget(calls=20, cost=0.01, layers=2))
def 代价比线(工单们):
    """长处：`jv.cut(r, cost=(fp, fn))`——线由标注集按代价矩阵算出（§2.3），程序只说代价、不说线。
    裸调用写不出：它只能手写 0.5 或改两个常数，线不跟代价走、也不跟标注集走（I4）。"""
    要退 = jv.test("这条工单应当退款吗？", calib=jv.calib("ticket.退款"))
    rs = jv.judge([jv.state(on=t) for t in 工单们], 要退)
    漏退更贵 = jv.cut(rs, cost=(1, 10))     # fn 是 fp 的 10 倍：宁可多退
    误退更贵 = jv.cut(rs, cost=(10, 1))     # 反过来：宁可不退
    jv.consume(漏退更贵, unsure=jv.drop); jv.consume(误退更贵, unsure=jv.drop)
    return {"漏退更贵": 漏退更贵, "误退更贵": 误退更贵}


def _labeled_set(n: int = 120, seed: int = 1) -> list:
    """假标注集：p 均匀铺开，label 与 p 相关且中段带噪声；n ≥ 100。"""
    rnd = random.Random(seed)
    out = []
    for i in range(n):
        p = round(i / (n - 1), 4)
        flip = rnd.random() < max(0.0, 0.35 - abs(p - 0.5))     # 越靠中间越可能翻
        y = (1 if p >= 0.5 else 0) ^ (1 if flip else 0)
        out.append([p, y])
    return out


def run_costline(rt) -> dict:
    ts = [jv.lit(t) for t, _ in 工单]
    out = 代价比线(ts)
    a, b = out["漏退更贵"], out["误退更贵"]
    kinds = lambda es: [e.kind for e in es]
    diff = sum(1 for x, y in zip(a, b) if x.kind != y.kind)
    line_a = a[0].detail["cost_line"]["line"]; line_b = b[0].detail["cost_line"]["line"]
    return {"程序": "代价比线", "线·漏退更贵": line_a, "线·误退更贵": line_b,
            "出口·漏退更贵": kinds(a), "出口·误退更贵": kinds(b), "出口不同的条数": diff,
            "Act 数": (sum(k == "act" for k in kinds(a)), sum(k == "act" for k in kinds(b)))}


# =============================================================== 3. fit 桥
变更 = [("diff-1 只改 parse()；报告 42 passed", 0.92, 0.9), ("diff-2 改了 parse() 与 3 处调用；报告 42 passed", 0.9, 0.3),
      ("diff-3 只改 parse()；报告 40 passed 2 failed", 0.1, 0.88), ("diff-4 改了 5 个文件；报告 41 passed 1 failed", 0.15, 0.1),
      ("diff-5 只改 parse()；报告 42 passed 1 skipped", 0.75, 0.9)]
P_变更 = {t: (a, b) for t, a, b in 变更}


def register_fit(rt):
    """训练过程产生的注册（J-16）：两特征 → n ≥ max(50, 40)；trained_from ≠ 后续保形集 id。"""
    rt.fits.register("绿且局部", trained_from="train-A", n=60, error_rate=0.08, version="1",
                     features=[("test.全过", "test"), ("diff.局部", "test")],
                     fn=lambda a, b: a * b)                          # 训练得到的合成：两题同时高才高


@jv.program(budget=jv.Budget(calls=20, cost=0.01, layers=1))
def 自动合入(变更们):
    """长处：`jv.fit`——跨题读数唯一合法的合成（I3），且只认注册表签名（J-16）；分数仍要 `cut(calib)` 才出口。
    裸调用写不出：p1*p2 在程序里是 J-01 错（跨题算术无定义），只有注册过的桥能做。"""
    全过 = jv.test("测试报告显示全部通过吗？", calib=jv.calib("test.全过"))
    局部 = jv.test("diff 只改了被测函数吗？", calib=jv.calib("diff.局部"))
    rs = jv.judge([jv.state(on=c) for c in 变更们], 全过, 局部)     # 一层：每状态两题一次调用
    出口 = [jv.cut(jv.fit(jv.fitref("绿且局部"), r[0], r[1]), calib=jv.calib("fit.绿且局部")) for r in rs]
    jv.consume(出口, unsure=jv.drop)
    return [c.content.split()[0] for c, e in zip(变更们, 出口) if isinstance(e, jv.Act)]


def run_fit(rt) -> dict:
    out = 自动合入([jv.lit(t) for t, _, _ in 变更])
    return {"程序": "自动合入", "合入": out}


# =============================================================== 假客户端与校准记录
def fake_rule(text, qid, q):
    st = json.loads(text) if text.startswith("{") else {}
    on = str(st.get("on", ""))
    if q["type"] != "noul":
        return None
    for t, p, _ in 段落:
        if t == on:
            return {"type": "noul", "noul": p}
    if on in P_工单:
        return {"type": "noul", "noul": P_工单[on]}
    if on in P_变更:
        a, b = P_变更[on]
        return {"type": "noul", "noul": a if "通过" in q["instructions"] else b}
    return {"type": "noul", "noul": 0.5}


def calib_all(rt):
    rt.calib.put("doc.含数字", hi=0.65, lo=0.35, n=100, status="上岗", set_id="conf-doc", unsure_rate=0.2)
    rt.calib.put("ticket.退款", hi=0.65, lo=0.35, n=120, status="上岗", set_id="conf-B",
                 samples=_labeled_set(120), label_set_id="label-A")
    rt.calib.put("test.全过", hi=0.6, lo=0.3, n=60, status="上岗", set_id="conf-t")
    rt.calib.put("diff.局部", hi=0.6, lo=0.3, n=60, status="上岗", set_id="conf-d")
    rt.calib.put("fit.绿且局部", hi=0.6, lo=0.3, n=80, status="上岗", set_id="conf-B")   # ≠ trained_from "train-A"
    register_fit(rt)


def _stats_row(rt, name: str) -> dict:
    layers, st = rt.stats["layers"], rt.stats_report()
    return {"程序": name, "层数": len(layers), "每层题数": [l["questions"] for l in layers],
            "每层调用": [l["calls"] for l in layers], "题": rt.stats["questions"], "调用": rt.stats["calls"],
            "融合率": st["fusion_rate"], "账本命中": st["ledger_hits"], "停层": st["stopped_layers"], "钱": st["cost"],
            "警告": rt.stats["warnings"]}


def run_all(root: str | None = None, passes: dict | None = None) -> list[dict]:
    """三条各开一个 Runtime（互不共账），返回 stats 行（与 six.stats_table 同形）+ 结果。"""
    rows = []
    for name, fn in (("分配复核", run_allocate), ("代价比线", run_costline), ("自动合入", run_fit)):
        with jv.Runtime(client=jv.FakeClient(rule=fake_rule), root=root, passes=passes) as rt:
            calib_all(rt)
            res = fn(rt)
            row = _stats_row(rt, name)
            row["结果"] = res
            rows.append(row)
    return rows


if __name__ == "__main__":
    for r in run_all():
        print(json.dumps({k: v for k, v in r.items()}, ensure_ascii=False, default=str))
