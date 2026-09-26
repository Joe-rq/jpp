# J++ 新开发者一页纸

一页够用的入门；完整接口在 `crates/jpp/INTERFACE.md`（现行接口与施工日志混排，
按小节标题找），语法在 `FRONTEND.md`，观察/重放的可运行教程在 `METHODS-AND-LIFECYCLE.md`。
本文全部命令已在本仓库实跑验证（2026-09-24）。

## 装、跑

```sh
cargo build --workspace          # 需要支持 edition 2024 的 Rust 工具链
cargo test --workspace           # 含金样与重放对照
cargo run -p jpp -- parse examples/composition.jpp --ast
cargo run -p jpp -- check examples/composition.jpp
cargo run -p jpp -- run examples/composition.jpp
cargo run -p jpp -- run examples/adaptive.jpp \
  --fixtures examples/fixtures/adaptive.json --output report.json
```

`run` 默认吃固定观察（fixture），不发真实模型请求；不带 `--fixtures` 时只能跑不含
判断效应的程序（如 `composition.jpp`）。`--output` 落报告 JSON，`--ledger-out` 另落账本，
`--replay <ledger.json>` 用账本重放（拒绝任何新调用）、`--resume <ledger.json>` 续跑。

脱离本仓库单独安装：

```sh
cargo install --locked --path crates/jpp --root /tmp/jpp-native
/tmp/jpp-native/bin/jpp run examples/composition.jpp
```

装出来的可执行文件不需要 Python 或 Cargo；固定观察运行不需要账号、密钥或网络。

## 夹具（fixture）格式

一份 JSON，键为 `description`（自由文本）、`calibrations`（该次运行要用到的校准记录，
测试用途可手写，正式线不许手写，见下）、`observations`（材料 × 题 → 读数的穷举表，
缺一条就是错误而不是瞎猜）。

```json
{
 "description": "……",
 "calibrations": [
   {"key": "cs-refund", "hi": 0.75, "lo": 0.25, "n": 1, "status": "上岗", "delta": 0.05}
 ],
 "observations": [
   {"on": ["材料文本或状态记录"], "op": "test", "text": "题面文字",
    "calib": "cs-refund", "answer": {"Noul": 0.92}},
   {"on": ["材料"], "over": ["候选一", "候选二", "候选三"], "op": "select",
    "text": "……", "calib": "……", "answer": {"Choice": [0.9, 0.07, 0.03]}},
   {"on": [{"a": "…", "b": "…"}], "op": "measure", "scale": ["档位一", "档位二", "档位三"],
    "text": "……", "calib": "……", "answer": {"Score": [0.02, 0.08, 0.9]}}
 ]
}
```

`op` 对应三种题型：`test`（是非，`answer.Noul` 是一个概率）、`select`（K 选一，`over`
给候选，`answer.Choice` 是逐候选概率向量）、`measure`（打分，`scale` 给有序档位，
`answer.Score` 是逐档概率向量）。`on` 是材料（是非/打分题一个元素；关系题一对，如
`{"a":…, "b":…}`）。找不到公开示例时抄 `probes/winnow/make_fixture.py`（是非题）、
`probes/folio/fixture.json`（select）、`probes/entity-align/fixture.json`（measure）。

**`calibrations` 里的 `delta` 不能省。** 校准记录没有 `delta` 字段时，J-15 按「这条判据没测过」处理，
出口一律 `unsure(untested:Delta)`，不会到 `act`/`ignore`（δ 只能从校准记录取，不能由内核猜，B104、
步 15d-2）。手写测试夹具时把 `delta` 当必填字段，不要只抄 `hi`/`lo`/`n`/`status` 四项。

## 元素字段：`index` / `pos` / `trail`（步 25-0，B81/B82）

`sieve` 的输出元素保留输入元素的全部字段，再补或更新 `{item, pos, trail, exit, cause, q, qi, key}`
（填法形式另带 `fill`）；`pair` 的产物是 `{item: {a, b}, trail, left, right, at, pos}`。

- **`index`** 是原始编号：只在输入不是元素记录时赋为输入位置，此后经任何过滤、配对不变。
  链式过滤时第二层元素的 `index` 仍是最初列表里的位置，直接用 `e.index`。配对产物没有 `index`。
- **`pos`** 是该元素在**这一次调用的输入列表**里的位置（`first_k` 按它排）。
- **`trail`** 是接上来的历次出口列表。配对的 `left` / `right` / `at` 经过滤后仍在元素上（`e.left`），
  原来的 `source` 字段已撤。
- **未决条目就是元素本身**（带 `exit` 与 `cause`），`p.index`、`p.left` 直接读；整体返回
  `undecided(o)` 与 `unobserved(o)` 即转交责任。只返回投影（去掉 `exit` 的记录）不算转交，运行期报 J-05。
- **多题与多填法**：`sieve(items, [q1, …])`、`sieve(items, form, [fill1, …])` 返回**一个**契约值，
  元素 = 材料 × 题（材料主序、题次序），元素带 `q`、`qi`（填法另带整条 `fill` 记录，槽以外的键也在）；
  `by_q(o, k)`（`lib/outcome.jpp`，别名 `by_fill`）取第 k 道题的子契约值。
- **线**：末位可以给选项 `{line: 线}`——`sieve(items, q, {line: {declare: {hi: 0.7, lo: 0.3}}})`、
  `sieve(items, form, [fill1, …], {line: …})`。`line` 的值就是 `cut` 的第二参，原样交给每个元素的 `cut`
  （作者声明线，见「线从哪里来」）。不给时逐元素 `cut(r)`，走 `cut` 的缺省规则；要按自己的判定规则切（多少分算是），
  就给 `line`。
- 程序构造的契约值 `outcome({…})` 不收 `spent`，花费由运行时按 `evidence` 的账本键算（A-2）。

## 内置函数名单

以 `crates/jpp-runtime/src/lib.rs::BUILTINS` 为准（`env` 里同名字会被用户绑定
遮蔽，静态检查报 `W-shadow`，允许但会提示）。

| 类别 | 名字 |
|---|---|
| 材料与状态 | `mat` `content` `state` `transform` |
| 出题 | `test`（是非） `select`（K 选一） `measure`（打分） `form`（题式模板） `fill`（按题式填题） |
| 判断与过桥 | `judge` `cut` `ask` `key_of` |
| 出口处理 | `handle` `consume` `exit_kind` `unsure` `pending` `line_source` `taint` |
| 未决责任 | `unsure_cause` `untested` `escalate` `literalize` |
| 效应与失败 | `do` `gen` `fail` `is_fail` |
| 三路过滤/配对/聚合/迭代 | `sieve` `pair` `tally` `first_k` `iterate` `outcome` |
| 有界控制 | `loop` `stop` `map` `filter` `fold` |
| 读数长处（只读校准线，不花钱、不进账本） | `allocate` `unsure_bound` `repeat`（同题重复读数取均值/中位数，旧名 `agg` 一个版本内仍可用并报 `W-deprecated`） `order`（同尺度排序） `fit`（喂已注册特征） |
| 数据与文本 | `len` `range` `append` `concat` `slice` `contains` `sum` `reverse` `keys` `has` `with` `text` `join` `print` `min` `max` `abs` `floor` |
| 文本与数据（确定性，B157）| `split` `lower` `upper` `trim` `replace` `starts_with` `ends_with` `index_of` `chars` `regex_match` `regex_find` `sort` `sort_by` `parse_json` `to_json` `hash` `date_parse` `date_format` `date_add` |
| 带种子伪随机（B158）| `rand(seed, k)` `rand_int(seed, k, n)` `shuffle(list, seed)` |

`accepted` / `ignored` / `undecided` / `unobserved` / `stopped` **不是内置**，是
`lib/outcome.jpp` 里按契约值字段取值的库函数（`import "lib/outcome.jpp";` 后可用），
契约值形状见 `INTERFACE.md` §三·四·四。

## 确定性处理在 `.jpp` 里写

文本切分、正则、排序、JSON 解析/序列化、哈希、日期解析/格式化这类**纯确定性**处理直接
在程序里写（上表「文本与数据」行），不必再交给宿主或用 `fold` 逐字符拼：它们不记账、
不是刷新点，taint 按「结果携带全部输入 taint 的并集」统一算——任一输入不可信，结果就不可信。

`parse_json`/`date_parse` 解析失败返回 `Fail`（`is_fail(v)` 判），不是运行期错误：

```jpp
let v = parse_json(text);
if is_fail(v) { "格式不对" } else { v.field }
```

**`now` 不是内置，无种子随机也不提供**（B157 (2)）：不确定性只经端口进入，
两次运行同样的程序不能因为读了当前时间或随机数就走出不同的控制流，账本上看不出为什么。
当前时间的两种取法：

- 宿主经 `--input` 给一个时间戳字段（进 `entry_hash`，账本能看到用的是哪个时间）；
- 登记动作 `do("clock:now")`（可逆、成本 0、`taint_out: trusted`，同样进账本，重放拿到同一个值）。

要「每次不同」的随机数，把种子作入口交进来（同样进 `entry_hash`）；`rand`/`rand_int`/`shuffle`
本身对同一个 `(seed, k)` 恒给同一个值（splitmix64，算法与版本号写死，参见测试），
不是「看起来随机」，是「给定种子就是确定的可重放计算」。

## 环境循环三种写法

游戏、交互类程序的「环境」（棋盘、蛇身、agent 位置……）不需要语言加新构造（B159）：

1. **纯函数环境**：环境是一个值（列表/记录），转移函数 `step(env, action) → env'` 是纯
   `.jpp` 函数，配合 `iterate(bound, init, step, measure)` 或 `loop` 跑循环；这条只规定环境是值、
   转移是纯函数，不规定每轮必须判断——动作能由代码直接算出时（如 `examples/env-snake.jpp`：
   往哪走只看食物相对头部的 dx、dy 两个已知整数，谁大走谁，属于代码能定的事，意图汇编 7a）就
   不必花判断；真要在环境循环里判断，才把渲染后的环境当材料（`mat(render(env))`）问 JEV，
   `examples/iterate.jpp` 的 `step` 就是这样，每层判一句话是否提到价格、涉及金额是否低于一百
   元——这类要读懂自然语言的问题才值得花判断。小环境、不依赖外部模拟器时用这条。
2. **宿主动作**：环境状态留在宿主，登记一个动作 `env:step(env_json, action) → env_json'`
   （`reversible: true`、成本 0），结果进账本、重放不重算；物理引擎、第三方模拟器这类
   宿主已有的东西走这条，不必在 `.jpp` 里重写一遍它的转移规则。
