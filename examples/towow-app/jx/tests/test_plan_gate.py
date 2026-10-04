"""方案门（预注册 11）：决定性题出口 act 加每位成员前 2，不再要求 0.6；只看过公开层的方案标草案。"""
from conftest import FnPort, run
from netkit import PACKS, make_engine

from jx.ports import gen as genmod


def lukewarm(state, q):
    """只交 t0 时的公网读数带：开放题略过半（0.56），整体「能成吗」偏低（0.45），负例与「去掉某人」答否。"""
    if q.op == "test":
        if "已经解决" in q.text or "冲突" in q.text or "同一种" in q.text or "去掉" in q.text:
            return 0.1
        return 0.45 if "能成吗" in q.text else 0.56
    if q.op == "select":
        labels = list(q.criteria)
        pick = next((x for x in ("both", "direct") if x in labels), labels[0])
        return {x: (0.9 if x == pick else 0.1 / max(1, len(labels) - 1)) for x in labels}
    return None


class FakeGen:
    def __init__(self):
        self.prompts = []

    async def json(self, prompt):
        self.prompts.append(prompt)
        return {"title": "一起做", "summary": "s", "to_confirm": ["时间安排"]}

    gen_json = genmod._gen_json


def test_pair_plans_without_point_six_line_top2_and_draft():
    g = FakeGen()

    async def go():
        eng = make_engine(judge=FnPort(lukewarm))
        eng.gen_port = g
        await eng.start()
        for p in PACKS:
            await eng.put_source("world", [p["id"]], p)
        await eng.idle()
        cfgs = {k[0]: eng.read("config", k) for k in eng.keys("config")}
        plans = {k[0]: eng.read("plan", k) for k in eng.keys("plan")}
        await eng.stop()
        return cfgs, plans

    cfgs, plans = run(go())
    pairs = {k: c for k, c in cfgs.items() if c["shape"] == "pair"}
    assert pairs, "整体读数 0.45 时两人构型仍应由两两的决定性题成立"
    assert all(abs(c["hold"]["p"] - 0.56) < 1e-6 for c in pairs.values())      # 置信度 = 两两决定性题读数
    assert plans, "旧门（hold ≥ 0.6）下这里一份方案都没有"
    # fixture 居民会按策略回补信息：解锁过片段的构型出定稿，没有的出草案；草案标题由生成端加前缀
    drafts = [p for p in plans.values() if p.get("draft")]
    assert drafts and all(p["title"].startswith("草案") for p in drafts)
    assert all(not p["title"].startswith("草案") for p in plans.values() if not p.get("draft"))
    per = {}
    for k in plans:
        for m in cfgs[k]["members"]:
            per[m] = per.get(m, 0) + 1
    assert max(per.values()) <= 2                     # 每位成员都把它排进前 2：任何人至多 2 份
    assert any('"draft": true' in p for p in g.prompts)
