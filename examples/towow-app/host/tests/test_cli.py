import json

import pytest

from host.cli import opp_line, opp_sig
from host.tests.fake_engine import FakeEngine


@pytest.fixture
def anyio_backend():
    return "asyncio"


def test_opp_line_and_sig():
    o = {"id": "k1", "shape": "ring", "status": "settled", "my_role": "供给→b",
         "with": [{"display": "心理咨询师老周"}, {"id": "a7"}],
         "confidence": {"kind": "act", "p": 0.714, "q": "这几个人按这个形状合作，能成吗？"},
         "lacks": [{"cat": "时间安排", "denied": True}], "pending": [], "plan": {"title": "三方互助环"}}
    line = opp_line(o)
    for s in ("ring", "心理咨询师老周", "a7", "p=0.71", "能成吗", "时间安排（拒）", "三方互助环"):
        assert s in line
    o2 = json.loads(json.dumps(o))
    o2["confidence"]["p"] = 0.8
    assert opp_sig(o) != opp_sig(o2)
    assert "未判" in opp_line({"id": "x", "shape": "pair", "with": []})


@pytest.mark.anyio
async def test_preload_order_and_resume(tmp_path):
    from host.simulate import preload, preload_order
    for i in range(5):
        (tmp_path / f"a{i}.json").write_text(json.dumps({"id": f"a{i}", "signals": [{"text": "x", "tier": "t0"}],
                                                         "owner_display": f"人{i}"}), encoding="utf-8")
    eng = FakeEngine()
    order, _ = preload_order(str(tmp_path), 3, seed=1)
    done = await preload(eng, str(tmp_path), n=3, seed=1, log=lambda m: None)
    assert done == order and [w[2][0] for w in eng.writes] == order
    assert eng.read("world", [order[0]])["real"] is False
    n_writes = len(eng.writes)
    await preload(eng, str(tmp_path), n=3, seed=1, log=lambda m: None)    # 重启续载：已在的不重写
    assert len(eng.writes) == n_writes


def test_tighten_cost_only_tightens():
    from host.server import tighten_cost
    from jx.engine import Engine
    e = Engine.from_source("budget {calls: 10, cost: 4.5};")
    assert tighten_cost(e, 4.0) == 4.0 and e.account.cap_cost == 4.0
    assert tighten_cost(e, 9.0) == 4.0          # 不放宽
    assert tighten_cost(e, None) is None and e.account.cap_cost == 4.0
    e2 = Engine.from_source("cell x[a] reducer single;")   # 程序没写花费上限
    assert tighten_cost(e2, 0.5) == 0.5
