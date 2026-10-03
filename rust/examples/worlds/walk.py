#!/usr/bin/env python3
"""purpose-drive 示例用的小世界：数轴上的一个点从 0 出发，走到目标（开局参数 reset）为止。

宿主世界的契约（GUIDE.md「环境」一节，Z0885）：从 stdin 读一行 JSON 请求 {state, action, reset?}，
往 stdout 写一行 JSON 结果 {state, obs, actions, idle, done, hash, result?}。两次调用之间不留状态——
状态整个在请求与结果里来回。每局最多 12 步。
"""
import hashlib
import json
import sys

req = json.loads(sys.stdin.readline())
st = req.get("state")
if st is None:
    st = {"pos": 0, "goal": req.get("reset", 0), "t": 0}
else:
    step = {"right": 1, "left": -1}.get(req.get("action"), 0)
    st = {"pos": st["pos"] + step, "goal": st["goal"], "t": st["t"] + 1}
done = st["pos"] == st["goal"] or st["t"] >= 12
out = {
    "state": st,
    "obs": {"t": st["t"], "pos": st["pos"], "goal": st["goal"]},
    "actions": ["left", "right", "wait"],
    "idle": "wait",
    "done": done,
    "hash": hashlib.sha256(json.dumps(st, sort_keys=True).encode()).hexdigest()[:16],
    "result": {"reached": st["pos"] == st["goal"]},
}
print(json.dumps(out))
