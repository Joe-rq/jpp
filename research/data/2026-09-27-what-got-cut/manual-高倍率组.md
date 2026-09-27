# 人工对照：高倍率组（5 项目）

方法：对每个项目的 `measure.toml` `[[core]]` 精确行号范围逐块读原代码，与 `prog.jpp`、`result.json`/`rerun.json`、`预注册.md`/`选题说明.md` 逐一核对哪些内容有直接对应文本、哪些没有；把"没有直接对应"的部分按任务书给的 A–E 分类法归类（互斥，取第一个命中的类：A 杂活 / B 组合方式 / C 数据整形 / D 语法差异 / E 功能缺失），有直接对应的记"保留"（不计入被压掉的行数，只作分母参照）。

**口径说明（如实记录，不掩盖）**：本表 4/5 个项目（B5-10、B4-12、B4-01、B4-09）按"原代码物理行范围"计块，逐项合计会略高于 `measure.toml`/`result.json` 声明的核心行数——差额经逐项核对，来自空行与纯注释/docstring 行（`measure.toml` 的口径排除这些，本表为了让"这块代码在干什么"读起来完整，保留了物理行范围）。第 1 项 B4-06 由另一位协作者用更细的方法重写，改成"净压掉行数"（原代码行数 − 对应 J++ 行数）计口径，并新增了"挪走"类别（内容还在，只是搬了文件位置，不计入压掉/A-E）；这与另外 4 项的"原代码物理行数"口径不同，下方汇总表两种口径都会影响到具体数字，已在汇总表下方单独说明，不直接假装两者等价相加。

## 1 B4-06-jevyoumean（折行倍率 7.44）

- 核心总行数（measure.toml `[[core]]` 合计）：372（decide.go 91 + suggest.go 123 + client.go 110 + main.go 48，见 rerun.json）；J++ 行数（`jpp_lines_raw`）：50
- 逐块分类。新增「J++ 行数」= 该块内容在 `prog.jpp` 里对应几行（多块共用同一段 J++ 代码时按内容归属拆分估算，非编译器级精确对账）；「净压掉」= 原代码行数 − J++ 行数；「挪走」是独立于 A–E 的类别（内容还在，只是搬了文件位置，不计入净压掉/不计入 D 或 E）。已核实的搬运目的地：`build_jpp_input.py`（宿主脚本，生成 `jpp_input.json`）与 `jpp_input.json`/`fixture.json`（数据文件）。

| 原代码行范围 | 原行数 | J++ 行数 | 净压掉 | 这块在干什么 | 归类 | 对应 J++ |
|---|---|---|---|---|---|---|
| decide.go:1-10 | 7 | 0 | 7 | package/import 声明 | D | 无对应 |
| decide.go:12-38 | 20 | 0 | 20 | `Action` 类型 + 四个常量 + `String()` 方法 | D | prog.jpp 直接用字面字符串 `"pass-through"`/`"hint"`/`"prompt"`/`"auto-run"` |
| decide.go:40-44 | 4 | 0 | 4 | `Candidate` 结构体（Name+P） | D | prog.jpp 直接用记录 `{id, action, corrected, exit}` |
| decide.go:48-55 | 8 | 0 | 8 | `Config` 结构体：六个阈值/模式字段声明 | A | 字段本身无残留；**已核实挪走**：字段承载的具体数值中，SuggestThreshold=0.3、AutoRunThreshold=0.95 挪进 `fixture.json` 的 `calibrations[].hi/lo`（key=`jym-suggest`/`jym-autorun`），MinConfidence=0.5 未挪走，直接内联在 prog.jpp:45 `declare:{hi:0.5}` |
| decide.go:57-64 | 6 | — | — | `DefaultDenylist`：13 个危险子命令名单 | **挪走** | 逐字复制进 `build_jpp_input.py:7-8` 的 `DENYLIST` 集合（同 13 个词，宿主用它预算出 `denylist_hit` 布尔值传入 prog.jpp:54，不计入压掉/A-E） |
| decide.go:66-67 | 1 | 0 | 1 | `MaxCandidates = 3`：展示候选数上限 | E | 无对应（候选列表第 2、3 名排序/展示做不到，result.json gaps 第 1 条） |
| decide.go:75-104 | 29 | 37 | **-8** | `Decide()`：none 短路→confidence 门→suggest 门→mode 分派→denylist 覆盖→非交互降级，五层判断链写在一个函数里 | A（出口分派：全部基于同一次 `judge()` 读数 `r` 的链式 `cut`，单次判断分流） | prog.jpp:36-78 `decide_one()` 的整棵 `handle(cut(r,...))` 嵌套树；J++ 侧比原代码**多**8 行——嵌套 `handle/cut/fn` 闭包比 Go 的 flat if/early-return 更啰嗦，这是本项目里少数"没压反而变长"的局部 |
| decide.go:106-123 | 14 | 1 | 13 | `candidates()`：按 minP 过滤（1 行归属 jym-suggest 门槛）+ 按概率降序排序 + 截断前 3 | E（排序/截断部分为主） | 过滤部分对应 prog.jpp:47 `cut(r, "jym-suggest")`；排序/截断无对应 |
| suggest.go:3-10 | 7 | 0 | 7 | import 声明 | D | 无对应 |
| suggest.go:13 | 1 | 1 | 0 | `NoneOption = "__none__"` | 保留 | prog.jpp:42 `label == "__none__"`，字面量原样保留 |
| suggest.go:16 | 1 | 0 | 1 | `maxOptions = 254`：Choice 选项数硬上限 | A | 无对应（门槛常量，服务于下面的分片缺口） |
| suggest.go:19-27 | 6 | 0 | 6 | `State` 结构体（4 字段+json tag） | D | `it.state`（宿主直接给 JSON） |
| suggest.go:59-62 | 4 | 1 | 0（按保留不计） | `instructions`：question+note 两段文本 | 保留 | prog.jpp:30，题面文本原样折叠成一行（`fold_text.py` 只是把夹具的匹配 key 同步折叠，内容未改） |
| suggest.go:66-77（候选名单部分） | 4 | — | — | `criteria()` 里构建候选名→`__none__`兜底的**名单**部分 | **挪走** | 候选名单（17 个命令名）挪进 `jpp_input.json` 的 `items[].over` 与 `fixture.json` 的 `observations[].over`，两处逐一核对与 `calls.jsonl` 里真实请求的 `criteria` key 顺序一致 |
| suggest.go:66-77（候选描述部分） | 7 | 0 | 7 | `criteria()` 里给每个候选取 `cmd.Description` 拼描述文本的部分 | E | **无对应且真实丢失**：`build_jpp_input.py:29` `over = list(q["criteria"].keys())` 只取 key 不取 value——已用 `calls.jsonl` 核实真实 Go 请求的 criteria 是带描述的（如 `"add": "Add file contents to the index"`），J++ 侧候选只剩裸名字，描述文本在 jpp 侧任何文件里都不存在；这不是"语法差异"，是判断材料变瘦了，只是没有被 result.json 列进 gaps（因为读数是从原 Go 真机请求捕获后固定重放，这次等价测试不需要重新发问） |
| suggest.go:79-101 | 23 | 0 | 23 | `BuildQuestions()` + `shard()`：候选数≤254 包一个 Question，超过则切片打包成多份 | E | 无对应（超 254 候选两段式分片，gaps 第 5 条，本项目候选表 17 个未触发） |
| suggest.go:~108-113 | 6 | 3 | 3 | `Suggest()` 单请求路径：组装 Questions→`Ask()`→`answerOrNil` 取答案 | C | prog.jpp:29-31,38（`select(...)`+`judge(st, intended_q)`），调用与取值合一 |
| suggest.go:~115-146 | 32 | 0 | 32 | `Suggest()` 两阶段路径：phase1 给每个 shard 打分→池化 top-3→phase2 终选 | B（出口分派：基于**两轮**独立判断调用的结果组合选池） | 无对应（B155/B156 缺口："上一轮结果决定下一轮候选集"这个模式还没压成一条语句） |
| suggest.go:~148-153 | 5 | 0 | 5 | `answerOrNil()`：按 key 从 map 取一条答案 | C | `judge()` 返回值本身就是这道题的读数，不用再挑 key |
| suggest.go:~155-173 | 19 | 0 | 19 | `top()`：阈值过滤+排序+截断前 n，供两阶段池化用 | B（同上两阶段组合的辅助环节） | 无对应（同上缺口的一部分） |
| client.go:47-58 | 9 | 0 | 9 | `Question`/`request` 请求体结构体声明 | D | J++ 运行时内置请求组装 |
| client.go:60-77 | 15 | 0 | 15 | `ChoiceAnswer`/`answer`/`response` 响应体结构体声明 | D | `judge()` 返回值直接是读数 |
| client.go:84-113 | 10 | 0 | 10 | `Ask()` 前半：取默认 model、序列化、发起 POST | C | `select()`/`judge()` 内置发送 |
| client.go:98-116 | 18 | 0 | 18 | `Ask()` 重试圈：HTTP 429/529 重试一次、200ms 退避 | A（重试与失败） | 无对应，交给运行时/传输层 |
| client.go:118-140 | 20 | 0 | 20 | `Ask()` 后半：反序列化响应、按题 id 回填校验 | C | `judge()` 内置读数解析 |
| client.go:142-176 | 33 | 0 | 33 | `validateChoice()`：choice 非空/在 criteria 内、confidence/概率范围合法、choice 必须是 argmax | A（拿不准的处理/健壮性校验） | 无对应，J++ 契约保证读数合法（`is_fail`） |
| client.go:178-180 | 3 | 0 | 3 | `finite01()`：NaN/Inf/范围校验小工具 | A | 无对应 |
| main.go:256-262 | 6 | 0 | 6 | 变量声明（action/cands/offline/jevOK） | D | 无对应 |
| main.go:265-270 | 6 | 1 | 5 | 组装 `jev.State`（从 det/inv 取字段） | C | prog.jpp:37 `state(mat(it.state), {over: it.over})`，材料由宿主预拼好传入 |
| main.go:271-272 | 2 | 1 | 0 | 调用 `client.Suggest(...)` 本体 | 保留 | prog.jpp:38 `judge(st, intended_q)`，判断调用本身 |
| main.go:273 | 1 | 0 | 1 | 记录调用延迟日志 | A（费用与调用计数类杂活） | 无对应 |
| main.go:274-287 | 12 | 0 | 12 | 错误吞掉打日志 + 现场组装 `decide.Config{...}` 六字段 + 调用 `decide.Decide()` | A（出口分派：基于同一次 `ans` 的分派准备） | 对应具名校准线引用（"jym-argmax" 等）与内联 `declare:{hi:0.5}`，已计入 decide.go 相关行 |
| main.go:289-308 | 20 | 0 | 20 | `!jevOK` 分支：无 key/请求失败时走编辑距离离线匹配，整套独立非 JEV 路径 | E | 无对应（excluded_fields 第 3 条：不经过 JEV，但仍登记在 core 行号区间内） |

