# Complex composition: foundation and progress / 复杂组合的地基与进度

2026-09-23. Documentation and research-workspace verification only; this update does not sync a newer Rust runtime into this repository.

## Position / 定位

J++ aims to provide a small set of underlying constructs from which many complex algorithms can be built. A composition should remain a component that participates in a larger composition. The goal includes programs that have not already been anticipated as library functions. Existing ecosystem algorithms are material for discovering common structure and testing this generative capacity.

J++ 要提供少量底层构造，让多种复杂算法从中形成，并让组合结果继续成为下一层构件。目标包括构造此前没有预置为库函数的程序。外部算法用于观察共同结构和检验这种生成能力，不能把“别人尚无组合能力”当作语言定位，也不能把语言缩成可靠性工具。预算、追踪、校准和失败处理服务于复杂组合。

The earlier ecosystem reassessment remains a source review. Its proposed audit-first ordering is not the current language roadmap. The question is whether the same constructs express several different algorithms and naturally extend to an unfamiliar program, reducing the complete implementation burden rather than hiding it behind a task-specific wrapper.

此前生态复盘继续作为来源报告；其中优先做审计边界的排序不作为当前语言路线。建设要检验同一构造能否跨算法复用、能否自然表达陌生程序，并减少包括配套代码在内的完整实现负担。

## Available mechanisms / 已有机制

The local research workspace at `520fef2` was checked again with `cargo test --workspace --offline`: **315 passed, 0 failed, 3 ignored**. The ignored cases are two live-model tests and one documentation example. Four source programs and ledger replay were also run directly without model API requests.

本地研究工作区 `520fef2` 本次离线复跑：**315 通过、0 失败、3 忽略**（两条真机测试和一条文档例子）。另外直接运行四个源码程序及账本重放，未调用模型。

| Program / 程序 | Current observed effect / 本次效果 | Meaning / 意义 |
|---|---|---|
| Nested methods / 方法再组合 | 20 → 43 | Returned methods compose again / 返回的方法继续组合 |
| Source library / 跨文件方法库 | `[4,6,42]`, direct result 42 | Store, import and invoke composed methods / 组合方法可保存、导入与调用 |
| Adaptive questions / 自适应选问 | 1,000 candidates → 731 with 10 fixed observations | Later questions depend on earlier answers / 后题由前答产生 |
| Partial continuation / 部分结果续接 | A+B at cost 9 → C at cost 2; six observations, A/B/C checked once | Usable results retain unfinished work and another strategy / 可用结果保留未完成部分与后续策略 |
| Replay / 重放 | Same value; zero new judgments and local checks | Recorded effects are reused / 复用已记录效应 |

These establish bounded execution mechanisms, not live-model accuracy, monetary savings or broad algorithmic compression. The suite also verifies same-state question fusion and a specific loop shape whose execution layers fall from three to one; this is not a universal scheduling claim.

这些结果证明有界执行机制，不证明模型准确率、实际费用节省或广泛算法压缩率。测试也覆盖同状态多题融合，以及特定循环结构从三层压成一层；不外推为完整调度器或普遍性能收益。

## Work distribution and remaining scope / 工作分布与剩余范围

Implementation has mainly followed two lines: the Rust core, and the source frontend/CLI with runnable programs. Model/calibration research, Python composition/discovery applications and ecosystem research support them. Later work on September 21 concentrated on calibration records, provenance, diagnostics and integration. September 22–23 collection and reporting are not new language features.

工程主要是两条线：Rust 内核，以及源码前端/CLI/可运行程序；模型校准研究、Python 组合与发现应用、外部调查作为支撑。9 月 21 日后半段较集中于校准记录、来源、诊断与接线，9 月 22–23 日调查不算新增语言功能。

The runnable foundation exists; a complete first body does not. Important gaps include reusable material-set constructs and algorithm skeletons, remaining planning passes, a live CLI backend, a usable truth/commissioning path, and integration of research Rust with the older public snapshot. The small source library does not yet demonstrate that a few constructs cover a broad family of unfamiliar algorithms.

地基已可运行，第一版主体尚未完成。主要缺口包括可复用材料集合构造与算法骨架、其余规划能力、CLI 真实后端、真值/认证使用路径，以及研究 Rust 与公开旧快照的整合。现有小型源码库尚未证明少量构造能覆盖广泛的陌生算法。

The early five-operator design and later IR/effect contract use different layering: the latter places combinators, aggregators and skeletons in libraries. Boolean `filter` and reading-level `agg` do not supply the earlier three-stream material filtering and set aggregation. This is a missing capability mapping, not sufficient evidence that those existing functions are incorrect. No language specification changes in this report.

早期五算子图与后来的 IR/效应契约分层不同，后者将组合子、聚合子、骨架放在库层。当前布尔 `filter` 与读数 `agg` 不能充当早期的三路材料过滤和集合聚合；这是能力映射尚未补齐，不能仅凭同名就断定现有函数错误。本报告不修改规范。

## Reference organization / 参考资料整理

The local portal links 173 research/feedback documents across prior-work research, the earlier ecosystem sample, the fresh review and needs evidence. It separately indexes 3,361 machine-generated repository cards and the downloaded source archive. Historical claims and later corrections remain traceable. Neither count measures independent user demand or human-reviewed projects. Third-party sources and raw private workspace materials are not included in this documentation update.

本地统一入口已链接173份研究/反馈材料，涵盖先行工作、早期采样、新一轮分析和需求证据；3,361份机器逐仓卡片及源码归档另列。历史结论和后续订正保留可追溯关系，数量不代表独立需求票数或人工深读项目数。本次仅同步说明，不上传第三方源码或工作区原始私人材料。
