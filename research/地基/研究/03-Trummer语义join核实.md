# Trummer 2510.08489 核实：是否提出"一左多右选择吸收"手段

一手来源：<https://arxiv.org/abs/2510.08489>（摘要页元数据）+ <https://arxiv.org/html/2510.08489>（全文 HTML，LaTeXML 版，2025-10-09 提交）。Immanuel Trummer，Cornell，单作者。

## 结论

**没有**。论文提出的是**对称双侧分块**的 Block Nested Loops Join（算法名 `BlockJoin`，即"块嵌套循环 join"），左右两表各自的批大小 `b1`、`b2` 都作为待优化变量联合求解，不是"左表固定 1 条、右表塞 K 条"的非对称设计；且论文**明确指出**"一边最大化、另一边最小化"（即我们设计里 b1=1、b2=K 的极端形式）这一传统数据库 BNL 里最优的策略，**在 LLM 场景下并不最优**——这是原文的一条直接结论，跟我们的设计假设方向相反，值得注意。论文全文未出现 "calibrat"、"probability"、"confidence"、"relative judg" 任何字样，没有把批处理与校准概率或相对判断做任何关联。

## 算法与原文摘录

- 算法名：**Algorithm 2, `BlockJoin`**（4.1 节），作者称之为 "a variant of the block nested loops join"。
- 核心机制（摘要原句）："The proposed algorithm integrates batches of rows from **both** input tables into a single prompt. The goal of the LLM invocation is to identify all matching row pairs in the current input."
- 提示词模板（Figure 2，`BlockPrompt`）：把 collection 1（来自 R1 的 b1 条）和 collection 2（来自 R2 的 b2 条）都编号列出，让模型输出所有满足 join 条件的 `x,y` 索引对，末尾以关键词 "Finished" 标记输出完整（防止 token 截断导致漏判无法判断）。
- 原文对左右不对称策略的明确否定（8. Related Work）："traditional block nested loop join variants... maximizing the size of one input buffer while minimizing the size of the other, a strategy that works best for block nested loops join in a traditional setting, **does not maximize performance when executing joins via language models**"——因为 LLM 每次调用要重复付费读入全部 token，不像传统 DB 可以"复用已加载的缓冲区"。

## 每 prompt 的批大小怎么定

不是拍脑袋定 K，而是把批大小当成两个连续变量联合优化：

- 约束（式 1）：`b1·s1 + b2·s2 + b1·b2·s3·σ ≤ t`（s1/s2=左右元组 token 数，s3=每对结果的输出 token 数，σ=join 谓词选择率，t=去掉固定提示词后可用的 token 预算）。
- Theorem 5.2：证明"用满 token 预算"总是更省钱（不留余量）。
- Lemma 5.5/Theorem 5.6：解出最优 `b1* = [−s1s2 + √(s1²s2² + s1s2s3σt)] / (s1s3σ)`（对 b2 对称同理）——本质是在"输入 token 花费"与"因批大乘积增大导致的预期输出 token 花费"之间做二次型权衡，解是一个 sqrt 形式的闭式解，而非经验取整。
- 6 节另给出"自适应 join"（Adaptive Join，用参数 α=4 起步倍增/回退式调整批大小）应对选择率难以预估、容易 token 溢出（`<Overflow>`）的情况。

## 成本与准确率数字

- 模拟设定：GPT-4 定价（读 3¢/千 token，写 6¢/千 token，g=2），r1=r2=5000，σ=0.001，s1=s2=30，s3=2，context 8192。
- 真实 GPT-4（gpt-4-0613，context 2000 token）三个场景（Emails/Reviews/Ads）：
  - block join 系列比 tuple nested loop 便宜"高出几个数量级"；Emails 场景生成完整结果耗时从 tuple join 的 435 秒降到 adaptive join 的 3 秒。
  - adaptive join 比（保守估计 σ=1 的）block join 最多省 30% 成本，最差多花 <3%（Reviews 场景，因为该场景选择率本来就高，保守估计接近真值）。
  - 精度：两个场景里 block join 比 tuple join F1 略降；但 Emails 场景 adaptive join 比 tuple join **F1 几乎翻倍**（作者解释：GPT-4 看到更大样本反而更容易识别"矛盾陈述"对）。
  - 对比 baseline：embedding join 成本最低但精度不稳（Emails F1=0，Ads F1=1）；LOTUS 1.1.4 token 消耗和 tuple join 相当（成本高于 block join 系列），但并行调用使其运行时更快。

## 与"校准概率/相对判断"的关系

无关联。全文围绕**token 预算约束下的批大小优化**与**输出截断/溢出检测**（"Finished" 标记），未讨论输出概率校准，也未讨论"相对判断优于绝对判断"这类认知偏差论证。

## 同作者/同主题 2025–2026 相关"batched semantic join"工作

- **SemJoin**（Purdue，Gou/Banerjee/Wang/Liu，2026-06）：直接把 Trummer 的 Adaptive Block Join (ABJ) 当作 baseline 之一，提出用 LLM agent 按数据/谓词特征在 Cluster Join（embedding 聚类剪枝）与 Classifier（谓词可归约为离散标签时）两种策略间路由，三数据集上 F1 比 ABJ 高 20–33 点。<https://arxiv.org/abs/2606.29532>
- **LLM-Enhanced Relational Operators 综述/基准 LROBench**（阿里+清华+人大，2026-03）：把包括语义 join 在内的各类 LRO（Select/Match/Impute/Cluster/Order）统一分类基准化，属于同主题但不是同一手段的延伸。<https://www.arxiv.org/pdf/2603.02537>
