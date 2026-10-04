"""引擎行为：cut 三值、合批、标脏、两种读、占用、补信息链、预算、缺席。"""
import json

from conftest import FnPort, make, run
from jx.core import Exit, Reading, cut, measure, select
from jx.core import test as q_test


# ---------------------------------------------------------------- cut 三值

def test_cut_three_values():
    q = q_test("x?")
    assert cut(Reading(q, p=0.8)).kind == "act"
    assert cut(Reading(q, p=0.2)).kind == "ignore"
    assert cut(Reading(q, p=0.5)).cause == "tie"
    e = cut(Reading(q, p=0.6), {"declare": {"hi": 0.7, "lo": 0.3}})
    assert e.kind == "unsure" and e.cause == "band" and e.grade == "Declared"
    assert cut(Reading(q, p=0.75), {"hi": 0.7, "lo": 0.3}).kind == "act"
    s = select("哪个？", {"a": "甲", "b": "乙"})
    assert cut(Reading(s, dist=(("a", 0.3), ("b", 0.7)))).value == "b"
    assert cut(Reading(s, dist=(("a", 0.5), ("b", 0.5)))).cause == "tie"
    m = measure("多大？", ["小", "中", "大"])
    assert cut(Reading(m, dist=(0.1, 0.2, 0.7))).value == 2
    assert cut(None).cause == "absent"
    d = cut(Reading(q, p=0.8, by="enc"))
    assert d.kind == "unsure" and d.cause == "absent" and d.lean == "act" and d.grade == "Degraded"


def test_lazy_judge_one_call_per_state():
    src = '''budget {calls: 10, cost: 1};
let s = state(mat("材料"));
let r = judge(s, [test("甲？"), test("乙？"), select("丙？", {a: "A", b: "B"})]);
let t = judge(state(mat("另一份")), test("丁？"));
{x: act(cut(r[0])), y: act(cut(r[1])), z: cut(r[2]).value, w: act(cut(t))}'''
    port = FnPort(lambda st, q: 0.9)

    async def go():
        eng = make(src, port=port)
        await eng.start()
        r = await eng.report()
        return eng, r
    eng, r = run(go())
    assert eng.sched.calls == 2 and eng.sched.questions == 4      # 同一 state 的三道题一次调用
    assert r["x"] is True


def test_handle_arms_and_unsure_arm():
    src = '''budget {calls: 10, cost: 1};
let e = cut(judge(state(mat("m")), test("q?")), {declare: {hi: 0.7, lo: 0.3}});
handle(e, {act: fn() { "A" }, ignore: fn() { "I" }, unsure: fn(u) { consume(u, "drop"); "U:" + u.cause }})'''
    for p, want in [(0.9, "A"), (0.1, "I"), (0.5, "U:band")]:
        async def go():
            eng = make(src, port=FnPort(lambda st, q, p=p: p))
            await eng.start()
            return await eng.report()
        assert run(go()) == want


# ---------------------------------------------------------------- 合批：两种写法

PAIR_SRC = '''budget {calls: 1000, cost: 10};
cell world[a] reducer single;
let QS = [test("甲？"), test("乙？"), test("丙？"), test("丁？"), test("戊？"), test("己？")];
resident 配对(a) on [change(world[a])] {
    let me = settled world[a];
    let others = filter(members(world), fn(b) { b != a });
    map(others, fn(b) { WRITE(a, b, me, peek world[b]) })
}
'''
STYLE_A = '''fn WRITE(a, b, me, other) {
    let s = state({a: me, b: other});
    let rs = judge(s, QS);
    map(rs, fn(r) { act(cut(r)) })
}'''
STYLE_B = '''fn WRITE(a, b, me, other) {
    let s = state({a: me, b: other});
    let r0 = cut(judge(s, QS[0]));
    let x0 = if act(r0) { 1 } else { 0 };
    let r1 = cut(judge(s, QS[1]));
    let x1 = if act(r1) { 1 } else { 0 };
    let r2 = cut(judge(s, QS[2]));
    let x2 = if act(r2) { 1 } else { 0 };
    let r3 = cut(judge(s, QS[3]));
    let x3 = if act(r3) { 1 } else { 0 };
    let r4 = cut(judge(s, QS[4]));
    let x4 = if act(r4) { 1 } else { 0 };
    let r5 = cut(judge(s, QS[5]));
    let x5 = if act(r5) { 1 } else { 0 };
    [x0, x1, x2, x3, x4, x5]
}'''


