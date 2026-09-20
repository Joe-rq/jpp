"""Algorithm constructors assembled from the same Component/iterate mechanism."""
from __future__ import annotations
from .core import Component, CompositionError, iterate
from .observation import Observation, Request, observe


def inquire(prepare: Component, update: Component, done: Component, *, limit: int,
            observer: Component = observe, name="inquire") -> Component:
    """Construct an adaptive inquiry from S→Request, (S,Observation)→S, S→bool.

    Neither domain data nor a partition/selection policy is built into this loop.
    The resulting calculation remains a Component and can be passed or composed.
    """
    state_type = prepare.input_type
    if prepare.output_type is not Request or observer.input_type is not Request:
        raise CompositionError("inquire prepare and observer must share Request")
    if observer.output_type is not Observation or update.input_type is not tuple or update.output_type != state_type:
        raise CompositionError("inquire update must accept (state, observation) and return the state type")

    def advance(state):
        request = prepare(state)
        answer = observer(request)
        return update((state, answer))

    step = Component(f"{name}.step", state_type, state_type, advance,
                     prepare.effects | observer.effects | update.effects, "inquiry_step",
                     (prepare, observer, update))
    return iterate(step, done, limit=limit, name=name)


def feedback(propose: Component, inspect: Component, update: Component, done: Component,
             *, limit: int, name="feedback") -> Component:
    """Construct generation/checking feedback: S→list, (S,list)→list, (S,list)→S.

    Inspection may run exact checks, semantic judgment, another solver, or a
    composition of these. Its reports are ordinary values fed to the updater.
    """
    state_type = propose.input_type
    if (propose.output_type is not list or inspect.input_type is not tuple or inspect.output_type is not list
            or update.input_type is not tuple or update.output_type != state_type):
        raise CompositionError("feedback requires S→list, (S,list)→list and (S,list)→S")

    def advance(state):
        candidates = propose(state)
        reports = inspect((state, candidates))
        return update((state, reports))

    step = Component(f"{name}.step", state_type, state_type, advance,
                     propose.effects | inspect.effects | update.effects, "feedback_step",
                     (propose, inspect, update))
    return iterate(step, done, limit=limit, name=name)
