# 2026-09-20 update: kernel v0.1 sync and research record / 内核 v0.1 同步与研究记录

Dated update following the practice in `progress.md`. Everything below was re-run in this repository on the date above; numbers not re-run here are marked as workspace measurements.

## What changed for a user / 用户可见的变化

- `foundation.jv` is now the v0.1 kernel: six effect forms (`state / judge / cut / gen / do / ask`) plus `fit` as a library bridge and `transform` for accounted host transformations; lazy effects with demand-driven flush, so ordinary Python control flow is used as-is and judgments in the same straight-line segment fuse into one call.
- Seven compiler passes each have a switch (`Runtime(passes=...)`); `python -m foundation.jv plan|check|stats|ablate <module:program>` estimate cost, run the static checker, print layer/question/call statistics, or run pass ablations.
- Checker rules J-01…J-18 with fix-it messages; failures are values (J-12): a client, generator, action or transform exception becomes `Unsure("fail")` / `Fail` instead of crashing the program.
- `src/foundation/jv/README.md` is written for a first-time user and ends with an API contract table (31 public names × 7 columns, each cell pinned by a test).
- Examples: the six specification programs, a `measure` example, three "strength" programs (uncertainty-driven review allocation, cost-ratio thresholds from a labelled set, the `fit` bridge), and the 21-program rewrite used to measure static interception.
- The composition library `src/jev_compose` is synced to the version that runs on this kernel.
- `research/` holds the design trail, ledger, red-team and reader rounds, and experiment records (see `research/README.md`).

内核 `foundation.jv` 更新为 v0.1：六种效应形式、`fit` 桥库与记账的 `transform`；惰性效应 + 需求驱动刷新，Python 原生控制流照常用，同一直线段内的判断融合为一次调用。七个编译 pass 各有开关；`plan / check / stats / ablate` 四个命令；检查器 J-01…J-18 带修法；失败一律是值。README 面向第一次用的人，末尾是 31 行 × 7 列的 API 契约表，每格有测试。`research/` 是研究工作区的整理副本。

## Demonstrate / 演示

```sh
python -m pytest -q tests/foundation_jv          # kernel tests, offline
python -m foundation.jv stats foundation.jv.examples.six:run_all
python -m foundation.jv ablate foundation.jv.examples.six:run_all
```

## Verification in this repository / 本仓库验证

| Check | Result |
|---|---|
| `pip install -e '.[dev]'`, `jpp demo` | Passed (offline, no API cost) |
| `python -m pytest -q` on committed files | 408 passed (383 kernel tests under `tests/foundation_jv`, 25 composition tests) |
| Kernel package fingerprint | `4f7a31bc48fa` (sha256 of concatenated `src/foundation/jv/*.py`, first 12 hex) |

Workspace measurements, not re-run here (see `research/地基/foundation/experiments/前提结论.md` and `research/地基/DECISIONS.md`):

- Pass ablation on the six specification programs: all passes on 25 calls / 21 layers; fusion off 36 calls; ledger off, second run 25 calls instead of 0.
- Static interception of the six bootstrapping error patterns over 21 programs × 6 injected variants: 126 / 126 after the fix for unpacked vectorized readings.
- Zero-context reader rounds on the builder: 24 → 24 → 21 guesses (round six: 7 of 21 were present in the docs and missed; all three programs ran in one layer with zero static errors). The pass line is still "zero guesses" and has not been met.
- **E9f, gain in the expensive-executor regime**: networkx full test suite (230 s) as the executor, 142 candidate changes, Jev asked which tests would catch each; at fixed recall ≥ 0.95, Jev + test-impact analysis skipped 35 % of full runs (recall 0.966), Jev alone 19 %; test-impact analysis alone missed recall (0.79), a haiku judge saved 16 % at 30× latency and 180× cost. Jev spend $0.029. The same model showed no gain in the earlier free-executor regime: the gain is a function of executor cost ratio, not a property of the model.

## Next design question / 下一个设计问题

Whether reader guesses converge under a per-name contract table, or whether the remaining guesses are undefined semantics that need decisions (fifteen candidates are listed in `research/地基/设计/G45-猜点分类.md`). Second: the "trusted" source of an action is currently self-declared with a reason string; the registry has no reviewer, which is a gap between invariant I6 and §2.11 of the IR spec.
