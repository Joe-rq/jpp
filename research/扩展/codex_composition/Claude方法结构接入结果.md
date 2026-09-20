# Claude 方法结构接入结果：`__jv_structure__` 新增 product / iterate / bind

2026-09-20（同日第二轮）。在已落地的 `leaf/identity/then/branch/opaque` 五种 operation 之上，
补齐组合库另外三种结构：`product`（顺序全跑，资源相加）、`iterate`（有限步循环，先查 done）、
`bind`（前段 + 运行期动态 factory）。只改了 `foundation/jv/plan.py` 与
`foundation/tests/test_jv_structure.py`，未碰 Runtime / ir.py / 组合库；ir-impl-6 与 Codex
的组合库改动并发进行中，未触碰。

## 规则与实现（`foundation/jv/plan.py`，`_cost_of_product`/`_cost_of_iterate`/`_cost_of_bind`）

- **product**：`children` 必须非空（≥1），否则 `JvError`；顺序对每个子节点求成本，9 个 Sym 字段逐项相加。
  不区分子节点顺序（成本合成是可交换的加法），"顺序全部执行" 只影响运行期语义，不影响计划期成本。
- **iterate**：`children` 必须恰 2 个 `(step, done)`；`parameters.limit` 必须是非负整数（显式拒绝
  `bool`、负数、非 `int`），否则 `JvError`。成本 = `step_cost × N + done_cost × (N+1)`——先查一次 done
  才决定进不进 step，最多 N 步、最多 N+1 次 done 检查；`N=0` 时只剩一次 done 检查，`step` 系数为 0。
  这是保守上界（N 步全部执行、每步都要重新查 done），不是精确值。
- **bind**：`children` 必须恰 2 个 `(前段, factory)`；两者都按 `_cost_of_node` 递归求值（factory 子节点
  若是 `leaf`，沿用既有 leaf 规则：`plan(factory.function)` 做 AST 分析，只估计 factory 函数体自身直接可见
  的调用，**不代表 factory 返回的 Component**）。factory 返回的 continuation 只在运行期产生，静态计划绝
  不调用 factory（测试用 `factory` 函数体里放一个 `raise AssertionError` 验证这点——AST 分析只读源码，
  不执行，所以正常情况下这个 assertion 永远不会被触发）。`parameters.continuation_effects` 即便声明为
  空元组，也不能当「已知零」处理（空声明只是作者声明，不是证明，与 leaf 的规则一致）——continuation
  的全部 9 个 Sym 字段统一记未知符号 `bind:<name>:continuation:<field>`，并告警 `W-dynamic`。
  总成本 = 前段成本 + factory 调用本身的成本 + continuation 未知成本，三者相加。

`_STRUCT_OPS` 从 5 种扩到 8 种；`_cost_of_node` 的 dispatch 表相应加了三行。不引入新的 IR/运行时，
不改变已有 `leaf/identity/then/branch/opaque` 的行为，`PlanReport.structure` 仍原样保存 root 节点引用。

## 实际命令与结果

```
cd /Users/nature/个人项目/jev/地基
.venv/bin/python -m pytest foundation/tests/test_jv_structure.py foundation/tests/test_jv_passes.py -q
# 51 passed, 35 warnings
# （test_jv_structure.py 25 个：上一轮 16 个 + 本轮新增 9 个；test_jv_passes.py 26 个既有全绿，
#   原生 AST 路线未受影响；warning 数增到 35 是因为并发的 ir-impl-6 在 runtime.py/spec.py 加了新
#   告警点，与本次改动无关，不影响断言）
```

新增 9 个定向测试：
- `test_structure_product_sums_all_children`：3 个叶（2+1+1 次 gen）求和得 4。
- `test_structure_product_empty_children_raises_clear_error`：0 个 children 清楚报错。
- `test_structure_iterate_cost_is_n_step_plus_n_plus_1_done`：N=3 时 `3·step + 4·done`。
- `test_structure_iterate_limit_zero_is_done_only`：N=0 时只剩一次 done，step 不计。
- `test_structure_iterate_negative_limit_raises_clear_error` / `..._non_int_limit_...`：limit 校验。
- `test_structure_iterate_wrong_children_count_raises_clear_error`：children≠2 报错。
- `test_structure_bind_never_calls_factory_and_continuation_is_unknown`：factory 函数体里挂
  `raise AssertionError` 反证「静态计划绝不调用 factory」；同时断言 `gen_calls`/`do_calls` 因
  continuation 未知而非数值，且必带 `W-dynamic`。
- `test_structure_bind_wrong_children_count_raises_clear_error`：children≠2 报错。

保留了上一轮全部测试（识别端 5 种 operation + 两处 `_cost_of_leaf` 协议修正回归测试）不动。

## 边界与已知限制

- `product`/`iterate`/`bind` 的成本合成都是「保守上界」，不是精确模拟：`iterate` 假设 N 步全跑且每步
  都重新查 done；`bind` 完全放弃对 continuation 的结构性认知，只给未知符号——这是协议要求的方向
  （宁可保守也不可当 0），不是可以后续「优化掉」的粗糙实现。
- `bind` 的 factory 子节点如果本身也是 `opaque`/`bind`（嵌套动态），会正常递归求值并各自贡献未知符号+
  告警，没有特殊处理，符合"子为任意叶时沿既有保守规则"的推广（这里推广到"子为任意结构"）。
- 未新增运行时、未新增 IR 形式；`then`/`branch` 之外的执行器仍完全由 Codex 负责，本文件只记识别端结果。
- 未跑全基础设施，只跑了本次改动直接相关的 `test_jv_structure.py` 与既有 `test_jv_passes.py`（含 `jv.plan`
  唯一两个测试文件）。模型外呼 0 次，未启动额外代理。
