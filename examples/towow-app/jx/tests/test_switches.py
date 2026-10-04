"""七个消融开关各一个行为测试（Fable-A 第七条）。每个开关关掉运行时接管的一件事。"""
import pytest

from conftest import FnPort, make, run
from test_runtime import (ABS_SRC, DEP_SRC, FILL_SRC, PAIR_SRC, STYLE_A, STYLE_B, _fill_port, _pair_calls,
                          _run_fill)


def test_no_cells_reruns_everything():
    async def go(flags):
        eng = make(DEP_SRC, flags=flags)
        await eng.start()
        for i in range(5):
            await eng.put_source("world", [f"n{i}"], {"t": f"人{i}"})
        await eng.idle()
        c, a = eng.sched.calls, eng.attempts
        await eng.put_source("world", ["n3"], {"t": "人3 改了"})
        await eng.idle()
        return eng.sched.calls - c, eng.attempts - a
    on, off = run(go({})), run(go({"no_cells": True}))
    assert on == (2, 2)
    assert off[1] == 10 and off[0] == 10      # 全部 10 个常驻实例从零重跑，判断全部重发


def test_no_fill_goes_straight_to_pending():
    eng, val, st, evs = _run_fill(True, flags={"no_fill": True})
    assert isinstance(val, dict) and val["kind"] == "unsure"
    assert not [e for e in evs if e["type"] == "disclose_request"]
    assert st["pending"][0]["needed"] is None          # lacks 全空


def test_no_batch_style_ratio():
    a, b = _pair_calls(STYLE_A, {"no_batch": True}), _pair_calls(STYLE_B, {"no_batch": True})
    assert b / a >= 5


META_SRC = '''budget {calls: 1000, cost: 10, depth: 3};
cell node[x] reducer single;
resident 组(x) on [change(node[x])] {
    let n = settled node[x];
    let others = filter(members(node), fn(y) { y != x && !has(n.members, y) });
    map(others, fn(y) {
        let m = peek node[y] ?? {members: [x]};
        let k = join(sort(concat(n.members, m.members)), "+");
        let disjoint = !any(m.members, fn(z) { has(n.members, z) });
        if disjoint && len(n.members) + len(m.members) <= 4 && x < y {
            handle(cut(judge(state({a: n, b: m}), test("合起来能成吗？"))), {
                act: fn() { put node[k] <- {members: sort(concat(n.members, m.members))} },
                ignore: fn() { unit }})
        } else { unit }
    })
}
'''


def test_no_meta_blocks_configs_as_nodes():
    async def go(flags):
        eng = make(META_SRC, flags=flags, port=FnPort(lambda st, q: 0.9))
        await eng.start()
        for x in ["p", "q", "r", "s"]:
            await eng.put_source("node", [x], {"members": [x]})
        await eng.idle()
        assert not eng.errors, eng.errors[:1]
        return sorted(k[0] for k in [tuple(k) for k in eng.keys("node")] if "+" in k[0])
    on, off = run(go({})), run(go({"no_meta": True}))
    assert "p+q" in on and any(len(k.split("+")) == 4 for k in on)    # 构型的构型（层数 2 < 上限 3）
    assert off == []


def test_no_budget_chain_ignores_event_budget():
    src = '''budget {calls: 100, cost: 10};
cell world[a] reducer single;
resident 判(a) on [change(world[a])] {
    let w = settled world[a];
    map(range(5), fn(i) { act(cut(judge(state({w: w, i: i}), test("好？")))) })
}
'''

    async def go(flags):
        eng = make(src, flags=flags)
        await eng.start()
        await eng.put_source("world", ["a"], "x", budget={"calls": 2})
        await eng.idle()
        return eng.sched.calls
    assert run(go({})) == 2                   # 接入事件给 2 次：只收紧到 2
    assert run(go({"no_budget_chain": True})) == 5


