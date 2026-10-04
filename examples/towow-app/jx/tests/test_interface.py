"""宿主接口 v1（jx/README.md 顶部）逐项按文档写法调用；跨尝试同 state 合批；E-tier。"""
import pytest

from conftest import ROOT, FnPort, make, run
from jx.engine import Engine, VirtualClock

IFACE_SRC = '''budget {calls: 100, cost: 1};
cell world[a] reducer single;
cell inbox[b] reducer union;
cell node[x] reducer single;
resident 发布(a) on [change(world[a])] {
    let w = settled world[a];
    let hits = do("route", [a, w], 0);
    let e = cut(judge(state({me: w, hits: hits}), test("值得认识吗？")));
    let n = {id: a, ok: act(e), n_hits: len(hits)};
    put node[a] <- n;
    n
}
resident 收(e) on [event(ping)] { {got: ev} }
'''


def test_host_interface_as_documented(tmp_path):
    src = tmp_path / "p.jpx"
    src.write_text(IFACE_SRC)
    events, routed = [], []

    async def go():
        eng = Engine.load(str(src), ports={"judge": FnPort(lambda st, q: 0.8)}, flags={"no_fill": False},
                          seed=0, ledger_path=str(tmp_path / "l.jsonl"), events_path=str(tmp_path / "e.jsonl"),
                          clock=VirtualClock())

        def route(x, node):
            routed.append(x)
            return [{"peer": p} for p in ["a1", "a2"] if p != x]
        eng.register_action("route", route, cost_usd=0.0, transparent=True, depends_on=["node"])
        eng.bus.subscribe(events.append)
        await eng.start()
        e1 = await eng.put_source("world", ["a1"], {"t": "一"}, budget={"calls": 10}, deadline_s=15)
        e2 = await eng.put_source("world", ["a2"], {"t": "二"})
        await eng.idle()
        assert isinstance(e1, int) and e2 >= e1
        assert eng.read("node", ["a1"], mode="peek") == {"id": "a1", "ok": True, "n_hits": 1}
        assert eng.read("node", ["a1"], mode="settled")["ok"] is True
        assert eng.read("node", ["zz"]) is None
        st = eng.status("发布", ["a1"])
        assert st["status"] == "settled" and st["version"] >= 1 and st["pending"] == []
        assert sorted(eng.keys("node")) == [["a1"], ["a2"]]
        assert eng.keys("node", contains="a2") == [["a2"]]
        await eng.event("ping", {"x": 1})
        await eng.idle()
        await eng.remove_source("world", ["a2"])
        await eng.idle()
        assert eng.read("world", ["a2"]) is None and eng.read("node", ["a2"]) is None   # 写者撤回，节点删除
        await eng.stop()
    run(go())
    types = {e["type"] for e in events}
    assert {"publish", "batch", "judge", "invalidate", "stats"} <= types
    pub = [e for e in events if e["type"] == "publish" and e["cell"] == "node"]
    assert pub[0]["key"] == ["a1"] or pub[0]["key"] == ["a2"]
    assert any(e["status"] == "removed" for e in pub)
    assert (tmp_path / "e.jsonl").read_text().count("\n") == len(events)
    assert any(e["type"] == "publish" and e["cell"] == "收" and e["value"] == {"got": {"x": 1}} for e in events)


SHARED_SRC = '''budget {calls: 100, cost: 1};
cell world[a] reducer single;
resident 看(a) on [change(world[a])] {
    let others = filter(members(world), fn(b) { b != a });
    map(others, fn(b) {
        let lo = if a < b { a } else { b };
        let hi = if a < b { b } else { a };
        let s = state({lo: peek world[lo], hi: peek world[hi]});        // 两个程序单元造出同一 state
        act(cut(judge(s, test(a + " 能帮上对方吗？"))))
    })
}
'''


