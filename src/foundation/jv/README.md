# jv — 在普通 Python 里把「判断」当一等效应用

日期 2026-09-21。这是 `12-IR与类契约-v0.1.md` 的实现（建造顺序 §9 第 1–3 步），本 README 写给**第一次用它的人**：先看 §0 的最小程序，再按 §1 的概念表和 §2 的 API 逐段查。规范文本（12 / 09 / 00-宪法）不在这里改；实现与规范不合处列在 §7「偏差表」作为提议。$0 测试 325 条全过；真机冒烟 E-IR-SMOKE（$0.00025）见 `experiments/前提结论.md`。

一句话：程序是普通 Python，里面有四种效应——`jv.judge`（问模型）、`jv.gen`（生成候选）、`jv.do`（触世界）、`jv.ask`（问人）。`judge` 返回**读数**，读数不能比较、不能算术、不能当真值用；只能经 `jv.cut` 变成**出口**（做 / 不做 / 拿不准 / 选第 k 个 / 落第 k 档）。程序对每一个「拿不准」都必须有交代。

## 0. 一分钟上手

```python
import foundation.jv as jv

同人 = jv.test("这两条客户记录指的是同一个人吗？", calib=jv.calib("crm.同人"))   # 题：是非题，校准键必填

@jv.program(budget=jv.Budget(calls=50, cost=0.02, layers=2))
def 名单合并(甲, 乙):                                  # 甲、乙：Mat 列表
    对子 = [(i, j) for i in range(len(甲)) for j in range(len(乙))]
    exits = jv.cut(jv.judge([jv.state(on=甲[i], ctx=[乙[j]]) for i, j in 对子], 同人))   # 一层：全部对子并发
    并掉 = set()
    for (i, j), e in zip(对子, exits):               # exits 与 对子 顺序一一对齐
        match e:
            case jv.Act(): 并掉.add(j)                # 是同一个人
            case jv.Ignore(): pass                    # 不是
            case jv.Unsure(c): jv.handle(c, then=jv.escalate)   # 拿不准 → 交给人（可能抛 Pending）
    return [m.content for m in 甲] + [m.content for j, m in enumerate(乙) if j not in 并掉]

with jv.Runtime(client=jv.FakeClient()) as rt:        # 测试用假客户端；真机用 jv.JevClient()
    rt.calib.put("crm.同人", hi=0.6, lo=0.3, n=50, status="上岗", set_id="demo")   # 线只从校准记录来
    print(名单合并([jv.lit("张三 <zs@a.com>")], [jv.lit("张三 华为 <zs@a.com>"), jv.lit("王五")]))
    print(jv.stats())                                 # 层数、每层题数、调用数、钱
```

跑六条规范示例与第七条 measure 示例：`cd 地基 && .venv/bin/python -m foundation.jv stats`。

## 1. 概念表：材料 → 状态 → 题 → 读数 → 出口

| 概念 | 类型 | 怎么来 | 怎么用 |
|---|---|---|---|
| **材料** | `jv.Mat` | 字面量 `jv.lit(x)`（**taint = trusted**，来源 `("lit",)`；`jv.mat` 同，但在程序帧内对非标量实参报 `W-literal-from-host`）；效应输出（`do` / `gen` / `transform` / `ask`）；出口也是材料的子类型 | `m.content`（原值）、`m.text()`（字符串）、`m.taint`（trusted / untrusted）。**Mat 不是字符串**：`"x" in m`、`m == "x"`、`for c in m` 都是类型错，要用 `m.content`。相等与哈希按内容（同内容同地址即相等，可进 set / dict） |
| **期物** | `MatFuture` | `jv.do(...)` 的返回值 | 惰性；读 `.content` / `.text()` 触发刷新。**直接 `return` 出程序会自动解析成 `Mat`**。同样不能 `in` / 迭代 |
| **状态** | `jv.State` | `jv.state(on=…, ctx=[…], ref=[…], over=[…])` | 一次调用看的全部材料（§2.1） |
| **题** | `jv.Q` | `jv.test` / `jv.select` / `jv.measure`，`calib=jv.calib("键")` 必填 | 一题一命题；同一个 Q 可在多个状态上复用 |
| **读数** | `Readings` / `ReadingsVec` | `jv.judge(state, q1, q2, …)` / `jv.judge([s1, s2, …], q)` | 惰性、不透明。只有 `.agg()`（同题跨运行）与 `.order()`（同题跨对象偏序，向量化才有）。任何比较 / 算术 / `if r:` / `match r` 都是 J-01 错 |
| **出口** | `jv.Exit` 族 | `jv.cut(读数)` | 见下表；属性 `.kind` / `.p` / `.detail` / `.consumed` / `.taint`，`Pick.k`、`At.level`、`Unsure.cause`。**只有 `match` / `jv.handle` / `jv.consume` 消费 `Unsure`**；裸 `isinstance(e, jv.Unsure)` 只是看一眼 |

**出口映射表**（G4 猜 7–12 的答案）：

| 题 | `jv.cut` 的出口 | 含义 | p 是什么 |
|---|---|---|---|
| `jv.test` | `jv.Act()` / `jv.Ignore()` / `jv.Unsure(cause)` | 是 / 否 / 拿不准 | 该题「是」的概率，与校准线 hi / lo 比 |
| `jv.select`（状态带 `over=`） | `jv.Pick(k)` / `jv.Unsure(cause)` | 选第 k 个候选，**k 是 `over` 的 0 起下标** | 众数候选的概率；两次置换众数不一致 → `Unsure("tie")` |
| `jv.measure(scale=…)` | `jv.At(k)` / `jv.Unsure(cause)` | 落在第 k 档，**k 是 `scale` 的 0 起下标**（`scale=("低","中","高")` 时 `At(2)` 是「高」） | 该档位的概率，与 hi 比；档位本身不参与过线 |

`cause` ∈ `band`（线附近；measure 时 `e.detail["nearest_level"]` 是最高概率档）、`tie`（候选并列）、`cold`（校准键无记录，带临时出口）、`insufficient`（题声明的证据槽缺）、`fail`（模型调用 / `do` / `transform` 失败——**失败一律是值，程序不崩**，J-12）、`budget`（超预算）、`noprogress`（循环无进展）、`drift`（校准停岗）、`taint`。

## 2. API 逐段

### 2.1 `jv.state(on, ctx=[], ref=[], over=[])` —— 槽的分工（猜 1）

- `on`：**被判的那一个对象**（恰一个；关系用 `on=(a, b)` 二元组）。题面默认指它。
- `ctx`：判断时要看的背景材料（目标描述、基线报告、另一条记录）。`ctx` 可含 untrusted 材料，但状态随之 untrusted（守卫见 §2.6）。
- `ref`：参照（锚点样例、标准、模板）。它和 `ctx` 一样渲染进状态（假客户端的 `text` 里有 `ref` 键）；区别只在题面怎么指它与校准键。
- `over`：候选集，只有 `select` 题用；`Pick(k)` 的 k 指它的下标。
- 进槽的东西只能是 `Mat` / 期物 / 出口；宿主函数的返回值要先经 `jv.transform`（J-11）。裸字符串进槽是错，用 `jv.lit`。在程序帧里用 `jv.mat(宿主算出来的东西)` 把它「洗」成字面量会报 `W-literal-from-host`（静态：实参不是常量也不是程序参数；运行期：非标量）。
- `select` 题的状态必须带非空 `over`；`over=[]` 在 `judge` 时报错（空集无题）。向量化 `jv.judge([], q)` 返回空向量，`cut` 得 `[]`。
- 「一对记录是不是同一个人」写 `on=甲, ctx=[乙]`；两条并列比较写 `on=(甲, 乙)`。

