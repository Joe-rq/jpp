# J++ (Rust implementation) / J++（Rust 实现）

**What it is.** J++ is a small programming language for programs that need a judgment a model can make
("is this supplier likely to deliver on time?") together with ordinary code, search and graph algorithms.
You write a `.jpp` file; the `jpp` command checks it and runs it. A question is a value, a method is a value,
and a finished method can be passed to another one. This folder is the native implementation: one Rust workspace
and the `jpp` command-line tool. Everything below runs offline on recorded answers and costs nothing.

**这是什么。** J++ 是一门小语言，用来写「既要让模型做判断（这家供应商能按期交货吗？），又要普通代码、搜索和图算法」的程序。
你写一个 `.jpp` 文件，`jpp` 命令检查并运行它。问题是值，方法也是值，写好的方法可以交给另一个方法。
本目录是原生实现：一个 Rust 工作区和命令行工具 `jpp`。下面所有命令都离线运行，用事先录好的答案，不花钱。

## Install and run the first example / 装上，跑第一个例子

You need a Rust toolchain at 1.85 or newer (the code uses the 2024 edition) and Python 3 (only the process example uses it).
Get the repository, go to the folder that holds this file (`rust/` in the public repository), and install:

你需要 1.85 或更新的 Rust 工具链（代码用 2024 版），以及 Python 3（只有过程入口的例子用到它）。
取到仓库，进入放着本文件的目录（公开仓库里是 `rust/`），装上：

```text
git clone https://github.com/Towow-ai/jpp.git
cd jpp/rust
```

```sh
cargo install --locked --path crates/jpp
jpp run examples/composition.jpp 2>/dev/null | tail -n 5
```

The first command builds `jpp` in release mode (about two minutes) and puts it in `~/.cargo/bin`; the second runs a program that
composes two small methods into a new one. The report is JSON, and the program's own answer is the `value` at its end:

第一条命令把 `jpp` 编成 release 版（约两分钟）并放进 `~/.cargo/bin`；第二条运行一个把两个小方法组合成新方法的程序。
报告是 JSON，程序自己的答案是结尾的 `value`：

```output
  "value": {
    "expected": 43,
    "result": 43
  }
}
```

Run programs from this folder: a program finds its libraries (`import "../lib/…"`) relative to its own file, and the installed `jpp`
does not carry `lib/` or `examples/` with it. If you also installed the Python package of this project, its command is also called `jpp`;
call the native one by its full path (`~/.cargo/bin/jpp`). Lines on the screen that start with `W-` are advice about the program,
not failures; a real failure starts with `E-` and `jpp` exits with a non-zero code.

请在本目录里运行程序：程序按自己文件的位置找库（`import "../lib/…"`），装出来的 `jpp` 不带 `lib/` 和 `examples/`。
如果还装了本项目的 Python 包，它的命令也叫 `jpp`，用完整路径（`~/.cargo/bin/jpp`）调原生这个。屏幕上以 `W-` 开头的行是对程序的建议，
不是失败；真正的失败以 `E-` 开头，`jpp` 会以非零码退出。

## Three ways in, one example each / 三种入口，各一个例子

All three take a plain-language purpose (or the pieces of one) and build the questions for the judge themselves. The recorded answers in
`examples/fixtures/` are made up for the demo: they check that the program is built and wired correctly, and say nothing about how well a real model judges.
Two more examples (filling in missing information, and replaying a run from its ledger) are in [examples/README.md](examples/README.md).

三种入口都从一句话的目的（或目的的各个部分）出发，由语言自己给判断器出题。`examples/fixtures/` 里录好的答案是演示用的合成数据：
它们检查程序搭得对不对，不代表真实模型判得准不准。另外两个例子（拿不准时补信息、按账本重放）在 [examples/README.md](examples/README.md)。

**1. A batch of items: `purpose_run`.** You give one purpose and a list of items (here three suppliers and our order). The language reads the
purpose, writes the questions, asks every item the same questions and returns one record per item.
**批量入口。** 给一句目的和一批条目（这里是三家供应商和我们的订单）。语言读目的、写题，对每个条目问同一批题，每个条目返回一条记录。

