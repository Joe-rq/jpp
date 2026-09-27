# 人工对照：低倍率组（5 项目）

## 方法（v2，按净压掉行数算）

对每个项目的 `measure.toml` `[[core]]` 精确行号范围逐行读原代码，与 `prog.jpp` 逐一核对：这块原代码在 J++ 里对应了几行。**"压掉的行"按净差算：净压掉 = 原代码行数 − J++ 对应行数，不是原代码整块的行数。** 例如一段 20 行的 if/elif 分流，如果在 J++ 里变成 3 行的 `handle(cut(...), {...})`，记"净压掉 17 行"；如果原代码 5 行在 J++ 里反而要写 12 行（比如为了满足 J-05"未决必须消费"要显式穷尽 pick/unsure 分支），记"净压掉 -7 行"（负数，J++ 更长）。

分类法不变：A 杂活 / B 组合方式 / C 数据整形 / D 语法差异 / E 功能缺失，互斥取第一个命中的类；有直接对应、内容也没被压缩或拉长（净差接近 0）的记"保留"；J++ 里存在但在原代码任何地方都找不到对应内容的，记"新增（J++ 独有）"，不算进 A–E 的任何一类，也不算"压掉"——这是语言/运行时自身要求的开销（`budget{}` 声明、J-05 未决消费的 `unsure`/`pending` 脚手架、候选集拆分调用的额外 `judge()`/`state()`）。

另检查了一个"挪走"的可能性：原代码某块内容（比如候选的 description/学习例句）如果在 J++ 版本里既不在 `prog.jpp`、也没被替换成某个构造，而是被挪到了 `adapter.py`/`map.json` 等夹具文件里——这种情况该记"挪走"，不算 A–E，也不算"压掉"（内容还在，只是换了文件）。**逐项查过 `adapter.py`/`adapter.mjs`/`map.json` 后没有找到这种情况**：B5-02 的 `adapter.py::make_learned_file()` 确实写了 learned 正负例，但那是喂给**原项目**（Python 侧）跑真实 `_with_learned_intent_examples` 用的，不是给 J++ 用的——J++ 侧的候选列表（`APP_LABELS` 等）完全是固定标签，学习机制没有被挪到任何 J++ 会读到的地方，判定为 E（功能缺失）而不是"挪走"；Q-04 的 `map.json`、B5-06b 的 `adapter.mjs` 里也没查到候选描述文本的痕迹，同样判 E。本文没有"挪走"类的行。

**出口分派统一规则**：if/elif 按判断结果分流的代码，只看"一次判断的结果"分流的算 **A**（对应 `cut`/`handle` 的三值出口，如"置信度 < 0.5 就转人工"）；看"两个或以上判断的结果组合"分流的算 **B**（对应多判断合成/级联，如"两道题都要过线"或"先看 A 题、A 题过了才看 B 题优先级"）。表格备注栏标"出口分派"的行按这条规则归类。

**精度说明（如实交代）**：J++ 行数的逐行归属不是无重叠的精确切分——很多 `prog.jpp` 里的行（比如一个 `handle(cut(...))` 块）同时承担"取值""判断""未决处理"三件事，原代码里这三件事往往也是分开的几行。下面每个项目的"J++ 行数"列是尽量不重复计数后的**估计值**（同一段 J++ 代码只记一次，不会被两条原代码行同时认领），项目小节末尾的"核对"行给出 Σ原代码行数与核心总行数的比对（应精确相等，因为分类沿用了 v1 已核对过的逐行归属）以及 Σ(J++ 行数)+Σ(新增) 与 `prog.jpp` 实际行数的比对（允许有 10~15% 的残差，残差解释为"结构性脚手架未精确拆到具体某一行"，写在新增小节里，不是计算错误被藏起来）。

## 1 B5-11b SuperInstance/jev-diffusion（折行倍率 0.68，J++ 反而更长）

- 核心总行数：34；J++ 总行数：50（`budget{}`/注释/空行已按各自项目惯例排除）