### 2.2 题：`jv.test` / `jv.select` / `jv.measure`

```python
jv.test("这段输出符合目标吗？", calib=jv.calib("cmd.成功"), evidence=("ctx",))      # evidence：J-09 决定性证据槽
jv.select("哪个修改最可能修好？", calib=jv.calib("fix.最可能"), prior=jv.prior.pass_count)
jv.measure("严重程度是？", scale=("低", "中", "高"), calib=jv.calib("log.严重"))     # scale：有序档位**标签**，≥ 2 档
```
- `calib` 必须是 `jv.calib("键")`；传字符串是 J-03 错。线（hi / lo）在校准记录里，不在程序里。
- 三档有序（低 / 中 / 高）用 `measure`；无序标签集（财务 / 技术 / 售后）用 `select` + `over=`。
- `measure` 下沉后的物理题型叫 `score`；`select` 下沉为 `choice`（两个置换同一次调用取众数）或 K 道 `noul`；`test` 是 `noul`。

### 2.3 `jv.judge` 与向量化（猜 2）

```python
r  = jv.judge(state, q1, q2)          # Readings：r[0] 是 q1 的读数，r[1] 是 q2 的
rs = jv.judge([s1, s2, s3], q)        # ReadingsVec：rs[i] 是 s_i 的 Readings；同层并发
rs = jv.judge([s1, s2], q1, q2)       # rs[i][j]
```
`judge` 只登记不发。**同一直线段（两个刷新点之间）登记的、同一个状态上的题，融合成一次调用**；多个状态同层并发。

**判断向量的两个方法**（G5 猜 11、12）：

```python
tiers = rs.order()        # [[0], [2, 3, 1], [4]]：下标的分组列表，组间从高到低，组内并列（读数差 ≤ δ）
r2 = rs.agg()             # 同题跨运行合并（band 重跑后用）；形状不变
```
- `.order()` 只对向量化读数有定义（单状态 `Readings` 上调它是 J-01 错）；按 p 排（measure 按档位再按该档 p）；**是刷新点**（读数未就绪就先发层）；**不消费**读数、不产生出口；失败（`fail`）的读数单独排最后一档。要「中风险按从高到低」：展平后过滤。
- 材料与出口都不带位置：条目编号用宿主 `enumerate`。

### 2.4 `jv.cut` —— 读数变出口

```python
e  = jv.cut(r[0])                      # 单题 → 一个出口
es = jv.cut(rs)                        # 向量化 → 出口**列表，顺序与输入状态一一对齐**（可直接 zip）
es = jv.cut([r[0] for r in rs])        # 也接受读数列表
```
`cut([])` 返回 `[]`。measure 落带时 `Unsure("band").detail["nearest_level"]` 给最高概率档，归档由你（handler）定。
`cut` 是刷新点：此前登记的所有就绪判断在这里按层发出。顺序：J-02 禁自指 → budget / fail → J-09 insufficient → 停岗（drift）→ 冷（`Unsure("cold")`，`e.detail["provisional"]` 带保守线的临时出口）→ 线 ± δ → select 的置换众数不一致 / K-noul 前二差 ≤ δ → `Unsure("tie")`。

### 2.5 出口的消费：`match` / `isinstance` / `jv.handle` / `jv.consume`（猜 17、22、23）

- **必须消费的只有 `Unsure`**。`Act` / `Ignore` / `Pick` / `At` 不消费也不报错。程序返回前有未消费的 `Unsure` → J-05 错，报文带程序名。
- **消费 `Unsure` 的只有三条路**：`match e: case jv.Unsure(): …`（绑不绑 cause 都算）、`jv.handle(e 或 cause, …)`、`jv.consume(exits, …)`。裸 `isinstance(e, jv.Unsure)` **不**消费——否则任何包装层一判种类 J-05 就失效。`Act` / `Ignore` / `Pick` / `At` 不需消费（`isinstance` 命中只作记账）。**消费是幂等的**：`case jv.Pick(k) if 条件:` 守卫失败落到下一个 `case jv.Pick(k)` 不算重复。
- `jv.consume(exits, unsure=jv.drop)`：批量消费；`Unsure` 按 `unsure=` 处理（`jv.drop` 记账丢弃 → 返回 `None`；`jv.escalate` 逐个问人，可能 `Pending`）；非 Unsure 原样返回。里面混 `Ignore` 没有副作用。
- `jv.handle(c, then=…, keep=…, regen=False)`：处理一个 `Unsure`。`c` 可以是 cause 字符串（取最近 `match` 命中的那个）或出口本身。按 cause 走 handler 库：
  - `band` → **同状态同题重跑一次**（`run_seq+1`，`.agg()` 后重切）——这会多发一层，G4 看到的「handle 之后多出层数」就是它；重跑仍拿不准才看 `then` / `keep`。
  - `cold` → 返回临时出口（保守线）；`budget` → 原样返回；
  - `then=jv.escalate` → 对同一状态同题 `jv.ask`（可能抛 `Pending`）；`then=jv.drop` / `None` → 返回 `None`；`then=函数` → 调它；
  - `keep=值` → 直接返回该值（「拿不准就当作 值」）；`regen=True` → 返回 `None`，由程序自己再 `gen`。
- 读数当出口 `match`（忘了 `cut`）在第一个 case 就抛 J-01，不会静默不命中。

### 2.6 `jv.do`、`jv.Action`、守卫与 taint（猜 15、16、18）

```python
跑测试 = jv.register_action("run_pytest", fn=_run, taint_out="trusted", reason="仓库自带测试框架，报告不含用户文本", cost=0.0)
发告警 = jv.register_action("alert", fn=_alert, taint_out="inherit", reversible=False, cost=0.001)
报告 = jv.do(跑测试, repo, iter_seq=0)                         # 惰性 MatFuture；同层多个 do 并发
jv.do(发告警, seg, iter_seq=i, guard=e)                         # 不可逆动作必带 guard
```
- `iter_seq`：**每次调用必给**。`jv.loop` 里用 `it.n`；普通 `for i, … in enumerate(…)` / `range` 里用循环变量 `i`；直线段用 `0`。循环里写常量序号是 J-13 错（参数含循环变量时降为 W-seq-const）。它进账本键：同站点同参数同序号 → 重放取账本、不重触世界。
- `Action.taint_out`：动作**输出**的可信级。`"trusted"`（可信执行器：测试框架、可信工具）、`"untrusted"`（沙箱跑不可信代码、网页正文）、`"inherit"`（∨ 参数的 taint）。
- **可信来自来源登记，不来自程序自报**：`taint_out="trusted"` 要用 `jv.register_action(..., reason="为什么可信")`（S 库登记）。程序模块里直接 `jv.Action(..., taint_out="trusted")` 会报 `W-self-trusted`（静态 + 运行期各一次）——G4 猜 16 把自家页面读取器标 trusted 绕守卫的做法，现在会被指出来。
- `guard=`：传出口（或出口列表）。放行条件（J-08）：至少一个是**来自 trusted 状态的 `jv.Act`**，或 `jv.ask` 的答案；untrusted 出口再多也不够。传 `Ignore` / `Unsure` / `Pick` / `At` → J-08 错并带修法（Pick/At 是选择或档位，不是命题；再问一道 test 题作守卫）。守卫里的出口一并算消费。
- `Action.cost`：每次执行计入当前程序帧的 `Budget.cost`，在层边界核。
- `do` 失败是值（`Fail`），进账本；依赖它的判断切成 `Unsure("fail")`；`jv.on_fail(期物, 替代)`。

