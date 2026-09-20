"""Partial results with explicit, composable continuation methods.

State belongs to the algorithm. Execution, budgets and observations still belong
to foundation.jv; this module only constructs ordinary Components.
"""
from __future__ import annotations
from dataclasses import dataclass
from typing import Any

from .core import Component, CompositionError


@dataclass(frozen=True)
class Partial:
    value: Any
    pending: tuple
    evidence: tuple
    continuation: Component | None

    def satisfies(self, requirement: Component) -> bool:
        if requirement.output_type is not bool:
            raise CompositionError("A result requirement must return bool")
        return requirement(self.value)


def checkpoint(state, *, view: Component, pending: Component, evidence: Component,
               advance: Component | None) -> Partial:
    """Expose a state and a continuation accepting a replacement strategy.

    view/pending/evidence are declared effect-free projections of the same state.
    advance accepts (state, strategy Component), returns the next state, and owns
    the algorithm's effects. Only invoking continuation performs advancement.
    """
    for projection in (view, pending, evidence):
        if projection.effects:
            raise CompositionError("Checkpoint projections must declare no effects")
    remaining = tuple(pending(state))
    result, sources = view(state), tuple(evidence(state))
    if not remaining or advance is None:
        return Partial(result, remaining, sources, None)
    if advance.input_type is not tuple:
        raise CompositionError("advance must accept (state, strategy) as a tuple")

    def carry(strategy):
        if not strategy.effects <= advance.effects:
            raise CompositionError("Strategy capabilities exceed the advance contract")
        return state, strategy

    def expose(updated):
        return checkpoint(updated, view=view, pending=pending, evidence=evidence, advance=advance)

    continuation = Component("carry_checkpoint", Component, tuple, carry).then(advance).then(
        Component("expose_checkpoint", advance.output_type, Partial, expose),
        name="continue_checkpoint")
    return Partial(result, remaining, sources, continuation)


def map_partial(project: Component) -> Component:
    """Apply an effect-free algorithm to a current result and all continuations.

    Pending identities and evidence are retained unchanged. Exact projections may
    be recomputed when input grows; observations and actions are not repeated here.
    """
    if project.effects:
        raise CompositionError("map_partial requires a declared effect-free result projection")

    def apply(partial):
        value = project(partial.value)
        continuation = (partial.continuation.then(map_partial(project), name=f"continue_{project.name}")
                        if partial.continuation is not None else None)
        return Partial(value, partial.pending, partial.evidence, continuation)
    return Component(f"partial_{project.name}", Partial, Partial, apply)


def continue_with(strategy: Component, *, effects: frozenset[str]) -> Component:
    """A reusable Partial→Partial method, suitable for bind/then/product/iterate.

    effects bounds the selected continuation. checkpoint also requires strategy
    capabilities to fit the advance declaration; planning keeps bind symbolic.
    Terminal packets are returned unchanged. The original packet is never edited.
    """
    from .core import identity

    def choose(partial):
        if partial.continuation is None:
            return identity(Partial)
        supply = Component("supply_strategy", Partial, Component, lambda _: strategy)
        return supply.then(partial.continuation)

    factory = Component("choose_continuation", Partial, Component, choose)
    return identity(Partial).bind(factory, Partial, effects=frozenset(effects),
                                  name=f"continue_with_{strategy.name}")
