# jev-compose：组合标准库初版

这套库把问题、计算组件和求解方法接到同一个 `foundation.jv` 内核上。用户定义组件，用接续、分支、动态接续和迭代构造算法；算法仍是同一种组件，可以继续传递、返回和组合。

当前已贯通 `then / branch / product / iterate / bind`：不可变子节点决定执行、描述与共同计划；动态后续保留本次生成结构。完整程序、实际输出及文件集见 [方法构造方法交付.md](方法构造方法交付.md)。上一包等价用法保留在 [IR贯通验收.md](IR贯通验收.md)。

实现采用 Python 3.12，因为 Claude 的当前内核以 Python 为公开入口。库不包含第二套模型客户端、预算执行器或账本。OCaml / Rust 的独立内核选型保持为后续设计选择。

## 运行

从项目的 `地基` 目录运行：

```sh
.venv/bin/python 扩展/codex_composition/run.py demo
.venv/bin/python 扩展/codex_composition/run.py test
.venv/bin/python 扩展/codex_composition/run.py integrate
.venv/bin/python 扩展/codex_composition/run.py test-integration
```

`run.py` 使用 `_kernel_snapshot/` 中未修改的源码快照，具体文件及 SHA-256 见 `kernel_manifest.json`。快照仅包含内核源码、依赖源码和模型档案，不含凭据。模型演示默认使用明确标注的合成观察，费用为零；执行算法和表达式检查实际运行。

其中 `integrate` 与 `test-integration` 明确使用**工作内核**，需要 Claude 实现的 `jv.plan` 结构协议；旧 `demo` / `test` 入口继续使用初版快照，保留可重复的历史依赖。

运行后打开 `out/demo.html` 可查看候选收缩、表达式构造和预算分配；完整执行结构、材料来源与数值保存在 `out/results.json`。重新运行会更新本包的这两个输出文件。`run.py verify` 核对全部快照文件。

已有脚本从项目 `地基` 启动时，可把本目录加入模块路径后直接导入 `jev_compose`；`foundation.jv` 仍由宿主项目提供。演示入口额外固定依赖快照。`pyproject.toml` 仅打包组合库，未把整个语言内核发布成独立安装包。

## 定义组件，构造新的组件

```python
from foundation import jv
from jev_compose import component, execute
from jev_compose.fixtures import runtime

@component("length", str, int)
def length(text):
    return len(text)

@component("double", int, int)
def double(n):
    return 2 * n

method = length.then(double)
result = execute(method, "hello", runtime())
assert result.value == 10
```

`component(name, input_type, output_type, effects=...)` 声明一个叶组件。定义时调用现有内核检查器检查其可见源码；组件连接时检查端口类型，运行时检查顶层值类型。类型检查是名义类型及容器外形检查，不宣称完整的 Python 泛型静态证明。`Any` 表示显式开放边界。

组件通过普通调用执行，也可传入/返回其他组件。所有组件调用共用当前 jv 执行作用域。`component.program(name=..., budget=...)` 使用真正的 `@jv.program` 入口，使组件可以独立使用或嵌入另一段 jv 程序。默认检查器一直开启。

## 公共组合操作

| 操作 | 语义 |
|---|---|
| `a.then(b)` | a 的结果成为 b 的输入；检查相接类型 |
| `identity(T)` | 返回输入，便于统一构造 |
| `a.bind(factory, Output, effects=..., factory_effects=...)` | a 的结果交给 factory；factory 本身可调用能力，返回下一段组件，然后以该结果为输入执行它 |
| `branch(predicate, yes, no)` | predicate 返回明确 bool，只执行被选中的一支 |
| `product(a, b, ...)` | 同一输入产生多个结果，按声明顺序执行；不自动推测或重排外部动作 |
| `iterate(step, done, limit=N)` | step 为 S→S，done 为 S→bool；返回 `Iteration(state, steps, reason)` |
| `describe()` | 返回组合结构、输入输出、效应声明和静态提示 |
| `replace_at((子节点下标, ...), replacement)` | 沿 then / branch / product / iterate / bind 重构；重新核对接口及能力，旧组件不变 |
| `structure()` | 产生交给共同计划器的只读 v1 结构；叶保留 callable，本轮不提供序列化 |

`Iteration.reason="done"` 表示调用者定义的停止条件成立，并不自动意味着业务成功。业务状态中应保留成功、未决、无候选等区别。