### 2.7 `jv.gen`、`jv.transform`（猜 14）

```python
cands = jv.gen("按规格改代码", ctx=[spec, 报告], n=4, retry_seq=it.n)   # 一登记就发（慢），返回 [Mat]，taint = ∨ ctx
xs = jv.transform(f, m1, m2)                                        # 宿主函数变材料：返回 Mat 或 [Mat]，记账、核纯性
```
- `retry_seq` 与 `iter_seq` 同规则（普通 `for` 里用循环变量；常量会被 `W-seq-const` 提醒）。**`n` 是上限不是恰好**：返回 0..n 条，生成器超时 / 抛异常 → `[]` + `W-gen-fail`（失败是值）。`ctx=[]` 时输出 taint = trusted。
- 生成器怎么接：`jv.Runtime(generator=fn)` 或 `jv.gen(..., generator=fn)`，签名 `fn(prompt: str, ctx: list[Mat], n: int, retry_seq: int) -> iterable[内容]`，`ctx` 元素是 `Mat`（用 `.content`）；每个返回元素包成一个 `Mat`。
- **`jv.transform` 返回的列表元素是 `Mat` 不是 str**（G4 报错 4）：`"提交" in 动作[k]` 要写 `"提交" in 动作[k].content`。参数可含材料列表（键按元素哈希；宿主函数收到的是 `list[Mat]`）。宿主函数返回 `list[str]` → 逐个包成 `Mat`；返回 `[]` → `[]`。宿主函数抛异常 → 返回一个 fail 材料（`m.content == {"fail": …}`）+ `W-transform-fail`，进槽的判断切成 `Unsure("fail")`，`jv.on_fail(m, 替代)` 可换。同输入异输出 → `W-impure`，本直线段禁融合。

### 2.8 `jv.ask` → `Pending` → `jv.answer` → 重跑（猜 3、5、6）

```python
答 = jv.ask(jv.state(on=a, ctx=[b]), 同人)      # 有答案：返回出口（Act/Ignore/Pick/At，taint=trusted，p=1.0）；无答案：抛 jv.Pending
```
- `Pending` 是**程序级出口**：程序整体挂起并结束本次运行。`with jv.Runtime(...)` 外层捕它，`e.key` 是这条 ask 的键。
- 人答到达：`jv.answer(e.key, kind, k=None, level=None)`，`kind` ∈ `"act"` / `"ignore"`（test 题）、`"pick"` + `k=`（select）、`"at"` + `level=`（measure）。其他值直接报错。
- 然后**重跑同一个程序**：账本重放让此前的 `judge` / `do` / `gen` 不付费、不重触世界，`ask` 返回人答的出口。同一个 `Runtime` 里不需要 `root`（内存账本按程序名保留）；跨进程要 `jv.Runtime(root=目录)` 让账本落盘。
- `ask` 计入 `Budget.escalate`。
- **「攒起来问人」**（G5 猜 5）：`ask` 一次一题、一 Pending 即挂起，攒不成一批。批量交人写 `return {..., "问人": jv.escalate(列表)}`——`Escalated(payload, note)` 是普通返回值，透传出程序、不经 J-05；要每条都进校准集则 `jv.consume(exits, unsure=jv.escalate)`（逐条 ask，第一条就 Pending，恢复即重放）。

### 2.9 `jv.loop`、`jv.decreasing`、`it.n`（猜 13）

```python
for it in jv.loop(bound=8, variant=jv.decreasing(lambda: len(cands))):   # bound 与 variant 都必填（J-06）
    ... it.n ...                                                         # 本轮序号，给 iter_seq / retry_seq
```
- `variant` 是严格递减的宿主计量；某轮不降 → 停止并记一个已消费的 `Unsure("noprogress")` + `W-noprogress`。账本键在循环内重复也停。
- 变式 lambda 里读期物 `.content` 会触发刷新——它在每轮开头，通常就是你要的层边界；不会「拆散」本轮的判断。
- 不用 `jv.loop` 的普通 `for` / `while` 也可以，但那时进展与终止由你自己保证，`iter_seq` 用循环变量。

### 2.10 `jv.escalate`、`jv.Budget`、`@jv.program`（猜 19、20、21）

- `jv.escalate(x, note="")`：**返回** `jv.Escalated(x)` 值，不抛异常；计一次 escalate。作 `handle(then=jv.escalate)` 的标记时含义是「对该题问人」。
- `jv.Budget(calls, cost, layers, escalate, unsure)`：**五个字段默认 `None` = 不限**。
  - `calls`：融合后的模型调用次数上限；`cost`：美元（模型调用 + `Action.cost`）；
  - `layers`：**本程序帧里发出的 judge 层数**（一次 `cut` 通常一层；向量化一次 `cut` 也是一层；循环里逐个 `cut` 每轮一层；只刷新 `do` / 返回时的刷新不算层；`.order()` / `.agg()` / `jv.allocate` 若触发发层也算）；
  - `escalate`：`jv.ask` + `jv.escalate` 次数；`unsure`：返回时 `Unsure` 占出口的比例上限（超只 warn）。
  - 超 `calls` / `cost` / `layers` 在**层边界**核：那一层不发，读数记 `Unsure("budget")`，程序继续；超 `escalate` 直接抛。
- `@jv.program(budget=…)`：进入时静态检查一次（错即拒，报文 `J-xx: … 修法：…`）并做计划期估计（只告警）；返回前刷新、把返回值里的期物解析成 `Mat`、核 J-05。程序可以调用程序（§3.3）。

### 2.11 长处构件：`jv.allocate` / `jv.unsure_bound` / `jv.cut(cost=)` / `jv.fit`（examples/strength.py）

读数不是值，是带校准线的随机变量。四个构件各用它的一种性质，裸调用（拿到 p 就 `if p > 0.5`）写不出：

| 构件 | 用的性质 | 一句话 |
|---|---|---|
| `jv.allocate(rs, k) → [下标]` | 离决定带的距离 | 把 k 份复核（人、第二传感器、更贵的执行）分给最不确定的 k 条；k 通常取 `jv.budget().escalate`。带内 = 最不确定；带外按到带边的距离 |
| `jv.unsure_bound(rs) → {union_bound, independent_any, n_unknown}` | J-10 | 整批读数落入 unsure 的期望数：联合界 Σuᵢ 是上界，独立估计只作参考；uᵢ 取各题校准记录的 `unsure_rate`，无记录的按 1 计 |
| `jv.cut(r, cost=(fp, fn))` | 线随代价移动（§2.3） | 线由该校准键的**标注集**（`samples`，`label_set_id` ≠ 保形集 `set_id`）按代价矩阵算出：经验代价最小阈值（贝叶斯代价比线的经验版）；出口 `detail["cost_line"]` 带线与代价。只对 test 题有定义；无标注集时报 `W-cost-unfit` 用记录线 |
| `jv.fit(jv.fitref(名), r1, r2, …) → Score`，再 `jv.cut(score, calib=…)` | I3：跨题读数唯一合法的合成 | 只认注册表签名（J-16：指纹逐项相同、n ≥ max(50, 20×特征数)、训练集 ≠ 保形集）；程序里 `r1 * r2` 是 J-01 错 |

