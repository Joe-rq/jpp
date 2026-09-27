# 人工对照：中倍率组（5 项目）

统计口径说明（团队负责人 2026-09-27 补充修正后统一采用）：每个逐块分类表格给出「原行数」「J++行数」两列，**净压掉行数 = 原行数 − J++行数**；本项目小计、批次汇总都按净压掉行数算，不按原代码整块行数算。归类 A–E 之外新增「挪走」——内容没被判断构造替代、也没被砍掉，只是搬到了 prog.jpp 之外的文件（fixture/adapter/画像/CLI 参数）；「保留」——原始判定文字本体，只换了容器语法，不计入 A–E 压掉分子，只作分母参照。出口分派（if/elif 按判断结果分流）：单一判断自己的结果决定自己走哪支归 **A**（对应 `cut`/`handle` 三值出口）；两个以上判断结果组合分流才归 **B**（多判断合成/级联），备注栏标"出口分派"。

---

## B6-03 Sahilll15/jobfit（折行倍率 2.64）

- 核心总行数：66（route.ts 56 + lib.ts 10）；J++ 对应部分行数：23（另有 2 行 J++ 侧新增、原代码无对应：`budget{...}` 声明 1 行、`pending: fold(...)` 未决兜底 1 行，合计 prog.jpp raw 25 行）

| 原代码行范围 | 原行数 | J++行数 | 净压掉 | 这块在干什么 | 归类 | 对应 J++ |
|---|---|---|---|---|---|---|
| route.ts:16 | 1 | 1 | 0 | `scoreLine` 函数签名 | D | prog.jpp:19 `fn score_line(it) !{judge} {` |
| route.ts:17-19 | 3 | 2 | 1 | 拆 answers/usage、写死 model 名、把 line/cv 拼进 state | C | prog.jpp:20-21 `let st=state(mat(...)); let rs=judge(st,[...]);` |
| route.ts:20-23,28-32,38-41 | 19 | 4 | 15* | 四道题 instructions/criteria 文本本体 | 保留 | prog.jpp:14-17 四个 `test()`/`measure()` 声明 |
| route.ts:24-27 | 4 | 0 | 4 | isRequirement 的 true/false 语义描述 | **E**（查证：fixture.json 无 criteria 字段，彻底没了） | 无对应 |
| route.ts:43 | 1 | 0 | 1 | `evaluate()` 调用收尾括号 | D | 无对应 |
| route.ts:45-54 | 10 | 2 | 8 | 把 answers 字段搬进 Requirement 返回对象 | C | prog.jpp:22-23 `let r_is_req=rs[0]; let r_met=rs[1];` |
| route.ts:55 | 1 | 1 | 0 | `scoreLine` 收尾括号 | D | prog.jpp:39 `}` |
| route.ts:74 | 1 | 0 | — | 调 `splitRequirements` 拆行 | **挪走**（原样搬到 adapter.mjs:70，prog.jpp 不调用） | 无 |
| route.ts:75-77 | 3 | 0 | 3 | 拆出 0 条需求提前报错 | E | 无对应 |
| route.ts:79 | 1 | 0 | 1 | try 包裹判断调用 | A（4 重试与失败） | 无对应 |
| route.ts:80 | 1 | 1 | 0 | `Promise.all(lines.map(scoreLine))` | B（1 调用循环与合批） | prog.jpp:41 `map(input.lines, fn(it) !{judge} {...})` |
| route.ts:81 | 1 | 6 | **−5** | `.filter(isRequirement>=0.6)` 门槛过滤 | A（2 门槛，出口分派：单次判断 2/3 路分流） | prog.jpp:27,28,35-38 `handle(cut(r_is_req,"is-req"),{act/ignore/unsure})` |
| route.ts:83-85 | 3 | 0 | 3 | 过滤后需求为空再报错 | E | 无对应 |
| route.ts:88-91 | 4 | 0 | 4 | weight() + fit 加权综合分 | **E**（查证：map.json/fixture.json 都无 fit 公式，彻底没了） | 无对应 |
| route.ts:93 | 1 | 0 | 1 | 汇总 inputTokens | E | 无对应 |
| route.ts:95-96 | 2 | 1 | 1 | 拼响应体 + 两级排序 | E（排序未实现） | prog.jpp:43 只覆盖列表本体，不覆盖排序 |
| lib.ts:16-23 | 7 | 5 | 2 | MEETS(0.7)/MISSING(0.3) + bucket() 三分支 | A（2 门槛，出口分派：单次判断 3 路分流） | prog.jpp:30-34 `handle(cut(r_met,"met-bucket"),{act/ignore/unsure})` |
| lib.ts:33-35 | 3 | 0 | 3 | coverage()（`min(strength/2,1)`） | **E**（查证：无处引用，彻底没了） | 无对应 |

