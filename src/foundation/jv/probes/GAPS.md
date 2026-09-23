# E-PROBE-10 缺口表（建造第 5 步；只记不修，不改包、不改依据文本）

写十条探针时构建器写不出或要绕的地方。每条：缺口、最小复现、提议对应 v0.1 哪一条。

| # | 缺口 | 最小复现 | 影响 | 提议（对应条款） |
|---|---|---|---|---|
| 1 | **出口不能带出程序**：程序返回 `jv.cut(...)` 的出口列表给调用方处理，返回前 J-05 报「24 个未消费的 Unsure」。冷键下每个出口都是 `Unsure(cold)`，于是「程序只负责判、调用方负责收口」这种分层写不出，收口逻辑必须进程序（十条全部改成把 `(id, truth)` 传进程序、在程序内 `exit_to_result`） | `@jv.program def f(items): return jv.cut(jv.judge([...], q))` → 调用 → `J-05: 程序 f 返回前有 N 个未消费的 Unsure` | 库层（组合、复用）写不出「返回出口的组件」；Codex 组合库评审里的 `Observation` 就是在绕这条 | J-05「作用域 = 本 program 帧」与「出口是 Mat 子类型可进槽」之间缺一条：**出口作为返回值离开程序帧时算「移交」**（消费责任转给接收帧；最外层返回仍核）。或明确写「出口不得作为程序返回值」并给组合子 |
| 2 | **静态检查追不到 helper 内的消费**：`match e: case jv.Unsure(c)` 写在 `_common.exit_to_result` 里，程序体只调用它，检查器报 `W-fail: 程序含 jv.do 但没有任何 Unsure 处理`（假阳性 warn，运行期正确） | p13/p22/p45 每次运行各一条 W-fail | 噪声告警；把 J-05 消费放进公共函数是自然写法 | J-17 报文加「或由被调函数消费（运行期核）」；或让检查器识别「被调函数含 match Unsure」（跨函数一跳） |
| 3 | **冷键下 select / measure 的临时出口被 δ 带吞掉**：临时出口用档案保守线 0.75/0.25，choice 众数概率 0.7（三选一常见值）→ `Unsure(band)`，p 仍在 `e.p`。AUC/准确率只能用 p 自己算，出口本身几乎全是拿不准 | FakeClient 概率 0.7 时十条 choice 探针 answered = 0；改 0.9 才出 Pick | 冷键探针的「出口」不可用，只有读数可用；这正是「无校准就无出口」（能做域表）的实感 | 不是缺口，是纪律；但 handler 库 `cold` 路径可以给「按众数取但标 provisional」而不是再过保守线（提议进 §5 handler 表） |
| 4 | **同一批样本要同时报两道题的指标**（p87 noul + score、p77 noul + score）：`Probe` 一次只报一种指标，只能注册两条探针跑两遍（真机付两次钱；账本不同 root 不复用） | p87 / p87s、p77 / p77s | 实验框架层，不是语言 | 无 |
| 5 | **相同材料的样本被同状态去重合并**：p81 首版 24 样本里正例 i 与 i+12 材料逐字节相同 → 同状态同键，构建器 7 次调用、裸调 24 次。行为正确（P3 账本），但探针设计者容易误读成「构建器省了调用」 | p81 首版 `calls=7` vs bare 24 | 无；已把 24 条改成互不相同 | README §3.1 加一句「同材料同题的向量元素会合并为一次」 |
| 6 | **gen 的候选与样本对齐要靠宿主自己记序**：`jv.gen` 返回的候选没有身份，`cut` 后的出口只按登记顺序对齐，p77 要自己维护 `(goal, j) → sample` 映射，多出的候选要显式 `consume` | p77 `生成执行` 的 `order`/`keys` | 写法繁琐，不是写不出 | 材料带 `addr`（已有）——若 `gen` 给每个候选填 `addr=f"{site}/{retry_seq}/{j}"`，宿主可按 addr 对齐 |
| 7 | **`jv.transform` 的宿主函数收到的是 `Mat` / `list[Mat]`，同一函数不能直接给裸调臂复用**：p45 的 `_table(msgs, results)` 里要 `.content`，裸调臂另写一遍 | p45 `_table` vs `bare` 里的 f-string | 重复代码 | 无（是 J-11 的代价） |

## 不是缺口但要记的

