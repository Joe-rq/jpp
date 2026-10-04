# J++x · 实验版 J++（Python）

> 本文件顶部是**宿主接口**（2026-10-04 定，改动会在黑板 `通爻网/语言/宿主接口` 记一条）。语言速览、与现行 J++ 的差异、J++ 主线问题清单在后面。

## 宿主接口 v1

```python
from jx.engine import Engine

eng = Engine.load(
    "app/net.jpx",
    ports={"judge": jev_port,        # 有 async call(state: dict, qs: list[WireQ]) -> list[raw]（jx/ports/jev.py 的 JevPort 即可）
           "gen": gen_port,          # 可选：有 async gen_json(template: str, args: list) -> 纯 Python 值
           "enc": encoder},          # 可选：有 encode(texts) -> np.ndarray（缺席降级用；不给用内置哈希编码器）
    flags={"no_cells": False, "no_fill": False, "no_batch": False, "no_meta": False,
           "no_budget_chain": False, "no_absent": False, "no_deadline": False},
    seed=0, ledger_path="runs/x.ledger.jsonl", events_path="runs/x.events.jsonl",
    clock=None)                      # 可选：callable -> 秒。不给用 time.monotonic；宿主不用供拍，timer/截止由引擎自己排
await eng.start()                    # 在当前 asyncio 事件循环里起引擎（与 FastAPI 同一个循环）
...
await eng.idle()                     # 等到静止（没有脏单元、没有在飞尝试与调用）
await eng.stop()
```

**(a) 注册 do 动作**

```python
eng.register_action(name, fn, cost_usd=0.0, transparent=True, depends_on=None)
```

- `fn(*args)` 同步或 `async` 都行，返回纯 Python 值（dict / list / str / 数 / bool / None），引擎转成 J++x 的记录与列表。
- 程序里写 `do("route", [x, n, 20], 0)`，`fn` 收到 `(x, n, 20)`。参数一律转成纯 Python：记录 → dict，列表 → list；
  `Mat` → `{"mat": True, "content", "origin"}`；出口 `Exit` → `{"exit": True, "kind": "act|ignore|pick|at|unsure", "value", "cause", "grade", "p", "conf", "q", "key", "lean"}`
  （`p` 是出口所在块的读数，`q` 是题面）；未决值 → `{"pending": cause, "needed": ...}`。顶层参数是未决值时动作不发出（记跳过）。
- `transparent=True`：结果只由参数决定，同一尝试里同参数只调一次。有副作用的（如 `index_put`）写 `transparent=False`。
- `depends_on=["node"]`：结果还依赖某个单元族（索引读的就是 node 族）。调用它的程序单元登记对整个族的依赖，族里任一实例变了就被标脏重跑（两层截断照常：结果没变，下游不动）。
- 失败：抛异常即 `Fail` 值，走 `Unsure(fail)`。

**(b) 写源单元、发事件**

```python
epoch = await eng.put_source("world", ["a0001"], value)        # 宿主写单元（也用于 reply[b,a,cat]、unlocked[b,a]）
epoch = await eng.remove_source("world", ["a0001"])            # 离开：实例删除，读者被标脏，读到 None
epoch = await eng.event("join", value, budget={"calls": 64}, deadline_s=15)   # 给 `on [event(join)]` 的常驻程序
```

- `put_source` 对 union 单元是追加一个元素，其余归约器按归约器合入；宿主是一个写者（writer id = `host`）。
- `put_source`/`event` 都可带 `budget={"calls":..,"cost":..}`（从整场账户派生，只收紧）与 `deadline_s`，开一条**接入事件链**；这条链触发的尝试、判断都记在它名下。不带则用整场账户、无截止。

**(c) 只读（进账本，读者记为 `host`）**

```python
eng.read("node", ["x"], mode="peek")       # 当前最新版本的值（可能是进行中）；没有则 None（同名别名 read_host）
eng.read("node", ["x"], mode="settled")    # 已定下的版本的值；没定下则 None
eng.status("node", ["x"])                  # {"status": "running|settled|removed", "version", "progress": {"judged","total"}, "pending": [...]}
eng.keys("edge", contains="a0001")         # 族里的实例键（list 的 list）；contains 过滤含某元素的键
```

**(d) 事件总线**

```python
eng.bus.subscribe(cb)        # cb(event: dict)，同步回调；同时写 events_path（.jsonl）
```