3. **入口序列**：环境转移由 `--input` 里逐步给出的观测序列决定，程序只做每步的判断与
   动作选择，宿主按拍调用程序——不需要程序自己知道「下一步会怎样」。

实时时钟、渲染、物理都在宿主：宿主每拍调一次程序（同一账本 `--resume`，或每拍一份账本），
程序里没有时间（见上一节）。

## 线从哪里来

`cut` 把读数（概率）变成出口。**默认不需要线**（意图汇编 11a）：没有记录的线、作者也没写线时，`cut` 按判断器自己的回答走——是非题 p > 0.5 出 `act`、p < 0.5 出 `ignore`；`select` 取概率最大的候选出 `pick`；`measure` 取概率最大的档位出 `at`；恰好并列（p = 0.5，或最大值不止一个）时判断器没有给出回答，出 `unsure(tie)`。报告 `exits` 行的等级是 `Answer`。`sieve`、`search`、`judged_graph`、题树这些组合里没传线的地方都一样，所以新题第一次跑就有结论，不用先标注。

作者想按自己的策略切，就写**作者声明线**：`cut(r, {declare: {hi: 0.7, lo: 0.3}})`。按写的数切（读数 ≥ hi 为 act，≤ lo 为 ignore，其间 `unsure(band)`），不平移、不被同键记录替换，也不作错误率保证。等级 `Declared`，报告 `exits` 行给出同键标注里按这条线切错几条、本趟有多少读数落在线 ±0.2 内。作者写了线就按作者的线切，校准记录里有认证线时也按认证线切。

要语言担保错误率时，用**认证线**（可选工具）。按要花的功夫从少到多有三个来路：

1. **题库**：`bank/bank.json` 里已认证的同题型题式，`fill(题式, {…})` 填上就用，作者一条不标。
2. **真值可算**：标签由程序算出（`source: computed`），经 `calib-import` 导入认证，零人工。
3. **标注或代标**：带真值样本经 `calib-import` 认证（可由强模型代标加人工复核）。

`cut` 的第二、三位还有三种写法：`{cost: [fp, fn]}` 按两种错的代价在已有证书里选线，`{alpha: a}` 在 α ≤ a 的证书里选已决最多的一张，二者的线都来自记录；作者写了这两种而没有对应证书时没有线，按判断器的回答走，另报一条 `W-untested`（载体 `cost_line` / `alpha_line`）告诉你要的证书没找到。停岗的记录同样不供线（B187：`unsure` 的原因里不再有 `cold` 与 `drift`）。直接写一个数 `cut(r, 0.7)` 等于 `cut(r, {declare: {hi: 0.7}})`。声明线另有两个选项：`stat` 指定线切在读数的哪个统计量上（`"expect"` 期望档位、`{mass: [0, 1]}` 几个候选的概率和、`"confidence"` 判断器自报的置信度；`expect` 还可写 `{cuts: [0.5, 1.5, 2.5]}` 分桶出 `at`），`closed: {lo: false}` 让下端变开（读数等于 lo 归中间带，复刻 `elif p >= lo`）。`cuts` 线的端位可以逐个切点写：`closed: {cuts: [false, true, false, true]}` 配 `cuts: [0.5, 1.5, 2.5, 3.5]` 复刻 Python `round()` 的银行家舍入（B176）。线的数也可以是运行期算出来的：`{declare: {hi: rand(seed, k)}}` 按读数比例抽样（B175 (3)），种子只来自字面量或入口，重放同值；同一站点每个不同的线各记一条账本条目。`stat: {mass: […]}` 不写线时按概率和的多数块走（> 0.5 为 act、< 0.5 为 ignore、恰好 0.5 为 `unsure(tie)`）；`expect` 与 `confidence` 上没有现成的回答，不写线报 `E-cut-options`。

**几道题的读数按自己的换算率合成一个分，再在分上写线**（声明式拟合，B153）：

```jpp
let 匹配 = fn(a, b, c, d) { (0.4 * a.expect + 0.35 * b.expect + 0.25 * c.expect) / 3.0 * d.p };
let s = fit({declare: 匹配}, [judge(st, 技能), judge(st, 经历), judge(st, 文化), judge(st, 在招)]);
handle(cut(s, {declare: {hi: 0.6}}), {…})
```

闭包按位收每条读数的统计量记录：是非题 `{p}`，选择题 `{probs, max, argmax}`，打分题再加 `expect`；判断器报了自报置信度时另有 `confidence`。外部的数（价格、权重）放进第三个参数 `extra`，按位接在读数后面：`fit({declare: fn(a, price) { a.p - price }}, [r], [0.65])`。单读数也可以：同一道选择题两个候选的概率差设门写 `fit({declare: fn(a) { a.probs[0] - a.probs[1] }}, [r])` 配 `cut(…, {declare: {hi: 0.10}})`（`probs` 按 `over` 序，B174）。闭包只见参数与内置（`max`、`min`、`if`、下标等），看不见外层的名字，里面不能发判断、不能调效应或构造。结果是 `Score`：它不是数，不能算、不能读，只能进 `cut` 的声明线（`{hi, lo?}` 或 `{cuts: […]}`，数的范围不限）或同一拟合的 `order`（`fit({declare: f, tie: 0.05}, …)` 的 `tie` 是并档带宽）。出口等级 `Declared`；`Score` 不写 `declare` 报 `E-cut-options`（拟合分数不是判断器的回答）。

推荐写法是两侧线加 unsure 臂转人工，读数落在两线之间的由人拍板：

```jpp
handle(cut(judge(state(m), refund), {declare: {hi: 0.7, lo: 0.3}}), {
    act: fn() { 退款(chat) },
    ignore: fn() { "不退" },
    unsure: fn(u) { consume(u, "drop"); handle(ask(state(m), refund), {…}) }})
```

## 可选工具：放行把关（`--guard`）

默认 J++ 相信判断器（B187）：不拦任何 `do`（包括 `write_json` 这类撤不回的动作），不可信标记（taint）照常记录但不拦东西；线等级的说明（`W-fixture-line`、`W-trial-line`、`W-declared-line` 等）不打印，同样的信息在报告 `exits` 行（`grade`、`line_source`、各正交位）；执行器（`exec_py`、`check_tests`、`exec_sql`）有操作系统沙箱就在沙箱里跑，没有就在普通子进程里跑，检查期告警 `W-action-no-sandbox`；有撤不回动作的程序没给 `--ledger-out` 时，账本写到源文件旁的 `<源文件名>.ledger.jsonl`，第一行打出路径，报告另记 `ledger_path`。账本是记录，照常写。

宿主要把关时带 `--guard`（`run` 与 `check` 都收；库宿主用 `EntryArgs.guard`）。开了之后：J-08 在检查期与运行期生效（本节下文，含谱系与题面 taint）；线等级的说明照旧打印；没有沙箱的执行器按不可逆算（B164），要有守卫；有撤不回动作的程序必须给 `--ledger-out`（`E-ledger-required`）。开关进账本头 `entry_hash`（不开时哈希不变），报告多一个 `guard: true`。`scripts/guard_baseline.py` 核对带 `--guard` 跑的金样与翻转前一致。

开 `--guard` 时，登记为不可逆的动作（CLI 的 `write_json`）只有在守卫里至少有一项来自**可信材料上、线放行的已决判断**，或来自 `ask` 时才执行。这份证据只在 `handle` 分派出口时产生：所选臂的守卫栈压上它，臂返回值里的每个 `Bool` 也带上它，经 `let`、字段、下标、函数返回原样带走；`&&` 保留两侧的证据，`||`、`!`、比较（`==`、`>` 等）和其他运算都不带；`if` 不把条件的证据传给分支里造出来的值；未决出口不给证据（B121）。三种写法：

```jpp
// 逐项：do 写进臂里，每个动作由选出它的那次判断放行
map(accepted(r), fn(e) { handle(e.exit, {act: fn() { do("write_json", ["out.json", e.item], 0) },
                                         ignore: fn() { unit }, unsure: fn(u) { consume(u, "drop"); unit }}) })

// 批量：取一个被接受元素的出口作见证（accepted 里的元素都由 Act 选出）
if len(xs) > 0 { handle(accepted(r)[0].exit, {act: fn() { do("write_json", ["out.json", xs], 0) },
                                              ignore: fn() { unit }, unsure: fn(u) { consume(u, "drop"); unit }}) } else { unit }

// 不放行：比较式没有证据；fold 里用 || 累积也没有
if len(xs) > 0 { do("write_json", ["out.json", xs], 0) } else { unit }
```

`let ok = handle(cut(judge(…)), {act: fn() { true }, ignore: fn() { false }, unsure: fn(u) { …; false }}); if ok { do(…) }` 同样合法：`ok` 带着那次判断的证据。

两条会让可信材料上的判断也不放行（步 17b）：题面里填进了不可信文本（`fill` 的槽值、拼出来的题面、`purpose`、`gen` 或入口给的模板），出口随题面不可信（B58）；被判断的材料是由试用线、夹具线等不放行的线或未决出口一路选出来的，出口也不算证据，报文写「该材料由 {等级} 线的出口选出」（谱系放行，B72-4）。

判断器回答（等级 `Answer`）与作者声明线（`Declared`）的出口在把关下默认不算放行证据。宿主确认声明线由自己担责后带 `--release-on-declared`（只在 `--guard` 下有意义；库宿主用 `EntryArgs.accept.declared_lines`）：声明线出口的 `releases` 变为真，报告多一个 `accept: {declared_lines: true}`，开关进账本头 `entry_hash`，换开关状态重放报 `W-header: entry_hash`。这个开关的意思是「这些线由我担责」，不是「这些线是对的」；它也不改材料的可信与否，不可信材料上的判断带了开关照样不放行。

`--input-trusted`（步 14b-1，B108）让 CLI `--input` 材料上的判断也能作可信合取项；它只是说「这份材料来自我信任的来源」，不是说「内容正确」——出口仍要经认证过的线放行才算证据。这个声明进 `entry_hash`（像其他入口条目的 taint 一样），换一次 `--input-trusted` 状态重放会报 `W-header: entry_hash`。

## 真机运行

固定观察只验证程序结构，不产出真实读数；要拿真实判断，需要 `--features live` 编译并
带 `~/.typesafe-key`：