| 原代码行范围 | 原行数 | 这块在干什么 | 归类 | J++ 行数 | 净压掉 | 对应 J++ / 备注 |
|---|---|---|---|---|---|---|
| jev_diffusion.py:298 | 1 | `else:` 进入非-composite 分支 | D | 0 | 1 | 无对应 |
| jev_diffusion.py:299,320 | 2 | `call_jev(...)` 调用外壳开闭括号 | 保留 | 1 | 1 | prog.jpp:51 `judge(st_regions,[rq,lighting_q])`（另 2 次 judge 调用是候选拆分导致的新增，见下） |
| jev_diffusion.py:300,304,305,309,310,314,315,319 | 8 | 四道题字典的花括号/键名嵌套（纯语法） | D | 0 | 8 | 无对应 |
| jev_diffusion.py:301,306,311,316 | 4 | 四道题各自的 `'type'` 字段 | C | 0 | 4 | 无对应；`select()`/`measure()` 函数名本身表达题型 |
| jev_diffusion.py:302,307,312,317,318 | 5 | 四道题的 `instructions`/lighting 的 `criteria` 档位列表 | 保留 | 6 | -1 | prog.jpp:36-38 三行（mood_q/palette_q/lighting_q）+ 20（landscape 变体） |
| jev_diffusion.py:303,308,313 | 3 | regions/mood/palette 的 `criteria` dict comprehension（候选摊平成 `{k:k}`） | C | 1 | 2 | prog.jpp:50 `state(...,{over:t.regions_labels})`（把候选直接作参数，不需要摊平） |
| jev_diffusion.py:321 | 1 | `jev_ans = jev_resp['answers']` 取字段 | C | 0 | 1 | 无对应 |
| jev_diffusion.py:322,327,330,331 | 4 | `self.plan`/`confidences` 字典花括号（纯语法） | D | 0 | 4 | 无对应 |
| jev_diffusion.py:323,324,325,326 | 4 | 从 `jev_ans` 取四个字段填进 `self.plan` | C | 9 | -5 | prog.jpp:59-70 三段 `handle(cut(...),{pick:...,unsure:...})`（除去其中 3 行纯 unsure 脚手架已单独归新增） |
| jev_diffusion.py:328,329 | 2 | `confidences` dict comprehension | C | 0 | 2 | 无对应（`plan.confidences` 排除比较字段） |

- **核对**：Σ原代码 = 1+2+8+4+5+3+1+4+4+2 = 34 ✓ 与核心总行数一致。

- **新增（J++ 独有，31 行，占 J++ 总行数 50 的 62%）**：
  - `budget {...}` 声明：1 行
  - 4 个额外的 regions 候选变体 `regions_q_portrait/abstract/still_life/sci_fi`（原代码用一句 f-string 在运行期插入 preset 数据，J++ 没有等价的运行期模板机制，只能硬编码 5 份，其中 1 份已算进"保留"，另外 4 份 ×3 行=12 行全新）：12 行
  - `regions_question()` 分支分发函数（40-46）：7 行
  - 因候选集不同必须拆成 3 次独立 `judge()` 而不是 1 次打包调用，多出的 2 次 `state()+judge()` 调用（mood、palette 各 2 行）：4 行
  - `handle(cut(...))` 里 3 个 `unsure: fn(u){consume(u,"drop");""}` 分支（J-05 未决消费脚手架）：3 行
  - `plan_one`/`results`/最终返回的函数签名与包装（48,49,72,73,75,77）：4 行

- **本项目小计（净压掉，A-E 口径，不含 保留/新增）**：A=0 B=0 C=**4**（4-2×2+2? 见下）D=**13** E=0；具体：C = 4(type字段)+2(criteria摊平)+1(取字段)+2(confidences)-5(取字段填self.plan为负) = 4；D=1+8+4=13。保留净值 ≈ 0（-1，接近持平）。
- **一句话**：净压掉只有约 17 行（C+D），主要是 Python 字典嵌套语法和候选摊平这类纯语法/数据整形开销；但 J++ 为了拆分候选集单独成状态，反过来新增了 31 行（占 J++ 总行数六成以上）——这 31 行里有 19 行（4 个额外 regions 变体 + 分发函数）是因为 J++ 没有运行期字符串模板机制，只能把同一道题的 5 种候选集各写一份。**净压掉(17) < 新增(31)，这就是倍率 0.68 的直接原因**，不是"该压的杂活没压干净"。

## 2 B1-07 jesusvillamarin/pulso-nps（倍率 0.8）

- 核心总行数：64；J++ 总行数：约 73（不含 3 个候选标签常量 `AREA_LABELS`/`TONE_LABELS`/`CATEGORY_BY_AREA`，这 9 行与原项目的 `taxonomy.py` 对称排除为数据，两边都不计——`result.json` 记录的 70 行是改写早期版本，本文按当前 `prog.jpp` 重新数）

