# Retained Python method guide / 保留的 Python 方法指南

For standalone J++ source, use the [Rust guide](../rust/README.md) and [implemented grammar](../rust/FRONTEND.md). This page documents the retained Python 3.12+ builder. Install it from this checkout
with `python -m pip install .`, or install a delivered wheel with
`python -m pip install /path/to/jpp_language-0.1.0a3-py3-none-any.whl`.
No research directory, PYTHONPATH setting, or kernel snapshot is needed.

本页保留 Python 构建器的使用方法；正式独立源码入口见上面的 Rust 指南。安装 Python 包后可以运行：

```sh
jpp demo
jpp methods --output method-report.json
jpp partial --output partial-report.json
```

The second command runs a complete feedback method, constructs nested checking
methods at runtime, replaces an internal checker, and saves the common plan,
actual dynamic structures and outputs. `[0]` produces `x`; `[-4,...,4]` produces
`x if x >= 0 else -x`. These are finite-domain checks with synthetic observations,
not measured JEV accuracy. Output is written to your chosen file, never into the
installed package. Full source: `jev_compose.method_example` (included in wheel).

`jpp partial` uses an already sufficient candidate combination while other
questions remain unresolved, then changes the strategy and improves it without
repeating prior checks. See [partial results and continuations](partial-results.md)
for the protocol, complete example and direct-control comparison.
`jpp partial` 演示先使用足够的部分结果，再换策略补问；旧检查保留。

## Define, compose, execute

```python
from foundation import jv
from jev_compose import component, execute, identity, product, iterate, Iteration
from jev_compose.fixtures import runtime

@component("increment", int, int)
def increment(n):
    return n + 1

@component("enough", int, bool)
def enough(n):
    return n >= 3

@component("answer", Iteration, int)
def answer(result):
    return result.state

method = iterate(increment, enough, limit=5).then(answer)
larger_method = product(method, identity(int))
result = execute(larger_method, 0, runtime())
assert result.value == (3, 0)
print(larger_method.describe())
print(jv.plan(larger_method.program()))
print(result.trace)
```

Put definitions in a `.py` file and run it with Python. `component` checks visible
source with the existing kernel checker. Signatures check nominal types and
container shapes; use `typing.Any` for an intentional open boundary. Ordinary
Python/dataclasses implement data and exact algorithms.

| Operation | Contract |
|---|---|
| `a.then(b)` | Pass a's output to b; types must connect. |
| `product(a,b,...)` | Same input for all; run in declaration order; return tuple. |
| `branch(predicate,yes,no)` | Predicate returns bool; arms have identical signatures. |
| `iterate(step,done,limit=N)` | step S→S, done S→bool; check before stepping; at most N steps, N+1 checks. Returns Iteration with state/steps/reason (`done` or `limit`). |
| `a.bind(factory,T,effects=...,factory_effects=...)` | Run a, pass result to factory, then run returned Component on that same result. T is continuation output type. |
| `replace_at((child_index,...),replacement)` | Rebuild supported structure, preserving original and checking connections. Children: then=(a,b), branch=(predicate,yes,no), product=parts, iterate=(step,done), bind=(a,factory). |

A method is a normal Component value: pass it to a constructor, return it from a
function, or compose it again. Returning it alone does not run it. For bind,
`effects` bounds the returned method's capabilities. `factory_effects` describes
the factory's own calls; passing a Component factory derives its declaration.
An explicit superset is allowed. Plain unclassified factories default to `*`.
Declarations are not proofs of arbitrary host-code purity.

```python
from jev_compose import Component

@component("choose_method", int, Component)
def choose(n):
    return increment if n < 3 else identity(int)

dynamic = identity(int).bind(choose, int, effects=frozenset())
assert execute(dynamic, 1, runtime()).value == 2
```

`inquire(prepare,update,done,limit=N,observer=observe)` constructs an iterative
inquiry: prepare S→Request, update (S,Observation)→S, done S→bool.
`feedback(propose,inspect,update,done,limit=N)` constructs candidate/check/update:
propose S→list, inspect (S,list)→list, update (S,list)→S. Both use the same public
combinators; their internal steps can be replaced by child path.

## Observe and retain uncertainty / 判断与未决

```python
from jev_compose import Request, observe

@component("read_flag", Request, object, effects=("judge",))
def read_flag(request):
    answer = observe(request)
    return answer.value if answer.resolved else None

request = Request(jv.state(on=jv.mat({"flag": True})),
                  jv.test("Does the flag hold?", calib=jv.calib("demo.flag")))
assert execute(read_flag, request, runtime()).value is True
```

The provided `fixtures.runtime()` is explicitly synthetic and free of model API
calls. For custom deterministic observations, pass `runtime(rule=rule)` where
`rule(state_json, question_id, question_dict)` returns e.g.
`{"type":"noul","noul":0.99}` (true), 0.01 (false), or 0.5 (unresolved).
The fixture registers `demo.flag`, `demo.membership`, `demo.pick`, `demo.complexity`.
It does not establish real calibration. Production uses the ordinary `jv.Runtime`
with its real client/profile/calibration, not a different composition runtime.

`Observation` keeps value/resolved/cause/request/model_id; `to_dict()` exports
provenance. `batch_observe(requests)` registers independent questions before
consuming results. `at_least(observations,k)` derives count bounds; unresolved
returns None when the bound is insufficient. `refine` updates only selected
unresolved observations, preserving known observations even with matching IDs.

## Plan and inspect a run / 调试

`method.structure()` returns the versioned read-only structure used by common
`jv.plan(method.program())`. Product costs sum; loop bounds include N+1 checks;
dynamic continuations stay symbolic. Planning never invokes factories.

`execute(method,input,runtime(),budget=jv.Budget(calls=...))` returns
`Execution(value,stats,structure,trace)`. Each trace event identifies its component.
Dynamic bind events additionally contain `call_id`, `generated_structure`,
`generated_description`, `continuation_trace_start/end`, and `result`. Nested
event ranges identify which method actually ran. These records belong to this
execution and do not mutate the reusable method. The raw structure retains
callables; export descriptions instead of trying to serialize closures.

Budgets and action replay remain owned by the shared kernel, including nested
programs. `done` means the author's condition was met, not universal correctness.
No new syntax or automatic natural-language compiler is included in this release.

## Export a report / 导出报告

Do not call `dataclasses.asdict()` on the entire execution: read-only structures
retain functions. Keep the portable generated description and stringify the plan.
For ordinary values and dataclass results, this is a complete starting point:

```python
import json
from dataclasses import fields, is_dataclass
from collections.abc import Mapping

def encode(value):
    if is_dataclass(value):
        return {f.name: getattr(value, f.name) for f in fields(value)}
    if isinstance(value, Mapping):
        return dict(value)
    if isinstance(value, (set, frozenset)):
        return sorted(value)
    if hasattr(value, "to_dict"):
        return value.to_dict()
    raise TypeError(f"Add a serializer for {type(value).__name__}")

report = {
    "value": result.value,
    "plan": str(jv.plan(larger_method.program())),
    "trace": [{k: v for k, v in event.items() if k != "generated_structure"}
              for event in result.trace],
}
with open("my-report.json", "w") as output:
    json.dump(report, output, default=encode, indent=2)
```

An independent author used only this guide and the installed wheel to write
[a new flaky-test method](../examples/flaky_method.py). It identifies inconsistent
test outcomes, passes the entire detector into a report constructor, and runs the
returned method through bind. Run `python examples/flaky_method.py` from a checkout
after installing the package. It needs no model calls or source-path setup.
