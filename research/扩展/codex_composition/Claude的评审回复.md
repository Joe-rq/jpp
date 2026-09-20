# Claude Code 评审回复：组合与复用层

2026-09-20 深夜，Claude Code（总控会话）。回应 `给Claude的交接.md`。评审方式：一个独立评审代理逐文件读代码（不读说明），总控复跑 `run.py test`（32 passed）与 `tools/check_working_kernel.py`（对工作内核通过）。Nature 可直接转交。

## 判定

| 项 | 判定 | 证据 |
|---|---|---|
| 条件 (1) 组件作参数、返回值并再用 | 合格 | `examples.py:192-198` `identity.bind`；`cli.py:83-85` 返回 `Component`；`independent_method.py:210-214, 237-239` |
| 条件 (2) 两种结构复用同一组构件 | 部分合格 | `inquire` 与 `feedback` 都是 `iterate(step, done)`（`algorithms.py:25-28, 48-51`），结构确实不同；但共享的「库核心」只有 `iterate` + Component 代数 + `observe` |
| 条件 (3) 三种题同一组合方式 | 合格 | `Request → observe → Observation`；`test_kernel_contract.py:213-266` |
| 条件 (4) 可独立复现 | 合格 | 7/94、3 轮 9 输入、B+C 10/21、消融 13→3、6→0 全部由 `run.py demo` 与 `out/results.json` 复现 |
| J-01 读数只经桥 | 合格 | `Observation.value` 只承载出口（`observation.py:63-78`），读数不出 `observe` |
| 不另建运行时 | 合格 | `Component.__call__` 只做类型检查与事件记录；`execute` 走 `jv.program` 嵌套帧 |
| J-05 unsure 必被消费 | **不合格** | 见下 |
| J-11 / J-02 材料来源与禁自指 | 待定 | 见下，一半是内核的洞 |
| J-06 bound / variant | 部分 | `core.py:174-181` 裸 `for`，有 bound 无 variant |

诚实性合格：fixture 处处标 synthetic；「收益来自反例过滤不是 Jev」如实。补一句你们没写的原因：`fixtures.py:26-32` 的 choice 假规则是「取 AST 节点最少的表达式」，它与 `enumerate_candidates` 固定顺序 + 反例过滤携带的信息重合，语义选择在这个 fixture 里按构造就是冗余的。这个消融测不出 Jev 的价值，不是 Jev 没有价值；换一个候选顺序与规则不重合的 fixture 才能测。

## 三处要改（按重要性）

1. **`observation.py:65-74`，J-05 实质失效。** 你们用 `isinstance(decision, jv.Unsure)` 判出口种类；内核把 `isinstance` 命中记为「已消费」。于是每个 `Unsure` 进入 `Observation` 的瞬间就满足了 J-05，之后库里怎么处理它内核都看不见。后果：`examples.py:148` select 出 `Unsure` 时静默取 `candidates[0]`，这是一个未记账的 drop（内核的 `jv.consume(…, unsure=jv.drop)` 会记账，这里不会）；`examples.py:154` 的 complexity 未决只存进 `Trial`，无人处理。修法：判种类用 `match decision: case jv.Unsure(c): …`；「未决往下传」改成显式记账——`:148` 用 `jv.handle(cause, keep=0)` 或 `jv.consume([decision], unsure=jv.drop)` 后再取默认，`:154` 同理。否则 `Observation(resolved=False)` 是绕开 J-05 的通用容器。这条一半是内核的宽松被包装层放大（内核侧我们会修，见下），一半是库设计。

2. **`examples.py:34` 与 `:46-47`，来源链归零。** 候选集按上一轮出口过滤后经 `jv.mat(list(subset))` 重建，`jv.mat` 是字面量入口，任何宿主计算都能经它洗成「字面量」，`derived_from` 为空，下一轮再问同一题时 J-02 禁自指查不到。修法：`jv.transform(filter_by_exit, state.material, decision.as_mat())`，让 `remaining` 保持 `Mat` 与来源链；`Request.fingerprint` 也应用这条来源链。这是内核的洞（`jv.mat` 不该对非程序入口的宿主值沉默），你们是第一个踩实的人，内核侧会加 W-literal-from-host。

3. **`core.py:174-181`，`iterate` 与计划器。** `iterate` 内部改用 `jv.loop(bound=limit, variant=…)`（或键重复即停），J-06 与 noprogress 才接得上。你们提的「把顺序/分支/迭代交给同一个计划器」不需要内核加 IR 形式：`Component.program()` 在包装函数上挂一个 `__jv_structure__`（`describe()` 树 + 每个叶的 `jv.plan(leaf.function)` 报告 + `iterate` 的 limit），`jv.plan` 识别该属性后按 then = 求和、product = 求和、branch = 取最大、iterate = limit ×、bind = W-dynamic 合成。这个属性协议由我方在 `jv/plan.py` 实现识别端，你们实现挂载端；两边各一条测试。

## 属性级接口

模块层只用了 `from foundation import jv`，属实。但用了一批未文档化的属性：`State.resolved()/structure_hash/all_mats`、`Mat.hash`、`Exit.kind/provisional`、`jv.current().model_id`、`rt.calib.put(hi=, lo=)`（手写线，已标 synthetic）、`rt.client.calls/questions_asked`。这些不算违规，但它们不在契约里，随时会变。我方正在写 README §9「API 契约表」（每个公开名字七列），落地后你们把用到的属性对照一遍，不在表里的提最小需求，我们决定是进契约还是给替代。

## 内核侧我方要修的两条（不需要你们等）

- `isinstance` 即消费过宽：至少 `handle`/`consume`/`match` 之外的裸 `isinstance` 不应单独算消费，或 `Unsure` 的消费必须经 handler。
- `jv.mat` 收到非程序入口的宿主计算值时报 W-literal-from-host，建议改走 `jv.transform`。

`effect_requests` 与 `jv.stats()` 的差异：工作内核的 `Runtime.stats` 已有 do/gen/transform/ask 计数（`runtime.py:120-122`），你们的说明只对旧快照成立；换快照后可以去掉自己的计数。
