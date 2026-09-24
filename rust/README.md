# J++ Rust source implementation

Method effect types, relative source imports, fixed generation/responses and JSON
file actions are connected to the same core. See [the runnable guide](METHODS-AND-LIFECYCLE.md)
/ [中文说明](METHODS-AND-LIFECYCLE.zh-CN.md) for pending → resume → replay.

本包已在已提交第四包内核上验证方法契约、跨文件源码库和固定后端恢复；后续 core
在途修改由原归口继续，不属于本次验证快照。

Source parsing, lowering, shared checking and execution are connected. All five
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

## Getting usable exits on the live backend / 怎么让真机跑出可用出口

A live run returns readings, but `cut` turns a reading into act / ignore only with a
certified line; with no calibration record every exit is `unsure(cold)`. Lines come
only from labelled data (J-03). The truth channel imports labels and certifies them:

```sh
cargo build -p jpp-cli --features live --release
# 1. run the program live once and keep the readings (report / ledger)
# 2. label those readings: one JSON object per line, e.g.
#    {"form": {"op": "test", "template": "这段话是否提到了{city}？"}, "item": "n1", "p": 0.99, "label": true, "source": "computed"}
./target/release/jpp calib-import labels.jsonl --calib-out calib
# 3. run again with the records; replay later needs only the ledger
./target/release/jpp run examples/sieve.jpp --backend live --profiles-dir profiles --calib calib --ledger-out ledger.json
./target/release/jpp run examples/sieve.jpp --replay ledger.json
```

Live runs need a capability profile (B73). The repository ships `profiles/jev-1.13.0.json` (delta, concurrency, price, and the measurement IDs it was built from; generated from `地基/foundation/profile/profiles/` by `scripts/gen_profiles.py`). The CLI takes `--profile <file>`, else `--profiles-dir <dir>/<model>.json`, else `profiles/` next to the `jpp` executable; if none is found the run stops with `E-profile-missing` and lists the paths it tried. The profile hash goes into the ledger header, and the price comes only from the profile.

真机运行必须带能力画像（B73）。仓库附带 `profiles/jev-1.13.0.json`（含 δ、并发、价格，并注明来源实测编号；由 `scripts/gen_profiles.py` 从 `地基/foundation/profile/profiles/` 生成）。CLI 按 `--profile <文件>`，否则按 `--profiles-dir <目录>/<model>.json`，再否则按 `jpp` 可执行文件旁的 `profiles/` 查找；找不到时报 `E-profile-missing` 并列出试过的路径。画像哈希写进账本头，价格只从画像读。

真机只给读数；出口要靠校准线，线只从带真值的标注来。做法：先真机跑一次拿读数，给读数标真值（`human`、`computed` 或 `model:<名>`），用 `calib-import` 导入并认证，再带 `--calib` 运行。按题式（`form`）导入的线由该题式的所有填法共用（B2 待批，本版为回退层），出口会注明「题式级」。只有模型标注时，需要同一题式的人工抽检一致率达到门槛（默认 0.9）才上岗；达不到时记录标为「待核」，运行时告警写明原因。账本记下了当次用到的校准记录，只凭账本重放也得到同样的出口。

**样本量与两档线（B72、B75）。** 拆分认证要求四个格（选线半的正例、负例，认证半的上侧、下侧）各有足够的零错已决条数，并且随机分半。离线对照（`地基/评估/2026-09-24-B72对照/results_sweep.md`）得出的实际下限：正式线（`--alpha 0.1`）字面题式约 160 条，语义题式 200 条也不保证；试用线（`--alpha-trial 0.25`）字面题式约 60–80 条，语义题式约 80–100 条。`calib-import` 先按正式 α 认证，不过再按试用 α 认证；已有正式线的键不会被试用线覆盖。试用线的出口照常给出 act/ignore 供路由，报 `W-trial-line`，但不能放行不可逆 `do`。报告的 `exits` 表逐出口写明所用线的等级（`Certified` 正式、`Form` 题式、`Trial` 试用、`Class` 借线、`Fixture` 夹具、`Cold` 没用上线等）和是否放行。类记录（`class` 行）要来自至少两个不同题式（同一题式的不同填法算一个来源，`--class-min-sources`），每个来源的条数要够该档要求；类线同样只路由、不放行不可逆 `do`。

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
