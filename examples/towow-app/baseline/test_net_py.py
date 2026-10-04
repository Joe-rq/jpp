import asyncio, sys, os
sys.path.insert(0, os.path.dirname(__file__))
import pytest
from net_py import Net, Ports, Reading, pair_key


def W(name, need=(), offer=(), hidden=(), policy="愿意"):
    return {"display": name, "signals": [{"text": f"need:{n}"} for n in need],
            "offers": [{"text": f"offer:{o}"} for o in offer],
            "catchers": [], "projects": [],
            "signals_t1": hidden, "policy": policy}


def with_hidden(w):                       # t1 片段：只在对方要、本人同意后才进状态
    w = dict(w); w["signals"] = w["signals"] + [{"text": f"need:{n}", "tier": "t1"} for n in w.pop("signals_t1")]
    return w


class Fake(Ports):
    """假判断器：确定性、免费。open_ab 成立 ⇔ B 可见 offer 与 A 可见 need 同名；一方可见 need 全空 → 未决，缺「近况细节」。"""
    def __init__(self, with_cfg=False):
        self.states, self.index, self.with_cfg = [], {}, with_cfg

    def index_put(self, x, n): self.index[x] = n
    def index_del(self, x): self.index.pop(x, None)

    async def judge(self, state, qs):
        self.states.append(state)
        out = []
        if len(state.get("sides", [])) == 2 and len(state["owners"]) == 2:
            A, B = state["sides"]
            vis = lambda S, k: {t.split(":")[-1] for t in S[k]} | {f["text"].split(":")[-1] for f in S["已解锁"] if f["text"].startswith(k[:-1] if k=="offers" else "need")}
            ab = bool(vis(B, "offers") & vis(A, "signals"))
            ba = bool(vis(A, "offers") & vis(B, "signals"))
            empty = lambda S: not vis(S, "signals") and S["kind"] == "agent"
            for q in qs:
                if q.tag == "open":
                    mine = ab if "读 B" in q.text else ba
                    unsure = empty(A if "读 B" in q.text else B) and not mine
                    holder = state["owners"][0] if "读 B" in q.text else state["owners"][1]
                    out.append(Reading(act=None, needed=("近况细节", holder)) if unsure else Reading(act=mine))
                elif q.tag == "form": out.append(Reading(value="direct" if ab or ba else "none"))
                elif q.tag == "dir": out.append(Reading(value="both" if ab and ba else "ab" if ab else "ba" if ba else "none"))
                elif q.tag == "value": out.append(Reading(value="中"))
                elif q.tag == "timing": out.append(Reading(act=True))
                else: out.append(Reading(act=False))
            return out
        for q in qs:
            if q.tag == "disclose": out.append(Reading(act="不给" not in str(state.get("policy"))))
            elif q.tag == "hold": out.append(Reading(act=True, p=0.8))
            elif q.tag == "weakest": out.append(Reading(value=q.opts[0]))
            elif q.tag == "value": out.append(Reading(value="中"))
            else: out.append(Reading(act=False))
        return out

    async def route(self, x, node, k):
        return [{"peer": y, "routes": [{"route": "r", "mine": "", "theirs": ""}]} for y, n in self.index.items()
                if y != x and (self.with_cfg or n["kind"] == "agent") and (self.with_cfg or node["kind"] == "agent")][:k]

    def graph_local(self, x, links, limits):
        nb = {}
        for l in links: nb.setdefault(l["a"], set()).add(l["b"]); nb.setdefault(l["b"], set()).add(l["a"])
        return [{"shape": "chain", "members": sorted([x, y, z]), "roles": {}} for y in nb.get(x, ()) for z in nb.get(y, ()) if z != x]


def pairs(f): return {tuple(s["owners"]) for s in f.states if len(s["owners"]) == 2}
def pair_calls(f, since=0): return [tuple(s["owners"]) for s in f.states[since:] if len(s["owners"]) == 2]


async def build(f, n=10):
    net = Net(f)
    for i in range(n):   # a0..a9：a_i 要 X_i，提供 X_{(i+1)%n}；链式互补
        await net.join(f"a{i}", W(f"人{i}", need=[f"X{i}"], offer=[f"X{(i+1) % n}"]))
    await net.quiet()
    return net