```sh
cargo build -p jpp --features live --release
./target/release/jpp run examples/sieve.jpp --backend live --profiles-dir profiles --calib calib --ledger-out ledger.json
./target/release/jpp run examples/sieve.jpp --replay ledger.json   # 事后离线重放，逐字节一致
```

真机运行必须带能力画像（B73）：`--profile <文件>`，否则 `--profiles-dir <目录>/<model>.json`，再否则 `jpp` 可执行文件旁的 `profiles/`；找不到报 `E-profile-missing`。仓库附带 `profiles/jev-1.13.0.json`，价格与 δ 都从它读，它的哈希进账本头。重放不发调用。证书记下认证时的 δ（B104），判区按证书的 δ 取（B187：旧的「取更严者」与 `W-delta-mismatch` 已退役）；记录没有 δ 时取画像 δ（没有取 0），出口记 `delta_unknown`。要逐字节复现真机运行，重放时给同一份画像。没有记下 δ 的旧证书、没有范围指纹的记录（认证集没有材料文本），出口照常路由，开 `--guard` 时不放行不可逆 `do`（`W-delta-unknown`、`W-scope-unknown`），重新导入（标注行带 `text`）即可。

真机只给读数（概率）。新题第一次跑，`cut` 就按判断器的回答把读数变成 act/ignore/pick/at
（等级 `Answer`，见「线从哪里来」）。要语言担保错误率时（可选），标真值再导入认证线
（J-03 禁止程序自己写线，线只从记录来）：

```sh
# labels.jsonl 每行：{"key": "cs-refund", "item": "c1", "p": 0.99,
#                     "label": true, "source": "computed"}
./target/release/jpp calib-import labels.jsonl --calib-out calib
```

**样本量**：默认 `--alpha 0.1 --conf-delta 0.1` 下，认证的每一侧（act 一侧、ignore
一侧）各自至少需要 **22 条零错误的已判定样本**。缺省的固定序认证（B86）不拆分样本，
一道字面题式约 **60 条**、语义题式约 **60–80 条**带真值的标注即可正式上岗；试用线
（`--alpha-trial 0.25`，只路由）约 32 条（`jpp calib-import --help` 里写明，出处是离线对照
`地基/评估/2026-09-24-新题标注门槛-对照/results.md`）。`--certify split` 是旧的拆分认证，条数约 2–3 倍。
样本不够时不报错、也不瞎放行，而是停在「待核」，例如 5 条正例、5 条负例的构造真值：

```
"gate": "待核：样本不足（正例 5、负例 5，固定序零错误也需每侧已决 ≥ 22）"
```

**待标清单与标注包（B88、B107、B120）。** 用首跑账本导出清单：
`jpp calib-import --from-ledger 账本 --key 键 --list-out 清单.jsonl --report report.json [--materials 材料.json]`。
清单每行带 `item`（材料的状态哈希）、`q`（题哈希）和组号；带 `--report` 时另附题面 `template` 与填法 `fill`，
标注者看得见问的是哪道题。一道题式多个填法问同一批材料时，同一个 `item` 会出现多次，每次 `q` 不同：
标注行要原样带上 `q`，回填按 `(item, q)` 接回读数；不带 `q` 而该材料有多个读数时报 `E-list-ambiguous`。
清单、标注包、复核包里的编号、行序、文件名**不得携带材料生成者的任何信息**（设计意图、真值、分组、正反例后缀）：
清单的状态哈希满足这一条；手工出包要用不透明编号并打乱顺序，否则整批作废重标（`地基/题库/规范.md` §1.3）。
回填时带 `--report`，记录的题类取运行时算出的精化类（例如一对材料上的题是关系类）。

样本不足时该题没有认证线，出口按判断器的回答走（开 `--guard` 时不放行不可逆的 `do`）。只有模型自己
标注、没有人工真值时，还需要同一题式的人工抽检一致率过 `--spot-check-min`（默认
0.9）才准临时上岗（`W-provisional`），否则线停在「待真值」。

## 搭配：元素 → 组合 → 嵌套

「搭配」是判断器（JEV）与生成器（大模型）、执行器、精确算法、检索这些组件搭在一起的写法。它是
J++ 的底层，不是可选功能：判断是唯一产出读数的效应，别的组件各带自己的画像（成本、时延、是否
确定、错不错），搭配层规定它们怎么互相喂材料、怎么互相接未决责任。这一章按三层展开，对应
`00-Nature意图汇编.md` 第 4、5 条：**最小的搭配是一个元素**（一次判断、一次生成、一次执行……），
**元素组合成更大的搭配**（搜索、判出来的图、接地核验……），**组合本身还能再组合**（搜索里嵌接地、
图的产物再判、再建一张图、循环里跑多个独立的搜索）——组合的产物与元素同一种契约值形状，能再
交给下一层 `sieve`、`pair`、`map`，这是「组合封闭性」在搭配层的体现，不是额外规定的接口。

跨层时，未决的去向、误差界、预算、账本与重放、放行把关都由语言与库自动带着走，不需要在每一层
重新手写：`sieve`/`search`/`graph.jpp` 的产物都自带 `pending`，出口都能 `compose`，账本键跨层
仍然唯一。这正是「换判断器程序不改一个字」「一句顶一百句」这两条验收在搭配层的落点：写一次
`search`，判断器从固定观察换成真机、生成器从占位换成 `claude -p`，程序不用动。

**JEV 判什么、代码判什么（判断分工定律，B181/B182）。** 一道题满足下面三条才交给 JEV，缺一条按
对应修法处理：

1. **无确定算法**——从材料到结论没有代码能写出的确定规则（相等、包含、字段存在、计数、排序、
   算术都有）。不满足：改用 `.jpp` 内置确定性函数，或在 `search`/`verify` 里用 `opts.keep`（下面
   「ground / verify」一节）。
2. **证据已在材料里**——不需要再运行、取、算、渲染。不满足：先用 `do`/`transform`/`gen` 把证据
   摆到材料里（接地），再判。
3. **一跳一命题**——题问一个命题，答案是划分上的一块；命题可以任意深，但不能是「且/或」拼起来的
   复合题。不满足：拆成题树，每层一跳，由上一层的出口选下一层问哪道题。

反例（示例、文档、题库不得再出现这类题面）：

| 题面 | 为什么不该交 JEV | 改成 |
|---|---|---|
| 这段代码的输出是否等于 385 | 预期值已知，相等比较 | `trim(stdout) == "385"` |
| 这个 JSON 是否有字段 F（字段名必须完全一致） | `has(parse_json(t), F)` | 代码 `has(...)` |
| 这段话里直接写出了下列哪个城市的名字 | `index_of`/`contains` 能算 | 代码 `index_of` |
| target < 500 这类数值比较 | 数值比较 | 代码 `<` |
| 这条待办是否写了「今天/马上/尽快」 | 词表匹配 | `regex_match` |
| 这条评论是否包含「退款」这个词 | 字符串包含 | `contains` |

三条判据全满足的题，即使语义很深也该交给 JEV，不必先拆浅（意图汇编 7b/7c）：「这两个人适不适合
合作」「透露这条信息会不会推进合作」都是无确定算法、证据已在材料里、一跳一命题的语义判断。

本节的每个签名以 `lib/compose/*.jpp` 源码为准。**`地基/比赛/搭配用法手册.md`（施工前写的草稿）
里的部分签名已经过时**——最常见的两处：`ground` 手册写成两个参数 `ground(action, render)`，
源码是三个参数 `ground(action, args_of, render)`（执行器要三个实参：代码、标准输入、超时秒数）；
`search` 手册把 `rounds` 塞进 `opts` 里，源码里 `rounds` 是单独的位置参数、写在 `opts` 前面。
本节之后，手册以本章为准。

每段代码都在 `examples/guide/` 下有对应的可运行文件，配好了固定观察夹具；`scripts/guide_check.py`
逐个用 `jpp check`、能跑的再 `jpp run --fixtures` 实际跑过一遍，不是誊抄示意伪代码。

### 元素

元素是搭配里不可再拆的一次调用：一次判断、一次生成、一次执行、一次精确算法、一次检索，或者
纯确定性的文本与数据处理。它们各自的成本、是否确定、要不要沙箱都不一样，写法上却有一个共同点：
输入输出都是普通值或材料，不夹带读数（数字不流，`00-目标与动机` §四）。

**一次判断（judge + cut）。** 一道题（`test`/`select`/`measure`）问在一份材料（`state(mat(...))`）
上，`judge` 拿到读数（概率分布），`cut` 按线把读数切成三值出口（act / ignore / unsure），`handle`
把三个出口分派到三段代码。线从哪里来（默认按判断器回答走）、`declare`/`stat`/`closed`/`--guard` 怎么用，
本文件前面「线从哪里来」「可选工具：放行把关」两节已经讲透，这里不重复，只给最小可运行的样子。
这道题要理解语义才能判——客户在抱怨、还是在明确要求退款，材料把两者混在一起，不是关键词匹配能
分的事（意图汇编 7a：相等、数值比较、格式、是否为空这类字面可算的事不该占这个位置）：

```jpp
budget {calls: 4, cost: 0.01, depth: 8};
let refund_ask = test("这条客户留言是不是在明确要求退款？", "customer-refund-request");
let r = judge(state(mat("这台空气净化器用了不到一周就一直报警，联系客服说要寄回厂家检测，我等不及了，东西你们拿回去，钱退给我就行。")), refund_ask);
handle(cut(r), {
    act: fn() { "退款：升级处理" },
    ignore: fn() { "非退款请求" },
    unsure: fn(u) { consume(u, "drop"); "线附近，转人工" }})
```
（`examples/guide/elem-judge.jpp`）统计量线（`stat: "expect"`/`{mass: […]}`/`"confidence"`）与
两端开闭（`closed`）见 `examples/guide/elem-declare-stat.jpp`；作者声明线守不可逆动作见
`examples/guide/elem-declare-accept.jpp`（默认不开把关，act 臂的 `write_json` 照常执行；开 `--guard`
时不带 `--release-on-declared` 的声明线出口只路由不放行，带上之后才放行）。

**生成（gen）。** 需要解空间里本来没有的东西（候选、模板、测试、分类）时才调，已有的东西里挑
用判断，不调生成器（`00-Nature意图汇编` 第 6 条）。`gen(题面, ctx, n, retry_seq)` 是非阻塞的：
调用在本层刷新点登记、发出去，程序照常往下跑，结果只在第一次真正被读到的地方等（惰性求值），
所以同一层里独立的几个 `gen` 与判断会重叠执行，不必手动并发。产物默认 `taint: untrusted`（生成器
画像声明），失败时返回 `Fail`，不中断程序：

