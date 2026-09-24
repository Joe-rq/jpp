"""CI 脚本共用：报告模式与基线比较（21 §九·1：先报告、记基线，只许减少）。

模式由环境变量 JPP_CI_MODE 决定：report（默认，只报告不失败）或 fail（超过基线即非零退出）。
脚本按 21 §九·1 列出的步各自改为 fail 模式；在那之前 ci.sh 以 report 模式运行。
"""
import json, os, pathlib, sys

ROOT = pathlib.Path(__file__).resolve().parent.parent
BASE = ROOT / "scripts" / "baselines"


def finish(name: str, count: int, detail: list, update: bool = False) -> None:
    """打印结果，与基线比较；JPP_CI_UPDATE_BASELINE=1 时写入基线。"""
    path = BASE / f"{name}.json"
    base = json.loads(path.read_text()) if path.exists() else None
    if update or os.environ.get("JPP_CI_UPDATE_BASELINE") == "1" or base is None:
        BASE.mkdir(parents=True, exist_ok=True)
        path.write_text(json.dumps({"count": count, "detail": detail}, ensure_ascii=False, indent=1) + "\n")
        base = {"count": count}
    mode = os.environ.get("JPP_CI_MODE", "report")
    delta = count - base["count"]
    print(f"[{name}] 违规 {count}（基线 {base['count']}，变化 {delta:+d}；模式 {mode}）")
    for d in detail[:20]:
        print(f"  - {d}")
    if len(detail) > 20:
        print(f"  …… 另 {len(detail) - 20} 条，全文见 scripts/baselines/{name}.json")
    if mode == "fail" and delta > 0:
        sys.exit(1)


def rust_files(*crates):
    for c in crates or [p.name for p in (ROOT / "crates").iterdir()]:
        yield from sorted((ROOT / "crates" / c / "src").rglob("*.rs"))