```sh
jpp run examples/purpose-run.jpp --purpose "在这些供应商里找出最可能按期交付我们这份订单的，排好先后，并说出每家最大的风险。" --fixtures examples/fixtures/purpose-run.json --output report.json
python3 -c "import json; [print(r['item']['name'], r['fields']) for r in json.load(open('report.json'))['value']['value']]"
```

```output
examples/purpose-run.jpp: returned (new model calls: 17); report: report.json
供应商0 {'rank': 1, 'risk': '产能'}
供应商1 {'rank': 3, 'risk': '资金'}
供应商2 {'rank': 2, 'risk': '质量'}
```

`rank` 1 is the most likely to deliver on time; `risk` is the biggest risk. The source, [`examples/purpose-run.jpp`](examples/purpose-run.jpp), lists the three
suppliers and writes no question text. `rank` 1 是最可能按期交货的；`risk` 是最大的风险。源码 [`examples/purpose-run.jpp`](examples/purpose-run.jpp) 只列了三家供应商，没写任何题面。

**2. A process that keeps changing: `purpose_drive`.** A small world (a point on a number line that must reach a goal) answers over a command-line pipe with
what it looks like now and which actions are allowed. At every step the language asks the judge for the best next action, hands it back to the world and
repeats until the world says it is done. The path after `python3` is relative to the program's folder (`examples/`).
**过程入口。** 一个小世界（数轴上要走到目标的一个点）通过命令行管道告诉程序「现在什么样、能做哪些动作」。每一步语言问判断器最该做哪个动作，交还给世界，
重复到世界说完成。`python3` 后面的路径相对程序文件所在目录（`examples/`）。

```sh
jpp run examples/purpose-drive.jpp --purpose "控制数轴上的一个点，尽快走到目标位置。" --fixtures examples/fixtures/purpose-drive.json --env 'walk=python3 worlds/walk.py' --ledger-out drive-ledger.json --output report.json
python3 -c "import json; v=json.load(open('report.json'))['value']; print([x['action'] for x in v['rows']])"
```

```output
examples/purpose-drive.jpp: returned (new model calls: 21); report: report.json
['right', 'right', 'right', 'left', 'left']
```

Two games (goal at 3, goal at −2) were played in five steps in total. Every step went into `drive-ledger.json`; running again with `--replay drive-ledger.json`
reproduces the result without starting the world or asking the judge. 两局（目标在 3 和 −2）一共走了五步。每一步都记进 `drive-ledger.json`，
再加 `--replay drive-ledger.json` 运行，不用启动世界、不用问判断器就能重现结果。

**3. You fill in the modules yourself: `modules_run`.** Instead of one sentence you write the pieces the language would otherwise extract
(the material, the predicates and how to cut them, a reference, a context, a premise); the language assembles questions from your pieces by the same rules.
**作者填模块。** 不写一句话，而是自己填语言本来要从话里抽出来的各个部分（材料、谓词与切法、参照、语境、前提），语言按同一套规则用你的模块组装题。

```sh
jpp run examples/modules-fill.jpp --fixtures examples/fixtures/modules-fill.json --output report.json
python3 -c "import json; [print(q['q']) for q in json.load(open('report.json'))['value']['questions']]"
```

```output
examples/modules-fill.jpp: returned (new model calls: 5); report: report.json
就这份材料来说，这套房满足租客需求的程度落在哪一档？
就这份材料来说，这套房对这位租客最大的不合适之处是下列哪一项？
```

The two questions above were written by the language from the modules in [`examples/modules-fill.jpp`](examples/modules-fill.jpp); read that file to see what you fill in.
上面两道题是语言按 [`examples/modules-fill.jpp`](examples/modules-fill.jpp) 里的模块写出来的；看那个文件就知道要填什么。

