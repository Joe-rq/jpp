# 模型档案测试组

把 E1–E10 的实验包装成按模型版本一键重跑的测试组，产出 `profiles/<model>.json`。
语言只写类契约（状态 + 带类型的题 → 校准概率；概率跨题不成恒等式；unsure 一等；一跳字面；算术在宿主）；
jev-1.13 的窗口、δ、K 上限、偏置、成本系数全在档案里，程序只引用档案里的量。

## 跑

```
cd 地基
.venv/bin/python -m foundation.profile.run --dry-run                       # 每项调用数与预算估计，不发请求
.venv/bin/python -m foundation.profile.run --model jev-1.13.0 --budget 0.50 # 实跑，累计超预算的项跳过并标「未测」
.venv/bin/python -m foundation.profile.run --only choice_k,score_anchor --budget 0.10
.venv/bin/python -m foundation.profile.build_from_raw --model jev-1.13.0    # 只从已有 raw/ 汇总，不发请求
```

- key 只从 `~/.typesafe-key` 读（各实验脚本自己读）。
- 模型版本通过 `foundation/profile/_boot.py` 注入（改 `Params.DEFAULTS.model_version` 与 `common.call` 的默认参数），实验脚本与 `foundation/core/` 不改。
- 各项 = 一个已有实验脚本，见 `run.py` 的 `ITEMS`；预算估计取自各实验的实跑记录。
- 真跑后 `build_from_raw` 从 `experiments/*.json` 与 `raw/*/` 汇总；实验脚本写到哪里它就读哪里，所以新模型跑完前请先把旧 `raw/e10`、`e5_results.json` 等按模型名归档（脚本目前不按模型分目录，这是下一步要改的地方）。

## 窗口常数按 state 表示分列

`window.text_slots`（[JVR] 文字槽标记）与 `window.json_slots`（JSON 具名槽）是两组常数：带主张语境 ≈1,000 token 时文字翻转 61%、JSON 7%；≈1,800 时 JSON 3.6%（E-JSON-hi）。
文字 `bound=upper 1000`（可用下界 500），JSON `bound=lower 1800`、上限未测。`run.py --only window_repr --repr text|json|both` 选臂；JSON 3k/5k/8k 剂量尚未包装。
程序默认表示 `state_representation_default = json_slots`。

## 换模型时

1. 归档旧结果：`experiments/raw/` 与 `experiments/e*_results.json` 移到 `experiments/archive/<old-model>/`。
2. `run --model <new> --budget 0.50`。约 1,900 次调用、$0.10（按 jev-1.13.0 估）。
3. 生成迁移报告：`profiles/<old>.json` 与 `profiles/<new>.json` 逐字段 diff。

## 迁移报告长什么样

```
迁移 jev-1.13.0 → jev-1.14.0（2026-xx-xx）
字段                                   旧            新          变化      引用它的程序/pass
window.json_slots….bound.token         1800 (lower)  4000 (lower) 放宽     裂变 pass、料库切片长度
window.text_slots….bound.token         1000 (upper)  1000 (upper) 不变     仅调试显示，不影响程序
delta.noul.immediate.p99               0.04          0.03        更稳      迟滞 δ（outlet）
k_limit.by_candidate_tokens.>=300      4             8           放宽      select 下沉表
position_bias.choice_first_pos_share   0.25          0.14        改善      select 的置换次数默认
cost.regression.intercept_tokens       271           190         更便宜    规划器成本模型、干跑报告
concurrency.lower_bound_ok             32            32          不变      执行器并发
calibration.ece_by_source.*            …             …           —         校准键、线是否启用代价推阈值
未测项                                 2             1           —         —
结论：3 处放宽可提高 K 与切片长度；无字段收紧；程序不改，重跑验题与校准（线按新档案重扫）。
```

「引用它的程序/pass」一列由编译器的 `jv lower` / `jv plan` 输出反查：哪些 pass 读了档案的哪个字段，在迁移时逐条列出。

## 目录

```
profile/
  run.py              调度：项、预算估计、跳过、写档案
  _boot.py            子进程注入模型版本，不改实验脚本
  build_from_raw.py   从已有结果汇总档案（$0）
  SCHEMA.md           字段定义
  profiles/           <model>.json
```

## 已知缺口

- 并发 ≥64 与状态大小的关系未包装（E10 在并发 16 + 大状态见 SSL EOF），档案里 `concurrency.upper_bound` 标未测。
- JSON 具名槽的窗口上限未测（3k/5k/8k 剂量臂未包装）。
- 候选 120–250 token 一档的 K 上限未测（`k_limit` 标未测）。
- 中文可靠性曲线要 300 条人工标注（`experiments/e_cal_labels.csv`）。
- 实验脚本的输出目录不按模型分，换模型前要手动归档（上文第 1 步）。