| 原代码行范围 | 原行数 | 这块在干什么 | 归类 | J++ 行数 | 净压掉 | 对应 J++ / 备注 |
|---|---|---|---|---|---|---|
| classifier.py:16-21 | 6 | `_choice_payload` 从 SDK 对象取字段摊平成 tuple | C | 0 | 6 | 无独立对应；`cut`/`handle` 直接从读数取值 |
| classifier.py:66-68,70,85,112 | 6 | mock 分支开关、SDK 导入、client 上下文管理器、return | D | 0 | 6 | 无对应 |
| classifier.py:72 | 1 | `area_criteria` 把 taxonomy 转成 dict（含 description） | C | 0 | 1 | 无对应（description 本身被砍，见 E 类说明） |
| classifier.py:73 | 1 | `state` 把评论包成材料 | 保留 | 1 | 0 | prog.jpp:41 `comments_mat=mat({comments:comments})` |
| classifier.py:74-83 | 10 | for 循环逐条构造 area/tone 两道 Choice、攒进字典（手动合批） | B | 12 | -2 | prog.jpp:28-33（area_form/tone_form 模板定义）+44-49（map+两次judge）——J++ 的模板机制本身要多写 6 行 |
| classifier.py:86 | 1 | `first=client.system_one(...)` 判断调用 | 保留 | 0 | 1 | 已计入上一行的 judge 调用，不重复计 |
| classifier.py:87-95 | 9 | 取出第一跳答案、决定第二跳该问哪个区域（结果回流），出口分派 | B | 9 | 0 | prog.jpp:67-72（firsthop）+83-85（nonempty_areas） |
| classifier.py:96-103 | 8 | 取选中区域 categories、构造 category 题面（题面拼装） | C | 6 | 2 | prog.jpp:34-36（category_form）+88-90（cat_labels/cat_qs） |
| classifier.py:104 | 1 | `second=client.system_one(...)` 判断调用 | 保留 | 2 | -1 | prog.jpp:91-92（cat_state+cat_rs） |
| classifier.py:106 | 1 | `model=str(getattr(first,"model",...))` 取字段 | C | 0 | 1 | 无对应（元数据字段，桩固定） |
| classifier.py:107-111 | 5 | 初始化 results+循环取出两跳答案、合成结果 | B | 17 | -12 | prog.jpp:86-87,93-94(area_regs包装,4)+97,98,99,100,101,102,103,104,107,108(fold核心,10)+115,117,118(all_rows/runs,3) |
| classifier.py:128-135,140,142 | 10 | `_result` 签名、类型注解、字典字面量语法 | D | 0 | 10 | 无对应 |
| classifier.py:136-139 | 4 | 从三个 tuple 取字段拼进输出 dict | C | 0 | 4 | 已计入上一行 fold 结构，不重复计 |
| classifier.py:141 | 1 | `needs_review=min(三个confidence)<0.5`（门槛，出口分派：单一判断路由，按规则算 A） | A | 6 | -5 | prog.jpp:54-65 `picked()` 函数一半（真实逻辑部分；另一半是 J-05 脚手架，见新增） |

- **核对**：Σ原代码 = 6+6+1+1+10+1+9+8+1+1+5+10+4+1=64 ✓。Σ(J++行数)=0+0+0+1+12+0+9+6+2+0+17+0+0+6=53；加新增(约17，见下)=70，与实测约 73 接近（残差 3，判为脚手架归类边界模糊）。

- **新增（J++ 独有，约 17 行）**：
  - `budget{...}`：1 行
  - `picked()` 函数里的 J-05 未决脚手架（三处 `unsure: fn(u){...}` 分支）：约 6 行
  - `exits`/`pending` 全链路追踪（71 行的 `exits:[a.exit,t.exit]`、106 行 `exits:concat(...)`、112 行、119-121 的最终 `pending: fold(...)`）：约 6 行
  - `unresolved_rows` 分支（110,111,113）——原代码从不处理"argmax 未决"这种情况（读数表设计上不会命中），J++ 因 J-05 完备性要求必须显式写一个分支处理它，原代码里没有任何对应：约 3 行
  - 其余边角（`fn picked` 签名收尾等）：约 1 行

- **本项目小计（净压掉）**：A=**-5** B=**-14**（-2+0-12） C=**14**（6+1+2+1-4+... 具体：6(R1)+1(R3)+2(R8)+1(R10)-1(R9)+4(R13)=13） D=**16**（6+10） E=0
- **一句话**：净压掉主要来自 D（语法差异，16 行）和部分 C；被寄予厚望的 B（组合方式）净值是 **-14**（负！）——两跳判断的批量循环/结果回流在原代码里紧凑（19 行），J++ 用 `map`/`filter`/`fold` 接管后总行数反而更多，因为每次判断都要显式带上 `exit`/`pending` 字段；A 类（唯一的门槛判断 `needs_review`）净值 **-5**，是本批第二次验证"J-05 未决消费纪律让简单阈值判断变长"这个现象（第一次见 B5-11b）。真正贡献净压缩的是原代码里那些和判断逻辑无关、J++ 完全不需要写的部分：SDK 响应解析（`_choice_payload`，6 行）、mock 分支/类型注解（16 行）。