def _pair_calls(style, flags=None, n=8):
    async def go():
        eng = make(PAIR_SRC.replace("WRITE", "w_") + style.replace("WRITE", "w_"), flags=flags)
        await eng.start()
        for i in range(n):
            await eng.put_source("world", [f"a{i}"], {"id": f"a{i}", "text": f"第 {i} 个人"})
        await eng.idle()
        assert not eng.errors, eng.errors[:1]
        return eng.sched.calls
    return run(go())


def test_batching_two_styles_ratio():
    a, b = _pair_calls(STYLE_A), _pair_calls(STYLE_B)
    assert a > 0
    assert b / a <= 1.2, (a, b)


# ---------------------------------------------------------------- 标脏：只重算依赖者

DEP_SRC = '''budget {calls: 1000, cost: 10};
cell world[a] reducer single;
resident 看(a) on [change(world[a])] {
    let w = settled world[a];
    act(cut(judge(state(mat(w)), test("这个人在找合作吗？"))))
}
resident 对(a) on [change(world[a])] {
    let w = settled world[a];
    let nb = peek world["n0"];
    act(cut(judge(state({me: w, n0: nb}), test("他和 n0 合得来吗？"))))
}
'''


def test_dirty_only_dependents():
    async def go():
        eng = make(DEP_SRC)
        await eng.start()
        for i in range(5):
            await eng.put_source("world", [f"n{i}"], {"t": f"人{i}"})
        await eng.idle()
        base_calls, base_att = eng.sched.calls, eng.attempts
        await eng.put_source("world", ["n3"], {"t": "人3 改了"})
        await eng.idle()
        d1 = (eng.sched.calls - base_calls, eng.attempts - base_att)
        c2, a2 = eng.sched.calls, eng.attempts
        await eng.put_source("world", ["n0"], {"t": "人0 改了"})
        await eng.idle()
        d2 = (eng.sched.calls - c2, eng.attempts - a2)
        c3, a3 = eng.sched.calls, eng.attempts
        await eng.put_source("world", ["n2"], {"t": "人2"})          # 值没变：第二层截断，什么都不重算
        await eng.idle()
        d3 = (eng.sched.calls - c3, eng.attempts - a3)
        return d1, d2, d3
    d1, d2, d3 = run(go())
    assert d1 == (2, 2)          # 看(n3)、对(n3) 各重算一次，各一道新题
    assert d2[1] == 6 and d2[0] == 6   # n0 被 5 个 对(…) 读、加 看(n0)：重算 6 个，新题 6 道
    assert d3 == (0, 0)


# ---------------------------------------------------------------- 两种读

READ_SRC = '''budget {calls: 1000, cost: 10};
cell src[k] reducer single;
resident 慢(k) on [change(src[k])] {
    let v = settled src[k];
    let rs = judge(state(mat(v)), [test("一？"), test("二？")]);
    {k: k, a: act(cut(rs[0])), b: act(cut(rs[1]))}
}
resident 等(k) on [change(src[k])] {
    let x = settled 慢[k];
    if is_pend(x) { "等着" } else { "拿到:" + str(x.a) }
}
'''


def test_settled_waits_and_reruns():
    async def go():
        eng = make(READ_SRC)
        seen = []
        eng.bus.subscribe(lambda e: seen.append(e) if e["type"] == "publish" and e["cell"] == "等" else None)
        await eng.start()
        await eng.put_source("src", ["x"], "材料")
        await eng.idle()
        return [(e["status"], e["value"]) for e in seen], eng.read_host("等", ["x"], "settled")
    seq, final = run(go())
    assert seq[0] == ("running", "等着")          # 上游没定下：发布「进行中」，登记等待
    assert seq[-1][0] == "settled" and final.startswith("拿到:")   # 上游定下后重来