- 本项目小计（按净压掉行数）：**A=68　B=51　C=43　D=74　E=64**（合计压掉 300 行）；另有**保留**（内容原样，只是折了行数，不计入压掉）Σ原行数=7；**挪走**（内容还在，搬了文件位置，不计入压掉/AE）Σ原行数=10（6 行去 `build_jpp_input.py` 的 DENYLIST，4 行去 `jpp_input.json`/`fixture.json` 的 `over`）
- 一句话：这个项目净压掉的 300 行里，最大的两块是 D（74，类型/结构体声明与响应体反序列化）和 E（64，主要是候选列表排序截断、候选描述文本丢失、超 254 候选分片这三处 J++ 现在做不到/没测到的能力），A（68，含 -8 的"分派逻辑反而变长"）和 B（51，两阶段候选池化组合）也不小；同时发现两处真实的"挪走"而非"压掉"——13 个危险命令的黑名单和 17 个候选名的清单，都是原样目的地换了文件，不是被语言构造替掉的。
- 候选好例子：

  例 A：判断结果的自洽性校验，J++ 靠契约免写
  ```go
  // client.go:168-175
  p, ok := a.Probabilities[a.Choice]
  if !ok {
      return errors.New("choice missing from probabilities")
  }
  if p != maxP {
      return fmt.Errorf("choice %q (%.2f) is not the argmax (%.2f)", a.Choice, p, maxP)
  }
  ```
  ```
  // prog.jpp：无对应代码，judge() 返回的读数本身就保证内部一致
  let r = judge(st, intended_q);
  ```
  说明：原项目自己校验"判断器选中的候选，概率是不是真的最大"这类响应体自洽性，J++ 的 `judge()` 读数不需要程序自己二次校验（33 行→0 行，净压掉全部计入 A）。

  例 B：置信度门槛 + auto 模式的黑名单覆盖，压成具名声明线
  ```go
  // decide.go:79-81, 92-96
  if ans.Confidence < cfg.MinConfidence {
      return PassThrough, nil
  }
  ...
  if cands[0].P >= cfg.AutoRunThreshold && !cfg.Denylist[cands[0].Name] {
      action = AutoRun
  } else {
      action = Prompt
  }
  ```
  ```jpp
  // prog.jpp:45-57（节选）
  handle(cut(r, {stat: "confidence", declare: {hi: 0.5}}), {
      act: fn() {
          handle(cut(r, "jym-autorun"), {
              pick: fn(_a) { {action: if it.denylist_hit { "prompt" } else { "auto-run" }, exit: unit} },
              unsure: fn(u) { {action: "prompt", exit: u} }
          })
      },
      ignore: fn() { {action: "pass-through"} }
  })
  ```
  说明：注意这个例子净行数其实没有压缩（decide.go 整个 `Decide()` 函数 29 行对 prog.jpp 37 行，J++ 更长）——阈值本身被压掉了（挪进具名校准线/内联 `declare`），但把判断链表达成嵌套 `handle/cut` 闭包比 Go 的 flat if 更占行数，这是"净压掉"为负的具体案例。

  例 C：危险命令黑名单，原样换了文件，不是被语言构造替掉
  ```go
  // decide.go:59-64
  var DefaultDenylist = map[string]bool{
      "rm": true, "remove": true, "delete": true, "destroy": true,
      "clean": true, "reset": true, "prune": true, "purge": true,
      "drop": true, "push": true, "publish": true, "apply": true,
      "deploy": true,
  }
  ```
  ```python
  # build_jpp_input.py:7-8（挪走目的地，非 prog.jpp）
  DENYLIST = {"rm", "remove", "delete", "destroy", "clean", "reset", "prune",
              "purge", "drop", "push", "publish", "apply", "deploy"}
  ```
  说明：这 13 个词从 Go 的 map 字面量原样搬进 Python 宿主脚本的 set 字面量，宿主用它预算出 `denylist_hit` 布尔值传给 prog.jpp:54；语言构造没有替掉什么，只是判断的责任边界变了——"要不要把某个候选算作危险"这件事本来就不该问 JEV（意图汇编 7a：字面表匹配代码能判），原项目自己也是这么写的，只是刚好落在 J++ 判断核心的输入准备阶段而不是判断本身。

