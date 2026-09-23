# 2026-09-23: reassessing the public JEV ecosystem / 重新盘点公开 JEV 生态

The language target and the use of this research are clarified in the [subsequent foundation report](2026-09-23-composable-foundation-status.md): complex composition and compositions that remain components. / 本文研究的使用方式与语言定位，见后续[地基进度说明](2026-09-23-composable-foundation-status.md)：少量底层构造支撑复杂组合，组合结果继续成为构件。

This update records research, not a runtime release. A new, separate local archive collects pinned public GitHub source snapshots, published npm/PyPI package text, Hugging Face code/model cards, official documentation and public issue/PR evidence. Downloaded projects were not installed or executed; weights were not downloaded. Search limits, transport failures, source-size exclusions and partial recoveries are recorded rather than treated as complete coverage of the internet. Third-party source snapshots and raw discussions remain outside this publication repository.

本轮是研究进展，不是运行时发布。在独立本地目录重新采集公开 GitHub 固定提交源码、npm/PyPI 发行包文本、Hugging Face 代码与模型卡、官方文档和公开 issue/PR。未安装或执行第三方项目，未下载权重。搜索上限、网络失败、体积排除与部分恢复分别记账，不将采集量称为全网覆盖；本仓库不收录整批第三方源码和原始讨论。

## What changed our understanding / 哪些证据改变了判断

- **Typed composition already exists.** [Haskell Questions](https://github.com/byteally/typesafe-sdk), [Rust derive and policy](https://github.com/AbdelStark/s1-rs), and [Rig's TypeSafe adapter](https://github.com/0xPlaygrounds/rig) demonstrate typed question collections, static or dynamic schemas and application-side policies. Independent questions sharing one state are distinct from later questions that depend on earlier answers.
- **类型化组合已经存在。** Haskell Applicative 题集、Rust derive/策略和 Rig 的静态/动态 Query 已解决部分组合问题。同一状态上的独立批题与跨请求依赖应分开表达。
- **Algorithms are not an empty field.** [`@hikae/jev-algorithms`](https://github.com/HikaruEgashira/jev-algorithms) includes sorting, top-k, adaptive binary questions, clustering and matching. Monotonicity, transitivity, sampling coverage and missing/uncertain answers remain important algorithm assumptions to preserve.
- **算法层不是空白。** 已有排序、top-k、自适应二分选问、聚类和匹配实现。J++ 的价值需要在算法前提、未决处理、覆盖说明及跨阶段组合上具体比较。
- **Caching, budgets and fallback have several existing implementations.** CI selection, SQL, email, routing and context-compression applications already combine subsets of these mechanisms. Games and browser agents make different choices: stop, pause, local takeover, fail open or request human review. A universal fallback policy would erase those differences.
- **缓存、预算和失败处理已有多种实现。** 不同产品对停局、停手、接管、继续执行与人工确认的选择不同，不应概括为共同的“三重熔断”。
- **Compatible endpoints do not imply interchangeable judgments.** Local alternatives differ in candidate/context limits, languages, startup behavior, probability definitions and calibration evidence. Preserve the resolved checkpoint and capability conditions when changing backends.
- **兼容接口不等于等价判断。** 本地实现的候选/上下文上限、语言、冷启动、概率定义及校准条件不同。换后端需要保留真实模型身份和能力条件。

## Developer needs and evidence boundaries / 需求与证据边界

Explicit requests, implemented responses and inferred needs are recorded separately. Concrete evidence includes local/offline operation, cost and latency, input coverage, privacy, response failures, misleading evaluation metrics and separating a selected action from a verified effect. Some detailed public reports come from the same external reviewer, so issue counts are not independent demand votes or a ranking of the entire market's biggest problems. Author-reported benchmarks and production claims have not been independently reproduced here.

明确请求、已有应对与推断需求分开记录。具体证据指向本地运行、成本与时延、输入覆盖、隐私、错误处理、评测含义和动作效果验证。部分详细报告来自同一外部审阅者，不能把 issue 数当独立需求票数，更不能据此给全市场痛点排名。作者自报的基准和生产效果没有在本轮复现。

## Consequences for J++ / 对 J++ 的影响

The goal remains a general composable language that can exploit inexpensive parallel semantic judgments to construct more complex programs. Existing tools should be reused where appropriate. Missing language capabilities and minimal algorithm fragments may be developed together; a complete host-language implementation is not a prerequisite. The ecosystem provides useful counterexamples and acceptance material, not proof that a new language is necessary for every task.

目标仍是构造通用、可组合的语言，充分利用便宜、并行的语义判断来写更复杂的程序。已有工具按需复用；缺少的语言能力可与最小算法片段共同建设，不以先完成一套宿主语言实现为前提。外部生态提供反例与验收材料，不证明每个任务都必须使用新语言。

Five proposed comparisons are: (1) top-k and relation clustering; (2) evidence coverage and document localization; (3) conservative CI selection under budgets; (4) compression/routing with cache and switching costs; and (5) multi-step programs with real effects. Record correctness and unresolved cases, provenance, actual calls/bytes/tokens/latency, failure exits and replay. Keep evaluation data separate from threshold fitting. These are proposed experiments, not completed results.

提出五组后续对照：top-k/关系聚类、材料覆盖与文档定位、预算下的保守 CI 选择、考虑缓存与切换成本的压缩/路由、有真实动作的多步程序。记录正确与未决、来源、实际调用/字节/token/时延、失败出口与重放，评测集与调阈值数据分离。以上均为实验设计，不是已完成结果。

This publication changes documentation only. Verification is limited to the research archive's source/manifest integrity and publication-document checks; no new J++ runtime, model-quality or customer-outcome claim is made.

本次公开同步只改文档。验证限于研究归档的文件/清单完整性及发布文档检查，没有新增 J++ 运行代码、模型质量或客户效果结论。
