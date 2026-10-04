"""MCP 工具往返（进程内 + 真 HTTP）、/api/state、/events、展示映射、宿主不越界。"""
import asyncio
import json
import socket

import pytest
import uvicorn

from host.index import FragmentIndex
from host.server import Host, agent_id_for
from host.tests.fake_engine import FakeEngine


@pytest.fixture
def anyio_backend():
    return "asyncio"


def X(kind, value=None, p=None, q=None, grade="Answer"):
    return {"exit": True, "kind": kind, "value": value, "p": p, "conf": p, "q": q, "grade": grade, "cause": None}


PACK = {
    "display": "独立插画师，杭州",
    "lang": "zh",
    "signals": [{"text": "最近手头紧，工作室房租又涨了", "tier": "t0", "src": "note"},
                {"text": "睡眠很差", "tier": "t1", "src": "note"}],
    "offers": [{"text": "会画绘本插画", "tier": "t0", "src": "skill"}],
    "catchers": [{"hypo": "有人说孩子想学画画", "can": "周末带孩子画画", "confirm": "对方要的是兴趣课吗？", "tier": "t0", "src": "skill"}],
    "forbids": [{"text": "不做加密货币", "tier": "t0"}],
    "policy": {"t1": "有苗头时可以说近况", "t2": "具体收入", "never": "健康细节"},
}


def make_host(fake_enc):
    eng = FakeEngine()
    return eng, Host(eng, FragmentIndex(fake_enc))


def program_publishes(eng, me, other="a0042"):
    """模拟 net.jpx 已经发布的单元：对方 node、一条 act 边、一个构型与方案、一条补信息请求。"""
    eng.sim_put("node", [me], {"id": me, "kind": "agent", "display": PACK["display"], "signals": PACK["signals"][:1],
                               "members": [me]})
    eng.sim_put("node", [other], {"id": other, "kind": "agent", "display": "心理咨询师老周",
                                  "signals": [{"text": "想多接几个低价个案", "tier": "t0"}],
                                  "offers": [{"text": "低价心理咨询", "tier": "t0"}], "members": [other]})
    a, b = sorted([me, other])
    eng.sim_put("edge", [a, b], {"a": a, "b": b, "holds": True,
                                 "dir": X("pick", "ab" if a == me else "ba", 0.7, "如果他们合作，谁主要帮谁？"),
                                 "form": X("pick", "direct", 0.6, "最可能的合作形式是哪一种？"),
                                 "value": X("at", 1, 0.55, "如果合作成了，对双方的价值有多大？"),
                                 "decisive": {"q": "关于 A 的状态：这个人的状态是不是孤立引起的？", "key": "catcher", "p": 0.83,
                                              "exit": "act", "grade": "Answer", "side": "ab"},
                                 "tier_seen": "t0", "routes": [{"route": "sig→catcher", "mine": "x", "theirs": "y", "sim": 0.8}]})
    eng.sim_put("config", ["k1"], {"id": "k1", "kind": "config", "shape": "ring", "members": [me, other, "a0007"],
                                   "roles": {me: "供给→a0042"}, "hold": X("act", None, 0.71, "这几个人按这个形状合作，能成吗？"),
                                   "value": X("at", 2, 0.5, "这个合作整体价值多大？"),
                                   "lacks": [{"cat": "时间安排", "by": "a0007", "denied": True}]})
    eng.sim_put("plan", ["k1"], {"title": "三方互助环", "summary": "插画换咨询换场地"})
    eng.sim_put("inbox", [me], [{"from": other, "cat": "近况细节", "purpose": "确认是不是孤立引起的",
                                 "q": "最近的状态", "asker_display": "心理咨询师老周"}])


