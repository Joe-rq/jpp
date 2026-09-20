# J++ progress / 项目进度

Updated: 2026-09-20. This is a dated report, not an automatically updated dashboard.

## Available now / 现在可以使用

The public `0.1.0a1` snapshot contains a Python embedded implementation, judgment runtime, composition library and installed `jpp demo` command. [Download the alpha](https://github.com/Towow-ai/jpp/releases/tag/v0.1.0-alpha.1).

| Capability / 能力 | Evidence in this repository / 本仓库依据 |
|---|---|
| Questions as values / 问题可保存恢复 | `question_to_dict`, `question_from_dict`; composition tests |
| Compositions remain components / 组合继续参与组合 | Sequential, dynamic and nested component tests |
| Adaptive inquiry / 自适应提问 | Installed demo: 1,000 candidates, target 731, 10 synthetic answers |
| Candidate/check/feedback / 候选与反例反馈 | Installed demo: 3 trials, all 9 declared inputs checked |
| Local uncertainty / 局部未决 | Conditional count bounds and refinement tests |
| Independent method / 新方法接入 | `examples/independent_method.py` and its tests |
| Reproducible distribution / 可安装版本 | Wheel installation and offline demo verified outside the source tree |

The initial public commit passed 25 tests locally. Linux CI passed on Python 3.12 and 3.13. [Recorded CI run](https://github.com/Towow-ai/jpp/actions/runs/35500401184). These are mechanism and synthetic-observation tests, not a live-model accuracy evaluation.

## Work in the research workspace / 研究中的工作

### 2026-09-20: animated graph with full explanations / 图谱动画与完整说明

`jpp towow` 生成的页面保留完整十人图谱和固定人物位置，通过节点发光与沿线移动的光点讲解四个阶段；图旁和下方保留完整段落介绍。支持暂停、重播、调速、选择阶段和人物依据查看。模型记录与计算方法不变，页面不产生新的调用。The viewer keeps the complete graph and fixed node positions, animating processing nodes and particles along edges through four stages. Full prose remains beside and below the graph, with playback controls and inspectable evidence. It reuses the existing execution record without changing the discovery method or making new model calls. [运行方法 / Run it](towow-demo.zh-CN.md).

已检查窄屏图谱、阶段切换、暂停时光点冻结、恢复后推进、人物资料与未确定候选；离线案例检查通过，wheel 在源码之外安装后能生成完整页面。本次验证覆盖录制案例的展示，不是新的模型评估。The graph layout, stage selection, frozen particles while paused, resumed progression, person evidence, and unresolved candidates were checked in the browser. The offline example check passed, and a wheel installed outside the source tree generated the complete page. This verifies the recorded example's presentation, not new model quality. 下一步关注首次观看者能否结合图谱与文字理解转介和组合 / Next: check whether first-time viewers understand referrals and composition using the graph and prose together.

[实现与检查 / Implementation and checks](https://github.com/Towow-ai/jpp/pull/2).

### 2026-09-20: first application example / 首个应用案例

The current main branch adds `jpp towow`: receiver-local judgments, a referral, a candidate combination, and subsequent discovery in ten fictional participants. The shipped recording comes from real `jev-1.13.0` requests; default replay is offline. The three-stage live run took 5.215 seconds, identical-input reuse made zero new calls, and changing one participant reused 16 of 32 judgments. A separate single-run comparison of identical first-stage request bodies measured 7.847 seconds sequentially and 1.042 seconds concurrently. These are demonstration measurements, not accuracy or large-network claims. [Proposal](first-problem-towow.zh-CN.md) · [Run and inspect the example](towow-demo.zh-CN.md).

Verification / 验收：本次案例在公开基础版本的独立检出中通过全部 31 项测试；构建 wheel 后，在源码目录之外安装并执行 `jpp towow` 成功，默认路径未访问网络。All 31 tests passed in an isolated checkout of the published baseline. The built wheel was installed outside the source tree and its offline `jpp towow` command completed successfully. This evidence does not cover a concurrent kernel upgrade / 本次验收不覆盖并行施工中的内核升级。

Runtime and composition development continue in a separate research workspace. The status below comes from maintainers' implementation notes inspected on the date above; it does not mean that new code has shipped here.

运行内核这条线正在完善三种题型的接口、执行与成本分析，并根据首次使用者暴露的疑问补全文档。组合这条线已实现两种算法构造器和第三个独立方法，正在完善演示、材料说明与接入交接。研究中的新结果会经过发布仓库的安装和测试后再同步。

## Next questions / 接下来要弄清楚

| Work / 工作 | Desired outcome / 想得到的结果 |
|---|---|
| Clearer semantics / 讲清运行规则 | A new reader can construct a method without guessing question, material or result formats |
| More algorithm constructions / 更多算法构造 | Different methods reuse the same primitives; repeated glue code becomes visible |
| Real judgment backend / 真实判断后端 | A reproducible example reports evaluation data, calibration, quality and cost |
| Composition rules / 组合规则 | Examples show which transformations preserve behavior and which effects constrain them |
| Surface syntax / 表层语法 | A small notation expresses demonstrated needs and runs against shared examples |

The full language is still taking shape. We do not assign a completion percentage. The current milestone is an executable alpha; the next evidence we want is broader reuse by people who did not design it.

## Update practice / 后续更新方式

Each update should identify the user-visible change, a command or example demonstrating it, its verification, and the next design question. Keep publication results separate from ongoing research. Date each update; old test counts do not automatically cover new commits.

### 2026-09-20: bilingual progress updates / 双语进展更新

We now publish each completed, verified advance to GitHub with Chinese and English commit messages and progress notes. Follow the [commit history](https://github.com/Towow-ai/jpp/commits/main/) to see what changed and the [maintenance practice](maintaining.md) for how updates are prepared. This update adds documentation only; the runtime and existing verification results are unchanged. The next entries will link completed implementation milestones to their examples and checks.

今后每完成一项可验证的实际进展，就同步 GitHub，并提供中英双语提交说明和进度记录。关注者可以通过[提交历史](https://github.com/Towow-ai/jpp/commits/main/)了解改变，通过[维护约定](maintaining.md)了解更新方式。本次只更新文档，运行代码未变；后续进展会附上对应成果、用法和验证依据。