def test_peek_reads_running_version():
    src = '''budget {calls: 1000, cost: 10};
cell src[k] reducer single; cell reply[k] reducer single;
resident 上(k) on [change(src[k])] {
    let r = settled reply[k];
    {k: k, have: if is_pend(r) { "还没有" } else { r }}
}
resident 下(k) on [change(src[k])] {
    let x = peek 上[k];
    let st = status(上, [k]);
    {saw: x, status: st.status}
}
'''

    async def go():
        eng = make(src)
        await eng.start()
        await eng.put_source("src", ["x"], 1)
        await eng.idle()
        mid = eng.read_host("下", ["x"], "peek")
        await eng.put_source("reply", ["x"], "答复")
        await eng.idle()
        return mid, eng.read_host("下", ["x"], "peek"), eng.status("上", ["x"])["status"]
    mid, after, st = run(go())
    assert mid["status"] == "running" and mid["saw"]["have"] == "还没有"   # peek 读到进行中的版本
    assert after["saw"]["have"] == "答复" and st == "settled"


# ---------------------------------------------------------------- 占用

def test_claim_conflict():
    src = '''budget {calls: 10, cost: 1};
cell want[a] reducer single; cell seat[s] reducer claim;
resident 抢(a) on [change(want[a])] {
    let w = settled want[a];
    handle(claim seat[w] <- a, {act: fn() { "得到" }, unsure: fn(u) { consume(u, "drop"); u.cause + ":" + u.needed }})
}
'''

    async def go():
        eng = make(src, seed=3)
        await eng.start()
        await eng.put_source("want", ["p1"], "s1")
        await eng.put_source("want", ["p2"], "s1")
        await eng.idle()
        a, b = eng.read_host("抢", ["p1"]), eng.read_host("抢", ["p2"])
        # p1 改主意，释放 s1 → p2 被标脏重跑，拿到
        await eng.put_source("want", ["p1"], "s9")
        await eng.idle()
        return a, b, eng.read_host("抢", ["p2"])
    a, b, b2 = run(go())
    assert sorted([a, b]) == ["claim_conflict:抢(p1)", "得到"] or sorted([a, b]) == ["claim_conflict:抢(p2)", "得到"]
    assert b2 == "得到"


# ---------------------------------------------------------------- 补信息链

FILL_SRC = '''budget {calls: 1000, cost: 10};
cell world[a] reducer single; cell inbox[b] reducer union; cell reply[b, a, cat] reducer single;
let Q = test("B 能帮上 A 吗？", "catch", ["时间安排", "预算或报酬"]);
resident 判(a, b) on [change(world[a])] {
    let s = state({A: mat(peek world[a], {origin: a}), B: mat(peek world[b], {origin: b})}, {owners: [a, b]});
    unsure_source({from: a});
    let e = cut(judge(s, Q), {declare: {hi: 0.7, lo: 0.3}});
    handle(e, {act: fn() { "成" }, ignore: fn() { "不成" }})
}
resident 披露(b) on [change(inbox[b])] {
    map(peek inbox[b], fn(req) {
        let ok = cut(judge(state({want: req.cat, from: req.from}), test("给吗？", "disclose")));
        handle(ok, {act: fn() { put reply[b, req.from, req.cat] <- {granted: true, text: "下周都有空"} },
                    ignore: fn() { put reply[b, req.from, req.cat] <- {denied: true, cat: req.cat, by: b} }})
    })
}
'''


def _fill_port(grant: bool, comp_label="c0"):
    def fn(st, q):
        t = json.dumps(st, ensure_ascii=False)
        if q.text == "给吗？":
            return 0.9 if grant else 0.1
        if q.op == "select":
            return {lab: (0.8 if lab == comp_label else 0.2 / max(1, len(q.criteria) - 1)) for lab in q.criteria}
        if q.text.startswith("B 能帮上"):
            return 0.9 if "下周都有空" in t else 0.5
        return 0.5
    return FnPort(fn)


