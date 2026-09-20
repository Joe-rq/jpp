# GATES · 逐阶段验收关口

每一行是施工单 §8 的一个验收阶段。"验收者"是执行该阶段真实测试、写 PASS/FAIL 判词的
角色；施工单 §0 A12 的处置是"今晚由独立验收 agent 签字，明早 Nature 补签"——**这张表
里的"签字"到今晚为止都只是独立验收 agent 的**，Nature 的签字栏全部是空的，等明早本人
过一遍 `02-明早你亲手验什么.md` 后自己填。

| Stage | 范围 | 时间 | 验收者 | PASS | FAIL | 详情 | Nature 签字 |
|---|---|---|---|---:|---:|---|---|
| S1 | 单元与桌面 | 2026-09-20 约 05:41 | 独立验收 agent（`acceptance-verifier`，不继承建造者上下文） | 5 | 0 | `foundation/reports/stage-1.md` | 待 Nature 明早补签 |
| S2 | 并排、账本、确定性 | 2026-09-20 约 05:45 | 同上 | 6 | 0 | `foundation/reports/stage-2.md` | 待 Nature 明早补签 |
| S3 | 手、接力、保险 | 2026-09-20 约 05:45 | 同上 | 7 | 0 | `foundation/reports/stage-3.md`（S3.2 历史上曾 FAIL，已由建造者修复、经本轮独立复验为 PASS——判定变更过程原样留在报告里，不是把 FAIL 抹掉） | 待 Nature 明早补签 |
| S4 | 裁决、上交、校准 | 2026-09-20 最终 08:31（另两轮独立复核 07:47、08:14–08:27） | 独立验收 agent（`acceptance-verifier-2`，与 S1-S3 不同会话；本阶段被独立重新执行了三遍，互不共享上下文） | 6 | 0 | `foundation/reports/stage-4.md` | 待 Nature 明早补签 |
| S5 | 包 | 2026-09-20 最终 08:31（同一批三轮独立复核） | 同上 | 4 | 1 | `foundation/reports/stage-5.md`——S5.2 前半句"replay 模式交出物 id 集合相同"，见下方专门一节 | **Nature 已签（2026-09-20）：认可取代判据，id 定义不改** |
| S6 | 检查器（不阻塞验收） | 2026-09-20 08:54 | **收尾者自验，不是独立验收 agent**——施工单 §8 开头要求的"独立验收 agent 执行"这一条，S6 没有做到，原因与后果见下方专门一节 | 1 | 0 | `foundation/reports/stage-6.md` | 待 Nature 明早补签，且待 Nature 决定 S6 要不要另开一轮真正独立的验收 |
| S6 独立复核 | 检查器四项定义逐项核验（不看 test_s6.py 自造四份埋错 defs） | 2026-09-20 | 独立验收 agent `s6verify`（不继承建造者上下文，未改任何代码） | 0 | 1 | `foundation/reports/stage-6-independent.md`——悬空产出/无源之水/同层同优先级冲突 3 项 PASS；"越权手"1 项 FAIL：`checker.py` 实现的判据是"watch_kinds 留空+会取代原物"，不是语言规范 §5.2 / 00-总册第345行定义的"draft（没上岗）单元配 mark 以外的手"，`check` 命令不读任何运行态、结构上查不了这一条；`test_s6.py` 的"越权手"用例测的也是 checker 自己那条判据，未对照语言规范，DECISIONS.md 里没有记这处偏离 | 待 Nature 明早过目，决定要不要修 checker.py 或改判据文字 |
| S6 修复后 | 越权手规则按规范原文重做 | 2026-09-20 10:xx | 总控 main 自修自测（非独立） | 5 | 0 | `foundation/reports/stage-6-independent.md` 末节；复核者的 FAIL 判定原样保留 | 待 Nature 明早补签，可再派独立复核（零花费） |
| S6 独立复核 r2 | 对修复后的 checker.py 四项定义逐项核验（不看 test_s6.py 与上一轮用例，另造 4 份埋错 + 1 份对照） | 2026-09-20 | 独立验收 agent（第二轮，不继承建造者/上一轮复核者上下文，未改任何代码） | 4 | 0 | `foundation/reports/stage-6-independent-r2.md`——四项定义逐一被正确报出，越权手对照组无假阳性；a_notes/mini_todo 仍 0 error；`test_s6.py` 诚实分离"不限种类取代"与"越权手"，未放水；另指出 `DECISIONS.md`「验题够但缝不足」这条静态盲区注明未覆盖第二种同样查不到的情形（曾上岗后被 suspended），建议补注，不影响本轮 PASS | 待 Nature 明早过目；上一轮 S6 独立复核的 FAIL 判定原样保留，不因本轮 PASS 而改写 |

