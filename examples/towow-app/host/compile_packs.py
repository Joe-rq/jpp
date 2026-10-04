"""模拟 agent 的端侧编译：每个 agent 用同一份 towow.spec 说明（design/operator-pack-spec.md）
经 claude -p 把自己的本地世界编译成算子包。这一步在真实网络里由 agent 自己的模型做，网络不碰世界。
用法：.venv/bin/python host/compile_packs.py [--only a0001,a0002] [--conc 6]
产物：world/packs/<id>.json（按世界文件哈希缓存于 runs/gen-cache.sqlite）
"""
import argparse, asyncio, glob, hashlib, json, os, sys, time
ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
sys.path.insert(0, ROOT)
from jx.ports.gen import GenPort, GenFail

SPEC = open(os.path.join(ROOT, "design/operator-pack-spec.md")).read()

def prompt_of(agent: dict) -> str:
    w = {"owner": agent["owner"], "world": agent["world"], "disclosure_policy_written_by_owner": agent.get("disclosure_policy")}
    return SPEC + "\n\n## 主人的本地世界（只有你读得到）\n\n```json\n" + json.dumps(w, ensure_ascii=False, indent=1) + "\n```\n"

async def one(gen, path, outdir, sem_log):
    raw = open(path, "rb").read()
    sha = hashlib.sha256(raw).hexdigest()
    a = json.loads(raw)
    out = os.path.join(outdir, a["id"] + ".json")
    if os.path.exists(out):
        try:
            old = json.load(open(out))
        except Exception:
            old = {}
        if old.get("world_sha256") == sha:
            return "skip"
        if "world_sha256" not in old and os.path.getmtime(out) >= os.path.getmtime(path):
            old["world_sha256"] = sha; json.dump(old, open(out, "w"), ensure_ascii=False, indent=1)
            return "skip"
    for attempt in range(3):
        try:
            pack = await gen.json(prompt_of(a))
            assert isinstance(pack, dict) and pack.get("signals") and pack.get("catchers")
            pack["id"] = a["id"]; pack["host_agent"] = a.get("host_agent"); pack["owner_display"] = a["owner"].get("display_name")
            pack["city"] = a["owner"].get("city"); pack["world_sha256"] = sha
            json.dump(pack, open(out, "w"), ensure_ascii=False, indent=1)
            return "ok"
        except (GenFail, AssertionError, KeyError) as e:
            sem_log.append(f"{a['id']} 第{attempt+1}次失败：{str(e)[:120]}")
    return "fail"

async def main():
    ap = argparse.ArgumentParser(); ap.add_argument("--only"); ap.add_argument("--conc", type=int, default=6)
    ap.add_argument("--model", default="sonnet")
    args = ap.parse_args()
    outdir = os.path.join(ROOT, "world/packs"); os.makedirs(outdir, exist_ok=True)
    gen = GenPort(os.path.join(ROOT, "runs/gen-cache.sqlite"), model=args.model, concurrency=args.conc)
    paths = sorted(glob.glob(os.path.join(ROOT, "world/agents/*.json")))
    if args.only:
        keep = set(args.only.split(",")); paths = [p for p in paths if os.path.basename(p)[:-5] in keep]
    log = []; t0 = time.time()
    res = await asyncio.gather(*[one(gen, p, outdir, log) for p in paths])
    print({k: res.count(k) for k in set(res)}, "生成调用", gen.calls, "秒", round(time.time() - t0))
    for l in log[-20:]: print(l)

asyncio.run(main())
