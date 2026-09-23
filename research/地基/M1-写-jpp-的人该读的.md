# 写 `.jpp` 的人该读的

2026-09-21。作者：一个当天第一次见到这门语言、只读了 `crates/jpp-core/INTERFACE.md`、
`examples/*.jpp`、`examples/fixtures/*.json` 和 `cargo run` 报错的人。**写完为止没有读过任何 `.rs`。**

**这份文件的用处**：`INTERFACE.md` 是给**接线的人**（Rust 侧 lower、CLI）写的，它讲的是
`pub struct Program`、`Value` 枚举、`Client` trait。你要写的是 `.jpp` 源码，那份文件里
**没有一行 `.jpp` 语法**。这份文件补那一段。

**这份文件的纪律**：每一条都是我跑出来的。没跑过的标 **〔未验证〕**。有一节专门写
「我知道我不知道」。**不要把这份文件当规范——它是一份实测记录**，与依据文本冲突时以依据为准。

---

## 〇、先跑起来（五分钟）

```
cargo run -p jpp-cli -- --help
```

三行，比 `INTERFACE.md` 的 1520 行对你有用：

```
  jpp parse <file.jpp> [--ast]
  jpp check <file.jpp>
  jpp run <file.jpp> [--fixtures <file.json>] [--output <report.json>]
          [--ledger-out <ledger.json>] [--replay <ledger.json> | --resume <ledger.json>]
```

已登记的动作只有三个：`record_check`、`read_json(path)`、`write_json(path,value)`。
`run` 走固定观察，**不发任何真实模型请求**。

最小的能跑的程序：

```jpp
budget {calls: 0, cost: 0, depth: 64};
1 + 1
```

```
cargo run -p jpp-cli -- run hello.jpp
```
（没有效应的程序**不需要** `--fixtures`。）

**输出读法**：告警走 stderr（先打出来），结果是一个 JSON 对象走 stdout。你最常看的四个键：
`status`（`returned` / `pending`）、`value`、`cost`（`calls/replayed/asks/usd`）、
`trace.warnings`。还有一个容易漏掉但很重要的：`returned_unsure`——
**它非空意味着你把未决责任带出了程序**。

**不在项目目录里跑**：`do("write_json", …)` 按**进程工作目录**写文件，`--output` / `--ledger-out`
也是。用 `--manifest-path` 从你自己的目录调 CLI：

```
cargo run -q --manifest-path <repo>/rust-jpp/Cargo.toml -p jpp-cli -- run my.jpp --fixtures fx.json
```

---

## 一、`.jpp` 语法

全部从 `examples/*.jpp` 反推 + 实测。**除非标注，下面每一条我都跑过。**

### 程序骨架

```jpp
import "../lib/methods.jpp";          // 可选，可多条，必须在 budget 之前
budget {calls: 10, cost: 0, depth: 256, escalate: 2};   // 必需，分号结尾
fn helper(x: Int) -> Int !{} { x + 1 }                  // 语句
let value = helper(41);                                 // 语句，分号结尾
{result: value}                                         // 最后一个表达式 = 程序的返回值
```

- **`budget` 是必需的**，缺了报 `J-07`，修法就写在报文里。
- 字段：`calls`、`cost`、`depth`、`escalate`。**`calls` 与 `cost` 必填**
  （省了报 `budget requires 'calls'` / `budget requires 'cost'`），**`depth` 与 `escalate` 可省**。
  字段顺序随便（实测 `{depth, cost, calls}` 也过）。
- **程序里有 `ask` 或 `escalate` 就必须给 `escalate: N`**，否则 `J-07`。
- **`budget` 的值在源码里读不到。** `budget.escalate` 报 `E-name`。见第四节。
- `import` 路径**相对 `.jpp` 文件自己所在的目录**（不是工作目录）。**必须写在 `budget` 之前**，
  写在后面报 `expected ';' between expressions`。
- 空程序（只有 `budget;`）合法，`value` 是 `null`。

### 注释

只有 `// 到行尾`。**`/* */` 不支持**，写了报 `expected an expression`。

### 字面量

```jpp
42          -3          1.5         true        false       unit
"文本"      "带\"引号\""  "带\n换行"
[1, 2, 3]   [1, 2,]                      // 列表，允许尾逗号
{a: 1, b: "x"}   {a: 1,}                 // 记录，允许尾逗号
```
字符串里中文没问题。**标识符里中文也能用**（`fn 加一(x: Int)` 过了检查），
但 `examples` 全是 ASCII，我自己写正式程序时没用——你自行判断。

### 运算符（全部实测）