## 2 B5-10-jev-snake（折行倍率 8.8）

**重要说明（这个项目和其他四个不一样，先说清楚）**：`result.json` 自己在 gaps 里写明——`SnakeApp.tsx:115-219`（95 行）描述的是"多 tick 自动对局 + 非法/缺失移动就停手"这一整套前端循环逻辑，`选题.json` 的 acceptance 明确"逐帧比"、"不做多 tick 自动对局"，这套逻辑**从一开始就不在 J++ 的被测范围内**，但这 95 行仍按口径计入核心行数分母（132 行里的 95 行），导致这个项目的折行倍率（8.8x）"因此偏高，不是纯判断逻辑精简的倍率"（原话）。也就是说，这个项目被压掉的行里，绝大多数不是"J++ 用什么构造替代了杂活"，而是"这部分功能范围本来就没有对应实现"——分类结果因此以 E 占绝对多数，这是如实的结果，不是分类方法出了问题。

- 核心总行数：132（measure.toml 口径：`route.ts(30-58,111-121)`=37 + `SnakeApp.tsx(115-219)`=95）；J++ 行数：15（raw）/18（折行）
- 逐块分类：

| 原代码行范围 | 行数 | 这块在干什么 | 归类 | 对应 J++ |
|---|---|---|---|---|
| route.ts:30-48 | 19 | `moveCriteria`：给每个合法方向动态拼一段候选描述文本（是否会让到食物的距离变近、要不要避开死路等，根据 `hunts`/`dist` 条件选不同短语） | C（数据整形，题面拼装） | prog.jpp:14 `state(mat(it.state), {over: it.legal})`——原代码"每个候选各自定制一段推理文本"被简化成"合法方向名称列表 + 一段通用 instructions"，见 `result.json` gap「六·7：candidate 只能并成一段候选文本」 |
| route.ts:50-58 | 9 | 组装 `questions.move`（type/instructions/criteria），发给判断器的 JSON 结构 | C（数据整形） | prog.jpp:9-11 `select("Choose next Snake direction...", "move-argmax")` |
| route.ts:111-113 | 3 | 从响应里取 `answers.move`（判断结果字段） | C（数据整形） | 内置 `judge()`/`cut()` 直接给出结构化读数 |
| route.ts:115-119 | 5 | batch 模式：把每个方向的 `score_*` 回答整理成 `scores` 字典 | E | `result.json` excluded_fields「batch 模式的回显信息，不用于决定下一步方向」——纯展示用途，不进比较、J++ 未实现 |
| route.ts:120-121 | 2 | batch 模式：取 `foresight` 判断的回显（choice/confidence/probabilities） | E | 同上，J++ 未实现 batch 模式 |
| SnakeApp.tsx:115-132 | 18 | `step()` 起手：检查是否忙碌/游戏是否在进行中，取合法方向，无合法方向就判定游戏失败 | E | J++ 只做单帧决策，不做多 tick 循环与终局判定，见上方说明 |
| SnakeApp.tsx:134-155 | 21 | 更新 UI 显示当前判断阶段（"batch foresight…"/"Choice(move)…"），发起 HTTP 请求给自家 `/api/jev-move` | E | 同上；若要写对应的 J++ 逻辑，发请求本身对应 prog.jpp:15 `judge(st, q_move)`，但循环调度、UI 状态不在 J++ 程序范围内 |
| SnakeApp.tsx:156-190 | 34 | HTTP 失败则报错停手；判断结果非法或缺失（不在合法方向里/没给出选择）也直接停手——原文注释明确写「every move must come from Jev, no local/heuristic fallback」 | E | 同上；这条"判断结果不合法就停手、不本地兜底"的设计精神和 J++ 的 J-05（未决必须消费）相通，但 J++ 把它交给 `handle(cut(...), {unsure: fn(u){...}})` 的 `pending` 机制处理，不是这套显式 if/停手代码 |
| SnakeApp.tsx:192-219 | 28 | 应用移动、更新游戏状态和 UI、检查游戏是否结束；catch/finally 收尾 | E | 同上，环境转移与终局判定完全在 J++ 程序之外（宿主职责） |

- 本项目小计（按表格行汇总）：A=0 B=0 C=31 D=0 E=108 保留=0
- 一句话：这个项目被压掉的行里只有 31 行（`route.ts` 的题面拼装）是真正意义上"J++ 用一个构造替代了原来手写的代码"；剩下 108 行（占大多数）是 `SnakeApp.tsx` 里整套多 tick 自动对局循环——这部分从选题范围上就没有被 J++ 覆盖，不是被"压缩"掉的，是从比较范围里被划出去的，这个项目的高倍率数字需要连着这条说明一起看，不能单独引用 8.8x。
- 候选好例子（题面拼装被简化，供参考而非"消失"类型；该仓库未附开源许可证声明，本文只按编号引用、描述内容，不贴原始代码）：

  原代码（route.ts:30-48，19 行）在干什么：`moveCriteria` 给每个合法方向单独拼一句"为什么这个方向可能是对的"推理提示——是否会让到食物的距离变近（用 `hunts`/`dist` 条件挑选不同短语）、是否要避开死路、有没有推进填满棋盘，四个方向就要手写四段不同的组合文本。

  J++（prog.jpp:9-14）：用一个 `select(...)` 声明一份通用的优先级说明（存活优先、其次减少到食物的距离……），候选只给方向名称本身；每次决策时把状态材料（`state(mat(it.state), {over: it.legal})`）和这份通用题面一起交给 `judge()`。

  说明：J++ 把"是否让到食物变近""有没有死路风险"这类线索留在材料里，候选只给方向名称，题面文本只写一份通用的优先级说明，让判断器自己从材料里读线索，不需要程序员每个候选单独构造理由——原改写记录也如实记了这处简化在真机上可能改变读数，不是免费的。

## 3 B4-12-jev-relevance（折行倍率 9.09）

说明：measure.toml/result.json 声明的核心行数是 209（`constants.py`=12、`scoring.py`=46、`strategies.py`=83、`retriever.py`=68，排除空行/注释的口径）。本表按逻辑块给的是物理行范围（含空行、docstring），逐块合计约 292，比声明高，差额同样来自计数口径（已用 constants.py 的 12 行核对过：42-46+69-84 共 21 物理行，去掉空行和注释后正好剩 12 行代码，方法一致）。

- 核心总行数：209（measure.toml 口径）；J++ 行数：23（raw）
- 逐块分类：