async def roundtrip(client_factory, eng):
    async with client_factory() as c:
        tools = sorted(t.name for t in (await c.list_tools()).tools)
        assert tools == ["towow_inbox", "towow_join", "towow_opportunities", "towow_respond", "towow_spec"]
        assert "towow_spec" in (c.instructions or "") and "不可信" in c.instructions

        def data(res):
            assert not res.is_error, res
            sc = getattr(res, "structured_content", None)
            if sc:
                return sc.get("result", sc) if set(sc) == {"result"} else sc
            return json.loads(res.content[0].text)

        spec = data(await c.call_tool("towow_spec", {}))
        assert "算子包" in spec["spec"] and "network" in spec

        j = data(await c.call_tool("towow_join", {"pack": PACK, "agent_name": "Nature", "host_agent": "Claude Code"}))
        me = j["agent_id"]
        assert me == agent_id_for("Nature", "Claude Code") and j["eta_first_batch_s"] > 0 and j["t0_fragments"] == 3
        w = eng.read("world", [me])
        tok = j["token"]
        # 服务端只收 t0：t1/t2 在 join 时丢掉、不进 world，策略也不交给网络
        assert w["real"] is True and w["host_agent"] == "Claude Code" and j["dropped_non_t0"] >= 1
        assert all(f.get("tier", "t0") == "t0" for k in ("signals", "offers", "catchers", "projects") for f in w[k])
        assert w["policy"] == {}
        # 第三位反驳者的复现：带层的 forbids、自定义顶层字段、owner 详情都不能进 world
        leak = dict(PACK, forbids=[{"text": "公开的禁区", "tier": "t0"}, {"text": "私密禁区", "tier": "t2"}],
                    secret_notes="银行账户 123", owner={"bio": "家人住院"})
        jl = data(await c.call_tool("towow_join", {"pack": leak, "agent_name": "Leak", "host_agent": "Claude Code"}))
        wl = eng.read("world", [jl["agent_id"]])
        assert [f["text"] for f in wl["forbids"]] == ["公开的禁区"] and "secret_notes" not in wl
        assert "bio" not in wl["owner"] and "123" not in json.dumps(wl, ensure_ascii=False)
        # 同名重新 join 要带 token；字符串形式的 pack 也收
        stolen = data(await c.call_tool("towow_join", {"pack": PACK, "agent_name": "Nature", "host_agent": "Claude Code"}))
        assert "error" in stolen
        j2 = data(await c.call_tool("towow_join", {"pack": json.dumps(PACK), "agent_name": "Nature", "host_agent": "Claude Code",
                                                   "token": tok}))
        assert j2["agent_id"] == me and j2["token"] == tok
        bad = data(await c.call_tool("towow_join", {"pack": {"display": "x"}, "agent_name": "n", "host_agent": "h"}))
        assert "error" in bad

        program_publishes(eng, me)
        assert "error" in data(await c.call_tool("towow_opportunities", {"agent_id": me, "token": "guess"}))
        assert "error" in data(await c.call_tool("towow_inbox", {"agent_id": me, "token": ""}))
        o = data(await c.call_tool("towow_opportunities", {"agent_id": me, "token": tok}))
        assert o["n"] == 2 and "不可信" in o["note"]
        ring, pair = o["opportunities"]
        assert ring["shape"] == "ring" and ring["confidence"]["p"] == 0.71 and "能成吗" in ring["confidence"]["q"]
        assert ring["plan"]["title"] == "三方互助环" and ring["value"] == "大" and ring["lacks"][0]["denied"]
        assert pair["shape"] == "pair" and pair["confidence"]["p"] == 0.83 and pair["direction"] == "对方帮你"
        assert pair["confidence"]["kind"] == "act" and "孤立" in pair["confidence"]["q"]
        assert pair["with"][0]["display"] == "心理咨询师老周" and pair["value"] == "中" and pair["form"] == "直接互补"

        ib = data(await c.call_tool("towow_inbox", {"agent_id": me, "token": tok}))
        assert ib["n"] == 1 and ib["requests"][0]["request_id"] == "a0042|近况细节"
        r = data(await c.call_tool("towow_respond", {"agent_id": me, "token": tok, "request_id": "a0042|近况细节", "grant": True,
                                                     "text": "最近接单少，一个人在工作室，挺孤立的"}))
        assert r["ok"] and r["granted"]
        assert eng.read("reply", [me, "a0042", "近况细节"])["granted"] is True
        assert eng.read("unlocked", [me, "a0042"])[0]["tier"] == "t1"
        assert data(await c.call_tool("towow_inbox", {"agent_id": me, "token": tok}))["n"] == 0
        r2 = data(await c.call_tool("towow_respond", {"agent_id": me, "token": tok, "request_id": "a0099|预算或报酬", "grant": False}))
        assert eng.read("reply", [me, "a0099", "预算或报酬"])["denied"] is True and r2["granted"] is False
        return me, tok