**合计（S1–S6 共 30 个验收子项，另加本次 S6 独立复核 1 项）：29 PASS / 2 FAIL。**
（"S6 修复后"5 PASS 与"S6 独立复核 r2"4 PASS 均未计入这个合计——沿用"S6 修复后"
那行已经立下的先例：这两行是对同一处 S6.1 越权手判据的修复与复核，不是新开的
验收子项，计入会让"30 个子项"这个分母失去意义。上一轮"S6 独立复核"那 1 项 FAIL
仍按原样计入合计，不因 r2 判 PASS 而从分子分母里减掉。）

---

## 唯一的 FAIL：S5.2 前半句

施工单 §8 S5.2 前半句「放进一个外层包再跑同样输入，交出物 id 集合相同」在 `Item.id
= H(kind, body, about, supersedes, made_by, scope)` 的定义下结构性为假——`scope` 进
id 正是 S5.1 隔离成立的前提，两者不能同时要。这不是一次性的测试失败，是**设计层面
两条要求互斥**：`design-issues.md` DI-1 与 `DECISIONS.md`「F2-1」到「F2-3」记录了完整
判断过程（公开取代判据，`Item.id` 本身一个字没动）、九个独立真实数据点（不同规模、
不同会话，交集恒为 0）、以及三轮独立验收对同一条 FAIL 的独立复核结论一致。

**这条 FAIL 按施工单工作约定 3"FAIL 原样上报"终局归档**（`DECISIONS.md`「F2-3」）：
往后每一次验收都会再报一次，因为判据按字面确实为假；判定它"要不要再动手"只需核对
`Item.id` 定义是否还是上面那条公式，以及承接它的 S5.2b（`(unit, view_fp)` 出口相同）
/ S5.2c（`(kind, body)` 多重集合相同）是否仍是 PASS——两条替代判据本轮全部 PASS，
细节见 `stage-5.md`。**这条 FAIL 需要 Nature 明早过目一次，决定是否认可"取代判据、
不改 `Item.id`"这个设计方向；施工单第 265 行 S5.2 判据文字本身已由总控 main 于
08:41:10 改写以反映这个取代——那次改写的授权链条也记在 `DECISIONS.md` 同一节。**

## S6 为什么不是独立验收

施工单 §8 开头："验收（独立验收 agent 执行，真实调用跑一遍 + 断网 replay 一遍；报告
只有 PASS/FAIL）"。S1–S5 五个阶段都由不继承建造者上下文的独立会话做到了这一条；S6
没有——`core/checker.py`、`foundation/acceptance/test_s6.py`、`stage-6.md` 是同一个
收尾者一并写的，等于建造者兼验收者。原因：施工单 §0 把最小检查器列为"部分接受……
作为最后一个阶段，不阻塞验收"，红队原案是"推迟"；今晚的处置里没有专门再派一个独立
验收 agent 去复核这四项静态检查（跟 S1–S5 不同，S1–S5 每一项施工单都点名要独立验收）。
`stage-6.md` 与 `DECISIONS.md`「收尾" 一节都原样写了这一点，不冒充独立验收。**S6 的
PASS 判定目前只有收尾者一人核实过，Nature 明早如果要让它跟其他五个阶段同等可信，
需要另找一个独立会话按 `foundation/acceptance/test_s6.py` 的方法重新核一遍——命令
是 `.venv/bin/python -m pytest foundation/acceptance/test_s6.py -v`，零花费，几秒钟。**

---

## 不在这张表里的东西

- **第零步实验**（`foundation/experiments/`）：不是施工单 §8 的验收子项，是给参数
  定数的前置实测，花费 $0.0823，结论见 `foundation/experiments/前提结论.md`；本表
  不把它计成 PASS/FAIL 的一项，也没有人去"验收"它，只有 DECISIONS.md 记了怎么把
  它的结论落进 `core/params.py`。
- **契约第一版**（S4.6 的产物，`foundation/reports/contract-sample.md`）：S4.6 本身
  已经计入 S4 那一行的 PASS，但契约清单"人标"之后重算两条线这件事，今晚只出到
  "读数与抽样"这一步，没有过关线、没有判定，明天标注完之后也不会自动产生一个新的
  PASS/FAIL——按红队 A7 的处置，这本来就不是一次性的验收动作。