推两类：
1. events.md 格式的运行时事件：`batch`、`judge`、`unsure_route`、`disclose_request`、`invalidate`、`stats`，另加 `error`。
2. 单元发布：`{"type": "publish", "t", "cell", "key": [...], "version", "status", "value", "prev"}`（`value`/`prev` 已转纯 Python；删除时 `status: "removed"`）。union 单元另带 `added`（新加的元素）；写进 .jsonl 文件时省掉 `prev`，union 单元也省掉 `value`，只留 `added`。
   `t` 是墙钟秒（`time.time()`；虚拟时钟下是虚拟时间）。`invalidate` 带 `ids`（宿主写入的单元键）与 `cause`（`put_source(..., cause=)` 传的值）。
   撤回造成的删除等到静止才定：同一键（如构型的成员集合键）在重算中被新写者接上，只发一次版本更新，不发「删除再新建」。
   `node_join`/`edge`/`config`/`plan` 由宿主按 `publish` 做纯展示映射（引擎不知道哪个族叫 edge）。

**(e) 时钟**：引擎自己排 timer 与截止；宿主可传 `clock=callable` 替换时间源（测试用虚拟时钟）。

**(f) 其他**：`eng.stats()` 给调用数、题数、花费、缓存命中、尝试数、截断数、错误数，`by_resident`（每个常驻程序的尝试数、记在它名下的调用与题数）、`why_rerun`（每个常驻程序被重算的原因与次数：`来源:族`、`读过:族`、`等到:族`、`新实例:族`、`spawn`）；`eng.errors` 是程序单元的运行错误（带 文件:行:列）。
`Engine.load(..., max_calls=N)` 在程序的 `budget.calls` 之上再收紧；`cache_path="x.sqlite"` 让判断单元跨运行复用。

---

## 怎么跑

```bash
cd 应用/通爻网
.venv/bin/python -m jx check app/net.jpx
.venv/bin/python -m jx run jx/examples/mini-net.jpx --world jx/examples/mini-net.world.json \
    --events jx/examples/mini-net.events.jsonl --quiet                    # 离线：录好的答案 + 哈希伪读数（标 fixture-synthetic）
.venv/bin/python -m jx run jx/examples/mini-net.jpx --live --record f.json --max-calls 50 ...   # 真机，边跑边录
.venv/bin/python -m jx run ... --fixtures f.json                          # 用录下的答案零花费复跑
.venv/bin/python -m pytest -c jx/tests/pytest.ini jx/tests -p no:cacheprovider   # 测试
```

`run` 的开关：`--seed`、`--ledger-out`、`--events-out`、`--cache`、`--max-calls`、`--enc bge`，以及七个消融开关
`--no-cells --no-fill --no-batch --no-meta --no-budget-chain --no-absent --no-deadline`。
`--world` 是宿主先写入的源单元（`{族: {键: 值}}`，多键用 `|` 连），`--events` 是之后逐条送入的宿主事件（`{op: put|remove|event, cell, key, value, budget?, deadline_s?, label?}`），每条之后等静止并记一段调用数。

## 文件

| 文件 | 作用 |
|---|---|
| `lexer.py`、`parser.py`、`ast.py` | 词法、语法、AST；报错带 文件:行:列 与源码行；`load` 处理相对 import |
| `interp.py` | 树遍历求值（async）、惰性判断与出口、前瞻提升、J-05 默认链、全部内置函数 |
| `engine.py` | 单元图、常驻程序、尝试、两种读、归约器、占用、派生、静止点、事件链、截止、宿主接口 |
| `sched.py` | 判断调度：合批、排序、并发、缓存、预算账户、时延、缺席降级；离线端口 `FixturePort`、录制端口 `RecordingPort` |
| `core.py` | 题、读数、`cut`、出口、账本 |
| `bus.py` | 事件总线 |
| `check.py` | `jx check` 的静态检查 |
| `hostlib.py` | 离线用的标准动作 `embed_topk`、`graph_components` |
| `ports/` | 真机端口：`jev.py`（JEV）、`gen.py`（`claude -p`）、`enc.py`（bge-m3） |
| `examples/mini-net.*` | 20 个 agent 的小网络 + 两个后续事件 + 6 人真机录制答案 |
| `tests/` | pytest，37 项（`netkit.py` 是离线跑 `app/net.jpx` 的小工具，接 host 的索引与图动作） |

---

## 语言速览

### 文法（现行 `.jpp` 表达式 + Fable-A 五个构造）