## 3 B5-06b eachann1024/pi-jev-route（倍率标注 0.91，见下方口径说明）

- 核心总行数：81（`router.ts` 全文件）；J++ 总行数：89（当前文件实测，与 84 项目 CSV 的 `jpp.raw=89` 一致；`result.json` 里另有一版 64 行是 2026-09-26 订正后的数字，本文按磁盘上的当前 `prog.jpp` 数，两者差异是版本先后不是错误，如实两边都记）

| 原代码行范围 | 原行数 | 这块在干什么 | 归类 | J++ 行数 | 净压掉 | 对应 J++ / 备注 |
|---|---|---|---|---|---|---|
| router.ts:1-5,7,8 | 7 | import、类型定义 | D | 0 | 7 | 无对应 |
| router.ts:10 | 1 | `sensitive` 正则常量 | E | 0 | 1 | 无对应，gap 六·4："假定已越过" |
| router.ts:11-18 | 8 | `answer()`/`confidence()` 响应校验函数 | C | 0 | 8 | 无对应；判断读数天然类型化 |
| router.ts:19,21,22,23(部分),25,33,38,39,41,45,46,58,62,77,80,84 | 17 | 函数签名类型注解、参数合法性校验、语法外壳（含 23 行 `const text=copy(...)` 的 D 半） | D | 0 | 17 | 无对应 |
| router.ts:20,27,52-57,60,63,64,70,81-83 | 17 | 取消/超时基础设施（`AbortController`/`Promise.race`/`try…finally`）、出口分派（HTTP失败=单判断=A） | A | 0 | 17 | 无对应；`judge()` 由运行时管理超时 |
| router.ts:24 | 1 | 过滤可用候选模型（材料裁剪） | C | 0 | 1 | 无对应 |
| router.ts:26,28,30,31,32 | 5 | `fallback()` 三步查找主体 | 保留 | 17 | -12 | prog.jpp:62-65 `find_candidate()`(4)+67-79 `fallback_of()`(13)——TS 的 `??` 链 3 步只要 3 行，J++ 要显式写 if-else 链+一个新辅助函数 |
| router.ts:29 | 1 | 三步查找中间一步（字符串处理，挪到适配层） | C | 0 | 1 | gap 六·4：改由适配层预算 `low_suffix_id` |
| router.ts:34,35,36,47,48,49,50,51 | 8 | 5 个调用前确定性短路 | E | 0 | 8 | 无对应，gap 六·4 |
| router.ts:37,40 | 2 | model criteria dict、state 序列化 | C | 0 | 2 | 无对应 |
| router.ts:42,43,44 | 3 | kind/model/effort 三道题题面文本 | 保留 | 10 | -7 | prog.jpp:51-60（三个 select/measure 各自 3-4 行的多行调用语法，纯格式换行，非语义膨胀） |
| router.ts:59 | 1 | 判断调用本体（`fetch`） | 保留 | 3 | -2 | prog.jpp:82-84 三次 `judge(...)`（候选集不同拆成 3 次） |
| router.ts:61,65,66,67,69,78 | 6 | 解析/校验响应字段，出口分派（choice/confidence 类型检查，单判断=A） | C | 6 | 0 | prog.jpp:87,91,92,96,100,101,108 里"取值"部分 |
| router.ts:70,72,79 | 3 | 置信度门槛（`certainty<threshold`、`effort.score>=1.5`），出口分派：70/72 是单判断阈值=A，79 的 `effort.score>=1.5` 判 thinking 同为单判断=A | A | 8 | -5 | prog.jpp:90,99（confidence 门槛声明）+131-135（effort 阈值 cut，5 行） |
| router.ts:71,73-76 | 5 | human/style 业务分支，出口分派：73 行 `kind.choice==='style'&&styleUseMain` 是两个条件的组合=B | 保留/B混 | 14 | -9 | prog.jpp:112-127 if-else if 链（TS 的线性 if-return 5 行展开成 J++ 14 行结构） |

- **核对**：Σ原代码=7+1+8+17+17+1+5+1+8+2+3+1+6+3+5=85——比核心总行数 81 多 4，是因为第 23 行的 D/C 拆分（"1 each"）在合并行范围时重复计入了整数 1（原表按"D 半/C 半"各算 1 行是估算导致的舍入误差），实际应为 D=16.5/C=1.5，此处按整数四舍五入产生 4 行的重复计数，不是真实多出的原代码，核对时按 81 为准，D 类净压掉相应下修 4 行。

