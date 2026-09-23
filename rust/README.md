# J++ Rust source implementation

Method effect types, relative source imports, fixed generation/responses and JSON
file actions are now connected to the same core. The complete pending → resume →
replay example reuses completed work without rewriting its output on replay.
See [the runnable guide](METHODS-AND-LIFECYCLE.md) / [中文说明](METHODS-AND-LIFECYCLE.zh-CN.md).

方法类型、跨文件源码复用与固定后端的暂停／恢复已经贯通。本包基于已提交第四包内核；
后续在途核心能力不包含在这份快照中。真实模型 CLI 与更广算法库继续按原分工推进。

Source parsing, lowering, shared checking and execution are connected. All three
source examples, source-position errors, budget stopping and ledger replay have
passed integration tests. A native install outside the checkout runs with an empty
PATH, without Python or Cargo. The checks use fixed observations, not a live model.

本包的目标是直接写 `.jpp` 源码，通过检查后由唯一 Rust 内核执行。Python 已发布
程序保留为行为对照。前端不执行算法；`jpp-core` 负责值、方法环境、效应和运行。

## Build and run / 构建与运行

From this directory, using a Rust toolchain supporting edition 2024:

```sh
cargo build --workspace
cargo test --workspace
cargo run -p jpp-cli -- parse examples/composition.jpp
cargo run -p jpp-cli -- check examples/composition.jpp
cargo run -p jpp-cli -- run examples/composition.jpp
cargo run -p jpp-cli -- run examples/adaptive.jpp --fixtures examples/fixtures/adaptive.json --output adaptive-report.json
cargo run -p jpp-cli -- run examples/partial.jpp --fixtures examples/fixtures/partial.json --output partial-report.json --ledger-out partial-ledger.json
```

These source programs contain the methods. The CLI loads fixed observations and
registers a local `record_check` action, which records and returns the value the
source already computed. It contains no hidden search or candidate solver.

算法写在源码中。CLI只加载固定观察和登记动作；候选是否通过基本检查、怎样枚举
组合、满足什么约束、继续问谁，都由 `.jpp` 程序表达。

## What the programs demonstrate / 程序效果

`composition.jpp` passes two methods to `compose`, returns a new method, and composes
it again. Its result is 43. This needs no observation fixture.

`adaptive.jpp` constructs each next question from the previous answer. The supplied
ten fixed observations locate 731 among 1,000 candidates. The source uses the common
bounded loop and explicit stop. Question values are constructed and passed to the
ordinary observation method.

`partial.jpp` validates candidates, builds their power set, applies cost/skill
constraints, and packages the result with a continuation. It uses A+B at cost9
while C/D remain unresolved, then asks only about C to obtain cost2, and finally
uses another strategy for D. Already checked candidates are retained. Expected
observations:6; local checks:A,B,C once each. The continuation is a source function
capturing algorithm state and other methods, represented by core AST/environment.

组合例子返回一个新方法再调用。选问例子根据前次答案产生下一题。部分结果例子先
使用够用的方案，再变更策略继续处理，并保留旧检查。精确组合投影仍重新计算；
本包不宣称任意算法都有自动增量优化。

## Observations, partial results and replay / 观察、部分结果与重放

The fixture JSON contains exact material/question/answer records and explicitly
synthetic calibration. No model API is called. Missing records are errors, not
guessed answers. The source observation helper uses test → judge → cut → an
exhaustive handle; it retains material, question and handled exit alongside the
resolved/value fields. Revised material yields a distinct observation identity.

Two kinds of unfinished work are separate: `value.pending` belongs to the source
algorithm (a usable result can still have pending candidates); the report's outer
`pending` is a program-level halt such as exhausted budget. A method's declared
completion criterion does not claim every question is resolved.

The ledger records common-core effects, not serialized native closures. Replaying
the same source and fixtures reconstructs its method environments and uses recorded
effects. The CLI's replay mode uses NoCallClient to reject new model requests:

```sh
cargo run -p jpp-cli -- run examples/partial.jpp --fixtures examples/fixtures/partial.json --replay partial-ledger.json --output replay-report.json
```

未决候选属于算法返回值；程序级挂起另行显示。账本保存观察和动作记录，重放重建
方法环境；不能把它说成任意闭包已支持跨进程保存。保持相同源码、输入和校准记录
才是本包验证的重放条件。

## Source and diagnostics / 源码与诊断

Read [the grammar and frontend boundary](FRONTEND.md). `budget` declares literal
resource limits. Functions accept optional type/effect annotations; the shared
checker defines and checks them, rather than relying on Rust's type system.
Parser, checker and runtime errors are rendered against the `.jpp` file/line/column.
`examples/errors/` provides malformed syntax, missing budget and wrong argument type.

Structure: `crates/jpp-frontend` parses and lowers source; `crates/jpp-core` checks
and interprets it; `crates/jpp-cli` wires input files, fixtures and reports.
`examples/expected/` records behavior to compare against the retained reference.

[Source versus direct core construction](COMPARISON.md) includes a runnable
equivalence test. To install a native command outside this checkout:

```sh
cargo install --locked --path crates/jpp-cli --root /tmp/jpp-native
/tmp/jpp-native/bin/jpp run examples/composition.jpp
```

The installed executable runs without Python or Cargo. Building it requires the
Rust toolchain; fixed-fixture runs require no account, API key or network access.
Both the retained Python package and native executable use the name `jpp`; use
the explicit native path when both are installed.

尚未迁移的 Python 优化、实验工具及接口不自动视为 Rust 已有能力。此首包的交付
判断依据是上述源码实际运行及行为对照，不是 Rust 文件数量或历史 Python 测试数。
