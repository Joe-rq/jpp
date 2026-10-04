"""把一次运行的 events.jsonl 压成前端回放用的文件（GitHub Pages 上放得下、加载快）。
保留：meta、node_join/leave、edge、config、plan、disclose*、unsure_route、spotlight、stats、invalidate。
判断事件只留决定成立的几类题（open、hold、disclose）与所有「拿不准」；batch 只留计数用的每第 4 条。
用法：.venv/bin/python host/trim_replay.py runs/full-500/events.jsonl web/public/replay.jsonl [--max-mb 12]
"""
import json, os, sys

KEEP_Q = {"open", "hold", "disclose", "tighter"}


def main():
    src, dst = sys.argv[1], sys.argv[2]
    max_mb = float(sys.argv[sys.argv.index("--max-mb") + 1]) if "--max-mb" in sys.argv else 12.0
    out, nb, size = [], 0, 0
    for line in open(src):
        e = json.loads(line)
        t = e.get("type")
        if t == "judge":
            if e.get("q") not in KEEP_Q and e.get("exit") != "unsure":
                continue
        elif t == "batch":
            nb += 1
            if nb % 4:
                continue
        elif t == "probe":
            continue
        s = json.dumps(e, ensure_ascii=False, separators=(",", ":"))
        size += len(s.encode()) + 1
        if size > max_mb * 1e6:
            break
        out.append(s)
    with open(dst, "w") as f:
        f.write("\n".join(out) + "\n")
    print(f"{dst}：{len(out)} 行，{os.path.getsize(dst) / 1e6:.1f} MB")


if __name__ == "__main__":
    main()
