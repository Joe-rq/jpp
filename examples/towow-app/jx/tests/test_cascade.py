"""事件链的级联预算（预注册 16）：每次接入的级联调用有上限、按排序值准入、用尽挂起、下一次相关事件续算；
构型节点不召回必然无效的对方；方案门按展开后的 agent 算前 2。"""
import json

from conftest import FnPort, run
from netkit import PACKS, make_engine
from test_leave import agreeable

from jx.sched import FixturePort


def _final(eng):
    edges = {tuple(k): (eng.read("edge", k) or {}).get("holds") for k in eng.keys("edge")}
    return edges, sorted(k[0] for k in eng.keys("config")), sorted(k[0] for k in eng.keys("plan"))


async def _one_join(judge, flags=None, seed=0, joiner="n01", cap=None, ledger=None, gen=None):
    """其余 7 人作背景预载（只进索引、自己不召回），再接入一人：一次接入就是一条事件链。"""
    eng = make_engine(judge=judge, flags=flags or {}, seed=seed, ledger_path=ledger)
    if gen is not None:
        eng.gen_port = gen
    if cap is not None:
        eng.cascade = dict(eng.cascade, calls=cap)
    await eng.start()
    for p in PACKS:
        if p["id"] != joiner:
            await eng.put_source("world", [p["id"]], {**p, "background": True})
    await eng.idle()
    await eng.put_source("world", [joiner], next(p for p in PACKS if p["id"] == joiner))
    await eng.idle()
    return eng


def test_join_cascade_within_budget_parks_rest_without_skipping(tmp_path):
    led = str(tmp_path / "l.jsonl")

    async def go():
        eng = await _one_join(FnPort(agreeable), ledger=led)
        st = eng.stats()
        out = (st["cascade_calls"], len(eng.parked), eng.sched.budget_skips, eng.cascade["calls"])
        await eng.stop()
        return out
    used, parked, skips, cap = run(go())
    rows = [json.loads(x) for x in open(led)]
    assert used <= cap                         # 级联调用不超过本链预算（硬上限）
    assert parked > 0                          # 「什么都说成」的小网：预算碰顶，剩下的挂起
    assert skips == 0                          # 挂起不是跳过：没有因预算未观察的题
    assert not any(r["kind"] == "unobserved" for r in rows)
    assert any(r["kind"] == "park" for r in rows) and any(r["kind"] == "admit" for r in rows)


def test_admission_puts_units_above_line_first(tmp_path):
    """同一条链里，有线上（≥ 0.65）的单元在等时，线下的不准入。"""
    led = str(tmp_path / "l.jsonl")

    async def go():
        eng = await _one_join(FnPort(agreeable), ledger=led)
        line = eng.cascade["line"]
        await eng.stop()
        return line
    line = run(go())
    waiting: dict = {}
    n_below = 0
    for r in (json.loads(x) for x in open(led)):
        if r["kind"] == "park":
            waiting[r["unit"]] = (r["chain"], r["rank"])
        elif r["kind"] in ("admit", "retract"):
            if r["kind"] == "admit" and r["rank"] < line:
                n_below += 1
                assert not any(c == r["chain"] and rk >= line for u, (c, rk) in waiting.items() if u != r["unit"]), r
            waiting.pop(r["unit"], None)
    assert n_below >= 0


def test_parked_pair_resumes_on_member_disclosure():
    """挂起的两两在成员披露（宿主写 unlocked/reply，另一条事件链）时续算，记到那条链的级联账上。"""
    async def go():
        eng = await _one_join(FnPort(agreeable), cap=4)        # 接入者有 7 个对方：根上的两两只准入 4 对
        pairs = [uid for uid in eng.parked if uid.startswith("两两")]
        assert pairs, "预算 4 时应有挂起的两两"
        u = eng.units[pairs[0]]
        k = u.args[0]
        n_chain = next(eng.chain_seq)
        await eng.put_source("unlocked", [k["b"], k["a"]], [{"text": "下周三以后都有空", "tier": "t1", "cat": "时间安排"}])
        await eng.idle()
        ch = u.chain
        out = (pairs[0] in eng.parked, ch.seq > n_chain, ch.cascade_used, ch.cause)
        await eng.stop()
        return out
    still, new_chain, used, cause = run(go())
    assert not still and new_chain and used >= 1 and cause.startswith("put:unlocked")