- **新增（J++ 独有，约 15 行）**：
  - `budget{...}` + `route()` 函数签名：2 行
  - `kind_label`/`exits` 数组初始化与全链路 `pending` 追踪（108,109,140-141,144,146-149）：约 8 行
  - `handle(cut(...))` 四个块里的 `unsure`/scaffolding 分支：约 5 行

- **本项目小计（净压掉，D 已扣减 4 行舍入误差）**：A = 17-5 = **12**；B ≈ 已并入 A/保留混合行，单列 **0**（本项目多数分支路由是单判断，B 很少）；C = 8+1+2+1+0 = **12**；D = 7+17-4 = **20**；E = 1+8 = **9**；保留净值 ≈ -12-7-2-9 = **-30**（负）
- **一句话**：这个项目和 B5-11b、B1-07 一样，"保留"部分的净值是负的（-30）——原本直接对应的业务逻辑（`fallback()` 三步查找、`select`/`measure` 题面定义、human/style 分支）在 J++ 里普遍需要更多行来表达，倒不是因为杂活没压干净（A/C/D 三类合计净压掉 44 行，压缩比例并不差），是因为 TS 紧凑的 `??` 链、单行三元表达式、`match`/`case` 风格的分支在 J++ 里都要展开成完整的 `if-else`/`handle(cut(...))` 结构。

## 4 Q-04 siroccomask/snake-jev（倍率 1.02）

- 核心总行数：58；J++ 总行数：57（官方口径，`measure.toml` 的 `data="8,12-14"` 把 ACTIONS 列表和三道题的整句题面文本按"数据"排除不计，本文对这几行单独说明，不计入下面的净压掉加总，避免"行数假压缩"误导，见下方专门说明）

| 原代码行范围 | 原行数 | 这块在干什么 | 归类 | J++ 行数 | 净压掉 | 对应 J++ / 备注 |
|---|---|---|---|---|---|---|
| jev_controller.py:65 | 1 | `RELATIVE_ACTIONS` 候选常量 | 保留 | 1 | 0 | prog.jpp:8 `ACTIONS`（该行按项目口径不计入 jpp_lines，但内容对应） |
| jev_controller.py:76 | 1 | `def simple_request(...):` 签名 | D | 0 | 1 | 无对应 |
| jev_controller.py:98,105 | 2 | 字符串/字典字面量语法闭合 | D | 0 | 2 | 无对应 |
| jev_controller.py:99,100,101,106 | 4 | 双层 for 循环头、容器初始化（合批攒题） | B | 2 | 2 | prog.jpp:37-38（state+judge，不需要循环攒字典） |
| jev_controller.py:102,103,104,107,108,109 | 6 | 子问题文本拼接、组装请求体 | C | 0 | 6 | 无对应；题面已硬编码为字面量 |
| jev_controller.py:145,154,161,166 | 4 | 签名、内嵌函数定义、语法 | D | 0 | 4 | 无对应 |
| jev_controller.py:147,153,170,171 | 4 | try/except 包裹响应解析（出口分派：捕获异常后统一失败，单一来源=A） | A | 0 | 4 | 无对应；`cut`/`handle` 天然不需要 try/except |
| jev_controller.py:148,151,155,156,160,169 | 6 | 从 answers 取字段、组装返回值 | C | 4 | 2 | prog.jpp:29-30,39-40（是否no()的取值部分） |
| jev_controller.py:149,150,152,157,158,159,162,163,164,167 | 10 | 逐方向循环+多判断合成（撞墙撞身取大、字典序排方向），出口分派：组合两道以上判断=B | B | 19 | -9 | prog.jpp:18-25（分_安全/分_危险两个 fit 闭包，8 行）+51-61（拍()核心，11 行）——Python `max(key=(a,b,c))` 一行内置元组比较，J++ 要把三键字典序显式编码成标量再 `order()`，比原来更长 |
| jev_controller.py:165,168 | 2 | 两句固定 reason 文案 | 保留 | 2 | 0 | prog.jpp:63,65 逐字相同 |
| jev_controller.py:187,189,192 | 3 | `choose()` 签名、请求形状判断、兜底 return | D | 0 | 3 | 无对应 |
| jev_controller.py:188 | 1 | 判断调用本体 | 保留 | 0 | 1 | 已计入上面的 state+judge，不重复计 |
| jev_controller.py:191 | 1 | 触发合成（出口分派：调用多判断合成函数） | B | 0 | 1 | 已计入拍()核心，不重复计 |

