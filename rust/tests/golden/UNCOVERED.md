# 金样覆盖说明（21 §三·2 步 0）

## 示例与夹具对应

`examples/` 下 21 个示例全部进了金样。`manifest.json` 共 29 个用例：21 个示例各一次固定观察运行，2 个真值通道变体（`sieve@truth`、`question-forms@truth`），1 个续跑（`lifecycle-resume`），5 个 `examples/errors/` 下预期报错的示例（金样为规范化后的 stderr）。21 列出的 10 个无同名夹具示例的核对结果：

| 示例 | 结论 | 用的夹具 |
|---|---|---|
| composition | 不含判断 | 无 |
| library-methods | 不含判断 | 无 |
| sieve-batch-new | 共用夹具 | `sieve-batch.json` |
| sieve-batch-old | 共用夹具 | `sieve-batch.json` |
| sieve-budget | 共用夹具 | `sieve.json` |
| sieve-compose | 共用夹具 | `sieve.json` |
| sieve-review | 共用夹具（含 `gen` 记录） | `sieve.json` |
| tally-budget | 共用夹具 | `tally.json` |
| topic-relevance | 缺夹具，本步补录 | `topic-relevance.json`：取自 2026-09-23 真机账本 `进展/2026-09-23/live/topic-relevance-ledger.json` 的真实读数，按「每段材料 × 三个概念」的登记顺序排成固定观察；配校准目录 `tests/golden/_calib/b24` |
| truth-pending | 缺夹具，本步补录 | `truth-pending.json`：取自真机账本 `truth-truth-pending-live-ledger.json`；配 `tests/golden/_calib/pending`（含待核的语义题式记录） |

`tests/golden/_calib/` 是 `实测/校准题式-2026-09-23/真值通道/` 下 `calib-b24` 与 `calib` 两个目录的副本，使金样不依赖研究树以外的路径。

## 登记的重放例外（manifest 的 `replay` 字段）

未登记的用例重放必须成功、新增调用 0、值与首跑相同。登记的例外重放必须按登记的样子失败，报文进金样 `replay-stderr.txt`：

- **lifecycle**：首跑停在 `ask`（pending），未答的 `ask` 不入账，重放走到 `ask` 报 `E-replay`（B35，步 3 起）。
- ~~sieve-budget、tally-budget~~（步 3 已解除）：步 0 暴露的「预算停机后重放与首跑不同」已在步 3 修正——只凭账本重放（`--replay`）是审计重现，账本记过的调用照记录计入预算，首跑在哪里停，重放就在哪里停；续跑（`--resume`）不计。两例现在按正常重放金样比对（`replay-report.json`）。

## 暂未覆盖

- ~~`ir.txt`~~（步 12a 已补）：每个用例的 IR 打印存为 `ir.txt`，由 `golden.rs` 的 `ir_is_wellformed_and_prints_stably` 比对；它只要求与自身一致、通过 `wellformed`，不参与行为判定。缺预算的示例存 IR 诊断，语法错的示例不产出。
- ~~`annot.txt`~~（步 12b 已补）：IR 打印并在节点行尾附检查器产出的标注（效应行、Φ 第一版、责任种类），检查不带档案；同一测试断言检查器转出的节点号、站点号与 `ir.txt` 的一致。只要求与自身一致，不参与行为判定。
- 真实后端：金样只跑固定观察；真机只在 21 §六·3 的步上手动跑。
- 投影缺的两项（`cut` 站点出口、层数）：见 `NORMALIZE.md`。