def test_no_budget_chain_order():
    """开着时新接入事件的题先发；关掉后按登记先后。"""
    src = '''budget {calls: 100, cost: 10};
cell world[a] reducer single;
resident 判(a) on [change(world[a])] {
    let w = settled world[a];
    act(cut(judge(state({w: w}), test("好？"))))
}
'''

    async def go(flags):
        port = FnPort(lambda st, q: 0.9)
        eng = make(src, flags=flags, port=port)
        eng.prof.concurrency = 1
        await eng.start()
        await eng.put_source("world", ["old"], "旧", budget={"calls": 10})
        await eng.put_source("world", ["new"], "新", budget={"calls": 10})
        await eng.idle()
        return [s["on"]["w"] for s, _ in port.calls]
    assert run(go({}))[0] == "新"
    assert run(go({"no_budget_chain": True}))[0] == "旧"


def test_no_absent_raises():
    port = FnPort(lambda st, q: 0.9)

    async def go(flags):
        eng = make(ABS_SRC, port=port, flags=flags)
        await eng.start()
        port.down = True
        await eng.put_source("world", ["a"], "x")
        await eng.idle(advance=False)
        port.down = False
        return eng.read_host("判", ["a"]), eng.errors
    v, errs = run(go({}))
    assert v["cause"] == "absent" and not errs
    v, errs = run(go({"no_absent": True}))
    assert v is None and errs and "缺席" in errs[0]["error"]


def test_no_deadline_waits_forever():
    src = FILL_SRC.replace("resident 披露", "resident 不理").replace("put reply[b, req.from, req.cat]", "put reply[b, \"x\", req.cat]")

    async def go(flags):
        eng = make(src, port=_fill_port(True), flags=flags)
        await eng.start()
        await eng.put_source("world", ["a1"], "A")
        await eng.put_source("world", ["b1"], "B")
        u = eng._unit(eng.residents["判"], ["a1", "b1"], "test", None)
        eng._mark(u, None, None, force=True)
        await eng.idle()
        return eng.status("判", ["a1", "b1"])
    on, off = run(go({})), run(go({"no_deadline": True}))
    assert on["status"] == "settled" and on["pending"][0]["cause"] == "deadline"
    assert off["status"] == "running" and off["pending"][0]["waiting"] is True


def test_no_deadline_chain_partial():
    """开着时接入事件的截止一到，还没发出的题转 Unsure(deadline)、发布部分值；关掉后照发。"""
    src = '''budget {calls: 100, cost: 10};
cell world[a] reducer single;
resident 判(a) on [change(world[a])] {
    let w = settled world[a];
    map(range(3), fn(i) { cut(judge(state({w: w, i: i}), test("好？"))).kind })
}
'''

    async def go(flags):
        port = FnPort(lambda st, q: 0.9, latency=lambda st, qs: 0.0)
        eng = make(src, port=port, flags=flags)
        await eng.start()
        eng.clock.t = 0.0
        await eng.put_source("world", ["a"], "x", deadline_s=1.0)
        eng.clock.t = 5.0              # 截止已过才静止发出
        await eng.idle()
        return eng.read_host("判", ["a"])
    assert run(go({})) == ["unsure", "unsure", "unsure"]
    assert run(go({"no_deadline": True})) == ["act", "act", "act"]


def test_depth_carried_through_feedback_loop():
    """node[k] → 甲 → adj[k] → 乙（读 node、写 node）→ node[k'] → …：层数沿整圈携带，到上限 3 停。"""
    src = '''budget {calls: 100, cost: 1, depth: 3};
cell node[x] reducer single; cell adj[x] reducer single;
resident 甲(x) on [change(node[x])] { put adj[x] <- {from: x} }
resident 乙(x) on [change(adj[x])] { let n = peek node[x]; put node[x + "'"] <- {n: n} }
'''

    async def go():
        eng = make(src)
        await eng.start()
        await eng.put_source("node", ["a"], {"n": 0})
        await eng.idle()
        return sorted(k[0] for k in eng.keys("node")), eng.errors
    keys, errs = run(go())
    assert not errs
    assert keys == ["a", "a'", "a''"]          # 第 3 层的写被丢弃（Unsure(depth) 进 pending）