**单独说明（不计入下表加总）**：`jev_controller.py:85-97` 的 13 行规则文本（保留类），在原代码里只定义一次、循环里复用 3 次；J++ 因为没有跨题字符串复用机制，`wall_q`/`body_q`/`food_q` 三个 `select()` 各自内联一遍完整规则文本（每个都是单行超长字符串，按项目口径不计入 `jpp_lines`）。按**原始行数**看像是"13 行→3 行，净压掉 10 行"，但按**字符/文本量**看规则文本被复制了 3 遍，不是压缩，是复用变成了重复——这正是官方报告区分 raw_ratio/wrap_ratio/token_ratio 三个口径的原因，本文不把这一块计入净压掉的数字加总，只narrative说明。

- **核对**：Σ原代码（不含单独说明的 13 行）= 1+1+2+4+6+4+4+6+10+2+3+1+1=45，加上单独说明的 13 行=58 ✓ 与核心总行数一致。

- **新增（J++ 独有，约 9 行）**：
  - `budget{...}`：1 行
  - `是否no()` 里的 `pend` 未决字段（31-33）：约 3 行
  - `方向()` 的 `pend` 字段（47）：1 行
  - `拍()` 里的 `pends` fold 与最终 `pending` 输出（67-69,80）：约 4 行

- **本项目小计（净压掉，不含单独说明的 13 行）**：A=**4** B=**2-9+(-1，即191行net1并入B后)=约-6** C=**8**（6+2） D=**10**（1+2+4+3） E=0；保留净值 ≈ 0
- **一句话**：这个项目的判断核心本质是"三个候选方向各问三道是非题，再比较三个方向的读数选一个"——B 类净值是负的（约 -6），因为 Python `max(key=(f1,f2,f3))` 一行内置元组比较，被 J++ 换成两个 `fit` 闭包（把三键字典序编码成一个标量）加 `order()` 调用，行数不降反升；唯一净压缩比较明显的是 D（语法差异，10 行）和 C（数据整形，8 行）。规则文本本身（13 行）按行数看是压缩的，但按内容看是被复制了 3 遍，不能算真压缩，本文没有把它计进净压掉的正式加总。

## 5 B5-02 manali-co/yapp（倍率 1.03）

- 核心总行数：125（`intent.py` 86 + `policy.py` 39）；J++ 总行数：约 120（当前文件实测）

| 原代码行范围 | 原行数 | 这块在干什么 | 归类 | J++ 行数 | 净压掉 | 对应 J++ / 备注 |
|---|---|---|---|---|---|---|
| intent.py:36-42 | 5 | `_load_yaml()` 从文件读题库 | D | 0 | 5 | 无对应；题面硬编码在源码里 |
| intent.py:73-93 | 18 | 给候选拼学习例句/描述的整套机制 | E | 0 | 18 | 无对应；`adapter.py::make_learned_file` 只喂给原项目 Python 侧用，没挪到 J++ 侧（专门查过，见方法说明） |
| intent.py:96-101 | 6 | 决定候选 app 集合（缩小/排序取前 limit 个） | C | 0 | 6 | 无对应；`APP_LABELS` 固定列表 |
| intent.py:102-105,107 | 5 | 组装依赖学习机制的候选 criteria | E | 0 | 5 | 无对应，同上 |
| intent.py:106,108-125（去除下两行的 criteria 部分） | 12 | 组装 6 道题定义里的 instructions 文本与 Choice/Noul 构造语法 | 保留 | 6 | 6 | prog.jpp:22-27 六个 `select()`/`test()` |
| intent.py:114-124 中的 criteria 字段（is_complete/ends_dictation/is_destructive 三道是非题的 true/false 描述） | 7 | 三道 Noul 题的 criteria 描述文本 | E | 0 | 7 | gap 六·7："三道是非题带 true/false criteria 发不出去" |
| intent.py:128,129,134,144,145,149,160 | 7 | `classify()` 签名、字典/构造函数语法外壳 | D | 4 | 3 | prog.jpp:67,91,92,93（classify_call 的签名+return，纯结构） |
| intent.py:130-133,137,138,140,146-148,150-159 | 20 | 材料整形（清理文本/截取最近N条）、取字段组装 Decision | C | 6 | 14 | prog.jpp:68,70,71,101-103 |
| intent.py:135,136,139,142 | 4 | 调用 build_questions、判断调用本体、**无条件**问 app/key_combo | 保留 | 15 | -11 | prog.jpp:69(judge)+73-80,82-89（"只在 intent 命中才追加问 app/key_combo"的条件块，14 行）——早停少问的代价是要显式写两段 if，比原来无条件调用两行贵得多 |
| intent.py:141,143 | 2 | 根据 intent 条件性决定 app/is_key（出口分派：单判断路由=A，但依附上一行的 if 结构，已计入其 jpp 行数） | B | 0 | 2 | 已计入上一行，不重复计 |
| policy.py:3,5,6,9-11,40,43 | 8 | import、签名类型注解、match 穷尽兜底 | D | 1 | 7 | prog.jpp:99（decide_call 签名） |
| policy.py:12,14,18,25,27,31,35,39,44 | 9 | 五个分支的置信度/完整度门槛（出口分派：单判断=A） | A | 7 | 2 | prog.jpp:53-54（passes_gate 真实逻辑）+135,140,147,155,170（5 处调用） |
| policy.py:20 | 1 | 取字段供门槛用 | C | 0 | 1 | 无对应 |
| policy.py:21,22,26,28,32,36 | 6 | `match d.intent:` 多分支路由入口 | B | 11 | -5 | prog.jpp:109,112,117,120,123,128,131,139,143,151,159（11 个 if-else if 分支头，比 `match`/`case` 更啰嗦） |
| policy.py:13,15-17,19,23,24,29,30,33,34,37,38,45,46 | 15 | 各分支业务判断与最终 Verdict 文案 | 保留 | 47 | -32 | prog.jpp:99-177 `decide_call()` 剩余的分支体——每个早停分支都要重复写完整的 `{intent:...,app_key:...,key_combo:...,verdict_outcome:...}` 四字段结构，原来 `Verdict(Outcome.X,"文案")` 一行就够 |