def test_cross_attempt_same_state_one_call():
    batches = []

    async def go():
        eng = make(SHARED_SRC, port=FnPort(lambda st, q: 0.9))
        eng.bus.subscribe(lambda e: batches.append(e) if e["type"] == "batch" else None)
        await eng.start()
        for a in ["p", "q", "r", "s"]:
            await eng.put_source("world", [a], {"t": a})
        await eng.idle()
        assert not eng.errors
        return eng.sched.calls, eng.sched.questions
    calls, qs = run(go())
    assert calls == 6 and qs == 12            # 6 个无序对 = 6 个 state；每个 state 上两个程序单元各一题，合成一次
    assert all(b["merged_from"] >= 2 for b in batches)


ETIER_SRC = '''budget {calls: 10, cost: 1};
cell unlocked[a, b] reducer union;
cell go[k] reducer single;
resident 判(k) on [change(go[k])] {
    let g = settled go[k];
    let m = mat({x: "a 的私事"}, {owner: "a", tier: g.tier});
    act(cut(judge(state({m: m}, {owners: ["a", "b"]}), test("好？"))))
}
'''


def test_e_tier():
    async def go():
        eng = make(ETIER_SRC, port=FnPort(lambda st, q: 0.9))
        await eng.start()
        await eng.put_source("go", ["1"], {"tier": "t1"})
        await eng.idle()
        bad = list(eng.errors)
        await eng.put_source("unlocked", ["a", "b"], {"cat": "近况", "tier": "t1", "text": "…"})
        await eng.idle()
        ok = eng.read("判", ["1"])
        await eng.put_source("go", ["2"], {"tier": "never"})
        await eng.idle()
        return bad, ok, eng.errors
    bad, ok, errs = run(go())
    assert bad and "E-tier" in bad[0]["error"] and "只向 b 解锁到 t0" in bad[0]["error"]
    assert ok is True                            # 解锁到 t1 之后，同一程序被标脏重跑、通过
    assert any("never" in e["error"] for e in errs)


def test_resident_budget_clause_inherited_by_spawn():
    src = '''budget {calls: 100, cost: 10};
cell world[a] reducer single;
resident 接入(a) on [change(world[a])] budget {calls: 3} deadline 15 {
    let w = settled world[a];
    map(range(4), fn(i) { spawn 判(a, i) })
}
resident 判(a, i) { cut(judge(state({a: a, i: i, w: peek world[a]}), test("好？"))).kind }
'''

    async def go():
        eng = make(src, port=FnPort(lambda st, q: 0.9))
        await eng.start()
        await eng.put_source("world", ["x"], "一")
        await eng.idle()
        r1 = sorted(eng.read("判", ["x", i]) for i in range(4))
        c1 = eng.sched.calls
        await eng.put_source("world", ["x"], "二")        # 新事件链：再派生 3 次
        await eng.idle()
        return r1, c1, eng.sched.calls
    r1, c1, c2 = run(go())
    assert c1 == 3 and r1 == ["act", "act", "act", "unsure"]
    assert c2 - c1 == 3


def test_retract_then_rewrite_same_key_no_churn():
    """派生者重算后换了一个实参不同、写同一键的下游：只发版本更新，不发删除再新建。"""
    src = '''budget {calls: 100, cost: 10};
cell src[k] reducer single; cell out[k] reducer single;
resident 父(k) on [change(src[k])] { let v = settled src[k]; spawn 子(k, v) }
resident 子(k, v) { put out[k] <- {k: k, v: v}; v }
'''
    pubs = []

    async def go():
        eng = make(src)
        eng.bus.subscribe(lambda e: pubs.append(e["status"]) if e["type"] == "publish" and e["cell"] == "out" else None)
        await eng.start()
        await eng.put_source("src", ["a"], 1)
        await eng.idle()
        await eng.put_source("src", ["a"], 2)
        await eng.idle()
        return eng.read("out", ["a"])
    v = run(go())
    assert v == {"k": "a", "v": 2}
    assert "removed" not in pubs and pubs.count("settled") == 2
