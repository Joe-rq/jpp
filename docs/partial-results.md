# Use a result now, continue later / 先使用结果，再继续求解

A method can now return a usable value, unresolved questions, evidence and a
normal Component that continues the work. A caller decides whether the value is
already sufficient. The same packet can flow through another exact algorithm;
when it resumes, that algorithm is reapplied to the enlarged result.

一个方法现在可以同时交出“已有结果、未决问题、依据、继续方法”。调用者能先用
已经够用的结果，随后换一种策略处理剩余问题。部分结果接入下一种算法后，继续
求解仍会回到这条组合链。内核、算法主体都不需要为每个调用者重写。

## Run the complete example / 运行完整例子

Install Python 3.12+ and `python -m pip install .` from the repository, or install
the `jpp_language-0.1.0a3-py3-none-any.whl` artifact. In any ordinary directory:

```sh
jpp partial --output partial-report.json
```

The task is to cover both web and database skills at cost at most 10. A costs 4
and provides web; B costs 5 and provides database; C costs 2 and offers both;
D costs 20 and offers web. Initial fixed judgments accept A and B, leaving C and
D unresolved. A real local action checks each accepted candidate's cost and skill
fields. An exact enumeration algorithm then constructs feasible combinations.

要找一组同时具备网页和数据库能力、总成本不超过 10 的候选。第一轮 A、B 已获
接受且完成本地检查，C、D 尚未确定。候选检验交出的部分结果直接进入精确组合算法。

| Stage / 阶段 | Best usable combination / 当前可用组合 | Pending / 未决 | Checks executed so far / 已执行检查 |
|---|---|---|---|
| Initial / 首轮 | A + B, cost 9 | C, D | A, B |
| Only promising / 只补有希望的 | C, cost 2 | D | A, B, C |
| All remaining / 再补剩余的 | C, cost 2 | none / 无 | A, B, C |

The caller actually consumes A+B before all questions are resolved. It then asks
for cost at most 2 and supplies another strategy. Six observations and three
local checks occur in total. Prior A/B observations and checked materials are
retained. Revised C/D material creates new observation identities; the old unknown
observations remain in evidence. The JSON contains these identities, source hashes,
both dynamic continuation descriptions and an equivalent direct-control run.

上层实际取用了第一轮方案，之后提高要求并补问 C，A、B 没有重复检查。最后处理 D
也没有重新检查 C。报告保留新旧观察及材料哈希，并与直接手写控制程序对照。
固定判断用于验证组合行为，不代表真实模型正确率。This is a deterministic fixture
with actual local checks, not a live-model evaluation. No model-speed or accuracy
gain is claimed.

More precisely: resuming all four questions at each of two stages would use
4+4+4=12 observations; this example uses 4+1+1=6. A carefully handwritten program
also uses 6. Resuming both pending questions immediately would use 4+2=6, but would
resolve D before the caller needs it. The library's gain is reuse of the control
protocol; the strategy chooses which work to defer.

## Public protocol / 公开协议

```python
from jev_compose import Partial, checkpoint, map_partial, continue_with
```

| API | Meaning / 契约 |
|---|---|
| `Partial(value, pending, evidence, continuation)` | Algorithm-specific value; tuples of pending identities/items and evidence; Component or None. |
| `packet.satisfies(requirement)` | Run a Component from the value's type to bool; caller owns the condition. Enough does not mean everything is resolved. |
| `checkpoint(state, view=..., pending=..., evidence=..., advance=...)` | Project algorithm state into a packet and capture its continuation. Projections declare no effects. |
| `advance` | Component accepting `(state, strategy_component)` as a tuple and returning the next state. It owns observation/action effects and decides how a strategy is used. |
| `map_partial(project)` | Effect-free Component `V→W` becomes `Partial→Partial`. Map the current value and all future values, retaining pending/evidence. |
| `continue_with(strategy, effects=...)` | Component `Partial→Partial`. Select and execute the packet's continuation through existing bind, supplying the strategy. Terminal packets pass through unchanged. |

`checkpoint` only advances when the continuation is called. It attaches no
continuation when pending is empty or advance is None. A packet can still contain
unresolved items with no available continuation; callers inspect both fields.
The supplied strategy's declared effects must fit advance.effects, and
continue_with's effects must cover the selected continuation. These declarations
are contracts, not proofs of arbitrary Python purity. Planning keeps dynamic bind
costs symbolic; actual generated structures appear in the execution trace.

构造结果包不执行后续工作。未决项如何产生、怎样识别、用什么新证据解决，由算法
负责。`map_partial` 可以重新计算精确投影，但不会自行重复观察或动作。它不承诺
任意算法的增量优化；这个例子的组合枚举会重算，已发生的候选检查则保留。