| 类 | 符号 | 备注 |
|---|---|---|
| 算术 | `+ - * / %` | `/` 是整除（`7 / 2 == 3`）。`+` 也是**文本拼接**（`"a" + "b"`） |
| 比较 | `== != < <= > >=` | |
| 布尔 | `&& \|\| !` | |
| 取负 | `-x` | |

**Int 是 64 位有符号，溢出/除零/模零是指向你源码的运行错误，不是回绕**：

```
Int 加法溢出：9223372036854775807 与 1 的结果超出有符号 64 位范围（…）。
Int 是 64 位有符号整数，溢出是错误不是回绕
```

### 取值

```jpp
r.field          // 记录取字段
xs[0]            // 列表下标；下标可以是变量：reports[i]
```

### 函数

```jpp
fn name(p: T, q) -> R !{judge, do} { 体 }    // 语句形式；参数标注、返回类型、效应行都可省
fn(x) { x + 1 }                              // 表达式形式（lambda），同样可省标注
```

- **效应行 `!{…}`** 只认四个名字：`judge` / `gen` / `do` / `ask`。
  `!{}` = 显式声明纯；**不写 = 让它推断**。
  - 标少了报 `E-effect`（错，挡程序）；标多了报 `W-effect`（只是提示）。
  - **想要效应多态的高阶助手就别标**——标了就是给所有调用者的承诺，
    某次实参恰好是纯的也不会放松。
  - **`cut` 不算 `judge`，哪怕调用实际发生在它那里。** 判断是惰性的：`judge(…)` 只登记，
    `cut` 才触发刷新、真的把请求发出去。但效应行跟的是 **`judge` 站点**，不是发出的时刻——
    `fn settle(r: Reading) -> Text !{} { exit_kind(cut(r)) }` **标 `!{}` 就能过**（我验过）。
    **所以一个只收读数、只 `cut` 的助手，它的效应标注不会告诉调用者这里会花钱。**
    估预算按 `judge` 写在哪儿算，不按 `cut` 写在哪儿算。
- **递归可以**，**块内嵌套 `fn` 可以**，**后定义先被前面的函数体引用可以**
  （同一块里的绑定共享一个环境）。**语句位置的直接引用**要求先定义后使用，否则 `E-name`。
- 内置名可以当值传（`map(readings, cut)` 实测可用）。〔未验证：所有内置都能这样传。〕

### 类型名

标注里认得：`Int`、`Decimal`/`Float`、`Bool`、`Text`、`List`、`Record`、`Unit`、`Exit`、
`Mat`、`State`、`Question`、`Reading`、`Method`。带参数的写得出：`List<Record>`、`Record<Exit>`。
函数类型：`Fn(A) -> B`；**带效应行**：`Fn(A) -!{judge}-> B`；**捕获了未决责任的**：`Fn1(A) -!{}-> B`。

**静态类型检查只在「标注是基本档、实参又是字面量」时判**（`E-type`）。
其余交给运行期。所以 `Mat` / `Exit` 这类标了也只是给人看的，不要指望它替你拦住什么。

### 控制流

**只有 `if`，没有 `for` / `while` / 赋值。**

```jpp
if cond { a } else { b }
if c1 { a } else if c2 { b } else { c }      // else if 链可以
```

循环、迭代、聚合全靠内置：`loop` / `map` / `filter` / `fold`（下一节）。

### 块

`{ 语句; 语句; 最后一个表达式 }`。handler 的臂体里也可以写多条语句：

```jpp
unsure: fn(u) { let c = unsure_cause(u); consume(u, "drop"); c }
```

---

## 二、内置清单

我给 `examples` 里**一次都没出现过**的每个内置写了三行程序，先 `check` 再 `run`。
**25 个探针里内置的名字与参数顺序 24/25 一次猜对**，全部与 `INTERFACE.md` §三 的签名一致——
**这门语言在这件事上守信用**，照签名写就行。下面是我验过能跑的那份，按你会用到的顺序。

### 材料与状态

```jpp
mat(任意值) -> Mat                       // 出口也能变材料
content(m) -> 值                         // 取内容；读数进来是 J-01 错
state(m) -> State
state(m, {ctx: …, ref: …, over: […]}) -> State
transform(fn(old) { … }, m…) -> Mat      // 记账变换
```
**`on` 槽可以直接写列表**：`state([m])` 与 `state(m)` 都行，关系题写 `state([a, b])`。
**最多两个对象**（一题一对象，关系用一对），多了 `J-14`，报文直接给修法：
「逐个对象建状态，或把它们放进 over 槽当候选」。

### 出题与判断