def test_join_disclose_leave_incremental():
    async def go():
        f = Fake(); net = await build(f)
        assert len(pairs(f)) == 45                                  # 全部 45 对各判一次
        assert sum(e["holds"] for e in net.edge.values()) == 10     # 环上相邻 10 对成立
        base = len(f.states)

        # 披露：a3 多给一个 t0 提供项 → 只有读到 a3 的对（9 对）重判，其余零调用
        await net.disclose("a3", {"offers": [{"text": "offer:Z"}]}); await net.quiet()
        redo = pair_calls(f, base)
        assert sorted(redo) == sorted(pair_key("a3", f"a{j}") for j in range(10) if j != 3)
        assert len(redo) == len(set(redo)) == 9

        # 缓存：强制重跑一对（状态没变）→ 端口零调用
        c0 = net.n_calls; net.mark("pair", ("a0", "a1")); await net.quiet()
        assert net.n_calls == c0

        # 同值重发：缓存与同值不升版本，零新调用
        mid = len(f.states); n_calls = net.n_calls
        await net.disclose("a3", {}); await net.quiet()
        assert len(f.states) == mid and net.n_calls == n_calls

        # 离开：a5 离开不触发任何一对重判（只撤回它参与的对）
        base = len(f.states)
        await net.leave("a5"); await net.quiet()
        assert pair_calls(f, base) == []
        assert not any("a5" in k for k in net.edge) and not any("a5" in k for k in net.adj.get("a4", {}))
        assert not any("a5" in c["members"] for c in net.config.values()) and not any("a5" in c["members"] for c in net.cand.values())
        assert net.errors == []
    asyncio.run(go())


def test_unsure_fill_chain_and_etier():
    async def go():
        f = Fake(); net = Net(f)
        await net.join("p", with_hidden(W("P", need=[], offer=["Q"], hidden=["R"])))   # t0 无需求 → 对 q 未决
        await net.join("q", W("Q", need=["Q"], offer=["R"]))
        await net.quiet()
        k = pair_key("p", "q")
        assert net.reply[("p", "q", "近况细节")]["granted"]                                # 补信息：q 向 p 要 → p 策略放行
        assert any(fr["text"] == "need:R" for fr in net.unlocked[("p", "q")])
        assert net.edge[k]["holds"] and "t1" in net.edge[k]["tier_seen"]
        # E-tier：t1 片段只在这一对的状态里，其它对的状态没有它
        await net.join("z", W("Z", need=["Q"], offer=["X"])); await net.quiet()
        zs = [s for s in f.states if set(s["owners"]) == {"p", "z"}]
        assert zs and all(fr["text"] in [u["text"] for u in net.unlocked.get(("p", "z"), [])] or fr["tier"] == "t0"
                          for s in zs for sd in s["sides"] for fr in sd["已解锁"] if fr["owner"] == "p")
        bad = {"sides": [{"已解锁": [{"owner": "p", "tier": "t1", "text": "need:R"}]}], "owners": ["p", "zz"]}
        with pytest.raises(AssertionError): await net.judge(bad, [])                        # 未解锁给 zz → 进 judge 前被闸拦下
        # 拒绝：策略不给 → denied，进 pending，不循环
        net2 = Net(Fake()); await net2.join("p", with_hidden(W("P", offer=["Q"], hidden=["R"], policy="不给"))); await net2.join("q", W("Q", need=["Q"])); await net2.quiet()
        assert net2.reply[("p", "q", "近况细节")]["denied"]
        assert any("拒绝" in x.get("needed", "") for x in net2.pending[("pair", pair_key("p", "q"))])
    asyncio.run(go())


def test_composition_depth_and_quiescence():
    async def go():
        f = Fake(with_cfg=True); net = await build(f, 6)
        assert net.errors == [] and net.config
        assert all(c["depth"] < net.cap for c in net.config.values())
        assert all(not (c["members"] and sum(m.startswith("cfg:") for m in c["members"]) > 1) for c in net.config.values())
        before = len(f.states); await net.quiet()
        assert len(f.states) == before                                  # 静止：再等一轮不再有调用
        await net.leave("a2"); await net.quiet()
        assert not any("a2" in net._closure(k) for k in net.config)       # 依赖它的构型（含构型的构型）全撤销
    asyncio.run(go())
