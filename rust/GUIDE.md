# J++ 新开发者一页纸

一页够用的入门；完整接口在 `crates/jpp-core/INTERFACE.md`（现行接口与施工日志混排，
按小节标题找），语法在 `FRONTEND.md`，观察/重放的可运行教程在 `METHODS-AND-LIFECYCLE.md`。
本文全部命令已在本仓库实跑验证（2026-09-24）。

## 装、跑

```sh
cargo build --workspace          # 需要支持 edition 2024 的 Rust 工具链
cargo test --workspace           # 含金样与重放对照
cargo run -p jpp-cli -- parse examples/composition.jpp --ast
cargo run -p jpp-cli -- check examples/composition.jpp
cargo run -p jpp-cli -- run examples/composition.jpp
cargo run -p jpp-cli -- run examples/adaptive.jpp \
  --fixtures examples/fixtures/adaptive.json --output report.json
```

`run` 默认吃固定观察（fixture），不发真实模型请求；不带 `--fixtures` 时只能跑不含
判断效应的程序（如 `composition.jpp`）。`--output` 落报告 JSON，`--ledger-out` 另落账本，
`--replay <ledger.json>` 用账本重放（拒绝任何新调用）、`--resume <ledger.json>` 续跑。

脱离本仓库单独安装：

```sh
cargo install --locked --path crates/jpp-cli --root /tmp/jpp-native
/tmp/jpp-native/bin/jpp run examples/composition.jpp
```

装出来的可执行文件不需要 Python 或 Cargo；固定观察运行不需要账号、密钥或网络。

## 夹具（fixture）格式

一份 JSON，键为 `description`（自由文本）、`calibrations`（该次运行要用到的校准记录，
测试用途可手写，正式线不许手写，见下）、`observations`（材料 × 题 → 读数的穷举表，
缺一条就是错误而不是瞎猜）。

```json
{
 "description": "……",
 "calibrations": [
   {"key": "cs-refund", "hi": 0.75, "lo": 0.25, "n": 1, "status": "上岗"}
 ],
 "observations": [
   {"on": ["材料文本或状态记录"], "op": "test", "text": "题面文字",
    "calib": "cs-refund", "answer": {"Noul": 0.92}},
   {"on": ["材料"], "over": ["候选一", "候选二", "候选三"], "op": "select",
    "text": "……", "calib": "……", "answer": {"Choice": [0.9, 0.07, 0.03]}},
   {"on": [{"a": "…", "b": "…"}], "op": "measure", "scale": ["档位一", "档位二", "档位三"],
    "text": "……", "calib": "……", "answer": {"Score": [0.02, 0.08, 0.9]}}
 ]
}
```

`op` 对应三种题型：`test`（是非，`answer.Noul` 是一个概率）、`select`（K 选一，`over`
给候选，`answer.Choice` 是逐候选概率向量）、`measure`（打分，`scale` 给有序档位，
`answer.Score` 是逐档概率向量）。`on` 是材料（是非/打分题一个元素；关系题一对，如
`{"a":…, "b":…}`）。找不到公开示例时抄 `probes/winnow/make_fixture.py`（是非题）、
`probes/folio/fixture.json`（select）、`probes/entity-align/fixture.json`（measure）。

## 元素字段：`index` / `source` / `trail`

`sieve`、`pair` 的输出元素形如 `{item, index, trail, exit, cause}`（`sieve` 另带
`source`）。三个字段容易混淆，是本文唯一要单独强调的语义坑（阶段评估①试写程序
`refund.jpp` 撞到过，无任何报错、只有对照材料才发现）：

- **`index`** 是该元素在**这一次调用的输入列表**里的位置，不是原始材料在最初列表里
  的位置。把 `sieve` 的产物再喂给下一次 `sieve`（链式过滤）时，第二层的 `index` 是
  「第一层接受流」内部的序号，从 0 重新数起。
- **`source`** 原样保留调用者交进来的那个元素。链式过滤时，第二层元素的 `source`
  就是第一层的输出元素，一路能追到最初的 `{item, index}`。要拿原始材料在最初列表
  里的位置，用 `source.index`，不要用 `index`。
- **`trail`** 是接上来的历次出口列表，供再过滤、再配对时判断这条链路已经经过了什么。