`jv.budget()` 返回当前 program 帧的 `Budget`（嵌套时是内层的）。三条示例（`python -m foundation.jv.examples.strength`）：按不确定性分配四份复核，错误率 0.333 → 0（随机抽同样四份：0.167）；同一批读数在 fn=10·fp 与 fp=10·fn 下线从 0.21 移到 0.65，12 条工单里 7 条出口不同；两题经注册 fit 合成后只合入「全绿且只改被测函数」的两条。

## 3. 执行模型（§6.0 的落法）

### 3.1 刷新点与层

`@jv.program` 内逐语句即时执行。`jv.judge` 只登记（返回惰性 `Readings`），`jv.do` 只登记（返回 `MatFuture`），`jv.gen` / `jv.ask` / `jv.transform` 立即执行（输入含期物时先刷新 `do`）。**刷新点** = `jv.cut`、`jv.fit`、读期物 `.content`、`gen/transform/ask` 的输入含期物、程序返回。一次刷新：先按依赖分波并发解析全部待执行 `do`；再把已登记且输入就绪的 `judge` 排成**一层**：下沉（test→noul；select→choice 两个置换融合进同一次调用，或档案未测档 / 长候选→K-noul；measure→score）、裂变（对象槽超窗按块切，合回 exists/all、K-noul 分块再决、档位计数）、融合（同结构哈希同段合一次调用，≤ 200 题）、层边界核预算、账本（先账本键，再跨程序缓存键，缺的才发）、并发发出。返回体经 `validate_answers` 校验（§4），键不合即报错。

**写法决定层数**（§8-10 要量的东西）：

```python
for x in xs:                                            # 每轮一层：融合率 0
    match jv.cut(jv.judge(jv.state(on=x), q)[0]): ...
es = jv.cut(jv.judge([jv.state(on=x) for x in xs], q))  # 一层：全部并发，同状态多题同一次调用
for x, e in zip(xs, es): match e: ...
```
同一状态上的两道题写在同一直线段里（中间没有 `cut` / `match` / `.content`）才融合；`取物` 示例的「到达」与「下一步」被 `match` 隔开，所以两层。

`cut` 顺序见 §2.4。J-05 的实现细节：CPython 对类型完全相等的 `isinstance` 走快路径不调 `__instancecheck__`，所以 `Act()` 实际构造隐藏子类 `ActImpl` 的实例；运行时内部一律用不消费的 `_is`。账本键里的题标识不含效应序号：同状态同题同站点 → 同键，同一运行时内第二次运行才真的重放。

### 3.2 七个 pass 的开关（消融）

`jv.Runtime(client, passes={"fuse": False, …})` 或命令行 `--no-fuse`。关掉后按规范 §4「不做会坏什么」退化：

| pass | 关掉后 | 证明它真关掉的测试 |
|---|---|---|
| `lift` | 每个 `judge` 登记即刷新：层数 = 判断数，同状态两题也不融合 | `test_switch_lift_off_*` |
| `fuse` | 逐题调用：调用数 = 题数 | `test_switch_fuse_off_*` |
| `fission` | 超窗对象不切、不报 W-window | `test_switch_fission_off_*` |
| `lower` | select 一律 K-noul、不置换；measure/test 不变 | `test_switch_lower_off_*` |
| `schedule` | 层内 do / 调用串行；调用数不变 | `test_switch_schedule_off_*` |
| `plan` | 不做计划期估计、不核层边界预算（超预算照发） | `test_switch_plan_off_*` |
| `ledger` | 不查不写账本 / 缓存：第二遍照发 | `test_switch_ledger_off_*` |

`jv plan`（J-07 计划期、J-10 静态上界）：`@jv.program` 进入时自动跑一次，告警进 `rt.stats["warnings"]`；也可 `python -m foundation.jv plan 模块:函数`。层数是**上界**：一条语句里的多个 `cut` 记一层；循环里 cut 循环外登记的向量按登记处倍率算。unsure 上界 Σuᵢ，未标注的键留符号。

### 3.3 嵌套程序（程序调用程序）

`@jv.program` 可以调用 `@jv.program`。最外层 `begin` 才重置统计、开账本、写账本头；内层只压一个**子账帧**（`_Frame`），复用外层的 pending 队列、账本、料库，所以外层登记而未刷新的 `judge` 会和内层的判断同层融合。

- **预算是子账**：内层 `Budget` 只核自己帧内的调用 / 层 / 钱 / escalate，同时计入外层总数。某帧超 → 该帧及其内层登记的判断停并记 `Unsure(budget)`，外层的判断照发；最外层超 → 整层停。
- **J-05 按作用域**：内层返回前只核本帧登记的 `Exit.consumed`，报错带内层函数名。
- **返回值进槽**：内层返回的 `Mat` / `Exit` / 期物 / 材料列表可直接放进外层的槽（J-11 合法）。被调名解析到 `@jv.program` → 合法；普通宿主函数 → J-11 错；编译期不可见（参数、局部名、方法）→ `W-dynamic` 一次，运行期仍按 J-11 核。
- 每帧结束写 `rt.stats["frames"]`。

## 4. 假客户端与 rule 的格式（猜 9、10、24）

`jv.FakeClient(rule=fn)`，`rule(text, qid, q) -> 答案字典 | None`（None 落到默认规则）：

- **`text` 是状态渲染成的 JSON 字符串**：`{"on": …, "ctx": [...], "ref": [...], "over": {"c0": 候选0原文, "c1": …}}`（关系时 `"on": {"a": …, "b": …}`）。`json.loads(text)` 后按键取，不要当纯文本切。
- `q` 是物理题：`{"type": "noul" | "choice" | "score", "instructions": 题面, "criteria": …}`。choice 的 `criteria` 是 `{"c0": 候选原文, …}`（置换后顺序不同但键不变）；score 的 `criteria` 是 `scale` 的标签列表。
- 三种返回体（**键错了立即 JvError 带修法**，不再静默变成全 Unsure）：

| 题型 | 返回体 |
|---|---|
| noul | `{"type": "noul", "noul": p}`，p ∈ [0, 1] |
| choice | `{"type": "choice", "choice": "c1", "probabilities": {"c0": 0.1, "c1": 0.9}}`，键 = `criteria` 的键 |
| score | `{"type": "score", "score": 2.0, "probabilities": {"0": 0.05, "1": 0.05, "2": 0.9}}`，`score` 是**档位下标**的数，`probabilities` 的键是 `"0".."n-1"`（int 键也接受，自动转 str） |

真机 `jv.JevClient()` 走 `clients.eye_client`（密钥只从 `~/.typesafe-key`），返回体过同一校验。

