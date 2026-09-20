# 从关系排序到可续接的发现 / From ranking to composable discovery

2026-09-20。本轮沿用[发现路线](towow-discovery-roadmap.zh-CN.md)，先完成固定候选名额的实际对照，再把应用从固定故事调整为可传入意图、判断问题和组合规则的接口。J++ 正式内核继续沿 Rust 路线建设；这里复用已发布的 Python 行为参考与组合运行时，不另建一套语言内核。

## 要解决的问题

通爻发现前端原任务书 `发现前端簇/SPEC.txt` 的终点是：互不认识的主体持有丰富且持续变化的局部上下文；一份模糊语料应有机会到达发送方不知道的接收者，由接收方结合自己的世界提出可能有价值的关系。关系候选可以是双方、多方或转介；中间候选还能继续参与发现。原文第 56–66、138–162 行明确要求局部算子可自行构成，协议不预先穷举全部语义维度。

因此，找回旧关系、按职业分配名额、预先写好“音频＋视觉”的脚本，分别只能检验这个问题的一部分。实际需要积累的是调用者可以更换问题、提名方法和组合方法，并把输出作为下一次输入的能力。接收方自行编译算子、渐进披露、网络传播和大规模动态路由仍是后续建设对象。

The reusable target is receiver-context discovery: an input can be a vague signal or a previous nomination; local reasoning can propose a relationship the sender did not name. The examples are inputs to a replaceable application interface, not a protocol-wide list of semantic dimensions. This iteration does not deliver a distributed network or autonomous operator synthesis.

## 固定二十个名额：来源配额没有改善最终结果

使用同一批 325 份已公开处理的职业档案、同一个判断问题、同一个 J++ 相对排序操作。保留 RRF 前十六，取原排序中前四个不同资料来源的候选；去重后按原排序补齐二十。候选生成不读旧关系标签。不同来源是探索对照，不是互补能力的定义。

| 指标 | 原 RRF20 → J++ | 含来源探索 → J++ |
|---|---:|---:|
| 候选池覆盖旧关系 | 830 / 963 | 797 / 963 |
| 候选池覆盖跨来源种子 | 0 / 24 | 7 / 24 |
| 最终前十命中旧关系 | 697 / 963 | 681 / 963 |
| 最终前十命中跨来源种子 | 0 / 24 | 0 / 24 |
| 有向前十命中 | 1185 / 1926 | 1166 / 1926 |

最终相对旧 J++ 排序新增 6 条、失去 22 条，净减 16。保留这个结果用于观察入口取舍，**不替换默认方法**。24 条跨来源标签是人工种子关联，不是已证实的合作。没有完整负例标注，本表不是准确率。

实际执行 6,500 个问题，复用 4,328 个精确的状态／题目回答；2,172 个问题经 1,096 个新增后端请求完成。部分历史重复文本对应冲突回答，不能安全复用的项也重新观察。因此这是保留同一题面和方法的开发对照，不是冻结全部模型读数的纯候选机制消融。执行 38.719 秒，新增估算费用 $0.039513；相同输入离线重放新增调用 0，候选、等级、排序和比较结果一致。

The source-diversity experiment regressed from 697 to 681 known proxy edges at top ten, with six gains and twenty-two losses. Seven curated cross-source seeds entered the candidate pool, but none survived into the final ten. The original default is retained. This is a development comparison, not evidence of unseen cooperation or a causal claim based on identical frozen observations.