| 原代码行范围 | 行数 | 这块在干什么 | 归类 | 对应 J++ |
|---|---|---|---|---|
| constants.py:42-46 | 5 | `DEFAULT_THRESHOLD = 0.5`：noul 模式的相关性门槛常量 | A（门槛） | prog.jpp:24 `cut(r)`（默认线 p>0.5 出 act，等价于 `>= 0.5` 两端闭，改写流程 §三） |
| constants.py:69-84 | 16 | 题面文本：`NOUL_INSTRUCTIONS`/`NOUL_CRITERIA_TRUE`/`NOUL_CRITERIA_FALSE`/`SCORE_INSTRUCTIONS`/`SCORE_RUBRIC`（题面与候选描述文本本身） | 保留 | prog.jpp:15 `test("Does this passage answer the query?", "relevance-threshold")` |
| scoring.py:1-25 | 24 | 模块 docstring + imports | D | 无对应 |
| scoring.py:28-43 | 16 | `build_noul_question()`：组装 `Noul(instructions=..., criteria={"true":..., "false":...})` | C（数据整形，题面拼装） | prog.jpp:15 `test(...)` |
| scoring.py:46-62 | 17 | `build_noul_question_batched()`：把 passage 内嵌进题面文本，供 batched 模式一次请求带多道题用 | E | `result.json` gap「scoring_mode='batched' 未测」——本次只测 per_chunk，J++ 未实现 |
| scoring.py:65-67 | 3 | `build_score_question()`：0-3 档打分题 | E | `result.json` gap「question_mode='score' 在期望分上设阈值做不到（§六·1）」——本次只测 noul 模式 |
| scoring.py:70-80 | 11 | `build_state()`：把 query/passage 组装成 `{"query":..., "passage":...}` 字典 | C（数据整形） | prog.jpp:22 `state(mat({query: query, passage: doc.passage}))` |
| scoring.py:83-88 | 6 | `build_batched_state()`：batched 模式的共享 state | E | 同上 batched 未测 |
| scoring.py:91-98 | 8 | `__all__` 导出列表 | D | 无对应 |
| strategies.py:64-88 | 25 | `ScoringStrategy` 抽象基类定义 + `__init__` + `score`/`ascore` 抽象方法签名 | D | 无对应 |
| strategies.py:94-118 | 25 | `_read_answer()`：按 `question_mode` 从响应里取 noul 或 score 字段，返回 (relevance, confidence) | C（数据整形，从判断器响应对象取字段） | 内置 `judge()`/`cut()` 直接给出结构化读数 |
| strategies.py:121-133 | 13 | `PerChunkScorer` 类定义 + `_build_question()`：按 question_mode 选 noul 题或 score 题 | D（类定义 7 行）+ C（选题逻辑 5 行） | prog.jpp:15（只有一种题，noul 模式写死） |
| strategies.py:135-159 | 25 | `_score_one`/`_ascore_one`：同步/异步两个几乎逐行重复的方法，各自组装 request（state+questions）、调用 classifier、读取答案返回 `DocumentScore` | C（数据整形；同时是同步/异步重复代码） | prog.jpp:21-27 `score_one(query, doc)`（只写一遍，不分同步异步） |
| strategies.py:161-171 | 11 | `score()`：用 `ThreadPoolExecutor(max_workers=...)` 起线程池，逐个 document 提交判断任务并收集结果 | A（并发限制，`max_workers` 即并发上限参数） | prog.jpp:30 `map(c.docs, fn(d) !{judge} { score_one(c.query, d) })`——不手写线程池，好例子候选 |
| strategies.py:173-183 | 11 | `ascore()`：同上的 async 版本，用 `asyncio.Semaphore` 做并发限制 + `asyncio.gather` | A（并发限制） | 同上（J++ 不分同步异步两套） |
| retriever.py:77-89 | 13 | `_get_relevant_documents()`：取候选文档，空则直接返回；try 调用 `strategy.score()`，异常走 `_handle_scoring_failure` | C（取候选，8 行）+ A（重试与失败，5 行） | prog.jpp:29-30 `run_case`；try/except 无对应 |
| retriever.py:91-106 | 16 | `_aget_relevant_documents()`：同上的异步版本 | C（11 行）+ A（重试与失败，5 行） | 同上（不分同步异步） |
| retriever.py:112-129 | 18 | `_handle_scoring_failure()`：`fail_open` 为真则记警告、返回未过滤的原始候选；否则重新抛出异常 | A（重试与失败，`result.json` 标注这是固定观察下从不触发的死分支，仍全额计入分母） | 无对应 |
| retriever.py:131-140 | 10 | `_retain_relevant()` 前半：遍历分数，不过线的候选直接跳过 | A（门槛） | prog.jpp:24 `cut(r)` 的 ignore/unsure 分支 |
| retriever.py:141-150 | 10 | 复制文档、把 relevance/confidence 写回 metadata（结果回流：判断结果写回材料副本） | C（数据整形） | J++ 侧不需要，`excluded_fields` 里 `relevance`/`relevance_confidence` 本就不进比较 |
| retriever.py:152-158 | 7 | 按 relevance 降序排序，再按 `top_k` 截断 | B（组合方式：排序 + 取前 K） | prog.jpp:32-34 `order(...)` + `fold` 摊平 + `slice(ordered, 0, c.top_k)`，好例子候选 |
| retriever.py:160-164 | 5 | `_clears_threshold()`：按 question_mode 选 `threshold` 还是 `score_threshold` 做比较 | A（门槛） | prog.jpp:24 `cut(r)` |

- 本项目小计（按表格行汇总，约 292 物理行，declared 核心为 209）：A=70 B=7 C=110 D=63 E=26 保留=16
- 一句话：这个项目被压掉的行主要是 **C 数据整形**（`_score_one`/`_ascore_one`/`_read_answer` 这类"组装请求、从响应取字段"的重复代码，以及同步/异步两套几乎一样的实现）和 **D 语法差异**（抽象基类、类型声明），但最值得记住的一点是这个项目自己的 `result.json` 已经点破：折行倍率比预注册预测高出约 50%，主因正是 `strategies.py` 里的 `ThreadPoolExecutor`/`asyncio.Semaphore` 并发调度——J++ 把"逐个候选发起判断"的并发调度交给运行时接管，程序员完全不用写线程池或信号量，这是本表里唯一被原始改写记录明确点名"是行数比偏高的主要来源"的一类。
- 候选好例子 1（并发调度整段消失，且原项目自己承认这是省得最多的部分）：
  ```python
  # strategies.py:161-171（score() 同步版；173-183 的 ascore() 是几乎逐行重复的 async 版本）
  def score(self, query, documents):
      question_key = RELEVANCE_QUESTION_KEY
      with ThreadPoolExecutor(max_workers=self.config.max_concurrency) as executor:
          futures = [
              executor.submit(self._score_one, query, document, question_key)
              for document in documents
          ]
          scores = [future.result() for future in futures]
      return scores
  ```
  ```jpp
  // prog.jpp:30
  let rows = map(c.docs, fn(d) !{judge} { score_one(c.query, d) });
  ```
  说明：原项目要手写一个线程池（外加一份用 `asyncio.Semaphore` 的异步复刻版）来并发发起对每个候选文档的判断请求，自己管理 `max_workers` 并发上限；J++ 的 `map` 加 `!{judge}` 效应标记直接对每个文档发起判断，判断调用本身是否重叠执行由运行时决定，程序里不需要出现"线程池"或"信号量"这类词。

- 候选好例子 2（排序+截断 → order/fold/slice）：
  ```python
  # retriever.py:152-158
  retained.sort(
      key=lambda document: float(document.metadata.get(self.config.metadata_relevance_key, 0.0)),
      reverse=True,
  )
  return retained[: self.config.top_k] if self.config.top_k is not None else retained
  ```
  ```jpp
  // prog.jpp:32-34
  let tiers = order(map(hit_rows, fn(row) { row.reading }));
  let ordered = fold(tiers, [], fn(acc, tier) { concat(acc, map(tier, fn(i) { hit_rows[i] })) });
  let sliced = if c.top_k != unit { slice(ordered, 0, c.top_k) } else { ordered };
  ```
  说明：原项目从"文档的 metadata 字段"里取出已经写回的 relevance 数值再排序；J++ 的 `order()` 直接吃判断的原始读数（不需要先把结果写回材料再读出来排序），只是这里因为 `order` 不吃 `sieve` 的输出契约值，`prog.jpp` 改用手动建行的方式接住，行数没有排序本身那么好看，但排序这件事真正需要作者手写的逻辑（比较函数、稳定性、reverse）确实被 `order` 内置吸收了。

## 4 B4-01-RoboJEV（折行倍率 9.21）

