# G3 · 零上下文读者第三次复测

角色声明：我只读了 `11-语言规范-v1.md`（v1.1），没看仓库里任何别的文件，没联网。下面是我作为一个第一次接触这门语言的程序员，仅凭这一份文档能理解到什么程度、能不能写出程序。

## (1) 我理解的这门语言

这是一门"判断语言"：普通程序里 if/while 的条件判断，在这里可以换成一次"问 Jev"——把一段文字状态（叫 Mat，材料）和一道带类型的问题丢给一个黑箱评委，它吐回一个经过校准的三值结果（act/ignore/unsure）、一个选中项、或一个档位，这就是全部的不确定性来源；除了"问 Jev"这一步，程序里其余代码（切材料、拼候选集、跑测试、写文件）都是普通确定性代码（S 函数，用 do/pure 标注），所以整个程序理论上能逐字节重放。写程序的方式是：先声明预算 `budget`（花多少钱、调多少次、绑多少层），再用 `q 名字 : 题式 = "问题字面"` 声明要问的问题（题式从七种里选：attr/rel/cmp/class/mention/degree/decide），然后用 if/select/switch/while/partition/choose 这些控制结构把问题的调用（`题名(on:…, ctx:…, over:…, ref:…)`）接进去，凡是可能出现"不确定"（unsure）的地方都必须显式写清楚往哪儿走——细化材料、换个语境、升级给人、还是丢弃——不写就是编译错误，不允许默默吞掉。程序最后 return 的类型如果不含 `⊎ unsure`，中间所有没处理掉的不确定都必须提前收口。

## (2) 三个程序（严格按 §2 文法）

### a. 找出互相引用同一数据集但结论相反的论文对

```
program 冲突数据集论文(论文: Set) -> Set ⊎ unsure {
  budget calls <= 200, cost <= $0.02, layers <= 1
  q 同数据集 : rel = "这两篇论文引用了同一个数据集吗？"
  q 结论相反 : rel = "这两篇论文的结论相反吗？"
  let (冲突, 一致, 疑) = partition p in pairs(论文) where 同数据集(on: left(p), ctx: right(p)) ∧ 结论相反(on: left(p), ctx: right(p))
  return 冲突 ∪ mark_unsure(疑)
}
```
7 行。用 `pairs(S)` 拿无序不去自反的论文对；按 §6.2 第五规则"一题一命题"把"同数据集且结论相反"拆成两道 `rel` 题，用 `∧` 合成（对应 9.14 的 `pairs/left/right` 写法）。

### b. 客服工单按部门分派，判断不清要能升级给人

```
program 工单分派(工单: Mat) -> Mat ⊎ unsure {
  budget calls <= 5, cost <= $0.005, layers <= 1, escalate <= 1
  q 类别 : class[技术, 账单, 退款, 其他] = "这张工单该转给哪个部门？"
  switch 类别(on: 工单) {
    case 技术 { return lit("技术部") }
    case 账单 { return lit("账单部") }
    case 退款 { return lit("退款部") }
    case 其他 { return lit("综合处理组") }
  } unsure -> escalate
}
```
10 行（非空行：program/budget/q/switch/4×case/`} unsure`/`}`）。按单张工单为粒度写（参照 G-b 处理单条对话的先例），`switch` 恰好覆盖 `class` 的四个标签（避免 E6），转不清楚（unsure）时去向写 `escalate`，对应"转错要能升级给人"。之所以没有按 9.4 那样写成 `for t in 工单流 yield (t, 类别(on: t))` 一次处理整批工单，是因为 `for…yield` 的体是 `expr`，挂不上语句级的 `unsure -> escalate`（见猜测 2）——单张工单粒度是我为了留住"能升级给人"这个要求而选的写法，不是文法逼出的唯一解。

### c. 给 Python 仓库每个函数写 docstring：生成 3 版选最忠实，测试跑不过的函数不改

```
program 补写docstring(repo: Mat) -> Map {
  budget calls <= 400, cost <= $0.04, layers <= 2, escalate <= 20
  q 有失败 : attr = "从这份测试报告看，这个函数当前有相关测试失败吗？"
  q 最忠实 : cmp = "这份候选 docstring 最忠实地描述了函数的实际行为吗？"
  let 报告 = on_fail(do run("pytest -q", repo), lit("测试未能运行"))
  let (跳过, 待写, 疑) = partition f in ast(repo, "functions") where 有失败(on: f, ctx: 报告)
  drop 疑
  return for f in 待写 yield (f, take(最忠实(over: do gen(3, "为这个函数写一句忠实准确的 docstring", {signature(f)}), ctx: f)))
}
```
9 行。`on_fail` 先把 `do run` 可能的 `fail` 吸收掉（照抄 §4.11 的示例句）；`有失败` 把"这个函数测试过不过"变成一道字面题，用 `partition` 分出"测试失败的（跳过，不碰）"与"待写的"；`疑` 显式 `drop` 掉避免 E3；每个待写函数用 `do gen(3, …)` 生成 3 个候选，`take(最忠实(…))` 挑最忠实的一个。`budget` 里补了 `escalate <= 20`——理由见下面的猜测 1。

