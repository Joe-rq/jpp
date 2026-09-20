# 2026-09-21 update: fusion by mechanism, exits across frames, ten runnable probes / 融合按机制成立、出口跨帧、十条可运行探针

Dated update following the practice in `progress.md`. Every number in the "Verification in this repository" section was re-run in this repository on the date above with the committed files; workspace measurements are marked as such.

## What changed for a user / 用户可见的变化

- **Fusion no longer depends on how you write the loop.** Two new compiler passes, `speculate` and `vectorize` (`src/foundation/jv/spec.py`), let the runtime ask, at the first flush point, every question that is statically reachable on the same material (judgments only, never `gen` / `do` / `ask`, and never inside a branch that may not run), and turn a data-parallel `for` over materials into one layer. The specification's example programs were not edited; their layer counts dropped to the theoretical values (`取物` 7 → 4, `写docstring` 4 → 2). Both passes have switches and ablation rows.
- **An `Unsure` can now leave a function.** A `@jv.program` whose return annotation includes `jv.Unsure` (for example `-> jv.Exit | jv.Unsure` or `-> list[jv.Exit]`) may return unresolved exits; they are recorded as consumed-by-return-type and re-registered in the caller's frame, so the caller must handle them (J-05). Without such an annotation, returning an `Unsure` is still an error with a fix-it message. This closes the gap that made every probe pass ground truth into the program instead of returning exits.
- **Failure shapes are consistent.** A list-typed `transform` that fails returns an empty `FailList` rather than a single fail material; a `transform` whose output equals one of its inputs keeps that input's `Mat` and provenance.
- **Silent mistakes now warn.** `W-wildcard-unsure` (a `case _:` that swallows an `Unsure`), `W-drop-vs-escalate` (dropping unsure exits in a frame that also escalates), `W-literal-from-host` (`jv.mat` on a host-computed value), `W-self-trusted` (an action declaring its own output trusted).
- **Bug fix (found by this repository's Towow tests):** the J-09 evidence check applied Python truthiness to a `Mat`; since `Mat` forbids implicit bool/len, any question with `evidence=("on", …)` crashed. Fixed in the kernel; regression tests added.
- **Ten runnable probes** (`src/foundation/jv/probes/`, `python -m foundation.jv.probes`): compile-error attribution, commit-message/diff consistency, flaky tests, log-line classification, performance-regression bisection, column-type inference, docstring/signature mismatch, dependency-upgrade breakage, configuration drift, command generation to successful execution. Each has a builder program, a bare-client control arm, a no-model baseline, and injected ground truth; all run offline by default, and `--real` requires a pre-registered budget.
- The README now has a contract table entry for `program` return annotations, sections on ordering multiple questions, guards (affirmative propositions; the two routes for untrusted material), and `transform` failure shapes.

**中文摘要。** 融合不再取决于写法：新的 `speculate` 与 `vectorize` 两个 pass 让运行时在第一个刷新点就把同一材料上静态可达的题一起发出（只推测判断，分支内不推测），并把数据并行的 `for` 合成一层；规范示例一字未改，层数降到理论值（取物 7 → 4，写docstring 4 → 2）。「拿不准」可以带出函数：返回注解含 `jv.Unsure` 的程序可以返回未决出口，调用者必须处理。列表型 `transform` 失败返回空 `FailList`；四条新告警堵住此前静默的误写；修了一个由本仓库通爻测试踩出的内核 bug（证据槽检查对材料取真值）。新增十条可运行探针，默认离线，`--real` 需先预注册预算。

## Demonstrate / 演示

```sh
python -m pytest -q                                       # whole repository, offline
python -m foundation.jv stats                             # layers / questions / calls for the example programs
python -m foundation.jv ablate                            # switch each pass off, including speculate / vectorize
python -m foundation.jv.probes                            # ten probes with the offline FakeClient (no API cost)
python -m foundation.jv.probes --only p22 --real --tag t  # one probe against the live model (pre-register first)
```

## Verification in this repository / 本仓库验证

| Check | Result |
|---|---|
| `python -m pytest -q`, Python 3.12 | 494 passed (kernel tests under `tests/foundation_jv` incl. 25 probe tests, composition tests, Towow tests) |
| `python -m pytest -q`, Python 3.13 | 494 passed |
| Kernel package fingerprint | `82b6d9448e45` (sha256 of concatenated `src/foundation/jv/*.py`, first 12 hex) |
| `python -m foundation.jv stats` | ten example programs: 20 layers, 84 questions, 60 calls; `取物` 4 layers, `写docstring` 2 layers |
| `python -m foundation.jv ablate` (six specification programs) | all passes on: 16 layers / 42 questions / 27 calls; `speculate` off: 19 layers (`取物` back to 7); `vectorize` off: 18 layers, one layer stopped by budget (`写docstring` back to 4); `lift` off: 21 layers; `fuse` off: 40 calls = questions; `ledger` on, second run: 0 calls, 42 ledger hits |
| 21-program rewrite (`tests/foundation_jv/test_twentyone.py`) | 169 passed; 32 layers / 102 questions / 73 calls in total (was 38 / 100 / 72 before the two passes) |
| `python -m foundation.jv.probes` (offline) | all ten probes run; estimated cost $0.0128 at live prices |

Workspace measurements, not re-run here (see `research/地基/foundation/experiments/前提结论.md` §E-PROBE-10 and `research/地基/DECISIONS.md`):

- Ten probes against the live model: total spend $0.0137 (budget $0.30); 9 / 10 pre-registered accuracy bets met or exceeded; builder call counts equal the bare-client arm 10 / 10; replay 0 calls 12 / 12; all programs one layer, zero static errors. Gain is at the zero point of the gain formula: the three probes that measure saved expensive steps use second-scale executors, so cost ratio ≈ 1; the five template probes have heuristic baselines at 1.0 and only test that the programs can be written and run.
- Zero-context reader rounds on the builder: 24 → 24 → 21 guesses (round six: 7 of 21 present in the docs and missed; net 14). The pass line remains open; the ledger proposes replacing "zero guesses" with "zero silent wrong results" as the hard line.
- E9f (expensive-executor regime, networkx full suite, 230 s): at recall ≥ 0.95, Jev + test-impact analysis skipped 35 % of full runs; details in the 2026-09-20 update.

## Next design question / 下一个设计问题

Speculative lifting evaluates the program's own state expressions on a snapshot of the frame; it is bounded by a whitelist of pure calls, and silently falls back to the old behaviour when a user-defined helper appears in a site expression. The next question is whether the reachable-question analysis should live in the IR (so the planner can price mis-speculation and the checker can name what blocked lifting) rather than in an evaluator over Python AST.

推测提升是在帧快照上求值程序自己的状态表达式，安全边界是一张纯调用白名单；站点表达式里出现用户自定义函数时会静默退回旧行为。下一个问题是：可达题分析应该进 IR（让计划器给推错定价、让检查器指出是什么挡住了提升），还是继续停留在 Python AST 上的求值器。