```jpp
import "../../lib/outcome.jpp";
budget {calls: 4, cost: 0.01, depth: 64};

let brief = mat("为一家社区咖啡馆起名：温暖、好记、不超过四个汉字");
let names = gen("按下面的需求提出 3 个候选店名，每个候选是一个字符串", [brief], 3, 0);
let fits = test("这个名字适合做一家社区咖啡馆的店名吗？", "gen-choose-fit");

if is_fail(names) {
    {failed: text(names), chosen: [], rejected: [], pending: []}
} else {
    let r = sieve(names, fits);
    {chosen: map(accepted(r), fn(e) { e.item }),
     rejected: map(ignored(r), fn(e) { e.item }),
     taints: map(names, fn(m) { m.taint }),
     pending: r.pending}
}
```
（`examples/guide/elem-gen.jpp`）真机下把固定观察换成 `--gen-model sonnet`（`claude -p`），程序
不改；重复调用按账本键（题面、语境哈希、`n`、`retry_seq`）复用，配 `--gen-cache` 跨运行也不重发。
生成物应当按缓存复用，日常运行只判、不再生成——搭建一次、日常只判的完整写法见「搭配用法手册」
§六（未过时的部分）。

**执行（exec_py / check_tests / exec_sql）。** 判断只看字面（〇.1）：代码本身不是字面材料，只有
跑出来的输出才是。三个执行动作有操作系统沙箱时在沙箱里跑（B164）：临时目录是唯一可写处、断网、有
超时。没有沙箱时直接在普通子进程里跑（同一个临时工作目录、静态拒绝表与断网补丁照旧，没有系统级隔离），
检查期报 `W-action-no-sandbox` 告警，照常出结果（B187，不再有 `Fail(NoSandbox)`）。没有沙箱的执行器
按不可逆算：默认只是多写一份账本；开 `--guard` 时处在由不可信判断的出口决定要不要走的分支里（例如放进
`search` 每一轮的 `opts.ground`）会撞上 J-08。`exec_sql` 只放行
只读语句（`query_only`），要 `SELECT` 一个已经建好的库，不能建表；写数据、改库不属于这三个动作。

```jpp
budget {calls: 4, cost: 0, depth: 8};
let out = content(do("exec_py", ["print(sum(range(1, 11)))", "", 5], 0));
{stdout: out.stdout, exit_code: out.exit_code, timed_out: out.timed_out}
```
（`examples/guide/elem-exec.jpp`）把执行输出交给判断，是下一节「组合：ground / verify」做的事，
这里只演示执行本身。`exec_py`/`check_tests` 的实参形状是 `[代码, 标准输入或测试, 超时秒数]`。

**精确算法（graph:\*）。** 六个动作：`graph:matching`（带权最大匹配）、`graph:shortest_path`
（Dijkstra）、`graph:max_clique`（Bron–Kerbosch）、`graph:components`（并查集）、`graph:set_cover`、
`graph:max_flow`（Dinic）。全部纯函数、可逆、成本 0。边权是程序自己给的常量，不是判断读数——
数字不流；判断出来的图（读数决定「有没有这条边」）在下一节。

```jpp
budget {calls: 4, cost: 0, depth: 8};
content(do("graph:matching", [{edges: [{u: 0, v: 1, w: 3}, {u: 1, v: 2, w: 5}, {u: 0, v: 2, w: 1}], nodes: [0, 1, 2]}], 0))
```
（`examples/guide/elem-graph-algo.jpp`）`set_cover`、`max_flow` 的产物形状与另外四个不同，
`lib/compose/graph.jpp` 的 `on_graph` 不接这两个，要用就直接 `do`。

**检索（embed_topk / bm25_topk）。** 检索只负责召回，不负责精度：候选数万级时先粗筛缩到几百，
再交给判断逐条精判（「搭配用法手册」§一「先便宜后贵」）。`bm25_topk(query, corpus, k)` 纯 Rust
实现，任何机器上都能跑，不需要联网或本地模型：

```jpp
budget {calls: 4, cost: 0, depth: 8};
content(do("bm25_topk", ["苹果", ["苹果 香蕉 橙子", "今天天气很好", "苹果派配咖啡"], 2], 0))
```
（`examples/guide/elem-retrieval-bm25.jpp`，返回 `[{id, score}, …]` 按分数降序。）`embed_topk
(texts, query, k)` 子进程调离线的 sentence-transformers（默认模型
`paraphrase-multilingual-MiniLM-L12-v2`），语义相近但字面不重合时才需要它；子进程要本机已经
缓存过这个模型，比赛现场的机器不一定联网下载过，`examples/guide/elem-retrieval-embed.jpp` 只
`check` 不 `run`（静态检查不执行动作，不依赖模型是否在本机）。字面能重合就优先用 `bm25_topk`。

**文本与数据内置（确定性，B157）。** `split`、`trim`、`regex_match`/`regex_find`、`parse_json`、
`hash`、`date_parse`/`date_format` 这类纯确定性处理直接写在 `.jpp` 里，不进账本、不是刷新点。
`regex_find` 返回全部整段匹配组成的列表，**不返回捕获组**（`re.find_iter` 逐个整段匹配，不看
括号）；要提取子串就把要的部分整段写进正则，不要指望括号里的内容单独出现在结果里：

```jpp
budget {calls: 0, cost: 0, depth: 8};
let raw = "  订单号=A203, 金额=199.50, 状态=待发货 ";
let trimmed = trim(raw);
let parts = split(trimmed, ", ");
let amount = parse_json("199.50");
let has_ship = index_of(trimmed, "待发货") >= 0;
let m = regex_find(trimmed, "订单号=[A-Z0-9]+");
{trimmed: trimmed, parts: parts, amount: amount, has_ship: has_ship, matched: m, fingerprint: hash(trimmed)}
```
（`examples/guide/elem-text.jpp`）完整名单与 `now`/随机数的处理见本文件前面「确定性处理在 .jpp
里写」一节，这里不重复。

### 组合

组合是库函数（`lib/compose/*.jpp`），不是新的语言构造：全部写在 `pair`、`sieve`、`outcome`、
`compose`、`element` 这些已经开放的内核构造之上，`import` 之后当普通函数用，能作参数传、能嵌进
`map`/`filter`，能嵌进另一个组合。每个组合把「元素怎么循环、未决怎么去向、出口怎么合成」收进一个
函数，调用者只管给判断题、给候选来源、给要跑的算法。

**组合里的线由作者写。** 组合里的判断都经 `sieve` 切出口。不给线时每个元素走 `cut` 的缺省规则（「线从哪里来」
一节）；你要自己定「多少分算是、多少分算否、中间拿不准的交给谁」，就在组合的 `opts.line` 里写，写法与 `cut`
的第二参相同，组合把它原样交给每一次判断：

```
search(seed, propose, feasible, objective, 3, {width: 2, line: {declare: {hi: 0.7, lo: 0.3}}})
verify(cands, grounder, q, {line: {declare: {hi: 0.7, lo: 0.3}}})
judged_graph(nodes, edge_q, {line: {declare: {hi: 0.7, lo: 0.3}}})
sieve(items, q, {line: {declare: {hi: 0.7, lo: 0.3}}})
walk(tree, material, 4, {line: {declare: {hi: 0.7, lo: 0.3}}})     // 题树同一个写法
```
出口等级 `Declared`，报告 `exits` 行有每个出口的证据量（开 `--guard` 时另打印 `W-declared-line`）；这些出口
默认直接驱动撤不回的动作，开 `--guard` 时按「可选工具：放行把关」一节的规则。`search` 另有 `objective_line`，给目标题单写一条。本节下面的
示例在固定观察下跑，夹具里带了校准记录，没写 `line`；换成真机、要按自己的策略切时补上。

**search（提出 → 判 → 再提出）。** 解空间列不完，但能从好的部分解长出更好的解时用：写方案、写
代码、起名字。生成器负责提，判断器负责判，有界迭代负责收敛。

```
search(seed, propose, feasible, objective, rounds, opts) -> 契约值
  seed      第一轮前沿（材料列表）
  propose   fn(前沿, 轮次) -> [Mat]，通常包 gen；返回 Fail 时这一轮零候选，记一条 Unsure(fail)
  feasible  是非题：候选在可行域里吗
  objective 是非题、打分题（measure，只排序，见下），或 unit（只按可行域筛）
  rounds    轮数上限，写字面量或顶层 let 常量（调用点核，B111）；喂一个不可判定的表达式
            （函数参数、`len(...)` 之类）不是硬错误，是 `W-bound` 警告：静态估不出上界
  opts      {width, unsure_to?: "carry"|"refine"|fn, ground?: fn(Mat) -> Mat, rank?: {stat?, tie?},
             keep?: fn(Mat) -> Bool, line?, objective_line?}
            line：可行域题的线，也是是非题目标的缺省线；objective_line：目标题另给的线（打分题或 unit
            目标上给了报 E-search-options）。写法同 cut 的第二参；不给走 cut 的缺省规则
```
好候选按轮累积：每轮的新候选去重（按材料哈希，判过的不再判、不花钱）、`opts.ground` 给了就先接地、
`sieve(可行域)`、`objective` 不是 `unit` 再 `sieve(目标)`，新判好的并进累积池，下一轮前沿取池的前
`width` 个。终止用 `iterate` 的线：池凑够 `width` 即 `stop`；一轮没有新的好候选（含只有重提旧候选）
即 `noshrink`；轮数用尽即 `bound`。未决按 `opts.unsure_to`：`"carry"`（缺省）并进 `pending`；
`"refine"` 另把未决材料并进下一轮前沿，让生成器据此改进（出口照样进 `pending`，不因细化销账）；
给一个函数就逐轮交给它，交人用 `lib/compose/ask.jpp` 的 `ask_human`（程序要写 `budget {escalate: N}`，
J-07 会静态查）。返回值 `value` 是累积前沿（每个元素带 `round`：第一次判好的轮次），出口是
`trail` 上全部出口与自己出口的 `compose(…, "all")`；`detail = {ignore, rounds, reason, measures,
fails, duplicates}`。

