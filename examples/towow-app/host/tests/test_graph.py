from jx.core import Exit

from host.graph import arcs_of, graph_local


def E(a, b, dir="ba", form="direct", holds=True):
    """dir 相对记录自身的 a、b：ba = A 帮 B（弧 a→b），ab = B 帮 A（弧 b→a）。"""
    a, b = sorted((a, b)) if a < b else (a, b)
    return {"a": a, "b": b, "dir": dir, "form": form, "holds": holds}


def give(u, v, form="direct"):
    """u 供给 v，自动换算成 a<b 的记录。"""
    if u < v:
        return {"a": u, "b": v, "dir": "ba", "form": form, "holds": True}
    return {"a": v, "b": u, "dir": "ab", "form": form, "holds": True}


def shapes(res, shape):
    return [c for c in res if c["shape"] == shape]


def test_direction_mapping():
    assert arcs_of({"a": "a", "b": "b", "dir": "ab"}) == ([("b", "a")], False)
    assert arcs_of({"a": "a", "b": "b", "dir": "ba"}) == ([("a", "b")], False)
    assert arcs_of({"a": "a", "b": "b", "dir": "both"}) == ([("a", "b"), ("b", "a")], True)
    assert arcs_of({"a": "a", "b": "b", "dir": "none"}) == ([], False)
    assert arcs_of({"a": "a", "b": "b", "dir": "a>b"}) == ([("a", "b")], False)
    assert arcs_of({"a": "a", "b": "b", "dir": Exit("pick", value="ab")}) == ([("b", "a")], False)
    assert arcs_of({"a": "a", "b": "b", "dir": {"kind": "pick", "value": "ba"}}) == ([("a", "b")], False)
    assert arcs_of({"a": "a", "b": "b", "dir": Exit("unsure", cause="tie")}) == ([], False)


def test_holds_filter_accepts_exit_and_dict():
    es = [give("x", "p"), {**give("x", "q"), "holds": False}, {**give("x", "r"), "holds": Exit("act")},
          {**give("x", "s"), "holds": {"act": False}}]
    ms = {tuple(c["members"]) for c in shapes(graph_local("x", es), "pair")}
    assert ms == {("p", "x"), ("r", "x")}


def test_chain_three_and_four():
    es = [give("a", "b"), give("b", "c"), give("c", "d")]
    res = graph_local("b", es)
    ch = {tuple(c["order"]) for c in shapes(res, "chain")}
    assert ("a", "b", "c") in ch and ("a", "b", "c", "d") in ch and ("b", "c", "d") in ch
    c4 = next(c for c in shapes(res, "chain") if len(c["members"]) == 4)
    assert c4["members"] == ["a", "b", "c", "d"] and c4["roles"]["a"].startswith("起点")
    # chain=2 条边上限：没有四人链
    assert all(len(c["members"]) <= 3 for c in shapes(graph_local("b", es, {"chain": 2}), "chain"))


def test_chain_excludes_mutual_and_requires_x():
    es = [give("a", "b"), E("b", "c", dir="both"), give("c", "d")]
    assert shapes(graph_local("b", es), "chain") == []
    assert all("z" in c["members"] for c in graph_local("z", [give("a", "b"), give("b", "c"), give("z", "a")]))


def test_ring_and_reciprocity_rule():
    es = [give("a", "b"), give("b", "c"), give("c", "a")]
    r = shapes(graph_local("a", es), "ring")
    assert len(r) == 1 and r[0]["members"] == ["a", "b", "c"] and r[0]["order"] == ["a", "b", "c"]
    # 任两人之间有互惠 act 边 → 不是环（即使互惠边不在环上）
    es4 = [give("a", "b"), give("b", "c"), give("c", "d"), give("d", "a"), E("a", "c", dir="both")]
    assert shapes(graph_local("a", es4), "ring") == []
    es4b = es4[:4]
    r4 = shapes(graph_local("a", es4b), "ring")
    assert [c["members"] for c in r4] == [["a", "b", "c", "d"]]
    assert shapes(graph_local("a", es4b, {"ring": 3}), "ring") == []


def test_star_center_and_periphery_cap():
    es = [give("h", p) for p in ["p1", "p2", "p3", "p4", "p5", "p6", "p7"]]
    st = shapes(graph_local("h", es), "star")
    assert len(st) == 1 and st[0]["center"] == "h" and len(st[0]["members"]) == 7   # 中心 + 外围 6
    # x 是外围：接到已有中心
    st2 = shapes(graph_local("p7", es), "star")
    assert st2 and "p7" in st2[0]["members"] and st2[0]["center"] == "h"
    assert shapes(graph_local("h", es[:2]), "star") == []


