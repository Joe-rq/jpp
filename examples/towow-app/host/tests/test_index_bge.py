"""真 bge-m3 上的路由语义（slow）：信号—catcher 同空间路由能把表面措辞不同、意思接得住的片段排到前面。
运行：.venv/bin/python -m pytest host/tests/test_index_bge.py -m slow"""
import os
import tempfile

import pytest

from host.index import FragmentIndex

pytestmark = pytest.mark.slow


@pytest.fixture(scope="module")
def bge():
    pytest.importorskip("sentence_transformers")       # 没装 [bge] 可选依赖时跳过
    from jx.ports.enc import EncPort
    return EncPort(os.path.join(tempfile.mkdtemp(), "enc.sqlite"))


def A(i, **kw):
    n = {"id": i, "members": [i]}
    for k, v in kw.items():
        n[k] = [({"hypo": t, "tier": "t0"} if k == "catchers" else {"text": t, "tier": "t0"}) for t in v]
    return n


def test_signal_to_catcher_semantic(bge):
    ix = FragmentIndex(bge)
    ix.index_put("counsel", A("counsel", catchers=["有人说最近钱很紧、一个人扛着，压力大到睡不着"],
                              offers=["提供低价心理咨询"]))
    ix.index_put("granny", A("granny", catchers=["有人需要出门时有人陪着，最好会开车"],
                             signals=["孙子上大学走了，家里很冷清"]))
    ix.index_put("baker", A("baker", catchers=["有人想学做面包"], offers=["周末开烘焙课"]))
    ix.index_put("cross", A("cross", catchers=["Someone says money is tight lately and they feel isolated"]))
    q = A("me", signals=["这个月房租又涨了，手头紧得很，晚上老失眠"])
    r = ix.route("me", q, 10)
    top = [z["peer"] for z in r]
    assert top[0] in ("counsel", "cross")
    assert set(top[:2]) == {"counsel", "cross"}                 # 跨语言也接得住
    assert top.index("baker") > 1
    # 我接得住谁：老人的冷清 → 我的 catcher「有人孤单想找人说话」
    q2 = A("me2", catchers=["有人说身边人都走了，一个人很孤单，想找人说说话"])
    r2 = ix.route("me2", q2, 10)
    assert r2[0]["peer"] == "granny" and r2[0]["routes"][0]["route"] == "catcher→sig"
