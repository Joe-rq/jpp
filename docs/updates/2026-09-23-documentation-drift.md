# Documentation drift correction / 文档状态校正

> 合并时补记 / Merge-time note: PR #25 has now merged with portability and numerical
> fixes. The open-PR statements below describe the earlier documentation inspection.
> 下文 315 项仍是研究树记录；公开实现的当前范围见 [状态页](../status.md)。

2026-09-23。本轮按当前代码与已有执行证据校正稳定文档，不修改运行代码或重新裁定语言设计。由三个 GPT-5.6 Luna 分别核查研究 Rust 文档、公开入口和稳定规划，汇总后复核。没有把进行中的实验或未验证实现批量标成完成。

## 具体修正

| 旧表述或容易造成的误读 | 本轮校正 | 依据与范围 |
|---|---|---|
| 原生源码仍只有三个示例 | 研究区已有五份 `.jpp` 示例；修正 README 和文法说明 | 在自适应选问、组合、部分求解之外，已有跨文件方法库和生命周期程序 |
| `ask` 没有示例 | 生命周期程序通过 `request_test` 调用 `ask` | [生命周期源码](../../rust/examples/lifecycle.jpp)、[观察库](../../rust/lib/observations.jpp)、[CLI 行为测试](../../rust/crates/jpp-cli/tests/library_lifecycle.rs) 覆盖等待、回应、恢复与重放；固定回应不等于真实人机界面 |
| 旧规划中的“未迁移”“待接线”都仍是当前待办 | 给研究计划、状态页和旧总规划添加日期与当前入口，保留历史正文 | 后续方法组合、未决续接、执行优化及档案接线已有进展；具体范围见[现状地图](../design-and-delivery-map.zh-CN.md) |
| Rust 还待选择，或最初首包仍待启动 | 正式实现已经选定 Rust，独立源码首包已经交付；历史通知不再作当前等待依据 | 保留施工决定，指向当前地图和[下一段任务](../implementation-handoff-2026-09-23.zh-CN.md) |
| OCaml 尚未探索，或该探索证明 OCaml 没有价值 | 补记已发生的三个类型系统问题纸面分析 | 当时未编译 OCaml 程序，没有采用新正式组件；阶段性意见不等于实测优劣结论 |

校准记录的写回与读回，只说明资产往返已接通。运行观察尚缺真值标签、认证仍有宿主侧边界，因此不把它写成开发者已经能走完整个反馈认证流程。

本轮 Luna 在本地研究 Rust 工作区运行 `cargo test --workspace --quiet`；汇总原始输出为 **315 通过、0 失败、3 忽略**（两项集成测试、一项文档示例）。总控复核了完整输出并保存在研究目录 `进展/2026-09-23/verification/docs-drift-cargo-test.log`。这不是公开旧快照的重跑，也不是实时模型评测。

## 哪些没有改

公开 README、开发指南和路线图的主要 Rust 定位已经更新，核查后保留。历史审查意见保留其发生时的含义，不因后来有修复就删除，也不在缺少逐项证据时统改为“审核完成”。实验提案、进行中的结果和没有运行依据的能力维持原有状态。

本次核对时 Rust PR #25 仍未合并。研究区后续功能和测试数不作为当前公开 `rust/` 的发行承诺；本轮公开同步只涉及文档。读者应区分研究现状、待合并代码、已发布快照和真实模型效果。

## English summary

Three GPT-5.6 Luna agents checked stable research-runtime documents, public entry points and planning documents against current code and available execution evidence. Corrections cover the five source examples, the existing indirect `ask` lifecycle example, obsolete implementation backlog statements and the bounded OCaml paper exploration. Historical records and unfinished experiments remain intact.

The lifecycle source and CLI tests demonstrate fixed-response suspension, resume and replay, not a delivered interactive UI. Calibration asset round trips do not establish a complete truth-label and certification workflow. Public Rust guidance was already largely current; research-only capabilities were not promoted to release claims. Runtime PR #25 remained open at inspection. This update changes documentation only.

The local research workspace test run completed with 315 passing tests, zero failures and three ignored targets (two integration tests and one documentation example). Its raw output was reviewed and retained locally. This result does not validate the older public Rust snapshot or live-model performance.
