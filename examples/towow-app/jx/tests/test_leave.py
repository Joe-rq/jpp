"""离开：网络程序在成员离开后不留下含他的构型、构型节点与边（10-05 公网离开实测的回归）。"""
from conftest import FnPort, run
from netkit import PACKS, make_engine


def agreeable(state, q):
    """什么都说成：保证 fixture 小网里有边、有构型（含三人构型）。负例题、「去掉某人还成立吗」答否。"""
    if q.op == "test":
        return 0.1 if ("已经解决" in q.text or "冲突" in q.text or "同一种" in q.text or "去掉" in q.text) else 0.95
    if q.op == "select":
        labels = list(q.criteria)
        pick = next((x for x in ("both", "direct") if x in labels), labels[0])
        return {x: (0.9 if x == pick else 0.1 / max(1, len(labels) - 1)) for x in labels}
    return None


def _mentions(eng, who):
    out = []
    for fam in ("edge", "config", "node", "material", "world", "adj", "derived", "plan", "unlocked", "reply"):
        for k in eng.keys(fam):
            if any(who in str(x) for x in k):
                out.append((fam, k))
    for fam in ("adj", "derived"):            # 按对方覆盖的单元：别人那里不该还留着他
        for k in eng.keys(fam):
            v = eng.read(fam, k) or {}
            if isinstance(v, dict) and who in v:
                out.append((fam, k, "peer"))
    return out


def test_leave_retracts_configs_and_edges():
    async def go():
        eng = make_engine(judge=FnPort(agreeable))
        await eng.start()
        for p in PACKS:
            await eng.put_source("world", [p["id"]], p)
        await eng.idle()
        cfgs = [eng.read("config", k) for k in eng.keys("config")]
        assert cfgs, "fixture 网络里应当有构型，否则这个测试测不到构型的撤回"
        who = sorted(cfgs, key=lambda c: -len(c["members"]))[0]["members"][0]
        assert _mentions(eng, who)
        await eng.remove_source("world", [who])
        await eng.idle()
        left = _mentions(eng, who)
        await eng.stop()
        return left
    assert run(go()) == []