```jpp
test("题面", "校准键") -> Question
select("题面", "校准键") -> Question              // 要在 state 的 over 里挑
measure("题面", ["low", "high"], "校准键") -> Question   // 至少两档
judge(state, question) -> Reading
judge(state, [q1, q2, …]) -> [Reading]           // 一状态多题一次问完
cut(reading) -> Exit
cut(reading, "另一个校准键") -> Exit
ask(state, question) -> Exit                     // 问人
```

**校准键那一位只收 Text**。写数字字面量是 `J-03`：「线不可字面，这一位只收校准记录的键」。

**判断是惰性的**：`judge` 造出来的 `Reading` 里没有答案，到**刷新点**才一次发出。
刷新点是「凡结果依赖于答案的操作」：`cut`、`if`、`content`、`allocate`、`unsure_bound`、程序结束。
**同一个状态的多道题会被融合成一次调用**（实测：一状态两题 = `calls: 1`）。
**不同状态不会融合**（实测：三个材料各一题 = `calls: 3`）。

### 出口与未决责任

```jpp
handle(exit, {act, ignore, pick, at, unsure, otherwise}) -> 值
exit_kind(e) -> Text          // "act" / "ignore" / "pick(k)" / "at(l)" / "unsure(cause)"
line_source(e) -> Text        // "" / "题级·手填" / "题级·证书:α=…" / "模式级·…"
consume(e, "drop") -> Unit
unsure("我的理由") -> Exit     // 自己造一个未决出口
pending("说明")               // 整个程序停在这里
```

**穷尽规则**：test 要 `act`/`ignore`/`unsure`；select 要 `pick`/`unsure`；measure 要 `at`/`unsure`。
`otherwise` 兜得住前四种，**兜不住 `unsure`**——`unsure` 臂必须自己写，而且必须是收至少一个参数的方法。

`unsure` 臂拿到的**不是原因文本，是未决责任本身**（一个可以被追踪的值）。四条合法去向：

```jpp
unsure: fn(u) { escalate(u, state(m), test("人来定？", "human")) }   // 交给人（效应 ask）
unsure: fn(u) { literalize(u, state(m), test("更字面的问法", "k")) } // 换题重问（效应 judge）
unsure: fn(u) { consume(u, "drop"); 兜底值 }                          // 显式丢弃（发 W-drop-vs-escalate）
unsure: fn(u) { {ok: false, duty: u} }                                // 放进返回值，交给调用者
```
**四条都不走就是 J-05 错**。只读不算处理：

```jpp
unsure_cause(u) -> Text    // 路由键：band / cold / tie / untested / fail:…
untested(u) -> Text        // 哪个量在这条路上没测："" / calib_line / permutation
```

### 有界控制与数据

```jpp
loop(bound, 初值, fn(acc, i) { … stop(v) … })   // 唯一的循环，bound 必须是正整数
map(list, fn) / filter(list, fn)                 // 体内不能含 loop / stop（E7）
fold(list, 初值, fn(acc, x) { … })
```
**`map` / `filter` 的体内可以做 `judge` / `do` / `transform`**——这条 2026-09-21 由总控裁定关闭，
`INTERFACE.md` §七 第 3 条那个「要不要收紧」已经结案，**以依据为准，不收紧**。

其余全部实测可用：`len` `range(a,b)` `append` `concat` `slice` `contains` `sum` `reverse`
`keys` `has(r,k)` `with(r,k,v)` `text` `join` `print` `min` `max` `abs` `floor`。

### 效应与失败

```jpp
do("动作名", [参数…], iter_seq) -> Mat | Fail
gen("prompt", [ctx], n, retry_seq) -> [Mat]
fail("理由") -> Fail        is_fail(v) -> Bool
```
**动作只能是已登记的三个**，否则 `J-11`——**报错会列出全部登记动作及其可逆性**
（`read_json（可逆）、record_check（可逆）、write_json（**不可逆**）`）。
**故意打错一次名字，是今天查「哪个动作不可逆」最快的办法**，而那一条正是 J-08 管不管你的前提。
循环里 `iter_seq` 要随轮次变（写成循环外的 `let s = 0` 也照样告警，我验过），
常量会发 `W-seq-const`（「改个写法就会碰撞。修法：把轮次 i 传给 iter_seq」），
真撞键是 `J-13`。**`do` 失败是值不是异常**：`is_fail(do("read_json", ["没这文件.json"], 0))` → `true`，零告警。

### 发挥不确定性长处（这两个不花钱、不进账本、不动预算）

```jpp
allocate(读数们, k) -> Record       // {picked: [下标…], 算不出: [下标…]}
unsure_bound(读数们) -> Record       // {n, union_bound, independent_any, n_unknown}
```
**⚠ `allocate` 返回的是记录，不是列表。** 我第一版把它写成 `-> [Int]`，**照那个写
`picks[0]` 会直接报 `Record[Int] 不可索引` 打断程序**。正确写法：