说明：measure.toml/result.json 声明的核心行数是 396（`policy.py`=282、`challenge_policy.py`=78、`push_policy.py`=36，排除空行/注释口径）。本表按物理行范围分块，逐块合计约 411，略高于声明值（同样是空行/注释差异）。这个项目的一大特点（`result.json` 自己点破）：折行倍率比预注册预测高约 30~50%，主因是 `RULES`/`MOTOR_RULES`/`axis_criteria`/`motion_criteria` 这类大段自然语言规则与候选描述文本——它们是喂给 JEV 的题面材料本身，不是判断分支代码，J++ 侧同等内容搬进 `jpp_input.json`（由 `adapter.py` 从调用日志原样取回），**不计入 `prog.jpp` 行数，但计入原项目核心行数**，这是本项目"保留"类占比異常高的原因，如实记录，不是分类夸大。

- 核心总行数：396（measure.toml 口径）；J++ 行数：43
- 逐块分类：

| 原代码行范围 | 行数 | 这块在干什么 | 归类 | 对应 J++ |
|---|---|---|---|---|
| policy.py:1-17 | 17 | imports | D | 无对应 |
| policy.py:19-35 | 17 | `RULES`：给 x/y/z/gripper 四题共用的大段自然语言操作规则文本 | 保留 | 由 adapter.py 从调用日志取回，进 `jpp_input.json`，不在 prog.jpp 里重新拼 |
| policy.py:38-61 | 24 | `request_body()`：组装 questions 字典（x/y/z 轴题 + gripper 题，各自 criteria+instructions） | C（数据整形，题面拼装） | prog.jpp:26-29 `select(s.axis_text.x, ...)` 等四次 `select` |
| policy.py:64-91 | 28 | `axis_criteria()`：按 task/是否持有 cube 动态生成 x/y/z 三轴的候选描述文本 | 保留 | `s.axis_labels`/`s.axis_text` 数据源 |
| policy.py:94-108 | 15 | `validate_answer()`：校验答案是字典、choice 在候选里、probabilities 键集合匹配、所有数值在 [0,1]、概率和≈1、choice 是 argmax | A（拿不准的处理，好例子候选） | 无对应 |
| policy.py:111-128 | 18 | `INTENTS`：8 种意图（approach/grasp/lift/carry/lower/release/withdraw/finish）的候选描述文本 | 保留 | `s.intent_labels`/`s.intent_text` 数据源 |
| policy.py:130-137 | 8 | `MOTOR_RULES`：intent 已选定后、motor 阶段四题共用的规则文本 | 保留 | 同上 |
| policy.py:140-165 | 26 | `intent_body()`：按 task 是否在 CHALLENGES 里分两条分支，动态拼 intent 题的 criteria+instructions | C（数据整形，题面拼装） | prog.jpp:20 `select(s.intent_text, "robo-intent-argmax")` |
| policy.py:168-184 | 17 | `motor_criteria()`：intent 已知后，按 intent 动态生成 x/y/z 轴候选描述 | 保留 | `s.axis_labels`/`s.axis_text` 数据源（motor 阶段版本） |
| policy.py:187-201 | 15 | `JevPolicy.__init__`/`close()`：读 API key、建 httpx client、无 key 就报错 | D | 无对应 |
| policy.py:203-208 | 6 | `decide()`：计时包装，finally 记录 latency | A（费用与调用计数） | 无对应 |
| policy.py:210-219 | 10 | `_decide()` 起手：`jev_stages==2` 时组装 intent 请求、调用 `_query`、`validate_answer` 取 intent 字符串 | C（7 行组装）+ 保留（1 行调用 `_query` 本身）+ C（2 行取值） | prog.jpp:19-23 `intent_rows = map(filter(...), fn(s) { ... judge(st, iq) ... })` |
| policy.py:220 | 1 | `body = request_body(state, config)` | C（数据整形） | prog.jpp:25-29 `motor_rows` 的题面构造 |
| policy.py:221-242 | 22 | 已拿到 intent 后，把 intent 写入 state，**用 intent 结果替换 x/y/z/gripper 四题的 criteria**（换成 `motor_criteria`/`push_motor_criteria`）和 rules（换成 `MOTOR_RULES`/`PUSH_RULES`） | B（组合方式：结果回流，上一轮 intent 判断结果决定下一轮 motor 判断的候选集/材料，好例子候选） | prog.jpp 侧 intent 与 motor 两组题各自独立提问、不互相决定候选集（见下方好例子说明这处简化） |
| policy.py:243-247 | 5 | 若 task 是挑战任务（peg_insert 等），用 `motion_criteria()` 整体覆盖题面 | C（数据整形） | 同上，题面来自 `s.axis_text`（adapter.py 已按 task 分好） |
| policy.py:248-251 | 4 | 组装 motor_exchange，调用 `_query` 发起判断 | 保留 | prog.jpp:34 四次 `judge(...)` |
| policy.py:252-254 | 3 | 对 x/y/z/gripper 四个答案分别调用 `validate_answer` | A（拿不准的处理） | 无对应 |
| policy.py:255-258 | 4 | 合并 intent 请求与 motor 请求两次调用的 token 用量统计 | A（费用与调用计数） | 无对应 |
| policy.py:259-264 | 6 | 组装最终 `EefDecision`（x/y/z/gripper + metadata：intent/usage/latency/attempts） | C（数据整形） | prog.jpp:59-60 结果记录组装 |
| policy.py:266-271 | 6 | `_query()`：计时包装，finally 记录 latency | A（费用与调用计数） | 无对应 |
| policy.py:273-308 | 36 | `_query_impl()`：HTTP 429/500/502/503/504/529 状态码重试（读 `retry-after` 头或指数退避）、传输错误重试、成功后校验 answers 的键集合与 model 版本 | A（重试与失败，好例子候选） | 无对应 |
| challenge_policy.py:1-6 | 6 | docstring + import | D | 无对应 |
| challenge_policy.py:7-30 | 24 | `intents(task)`：8 个基础意图描述 + peg_insert/门任务的差异化覆盖 | 保留 | 候选文本数据源 |
| challenge_policy.py:33-84 | 52 | `motion_criteria(task)`：按 task/axis/choice 三层条件动态拼接候选描述文本（大量字符串拼接与条件分支） | C（数据整形，题面拼装，含真实的字符串拼接逻辑） | 候选文本数据源（这段生成逻辑本身在 J++ 侧不存在，adapter.py 只取生成后的最终文本） |
| push_policy.py:1-8 | 8 | docstring + import | D | 无对应 |
| push_policy.py:9-20 | 12 | `PUSH_INTENTS`：5 个 push 任务意图描述 | 保留 | 候选文本数据源 |
| push_policy.py:22-30 | 9 | `PUSH_RULES` 文本 | 保留 | 候选文本数据源 |
| push_policy.py:33-44 | 12 | `push_motor_criteria(axis)`：x/y 轴用 dict comprehension 拼接，z 轴手写字典 | C（数据整形） | 候选文本数据源 |