\* 保留类净行数不计入 A–E 压掉分子，只是同一段文字换了更紧凑容器。

- 核对：Σ原行数=66=核心总行数；Σ对应J++行数=23，加上 2 行新增开销=25=prog.jpp raw。
- 本项目小计（净压掉）：**A=−2　B=0　C=9　D=1　E=19**，合计净压掉 **27**；保留净 15；挪走 1。27+15+1=43=66−23，核对一致。
- 一句话：净压掉的 27 行里 19 行（七成）是 E——选题时就裁定不比的 fit 综合分、两级排序、coverage()、isRequirement criteria，都是原项目有而 J++ 版本彻底没实现的功能，不是被压缩掉的；A 类净值反而是 **负 2**——把一道判断的过滤条件从一行 `.filter(...)` 展开成显式 act/ignore/unsure 三分支，比原代码多写了行。

---

## Q-07 yodablocks/jobbyjev（折行倍率 2.79）

- 核心总行数（原始口径）：173（questions.py 153 + rank.py 20，选题说明记 174，差 1 行文件尾计数）；J++ 官方计入行数：52（result.json raw_ratio 2.79）；本表能对应到具体原代码块的 J++ 行数 53，另有约 18 行是 J++ 侧新增开销（未决记账、分档弥补、结果组装）。

| 原代码行范围 | 原行数 | J++行数 | 这块在干什么 | 归类 | 对应 J++ |
|---|---|---|---|---|---|
| questions.py:1-17 | 17 | 0 | docstring、`from __future__ import annotations`、空行 | D | 无对应 |
| questions.py:18-20 | 3 | 0 | STATE_COMPANY_FIELDS 常量 | **挪走** | 挪进 adapter.py，prog.jpp 不含这段投影逻辑 |
| questions.py:21-104 | 84 | 37 | QUESTIONS 六道题的 type/instructions/criteria | 保留 | prog.jpp:11-51 四张常量表 + 六次 measure/test/select |
| questions.py:105-107 | 3 | 0 | SCORE_IDS/SCORE_TOP 基础设施 | B | 无对应（J++ 用具名闭包参数直接引用） |
| questions.py:108-113 | 6 | 0 | WEIGHTS/INTERVIEW_WEIGHT/LIKELY_THRESHOLD/MISMATCH_MIN_CONF 四个门槛常量 | A | 数值内嵌进 prog.jpp:58,68,82 闭包/cut declare，不再单独占行 |
| questions.py:114-124 | 11 | 0 | company_state()/build_state() 状态投影 | **挪走** | 执行搬到 adapter.py |
| questions.py:125-126 | 2 | 0 | compose() 函数签名+docstring | D | 无对应 |
| questions.py:127-134 | 8 | 0 | signals 循环：三道打分题摊平进 dict | C | 无对应（闭包直接读 rs[i].expect） |
| questions.py:135,138-139 | 3 | 4 | fit=Σweight·value；chance=0.5·interview+0.5·fit；chance*=location | B | prog.jpp:57-59 声明式拟合闭包 + 65 `fit({declare:匹配},…)` |
| questions.py:136-137 | 2 | 0 | interview/location 字段搬运 | C | 无对应（闭包内直接用 .p） |
| questions.py:140 | 1 | 0 | mm=answers["mismatch"] 字段搬运 | C | 无对应（直接用 rs[5]） |
| questions.py:141 | 1 | 5 | mismatch=choice if(标签选中且置信度过门) else None——两判断结果组合 | **B（出口分派：两判断合成）** | prog.jpp:79,80,82,83,85 pick_e/pick/conf_e/conf_ok/mismatch |
| questions.py:142,144-145,149,151 | 5 | 0 | 综合 confidence 计算 + chance/confidence/fit/mismatch_probs 数值输出 | **E**（B153：合成分读不出数，程序值层面永久拿不到） | prog.jpp:74-76 chance_bucket 三档分档部分弥补，非等价 |
| questions.py:143,153 | 2 | 0 | return {}/{} 字典包裹符 | D | 无对应 |
| questions.py:146 | 1 | 2 | likely_interview = chance>=0.70 | A | prog.jpp:68-69 likely_e/likely |
| questions.py:147-148,152 | 3 | 0 | would_interview/location_ok/signals 三个展示回显字段 | C | 无对应（验收字段已剔除） |
| questions.py:150 | 1 | 0 | "mismatch": mismatch 塞进返回记录 | C | 并入 prog.jpp:90 record literal |
| rank.py:79 | 1 | 1 | def score_company 函数签名 | D | prog.jpp:62 |
| rank.py:80 | 1 | 2 | client.ask(...) 调判断接口 | 保留 | prog.jpp:63-64 state()+judge() |
| rank.py:81 | 1 | 0 | compose(response["answers"]) 调用 | D | 无对应 |
| rank.py:82-95 | 14 | 0 | 返回记录：公司元数据回显+**composed 展开+model/input_tokens | C | 无对应（元数据与用量已剔除） |
| rank.py:141-143 | 3 | 2 | results.sort(key=(-chance,-confidence,name))+赋rank | B | prog.jpp:97,101 order()；二级键打不破平局，已知不等价 |