```jpp
let a = allocate(readings, 2);
let picks = a.picked;        // 真正的下标表
let blind = a.算不出;         // 这些读数根本算不出不确定度——见第四节第 3 条
```

`算不出` 那一栏是**承重的**：落在里面的读数**不进榜**，它们不是「排在后面」，是**没被排**。
**你每次用 `allocate`，都该看一眼这一栏是不是空的。**

**两个都只收读数，出口传进去即错。** 用之前先读第四节。

### 判断向量

```jpp
agg([r1, r2]) -> Reading      // 合并后仍是读数，还能 cut
order([r1, r2]) -> [[Int]]    // 分组排序；同组 = 在 δ 之内不可分
```

---

## 三、fixture JSON 的 schema

**这一节是我花时间最多的地方，也是这份文件最该存在的理由。**
`--fixtures` 吃的那个 JSON，`INTERFACE.md` 里一个字都没有。下面全部实测。

```json
{
  "description": "随便写一句，会原样出现在输出的 fixture_description 里",
  "calibrations": [
    {"key": "校准键", "hi": 0.75, "lo": 0.25, "n": 60, "status": "上岗"}
  ],
  "generations": [
    {"prompt": "写一段发布说明", "retry_seq": 0, "output": [{"headline": "v2", "body": "…"}]}
  ],
  "observations": [
    {"on": [{"item": "flight", "amount": 4200}],
     "op": "test", "text": "这笔报销合规吗？", "calib": "expense",
     "answer": {"Noul": 0.96}}
  ],
  "responses": [
    {"on": [{"item": "gift card", "amount": 900}],
     "op": "test", "text": "人来定：这笔批不批？", "calib": "human",
     "answer": {"Noul": 1.0}}
  ]
}
```

**逐字段**：

| 字段 | 值 | 备注 |
|---|---|---|
| `calibrations[].status` | `上岗` / `冷` / `停岗` / `待真值` | `上岗` 必须 `n > 0` |
| `observations[].on` | **列表**，元素是 `state` 的 `on` 槽里那个材料的内容 | `mat({a:1})` → `[{"a":1}]` |
| `observations[].op` | **只认 `test` / `select` / `measure`** | 别的报 `unknown fixture question kind`，**而且报错不列可选值** |
| `observations[].text` | 题面，与源码里 `test("…")` 逐字相同 | |
| `observations[].calib` | 校准键 | |
| `observations[].answer` | `{"Noul": p}` / `{"Choice": [p…]}` / `{"Score": [p…]}` | 按 `op` 对应 |
| `observations[].over` | `["x","y"]` | **`select` 必填**，写在观察的**顶层**，不是嵌在别处 |
| `observations[].scale` | `["low","high"]` | **`measure` 必填**，顶层。**这个字段名我猜了 4 次才中** |
| `responses[]` | 与 `observations[]` 同形 | 给 `ask` / `escalate` 用；配 `--resume` |
| `generations[]` | `{prompt, retry_seq, output: [值…]}` | 给 `gen` 用 |

**⚠ `calib` 今天不参与匹配。** 观察键的分量里没有 `calib`——我实测把它写成
`"完全不相干的键"`，**照常命中、零告警**，而程序里那道题用的仍是它自己写的校准键。
后果是**你可以把一个答案接到错误的校准键上而永远不知道**，而校准键决定用哪条线、
**线决定出口**。在它被修好之前，请把夹具里的 `calib` 当成**注释**，
真正生效的是**源码里 `test(…, "键")` 写的那个**，以及 `calibrations` 里有没有那一条。

**必须知道的第二条**：**fixture 加载器对认不得的字段既不报错也不告警。**
我写 `"zzz": 123` 进校准记录，静默通过；写 `"slots": {...}` 进观察，静默忽略。
后果是**「我写错了字段名」和「我写对了但状态真的不一样」在报错上分不开**。

**未命中的报错现在会把答案直接给你**（2026-09-21 改的；在那之前它只给两个哈希，
我就是在那个版本上猜了四次 `scale`）：

```
固定观察未命中：题「多大把握」× 状态 95677678（键 50da1f43…）；固定观察不是模型，缺记录即错。
内核这次算出来的状态 = {"on":{"a":1}}；
题 = {"calib":"k","evidence":[],"op":"score","scale":["low","high"],"text":"多大把握"}。
夹具里要有一条 observation，其 on/ctx/ref/over 与上面的状态逐字段相同，
op/text/calib/scale/evidence 与上面的题相同
```
**调法就是照抄这两份 JSON**，两处要手动搬一下（我验过一次粘贴即过）：
- **`over` 印在「状态」那份里，但要写到 observation 的顶层**（和 `scale` 一样是顶层字段）。
- **`evidence` 不用写**，留空即可。
- `on` 印出来就是列表、`op` 印出来就是夹具认的题式名（`test`/`select`/`measure`），
  这两处可以直接抄。〔2026-09-21 之前印的是对象和内核名 `noul`/`choice`/`score`，
  抄了会报 `invalid type: map, expected a sequence` 或 `unknown fixture question kind`。〕