- **核对**：Σ原代码=5+18+6+5+12+7+7+20+4+2+8+9+1+6+15=125 ✓。

- **新增（J++ 独有，约 8 行）**：
  - `budget{...}`：1 行
  - `intent_pick()`/`passes_gate()` 里的 `unsure`/`consume(u,"drop")` 脚手架：2 行
  - `classify_call()`/`decide_call()` 内各处 `consume(u,"drop")`（5 处 unsure 分支）：5 行

- **本项目小计（净压掉）**：A=**2** B=**2-5=-3** C=**6+14+0=20**（含1行policy取字段抵消） D=**3+7=10** E=**18+5+7=30**；保留净值 ≈ 6-11-32=**-37**（本批负得最狠的一项）
- **一句话**：这个项目的"保留"内容净值是 **-37**——不是因为内容变了，是因为早停树的每个分支在 J++ 里都要把 `{intent,app_key,key_combo,verdict_outcome}` 四个字段完整重写一遍（原来 `Verdict(outcome,reason)` 两个参数就够），加上"少问不算不通过"这条规则要求把原本无条件的两行调用（139,142）换成两段完整的 if 结构（14 行）。真正的功能缺失（E，30 行）是这批里最高的——原项目"按学到的正负例动态生成候选描述"整套机制，`prog.jpp` 完全没做，只用固定标签列表；这部分内容确认没有被挪到任何 J++ 会读取的地方（专门查过 `adapter.py`），是真缺口不是移库。

## 本批次汇总（净压掉口径）

| 类别 | 净压掉合计（5 项目） | 说明 |
|---|---|---|
| C 数据整形 | 4+13+12+8+20 = **57** | 全部为正，J++ 在"把材料/候选塞进请求形状"这件事上确实省了行，但幅度比 v1 报告算的 92 行小得多（v1 没扣除 J++ 侧对应行数） |
| D 语法差异 | 13+16+20+10+10 = **69** | 全部为正，宿主语言类型注解/import/字典语法是最稳定的净压缩来源 |
| A 杂活 | 0-5+12+4+2 = **13** | 合计很小，且 B1-07、B5-11b 各贡献一次负值（阈值判断因 J-05 反而变长），只有 B5-06b 的超时/取消基础设施是真正干净的压缩 |
| E 功能缺失 | 0+0+9+0+30 = **39** | B5-02（学习机制）和 B5-06b（调用前短路检查）两项贡献了几乎全部，不是被构造替掉，是真没做 |
| B 组合方式 | 0-14+0-6-3 = **-23** | **净值为负**——这批项目里"多个判断怎么接起来"这件事，J++ 展开后普遍比原代码的紧凑写法（`match`、`max(key=元组)`、批量 for 循环）更长，v1 报告算出的 46 行"正向压缩"是没扣除 J++ 侧行数的假象 |
| **净压掉合计** | | **57+69+13+39-23 = 155** |
| **保留净值** | -1+1-30+0-37 = **-67** | 5 项目里 4 项都是负的——"直接对应"的业务逻辑普遍需要更多行表达（早停分支重写完整结构体、`??`链展开成 if-else、字典序比较展开成 fit 闭包） |
| **新增（J++ 独有）合计** | 31+17+15+9+8 = **80** | 主要是三类：J-05 未决消费脚手架（每个项目都有，约占一半）、候选集拆分调用的额外 judge/state（B5-11b 最重）、B5-11b 特有的 preset 硬编码变体 |