@pytest.mark.anyio
async def test_mcp_inprocess_roundtrip(fake_enc):
    from mcp import Client
    eng, host = make_host(fake_enc)
    mcp = host.build_mcp()
    await roundtrip(lambda: Client(mcp), eng)
    # 宿主只写源单元：world、unlocked、reply，写者都是 host
    assert {(w[0], w[1]) for w in eng.writes if w[0] == "host"} == {("host", "world"), ("host", "unlocked"), ("host", "reply")}


def free_port():
    s = socket.socket()
    s.bind(("127.0.0.1", 0))
    p = s.getsockname()[1]
    s.close()
    return p


@pytest.mark.anyio
async def test_http_mcp_events_and_state(fake_enc):
    """真 HTTP：claude mcp add --transport http 走的就是这条路（挂载路径、lifespan、DNS 保护）。"""
    import httpx
    import websockets
    from mcp import Client

    eng, host = make_host(fake_enc)
    app = host.build_app()
    port = free_port()
    server = uvicorn.Server(uvicorn.Config(app, host="127.0.0.1", port=port, log_level="warning", lifespan="on"))
    task = asyncio.create_task(server.serve())
    for _ in range(100):
        if server.started:
            break
        await asyncio.sleep(0.05)
    try:
        url = f"http://localhost:{port}"
        async with websockets.connect(f"ws://localhost:{port}/events") as ws:
            snap = json.loads(await ws.recv())
            assert snap["type"] == "snapshot" and snap["nodes"] == []
            me, tok = await roundtrip(lambda: Client(f"{url}/mcp"), eng)
            seen = []
            while True:
                try:
                    seen.append(json.loads(await asyncio.wait_for(ws.recv(), 1.0)))
                except asyncio.TimeoutError:
                    break
            types = {e["type"] for e in seen}
            assert {"node_join", "edge", "config", "plan", "disclose"} <= types   # disclose_request 由引擎发（去重、同一 id），不经映射
            nj = next(e for e in seen if e["type"] == "node_join" and e["id"] == me)
            assert nj["host_agent"] == "Claude Code" and len(nj.get("vec3") or [0, 0, 0]) == 3
            ed = next(e for e in seen if e["type"] == "edge")
            assert ed["state"] == "new" and ed["conf"] == 0.83 and ed["dir"] in ("a>b", "b>a")
        async with httpx.AsyncClient() as h:
            st = (await h.get(f"{url}/api/state")).json()
            assert st["stats"]["agents"] == 2 and len(st["edges"]) == 1 and st["configs"][0]["conf"] == 0.71
            assert (await h.get(f"{url}/api/opportunities/{me}")).json()["n"] == 2
            assert (await h.post(f"{url}/api/leave/{me}")).status_code == 403          # 没有 token 不能让别人离开
            assert (await h.get(f"{url}/api/inbox/{me}")).status_code == 403
            assert (await h.post(f"{url}/api/leave/{me}", headers={"x-towow-token": tok})).json()["ok"]
            assert eng.read("world", [me]) is None
    finally:
        server.should_exit = True
        await task


@pytest.mark.anyio
async def test_do_actions_registered(fake_enc):
    eng, host = make_host(fake_enc)
    acts = eng.actions
    assert set(acts) == {"index_put", "route", "route_offers", "graph_local"}
    assert acts["index_put"][1] is False                     # 有副作用
    assert all(acts[n][2] is None for n in acts)             # 不挂 node 族依赖（见 README：每次接入成本与 N 无关）
    await acts["index_put"][0]("a", {"signals": [{"text": "缺钱", "tier": "t0"}], "members": ["a"]})
    await acts["index_put"][0]("b", {"catchers": [{"hypo": "有人缺钱", "tier": "t0"}], "members": ["b"]})
    got = []
    host.hub.emit = lambda ev: got.append(ev)
    r = await acts["route"][0]("a", {"signals": [{"text": "缺钱", "tier": "t0"}]}, 5)
    assert r[0]["peer"] == "b" and got[0]["type"] == "probe" and got[0]["to"] == ["b"]
    g = acts["graph_local"][0]("a", [{"a": "a", "b": "b", "dir": X("pick", "ba"), "form": X("pick", "direct"),
                                      "holds": True}], {})
    assert g[0]["shape"] == "pair"


