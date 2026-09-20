# Current scope / 当前实现范围

Updated 2026-09-21. Standalone `.jpp` source runs through the native Rust implementation in [`rust/`](../rust/README.md). The Python package and published discovery demonstrations remain available as references and experiments. [Delivered source milestone](https://github.com/Towow-ai/jpp/pull/12).

- Rust: source parser, shared AST, a defined static-check subset, interpreter, native CLI, budgets, fixed observations and ledger replay. Source programs implement nested method composition, adaptive questions and partial-result continuation.
- Python: retained runtime/composition library, installable package and discovery experiments, including published real JEV recordings. These capabilities are not automatically all present in Rust.
- Not delivered: machine-code compilation, a complete static type/effect system, arbitrary closure serialization, or a standalone Rust live-backend CLI. Exact remaining rule differences are listed in the [core interface](../rust/crates/jpp-core/INTERFACE.md).
- Known defects, reproduced in the repository: a method's identity ignores what it captured, so `transform` can return another method's cached result; a completed model call is discarded when its actual cost exceeds the budget, so resuming pays twice; integer overflow panics in a debug build instead of raising a source-located runtime error. All three are pinned by `#[ignore]`d tests in `rust/crates/jpp-core/tests/known_defects.rs` — run `cargo test -p jpp-core --test known_defects -- --ignored` to see them fail. They are rules 4, 5 and 6 of the [current design revision](../research/地基/13-Rust实践反馈设计修订-v0.2.md). Rules 5 and 6 are already fixed in the kernel workspace and will arrive with the next kernel merge; rule 4 is in progress there.
- Ahead of this snapshot, not yet merged: the kernel workspace has two further packages — an unresolved exit carried as a value rather than discharged by entering an arm, and rank-1 effect polymorphism that makes `!{…}` on a higher-order method a checked upper bound instead of documentation. Until they merge, `INTERFACE.md` §七.5 understates what the kernel can check.
- Next: the six revisions in [`13`](../research/地基/13-Rust实践反馈设计修订-v0.2.md), then reusable source-library methods, executable composition rules and a bounded application using standalone source. See the [roadmap](../ROADMAP.md) and the [dated update](updates/2026-09-21-design-revisions-from-rust-practice.md).

独立语法和原生执行已经交付，当前不是“只有 Python 封装”。41 项 Rust 测试覆盖完整程序、错误定位、预算停止、直接内核对照与重放；[验证记录](verification.md)提供依据。另有三条已知缺陷在库里有复现测试（方法身份漏掉捕获值、超预算抹掉已发生的调用、整数溢出在 debug 构建下 panic），默认跳过，加 `--ignored` 可以看见它们失败。下一步是落实[设计修订](../research/地基/13-Rust实践反馈设计修订-v0.2.md)的六条规则，并让这些源码方法更方便地复用。

公开仓库不包含凭据、私人对话或模型权重。经整理的公开模型录制与实验输出已随发现案例提供；不能再笼统地称仓库没有模型录制。

## Backends

The native `jpp run` examples use fixed JSON observations; the retained Python `jpp demo` uses `FixtureClient`. Both use synthetic calibration for mechanism tests at zero API cost. The two executables share the name `jpp`; use an explicit native path if both are installed.

The bundled historical JEV adapter targets a specific API/model version and reads a credential from `~/.typesafe-key`. It is not used by the demo. Its current service compatibility and model accuracy have not been validated as part of this release. Do not reuse fixture calibration records for real decisions. A documented, tested live-backend quickstart is a roadmap item.

PyYAML is a dependency of the bundled foundation utilities; pytest is a development dependency. They are installed separately and retain their own licenses. Model services are not distributed or licensed by this repository.
