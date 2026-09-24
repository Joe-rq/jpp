#!/usr/bin/env python3
"""T1 (b) 预算停机的期望输出：按任务书文字、用固定观察的读数与 T0 阈值独立计算（步 31-1；不拿 J++ 运行结果当标准答案）。

T1 的线在固定观察读数上给出与 T0 阈值相同的出口（`make_labels_t1.py`、过程记录 31-1 §二），所以这里直接用 T0 阈值判。
写 `probes/<项目>/baseline/t1/expected-budget.json`：{"N": N, "expected": 输出}；另写 `expected-t1.json`
（不设上限时的 T1 期望 = T0 期望加空的 unobserved 字段）。

用法：python3 scripts/budget_expected.py
"""
import json
import pathlib
import sys

ROOT = pathlib.Path(__file__).resolve().parents[1]
P = ROOT / "probes"
sys.path.insert(0, str(P / "_baseline"))
FIX = {"winnow": P / "winnow/fixture.json", "folio": P / "folio/fixture.json",
       "entity-align": P / "entity-align/fixture.json",
       "trial-refund": ROOT.parent / "评估/2026-09-24-试写/refund/fixture.json",
       "trial-interview": ROOT.parent / "评估/2026-09-24-试写/interview/fixture.json"}
BUDGET = {"trial-refund": 12, "trial-interview": 8, "entity-align": 6, "winnow": 8, "folio": 4}


class Judge:
    """按固定观察取读数并计调用；预算用完时返回 None（该份材料没问到）。"""

    def __init__(self, project, n):
        os_env = __import__("os").environ
        os_env["JEV_FIXTURE"] = str(FIX[project])
        os_env["JEV_BACKEND"] = "fixture"
        import importlib
        import jev_interface
        importlib.reload(jev_interface)
        self.j = jev_interface
        self.n = n
        self.used = 0

    def ask(self, material, qs, over=None):
        if self.n is not None and self.used >= self.n:
            return None
        self.used += 1
        return self.j.judge(material, qs, over)


def noul(p, t):
    return True if p >= t["hi"] + t["delta"] else (False if p <= t["lo"] - t["delta"] else None)


def refund(m, J):
    th = m["thresholds"]
    q1 = {"type": "noul", "instructions": "这段客服对话里，顾客是否明确提出要退款或退钱？"}
    q2 = {"type": "noul", "instructions": "这段客服对话里，顾客的措辞是否情绪激烈（例如愤怒、威胁投诉或曝光、连续质问）？"}
    out = {"urgent": [], "refund_calm": [], "no_refund": [], "review": [], "unobserved": []}
    yes = []
    for i, c in enumerate(m["chats"]):
        a = J.ask(c, [q1])
        if a is None:
            out["unobserved"].append(i); continue
        d = noul(a[0]["noul"], th["cs-refund"])
        (out["review"] if d is None else yes if d else out["no_refund"]).append(i)
    for i in yes:
        a = J.ask(m["chats"][i], [q2])
        if a is None:
            out["unobserved"].append(i); continue
        d = noul(a[0]["noul"], th["cs-heated"])
        (out["review"] if d is None else out["urgent"] if d else out["refund_calm"]).append(i)
    for k in out:
        out[k].sort()
    # 上界：急件 + 复核 + 没问到的（T1 任务书：没问到的也可能是急件；不设预算时 unobserved 为空，与 T0 公式相同）
    out["urgent_count"] = [len(out["urgent"]), len(out["urgent"]) + len(out["review"]) + len(out["unobserved"])]
    return out