def test_host_source_has_no_scheduling():
    """B1-6 粗检：宿主源码里没有常驻/排队/合批/重算/预算/截止的实现痕迹（只允许透传参数名）。"""
    import pathlib
    import re
    served = ("server.py", "views.py", "index.py", "graph.py", "cli.py")     # 驱动与压测脚本（simulate、bench）供时钟，不在此列
    src = "\n".join((pathlib.Path(__file__).parents[1] / f).read_text() for f in served)
    src = "\n".join(line for line in src.splitlines() if "# 展示时钟" not in line)   # 唯一允许的定时：每秒 stats
    for pat in [r"asyncio\.sleep\(", r"create_task\(", r"heapq", r"PriorityQueue", r"\bretry\b", r"while True:\s*\n\s*await asyncio"]:
        hits = [m.group(0) for m in re.finditer(pat, src)]
        assert not hits, (pat, hits)


def test_mapper_spotlight_invalidate_vec3(fake_enc):
    from host import views
    ix = FragmentIndex(fake_enc)
    ix.index_put("a", {"signals": [{"text": "缺钱", "tier": "t0"}], "members": ["a"]})
    ix.index_put("b", {"offers": [{"text": "借钱", "tier": "t0"}], "members": ["b"]})
    m = views.EventMapper(None, ix.vec3)
    P = lambda cell, key, val, prev=None: m.map({"type": "publish", "t": 1.0, "cell": cell, "key": key,
                                                 "value": val, "prev": prev, "status": "settled"})
    out = P("edge", ["a", "b"], {"holds": True, "dir": X("pick", "ab"), "form": X("pick", "direct"),
                                 "decisive": {"q": "q", "p": 0.9, "exit": "act"}})
    assert [e["why"] for e in out if e["type"] == "spotlight"] == ["join_first_opp", "join_first_opp"]
    out = P("edge", ["a", "b"], {"holds": True, "decisive": {"p": 0.95}}, {"holds": True, "decisive": {"p": 0.9}})
    assert not [e for e in out if e["type"] == "spotlight"]
    out = P("config", ["cfg:a+b"], {"shape": "pair", "members": ["a", "b"], "hold": X("act", p=0.8)})
    assert [e["why"] for e in out if e["type"] == "spotlight"] == ["config_formed"]
    nj = P("node", ["cfg:a+b"], {"kind": "config", "members": ["a", "b"], "shape": "pair"})[0]
    assert nj["type"] == "node_join" and nj["config"] == "cfg:a+b" and len(nj["vec3"]) == 3
    out = P("config", ["cfg:c+cfg:a+b"], {"shape": "pair", "members": ["c", "cfg:a+b"]})
    assert {e["why"] for e in out if e["type"] == "spotlight"} == {"meta_formed", "join_first_opp"}
    assert P("plan", ["cfg:a+b"], {"title": "t"})[1]["why"] == "plan_ready"
    assert P("reply", ["b", "a", "近况"], {"granted": True})[0]["why"] == "disclose_granted"
    inv = m.map({"type": "invalidate", "t": 2.0, "cause": "join:a", "n_judgments": 3})[0]
    assert inv["cause"] == "join" and inv["id"] == "a" and {"a", "b", "cfg:a+b"} <= set(inv["ids"])
    assert m.map({"type": "invalidate", "cause": "remove:world"})[0]["ids"] == []
    assert P("unlocked", ["a", "b"], [{"text": "x", "tier": "t2"}])[0]["tier"] == 2
    for e in [x for x in out if x["type"] == "spotlight"]:
        assert e["why"] in views.SPOTLIGHT_WHY
