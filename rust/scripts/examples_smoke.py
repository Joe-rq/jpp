#!/usr/bin/env python3
"""装机冒烟：用装出来的 `jpp` 把 examples/README.md 里的五条命令各跑一遍，返回值与金样核对（B0672 ①）。

为什么：`cargo test`（crates/jpp/tests/examples_five.rs）用的是测试构建出来的 jpp。外人拿到的是 `cargo install` 装出来的 release 版，
装得上、在检出目录里跑得起来，要单独证明一次。CI 在干净机器上装出 jpp，调本脚本，任何一个示例跑不通就红。
这里不比逐字节金样（那归 `cargo test -p jpp --test golden`），比的是：退出码 0、状态 returned、返回值（`value`）与
`tests/golden/<用例>/projection.json` 里的相同；第 5 条（重放）要求零新调用、返回值与第 1 条相同。

用法：  python3 scripts/examples_smoke.py [--jpp <jpp 可执行文件>]      缺省 target/debug/jpp
命令从 examples/README.md 的 ```sh 块里读（和读者看到的是同一份），在 rust-jpp 目录下执行（README 写明的位置）。
"""
import argparse
import json
import pathlib
import shlex
import subprocess
import sys

ROOT = pathlib.Path(__file__).resolve().parent.parent
PROGRAM_CASE = {"purpose-run": "purpose-run", "purpose-drive": "purpose-drive", "modules-fill": "modules-fill",
                "unsure-default": "unsure-default"}


def sh_blocks(text):
    out, buf, on = [], [], False
    for line in text.splitlines():
        if line.startswith("```"):
            if on:
                out.append("\n".join(buf))
                buf = []
            on = line.strip() == "```sh" if not on else False
        elif on:
            buf.append(line)
    return out


def main():
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("--jpp", default=str(ROOT / "target" / "debug" / "jpp"))
    jpp = pathlib.Path(ap.parse_args().jpp).resolve()
    if not jpp.exists():
        sys.exit(f"找不到 {jpp}")
    readme = (ROOT / "examples" / "README.md").read_text(encoding="utf-8")
    # 末尾「重建夹具」那一段的命令不是示例，不算
    commands = [b for b in sh_blocks(readme.split("## 重建夹具")[0]) if b.strip().startswith("jpp run ")]
    if len(commands) != 5:
        sys.exit(f"examples/README.md 里应有 5 条示例命令，实有 {len(commands)}")
    first_value, bad = None, 0
    for n, block in enumerate(commands, 1):
        argv = shlex.split(" ".join(block.split("\\\n")))
        prog = pathlib.Path(argv[2]).stem
        problems = []
        r = subprocess.run([str(jpp), *argv[1:]], cwd=ROOT, capture_output=True, text=True, timeout=600)
        if r.returncode != 0:
            problems.append(f"退出 {r.returncode}：{r.stderr.strip().splitlines()[-1:]}")
        else:
            report = json.loads(r.stdout)
            if report.get("status") != "returned":
                problems.append(f"status {report.get('status')}")
            if "--replay" in argv:
                if report["cost"]["calls"] != 0 or report["cost"]["asks"] != 0:
                    problems.append(f"重放有新调用：{report['cost']}")
                if report.get("value") != first_value:
                    problems.append("重放的 value 与第 1 条不同")
            else:
                golden = json.loads((ROOT / "tests/golden" / PROGRAM_CASE[prog] / "projection.json").read_text(encoding="utf-8"))
                if report.get("value") != golden["value"]:
                    problems.append("value 与金样不同")
                if n == 1:
                    first_value = report.get("value")
        print(f"[{'未通过' if problems else '通过'}] {n} {prog}" + ("：" + "；".join(problems) if problems else ""))
        bad += bool(problems)
    # 没有操作系统沙箱的机器上，带撤不回动作的过程示例会把账本写到源文件旁（INTERFACE §〇）：清掉
    (ROOT / "examples" / "purpose-drive.ledger.jsonl").unlink(missing_ok=True)
    print(f"合计：{5 - bad} 通过，{bad} 未通过（jpp = {jpp}）")
    sys.exit(1 if bad else 0)


if __name__ == "__main__":
    main()