def _run_fill(grant, flags=None):
    async def go():
        eng = make(FILL_SRC, port=_fill_port(grant), flags=flags)
        evs = []
        eng.bus.subscribe(lambda e: evs.append(e) if e["type"] in ("unsure_route", "disclose_request") else None)
        await eng.start()
        await eng.put_source("world", ["a1"], "A 在找周末能带孩子的人")
        await eng.put_source("world", ["b1"], "B 是大学生，学教育")
        u = eng._unit(eng.residents["判"], ["a1", "b1"], "test", None)
        eng._mark(u, None, None, force=True)
        await eng.idle()
        return eng, eng.read_host("判", ["a1", "b1"], "settled"), eng.status("判", ["a1", "b1"]), evs
    return run(go())


def test_fill_chain_grant_rejudge():
    eng, val, st, evs = _run_fill(True)
    assert val == "成"                     # 问缺哪类 → put inbox[b1] → settled reply → 补进 ctx 重判 → act
    routes = [e["route"] for e in evs if e["type"] == "unsure_route"]
    assert "disclose_request" in routes and "refine" in routes
    assert any(e["type"] == "disclose_request" and e["status"] == "granted" and e["category"] == "时间安排" for e in evs)
    assert st["pending"] == []


def test_fill_chain_denied_goes_to_pending():
    eng, val, st, evs = _run_fill(False)
    assert isinstance(val, dict) and val["kind"] == "unsure"     # 没有对应臂：出口随值转交
    p = st["pending"][0]
    assert p["cause"] == "band" and "时间安排" in p["needed"] and "拒绝" in p["needed"] and p["ask_to"] == ["b1"]


def test_fill_waits_until_deadline():
    src = FILL_SRC.replace("resident 披露", "resident 不理").replace("put reply[b, req.from, req.cat]", "put reply[b, \"x\", req.cat]")

    async def go():
        eng = make(src, port=_fill_port(True))
        await eng.start()
        await eng.put_source("world", ["a1"], "A")
        await eng.put_source("world", ["b1"], "B")
        u = eng._unit(eng.residents["判"], ["a1", "b1"], "test", None)
        eng._mark(u, None, None, force=True)
        await eng.idle(advance=False)
        mid = eng.status("判", ["a1", "b1"])
        await eng.idle()           # 虚拟时钟拨到截止
        return mid, eng.status("判", ["a1", "b1"])
    mid, end = run(go())
    assert mid["status"] == "running" and mid["pending"][0]["waiting"] is True
    assert end["status"] == "settled" and end["pending"][0]["cause"] == "deadline"
    assert "截止" in end["pending"][0]["needed"]


# ---------------------------------------------------------------- 预算只收紧

def test_budget_only_tightens():
    src = '''budget {calls: 5, cost: 10};
cell world[a] reducer single;
resident 判(a) on [change(world[a])] {
    let w = settled world[a];
    let rs = map(range(4), fn(i) { judge(state({w: w, i: i}), test("好？")) });
    map(rs, fn(r) { handle(cut(r), {act: fn() { 1 }, ignore: fn() { 0 }}) })
}
'''

    async def go():
        eng = make(src)
        await eng.start()
        await eng.put_source("world", ["a"], "x", budget={"calls": 100})     # 事件预算大于整场：取整场剩余
        await eng.idle()
        r1 = eng.read_host("判", ["a"])
        c1 = eng.sched.calls
        await eng.put_source("world", ["b"], "y", budget={"calls": 2})
        await eng.idle()
        return r1, c1, eng.sched.calls, eng.read_host("判", ["b"]), eng.status("判", ["b"]), len(eng.errors)
    r1, c1, c2, rb, st, nerr = run(go())
    assert c1 == 4 and all(x in (0, 1) for x in r1)
    assert c2 - c1 == 1                       # 整场剩 1 次，这条链给了 2 次 → 收紧到 1
    assert sum(1 for x in rb if isinstance(x, dict) and x.get("cause") == "budget") == 3
    assert nerr == 0                           # 用完只停发，不停程序
    assert sum(1 for p in st["pending"] if p["cause"] == "budget") == 3


# ---------------------------------------------------------------- 判断力缺席

ABS_SRC = '''budget {calls: 100, cost: 10};
cell world[a] reducer single;
resident 判(a) on [change(world[a])] {
    let w = settled world[a];
    let e = cut(judge(state({a: w, b: "同样喜欢做陶艺的人"}), test("他们合得来吗？")));
    {kind: e.kind, cause: e.cause, lean: e.lean, grade: e.grade}
}
'''


