"""Two complete programs: adaptive inquiry and finite-domain expression synthesis."""
from __future__ import annotations

import ast
from dataclasses import dataclass, replace
from foundation import jv
from .algorithms import feedback, inquire
from .core import Component, Iteration, component
from .observation import Observation, Request, observe


@dataclass(frozen=True)
class InquiryState:
    material: jv.Mat
    remaining: tuple[int, ...]
    observations: tuple[Observation, ...] = ()
    paused: bool = False


def balanced(candidates):
    return candidates[:max(1, len(candidates) // 2)]


def sequential(candidates):
    return candidates[:1]


def make_inquiry(partition=balanced, *, limit=16) -> Component:
    @component("prepare_membership", InquiryState, Request, effects=("transform",))
    def prepare(state):
        subset = tuple(partition(state.remaining))
        if not subset or set(subset) == set(state.remaining) or not set(subset) <= set(state.remaining):
            raise ValueError("partition must be a nonempty proper subset of remaining candidates")
        subset_mat = jv.mat(list(subset))
        material = jv.transform(_membership_material, state.material, subset_mat)
        question = jv.test(f"目标编号是否属于这 {len(subset)} 个候选组成的 subset？ [membership]",
                           calib=jv.calib("demo.membership"))
        return Request(jv.state(on=material), question, "membership", subset)

    @component("update_candidates", tuple, InquiryState)
    def update(pair):
        state, answer = pair
        history = state.observations + (answer,)
        if not answer.resolved:
            return replace(state, observations=history, paused=True)
        selected = set(answer.request.tag)
        remaining = tuple(x for x in state.remaining if (x in selected) == answer.value)
        return replace(state, remaining=remaining, observations=history)

    @component("inquiry_done", InquiryState, bool)
    def done(state):
        return len(state.remaining) <= 1 or state.paused

    return inquire(prepare, update, done, limit=limit, name=f"inquiry_{partition.__name__}")


def _membership_material(material, subset):
    return {**material.content, "subset": subset.content}


EXPRESSIONS = ("x", "-x", "0", "1", "x * x", "x + 1", "x - 1", "x * 2",
               "x if x >= 0 else -x", "x if x >= 0 else 0", "0 if x >= 0 else -x",
               "1 if x > 0 else 0", "x if x <= 2 else 2")
_NODES = (ast.Expression, ast.BinOp, ast.UnaryOp, ast.IfExp, ast.Compare, ast.Name,
          ast.Load, ast.Constant, ast.Add, ast.Sub, ast.Mult, ast.USub, ast.UAdd,
          ast.GtE, ast.LtE, ast.Gt, ast.Lt, ast.Eq, ast.NotEq)


def evaluate_expression(expression: str, x: int) -> int:
    tree = ast.parse(expression, mode="eval")
    if any(not isinstance(node, _NODES) for node in ast.walk(tree)):
        raise ValueError("Expression outside the finite demo grammar")
    if any(isinstance(node, ast.Name) and node.id != "x" for node in ast.walk(tree)):
        raise ValueError("Only x is bound in the demo grammar")
    return eval(compile(tree, "<generated-expression>", "eval"), {"__builtins__": {}}, {"x": x})


def enumerate_candidates(prompt, context, n, retry_seq):
    data = context[-1].content
    constraints = data.get("counterexamples", [])
    rejected = set(data.get("tried", []))
    candidates = []
    for expression in EXPRESSIONS:
        if expression not in rejected and all(evaluate_expression(expression, e["x"]) == e["expected"]
                                              for e in constraints):
            candidates.append({"expression": expression})
    return candidates[:n]


def verify_expression(candidate, specification):
    expression = candidate.content["expression"]
    spec = specification.content
    if not spec["domain"] or len(spec["domain"]) != len(spec["expected"]):
        raise ValueError("A finite specification needs nonempty, equally sized input and expected lists")
    for x, expected in zip(spec["domain"], spec["expected"]):
        actual = evaluate_expression(expression, x)
        if actual != expected:
            return {"passed": False, "counterexample": {"x": x, "expected": expected, "actual": actual},
                    "expression": expression, "domain_size": len(spec["domain"])}
    return {"passed": True, "expression": expression, "domain_size": len(spec["domain"]),
            "guarantee": "All declared finite-domain inputs were executed"}


if hasattr(jv, "register_action"):
    VERIFY = jv.register_action("composition.verify_expression", fn=verify_expression, taint_out="trusted",
                                reason="Repository checker executes a restricted finite expression grammar against every declared input")
else:
    # The immutable kernel snapshot predates the public action registry.
    VERIFY = jv.Action("composition.verify_expression", fn=verify_expression, taint_out="trusted")


@dataclass(frozen=True)
class Trial:
    candidate: jv.Mat
    report: jv.Mat
    selection: Observation
    complexity: Observation


@dataclass(frozen=True)
class SynthesisState:
    specification: jv.Mat
    trials: tuple[Trial, ...] = ()
    solution: jv.Mat | None = None
    exhausted: bool = False


def _history_material(*reports):
    return {"counterexamples": [r.content["counterexample"] for r in reports if "counterexample" in r.content],
            "tried": [r.content["expression"] for r in reports if "expression" in r.content]}


@component("generate_candidates", SynthesisState, list, effects=("gen", "transform"))
def propose_expressions(state):
    history = jv.transform(_history_material, *(t.report for t in state.trials))
    return jv.gen("Generate expressions satisfying the counterexamples", ctx=[state.specification, history],
                  n=3, retry_seq=len(state.trials))


@component("rank_and_verify", tuple, list, effects=("judge", "do"))
def inspect_expressions(pair):
    state, candidates = pair
    if not candidates:
        return []
    q = jv.select("哪一个表达式更简单且值得先检验？ [pick expression]", calib=jv.calib("demo.pick"))
    selection = observe(Request(jv.state(on=state.specification, over=candidates), q, "candidate_selection"))
    # Uncertainty changes the search order, never the definition of validity.
    candidate = candidates[selection.value if selection.resolved else 0]
    outcome = jv.do(VERIFY, candidate, state.specification, iter_seq=len(state.trials))
    report = jv.on_fail(outcome, alt=jv.mat({"passed": False, "failure": "verifier_failed"}))
    report.content                        # Force the actual checker before using its verdict.
    complexity_q = jv.measure("表达式的结构复杂度？ [complexity]", scale=("短", "中", "长"),
                              calib=jv.calib("demo.complexity"))
    complexity = observe(Request(jv.state(on=candidate), complexity_q, "expression_complexity"))
    return [Trial(candidate, report, selection, complexity)]


@component("absorb_counterexamples", tuple, SynthesisState)
def update_synthesis(pair):
    state, reports = pair
    if not reports:
        return replace(state, exhausted=True)
    trials = state.trials + tuple(reports)
    valid = next((t.candidate for t in reports if t.report.content.get("passed")), None)
    return replace(state, trials=trials, solution=valid)


@component("synthesis_done", SynthesisState, bool)
def synthesis_done(state):
    return state.solution is not None or state.exhausted


def make_synthesis(*, proposer=propose_expressions, inspector=inspect_expressions, limit=8) -> Component:
    return feedback(proposer, inspector, update_synthesis, synthesis_done,
                    limit=limit, name="construct_verified_expression")


def absolute_value_problem() -> SynthesisState:
    domain = list(range(-4, 5))
    return SynthesisState(jv.mat({"goal": "返回 x 的绝对值", "domain": domain,
                                 "expected": [abs(x) for x in domain]}))


@component("extract_identified_target", Iteration, int)
def identified_target(result):
    state = result.state
    if len(state.remaining) != 1 or state.paused:
        raise ValueError("The inquiry has not identified one candidate")
    return state.remaining[0]


def make_solver_from_strategy(strategy: Component, *, name="chosen_solver") -> Component:
    """A program returns a solver; bind explicitly calls that resulting program."""
    effects = frozenset({"judge", "transform"})
    from .core import identity
    return identity(InquiryState).bind(strategy, Iteration, effects=effects, name=name)