- 本项目小计（按表格行汇总，约 411 物理行，declared 核心为 396）：A=70 B=22 C=135 D=46 E=0 保留=138
- 一句话：这个项目和其他四个都不一样——被"压掉"的行里超过三分之一（保留=138）根本不是杂活，而是判断题面本身的自然语言规则/候选描述文本，只是换了个文件存放（从 Python 字符串字面量搬进 `jpp_input.json`），`result.json` 自己也承认这是折行倍率偏高于预注册预测的主因；真正被语言构造替掉的杂活集中在 **A**（HTTP 重试退避 36 行 + 响应合规校验 15+3 行 + 三处费用/延迟统计，共 70 行）和 **C**（各函数动态拼题面的组装代码，135 行），另有 22 行 **B**（intent 判断结果决定 motor 判断候选集，这套"结果回流"在 J++ 版本里被简化成两组题各自独立提问，不做真正的候选集替换）。
- 候选好例子 1（HTTP 重试与响应校验，整段消失）：
  ```python
  # policy.py:273-297（节选，完整函数 273-308 共 36 行）
  for attempt in range(self.config.api_retries + 1):
      exchange["attempts"] += 1
      try:
          response = self.client.post(self.config.api_url, json=exchange["request"],
                                      headers={"Authorization": f"Bearer {self._key}"})
      except httpx.TransportError:
          if attempt == self.config.api_retries:
              raise PolicyError("TypeSafe transport error; no action executed") from None
          self.sleep(min(2**attempt, 5))
          continue
      exchange["http_status"] = response.status_code
      if response.status_code in {429, 500, 502, 503, 504, 529} and attempt < self.config.api_retries:
          try:
              delay = float(response.headers.get("retry-after", 2**attempt))
          except ValueError:
              delay = 2**attempt
          self.sleep(min(max(delay, .1), 10) if math.isfinite(delay) else 1)
          continue
      if response.status_code != 200:
          raise PolicyError(f"TypeSafe returned HTTP {response.status_code}; no action executed")
      break
  ```
  ```
  （J++ 侧无对应代码——0 行；prog.jpp:34 的 judge(stx, xq) 一行覆盖发请求、重试、拿读数全过程）
  ```
  说明：原项目手写了一套完整的传输层重试策略——网络错误重试、5xx/429 按 `retry-after` 头或指数退避重试、超过重试次数才报错——外加成功之后还要校验返回的 answers 键集合和 model 版本对不对。`judge()` 是 J++ 的内置效应，这些全部由运行时负责，程序里不出现"重试"两个字。

- 候选好例子 2（intent 决定 motor 候选集，J++ 简化成两组独立提问）：
  ```python
  # policy.py:221-232（节选）
  if intent is not None:
      body["state"]["jev_intent"] = intent
      for axis in "xyz":
          body["questions"][axis]["criteria"] = motor_criteria(axis)
      body["questions"]["gripper"]["criteria"] = {
          "open": "Selected intent is approach, release, withdraw or finish: keep/open fingers.",
          "close": "Selected intent is grasp: close fingers around the aligned cube.",
          "hold": "Selected intent is lift, carry or lower: preserve closed grasp.",
      }
      for question in body["questions"].values():
          question["instructions"]["rules"] = MOTOR_RULES
  ```
  ```jpp
  // prog.jpp:19-35（intent_rows 与 motor_rows 各自独立 map，不互相决定候选集）
  let intent_rows = map(filter(input.scenarios, fn(s) { s.jev_stages == 2 }), fn(s) !{judge} {
      let iq = select(s.intent_text, "robo-intent-argmax");
      ...
  });
  let motor_rows = map(input.scenarios, fn(s) !{judge} {
      let xq = select(s.axis_text.x, "robo-x-argmax");
      ...
  });
  ```
  说明：原项目里 intent 判断的结果会**实际改写** motor 阶段四道题的候选描述文字（比如"approach"意图选中后，x/y/z 轴的候选描述就换成"仅在 approach 或 carry 时才会选这个方向"这类以 intent 为前提的表述）；J++ 版本没有重新实现这种"用上一轮结果改写下一轮题面"的机制——`s.axis_text` 是 adapter.py 按每个场景已经算好的、intent 已知情形下的最终文本，intent 判断和 motor 判断在 prog.jpp 里各自独立提问，行为等价（因为夹具按场景固定），但把"结果回流改写候选集"这套真实存在的机制变成了两组并列题目，这是本表唯一一处归为 B 的地方，也是这个项目里"组合方式"没有被完整复现、而是被数据层面绕过的例子。

## 5 B4-09-jev-effort-router（折行倍率 10.95）

说明：`measure.toml` 声明的核心行数为 427（`router.py(83-192,230-276)`=118、`client.py(all)`=222、`state.py(18-35,147-160,185-188)`=32、`grid.py(25-67,113-152)`=55），J++ 行数 37（`result.json`）。这个 427/32/55 是 measure.toml 口径的计数（排除空行与部分注释/文档字符串行），本表为了讲清楚"这块代码在干什么"按逻辑块切分，逐块记的是该块的**物理行范围**（含少量空行），因此表格行数合计（约 536）会高于官方声明的 427——差额是空行、纯分节注释这类不计入"核心"但仍落在行号范围内的内容，不代表多算了逻辑块，核对时以行号范围能对上原文件为准。

- 核心总行数：427（measure.toml 口径）；J++ 行数：37
- 逐块分类：