`objective` 是打分题（`measure`）时它只排序、不过滤：可行的候选都进累积池，新进的各判一次打分题，
每轮按 `order(读数们, opts.rank)` 排（缺省按档位，`rank: {stat: "expect"}` 按期望档位），取前
`width` 个作前沿与 `value`（最好的在前）。前 `width` 个里进了新候选才算进展，与末位并列的新候选挤不掉
旧的；打分题没有「凑够」，所以只有 `noshrink` 与 `bound` 两条终止线。打分题不产生出口，`value` 元素的
出口只合成可行域那道题。示例 `examples/search-rank.jpp`。

```jpp
import "../../lib/compose/search.jpp";
budget {calls: 8, cost: 0.01, depth: 64};

let brief = mat("为一家社区咖啡馆起名：温暖、好记、不超过四个汉字");
let propose = fn(frontier, i) {
    gen("按下面的需求提出 3 个候选店名，每个候选是一个字符串；需求之后的上下文是上一轮留下的候选", concat([brief], frontier), 3, i)
};
let fits = test("这个名字适合做一家社区咖啡馆的店名吗？", "search-fit");

let r = search([], propose, fits, unit, 3, {width: 2});
{kept: map(r.value, fn(e) { e.item }), found_in: map(r.value, fn(e) { e.round }), duplicates: r.detail.duplicates,
 rejected: map(r.detail.ignore, fn(e) { e.item }),
 reason: r.detail.reason, rounds: r.detail.rounds, measures: r.detail.measures,
 pending: r.pending}
```
（`examples/guide/comp-search.jpp`，第一轮就有两个合适的候选，凑够 `width = 2`，`reason: "stop"`。）
轮数用尽、一轮不再有新候选的两种收尾分别见 `examples/search-bound.jpp`、`examples/search-noshrink.jpp`
（未收进 `examples/guide/`，行为与上面同一族，读法一样）。

**ground / verify（执行 → 判）。** 要判的东西不是字面的：代码对不对、查询答没答对。**能否运行、
数值相不相等、格式对不对、结果是不是空这类代码自己就能算的事，先用普通代码筛掉，不进 JEV、不花
一次调用；JEV 只判筛剩下的那些没有标准答案、要理解语义的问题**（意图汇编 7a——早先这一节的例子
让 JEV 判「代码输出是否等于 385」，这正是代码能定的事，已改成下面这条）。代码核的：跑得通、有
结果；JEV 判的：结果是否真的回答了需求（B181 判断分工定律的 E9 重述）。需求的口径（时间范围、
统计维度、取舍规则……）写进需求材料本身，题面只问一句「这个结果回答了需求里的问题吗」——不拆成
几道子命题让判断逐条核对（B182 (3) 一跳一命题，B0366）。

```
ground(action, args_of, render) -> fn(候选材料) -> 材料 | Fail
  opts.keep?   fn(材料) -> Bool。代码谓词（B184）：ground 之后、feasible 之前对每个新候选求值，
               假即排除——不进 sieve、不花调用、不进 pending，记 detail.excluded: [{item, round}]。
verify(cands, grounder, q, opts) = sieve(kept, q, {line: opts.line})
  opts.keep?   kept 按它过滤，语义同 search 的 opts.keep
  opts.line?   判断的线，写法同 cut 的第二参；不给走 cut 的缺省规则
```
`ground` 返回一个函数值：对每个候选跑 `do(action, args_of(候选), 0)`，失败原样返回失败值，否则把候选与执行输出一起渲染成材料（`render(候选, 输出)`）。
`render` 可以返回文字，也可以返回记录——推荐返回记录：结构化字段（`error`、`rows`、`exit_code`
之类）供 `opts.keep` 读，渲染文字供 JEV 读，同一份材料两路用。taint 承接执行输出（执行器为
`untrusted`）。`ground` 的返回值可以直接放进 `search` 的 `opts.ground`——这就是「提出 → 执行 →
判 → 再提出」的闭环，不是第四个原语。`opts.keep` 在接地之后代码判定「不可行」的候选，是判断分工
定律谓词半在库里的落点（B181/B184）：代码已经能确定的结论不必再花一次判断调用、不必再背一条未决
责任。

```jpp
import "../../lib/compose/search.jpp";
import "../../lib/compose/ground.jpp";
budget {calls: 20, cost: 0.01, depth: 64};

let db = "examples/fixtures/returns.db";
let brief = mat("写一条 SQL 查询，回答一个统计问题：需求要求只统计上个季度（2026 年 4 月 1 日到 6 月 30 日）的退货记录，按商品类别汇总数量，找出退货数量最多的一类。假设今天在 2026 年第三季度。数据库表 returns(id, category, product_name, quantity, return_date)");
let propose = fn(frontier, i) {
    gen("按下面的需求提出 3 条 SQL 查询，每条是一个字符串；需求之后的上下文是上一轮留下的候选（SQL 与执行结果）", concat([brief], frontier), 3, i)
};
// 接地：库函数 ground()（B148/B184）。render 返回记录：结构化字段（error、rows）供下面的 keep
// 读，渲染文字（text）供 JEV 读，且把需求原文（含口径）一起摆进这份材料——JEV 判的是「结果对不对
// 上需求」这一句，不必自己记住口径也不必被拆成三道子命题（B184 §3·(3)；B0366）。
let run_sql = ground("exec_sql", fn(c) { [db, content(c)] }, fn(c, o) {
    let r = content(o);
    let rendered = "需求：" + content(brief) + "\nSQL：" + content(c) + "\n" +
        (if r.error != unit { "报错：" + r.error }
         else if len(r.rows) == 0 { "结果为空" }
         else { "结果：" + text(r.rows[0][0]) + " 共 " + text(r.rows[0][1]) + " 件" });
    {sql: content(c), error: r.error, rows: r.rows, text: rendered}
});
// 代码谓词（B184）：SQL 报错或查不出行的候选在这里排除，不进 sieve、不花一次判断调用。
let keep = fn(m) { let r = content(m); r.error == unit && len(r.rows) > 0 };
let answers_question = test("这条查询的结果是否回答了需求里的问题？", "sql-answers-question");

let r = search([], propose, answers_question, unit, 3, {width: 1, unsure_to: "refine", ground: run_sql, keep: keep});
{kept: map(r.value, fn(e) { e.item }), found_in: map(r.value, fn(e) { e.round }),
 rejected: map(r.detail.ignore, fn(e) { e.item }),
 excluded: map(r.detail.excluded, fn(e) { {item: e.item, round: e.round} }),
 undecided: map(r.pending, fn(p) { {item: p.item, cause: p.cause, via: p.via} }),
 reason: r.detail.reason, rounds: r.detail.rounds, measures: r.detail.measures, duplicates: r.detail.duplicates,
 pending: r.pending}
```
（`examples/guide/comp-ground-verify.jpp`，与 `examples/search-ground.jpp` 同一份程序：第 1 轮
3 条候选——不限定季度（错范围，家居 60 件，`keep` 通过，JEV 判否：没答上「只算上个季度」这条口径）、
列名打错的 SQL（跑不通，`keep` 排除，不进 JEV）、限定到上季度但漏了 6 月 30 日一天（边界，数码 25
件，`keep` 通过，JEV 判不确定，落带内）。`keep` 排除的候选与判过拿不准的候选一起，经
`unsure_to: "refine"` 并进第 2 轮的生成器上下文——它们是接地成功、代码判定不可行的正常记录材料，
不是 `Fail` 值，可以放心进前沿（B184 §3.2）。第 2 轮 3 条——补全边界后的正确查询（数码 40 件，
JEV 判对，凑够 `width = 1`，`stop`）、限定季度但按商品分组（错颗粒度，耳机 25 件，`keep` 通过，
JEV 判否：答的是哪个商品而不是哪个类别）、查一个不存在类别的 SQL（查不出行，`keep` 排除，不进
JEV）。6 次 `exec_sql` 只换来 4 次 JEV 判断，且这 4 次问的都是同一句「结果回答了需求吗」，需求的
口径（时间范围、汇总维度、取最多）已经写在渲染进材料的需求原文里，判断不用再逐条核对三个条件
（B182 (3) 一跳一命题，B0366），也没有一次在问字面可算的事。金样不在检查期执行 SQL——本例的
`guide_check.py` 从已经在有 `sandbox-exec` 的机器上真跑一次录下的种子账本
`tests/golden/search-ground/seed.ledger.jsonl` 用 `--resume` 续跑，什么都不需要重新执行——种子
账本里已经带着两轮全部的 `gen`/`exec_sql`/`transform` 记录，也带着 4 条 `judge` 记录（判断在固定
观察下按键从 `--fixtures` 的 `observations` 取，但同一次真实运行会把取到的答案连同其余效应一起
写进账本；`--resume` 续跑时判断键若命中账本就直接读账本，不必再命中夹具，两条路径都能到，种子录制
时两条一起落盘）。种子是在有 `sandbox-exec` 的机器上把整个程序跑一遍录下的，`--resume` 只是把这些
记录接上。没有沙箱的机器上默认也能跑：执行器没有沙箱时直接在普通子进程里跑，检查期只报
`W-action-no-sandbox`（B187；`工程-步25e.md` §二·4「没有沙箱时照样逐字节通过」这条断言因此重新成立，
在 `JPP_FORCE_NO_SANDBOX=1` 下的核对见过程记录 `工程-默认相信判断器.md`）。这里的
`do("exec_sql", ...)` 现在经由库函数 `ground()` 调用，动作名不是字面量（`ground.jpp` 内部把它当
形参 `action` 用），CLI 按可能不可逆处理：没给 `--ledger-out` 时账本写到缺省路径，开 `--guard` 时
要求 `--ledger-out`。**排除放在第 1 轮（不是最后一轮）**：
旧版把会被代码筛掉的候选特意安排到最后一轮，是为了绕开「`fail(...)` 构造的真 `Fail` 值不能喂给
`gen`」；`opts.keep` 排除的材料从未变成 `Fail`，本例特意把它放进第 1 轮，验证它确实经 `refine`
进入第 2 轮的生成器上下文。

**judged_graph / judged_bipartite / on_graph / interval（判出来的图）。** 决策是全局的、不是逐条
的：配对、分组、找路时用图算法；但边要判断给出。边有三种状态：已决有、已决无、未决。算法只在
已决边上跑；未决边另跑一次「乐观图」（已决 ∪ 未决），两次产物之差（`differs`）就是该先问人的边。