- 核对：Σ表格原行数=173≈174；Σ表格J++对应行数=53，加约18行新增开销≈71 raw，与官方 `measure_expr.code_lines` 52 口径不同（统计方法差异，非矛盾）。
- 本项目小计（净压掉）：**A=5　B=−1　C=29　D=22　E=5**，合计净压掉 **60**；保留净 46（原85,J++39）；挪走 14。7+10+29+23+5=74(A-E原始分)+85+14=173 ✓。
- 一句话：被压掉的行主要是 Python 语法/展示层开销（D 类 22 行：docstring/import/dict 包裹符/函数签名）和结果字段搬运/元数据回显（C 类 29 行）；B 类（多判断合成）净值几乎为 0 甚至为负——声明式拟合闭包与三值出口都要求显式声明，不总是省行；另有 5 行是货真价实的功能缺失（合成分读不出数）。

---

## B3-08 joaovaleri/ruleworld（折行倍率 2.81）

- 核心总行数：76（judge.ts 70 + physics.ts:52-57 共 6）；J++ 对应部分行数：16；prog.jpp 实际 raw 总行数：27（差额 11 行是 `budget` 声明、`{calib:"obj-cat"}` 绑定、函数签名本身、`judge()` 调用行、6 条规则外层 map、输出打包——J++ 侧新增脚手架，原代码无一一对应）。

