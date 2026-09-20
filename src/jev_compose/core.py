"""Typed, inspectable function composition. All effects execute in foundation.jv."""
from __future__ import annotations

from contextvars import ContextVar
from dataclasses import dataclass, field
from types import MappingProxyType
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
    function: Callable[[I], O] | None = field(repr=False, compare=False)
    effects: frozenset[str] = frozenset()
    operation: str = "leaf"
    children: tuple[Component, ...] = ()
    parameters: tuple[tuple[str, Any], ...] = ()
    warnings: tuple[str, ...] = ()

    def __post_init__(self):
        # These nodes are executable structure, not labels attached to closures.
        if self.operation in ("then", "branch"):
            if self.function is not None:
                raise CompositionError("Structural then/branch nodes cannot carry a separate execution closure")
            expected = 2 if self.operation == "then" else 3
            if len(self.children) != expected:
                raise CompositionError(f"{self.operation} requires {expected} children")
            declared = frozenset().union(*(c.effects for c in self.children))
            if self.effects != declared:
                raise CompositionError("Structural capabilities must match children; use replace_at to rebuild")

    def __call__(self, value: I) -> O:
        _check_value(self.input_type, value, f"{self.name} input")
        events = _events.get()
        if events is not None:
            events.append({"component": self.name, "operation": self.operation})
        if self.operation == "then":
            first, second = self.children
            result = second(first(value))
        elif self.operation == "branch":
            predicate, yes, no = self.children
            result = (yes if predicate(value) else no)(value)
        else:
            result = self.function(value)
        _check_value(self.output_type, result, f"{self.name} output")
        return result

    def then(self, other: Component[O, T], *, name: str | None = None) -> Component[I, T]:
        if not _accepts(other.input_type, self.output_type):
            raise CompositionError(f"Cannot connect {self.name}:{_label(self.output_type)} "
                                   f"to {other.name}:{_label(other.input_type)}")

        return Component(name or f"{self.name} → {other.name}", self.input_type, other.output_type,
                         None, self.effects | other.effects, "then", (self, other))

    def bind(self, factory: Callable[[O], Component[O, T]], output_type: Any,
             *, effects: frozenset[str], factory_effects: frozenset[str] | None = None,
             name: str = "bind") -> Component[I, T]:
        """Choose/construct the next component from this result, then execute it.

        To return a component as data, use an ordinary component with Component
        as output_type. `bind` explicitly executes the selected continuation.
        `effects` bounds the returned continuation, not the factory. A factory
        may itself call capabilities: declare factory_effects, pass a Component,
        or leave it unknown (represented by '*'). Empty is a declaration, never
        a proof of purity. The shared planner treats bind as an opaque boundary.
        """
        allowed = frozenset(effects)
        if isinstance(factory, Component):
            if not _accepts(factory.input_type, self.output_type) or not _accepts(Component, factory.output_type):
                raise CompositionError("bind factory component must accept the preceding result and return Component")
            if factory_effects is None:
                factory_effects = factory.effects
            elif not factory.effects <= frozenset(factory_effects):
                raise CompositionError("factory_effects omits the factory component's declared capabilities")
        factory_caps = frozenset({"*"}) if factory_effects is None else frozenset(factory_effects)
        factory_contract = "unknown" if "*" in factory_caps else "declared"

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
                         self.effects | allowed | factory_caps, "bind", (self,),
                         (("factory", getattr(factory, "name", getattr(factory, "__name__", "factory"))),
                          ("factory_effects", None if factory_effects is None else tuple(sorted(factory_caps))),
                          ("factory_contract", factory_contract),
                          ("continuation_effects", tuple(sorted(allowed)))),
                         ("W-dynamic: bind factory and continuation execute at runtime; capability declarations are not purity proofs",))

    def describe(self) -> dict:
        return {"name": self.name, "operation": self.operation,
                "input": _label(self.input_type), "output": _label(self.output_type),
                "effects": sorted(self.effects), "parameters": dict(self.parameters),
                "warnings": list(self.warnings),
                "children": [child.describe() for child in self.children]}

    def structure(self):
        """Versioned shared-planner input, projected from the executing nodes.

        Only then/branch/identity/leaf are structural in v1. Other operations
        explicitly remain opaque. Callables are retained, not serialized.
        """
        def node(part):
            structural = part.operation in ("then", "branch", "identity", "leaf")
            operation = part.operation if structural else "opaque"
            data = {"operation": operation, "name": part.name,
                    "input": _label(part.input_type), "output": _label(part.output_type),
                    "effects": tuple(sorted(part.effects)),
                    "effects_contract": ("unknown" if "*" in part.effects else
                                         "structural" if operation in ("then", "branch", "identity") else "declared"),
                    "children": tuple(node(child) for child in part.children) if structural else (),
                    "parameters": MappingProxyType(dict(part.parameters))}
            if operation in ("leaf", "opaque"):
                data["function"] = part.function
            if not structural:
                data["source_operation"] = part.operation
            return MappingProxyType(data)
        return MappingProxyType({"version": 1, "root": node(self)})

    def replace_at(self, path: tuple[int, ...], replacement: Component) -> Component:
        """Rebuild one child of the supported immutable then/branch structure."""
        if not path:
            return replacement
        if self.operation not in ("then", "branch"):
            raise CompositionError("replace_at traverses then/branch only; opaque methods must be rebuilt by their factory")
        index, *tail = path
        if type(index) is not int or index < 0 or index >= len(self.children):
            raise CompositionError("replace_at child index is out of range")
        children = list(self.children)
        children[index] = children[index].replace_at(tuple(tail), replacement)
        if self.operation == "then":
            return children[0].then(children[1], name=self.name)
        return branch(*children, name=self.name)

    def program(self, *, name: str | None = None, budget=None):
        """Compile this component's entry to the existing public jv decorator."""
        calculation = self

        def entry(value):
            return calculation(value)

        entry.__name__ = name or self.name
        structure = calculation.structure()
        entry.__jv_structure__ = structure
        compiled = jv.program(budget=budget or jv.Budget())(entry)
        compiled.__jv_structure__ = structure
        return compiled


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

    return Component(name, yes.input_type, yes.output_type, None,
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
