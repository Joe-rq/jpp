# 独立作者首次使用记录

本记录保留第一轮独立使用的事实及当时发现的缺口；文末另记公开说明补齐后的第二轮原生观察接入结果。

本次作者只阅读本包 `README.md`，以及 `jev_compose/__init__.py` 的公共导出列表。没有读取核心实现、算法实现、观察实现、现有示例、现有测试或其他会话。新增文件为 `independent_method.py`、本记录及 `tests/test_independent_method.py`，没有修改库。

## 新方法：保留未决项目的预算分配

调用者提交项目、成本、收益、审核答案和预算。方法先按“至少获得 k 份支持”计算每个项目的确定下界和可能上界，把项目分为合格、未决和不合格，再将合格项目交给调用者提供的分配方法。默认分配方法通过有限动态规划求解预算内收益最大的项目子集；收益相同时优先较低成本。

新方法 `build_reviewed_allocator(allocator: Component) -> Component` 接收一个完整方法，返回另一个可运行的完整方法。`build_report(allocator)` 再把返回的方法嵌入 `product`，与申请总额、未决答案数一起产生报告。测试还将这个完整报告方法作为普通组件中的函数继续调用。

所有审核数据都是明确标注 `synthetic_fixture_no_model_call` 的 `SyntheticReview`。这是本地测试数据类型，没有冒充内核 `Observation`，没有调用模型。观察值的正确性、模型校准、原生观察记录均不在此次验证范围。

## 我从文档理解并实际使用的接口

| 接口 | 本方法中的用途 | 实际验证 |
|---|---|---|
| `component(name, Input, Output)` | 声明项目审核、预算分配、报告打包等叶组件 | 自定义 dataclass 端口可连接、运行 |
| `product(a, b, ...)` | 把同一项目池交给审核及 `identity`；外层同时产生分配报告所需结果 | `tuple` 中顺序与声明一致 |
| `identity(Portfolio)` | 保留原始预算以便与审核结果重新合并 | 后续分配使用输入预算 |
| `a.then(b)` | 把审核与预算准备接到分配，再把并列结果接到报告 | 最终得到 `Report` |
| `branch(predicate, yes, no)` | 没有合格项目时生成空分配结果 | 会抛错的分配器未被执行 |
| `Component` 作为参数及返回值 | 替换精确分配器与最低成本优先分配器 | 同样预算 6，分别选 A 与 B |
| 普通组件调用 | 在另一个 `@component` 中调用返回的完整报告方法 | 外层读到收益 21 |
| `execute(method, value, runtime())` | 运行完整方法 | 程序及所有测试成功 |
| `describe()` | 检查可见组合结构 | 显示嵌套的 `then` / `product` / `branch` / `identity`；effects、warnings 为空 |

我把 `Component` 理解为可声明输入输出、可调用、可连接的同一种方法对象。构造高阶方法只需要普通 Python 工厂；库提供的组合操作负责接线并保持类型接口。业务数据中的“未决”和“合格但未获预算”由作者分别保存。

## 实际结果

样例预算为 10，至少需要两份支持。

| 项目 | 成本 | 收益 | 合成审核答案 | 处理结果 |
|---|---:|---:|---|---|
| A | 6 | 13 | True、True、None | 已确定合格，未获预算 |
| B | 5 | 11 | True、True、False | 选中 |
| C | 5 | 10 | True、True、True | 选中 |
| D | 2 | 100 | True、None、False | 保留未决 |
| E | 1 | 99 | False、False、None | 已确定不合格 |

实际输出选择 B、C，总成本 10，总收益 21，申请总额 19，未决答案数 3。A 的一项答案未决并不妨碍其达到支持下界，E 的一项答案未决也不足以使其达到支持上界；只有 D 的项目状态仍未决。方法保留 D，继续计算 B、C 的确定结果。

从本包根目录执行：

```sh
/Users/nature/个人项目/jev/地基/.venv/bin/python -B independent_method.py
/Users/nature/个人项目/jev/地基/.venv/bin/python -B -m unittest discover -s tests -p test_independent_method.py -v
/Users/nature/个人项目/jev/地基/.venv/bin/python -B -c 'import json; from independent_method import build_report; print(json.dumps(build_report().describe(), ensure_ascii=False, indent=2))'
```