```
file     ::= {import "相对路径";} [budget 记录;] {decl} [结果表达式]
decl     ::= let 名 = 表达式; | fn 名(参数…) { … } | 表达式; | cell … | resident …
cell     ::= cell 名[键名, …] [reducer single | union | by_key(字段) | claim | override];
resident ::= resident 名(参数…) [on [来源, …]] [budget 记录] [deadline 秒] { … }
来源      ::= event(名) | timer(秒) | change(单元引用) | settled(单元引用)
单元引用   ::= 名[表达式, …]
表达式     ::= 现行表达式（字面量、记录、列表、块、fn、if/else、调用、字段、下标、! - * / % + - 比较 == != && ||）
           | a ?? b                                    （J++x 扩展：a 是 unit 或字段不存在时取 b）
           | ev | peek 单元引用 | settled 单元引用
           | put 单元引用 <- 表达式 | claim 单元引用 <- 表达式
           | spawn 名(实参…) [budget 记录] [deadline 秒]
```

名字支持中文。`//` 与 `/* */` 注释。`budget {calls, cost, depth?, latency_p95?}` 是整场账户；`depth` 是组合层数上限（缺省 3）。

### 五个构造的语义（依 Fable-A 第一条）

- **`cell 名[k…] reducer r`**：有版本的单元族。宿主或程序写；值的哈希没变就不升版本。归约器合并各写者的贡献（写者按 id 排序，合并顺序进账本）：`single` 取最近提交的写者（多写者时记警告，`jx check` 也提示）；`union` 是**只增集合**（同一写者历次写入累积，不随重算撤回）；`by_key(f)` 按字段 f 合并记录；`override` 最近写赢；`claim` 只能用 `claim` 写。
- **`resident 名(参数) on [来源] { … }`**：常驻程序。`on [change(world[a])]` 这种来源把参数全部绑到某个单元族的键上时，族里每出现一个键就实例化一个程序单元（身份 = 名 + 实参内容）；参数没被来源绑定的只能 `spawn`。程序体的值就是这个单元这次发布的版本，别的程序可以 `peek 名[实参]` / `settled 名[实参]`。
- **尝试**：来源触发或它读过的单元变了 → 标脏 → 排一次尝试。尝试开始时若上次读过的一切版本、触发值都没变，跳过（第一层截断）；发布的值哈希没变，读者不被标脏（第二层截断）。
- **`peek c`**：读当前最新版本，可能是「进行中」。**`settled c`**：读最新的已定版本；上游没定下时返回**未决值**（原因 `waiting`）并登记等待，这次尝试照常跑完、发布「进行中」版本（带部分值），上游定下后重来。等待可带截止，到时读到的是 `Unsure(deadline)` 类未决。
- **`put c <- v`**：尝试提交时经归约器合入。值里有未决的写记跳过。
- **`claim c <- v`**：即时占用，引擎串行化；成功得 `Act`，被占得 `Unsure(claim_conflict)`（`needed` 是占用者），走未决去向。占用者下次尝试不再占就释放，被拒者被标脏重跑。
- **`spawn 名(实参) [budget {…}] [deadline 秒]`**：显式派生下游程序单元，返回它的句柄（可 `peek`/`settled`/`status`）。预算从派生者账户派生、截止取较早，只收紧。派生者重算后不再派生它，它就被撤回（贡献撤回、发布 removed）。
- **`resident … budget {calls, cost} deadline S`**：接入预算与截止子句。这个程序被一条新的宿主事件链触发时，从链账户派生子账户（只收紧）、截止 = 当时 + S；它 spawn 的下游与默认链发的补信息请求都继承这个账户与截止。宿主不必传。`--no-budget-chain` / `--no-deadline` 时子句不生效。
- **`ev`**：触发这次尝试的事件值（`change` 来源是新值，`event` 来源是事件载荷）。

### 判断、出口与默认链