```
judged_graph(nodes, edge_q, {over?, prune?, line?})        单集合、无向：候选取 i < j
judged_bipartite(left, right, edge_q, {over?, prune?, line?})  两组节点，右侧节点号从 len(left) 起
  line：判边的线，写法同 cut 的第二参；不给走 cut 的缺省规则
on_graph(g, algo, args, mode)   mode: "decided"（只用已决边）| "optimistic"（已决 ∪ 未决）
interval(g, algo, args) -> {lo, hi, exit, differs, complete, same, alpha_bound, n_unknown}
```
`prune(nodes)`（或 `prune(left, right)`）给候选下标对，通常来自向量粗筛；不给就全配对（`over`
可以再筛一遍原节点）。`on_graph` 的 `algo` 只认 `matching`/`shortest_path`/`max_clique`/
`components`；`args.weight: fn(边) -> 数` 只能给非判断来源的权，缺省 1——数字不流，边权不能是
读数。产物元素带 `item`，能再交给 `sieve`、`pair`，或作下一张图的节点。

```jpp
import "../../lib/compose/graph.jpp";
budget {calls: 16, cost: 0.01, depth: 64};

let tasks = ["任务：写登录页", "任务：设计数据库表", "任务：写接口文档", "任务：搭部署脚本", "任务：做数据看板", "任务：写单元测试"];
let people = ["小林：前端三年，做过登录与注册", "小周：后端，主写数据库迁移", "小吴：技术写作，维护过接口文档", "小郑：运维，写过部署流水线", "小王：数据分析，常做报表", "小陈：测试开发，熟悉单元测试框架"];

// 候选对由调用者给（通常来自向量粗筛）；这里写死十对
let cand = [[0, 0], [1, 1], [2, 2], [3, 3], [4, 0], [5, 1], [0, 4], [1, 5], [4, 4], [5, 5]];
let can_do = test("a 描述的这项任务，b 这个人能接下来吗？", "graph-can-do");

let g = judged_bipartite(tasks, people, can_do, {prune: fn(l, r) { cand }});
let t = interval(g, "matching", {});

{assigned: map(t.lo.value, fn(p) { p.nodes }),
 if_all_yes: map(concat(t.hi.value, t.hi.pending), fn(p) { p.nodes }),
 ask_first: map(t.differs, fn(e) { [e.left, e.right] }),
 same: t.same, complete: t.complete, alpha_bound: t.alpha_bound, n_unknown: t.n_unknown,
 edges: {yes: len(g.edges), no: len(g.rejected), open: len(g.pending)},
 pending: concat(t.hi.pending, g.pending)}
```
（`examples/guide/comp-graph-interval.jpp`：十条候选边里两条落在带内，只信已决边能分出 4 项，
把未决边都当有能分出 6 项，`differs` 恰是那两条。）`g.pending` 一定要整体交出去——只带
`t.hi.pending` 不够，没被乐观产物用到的未决边会漏掉（同一条边在几处的责任是同一份，B162）。
`alpha_bound` 读作「这组结果里至少一条边判错的概率上界」：未经认证的线（夹具线、声明线、试用线、
类线、冷线）一律按 1 计，`n_unknown` 就是有几条这样的边，`alpha_bound = 1` 说明还没有能担保的线。
六个 `do("graph:*")` 动作名是拼出来的（`join(["graph:", algo], "")`），CLI 按可能不可逆处理，
没给 `--ledger-out` 时账本写到源文件旁的 `<源文件名>.ledger.jsonl`（开 `--guard` 时要求 `--ledger-out`）。

**compose 与 cert（出口合成的原语）。** `search`、`graph.jpp` 内部都靠这两个原语把多个出口合成
一个、读出联合的误差界；直接用它们，是搭一个新组合（不是套现成的 `search`/`graph`）时才需要的
写法。

```
compose(出口们, "any" | "all" | "min" | {first: k} | {sup: 候选下标…}) -> Exit
cert(出口) -> {grade, alpha, alpha_bound, n_unknown, line}
```
`compose(…, "all")` 只在全部分量都放行时才放行，结果未决时吸收全部未决分量的责任。`cert` 只读，
不销账：单个出口的 `alpha` 是它自己的置信度上界，合成出口的 `alpha` 是 `unit`、界读 `alpha_bound`。

```jpp
budget {calls: 4, cost: 0.01, depth: 8};
let a = test("材料 a 是否满足条件一？", "cond-a");
let b = test("材料 b 是否满足条件二？", "cond-b");
let ea = cut(judge(state(mat("材料 a 的内容")), a));
let eb = cut(judge(state(mat("材料 b 的内容")), b));
let joined = compose([ea, eb], "all");
{joined: exit_kind(joined), cert: cert(joined)}
```
（`examples/guide/comp-compose-cert.jpp`：两条都是夹具线，`cert.grade` 是 `"Cold"`、
`alpha_bound` 是 `1.0`——没有认证过的线，界只能报到最保守。）

**element（元素记录构造的原语）。** `sieve`、`search`、`ground` 内部都用它把「材料 + 出口」造成
一条标准形状的元素记录（带 `item`/`pos`/`trail`/`exit`/`cause`/`q`/`qi`/`key`）。它做两件
`.jpp` 代码本身做不到的事：`item` 只加一条「选择依赖边」（读数只是选中了这份材料，不代表内容由
这道题派生出来，J-02 因此不会误拦「同一材料再问一道题」）；报告的 `exits` 表里补上这一行的
`index`/`pos`。库作者自己搭新组合时才需要直接用它：

```jpp
budget {calls: 4, cost: 0.01, depth: 8};
let q = test("这份材料是否合格？", "elem-ok");
let e = cut(judge(state(mat("样品 A")), q));
let el = element(mat("样品 A"), e, {pos: 0, q: q, key: "elem-ok"});
{item: content(el.item), exit: exit_kind(el.exit), pos: el.pos, cause: el.cause}
```
（`examples/guide/comp-element.jpp`）

### 嵌套

三条已经跑通的嵌套写法，对应 `00-Nature意图汇编` 第 5 条「元素之间继续组合，组合的组合再组合」：

**search 接 ground 的闭环。** 上一节 `comp-ground-verify.jpp` 本身就是这条：接地闭包（不论是库
函数 `ground()`，还是照它的形状另写的 `run_sql`）的返回值是一个普通函数值，直接填进 `search` 的
`opts.ground`，外层看不出内层在执行代码——闭环不是新增的第三个原语，是「组合能当参数传」这条
规则的直接结果。

**图上分工的产物再判、或再建一张图。** `judged_bipartite`/`judged_graph` 的产物元素带 `item`，
和 `sieve`/`pair` 的产物是同一种形状，能原样再交给下一层：

```jpp
import "../../lib/compose/graph.jpp";
budget {calls: 40, cost: 0.01, depth: 64};

let tasks = ["任务：写登录页", "任务：设计数据库表", "任务：写接口文档", "任务：搭部署脚本", "任务：做数据看板", "任务：写单元测试"];
let people = ["小林：前端三年，做过登录与注册", "小周：后端，主写数据库迁移", "小吴：技术写作，维护过接口文档", "小郑：运维，写过部署流水线", "小王：数据分析，常做报表", "小陈：测试开发，熟悉单元测试框架"];
let cand = [[0, 0], [1, 1], [2, 2], [3, 3], [4, 0], [5, 1], [0, 4], [1, 5], [4, 4], [5, 5]];

// 第一层：人与任务的图，只信已决边的分工
let g = judged_bipartite(tasks, people, test("a 描述的这项任务，b 这个人能接下来吗？", "graph-can-do"), {prune: fn(l, r) { cand }});
let t = interval(g, "matching", {});

// 嵌套 (1)：分工产物的 item 就是判边时那一对材料，直接交给 sieve
let ok = sieve(t.lo.value, test("这一组分工（a 是任务，b 是接手的人）能在一周内交付吗？", "graph-team"));

// 嵌套 (2)：分工产物当节点，判两组能否合并，再在第二张图上匹配
let g2 = judged_graph(t.lo.value, test("a 与 b 两组分工能合并成一个小组、互相补位吗？", "graph-merge"), {});
let t2 = interval(g2, "matching", {});

{deliverable: map(ok.value, fn(e) { e.nodes }),
 not_in_a_week: map(ok.detail.ignore, fn(e) { e.nodes }),
 groups: map(t2.lo.value, fn(p) { map(p.nodes, fn(team) { team.nodes }) }),
 layer2_same: t2.same,
 ask_first: map(t.differs, fn(e) { [e.left, e.right] }),
 pending: concat(concat(ok.pending, g2.pending), concat(t.hi.pending, g.pending))}
```
（`examples/guide/nest-graph-then-graph.jpp`）两层都不手写批、预算或未决处理：每层一次刷新，
两层的未决都交回调用者，`pending` 的 `concat` 顺序不影响正确性、只影响读者看到的顺序。
「图上分工再交给 search 出方案」是同一族写法（`map(t.lo.value, fn(p) { search([p.item], …) })`），
本章没有单独收一份可运行示例，因为它就是上面两条的直接叠加，不引入新机制。

**map 里跑多个独立的 search。** 需要对一批需求各自搜索一遍（例如给每个需求各出一个方案）时，
`map` 里嵌 `search` 是最直接的写法：

```jpp
import "../../lib/compose/search.jpp";
budget {calls: 8, cost: 0.01, depth: 64};

let briefs = ["二手书店", "宠物美容店"];
let propose = fn(topic) { fn(frontier, i) { gen("为「" + topic + "」起一个不超过四个字的名字，输出 1 个候选，字符串", frontier, 1, i) } };
let fits = test("这个名字符合需求吗？", "map-search-fit");

let plans = map(briefs, fn(b) { search([], propose(b), fits, unit, 1, {width: 1}) });
{named: map(plans, fn(r) { map(r.value, fn(e) { e.item }) }),
 reasons: map(plans, fn(r) { r.detail.reason }),
 pending: concat(plans[0].pending, plans[1].pending)}
```
（`examples/guide/nest-map-search.jpp`）**这里的多个 `search` 现在按构造串行生成**：`map` 本身
不并行调度，每次 `search` 内部的 `gen` 虽然非阻塞，但轮次之间仍按 `iterate` 的顺序推进，两个
`search` 之间也不交叠（INTERFACE.md §三·四·四「已知限制」）。跨 `map` 的调度交给以后的施工步，
现在写法上没有别的选择，也不需要为此改变程序结构。固定观察下有一个实测细节：两次 `propose` 的
题面必须带上能区分彼此的文字（这里是 `topic`），不能只靠上下文材料不同——夹具按「题面文字 +
`retry_seq`」匹配生成记录，题面相同时后一条会覆盖前一条，返回同一个生成结果（本例的
`examples/guide/fixtures/nest-map-search.json` 就是按这条改过一次才对上）；真机运行没有这个
限制，因为真实生成器会按完整上下文各自生成。

