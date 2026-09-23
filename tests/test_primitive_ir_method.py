"""Independent tests for the five-primitive IR method; no real model calls."""

import sys
from pathlib import Path

import pytest

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT))
sys.path.insert(0, str(ROOT / "src"))

from jev_compose import component, execute
from jev_compose.fixtures import runtime
from primitive_ir_method import (
    DEFAULT_THRESHOLDS, AnalystNote, NoteReport, build_note_compiler,
    check_postconditions, diplomacy_note, judge_primitive_types, policy_note,
    sample_note, segment_note,
)


def run_compiler(note=None, thresholds=None) -> NoteReport:
    return execute(build_note_compiler(thresholds), note or sample_note(),
                   runtime()).value


def test_full_pipeline_types_fields_and_card():
    report = run_compiler()
    assert isinstance(report, NoteReport)
    assert report.card == "schedule"
    assert [p.type for p in report.primitives] == ["Occurrence", "Occurrence", "Rule", "Directive"]
    assert [p.entropy for p in report.primitives] == [0.20, 0.20, 0.15, 0.45]
    # Merged pairs survive as fields parsed from the text, on every type.
    rule, directive = report.primitives[2], report.primitives[3]
    assert rule.trigger == "conditional" and rule.force == "may"
    assert directive.force == "must" and directive.trigger == "standing"
    assert report.focus == ("schedule: Trim our airline exposure before the OPEC meeting on Thursday.",)
    assert len(report.clarifications) == 1
    assert "missing quantity" in report.clarifications[0]
    assert report.overall_entropy == pytest.approx(0.25)
    check_postconditions(report, DEFAULT_THRESHOLDS)


def test_missing_quantity_stays_unresolved_and_bounds_count():
    report = run_compiler()
    unclear = report.primitives[3]
    assert unclear.entropy > DEFAULT_THRESHOLDS["Directive"]
    assert not unclear.readiness.resolved
    assert unclear.readiness.value is None
    assert unclear.readiness.cause is not None
    assert report.ready.value is None
    assert (report.ready.lower, report.ready.upper) == (3, 4)
    assert len(report.ready.unresolved) == 1
    assert all(p.readiness.resolved for p in report.primitives[:3])


def test_thresholds_are_a_dial_not_a_constant():
    strict = run_compiler(thresholds={"Rule": 0.1})
    rule = strict.primitives[2]
    # entropy 0.15 now exceeds the Rule bar: resolved as not-ready, and the
    # clarify-don't-guess contract demands a clarification for it.
    assert rule.readiness.resolved and rule.readiness.value is False
    assert len(strict.clarifications) == 2
    assert any(c.startswith("span 2:") for c in strict.clarifications)
    assert (strict.ready.lower, strict.ready.upper) == (2, 3)
    # Routing by capability is unaffected: the must-directive still schedules.
    assert strict.card == "schedule"


def test_routing_follows_capability_not_dominant_type():
    assert run_compiler(diplomacy_note()).card == "reflection"
    assert run_compiler(policy_note()).card == "monitor"


def test_construction_makes_no_kernel_calls():
    rt = runtime()
    build_note_compiler()
    assert rt.client.calls == 0


def test_type_judgments_batch_into_fused_kernel_calls():
    rt = runtime()
    spans = segment_note(sample_note())
    result = execute(judge_primitive_types, spans, rt)
    assert [j.type for j in result.value] == ["Occurrence", "Occurrence", "Rule", "Directive"]
    # Each span is its own state; the runtime currently fuses the two
    # permutation items of one select into a single kernel call, so four
    # spans cost four calls and eight questions.  These numbers pin the
    # present fusing behaviour — if the kernel's fusing strategy changes,
    # update both assertions to match.
    assert rt.client.calls == 4
    assert result.stats["questions"] == 8


def test_compiled_method_runs_as_nested_component():
    compiler = build_note_compiler()

    @component("count_ready", AnalystNote, int)
    def count_ready(note):
        return sum(p.readiness.resolved and p.readiness.value is True
                   for p in compiler(note).primitives)

    result = execute(count_ready, sample_note(), runtime())
    assert result.value == 3


def test_replace_at_swaps_judgment_stage_and_original_stays_intact():
    from primitive_ir_method import JudgedSpan

    @component("ablate_types_to_participant", tuple, tuple)
    def ablate_types(judged):
        """Lexical-only ablation: keep the judgments, flatten every type."""
        return tuple(JudgedSpan(j.span, "Participant", j.type_observation) for j in judged)

    compiler = build_note_compiler()
    # Index path into build_note_compiler's tree, segment by segment:
    #   (0) the pre-bind pipeline (type_score_assess)
    #   (1) the typing pipeline attached after segment_note
    #   (0) its score_and_parse stage
    #   (0) the judge leaf being replaced
    # The path is tightly coupled to the pipeline's internal shape; any
    # structural change requires updating it to match.
    ablated = compiler.replace_at((0, 1, 0, 0),
                                  judge_primitive_types.then(ablate_types))
    report = execute(ablated, sample_note(), runtime()).value
    assert [p.type for p in report.primitives] == ["Participant"] * 4
    # Capability routing reads the text, not the type tags: flattening every
    # type leaves the schedule card intact.
    assert report.card == "schedule"
    # The original method is untouched and routes the same way.
    assert run_compiler(sample_note()).card == "schedule"


def test_unknown_type_still_clarifies_and_focuses():
    """A span whose type the judge cannot place must not vanish from output."""
    from primitive_ir_method import JudgedSpan

    @component("judge_no_type", tuple, tuple, effects=("judge",))
    def judge_no_type(spans):
        observed = judge_primitive_types(spans)
        return tuple(JudgedSpan(o.span, None, o.type_observation) for o in observed)

    note = AnalystNote("Something happened.", ((0, None),))
    # Same coupled path as the ablation test above: (0) pre-bind pipeline,
    # (1) typing pipeline, (0) score_and_parse, (0) judge leaf.
    compiler = build_note_compiler().replace_at((0, 1, 0, 0), judge_no_type)
    report = execute(compiler, note, runtime()).value
    assert report.card == "reflection"
    assert len(report.primitives) == 1 and report.primitives[0].type is None
    assert len(report.ready.unresolved) == 1
    assert len(report.clarifications) == 1
    assert "span 0" in report.clarifications[0]
    assert report.focus and "unresolved" in report.focus[0]