**`--resume` 的 `responses` 未命中也一样会大声报**（我验过：只把题面的全角「？」改成半角，
它就报 `ask 失败：responses 未命中：…夹具给了 1 条 responses，没有一条对得上`，并同样打出两份 JSON）。
〔这条在 2026-09-21 之前是**静默**的——`status` 仍是 `pending`、stderr 空、warnings 空，
与「人确实还没答」逐字段一样。第二个用这份文件的作者在那个版本上花了 5 个探针才定位，
**写在这里是因为如果你手上的构建是旧的，这条会花掉你整个下午**。〕

**三条键规则，注意前两条是两个不同的数**：
- **状态哈希**含 `on` / `ctx` / `ref` / `over`。**`over` 在这里面。**
- **`scale` 不在状态哈希里**——它是观察键的**另一个独立分量**。所以报错里的
  「状态 {hash}」和「键 {k}」是两个数，**想核 `scale` 就不要盯着状态哈希看**，看题那份 JSON。
  （我原先把 `scale` 说成在状态里，是错的；内核侧核对时指出来的。）
- **账本键带调用点**。同状态同题写在源码两个不同位置，是两条不同的记录。

**重放**：

```
jpp run p.jpp --fixtures fx.json --ledger-out l.json     # 第一趟
jpp run p.jpp --fixtures fx.json --replay l.json          # 零调用重放（实测 calls:0 replayed:8）
jpp run p.jpp --fixtures resp.json --resume l.json        # 挂起后人答了，接着跑
```
`--resume` 的那份 fixture **只要 `calibrations` + `responses`**，不必重列 `observations`
（前一趟的答案从账本命中）。

---

## 四、存在但你碰不到（写下来比让你再试一遍便宜）

这一节每一条都是我先照文档做、做不到、才查出来的。**不是 bug 清单，是边界清单。**

### 1. `budget` 的值在源码里读不到

`INTERFACE.md` 说 `allocate` 的「`k` 通常取 `budget.escalate`」。照字面写：

```
E-name: 未定义的名字 budget：既不是这个块里的绑定，也不是内置操作
```
**`k` 只能硬写成字面量。**

### 2. `unsure_bound` 的 `union_bound` 恒等于 `n`

`uᵢ` 取各题校准记录的 `unsure_rate`，而**那个字段只能从 Rust 侧
`CalibStore::set_unsure_rate` 写**。我在 fixture 的校准记录里写 `"unsure_rate": 0.1`
→ **静默吃掉**，结果仍是 `n_unknown = n`、`union_bound = n`。

**所以：`unsure_bound` 在 `.jpp` + CLI 这条路上不是判据，是常量。** 别拿它做分支。
（`budget.unsure` 这个字段也不存在，写了报 `unknown budget field 'unsure'`。）

### 3. `allocate` 算不出不确定度时，那些读数**不进榜**（2026-09-21 起会告警）

`allocate` 要的是「离决定带多远」。**这个量算不出来的时候**（校准键没有上岗记录、
没有属于自己的线），那些读数**不是排在后面，是根本没被排**。

**第一版的我在这里被坑过，而且是最坏的一种**：当时**零告警**，同一份源码换个冷夹具，
`allocate` 安静地返回 `[0, 1]`（文件顺序）而不是 `[2, 3]`（真正落带内的那两条）。
**这是我整个任务里唯一撞到的「算出错误结果而不报错」。**

**今天它会说话了，而且把证据放在返回值里**（我重跑验证过）：

```
value   = {"picked": [], "算不出": [0, 1]}
warning = W-untested: uncertainty 对 2 条读数算不出（它们的校准键没有上岗记录，
          没有属于自己的线），这些读数不进 allocate 的榜。按 J-15 不阻塞。
          修法【作者可改】：按 `算不出` 那一栏另行处置（那一栏就在 allocate 的返回值里）；
          【需接线人】给那些键写上岗记录、或给 CLI 接上 --calib/--profile
```
键正常时是 `{"picked": [1], "算不出": []}`，零告警。

**照着做**：
1. **每次都看 `算不出` 那一栏**。它非空 = 这批读数里有一部分没有参与排序。
2. 夹具里给那个校准键一条 **`上岗` 且 `hi`/`lo` 真实**的记录。
3. **退化的老习惯仍然值得留**：拿一批你已知答案的 p 先试一次，返回恰好是 `[0,1,2,…]` 就先怀疑。

