# Current scope / 当前实现范围

Updated 2026-09-21. Standalone `.jpp` source runs through the native Rust implementation in [`rust/`](../rust/README.md). The Python package and published discovery demonstrations remain available as references and experiments. [Delivered source milestone](https://github.com/Towow-ai/jpp/pull/12).

- Rust: source parser, shared AST, a defined static-check subset, interpreter, native CLI, budgets, fixed observations and ledger replay. Source programs implement nested method composition, adaptive questions and partial-result continuation.
- Python: retained runtime/composition library, installable package and discovery experiments, including published real JEV recordings. These capabilities are not automatically all present in Rust.
- Not delivered: machine-code compilation, a complete static type/effect system, arbitrary closure serialization, or a standalone Rust live-backend CLI. Exact remaining rule differences are listed in the [core interface](../rust/crates/jpp-core/INTERFACE.md).
- Next: reusable source-library methods, executable composition rules and a bounded application using standalone source. See the [roadmap](../ROADMAP.md).

独立语法和原生执行已经交付，当前不是“只有 Python 封装”。41 项 Rust 测试覆盖完整程序、错误定位、预算停止、直接内核对照与重放；[验证记录](verification.md)提供依据。下一步要让这些源码方法更方便地复用，并让真实应用用上它们。

公开仓库不包含凭据、私人对话或模型权重。经整理的公开模型录制与实验输出已随发现案例提供；不能再笼统地称仓库没有模型录制。

## Backends

The native `jpp run` examples use fixed JSON observations; the retained Python `jpp demo` uses `FixtureClient`. Both use synthetic calibration for mechanism tests at zero API cost. The two executables share the name `jpp`; use an explicit native path if both are installed.

The bundled historical JEV adapter targets a specific API/model version and reads a credential from `~/.typesafe-key`. It is not used by the demo. Its current service compatibility and model accuracy have not been validated as part of this release. Do not reuse fixture calibration records for real decisions. A documented, tested live-backend quickstart is a roadmap item.

PyYAML is a dependency of the bundled foundation utilities; pytest is a development dependency. They are installed separately and retain their own licenses. Model services are not distributed or licensed by this repository.