| 原代码行范围 | 行数 | 这块在干什么 | 归类 | 对应 J++ |
|---|---|---|---|---|
| router.py:83-104 | 22 | Hermes 中间件入口 `on_llm_request` 的签名（14 个具名参数+`**kwargs`）与 docstring | D | prog.jpp:69 `map(input.requests, fn(r) {...})` 直接读结构化输入，无需声明中间件参数签名 |
| router.py:105-109 | 5 | try/except 包住取配置，异常就原样放行请求 | A（重试与失败） | 无对应 |
| router.py:111-118 | 8 | 构造 `_Where` 审计元组（turn_id/session_id/platform 等打包） | C（数据整形） | 无对应 |
| router.py:120-137 | 18 | 五道跳过门：未启用/provider 不符/api_mode 不符/model 不在 grid/请求不是 dict | E | `result.json` gaps「provider/api_mode 跳过门控未测」——本批不覆盖，见下方说明 |
| router.py:139-144 | 6 | 查 memo 缓存（按 turn/session），命中就直接用缓存的旧决定，不再判断 | A（缓存） | 无对应（J++ 每次都判断，不做跨轮缓存） |
| router.py:145-149 | 5 | 缓存未命中，调用 `_decide` 发起一次判断 | 保留 | prog.jpp:37,39 `judge(st_m, model_q)` / `judge(st_e, effort_q)` |
| router.py:150-161 | 12 | 校验判断选中的模型是否真的在 provider 目录里，不在就丢弃、记 skip、回退 | A（拿不准的处理） | 无对应；design_deviations 说明 J++ 的 `select()` 结构上不可能选出候选集外的答案，这类校验整段消失（好例子候选） |
| router.py:162-179 | 18 | 构造 `Memo` 对象，写入 turn/session 缓存（含解释为何用"memo为空"当首次判断信号的长注释） | A（缓存） | 无对应 |
| router.py:181-183 | 3 | 调用 `_apply` 组装最终请求、记路由审计、返回结果 | C（数据整形） | prog.jpp:44/55 `{model:…}`/`{effort:…}` 结构组装 |
| router.py:184-191 | 8 | 顶层 try/except 兜底，任何异常都吞掉、记 skip、放行原请求 | A（重试与失败） | 无对应 |
| router.py:230-234 | 5 | `_decide` 签名+取 `messages` 字段+校验非空 | C（数据整形） | 无直接对应（材料直接来自 input.json） |
| router.py:236-246 | 11 | try/except 包住对判断接口 `client.decide(...)` 的调用 | A（重试与失败） | 无对应 |
| router.py:248-253 | 6 | 判断返回 None 时记日志、记 skip、放行原请求 | A（重试与失败） | 无对应 |
| router.py:255-269 | 15 | `_apply` 方法的 docstring（解释为何用 `original_request`、为何 `reasoning_config` 不能上线） | D | 纯文档，无逐字对应 |
| router.py:270-276 | 7 | 组装最终 request：写入 `model`，按 `effort` 是否为空写入或清除 `reasoning_effort` | C（数据整形） | prog.jpp:44,55（model_out/effort_out 组装）+ postprocess.py 的 clamp |
| client.py:1-25 | 25 | 模块 docstring + imports + logger + 15 个 `REASON_*` 错误码常量 | A（拿不准的处理） | 无对应；`REASON_*` 本质是给"拿不准/失败"打标签的枚举 |
| client.py:45-64 | 20 | `Decision` dataclass 字段声明 + `degraded` property | D | prog.jpp 里 `{model:…, effort:…, exit:…}` 字面量结构 |
| client.py:67-78 | 12 | `_as_float`/`_as_probabilities`：把原始 JSON 值安全转成 float/dict | C（数据整形） | 无对应（judge() 直接给出结构化读数） |
| client.py:81-107 | 27 | `_answer`/`_choice_text`/`answer_type`：从 `answers` 字典按 question_id 取值、校验类型是 choice 才读 choice 字段 | C（数据整形） | 内置 judge() 读数结构本身 |
| client.py:110-122 | 13 | `JevClient` 类定义+docstring+`__init__` | D | 无对应 |
| client.py:126-153 | 28 | `_post`：组装 HTTP headers、调用注入的 transport 或懒加载 httpx 发请求 | D | judge() 内置效应，HTTP 传输细节不需要手写 |
| client.py:157-176 | 20 | `decide` 签名+大段 docstring（解释为何不传 `current_model`：会把判断锚定在已配置模型上） | D | 纯文档 |
| client.py:177-179 | 3 | `if not api_key(): return None, REASON_NO_API_KEY` | A（拿不准的处理） | 无对应 |
| client.py:181-188 | 8 | `build_payload` 组装发给 JEV 的请求体（题面拼装） | C（数据整形） | prog.jpp:36 `state(mat(r.state), {over: MODEL_LABELS})` |
| client.py:190-201 | 12 | try/except 包住 HTTP 调用，区分超时与其他错误返回不同 reason | A（重试与失败） | 无对应 |
| client.py:202-208 | 7 | 检查 HTTP 状态码是否在 200-300 之间 | A（重试与失败） | 无对应 |
| client.py:210-214 | 5 | try/except 解析 JSON 响应，失败返回 MALFORMED | A（重试与失败） | 无对应 |
| client.py:218-229 | 12 | `_interpret` 签名+校验 data 是 dict+取 answers+两问 answer 都为空则 MALFORMED | C（数据整形） | prog.jpp:37,39 judge() 直接返回结构化读数 |
| client.py:232-248 | 17 | model 分支：取 choice/confidence/probabilities，查表 `resolve`，未命中或低置信度就丢弃 chosen | A（门槛+拿不准，好例子候选） | prog.jpp:41-50 两层 `cut(rm, "router-model-argmax")` + `cut(rm, {stat:"confidence", declare:{hi:0.5}})` |
| client.py:250-254 | 5 | chosen 为空则回退 `default_model` | A（拿不准的处理） | prog.jpp:45,46 ignore/unsure 臂 `{model: input.default_model, ...}` |
| client.py:256-283 | 28 | effort 分支：与 model 分支几乎并列重复的取值+门槛判断+回退（含大段注释解释"没回答"和"回答但不信"两种要分开处理） | A（门槛+拿不准） | prog.jpp:52-61 同构的 `cut(re, ...)` 两层 |
| client.py:284-287 | 4 | `requested` 为空但已作答则回退 `default_effort`；调用未改的 `resolve_effort` 做 clamp | E | `result.json` gap「clamp 表（effort.py）不进 J++ 判断本身」——移到 postprocess.py 宿主胶水，J++ 判断核心只产出 clamp 前的 `effort_requested` |
| client.py:289-291 | 3 | 构造 `alternatives` 审计字段（其余候选模型列表） | A（费用与调用计数/审计） | 无对应；`result.json` excluded_fields 里明确排除这个字段 |
| client.py:292-310 | 19 | 组装 `Decision(...)` 返回值，把 model/effort 两路结果打包成一个对象 | C（数据整形） | prog.jpp:63-64 `{id:…, model:…, effort_requested:…, model_exit:…, effort_exit:…}` |
| client.py:313-319 | 7 | `_criteria_map`：从 model_answer 取 criteria 字段做类型转换 | C（数据整形） | 内置 judge() 读数结构 |
| state.py:18-19 | 2 | 两个题目 ID 常量（MODEL_QUESTION_ID/EFFORT_QUESTION_ID） | 保留 | prog.jpp 的 `select()` 调用与题目定义对应，概念保留，只是换了位置 |
| state.py:21-29 | 9 | 两道题的题面文本（法语 instructions） | 保留 | input.json 的 `model_text`/`effort_text` 数据源 |
| state.py:31-35 | 5 | `EFFORT_CRITERIA` 三档候选描述文本（low/medium/high） | 保留 | input.json 的 `effort_labels` |
| state.py:147-160 | 14 | `build_questions`：组装两个 Choice 题目的 JSON 结构（type/instructions/criteria） | C（数据整形） | prog.jpp:17-18 `select(input.model_text, ...)`/`select(input.effort_text, ...)` 内置直接接收题面 |
| state.py:185-188 | 4 | `normalise_effort_choice`：规范化回答文本（strip/lower）并校验在 `EFFORT_LEVELS` 里 | C（数据整形） | 内置 `select()` 回答本身就是候选索引，不需要规范化字符串 |
| grid.py:25-32 | 8 | `Entry` dataclass 字段声明 | D | 无对应 |
| grid.py:34-37 | 4 | `criterion` 属性，拼接成 `"model_id: description"` 字符串 | C（数据整形） | input.model_labels 数据源（题面拼装） |
| grid.py:40-67 | 28 | `DEFAULT_GRID`：6 个模型的候选描述数据 | 保留 | input.json 的 `model_labels`/`grid_model_ids` |
| grid.py:113-120 | 8 | `criteria()`：把 grid 转成位置索引字典（`{"1": ..., "2": ...}`） | C（数据整形） | prog.jpp 里 `select` 内置处理候选集，不需要手动编号 |
| grid.py:123-152 | 30 | `resolve()`：把 JEV 回答的 choice 字符串按四种方式（位置 key/index 前缀/model_id 原文/criteria 文本）挨个匹配回 grid entry，全部失败返回 None | A（拿不准的处理，好例子候选） | **无对应，0 行**；design_deviations 说明 J++ `select()` 结构上只能选出候选集内的答案，这套"回答对不上候选"的兜底整段消失 |