**归因**（内核侧核对时给的，我原先写成「冷键的带很宽」是错的）：根子是
**CLI 从不加载档案**，而带的兜底宽度来自档案。**所以从 `.jpp` + CLI 这条路走，
这件事不是偶发，是必然会撞上。**

### 4. `select` 拿不到强出口（`pick` 臂是死代码）

`select` 要靠正逆两序置换测一致性才给 `Pick`，而**置换的开关在 `JevClient` 上**——
CLI 跑的是 `FixedClient`，没有任何 flag 或 fixture 字段能开它。实测：

```
value = -1   （走了 unsure 臂）
W-untested: permutation 在本次路径上没有被测量，按 J-15 取保守项（出口 untested）。
修法：要更强的出口就开置换（JevClient.permute = true，select 的调用数 ×2）
```
**这条修法你执行不了。** 但 J-05 仍然逼你写 `pick` 臂。**写一个永远跑不到的臂，这是现状。**

### 5. `W-uncertified` 的修法你也执行不了

```
W-uncertified: 键 k 的线是手填的、没有保形证书（n=…），而这里给出了强出口 Act。
不阻塞；要凭据就走 commission
```
`commission` 是 `CalibStore` 的 Rust 方法。我在 fixture 里写 `"certs"` / `"commission"`
→ 静默忽略，告警照旧。**这条告警在任何用手填线的程序上都会响，你没有办法让它闭嘴，
也不该为此改程序。**

**报文现在会自己说清这一点**（2026-09-21 改）：修法带 `【作者可改】` / `【需接线人】` 标签。
看到 `【需接线人】` 就别再试了——那不是你能改的。

**但标签今天只贴在 `W-*` 告警上，`J-*` 错误还没有**（我验过：J-08 的三条修法一个标签都没有，
而其中「把这个动作登记成可逆」恰恰是作者改不了的）。**所以看到 `J-*` 的修法，
还是得自己判断哪一条在你手里。** 一个判据：提到 `register` / `commission` / `Client` /
`profile` 的，都是接线人的事。

（对照：`W-untested` 的 **cold** 那条，修法是「给这道题的校准键写一条上岗记录」，
**这条你做得到**——往 fixture 的 `calibrations` 里加一条。**同一个前缀的告警，
有的可执行有的不可执行，照报文做之前先看它指的是哪一侧。**）

### 6. 层数在 CLI 输出里看不到（**不是内核没算**）

`INTERFACE.md` §三·五 讲了「一次刷新 = 一层」。CLI 的输出里没有层数，
`trace.events` 是平的，你只能从 `cost.calls` 反推——**而 `calls: 8` 与「两层 6+2」
和「八层各一次」同样对得上**。

**订正**：我原先写的是「`Outcome` 里没有 `layers`」。**`Outcome.layers` 是有的，
是 CLI 没把它透出来**（内核侧核对时指出）。**这两件事在你这一侧长得一模一样，
而修法完全不同**——一个要改内核，一个改 CLI 输出一行。遇到类似情形，
「我看不到 X」不等于「X 不存在」，**写成前者**。

### 7. `J-08` 是对的——**我原先在这里判错了，订正在此**

`J-08` = 「不可信材料上的判断不得**单独**放行不可逆 `do`」。我第一版写的是
「我一次也没见它触发，不要指望它替你把关」。**那是错的，而且错的方式值得你学一遍。**

我当时的测法是：`gen("写一段发布说明", [checklist], 1, 0)` 造材料 → 判断 → `handle` 的
`act` 臂里 `do("write_json", …)`。`do` 照常执行，无 J-08。我据此说「拦不住」。

**去混淆之后的四次对照**（全部我自己跑的）：

| | 材料来源 | 动作 | 结果 |
|---|---|---|---|
| D1 | `gen`，ctx 是我写的字面量 | `write_json` | 执行，无 J-08 |
| D4 | **`do("read_json", …)`** | `write_json` | **J-08 触发** |
| D6 | `gen`，**ctx 来自 `read_json`** | `write_json` | **J-08 触发** |
| D7 | `gen`，**ctx 为空** | `write_json` | 执行，无 J-08 |

**结论**：`gen` 的输出**继承 ctx 的 taint**。ctx 干净（或为空）→ 输出可信 → 那根本不是
「不可信材料上的判断」，**J-08 不触发是对的**。我那个「文档说不许写的程序」**从来就不是它**。