- 十条里 8 条一层写完（向量化 judge），p13/p22/p45/p77 含 `do` 也是一层：`do` 期物在刷新点先解析再判，没有拆层。`Budget(layers=1)` 全部够用。
- `Action.taint_out="untrusted"` 的沙箱动作（导入、基准、sh）输出进 `on` 槽，状态随之 untrusted；这些探针没有不可逆动作，J-08 不触发。
- 探针的合成模板数据上，启发式基线在 p13/p42/p49/p81 是 1.0（预注册 H4 已赌）；这是探针设计的性质，不是 Jev 的。
| 8 | **`jv.handle(cause字符串)` 在向量化出口上取错对象**：`for e in exits: match e: case jv.Unsure(c): p = jv.handle(c)`，「取最近 match 命中的那个」在 24 个 cold 出口上 6/24 返回 None（其余返回的临时出口也未必是本条的）。改为 `jv.handle(e)` 传出口本身 + 直接读 `e.detail["provisional"]` 后正确 | run1 第一遍 p17/p22 各 6/24、5/24 行 p=0.0（`summary_before_p45fix.json`） | 向量化循环里按 cause 字符串 handle 不可靠；README §2.5 的「取最近 match 命中的那个」在循环里语义含糊 | README §2.5 写明「循环里请传出口本身」；或 `handle(cause)` 在有多个候选时直接报错而不是猜 |

## E-CAL 正式版落地收尾新增（2026-09-20，只记不修）

| # | 缺口 | 最小复现 | 影响 | 提议（对应条款） |
|---|---|---|---|---|
| 9 | **`CalibRecord` 没有放校准证据的结构化字段**：`12 §2.3` 的记录结构写了 `n / coverage / δ / status / drift_stat`，实现里却没有 `ece`、可靠性桶、`label_source`，写校准记录时只能把这三样塞进自由字符串 `source`（本次三条记录的 `source` 都是几百字的串） | `CalibStore.put(key, ece=…)` → `TypeError: unexpected keyword 'ece'` | 校准证据不可机读：漂移监控、`ece_by_source` 档案回填、「这条线凭什么」都得解析字符串 | `CalibRecord` 加 `ece: float \| None`、`buckets: list \| None`、`label_source: str`、`coverage: float \| None`；`12 §2.3` 的记录结构同步写全 |
| 10 | **校准记录的作用域是账本目录，不是键**：`Runtime.__init__` 取 `CalibStore(root/"calib")`，所以「线只从校准记录来」在实现上是「从**这个 run 目录**的记录来」。同一个 `e_cal.noul` 键，换一个 root 的程序照样冷启动、出口全 `Unsure(cold)` | 写好记录后 `jv.Runtime(root="runs/jv/别处")` 再判同题 → `cause="cold"` | 校准记录不能跨程序复用，与「能做域 = 有校准记录的 (题, 域) 对」的说法冲突；实验之间互相看不见对方的线 | 全局校准库（如 `foundation/calib/` 或 `JV_CALIB_DIR`）+ run 目录只放本次新增的记录；或 `Runtime(calib=...)` 在 README 里明写「跨 run 复用要自己传 store」 |
| 11 | **`select` 下沉成 K-noul 后，「置换一致率」变成恒真项**：`runtime.py` 的 K-noul 聚合分支把 `mode_share` 写死 1.0（该路径根本没有置换），而下游普遍用 `mode_share ≥ 1.0` 当「正逆置换一致」。E-CAL 正式版 97 条 select 里 89 条走这条路，于是报出「置换一致 1.000（74/74）」，真测量只有 8 条 | `readings.jsonl` 里 `knoul=True` 的条目全部 `mode_share=1.0`、无 `perms` 字段 | 指标分不出「没置换」与「置换一致」，会把「无首位偏置」这类结论建在恒真项上（本次已发生，见 `e_cal_曲线.md` §0） | K-noul 路径不要填 `mode_share`（或填 `None`），并统一用 `perms >= 2` 作为「真跑了置换」的判据；README 的指标口径同步 |
| 12 | **同一个校准键横跨两种物理形式，线的含义不同**：`cut` 按 `q.calib.key` 取记录，但 `12 §2.2` 的 `calib_key` 本来含 `phys`。K-noul 路径的 `p` 是各候选 noul 的最大值、**不归一**（实测各候选和 1.02–2.24，系统性欠自信），真 choice 的 `p` 是归一概率；两者共用 `e_cal.choice` 的同一条 hi 线。δ 也因此分叉（K-noul 取 `delta_for("noul")=0.04`，真 choice 取 0.0781） | 同键同线下，K-noul 桶曲线在对角线上方（差 +0.14…+0.30），真 choice 的桶两极分化 | 线在一种形式上定得合适，在另一种上就偏；候选长度一变（`k_limit` 档一变）线的含义悄悄换了 | 记录键按实际物理形式分（`e_cal.choice@choice` / `e_cal.choice@knoul`），或 `CalibRecord` 按 `phys` 存多组线；短期至少在 `source` 里标明本线由哪种形式扫出（本次已标） |

- 不是缺口但要记：`handle(band)` 的重跑路径（`_rerun`，`run_seq+1`）在账本重放里必然未命中——验证脚本用「一调用就抛错」的客户端时会看到 `W-call-fail`，首判出口不受影响，但真跑时每条 band unsure 要多一次调用。校准记录里的 `unsure_rate` 是**首判口径**。