- 本项目小计（按表格行汇总）：A=201 B=0 C=138 D=126 E=22 保留=49（合计约 536 物理行，高于 measure.toml 官方口径的 427，差额为未单列的空行/分节注释，见表格上方说明）
- 一句话：这个项目被压掉的行主要是 **A 杂活**（confidence 门槛判断+回退、异常包装、memo 缓存读写，占比最大）和 **C 数据整形**（题面拼装、从判断器响应对象层层取字段、组装最终请求结构），因为这是一个"两问路由器"而不是多判断串联的项目，几乎没有 B 组合方式；此外有 22 行属于 E（原项目的 skip 门控与 effort clamp 表，J++ 版本没有覆盖/移到了宿主胶水）。
- 候选好例子 1（门槛+回退，两路重复代码 → 两层 cut）：
  ```python
  # client.py:237-248（effort 分支 256-283 几乎逐行重复同一套逻辑）
  chosen = resolve(_criteria_map(model_answer), model_choice_text, grid)
  if chosen is None:
      reasons.append(REASON_UNKNOWN_CHOICE)
  elif model_confidence < settings.confidence_threshold:
      reasons.append(REASON_LOW_CONFIDENCE)
      chosen = None
  if chosen is None:
      chosen = self._settings.entry_for(settings.default_model)
      model = chosen.model_id if chosen else settings.default_model
  else:
      model = chosen.model_id
  ```
  ```jpp
  // prog.jpp:41-50
  let model_out = handle(cut(rm, "router-model-argmax"), {
      pick: fn(km) {
          handle(cut(rm, {stat: "confidence", declare: {hi: 0.5}}), {
              act: fn() { {model: input.grid_model_ids[km], exit: unit} },
              ignore: fn() { {model: input.default_model, exit: unit} },
              unsure: fn(u) { {model: input.default_model, exit: u} }
          })
      },
      unsure: fn(u) { {model: input.default_model, exit: u} }
  });
  ```
  说明：原代码要手写"选中的候选是否真的能查到、置信度够不够，两种情况分别回退到默认模型"，而且 model 和 effort 两路各写一遍，没法复用；J++ 用两层 `cut`（先按 argmax 取候选，再在 confidence 这个统计量上切一刀）把同一套判断线表达成三个出口（选中且够信→用它、够信但线下→用默认、argmax 本身未决→用默认），effort 那一路是同一结构直接复用，不用再写一遍。

- 候选好例子 2（候选匹配兜底 → 结构性消失）：
  ```python
  # grid.py:123-152
  def resolve(criteria_map, choice, grid):
      if choice is None:
          return None
      key = str(choice).strip()
      if not key:
          return None
      by_position = {str(i): e for i, e in enumerate(grid, start=1)}
      if key in by_position:
          return by_position[key]
      head = key.split(":", 1)[0].strip()
      if head in by_position:
          return by_position[head]
      for candidate in (key, head):
          for entry in grid:
              if entry.model_id == candidate:
                  return entry
      for position, text in (criteria_map or {}).items():
          if text == key:
              return by_position.get(str(position))
      return None
  ```
  ```
  （J++ 侧无对应代码——0 行）
  ```
  说明：原项目要防着判断接口回显一个自己认不出的字符串（位置号、"序号: 文本"、model_id 原文、候选描述原文四种可能都要试），因为它调的是通用 HTTP 端点，返回的是裸字符串；J++ 的 `select()` 内置就从候选集里选，答案类型本身就是候选索引，不存在"回答对不上候选表"这种状态——这不是把 30 行压缩成几行，是这类代码在 J++ 里根本没有存在的理由。

## 本批次汇总

**口径提醒**：下表把 5 个项目的 A–E 行数直接相加。其中 4 个项目（B5-10、B4-12、B4-01、B4-09）用"原代码物理行范围"计数，B4-06 用"净压掉行数"（原代码行数 − 对应 J++ 行数）计数——两种口径混在一起相加会让 B4-06 的贡献偏小（净压掉天然低于原始物理行数），下表数字因此是一个偏保守的合计，不是同口径的精确加总；分项目的数字（见上方各节）比这张汇总表更可信。

| 类别 | 合计行数 | 占被压掉行数（A+B+C+D+E）的比例 |
|---|---|---|
| A 杂活 | 409（B4-09:201 + B5-10:0 + B4-12:70 + B4-01:70 + B4-06:68） | 27.7% |
| B 组合方式 | 80（B4-09:0 + B5-10:0 + B4-12:7 + B4-01:22 + B4-06:51） | 5.4% |
| C 数据整形 | 457（B4-09:138 + B5-10:31 + B4-12:110 + B4-01:135 + B4-06:43） | 31.0% |
| D 语法差异 | 309（B4-09:126 + B5-10:0 + B4-12:63 + B4-01:46 + B4-06:74） | 21.0% |
| E 功能缺失 | 220（B4-09:22 + B5-10:108 + B4-12:26 + B4-01:0 + B4-06:64） | 14.9% |
| **合计（A–E）** | **1475** | 100% |
| 保留（不计入比例，仅作参照） | 203（B4-09:49 + B5-10:0 + B4-12:16 + B4-01:138 + B4-06:7） | — |

这批"折行倍率最高"的 5 个项目里，最大的两块被压掉的行是 **C 数据整形（31.0%）** 和 **A 杂活（27.7%）**，合计接近六成——和研究 18 报告的"重试与失败、调用循环、门槛是最普遍的三类杂活"的结论方向一致，但这批高倍率项目里题面拼装/响应取值（C）的分量明显超过研究 18 全量 84 项目统计里单看"杂活占比"给出的印象，原因是本批 5 个项目普遍围绕"一次或几次独立判断+复杂的题面组装"展开（路由、蛇游戏、检索过滤、机械臂），不是"多道判断串联决策"的项目，B 组合方式（5.4%）因此在这批里明显偏低，与研究 18 里"多个判断合成（6.2%）、并发限制（11.1%）是最少见的两类"的发现互相印证。

E（14.9%）与 D（21.0%）也不小：E 集中在两个项目——B5-10 的 95 行多 tick 自动对局循环（整个功能范围不在被测范围内，不是语言做不到）和 B4-09 的 skip 门控/effort clamp（本批未覆盖或移到宿主）；D 主要是各语言的类型声明、struct/dataclass、docstring 这类和判断逻辑无关的样板代码。**保留类（203 行，不计入比例）里 B4-01 一个项目就占了 138 行**——这是本批唯一一个"判断题面本身的自然语言规则文本"占比极高的项目，如果不把这类文本单独摘出来算"保留"，会把它的杂活占比虚高地算进 A/C。

## 挑出的好例子（如果有）

以下从每节摘出最典型的对照，完整代码见上方对应小节：

1. **B4-06 client.go `validateChoice()`（35 行）→ 0 行**：原项目直接调用通用 HTTP 端点，必须自己校验对方有没有乱回（choice 在不在候选表里、confidence/概率是不是 [0,1] 内有限数、choice 是不是真的 argmax）；J++ 的 `judge()`/`select()` 内置保证读数结构合规，这套自我防御性校验整段消失。
2. **B4-06 client.go `Ask()` 的 429/529 重试圈（19 行）→ 0 行**：`judge()` 是内置效应，HTTP 请求怎么发、要不要重试是运行时职责，程序里不出现"重试"两个字。
3. **B4-12 strategies.py `score()`/`ascore()` 的线程池/信号量并发调度（22 行）→ 1 行 `map`**：原项目自己承认这是折行倍率比预注册预测高出约 50% 的主因；J++ 把"逐个候选发起判断"的并发调度交给运行时接管。
4. **B4-01 policy.py `_query_impl()` 的 HTTP 429/500-529 状态码重试 + `retry-after` 退避（36 行）→ 0 行**：本批"重试与失败"写得最完整的一份代码，`judge()` 一行覆盖发请求、重试、拿读数全过程。
5. **B4-09 grid.py `resolve()`（30 行）→ 0 行**：原项目要防着判断接口回显一个自己认不出的字符串，四种匹配方式挨个试；J++ `select()` 的答案类型结构上就是候选索引，不存在"回答对不上候选表"这种状态——这不是压缩，是这类代码根本没有存在的理由。
6. **B5-10 route.ts `moveCriteria`（19 行）→ 材料+通用 instructions**：唯一一个"简化而非消失"的例子——原项目给每个候选单独拼一句推理提示，J++ 把线索留在材料里、候选只给名称，代价是这处简化被 `result.json` 记录为真机上可能改变读数的已知缺口，不是免费的压缩。