J-08 触发时的报文（`if` 守卫与 `handle` 臂两种写法我都验过，都报）：
```
J-08: 不可逆动作 write_json 的守卫里没有一个来自可信状态的合取项：不可信材料上的判断
不得**单独**放行不可逆动作（宪法 IFC / 12 §5 J-08）。修法：在条件里再合取一个来自
trusted 状态的判断，或改走 ask 让人拍板，或把这个动作登记成可逆
```

**哪个动作不可逆，现在查得到**（2026-09-21 改）：`do` 打错名字时 J-11 的报错列出全部登记动作
及可逆性——`read_json（可逆）、record_check（可逆）、write_json（**不可逆**）`。
**故意打错一次名字，就是今天最快的查法。**

**留给你的一个真问题**（我不下结论）：**一段模型生成的文本，仅仅因为喂给它的 ctx 是干净的，
就算可信吗？** `gen` 的输出继承 ctx 的 taint 这条规则本身是清楚的、可预测的；
但「模型说的话」和「我写的字面量」在 IFC 意义上是不是同一档，那是设计问题，不是我能断言的。
**你用 `gen` 造材料再拿它当守卫时，知道这条规则就行。**

---

## 五、报错怎么读

这门语言的报错格式是「一句话说错在哪。修法：…」，**大部分时候照做就对**。
三个我实测过、照做一次就过的例子：

```
J-07: 程序缺 budget…修法：源码首行写 `budget {calls: N, cost: X, depth: D};`
J-14: state 的 on 槽放了 3 个对象…修法：逐个对象建状态，或把它们放进 over 槽当候选
J-03: test 的第 2 个参数是数字字面量…修法：test(…, "校准键")
```

**一个例外，值得你提前知道**：`J-05`「程序结束时有未消费的 unsure」的报文，
**指的是责任诞生的地方，不是责任消失的地方**。我遇到的那次：

```
prog3c.jpp:14:22: J-05: 程序结束时有未消费的 unsure(band)（题 fcfda4cc）。
修法：用 handle(e, {…, unsure: …}) 或 consume(e, "drop") 处理，或把它带在返回值里
let checks = yes_no(cut(judge(state(checklist), …)));
             ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
```
它给的三条修法，**头两条我在被指着的那一行上早就做了**（`yes_no` 里就是带 `unsure` 臂的
`handle`）。真正丢掉责任的在二十行以外：另一个 `if` 分支的返回记录里没有带上 `checks`。

**排查顺序**：看到这条报错，**不要改它指的那一行**。去找「这份责任从被 `handle` 之后，
到程序结束之前，经过了哪些返回值」，其中有一条路径把它落下了。

---

## 六、留白（我知道我不知道的）

**第一版这里列了九项。内核侧核对后，我把每一项自己又跑了一遍——七项关掉了，两项是真空白。**
关掉的那七项下面写的是**我自己跑出来的报文**，不是转述。

### 已关掉（附我的验证）

| 原留白 | 答案 | 我怎么验的 |
|---|---|---|
| `Fn(A) -!{ε}-> B` 真的起作用吗 | **起作用，而且报在实参上** | `fn apply(m, f: Fn(Mat) -!{}-> Text)` 传一个会 judge 的方法 → `E-effect: apply 的第 2 个参数 f 标成只收 !{} 的方法，这次传的 peek 会 judge：类型上的效应行是契约，实参必须在它之内` |
| `agg` 的拒绝面 | **跨题即拒，是 `J-01`** | 一状态两题拿到两条读数 → `agg(rs)` → `J-01: agg 只合并**同一道题**跨运行的读数` |
| `order` 的拒绝面 | **跨题即拒，是 `J-04`** | 同上 → `order(rs)` → `J-04: order 只排**同一道题**跨对象的读数：跨题的读数不可比（各有各的校准线，p 不在同一把尺子上）` |
| `depth` 超限 | **是 `J-06`，报文带修法** | `budget {…, depth: 8}` + 50 层递归 → `J-06: 调用深度超过 8（递归无界）。修法：用 loop(bound, …) 或提高 budget.depth` |
| `J-06` 一轮之内账本键重复 | **停循环 + 两条告警** | 循环里反复判同一状态同一题 → 第 2 轮停，`W-noprogress: 第 2 轮重复了账本键 8a09b956，循环停止（J-06 键重复即停）` + `W-bound: loop 到 bound=5 仍未 stop`，`calls: 1 / replayed: 1` |
| 多文件 `import` 重名 | **是错，不是后者覆盖前者** | 两个库都定义 `bump` → `while resolving this import` + 指出冲突的名字与来处 |
| `transform` 的「进槽材料来源」条号 | **我原先记成 J-11，是错的** | 见下 |

**`transform` 那条我核过了，结果比「改个号」更有用**——三件事是三个不同的规则：