动态接续的三种写法不同：返回问题值是构造/交付数据；执行 `observe` 是取得观察；返回一个 `Component` 是交付方法。`bind` 明确选择并执行下一段方法。`effects` **只约束 factory 返回的后续组件**，不能用它解释 factory 已经发生的调用。

factory 自身的契约：

- 普通 Python factory 未给 `factory_effects` 时，能力为未知，整体 `effects` 含 `"*"`。
- 显式 `factory_effects={"judge"}` 等声明计入整体能力；若 factory 自身是 `Component`，默认采用它的声明。
- `factory_effects=()` 是作者声明，不是已经证明纯净。库不承诺发现任意 Python 黑盒的隐藏调用。共同计划器读取 bind 的前段和工厂，后续保留未知符号，不据空声明删除、改序或当零成本。
- 后续组件仍单独受 `effects` 子集检查；factory 声明不能替后续组件放宽边界。

全部公开组合使用子节点直接执行，不另存独立执行闭包。`program()` 从同一节点产生 `__jv_structure__`；共同 `jv.plan` 对 product 求和，对 iterate 取最多 N 步与 N+1 次停止检查，对 bind 保留动态未知。`execute()` 的本次 trace 保存 bind 的生成结构、调用编号、后续事件范围和结果；不会写回共享组件。普通宿主叶仍按契约调用，未知不当零成本。

## 问题、观察和局部未决

```python
from jev_compose import Request, observe, batch_observe, at_least

q = jv.test("满足指定条件吗？", calib=jv.calib("my.question"))
request = Request(jv.state(on=jv.mat({"description": "..."})), q)
# 构造 request 不发起调用；以下代码在 jv 运行作用域内执行。
answer = observe(request)
```

`Request(state, question, label="", tag=None)` 使用内核自己的题和材料。`tag` 是用户的程序数据，不发送给模型，可保存例如问题对应的候选子集。

`Observation` 提供：`id`、`resolved`、`value`、`cause`、`request`、`decision`、`model_id`。布尔题的值为 bool，选择题为候选索引，度量为档位索引；未决时 `resolved=False`。它保留实际内核出口与材料来源。`to_dict()` 用于输出记录。

`id` 是**请求内容与来源身份**，并不是某一次执行记录的唯一身份。同一请求在不同预算下可能分别得到已知与未决，不能因此互相覆盖。`observe` 对未决采用显式 `retain` 策略：通过原生 `match` 承接出口，保存原出口、原因和来源，返回部分结果；调用者仍需决定如何使用该部分结果。`to_dict()` 标出 `uncertainty_policy="retain"`。

`batch_observe(list[Request])` 先登记全部判断，再取得出口，使现有内核有机会在合法条件下融合。不同材料仍遵循内核的状态规则；不是把所有对象拼成一个状态。

`question_to_dict(q)` / `question_from_dict(data)` 保存和恢复问题值。恢复问题也不会执行它。

`at_least(observations, k)` 对布尔观察计算确定的数量上下界；只有上下界已经足够决定“至少 k”时才返回 bool，否则返回 `None`。结论以当前接受的观察为前提，不是现实正确率保证。

`refine(observations, {unknown_id: new_request})` 只重问指定未决项，已有结果继续复用。同一未知的多个引用获得同一份更新；即使已知观察拥有相同请求 `id`，它仍保持原对象，绝不被替换。新证据通过 `jv.transform` 等现有材料入口准备；改变问题或材料产生新的请求身份。

### 独立开发时获得观察

下面的完整入口使用本包固定观察：`flag=True/False` 分别得到肯定/否定，`unknown=True` 得到未决。它实际经过原生 `judge → cut`，便于先运行自己的组合。

```python
from foundation import jv
from jev_compose import Request, batch_observe, execute
from jev_compose.fixtures import runtime

q = jv.test("flag 成立吗？", calib=jv.calib("demo.flag"))
requests = [Request(jv.state(on=jv.mat(record)), q)
            for record in ({"flag": True}, {"flag": False}, {"unknown": True})]
result = execute(batch_observe, requests, runtime())
assert [(o.resolved, o.value) for o in result.value] == [(True, True), (True, False), (False, None)]
```

自定义合成观察可向 `runtime(rule=my_rule)` 传入函数，签名为 `(state_text, question_id, lowered_question) → answer_dict`；返回体遵循 `jv.FakeClient` 的公开客户端协议。测试应给这些结果标记合成来源。也可以把调用 `batch_observe` 的叶组件接到自己的精确算法前面，保留观察对象和未决身份。