def test_absent_degrades_then_rejudges():
    port = FnPort(lambda st, q: 0.9)

    async def go():
        eng = make(ABS_SRC, port=port)
        await eng.start()
        port.down = True
        await eng.put_source("world", ["a"], "喜欢做陶艺的人", deadline_s=None)
        await eng.idle(advance=False)
        during = eng.read_host("判", ["a"])
        port.down = False
        await eng.idle()            # 时钟拨到探测点：消费过降级读数的单元重判
        return during, eng.read_host("判", ["a"])
    during, after = run(go())
    assert during["kind"] == "unsure" and during["cause"] == "absent" and during["grade"] == "Degraded"
    assert during["lean"] in ("act", "ignore")
    assert after["kind"] == "act" and after["grade"] == "Answer"


def test_latency_timeout_unsure():
    src = '''budget {calls: 100, cost: 10, latency_p95: 2};
let e = cut(judge(state(mat("x")), test("q?")));
{kind: e.kind, cause: e.cause}'''
    port = FnPort(lambda st, q: 0.9, latency=lambda st, qs: 5.0)

    async def go():
        eng = make(src, port=port)
        await eng.start()
        return await eng.report()
    assert run(go()) == {"kind": "unsure", "cause": "latency"}


def test_near_boundary_read_trigger():
    """无线是非题读数落在边界带内（画像 near_band）且有地方补：先补再判，出口按补后读数定。"""
    src = FILL_SRC.replace(', {declare: {hi: 0.7, lo: 0.3}}', '')

    def fn(st, q):
        t = json.dumps(st, ensure_ascii=False)
        if q.text == "给吗？":
            return 0.9
        if q.op == "select":
            return {lab: (0.8 if lab == "c0" else 0.1) for lab in q.criteria}
        return 0.2 if "下周都有空" in t else 0.55        # 带内偏「成」，补完后明确「不成」

    async def go():
        eng = make(src, port=FnPort(fn))
        await eng.start()
        await eng.put_source("world", ["a1"], "A")
        await eng.put_source("world", ["b1"], "B")
        u = eng._unit(eng.residents["判"], ["a1", "b1"], "test", None)
        eng._mark(u, None, None, force=True)
        await eng.idle()
        return eng.read_host("判", ["a1", "b1"], "settled")
    assert run(go()) == "不成"


def test_mini_net_fixture_run():
    import subprocess
    import sys as _s
    from conftest import ROOT
    out = subprocess.run([_s.executable, "-m", "jx", "run", "jx/examples/mini-net.jpx",
                          "--world", "jx/examples/mini-net.world.json", "--events", "jx/examples/mini-net.events.jsonl",
                          "--quiet"], cwd=ROOT, capture_output=True, text=True, timeout=120)
    d = json.loads(out.stdout)
    assert not d["errors"]
    ph = {p["phase"]: p for p in d["phases"]}
    assert ph["world"]["calls"] > 0
    # 增量：一个 agent 多说一层只重算读到它的程序；调用数远小于从零
    assert ph["a19 多说了一层"]["calls"] * 5 < ph["world"]["calls"]
    assert ph["a21 接入"]["calls"] * 5 < ph["world"]["calls"]


def test_budget_holds_with_concurrent_async_calls():
    """并发在飞的调用也不超账：发出即预留。"""
    import asyncio as _a

    class SlowPort:
        sync = False

        def __init__(self):
            self.n = 0

        async def call(self, state, qs):
            self.n += 1
            await _a.sleep(0.01)
            return [{"type": "noul", "noul": 0.9} for _ in qs]
    src = '''budget {calls: 7, cost: 10};
map(range(20), fn(i) { cut(judge(state({i: i}), test("好？"))).kind })'''
    port = SlowPort()

    async def go():
        from jx.engine import Engine
        eng = Engine.from_source(src, ports={"judge": port})
        await eng.start()
        r = await eng.report()
        await eng.idle()
        return r
    r = run(go())
    assert port.n == 7 and r.count("act") == 7 and r.count("unsure") == 13
