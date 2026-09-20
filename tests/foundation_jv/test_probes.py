"""E-PROBE-10：十条可运行探针在 FakeClient 下跑通 + 真值生成器正负平衡。$0。"""

from __future__ import annotations

import pytest

import foundation.jv as jv
from foundation.jv.probes import PROBES
from foundation.jv.probes._common import fake_rule, run_probe

FAST = ["p13", "p17", "p42", "p49", "p81", "p85", "p87", "p87s"]      # 秒级
SLOW = ["p22", "p45", "p77", "p77s"]                                 # 子进程多，十秒级


@pytest.mark.parametrize("name", FAST + SLOW)
def test_probe_runs_under_fake_client(name):
    p = PROBES[name]
    r = run_probe(p, lambda: jv.FakeClient(rule=fake_rule), None, 12, 0, replay=False)
    b = r["builder"]
    assert b["metrics"]["n"] == 12
    assert b["layers"] >= 1 and b["calls"] >= 1
    assert r["bare"]["calls"] >= 1
    if p.baseline:
        assert r["baseline"] is None or r["baseline"]["metrics"]["n"] == 12


@pytest.mark.parametrize("name", [n for n in FAST + SLOW if PROBES[n].kind == "noul"])
def test_truth_generator_balanced(name):
    ss = PROBES[name].make_samples(24, 0)
    pos = sum(1 for s in ss if bool(s.truth))
    assert 6 <= pos <= 18, (name, pos)          # p77 由设计是 1/3 正例（枚举器每目标 1 对 2 错），其余 ≈ 一半
    assert len({s.id for s in ss}) == 24


@pytest.mark.parametrize("name", [n for n in FAST + SLOW if PROBES[n].kind != "noul"])
def test_truth_generator_covers_classes(name):
    ss = PROBES[name].make_samples(24, 0)
    classes = {s.truth for s in ss}
    assert len(classes) >= 2, (name, classes)
    counts = {c: sum(1 for s in ss if s.truth == c) for c in classes}
    assert max(counts.values()) <= 2 * min(counts.values()) + 2, counts


def test_no_unconsumed_unsure_and_no_static_errors():
    """十条程序静态检查零错（探针不改语言、不关检查器）。"""
    import importlib
    for name in ["p13", "p17", "p22", "p42", "p45", "p49", "p77", "p81", "p85", "p87"]:
        mod = importlib.import_module(f"foundation.jv.probes.{name}")
        progs = [v for v in vars(mod).values() if getattr(v, "__jv_program__", False)]
        assert progs, name
        for prog in progs:
            rep = prog.check()
            assert not rep.errors, (name, rep.errors)