Continuation closures are callable within the current process and use the same
`foundation.jv` runtime. Store/export observation evidence normally; serializing
a closure for another process is not provided. Keep captured algorithm state
immutable between continuations. Two calls from the same old packet are separate
branches, not an automatic merge. Native budgets and uncertainty exits still apply.

## Compose and change the caller / 组合并更换需求

Save this as a Python file and run it after installation. All imports below are
included in the wheel. `partial_example` provides the documented candidate fixture;
the four protocol functions above are general-purpose library APIs.

```python
from jev_compose import Partial, component, execute, map_partial, continue_with
from jev_compose.fixtures import runtime
from jev_compose.partial_example import (
    Plans, validate, combine, requests, promising, advance,
)

@component("cost_at_most_three", Plans, bool)
def need(plans):
    return plans.cheapest is not None and plans.cheapest["cost"] <= 3

@component("my_caller", list, Partial, effects=("judge", "do", "transform"))
def my_caller(items):
    packet = validate.then(map_partial(combine))(items)
    if not packet.satisfies(need):
        packet = continue_with(promising, effects=advance.effects)(packet)
    return packet

packet = execute(my_caller, requests(), runtime()).value
assert packet.value.cheapest["cost"] == 2
assert [o.request.tag for o in packet.pending] == ["D"]
```

To write a **new strategy** for this fixture, define a Component `tuple→dict`.
It receives only unresolved Observation objects. Return `{old_observation.id:
replacement_Request}` for the ones to reconsider. Observation.request contains
state, question, label and tag. `state.resolved().all_mats[0]` is the Mat whose
`.content` has name/cost/skills/flag/unknown/revision. Use `jv.transform(fn, old_mat)`
to supply a new evidence revision; fn receives a **Mat**, not its content dict.
Construct `Request(jv.state(on=new_mat), old.request.question, old.request.label,
old.request.tag)`. The installed fixture helper `supplement(mat)` sets revision=1,
unknown=False, and accepts C while rejecting D. A strategy using transform declares
`effects=("transform",)`. Select the items yourself before calling this helper.
`advance` uses refine and verifies newly accepted candidates; do not call it from
the strategy. Empty updates mean no progress, not automatic success.

新作者可以只改需求判定，或新写一次只处理一个问题、按价格排序等续接策略，再用
`then`、`product`、`iterate`、`bind` 组成更大的方法。候选检验与组合算法保持原样。

## Direct control versus reusable control / 直接手写与可复用写法

The installed `partial_example.direct_workflow` uses the same observation/checking
and combination bodies, but explicitly carries CandidateState, calls advance,
reapplies combine and constructs each report. `workflow` instead composes:

```python
solver = validate.then(map_partial(combine))
packet = solver(items)
# use packet.value when packet.satisfies(your_requirement)
packet = continue_with(your_strategy, effects=advance.effects)(packet)
```

The CLI executes both and checks equality of results, evidence records,
observation counts and action counts (`direct_equivalent: true`). The reusable
part is checkpoint capture, value mapping through future continuations, strategy
dispatch and composable return type. Domain algorithms and stopping conditions
are still explicit code. 没有声称新语言能做到 Python 无法计算的事情；这里把反复手写
的续接规矩做成共同接口，使另一种算法和另一个调用者可以直接接上。

## Inspect work and an independent strategy / 工作量与独立策略

`execute` returns an Execution; use `result.value` for the packet and
`result.stats` for the shared runtime report. Useful fields are:

| Field | Meaning |
|---|---|
| `questions`, `questions_per_layer` | Evaluated judgment questions in total and by layer. |
| `calls`, `calls_per_layer` | Backend calls; one call can contain multiple questions. This fixture has one question per call. |
| `effect_requests["do"]` | Action requests, including any governed by native replay. The example's check log independently confirms three actual local checks. |
| `effect_requests["transform"]` | Requested material transformations; two revisions in the full example. |
| `ledger_hits`, `cache_hits` | Shared kernel reuse counters; not additional independent evidence. |
| `cost` | Runtime-reported cost; zero for the synthetic fixture, not an estimate for live use. |

An author who read only the public guide wrote
[an expensive-first strategy](../examples/priority_resume.py): resolve one pending
candidate per step, then require both cost <= 2 and no pending questions. It uses
then/product/iterate/bind with the same protocol and installed algorithms. Run:

```sh
python examples/priority_resume.py > priority-report.json
```

The sequence is A+B (C/D pending), then A+B (only C pending), then C (none pending).
It uses six fixed-observation calls, three local checks and two transformations.
[Recorded output](demos/partial/independent-output.json). 新作者未阅读实现，成功改变
策略和完成要求；第一次组合从同一旧结果分出两个续接分支，重复请求了 D，随后改为
顺序续接。库会保留分支语义，并不会自动合并不同调用者的工作。统计字段说明根据
这次独立使用反馈补齐。
