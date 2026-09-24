# Current scope / 当前实现范围

Updated 2026-09-24, after PRs [#27](https://github.com/Towow-ai/jpp/pull/27), [#28](https://github.com/Towow-ai/jpp/pull/28), [#29](https://github.com/Towow-ai/jpp/pull/29) and [#30](https://github.com/Towow-ai/jpp/pull/30) merged. Standalone `.jpp` source runs through the native Rust implementation in [`rust/`](../rust/README.md), which is now the primary implementation; the Python package remains available as a behavioral reference and for the discovery demonstrations. `cargo test --workspace --offline`: **394 passed, 0 failed, 3 ignored**.

## What's in this repository

- **Rust kernel** (crates `jpp-cli`, `jpp-core`, `jpp-frontend`): source parser, shared AST, a defined static-check subset, interpreter, native CLI, budgets, fixed observations and ledger replay.
- **A real JEV backend.** `jpp run --backend live` sends actual judgment calls; `jpp calib-import` builds a calibration record from labeled evidence (human, constructed, or model-labeled with a spot-check gate) and certifies a threshold from it using a two-sided, split-sample statistical bound; `jpp calib-confirm` lets a human confirm a drift-flagged record. Live runs with no matching calibration record come back explicitly undecided (`Unsure(cold)`) rather than guessing a threshold.
- **Questions as first-class values** (`form`/`fill`), a **three-way sieve** construct that takes questions directly and can also ingest review-opinion material, **pairing** (`pair`), **set aggregation** (`tally`/`first_k`), **bounded iteration** with a required shrink condition (`iterate`), and a **composition-closure contract**: every set-level construct returns the same kind of value, so its output can be fed into another one of these constructs.
- **Value-level taint tracking.** Every scalar value carries a trust bit, propagated through operators and built-in dispatch, and checked before content derived from untrusted input can license an irreversible action (a file write, an external call). This replaced an earlier substring-matching approach that was demonstrably wrong in both directions (it both let untrusted content through and blocked trusted program literals that happened to share text with untrusted input).
- A batch of five smaller rules landed together: fixture-only calibration records can route a program but cannot license an irreversible action; suspected-drift calibration records are auto-flagged and require human confirmation; merged readings use mean or median only (never majority vote) and get their own calibration key; judgment-absence handling (retry, backoff, escalate, circuit-break) plus a static latency budget; and `unsure` outcomes carry a named cause (`rejected_all`, `no_candidate`) with library-level routing for both.

## What's in the same-day sync branch, pending review, not yet in `main`

- **An architecture refactor.** The research workspace split the Rust kernel from 3 crates into 10 -- `jpp-ir`, `jpp-value`, `jpp-effects`, `jpp-ledger`, `jpp-calib`, `jpp-check`, `jpp-plan`, `jpp-core`, `jpp-syntax` (renamed from `jpp-frontend`), and `jpp-cli`, with a ruled cap of 11 -- rebuilding the intermediate representation and the ledger format, and adding structural provenance tracking (which judgment produced which downstream value, and how many chained layers deep a result sits). It also added the trial-grade calibration tier, per-exit reporting of which grade of line backed each outcome, a rule requiring every live run to carry a capability profile, and the verification-dashboard scripts. This is tested work -- `cargo test --locked --workspace` on the branch: 579 passed, 0 failed, 3 ignored -- on the same-day branch `sync/2026-09-24-architecture` (synced through research-tree commit `9716e61b`), pending review before it is pushed and merged into this repository. Anything mentioning these crate names, this test count, or a live trial-grade calibration outcome describes that branch, not `rust/` here. See [dashboard and expressiveness reference](updates/2026-09-24-dashboard-and-expressiveness-reference.md) and [live trial-grade calibration lines](updates/2026-09-24-live-trial-lines.md).

## What's ruled but not yet built anywhere

- **A cheaper formal-certification method.** Getting a question from "no calibration record" to a formally certified live outcome still requires labeled evidence at the thresholds documented in `jpp calib-import --help`; a fixed-sequence certification method, a sequential variant, and a fix to a separate specification gap in how a certified line's scope may be extended are written decisions, validated offline against existing data, but not yet implemented in any branch's calibration code. See [cutting the labeling threshold](updates/2026-09-24-labeling-threshold.md).
- A complete static type/effect system, arbitrary closure serialization, machine-code compilation.

## Honest evidence on the project's three acceptance criteria

Expressiveness (lines/effort saved versus hand-written code) has partial evidence at 2x-5x on isolated comparisons, short of the 9x-20x literature reference band. A corrected two-tier measurement method exists and a first multi-implementation measurement has been run under it in the research workspace, but the resulting ratio is not published in this update; public readings include the 2x-5x probe comparisons above and a 1.2x-1.5x reading on short trial programs (see the dashboard update linked above). Depth (how many chained judgment layers a program can sustain) now has a curve of 22/12/4 decided outcomes across one/two/three chained layers. Backend interchangeability ("swap the judge, program doesn't change") has been exercised for 1 of 8 tracked capability assumptions. The live backend's share of decided outcomes on a set of new-question test runs is 0.255 (14 of 55).

## Backends / 后端

The native `jpp run` examples use fixed JSON observations by default, for zero-cost mechanism testing; `--backend live` connects to the real JEV service and requires a capability profile and, for any question without one, a certified calibration record (see above). The retained Python `jpp demo` uses a `FixtureClient` with synthetic calibration, also for zero-cost testing. The two executables share the name `jpp`; use an explicit native path if both are installed.

Do not reuse fixture calibration records for real decisions -- they exist only to exercise the mechanism. Model service credentials are never committed to this repository; `PyYAML` and `pytest` are separate, separately-licensed dependencies of the Python side. Model services themselves are not distributed or licensed by this repository.

---

独立语法、原生执行、真实 JEV 后端、题成为一等值、三路过滤、配对、聚合、带终止线的迭代、校准进料（真值通道 + 拆分样本认证）、组合封闭性契约、值级 taint 与一批规则修复，已经全部合入本仓库 `main`。`cargo test --workspace --offline`：394 通过、0 失败、3 忽略。

同一天研究工作区还做了一次架构重构：内核从 3 个 crate 拆成 10 个（`jpp-ir`、`jpp-value`、`jpp-effects`、`jpp-ledger`、`jpp-calib`、`jpp-check`、`jpp-plan`、`jpp-core`、由 `jpp-frontend` 改名的 `jpp-syntax`、`jpp-cli`，已裁定的上限是 11 个），随附试用档校准等级、逐出口记线等级、真机必须带能力画像的规则、验收仪表脚本。这些都造出并测试过——该分支 `cargo test --locked --workspace`：579 通过、0 失败、3 忽略——代码在同日分支 `sync/2026-09-24-architecture` 上（同步到研究树提交 `9716e61b`），待审核后推送并合入本仓库。

另有一批工作只写进了设计裁定，任何分支都还没有造：一种能把正式认证门槛压到约 60 条标注（不放松安全边界）的固定序检验方法、一个序贯变体，以及一处「已认证线扩展适用范围」规格缺口的修法，都已离线用现有数据验证过，但还没有实现进校准代码。

项目三条验收标准的如实数字：表达量在孤立对照上是 2–5 倍，还没到 9–20 倍的文献参考带；按新方法做的第一次多实现测量已经在研究工作区跑过，读数本篇不公布，可以公开的是上面这 2–5 倍的探针读数与一组短试写程序 1.2–1.5 倍的读数。深度证据现在是一条一/二/三层各 22/12/4 个已决出口的曲线。换后端可用性目前只验证了追踪的八类能力假设中的一类。真机后端上，一批新题测试运行的已决出口占比是 0.255（14/55）。

公开仓库不包含凭据、私人对话或模型权重。`jpp run` 默认用固定 JSON 观察做零成本机制测试；`--backend live` 接真实 JEV 服务，需要能力画像，新题没有校准记录时会明确返回未决而不是瞎猜阈值。夹具校准记录只用于机制测试，不得用于真实决策。