## (3) 我猜的地方

1. **`take(call)` 没写 `prior` 时内置去向链自带 `escalate`，算不算"escalate 出现"而触发 E10**：§6.4 第 321 行 `take` 的固定去向是"unsure → regen（over 来自 gen）→ escalate → 返回 unsure"；程序 c 里 `take(最忠实(over: do gen(...), ...))` 的 `over` 正好来自 `do gen`，整条链（含 escalate）都会被激活。E10（第 135 行）说"`escalate` 出现而 `budget escalate` 为 0"是错误，但这是指源码里字面写 `escalate` 关键字（像程序 b 那样），还是也把组合子内置的隐式 escalate 算进去，规范没说。我按"算"处理，在程序 c 的 budget 里补了 `escalate <= 20`；程序 b 因为字面写了 `escalate` 所以本来就有 `escalate <= 1`，这个不对称正是让我意识到这处空白的地方。

2. **`for x in S yield e` 挂不上语句级的 `unsure -> dest`**（注意：这条比"没有逐元素跑语句的循环"要窄，我第一版写宽了，纠正如下）：`for…yield` 处理整个集合是完全合法的（9.4 就是 `for r in log_stage(log) yield (r, 类别(on: r))`），我不是写不出批量版本。真正缺的是：`yield` 后面是 `expr`（§2 第 87 行），不是 `block`，所以没有地方挂 `unsure -> escalate` 这种语句级去向子句；`select`/`switch` 这类带 `unsure -> dest` 的结构是 `stmt`，塞不进 `expr` 位置。`loop bound n`（第 73 行）也没有 `loop x in S` 这种绑定集合元素的写法。所以"批量处理 + 每个元素独立升级给人"这个组合，文法里没有直接写法。程序 b 因此改成单张工单粒度（参照 G-b），这是我为了保住"能升级给人"而做的选择，不是文法唯一解。

3. **`Outlet` 类型定义没有把 `Chosen`（select/cmp/decide 的出口）算进去**：§3.1 第 112 行写"`Outlet = Outlet3 ⊎ 标签 ⊎ 档 ⊎ unsure`"，没提 `Chosen`；但示例 9.1（第 391–397 行，`哪块(on:e, over:块)` 是 `cmp`）和 9.6（第 447–451 行，`class` 结果）都直接把 select/class 的结果塞进 `for…yield (k,v)` 得到 `Map`，9.17 核对表还标"✓"。正文类型定义和示例这里对不上。

4. **`let` 绑定过、guard 里不含字面 `do` 关键字的变量，若其值在运行期是 `fail`，算不算触发 E15**：E15（第 140 行）按"`do` 是否字面出现在守卫里"判定要不要绑四流；但 §4.1（148–153 行）与 §4.11 第 3 条（239 行）讲的是"槽位含 fail 值"这个运行期语义，跟 `do` 关键字是否字面写在 guard 里无关。两处判定依据不一致，我在程序 c 里用 `on_fail(do run(...), …)` 提前吸收掉了 fail，绕开了这个问题，不是规范给出的答案。

5. **`do gen(...)` 的 `Set ⊎ fail` 直接塞进题的 `over:` 槽、又被 `take(...)` 包裹后，`take` 的返回类型要不要额外带 `⊎ fail`**：§4.11 第 3 条说槽位含 fail 元素时判断出口是 `failed`，按 unsure 去向表处理；但 `take` 的签名（§6.4）只写 `Mat ⊎ unsure`，规范没给出"fail 经槽位吸收后是否还需要额外声明 `⊎ fail`"这种嵌套场景的例子。我按"槽位已经把 fail 折进 unsure 去向表"处理，没加 `⊎ fail`，这是我的猜测。

6. **`allow pairs` 写不出来**：§5.4 第 271 行说笛卡尔积静态可估时"报 W-cost 并要求显式 `allow pairs`"，§7 第 369 行的诊断示例也说"加 `allow pairs`"；但 `allow` 既不在 §1 的关键字表（第 36 行）里，§2 文法（`stmt`/`decl` 等所有产生式）也没有任何地方能写出 `allow pairs` 这种子句。规范让我写一个文法里不存在的东西。程序 a 用了 `pairs(论文)`（N² 个状态）正撞上这条；更麻烦的是示例 9.14 同样用 `pairs(log_stage(log), 规则)`，既没写 `allow pairs`，9.17 核对表还标"✓"——正文规则和示例又对不上。顺带：`unsure` 产生式（§2 第 77 行）里的 `then` 也不在关键字表里，第 36 行还写着"关键字（22 个）"，但我数了一下实际列出的有 39 个（`program budget q s pure do anchors import let if else host unsure select as prior switch case while bound improving patience decreasing increasing choose loop escalate drop return for in yield partition where state on ctx over ref`），数字和列表本身就对不上。

## (4) 一句话评价

能让新手照抄书里的 18 个示例模式解决大多数常规任务，但一旦任务偏离示例覆盖的模式（比如要对集合逐元素跑语句、要判断"这次不确定算不算失败"），规范留的空白就会逼着新手自己下判断或去问人，不算"零问人"。