两层以上的链路里，某个 `unsure` 元素若来自第一层，它的 `index` 用的是第一层坐标
（未随包转移到第二层坐标系）；同一个输出里可能混着两套编号，读的时候按 `source`
链路往回追，不要只看当前这一层的 `index`。

## 内置函数名单

以 `crates/jpp-core/src/interp/mod.rs::BUILTINS` 为准（`env` 里同名字会被用户绑定
遮蔽，静态检查报 `W-shadow`，允许但会提示）。

| 类别 | 名字 |
|---|---|
| 材料与状态 | `mat` `content` `state` `transform` |
| 出题 | `test`（是非） `select`（K 选一） `measure`（打分） `form`（题式模板） `fill`（按题式填题） |
| 判断与过桥 | `judge` `cut` `ask` `key_of` |
| 出口处理 | `handle` `consume` `exit_kind` `unsure` `pending` `line_source` `taint` |
| 未决责任 | `unsure_cause` `untested` `escalate` `literalize` |
| 效应与失败 | `do` `gen` `fail` `is_fail` |
| 三路过滤/配对/聚合/迭代 | `sieve` `pair` `tally` `first_k` `iterate` `outcome` |
| 有界控制 | `loop` `stop` `map` `filter` `fold` |
| 读数长处（只读校准线，不花钱、不进账本） | `allocate` `unsure_bound` `repeat`（同题重复读数取均值/中位数，旧名 `agg` 一个版本内仍可用并报 `W-deprecated`） `order`（同尺度排序） `fit`（喂已注册特征） |
| 数据与文本 | `len` `range` `append` `concat` `slice` `contains` `sum` `reverse` `keys` `has` `with` `text` `join` `print` `min` `max` `abs` `floor` |

`accepted` / `ignored` / `undecided` / `unobserved` / `stopped` **不是内置**，是
`lib/outcome.jpp` 里按契约值字段取值的库函数（`import "lib/outcome.jpp";` 后可用），
契约值形状见 `INTERFACE.md` §三·四·四。

## 真机运行

固定观察只验证程序结构，不产出真实读数；要拿真实判断，需要 `--features live` 编译并
带 `~/.typesafe-key`：

```sh
cargo build -p jpp-cli --features live --release
./target/release/jpp run examples/sieve.jpp --backend live --profiles-dir profiles --calib calib --ledger-out ledger.json
./target/release/jpp run examples/sieve.jpp --replay ledger.json   # 事后离线重放，逐字节一致
```

真机运行必须带能力画像（B73）：`--profile <文件>`，否则 `--profiles-dir <目录>/<model>.json`，再否则 `jpp` 可执行文件旁的 `profiles/`；找不到报 `E-profile-missing`。仓库附带 `profiles/jev-1.13.0.json`，价格与 δ 都从它读，它的哈希进账本头。重放不发调用，不需要画像。

真机只给读数（概率），不给出口：`cut` 要把读数变成 act/ignore/pick/at 必须有一条
**认证过的线**（J-03 禁止程序自己写线），新题第一次跑一律 `Unsure(cold)`。上线的
唯一路径是标真值再导入：

```sh
# labels.jsonl 每行：{"key": "cs-refund", "item": "c1", "p": 0.99,
#                     "label": true, "source": "computed"}
./target/release/jpp calib-import labels.jsonl --calib-out calib
```

**样本量**：默认 `--alpha 0.1 --conf-delta 0.1` 下，认证的每一侧（act 一侧、ignore
一侧）各自至少需要 **22 条零错误的已判定样本**，两侧合起来一道题大致要 **200 条**
带真值的标注才能正式上岗（这是 `jpp calib-import --help` 里写明的数字，不是估算）。
样本不够时不报错、也不瞎放行，而是停在「待核」，例如本仓库 10 条构造真值实跑的结果：

```
"gate": "待核：选线半样本不足（正例 2、负例 3，零错误也需每侧 ≥ 22；选线半 5 条、认证半 5 条）"
```

样本不足时该题的出口仍会路由（按冷键处理），但不能放行不可逆的 `do`。只有模型自己
标注、没有人工真值时，还需要同一题式的人工抽检一致率过 `--spot-check-min`（默认
0.9）才准临时上岗（`W-provisional`），否则线停在「待真值」。
