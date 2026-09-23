# Current scope / 当前实现范围

Updated 2026-09-23 after the constructs, live-backend and template-level calibration sync in [PR #28](https://github.com/Towow-ai/jpp/pull/28) (stacked on the earlier kernel synchronization in [PR #25](https://github.com/Towow-ai/jpp/pull/25)). Standalone `.jpp` source runs through the native Rust implementation in [`rust/`](../rust/README.md). The Python package and published discovery demonstrations remain available as references and experiments. [Delivered source milestone](https://github.com/Towow-ai/jpp/pull/12).

- Rust: source parser, shared AST, a defined static-check subset, interpreter, native CLI, budgets, fixed observations and ledger replay. Source programs implement nested method composition, adaptive questions and partial-result continuation.
- Python: retained runtime/composition library, installable package and discovery experiments, including published real JEV recordings. These capabilities are not automatically all present in Rust.
- Not delivered: machine-code compilation, a complete static type/effect system, arbitrary closure serialization, or a truth channel that covers every question template (only the "topic-relevance" template is certified so far; others remain `pending`/`cold`). Exact remaining rule differences are listed in the [core interface](../rust/crates/jpp-core/INTERFACE.md).
- Fixed in the public Rust kernel: captured method identity, preservation of completed over-budget calls, and checked integer arithmetic (rules 4–6 of the [design revision](../research/地基/13-Rust实践反馈设计修订-v0.2.md)). `v13_rules.rs` exercises their behavior; `known_defects.rs` now adds an enabled regression for the exact overflow source span instead of three obsolete ignored reproducers.
- Also present: unresolved exits carried as values, rank-1 effect polymorphism, source-library methods and fixture-backed lifecycle examples. These were once ahead of the public snapshot; their code is now included. Remaining restrictions are documented in `INTERFACE.md`.
- Next: certify more question templates through the truth channel, connect certified lines to the default run path, and continue reusable semantic algorithms toward a bounded application. Two fail-open defects found by the design ledger, and several unimplemented rulings (B25, B28-B32), are fixed/specified in the research tree and pending sync into this repository. The host calibration APIs are experimental; selected-threshold risk guarantees are not established. See [Rust status](../rust/README-status.md) and the [roadmap](../ROADMAP.md).

独立语法、原生执行和后续 Rust 修复已经进入公开仓库。此前三条缺陷已有正常运行的回归测试，源码方法库、未决责任和效应行实现也已同步。41 项是[首包验证记录](verification.md)的历史数字；当前核查入口见 [Rust 状态](../rust/README-status.md)。原生真机入口（`--backend live`）与标注校准的真值通道（`calib-import`）已接通；下一步是把认证扩到更多题式、把已认证线接入默认运行路径，并继续构建可复用算法；统计选线实验不能视为一般风险保证。

公开仓库不包含凭据、私人对话或模型权重。经整理的公开模型录制与实验输出已随发现案例提供；不能再笼统地称仓库没有模型录制。

## Backends

The native `jpp run` examples use fixed JSON observations by default; the retained Python `jpp demo` uses `FixtureClient`. Both use synthetic calibration for mechanism tests at zero API cost. The two executables share the name `jpp`; use an explicit native path if both are installed.

`jpp run --backend live` (Rust, built with `cargo build -p jpp-cli --features live`) reaches the real JEV backend through `JevClient`. The credential is read only from `~/.typesafe-key`, never logged or written to a report, and `--backend live` is mutually exclusive with `--fixtures`. Replay restores the client-independent `model_id` from the ledger header, so a live-backed run replays with zero new calls. Exits without a certified calibration line come back `unsure(cold)` — a live reading alone does not produce a usable answer.

`jpp calib-import` is the truth channel: it folds labelled readings (source `human`, `computed`, or `model:<name>`) into per-question-template calibration records and certifies them by split-sample two-sided commission; model-only labels need a same-key human spot check above a configurable threshold before they certify (provisionally, then fully). So far only the "topic-relevance" template has cleared that bar; other templates remain pending. See the "Getting usable exits on the live backend" section of the [Rust README](../rust/README.md) for the exact commands.

The bundled historical Python JEV adapter targets a specific API/model version and reads a credential from `~/.typesafe-key`. It is not used by the demo. Its current service compatibility and model accuracy have not been validated as part of this release. Do not reuse fixture calibration records for real decisions.

PyYAML is a dependency of the bundled foundation utilities; pytest is a development dependency. They are installed separately and retain their own licenses. Model services are not distributed or licensed by this repository.