def interview(m, J):
    q = {"type": "noul", "instructions": "面试官 b 的专长是否覆盖候选人 a 的技术方向，足以对其进行技术面试？"}
    pairs = [(c, iv) for c in m["candidates"] for iv in m["interviewers"] if c["level"] in iv["levels"]]
    yes, review, rejected, unobs = [], [], [], []
    for c, iv in pairs:
        who = {"candidate": c["name"], "interviewer": iv["name"]}
        a = J.ask({"a": c, "b": iv}, [q])
        if a is None:
            unobs.append(who); continue
        d = noul(a[0]["noul"], m["thresholds"]["iv-cover"])
        (review if d is None else yes if d else rejected).append(who)
    plan, load = [], {}
    for w in yes:
        if w["candidate"] not in [p["candidate"] for p in plan] and load.get(w["interviewer"], 0) < 2:
            plan.append(w); load[w["interviewer"]] = load.get(w["interviewer"], 0) + 1
    placed = [p["candidate"] for p in plan]
    return {"pairs_asked": len(pairs), "plan": plan,
            "unplaced": [c["name"] for c in m["candidates"] if c["name"] not in placed],
            "review": review, "rejected": rejected, "unobserved": unobs}


def align(m, J):
    th = m["thresholds"]
    crit = ["它们描述的是两种不同的产品。",
            "它们描述的是密切相关、可能是也可能不是同一款的产品：变体、特别版，或名字两种理解都说得通。",
            "它们描述的是同一款产品。"]
    qs = [{"type": "score", "instructions": "两条实体描述作为产品是什么关系？", "criteria": crit},
          {"type": "noul", "instructions": "两条实体写的是同一个啤酒名吗？"},
          {"type": "noul", "instructions": "两条实体来自同一家酒厂吗？"},
          {"type": "noul", "instructions": "两条实体描述的是同一种啤酒风格吗？"}]
    pairs = [(a, b) for a in m["catalog_a"] for b in m["catalog_b"] if abs(a["abv"] - b["abv"]) <= 1.5]
    names = ["leave unlinked", "curator queue", "assert sameAs"]
    routed, unobs = [], []
    for a, b in pairs:
        who = {"a": a["name"], "b": b["name"]}
        ans = J.ask({"a": a, "b": b}, qs)
        if ans is None:
            unobs.append(who); continue
        pr = ans[0]["probabilities"]
        k = max(range(3), key=lambda i: (pr[str(i)], -i))
        t = th["align-link"]
        outc = names[k] if pr[str(k)] >= t["hi"] + t["delta"] else "curator queue"
        h = {}
        for f, qi, key in (("name", 1, "align-name"), ("brewery", 2, "align-brewery"), ("style", 3, "align-style")):
            d = noul(ans[qi]["noul"], th[key])
            h[f] = "未决" if d is None else ("是" if d else "否")
        routed.append({"pair": who, "outcome": outc, "hints": h})
    cnt = lambda o: sum(1 for r in routed if r["outcome"] == o)  # noqa: E731
    return {"candidates": len(pairs), "tally": {"sameAs": cnt("assert sameAs"), "curator": cnt("curator queue"),
                                                 "unlinked": cnt("leave unlinked")},
            "routed": routed, "unobserved": unobs}


def winnow(m, J):
    th = m["thresholds"]
    data = m["tool_outputs"]
    eq = {"type": "noul", "instructions": "这段工具输出里是否含有报错、失败或异常信息？"}
    rq = {"type": "noul", "instructions": f"这段话的内容是否与「{data['task']}」这个话题相关？"}
    res = []
    for r in data["results"]:
        ch = r["chunks"]
        # 一份输出一层：报错题在前，逐块题按块编号在后
        e = J.ask("\n".join(ch), [eq])
        rel = [J.ask(c, [rq]) for c in ch]
        unobs = [i for i, a in enumerate(rel) if a is None]
        if e is None:
            res.append({"id": r["id"], "reason": "error_unobserved", "pruned": [], "uncertain": [],
                        "unobserved": list(range(len(ch)))}); continue
        d = noul(e[0]["noul"], th["winnow-error"])
        if d is not False:
            res.append({"id": r["id"], "reason": "error_present" if d else "error_unsure", "pruned": [],
                        "uncertain": [], "unobserved": unobs}); continue
        dec = [None if a is None else noul(a[0]["noul"], th["form-topic"]) for a in rel]
        cand = [i for i, x in enumerate(dec) if x is False]
        unc = [i for i, x in enumerate(dec) if x is None and rel[i] is not None]
        if sum(len(ch[i]) for i in cand) * 100 < sum(len(c) for c in ch) * 20:
            res.append({"id": r["id"], "reason": "below_min_prune_ratio", "pruned": [], "uncertain": [],
                        "unobserved": unobs})
        else:
            res.append({"id": r["id"], "reason": "pruned", "pruned": cand, "uncertain": unc, "unobserved": unobs})
    return {"task": data["task"], "results": res}