2026-09-20 实际运行：程序退出码 0；5 项测试全部通过，`Ran 5 tests in 0.033s`。测试覆盖具体输出、策略替换、未被选中的分支不执行、返回方法继续嵌套，以及预算 0 到 17 的 18 个有限输入与独立子集穷举结果一致。这里只证明这些有限实例上的最优结果，没有把样例测试写成一般正确性证明。

加载时把本包根目录及 `_kernel_snapshot` 插入 `sys.path`，基础内核入口仅为 `from foundation import jv`；没有导入内核内部模块，没有关闭检查器，也没有调用原有 examples。`jv` 保留为公开入口，本例的纯计算无需直接发起内核动作。

## 文档缺口

纯计算组件、组合以及高阶传递的说明足以完成本方法；未遇到运行错误，也不需要查看实现。

README 展示了 `runtime()`，但没有介绍如何给指定题目配置可控的合成 True / False / 未决观察，也没有给出 `Observation` 的构造或 fixture 工厂示例。因此本次作者采用自有 `SyntheticReview` 来测试业务保留未决的行为，没有声称完成 `observe` 或 `at_least` 的独立接入。补一段使用公开 fixture 生成原生 `Observation` 的最小例子，才能让首次作者仅凭说明接续这条路径。

README 给出 `component.program(name=..., budget=...)`、`jv.Action` 和 `jv.do` 的入口，但未提供本说明内可直接运行的预算值与 Action 构造例子。本例选择已有完整运行示例中的 `execute(..., runtime())`，精确计算由普通 Python 实现。它证明新方法的构造、替换和嵌套可用，不构成外部动作、预算消耗或模型能力的验证。

## 公开说明修复后的原生观察接入

2026-09-20，维护者在 README 新增“独立开发时获得观察”，提供了 `demo.flag` 问题、`flag=True/False` 与 `unknown=True` 固定数据、`execute(batch_observe, requests, runtime())` 的完整入口。作者只读这段新增公共说明，即完成原生接入，没有查看内核或库内部。这修复了第一轮所遇到的固定观察接入缺口。

新增的 `sample_native_portfolio()` 把原样例的 15 份审核答案转成独立材料，交由公开 `batch_observe` 取得原生 `Observation`。`NativeReview` 适配器只读取公开的 `resolved`、`value`，向原算法提供同样的 True / False / None 接口，并保留观察对象。原问题、分类逻辑、动态规划、策略替换以及原合成示例均保持原样。

新增 `build_native_report(allocator=exact_allocator)` 接受 `NativePortfolio`，以 `adapt_native_reviews.then(build_report(allocator))` 复用整套原方法，再通过外层 `product` 和报告组件附上全部原始观察。输入适配、精确分配、再次封装均实际运行。模型数据依然来自免费合成 fixture，报告标记 `synthetic_fixture_native_judge_cut`。

供 CLI 集成的函数为 `independent_method.native_observation_demo()`。它返回 JSON 可序列化的字典：`report` 保存分配结果，`observations` 保存全部原生观察的 `to_dict()` 记录，包括观察身份、题目、模型标识、出口与材料哈希。

```sh
/Users/nature/个人项目/jev/地基/.venv/bin/python -B -c 'import json; from independent_method import native_observation_demo; print(json.dumps(native_observation_demo(), ensure_ascii=False, indent=2))'
/Users/nature/个人项目/jev/地基/.venv/bin/python -B -m unittest discover -s tests -p test_independent_method.py -v
```

两条命令实际退出码均为 0。原生观察共 15 个，其中肯定 8 个、否定 4 个、未决 3 个；输出记录的 `model_id` 为 `synthetic-composition-fixture-v1`，未决出口为 `unsure`，原因为 `band`。结果仍选择 B、C，成本 10、收益 21；保留 D 未决、E 不合格、A 合格未获预算。

测试由 5 项增加至 7 项，实际结果 `Ran 7 tests in 0.337s`、全部通过。新增测试验证原生出口数量、最终业务结果、JSON 记录，以及所有 15 个观察和对应 `Request` 在外层报告中仍是原对象。此次补充验证了合成数据经过公开原生观察入口后可接入新方法，没有验证真实模型准确率、真实校准或付费模型执行。
