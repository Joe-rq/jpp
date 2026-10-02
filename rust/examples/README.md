# 五个随仓示例 / Five bundled examples

这五个示例都不花钱：用的是固定观察（`--fixtures`），没有真调判断器，读数是合成的，只检查构造，不代表模型表现。
命令在 `地基/rust-jpp/` 下执行，`jpp` 是装好的可执行文件（没装时把 `jpp` 换成 `cargo run -p jpp --`）。
每条命令会往 stdout 打一份 JSON 报告，要看的是其中的 `value`；其余字段（`cost`、`exits`、`trace` 等）是账与证据。
测试 `crates/jpp/tests/examples_five.rs` 逐条执行下面这五行命令，把输出与 `tests/golden/` 里的金样核对，README 里的命令和测试里的不一致时测试会失败。

## 1. 批量入口：只给一句目的

```sh
jpp run examples/purpose-run.jpp --purpose "在这些供应商里找出最可能按期交付我们这份订单的，排好先后，并说出每家最大的风险。" --fixtures examples/fixtures/purpose-run.json
```

三家供应商，作者只写了这句目的，一道题也没写。语言自己把目的拆成模块、出题、过闸门，再对每家问同一批题。
`value` 里每家有两个字段：`rank`（按期交付把握的名次，1 最高）和 `risk`（最大的风险是产能、资金还是质量）；调用 17 次，其中生成器 4 次，与供应商家数无关。

## 2. 过程入口：按步出题，配一个小世界

```sh
jpp run examples/purpose-drive.jpp --purpose "控制数轴上的一个点，尽快走到目标位置。" --fixtures examples/fixtures/purpose-drive.json --env 'walk=python3 worlds/walk.py'
```

`examples/worlds/walk.py` 是个几十行的小世界：数轴上的点从 0 出发，每步按动作左右移一格。`--env` 把它登记成名为 `walk` 的世界（路径相对程序文件所在目录）。
题只出一次，之后每走一步只判断「下一步做什么」。开两局，目标分别在 3 和 -2，`value.rows` 逐步列出动作（`right`、`right`、`right`，`left`、`left`），`value.results` 显示两局都走到了。

## 3. 作者填模块：只填十个模块，不写题面

```sh
jpp run examples/modules-fill.jpp --fixtures examples/fixtures/modules-fill.json
```

作者给材料（三套房源）、谓词（满足需求的程度、最大的不合适之处）、切法、参照、语境和前提；题面和输入槽由语言按守则组装，没填的模块由语言补上并标注来源。
`value` 里每套房有两个字段（`fit`、`flaw`），`questions` 是组装出的两道题，`sources` 记每个模块是作者给的还是系统补的。

## 4. 补信息：拿不准时去取材料，再判一次

```sh
jpp run examples/unsure-default.jpp --fixtures examples/fixtures/unsure-default.json
```

四家供应商里有两家判断器答不准（读数恰好 0.5），程序一个 `handle` 都没写，只用 `unsure_source` 登记了怎么取材料。语言先判断缺哪一类信息：缺「过往项目」的那家按登记的 `fetch` 取来材料再判，判成了；材料都在、题也清楚的那家两可，记账放弃。
报告的 `unsure_default` 段两条，一条 `end: "decided"` 带 `fetched: ["过往项目"]`，一条 `end: "drop"`。

## 5. 重放：只凭账本，结果相同

```sh
jpp run examples/purpose-run.jpp --purpose "在这些供应商里找出最可能按期交付我们这份订单的，排好先后，并说出每家最大的风险。" --replay tests/golden/purpose-run/ledger.json
```

同样是第 1 个示例的程序，这次不给 `--fixtures`，改用示例 1 跑出来的账本（仓库自带，`tests/golden/purpose-run/ledger.json`）。输出里 `cost.calls` 是 0、`cost.replayed` 是 106，`value` 与示例 1 逐字相同：账本记下了每一次判断和生成，重放不再调用任何东西。
自己的程序想留账本，运行时加 `--ledger-out 账本文件`，以后用 `--replay 账本文件` 重放。

## 重建夹具

`examples/fixtures/purpose-run.json` 和 `purpose-drive.json` 由脚本生成，示例 3、4 的夹具是手写的：

```sh
python3 examples/fixtures/build-fixtures.py
```