def folio(m, J):
    th, tree, doc = m["thresholds"], m["tree"], m["document"]
    kids = lambda n: tree.get(n, [])  # noqa: E731
    node, path, stopped, unobs_a = m["root"], [], "depth", []
    for _ in range(6):
        opts = kids(node)
        if not opts:
            stopped = "leaf"; break
        a = J.ask(doc, [{"type": "choice", "instructions": "这份法律文书最应归入下列哪一类？"}], opts)
        if a is None:
            stopped, unobs_a = "budget", list(opts); break
        pr = a[0]["probabilities"]
        k = max(range(len(opts)), key=lambda i: (pr[f"c{i}"], -i))
        t = th["folio-level"]
        if a[0].get("mode_share") == 1.0 and pr[f"c{k}"] >= t["hi"] + t["delta"]:
            node = opts[k]; path.append(node)
        else:
            stopped = "unsure"; break
    single = {"leaf": node, "path": path, "stopped": stopped, "unobserved": unobs_a}
    frontier, leaves, dead, layers, und, unobs_b = [m["root"]], [], [], 0, 0, []
    for _ in range(6):
        inner = [n for n in frontier if kids(n)]
        done = [n for n in frontier if not kids(n)]
        if not inner:
            leaves += done; break
        asked = [(n, c) for n in inner for c in kids(n)]
        a = J.ask(doc, [{"type": "noul", "instructions": f"这段话的内容是否与「{c}」这个话题相关？"} for _, c in asked])
        if a is None:
            unobs_b = [c for _, c in asked]; break
        dec = [noul(x["noul"], th["form-topic"]) for x in a]
        nxt = [c for (n, c), d in zip(asked, dec) if d]
        dead += [n for n in inner if not any(d and p == n for (p, _), d in zip(asked, dec))]
        und += sum(1 for d in dec if d is None)
        leaves += done
        frontier = nxt
        layers += 1
    multi = {"leaves": leaves, "dead_ends": dead, "layers": layers, "undecided": und, "unobserved": unobs_b}
    return {"single_path": single, "multi_path": multi}


FN = {"trial-refund": refund, "trial-interview": interview, "entity-align": align, "winnow": winnow, "folio": folio}


def main():
    for proj, f in FN.items():
        m = json.loads((P / proj / "baseline/materials.json").read_text(encoding="utf-8"))
        free = f(m, Judge(proj, None))
        exp0 = json.loads((P / proj / "baseline/expected.json").read_text(encoding="utf-8"))
        # 自检：不设上限时去掉 unobserved 应与 T0 期望在 T0 比较的键上相同
        import measure_expr
        strip = json.loads(json.dumps(free))
        d = measure_expr.first_diff(exp0, strip)
        assert d is None, (proj, d)
        J = Judge(proj, BUDGET[proj])
        b = f(m, J)
        assert J.used == BUDGET[proj], (proj, J.used)
        t1 = P / proj / "baseline/t1"
        (t1 / "expected-budget.json").write_text(json.dumps({"N": BUDGET[proj], "calls": J.used, "expected": b},
                                                            ensure_ascii=False, indent=1) + "\n", encoding="utf-8")
        (t1 / "expected-t1.json").write_text(json.dumps(free, ensure_ascii=False, indent=1) + "\n", encoding="utf-8")
        print(proj, "N", BUDGET[proj], "unobserved:", json.dumps(
            b.get("unobserved") or [x.get("unobserved") for x in b.get("results", [])] or b, ensure_ascii=False)[:200])


if __name__ == "__main__":
    sys.path.insert(0, str(ROOT / "scripts"))
    main()
