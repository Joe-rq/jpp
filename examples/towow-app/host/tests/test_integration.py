"""宿主 + 真引擎（jx.engine）+ app/net.jpx，判断端口用引擎的离线伪读数端口（不花钱）。
宿主侧的断言：接入写进 world、do 动作被调用、事件与机会渲染不报错。网络程序本身的正确性归 lang 的测试。"""
import json

import pytest

from host.index import FragmentIndex
from host.server import Host
from host.tests.test_server import PACK
from host import views

engine_mod = pytest.importorskip("jx.engine")


@pytest.fixture
def anyio_backend():
    return "asyncio"


@pytest.mark.anyio
async def test_join_three_agents_on_real_engine(fake_enc):
    eng = engine_mod.Engine.load("app/net.jpx", seed=0)
    host = Host(eng, FragmentIndex(fake_enc))
    calls = {"index_put": 0, "route": 0}
    def counted(f, n):
        async def g(*a):
            calls[n] += 1
            return await f(*a)
        return g

    for name in calls:
        eng.actions[name]["fn"] = counted(eng.actions[name]["fn"], name)
    evs = []
    host.hub.emit = lambda ev: evs.append(ev)
    await eng.start()
    ids = []
    for name in ["甲", "乙", "丙"]:
        p = json.loads(json.dumps(PACK))
        p["display"] = f"{name}，插画师"
        ids.append((await host.join(p, name, "test"))["agent_id"])
    await eng.idle(timeout=60)
    try:
        assert eng.read_host("world", [ids[0]])["real"] is True
        # 宿主侧：do 动作被调用、node_join 映射出来、机会与快照渲染不报错
        assert calls["index_put"] >= 3 and calls["route"] >= 3
        assert sum(e["type"] == "node_join" for e in evs) >= 3
        o = views.opportunities(host.eng, ids[0])
        assert o["published"] is True
        snap = host.state()
        assert snap["stats"]["agents"] >= 3
        host_errors = [e for e in eng.errors if "host/" in str(e)]
        assert not host_errors, host_errors[0]
        if eng.errors:
            pytest.xfail(f"网络程序在引擎上还有错误（lang 在做）：{str(eng.errors[0])[:300]}")
        assert len(eng.keys("edge")) >= 1          # 两两判断跑过（成不成立看伪读数，不断言）
    finally:
        await eng.stop()