The first line of each output is `jpp`'s own summary; a "model call" here is a lookup in the recorded answers. The derive library prints a number of `W-` advice
lines about itself on the screen while these programs run (about its own source, not about yours); they are known and harmless, and `2>/dev/null` hides them.
每段输出的第一行是 `jpp` 自己的小结，这里的「模型调用」是在录好的答案里查一次。这几个程序运行时，出题库会在屏幕上打印一些关于它自己源码的 `W-` 建议行
（不是关于你的程序），已知且无害，加 `2>/dev/null` 可以隐藏。

## What it cannot do yet / 现在还不能做什么

- On a real model it has so far run two real projects (matching résumés to jobs, triaging GitHub issues) with results comparable to hand-written versions, and a game bot that did not reach a usable level; the examples here use recorded answers, and how well judgments hold up depends on the model and the questions, so it is measured per project, not promised by the language.
- Long-running programs that react to events as they arrive (for example a network of hundreds of agents) are not written in `.jpp` yet; a program is one input, one result.
- A world driven through `--env` must hand its whole state back and forth at every step (one subprocess per step); simulators whose state cannot be serialized cannot be attached yet.
- The checker catches the common mistakes before a run, but not every one (an unresolved judgment passed through several functions is caught only when the program ends).
- 在真实模型上目前跑通了两个真实项目（简历与岗位匹配、GitHub issue 分诊），效果与手写版相当，另有一个游戏机器人没有打出能用的水平；本目录的例子用的是录好的答案，判得准不准取决于模型和题，逐项目测量，语言不做担保。
- 事件一到就反应的长驻程序（比如几百个主体的网络）还不能用 `.jpp` 写；一个程序是一次输入、一次返回。
- 通过 `--env` 驱动的世界每一步都要把整份状态传进传出（每步起一个子进程）；状态无法序列化的仿真还接不进来。
- 检查器能在运行前拦下常见的错，但不是所有：未决的判断结果经过几层函数转手，要到程序结束时才会被发现。

