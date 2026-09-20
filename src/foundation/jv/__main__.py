"""jv 命令行：

  python -m foundation.jv plan  foundation.jv.examples.six:定位回归      # 计划期估计（J-07 / J-10）
  python -m foundation.jv stats [--no-fuse --no-lift --no-fission --no-lower --no-schedule --no-plan --no-ledger]
                                 [--root DIR] [--replay]                  # 跑六条示例，出层数/融合率/账本命中/钱
  python -m foundation.jv ablate [--root DIR]                             # 七个开关逐个关，出消融表
  python -m foundation.jv check foundation.jv.examples.six:定位回归      # 静态检查（J 规则）
"""

from __future__ import annotations

import argparse
import importlib
import json
import sys

PASSES = ("lift", "fuse", "fission", "lower", "schedule", "plan", "ledger")


def _load(spec: str):
    mod, _, name = spec.partition(":")
    m = importlib.import_module(mod)
    return getattr(m, name) if name else m


def _passes_from(args) -> dict:
    return {p: not getattr(args, f"no_{p}") for p in PASSES}


def cmd_plan(args) -> int:
    import foundation.jv as jv
    from foundation.jv.examples import six
    fn = _load(args.target)
    with jv.Runtime(client=jv.FakeClient(rule=six.fake_rule), generator=six.fake_generator) as rt:
        six.calib_all(rt)
        rep = jv.plan(fn, rt=rt)
    print(rep.render())
    return 0


def cmd_check(args) -> int:
    import foundation.jv as jv
    fn = _load(args.target)
    rep = jv.check(getattr(fn, "__jv_fn__", fn))
    for e in rep.errors:
        print("E ", e)
    for w in rep.warnings:
        print("W ", w)
    print(f"{len(rep.errors)} 错 {len(rep.warnings)} 警")
    return 1 if rep.errors else 0


def cmd_stats(args) -> int:
    from foundation.jv.examples import six
    passes = _passes_from(args)
    from foundation.jv.examples import seven, strength
    def _rows():
        return (six.run_all(root=args.root, passes=passes) + [seven.run_seven(root=args.root, passes=passes)]
                + [{k: v for k, v in r.items() if k != "结果"} for r in strength.run_all(root=args.root, passes=passes)])
    rows = _rows()
    if args.replay:
        rows = _rows()
    print(six.stats_table(rows, title=f"passes={ {k for k, v in passes.items() if not v} or '全开'}"))
    if args.json:
        print(json.dumps(rows, ensure_ascii=False, default=str))
    return 0


def cmd_ablate(args) -> int:
    from foundation.jv.examples import six
    print(six.ablation_table(root=args.root))
    return 0


def main(argv: list[str] | None = None) -> int:
    ap = argparse.ArgumentParser(prog="python -m foundation.jv")
    sub = ap.add_subparsers(dest="cmd", required=True)
    p = sub.add_parser("plan"); p.add_argument("target"); p.set_defaults(fn=cmd_plan)
    c = sub.add_parser("check"); c.add_argument("target"); c.set_defaults(fn=cmd_check)
    s = sub.add_parser("stats")
    for name in PASSES:
        s.add_argument(f"--no-{name}", action="store_true")
    s.add_argument("--root", default=None)
    s.add_argument("--replay", action="store_true", help="跑两遍，看第二遍账本命中")
    s.add_argument("--json", action="store_true")
    s.set_defaults(fn=cmd_stats)
    a = sub.add_parser("ablate"); a.add_argument("--root", default=None); a.set_defaults(fn=cmd_ablate)
    args = ap.parse_args(argv)
    return args.fn(args)


if __name__ == "__main__":
    sys.exit(main())
