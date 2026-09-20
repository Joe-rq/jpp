"""One sequence and one branch, shared planning, and the full direct-kernel peer."""
from __future__ import annotations

from dataclasses import dataclass
import hashlib
import inspect
import json
from pathlib import Path

from foundation import jv
from .core import branch, component, execute
from .fixtures import runtime
from .observation import Request

ACTION_LOG = []


def record_decision(decision, label):
    receipt = {"decision": decision.content, "label": label.content}
    ACTION_LOG.append(receipt)
    return receipt


RECORD = jv.register_action("composition.structure.record", fn=record_decision,
                            taint_out="inherit", reason="Local example receipt; appends only to ACTION_LOG")


@dataclass(frozen=True)
class Judgment:
    request: Request
    decision: object
    accepted: bool | None
    cause: str | None = None


def finish(judgment, receipts):
    return {"outcome": "accepted" if judgment.accepted is True else
                       "rejected" if judgment.accepted is False else "pending",
            "source": {"question": judgment.request.question.text,
                       "exit": judgment.decision.kind, "cause": judgment.cause,
                       "materials": [m.hash for m in judgment.request.state.resolved().all_mats],
                       "model_id": jv.current().model_id},
            "receipts": receipts}


@component("judge_flag", Request, Judgment, effects=("judge",))
def judge_flag(request):
    readings = jv.judge(request.state, request.question)
    decision = jv.cut(readings[0])
    match decision:
        case jv.Act():
            return Judgment(request, decision, True)
        case jv.Ignore():
            return Judgment(request, decision, False)
        case jv.Unsure(cause):
            return Judgment(request, decision, None, cause)
        case _:
            raise TypeError("This program expects a test question")


@component("is_accepted", Judgment, bool)
def is_accepted(judgment):
    return judgment.accepted is True


@component("record_acceptance", Judgment, dict, effects=("do",))
def record_acceptance(judgment):
    receipt = jv.do(RECORD, judgment.decision.as_mat(), jv.mat("accepted"), iter_seq=0)
    return finish(judgment, [receipt.content])


@component("retain_or_reject", Judgment, dict)
def retain_or_reject(judgment):
    return finish(judgment, [])


@component("record_acceptance_twice", Judgment, dict, effects=("do",))
def record_acceptance_twice(judgment):
    first = jv.do(RECORD, judgment.decision.as_mat(), jv.mat("accepted"), iter_seq=0)
    second = jv.do(RECORD, judgment.decision.as_mat(), jv.mat("secondary"), iter_seq=1)
    return finish(judgment, [first.content, second.content])


def composed():
    return judge_flag.then(branch(is_accepted, record_acceptance, retain_or_reject,
                                  name="route_judgment"), name="record_a_judgment")


@jv.program(budget=jv.Budget(calls=8, layers=8))
def direct_kernel(request):
    """Equivalent complete control flow, written without Component combinators."""
    readings = jv.judge(request.state, request.question)
    decision = jv.cut(readings[0])
    match decision:
        case jv.Act():
            judgment = Judgment(request, decision, True)
        case jv.Ignore():
            judgment = Judgment(request, decision, False)
        case jv.Unsure(cause):
            judgment = Judgment(request, decision, None, cause)
        case _:
            raise TypeError("This program expects a test question")
    if judgment.accepted is True:
        receipt = jv.do(RECORD, judgment.decision.as_mat(), jv.mat("accepted"), iter_seq=0)
        return finish(judgment, [receipt.content])
    return finish(judgment, [])


def request_for(value):
    record = {"unknown": True} if value is None else {"flag": value}
    return Request(jv.state(on=jv.mat(record, addr="structure:input")),
                   jv.test("flag 成立吗？", calib=jv.calib("demo.flag")))


def plan_summary(report):
    return {"calls": repr(report.calls), "questions": repr(report.questions),
            "layers": repr(report.layers), "do_calls": repr(report.do_calls),
            "warnings": report.warnings, "rendered": report.render()}