| 原代码行范围 | 原行数 | J++行数 | 这块在干什么 | 归类 | 对应 J++ |
|---|---|---|---|---|---|
| judge.ts:1 | 1 | 0 | import 类型声明 | D | 无对应 |
| judge.ts:2-3 | 2 | 0 | makeRequest 函数签名+开括号 | D | 无对应 |
| judge.ts:4 | 1 | 0 | `model:"jev-1.13.0"` 写死调用哪个模型 | **挪走** | 移到真机运行的 `--profile` 画像文件，不在 prog.jpp 源码内 |
| judge.ts:5-12 | 8 | 1 | state.category+OBJECTS.map 取字段拼进 state | C | prog.jpp:32 `let st=state(mat(it.state));` |
| judge.ts:13-16,24-26 | 7 | 1 | questions 字典外壳，按 id 建 24 项 | C | prog.jpp:33 `map(range(0,24), fn(i){fill(obj_form,{i:i})})` |
| judge.ts:17-18 | 2 | 2 | 题型+instructions 模板文本 | **保留** | prog.jpp:15-16 form("test", "Does the physical object …") |
| judge.ts:19-23 | 5 | 0 | criteria:{true,false} 是非题候选说明文字 | **E** | 无对应——发不出去（已知缺口） |
| judge.ts:27-28 | 2 | 0 | 函数收尾括号 | D | 无对应 |
| judge.ts:29-33 | 5 | 0 | parseAnswer 函数签名+类型标注 | D | 无对应 |
| judge.ts:34-38 | 5 | 0 | 响应体类型断言声明 | D | 无对应 |
| judge.ts:39-40 | 2 | 0 | 响应体完整性校验，不通过整体抛错 | A（4 重试与失败） | 无对应——judge() 保证返回结构化答案 |
| judge.ts:41-53 | 13 | 7 | 逐物体解析概率、校验类型/范围不过抛错 | A（4 重试与失败＋出口分派：单个物体自身读数决定有效性） | prog.jpp:23-29 obj_flag：cut+handle 三态出口 |
| judge.ts:54 | 1 | 0 | 收尾 `});` | D | 无对应 |
| judge.ts:55-59 | 5 | 0 | inputTokens 用量统计 | A（6 费用与调用计数） | 无对应——账本/budget 接管计费 |
| judge.ts:60-63 | 4 | 0 | version/rule/decisions 三字段直接透传 | C | 无对应 |
| judge.ts:64-67 | 4 | 0 | model/latencyMs/inputTokens/costUsd 四个调用元数据与花费字段 | A（6 费用与调用计数） | 无对应——result.json 明确排除比较的调用元数据 |
| judge.ts:68-70 | 3 | 0 | source:"live" 字面量+收尾括号 | D | 无对应 |
| physics.ts:52 | 1 | 0 | `if(!rule) return bases;` 空规则兜底 | D | 无对应 |
| physics.ts:53-57 | 5 | 5 | `OBJECTS.filter(p>=0.65)` 按每个物体自己的概率与门槛比较 | A（2 门槛＋出口分派：单一判断结果决定选中/不选中） | prog.jpp:35-39 map(rs,obj_flag)→selected→fold |

- 核对：Σ原行数=76=核心总行数；Σ对应J++行数=16，prog.jpp raw 27，差 11 行为语言侧脚手架（不算某块的压掉）。
- 本项目小计（净压掉）：**A=17　B=0　C=17　D=20　E=5**，合计净压掉 **59**（分母 75，即 76 减挪走 1 行）≈78.7%；保留净 0；挪走 1。
- 一句话：被压掉的行主要是响应校验+费用/用量记账（judge.ts 近半行在校验 API 返回结构合法、统计 token/latency/cost，被 judge() 的结构化保证和账本/budget 整体接管）和纯 TS 语法样板（函数签名、类型断言、收尾括号）；真正的判断语义（题面文本、门槛比较）几乎原样保留或只换了更短容器，净压掉很少甚至为 0。

---

## B6-06 diluteoxygen/JevMood（折行倍率 2.95）

材料说明：仓库许可证口径下 `original/` 不放代码副本，核心代码引用 `measure.toml` 登记的真实源码路径 `实测/jev-ecosystem-2026-09-23/sources/diluteoxygen__JevMood/api/mood.js`（`[[core]] ranges="11-25,27-32,104-152"`），手工数出核心 59 行，与 result.json 一致。

- 核心总行数：59；J++ 对应部分行数：35（另有 5 行是 `budget` 声明+顶层测试用例外壳，J++侧新增、原代码无对应，不计入任何一格）；官方 `jpp_lines`（排除数据行）：20。

