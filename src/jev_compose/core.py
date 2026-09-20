"""Typed, inspectable function composition. All effects execute in foundation.jv."""
from __future__ import annotations

from contextvars import ContextVar
from dataclasses import dataclass, field
from typing import Any, Callable, Generic, TypeVar, get_origin

from foundation import jv

I = TypeVar("I")
O = TypeVar("O")
T = TypeVar("T")
_events: ContextVar[list | None] = ContextVar("composition_events", default=None)


class CompositionError(TypeError):
    pass


def _label(t):
    return getattr(t, "__name__", str(t))


def _accepts(expected, actual):
    if expected is Any or actual is Any:
        return True
    expected, actual = get_origin(expected) or expected, get_origin(actual) or actual
    return expected == actual or (isinstance(expected, type) and isinstance(actual, type)
                                  and issubclass(actual, expected))


def _check_value(t, value, where):
    origin = get_origin(t) or t
    if t is not Any and isinstance(origin, type) and not isinstance(value, origin):
        raise CompositionError(f"{where}: expected {_label(t)}, got {type(value).__name__}")


@dataclass(frozen=True)
class Component(Generic[I, O]):
    """A reusable calculation, with a signature and visible composition structure.

    Calling it inside a jv program uses that program's execution scope. `program`
    adds the public jv program boundary for independent or nested use.
    """
    name: str
    input_type: Any
    output_type: Any
    function: Callable[[I], O] = field(repr=False, compare=False)
    effects: frozenset[str] = frozenset()
    operation: str = "leaf"
    children: tuple[Component, ...] = ()
    parameters: tuple[tuple[str, Any], ...] = ()
    warnings: tuple[str, ...] = ()

    def __call__(self, value: I) -> O:
        _check_value(self.input_type, value, f"{self.name} input")
        events = _events.get()
        if events is not None:
            events.append({"component": self.name, "operation": self.operation})
        result = self.function(value)
        _check_value(self.output_type, result, f"{self.name} output")
        return result

    def then(self, other: Component[O, T], *, name: str | None = None) -> Component[I, T]:
        if not _accepts(other.input_type, self.output_type):
            raise CompositionError(f"Cannot connect {self.name}:{_label(self.output_type)} "
                                   f"to {other.name}:{_label(other.input_type)}")

        def sequence(value):
            return other(self(value))

        return Component(name or f"{self.name} → {other.name}", self.input_type, other.output_type,
                         sequence, self.effects | other.effects, "then", (self, other))

    def bind(self, factory: Callable[[O], Component[O, T]], output_type: Any,
             *, effects: frozenset[str], name: str = "bind") -> Component[I, T]:
        """Choose/construct the next component from this result, then execute it.

        To return a component as data, use an ordinary component with Component
        as output_type. `bind` explicitly executes the selected continuation.
        """
        allowed = frozenset(effects)

        def continuation(value):
            intermediate = self(value)
            next_component = factory(intermediate)
            if not isinstance(next_component, Component):
                raise CompositionError("bind factory must return a Component")
            if not _accepts(output_type, next_component.output_type):
                raise CompositionError("bind continuation has an incompatible result type")
            if not next_component.effects <= allowed:
                raise CompositionError("bind continuation uses undeclared effects")
            return next_component(intermediate)

        return Component(name, self.input_type, output_type, continuation,
                         self.effects | allowed, "bind", (self,),
                         (("factory", getattr(factory, "__name__", "factory")),))

    def describe(self) -> dict:
        return {"name": self.name, "operation": self.operation,
                "input": _label(self.input_type), "output": _label(self.output_type),
                "effects": sorted(self.effects), "parameters": dict(self.parameters),
                "warnings": list(self.warnings),
                "children": [child.describe() for child in self.children]}

    def program(self, *, name: str | None = None, budget=None):
        """Compile this component's entry to the existing public jv decorator."""
        calculation = self

        def entry(value):
            return calculation(value)

        entry.__name__ = name or self.name
        return jv.program(budget=budget or jv.Budget())(entry)


def component(name: str, input_type: Any, output_type: Any, *, effects=()):
    """Define a leaf and run the kernel's checker on its visible source once."""
    def decorate(fn):
        report = jv.check(fn)
        if report.errors:
            raise CompositionError(f"{name}: " + "\n".join(report.errors))
        return Component(name, input_type, output_type, fn, frozenset(effects),
                         warnings=tuple(report.warnings))
    return decorate


def identity(t: Any = Any) -> Component:
    return Component("identity", t, t, lambda value: value, operation="identity")


def branch(predicate: Component, yes: Component, no: Component, *, name="branch") -> Component:
    if predicate.output_type is not bool:
        raise CompositionError("branch predicate must return bool; handle an unresolved observation explicitly")
    if yes.input_type != no.input_type or yes.output_type != no.output_type:
        raise CompositionError("branch arms must have the same input/output signature")
    if not _accepts(predicate.input_type, yes.input_type):
        raise CompositionError("branch predicate has incompatible input")

    def choose(value):
        return (yes if predicate(value) else no)(value)

    return Component(name, yes.input_type, yes.output_type, choose,
                     predicate.effects | yes.effects | no.effects, "branch", (predicate, yes, no))


def product(*parts: Component, name="product") -> Component:
    """Independent result fields. Does not speculate, reorder or parallelize effects.

    Use batch_observe for independent questions that should share kernel layers.
    """
    if not parts:
        raise CompositionError("product requires at least one component")
    if any(p.input_type != parts[0].input_type for p in parts):
        raise CompositionError("product components must accept the same input type")
    return Component(name, parts[0].input_type, tuple,
                     lambda value: tuple(p(value) for p in parts),
                     frozenset().union(*(p.effects for p in parts)), "product", parts)


@dataclass(frozen=True)
class Iteration(Generic[T]):
    state: T
    steps: int
    reason: str                  # done | limit


def iterate(step: Component, done: Component, *, limit: int, name="iterate") -> Component:
    if step.input_type != step.output_type or not _accepts(done.input_type, step.input_type):
        raise CompositionError("iterate requires S→S and S→bool")
    if done.output_type is not bool or limit < 0:
        raise CompositionError("iterate needs a boolean stop condition and nonnegative limit")

    def loop(value):
        state = value
        for n in range(limit + 1):
            if done(state):
                return Iteration(state, n, "done")
            if n < limit:
                state = step(state)
        return Iteration(state, limit, "limit")

    return Component(name, step.input_type, Iteration, loop, step.effects | done.effects,
                     "iterate", (step, done), (("limit", limit),))


@dataclass(frozen=True)
class Execution(Generic[T]):
    value: T
    stats: dict
    structure: dict
    trace: tuple[dict, ...]


def execute(calculation: Component, value, runtime, *, name=None, budget=None) -> Execution:
    events = []
    token = _events.set(events)
    try:
        with runtime:
            answer = calculation.program(name=name, budget=budget)(value)
            stats = jv.stats()
            stats["effect_requests"] = {kind: runtime.stats.get(kind, 0)
                                        for kind in ("gen", "do", "ask", "transform")}
        return Execution(answer, stats, calculation.describe(), tuple(events))
    finally:
        _events.reset(token)