def run_composed(calculation, value, *, root=None, budget=None):
    ACTION_LOG.clear()
    result = execute(calculation, request_for(value), runtime(root=root), budget=budget)
    return {"value": result.value, "calls": result.stats["calls"],
            "layers": result.stats["layers"], "actual_actions": list(ACTION_LOG),
            "leaf_order": [event["component"] for event in result.trace if event["operation"] == "leaf"],
            "structure": result.structure}


def run_direct(value, *, root=None, budget=None):
    ACTION_LOG.clear()
    fn = direct_kernel
    if budget is not None:
        fn = jv.program(budget=budget)(direct_kernel.__jv_fn__)
    with runtime(root=root):
        answer = fn(request_for(value))
        stats = jv.stats()
    return {"value": answer, "calls": stats["calls"], "layers": stats["layers"],
            "actual_actions": list(ACTION_LOG)}


def run_integration():
    method = composed()
    changed = method.replace_at((1, 1), record_acceptance_twice)
    report = jv.plan(method.program())
    changed_report = jv.plan(changed.program())
    if not getattr(report, "structure", None):
        raise RuntimeError("The working jv.plan has not installed the agreed structure protocol")
    cases = []
    for value in (True, False, None):
        native = run_direct(value)
        library = run_composed(method, value)
        assert all(library[key] == native[key] for key in ("value", "calls", "layers", "actual_actions"))
        cases.append({"input": value, "direct": native, "composed": library})
    modified = run_composed(changed, True)
    assert len(modified["actual_actions"]) == 2
    assert changed_report.do_calls.value == 2 and report.do_calls.value == 1
    root = Path(__file__).resolve().parents[1]
    kernel_root = Path(jv.__file__).parent
    report_data = {"scope": "composition standard library: then + branch",
                   "observation_source": "synthetic; no model external calls", "cases": cases,
                   "original_plan": plan_summary(report), "changed_plan": plan_summary(changed_report),
                   "controlled_replacement": {"path": [1, 1], "execution": modified},
                   "kernel_files": {str(path): hashlib.sha256(path.read_bytes()).hexdigest()
                                    for path in (kernel_root / "plan.py", kernel_root / "runtime.py", kernel_root / "ir.py")}}
    source = '''# Generated complete equivalent programs; all shared helpers are below.
from dataclasses import dataclass
from pathlib import Path
import sys
ROOT = Path(__file__).resolve().parents[1]
sys.path[:0] = [str(ROOT.parents[1]), str(ROOT)]
from foundation import jv
from jev_compose import Request, branch, component, execute
from jev_compose.fixtures import runtime
ACTION_LOG = []

'''
    source += "# Shared input/result/action definitions (used identically by both):\n"
    source += inspect.getsource(record_decision) + "\n"
    source += 'RECORD = jv.register_action("composition.structure.record", fn=record_decision, taint_out="inherit")\n\n'
    source += inspect.getsource(Judgment) + "\n" + inspect.getsource(finish) + "\n"
    source += inspect.getsource(request_for) + "\n"
    source += "# Full direct control flow:\n" + inspect.getsource(direct_kernel.__jv_fn__) + "\n"
    source += "# Complete compositional leaves and wiring:\n"
    for part in (judge_flag, is_accepted, record_acceptance, retain_or_reject):
        source += inspect.getsource(part.function) + "\n"
    source += inspect.getsource(composed)
    source += '''
if __name__ == "__main__":
    for value in (True, False, None):
        with runtime():
            direct = direct_kernel(request_for(value))
        combined = execute(composed(), request_for(value), runtime()).value
        assert direct == combined
        print(f"{value!r}: {combined['outcome']} (direct == composed)")
'''
    (root / "out" / "direct_vs_composed.py").write_text(source)
    return report_data


def main():
    root = Path(__file__).resolve().parents[1]
    (root / "out").mkdir(exist_ok=True)
    report = run_integration()
    (root / "out" / "ir-integration.json").write_text(json.dumps(report, ensure_ascii=False, indent=2) + "\n")
    print("共同结构、共同计划器与执行贯通；是/否/未决的直接写法与组合写法一致。")
    print("替换接受分支：计划动作数 1 → 2，实际动作数 1 → 2；原程序保持 1。")
    print(root / "out" / "ir-integration.json")