真实模型接入时，传入 `jv.Runtime(jv.JevClient(...), profile=...)`，其余组合方式相同。真实运行使用自己的模型档案和校准记录；本包 `fixtures.runtime` 中的合成校准不可迁移到真实模型。未校准题目正常返回 `Unsure(cold)`，由上层决定继续收集材料或使用明确的候补策略。

## 两种可复用的算法构造器

```text
inquire(prepare: S→Request,
        update: (S,Observation)→S,
        done: S→bool,
        limit=N, observer=observe) → Component<S,Iteration>

feedback(propose: S→list,
         inspect: (S,list)→list,
         update: (S,list)→S,
         done: S→bool,
         limit=N) → Component<S,Iteration>
```

参数都是组件。`inquire` 让结果决定下一道问题；`feedback` 把检查结果送回候选构造。它们都由同一个 `iterate` 构造，返回同一种 `Component`。选材、问题来源、候选生成和检查的具体方法属于调用者。

可以把一个求解器用作另一个求解器中的检查组件，也可以先写一个返回 `Component` 的策略，再通过 `bind` 执行它。只要签名兼容，不需要新增一类运行时。

## 材料、执行与检查

使用唯一内核入口 `from foundation import jv`。材料通过 `jv.mat`、`jv.transform` 或效应输出产生；模型原始读数经 `jv.cut` / `jv.fit` 后进入程序。库的 `observe` 已完成这一步，并显式保留未决。独立判断与复合方法都遵守同一个边界。

精确检查通过 `jv.Action` 和 `jv.do(..., iter_seq=...)` 接入，必要时使用 `jv.on_fail` 处理失败。生成器通过 `jv.gen(..., retry_seq=...)` 接入。已有内核负责实际调用、失败、重放与预算。

组件的 `effects` 是所用能力的声明，并通过组合向上传播；动态 `bind` 会检查所选组件的声明。它不是任意 Python 代码的沙箱，也不会自动识别隐藏在外部黑盒中的所有效应。

一个可运行的本地动作入口如下，完整表达式检查程序见 `jev_compose/examples.py`：

```python
def check_even(material):
    return {"even": material.content % 2 == 0}

check = jv.Action("example.check_even", fn=check_even, taint_out="trusted")

@component("check_one_number", int, dict, effects=("do",))
def check_number(number):
    output = jv.do(check, jv.mat(number), iter_seq=0)
    return jv.on_fail(output, alt=jv.mat({"failed": True})).content

result = execute(check_number, 4, runtime(), budget=jv.Budget(calls=2))
assert result.value == {"even": True}
```

这里的 `calls=2` 约束原生模型调用，具体动作费用仍以当前内核的能力为准。动作的可信声明由作者为这个可检查的精确函数给出，声明本身不证明任意外部工具正确。

## 换组件，写自己的程序

- **换问题/选材**：给 `inquire` 传入自己的 `S→Request` 组件；原循环、预算入口和观察出口继续复用。
- **换查找策略**：`make_inquiry(my_partition)`；传入的函数返回当前候选的一个非空真子集。选材通过跟踪材料的 `transform` 完成。
- **换生成与检查**：`make_synthesis(proposer=my_proposer, inspector=my_inspector)`；二者保留已声明的端口。换领域的数据示例见演示中的绝对值→平方。
- **换完整方法**：`independent_method.build_report(my_allocator)` 接收 `AllocationInput→Allocation`。`build_native_report` 还接受保留来源的原生布尔观察；完整例子只依靠本说明实现，见 `INDEPENDENT_USE.md`。
- **再封装**：返回的每个结果仍是 `Component`，可以继续 `.then`、`product`，也可 `.program(budget=...)` 后由更外层程序调用。

## 完整性验收

本包检验三项行为：

1. 独立、传参、返回、继续嵌套后，输出和所需执行行为仍然成立。
2. 两种算法复用相同构件，替换策略时不修改库核心。
3. 未参与实现的作者只凭本说明构造第三个未预置方法，再接入已有组合。

详细结果由运行命令生成。合成观察用于语义与工程验证；它们不证明真实模型准确率、真实校准或普遍加速比。表达式示例验证全部**指定有限输入**，不把它说成任意整数上的完整证明。