| 实际写法 | 实测报的是 |
|---|---|
| 输出是读数 / 函数 / 状态 | **`J-11`**：`transform 的输出要能成材料，收到 Reading` |
| **读数**放进槽 | **`J-01`**：`读数不能放进 transform 的槽：读数不是材料。修法：cut(读数) 得到出口，再 handle 它` |
| 「进槽的材料只能来自字面量、效应输出或这里」 | **我没能撞出来**：`mat(content(m))` 进槽、裸记录直接进槽，都照跑不误 |

第三条属于 `derived_from` 来源链那一族（同族的 `J-02` 我撞到了：
`J-02: 禁自指：状态含由题「q」派生的材料，不能再问同一题`）。
**`INTERFACE.md` §三 的表把第三条标成 `(J-11)`，而 `J-11` 实测报的是另外两件事**——
我第一版就是照抄它才记错的。**把这个记在这里，因为下一个人也会照抄那张表。**

### 真空白（两项，别猜）

- **预算拦截在 CLI 路上验不了**：`FixedClient` 的 `cost` 恒 0，`usd` 也恒 0，
  所以「超预算即停」这条我**没有办法**从 `.jpp` 这一侧看到它工作。内核侧说有测试，
  **但那不是我验的，所以这里只写「我验不了」。**
- **`fit` 的正参数**：我只验到「静态放行、运行期 `J-01` 拦住非读数输入」，
  **没有跑通一次真正的 `fit`**。内核侧说 `tests/fit.rs` 有完整例子——
  **那是测试文件，不在我能读的范围内，所以这一格对你我仍然是空的。**
  要用 `fit`，先找接线人要一个能跑的最小例子。

### 顺带：`literal_mode`

`INTERFACE.md` 四·十五 自己写着「键里有这一维、库里有这一维，
**但没有任何执行路径选得中默认档之外的格**」。**这一条我读对了，不用试。**

---

## 附：一个可以直接抄的骨架

判一批东西、判不准的交给人、并且不让未决悄悄消失。

```jpp
budget {calls: 4, cost: 0, depth: 256, escalate: 2};

fn review(m: Mat) -> Record !{judge, ask} {
    let e = cut(judge(state(m), test("这笔报销合规吗？", "expense")));
    handle(e, {
        act:    fn() { {decision: "approve", by: "auto", note: ""} },
        ignore: fn() { {decision: "reject",  by: "auto", note: ""} },
        unsure: fn(u) {
            let why = unsure_cause(u);
            let e2 = escalate(u, state(m), test("人来定：这笔批不批？", "human"));
            handle(e2, {
                act:    fn() { {decision: "approve", by: "human", note: why} },
                ignore: fn() { {decision: "reject",  by: "human", note: why} },
                unsure: fn(u2) { consume(u2, "drop"); {decision: "hold", by: "human", note: why} }
            })
        }
    })
}

let claims = [mat({item: "flight", amount: 4200}), mat({item: "gift card", amount: 900})];
let results = map(claims, review);
{results: results, human_touched: len(filter(results, fn(r) { r.by == "human" }))}
```

配套 fixture（两份，第一趟给观察、第二趟给人的回答）：

```json
// fx.json —— 第二笔落在带内，会升级给人
{"calibrations":[{"key":"expense","hi":0.75,"lo":0.25,"n":40,"status":"上岗"},
                 {"key":"human","hi":0.5,"lo":0.5,"n":1,"status":"上岗"}],
 "observations":[
  {"on":[{"item":"flight","amount":4200}],"op":"test","text":"这笔报销合规吗？","calib":"expense","answer":{"Noul":0.96}},
  {"on":[{"item":"gift card","amount":900}],"op":"test","text":"这笔报销合规吗？","calib":"expense","answer":{"Noul":0.52}}]}
```
```json
// resp.json —— 人答「批」
{"calibrations":[{"key":"expense","hi":0.75,"lo":0.25,"n":40,"status":"上岗"},
                 {"key":"human","hi":0.5,"lo":0.5,"n":1,"status":"上岗"}],
 "responses":[{"on":[{"item":"gift card","amount":900}],"op":"test","text":"人来定：这笔批不批？","calib":"human","answer":{"Noul":1.0}}]}
```
```
jpp run skeleton.jpp --fixtures fx.json --ledger-out l.json
  → status = pending，pending 里一条 ask

jpp run skeleton.jpp --fixtures resp.json --resume l.json
  → status = returned
    value = {"human_touched": 1,
             "results": [{"by":"auto","decision":"approve","note":""},
                         {"by":"human","decision":"approve","note":"band"}]}
```

**上面这份骨架与两份 fixture 是我逐字跑过的**（不是从别处誊的）。
我写它的那个版本第一次就跑对了——**如果你手上有这份文件，你也会。**
