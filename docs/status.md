# Current scope / 当前实现范围

Updated 2026-09-23 after the kernel synchronization in [PR #25](https://github.com/Towow-ai/jpp/pull/25). Standalone `.jpp` source runs through the native Rust implementation in [`rust/`](../rust/README.md). The Python package and published discovery demonstrations remain available as references and experiments. [Delivered source milestone](https://github.com/Towow-ai/jpp/pull/12).

- Rust: source parser, shared AST, a defined static-check subset, interpreter, native CLI, budgets, fixed observations and ledger replay. Source programs implement nested method composition, adaptive questions and partial-result continuation.
- Python: retained runtime/composition library, installable package and discovery experiments, including published real JEV recordings. These capabilities are not automatically all present in Rust.
- Not delivered: machine-code compilation, a complete static type/effect system, arbitrary closure serialization, or a standalone Rust live-backend CLI. Exact remaining rule differences are listed in the [core interface](../rust/crates/jpp-core/INTERFACE.md).
- Fixed in the public Rust kernel: captured method identity, preservation of completed over-budget calls, and checked integer arithmetic (rules 4–6 of the [design revision](../research/地基/13-Rust实践反馈设计修订-v0.2.md)). `v13_rules.rs` exercises their behavior; `known_defects.rs` now adds an enabled regression for the exact overflow source span instead of three obsolete ignored reproducers.
- Also present: unresolved exits carried as values, rank-1 effect polymorphism, source-library methods and fixture-backed lifecycle examples. These were once ahead of the public snapshot; their code is now included. Remaining restrictions are documented in `INTERFACE.md`.
- Next: complete the native live-backend CLI and labeled calibration workflow, then reusable semantic algorithms and a bounded application. The host calibration APIs are experimental; selected-threshold risk guarantees are not established. See [Rust status](../rust/README-status.md) and the [roadmap](../ROADMAP.md).

独立语法、原生执行和后续 Rust 修复已经进入公开仓库。此前三条缺陷已有正常运行的回归测试，源码方法库、未决责任和效应行实现也已同步。41 项是[首包验证记录](verification.md)的历史数字；当前核查入口见 [Rust 状态](../rust/README-status.md)。下一步是原生真机入口和标注校准流程，并继续构建可复用算法；统计选线实验不能视为一般风险保证。

公开仓库不包含凭据、私人对话或模型权重。经整理的公开模型录制与实验输出已随发现案例提供；不能再笼统地称仓库没有模型录制。

## Backends

The native `jpp run` examples use fixed JSON observations; the retained Python `jpp demo` uses `FixtureClient`. Both use synthetic calibration for mechanism tests at zero API cost. The two executables share the name `jpp`; use an explicit native path if both are installed.

The bundled historical JEV adapter targets a specific API/model version and reads a credential from `~/.typesafe-key`. It is not used by the demo. Its current service compatibility and model accuracy have not been validated as part of this release. Do not reuse fixture calibration records for real decisions. A documented, tested live-backend quickstart is a roadmap item.

PyYAML is a dependency of the bundled foundation utilities; pytest is a development dependency. They are installed separately and retain their own licenses. Model services are not distributed or licensed by this repository.