## 5. 数字：示例程序的层数与融合率（§8-10）

`python -m foundation.jv stats`（FakeClient，全开；六条规范示例 + 第七条 measure 示例）：

| 程序 | 层数 | 每层题数 | 每层调用 | 题 | 调用 | 融合率 | 说明 |
|---|---|---|---|---|---|---|---|
| 定位回归 | 4 | 1,1,1,1 | 1,1,1,1 | 4 | 4 | 1.0 | 二分：每轮一题一层，天然串行；返回 c-5 |
| 生成并执行 | 1 | 4 | 4 | 4 | 4 | 1.0 | 4 个状态向量化一层并发 |
| 生成到全绿 | 4 | 2,2,2,1 | 1,1,1,1 | 7 | 4 | 1.75 | select 两置换同调用；每轮一层 |
| 取物 | 7 | 1,2,1,2,1,2,1 | 1 | 10 | 7 | 1.43 | 到达题与下一步题**不同层**：同状态但中间隔了 `match` |
| 工单转部门 | 1 | 6 | 3 | 6 | 3 | 2.0 | 3 状态 × 2 置换，一层 |
| 写 docstring | 4 | 3,2,2,0 | 1,1,1,0 | 7 | 3 | 2.33 | for 里逐个 `cut` → 每函数一层；规范预算 layers=3 不够，第 4 层被预算停 |
| 日志分级（第七条，measure） | 1 | 8 | 4 | 8 | 4 | 2.0 | 4 段 × (measure + test 守卫题) 同状态同层：每段一次调用两题；告警 `Action.cost` 计入钱 |
| 分配复核（长处 1） | 1 | 12 | 12 | 12 | 12 | 1.0 | 12 段向量化一层；`allocate` 与 `unsure_bound` 不花调用 |
| 代价比线（长处 2） | 1 | 12 | 12 | 12 | 12 | 1.0 | 同一批读数两套 `cut(cost=)`，不多花一次调用 |
| 自动合入（长处 3，fit） | 1 | 10 | 5 | 10 | 5 | 2.0 | 每状态两题一次调用，fit 在桥里合成 |
| 合计（七条） | 22 | | | 46 | 29 | **1.59** | 六条第二遍（`--replay`）：调用 0，账本命中 38 |

`python -m foundation.jv ablate`（六条规范示例合计；每个 pass 单独关）：

| 关掉的 pass | 层数 | 题 | 调用 | 融合率 | 停层 | 说明 |
|---|---|---|---|---|---|---|
| （全开） | 21 | 38 | 25 | 1.52 | 1 | 基线 |
| lift | 21 | 38 | 25 | 1.52 | 1 | 六条示例里没有「同段两次 judge」的写法，关提升无变化（起作用的形态见 `test_switch_lift_off_*`） |
| fuse | 21 | 36 | 36 | 1.0 | 1 | 逐题调用：调用数 = 题数（E8：成本 +45%） |
| fission | 21 | 38 | 25 | 1.52 | 1 | 示例里无超窗对象 |
| lower | 14 | 35 | 18 | 1.94 | 1 | select 一律 K-noul、不置换：置换题消失；K-noul 与前一题同状态融合，层数 21 → 14 |
| schedule | 21 | 38 | 25 | 1.52 | 1 | 层内串行：调用数不变，只慢 |
| plan | 21 | 40 | 26 | 1.54 | 0 | 不核预算：写docstring 第 4 层照发 |
| ledger（第二遍） | | | 全开 0 次（命中 38）；关 ledger 25 次 | | | 重放不付费 vs 每次都发 |

`jv plan` 对六条示例的告警：`生成到全绿` W-cost（层上界 7 > layers=6）；`取物` W-cost（40 > 20）；`工单转部门` W-cost（ask 次数 `|工单流|` 含符号）；`写docstring` W-cost（`|fs| + 1` 含符号）；含 select 的四条各两条 W-untested（候选 120–250 档未测；置换同调用串扰未测）。21 条程序（`examples/twentyone.py`）的融合率与 §6.3 拦截率见 `examples/STATS.md`（126 / 126）。

## 6. 包结构

| 文件 | 行 | 内容 |
|---|---|---|
| `ir.py` | 710 | 六形式的数据：`Mat`（相等 / 哈希按内容；`in` / 迭代 / 与裸值比较是类型错）、`Q`、`State` / `ResolvedState`（JSON 具名槽渲染、结构哈希、taint=∨、derived_from=∪）、`Reading` / `Readings` / `ReadingsVec`（只有 `.agg()` `.order()`）、`Exit` 族（`consumed` 标记，match/isinstance 命中即消费；收到读数抛 J-01）、`Fail`、`Pending`、`CalibRef`、`FitRef`、`Action` + `register_action` / `ACTIONS`、`Budget` |
| `effects.py` | 62 | `JudgeEffect`、`DoEffect`、`MatFuture` |
| `runtime.py` | 1212 | 刷新点与层；七个 pass；嵌套帧 `_Frame`；`cut`；`gen` / `ask` / `answer`；`transform` 记账与纯性核；`loop`；handler 库；`fit` 桥库；`do` 的 `Action.cost` 进预算、W-self-trusted；返回值期物解析；`stats_report()` |
| `checker.py` | 345 | AST 静态检查：J-01/02/03/06/08/11/13/14/16 与 §6.3 六种模式（J-17）；解包追踪；`W-dynamic`；`W-self-trusted` |
| `plan.py` | 544 | `jv plan`：符号成本签名、层上界、Σuᵢ、六类告警 |
| `client.py` | 147 | `JevClient`、`FakeClient`、**`validate_answers`**（三种题返回体校验 + 规范化，真机与假客户端共用） |
| `calib.py` | 102 | 校准记录（四态）与 fit 注册表 |
| `store.py` | 106 | 账本键 / 缓存键 / 账本头三表；效应账本；料库 |
| `__init__.py` | 198 | 构建器 API（§2）+ `register_action` / `answer` / `validate_answers` / `stats` |
| `__main__.py` | 90 | `plan` / `check` / `stats` / `ablate` |
| `examples/six.py` | 287 | §6.1 六条程序原文 + 假 S 库（trusted 动作经 `register_action` 登记）|
| `examples/seven.py` | 89 | 第七条：日志分级（measure + 守卫 + 不可逆动作 + `Action.cost`） |
| `examples/twentyone.py` | 703 | 21 条程序（11 §9 18 条 + G3 3 条） |
| `examples/fresh4.py` | 172 | G4 零上下文读者写的三条（原样保留；现在会报 W-self-trusted） |
| `examples/smoke_e_ir.py` | 63 | E-IR-SMOKE 真机冒烟 |
| `../tests/test_jv_core.py` | 546 | 31 条：J 规则、融合 / 裂变 / 下沉、账本重放、Pending、预算、冷校准 … |
| `../tests/test_jv_passes.py` | 529 | 41 条：七个开关、裂变、`Sym` / `jv plan`、嵌套程序、D1–D3 |
| `../tests/test_jv_fresh.py` | 249 | 15 条：G4 五处报错各一条（报文带修法）、Mat 相等性、期物返回、`register_action` / W-self-trusted、`Action.cost` 进预算、`answer` 无 root 重放、守卫失败幂等、escalate 返回值、第七条示例 |
| `../tests/test_twentyone.py` | 278 | 169 条：21 条跑通 + 126 个 §6.3 变体拦截 |
| `../core/DEPRECATED.md` | | `beat/pack/arbiter` 标废弃不删；`item/log/table/canon/ledger/eye/registry` 沿用 |