**Reporting a problem / 报问题。** Open an issue at [github.com/Towow-ai/jpp/issues](https://github.com/Towow-ai/jpp/issues) with the command you ran, the `E-` or `W-` lines it printed
and, if you can, the `--ledger-out` file. 到 [github.com/Towow-ai/jpp/issues](https://github.com/Towow-ai/jpp/issues) 提 issue，贴上你运行的命令、屏幕上以 `E-` 或 `W-` 开头的行，
最好再附上 `--ledger-out` 写出的账本文件。

## Where to go next / 接下来看哪里

- [GUIDE.md](GUIDE.md): one-page developer guide: commands, built-ins, fixtures, live runs. 开发指南：命令、内置函数、夹具、真机运行。
- [FRONTEND.md](FRONTEND.md): the grammar. 语法。
- [crates/jpp/INTERFACE.md](crates/jpp/INTERFACE.md): the detailed interface of every option and library function. 每个开关和库函数的详细接口。
- [METHODS-AND-LIFECYCLE.md](METHODS-AND-LIFECYCLE.md) / [中文](METHODS-AND-LIFECYCLE.zh-CN.md): observe → pending → resume → replay, runnable. 观察、挂起、续跑、重放的可运行教程。
- `examples/`: sixty-odd small programs, each with recorded answers in `examples/fixtures/` and an expected report in `tests/golden/`. 六十多个小程序，答案在 `examples/fixtures/`，期望输出在 `tests/golden/`。
- Using a real model instead of recorded answers: see "Getting usable exits on the live backend" below. 换成真实模型：见下面「怎么让真机跑出可用出口」。

## Status / 现状

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
程序保留为行为对照。前端不执行算法；`jpp-runtime` 负责值、方法环境、效应和运行（步 14a 前是 `jpp-core`）。

## Build, test and the older run commands / 构建、测试与早期的运行命令

From this directory, using a Rust toolchain supporting edition 2024:

```sh
cargo build --workspace
cargo test --workspace
cargo run -p jpp -- parse examples/composition.jpp
cargo run -p jpp -- check examples/composition.jpp
cargo run -p jpp -- run examples/composition.jpp
cargo run -p jpp -- run examples/adaptive.jpp --fixtures examples/fixtures/adaptive.json --output adaptive-report.json
cargo run -p jpp -- run examples/partial.jpp --fixtures examples/fixtures/partial.json --output partial-report.json --ledger-out partial-ledger.json
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
effects. The CLI's replay mode uses ReplayPorts to reject new model requests:

```sh
cargo run -p jpp -- run examples/partial.jpp --fixtures examples/fixtures/partial.json --replay partial-ledger.json --output replay-report.json
```

未决候选属于算法返回值；程序级挂起另行显示。账本保存观察和动作记录，重放重建
方法环境；不能把它说成任意闭包已支持跨进程保存。保持相同源码、输入和校准记录
才是本包验证的重放条件。

## Getting usable exits on the live backend / 怎么让真机跑出可用出口

A live run returns readings. With no line and no calibration record, `cut` follows the
judge's own answer (grade `Answer`); a certified line is optional, for when you want the
language to vouch for the error rate. Certified lines come only from labelled data (J-03).
The truth channel imports labels and certifies them (the profile supplies δ):

```sh
cargo build -p jpp --features live --release
# 1. run the program live once and keep the readings (report / ledger)
# 2. label those readings: one JSON object per line, e.g.
#    {"form": {"op": "test", "template": "这段话是否提到了{city}？"}, "item": "n1", "p": 0.99, "label": true, "source": "computed"}
./target/release/jpp calib-import labels.jsonl --profile profiles/jev-1.13.0.json --calib-out calib
# 3. run again with the records; replay later needs only the ledger
./target/release/jpp run examples/sieve.jpp --backend live --profiles-dir profiles --calib calib --ledger-out ledger.json
./target/release/jpp run examples/sieve.jpp --replay ledger.json
```

Live runs need a capability profile (B73). The repository ships `profiles/jev-1.13.0.json` (delta, concurrency, price, and the measurement IDs it was built from; generated from `地基/foundation/profile/profiles/` by `scripts/gen_profiles.py`). The CLI takes `--profile <file>`, else `--profiles-dir <dir>/<model>.json`, else `profiles/` next to the `jpp` executable; if none is found the run stops with `E-profile-missing` and lists the paths it tried. The profile hash goes into the ledger header, and the price comes only from the profile.

真机运行必须带能力画像（B73）。仓库附带 `profiles/jev-1.13.0.json`（含 δ、并发、价格，并注明来源实测编号；由 `scripts/gen_profiles.py` 从 `地基/foundation/profile/profiles/` 生成）。CLI 按 `--profile <文件>`，否则按 `--profiles-dir <目录>/<model>.json`，再否则按 `jpp` 可执行文件旁的 `profiles/` 查找；找不到时报 `E-profile-missing` 并列出试过的路径。画像哈希写进账本头，价格只从画像读。

真机只给读数；出口要靠校准线，线只从带真值的标注来。做法：先真机跑一次拿读数，给读数标真值（`human`、`computed` 或 `model:<名>`），用 `calib-import` 导入并认证，再带 `--calib` 运行。按题式（`form`）导入的线由该题式的所有填法共用（B2 待批，本版为回退层），出口会注明「题式级」。只有模型标注时，需要同一题式的人工抽检一致率达到门槛（默认 0.9）才上岗；达不到时记录标为「待核」，运行时告警写明原因。账本记下了当次用到的校准记录，只凭账本重放也得到同样的出口。

**样本量与两档线（B86、B72、B75）。** `calib-import` 缺省按固定序认证（`--certify fixed-sequence`，B86）：候选阈值只由读数生成，从严到宽逐个检验，第一次不过即停，不拆分样本，每侧的保证与拆分认证同级。拆分认证丢掉的那一半样本，主要买的是「保证有证明」，而不是防止严重过拟合（嵌套阈值族上同批取最宽线的实际膨胀只有约两倍，但没有证明）；固定序不花样本就给出证明。正式线（`--alpha 0.1`，可放行不可逆 `do`）：字面题式约 60 条、语义题式约 60–80 条，外延未定的题式先改题面再标；试用线（`--alpha-trial 0.25`，可路由）：约 32 条，读数分散的题式约 40 条。`--certify split` 是旧的拆分认证（B85 起改为分层交替分半），条数约 2–3 倍。数字出处：`地基/评估/2026-09-24-新题标注门槛-对照/results.md` 与 `裁定复算/recompute.out.txt`。证书写明认证方式、候选规则版本、步长（`--step`，缺省池的 5%）与两侧停点。

**新题上手：边标边导，够了就停（B87、B88）。** 先跑一次（全冷，只得读数）；用 `jpp calib-import --from-ledger 账本 --key 键 --list-out 清单.jsonl --report 首跑报告.json [--materials 材料.json]` 导出待标清单（每行有材料编号 `item`、题哈希 `q`、组号，带 `--report` 时另有题面与填法，不带读数与出口，标注者看不到判断器的答案；一道题式多个填法问同一批材料时同一 `item` 出现多次，标注行要带上 `q`，B107）；按清单顺序标，首组约 24 + 24 条即可得可路由的窄线，继续标到程序报「再标也不会更宽」即正式线；每次把累计的标注用 `jpp calib-import 标注.jsonl --from-ledger 账本 --calib-out 目录` 导入（缺省序贯认证、两端先标的顺序；没停时门控写明已标多少、各侧 E 值、还差约几条）；再带 `--calib` 运行。序贯零错门槛：正式每侧 24 条、试用 9 条。`calib-import` 先按正式 α 认证，不过再按试用 α 认证；已有正式线的键不会被试用线覆盖。试用线的出口照常给出 act/ignore 供路由，报 `W-trial-line`，但不能放行不可逆 `do`。报告的 `exits` 表逐出口写明所用线的等级（`Certified` 正式、`Form` 题式、`Trial` 试用、`Class` 借线、`Fixture` 夹具、`Cold` 没用上线等）和是否放行。类记录（`class` 行）要来自至少两个不同题式（同一题式的不同填法算一个来源，`--class-min-sources`），每个来源的条数要够该档要求；类线同样只路由、不放行不可逆 `do`。

## Source and diagnostics / 源码与诊断

Read [the grammar and frontend boundary](FRONTEND.md). `budget` declares literal
resource limits. Functions accept optional type/effect annotations; the shared
checker defines and checks them, rather than relying on Rust's type system.
Parser, checker and runtime errors are rendered against the `.jpp` file/line/column.
`examples/errors/` provides malformed syntax, missing budget and wrong argument type.

Structure (B74, step 14a): `crates/jpp-syntax` parses and lowers source to the IR
(`crates/jpp-ir`); `crates/jpp-check` checks it, `crates/jpp-plan` plans batching, and
`crates/jpp-runtime` interprets it, over `jpp-value`, `jpp-effects`, `jpp-ledger` and
`jpp-calib`. `crates/jpp` is the host crate: its lib target is the facade and `Session`
(`src/session/`) plus the live backend (`src/backends/`, feature `live`); its bin target
`jpp` (`src/cli/`) wires input files, fixtures and reports.
结构（B74，步 14a）：`jpp-syntax` 解析并降到 IR，`jpp-check` 检查，`jpp-plan` 做合批规划，`jpp-runtime`
解释执行；`jpp` 是宿主 crate，lib 目标是外观与 `Session`、真机端口，bin 目标 `jpp` 是命令行。
`examples/expected/` records behavior to compare against the retained reference.

[Source versus direct core construction](COMPARISON.md) includes a runnable
equivalence test. To install the native command, see the top of this file
(`cargo install --locked --path crates/jpp`; add `--root <dir>` to install elsewhere).

The installed executable runs without Python or Cargo. Building it requires the
Rust toolchain; fixed-fixture runs require no account, API key or network access.
Both the retained Python package and native executable use the name `jpp`; use
the explicit native path when both are installed.

尚未迁移的 Python 优化、实验工具及接口不自动视为 Rust 已有能力。此首包的交付
判断依据是上述源码实际运行及行为对照，不是 Rust 文件数量或历史 Python 测试数。