查看[实际图谱](https://towow-ai.github.io/jpp/demos/towow/real/)、[结果](demos/towow/real/exploration.json)和[新增请求录制](demos/towow/real/exploration-recording.jsonl.gz)。

### 复现入口对照

先将原录制 `docs/demos/towow/real/recording.jsonl.gz` 与新增录制依次解压到同一个日志文件；这两份文件都已随仓库公开。然后执行：

```sh
gzip -dc docs/demos/towow/real/recording.jsonl.gz \
  docs/demos/towow/real/exploration-recording.jsonl.gz > /tmp/towow-explore-recording.jsonl
python -m jpp.towow_explore \
  --data docs/demos/towow/real/data.json \
  --results docs/demos/towow/real/results.json \
  --journal /tmp/towow-explore-recording.jsonl \
  --out /tmp/towow-explore-replay
```

默认只回放。`--proposal` 只检查候选入口。只有显式 `--live` 才调用 JEV；`--budget-total` 是包含该日志历史费用的累计上限。

## 可替换的发现程序

[`run_discovery`](../src/jpp/towow_teams.py) 接收意图、节点资料池、问题配置、可替换的路由函数与组合生成函数。默认执行过程为：路由少量候选 → J++ 判断各自贡献并相对排序 → 形成有数量上限的多成员提议 → 再用一个问题判断提议。该应用实例每轮最多使用 20 个判断；这个限制不是 J++ 语言本身的限制。

接口没有固定的职业或能力注册表。配置中的角色名、问题、刻度、提名词和组合问题可以更换；调用方也可以提供自己的路由与组合函数。当前默认路由仍扫描本地资料池做词面检索，组合采用有上限的候选组合枚举，不宣称计算量与网络规模无关。

`nomination_as_input(proposal)` 保留成员及原始上下文、贡献分工、触发问题与暂定判断、先前意图和组合父节点。将它或原始提议作为下一次调用的 `seed`，原成员会进入新的提议，新的判断可以补入其他成员。组合不是已完成合作，也不把先前模型判断升级为事实。

```python
first = run_discovery(plan, client, ledger_path, people=nodes)
seed = nomination_as_input(first["proposals"][0])
next_step = run_discovery(extension_plan, client, next_ledger_path,
                          people=nodes, seed=seed)
```

这一接口是 J++ 组合运行时上的应用组件。当前 JSON 配置是应用配置文件，不是独立 `.jpp` 源码；Rust 内核建设和这一案例的行为对照继续分开说明。

### 同一实现的三个实际运行

全部使用已经公开的 216 份合成资料。人物的旧预期命中标签不进入候选生成或模型问题。两个不同任务和一次续接只更换配置与输入，运行代码相同。

| 输入 | 问题数 | 输出 | 首次执行耗时 | 新增 JEV 估算费用 |
|---|---:|---|---:|---:|
| 想做一个把声音变成画面的东西 | 20 | 4 个两人提议 | 3.515 秒 | $0.000679 |
| 帮我部署一个能扛大流量的后端 | 20 | 4 个两人提议 | 3.265 秒 | $0.000722 |
| 上一步首个实际提议 + 安全检查方向 | 12 | 4 个三人提议 | 2.139 秒 | $0.000787 |

例如，第二次运行先得到“sudo_rm（后端实现）＋容器游牧民（部署维护）”这一暂定候选。第三次调用保留二者，再分别形成包含“0day”“数据包嗅探犬”等安全检查候选的三人提议。名字均来自旧合成样本。声音与视觉的组合判断仍处于未决状态；安全续接的四个组合得到暂定等级 2，但并未确认人员意愿、时间或真实合作结果。页面同时展示未决与暂定等级。

These three live runs use the same implementation with different plans, followed by one seeded continuation. They demonstrate replaceable inputs and questions, n-member composition, and output-as-input behavior. They are synthetic application examples, not an evaluation of real collaboration quality or general discovery accuracy.

三项结果均从构建的 wheel 在源码目录外安装后，用公开的 52 条录制完整重放；输入、候选和提议与真实运行一致。相同输入再次运行，运行时新增请求为 0，分别复用 20、20、12 个判断。关掉组合步骤，前两个任务各保留 16 个单人判断、输出 0 个组合。这里的消融说明组合步骤的接口作用，不证明默认组合策略优于其他策略。

另一个定向机制检查改变单个节点资料，只重算该节点的贡献判断及依赖它的组合，其他判断复用。该检查使用确定性测试客户端；尚未进行持续更新流或网络路由的真实模型性能对比。

查看[可交互图谱与三个任务](https://towow-ai.github.io/jpp/demos/towow/teams/)。页面是实际结果回放；Agent 或其他程序使用命令行或上述调用接口运行。

### 安装与复现

在仓库根目录安装当前源码（Python 3.12+），解压本例录制后运行：

```sh
python -m pip install -e .
gzip -dc docs/demos/towow/teams/recording.jsonl.gz > /tmp/towow-teams-recording.jsonl
python -m jpp.towow_teams --plan examples/towow/audio-visual.json \
  --journal /tmp/towow-teams-recording.jsonl --out /tmp/towow-audio/result.json
python -m jpp.towow_teams --plan examples/towow/backend.json \
  --journal /tmp/towow-teams-recording.jsonl --out /tmp/towow-backend/result.json
python -m jpp.towow_teams --plan examples/towow/security-extension.json \
  --seed docs/demos/towow/teams/backend-seed.json \
  --journal /tmp/towow-teams-recording.jsonl --out /tmp/towow-security/result.json
```

以上不调用模型。`--people` 可替换节点资料池，`--plan` 替换发现计划，`--seed` 接续之前的候选，`--no-combinations` 关闭组合阶段。输入改变后，旧录制可能无法覆盖新的状态；有需要时由调用方显式启用 `--live` 并设置累计费用上限。

## 本轮结论和继续建设的位置

来源配额未改善最终关系恢复，因此没有把它当成通用解法。可复用组件已经支持换任务、换问题、组合再输入，以及重复计算的复用；具体示例配置可以独立改写。这是通爻发现问题的一段可运行实现。

接下来的核心是把调用方提供的问题和路由接到接收方自己的局部算子，并让它能够提出新方向、转介或待披露的信息，而不要求发送方先列完所有贡献角色。再接已有 Discovery Front 的传播与版本更新接口，才进入动态图上的连续发现。新的算子、转介和传播都应继续使用这个可调用入口，页面只负责展示。

费用：本轮来源探索 $0.03951259，三个合成应用合计 $0.00218762，新增合计约 $0.04170021。连同此前本任务 JEV 调用累计约 $1.53090338，低于已批准 $5。这里按录制 token 的后端价格估算，不是账单，也不包含 Codex 对话或开发 Agent 的用量。

验证：新组件与录制复现通过；本地全量运行 543 项通过，唯一子进程导入失败在补上绝对 `PYTHONPATH` 后单独复查通过（共 544 项）。没有为这一环境问题修改语言内核。wheel 安装与三项离线案例在源码目录外验证。