## 7. 与规范 v0.1 的偏差表（提议，依据文本只 Nature 改；最重要在前）

| # | 规范处 | 实现处 | 提议 |
|---|---|---|---|
| 1 | §6.1 `写docstring` 预算 `layers=3`；§6.0「同一直线段内的多个 judge 自动同层融合」 | for 循环体内逐个 `cut` 是刷新点，每函数一层，实测 4 层；`取物` 同状态两题也被 `match` 隔成两层 | §6.0 加一句：融合只在**两个刷新点之间**登记的 judge 之间发生；示例改成先收集句柄再 `cut` 向量，或 budget 改 `layers=fs+1` |
| 2 | §3 J-05「被 match、handler、返回类型消费时置真」 | Python 无返回类型；只有 match / isinstance / handle / consume 四条路；且 CPython 快路径迫使出口用隐藏子类实例 | J-05 删「返回类型」；加「宿主若无 __instancecheck__ 钩子则由 consume 显式消费」 |
| 3 | §4.4 下沉「置换 ≥ 2 取众数」未说是否同一次调用；criteria 内容未定 | 两个置换作为两道题**融合进同一次调用**（同状态）；criteria 描述放候选原文（一跳，H4） | §4.4 写明：置换题同调用（choice 同调用串扰**未测**，列入档案待测项 E-PERM-SAME-CALL） |
| 4 | J-13「循环里常量序号是错」 | §6.1 `写docstring` 自身在 for / 列表推导里用 `iter_seq=0`；实现降为 `W-seq-const`，无循环变量参与时仍是错 | J-13 措辞改「常量序号且参数不含循环变量」 |
| 5 | §2.3 `cut(r, cost=(fp, fn))` 线由保形风险控制求出 | 记录无 `cost_matrix` 时忽略 cost 并报 `W-cost-unfit` | 写明 cost 只在校准记录带代价矩阵时生效 |
| 6 | §1「凡是数字，都是档案字段」；§5 handler `cold → 保守线 + 标记` | 保守线从档案 `lines.safety_default` 取（值仍是 v0 常数 0.75/0.25，`n=0`，标「待 E-CAL 定」） | 档案 SCHEMA 已加；由 E-CAL 定值 |
| 7 | J-07 符号成本签名、计划期估计；J-10 静态 unsure 上界 | `jv plan` 做了；层数是**上界** | §4-6 写明「层数估计是上界」 |
| 8 | §4.3 裂变三种 op | 已实现（test exists/all；select 分块 K-noul；measure 档位计数） | — |
| 9 | §2.6 `ask` 与人答通道 | `Pending` 抛出、恢复即重放；人答 `jv.answer(key, kind, k=, level=)` 写效应账本；无 root 时内存账本按程序名保留；无真实人机界面 | 第 7 步校准库接延迟真值 |
| 10 | §6.0「`gen`/`ask` 一登记就发」 | 成立；`gen` 的 ctx 含 `do` 期物时会先刷新 do（提前拆层） | 与 §11「惰性执行下的刷新点语义」同一待量项 |
| 11 | §6.1 `定位回归` 原文 `mid = cands[len(cands) // 2]` | 剩两项且 mid 慢时不缩小；`six.py` 改 `(len-1)//2`，规范原文不动 | §6.1 该行改 `(len(cands) - 1) // 2` |
| 12 | §5 handler `noprogress → 退出记账`；§2.7 变式 | `jv.loop` 无进展 / 键重复时显式登记已消费的 `Unsure(noprogress)` + `W-noprogress` | §2.7 写明 |
| 13 | §6.3 模式 1 | 扩到「任何读数（含向量元素）出现在 match/isinstance 位置」；拦截率 126/126 | 模式 1 定义扩写 |
| 14 | §2.8 `transform(f, args: [Mat])` | 参数可含材料列表，键按元素哈希 | §2.8 写明 |
| 15 | J-07 与嵌套 | 内层 Budget 是外层的子账 | J-07 加一句 |
| 16 | J-05 与嵌套 | 作用域 = 本 program 帧 | J-05 加「作用域 = 本 program 帧」 |
| 17 | §4-6 预算 pass | 层数估计是上界 | 写明 |
| 18 | §1 数字都是档案字段 | `lines.safety_default`、`batch_invariance.choice_same_call_perm_crosstalk` 两字段已加（未测） | — |
| 19 | §2.11 taint 代数「`do` 由动作声明」 | 声明可以由程序自己写，等于自封可信（G4 猜 16 的绕法）。实现：`taint_out="trusted"` 只应经 `jv.register_action(..., reason=)` 登记；程序模块里自声明报 `W-self-trusted`（warn，不拒） | §2.11 加一句：「trusted 只能由 S 库登记给出（带理由），程序内声明的动作 taint_out 上限为 inherit」；是否升为错由 Nature 定（与 I6「trusted 由来源给出」一致） |
| 20 | §2 `Mat` 定义无相等性 | 相等 / 哈希按内容 + 地址 + 模态 + 渲染版本，不看来源链与 taint；`in` / 迭代 / 与裸值 `==` 是类型错 | §2 写明 `Mat` 的相等性（同内容即同材料，来源链是元数据） |
| 21 | §2.5 期物 | 程序返回值里的期物自动解析成 `Mat`（不是 content：保留来源链与 taint，外层程序可直接进槽）；`isinstance(期物, Mat)` 仍为假，`jv.MatLike` 导出 | §2.5 写明「程序返回时期物已解析」 |
| 22 | §2.2 `judge` / §2.3 `cut` | `jv.cut` 接受读数列表（不只 `ReadingsVec`），出口列表与输入顺序一一对齐 | §2.3 写明向量化 cut 的返回顺序 |
| 23 | §2.5 `do` 画像里的 `cost` | 原实现不计入任何预算；现每次执行计入帧 `Budget.cost`，层边界核 | §3 J-07 写明「do 的 cost 计入 budget.cost」 |
| 24 | §2.6 `ask` 人答 | 客户端返回体（真机与假）统一经 `validate_answers` 校验：noul p 越界、choice 键非选项、score 用标签或 probabilities 按标签键，都立即 JvError 带修法（G4 报错 1/3/5 原先静默成全 Unsure） | §2.2 写明物理题返回体的形状是类契约的一部分（I1 的接口面），键不合是错不是 Unsure |
| 25 | §5 handler `band → 重跑/置换收窄` | `handle("band")` 重跑一次会多发一层（G4 猜 17 看到的「handle 后多出层数」） | §5 写明「重跑是一层」 |

