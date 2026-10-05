"""预注册 17：方案名额直接与嵌套分开排；构型判断材料不带判断结果状态；at_rest 程序在链结束时跑一次。"""
import json

from conftest import FnPort, make, run
from netkit import PACKS, make_engine
from test_leave import agreeable


AT_REST_SRC = '''budget {calls: 1000, cost: 10};
cell src[k] reducer single;
cell out[k] reducer single;
resident 判(k) on [change(src[k])] {
    let v = settled src[k];
    let r = judge(state(mat(v)), [test("是吗？")]);
    put out[k] <- {v: v, p: cut(r[0]).p};
    v
}
resident 收(k) on [change(out[k])] at_rest {
    let o = settled out[k];
    o.v
}
'''


def test_at_rest_runs_once_at_chain_end():
    """链内被标脏多次的 at_rest 程序，只在链结束时按最后的输入跑一次。"""
    async def go():
        eng = make(AT_REST_SRC)
        await eng.start()
        for i in range(4):                       # 不等静止，连写四次
            await eng.put_source("src", ["a"], {"t": f"第{i}版"})
        await eng.idle()
        n = eng.attempts_by.get("收", 0)
        v = eng.read("收", ["a"])
        await eng.stop()
        return n, v
    n, v = run(go())
    assert n == 1 and v == {"t": "第3版"}


def warm(state, q):
    """读数都落在边界带里（0.56）：开放题与整体「能成吗」都会先去要信息；负例题、「去掉某人」答否。"""
    if q.op == "test":
        return 0.1 if ("已经解决" in q.text or "冲突" in q.text or "同一种" in q.text or "去掉" in q.text) else 0.56
    if q.op == "select":
        labels = list(q.criteria)
        pick = next((x for x in ("both", "direct") if x in labels), labels[0])
        return {x: (0.9 if x == pick else 0.1 / max(1, len(labels) - 1)) for x in labels}
    return None


def _real(p):
    return {**p, "real": True}


def test_direct_and_nested_plan_quotas_are_separate():
    """直接构型：每位直接成员至多 2 份、best 里只有直接构型；嵌套构型：只记进真实接入者的 nbest、
    每份嵌套方案展开后都有真实接入者、每位真实接入者至多 2 份。"""
    class Gen:
        async def json(self, prompt):
            return {"title": "一起做", "summary": "s", "to_confirm": ["时间安排"]}
        from jx.ports import gen as _g
        gen_json = _g._gen_json

    async def go():
        eng = make_engine(judge=FnPort(agreeable), flags={"no_cascade": True})
        eng.gen_port = Gen()
        await eng.start()
        for p in PACKS:
            await eng.put_source("world", [p["id"]], _real(p) if p["id"] == "n01" else p)
        await eng.idle()
        mats = {k[0]: eng.read("material", k) for k in eng.keys("material")}
        cfgs = {k[0]: eng.read("config", k) for k in eng.keys("config")}
        plans = [k[0] for k in eng.keys("plan")]
        best = {k[0]: eng.read("best", k) or {} for k in eng.keys("best")}
        nbest = {k[0]: eng.read("nbest", k) or {} for k in eng.keys("nbest")}
        await eng.stop()
        return mats, cfgs, plans, best, nbest
    mats, cfgs, plans, best, nbest = run(go())

    def is_cfg(m):
        return str(m).startswith("cfg:")

    def closure(m):
        return [y for x in (mats.get(m) or {}).get("members", []) for y in closure(x)] if is_cfg(m) else [m]
    nested = {k for k, c in cfgs.items() if any(is_cfg(m) for m in c["members"])}
    assert nested, "fixture 小网里应有组合的组合"
    assert all(not is_cfg(m) for m in best)                                   # 名额只给 agent
    assert all(k not in nested for b in best.values() for k in b)            # 直接名额里没有嵌套构型
    assert set(nbest) <= {"n01"} and all(k in nested for b in nbest.values() for k in b)   # 嵌套名额只给真实接入者
    direct_plans = [k for k in plans if k not in nested]
    nested_plans = [k for k in plans if k in nested]
    assert direct_plans
    per: dict = {}
    for k in direct_plans:
        for m in cfgs[k]["members"]:
            per[m] = per.get(m, 0) + 1
    assert max(per.values()) <= 2
    assert all("n01" in [y for m in cfgs[k]["members"] for y in closure(m)] for k in nested_plans)
    assert len(nested_plans) <= 2


def test_config_material_has_no_judgment_state_and_denial_does_not_wake_config_recall(tmp_path):
    """构型的 material 不带 hold/lacks；接入者拒绝一次补信息后，构型节点的召回不重跑、构型材料不重发。"""
    led = str(tmp_path / "l.jsonl")

    async def go():
        eng = make_engine(judge=FnPort(warm), flags={"no_cascade": True}, ledger_path=led)
        await eng.start()
        for p in PACKS:
            if p["id"] != "n01":
                await eng.put_source("world", [p["id"]], {**p, "background": True})
        await eng.idle()
        await eng.put_source("world", ["n01"], _real(next(p for p in PACKS if p["id"] == "n01")))
        await eng.idle()
        cmats = {k[0]: eng.read("material", k) for k in eng.keys("material") if str(k[0]).startswith("cfg:")}
        reqs = [q for q in (eng.read("inbox", ["n01"]) or []) if str(q.get("q")) == "hold"] or (eng.read("inbox", ["n01"]) or [])
        mark = eng.ledger.n
        if reqs:
            q = reqs[0]
            await eng.put_source("reply", ["n01", q["from"], q["cat"]], {"denied": True, "cat": q["cat"], "by": "n01"})
            await eng.idle()
        await eng.stop()
        return cmats, bool(reqs), mark
    cmats, had_req, mark = run(go())
    assert cmats and all("hold" not in m and "lacks" not in m and "value" not in m for m in cmats.values())
    assert had_req, "真实接入者应收到补信息请求"
    after = [json.loads(x) for x in open(led)][mark:]
    assert not any(r["kind"] == "attempt" and str(r["unit"]).startswith("召回(cfg:") for r in after)
    assert not any(r["kind"] == "publish" and r.get("cell") == "material" and str(r["key"][0]).startswith("cfg:") for r in after)
