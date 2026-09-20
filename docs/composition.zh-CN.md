# jev-compose：语言的组合与复用层

这套库把问题、计算组件和求解方法接到同一个 `foundation.jv` 内核上。用户定义组件，用接续、分支、动态接续和迭代构造算法；算法仍是同一种组件，可以继续传递、返回和组合。

实现采用 Python 3.12，因为 Claude 的当前内核以 Python 为公开入口。库不包含第二套模型客户端、预算执行器或账本。OCaml / Rust 的独立内核选型保持为后续设计选择。

## 运行

安装步骤见仓库 README。安装后执行：

```sh
jpp demo
python -m pytest -q
```

本发布包包含固定的运行内核与组合源码。演示使用合成观察，不调用真实模型；算法与有限输入检查实际执行。

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
| `a.bind(factory, Output, effects=...)` | a 的结果交给 factory，factory 返回下一段组件，并以该结果作为输入执行它 |
| `branch(predicate, yes, no)` | predicate 返回明确 bool，只执行被选中的一支 |
| `product(a, b, ...)` | 同一输入产生多个结果，按声明顺序执行；不自动推测或重排外部动作 |
| `iterate(step, done, limit=N)` | step 为 S→S，done 为 S→bool；返回 `Iteration(state, steps, reason)` |
| `describe()` | 返回组合结构、输入输出、效应声明和静态提示 |

`Iteration.reason="done"` 表示调用者定义的停止条件成立，并不自动意味着业务成功。业务状态中应保留成功、未决、无候选等区别。

动态接续的三种写法不同：返回问题值是构造/交付数据；执行 `observe` 是取得观察；返回一个 `Component` 是交付方法。`bind` 明确选择并执行下一段方法。其 `effects` 参数为动态部分声明允许的能力，静态结构中也会保留这个边界。

`describe` 是组合库可见结构。内核在实际调用中仍负责效果、预算和记录；当前原生内核的静态成本分析未完整展开所有动态组件。动态宿主函数保留运行期检查，不把结构描述冒充全程序静态证明。

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

`batch_observe(list[Request])` 先登记全部判断，再取得出口，使现有内核有机会在合法条件下融合。不同材料仍遵循内核的状态规则；不是把所有对象拼成一个状态。

`question_to_dict(q)` / `question_from_dict(data)` 保存和恢复问题值。恢复问题也不会执行它。

`at_least(observations, k)` 对布尔观察计算确定的数量上下界；只有上下界已经足够决定“至少 k”时才返回 bool，否则返回 `None`。结论以当前接受的观察为前提，不是现实正确率保证。

`refine(observations, {unknown_id: new_request})` 只重问指定未决项，已有结果继续复用。同一未知的多个引用获得同一份更新。新证据通过 `jv.transform` 等现有材料入口准备；改变问题或材料产生新的观察身份。

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

## 完整性验收

本包检验三项行为：

1. 独立、传参、返回、继续嵌套后，输出和所需执行行为仍然成立。
2. 两种算法复用相同构件，替换策略时不修改库核心。
3. 未参与实现的作者只凭本说明构造第三个未预置方法，再接入已有组合。

详细结果由运行命令生成。合成观察用于语义与工程验证；它们不证明真实模型准确率、真实校准或普遍加速比。表达式示例验证全部**指定有限输入**，不把它说成任意整数上的完整证明。
