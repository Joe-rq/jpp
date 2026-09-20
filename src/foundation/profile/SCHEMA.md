# 模型档案 · 字段定义（profiles/<model>.json）

档案是**数据**，不是语言定义：语言只写类契约，程序只引用档案里的量。换模型 = 重跑测试组生成新档案 + 迁移报告。
所有数值字段都带 `n`（样本量）与来源实验；比例类字段带 `ci95`（Wilson 区间）；未测字段写成 `{"value": "未测", "by": "<run.py 的项>"}`。
每个常数标 `bound`：`lower` = 只知道到这里还没坏，`upper` = 到这里已经坏，`point` = 已定位。

| 字段 | 含义 | 单位 / 形状 | 来源项 |
|---|---|---|---|
| `model_version`, `date`, `sources` | 模型别名、生成日期、引用实验 | — | — |
| `window.text_slots.claim_bearing_ctx` | **文字槽标记**（[JVR]）渲染下，带主张语境 ~100/~1000/~1800 token 的读数翻转率与均值漂移；`bound=upper 1000`（1,000 处翻转 61%），`usable_lower=500` | 比例；token | window_repr --repr text |
| `window.json_slots.claim_bearing_ctx` | **JSON 具名槽**渲染下同上；`bound=lower 1800`（1,800 处翻转 3.6%），`upper` 未测（3k/5k/8k） | 比例；token | window_repr --repr json |
| `state_representation_default` | 程序默认 state 表示（当前 json_slots；账本键建在规范化 JSON 上） | — | E-JSON |
| `window.noul_claim_bearing` | （文字渲染）带主张无关填充对 noul 读数的剂量-反应：`median_drift`、`max_drift`、`flips_beyond_noise` 按填充 token 分档；`bound` | 概率差；token | window |
| `window.noul_neutral` | （文字渲染）中性填充同上 | 同上 | window |
| `window.choice` | 按候选长度分档：一致率仍可接受的最大状态 token 与对应 K | token；K | choice_k |
| `window.score` | 锚数最多、重跑一致率仍可接受的状态 token | token | score_anchor |
| `delta.<题型>` | 同题同状态重跑差：`immediate`（立即）与 `after_gap`（隔时）各给 `n/p95/p99/max`；题型 = noul / choice_prob_chosen / choice_confidence / score | 概率差 | delta |
| `flip_rate.choice_argmax_rerun` | 重跑后 choice 选中项改变的比例，`rate/n/ci95` | 比例 | delta |
| `flip_rate.noul_outlet_under_filler` | 各填充档下过线翻转数 | 计数 | window |
| `batch_invariance.noul` | 单独 vs 同批 10/50/200 题的读数均值差与最大差；choice 标签是否变 | 概率差 | batch |
| `batch_invariance.score` | 跨批众数档一致率、带同伴 vs 单独、配置内重跑；`rule` | 比例 | score_anchor |
| `batch_invariance.choice_same_call_perm_crosstalk` | 同一次调用里放两道置换 choice 题时，第二道的选中项/概率是否被第一道带偏（jv 下沉 pass 把两个置换融合进同一调用的前提） | 比例 | **未测**：E-PERM-SAME-CALL（预注册草案） |
| `cost.price_usd_per_input_token`, `cost.output_billed` | 单价与是否计输出 | $/token | 文档 |
| `cost.tokens_by_n_questions_300tok_state` | 300 token 状态挂 1/10/50/200 题的输入 token | token | cost |
| `cost.tokens_per_question` | 每多一题的边际 token | token | cost |
| `cost.latency_median_by_n` | 同上的时延中位数 | 秒 | cost |
| `cost.regression` | token ≈ intercept + state_char_coef×状态字符 + question_char_coef×题字符，附 `n_calls` | token/字符 | concurrency（E8 段） |
| `concurrency.lower_bound_ok` | 无限流地跑过的最大并发 | 并发数 | concurrency |
| `concurrency.throughput_calls_per_s` | 各并发下的调用吞吐 | 次/秒 | concurrency |
| `concurrency.latency_s` | p50/p95/max | 秒 | concurrency |
| `concurrency.n429/n529` | 限流计数 | 计数 | concurrency |
| `concurrency.upper_bound` | 开始限流或断连的并发（与状态大小相关） | 并发数 | concurrency（≥64 未包装） |
| `k_limit.by_candidate_tokens.<档>` | 按候选 token 分档的推荐 K 上限与依据 | K | choice_k |
| `k_limit.hard_max_options` | 接口硬上限 | 255 | 文档 |
| `position_bias.choice_*` | choice 置换后落首位比例、各位置分布、E10 各集众数占比（`runs_per_set` 次） | 比例 | choice_k（+ raw/e9* 引用） |
| `position_bias.noul_position_effect_*` | noul 换位置的读数极差 | 概率差 | choice_k |
| `position_bias.uniform_reference` | 均匀参照（1/K） | 比例 | — |
| `anchors.*` | score 锚数 0/3/5/10 的期望档漂移、相邻配置漂移、argmax 一致率、配置内重跑；`convergence_point`、`default_per_level` | 档；比例 | score_anchor |
| `calibration.ece_by_source` | 各来源的 `n/ece/brier/auc`；`rule`（校准是字面化程度的函数） | — | E-CAL-0（已有真值） |
| `calibration.zh_reliability_curve` | 中文 300 条标注的可靠性曲线 | — | E-CAL（待标注） |
| `lines.safety_default` | 冷校准键（无标注记录）时 handler 库给临时出口用的保守线 `hi/lo`；**非实测**（`n=0`，沿用 v0 常数 0.75/0.25），由 E-CAL 按域扫线后定 | 概率 | E-CAL（待标注） |
| `language.*` | 中英题面验题缝的均值与规则 | 概率差 | E3 |
| `modalities_accepted` | 模型接受的输入模态（渲染函数按此决定是否需要变文字） | 列表 | 文档 |
| `question_types` | 支持的题型 | 列表 | 文档 |
| `run_status`, `run_budget_usd` | 本次各项跑/跳/失败与预算 | — | run.py |
| `field_stats` | 已填叶字段数、未测项数 | — | build_from_raw |

程序引用档案的约定：只引 `bound=lower/point` 的常数；引 `upper` 的要留余量；引「未测」的编译期报错。