| 26 | §7 能做域表「判断力花在哪：判断向量 + unsure 上界 + `allocate` 组合子」；§5 组合子表未列 `allocate` | `jv.allocate(rs, k)` 已实现为运行时组合子：按「离决定带的距离」排序取前 k；返回宿主下标，不返回读数 | §5 组合子表加 `allocate`（位/索引/档之外的第四个：不确定度 → 复核槽）；写明它读校准线、不做跨题算术，因此不违反 J-01 |
| 27 | J-10「有标注集时用经验联合率；无则联合界 Σuᵢ」 | `jv.unsure_bound` 只做联合界与独立估计；无 `unsure_rate` 的题按 1 计（最保守）；「经验联合率」需要多题联合标注集，未实现 | J-10 写明「无联合标注集时上界按未知题计 1」 |
| 28 | §2.3「给 cost 时线由保形风险控制在标注集上按代价求出」 | 实现为**经验代价最小阈值**（贝叶斯代价比线的经验版），无有限样本修正；校准记录加 `samples`（标注集）与 `label_set_id`，要求 ≠ `set_id`；只对 test 题定义 | §2.3 写明「代价线 = 标注集上经验代价最小阈值，保形修正在校准库（第 7 步）」；select/measure 的代价线待定义 |
| 29 | §2.9 注册约束 1「输入读数指纹逐项相同」 | `_fp_kind` 只区分 test / select·K / measure·档数，不含题面哈希；同 calib 键即视为同题 | §2.9 写明「指纹 = calib 键 + 题型指纹种类」 |
| 30 | J-05「被 match、handler 消费」 | 裸 `isinstance(e, jv.Unsure)` **不再**消费（Codex 评审：包装层一判种类 J-05 失效）；`match` 靠调用帧操作码（MATCH_CLASS）与 `isinstance` 区分 | J-05 写明「消费 = match / handle / consume；isinstance 只是观察」 |
| 31 | J-11 材料来源 | `jv.mat` / `jv.lit` 在程序帧内对宿主计算结果报 `W-literal-from-host`（静态：实参非常量非参数；运行期：非标量）——只 warn，因为区分「字面量」与「宿主算出来的字面量」静态不可判 | J-11 补一句「字面量只指源码常量与程序参数；其余进槽经 transform」 |
| 32 | J-12「任一子表达式 Fail 则整体 Fail」 | 客户端 / 生成器 / `do` / `transform` 抛异常一律变值：`Unsure(fail)` / `[]` / Fail 材料；`.order()` 把失败读数排末档。G5 猜 23 的崩溃是实现漏洞，已修 | 无（按规范原意实现） |
| 33 | §2.3 measure 的 `band` | `Unsure("band").detail["nearest_level"]` 给最高概率档；归档由 handler 定 | §2.3 measure 行加 `nearest_level` |
| 34 | J-08 守卫 | `Pick` / `At` 不能作守卫（不是命题），报文指路再问 test 题 | J-08 写明「合取项 = test 题的 Act」 |
| 35 | §2.6 `ask` 与批量 | `ask` 一次一题；批量 = `escalate(列表)` 返回值或 `consume(unsure=escalate)` | §2.6 加「批量问人」一句 |
| 36 | J-07 `Budget.layers` | 只数 judge 层；只刷新 do 的那次不算层 | J-07 写明「层 = 一次融合调用的边界」 |
| 37 | §2.1 `over=[]` | select 题 `over=[]` 在 judge 时报错（I1：模型只在给定集合上分配概率） | §2.1 加一句 |

其余按规范：taint 代数（gen=∨ctx、do 按声明、ask=trusted、transform=∨args、state=∨slots、cut 继承）；J-08 守卫要求 trusted 的 Act 或 ask 答案；J-02 运行期查 `derived_from`、静态追名字；缓存键含槽结构哈希、perm_seed 在账本键、模型 id 用档案 `model_version`；账本头不同报 `W-header`。

## 8. G4 二十四条猜点 → 本文哪一句

| 猜 | 问题 | 答案在 |
|---|---|---|
| 1 | on / ctx 分工 | §2.1 |
| 2 | 向量化 cut 返回列表且顺序对齐 | §2.4 第二行；§2.3 |
| 3 | ask 重放后返回什么 | §2.8 第一行：出口，taint=trusted |
| 4 | Mat 能否进 set / `==` | §1 材料行；偏差 20 |
| 5 | 人答怎么给、要不要 root | §2.8：`jv.answer`，同一 Runtime 不要 root |
| 6 | `answer` 签名 | §2.8：`jv.answer(key, kind, k=, level=)` |
| 7 | 三档用 measure 还是 select | §2.2 第二条 |
| 8 | `scale` 是标签还是区间 | §2.2 代码注释：有序档位标签 |
| 9 | FakeClient 对 score 的返回体 | §4 表 |
| 10 | score 数值域、probabilities 键 | §4 表：档位下标；键 `"0".."n-1"`；错了立即报错 |
| 11 | measure 出口是 At(k)，k 是下标 | §1 出口映射表 |
| 12 | hi / lo 对 measure 的意义 | §1 出口映射表 p 列：该档概率与 hi 比 |
| 13 | loop 必带 variant；lambda 里读 .content 是否拆层 | §2.9 |
| 14 | transform 返回 Mat 不是 str | §2.7；`in` 报错带修法 |
| 15 | `guard=` 传什么 | §2.6 guard 条 |
| 16 | 页面树 taint、自封 trusted | §2.6：W-self-trusted，`register_action(reason=)` |
| 17 | `handle(keep=None)`；handle 会重判发层 | §2.5 handle 条 |
| 18 | 普通 for 里 iter_seq | §2.6 iter_seq 条 |
| 19 | Budget 字段各数什么 | §2.10 |
| 20 | escalate 抛还是返 | §2.10 第一条：返回 Escalated |
| 21 | MatFuture 能否直接 return | §1 期物行：自动解析成 Mat |
| 22 | match 守卫失败后的消费 | §2.5：幂等 |
| 23 | Ignore 要不要消费 | §2.5 第一条：只有 Unsure |
| 24 | rule 收到的 text 格式 | §4 第一条 |

## 9. API 契约表（每个公开名字七列；每格由 `tests/test_jv_contract.py` 一条用例盯住；「未定」格见 设计/G45-猜点分类.md）