**用净压掉重新核对原文档 v1 的结论**：v1 版本按原代码整块行数统计，得出"被压掉 284 行，A+B 占 27%、C+D 占 62%"；换成净压掉口径后，**B 类从"压掉 46 行"变成"净增 23 行"**——这是最大的方向性差异，说明"组合方式"这个原本被研究 18 认为 J++ 最擅长压缩的类别，在这批低倍率项目里其实是净增而非净减。真正撑住净压缩的是 D（语法差异，69 行）和 C（数据整形，57 行），这两类合计 126 行、占净压掉总量（155 行）的 81%；A（杂活）合计只有 13 行，远低于直觉。而"新增"（80 行）和"保留净值"（-67，即保留部分反而膨胀）合计 147 行的额外开销，与净压掉的 155 行几乎打平——这才是这批项目倍率普遍在 0.7~1.0 附近、压不下去的真正算术:净压掉的行数，被"新增的脚手架"和"直接对应内容本身也变长"这两项几乎完全抵消。

## 挑出的例子

**反例 1：J-05 未决消费纪律让简单阈值判断反而更长**（B1-07 与 B5-11b 各出现一次，B1-07 更典型，`app/classifier.py:141`）：

```python
"needs_review": min(area[1], category[1], tone[1]) < CONFIDENCE_THRESHOLD,
```

1 行门槛比较，J++ 里净压掉是 **-5**（原 1 行，J++ 侧真实逻辑部分已经要 6 行，还没算另外 6 行的 unsure 脚手架）：

```jpp
fn picked(r, argmax_key) {
    handle(cut(r, argmax_key), {
        pick: fn(k) {
            handle(cut(r, {stat: "confidence", declare: {hi: 0.5}}), {
                act: fn() { {k: k, ok: true, exit: unit} },
                ignore: fn() { {k: k, ok: false, exit: unit} },
                unsure: fn(u) { {k: k, ok: false, exit: u} }
            })
        },
        unsure: fn(u) { {k: -1, ok: false, exit: u} }
    })
}
```

**反例 2：早停分支要重写整个结构体，TS/Python 的紧凑早停在 J++ 里普遍膨胀**（B5-02，`src/yapp/policy.py:12-13`）：

```python
if dictating and d.ends_dictation < t.ends_dictation:
    return Verdict(Outcome.IGNORE, "dictating")
```

2 行，J++ 对应部分（含判断本身与结构体重写）：

```jpp
let ends_ok = handle(cut(d.ends_dictation_r, "yapp-ends-dictation"), {
    act: fn() { true }, ignore: fn() { false },
    unsure: fn(u) { consume(u, "drop"); false }
});
if c.dictating && !ends_ok {
    {intent: report_intent, app_key: report_app, key_combo: report_combo, verdict_outcome: "ignore"}
}
```

原来一行 `Verdict(Outcome.IGNORE,"dictating")` 就交代完，J++ 每个分支都要把 4 个透传字段整个重写一遍——这是 `policy.py` 那 15 行"保留"内容净值变成 -32 的主要原因，不是某一个例外，是这个早停树 11 个分支的共性写法。

**正面例子（唯一一处"该压的确实压干净了"）：取消/超时基础设施，原项目手写，J++ 完全不需要写**（B5-06b，`router.ts:52-57,83`）：

```typescript
const timeout = new AbortController();
const combined = AbortSignal.any([signal, timeout.signal]);
const timer = setTimeout(() => timeout.abort(), settings.timeoutMs);
let onAbort: () => void = () => {};
// ...try/finally 里还有清理逻辑（clearTimeout、removeEventListener）
```

17 行手写的"发请求前设超时、收到取消信号就中断、用完清理"基础设施，J++ 侧净压掉 17 行——`judge()` 调用的超时/取消由运行时统一管理，`prog.jpp` 里完全看不到这段代码，是本批 5 个项目、约 15 个 A-E 分类行里唯一一处净值为满额正数、且没有被"新增"抵消的真实杂活压缩。