- `judge(state, 题 | [题…])` 只登记，返回惰性读数；`cut(r, 线?)` 返回惰性出口；出口在 `handle`、`act(e)`、读字段、比较时才等。
- 所有等待只经过引擎的一个卡口；**全部在跑的纤程都在等**（引擎静止）时，调度器把所有尝试登记的题一起发出：同一 state 的题合成一次调用（≤100 题，超了裂变），组间按（截止 → 接入事件新近 → 价值密度）排，并发 32。
- **前瞻提升**：一个尝试要停下来等判断时，先把它各层块里后面那些只构造状态与题的 `let` 提前求值登记。于是「每道题后面跟一个 `if`」与「先算齐再 `map`」两种写法调用数相同（测试：56 vs 56）。
- `map`/`filter`/`sort_by`/`any`/`all` 的回调有效应时并发跑（各自登记，静止时一起发）。
- 无线时按多数块；`{declare: {hi, lo}}` 是作者线（等级 `Declared`）；编码器降级的读数出 `Unsure(absent)` 并带 `lean`（编码器倾向的出口），等级 `Degraded`。
- **J-05 默认链**：`handle` 没写 `unsure` 臂时，`band/tie/insufficient` 未决、以及**无线是非题读数离 0.5 小于画像 `near_band`（0.1）**时：
  1. 同批伴随题 `select`「要更有把握，最缺哪一类信息？」——候选来自 `unsure_source({need})` 或题的 `lacks`，外加「不缺信息，事情本身两可」；只有一类时不问；
  2. 取：作者 `unsure_source({fetch: fn(q, cat, state)})` 优先；否则向状态的每个持有方（`state(…, {owners})`，缺省取材料的 `origin`）`put inbox[持有方] <- {from, cat, purpose, q, asker_display, deadline_s}`，并 `settled reply[持有方, 请求方, cat]`（截止缺省 15 秒，`unsure_source({deadline_s})` 改）；
  3. 回复补进这道题的 `ctx` 重判，同一条线；被拒或截止 → 出口仍是未决，进 `pending`，`needed` 写「缺 X，持有者 b 拒绝 / 截止前没有回复」。缺席类（absent/budget/latency/deadline/depth）与 `claim_conflict` 直接随值转交。
  读数触发的那一路补不到时，按多数块出口照常走，出口上带 `needed`/`waiting`。
- 未决值传播：内置函数收到顶层未决参数时结果就是那个未决值；`if` 条件未决时两支都不跑；`state` 的材料里有未决值时状态不成立；`do` 参数未决时不发出。

### E-tier（01-定稿 要求进语言）

`mat(v, {owner, tier})` 带主人与层；`never` 层不能做成材料。程序声明了两键单元 `unlocked[a, b]`（或 `budget {tiers: "名"}`）时，`state` 构造时检查：带层材料的主人必须已向状态里其他每个主体解锁到该层（解锁记录里片段的 `tier`），否则运行时报 `E-tier`。

### 内置函数

题与判断：`mat state test select measure judge cut handle consume unsure_source pending act ignored is_unsure is_exit reading`
生成与执行：`gen gen_json do fail is_fail is_pend`（`gen` 一登记就发、不阻塞静止判定；无生成端口时给离线占位）
单元族读（内核读原语）：`peek_family(族, 元素?)`（族里键含该元素的全部实例的最新值，依赖整族该元素）、`members(族)`（键列表，依赖族成员变化）、`status(句柄 | 族, 键)`（`{status, version, progress:{judged,total}, pending}`）
高阶：`map filter fold loop stop iterate(初值, 上限, {patience, measure?}, fn) sort_by any all`
纯函数：`len keys values items concat has contains get set merge range str json parse_json hash join split replace trim upper lower starts_with substr round floor ceil abs min max sum not slice flatten sort reverse unique index_of enumerate zip type is_list is_record is_str print`
库：`import "lib.jpx";` 载入同目录 `.jpx`（只放声明，不能有 budget 与结果表达式）。

---

## 与现行 J++ / Fable-A 的差异

| 事项 | 现行 J++（rust-jpp） | Fable-A | J++x 实际 | 为什么 |
|---|---|---|---|---|
| 常驻、单元、两种读、归约器、占用、派生 | 只有单程序单元图（C1–C2） | 五个构造 | 五个构造全有 | 本实现的主体 |
| 刷新点 | 程序文本里的 `if` 等 | 引擎静止点 | 静止点 + 刷新前前瞻提升 + 有效应回调并发 | 只有静止点时，同一尝试里「判一题、if、再判一题」仍逐题一次调用，两种写法比 6:1；加提升后 1:1 |
| `settled` 读到未定 | — | 尝试到此结束，发布「进行中」，上游定下后重来 | 返回未决值（`waiting`）继续跑完，发布带部分值的「进行中」，上游定下后重来 | 「部分值」要从已跑完的部分来；让未决值按 5a 传播即可，不必中断 |
| `union` | — | 多写者合并 | 只增集合 | 可撤回时出现振荡：请求方补到信息后不再发请求 → 请求撤回 → 回复撤回 → 又拿不准 → 又发请求 |
| 组合层数 | — | 按触发链，每级加一 | 按触发链：尝试的层数 = 触发它的来源单元版本（或派生者）的层数；写回自己读过的族时 +1 | 按「读到过的最大层数」算会误杀：一个基础构型的程序读到别的深层构型，自己的产物也被算成深层而丢掉 |
| `--no-meta` | — | 构型不发布为 node | 写回自己读过的族的写一律丢弃 | 与层数用同一定义，不认族名 |
| 预算 | 层边界核 | 只收紧，用完只停发 | 只收紧；**发出即预留**，回来按实花费校正 | 并发 32 在飞时只在回来后记账会超账（真机首跑 50 上限花了 58 次） |
| 缺席降级 | `Unsure(absent)` | 编码器相似度、`by: enc`、Degraded | 出口 `Unsure(absent)` + `lean` + `Degraded`；降级读数不进缓存；消费过它的单元在探测点或恢复时重判 | Fable-A 第七条预测写「标 Unsure(absent) 的粗召回」 |
| 默认链读数触发 | 带宽 = 画像 `delta.mid` | — | 画像 `near_band = 0.1` | 网络程序多数题无线，只靠 `tie` 时默认链几乎不触发 |
| 类型标注 | 解析并检查 | — | 只解析不检查 | 实验版范围 |
| 检查器 | 部分静态规则 | — | `jx check`：未定义名字、单元引用与键数、put 写常驻程序、claim 写非占用单元、spawn 实参个数、single 多写者提示、参数未绑定的常驻程序 | — |
| `??` | 无 | 无 | 有 | 契约值字段可缺（agent 节点与构型节点同形但字段不全），否则库里每处都要 `has` 判断 |
| 账本 | 意向先落盘、可重放 | — | 逐行落盘，**不支持只凭账本重放**；离线复跑靠 `RecordingPort` 录的 fixture | 实验版范围 |
| 校准线、认证、题库、`fit` | 有 | 不做 | 只有无线与 `declare` | 实验版范围 |

