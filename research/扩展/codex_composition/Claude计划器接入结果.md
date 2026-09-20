# Claude 计划器接入结果：`__jv_structure__` 识别端

2026-09-20，Claude Code。对应 `Claude的评审回复.md` §「三处要改」第 3 条、`本轮Claude计划器任务.txt`。
只改了两处：`foundation/jv/plan.py`（识别端）、新增 `foundation/tests/test_jv_structure.py`（13 个测试）。
未碰 Runtime / ir.py / 组合库 / 组合库测试 / 规范文档。

## 接口（Codex 挂载端对齐用）

程序包装前 entry 与包装后函数均可挂：

```python
__jv_structure__ = {"version": 1, "root": node}
```

`node` 是 `Mapping`（`dict` 或 `MappingProxyType` 均可），公共字段：
`operation`（`leaf`/`identity`/`then`/`branch`/`opaque`）、`name`、`input`、`output`、
`effects`（tuple[str]）、`effects_contract`（`declared`/`unknown`/`structural`）、`children`（tuple[node]）。

各 operation 的成本合成规则（`foundation/jv/plan.py:196-329`，新增的
`_cost_of_node` / `_cost_of_leaf` / `_cost_of_then` / `_cost_of_branch` / `_cost_of_opaque`）：

- **leaf**：额外字段 `function`（真实 callable）。内部直接复用 `jv.plan(function)` 做原生 AST 估计，
  既有 warning（含 `W-nosource`）原样并入。之后对 `effects` 里声明的 `judge`/`gen`/`do`/`ask`
  逐个核对对应 Sym 字段（`calls`/`gen_calls`/`do_calls`/`asks`）：AST 估计为 0（或取不到源码）时，
  把该字段换成未知符号 `Sym.var("leaf:<name>:<effect>")` 并告警 `W-opaque`，不静默当 0。
  纯声明且 AST 能看见的简单 Python 叶，保留原估计不动。
- **identity**：零成本（全字段 `Sym(0)`）。
- **then**：`children` 必须恰 2 个，否则 `JvError`；左右两段成本逐字段相加。
- **branch**：`children` 必须恰 3 个（predicate/yes/no），否则 `JvError`；
  成本 = predicate 成本 + 两臂保守上界。上界规则（`_sym_upper_bound`）：两值都数值 → 取 `max`；
  符号表达式完全相同 → 直接复用；其余（不可比较）→ 取和值当上界，并告警 `W-branch-bound`。
- **opaque**：用于未贯通的 `bind`/`product`/`iterate`。额外字段 `source_operation`、`parameters`
  （可含 `factory_effects`/`continuation_effects`）、可选 `function`（不做完整分析）。
  全部 9 个 Sym 字段一律置为未知符号 `Sym.var("opaque:<name>:<field>")`，告警 `W-opaque`；
  若 `effects` 含 `'*'`，额外告警 `W-dynamic`（未声明动态能力，禁止按 0 计划）。

`PlanReport` 新增字段 `structure: object | None`：结构路径下存读到的 `root` 节点原样引用
（不序列化、不拷贝，供 Codex/测试核对"读到的结构"与"描述"是否一致）。原生 AST 路径下始终为 `None`。

`jv.plan(fn, ...)` 入口在计算 `inner`/`budget`/`rt`/`profile` 之后、进入原生 AST 分析之前，
先探测 `fn` 再探测 `inner` 上的 `__jv_structure__`；探测到就整段走新路径并直接 `return`，
不执行 `inspect.getsource`/`ast.parse`，**不触发任何执行**，只读 `Mapping` 结构。
`version != 1` 或 `root` 不是 `Mapping` 或某节点 `operation` 不在 5 种已知值内，一律
`raise JvError`（消息含具体字段，便于 Codex 侧定位）。

## 实际命令与结果

```
cd /Users/nature/个人项目/jev/地基
.venv/bin/python -m pytest foundation/tests/test_jv_structure.py foundation/tests/test_jv_passes.py -q
# 39 passed, 32 warnings（13 个新测试 + 26 个既有 test_jv_passes 全绿，原生 AST 路线未受影响）
```

新测试覆盖：`then` 成本相加、`branch` predicate+max（含数值可比与符号不可比两种分支）、
`opaque` 全字段未知符号且必带 `W-opaque`/`W-dynamic`、叶声明 effect 但 AST 未见到（含无源码
两种子情形）记未知不记 0、`identity` 零成本、既有 `@jv.program` 路径 `structure is None` 且行为不变、
未知 `version`/坏 `root`/未知 `operation`/`then` 或 `branch` 子节点数不对时清楚 `JvError`。
未跑全基础设施（按任务要求，只跑新测试 + 受影响的 plan 测试）。模型外呼 0 次。

## 状态

完成。未触及结构执行器（`then`/`branch` 按 children 直接调用、`describe()` 投影一致性）——
按分工由 Codex 实现；本文件只是识别端结果记录，不等 Codex 回复。

## 追加修正（2026-09-20，同日）：`_cost_of_leaf` 两处协议偏差

Codex 反馈后核对，`_cost_of_leaf` 有两处不符协议，均已修，只改 `plan.py` 与
`test_jv_structure.py`，未碰库/Runtime/ir.py。

1. **非零符号估计被误判「未见到」而覆盖。** 原判断 `seen = cost[field].is_numeric and cost[field].value != 0`：
   一旦字段是符号（`is_numeric=False`），不论其是否已由循环等得到非零估计（如 `|xs|`），都会被当作
   「未见到」重写成新的未知符号，等于把真实估计丢了。改为只在**明确数值 0**（`is_numeric and value == 0`）
   时才补未知；符号非零估计原样保留不动。
2. **无源码 + `effects=()` 时 9 项资源仍显示为 0。** 原代码只对 `declared` 里列出的 effect 做未知符号替换，
   `effects=()` 时循环不执行，无源码这个事实被丢掉，9 个 Sym 字段保持 `Sym(0)`——这正是协议禁止的
   「未知当 0」。现在改为：`function` 为 `None`、或 `effects` 含 `'*'`、或 `effects_contract == "unknown"`，
   整叶直接按不可证明处理，9 项资源全记未知符号（不再按单个 effect 摊）；`jv.plan(function)` 报出
   `W-nosource` 时同理，全字段置未知（不管 `declared` 是否为空）。同时把 effect→字段的映射从单字段
   扩成关联组（`_EFFECT_FIELDS`）：`judge` 未见到时连带 `questions`/`layers`/`unsure_bound` 一起记未知，
   `gen` 连带 `gen_latency`，`do` 连带 `do_cost`，不再让同一次「没见到」在报告里一半未知一半确定 0。
   另外补了明确拒绝：`function` 给了但不可调用（非 callable）是坏结构，直接 `JvError`，不静默吞掉。

新增 3 个定向测试（`test_jv_structure.py`）：
- `test_structure_leaf_preserves_nonzero_symbolic_estimate`：循环 `judge` 得到的 `calls=|xs|` 不被覆盖。
- `test_structure_leaf_no_source_and_empty_effects_marks_full_unknown`：无源码 + `effects=()` 时 9 项资源全非数值。
- `test_structure_leaf_noncallable_function_raises_clear_error`：`function` 非 callable 清楚报错。

```
cd /Users/nature/个人项目/jev/地基
.venv/bin/python -m pytest foundation/tests/test_jv_structure.py -q
# 16 passed（原 13 个 + 新增 3 个，全绿；原 test_jv_passes.py 26 个未重跑，未改动其涉及的原生 AST 路径）
```

模型外呼 0 次。此次只跑本文件测试（按 Nature 指示不跑全核心、不启动代理）。