def test_m2m_biclique():
    es = [give(l, r) for l in ["f1", "f2", "f3"] for r in ["r1", "r2", "r3"]]
    m = shapes(graph_local("f1", es), "m2m")
    assert m and m[0]["sides"] == [["f1", "f2", "f3"], ["r1", "r2", "r3"]]
    assert m[0]["roles"]["r2"] == "接受侧"
    assert shapes(graph_local("r1", es), "m2m")                 # x 在接受侧也找得到
    assert len(shapes(graph_local("f1", es, {"m2m": 2}), "m2m")[0]["sides"][0]) == 2


def test_team_roles_from_projects_and_offers():
    nodes = {
        "lead": {"projects": [{"name": "社区纪录片", "roles_needed": ["摄影师", "剪辑师", "配乐作曲"]}]},
        "p": {"offers": [{"text": "专业摄影师，有全画幅相机"}]},
        "q": {"offers": [{"text": "做过三年视频剪辑师"}]},
        "r": {"offers": [{"text": "会烤面包"}]},
    }
    es = [E("lead", "p", dir="both"), E("lead", "q", dir="none"), E("lead", "r", dir="none")]
    t = shapes(graph_local("q", es, {"nodes": nodes}), "team")
    assert len(t) == 1
    assert t[0]["members"] == ["lead", "p", "q"]
    assert t[0]["roles"]["p"] == "摄影师" and t[0]["roles"]["q"] == "剪辑师"
    assert t[0]["missing"] == ["配乐作曲"]
    assert shapes(graph_local("q", es), "team") == []             # 不给 nodes 就不猜


def test_relay_and_pair_roles():
    es = [{"a": "a", "b": "x", "dir": "ab", "form": "relay_a", "holds": True}, give("x", "y")]
    res = graph_local("x", es)
    rl = shapes(res, "relay")
    assert rl[0]["members"] == ["a", "x"] and rl[0]["via"] == "a" and rl[0]["roles"]["a"].startswith("转介方")
    pr = shapes(res, "pair")
    assert pr[0]["members"] == ["x", "y"] and pr[0]["roles"] == {"x": "提供方", "y": "受益方"}


def test_one_hop_only_limits():
    """只给 x 的一跳边：x 居中的三人链、x 为中心的星能出；四人链、环出不来（需要两跳边）。"""
    full = [give("a", "x"), give("x", "c"), give("c", "d"), give("d", "a")]
    one_hop = [e for e in full if "x" in (e["a"], e["b"])]
    res1 = graph_local("x", one_hop)
    assert {tuple(c["order"]) for c in shapes(res1, "chain")} == {("a", "x", "c")}
    assert shapes(res1, "ring") == []
    resf = graph_local("x", full)
    assert shapes(resf, "ring") and any(len(c["members"]) == 4 for c in shapes(resf, "chain"))


def test_dedupe_order_and_determinism():
    es = [give("a", "b"), give("b", "c"), give("c", "a"), give("b", "d"), give("b", "e")]
    r1 = graph_local("b", es)
    r2 = graph_local("b", list(reversed(es)))
    assert r1 == r2
    keys = [(c["shape"], tuple(c["members"])) for c in r1]
    assert len(keys) == len(set(keys))
    order = ["m2m", "team", "ring", "chain", "star", "relay", "pair"]
    assert [order.index(c["shape"]) for c in r1] == sorted(order.index(c["shape"]) for c in r1)
    assert all(c["members"] == sorted(c["members"]) for c in r1)
    assert len(graph_local("b", es, {"max": 3})) == 3


def test_isolated_and_config_nodes():
    assert graph_local("x", []) == []
    es = [give("cfg:a|b", "inv"), give("inv", "x")]
    res = graph_local("inv", es)
    assert shapes(res, "chain")[0]["order"] == ["cfg:a|b", "inv", "x"]


def test_team_nodes_fallback_lookup():
    nodes = {"lead": {"projects": [{"name": "纪录片", "roles_needed": ["摄影师", "剪辑师"]}]},
             "p": {"offers": ["专业摄影师"]}, "q": {"offers": ["视频剪辑师"]}}
    es = [E("lead", "p", dir="none"), E("lead", "q", dir="none")]
    t = shapes(graph_local("lead", es, {}, node_lookup=nodes.get), "team")
    assert t and t[0]["members"] == ["lead", "p", "q"]
    # 参数给的优先于兜底
    t2 = shapes(graph_local("lead", es, {"nodes": {"lead": {"projects": []}}}, node_lookup=nodes.get), "team")
    assert t2 == []