| 原代码行范围 | 原行数 | J++行数 | 净压掉 | 这块在干什么 | 归类 | 对应 J++ |
|---|---|---|---|---|---|---|
| mood.js:11-25 | 15 | 15 | 0 | SOUNDS 常量：13 条声道 id/name/label | 保留 | prog.jpp:18-32 |
| mood.js:27-32 | 6 | 5 | 1 | CRITERIA 常量：4 档打分题面文本 | 保留 | prog.jpp:11-15 |
| mood.js:104 | 1 | 0 | 1 | try{ 错误包裹开口 | A | 无对应 |
| mood.js:105 | 1 | 0 | 1 | questions={} 累加器初始化 | D | 无对应 |
| mood.js:106,112 | 2 | 2 | 0 | for 循环开口/收尾，遍历 13 声道构造题面 | B | prog.jpp:40,46 map() 开口/收尾 |
| mood.js:107-111 | 5 | 4 | 1 | 每条声道拼 instructions 模板串+挂 criteria | C | prog.jpp:41-44 |
| mood.js:114-118 | 5 | 3 | 2 | 13 道题打包，一次性 postToTypeSafe 调用（手写合批） | B | prog.jpp:39,45,47 函数外壳+judge()，靠材料哈希自动合批 |
| mood.js:120 | 1 | 0 | 1 | data.answers||{} 防御性默认 | A | 无对应 |
| mood.js:121-122 | 2 | 0 | 2 | soundscape/activeSounds 累加器初始化 | D | 无对应 |
| mood.js:124,149 | 2 | 1 | 1 | 逐声道循环开口/收尾 | B | prog.jpp:57 内 map(rs,...) |
| mood.js:125-126 | 2 | 0 | 2 | answers[id]||默认值 + typeof 类型收窄 | A | 无对应 |
| mood.js:131,132,135 | 3 | 5 | −2 | 门槛声明+if(rawScore>=1.0){...} | A（出口分派：单一判断结果分流） | prog.jpp:49-53 band_of：cut声明线+act/ignore/unsure |
| mood.js:133-134 | 2 | 0 | 2 | 音量线性缩放（连续数） | E（已知不等价） | 无对应 |
| mood.js:137,144 | 2 | 0 | 2 | soundscape[id]={} 对象开口/收尾 | D | 无对应 |
| mood.js:138 | 1 | 0 | 1 | id 字段搬运 | C | 已计入 prog.jpp:57 x.id |
| mood.js:139-140 | 2 | 0 | 2 | name/label 展示字段搬运（J++输出不出现） | C | 无对应 |
| mood.js:141 | 1 | 0 | 1 | volume 字段赋值 | E（同上缺口下游） | 无对应 |
| mood.js:142 | 1 | 0 | 1 | score:rawScore 原始读数展示字段 | C | 无对应 |
| mood.js:143 | 1 | 0 | 1 | confidence:ans.confidence||0 展示字段+防御默认 | A | 无对应 |
| mood.js:146-148,152 | 4 | 0 | 4 | 过滤活跃声道+按volume降序排序 | E（已知不等价） | 无对应 |

- 核对：Σ原行数=59=核心总行数；Σ对应J++行数=35，加5行新增外壳=40=prog.jpp排除注释空行后的物理行数；40再减SOUNDS/CRITERIA数据行20=20，等于result.json官方jpp_lines。
- 本项目小计（净压掉）：**A=3　B=3　C=5　D=5　E=7**，合计净压掉 **24**（=59−35）；保留净 1。
- 未发现"挪走"实例。
- **核实 64.4%**：脚本估的核心杂活占比用的是**原始行数**口径、按研究18旧的12类关键词（几乎命中除两段字面常量外的所有代码）。用同一原始行数口径重算 (A+B+C+D+E原始行数)/(59−21数据行)=38/59=**64.4%**，与脚本数字精确重合，**站得住**——但前提是把"组合方式"(B：循环+手写合批)、"数据整形"(C：题面拼装+字段搬运)都并入"杂活"。若按本文更细的 A 单独口径（只算错误包裹/防御默认/单判断门槛这类最窄杂活）：净压掉口径 A=3/24=12.5%，原始行数口径 A=8/59=13.6%，与"占大部分开发量"的直觉相差甚远。JevMood 恰好是把"高杂活占比往往是把 B/C 也算进去的产物"体现得最干净的案例。
- 一句话：被压掉的最大头不是最窄意义的杂活，而是组合方式（手写合批+循环）与数据整形（题面拼装+字段搬运），外加一块已知不等价、根本没法压的连续值（音量缩放/排序）；最窄杂活（错误包裹、防御默认、门槛判断本身）占比很小，其中门槛判断这块 J++ 反而比原代码更长（显式枚举三态出口）。

---

## B3-09 socai-io/jev-social（折行倍率 2.96）

- 核心总行数（measure.toml 登记）：145；本表逐块累计原行数：142（差 3 行函数收尾括号，未单列，并入相邻块）。J++ 行数（prog.jpp 全文本口径）：49；能对应到具体原代码块的：23（其余约26行对应原项目登记在核心范围之外的代码，如 app.js:151-157 run 对象构造/落盘，不在本项目 [[core]] 三段内）。

| 原代码行范围 | 原行数 | J++行数 | 这块在干什么 | 归类 | 对应 J++ |
|---|---|---|---|---|---|
| classifier.js:10-18 | 9 | 1 | classifySearch 函数签名与参数默认值 | D | prog.jpp:22 fn route_platform |
| classifier.js:19-24 | 6 | 0 | 校验平台是否受支持，不支持抛错 | E | 无对应 |
| classifier.js:25-27 | 3 | 0 | 校验 goal 非空，为空抛错 | E | 无对应 |
| classifier.js:29-39 | 11 | 1 | 拼 model+state{request,requested_platform,...} | C | prog.jpp:24 state(mat(...),{over:candidates}) |
| classifier.js:40-59 | 20 | 2 | 拼 questions.route 题面（任务+3条规则+4条criteria） | C | prog.jpp:11,14 ROUTE_TEXT常量+select() |
| classifier.js:61 | 1 | 1 | 真正发出判断请求 | 保留（残余） | prog.jpp:25 judge(st,route_q) |
| classifier.js:62-63 | 2 | 1 | selected映射platform | 保留/D | prog.jpp:28 pick分支 |
| classifier.js:64-72 | 9 | 0 | 出口分派：路由结果与用户指定平台不一致抛错中止 | A | 无对应 |
| classifier.js:73 | 1 | 0 | 结果对象拼装 | D | 并入pick分支返回值 |
| actions.js:156 | 1 | 1 | chooseAction 函数签名 | D | prog.jpp:37 fn choose_hop |
| actions.js:157 | 1 | 0 | 无候选抛错 | E | 无对应 |
| actions.js:158-171 | 14 | 1 | 拼state{...含.slice(0,400)材料裁剪} | C | prog.jpp:39 state() |
| actions.js:172-190 | 19 | 2 | 拼questions.action题面（任务+8条规则+criteria） | C | prog.jpp:12,15 ACTION_TEXT常量+select() |
| actions.js:192 | 1 | 1 | 真正发出判断请求 | 保留（残余） | prog.jpp:40 judge() |
| actions.js:193 | 1 | 5 | 出口分派+门槛：confidence<0.35抛错 | A | prog.jpp:49-53 handle(cut(r,{stat:"confidence"...}))——J++反而更长（语义忠实修正后所致） |
| actions.js:194 | 1 | 2 | 按id找回动作对象 | D | prog.jpp:43 |
| app.js:22 | 1 | 0 | 计时起点 | A | 无对应 |
| app.js:23-24 | 2 | 0 | model/options打包 | D | 无对应 |
| app.js:25 | 1 | 0 | 进度回调 | A | 无对应 |
| app.js:26 | 1 | 1 | 真正发出路由判断 | 保留 | prog.jpp:60 |
| app.js:27 | 1 | 0 | 路由为空抛错 | A | 无对应（未覆盖，已记） |
| app.js:28 | 1 | 0 | 路由置信度<0.35抛错 | A | 无对应（route-pick 无置信门，已知缺口） |
| app.js:29 | 1 | 0 | 取值 | D | 并入上面 |
| app.js:71 | 1 | 1 | 主循环：逐跳编排调用 | B | prog.jpp:61 map(sc.hops,choose_hop) |
| app.js:72 | 1 | 0 | 中断检查 | A | 无对应 |
| app.js:73 | 1 | 0 | 取候选集（真正构造逻辑在host_shared） | **挪走** | 候选集改由测试脚手架预先算好塞进jpp_input.json |
| app.js:74-75 | 2 | 0 | 进度回调+声明 | A/D | 无对应 |
| app.js:76-83 | 8 | 0 | try{...}catch{decision_failed;break} | A | 无对应（未覆盖，已记） |
| app.js:84 | 1 | 0 | 取值 | D | 并入choose_hop |
| app.js:85-91 | 7 | 0 | 拼entry元数据+状态累积 | C/D | 无对应（model/usage/elapsedMs已剔除） |
| app.js:92 | 1 | 2 | 出口分派：kind==finish决定是否跳出循环 | A | prog.jpp:66-67 |
| app.js:93-98 | 6 | 0 | 收尾判定completed/partial+checkpoint+break | A | 无对应（partial未覆盖，已记） |
| app.js:146-149 | 4 | 0 | 循环耗尽未finish→step_limit | A | 并入92行else分支 |

- 核对：Σ原行数=142，measure.toml登记145（差3行函数收尾括号）；Σ对应J++行数=23，prog.jpp raw 49，差26行对应核心范围外的代码。
- 本项目小计（净压掉）：**A=28　B=0　C=63　D=16　E=10**，合计净压掉 **117**；保留净1；挪走净1。28+0+63+16+10=117；117+1+1=119=142−23。
- 一句话：被压掉的行主要是 C（题面/state 拼装，63 行，占一半以上）和 A（出口分派类杂活，28 行）；唯一一处该涨行数的地方（actions.js:193 置信度门槛，从1行原始代码变成5行显式 `cut(...,{stat:"confidence"})`）恰恰是 J++ 反而写得更长——因为原代码读的是答案自带的 confidence 字段而非 argmax 概率，语义忠实修正后行数不降反升。

---

## 本批次汇总

各项目净压掉行数（A–E）：

| 项目 | A 杂活 | B 组合方式 | C 数据整形 | D 语法差异 | E 功能缺失 | 净压掉合计 | 保留(净) | 挪走 |
|---|---|---|---|---|---|---|---|---|
| B6-03 jobfit | −2 | 0 | 9 | 1 | 19 | 27 | 15 | 1 |
| Q-07 jobbyjev | 5 | −1 | 29 | 22 | 5 | 60 | 46 | 14 |
| B3-08 ruleworld | 17 | 0 | 17 | 20 | 5 | 59 | 0 | 1 |
| B6-06 JevMood | 3 | 3 | 5 | 5 | 7 | 24 | 1 | 0 |
| B3-09 jev-social | 28 | 0 | 63 | 16 | 10 | 117 | 1 | 1 |
| **合计** | **51** | **2** | **123** | **64** | **46** | **286** | 63 | 17 |

| 类别 | 合计净压掉行数 | 占被压掉行数的比例 |
|---|---|---|
| A 杂活 | 51 | 17.8% |
| B 组合方式 | 2 | 0.7% |
| C 数据整形 | 123 | 43.0% |
| D 语法差异 | 64 | 22.4% |
| E 功能缺失 | 46 | 16.1% |

这批 5 个项目（折行倍率 2.64–2.96，处在 84 项目倍率分布的中位数附近）净压掉的行里，**数据整形（C，43.0%）和语法差异（D，22.4%）合计占近三分之二**——大头是"拼题面文本/state 结构""字段搬运/展示回显"这类跟宿主语言啰嗦程度直接相关的代码，不是研究 18/19 定义的最窄意义"杂活"（A，只占 17.8%）。功能缺失（E，16.1%）也不小——不少净压掉行其实是原项目有、J++ 版本目前做不到或明确不比较的部分（跨题综合分、二级排序键、连续值输出），不是被语言优雅地替掉，是真的做不到。组合方式（B）净值几乎为零（0.7%），且两个项目（Q-07、B6-03）在这一类上是负数——把隐式的一行内置方法（`.filter`、加权算术）换成显式的 `cut`/`handle`/`fit` 结构，有时反而多写行；这与批次里唯一一处"语义修正后行数不降反升"的例子（B3-09 的置信度门槛）说明同一件事：J++ 压行不是无条件的，语义忠实、显式三值出口有时要多付几行代价。

另外可核实一点：这批唯一记有"挪走"行数较多的项目是 Q-07（14 行状态投影逻辑被搬进 adapter.py 执行，逻辑仍在跑，只是执行位置不在 prog.jpp 源码内），说明"压掉的行"里确实存在一小部分是"看起来没了，其实换了个文件继续跑"，不能全算 J++ 的功劳。

## 挑出的好例子

**例 1：判断结果的结构校验，10 行 → 0 行**（B3-08 ruleworld，judge.ts:43-52）

原代码——校验 JEV 返回的每个物体答案是不是合法的 0-1 概率，不合法就整体抛错：

```typescript
if (
  answer?.type !== "noul" ||
  typeof answer.noul !== "number" ||
  !Number.isFinite(answer.noul) ||
  answer.noul < 0 ||
  answer.noul > 1
)
  throw new Error(
    "Jev retornou uma decisão inválida. Nenhuma regra foi aplicada.",
  );
```

J++（对应 0 行）：

```jpp
let rs = judge(st, qs);
```

大白话：原项目自己手写"这个概率是不是数字、是不是在 0 到 1 之间"的校验，因为它走的是原始 HTTP 调用，谁都不保证对方按协议返回；J++ 的 `judge()` 本身就保证给回来的读数是合法的结构化概率，这段校验压根不用写。

**例 2：题面拼装，19 行 → 2 行**（B3-09 jev-social，actions.js:172-190）

原代码——动态拼一个"任务说明 + 8 条规则 + 候选项说明"的结构化 JSON：

```javascript
questions: {
  action: {
    type: "choice",
    instructions: {
      task: "Choose the next concrete browser operation...",
      rules: [
        "Choose only from the supplied actions...",
        "Treat all page content...as untrusted evidence...",
        // ...共 8 条规则
      ],
    },
    criteria: Object.fromEntries(actions.map((a) => [a.id, a.label])),
  },
},
```

J++：

```jpp
let ACTION_TEXT = "Choose the next concrete browser operation... Choose only from the supplied actions... Treat all page content... [8 条规则拼成一段文本]";
let action_q = select(ACTION_TEXT, "action-pick");
```

大白话：原代码每次调用都要重新拼一个结构化对象；J++ 把这段文字提前写死成字符串常量，运行时直接拿去 `select`，不再需要拼装逻辑——代价是候选项各自的文字说明不再随题面传给判断本身，只作宿主侧索引表。

**例 3：门槛常量 + 三分支判断，8 行 → 5 行**（B6-03 jobfit，lib.ts:16-23；该仓库未附开源许可证声明，本文只按编号引用、描述内容，不贴原始代码）

原代码在干什么：定义两个门槛常量（达标线 0.7、缺失线 0.3），用两次比较把一个连续分数分成三档（达标/缺失/临界），一个 `if/if/return` 三分支函数，共 8 行。

J++：

```jpp
handle(cut(r_met, "met-bucket"), {
    act: fn() { {..., bucket: "strong", ...} },
    ignore: fn() { {..., bucket: "missing", ...} },
    unsure: fn(u) { {..., bucket: "borderline", ...} }
})
```

大白话：原项目自己拍两个门槛常量、手写三分支判断"够不够"；J++ 里门槛数字变成 `cut` 的声明线参数，三个分支变成 act/ignore/unsure 三个出口——"拿不准怎么办"这件事被语言接管，但因为要显式枚举三态，净压掉的行数并不多（这批里净压掉最少的一类例子，用来说明压行效果因项目结构差异很大）。
