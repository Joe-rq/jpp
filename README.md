# J++

![J++ — compose questions and methods](assets/social-card.svg)

**Compose questions. Compose methods. Compose the compositions.**

[简体中文](README.zh-CN.md) · [Design](docs/design.md) · [Roadmap](ROADMAP.md) · [Contributing](CONTRIBUTING.md)

J++ is an experimental programming-language project exploring semantic judgment as a programmable operation. Questions are values. Methods are values. A composed method can become a building block in another method.

The current implementation is a **Python 3.12 embedded language**, with a runtime and a composition library. It combines JEV-style judgments with ordinary computation, candidate generation, exact checks, and feedback. Independent syntax and a standalone compiler are future work.

## Try it

```sh
git clone https://github.com/towow-ai/jpp.git
cd jpp
python3.12 -m venv .venv
source .venv/bin/activate
python -m pip install -e '.[dev]'
jpp demo
python -m pytest -q
```

Windows: activate with `.venv\Scripts\activate` instead.

The demo runs offline with no API key or API charges. It identifies a target among 1,000 candidates using at most 10 adaptive binary questions, then constructs an expression using counterexamples and checks all nine declared inputs. The answers are synthetic and the generator is finite enumeration: this demonstrates the execution and composition mechanisms, not real-model accuracy or a new search algorithm.

## A method stays a method

```python
from jev_compose import component, execute
from jev_compose.fixtures import runtime

@component("length", str, int)
def length(text):
    return len(text)

@component("double", int, int)
def double(n):
    return n * 2

method = length.then(double)
assert execute(method, "hello", runtime()).value == 10
```

The same interface supports methods that ask questions, choose subsequent methods, or iterate over feedback. `inquire(...)` and `feedback(...)` are themselves composition constructors; their strategies can be replaced without changing the runtime.

## What is here

| Layer | Current implementation |
|---|---|
| Questions | Test, selection, and measurement; save and restore question values |
| Composition | Sequential composition, branches, products, dynamic method selection, bounded iteration |
| Algorithms | Adaptive inquiry and candidate/check/counterexample feedback |
| Uncertainty | Explicit unresolved observations and conditional count bounds |
| Runtime | Materials, judgment effects, generation/action hooks, budgets, records and replay |
| Distribution | Installable Python package, offline CLI demonstration and tests |

`src/foundation` contains the runtime; `src/jev_compose` contains the composition layer; `src/jpp` provides the distribution entry point. Existing import names remain available while the language design develops.

This is an early alpha. APIs may change. Real-model quality requires separate evaluation; the default demo never contacts JEV. See [current scope and backend notes](docs/status.md).

## Help shape the language

The most useful contribution is a new method built from existing components, together with an example that runs. Tell us where composition becomes awkward, what you had to duplicate, and which primitive would eliminate that duplication. [Start here](CONTRIBUTING.md).

MIT licensed. J++ is an independent project, not an official JEV/TypeSafe product, and is unrelated to Microsoft's historical Visual J++.
