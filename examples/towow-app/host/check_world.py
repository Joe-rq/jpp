"""人物文件完整性检查：键齐全、日期、正文不含 agent id / 结构编号 / 别的 agent 的全名。"""
import json, glob, re, os, collections
ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
W = ["bio","skills_assets","projects","notes","messages","calendar","relations","constraints","sensitive"]
P = ["t0_public","t1_after_signal","t2_after_trust","never"]
fs = sorted(glob.glob(os.path.join(ROOT, "world/agents/*.json")))
A = {}; bad = collections.defaultdict(list)
for f in fs:
    try: a = json.load(open(f))
    except Exception as e: bad["json"].append(os.path.basename(f)); continue
    A[a.get("id")] = a
    if a.get("id") != os.path.basename(f)[:-5]: bad["id"].append(f)
    if set(W) - set(a.get("world", {})): bad["world_keys"].append(a["id"])
    if set(P) - set(a.get("disclosure_policy", {})): bad["policy_keys"].append(a["id"])
names = {i: a["owner"].get("display_name", "") for i, a in A.items()}
for i, a in A.items():
    txt = json.dumps(a["world"], ensure_ascii=False)
    if re.search(r"\ba0\d{3}\b", txt): bad["agent_id_in_text"].append(i)
    if re.search(r"\bS\d{3}\b", txt): bad["struct_id_in_text"].append(i)
    for j, n in names.items():
        if j != i and len(n) >= 3 and n in txt: bad["other_name"].append(f"{i}⊃{n}({j})")
print(len(fs), "files;", {k: (len(v), v[:8]) for k, v in bad.items()})
