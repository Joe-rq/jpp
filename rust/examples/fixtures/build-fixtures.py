#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""重建随仓示例 purpose-run、purpose-drive 的固定观察夹具（合成读数，只检查构造，不代表模型表现）。

做法：反复跑 `jpp run`，每次缺一条生成器输出或判断读数，jpp 报「固定生成未命中」/「固定观察未命中」并给出内核算出的
提示词、状态与题；本脚本按下面写死的规则补一条，再跑，直到程序跑完。规则与 crates/jpp/tests/purpose_run.rs、
z0886_drive.rs 的闭包端口同一套（虚构的「挑供应商」「数轴上走到目标」），不含任何真任务。

    cd 地基/rust-jpp && cargo build -p jpp && python3 examples/fixtures/build-fixtures.py [purpose-run|purpose-drive]

不带参数两个都重建。会覆盖 examples/fixtures/purpose-run.json、purpose-drive.json。
"""
import json
import os
import re
import subprocess
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.normpath(os.path.join(HERE, "..", ".."))
JPP = os.environ.get("JPP_BIN", os.path.join(ROOT, "target", "debug", "jpp"))

# ---------- purpose-run ----------
RUN_PURPOSE = "在这些供应商里找出最可能按期交付我们这份订单的，排好先后，并说出每家最大的风险。"
RUN_MODULES = {
    "material": "供应商；参照是我们的订单",
    "predicates": [
        {"text": "这家供应商能按期交付我们这份订单", "cut": "ordered", "scale": ["不能", "难说", "能"], "request": "all",
         "field": "rank", "cut_from": "purpose", "scale_from": "purpose"},
        {"text": "这家供应商交付这份订单最大的风险", "cut": "k_ary", "over": ["产能", "资金", "质量", "没有明显风险"],
         "request": "one", "field": "risk", "cut_from": "purpose", "over_from": "purpose"},
    ],
    "request": "每家一个名次与一个原因", "presupposition": None, "context": None, "reference": "我们的订单",
    "budget": None, "unsure": None, "done": ["rank", "risk"],
}


def run_gen(p):
    if p.startswith("下面是一段目的") and "拆成出题守则的十个模块" in p:
        return [RUN_MODULES]
    if "按期交付我们这份订单」" in p and "字面前提题" not in p:
        return [
            {"op": "measure", "text": "这家供应商按期交付这份订单的把握有多大？", "scale": ["不能", "难说", "能"], "evidence": ["ref"]},
            {"op": "test", "text": "这家供应商能按期交付，并且质量合格吗？"},
            {"op": "measure", "text": "这家供应商交付能力如何？"},
        ]
    if "最大的风险」" in p:
        return [{"op": "select", "text": "这家供应商交付这份订单最大的风险在哪一方面？",
                 "over": ["产能", "资金", "质量", "没有明显风险"], "evidence": ["ref"]}]
    return []  # 前提派生：补不上（记 missing），不筛项


def idx(state):
    m = re.search(r"供应商(\d+)", json.dumps(state, ensure_ascii=False))
    return int(m.group(1)) if m else 0


def run_answer(state, q):
    t = q["text"]
    if "需要分别回答的判断" in t:
        return {"Noul": 0.9 if "并且" in t else 0.1}
    if "这段材料里有没有" in t:
        return {"Noul": 0.9}
    if "已认证的题问的是同一件事" in t or "说的是同一件事" in t:
        k = len(state["over"])
        return {"Choice": [0.0] * (k - 1) + [1.0]}  # 题库都不是
    if "把握有多大" in t or "交付能力" in t:
        return {"Score": [[0.1, 0.2, 0.7], [0.6, 0.3, 0.1], [0.2, 0.6, 0.2]][idx(state) % 3]}
    if "最大的风险" in t:
        v = [0.1] * 4
        v[idx(state) % 4] = 0.7
        return {"Choice": v}
    return {"Noul": 0.5}


# ---------- purpose-drive ----------
DRIVE_PURPOSE = "控制数轴上的一个点，尽快走到目标位置。"
DRIVE_MODULES = {
    "material": "数轴上的位置",
    "predicates": [{"text": "离目标还远", "cut": "binary", "field": "far", "cut_from": "system"}],
    "request": None, "presupposition": None, "context": None, "reference": None, "budget": None, "unsure": None, "done": None,
}


def drive_gen(p):
    if "字面前提题" in p:
        return []
    if p.startswith("下面是一段目的") and "拆成出题守则的十个模块" in p:
        return [DRIVE_MODULES]
    if "离目标还远" in p:
        return [{"op": "test", "text": "离目标还远吗？"}]
    return []


def drive_answer(state, q):
    t = q["text"]
    over = state.get("over", [])
    if "需要分别回答的判断" in t:
        return {"Noul": 0.1}
    if "这段材料里有没有" in t:
        return {"Noul": 0.9}
    if "同一件事" in t:
        k = len(over)
        return {"Choice": [0.0] * (k - 1) + [1.0]}
    if "下一步最该做的动作" in t:
        c = state["on"][0]
        want = "right" if c["pos"] < c["goal"] else "left" if c["pos"] > c["goal"] else "wait"
        return {"Choice": [0.8 if x == want else 0.1 for x in over]}
    if "离目标还远" in t:
        return {"Noul": 0.7}
    return {"Noul": 0.5}


TARGETS = {
    "purpose-run": dict(gen=run_gen, answer=run_answer, purpose=RUN_PURPOSE, args=[]),
    "purpose-drive": dict(gen=drive_gen, answer=drive_answer, purpose=DRIVE_PURPOSE,
                          args=["--env", "walk=python3 worlds/walk.py"]),
}


def build(name):
    t = TARGETS[name]
    out = os.path.join(HERE, name + ".json")
    fx = {"description": f"Synthetic fixed observations for {name}.jpp (construction check only; not model evaluation). "
                         f"Built by build-fixtures.py.",
          "observations": [], "generations": []}
    for _ in range(2000):
        with open(out, "w", encoding="utf-8") as f:
            json.dump(fx, f, ensure_ascii=False, indent=1)
            f.write("\n")
        p = subprocess.run([JPP, "run", f"examples/{name}.jpp", "--purpose", t["purpose"], "--fixtures", out] + t["args"],
                           cwd=ROOT, capture_output=True, text=True)
        e = p.stderr
        m = re.search(r"固定生成未命中：(.*) retry_seq=(\d+)", e, re.S)
        if m:
            fx["generations"].append({"prompt": m.group(1), "retry_seq": int(m.group(2)), "output": t["gen"](m.group(1))})
            continue
        m = re.search(r"内核这次算出来的状态 = (\{.*?\})；题 = (\{.*?\})。夹具里要有", e, re.S)
        if m:
            state, q = json.loads(m.group(1)), json.loads(m.group(2))
            obs = dict(state)
            obs.update(q)
            obs["answer"] = t["answer"](state, q)
            fx["observations"].append(obs)
            continue
        if p.returncode != 0:
            sys.exit(f"{name}: 跑不通且不是缺观察：\n{e[:3000]}")
        # 闸门的诊断判断缺观察时不报错，按并列进 pending；把这些补上再跑，直到没有可补的
        rep = json.loads(subprocess.run([JPP, "run", f"examples/{name}.jpp", "--purpose", t["purpose"], "--fixtures", out,
                                         "--json"] + t["args"], cwd=ROOT, capture_output=True, text=True).stdout)
        added = False
        for pd in (rep.get("value") or {}).get("pending", []):
            if pd.get("cause") != "tie" or "item" not in pd or "q" not in pd:
                continue
            sl = pd["item"]["slots"]
            state = {k: (v if isinstance(v, list) else [v]) for k, v in sl.items()}
            q = {k: pd["q"][k] for k in ("calib", "op", "text")}
            if q["op"] == "noul":
                q["op"] = "test"
            ans = t["answer"](state, q)
            if ans == {"Noul": 0.5}:
                continue
            obs = dict(state)
            obs.update(q)
            obs["answer"] = ans
            fx["observations"].append(obs)
            added = True
        if added:
            continue
        print(f"{name}: {len(fx['generations'])} 条生成、{len(fx['observations'])} 条观察 → {out}")
        return
    sys.exit(f"{name}: 超过 2000 轮仍未跑完")


if __name__ == "__main__":
    for n in (sys.argv[1:] or list(TARGETS)):
        build(n)