def test_final_state_matches_unbudgeted_after_enough_events():
    """给足事件（反复 resume 到没有挂起）后，成立边、构型、方案与关掉级联预算时一致。
    用稀疏的哈希读数（多数对不成立）：「什么都说成」的稠密小网在不开预算时换个种子终态也不同（构型的 take(novel, 6)、
    每条链重跑上限），不能拿来比。"""
    async def go(flags, seed, joiner, resume):
        eng = await _one_join(FixturePort(skew=4.0), flags=flags, seed=seed, joiner=joiner, cap=None if not resume else 10)
        n = 0
        while resume and eng.parked and n < 100:
            await eng.resume()
            await eng.idle()
            n += 1
        f = _final(eng)
        parked_before = eng.parks
        await eng.stop()
        return f, n, parked_before
    for seed in (0, 1):
        off, _, _ = run(go({"no_cascade": True}, seed, "n01", False))
        on, n, parks = run(go({}, seed, "n01", True))
        assert parks > 0 and n >= 1             # 预算 10 确实挂起过、续算过
        assert on == off


class _Gen:
    async def json(self, prompt):
        return {"title": "一起做", "summary": "s", "to_confirm": ["时间安排"]}

    from jx.ports import gen as _g
    gen_json = _g._gen_json


def test_config_recall_and_plan_gate_on_dense_net():
    """「什么都说成」的 8 人小网，全体接入（关掉级联预算，只看程序的两处改动）：
    丙 构型节点不召回其他构型节点、不召回自己展开后的成员，嵌套到第二层的构型节点不召回；
    丁 方案门按展开后的 agent 算前 2——任何一个人（含经构型节点间接在内的）至多出现在 2 份方案里。"""
    async def go():
        eng = make_engine(judge=FnPort(agreeable), flags={"no_cascade": True})
        eng.gen_port = _Gen()
        await eng.start()
        for p in PACKS:
            await eng.put_source("world", [p["id"]], p)
        await eng.idle()
        mats = {k[0]: eng.read("material", k) for k in eng.keys("material")}
        edges = [tuple(k) for k in eng.keys("edge")]
        cfgs = {k[0]: eng.read("config", k) for k in eng.keys("config")}
        plans = [k[0] for k in eng.keys("plan")]
        await eng.stop()
        return mats, edges, cfgs, plans
    mats, edges, cfgs, plans = run(go())

    def is_cfg(m):
        return str(m).startswith("cfg:")

    def closure(m):
        return [y for x in (mats.get(m) or {}).get("members", []) for y in closure(x)] if is_cfg(m) else [m]

    def depth(m):
        return 1 + max([depth(x) for x in (mats.get(m) or {}).get("members", [])] or [0]) if is_cfg(m) else 0
    cfg_edges = [e for e in edges if any(is_cfg(x) for x in e)]
    assert cfg_edges, "fixture 小网里应有构型节点召回出的两两"
    for a, b in cfg_edges:
        assert not (is_cfg(a) and is_cfg(b)), (a, b)
        c, y = (a, b) if is_cfg(a) else (b, a)
        assert y not in closure(c) and depth(c) < 2, (a, b)
    assert plans
    assert any(any(is_cfg(m) for m in c["members"]) for c in cfgs.values()), "应有组合的组合"
    per: dict = {}
    for k in plans:
        for a in set(y for m in cfgs[k]["members"] for y in closure(m)):
            per[a] = per.get(a, 0) + 1
    assert max(per.values()) <= 2