### 题树

复杂的判断靠结构拆开，不靠把题问成显而易见（意图汇编 7b）。「两个人适不适合合作；第三个人加入会不会让合作更紧密；会的话金额会不会变大、增量从哪来；再加一个人会不会更多」——这一串追问在循环开始前由生成器一次写成一棵题树，JEV 沿每一步的出口往下判。节点是普通的是非题或 K 选一题，深问题照原样问，用作者声明线切（7c）；树的形状、往哪支走、拿不准时怎么办，由 `lib/compose/tree.jpp` 的 `walk` 管。树的 JSON 形状与现场模板 `地基/比赛/现场/examples/tree-collab.jpp` 相同，模板里手写的 `loop` + `handle` 可以直接换成 `walk`。

```
walk(tree, material, depth, opts) -> 契约值
  tree      是非题节点 {kind?: "test", q, act, ignore, unsure?, add?, need? 或 missing?}
            K 选一节点 {kind?: "select", q, over: [候选…], picks: [子节点…], unsure?, add?}
            叶 {leaf}；先 validate_tree(tree, depth)
  material  根节点看的材料，推荐具名字段记录 mat({a: …, b: …})
  depth     路径上最多走几个节点；写字面量或顶层 let 常量（J-06 在调用点核）
  opts      {line?, pick_line?, calib?, question?, grow?, keep?, enrich?, eval?}
            enrich = {rounds, fetch: fn(节点, 缺项, 当前材料) -> 值 | 材料 | Fail, line?, calib?, need?, merge?}
validate_tree(tree, depth) -> tree | Fail      查形状并规范 add、need 为文字列表；报文带路径，如「根.act.pick1：节点缺 act 或 ignore」
validate_tree_with(tree, depth, {need?: [信息类别词表], members?: [成员名]}) -> {tree, dropped} | Fail
                                               同上，再按词表剔除 add、need 里写错的，逐条记在 dropped
parse_tree(生成结果) -> tree | Fail             已是记录原样返回；是文字就取出 JSON 再解析
add_members(m, names, members)                 grow 用：逐个并入成员，已在材料里的、成员表里没有的跳过
with_member(m, name, x) / without_member(m, name)   记录材料加、减一个具名字段
```

每个节点按顺序做五件事：`grow` 把材料变成这个节点要看的材料（例如按节点的 `add` 把第三个人并进来，子节点沿用变过的材料）；`keep` 是代码谓词，代码能定的节点不发判断、直接走 ignore 支（B184）；取本节点的读数；`cut`（是非题按 `line`，K 选一按 `pick_line`，缺省取 `line` 的 `hi` 作最大概率门槛）；`handle` 按出口选子节点，K 选一的 `pick(k)` 走 `picks[k]`。K 选一题按正逆两序各问一次（`permute`），真机上「变大 / 变小 / 不变」这类三选一可靠，最大概率低于 0.7 当拿不准（`实测/语义深度-2026-09-26/结果.md`）。落到叶，`value` 里就有一个元素：`leaf` 是叶结论，`path` 是一路的题面、出口与支，`item` 是到叶时的材料。节点判出未决时，出口进 `pending` 随返回值交出；节点有 `unsure` 子节点就接着往下走，没有就停在这里（`leaf` 为 `unit`）。

**拿不准时补信息再判（7c）。** 生成器出题树时，给是非题节点列好「还可能缺哪几类信息」（`need`，模板里叫 `missing`，两个名字都认）。给了 `opts.enrich = {rounds, fetch, line?}`，节点判出未决后，JEV 先用一道 select 选出最缺的一类，程序调 `fetch(节点, 缺项, 当前材料)`——三个参数——取来，补进材料（记录材料用 `with_member` 加一个字段，文字材料接在末尾），再判同一节点；最多补 `rounds` 次，仍未决的照常进 `pending`。补过的材料一路往下带。上一次的未决被重判取代，记一次显式丢弃（报告里有一条 `W-drop-vs-escalate`）。「最缺哪一类」这道 select 按正逆两序各问一次（`permute`），选项顺序的影响测过才给 `pick`。`fetch` 由作者给真实来源：查数据，或由宿主动作去问人；`ask` 返回的是人的是非出口，不是信息，不能当 `fetch`。拿生成器补内容只是替身，生成器编出来的背景不是证据。

**两种求值。** 缺省 `eval: "all"`：从当前节点起，同一份材料上的整组节点题一次发出（运行时按状态合成一次调用），之后逐节点 `cut`、按出口下行，不再发判断；材料变了（`grow` 或补过信息）再对新材料发下一组。没走到的同组节点也付了费，但它们只有读数、不 `cut`，不产生未决责任。`eval: "path"` 每判一个节点一次调用，`grow` 会调 `gen`、`do` 这类贵的效应时用它。两种求值的叶、路径、未决相同。

```jpp
import "../../lib/compose/tree.jpp";
budget {calls: 30, cost: 0.02, depth: 64};

let pair_ab = mat({a: "甲：社区团购团长，做了三年，手上有两千户固定下单的家庭，一直缺稳定的蔬菜货源",
                   b: "乙：城郊蔬菜基地负责人，产量常年过剩，想找稳定的线上销售渠道"});
let members = {c: "丙：冷链配送公司老板，能做城区当日达，正在找新的货源客户",
               d: "丁：连锁餐饮的采购经理，每月固定采购大量蔬菜，看重稳定供货"};
// 作者手上的信息来源：键就是可取的信息类别（need 的词表）。这里是一张查得到的表；真实程序里是查数据库，或由宿主
// 动作去问人（ask 返回的是人的是非出口，不是信息）。拿生成器补内容只是替身——生成器编出来的背景不是证据。
let known = {"c 的配送覆盖范围": "丙的车队已经覆盖城西 30 个小区，甲的下单家庭有八成住在城西",
             "a 的下单家庭分布": "甲的两千户下单家庭八成住在城西，两成在城东",
             "b 的日供货能力": "乙的基地每天能稳定供应约 3 吨叶菜",
             "d 的采购规模": "丁每月固定采购约 40 吨蔬菜，要求每天早上 6 点前到货",
             "双方过往合作记录": "甲和乙去年合作过一次社区团购试单，交付准时"};
let vocab = keys(known);
let names = keys(members);
let skeleton = "{\"q\": \"题面，字符串，一道是非题\", \"add\": [\"进入这道题前要并入材料的成员名，字符串；不需要就省掉这个字段\"], \"need\": [\"判这道题时材料里最可能缺的信息类别，字符串\"], \"act\": 下一个节点或叶, \"ignore\": 下一个节点或叶, \"unsure\": 下一个节点或叶（可省）}；叶写成 {\"leaf\": \"结论，字符串\"}";
let brief = mat("判断 a、b 两人适不适合合作；适合的话，c 加入会不会更紧密、金额会不会变大、增量从哪来，再加入 d 会不会更多；不适合的话，透露一条信息能不能推进合作");
let made = gen("按下面的需求生成一棵合作判断题树。只输出一个 JSON 对象，不要代码块标记，不要解释。每个节点都照这个骨架写，字段名不能改：" + skeleton + "。a、b 是两位当事人，已经在材料里，不要写进 add；add 只能从这些成员名里选：" + join(names, "、") + "。need 只能从下面这些信息类别里原样照抄，一个节点写一到三个：" + join(vocab, "；") + "。从根到叶最多 5 道题。", [brief], 1, 0);
let v = if is_fail(made) { made } else { validate_tree_with(parse_tree(content(made[0])), 6, {need: vocab, members: names}) };
let grow = fn(m, node) { if has(node, "add") { add_members(m, node.add, members) } else { m } };
let amount = measure("这几个人一起做，这次合作能做成的生意金额有多大？", ["小", "中", "大"], "collab-tree");

if is_fail(v) { {failed: text(v)} } else {
    let r = walk(v.tree, pair_ab, 6, {line: {declare: {hi: 0.7, lo: 0.3}}, calib: "collab-tree", grow: grow,
                                     enrich: {rounds: 2, line: {declare: {hi: 0.4}},
                                              fetch: fn(node, need, m) { if has(known, need) { known[need] } else { fail("没有这一类信息：" + need) } }}});
    // 归因：叶材料本身作基线，再加上去掉每个加入者后的材料；先算齐材料，再连续登记判断（中间不夹 if，同一层发出）。
    // 按期望档位排序：去掉后金额落到最低一档、且低于基线的那位是关键成员；基线也在最低一档，说明去掉谁都不减少
    let item = if len(r.value) == 0 { pair_ab } else { r.value[0].item };
    let joined = filter(names, fn(x) { has(content(item), x) });
    let reads = map(concat([item], map(joined, fn(x) { without_member(item, x) })), fn(m) { judge(state(m), amount) });
    let tiers = order(reads, {stat: "expect"});
    let low = tiers[len(tiers) - 1];
    let key = if len(r.value) == 0 || contains(low, 0) { [] } else { map(low, fn(k) { joined[k - 1] }) };
    {leaf: map(r.value, fn(e) { e.leaf }),
     path: map(r.value, fn(e) { map(e.path, fn(p) { p.q + " → " + p.branch }) }),
     enriched: map(r.detail.enriched, fn(x) { x.need }),
     leaf_exit: map(r.value, fn(e) { exit_kind(e.exit) }),
     reason: r.detail.reason,
     key_member: key,
     dropped: v.dropped,
     pending: r.pending}
}
```
（`examples/guide/comp-tree.jpp`，与金样 `examples/tree-collab.jpp` 同一程序，共用夹具 `examples/fixtures/tree-collab.json`。）
**生成器的输出要先规范再用。** 真机上 `claude -p` 写的题树常常不合示例的假定（`地基/比赛/现场/预注册-冒烟.md` 冒烟 3）：`add` 写成列表，还把已经在材料里的当事人写进去；`need` 随手写「双方目标与诉求」这类类别，和作者手上能取到的信息对不上；有时省掉空格，有时在 JSON 外面包说明文字。示例的做法是三步：出题提示里给出节点骨架（字段名、类型、哪些可省）、成员名和信息类别词表——词表就是 `known` 表的键，也就是 `fetch` 真能取到的东西；`parse_tree` 取出 JSON；`validate_tree_with(…, {need: 词表, members: 成员名})` 规范形状、剔除写错的并记在 `dropped`。这样补信息时选中的类别一定在 `known` 表里。

