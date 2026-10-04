"""从事件流（events.md 格式）重建 result.json 的近似版本：运行被中断、拿不到引擎快照时用。
边：每对取最后一条 edge 事件（state=gone 视为不成立）；构型：每个 id 取最后一条 config 事件；方案：plan 事件。
注意：只有成立过的边才发 edge 事件，所以不成立的边与它们的读数不在这里（AUC 算不了）。"""
import json, sys
src, out = sys.argv[1], sys.argv[2]
edges, cfgs, plans, present, joins = {}, {}, {}, set(), {}
for l in open(src):
    d = json.loads(l); t = d.get("type")
    if t == "node_join" and d.get("kind", "agent") == "agent":
        present.add(d["id"]); joins.setdefault(d["id"], {"id": d["id"], "t_join": d["t"]})
    elif t == "node_leave":
        present.discard(d["id"])
    elif t == "edge":
        k = tuple(sorted((d["a"], d["b"])))
        edges[k] = d
        for x in k:
            j = joins.get(x)
            if j is not None and d.get("state") != "gone" and "t_first_opp" not in j:
                j["t_first_opp"] = d["t"]
    elif t == "config":
        cfgs[d["id"]] = d
    elif t == "plan":
        plans[d["config"]] = d
E = [{"a": a, "b": b, "holds": d.get("state") != "gone", "p_hold": d.get("conf"), "decisive_exit": "act" if d.get("state") != "gone" else "ignore",
      "form": d.get("form"), "dir": d.get("dir")} for (a, b), d in edges.items()]
C = [{"id": k, "shape": d.get("shape"), "members": d.get("members"), "p_hold": d.get("conf"),
      "stage": "plan" if k in plans else d.get("stage")} for k, d in cfgs.items() if d.get("stage") != "dissolved"]
json.dump({"present": sorted(present), "edges": E, "configs": C, "joins": list(joins.values()),
           "plans": list(plans.values()), "note": "从事件流重建（运行被中断）"}, open(out, "w"), ensure_ascii=False, indent=1)
print(len(present), "present;", len(E), "edges;", len(C), "configs;", len(plans), "plans")
