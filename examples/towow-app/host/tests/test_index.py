from host.index import FragmentIndex, node_frags


def agent(i, signals=(), offers=(), catchers=(), derived=(), members=None):
    return {"id": i, "kind": "agent", "signals": [{"text": s, "tier": "t0"} for s in signals],
            "offers": [{"text": s, "tier": "t0"} for s in offers],
            "catchers": [{"hypo": h, "can": "可以帮", "confirm": "是吗？", "tier": "t0"} for h in catchers],
            "derived": list(derived), "members": members or [i]}


A1 = dict(signals=["最近手头紧，工作室房租又涨了"], offers=["会做网站前端"])
A4 = dict(signals=["想找人做一个网站前端"], catchers=["孩子暑假没人带，想找人临时照看"])


def build(fake_enc):
    ix = FragmentIndex(fake_enc, cap=8)          # 小容量，逼出 resize
    ix.index_put("a1", agent("a1", **A1))
    ix.index_put("a2", agent("a2", catchers=["有人说最近手头紧、房租涨了，需要低价接点活"], offers=["有一间画室工作日空着"]))
    ix.index_put("a3", agent("a3", signals=["孩子暑假没人带"], offers=["会做网站前端开发"]))
    ix.index_put("a4", agent("a4", **A4))
    return ix


def peers(res):
    return [r["peer"] for r in res]


def test_tier_filter():
    n = {"signals": [{"text": "公开", "tier": "t0"}, {"text": "私密", "tier": "t1"}, "字符串片段"]}
    assert node_frags(n, "signals") == ["公开", "字符串片段"]


def test_t1_never_indexed(fake_enc):
    ix = FragmentIndex(fake_enc)
    ix.index_put("p", {"signals": [{"text": "私密的事", "tier": "t1"}], "offers": [{"text": "公开能力", "tier": "t0"}]})
    assert ix.stats()["vectors"]["signals"] == 0 and ix.stats()["vectors"]["offers"] == 1


def test_four_routes_and_exclude_self(fake_enc):
    ix = build(fake_enc)
    r = ix.route("a1", agent("a1", **A1), 5)
    assert "a1" not in peers(r)
    assert r[0]["peer"] == "a2"
    assert any(z["route"] == "sig→catcher" for z in r[0]["routes"])
    a4 = next(z for z in r if z["peer"] == "a4")
    assert any(z["route"] == "offer→sig" for z in a4["routes"])
    r4 = ix.route("a4", agent("a4", **A4), 5)
    a3 = next(z for z in r4 if z["peer"] == "a3")
    routes = {z["route"] for z in a3["routes"]}
    assert "catcher→sig" in routes and "sig→offer" in routes
    for z in a3["routes"]:
        assert set(z) == {"route", "mine", "theirs", "sim"}
    assert [z["score"] for z in r] == sorted([z["score"] for z in r], reverse=True)


def test_delete_and_replace(fake_enc):
    ix = build(fake_enc)
    q = agent("a1", signals=A1["signals"])
    assert "a2" in peers(ix.route("a1", q, 5))
    ix.index_put("a2", None)                      # 离开
    assert "a2" not in peers(ix.route("a1", q, 5))
    assert ix.stats()["owners"] == 3
    ix.index_put("a2", agent("a2", offers=["会修自行车"]))
    ix.index_put("a2", agent("a2", offers=["会修自行车"]))   # 重复 put：旧条目不残留
    assert ix.stats()["vectors"]["offers"] == 3
    assert ix.stats()["vectors"]["catchers"] == 1
    for _ in range(30):                            # 反复增删：槽位复用，索引不无限长
        ix.index_put("a9", agent("a9", signals=["临时片段", "另一个"], offers=["x"]))
        ix.index_put("a9", None)
    assert ix.subs["signals"].ix.get_current_count() < 40


def test_derived_put_and_revoke(fake_enc):
    ix = build(fake_enc)
    q = agent("a3", signals=["孩子暑假没人带"])
    ix.index_put("a2", agent("a2", catchers=["有人说最近手头紧"], derived=["正与一位孩子暑假没人带的家长形成看护关系"]))
    a2 = next(z for z in ix.route("a3", q, 5) if z["peer"] == "a2")
    assert any(z["route"] == "→derived" for z in a2["routes"])
    ix.index_put("a2", agent("a2", catchers=["有人说最近手头紧"], derived=[]))      # 派生片段撤销
    r = ix.route("a3", q, 5)
    assert not any(z["route"] == "→derived" for p in r if p["peer"] == "a2" for z in p["routes"])


def test_config_excluded_for_members_and_route_offers(fake_enc):
    ix = build(fake_enc)
    cfg = {"id": "cfg:a1|a2", "kind": "config", "members": ["a1", "a2"],
           "signals": [{"text": "还缺一个会做网站前端的人", "tier": "t0"}],
           "offers": [{"text": "孩子暑假没人带可以帮忙照看", "tier": "t0"}], "lacks": [{"cat": "具体技能或资源"}]}
    ix.index_put(cfg["id"], cfg)
    assert "cfg:a1|a2" not in peers(ix.route("a1", agent("a1", signals=["孩子暑假没人带"]), 10))
    assert "cfg:a1|a2" in peers(ix.route("a3", agent("a3", signals=["孩子暑假没人带"]), 10))
    ro = ix.route_offers(cfg, 5)
    ids = [z["id"] for z in ro]
    assert "a1" not in ids and "a2" not in ids and "cfg:a1|a2" not in ids
    assert ids[0] == "a3" and ro[0]["peer"] == "a3"


def test_k_respected_with_many_self_frags(fake_enc):
    ix = FragmentIndex(fake_enc)
    ix.index_put("me", agent("me", signals=[f"我的状态{i}号 网站" for i in range(20)], offers=[f"我能做网站{i}" for i in range(20)]))
    for j in range(5):
        ix.index_put(f"o{j}", agent(f"o{j}", offers=[f"别人能做网站{j}"]))
    r = ix.route("me", agent("me", signals=["我的状态0号 网站"]), 3)
    assert len(r) == 3 and "me" not in peers(r)


def test_empty_index_and_vec3(fake_enc):
    ix = FragmentIndex(fake_enc)
    assert ix.route("z", agent("z", signals=["随便"]), 5) == []
    ix.index_put("z", agent("z", signals=["随便"]))
    assert ix.route("z", agent("z", signals=["随便"]), 5) == []
    assert len(ix.vec3("z")) == 3 and ix.vec3("none") is None


def test_config_route_excludes_own_members_without_put(fake_enc):
    """构型节点没经 index_put 时，route 也要按 node.members 排除自己的成员。"""
    ix = build(fake_enc)
    cfg = {"id": "cfg:x", "kind": "config", "members": ["a1", "a2"],
           "signals": [{"text": "最近手头紧，工作室房租又涨了", "tier": "t0"}]}
    ps = peers(ix.route("cfg:x", cfg, 10))
    assert "a1" not in ps and "a2" not in ps and ps


def test_query_survives_heavy_deletion(fake_enc):
    ix = FragmentIndex(fake_enc, cap=64)
    for r in range(6):
        for j in range(40):
            ix.index_put(f"p{j}", agent(f"p{j}", signals=[f"轮{r} 片段{j} 网站"]))
        for j in range(38):
            ix.index_put(f"p{j}", None)
    r = ix.route("me", agent("me", signals=["网站"]), 20)
    assert {z["peer"] for z in r} <= {"p38", "p39"}
