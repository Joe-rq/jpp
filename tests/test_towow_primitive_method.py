"""Independent tests for the Towow five-primitive discovery example."""

import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT))
sys.path.insert(0, str(ROOT / "src"))
sys.path.insert(0, str(ROOT / "examples"))

from jev_compose import execute
from jev_compose.fixtures import runtime
from towow_primitive_method import (
    build_towow_method, population_cards, signal_spans,
)
from primitive_ir_method import build_typing_pipeline


def run_discovery():
    return execute(build_towow_method(), population_cards(), runtime()).value


def test_discovery_finds_direct_supporters_only():
    report = run_discovery()
    direct = {c.person for c in report.candidates if c.direct}
    # zhou offers the companion-drive service himself; mei offers listening.
    # lan only knows a driver (referral), tang only organises, an/he only join.
    assert direct == {"zhou", "mei"}


def test_zhou_availability_unresolved_until_the_update():
    report = run_discovery()
    zhou = next(c for c in report.candidates if c.person == "zhou")
    assert zhou.unresolved_before == 1
    assert zhou.unresolved_after == 0
    assert zhou.ready
    availability = next(s for s in zhou.spans if "unconfirmed" in s["flags"])
    assert availability["type"] == "Occurrence"


def test_ready_list_matches_the_scenario_story():
    report = run_discovery()
    assert [c.person for c in report.candidates if c.ready] == ["mei", "zhou"]


def test_signal_compiles_to_pure_occurrences():
    compiled = execute(build_typing_pipeline(), signal_spans(), runtime()).value
    assert [p.type for p in compiled] == ["Occurrence"] * len(compiled)
    assert all(p.entropy <= 0.2 for p in compiled)


def test_construction_makes_no_kernel_calls():
    rt = runtime()
    build_towow_method()
    assert rt.client.calls == 0


def test_resolved_false_readiness_is_not_ready():
    """A span judged not-ready blocks the candidate even with no unknowns."""
    from primitive_ir_method import NoteSpan
    from towow_primitive_method import PersonCard

    mei_like = PersonCard("mei", "梅", (
        NoteSpan(0, "提供支持性倾听。", "Object"),
        # entropy 0.30 > Rule threshold 0.2 -> resolved False readiness
        NoteSpan(1, "若 output 突破 ninety before Thursday.", "Rule"),
    ))
    report = execute(build_towow_method(), (mei_like,), runtime()).value
    mei = report.candidates[0]
    assert mei.direct
    assert mei.unresolved_after == 0
    assert mei.resolved_false == 1
    assert not mei.ready
    assert [c.person for c in report.candidates if c.ready] == []