这一趟（夹具仿真机输出的形状：根节点 `add: ["a", "b"]`、各节点 `need` 混有词表外的类别，`dropped` 记下 8 条）：N1、N2、N3 走 act；N4「金额变大主要来自 c 把配送扩到更多小区吗？」首判 0.52 落在线之间，select 在词表内的两类里选中「c 的配送覆盖范围」，从 `known` 表取来补进材料后判 act；N5 落在线之间，它的 `need` 全在词表外、规范后为空，不补，沿 `unsure` 支到叶「先按三方推进，再和 d 谈一次试单」，N5 的未决在 `pending` 里；归因以叶材料为基线，去掉 c 后金额最低，关键成员是 c。调用：1 次生成 + 8 次判断（{a, b} 上 N1、N6 一次；{a, b, c} 上 N2、N3、N4 一次；select 一次；补过的材料上重判 N4 一次；并入 d 后 N5 一次；归因三份材料同一层三次）。同一程序用现场二进制对真实 JEV 与 `claude -p` 跑过三趟，记录在冒烟 3 的重跑段。

**叶出口怎么读。** `value` 元素的 `exit` 是路径上是非题出口的 `compose(…, "all")`（强 Kleene 合取，B131；K 选一的 `pick` 不进合成）。路径上有未决时，合成里去掉 ignore，叶出口一律未决；路径上没有未决时读作「路径上每个是非题都判了是」：全 act 为 act，走过 ignore 支为 ignore。只有全 act 可能放行不可逆动作（还要看线的等级与 taint）。「每一步都已决」看 `pending` 是否为空。

**「是谁的加入带来的」用相对变化。** 这是反事实问题：逐个 `without_member` 去掉一个加入者，问同一道题，用 `order` 比较几份材料上的读数，去掉后读数掉得最多的那位是关键成员；不要各自按线切。依据是 `实测/语义深度-2026-09-26/结果.md`：逐人去掉后按绝对线判只对了一半，按读数的相对变化 12 组全部找对关键人。上面示例的最后几行就是这个写法。

**节点的线从哪来，叶结论能不能放行动作（B183 (2)）。** 生成器写的题没有认证记录，节点的线三条路：作者声明线 `line: {declare: {hi, lo}}`（最常用，等级 `Declared`）、试用线（`calib-import --alpha-trial`，只路由）、类键。题面来自生成器，出口随之不可信（B58、B149），所以开 `--guard` 时题树的叶结论只能路由、不能单独放行不可逆 `do`；要放行，叶结论交 `ask`，或生成器画像声明 `taint_out: trusted`。默认不开把关，叶结论直接驱动动作。题树节点不入题库；按整棵树认证是赛后的方向。

**再组合。** `walk` 的产物与 `sieve`、`search` 的产物同形，能接着组合：

<!-- 片段 -->
```jpp
// 对几组人各走一遍同一棵树，未决经 carry 并进外层
let rs = map(groups, fn(g) { walk(tree, g, 4, opts) });
let all = carry(carry([], rs[0].pending, "组#0"), rs[1].pending, "组#1");
// 叶元素的 item 是到叶时的材料，可以再判一道题；walk 的 pending 随契约值并进来
let s = sieve(walk(tree, m, 4, opts), 复核题);
// 几次 walk 的叶出口再合成
let joint = compose(map(rs, fn(r) { r.value[0].exit }), "all");
// search 选出的每个候选各走一遍
let ws = map(search(…).value, fn(e) { walk(tree, e.item, 4, opts) });
```
这四种写法在 `crates/jpp/tests/compose_tree.rs` 的 (i1)–(i4) 里都跑过。叶元素的 `item` 带着路径谱系：路径上每个判过的出口都挂成它的选择依赖边，路径题面的 taint 也并进了它。所以在叶材料上再判一道题、拿那道题的出口去守不可逆 `do` 时，J-08 看得到整条路径——路径用了宿主没接受的作者声明线，报「该材料由 Declared 线的出口选出（谱系放行，B72-4）」；题树来自生成器（题面不可信），叶材料随之不可信，下游判断不是可信合取项。测试 (L1)–(L4) 核了这四种情形（拦下两种、照发两种）。

**写法提醒。** 判断的账本键带调用站点：同一（材料, 题）经不同的 `judge` 调用处不会按键复用。`if` 是刷新点：想让几次判断在同一层发出，先算齐材料与题，再在不含 `if` 的 `map` 里连续登记（示例的归因就是先算齐去掉每个人后的材料，再登记判断）。

### 什么时候用哪个

| 情形 | 用什么 | 为什么 |
|---|---|---|
| 判断某个东西对不对、分不分类 | 一次判断（`judge`/`cut`） | 语义判断只能由 JEV 做，其余组件不带校准置信度 |
| 候选集不存在、需要新的东西 | 生成（`gen`），产物缓存复用 | 已有的东西里挑不需要生成；生成一次、判断反复用 |
| 候选集已在手上（文件、数据库、枚举器） | 直接判断，不调生成器 | 调生成器是每题贵几十倍、没有校准线的做法 |
| 要判的东西不是字面的（代码、查询、配置） | 先执行（`do`/`ground`）再判 | 字面化定律：判断只看字面，先把结果变成字面 |
| 解空间能列出来，几十到几万个 | 枚举 + `sieve` | 广度免费，先列全再判比边猜边判更省 |
| 解空间列不完，要从部分解长出更好的解 | `search` | 判断给可行域与目标两道题，生成器负责提，收敛靠迭代 |
| 要闭环验证（写代码要跑测试） | `search` 的 `opts.ground` | 提出、执行、判、再提出是同一个原语，不是拼出来的 |
| 决策是全局的（配对、分组、找路） | `judged_graph`/`judged_bipartite` + `interval` | 逐条判只给边，全局最优要图算法；边由判断给，算法只吃已决/未决两组 |
| 候选数万级 | 先 `bm25_topk`/`embed_topk` 粗筛，再判 | 检索只负责召回，判断负责精度；全配对成本按平方增长 |
| 判不清（`unsure`） | 不调大模型，走细化 / `ask` / 记账丢弃 | `unsure` 是深度的使能机制，不是要靠多问一次模型来消灭的失败 |
| 搭一个新组合（不是套现成的） | `compose`/`cert`/`element` | 这三个是搭配库自己用的原语，日常写程序很少直接碰 |

### 常见报错与修法

报文以 `examples/guide/err-*.jpp` 实际跑出来的原文为准；标了「未在本机复现」的两条引自源码，
本机条件（有 `sandbox-exec`、没有 r1 格式的旧账本）碰不到触发条件，没有假装跑过。

| 诊断码 | 触发场景 | 修法 |
|---|---|---|
| `J-05`（运行期） | `sieve`/契约值的 `pending` 被悄悄丢弃：只返回 `accepted(r)`，没提到 `undecided(r)`/`unobserved(r)`/`r.pending` | 把 `r.pending` 放进返回值，或 `consume(u, "drop")` 显式丢弃并记账；检查期会先给一条 `W-pending-unreturned` 警告，运行期真的丢了才是错误（`examples/guide/err-j05.jpp`） |
| `J-08`（静态子面，只在 `--guard` 下） | 宿主开了 `--guard`，不可逆动作（如 `write_json`）的每一层守卫都只来自作者声明线，且宿主没带 `--release-on-declared` | 带 `--release-on-declared`（确认这些线由自己担责），或在条件里再合取一个认证线上的判断，或改走 `ask`，或把动作登记成可逆；不需要把关就不带 `--guard`（`examples/guide/err-j08.jpp`，`guide_check.py` 带 `--guard` 跑） |
| `E-stat-unavailable` | `cut` 的 `stat: "confidence"` 用在没有声明 `reports_confidence` 的画像上 | 换一个声明了 `reports_confidence: true` 的画像，或改用缺省的 `stat: "max"`；固定观察下可以在夹具观察里直接给 `confidence` 字段（`examples/guide/err-stat-unavailable.jpp`，用 `--profile profiles/jev-1.13.0.json` 触发，这份发行画像没有该字段） |
| `E-ledger-required`（只在 `--guard` 下） | 宿主开了 `--guard`，程序里出现登记为不可逆的 `do`（字面动作名，如 `write_json`），或动作名不是字面量的 `do`（如 `graph.jpp` 内部 `join(["graph:", algo], "")` 拼出来的名字，或 `ground.jpp` 里的形参 `action`），又没给 `--ledger-out` | 加 `--ledger-out <文件>`。不开 `--guard` 时不报，账本自动写到源文件旁的 `<源文件名>.ledger.jsonl` 并打出路径 |
| `J-03` | 程序里拿读数当数算（`if p > 0.7`），或在 `test`/`select`/`form` 的校准位写数字 | 要按自己的数切，写 `cut(r, 0.7)` 或 `cut(r, {declare: {hi: …, lo: …}})`；要语言担保错误率，写校准键加 `calib-import`（可选工具） |
| `W-untested`（J-15） | 画像字段没测过（窗口、固定输出结构、校准等），或 `select` 声明了要置换却没测置换 | 按报文补画像或改写法；校准记录缺 `delta` 不再卡住出口（B187：取画像 δ，没有取 0，出口记 `delta_unknown`） |
| `E-render-version` | `--resume` 一份用旧渲染版本（`r1`）记的账本；15i 把线上材料形状升到 `r2` 后键分量变了 | 换一份同渲染版本的账本，或重新从 `r2` 起跑；**未在本机复现**——本仓库现存账本都已是 `r2`，没有 `r1` 账本可以拿来触发（引自 `crates/jpp-runtime/src/outcome.rs:36`） |
| `W-action-no-sandbox` | 检查期发现字面动作名（`exec_py`/`check_tests`/`exec_sql`）在没有沙箱工具的机器上。执行器直接在沙箱外跑（B187 起 `E-action-no-sandbox` 退役）；开 `--guard` 时它按不可逆动作算，要有守卫 | 要隔离就装 `sandbox-exec`（macOS 自带）或 Linux 的 `bwrap`；本机有 `sandbox-exec`，用 `JPP_FORCE_NO_SANDBOX=1` 在 `crates/jpp/tests/actions_no_sandbox.rs` 里跑过 |
| `E-list-ambiguous` / `W-no-candidate` / `E-kind-conflict` | `calib-import`/标注流程相关，与「真机运行」一节「待标清单与标注包」同一批 | 见本文件前面「真机运行」一节，不在搭配层重复 |