## 对 app/net.jpx 的改动（都已告知主会话）

1. 加 `import "lib.jpx";`（辅助函数在 `app/lib.jpx`）。
2. `iterate` 补初值：`iterate(C, 3, {patience: 2}, fn(cur) {…})`。
3. `两两(k, routes)` → `两两(k)`：程序单元身份 = 名 + 实参内容；召回路线每次重算都会变，放进实参会让同一对变成多个单元、都写 `edge[a,b]`。
4. `构型(x)` 把邻居的边也 `peek_family` 进来（两跳），链、环才出得来。
5. 构型节点补齐 agent 节点的字段（`display`、`catchers`、`projects`、`forbids`），与 agent 同形。
6. `app/lib.jpx` 的 `side` 不再把 `derived` 放进判断材料（只进召回索引）：派生片段由这一对的读数推出，放进材料会让「读数 → 材料 → 读数」成环，离线跑时同一对反复重判直到无进展上限。主会话随后改用独立的 `mat[x]` 单元承载判断材料，同一思路。
7. `整体(c)` 去掉 `on [...]`：只由 `构型` spawn，依赖由读收集。

## 发现的 J++ 主线问题

每条也记在黑板 `J++反馈` 板。

1. **静止点合批不足以让写法无关**（R-068、§2.13 R11.7）：同一尝试里顺序的「判 → if → 判」仍串行。要么像现行 `lift` 那样做提升，要么运行时前瞻。
2. **`union` 归约器要定成只增**（需求漏项，§2.13 R7）：可撤回的多写者集合在「请求—回复」回路上振荡。
3. **程序单元身份含易变实参时会裂成多个写者**（需求漏项，§2.13 R1）：检查器应提示「单元键里放了每次重算都会变的值」。
4. **组合层数要按触发链而不是读到的最大层数**（§2.13 R11.5）：后者在网络里误杀基础构型。
5. **派生信息进判断材料会成环**（R-067「计算改变可发现性」）：要分清「改变可发现性」（进召回索引）与「改变判断材料」（会自我强化），后者需要显式规则。
6. **并发在飞时预算要发出即预留**（J-07、§2.13 R11）：否则 32 路并发会把上限超出一截。
7. **`settled` 读到未定时应传播未决值而不是中断尝试**（§2.13 R5、Fable-A D2）：这样「进行中」版本自然带部分值。
8. **无线题的默认链靠读数触发才有用**（J-05 草案改法 3）：网络程序里题多数无线，没有带宽触发时补信息几乎不发生。

## 已知缺口

- 只凭账本重放、`ask`、`transform`、校准线与认证、`fit`、`order`、`repeat` 都没做。
- `let` 级记忆（Fable-A D1「绑定即单元」）没做：粒度是「程序单元 + 判断单元」，重跑一个单元时纯计算重做，判断按内容键命中缓存。
- 快照隔离是近似的：尝试读到的是读那一刻的最新版本，不是尝试开始时的快照；读完后别人发布了新版本会让它再跑一次。
- 版本只保留最近几个；`timer` 来源在真实时钟下用 `call_later`，虚拟时钟下 `idle()` 不会为周期定时器拨钟。
- 默认链向「状态的全部持有方」各发一份请求；同一持有方被多道题问同一类时合成一条（请求按内容去重）。