| 名字 | 输入 | 输出 | 空输入 | 输出 taint | 刷新点 | 消费 | 失败时 |
|---|---|---|---|---|---|---|---|
| `jv.lit(x, addr)` / `jv.mat` | 任意值 | `Mat` | `""` 合法 | trusted | 否 | — | `mat` 在帧内收非标量 → `W-literal-from-host` |
| `jv.state(on, ctx, ref, over)` | `Mat` / 期物 / 出口 | `State`（惰性；`.resolved()` 后有 `.taint`） | 缺 `on` → `JvError`；select 的 `over=[]` → judge 时 `JvError` | ∨ 各槽 | 否 | — | 裸字符串进槽 → `JvTypeError` |
| `jv.test` / `jv.select` / `jv.measure` | 题面 + `calib=jv.calib(键)` | `Q` | — | — | 否 | — | `calib` 是字符串 → J-03 |
| `jv.judge(s, *qs)` / `jv.judge([s…], q)` | 状态(列表) + 题 | `Readings` / `ReadingsVec`（惰性；无算术、无比较、无 bool） | `judge([], q)` → 空向量 | — | 否（登记） | — | 模型调用抛异常 → 读数记 fail → `cut` 得 `Unsure("fail")` |
| `jv.cut(r)` / `jv.cut(rs)` | 读数 / 读数向量 / 列表 | `Exit` / `Exit` 列表（与输入顺序对齐） | `cut([])` → `[]` | 继承状态 taint | **是** | `Unsure` 必须被 match / handle / consume 消费，否则返回时 J-05 | `calib` 传字符串 → J-03；fail 读数 → `Unsure("fail")` |
| `Readings.order()` | 向量化读数 | `list[list[int]]`（组间高→低，组内并列） | 空向量 → `[]` | — | **是** | 否（不产生出口） | 失败读数排末档；单状态 `Readings` 上调 → J-01 |
| `Readings.agg()` | 读数 | 同形状读数（跨运行合并） | — | — | **是** | 否 | — |
| `jv.gen(prompt, ctx, n, retry_seq)` | 提示 + 材料列表 | `list[Mat]`，0..n 条（**n 是上限**） | `ctx=[]` 合法 | ∨ ctx（空 ctx → trusted） | 一登记就发；输入含期物先刷新 do（不计层） | — | 生成器抛异常 → `[]` + `W-gen-fail`；缺 `retry_seq` → J-13 |
| `jv.do(action, *args, iter_seq, guard)` | 登记动作 + 材料 | `MatFuture`（`.content` 触发刷新；作返回值解析成 `Mat`） | — | 按 `Action.taint_out`（inherit = ∨ args） | 否（登记；读 `.content` 刷新，**只刷新 do 不算层**） | 守卫里的出口算消费 | 动作抛异常 → Fail 材料（`jv.on_fail` 换）；缺 `iter_seq` → J-13；不可逆无 trusted Act 守卫（含 Pick/At/Ignore/Unsure）→ J-08 |
| `jv.ask(s, q)` | 状态 + 题 | 出口（trusted，p=1）或抛 `Pending` | — | trusted | 是 | — | 超 `Budget.escalate` → J-07 |
| `jv.answer(key, kind, k, level)` | Pending 的键 + `act/ignore/pick/at` | 无（写账本） | — | — | 否 | — | kind 不合法 → `JvError`；键不在账本 → `KeyError` |
| `jv.transform(f, *args)` | 宿主函数 + 材料 / 材料列表 | `Mat` 或 `list[Mat]`（列表逐个包） | `f` 返回 `[]` → `[]` | ∨ args | 输入含期物先刷新 do | — | `f` 抛异常 → fail 材料 + `W-transform-fail`；同输入异输出 → `W-impure` |
| `jv.loop(bound, variant)` | 上界 + 递减变式 | 迭代器，`it.n` | — | — | 变式里读 `.content` 会刷新 | 无进展 → 已消费的 `Unsure("noprogress")` + `W-noprogress` | 缺 `bound`/`variant` → J-06 |
| `jv.handle(c, then, keep, regen)` | cause 或出口 | 出口 / `None` / `keep` 值 / then 的返回 | — | — | `band` 重跑一层 | **消费** | `then=jv.escalate` 可抛 `Pending` |
| `jv.consume(exits, unsure=drop\|escalate)` | 出口列表 | 同长列表（Unsure→`None`，其余原样） | `[]` → `[]` | — | 否 | **消费** | `unsure=jv.escalate` 可抛 `Pending` |
| `jv.escalate(x, note)` | 任意值 | `Escalated(payload, note)`（普通返回值，透传出程序） | — | — | 否 | 不经 J-05 | 超 `Budget.escalate` → J-07 |
| `jv.on_fail(expr, alt)` | 期物 / 材料 | `expr` 或 `alt` | — | — | 解析期物 | — | — |
| `jv.Budget(calls, cost, layers, escalate, unsure)` | 五个可选上限 | 值 | 全 `None` = 不限 | — | — | — | 超 calls/cost/layers 在层边界：整层不发、读数记 `Unsure("budget")`；超 escalate 抛 J-07；超 unsure 只 `W-unsure` |
| `@jv.program(budget, check_static)` | 函数 | 包装函数 | — | — | 返回前刷新 | 返回前核 J-05（本帧） | 静态错 → `JvError`（报文带修法）；`Pending` 原样抛 |
| `jv.FakeClient(rule)` | `rule(text, qid, q)` → 答案字典 / `None` | `(answers, tokens, cost)` | — | — | — | — | 返回体键不合 → `JvError`（不静默）；`rule` 抛异常 → 该层读数 fail |
| `jv.register_action(name, fn, taint_out, reason, …)` | 登记项 | `Action`（registered=True） | — | 由 `taint_out` 决定输出 | — | — | `trusted` 无 `reason` → `JvError` |
| `jv.Action(...)` 直接构造 | 同上 | `Action`（未登记） | — | 同上 | — | — | `taint_out="trusted"` → `W-self-trusted` |
| `jv.Act/Ignore/Pick(k)/At(level)/Unsure(cause)` | — | 出口；`.kind .p .detail .consumed .taint .as_mat()` | — | 由 cut 给 | — | 只有 `Unsure` 必须消费 | `e == jv.Act` → `W-cmp-type` 且恒假；非法 cause → `JvError` |
| `jv.Mat` | — | 相等 / 哈希按内容 | — | — | — | — | `bool(m)` / `len(m)` / `x in m` → `JvTypeError` |
| `jv.allocate(rs, k)` | 单题向量化读数 + k | `list[int]`（最不确定的前 k 个下标） | 空向量 → `[]` | — | 是 | 否 | `k < 0` → `JvError` |
| `jv.unsure_bound(rs)` | 读数 | `{union_bound, independent_any, n_unknown}` | — | — | 是 | 否 | — |
| `jv.budget()` / `jv.stats()` | — | 当前帧 `Budget` / 统计字典 | — | — | 否 | — | 无运行时 → `JvError` |
| `jv.fit(ref, *rs)` | `jv.fitref(名)` + 读数 | `Score`（仍要 `cut`） | — | — | 是 | — | 未注册 / 指纹不同 / n 不足 → J-16 |

「未定」的格：`Readings.agg()` 的空输入与失败列；`jv.ask` 的空输入；`jv.fit` 的 taint。它们对应的语义在 设计/G45-猜点分类.md「语义未定」表里没有条目，是实现上尚无路径可达，不是规范缺口。

## 10. 怎么跑

```
cd 地基 && .venv/bin/python -m pytest foundation/tests -q                     # $0，451 条
.venv/bin/python -m foundation.jv stats [--no-fuse …] [--replay --root DIR]   # 六条 + 第七条：层数 / 融合率 / 账本命中 / 钱
.venv/bin/python -m foundation.jv ablate                                      # 七个开关消融表（六条）
.venv/bin/python -m foundation.jv plan foundation.jv.examples.six:取物        # 计划期估计（J-07 / J-10）
.venv/bin/python -m foundation.jv check foundation.jv.examples.six:取物       # 静态检查
.venv/bin/python -m foundation.jv.examples.seven                              # 第七条（measure）
.venv/bin/python -m foundation.jv.examples.fresh4                             # G4 读者的三条（现在带 W-self-trusted）
.venv/bin/python -m foundation.jv.examples.fresh5                             # G5 读者的三条
SMOKE_TAG=e-ir-smoke-c .venv/bin/python -m foundation.jv.examples.smoke_e_ir  # 真机，≈$0.0001，先预注册
```
