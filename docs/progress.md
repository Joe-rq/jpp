# J++ progress / 项目进度

Updated: 2026-10-05. This is a dated report, not an automatically updated dashboard.

## 2026-10-05 (evening): Towow network — towow.ai homepage with a public live view, privacy fixes, and corrections / 通爻网：towow.ai 首页与公开实时画面、隐私修复、口径订正

> **What changed.** [towow.ai](https://towow.ai) (also net.towow.ai) is now the network's homepage: the full-screen 3D network with one headline, a click-to-copy join command and the current head count, in Chinese and English. When real people are in the network or a judgment ran in the last 5 minutes it shows the public live stream `/live`; otherwise it plays the 10-04 500-agent replay and says so. `/live` sends a per-event-type field whitelist: real joiners appear with their t0 name, needs and offers, disclosure requests carry no purpose, and plans that include a real person carry no title or content. The plain-text guide moved to `/guide`. An Opus refuter found three places where public mode still wrote joiners' text to disk; all are fixed: plans are generated with `claude -p --no-session-persistence`, new fragments' embeddings stay in memory, and agent ids are a salted HMAC instead of a guessable hash. The guide now names Cloudflare as a third party and says that zero opportunities can also mean recall missed someone.
> **Corrections to the 10-05 entry below.** The "$0.11 per cold join" is one measured join on the 500-person network, JEV input tokens only; plan generation by Claude is not counted (a Codex join in the same launch cost $0.016). The "3/25" baseline is vector top 200 + JEV re-rank, and the 10,000 figures come from a recall probe with 25 planted pairs; the network itself has not run at 10,000. "17 s" and "24 opportunities" were observed by the client. Besides the event and replay files that entry mentions, public mode also kept joiners' text in the three places fixed above.
> **How to use it.** Open https://towow.ai. Join as before: `claude mcp add --transport http towow https://net.towow.ai/mcp`.
>
> **变了什么。** [towow.ai](https://towow.ai)（与 net.towow.ai 相同）成了网络的首页：全屏 3D 网络，压一句标题、一行点击即复制的接入命令和当前人数，中英双语。网里有真人或 5 分钟内有判断时连公开实时流 `/live`，否则放 10-04 的 500 人回放并写明。`/live` 按事件类型的字段白名单发出：真实接入者显示 t0 的称呼、在找、能提供；补信息请求不带用途；含真人的方案不带标题和内容。纯文本说明移到 `/guide`。Opus 反驳者查出公网模式仍有三处把接入者的文字落盘，都已修：方案生成用 `claude -p --no-session-persistence`，新片段的向量只放内存，agent_id 改为加盐 HMAC，不再是可猜的哈希。说明页补上 Cloudflare 这个第三方，并写明 0 个机会也可能是召回漏掉了。
> **对下面 10-05 条目的订正。** 「每次冷接入约 $0.11」是 500 人网络上实测的一次，只算 JEV 输入 token，Claude 的方案生成不计（同一次启动里的 Codex 接入 $0.016）。「3/25」的基线是向量前 200 + JEV 重排；1 万人的数来自 25 对的召回探针，网络本身没有在 1 万人跑过。「17 秒」「24 个机会」是客户端观察到的。那一条只说了事件与回放文件；修复之前，公网模式还在上面三处留着接入者的文字。
> **怎么用。** 打开 https://towow.ai 。接入方式不变：`claude mcp add --transport http towow https://net.towow.ai/mcp`。

## 2026-10-05 (later): Towow network — a Codex agent's real join, and what it changed / 通爻网：一个 Codex agent 的真实接入，以及据此改了什么

> **What changed.** A Codex agent joined the public network for Nature, built its pack from three public project documents and stayed 4 minutes. The network found no opportunity for it among the 500 fictional residents. It received 5 disclosure requests, all from fictional residents, and denied all of them. Its report led to seven fixes in [`examples/towow-app`](../examples/towow-app/README.en.md). Dropped opportunities now say which side disclosed. `towow_opportunities` gains a `discovery` summary (pairs judged, whether the round is finished, how many opportunities involve real people) and lists opportunities with real people first. Question text names "you" and the counterpart instead of A/B. `towow_leave` reports what it removed and states that text already sent to JEV and Claude is outside its reach. The pack spec no longer asks agents to read relatives. The join hint now tells the agent to pass the token. The guide explains Codex tool approval for unattended runs. The per-start judgment budget is now visible in `/healthz` and in replies when it runs low. The app README has an [English version](../examples/towow-app/README.en.md).
> **Effect.** A real joiner who finds nothing now sees why. Spending is still capped only per server start ($4.5; at the one measured cold join's JEV cost that is about 40 joins, and Claude plan generation is not counted).
> **How to use it.** Unchanged: `claude mcp add --transport http towow https://net.towow.ai/mcp`.
>
> **变了什么。** 一个 Codex agent 代表 Nature 经公网接入，只用三份公开项目文档编包，停留 4 分钟：网络在 500 位虚构居民里没有为它找到机会，5 条补信息请求全来自虚构居民，全部拒绝。它的报告促成 [`examples/towow-app`](../examples/towow-app/README.md) 的七处修改：放下的机会写明是哪一方多给了信息；`towow_opportunities` 加 `discovery`（判了多少对、这一轮是否判完、和真人的机会有几个），和真人的机会排在前面；题面用「你」和对方名字代替 A/B；`towow_leave` 回报删了什么，并说明已发给 JEV 与 Claude 的文字删不到；编包说明不再要求读亲友；join 的下一步提示写明要带 token；首页说明 Codex 无人值守时怎么批准工具。每次启动的判断预算在 `/healthz` 和快用完时的回复里可见。应用 README 有了[英文版](../examples/towow-app/README.en.md)。
> **效果。** 真实接入者什么都没找到时，能看到原因。花费仍只按每次启动封顶（$4.5；按实测的那一次冷接入的 JEV 花费约合 40 次，方案生成不计）。
> **怎么用。** 不变：`claude mcp add --transport http towow https://net.towow.ai/mcp`。

## 2026-10-05: Towow network — public demo at net.towow.ai, and recall without vector gating / 通爻网：net.towow.ai 公网演示，召回不再由向量挡门

> **What changed.** [`examples/towow-app`](../examples/towow-app/README.md) now runs as a public demo: any MCP client joins with `claude mcp add --transport http towow https://net.towow.ai/mcp` (or `codex mcp add towow --url https://net.towow.ai/mcp`); the root page is the invitee guide. Counterparts in opportunities and disclosure requests carry `real`, and the 500 fictional residents are labelled as not real people. `towow_leave` removes the pack, disclosures, index entries and every edge and config derived from them; real joins survive restarts; in public mode no value-bearing event or replay file is written. Recall changed after three pre-registrations: with a generated, non-duplicated background of 9,500 people, vector top 200 + rerank kept only 3/25 true partners in the top 32 at 10,000 agents (08); JEV judging everyone present instead raised that to 10/25 (09); re-judging the top 200 with the full public pack raised it to 15/25 (10), so agent recall no longer goes through vectors. The J++x scheduler now splits same-state batches by estimated tokens and halves a batch that the judge rejects as too long.
> **Effect (real JEV).** At 10,000 agents true partners in the top 32 went 3/25 → 15/25. A fresh agent joining the public network: both recall stages about 7.5 s, first opportunity 17 s, quiet after 2.7 min, 24 opportunities, about $0.11 per cold join (recall about $0.01; the rest is pair, config and disclosure judgments). A live leave after a granted disclosure leaves no edge, config or node mentioning the agent.
> **How to use it.** Join the public demo with the command above, or run locally with `towow serve`. Operators: `deploy/up.sh` starts server, tunnel and worker; `deploy/watch.sh` under launchd restarts whichever part fails.
>
> **变了什么。** [`examples/towow-app`](../examples/towow-app/README.md) 开成了公网演示：任何 MCP 客户端用 `claude mcp add --transport http towow https://net.towow.ai/mcp`（或 `codex mcp add towow --url https://net.towow.ai/mcp`）接入，首页就是给被邀请人的说明。机会与补信息请求里的每位对方都带 `real`，500 位虚构居民标明不是真人。`towow_leave` 删掉算子包、补充信息、索引条目和由此算出的所有边与构型；真实接入在重启后恢复；公网模式不写带单元值的事件与回放文件。召回经三份预注册改过：用 Sonnet 生成、不复用片段的 9500 人做背景，1 万人时向量前 200 + 重排只留住 3/25 个真伙伴（08）；改为 JEV 判断在场全体，升到 10/25（09）；前 200 名再用完整公开包判一次，升到 15/25（10），所以 agent 召回不再经向量。J++x 调度器现在按估算 token 切批，判断器嫌太长时对半重发。
> **效果（真机 JEV）。** 1 万人时真伙伴进前 32：3/25 → 15/25。一个新 agent 接入公网网络：召回两段约 7.5 秒，第一个机会 17 秒，2.7 分钟后静止，24 个机会，冷缓存每次接入约 0.11 美元（召回约 0.01，其余是两两、整体与补信息判断）。给出补充信息后离开，网里不再有任何含他的边、构型或节点。
> **怎么用。** 用上面的命令接入公网演示，或本机 `towow serve`。运营方：`deploy/up.sh` 起服务、隧道与转发 Worker；launchd 常驻的 `deploy/watch.sh` 谁挂了重启谁。

## 2026-10-04 (later): Towow network — wide recall plus a same-state rerank lifts recall from 0.54 to 0.68 / 通爻网：宽召回加同状态重排，召回 0.54 → 0.68

> **What changed.** Recall in [`examples/towow-app`](../examples/towow-app/README.md) now takes the vector top 200 and reranks them on the joiner's own state with one yes/no question per candidate, answered in a single JEV call (pre-registration 06), then keeps the top 32. A second probe (pre-registration 07) tried a three-way "really helps / looks related but does not / unrelated" question for holds; it halved the hard-negative rate but missed its pre-set bar, so it was not adopted.
> **Effect (real JEV).** Probe: true partners in the top 32 rose from 6/3/0 to 19/11/3 of 25 at 500/2,000/10,000 agents — almost every partner inside the top 200 is recovered. Full run on the same 429 agents: overall recall of gold structures 0.54 → 0.68; one-to-one and relay at t0 0.65 → 0.89; teams/stars/many-to-many 0.36 → 0.45. Cost rose to about $0.0093 per agent and first opportunity p50 to 7.0 s. Still open: hard negatives (18 of 22 held), recall at 10,000 bounded by vector recall (3/25 in the top 200).
> **How to use it.** Unchanged: `towow serve` and `claude mcp add --transport http towow http://localhost:8794/mcp`.
>
> **变了什么。** [`examples/towow-app`](../examples/towow-app/README.md) 的召回改为：向量取前 200 名，再在接入者自己的状态上每个候选一道是非题、一次 JEV 调用答完（预注册 06），取前 32。另一个探针（预注册 07）把成立题改成「真能帮上 / 看似相关但不成 / 无关」三选一，难负例减半，但没过预先定的线，没有采用。
> **效果（真机 JEV）。** 探针：真伙伴进前 32 的数在 500 / 2000 / 1 万人时由 6/3/0 升到 19/11/3（分母 25），前 200 里的真伙伴几乎全部救回。同一批 429 人的全量：真值结构召回 0.54 → 0.68；t0 一对一与转介 0.65 → 0.89；团队/星/多对多 0.36 → 0.45。每人花费升到约 0.0093 美元，首个机会 p50 升到 7.0 秒。仍未解决：难负例（22 对里 18 对判成立）；1 万人时召回上限由向量召回决定（前 200 只有 3/25）。
> **怎么用。** 不变：`towow serve`，再 `claude mcp add --transport http towow http://localhost:8794/mcp`。

## 2026-10-04: Towow network — a J++ application with continuous computation, run live on 500 agents / 通爻网：一个需要持续计算的 J++ 应用，500 个 agent 真机运行

> **What changed.** A from-scratch application, [`examples/towow-app`](../examples/towow-app/README.md): personal agents join a stranger-discovery network with one MCP line and hand the network only their public layer (t0). The network finds pairs, relays, chains, rings, stars, teams, many-to-many groups and groups of groups, each with a confidence (the reading of the question that decided it, with its text); when it is unsure it asks the other side for one category of information; formed groups get a concrete plan. The application needs what the Rust J++ has explicitly not built — continuous computation and a long-lived stateful world — so it runs on an experimental Python language, J++x (`jx/`): versioned cells, residents, three read modes, composition closure, the default fill chain on unsure exits, budgets, deadlines and judge absence. The network program is `app/net.jpx` + `app/lib.jpx` (247 non-blank lines). Two Fable rulings fixed the language and the mechanism; five pre-registrations were committed before the code or runs they govern. A 3D frontend replays real runs.
> **Effect (real JEV).** 454 of 500 agents joined before the TypeSafe balance ran out. Recall of gold structures 0.50 (one-to-one at t0 0.86, chains and rings 0.25, nested 1/4). Of 23 judged hard negatives 19 held and the confidence AUC is 0.636: precision is the main open problem. About 6 opportunities per agent, first opportunity p50 3.7 s, $0.0019 per agent on a warm cache ($0.0068 cold in the first run). A 60-agent subset goes quiet after every join, disclosure and departure. Paired live ablations (small samples): without the fill chain deep recall 2/2 → 1/2; without composition closure nested 4/4 → 3/4. Scale: calls per join stay flat from 500 to 10,000 (255/221/219 under pseudo readings) and recall latency stays around 10 ms p50, but true partners in the top 32 fall with N (5/15 → 1/15 → 0/15), and synthetic clones do not explain it. With recall widened to 500, the median rank of true partners is 57 / 217 / 831 at 500 / 2,000 / 10,000 agents (88% / 48% / 12% within the top 200): a cheap rerank could recover them up to about 2,000; at 10,000 vector recall itself falls short, so discovery quality at 10,000 is not established. Real use: a Claude Code agent acting for one real person compiled its own pack from that person's project READMEs, joined in one call and handled 10 disclosure requests by policy; the simulated world held nobody that person needs, so no opportunity worth following came out. A plain Python + asyncio baseline of the same program is 370 lines against 247, and 34% of it is mechanism the runtime handles: J++ makes the program shorter and quieter to get wrong, not possible where it otherwise was not.
> **How to use it.** `cd examples/towow-app && pip install -e ".[bge]"` (Python 3.12+; the real encoder is the `bge` extra), then `towow serve` and, in Claude Code, `claude mcp add --transport http towow http://localhost:8794/mcp`. The server keeps only t0; t1/t2 leave the agent only through `towow_respond`; each agent gets a token at join. Replays: [full run](demos/towow-app/index.html?replay=full) · [one real agent joining](demos/towow-app/index.html?replay=nature). Verified: a clean virtualenv with `pip install -e ".[test]"` passes 74 tests (1 skipped: the real-encoder test needs the `bge` extra).
>
> **变了什么。** 一个从零写的应用 [`examples/towow-app`](../examples/towow-app/README.md)：个人 agent 用一行 MCP 配置接入陌生人发现网络，只把公开层（t0）交给网络。网络找出一对一、转介、链、环、星、团队、多对多以及「组合的组合」，每个机会带置信度（决定它的那道题的读数和题面）；拿不准时向对方要某一类信息；成形的合作写成具体方案。这个应用需要 Rust 版 J++ 明确「没造」的持续计算与常驻有状态环境，所以跑在实验语言 J++x（`jx/`）上：版本化单元、常驻程序、三种读法、组合封闭、未决出口的默认补信息链、预算、截止、判断器缺席。网络程序是 `app/net.jpx` 与 `app/lib.jpx`（非空 247 行）。两份 Fable 裁定定了语言和机制；五份预注册都早于它们约束的代码或运行提交。3D 前端回放真机运行。
> **效果（真机 JEV）。** 500 人里 454 人在 TypeSafe 额度用完前接入。真值结构召回 0.50（t0 一对一 0.86，链与环 0.25，嵌套 1/4）。判过的 23 对难负例里 19 对判成立，置信度 AUC 0.636：精度是最大的未解问题。每人约 6 个机会，首个机会 p50 3.7 秒，暖缓存下每人 0.0019 美元（第一轮冷启动 0.0068）。60 人子集在每次接入、披露、离开后都静止。真机成对消融（样本小）：关补信息链，深层召回 2/2 → 1/2；关组合封闭，嵌套 4/4 → 3/4。规模：每次接入的调用数从 500 到 1 万不增长（伪读数下 255/221/219），召回时延 p50 约 10 毫秒；但真伙伴进前 32 名的比例随 N 下降（5/15 → 1/15 → 0/15），且不是合成克隆造成的。把召回放宽到 500 名，真伙伴名次中位数在 500 / 2000 / 1 万人时是 57 / 217 / 831（落在前 200 的占 88% / 48% / 12%）：2000 人以内加一道便宜的重排有望救回，1 万人时向量召回本身不够，所以 1 万人时的发现质量没有成立。真实使用：Claude Code 里的 agent 代表一位真人，读他的项目 README 自己编包、一次接入、按策略处理 10 条补信息请求；这个模拟世界里没有他需要的人，没有产出值得跟进的合作。同一程序的普通 Python + asyncio 基线 370 行对 247 行，其中 34% 是运行时代管的机制：J++ 让程序更短、更不容易静默出错，而不是让别处做不到的事变得可能。
> **怎么用。** `cd examples/towow-app && pip install -e ".[bge]"`（Python 3.12+；真编码器在 `bge` 可选依赖里），然后 `towow serve`，再在 Claude Code 里 `claude mcp add --transport http towow http://localhost:8794/mcp`。服务端只收 t0，t1/t2 只经 `towow_respond` 离开 agent；接入时每个 agent 拿到自己的 token。回放：[全量运行](demos/towow-app/index.html?replay=full) · [一个真 agent 接入](demos/towow-app/index.html?replay=nature)。验证：干净虚拟环境 `pip install -e ".[test]"` 后 74 项测试通过（1 项跳过：真编码器测试需要 `bge` 可选依赖）。

## 2026-10-03 (wrap-up): development pauses here; what works, what is not built, how to report / 收尾：开发在这里暂停；能用什么、没造什么、怎么报问题

> **What changed.** Active development of J++ pauses at this point so that it can be used on real projects; problems found in use decide what is built next. No code changed since the previous sync (research tree `27995f8a9`). Before pausing, two rounds of the game-bot target were finished on the real judge, and the ledger keys were made stable (a site is identified by its definition path and an index within it, ledger format 6; editing only comments in the library keeps every cache entry, and the old ledgers of all three targets replay to the same values with the library they were written with).
> **Effect.** Works now: one-command install, three entries (a batch of items, a process driven through a world, modules filled in by the author), five bundled examples and every doc snippet checked by blocking CI. Ran on the real judge: job matching and GitHub issue triage, each from one sentence of purpose, with results comparable to the hand-written versions. Not reached: the game bot. Round 4 (telling the program how a game is scored) lowered its score, because it went after a scoring route it could not survive to reach; round 5 (each scoring route also says what it takes to get it) scored 52.3 against 17.7 for the control on seeds 1–3 and survived all 300 ticks, but only three games ran, so this is recorded as too few samples and only described ($0.161 and $0.346). Not built: continuous computation (one program reading another's in-progress results), a long-lived stateful world, a persistent material store, and most of the static checker. Known rough edge: yes/no questions derived from a purpose do not carry descriptions of what "yes" and "no" mean, so their readings run lower than in projects that write such criteria; hand-written `test(…, {labels: {yes, no}})` does carry them.
> **How to use it.** Install and run the examples from the first screen of [`rust/README.md`](../rust/README.md). Report a problem at [github.com/Towow-ai/jpp/issues](https://github.com/Towow-ai/jpp/issues) with the command, the `E-`/`W-` lines and, if you can, the `--ledger-out` file.
>
> **变了什么。** J++ 的施工在这里暂停，先拿去写真实项目，用的时候撞到的问题决定下一步造什么。自上次同步（研究树 `27995f8a9`）以来代码没有变。暂停前做完了两件事：游戏机器人靶子的第四、五圈真机；账本键稳定下来（站点用定义路径加定义内序号确定，账本格式第 6 版；只改库注释时缓存全部命中；三个靶子的旧账本配回写它时的库，都重放出相同的值）。
> **效果。** 现在能用：一条命令装上，三种入口（一批条目、驱动一个世界的过程、作者自己填模块），五个随仓示例与全部文档片段由阻断的 CI 守着。真机跑通：求职匹配、GitHub issue 分诊，都只给一句目的，效果与手写版相当。没达到：游戏机器人。第四圈告诉程序这局怎么计分，得分反而更低，因为它去追一条活不到就拿不到的计分途径；第五圈让每条计分途径也写明怎样才拿得到，种子 1–3 上自身得分 52.3（对照 17.7），三局都活满 300 拍，但只跑了三局，记为样本不足、只描述（花费 0.161 与 0.346 美元）。没造：持续计算（一个程序读另一个程序算到一半的结果）、常驻有状态环境、持久的料库，以及大部分静态检查器。已知的毛刺：由目的派生的是非题不带「是」「否」各指什么的描述，读数会比写了这种判断标准的项目偏低；手写的 `test(…, {labels: {yes, no}})` 会带上。
> **怎么用。** 安装和示例看 [`rust/README.md`](../rust/README.md) 第一屏。遇到问题到 [github.com/Towow-ai/jpp/issues](https://github.com/Towow-ai/jpp/issues) 提 issue，贴上运行的命令、以 `E-`/`W-` 开头的行，能给的话附上 `--ledger-out` 账本。

## 2026-10-02 (wrap-up sync): installable in one command, five bundled examples, docs and CI that block / 收尾同步：一条命令装上、五个随仓示例、挡得住的文档与 CI

> **What changed.** A newcomer can now install `jpp` with `cargo install --locked --path crates/jpp` (Rust 1.85 or newer), run the first example and see the answer, and run five bundled examples — a batch of items from one sentence of purpose, a process driven through a small world, modules filled in by the author, filling in missing information, and replay from a ledger — all offline on recorded answers at no cost. `rust/README.md` is rewritten in plain bilingual language (what it is, install, one example per entry, what it cannot do yet, how to report a problem); the root READMEs lead with the same install steps.
> **Effect.** CI now installs `jpp` on a clean Ubuntu and a clean macOS machine and runs the five examples with the installed binary against the goldens (`install smoke`); a new blocking job runs every code block in the docs and compares the output pasted under them verbatim (`docs and examples`); fmt (baseline 0, reached by the repository-wide `rustfmt`) and clippy (not above baseline; Linux has its own two-entries-higher baseline for platform-specific warnings) are now blocking (`JPP_CI_GATE=fail`, the job is called `rust-checks`). The doc-snippet run on this sync: 130 passed, 0 failed, 14 skipped (4 sketches, `cargo test` lines that the `rust-source` job already runs, and lines that need the real judge).
> **How to use it.** See the first screen of the [root README](../README.md) and [`rust/README.md`](../rust/README.md); the five commands are in [`rust/examples/README.md`](../rust/examples/README.md).
>
> **变了什么。** 新来的人现在可以用 `cargo install --locked --path crates/jpp`（Rust 1.85 或更新）装上 `jpp`，跑第一个例子并看到答案，再跑五个随仓示例：一句目的加一批条目、驱动一个小世界的过程、作者自己填模块、拿不准时补信息、按账本重放——全部离线、用录好的答案、不花钱。`rust/README.md` 用大白话重写（中英）：这是什么、怎么装、三种入口各一个例子、现在还不能做什么、怎么报问题；根目录两份 README 也把同样的装机步骤放到前面。
> **效果。** CI 现在在干净的 Ubuntu 与 macOS 上装出 `jpp`，用装出来的程序跑五个示例并与金样核对（`install smoke`）；新增阻断作业把文档里每一段代码跑一遍、并逐字核对贴在下面的输出（`docs and examples`）；fmt（基线 0，由整仓 `rustfmt` 达成）与 clippy（不超基线；Linux 因平台相关告警另有一份高两条的基线）现在是阻断的（`JPP_CI_GATE=fail`，作业名 `rust-checks`）。本次同步上的文档片段运行：130 过、0 未通过、14 跳过（4 条示意片段、`rust-source` 作业已跑的 `cargo test` 行、需要真机的行）。
> **怎么用。** 看[根 README](../README.md) 与 [`rust/README.md`](../rust/README.md) 的第一屏；五条命令在 [`rust/examples/README.md`](../rust/examples/README.md)。

This sync takes the research tree from `b8521e558` to `27995f8a9` (about 40 commits touching the Rust tree, ending with a formatting-only `cargo fmt --all` commit and a test fix). Besides the above it carries earlier work already in the research tree: ledger keys no longer contain source byte offsets (a site is identified by its definition path and label; the ledger header records `key_version`, and an older ledger is read by its header instead of failing), the process entry's question tree and completion conditions (rounds 3–5 of the game-bot target), and the clippy baseline lowered from 72 to 61. Known rough edge: the example programs built on the derive library print about a dozen `W-` advice lines about the library's own source on a first run; they are harmless, and they come from library code the author cannot edit.
本次同步把研究树从 `b8521e558` 带到 `27995f8a9`（动到 Rust 树的约 40 个提交，末尾是一笔只改格式的 `cargo fmt --all` 和一处测试修正）。除上面这些，还带来研究树里更早完成的工作：账本键不再含源码字节偏移（站点由定义路径加标签确定，账本头写 `key_version`，旧账本按头读取而不是报错）、过程入口的题树与完成条件（游戏机器人靶子的第三到五圈）、clippy 基线由 72 降到 61。已知的毛刺：基于出题库的示例首跑时屏幕上会打印十来行关于库源码本身的 `W-` 建议，无害，来自作者改不了的库代码。

## 2026-10-02 (sync): two more real-machine targets — GitHub issue triage and an arena game bot / 同步：又两个真机靶子——GitHub issue 分诊与竞技场游戏机器人

> **What changed.** The one-sentence-purpose program now runs on two more open-source projects, on the real judge, with the program source holding no question text, no candidate lists and no cut lines. Target 2 (`hush`, GitHub issue triage, 800 labelled issues) ran end to end and agrees with the maintainers' labels about as often as the project's own hand-written code does. Target 3 (`botcraft`, a bot that plays an arena game step by step) also ran end to end, three rounds, but **did not reach a usable level of play**; the three rounds are reported as they came out. To get there the language gained a process entry (`purpose_plan` / `purpose_step` / `purpose_drive`) over a host action `env:step`, a question pre-run tool, and several rules for what the default chain does with an undecided judgment.
> **Effect.** Target 2, one arm on the final library: exit 0, **$0.189**, E1–E7 all hold, 69.00% agreement with the maintainers' labels over 800 issues (one elicitation only). Total spend on target 2 across all arms and pre-runs: **$0.826**. Target 3: three rounds, **$0.079 + $0.139 + $0.047**; E1–E6 hold in every round, the quality floor E7 fails in rounds 2 and 3, and round 3 plays worse than round 2 (section 3).
> **How to use it.** `jpp run … --env <name>=<command>` registers an external world (a stateless command that reads one JSON request on stdin and writes one JSON result); `rust/lib/derive/drive.jpp` and `purpose_drive` run a process from a purpose; `rust/scripts/plan_preview.py` runs only the question-writing stage on the real judge (under a cap) so a dry run can be checked against what the real run will ask. Details in `rust/GUIDE.md` and `rust/crates/jpp/INTERFACE.md`.
>
> **变了什么。** 只给一句目的的程序又在两个开源项目上跑了真机：程序源码里没有题面、没有候选、没有线。第二靶子（`hush`，GitHub issue 分诊，800 条带标签的 issue）端到端跑通，与维护者标签的一致率与原项目手写代码相当。第三靶子（`botcraft`，一步一步打竞技场游戏的机器人）也端到端跑了三圈，但**没有打出能用的水平**，三圈按实际结果照实写。为此语言新增：建在宿主动作 `env:step` 上的过程入口（`purpose_plan` / `purpose_step` / `purpose_drive`）、出题预跑工具，以及几条「未决判断在默认链上怎么走」的规则。
> **效果。** 第二靶子在最终库上的一臂：退出码 0，**0.189 美元**，E1–E7 全过，800 条 issue 与维护者标签一致率 69.00%（只有一次引出）。第二靶子所有臂与预跑合计 **0.826 美元**。第三靶子：三圈共 **0.079 + 0.139 + 0.047 美元**；每圈 E1–E6 过，质量底线 E7 第二、三圈不过，第三圈比第二圈更差（第 3 节）。
> **怎么用。** `jpp run … --env 名字=命令` 登记一个外部世界（无状态的命令，stdin 读一个 JSON 请求，stdout 写一个 JSON 结果）；`rust/lib/derive/drive.jpp` 与 `purpose_drive` 从一句目的跑一个过程；`rust/scripts/plan_preview.py` 只在真机上跑出题阶段（带花费上限），让空跑能核对真机会问什么。细节见 `rust/GUIDE.md` 与 `rust/crates/jpp/INTERFACE.md`。

This sync takes private main from research-tree commit `f92e1179` (2026-10-01, the previous sync) through `b8521e558` (2026-10-02): 37 commits touching the Rust tree. 本次同步从 `f92e1179`（10-01，上次同步）到 `b8521e558`（10-02），Rust 树上 37 个提交。

**1. What landed in the language / 语言里新增的东西**

- **Host action `env:step` and the process entry.** `do("env:step", …)` advances an external simulation one step. The command registered with `--env` is started as a subprocess (in the OS sandbox when the machine has one), gets the whole world state in and returns the whole next state, so the same input always gives the same output; the ledger records each step with its input, output and wall-clock time, and a replay hits the ledger instead of starting the process. Without a sandbox the action is registered as irreversible. `purpose_plan`, `purpose_step` and `purpose_drive` share the question-writing, judging and field-landing code of `purpose_run`; the action candidates come from the process. A world may report an `effect` (what the last action caused, passed into the next step's context) and the `applied` action (what it actually executed); the language does not interpret either.
- **Question pre-run tool.** `rust/scripts/plan_preview.py` runs the question stage on the real judge under a cost cap ($0.01 for a process entry, $0.05 for a batch entry), writes a cache that the dry and live runs reuse, and records each question's hash. A real run is only started when its question set hashes the same as the pre-run's; the live runs below reused the pre-run judgments with zero extra calls. A dry run alone cannot show this, because the stand-in judge decides which questions pass the gate differently from the real one.
- **A premise must not be evidence.** A derived premise ("can this material answer the purpose at all?") has to be independent of the result: if, when it is false, the purpose could still be answered with one of the candidates, it is evidence for that answer, not a premise. The generator now states, for each premise, where the purpose lands if it is false (`if_false`: a candidate, or `unanswerable`); a premise that points at a candidate is dropped, a malformed one is rejected at the gate, and the original text and outcome of each round go into the ledger.
- **Process entry derives no premises.** In a process, every step's material is the whole observation the world returns under a contract, so answerability is guaranteed by the contract; the batch entry keeps deriving them.
- **Default chain, undecided with a missing slot.** When a judgment is `insufficient` and its candidates come from the question form's `lacks`, the chain fetches by `lacks` directly instead of asking "is the question unclear?" and "why unsure?" first (ruling 76). When the companion questions give a diagnosis out of band, the chain uses it directly instead of asking "why unsure?" serially; if the named category cannot be fetched, it goes to the end and hands over (ruling 77). Reports gain `route` per row and a `named_unfetchable` count per category.
- **Independent judgments go out in one layer.** `purpose` builds all nodes first and then issues the judgments of one step together (R-102). On the modules-fill golden, calls went from 8 to 5 with the same value.
- **Ties and companion readings use the profile's mid-band δ.** A K-of-N question with no line now treats the top two as tied when they differ by no more than the profile's mid-band δ; companion-question readings count only outside 0.5 ± δ (no hand-written 0.5 line). Readings inside the band carry no signal.
- **Window-overflow counts.** With a profile that has measured windows, the report's `window_over` counts object-slot, context-slot and same-material single-request overflows separately.
- **Host facts leave the golden comparison.** The facts about actions that vary with the host's OS sandbox (`exec_py`, `check_tests`, `exec_sql`) moved from the report's `action_facts` into a top-level `host` block, which the golden comparison drops. This replaces the public-only relaxation added to `golden.rs` on 2026-10-01 (see below).
- **Large-value performance (cell graph).** Identity caching of hashes, fingerprints and provenance unions for large lists and records removed the near-quadratic cost when a pure closure captured a multi-megabyte value on every call; a real 5.7 MB input that had not finished within a 30-minute limit now runs in one to three minutes with the cell graph off or on, with byte-identical output.

- **`env:step` 与过程入口。** `do("env:step", …)` 把一个外部仿真推进一步。`--env` 登记的命令作为子进程启动（机器有操作系统沙箱就在沙箱里），整份世界状态进、整份下一状态出，所以同一输入必得同一输出；账本每步记进出内容与墙钟，重放命中账本、不启动进程。没有沙箱时该动作登记为不可逆。`purpose_plan`、`purpose_step`、`purpose_drive` 与 `purpose_run` 共用出题、判断、落字段的代码，动作候选来自过程。世界可自报 `effect`（上一步动作引起的事件，进下一步的语境）与 `applied`（实际执行的动作）；语言不解释这两项。
- **出题预跑工具。** `rust/scripts/plan_preview.py` 在真机上只跑出题阶段，带花费上限（过程入口 0.01 美元、批量入口 0.05 美元），写出空跑与真机共用的缓存，并记每道题的哈希。真机只在题集哈希与预跑逐个相同时才开跑；下面的真机都复用了预跑的判断，新增调用为 0。只靠空跑看不到这一点：替身判断器决定哪些题过闸门，与真机不同。
- **前提不能是证据。** 派生的前提（「这份材料能不能回答这个目的」）必须与结果无关：它不成立时目的仍能用某个候选作答的，就是那个答案的证据，不是前提。生成器现在为每道前提自报「不成立时目的落到哪」（`if_false`：某个候选或 `unanswerable`）；指向候选的前提被弃，不合形的在闸门被拒，每轮的原文与处理结果进账本。
- **过程入口不派生前提。** 过程里每一步的材料是世界按契约交回的整份观测，可判性由契约保证；批量入口照旧派生。
- **默认链：缺槽的未决。** 判断为 `insufficient` 且候选来自题式的 `lacks` 时，链不再先问「题不清吗」「为什么拿不准」，直接按 `lacks` 取（裁定七十六）。伴随题在带外给出诊断时，链直接用它，不再串行问「为什么拿不准」；点名的类别取不到就到末端转交（裁定七十七）。报告每行加 `route`，并按类别记 `named_unfetchable`。
- **互不依赖的判断同层发出。** `purpose` 先建全部节点，再把同一步的判断一起发出（R-102）。modules-fill 金样调用数 8 降到 5，值不变。
- **并列与伴随题读数用画像的中段 δ。** 没写线的 K 选一，前两项差不超过画像的中段 δ 即并列；伴随题读数只在 0.5 ± δ 之外算数（去掉手写的 0.5 线），带内读数没有信号。
- **超窗次数。** 画像测过窗口时，报告的 `window_over` 分开计对象槽、语境槽、同材料一次请求的超窗。
- **宿主事实不进金样比较。** 随宿主操作系统沙箱而变的动作事实（`exec_py`、`check_tests`、`exec_sql`）从报告的 `action_facts` 挪到顶层 `host` 块，金样比较丢掉 `host`。这代替了 10-01 在公开仓 `golden.rs` 里加的只在公开侧的放宽（见下）。
- **大值性能（单元图）。** 对大列表与大记录的哈希、指纹、来源并集做身份缓存，去掉「纯闭包每次调用都捕获数 MB 大值」造成的近平方开销：一份在 30 分钟限时内没跑完的 5.7 MB 真实输入，单元图开或关都在一到三分钟内跑完，输出逐字节相同。

**Behaviour changes to know before upgrading / 升级前要知道的行为变化**

- A K-of-N question with no line now returns a tie when the top two readings are within the profile's mid-band δ (0.0971 for K-of-N under `jev-1.13.0`); the default chain then takes over. 没有线的 K 选一在前两项相差不超过画像中段 δ（`jev-1.13.0` 的 K 选一为 0.0971）时出并列，由默认链接手。
- Companion-question readings inside 0.5 ± mid-band δ no longer stop the chain; only readings outside the band count. 伴随题读数落在 0.5 ± 中段 δ 以内不再让链停下，只有带外读数算数。
- The report's `action_facts` no longer contains the three executor actions; they are under `host.action_facts` together with `host.sandbox`. A program with no such action produces a byte-identical report. 报告的 `action_facts` 不再含三个执行器动作，它们和 `host.sandbox` 一起在 `host.action_facts` 下；没有这类动作的程序报告逐字节不变。
- `--env <name>=<command>` and the `env:step` action are new; the action list in the CLI help gained one entry. `--env` 与 `env:step` 是新增的，CLI 帮助里的动作清单多一项。

**2. Target 2, `hush` issue triage: real-machine result / 第二靶子 `hush`：真机结果**

*Target.* `hush` is an MIT-licensed GitHub issue triage project; its benchmark is 800 closed issues with the maintainers' labels. Arm 0 runs the original code unchanged; arm 0′ is the same logic written in J++ with candidates that carry only the class names; arm 3 gets one sentence of purpose and nothing else. The final library was run once more on arm 3 (the "rerun" below) after the default-chain rules above landed.

*Agreement with the maintainers' labels (800 issues, most probable class).*

| Arm | Agreement |
|---|---|
| 0, original code | 71.75% |
| 0′, J++ with class names only | 68.63% |
| 3, purpose only (first run, earlier library) | 70.87% |
| 3, purpose only (same program, a second elicitation, earlier library) | 68.50% |
| 3, purpose only (rerun on the final library) | 69.00% |

The two elicitations of the same program differ by 2.37 points, so the purpose-only program and the hand-written one are at the same level within the generator's own variation; the rerun's 69.00% comes from a single elicitation and is not compared with the first run as better or worse. The 3.1-point gap between arm 0 and arm 0′ is the measured value of the boundary notes ("what it is and what it is not") in the candidates.

*Rerun on the final library (arm 3).* Exit 0, **$0.189** (cumulative on this target **$0.826**, cap $1.0). The question hashes equal the pre-run's one by one, and the 5,508 judgment keys of the pre-run were all reused. E1–E7 hold. **3.7% of the rows stopped (30 of 811)**; 781 rows fetched the reference and then decided, and both numbers follow from the construction (a declared-slot-missing judgment fetches directly), so they are not reported as system findings. Premise screening dropped 2.4% (19 of 800); ties were 2.0% (16 of 781). Three pre-registered rows were falsified, from two facts: companion diagnoses that fetch and then decide were 0 of 7 (all seven named the context, and the material store holds only references), and the share of duplicate judgments at "yes" among over-window rows (5.5%) was above both the 5% upper bound and the in-window share (3.9%); one count (781 against an expected 750–780) was over by one.

*What cannot be said.* Whether fetching a reference helps cannot be concluded: the duplicate-issue predicate has no ground truth, the 77 in-window rows coincide with the repository size, and the 704 over-window rows have no basis because the profile's window limit is untested (R-131: no ground truth, no conclusion). Of the 42 "yes" judgments, 17 re-judge inside the mid-band, so 5.5% is not a duplicate rate. The first run also showed that the supplied reference was often only a title where the question compares error messages and logs, so "fetch then judge" must not be read as "enrichment helps".

*Findings.* Hypotheses T1, T2, T4 held; T3 (premise layer) was falsified as written and weak (only one judgment premise was derived, so what the sampling check can drop was not exercised); T5 could not be tested as written. System findings filed: the gate diagnoses before the reference is fetched, which biases toward declaring the reference as a slot; an undecided value inside a list makes the whole list undecided and `concat` does not say how to collect it; premises derived as evidence (fixed by the `if_false` rules above).

**3. Target 3, `botcraft` arena bot: three rounds, not at a usable level / 第三靶子 `botcraft`：三圈，没有达到能用的水平**

*Target.* `botcraft` is an open-source arena game where a bot picks an action each tick. Two interfaces were run: A, the original actions (the judge must also walk), and B, where the host enumerates and expands actions (e.g. a shortest path for "go to this object") and the program only chooses. Each arm plays 3 matches (seeds 1–3) as player 0 against a greedy opponent; a random arm R is the quality floor. All runs use `--cells off`.

| Round | What changed | Spend | E1–E6 | E7 (quality floor) |
|---|---|---|---|---|
| 1 | interface A/B, `env:step`, process entry | $0.079 | hold, both interfaces | A: numbers pass, but the bot mostly gathers in place; B: passes literally, weakly |
| 2 | the world's `effect` into the next step's context; enumeration fixes | $0.139 (incl. pre-runs) | hold | fail, all three arms |
| 3 (arm 3g.z) | independent judgments in one layer; tie by mid-band δ; process entry derives no premises | $0.047 (incl. two pre-runs) | hold | fail |

*Round 1.* The runtime parts ran end to end. The program with only a purpose did not play well on either interface, because the result of the last action (`bleed`, `gather_fail`) did not reach the next step's question, so the judge repeated the same useless action. Per-step latency median was 0.32 s, well inside the original project's 600 ms budget (the pre-registered hypothesis that it would exceed it was falsified); the default chain never reached "fetch" (companions judged "unclear" and the chain dropped the row).

*Round 2.* The world's `effect` went into the next step's context, the enumeration was fixed, and both interfaces were rerun. The cause of the failure changed: with a "go toward the resource" candidate in view, the judge chose a standing-still `noop` and kept choosing it, so about 40% of surviving steps were `noop` (four deaths from consecutive no-ops). The `effect` did not pull it out and `noop` persisted with or without it. What is missing is a question about what the last result means; the program asks the same flat "which action is best next" every step.

*Round 3 (arm 3g.z) played worse than round 2.* Mean own score 6.7 against 22.7, surviving steps 85 against 127, and the share of surviving steps where the world received `noop` rose from 0.42 to 0.73 (all three seeds are lower on own score and surviving steps; only 3 matches). The cause is the combination of two things. The action question's readings are flat (the largest option's median probability is 0.21 over 8–18 candidates), so under the mid-band δ 73% of surviving steps are ties; the default chain asked "why unsure?" on those and the judge answered "question unclear" every time, no step fetched anything, and all 187 tied steps handed over an empty action. Replaying round 2's states under the new tie rule at zero cost shows 148 tied steps, of which 45 had chosen a real action; the rule therefore turns some real actions into `noop`, and the rest of the rise comes from shorter trajectories. Pre-registered predictions: T16 (ties ≥ 30%) held at 73%; T15 (latency median 0.61 s to 0.30–0.40 s) is recorded as falsified under the pre-registered caliber (median 0.76 s), though steps with a single model call had a median of 0.34 s, and a zero-cost estimate under ruling 77 puts the median at about 0.50–0.63 s (an estimate, not a measurement, with 0.37–0.38 s if one mapping does not hold); T17 (fetch ≥ 1) and T18 (`noop` share below 0.42) were falsified. The conclusion is about the shape of the question, not a tuning knob: asking the same flat question every step, neither of the two treatments tried (take the maximum, or let ties take the empty action) pulled the program out of `noop`. The next planned change is a question tree that follows the exits.

- 第三靶子 `botcraft` 是一个开源的竞技场游戏，机器人每个 tick 选一个动作。跑了两种接口：A 是原始动作（判断器还得会走路），B 是宿主枚举并展开动作（如「走向某物」用最短路展开），程序只负责选。每臂 3 局（种子 1–3），我方 0 号位，对手 greedy；随机臂 R 作质量底线。全部 `--cells off`。
- **第一圈**：运行时件端到端跑通。只给目的的程序在两个接口上都没打好，原因是上一步动作的结果（`bleed`、`gather_fail`）没进下一步的题，判断器一拍一拍重复同一个无效动作。每拍时延中位 0.32 秒，在原项目 600 毫秒预算之内（预注册里「会超过」的假设被推翻）；默认链一次也没走到「取」（伴随题判「题不清」，链就放弃这一行）。
- **第二圈**：世界自报的 `effect` 进了下一步语境、枚举改了，两个接口重跑。死因换了：「走向资源」的候选就在眼前，判断器却选了原地不动的 `noop`，并且持续选下去，存活拍里约四成是 `noop`（四次死亡都因连续空动作）。`effect` 没把它拉出来，带不带 `effect` 都一样。缺的是一道问「上一步的结果说明什么」的题；程序每拍都问同一道平的「下一步最该做哪个」。
- **第三圈（臂 3g.z）比第二圈更差**：自身得分均值 6.7 对 22.7，存活拍 85 对 127，存活拍里世界收到 `noop` 的比例从 0.42 升到 0.73（三个种子的自身得分和存活拍都更低；只有 3 局）。原因是两件事合在一起：动作题读数很平（8–18 个候选，最大一项的概率中位 0.21），按中段 δ 有 73% 的存活拍判成并列；默认链在这些拍上问「为什么拿不准」，判断器每次都答「题不清」，没有一拍取到材料，187 个并列拍都交了空动作。拿第二圈的状态零花费按新并列规则重算：148 个并列拍里有 45 拍原来选的是真动作，所以这条规则把一部分真动作变成了 `noop`，其余的升高来自轨迹变短。预注册的预测：T16（并列 ≥ 30%）成立，73%；T15（时延中位 0.61 秒降到 0.30–0.40 秒）按预注册口径记推翻（中位 0.76 秒），但只有一次模型调用的拍中位 0.34 秒，按裁定七十七零花费估计中位约 0.50–0.63 秒（是估计，不是测量；若一个映射不成立则约 0.37–0.38 秒）；T17（取到 ≥ 1 次）与 T18（`noop` 占比低于 0.42）推翻。结论说的是题的形状，不是调参：每拍问同一道平的题，试过的两种处理（取最大项、并列取空动作）都没把程序从 `noop` 里拉出来。下一步计划是让题树沿出口往下走。

**4. Public-repository changes in this sync / 本次同步对公开仓的改动**

- `rust/` is synced to research-tree commit `b8521e558` with `tools/sync-rust-from-research.sh`: 70 files changed and 9 added in `rust/` (the cell-graph identity cache, the `env:step` action, `lib/derive/drive.jpp`, `scripts/plan_preview.py`, and six test files).
- **The public-only relaxation of the golden comparison is withdrawn.** The 2026-10-01 sync made the public `golden.rs` drop `action_facts` when comparing, because the goldens were recorded on macOS and three of them went red on Linux CI. The research tree now keeps those facts in a `host` block that its own `golden.rs` drops, and the affected goldens were re-recorded, so the sync script no longer rewrites `golden.rs` and the public file is byte-identical to the research tree's. One further rewrite was added: an error message in `plan_preview.py` that pointed at the research machine's build queue now says `cargo build`.
- Not changed: the Python kernel, the judge profile (`jev-1.13.0.json` has not changed in the research tree since the last sync), the browser demo bundle. Not included: raw ledgers and run directories of the real runs, the targets' benchmark data and bot sources, per-run summaries, the blackboard and process notes.
- 公开仓这次的改动：`rust/` 用 `tools/sync-rust-from-research.sh` 同步到研究树 `b8521e558`，`rust/` 下改了 70 个文件、新增 9 个（单元图身份缓存、`env:step` 动作、`lib/derive/drive.jpp`、`scripts/plan_preview.py`、六个测试文件）。
- **撤销公开侧对金样比较的放宽。** 10-01 同步时公开仓的 `golden.rs` 比较金样前先去掉 `action_facts`，因为金样录在 macOS 上、三个金样在 Linux CI 变红。研究树现在把这些事实放进 `host` 块，它自己的 `golden.rs` 比较时丢掉 `host`，受影响的金样已重录；同步脚本不再改写 `golden.rs`，公开文件与研究树逐字节相同。另加一处改写：`plan_preview.py` 里指向研究机编译队列的报错提示改成 `cargo build`。
- 没动：Python 内核、判断器画像（`jev-1.13.0.json` 自上次同步后在研究树里没有变）、浏览器演示包。不带：真机运行的原始账本与运行目录、两个靶子的评测数据与机器人源码、逐次摘要、黑板与过程记录。

**Not included / 不包含.** The community and contest materials, the blackboard, agent notes and process records, per-run data and request ledgers of the real runs, and `发行说明-待发布.md`. 社区与比赛材料、黑板、代理笔记与过程记录、真机运行的逐次数据与请求账本，以及 `发行说明-待发布.md`。

**Verification / 验证.** In `rust/`: `cargo test --locked --workspace --no-fail-fast` through the research machine's `cargoq` (local, macOS): **2329 passed, 0 failed, 14 ignored**, 317 test targets. The same command on Linux (Ubuntu build host) gave 2314 passed, 15 failed, 14 ignored; all 15 failures are tests that read the judge profile from `src/foundation/profile/`, which sits outside `rust/` and was not copied to that host ("No such file or directory"), so they say nothing about the code; the Linux run is still the evidence that the re-recorded goldens (`golden`), `action_reversibility`, `z0885_env_step` and `z0886_drive` pass off macOS. Those five test files also pass with `JPP_FORCE_NO_SANDBOX=1` (no OS sandbox, as on CI). The `rust-checks` job is report-only and its figures moved against the recorded baseline, which was not updated: `cargo fmt --check` lists 19 files (baseline 0), clippy 137 warnings (baseline 72), 3 over-long files, and 19 documentation snippets plus one example registered as an expected error that now runs (`examples/errors/outcome-dropped.jpp`, unchanged since the unsure-default change) exit non-zero; 104 snippets and examples pass. These come from the research tree as synced and are left unfixed here. Python: the full `python -m pytest -q` passes 562 tests under both Python 3.12 and 3.13, and `jpp demo` runs (the Python kernel and the profile are unchanged in this sync). Credential check over all changed and added files: no key-shaped strings, no key/token assignments, and no occurrence of the value in `~/.typesafe-key`. 在 `rust/` 下经研究机的 `cargoq`（本机 macOS）跑 `cargo test --locked --workspace --no-fail-fast`：**2329 通过、0 失败、14 忽略**，317 个测试目标。同一条命令在 Linux（Ubuntu 编译机）上是 2314 通过、15 失败、14 忽略；15 个失败都是读 `src/foundation/profile/` 里判断器画像的测试，该目录在 `rust/` 之外、没被拷到那台机器（「没有那个文件」），与代码无关；这一趟 Linux 仍是重录后的金样（`golden`）、`action_reversibility`、`z0885_env_step`、`z0886_drive` 在非 macOS 上通过的证据。这五个测试文件在 `JPP_FORCE_NO_SANDBOX=1`（没有操作系统沙箱，同 CI）下也通过。`rust-checks` 作业只报告、不拦截，读数相对记录的基线变了、基线没更新：`cargo fmt --check` 列出 19 个文件（基线 0），clippy 137 条告警（基线 72），3 个超长文件，19 个文档片段与 1 个登记为预期报错却能跑成功的示例（`examples/errors/outcome-dropped.jpp`，自默认链改动以来没变）退出码非零；104 个片段与示例通过。这些来自同步进来的研究树本身，这里没有去修。Python：全仓 `python -m pytest -q` 在 Python 3.12 与 3.13 下都是 562 条通过，`jpp demo` 能跑（本次 Python 内核与画像都不变）。凭据检查覆盖全部改动与新增文件：无密钥样式串、无 key/token 赋值、`~/.typesafe-key` 的值一次也没出现。

## 2026-10-01 (sync): a program that is given only a purpose ran end to end on the real judge (line A, first target); cell-graph evaluation, premise derivation, CLI purpose/material entry, author-filled modules, premise three layers / 同步：只给一句目的的程序在真机上跑通（线 A 第一靶子）；单元图求值、前提派生、CLI 给目的与材料、作者填模块、前提三层

> **What changed.** You can now hand J++ one sentence of purpose plus the material to be judged, and no question text, no candidate lists, no cut lines. `lib/derive/purpose.jpp` (`purpose_run`) pulls the modules out of the sentence with one generation, fills the modules the sentence did not state by written rules, looks each predicate up in the certified question bank, elicits questions for the ones the bank does not have, assembles them, asks the same batch of questions about every item, and returns one record per item with one field per sub-request.
> **Effect.** On 2026-10-01 this ran on the real judge over 400 companies and one résumé, given one sentence: exit code 0, 1,634 calls, **$0.0857**, 446 s, zero hand-written question text in the source. Numbers and what did not hold are in section 2.
> **How to use it.** `jpp run examples/purpose-only.jpp --purpose "<one sentence>" --mat vendors=<file> --mat need=<file> ...`; the program is five lines (section 3).
>
> **变了什么。** 现在可以只给 J++ 一句目的和要判断的材料，不写题面、不写候选、不写线。`lib/derive/purpose.jpp` 的 `purpose_run` 用一次生成从这句话里抽出模块，目的没说的模块按守则由系统补，每个谓词先查已认证题库，题库没有的再唤出题，组装后对每一项问同一批题，每项返回一条记录，目的里的每个子请求是其中一个字段。
> **效果。** 2026-10-01 在真机上对 400 家公司和一份简历、只给一句目的跑通：退出码 0，1,634 次调用，**0.0857 美元**，446 秒，源码里没有手写题面。数字与没中的预测在第 2 节。
> **怎么用。** 见第 3 节：`--purpose` 给目的，`--mat 名字=文件` 给材料。

This sync takes private main from research-tree commit `418cbebd` (2026-09-27, the previous sync) through `f92e1179` (2026-10-01): 377 commits touching the Rust tree. 本次同步从 `418cbebd`（09-27，上次同步）到 `f92e1179`（10-01），Rust 树上 377 个提交。

**1. What landed in the language / 语言里新增的东西**

- **Purpose entry and premise derivation (B0470, Z0511, Z0860).** `purpose_run` generates once for the module extraction plus once per predicate the bank does not hold, so generator calls do not grow with the number of items. For each predicate the language also derives *premise* questions ("does the material state a hiring role?"), asked first so that items that cannot be judged are dropped before the expensive deep judgment. Z0860 (derive-14) splits premises into three layers: a premise code can decide runs as a code predicate (ledger entries `by: code`, no judge call); the generator sees a few sample values when it writes premises; a premise that needs judgment is tried on a sample first and dropped, then re-derived, if it does not split the sample. The first real run, described below, used the version before Z0860.
- **CLI gives purpose and material (B0472).** `--purpose <text>` binds the name `purpose` (untrusted text); `--mat <name>=<file>` (repeatable) binds a material entry, `.json` read as JSON, anything else as text; `--mat-store <dir>` keeps material marks on disk across runs. All three go into the ledger's `entry_hash`, so a library host that supplies the same three produces the same hash.
- **Author-filled modules (B0478).** `lib/derive/modules.jpp` (`modules_run`): an author fills up to ten modules (material, predicates with a cut kind, reference, context, premise, ...), writes no question text, and the language assembles the questions by the same rules; anything left blank is filled by the system and reported under `detail.sources`. See `examples/modules-fill.jpp`.
- **Ties and undecided fields take the default chain (N-T6).** A field whose judgment comes back undecided, including a tie in a K-of-N choice, no longer goes straight to the caller: it first asks which kind of information is missing, fetches it if the host configured a way to, and rejudges. What the chain gives up on is recorded in `detail.dropped`.
- **Cell-graph evaluation (C1, C2, C2b, C2c).** New crate `crates/jpp-cell`: source, code, judgment, program and multi-writer cells, dependency edges recorded at read time, dirty marking along reverse edges, and two-layer truncation (an unchanged input does not recompute; an unchanged memo hash does not recompute upstream). In C2c a call to a pure function becomes a code cell and is memoized; `--cells-stats` prints the counts.
- **Also in this range (names only; semantics in `rust/crates/jpp/INTERFACE.md` and `rust/GUIDE.md`).** Ledger v5; companion questions and the default chain at undecided `cut`/`sieve` sites; fission of a judgment; carried budget balance with `--carry-reauthorize`; question-bank lifecycle (`jpp bank …`, `jpp bank-stats`, `jpp derive-admit`); `--explain`; a closed list of sixteen undecided causes; resume re-asks absent judgments.

**Behaviour changes to know before upgrading / 升级前要知道的行为变化**

- `order` on a question with no calibration record now merges readings within the profile's mid-δ (0.1281 yes/no, 0.0971 K-of-N for profile `jev-1.13.0`) into one tier; before, only exactly equal readings shared a tier. Programs that take `tiers[0][0]` as the winner can pick a different item. Give the question a calibrated line, or write `stat: "expect"` with your own `tie`, to opt out.
- A judgment left undecided at the end of the program is no longer the runtime error J-05: the program returns normally, the report's `status` is `"violation"`, and the CLI exits with code 3 (0 success, 1 error, 2 usage). One violation is recorded per judgment even if several views of it were dropped.
- Under `--guard`, an irreversible `do` is held until the program has its conclusion and runs only if there is no violation; reading its result in the same run is `E-guard-irreversible-midway`. Deferred actions count against the budget from the moment they arrive.
- `unsure("…")` takes one of sixteen causes (`band, tie, insufficient, fail, budget, depth, latency, deadline, noprogress, rejected_all, no_candidate, absent, infeasible, cold, claim_conflict, violation`); anything else is `E-unsure-cause`. `unsure_cause(e)` returns the bare name; detail moved to a separate `detail` field.
- Ledger and header formats gained fields (`rust/crates/jpp/INTERFACE.md` lists them); a ledger with the newer `also` field is rejected by a binary from before Z0593.

- `order` 在没有校准记录的题上，读数相差不到画像中段 δ（`jev-1.13.0`：是非题 0.1281、K 选一 0.0971）并成一档，原来只有读数完全相等才同档；取 `tiers[0][0]` 当第一名的程序可能换人。要退出这个行为：给题认证一条线，或写 `stat: "expect"` 并给自己的 `tie`。
- 程序结束时还欠着的未决不再是运行期错误 J-05：程序照常返回，报告 `status` 为 `"violation"`，CLI 退出码为 3（0 成功、1 出错、2 用法错）；同一判断的多个视图没人接，只记一笔。
- `--guard` 下不可逆 `do` 推迟到程序有结论之后，没有违规才执行；同一次运行里读它的结果报 `E-guard-irreversible-midway`；推迟动作从到达起就占预算。
- `unsure("…")` 的原因必须是上面十六种之一，否则 `E-unsure-cause`；`unsure_cause(e)` 只返回原因名，细节另放 `detail`。
- 账本与头的格式有加字段，见 `rust/crates/jpp/INTERFACE.md`；带新字段 `also` 的账本，Z0593 之前的二进制读不了。

**2. Line A, first target: real-machine result / 线 A 第一靶子：真机结果**

*Target.* A job-matching task: 400 companies (whole records, no fields removed) and one résumé in the reference slot. The program is given one sentence of purpose and nothing else: "Here is my resume. Among these companies, find the ones most likely to invite me to a first interview, rank them, and for each one name the biggest reason it might not." The language turned that into two fields per company: `interview_likelihood`, an ordered question on 5 levels, and `main_risk`, a 5-way choice. The question bank had no match for either predicate, so both were elicited. All 4 generator calls were reused from an earlier dry run's cache; the real run generated nothing new.

*Run.* Exit code 0, `returned`, 445.7 s. 1,634 calls (800 premise, 800 deep judgment, 32 gate diagnosis, 2 bank lookup), 2,040,487 input tokens, **$0.0857** against a $0.1 stop line set beforehand and a $0.5 ceiling. 9,804 judgments in the ledger, every one traceable to a source.

*E1–E7, rechecked by a second agent from the raw ledger, spending nothing:*

| # | Check | Result |
|---|---|---|
| E1 | no hand-written content in the source | holds: 0 literal questions, 0 candidate lists, 0 field-name or question-name hits in the program; 15 hits in the 23 library files, all generic identifiers (`stage`, `candidate`, `mismatch`, `resume`) |
| E2 | real run completes | holds: exit 0, no panic, no `E-*` code |
| E3 | one row per company | holds: 400 rows, both fields present; 9 `main_risk` cells empty (ties), each with a recorded destination |
| E4 | every judgment traces to a source | holds: 9,804 of 9,804; the 4 reused generations match the dry run by key |
| E5 | every undecided has a destination | holds with a reservation: 0 J-05; 10 pending, all with `via`; but the "ask what is missing → fetch → rejudge" path was **not exercised** (the 13 default-chain rows had no fetch route, the 9 ties were handed over without a missing-kind). E5 shows nothing was lost, not that the language went and got the missing piece |
| E6 | cost | holds: $0.0857 ≤ $0.5; call, token and dollar totals equal the ledger sums |
| E7 | quality floor | holds, with a reservation: the shortlist (68 companies) overlaps the earlier hand-written arm's top 20 in 16 (bar: ≥ 6; random expectation 3.4), and none of that arm's 50 worst companies reached the shortlist (bar: ≤ 5; random expectation 8.5). The bar is weak at a 68-company shortlist; the measured values are far from random. Both arms use the same judge, so this says the two question sets agree on that judge, not which is right |

*Questions that carried a line.* 0 of 1,634 calls: every exit is graded `Answer` (no line, the judge's majority reading decides), against a pre-registered bound of under 10%.

*Pre-registered predictions, checked as written (8 of 13 missed, and the premise-layer hypothesis H3 was overturned):*

| Prediction | Result | |
|---|---|---|
| calls 660–1,140 | 1,634 | missed |
| predicate-lookup and gate-diagnosis judgments 15–25 | 34 | missed (slightly high) |
| premise-layer calls 500–700 | 800 | missed (at the ceiling) |
| survivors after the first premise layer 160–260, after all layers 100–220 | 400, 400 | missed, **falsified** |
| deep-judgment calls 100–220 | 800 | missed: nothing was filtered, and the two questions (ordered, K-of-N) are separate calls per company |
| chained follow-up questions 40–150 | 0 | missed: the purpose entry has no chaining |
| fetch-missing-information calls 0–40 | 0 | held |
| generator calls 2–9 | 4 (all reused) | held |
| undecided rate under 2% | 10 of 800 = 1.25% (`main_risk` alone 9 of 400 = 2.25%) | held, roughly |
| share of readings landing in the "near the line" band 5–15% | 13 of 800 = 1.6% | missed (low) |
| top-20 overlap with the hand-written arm 10–15 | 16 | missed (high) |
| that arm's bottom 50 reaching the shortlist 0–3 | 0 | held |
| cost $0.05–0.10 | $0.0857 | held; the controller's pre-run estimate of $0.02–0.05 was too low because it assumed half the companies would be filtered |
| H3: the premise layer saves calls; overturned if under 30% of companies are filtered | 0% filtered; the layer cost 800 calls and $0.0247, 29% of the run | **overturned** (the secondary bar "at least 17 of the top 20 survive" held 20 of 20, but nothing was filtered, so it carries no information) |

*System findings from the run / 系统发现:*

1. A literal premise of the form "does the record state X" is useless on uniform structured records: the generator only sees field names, so the premise it writes most naturally is satisfied by every record. Code can decide whether a field is present, so it should not spend a judgment (intent 7a). This is what Z0860 addresses (see section 1); Z0860 has been tested and reviewed but **not yet re-run on the real judge**.
2. The elicited question text names this dataset's fields (`hiring_for`, `description`, `size`, ...) because the module-extraction prompt shows the generator the material's field names. The source stays clean, but a differently-named dataset gets different question text and a different calibration identity; reuse across datasets needs the field names normalised out first.
3. Ordered questions give 5 tiers: 68 / 264 / 1 / 25 / 42 companies. The top tier of 68 tie, and the shortlist is that tier. "Ranked in order" degraded to "bucketed in five"; a real ranking would need a within-tier order, which is a design question still open.

**3. Usage: a program given only a purpose / 用法：只给目的的程序**

The program ships as [`rust/examples/purpose-only.jpp`](../rust/examples/purpose-only.jpp), with six invented supplier records and a one-line order in `rust/examples/purpose-only/` (no data from the line A run). The purpose and material switches for `jpp check` sit next to it in `purpose-only.args`, which `scripts/doc_snippets.py` passes; `jpp check` accepts it with no static errors. The program is:

```
import "../lib/derive/purpose.jpp";
budget {calls: 2000, cost: 0.5, depth: 100000};
let items = map(content(vendors).items, fn(x) { {on: mat(x), ref: need} });
let r = purpose_run(purpose, items, {});
{value: r.value, pending: r.pending, detail: r.detail}
```

```
cd rust && cargo run --locked -p jpp -- run examples/purpose-only.jpp \
  --purpose "Here is our order. Among these suppliers, find the ones most likely to deliver it on time, rank them, and for each one name the biggest risk." \
  --mat vendors=examples/purpose-only/vendors.json --mat need=examples/purpose-only/need.md \
  --backend live --model jev-1.13.0 --profile profiles/jev-1.13.0.json --confirm \
  --gen-model sonnet --gen-profile profiles/gen-claude-p.json --cache .jpp-cache \
  --output report.json --ledger-out ledger.jsonl
```

`vendors.json` is `{"items": [ ... one record per item ... ]}` (a `.json` file is read as JSON); `need.md` is read as text and goes into the reference slot. The line A run used the same five lines over its own company file and résumé. `purpose` and each `--mat` name are bound as untrusted values (a `--mat` name cannot be `input` or `purpose`). `r.value` has one record per item: `fields` holds one entry per sub-request of the purpose, `trust` marks each field trusted or untrusted; `r.detail.questions` lists every generated question with its source. `--cache <dir>` reuses generations and judgments across runs. **The live backend and the generator both call a model and spend money; `budget.cost` is the stop line, and a run priced above the confirm threshold needs `--confirm`.** This sync did not re-run the live example; the numbers in section 2 come from the 2026-10-01 run in the research tree. 本次同步没有重跑真机示例，第 2 节的数字来自研究树里 10-01 的那次运行。

**Not included / 不包含.** The community and contest materials, the blackboard, agent notes and process records, per-company run data, the request ledger of the real run, and `发行说明-待发布.md` (a release-notes draft that cites private paths; its content is summarised in section 1) stay private. Machine-specific tools that need the research machine's remote build host (`scripts/cargoq`, `cargoq-stale-repro`, `train.sh`, `test-companions-on`) are not synced: use plain `cargo`, and set `JPP_TEST_COMPANIONS=on` to run the suite with companion questions on. `COORDINATION.md` and the human spot-check file stay excluded as before. 社区与比赛材料、黑板、附注与过程记录、逐家的运行数据、真机的请求账本，以及引用私有路径的 `发行说明-待发布.md`（内容在第 1 节概述）不公开；依赖研究机远端编译机的 `scripts/cargoq`、`cargoq-stale-repro`、`train.sh`、`test-companions-on` 不同步，公开仓直接用 `cargo`，要开伴随题跑全量设 `JPP_TEST_COMPANIONS=on`；`COORDINATION.md` 与人工抽检文件照旧不带。

**Verification / 验证.** In `rust/`: `cargo test --workspace --no-fail-fast` through the research machine's `cargoq` (run on the public repo's `rust/`): **2282 passed, 0 failed, 13 ignored**, exit 0, 312 test targets, 1,605 s. An earlier full run of the same tree found failures that this PR fixes, all sync defects: `cross_kernel.rs` read an outdated judge profile under `src/foundation/` (fixed by syncing the profile), and two tests read research-tree directories that are not public (`ablation/plan.rs` and `jpp-plan`'s `ablation/fission.rs`; both now skip with a notice). `cargo check --locked --workspace --all-targets` is clean. `jpp check examples/purpose-only.jpp` with the switches in `purpose-only.args` reports no static errors; the example has not been run on the live backend. Python: the full `python -m pytest -q` passes 562 tests under both Python 3.12 and 3.13, and `jpp demo` runs (the Python reference kernel is unchanged in this sync; only the judge profile is brought forward and the browser bundle rebuilt). Credential check over all changed files: no key-shaped strings, no key/token assignments, and no occurrence of the value in `~/.typesafe-key`. 在 `rust/` 下经研究机的 `cargoq` 跑 `cargo test --workspace --no-fail-fast`：**2282 通过、0 失败、13 忽略**，退出码 0，312 个测试目标，1,605 秒。更早一次全量暴露的失败都是同步缺陷，本 PR 已修：`cross_kernel.rs` 读了 `src/foundation/` 下过时的画像（同步画像后解决），另有两个测试读研究区未公开的目录（`ablation/plan.rs` 与 `jpp-plan` 的 `ablation/fission.rs`，现改为打印提示后跳过）。`cargo check --locked --workspace --all-targets` 干净。`jpp check examples/purpose-only.jpp` 带 `purpose-only.args` 里的开关无静态错误，该示例没有在真机上跑过。Python：全仓 `python -m pytest -q` 在 Python 3.12 与 3.13 下都是 562 条通过，`jpp demo` 能跑（本次 Python 参照内核不变，只补了发行画像并重打了浏览器演示包）。凭据检查覆盖全部改动文件：无密钥样式串、无 key/token 赋值、`~/.typesafe-key` 的值一次也没出现。


## 2026-09-27 (daily sync): the judge is trusted by default; judgment-division law, question trees, declared lines everywhere, call reuse, field-stability fixes, five PR #37 review fixes / 每日同步：默认相信判断器；判断分工定律、题树、声明线全面接入、调用结果复用、现场稳定性修复、PR #37 五条评审修复

> ### **New default: J++ trusts the judge. Release gating is now opt-in (`--guard`). / 新默认：J++ 相信判断器，放行把关改为可选（`--guard`）。**
>
> **What changed.** A `cut` with no author-written line now follows the judge's own answer (yes/no above 0.5 acts, below 0.5 ignores, `select` picks the most probable candidate, `measure` the most probable level; an exact tie is `unsure(tie)`). The report's `exits` rows carry grade `Answer`. `cold` and `drift` are no longer causes of `unsure`. An irreversible `do` is **no longer blocked** by taint, lineage or line grade, and executors run in a plain subprocess when the machine has no OS sandbox (a `W-action-no-sandbox` warning, no refusal). A program with an irreversible `do` and no `--ledger-out` writes its ledger next to the source as `<name>.ledger.jsonl` (the ledger is a record, not a defence). A bare number is a declared line: `cut(r, 0.7)` means `cut(r, {declare: {hi: 0.7}})`.
> **What used to be the default is now `--guard`.** The release gate (J-08 at check and run time, including lineage and question-text taint), the line-grade warnings (`W-fixture-line`, `W-trial-line`, `W-declared-line`, ...) and the "a program with an irreversible `do` must give `--ledger-out`" rule (`E-ledger-required`) all apply only when you pass `--guard` (CLI `run`/`check`; library hosts set `EntryArgs.guard`). `--release-on-declared` only matters under `--guard`. Without `--guard`, the hashes, IR and reports of programs are byte-identical to before except for the new default routing.
> **Why.** Nature's ruling (intent compilation 11a, 2026-09-26): the language does not build in defences or impose our values on users who may not need them; it trusts the judge by default and lets the author or host switch defences on. Marking, certification and gating stay available as tools.
> **How to check it.** `cd rust && cargo run -p jpp -- run examples/truth-pending.jpp --fixtures examples/fixtures/truth-pending.json` follows the judge's answer (grade `Answer`); `--guard` turns the gate and the notes back on. `scripts/guard_baseline.py` compares runs under `--guard` with the pre-flip goldens (51/52 identical, the one difference is the intended `truth-pending` change); `scripts/replay_scan.py` replays 56 groups under both switches with 0 differences.
>
> **变了什么。** 作者没写线的 `cut` 现在按判断器自己的回答走（是非题 p > 0.5 出 `act`、< 0.5 出 `ignore`；`select` 取概率最大的候选；`measure` 取概率最大的档位；恰好并列出 `unsure(tie)`），报告 `exits` 行的等级是 `Answer`。`unsure` 的原因里不再有 `cold` 与 `drift`。不可逆 `do` **不再被** taint、谱系、线等级拦下；机器没有操作系统沙箱时，执行器在普通子进程里跑（报 `W-action-no-sandbox` 告警，不再拒绝）。有不可逆 `do` 又没给 `--ledger-out` 的程序，账本默认写到源文件旁的 `<源文件名>.ledger.jsonl`（账本是记录，不是防御）。裸数字是声明线：`cut(r, 0.7)` 等于 `cut(r, {declare: {hi: 0.7}})`。
> **原来的默认现在要用 `--guard` 打开。** 放行把关（检查期和运行期的 J-08，含谱系与题面 taint）、线等级告警（`W-fixture-line`、`W-trial-line`、`W-declared-line` 等）、「有不可逆 `do` 必须给 `--ledger-out`」（`E-ledger-required`）都只在传 `--guard` 时生效（CLI 的 `run`/`check`；库宿主设 `EntryArgs.guard`）。`--release-on-declared` 只在 `--guard` 下有意义。不传 `--guard` 时，程序的哈希、IR、报告与改前逐字节相同，只有缺省的分流不同。
> **为什么。** Nature 的裁定（意图汇编 11a，2026-09-26）：语言不内置防御，不把我们的价值观按进去；默认相信判断器，由作者或宿主自己决定要不要开防御。标注、认证、把关都保留为可选工具。
> **怎么验证。** `cd rust && cargo run -p jpp -- run examples/truth-pending.jpp --fixtures examples/fixtures/truth-pending.json` 按判断器的回答走（等级 `Answer`）；加 `--guard` 把关和告警重新打开。`scripts/guard_baseline.py` 把 `--guard` 下的运行与翻转前的金样对照（52 份中 51 份一致，差的一份是预期的 `truth-pending`）；`scripts/replay_scan.py` 两种开关下重放 56 组，0 差异。

This sync takes private main from the public PR #37 state (research-tree `abdec952`) through research-tree commit `418cbebd`. The Rust tree was synced in three parts so the PR #37 review fixes each have their own commit (listed under the fifth item below).

**1. Judgment-division law (batch 8).** *What:* code decides what code can decide; JEV judges only what needs understanding (intent 7a). `search` and `verify` gain `opts.keep`, a code predicate slot that filters candidates before JEV sees them, and a `Fail` value no longer enters the frontier. The question-tree walk `lib/compose/tree.jpp` (JEV descends a tree of follow-up questions the generator wrote in advance, each answer choosing the branch) is repaired against real generator output: `validate_tree_with` normalises shapes and filters by vocabulary, `parse_tree`, `add_members`. The SQL example now judges semantics (does the query answer the question) and pre-filters errors and empty results in code; GUIDE's Pairings chapter gains the criterion and counter-examples. *Effect:* examples no longer spend judge calls on facts a line of code can compute; a leaf item reached through a tree carries its path lineage. *Use:* `search(..., {keep: fn(c) {...}})`, `tree` (see `examples/tree-collab.jpp`, `examples/search-keep.jpp`, GUIDE "Question trees"). *Verify:* new tests `compose_tree.rs`, `compose_search_rank.rs`, goldens `tree-collab`, `search-keep`, `search-rank`.

**2. Declared lines everywhere.** *What:* `sieve`, `search` (`opts.line`, `opts.objective_line`), `verify`, `judged_graph`, `judged_bipartite` and `literalize` accept an author-declared line (`opts.line` / `{line}`), and the checker's J-03 and static J-08 read them. Declarative fit `fit({declare: f, tie?}, rs, extra?)` gets its second part: checker rule `E-fit-declare-effect`, `Score` accepted by `cut`/`order`. `closed.cuts` reports each cut point, `hi` may be a run-time value. *Use:* `sieve(items, q, [..], {line: 0.7})`; `examples/declare-fit.jpp`. *Verify:* `sieve_declared_line.rs`, `b153_declared_fit.rs`, `bypass_25_9_composite_release.rs`.

**3. Call-result reuse (step 19).** *What:* judgments, generations and transforms are reused by a cache key that leaves out the call site, within one run and across runs. *Use:* `jpp run prog.jpp --cache <dir>`; hits are written to the new ledger as reused entries (cost 0), so the ledger alone still replays; the report gains `cache {hits, ...}`. A missing directory is an empty cache; any other read error stops with `E-cache` (PR #37 review). `--gen-cache` is retired. Entries in an open layer (`do`, `ask`, `transform`, `repeat`) are booked in registration order (step 15h-3). *Verify:* `crates/jpp/tests/e2e/cache.rs`, `gen_layer_order.rs`.

**4. Field-stability fixes.** Network errors become absences (a judgment is `Unsure(absent)`, a `gen` is `Fail`) instead of aborting the run; `read_json` looks in the program's directory first, then the working directory; the `claude -p` reply format is relaxed (`W-gen-count`). *Verify:* `field_stability.rs` (12 tests).

**5. Five PR #37 review fixes, one public commit each (two findings share one commit).** / **PR #37 的五条评审修复，各有公开提交（date_add 两条同一提交）**

| # | Finding / 意见 | Public commit / 公开提交 |
|---|---|---|
| 1 | same-key in-flight generations merged (P1) / 同键生成合并 | `040910c` |
| 2 | generator profile `gen.timeout_s` validated (P2) / 画像 timeout 校验 | `2d10085` |
| 3 | `date_add` second argument must be a record / 第二参数必须是记录 | `5ad97a5` |
| 4 | `date_add` arithmetic overflow-checked / 算术溢出检查 | `5ad97a5` |
| 5 | `--cache` read errors reported as `E-cache` / 缓存读错误 | `2276e55` |

**Not included / 不包含.** Nature's spot-check labels, the community and contest materials, the blackboard, agent notes and local paths stay private. The foundation texts (12, 13, constitution, Nature's decision list, positioning, motivation, intent compilation) are not re-synced; the judgment-division law and the trust-by-default rule are described in this file and in the composition update instead. Research-tree design notes outside those files follow the sync script. / Nature 的抽检标注、社区与比赛材料、黑板、附注和本机路径不公开；依据文本（12、13、宪法、决策单、定位、动机、意图汇编）这次不重新同步，判断分工定律与默认相信判断器在本文里说明。其余设计文档照同步脚本走。

**Verification / 验证.** In `rust/`: `cargo fmt --check` clean; `cargo test --locked --workspace`: **1369 passed, 0 failed, 10 ignored**; `cargo clippy --workspace --all-targets --keep-going`: 76 warnings (the previous recorded baseline was 72; the warnings are style hints such as collapsible `if` and `expect_err` in tests, none reports a correctness problem, and the count is not reduced in this sync); `scripts/guide_check.py` GUIDE code blocks match their files (14/14); the two CI example runs (`adaptive.jpp`, `partial.jpp`) succeed; `scripts/ci_public.sh` runs in report mode (it lists 19 documentation snippets with non-zero exits, 90 passed, 6 skipped; report mode does not fail CI). 在 `rust/` 下：`cargo fmt --check` 干净；`cargo test --locked --workspace`：**1369 通过、0 失败、10 忽略**；clippy 76 条告警（上次记录的基线是 72；告警是可折叠 `if`、测试里的 `expect_err` 等风格提示，不涉及正确性，本次同步没有去清理）；GUIDE 代码块与文件逐字核对 14/14；CI 的两个示例程序跑通；`scripts/ci_public.sh` 为报告模式，不阻断。

## 2026-09-26 (second daily sync): composition primitives open up (search, ground, judged graphs), a real generator backend, durable ledger writes, declared-line maturation, batching / 第二次每日同步：搭配原语开放（搜索、接地、判出来的图）、生成器接真后端、账本落盘、声明线成熟、判断合批

Second sync of the day (see this same day's first entry, further below in this file, for the executor/graph action library and its PR #36 security hardening, not repeated here). This one takes private main from `2eb748dc` (where the first sync of the day stopped) through the current private main head, syncing everything else that landed today. A dedicated write-up on the composition layer -- the main substance of this entry -- with a worked example nesting three pairings in one closed loop, is in [today's second update](updates/2026-09-26-b-composition-is-the-foundation.md); this entry covers the same ground plus everything else.

**The composition layer opens: `search`, `ground`, judged graphs, and two new general-purpose primitives underneath them.** `lib/compose/ground.jpp` pairs JEV with any registered executor action (`exec_py`, `check_tests`, future ones) into a single "run it, then judge whether it worked" function; a failing action (including "no sandbox available") returns a failure value that `sieve` treats as undecided rather than a crash. `lib/compose/search.jpp`'s `search` runs propose-judge-repropose in rounds, accumulating good candidates across rounds (a same-day design correction over the initial per-round-only accumulation), handing not-yet-decided candidates to a `carry`/`refine`/hand-to-a-human policy, and terminating via the same bounded-iteration machinery every other construct in the language uses. `search` accepts an optional `ground` function, so pairing the two turns "generate, then judge" into "generate, run, then judge the outcome" -- demonstrated in a new example, `examples/search-ground.jpp`, where a generator proposes Python code, `exec_py` runs it, and JEV checks the output, looping until a correct candidate is found. `lib/compose/graph.jpp` treats JEV-judged pairs of nodes as a graph's edges and runs an exact algorithm (the six `graph:*` actions) on it, running the algorithm twice for edges not yet decided (once absent, once present) so the two results' disagreement marks exactly which edges are worth asking about next. Underneath all three, two general-purpose primitives opened today: `compose(exits, rule)` folds a list of judgment exits into one under a rule (`any`/`all`/`min`/first-or-highest), keeping the same three-way undecided semantics and statistical error-bound bookkeeping a single judgment carries; `cert(exit)` reads a certified error bound, an outstanding-uncertainty count, and the certification grade off any exit, single or composed. `element(input, exit, ctx)` was opened as a further primitive for building a single graph/search element from a judgment outcome without going through a full aggregate construct.

**A real generator backend, and it doesn't block the rest of the program.** `gen` now has a working port to `claude -p` (`--gen-model`), dispatched through a non-blocking submit/poll thread pool: a program's other judgments and executions keep moving while a generation call is outstanding. A generation call registers where it's written but is only sent at the next refresh point in its layer, and is only waited on when its result is actually read -- so a generated value a branch never reads is never paid for. Generated output carries an explicit `untrusted` trust tag from the generator's capability profile, propagated through everything derived from it by the same mechanism every other value's trust bit uses. `--gen-cache` skips paying for a repeated generation at the same site on a later run. One live run of the minimal `gen`+`sieve` example (`examples/gen-choose.jpp`) cost $0.00004 (one `claude -p` call, three JEV judgments) and replayed identically from the recorded ledger with zero new calls.

**Author-declared lines gain a host-acceptance path and a richer statistic.** A host can now explicitly accept an author-declared line (from the previous sync's ruling) to license an irreversible action (`--release-on-declared`, hashed into the run's `entry_hash` so accepting it is itself an auditable fact) instead of the run refusing outright. `cut` gained a `stat` option so a declared line can gate on a judgment's aggregate statistic -- an expected value across score buckets, or the total probability mass across a chosen subset of a multiple-choice answer -- rather than only its single top answer, with the comparison open or closed independently at each end of the line; a hand-audited default (rather than a derived one) keeps both ends closed unless explicitly opened, after a review flagged that a derived default would have silently done the opposite.

**Ledger writes are now durable line by line, not only at the end of a run.** Previously a crashed or killed process could leave no usable record of what it had done. Every ledger entry is now flushed to disk as it's written rather than batched until the run finishes, and an irreversible action's intent is recorded before it executes so a later resume can tell what was attempted even if the process died mid-action; a host that can't provide durable ledger storage at all is refused outright (`E-ledger-required`) rather than silently running without the safety property.

**22 new built-in functions for text and data, plus seeded randomness.** String operations (split, case conversion, trim, replace, prefix/suffix checks, character indexing), regular expressions, sorting (plain and by a custom key, both stable), JSON parse/serialize round-tripping through the same canonical form the ledger uses, content hashing through the same function ledger keys use, and date parsing/formatting/arithmetic -- plus `rand`/`rand_int`/`shuffle`, seeded and reproducible (documented as a fixed, versioned algorithm rather than "whatever the standard library does today," so a program's behavior doesn't silently drift under a future compiler/library upgrade). All follow the same trust-propagation rule as every other value in the language: untrusted input in, untrusted output out.

**Judgments on the same material batch together more often.** Previously, a yes/no question and a multiple-choice question about the same underlying material could end up issued as separate calls even when a hand-written program would combine them into one; candidates now travel with the question they're being asked about and get grouped by the material they're actually judging, closing that gap. This is a rendering-format change (bumped to a new render version, `r2`) that only affects the judgment request's on-the-wire shape, not any answer's meaning; an older recorded ledger written under the previous format still replays correctly (flagged, not silently reinterpreted), but cannot be resumed with new calls under the new format without an explicit re-recording step.

**A checked-in, checked-to-run guide, and a ranking fix it was waiting on.** `rust/GUIDE.md` gained a full chapter, "Pairings: element -> composition -> nesting," walking through every pairing this entry describes with real, checked-in example files under `rust/examples/guide/`; `rust/scripts/guide_check.py` runs and cross-checks every one of them against the chapter's prose so the two can't silently drift apart -- 20 of 20 example cases pass. Alongside it, `order` (which turns a set of judgment readings into ranked tiers) gained the statistic option `cut` already has and now defaults to ranking a scored judgment by its bucket position rather than its single most-probable bucket's raw probability -- the correct-ranking behavior `search`'s objective-based sorting had been left without since the composition layer opened. Full account of both in [today's second update](updates/2026-09-26-b-composition-is-the-foundation.md).

**Verification.** `cd rust && cargo fmt` (one file needed reformatting) then `cargo build --locked --workspace` succeed. `cargo clippy --workspace --all-targets --keep-going`: 72 warnings, matching this repository's current baseline (no new warnings anywhere in this sync). `rust/scripts/guide_check.py`: 20/20 example cases pass, GUIDE.md's 13 embedded code blocks match their source files byte for byte. `cargo test --locked --workspace`: **1221 passed, 0 failed, 10 ignored**.

今天第二次同步（同一天第一条同步的执行器/图算法动作库与它在 PR #36 上的安全加固见本文件更靠下的那一条，不重复）。这次把私有 main 从 `2eb748dc`（当天第一次同步停下的地方）推进到私有 main 当前 HEAD，把当天落地的其余全部内容同步过来。搭配层——本条目的主要内容——的专题写法，含把三种搭配嵌进同一个闭环的可跑例子，见[当天第二篇更新](updates/2026-09-26-b-composition-is-the-foundation.md)；本条目讲同样的内容，外加其余全部改动。

**搭配层开放：`search`、`ground`、判出来的图，以及它们底下的两个通用原语。** `lib/compose/ground.jpp` 把 JEV 与任何已登记的执行器动作（`exec_py`、`check_tests`，以后新加的也算）搭成一个「跑一遍再判对不对」的函数；动作失败（含「没有可用沙箱」）时返回一个失败值，`sieve` 把它当未决处理，不当崩溃。`lib/compose/search.jpp` 的 `search` 分轮跑「提出-判-再提出」，好候选跨轮累积（对首版「只累积本轮」的当天设计订正），把还没判定的候选交给 `carry`/`refine`/交人三种策略之一处置，终止用的是语言里其他构造都在用的同一套有界迭代机制。`search` 的选项里能给一个 `ground` 函数，把两者搭起来，「生成再判」就变成了「生成、跑、再判结果」——新增的示例 `examples/search-ground.jpp` 演示了这个用法：生成器提 Python 代码、`exec_py` 跑它、JEV 核对输出，循环直到找到一个对的候选。`lib/compose/graph.jpp` 把 JEV 判定为「有」的节点对当成图的边，在这张图上跑一个精确算法（六个 `graph:*` 动作），对还没判完的边把算法跑两遍（一遍当它不存在、一遍当它存在），两次结果不一致的地方正好标出接下来最该问哪些边。这三者底下，今天开放了两个通用原语：`compose(exits, rule)` 按一条规则（`any`/`all`/`min`/取第一个或最高排名的那个）把一批判断出口折成一个，仍然带着单个判断同样的三值未决语义与统计误差界记账；`cert(exit)` 能从任何出口（单个的或合成的）上读出经认证的误差界、剩余不确定性的计数，以及认证等级。`element(input, exit, ctx)` 作为又一个原语开放，让人能从一个判断产物直接造出一个图/搜索元素，不必经过完整的聚合构造。

**生成器接上真后端，而且不会卡住程序其余部分。** `gen` 现在接到 `claude -p`（`--gen-model`）的真实端口，经一个非阻塞的提交/轮询线程池调度：一次生成调用飞着的时候，程序里其余的判断与执行照常往前走。生成调用在写下的地方登记，但只在它所在层的下一个刷新点才真正发出，只有真被读到结果时才等它——所以一个分支根本没读到的生成值不花钱。生成出的材料带明确的 `untrusted` 可信标签（来自生成器的能力画像），这个标签沿用语言里其他值同一套传播机制。`--gen-cache` 让后续跑同一个生成站点不用再花一次钱。一次最小 `gen`+`sieve` 示例（`examples/gen-choose.jpp`）的真机运行花费 0.00004 美元（一次 `claude -p` 调用、三次 JEV 判断），从记录的账本重放时结果完全一致、零新增调用。

**作者声明线加了宿主接受路径，也能判更丰富的统计量。** 宿主现在可以显式接受一条作者声明线（上次同步的裁定）来放行不可逆动作（`--release-on-declared`，进这次运行的 `entry_hash`，接受本身就是可审计的事实），不再是直接拒绝运行。`cut` 加了 `stat` 选项，让声明线除了只按单一最高档答案判定外，也能判一个汇总统计量——跨打分档位的期望值，或多选题某个候选子集上的概率总和——线的两端各自独立可开可闭；一处评审发现「派生的缺省值会悄悄把端点变开」之后，改成手写的缺省值（两端都闭），不再靠派生。

**账本写入现在逐行落盘，不再只在一次运行结束时才写。** 此前一个被杀掉或崩溃的进程可能什么可用记录都留不下。现在每条账本条目写下的时候就落盘，不再攒到运行结束才写；一个不可逆动作在执行之前先记下意向，这样即便进程在动作执行中途死掉，后续续接也能看出当时到底试图做了什么；完全没法提供落盘能力的宿主直接被拒绝（`E-ledger-required`），不会悄悄在没有这层安全保障的情况下运行。

**22 个新的文本与数据内置函数，加带种子的随机数。** 字符串操作（切分、大小写转换、去空白、替换、前后缀判断、按字符取下标）、正则表达式、排序（普通与按自定义键，都稳定）、JSON 解析/序列化（往返用的是账本同一套规范形式）、内容哈希（用的是账本键同一个哈希函数）、日期解析/格式化/加减——加上 `rand`/`rand_int`/`shuffle`，带种子、可复现（文档写明是一个固定的、有版本号的算法，不是「标准库今天恰好怎么实现」，这样以后编译器/库升级不会让程序行为悄悄漂移）。全部遵循语言里其他值同一套可信传播规则：不可信输入进去，输出也不可信。

**同一份材料上的判断更常合批。** 此前，对同一份材料问的一道是非题和一道选择题，即便手写程序会把它们合成一次调用，J++ 有时也会分开发出。候选现在随着被问的那道题一起走，按它实际要判的材料分组，补上了这个缺口。这是一次请求格式的改动（渲染版本升到 `r2`），只影响判断请求在线上的形状，不改变任何答案的含义；旧格式录的账本仍能正确重放（会被标出来，不是悄悄按新格式重新解读），但不能在新格式下直接续接发新调用，需要一步显式的重录。

**一份签进仓库、能跑起来验的指南，加它一直在等的一处排序修复。** `rust/GUIDE.md` 新增完整一章「搭配：元素 → 组合 → 嵌套」，把本条目讲的每种搭配都配上真实签入的示例文件（`rust/examples/guide/`）；`rust/scripts/guide_check.py` 把每一段都跑一遍并与正文交叉核对，防止两者悄悄脱节——20 个示例用例全部通过。同一批还落地了：`order`（把一批判断读数变成有排名的档位）加了 `cut` 已有的那个统计量选项，现在对打分判断缺省按档位排、不再按最高档的原始概率排——这正是搭配层开放以来 `search` 按目标题排序一直缺的那个正确排序行为。两者完整说明见[当天第二篇更新](updates/2026-09-26-b-composition-is-the-foundation.md)。

**验证。** `cd rust && cargo fmt`（一个文件需要重新排版）之后 `cargo build --locked --workspace` 成功。`cargo clippy --workspace --all-targets --keep-going`：72 条告警，与本仓库当前基线相同（本次同步没有引入任何新告警）。`rust/scripts/guide_check.py`：20/20 示例用例通过，GUIDE.md 嵌入的 13 段代码块与源文件逐字节相同。`cargo test --locked --workspace`：**1221 passed, 0 failed, 10 ignored**。

## 2026-09-26 (daily sync): a composition-layer action library (host executors, graph algorithms, retrieval), lazy cut bridging, and two Codex-flagged calibration fixes / 搭配层动作库（宿主执行器、图算法、检索）、惰性过桥、两处 Codex 指出的校准修复

This is the daily sync from the research workspace (`tools/sync-rust-from-research.sh`, `tools/sync-from-workspace.sh`), taking private main from `85e28bfc` (the commit the previous sync, [PR #35](https://github.com/Towow-ai/jpp/pull/35), landed) through `2eb748dc` (2026-09-26) -- 111 research-workspace commits, most of them process record, blackboard and design-ledger entries around a smaller set of code changes grouped below.

**A single host action table, and 11 new entries in it.** The CLI's built-in `do`-actions (`record_check`, `read_json`, `write_json`) used to have their facts written in three places that could drift apart -- the registration table, the per-action closures, and a hand-written sentence in the help text. Two steps collapsed this to one table: C-1 moved the three facts (name, reversibility, `taint_out`, cost, the run function) into a single `crates/jpp/src/cli/actions/` table that both `check` and `run` read from (950 tests passed, help text byte-identical to `main` across three CLI invocations); C-1b then moved that table into the `jpp` crate's lib target (`crates/jpp/src/actions/`) so library-side consumers, not just the CLI binary, can see it. Two further blocks then just added rows: R2b added six exact-algorithm actions under a `graph:` prefix (`matching` -- Kuhn-Munkres for bipartite graphs plus an exact bitmask DP for general graphs up to 20 nodes, `shortest_path` -- Dijkstra, `max_clique` -- Bron-Kerbosch up to 60 nodes, `components` -- union-find, `set_cover` -- exact DP under 20 elements or greedy above, `max_flow` -- Dinic), each checked against a brute-force reference on 200 random graphs with zero mismatches and timed on realistic sizes (325-node components in 4.2ms, 325-node max-flow in 11.1ms; `max_clique` is NP-hard and was timed at its n=60 design limit instead, 0.49ms, not at 325 nodes -- recorded as a deliberate scope decision, not a shortcut). R2a added four more: `exec_py` and `check_tests` run arbitrary Python in a subprocess with environment stripped to a minimal `PATH`, a static import-and-`eval`/`exec` reject list, and a runtime monkeypatch on `socket.socket.connect`/`getaddrinfo` that blocks network access at the standard-library layer (explicitly documented as not a sandbox -- it does not stop `ctypes` or other routes around the Python `socket` module); `embed_topk` calls a local MiniLM model through a configurable interpreter path (`JPP_EMBED_PYTHON`, no hard-coded machine path -- an earlier draft that shipped one was corrected after review) and caches corpus embeddings by content hash; `bm25_topk` is a dependency-free Rust implementation. A follow-up commit added a fifth, `exec_sql`, which the design texts (`12`, `19`, `21` step 24e-1) list in the same block as the other four but the original task order omitted -- a read-only SQLite connection (writes rejected by SQLite's own `mode=ro` open flag, verified against a real write attempt rather than assumed), 3-second timeout, same network-disabling patch. The table now lists 14 actions total.

**Lazy cut bridging and straight-line lifting through function calls (ruling B94, research step 23c).** Previously, `cut` resolved a pending judgment's exit (checked it against a threshold, wrote the calibration-usage ledger entry) at the point it was written; this step makes `cut` return an unresolved `Value::Cut` immediately and defers resolution to a small set of "inspection points" (builtin arguments, `if` conditions, both sides of `&&`/`||`, field/index access, function boundaries) that force it when the value is actually needed. Combined with lifting `judge` calls (and now-lazy function calls with only name/literal arguments) to the top of a straight-line segment when they share the same state -- stopping at branches, loops, and short-circuit operators, so no call fires on a path that might not execute -- two of three constructed benchmark programs dropped from 4 judgment-call layers to 2 with the same 11 calls, and a frozen T1 acceptance case (`winnow`, hop-count criterion) went from failing to passing. All 42 fixed-observation golden programs stayed byte-identical (their `cut` results are already inspected before the next registration in every case), and a same-day review round found and fixed five follow-on issues before merge: a lineage-resolution error that was silently swallowed instead of propagated, a batching bug where fused-off flushing dropped the `speculative`/`lifted` flags on split registrations, a case where lifted registrations and their real-site siblings sharing a ledger key were asked as two separate questions instead of one, and straight-line lifting reaching into a callee's body across a short-circuit operator's unevaluated side. The final gate on this step: 947 passed, 0 failed, 10 ignored; 42 golden replay groups, 0 diffs.

**Two smaller static-check and CLI steps.** Step 20a-2b added `jpp check --questions-out <file>` (ruling B116 (5)): it walks a program's literal `test`/`select`/`measure` calls and exports each one's zero-slot form hash, verified to match byte-for-byte the hash the same question produces at runtime via `form(...)` -- the check this step exists to make possible, since a later migration step needs to key legacy per-author calibration records by this hash. The same step also narrowed four public legacy sample-splitting certification entry points down to one, explicitly named and documented as a test-only reproduction of the old method (a certified line built this way carries no delta bound and cannot be used to license an irreversible action). Step 24h closed out three of six previously-deferred "needs a closer look" checks from an earlier design review, and explicitly left the other three (a rendering-provenance warning, a scale-anchor check, and a loop-termination-measure check) in the backlog because each would require a language-level feature that does not exist yet, not just a new diagnostic rule -- written up rather than quietly attempted: a static shape precheck on `cut`'s `{cost: [fp, fn]}` argument, a new `W-legacy-fn-type` warning for old-style (`Fn(A) -> B`, no declared effects) function-typed parameters that are actually called inside the function body (this immediately flagged two of the project's own example programs, `adaptive.jpp` and `composition.jpp`, as a real, expected finding under the new rule, not a regression), and a new `W-unsure-not-bound` warning that fires even when a program's summed judgment budget is under its declared limit, if any of the judgment sites contributing to that sum sit inside a loop or a function body (the existing check only warned about this when the budget was already exceeded, missing the case a budget-under-limit program with an under-counted loop site is the one this check exists to catch).

**Two Codex review fixes on PR #35, now in this sync.** [PR #35](https://github.com/Towow-ai/jpp/pull/35)'s review flagged two issues after that sync had already landed; both are fixed as of this sync. First, `calib-import --from-ledger` built its sampling frame by parsing a ledger file's lines directly, without checking the hash chain between them -- a ledger with one answer edited (and the rest of the chain left alone) would be accepted and could seed a certificate from a tampered reading. It now decodes the file through the same chain-validating reader the `--replay`/`--resume` paths already used, and rejects a broken chain with `E-ledger-corrupt` before anything is written. Second, `calib-import --cost fp,fn` picked a cost-minimizing threshold from the full labeled sample and then certified that same threshold's error rate against the same sample -- a binomial confidence bound only holds for a threshold fixed before looking at the data, so certifying a data-selected threshold against the data that selected it can understate the true false-accept rate. It now splits the labeled sample the same way the project's other threshold-selection methods already do (alternating stratified halves, ruling B85): one half picks the cost-minimizing line, the other half certifies it, and the resulting certificate records which method produced it (`cost-split-stratified`) so it can be distinguished from a non-split certificate on reload. Both fixes required re-tuning four existing test fixtures whose sample counts had been sized for full-sample certification and no longer cleared the certification bar at half the effective sample size.

**A public-only test fixture the sync tooling had not caught.** One of step 20a-2b's four new tests (`b116_questions_out.rs`) reads a fixed example program from `../../../评估/2026-09-24-V7固定序/refund-do.jpp` -- a path that resolves inside the research workspace, one level above where this repository's `rust/` directory sits, and does not exist in this repository. This is the same class of problem the previous sync's `calib_load.rs` fix addressed (a test built against a path only the private research tree has), just not yet caught for this newer file: `cargo test --workspace` failed with `No such file or directory` on first run of this sync. Fixed the same way -- the exact program (a 9-line fixture, content unchanged) is now checked into `crates/jpp/tests/fixtures/refund-do.jpp`, and `tools/sync-rust-from-research.sh` rewrites the test's path to it on every future sync, the same pattern already used for `calib_load.rs`'s legacy-record fixture.

**A same-day security-hardening round on the new executor actions, added to this same PR after review.** PR review of the action library above (an external reviewer plus the coordinating session's own testing) found that `exec_py`/`check_tests`'s `reversible: true` claim rested only on a static code-text scan and a runtime network patch, neither of which stops every way to write a file (`pathlib.Path(...).write_text(...)` matches no keyword in the reject list) -- and, more severely, that `exec_sql`'s read-only connection does not stop `VACUUM INTO` or `ATTACH DATABASE` from creating new files on disk. Five issues in total (four from review, one found by the coordinator) are fixed: the three code-running actions now run inside an OS-level sandbox (macOS `sandbox-exec`, Linux `bwrap`, probed once at startup with an actual smoke test rather than a presence check, since a CI runner can have `bwrap` installed yet unable to use it) that is the layer actually enforcing "no writes outside a fresh temp directory, no network" -- explicitly not enforcing "no reading the host's files" or any CPU/memory/process-count limit, stated in the code rather than left implicit; when no usable sandbox is found, the three actions are marked non-reversible and a new static check, `E-action-no-sandbox`, fails `check`/`run` unconditionally. `exec_sql` gains `PRAGMA query_only` plus a SQLite authorizer callback allow-listing only read operations, denied statements now fail outright instead of reporting through the same field used for ordinary syntax errors. A `truncate` helper that could panic mid multi-byte character now backs off to a character boundary, and several graph actions' fixed absolute floating-point thresholds (which silently dropped small-magnitude edges while still claiming an exact result) are now either relative to the input's own scale or removed where an exact comparison suffices. Full account, including what the isolation still does not cover, in the [action-library update](updates/2026-09-26-composition-layer-actions.md). This round's own verification: `cargo build`/`cargo fmt --check` clean, `cargo clippy --workspace --all-targets --keep-going` at 72 warnings (down from 73, no new warnings), and the affected plus neighboring test files pass on this machine (macOS, where the sandbox is genuinely available). Pushing this and watching CI surfaced one more gap: 13 of `actions/exec.rs`'s own module-level unit tests called `exec_py_core`/`check_tests_core`/`exec_sql_core` directly without checking sandbox availability first, unlike the integration tests in `crates/jpp/tests/`, which the ported commits had already taught to skip gracefully -- these 13 failed outright on the public CI's Linux runner (no `bwrap`) with a `NoSandbox` panic instead of skipping. Added the same skip-and-print-reason guard (`if sandbox::tool().is_none() { ...; return; }`) already used by one sibling test in the same file to all 13, verified locally by forcing the no-sandbox path with `JPP_FORCE_NO_SANDBOX=1` (all 13 skip cleanly, 0 failures) and then confirming the normal, sandboxed run is unaffected: `cargo test --locked --workspace` **1052 passed, 0 failed, 10 ignored**, `cargo fmt --check`/`cargo clippy --workspace --all-targets --keep-going` (72 warnings, unchanged) still clean.

**Verification.** `cd rust && cargo fmt` (nine files needed reformatting -- pre-existing drift already present on the private main branch before this sync, not introduced by it) then `cargo build --locked --workspace` succeed. `cargo clippy --workspace --all-targets --keep-going`: 73 warnings, unchanged from this repository's recorded baseline (no new warnings from anything in this sync). `cargo test --locked --workspace`: **1033 passed, 0 failed, 10 ignored** (after the fixture fix above; the first run before that fix reported 1 failure in exactly the fixture-path test; this count predates the security-hardening round described above).

这是从研究工作区做的每日同步（`tools/sync-rust-from-research.sh`、`tools/sync-from-workspace.sh`），把私有 main 从 `85e28bfc`（上一次同步 [PR #35](https://github.com/Towow-ai/jpp/pull/35) 落地的那个提交）推进到 `2eb748dc`（2026-09-26）——111 个研究工作区提交，大多是过程记录、黑板与设计总账条目，围绕下面这几组代码改动展开。

**宿主动作表收成一张，新增 11 行。** CLI 的内置 `do` 动作（`record_check`、`read_json`、`write_json`）过去把事实分写在三处，容易漂移——注册表、各动作自己的闭包、帮助文本里手写的一句话。两步把它收成一张表：C-1 把三处事实（名字、是否可逆、`taint_out`、成本、执行函数）收进 `crates/jpp/src/cli/actions/` 的一张表，`check` 与 `run` 都从这里读（950 条测试通过，三种 CLI 调用的帮助文本与 main 逐字节相同）；C-1b 接着把这张表挪进 `jpp` crate 的 lib 目标（`crates/jpp/src/actions/`），让库层的调用方（不只是 CLI 二进制）也能看到它。随后两块施工只是往表里加行：R2b 加了六个带 `graph:` 前缀的精确算法动作（`matching`——二分图用 Kuhn-Munkres、一般图用精确位掩码 DP（n≤20）；`shortest_path`——Dijkstra；`max_clique`——Bron-Kerbosch（n≤60）；`components`——并查集；`set_cover`——20 个元素以内精确 DP、以上贪心；`max_flow`——Dinic），每个都对暴力解法跑了 200 组随机图对拍、零分歧，并在真实规模上计过时（325 节点的 `components` 4.2 毫秒、325 节点的 `max_flow` 11.1 毫秒；`max_clique` 是 NP-hard，按它 n=60 的算法设计上限计时（0.49 毫秒），没有硬凑到 325 节点——这是记录在案的范围决定，不是抄近路）。R2a 又加了四个：`exec_py`、`check_tests` 在子进程里跑任意 Python 代码，环境清到只剩最小 `PATH`，加一张静态的 import/`eval`/`exec` 拒绝表，外加运行期给 `socket.socket.connect`/`getaddrinfo` 打补丁挡住标准库层面的网络访问（文档里写明这不是沙箱——挡不住 `ctypes` 等绕开 Python `socket` 模块的路径）；`embed_topk` 经可配置的解释器路径（`JPP_EMBED_PYTHON`，不写死本机路径——早期草稿写死过，复核后已改正）调用本地 MiniLM 模型，按内容哈希缓存语料向量；`bm25_topk` 是不依赖新库的纯 Rust 实现。一次后续提交补上第五个 `exec_sql`——设计文本（`12`、`19`、`21` 步 24e-1）把它和前四个列在同一块，最初的任务指令漏列了——只读 SQLite 连接（写操作被 SQLite 自身的 `mode=ro` 打开位拒绝，用一次真实的写入尝试验证过，不是假设），3 秒超时，同一套网络禁用补丁。动作表现在共 14 行。

**惰性过桥与直线段提升穿过函数调用（B94 裁定，研究树步 23c）。** 此前 `cut` 在写下的那一刻就解析待定判断的出口（对阈值判断、写校准使用账本条目）；这一步把 `cut` 改成立即返回一个未解析的 `Value::Cut`，把解析推迟到少数几个「检视点」（内置函数实参、`if` 条件、`&&`/`||` 两侧、取字段/下标、函数边界）——真正用到这个值时才强制解析。配合把状态相同的 `judge` 调用（以及现在惰性化的、实参只有名字或字面量的函数调用）提升到直线段的段首——遇分支、循环、短路运算符即停，不让任何一条不一定会走到的路径上多发一次调用——三个构造出的基准程序里有两个从 4 层判断调用降到 2 层、调用数不变（仍是 11 次），一个此前失败的冻结验收用例（`winnow`，按跳数判据）由不过变为通过。42 个固定观察金样全部逐字节不变（它们的 `cut` 结果在下一次登记之前都已经被检视过），合入前的同日复核又发现并修了五处问题：谱系解析出错被悄悄吞掉而不是往上传、融合关时 flush 分组丢掉了拆分登记的 `speculative`/`lifted` 标记、提升登记与共享同一账本键的真站点被问成两道题而不是一道、直线段提升穿进被调函数体时没有绕开短路运算符不一定求值的那一侧。这一步最终门禁：947 通过、0 失败、10 忽略；42 组金样重放，0 差异。

**两个较小的静态检查与 CLI 步骤。** 步 20a-2b 新增 `jpp check --questions-out <file>`（B116 (5) 裁定）：遍历程序里的字面 `test`/`select`/`measure` 调用，导出每道题的零槽题式哈希，并核实过它与同一道题在运行时经 `form(...)` 算出的哈希逐字节相等——这正是本步要为之铺路的检查，因为后续一步迁移作者串的旧校准记录需要按这个哈希做主键。同一步还把四个公开的旧法种子分半认证入口收窄成一个，明确命名并写明它只用于测试、复现旧方法（这样认证出的线不带 δ 界，不能用于放行不可逆动作）。步 24h 从此前一次设计复核留下的六项「待核」里做完了三项，另外三项（渲染来源告警、刻度锚点检查、循环终止量检查）明确留在问题清单——因为每一项都需要一个当前还不存在的语言级特性，不只是缺一条诊断规则，如实写明而不是悄悄硬做：给 `cut` 的 `{cost: [fp, fn]}` 参数加了静态形状预检；新增 `W-legacy-fn-type` 告警，抓旧式（`Fn(A) -> B`，不带效应声明）的函数类型参数在函数体内被真的调用的情形（这条新规则立刻命中了项目自己的两个示例程序 `adaptive.jpp` 与 `composition.jpp`——这是预期内的真发现，不是回归）；新增 `W-unsure-not-bound` 告警，在程序的判断预算联合和没有超过声明上限时也会触发——只要贡献这个和的判断站点里有任何一个落在循环或函数体内（既有检查只在预算已经超限时才提示这一点，漏掉了「和算出来没超、但被循环站点低估了」这个本该被这条检查逮住的情形）。

**PR #35 上 Codex 评审指出的两处修复，现已并入本次同步。** [PR #35](https://github.com/Towow-ai/jpp/pull/35) 合入后，评审指出两处问题，本次同步时都已修复。其一，`calib-import --from-ledger` 直接逐行解析账本文件建抽样框，不校验行间哈希链——一份被改过一条答案（其余链未动）的账本会被照单全收，可能用一条未经认证的读数出证书。现在改用与 `--replay`/`--resume` 相同的校验链的解码器读整份文件，链断了在写任何东西之前就报 `E-ledger-corrupt` 拒绝。其二，`calib-import --cost fp,fn` 在全量标注样本上挑一条经验代价最小的阈值，再在同一批样本上认证这条阈值的错误率——二项置信界只对「看数据之前就已固定」的阈值成立，用挑出这条阈值的同一批数据去认证它，会低估真实的假放行率。现在按项目其余阈值选择方法早已在用的做法（分层交替分半，B85 裁定）拆分标注样本：一半选出代价最小的线，另一半认证它，证书上记录选线方法（`cost-split-stratified`），装载时能与未分半的证书区分开。两处修复都需要重新调整四个既有测试夹具的样本量——它们原先是按全量样本认证的门槛配的，样本量减半后原有数字不够用了。

**一处同步工具此前没接住的公开侧夹具问题。** 步 20a-2b 新增的四条测试里有一条（`b116_questions_out.rs`）读一个固定示例程序 `../../../评估/2026-09-24-V7固定序/refund-do.jpp`——这个路径解析到研究工作区内部、`rust/` 目录所在位置的上一层，本仓库里没有这个目录。这与上一次同步给 `calib_load.rs` 修的问题同一类（测试指向只有研究树才有的路径），只是这个更新的文件此前还没被接住：本次同步第一次跑 `cargo test --workspace` 时报 `No such file or directory` 失败。修法相同——把这个程序原样（9 行，内容未改）收进仓库 `crates/jpp/tests/fixtures/refund-do.jpp`，并让 `tools/sync-rust-from-research.sh` 在每次同步时把测试的路径改写指向它，与 `calib_load.rs` 那份旧格式记录夹具走的是同一套机制。

**同一天在这个 PR 上又加的一轮安全加固，针对新加的执行器动作。** 上面这批动作库的评审（一名外部评审加协调会话自己的实测）发现：`exec_py`/`check_tests` 的 `reversible: true` 断言只靠静态代码文本扫描与运行期网络补丁撑着，两者都挡不住所有写文件的写法（`pathlib.Path(...).write_text(...)` 不含拒绝表里的任何关键字）；更严重的是，`exec_sql` 的只读连接挡不住 `VACUUM INTO` 或 `ATTACH DATABASE` 在磁盘上新建文件。一共修了五条（评审四条、协调会话自己查出一条）：三个跑代码的动作现在跑在操作系统级沙箱里（macOS `sandbox-exec`、Linux `bwrap`，启动时用一次真实冒烟测试而不是只看文件存不存在来探测，因为 CI 跑的机器可能装了 `bwrap` 却用不了它），沙箱才是真正强制「不许写临时目录之外、不许联网」的那一层——明确不强制「不许读宿主文件」或任何 CPU/内存/进程数限制，这一点写在代码里、不是留给人猜；探测不到能用的沙箱时，这三个动作被标为不可逆，一条新的静态检查 `E-action-no-sandbox` 会无条件让 `check`/`run` 失败。`exec_sql` 加了 `PRAGMA query_only` 加一个只放行读操作的 SQLite 授权回调，被拒绝的语句现在直接让调用失败，不再走普通语法错误共用的那个字段。一个可能在多字节字符中间截断而 panic 的 `truncate` 函数改成退到字符边界；几个图算法动作原先写死的绝对浮点阈值（会悄悄丢掉数值很小的边、却仍报精确）现在改成按输入量纲取相对阈值，或者在本来就不需要容忍浮点噪声的地方直接去掉阈值。完整说明，包括隔离到底还盖不住什么，见[动作库专题更新](updates/2026-09-26-composition-layer-actions.md)。这一轮自己的验证：`cargo build`/`cargo fmt --check` 干净，`cargo clippy --workspace --all-targets --keep-going` 72 条告警（比之前的 73 少，没有新增），受影响与相邻的测试文件在本机（macOS，真的有沙箱可用）全部通过。推上去看 CI 又抓出一处缺口：`actions/exec.rs` 里有 13 条模块级单元测试直接调 `exec_py_core`/`check_tests_core`/`exec_sql_core`，没有先判沙箱可用性——不像 `crates/jpp/tests/` 里的集成测试，移植进来的提交已经教会它们探测不到就跳过——这 13 条在公开仓库 CI 的 Linux runner（没有 `bwrap`）上直接因为 `NoSandbox` panic 失败，没有跳过。给这 13 条都加上同文件里已有一条兄弟测试用的同一套判断（`if sandbox::tool().is_none() { ...; return; }`），本机用 `JPP_FORCE_NO_SANDBOX=1` 强制走无沙箱路径验证过（13 条全部干净跳过，0 失败），再确认正常的（有沙箱的）跑法不受影响：`cargo test --locked --workspace` **1052 通过、0 失败、10 忽略**，`cargo fmt --check`/`cargo clippy --workspace --all-targets --keep-going`（72 条告警，未变）仍然干净。

**验证。** `cd rust && cargo fmt`（九个文件需要重新排版——这是私有 main 分支本就有的既存格式漂移，不是这次同步引入的）之后 `cargo build --locked --workspace` 成功。`cargo clippy --workspace --all-targets --keep-going`：73 条告警，与本仓库记录的基线相同（本次同步没有引入任何新告警）。`cargo test --locked --workspace`：**1033 通过，0 失败，10 忽略**（上面那处夹具修复之后；修复前的第一次跑，唯一的失败正好是那处路径夹具的测试；这个数字是安全加固那一轮之前的）。

## 2026-09-25 (daily sync): the architecture refactor lands in `main`, ledger v3, host-side entry typing, live fixed-sequence certification, and a batch of static checks / 架构重构合入 `main`：账本 v3、宿主入口分型、真机固定序认证上线，外加一批静态检查

This is the daily sync from the research workspace (`tools/sync-rust-from-research.sh`, `tools/sync-from-workspace.sh`), taking research-tree commit `85e28bfc` (2026-09-25). It supersedes the "pending, on a same-day branch" language in the previous `docs/status.md`: that branch is the one landing here, plus a full day of further work on top of it. `git log --oneline 9716e61b..85e28bfc -- 地基/rust-jpp` lists 161 research commits since the last rust sync (2026-09-24, `9716e61b`); this entry groups them by effect rather than listing each one.

**A crate rename that this sync's tooling had to be taught about.** Research-tree step 14a (ruling B74) split the interpreter out into its own crate (`jpp-runtime`: budget, bridge, construct evaluation, host builtins) and merged the old `jpp-core` (checker, effects, ledger façade) with `jpp-cli` (the binary) into a single crate, `jpp` (a lib target plus a bin target). `rust/` now has 10 crates: `jpp`, `jpp-calib`, `jpp-check`, `jpp-effects`, `jpp-ir`, `jpp-ledger`, `jpp-plan`, `jpp-runtime`, `jpp-syntax`, `jpp-value`. Two files exist only on the public side (a legacy-format calibration fixture directory and an overflow-assertion regression test) and had to move by hand from `crates/jpp-core/tests/` to `crates/jpp/tests/` before the sync tool's protect-list could find them again; `tools/sync-rust-from-research.sh` and `tools/sync-composition.py`'s KEEP paths are updated to match, and `-p jpp-cli` references in the CI workflow, `README.md`/`README.zh-CN.md`, `rust/METHODS-AND-LIFECYCLE.md` and `rust/scripts/doc_snippets.py` are now `-p jpp`. Full account of what moved and what stayed public-only: [`rust/PUBLIC-SNAPSHOT.md`](../rust/PUBLIC-SNAPSHOT.md).

**Ledger v3 and typed host entry.** The ledger format gained a `CalibUsed` entry kind, source-typed edges on `output_mat` (value-dependency vs. selection-dependency, so `J-02`'s static reachability check reads only the value-dependency projection -- ruling B92), and reserved fields for a calibration reference (`calib_ref.key`/`kind`/`fill`) that later steps fill in; a `ledger-migrate` subcommand converts v2 ledgers (checked against 36 archived v2 ledgers, replayed with no diffs). Program entry is now typed: a host passes named value entries and material entries (`Value::Mat`, tagged `origin=input`) instead of one untyped blob, `--input-trusted` lets a CLI caller declare an entry's trust bit explicitly (naming the flag in the resulting `J-08` runtime message when it matters), and `entry_hash` covers the whole typed entry rather than a loose value. Numeric promotion (integer/float mixing) landed as its own step with an explicit rule rather than an implicit cast.

**Fixed-sequence certification is live, not just designed.** The previous sync's `docs/status.md` described a fixed-sequence/sequential certification method (cutting the labeled-evidence threshold for a formally certified line from roughly 160 examples toward roughly 60) as "validated offline, not implemented in any branch." It is implemented now: research step 20h-1 built the B104/B87 revision (cut bandwidth never below a certificate's recorded delta; sequential candidates aligned to a shared ordering; a fingerprint check that keeps random arrival label-independent), and step 20i finished `alpha_eff` and the truth-value baseline (ruling B89) and put the fixed-sequence tier into formal service after an 80-row blind review with zero disagreements. A stand-in judge and a backend registry (step 15g) let a program swap which capability profile executes it without changing the program text; of the 8 tracked backend-swap hypotheses in `crates/jpp/tests/profile_swap.rs`, step 24d removed the `ignore` marker from 4 more (one, H2, stays deferred to a later step) on top of the 1 that already passed, an increase this sync can state with the caveat below.

**Budget, library synthesis and a batch of static checks.** Budget exhaustion now degrades at the next refresh point instead of halting the program outright (`B93`, step 22-0): calls stop being issued, but a program already in flight keeps running with the results it has, and the ledger records `budget` as the cause on entries it could not reach. Windowed dispatch and in-port concurrency (step 15e) let one backend port serve several judgments from the same refresh window concurrently instead of one call at a time. `tally`/`first_k` gained a shared synthesis path (`compose`, `element`, rulings B131-B133, step 25-2b) and their aggregate exits stopped counting as static-check release evidence (`step 25-1`, a same-day hotfix in the release-direction that a follow-on step, `25-9`, still owes a matching change to `releases()`). Seven static-check additions landed as steps 24a through 24g: two pending-output warnings under `J-06` (`B69`/`B111`), a `J-14` runtime face for multi-object crosstalk, two deferred `J-08` static-face items, four `J-04`/`J-03`/`J-09`/`J-14` untested-profile-field warnings, a `J-04` comparative-fingerprint static face plus a `J-11` unregistered-action-name face, and a `J-08` diagnostics-layer static consumer for shape mismatches (`B51`-R2). `calib-import --cost fp,fn` (step 20a-2a, ruling B129) lets an author certify an action-space cost line from labeled evidence, alongside the existing reading-space `declare` and certificate-only `alpha` forms.

**Ruled today, not yet in this sync's code.** Rulings B122 through B146 went into the reference texts (`12`, `20` v2, `21`, `19`) today, including the author-sovereignty and declared-line batch (B128-B130): an author may write `declare:{hi, lo?}` directly on a `cut` as a stated policy line -- used verbatim, never smoothed toward a certified threshold, never written into the calibration store -- and an irreversible `do` gated on a declared line requires the host to explicitly accept it (`--release-on-declared` / `EntryArgs.accept`, hashed into `entry_hash`, visible in the report) or the run stops with a fix-it message instead of silently downgrading. This is ruled and reasoned through in `地基/附注/2026-09-25-作者主权与策略表达裁定.md`, not code: the construction step that wires it in, 20j-1, is an active work-in-progress branch as of this sync and is not part of what landed in `rust/` today.

**Honest numbers, not all improved.** The expressiveness-ratio reading did move today, but by a measurement-method change, not a backend improvement: step 31-1b (ruling B96) replaced point-based T1 grading with a held-out, property-based acceptance check and re-ran it against a second baseline set per implementation (9 of 10 new baselines passed on the first acceptance run). The resulting reading is T1 3.69x counting only implementations that pass the new acceptance check, 4.48x counting all implementations, and 4.97x under the old frozen grading kept for comparison (`t1-strict`) -- still well below the project's 9x-20x reference band, and not directly comparable to the `~4.1x`/`~3.4x` figures the previous `docs/status.md` carried from the pending branch, because the acceptance method under them changed. The backend-swap dashboard item (`item_profile_swap` in `地基/rust-jpp/scripts/dashboard.py`) reads 1 of 8 tracked hypotheses fully passing as of the last dashboard run recorded on the blackboard (step 15g); step 24d's un-ignoring of 4 more hypothesis groups (above) had not been re-run through the dashboard script as of this sync, so this entry does not claim a new fraction without rerunning it -- see verification below. Depth evidence is still fixed-observation hop counts only (22/12/4 judgments at hop one/two/three); a live depth curve is not measured yet.

**Verification.** `cd rust && cargo fmt` (two public-only files needed reformatting after the port and the crate rename -- `crates/jpp/tests/known_defects.rs`, `crates/jpp/tests/wiring.rs`) then `cargo build --locked --workspace` succeed. `cargo test --locked --workspace`: **884 passed, 0 failed, 9 ignored**. `crates/jpp/tests/known_defects.rs` needed a second, manual fix beyond the automatic port: it imported `jpp_core::*` and called the pre-14a `run(&program, &mut client, ...)` signature; both are now `jpp::*` and `run(&program, fixed.ports(), ...)` against `FixedPorts` (`FixedClient` itself was retired in an earlier, already-synced step, 15c). The full test run surfaced a third gap the port script's own checks missed: `crates/jpp-effects/src/profile.rs`'s inline unit test still joined the pre-rename `foundation/profile/profiles/...` path, because the tool's path-rewrite loop only scanned `crates/*/tests/*.rs`, not a crate's `src/` tree, where this particular test happens to live inline. Fixed both the test file and the tool (`tools/sync-rust-from-research.sh` now scans `crates/*/{src,tests}/**/*.rs`), and re-ran `cargo test -p jpp-effects --lib --offline` to confirm (16 passed, 0 failed) rather than repeating the full workspace run. To reproduce the backend-swap count claimed above, run `cargo test -p jpp --test profile_swap --offline -- h --test-threads=2` from `rust/` and count `... ok` groups the way `item_profile_swap()` does in `地基/rust-jpp/scripts/dashboard.py`.

这是从研究工作区做的每日同步（`tools/sync-rust-from-research.sh`、`tools/sync-from-workspace.sh`），取研究树提交 `85e28bfc`（2026-09-25）。它取代了此前 `docs/status.md` 里"在同日分支上待合入"的说法——那个分支连同它之后一整天的后续工作，今天一起进了这里。`git log --oneline 9716e61b..85e28bfc -- 地基/rust-jpp` 显示自上次 rust 同步（2026-09-24，`9716e61b`）以来研究树有 161 个提交；这里按效果分组叙述，不逐条列出。

**一次连累同步工具的改名。** 研究树步 14a（B74 裁定）把解释器独立成一个新 crate（`jpp-runtime`：预算、桥、构造求值、宿主内置函数），并把原 `jpp-core`（检查器、效应、账本外观）与 `jpp-cli`（二进制）合并为单个 crate `jpp`（lib 目标 + bin 目标）。`rust/` 现在是 10 个 crate：`jpp`、`jpp-calib`、`jpp-check`、`jpp-effects`、`jpp-ir`、`jpp-ledger`、`jpp-plan`、`jpp-runtime`、`jpp-syntax`、`jpp-value`。两个只在公开侧存在的文件（旧格式校准夹具目录、一条溢出断言回归测试）先手工从 `crates/jpp-core/tests/` 搬到 `crates/jpp/tests/`，同步工具的保护名单才能重新认得它们；`tools/sync-rust-from-research.sh` 与 `tools/sync-composition.py` 的 KEEP 路径已同步改过，CI 工作流、`README.md`/`README.zh-CN.md`、`rust/METHODS-AND-LIFECYCLE.md`、`rust/scripts/doc_snippets.py` 里的 `-p jpp-cli` 全部改成 `-p jpp`。搬了什么、什么只留在公开侧，完整记录见 [`rust/PUBLIC-SNAPSHOT.md`](../rust/PUBLIC-SNAPSHOT.md)。

**账本 v3 与类型化的宿主入口。** 账本格式加了 `CalibUsed` 条目种类、`output_mat` 的来源边带上种类（值依赖 / 选择依赖两种，`J-02` 的静态可达性检查因此只读值依赖投影——B92 裁定）、以及给后续步骤填的校准引用留位字段（`calib_ref.key`/`kind`/`fill`）；新增 `ledger-migrate` 子命令做 v2 账本迁移（对 36 份归档的 v2 账本核对过，重放零差异）。程序入口现在分了型：宿主传入具名的值条目与材料条目（`Value::Mat`，标 `origin=input`），不再是一整团无类型数据；`--input-trusted` 让 CLI 调用方显式声明某条入口的可信位（在相关的 `J-08` 运行期报文里点名这个开关）；`entry_hash` 现在覆盖整份类型化入口，不再只是一个零散值。数值提升（整数/浮点混算）作为独立一步落地，有明确规则而非隐式转换。

**固定序认证已经真机上线，不只是设计。** 上一次同步的 `docs/status.md` 把固定序/序贯认证方法（把正式档门槛从约 160 条标注压到约 60 条左右）描述成"离线验证过、任何分支都没实现"。现在实现了：研究树步 20h-1 造出 B104/B87 修订（判区带宽不小于证书记录的 δ；序贯候选对齐同一顺序；指纹检查保证随机到达与标签无关），步 20i 完成 `alpha_eff` 与真值基准（B89 裁定），80 条盲复核零分歧后把固定序档正式推上岗。替身判断器加后端注册表（步 15g）让程序换一个能力画像执行、程序文本不用改；`crates/jpp/tests/profile_swap.rs` 里追踪的 8 条换后端假设中，步 24d 在此前已过的 1 条之上又摘掉 4 条的 `ignore`（其中 H2 仍推迟到后续步骤）——这个增量本条目带着下面的保留说明一起写。

**预算、库层合成与一批静态检查。** 预算耗尽现在在下一个刷新点降级，不再直接停机整个程序（`B93`，步 22-0）：不再发新调用，但已经在跑的程序继续用手头已有的结果往下走，账本对够不到的条目记 `budget` 作缺席原因。按窗口发出与端口内并发（步 15e）让一个后端端口能同时服务同一刷新窗口里的几个判断，而不是一次一个。`tally`/`first_k` 有了共用的合成路径（`compose`、`element`，B131–B133 裁定，步 25-2b），它们的聚合出口不再算作静态检查的放行证据（步 25-1，当天的放行方向热修，后续步骤 25-9 还欠 `releases()` 的配套改动）。七项静态检查以步 24a 到 24g 落地：`J-06` 下两条未决输出告警（`B69`/`B111`）、`J-14` 运行期面处理多对象串扰、两处推迟的 `J-08` 静态面、四条 `J-04`/`J-03`/`J-09`/`J-14` 画像字段未测告警、`J-04` 比较性指纹静态面加 `J-11` 未登记动作名静态面、以及 `J-08` 诊断层对形状不匹配的静态消费者（`B51`-R2）。`calib-import --cost fp,fn`（步 20a-2a，B129 裁定）让作者能从标注证据认证一条动作空间的代价线，与既有的读数空间 `declare` 和只选证书的 `alpha` 两式并列。

**今天裁定了，但还没进这次同步的代码。** B122 到 B146 一批裁定今天写进了依据文本（`12`、`20` v2、`21`、`19`），包括作者主权与声明线一批（B128–B130）：作者可以直接在 `cut` 上写 `declare:{hi, lo?}` 作为明说的策略线——按写的数字原样用，不向认证阈值平移或取严，也不写进校准库；放行不可逆 `do` 的声明线需要宿主显式接受（`--release-on-declared` / `EntryArgs.accept`，进 `entry_hash`、报告里看得见），不接受就在 `check`/`release` 处停下并给出修法提示，不会静默降级。这些论证与裁定写在 `地基/附注/2026-09-25-作者主权与策略表达裁定.md`，还不是代码：把它接进去的施工步 20j-1，在本次同步时还是一个进行中的 worktree 分支，没有进入今天 `rust/` 里落地的内容。

**如实的数字，不是全都变好了。** 表达量比读数今天确实变了，但变的是量法，不是后端效果：步 31-1b（B96 裁定）把 T1 的逐点打分改成留出集、按性质验收，并对每个实现重新跑了第二套基线（十份新基线里九份第一次验收就过）。得到的读数是：只计通过新验收方法的实现 3.69×，计入全部实现 4.48×，按旧冻结打分法（留作对照的 `t1-strict`）4.97×——仍远低于项目 9–20× 的参考带，也不能直接拿来和此前 `docs/status.md` 从待合并分支里带的 `约 4.1×`/`约 3.4×` 相比，因为两者背后的验收方法本身变了。换后端仪表项（`地基/rust-jpp/scripts/dashboard.py` 的 `item_profile_swap`）按黑板记录的最近一次仪表读数（步 15g）是 8 条追踪假设里 1 条全过；步 24d 对另外 4 组假设摘掉 ignore（见上）之后，本次同步前没有重新跑过仪表脚本，所以这条不在没有重新验证的情况下声称新的分数——复现方法见下面「验证」。深度证据仍只有固定观察下的跳数分布（一/二/三跳 22/12/4 个判断），真机深度曲线还没有测量。

**验证。** `cd rust && cargo fmt`（两个只在公开侧的文件在改写和改名之后需要重新排版——`crates/jpp/tests/known_defects.rs`、`crates/jpp/tests/wiring.rs`）之后 `cargo build --locked --workspace` 成功。`cargo test --locked --workspace`：**884 passed, 0 failed, 9 ignored**。`crates/jpp/tests/known_defects.rs` 除了自动改写还需要一处手工修：它原来 `use jpp_core::*` 并调用 14a 之前的 `run(&program, &mut client, ...)` 签名；两处现在都改成 `jpp::*` 与针对 `FixedPorts` 的 `run(&program, fixed.ports(), ...)`（`FixedClient` 本身在更早、已经同步过的步骤 15c 里就退役了）。全量测试还揪出同步工具自己的检查没盖到的第三处：`crates/jpp-effects/src/profile.rs` 里一处内嵌单元测试仍拼着改名前的 `foundation/profile/profiles/...` 路径——工具的路径改写循环此前只扫 `crates/*/tests/*.rs`，扫不到这个测试实际所在的 crate `src/` 树。已经改了这个测试文件，也改了工具本身（`tools/sync-rust-from-research.sh` 现在扫 `crates/*/{src,tests}/**/*.rs`），改完后跑 `cargo test -p jpp-effects --lib --offline` 确认（16 通过、0 失败），没有为此再跑一遍全量。要复现上面提到的换后端计数，在 `rust/` 下跑 `cargo test -p jpp --test profile_swap --offline -- h --test-threads=2`，按 `地基/rust-jpp/scripts/dashboard.py` 里 `item_profile_swap()` 的算法数 `... ok` 组。

## 2026-09-25 (later): real-source rerun complete, $1.43 spent, all 10 pre-registered predictions confirmed / 真实来源重跑完成，花费 $1.43，10 条预注册预测全部命中

Closes out the entry directly below, which described the abstract trim without a rerun. Three more things happened after it, in order: the generic "The paper is about &lt;field&gt;." sentence was replaced with 40 hand-written, &lt;=25-word per-paper descriptions (`scripts/towow_real_paper_descriptions.json`, read by `scripts/trim_towow_real_abstracts.py`, not embedded in the script); a falsifiable prediction was pre-registered and committed (`docs/towow-real-rerun-preregistration-2026-09-25.md`) before any paid call; then the full pipeline ran live.

**Descriptions, finalized once.** 40 distinct paper titles across the 178 academic entries each got one hand-written English sentence describing what that paper does, paraphrased from (not copied out of) the original abstract -- checked for 6-word verbatim overlap with the source text; the only overlaps found were unavoidable technical-term phrases (e.g. "genomic foundation models trained on DNA sequences"), not lifted prose. This was treated as the last wording pass before spending, since any further edit to these 178 contexts invalidates the JEV request cache again.

**Pre-registration, committed before spend.** Built on `research/地基/DECISIONS.md:1005`'s hypothesis (removing shared-abstract text weakens `coauthor_same_field`/`github_same_org` recall) but went further by inspecting the actual data first: all 599 `coauthor_same_field` edges connect people who share an *exact, verbatim-preserved* paper title (title trimming was never part of this change), so the prediction diverged from the blanket framing -- this relation type was predicted to hold steady, not drop, because its strongest signal survives untouched. `github_same_org` (307 edges) and `same_team_*` (33 edges) connect endpoints whose text this edit never touched at all, predicted to stay within baseline noise. `cross_source_*` (24 edges) was already at a 0/24 floor pre-trim. Ten falsifiable numeric ranges were committed for `order20` (the page's default method) at K=1/5/10/20 plus `source_backed`, each with an explicit line stating what result would falsify it.

**The rerun.** Local retrieval first (`benchmark_towow_real.py`, BM25 + MiniLM + RRF fusion, 0 API calls): a confound check reran it with the exact recorded environment (torch 2.14.0, sentence-transformers 6.1.0, transformers 5.17.0, MiniLM revision `e8f8c211…`) against the *pre-trim* data and reproduced the published `retrieval.json`'s rankings identically (325/325 for all three methods) -- confirming zero environment drift, so the rerun's numbers reflect the data change alone. Then the same script ran against the trimmed data and its output became the new `retrieval.json`. Then live JEV: `cut20` ($0.080) -> full-pairwise `cut324` ($1.165 more, cumulative $1.245) -> `order20` ($0.150 more, cumulative $1.395) -> `exploration` ($0.039 more, cumulative **$1.4336** of the $2.00 cap authorized for this rerun). Credentials came only from `~/.typesafe-key`, read by the existing client code; verified afterward that no output or recording contains the key or an `Authorization` header. Live-call concurrency was temporarily capped at `max_workers=4` (down from this program's usual 8 for cut mode, 32 for order/explore) after the machine hit a resource limit mid-task from unrelated concurrent load; the two source files were reverted to their committed state immediately after the live calls finished (clean `git diff`).

**All 10 predictions confirmed, none falsified.** `order20` @ K=10: `coauthor_same_field` recall 0.8965 (predicted 0.83-0.95); `github_same_org` 0.3974, an *identical* hit count (122/307) to the pre-trim baseline, exactly as predicted for an untouched relation type; `same_team_*` 1.0000 and `cross_source_*` 0.0000, both as predicted. Overall: undirected recall@10 0.7186 (predicted 0.68-0.76), directed@10 0.6153 (predicted 0.57-0.66), undirected @1/@5/@20 0.1859/0.5306/0.8619 (all within the committed +/-0.05 band of baseline), `source_backed`@10 0.7369 (predicted 0.71-0.79). The specific divergence from `DECISIONS.md:1005`'s framing -- that `coauthor_same_field` would hold steady because the shared title, not the abstract, carries the signal -- held.

**Published and verified.** `results.json`, `reports.json.gz`, `recording.jsonl.gz` (rebuilt from the live run, 9,987 records), `exploration.json`, `exploration-recording.jsonl.gz` (the 1,095 records the exploration step added, extracted from the shared journal by line offset so the main run's records aren't duplicated into it), `diagnostics.json` (regenerated locally, 0 API calls). A grep for the old abstract text across every new artifact, including inside the gzipped recordings, found zero hits. All four `data_sha256` fields (`results`/`retrieval`/`diagnostics`/`exploration`) match the current `data.json`. `real/index.html`'s privacy notice now says the rankings and recordings are current as of this rerun. Full test suite: **561 passed, 0 failed** (up from 560 passed / 1 known failure before this rerun) -- `test_real_source_public_results_recompute_from_rankings` passes now that `results.json` matches `data.json`.

[Rerun commit / 重跑提交](https://github.com/Towow-ai/jpp/commit/35bd350), [40-description commit](https://github.com/Towow-ai/jpp/commit/7bc65aa), [retrieval + build-script commit](https://github.com/Towow-ai/jpp/commit/5618aaa), [pre-registration commit](https://github.com/Towow-ai/jpp/commit/d8d180d), [PR #33](https://github.com/Towow-ai/jpp/pull/33)

## 2026-09-25: real-source demo profiles - abstracts trimmed, source disclosed, rerun not done / 真实来源演示资料：删除摘要、公开来源，重跑未做

**Decision this responds to.** The 325 real-source profiles in `docs/demos/towow/real/data.json` (academic, GitHub and YC founder records) stay on public sources, are not treated as de-identified, and their git history is not rewritten. Two things changed going forward instead: the paper abstracts in the academic entries are trimmed, and the demo page states plainly where the data comes from and how to ask for removal.

**What changed.** In `data.json`, all 178 academic-source `context` fields had their paper abstract (the long passage after the title) removed; the institution line, the paper title and the trailing OpenAlex concept tags are kept verbatim, and the abstract is replaced with one short "The paper is about &lt;field&gt;." sentence using the field already named in that entry. The other 147 GitHub/startup records, and every top-level field (`schema`, `known_relations`, `provenance`, `scope`), are untouched -- confirmed with a full structural diff, not just a visual check. `real/index.html` now carries a bilingual notice: public sources (OpenAlex paper/author records, GitHub, YC founder pages), names replaced by IDs, real signals such as shared papers/institutions kept on purpose for the J++ matching demo, and a GitHub-issue path to request removal.

**What a repo-wide grep for anonymization language turned up.** `git grep -niE 'anonym|de-?identif|去标识|去识别|脱敏|匿名|identifier'` across all tracked files found no public-facing claim that this dataset is anonymized or de-identified beyond ordinary uses of "identifier" as a language term and one already-correct disclaimer in `data.json`'s own `scope` field ("not anonymous"). The private research-tree mirror under `research/地基/` (confirmed via `tools/sync-from-workspace.sh`, which copies from `~/个人项目/jev/地基` and is not edited from this repo) has three stale spots worth closing out at the source, in `jev/地基/` itself, not here: `待Nature裁定清单.md:62` and `14-实施计划-把语言做完整-v1.md:88` both still describe the de-identification question as undecided even though it was decided 2026-09-25, and `DECISIONS.md:1002` records a since-superseded "去标识提案完成" (de-identification proposal) framing that a reader could mistake for the final state. None of these are edited here since the next sync from the private workspace would overwrite an in-place fix; they need either a closing note or a `<!-- 公开替换 -->` marker at the source, then a resync.

**Title-integrity check on all 178 trimmed entries, not just a sample.** The extraction regex stops at the first `'.` after `includes '`, which would silently truncate a title containing that exact substring. Checked all 178: 40 distinct titles, all read complete against the source data, none contain an internal `'.`; and for every entry, confirmed the old (pre-trim) text right after the captured title actually starts the abstract (checked for an uppercase letter or "Background:" immediately following -- the only four apparent exceptions were abstracts starting with the digit "4" in "4D-STEM", verified by hand as correct, not truncated).

**What did not run: the demo results.** `results.json`, `reports.json.gz`, `recording.jsonl.gz`, `exploration.json` and `exploration-recording.jsonl.gz` are untouched and still reflect the pre-trim text. Every JEV request is cache-keyed by a hash of its exact input text (`fingerprint()` in `towow.py`), so trimming 178 of 325 profiles invalidates essentially the whole cache -- a real rerun needs live, paid JEV calls, which this change does not make. Two known consequences, left as-is on purpose: `tests/test_towow_public_artifacts.py::test_real_source_public_results_recompute_from_rankings` now fails because `results.json`'s recorded `data_sha256` no longer matches the trimmed `data.json` (confirmed both locally and in this PR's CI: the `offline (3.12)`/`offline (3.13)` jobs on PR #33 fail with exactly this one assertion, 1 failed / 560 passed; `rust-checks` and `rust-source` pass); and `recording.jsonl.gz` / `exploration-recording.jsonl.gz` still contain the full old abstract text in their recorded request bodies until a rerun happens, which `real/index.html`'s new notice now says explicitly. **PR #33 should not merge silently with this failure; pick one of: (a) full rerun, (b) reduced rerun, (c) merge knowingly with this one test red.** The live-call spend either (a) or (b) needs is within the project's standing $5-without-asking threshold, so the coordinating session can pick one directly rather than escalating to Nature; this task's own instructions were the specific reason no call was made without reporting the cost first. One thing to weigh for (c): once merged, `main`'s CI shows this one `offline` job red for every other agent's PR until a rerun lands, so (c) should come with a scheduled (a) or (b), not sit indefinitely.

**Rerun cost, not spent, with the prerequisites this estimate depends on.** Based on the existing recorded run: the full real-source pipeline (`cut20` + the full-pairwise `cut324` + `order20` + the `exploration` cross-source variant) previously cost about $1.49 cumulative for roughly 11,100 recorded requests, of which the full-pairwise `cut324` variant (105,300 judge sub-questions across all 324 candidates per person) is about $1.21 of that by itself. Trimmed text is about 16.7% shorter corpus-wide (25% shorter for the 178 academic entries: 77,774 to 58,297 characters); treat that as an upper bound on the discount, since prompt and question text are unchanged and only the profile text shrank, so a rerun would likely cost at or somewhat below these historical figures. Three things any rerun needs first, none of them optional: (1) `towow_real.py` hard-asserts the retrieval file's `data_sha256` matches `data.json` before it will run at all, so local retrieval (`benchmark_towow_real.py`: BM25 + MiniLM + RRF fusion, itself free, zero API calls) must be regenerated first -- installing `torch`/`sentence-transformers` and the MiniLM model weights (revision `e8f8c211…`) is free and local, it simply was not done here because the next step after it is the paid one this task's instructions said to stop before; (2) the "skip `cut324`" cheaper path is not a drop-in flag -- `build_towow_real.py` hardcodes `report-324.json` and the `cut324` method, and `real/index.html`'s "finding" sentence names it, so that option means editing the build script and the page copy, not just running fewer commands; `exploration.json` (~$0.04) stays cheap even after a rerun, since `towow_explore.py --journal` replays from whichever `order20` recording it is pointed at and only the ~2,172 genuinely cross-source questions need to go live -- with it included, that reduced path is roughly $0.27-0.28 against the historical baseline, for about 6,700 requests. (3) Per this project's own experiment discipline (地基 §5.2), the system-level hypothesis to pre-register before spending is already on record in this same public repository's `research/地基/DECISIONS.md:1005` (synced from the private workspace, not private itself): removing the shared-abstract text is expected to weaken exactly the `coauthor_same_field` and `github_same_org` relation-recovery signal, since those labels were largely inferred from the now-removed shared paper/employer text. A rerun should commit that prediction before looking at the new numbers, not after.

**The live public site is still serving the untrimmed data today.** GitHub Pages for this repository serves from `main:/docs` (confirmed via `gh api repos/Towow-ai/jpp/pages`). Until PR #33 merges, `https://towow-ai.github.io/jpp/demos/towow/real/` continues to serve the pre-trim `data.json` with full abstracts. Merging with the one known test failure (option (c) in the PR) is therefore the only option that removes the abstracts from the live site today; options (a)/(b) additionally require the paid rerun above first. This is within the project's standing $5-without-asking threshold (地基 §5.2), so the coordinating session can pick one directly rather than waiting on Nature.

[Implementation commit / 实现提交](https://github.com/Towow-ai/jpp/commit/b6738fa), [checked-in trim script](https://github.com/Towow-ai/jpp/commit/bb4a59a), [progress-log commits](https://github.com/Towow-ai/jpp/commit/1c1a22e) / [4592f18](https://github.com/Towow-ai/jpp/commit/4592f18) / [43b076e](https://github.com/Towow-ai/jpp/commit/43b076e), [PR #33](https://github.com/Towow-ai/jpp/pull/33)

## 2026-09-24: PRs #27-30 merged; new findings on whether the language has an effect yet / PR #27–30 合入；「有没有效果」的新发现

**What landed in this public repository today.** PRs [#27](https://github.com/Towow-ai/jpp/pull/27), [#28](https://github.com/Towow-ai/jpp/pull/28), [#29](https://github.com/Towow-ai/jpp/pull/29) and [#30](https://github.com/Towow-ai/jpp/pull/30) merged into `main` in dependency order; #30 itself includes two follow-up fixes from its Codex review (retry-billing correctness, and making the cost-reporting branch read the calibration record that was actually selected rather than an assumed one). `cargo test --workspace --offline` on the resulting `main`: **394 passed, 0 failed, 3 ignored**. This closes out everything summarized in the three 2026-09-23 entries below (rule batch, value-level taint, live backend, template-level calibration): those are in `main` now, not just synced from the research tree.

**A same-day architecture refactor, on a sync branch pending review.** The research tree spent today splitting its Rust kernel from 3 crates into 10 (`jpp-ir`, `jpp-value`, `jpp-effects`, `jpp-ledger`, `jpp-calib`, `jpp-check`, `jpp-plan`, `jpp-core`, `jpp-syntax` -- renamed from `jpp-frontend` -- and `jpp-cli`; the ruled cap is 11), rebuilding the intermediate representation and the ledger format, and adding structural provenance tracking (which judgment produced which downstream value, and how many chained layers deep a result sits). The code is on the same-day sync branch `sync/2026-09-24-architecture`, synced through research-tree commit `9716e61b` (steps 0 through 12e-2 of the refactor sequence, plus the interleaved steps 7b, 7c, 9a, 13a, 13b, 15d-0, 17a, 17c, 20b, 20d-1, 20e and 20f, the verification-dashboard scripts and `probes/`, and a new `GUIDE.md`); `cargo test --locked --workspace` on that branch: 579 passed, 0 failed, 3 ignored. It is waiting on review before being pushed and opened as a pull request; it is not yet in this repository's `main`. Anything below that names a crate, a step number, or a research-tree test count describes that branch.

**That branch's CI.** A new `rust-checks (report)` job runs the subset of the research tree's `scripts/ci.sh` that this public repository can run without a live backend -- formatting, lint, the dependency table, line-count and pattern checks, capability-profile cross-checks, and the equivalent-rewrite call-count pairs -- plus an actual run of the code fragments in `GUIDE.md`, `README.md` and `METHODS-AND-LIFECYCLE.md` and all bundled examples. It reports rather than blocks. First local run: 13 of 13 checks exited 0; of the documentation code fragments and examples, 40 passed, 1 did not, and 6 lines were skipped because they need the live backend. The one failure is `METHODS-AND-LIFECYCLE.md` still telling readers to run `cargo test -p jpp-frontend`, a crate that this refactor renamed to `jpp-syntax`. Two checks read above their recorded baseline: `grep_constants` at 140 (baseline 132) and `cargo fmt --check` flagging 75 files (baseline 74). All of this is on the pending sync branch, not in `main`.

**The finding that shaped today's design work.** An independent review of the research tree, timed right after the refactor's first milestone, found that the kernel's semantics hold up under controlled tests, but on the real JEV backend, every new question came back undecided -- zero decided outcomes, because no question had a calibrated threshold yet. The review also found the project's own fallback behavior (when a run has no capability profile) silently deciding outcomes through hardcoded constants, which the project's own design rules forbid. Three pieces of design work responded to this, each written up separately since each stands on its own:

- **A seven-item verification dashboard, and a fix to how the "how many times shorter" number is measured.** The project's target reference band (9x-20x shorter than hand-written code) turned out to apply only when the hand-written baseline also has to implement its own audit log, spending cap, request batching and calibrated threshold -- duties the project's early test tasks let the baseline skip. Task comparisons are now run in two tiers (a minimal brief and a fuller one that requires those four duties), and the line-count comparison itself is now normalized for line-wrapping width instead of counted raw, since J++ source runs measurably denser per line than the Python baselines. **Built, on the pending sync branch: the dashboard scripts. A first two-tier multi-implementation measurement has run in the research workspace.** The first reading under this rule: T1 (the deciding tier), wrap-normalized median 4.97x; T0, reported alongside, 1.36x. Both are below their reference bands (9x-20x for T1, 2.7x-4.3x for T0). The same day's independent diagnosis (ruling B96) traced about 0.9x-1.6x of the T1 figure to the measurement itself: the T1 task brief required the baseline's certification line (duty (d)) to reproduce `jpp calib-import` bit for bit, which raised the ratio by about 0.9x, and three J++ implementations that failed acceptance were still counted, which raised it by about 0.7x more. With both corrected, the T1 ratio is about 4.1x over all implementations and about 3.4x counting only implementations that pass acceptance. A re-measurement under the corrected rule (step 31-1b) is in progress. Other readings: probe comparisons at 2x-5x, two programs written during the review at 1.2x-1.5x, a live decided-exit share for new questions of 0.255 (14 of 55, counting earlier runs that were all cold), and a hop distribution of 22/12/4 judgments at one/two/three chained layers from fixed observations -- with these in, all seven dashboard items now have numbers. See [dashboard and expressiveness reference](updates/2026-09-24-dashboard-and-expressiveness-reference.md).
- **A "trial" calibration tier that unblocks the live backend.** Formal certification needed about 160 labeled examples; a new looser tier certifies at a wider (but still statistically bounded) error tolerance from far fewer, enough to route a program's control flow but explicitly barred from authorizing irreversible actions. **Built, on the pending sync branch**, along with per-exit reporting of which grade of calibration line backed each outcome and a rule requiring a capability profile on every live run. A live test with 80 constructed-truth customer-service dialogues got every outcome decided for the first time and reached a second chained judgment layer live for the first time, for under a fifth of a cent. A self-review caught and fixed a real gap before it shipped: trial recertification could have silently overwritten an already-suspended formal line. See [live trial-grade calibration lines](updates/2026-09-24-live-trial-lines.md).
- **Cutting the 160-example labeling threshold itself, without loosening the error bound.** Breaking the number apart found that roughly half the gap above the statistical minimum was an accident of a fixed random seed unevenly splitting the sample pool, not a real requirement. A different, literature-standard certification method (checking a fixed sequence of candidate thresholds against the same labeled sample instead of splitting it) was validated for free against all existing labeled data and cuts the requirement to about 60 examples. **Ruled, not built:** this method change, a companion sequential variant, and a fix to a separate specification gap in how a certified line's scope may be extended are written decisions, not yet implemented in the calibration code; live runs today still use the older, more expensive method. See [cutting the labeling threshold](updates/2026-09-24-labeling-threshold.md).

**State of the three acceptance criteria** ("shorter to write," "how deep a judgment chain can run," "swap the judge, program doesn't change"): expressiveness has partial evidence at 2x-5x on isolated probe comparisons, short of the 9x-20x target; the first T0/T1 multi-implementation reading is T1 4.97x and T0 1.36x, both below their reference bands; the same day's diagnosis (ruling B96) traced about 0.9x-1.6x of the T1 figure to the measurement, and the corrected T1 ratio is about 4.1x over all implementations and 3.4x over those that pass acceptance, with a re-measurement (step 31-1b) in progress. Depth evidence is a hop distribution from fixed observations: 22, 12 and 4 judgments at hops one, two and three; the per-hop undecided rate and the live depth curve are not measured yet. Capability-swap testing has exercised one of eight tracked capability assumptions.

---

**今天在这个公开仓库里合入的东西。** PR [#27](https://github.com/Towow-ai/jpp/pull/27)、[#28](https://github.com/Towow-ai/jpp/pull/28)、[#29](https://github.com/Towow-ai/jpp/pull/29)、[#30](https://github.com/Towow-ai/jpp/pull/30) 按依赖顺序合入 `main`；#30 本身就包含它评审后的两处后续修复（重试计费的正确性；让代价上报分支读取实际被选中的校准记录，而不是假定的那条）。合入后的 `main` 上 `cargo test --workspace --offline`：**394 通过、0 失败、3 忽略**。这收尾了下面 2026-09-23 三条条目里总结的全部内容（规则批、值级 taint、真实后端、题式级校准）——这些现在已经在 `main` 里，不只是同步自研究树。

**同一天的架构重构，在待审核的同步分支上。** 研究树今天把 Rust 内核从 3 个 crate 拆成 10 个（`jpp-ir`、`jpp-value`、`jpp-effects`、`jpp-ledger`、`jpp-calib`、`jpp-check`、`jpp-plan`、`jpp-core`、由 `jpp-frontend` 改名的 `jpp-syntax`、`jpp-cli`；已裁定的上限是 11 个），重建中间表示与账本格式，并加入结构化来源追踪（哪次判断产出了哪个下游值、一个结果链式地叠了几层）。代码在同日同步分支 `sync/2026-09-24-architecture` 上，同步到研究树提交 `9716e61b`（重构序列步 0 到 12e-2，加上交错插入的步 7b、7c、9a、13a、13b、15d-0、17a、17c、20b、20d-1、20e、20f、验收仪表脚本与 `probes/`、新增的 `GUIDE.md`）；该分支上 `cargo test --locked --workspace`：579 通过、0 失败、3 忽略。它在等待审核后推送并开 PR，还没有进本仓库 `main`。下文提到的 crate 名称、步骤编号或研究树测试数，说的都是这个分支。

**这个分支上的 CI。** 新增的 `rust-checks (report)` 作业跑研究树 `scripts/ci.sh` 里公开仓库不用真机也能跑的子集——格式、lint、依赖表、行数与模式类检查、能力画像核对、等价写法调用数对——并实际跑 `GUIDE.md`、`README.md`、`METHODS-AND-LIFECYCLE.md` 里的代码片段与全部随附示例。报告模式，不拦截。本地首跑：13 项检查全部退出 0；文档代码片段与示例里，40 项通过、1 项未通过、6 行因为需要真机而跳过。未通过的那一项是 `METHODS-AND-LIFECYCLE.md` 仍写着 `cargo test -p jpp-frontend`，而这次重构已经把这个 crate 改名为 `jpp-syntax`。两项读数高于记录的基线：`grep_constants` 140（基线 132）、`cargo fmt --check` 标出 75 个文件（基线 74）。以上全部在待合入的同步分支上，不在 `main` 里。

**今天设计工作的起点。** 一轮针对研究树重构第一个里程碑之后的独立复核发现：内核语义在受控测试下成立，但接上真实 JEV 后端后，任何新题的出口都是未决——已决出口是零个，因为还没有题有校准阈值。复核还发现项目自己的兜底行为（运行没有能力画像时）正在悄悄用硬编码常数决定出口，这正是项目自己的设计规则明令禁止的事。三项设计工作对此做出回应，各自成篇，因为各自独立成立：

- **一套七项验收仪表，以及对「短多少倍」这个数字量法的修正。** 项目的目标参考带（比手写代码短 9–20 倍）原来只在手写基线也要自己实现审计日志、花费上限、请求合批和校准阈值——这些是项目早期测试任务允许基线跳过的职责——时才适用。任务对照现在分两档运行（一份最小任务书，一份要求这四件事的完整任务书），行数对照本身也改成按折行宽度归一后再数，而不是数原始行数，因为 J++ 源码每行的密度明显高于 Python 基线。**已造出，在待合入的同步分支上：仪表脚本。第一次两档多实现测量已在研究工作区跑过。**按这条规则的第一次读数：T1（判定档）折行归一中位 4.97×，T0（并列报告）1.36×，都低于各自的参考带（T1 对 9–20×，T0 对 2.7–4.3×）。当天的独立诊断（裁定 B96）查出 T1 读数里约 0.9–1.6 倍来自量法本身：一是 T1 任务书要求基线的认证线（职责 (d)）按 `jpp calib-import` 逐位复刻，使比值抬高约 0.9；二是三份没通过验收的 J++ 实现也计入了，又抬高约 0.7。两处都纠正后，全部实现约 4.1×，只算通过验收的约 3.4×。按新口径的重测（步 31-1b）正在进行。其余读数：孤立探针对照 2–5 倍、评估时试写的两个程序 1.2–1.5 倍、真机新题已决出口占比 0.255（14/55，含此前全冷的运行）、固定观察下一/二/三跳的判断数 22/12/4——加上这些，仪表七项现在全部有数。见[验收仪表与表达量参考系](updates/2026-09-24-dashboard-and-expressiveness-reference.md)。
- **一档解除真机阻塞的「试用」校准等级。** 正式认证原来要约 160 条标注；新增的更松等级用更宽（但仍受统计边界约束）的错误容忍、少得多的样本即可认证，够给程序控制流分派路由，但明确不许用来放行不可逆动作。**已造出，在待合入的同步分支上**，随附逐出口记录背后校准线等级的报告，以及真机运行必须带能力画像的规则。一次用 80 段构造真值客服对话的真机测试第一次让全部出口都得到已决结果，也第一次在真机上走到链式判断的第二层，花费不到千分之二美元。上线前的一次自查抓到并修复了一个真实缺口：试用档的重新认证本可能悄悄覆盖一条已停岗的正式线。见[真机试用档校准线](updates/2026-09-24-live-trial-lines.md)。
- **把 160 条标注门槛本身降下来，安全边界不放松。** 把这个数字拆开后发现，超出统计最小值之上的差距里大约一半是固定随机种子把样本池切得不均匀造成的意外，不是真正的要求。一种不同的、文献里的标准认证方法（在同一批标注样本上检验一个固定的候选阈值序列，而不是切分样本）用项目已有的全部标注数据做了零成本离线验证，把门槛降到约 60 条。**已裁定，未造出：** 这项方法改动、一个配套的序贯变体，以及另一处「已认证线扩展适用范围」规格缺口的修法，都是已写下的决定，还没有在校准代码里实现；今天的真机运行仍在用较旧、更费样本的方法。见[压低标注门槛](updates/2026-09-24-labeling-threshold.md)。

**三条验收标准的现状**（「写得更短」「判断链能跑多深」「换判断器程序不改」）：表达量在孤立探针对照上有 2–5 倍的部分证据，还没到 9–20 倍的目标；按 T0/T1 做的第一次多实现测量读数是 T1 4.97×、T0 1.36×，都低于参考带；当天诊断（裁定 B96）查出 T1 读数里约 0.9–1.6 倍来自量法，纠正后约 4.1×（全部实现）、3.4×（只算通过验收的），按新口径的重测（步 31-1b）正在进行。深度证据是固定观察下的跳数分布：一、二、三跳的判断数为 22、12、4；逐跳未决率与真机深度曲线还没测。能力更换测试目前走通了追踪的八类能力假设中的一类。

## 2026-09-23: fail-open fixes, value-level taint, and five rule-batch items / 放行缺陷修复、值级 taint 与规则批五项

Synced from the research tree through commit `a17596a` (rules batch B29/B25/B28/B32/B3 and the
B33 value-level taint switch; B24 split-sample certification was already synced in an earlier
sync). Fixed two fail-open defects: `speculate`/`vectorize` executed `do` on a branch's state
expression before the branch's own judgement had decided the branch should run (effect analysis
treated calls to user-defined functions as pure); and untrusted content was laundered "trusted"
through concatenation (`+`), `join`, `text()`, `m.content`, or a failed untrusted `do`'s error
text, letting a `mat()` built from it pass the irreversible-`do` gate. The first fix used a side
table of untrusted text leaves matched by substring; measured against ten probe cases it was
wrong in both directions -- a one-digit untrusted number survived `text()` concatenation and
still laundered clean (false accept), while program literals that merely shared a substring with
previously-read untrusted text were rejected (false reject). It was replaced with value-level
taint: every scalar `Value` carries a taint bit, propagated through binary/unary operators and
built-in dispatch (an explicit exception list exempts pass-through built-ins such as `map` and
`filter`), and read out at `content()`, `m.content`, `text()`, and `Fail`. Five rule-batch items
landed: B29 makes host `put` write fixture-only records that cannot license an irreversible `do`;
B25 auto-flags suspension candidates on drift and adds a `calib-confirm` command for the human
decision; B28 replaces `agg` with `repeat` (mean or median only, no majority vote) and keys
merged readings by sample count; B32 adds judgement-absence handling (retry, backoff, escalate,
circuit breaker) and a static latency budget; B3 gives `unsure` two named causes, `rejected_all`
and `no_candidate`, with library routes for both. B30 (a deterministic tuple-based calibration
key) was ruled necessary but is not built -- the language surface already lets authors write
their own calibration keys (`test(question, "k")`), and reconciling that with a mandated tuple
serialization needs a scope decision first; it is paused, not silently dropped. The Codex review
comments on PRs [#27](https://github.com/Towow-ai/jpp/pull/27) and
[#28](https://github.com/Towow-ai/jpp/pull/28) were addressed with local, test-backed fixes (see
the PR threads for the itemized replies). `cargo build` and `cargo test --workspace --offline`:
382 passed, 0 failed, 3 ignored. Architecture and engineering reorganization plans are still
being finalized in the research workspace and are not part of this sync. Full account:
[rules batch and value-level taint](updates/2026-09-23-rules-and-taint.md).

同步来源是研究树，截至提交 `a17596a`（规则批 B29/B25/B28/B32/B3 与 B33 值级 taint 切换；B24
拆分样本认证此前已在更早一次同步中带过）。修复了两类放行方向缺陷：`speculate`/`vectorize`
在分支自己的判断决定要不要走之前，提前对分支状态表达式求值并执行了里面的 `do`（效应分析把
调用用户函数一律当纯）；不可信内容经 `+` 拼接、`join`、`text()`、`m.content`，或不可信 `do`
失败后的错误文本，被洗成「可信」，凭它构造的 `mat()` 越过了不可逆 `do` 的关卡。第一版修复用
旁路表（记不可信文本叶子做子串匹配），十个探针案例实测两个方向都错——一位数不可信数值经
`text()` 拼接后仍被洗白（假放行），程序自己的字面量因与读过的不可信文本共享子串被误拦（假
拒绝）。换成值级 taint：每个标量 `Value` 自带一位 taint，经二元/一元运算与内置分派传播
（`map`/`filter` 等只搬运元素的内置单列例外表），在 `content()`/`m.content`/`text()`/`Fail`
处读出。规则批五项落地：B29 让宿主 `put` 只写夹具记录，夹具线不得放行不可逆 `do`；B25 按漂移
自动标记停岗候选，加 `calib-confirm` 命令交人确认；B28 用 `repeat`（只许均值或中位数，禁众数）
取代 `agg`，合并读数按样本数独立开校准键；B32 加判断力缺席处理（重试/退避/升级/熔断）与静态
时延预算；B3 给 `unsure` 补 `rejected_all` 与 `no_candidate` 两个具名原因及库内去向。B30（确定
性元组校准键）已裁定要做但未造——语言表层已经让作者自己写校准键（`test(题面, "k")`），要与
「代码键必须是元组序列化」的条文对齐，得先裁定作者键的地位，此项暂停、不是被悄悄丢下。Codex
在 PR [#27](https://github.com/Towow-ai/jpp/pull/27) 与
[#28](https://github.com/Towow-ai/jpp/pull/28) 上的评审意见已逐条本地修复并配回归测试（逐条
回复见两个 PR 讨论串）。`cargo build` 与 `cargo test --workspace --offline`：382 通过、0 失败、
3 忽略。架构与工程方案的重整仍在研究区定稿中，未随本次同步公开。完整说明见
[规则批与值级 taint](updates/2026-09-23-rules-and-taint.md)。

## 2026-09-23: constructs, live backend and template-level calibration / 构造施工、真实后端与题式级校准

Synced from the research tree through commit `42988c5`. Built: the real JEV backend
(`--backend live`), questions as first-class values (`form`/`fill`), a three-way sieve that
takes questions directly plus review-opinion material, pairing (`pair`), set aggregation
(`tally`/`first_k`), bounded iteration with a shrink line (`iterate`), calibration intake
(`calib-import`, the truth channel) with form-level line fallback and split-sample two-sided
certification (B24), and the composition-closure contract (B17) shared by every set-level
construct. Found: the live backend returns correct readings but every exit is
`unsure(cold)` without a calibration record; literal question templates read bimodally and
need only a global line; semantic templates need real calibration, misclassifications
persist across reruns, and re-asking does not help; human spot-checking showed the
disagreement was an undefined question scope, not labelling noise — splitting the template
resolved it, and the topic-relevance template reached 30/30 spot-check agreement and is
certified. Designed in response: the question template, not the literal question, is the
calibration primary key (B2); split-sample certification with the gate read off a one-sided
95% confidence lower bound, not the raw agreement rate (B19/B24); the composition-closure
contract; and several changes carried from a four-line question-theory literature review.
Unfinished: of 415 design-ledger items, 89 are built; two classes of fail-open defects
(`speculate` executing `do` on a branch that should not run; untrusted content becoming
"trusted" through concatenation/join/failure paths) are ruled to need fixes but are not
fixed yet; B28–B32 are decided but not yet implemented. `cargo test --workspace --offline`:
341 passed, 0 failed, 3 ignored. Full account: [constructs and calibration](updates/2026-09-23-constructs-and-calibration.md).

同步来源是研究树，截至提交 `42988c5`。做成了：真实 JEV 后端（`--backend live`）、题成为
一等值（`form`/`fill`）、三路过滤直接吃题并把评审意见渲染成材料、配对 `pair`、聚合
`tally`/`first_k`、带收缩终止线的迭代 `iterate`、校准进料 `calib-import`（真值通道，带
题式级线回退与拆分样本两侧认证 B24）、以及所有集合级构造共用的组合封闭性契约（B17）。
发现了：真机读数本身正确，但没有校准记录时出口全是 `unsure(cold)`；字面题式读数两极，
一条全局线就够；语义题式需要真正的校准，错判在重跑间持续存在，重复提问无效；人工抽检
揭示分歧来自题面外延未定，不是标注噪声——拆题后话题相关题式抽检 30/30 一致并转正上岗。
针对问题设计了：题式而非字面题作校准主键（B2）；拆分样本认证，上岗门槛看抽检一致率的
单侧 95% 置信下界而不是原始一致率（B19/B24）；组合封闭性契约；以及四线问题理论调研带来
的多处改动。未完成：设计总账 415 条中已造出 89 条；两类放行方向缺陷（`speculate` 在不该
执行的分支上执行 `do`；不可信内容经拼接/join/失败路径变「可信」后越过不可逆 `do` 关卡）
已裁定要修但尚未修好；B28–B32 已定未造。`cargo test --workspace --offline`：341 通过、
0 失败、3 忽略。完整说明见[构造施工与校准](updates/2026-09-23-constructs-and-calibration.md)。

## 2026-09-23: sync research-tree runtime increments / 同步研究树运行时增量

Port the research tree's later Rust increments that the public tree lacked: the static half of J-10 (`budget.unsure` in the AST and a pre-call warning that sums each judge site's `unsure_rate`), a real producer for `CalibRecord.unsure_rate` when `commission` certifies a line, and drift warnings on `allocate`/`unsure_bound` as well as `cut`. The public review fixes (log-space binomial upper bound, the all-reject threshold, portable test paths and fixtures, honest certificate disclosures) are kept, not overwritten. `.jpp` source still cannot write `budget.unsure`; the frontend lowers it as absent. `cargo test --workspace`: 318 passed, 0 failed, 3 ignored.

把研究树里公开仓库缺少的 Rust 增量搬过来：J-10 的静态部分（AST 增加 `budget.unsure`，在任何模型调用之前按各判断位置的 `unsure_rate` 求和并告警）；`commission` 认证一条线时真正写出 `CalibRecord.unsure_rate`；漂移告警从 `cut` 扩到 `allocate` 与 `unsure_bound`。公开侧已有的审查修复（对数空间二项上界、全拒绝阈值、可移植的测试路径与夹具、如实的证书说明）全部保留，未被覆盖。`.jpp` 源码目前还写不出 `budget.unsure`，前端按缺省处理。`cargo test --workspace`：318 通过、0 失败、3 忽略。

## 2026-09-23: review and integrate the open PRs / 审查并整合待合入 PR

#17's community examples, #25's Rust synchronization and #20's design-revision record
are merged. #25 received public-test portability repairs, a rebuilt browser bundle and
two reproduced numerical fixes; #20 keeps an enabled exact overflow-span regression.
Rust and Python 3.12/3.13 CI pass. #26 reconciles the current design map and implementation
handoff with that public baseline; local links in its changed documents were checked.
See the [review record](updates/2026-09-23-pr-integration.md).

#17 社区示例、#25 Rust 同步、#20 设计记录已合入。修复公开测试路径、浏览器源码包
及两处数值边界错误，补上整数溢出准确位置回归；Rust 和 Python 双版本 CI 通过。
#26 将总设计图与下一段实施任务对齐这一公开基线。统计选线的一般保证、真机 CLI
和后续研究增量仍分别列为未完成工作，不混成已交付能力。

## 2026-09-23: correct stable documentation drift / 校正稳定文档漂移

Check stable documents against code and available execution evidence. Correct source-example and `ask` coverage descriptions in the research tree, distinguish old backlog snapshots from current work, and record the bounded OCaml paper exploration. Retain unfinished experiments and the research/public-runtime boundary. No runtime changed. [Correction details](updates/2026-09-23-documentation-drift.md).

对照代码与已有执行证据，修正研究区示例数量和 `ask` 覆盖表述，标清旧待办快照，补记 OCaml 纸面探索的阶段边界。未验证实验与研究/公开版本区别保留，不修改运行代码。[校正明细](updates/2026-09-23-documentation-drift.md)。

## 2026-09-23: concrete implementation handoff / 下一段具体施工交接

Prepare the next implementation package: shared native source constructions, two programs whose results compose again, followed by real JEV CLI integration and a consolidated release. Identify existing code and observable acceptance behavior; retire stale waiting instructions in the local handoff. This prepares work without launching an executor or changing runtime behavior. [Implementation task](implementation-handoff-2026-09-23.zh-CN.md).

明确下一段施工：共同原生源码构造、两份组合结果能再次组合的程序，随后真实 JEV CLI 接线与整版交付。列出现有代码及行为验收，并在本地入口更新历史等待状态。本轮准备任务，尚未启动执行或修改运行代码。[具体实施任务](implementation-handoff-2026-09-23.zh-CN.md)。

## 2026-09-23: overall design and delivery map / 总设计与交付地图

Connect the original five-step implementation plan to the current language layers, verified example behavior, remaining construction work and publication status. Distinguish semantic collection capabilities from similarly named list/reading operations, and propose completion packages without replacing the current contracts. This is documentation only: existing verification records and selected source were inspected; no new runtime change or model experiment was performed. [Read the map](design-and-delivery-map.zh-CN.md).

将原五步实施计划、语言各层、已有程序效果、剩余建设及公开状态连成一张图。澄清语义集合能力与同名列表/读数操作的区别，整理后续交付包，继续沿用现行契约。本轮只更新文档，读取已有验证记录并核对部分源码，没有改运行代码或进行新模型实验。[总设计与交付地图](design-and-delivery-map.zh-CN.md)。

## 2026-09-23: complex composition and a verified foundation / 复杂组合定位与地基现状核对

Clarify the target: a small set of underlying constructs should generate many complex algorithms, and composed results should compose again. Consolidate the external research as reference material. Re-run the local research Rust workspace at `520fef2`: 315 tests passed, zero failed, three ignored; direct examples reproduce nested methods, adaptive questions, partial continuation and zero-call replay. These are research-workspace results, not a new public runtime release or a live-model benchmark. The first complete language body remains unfinished. [Modules, effects, work distribution and remaining scope](updates/2026-09-23-composable-foundation-status.md).

明确目标：少量底层构造支撑多种复杂算法，组合结果继续组合；外部调查整理为参考依据。本地研究 Rust `520fef2` 复跑315项通过、0失败、3忽略，直接复现方法再组合、自适应选问、部分结果续接和零调用重放。这是研究工作区验证，不是新公开运行版本或模型基准；第一版完整主体仍未完成。[模块、效果、工作分布与剩余范围](updates/2026-09-23-composable-foundation-status.md)。

## 2026-09-23: source-based ecosystem reassessment / 根据公开源码重新核对生态需求

Fresh public-source collection and static reviews revise earlier ecosystem claims: typed question composition, adaptive algorithms, caching, budgets and several fallback policies already have implementations. Developer requests, implemented responses and inferred needs are separated; public issue counts are not treated as independent demand votes. Compatible local endpoints differ in capability and confidence semantics. The next proposed comparisons test complex composition, evidence coverage, budgets, cache costs and real effects. No downloaded project or model benchmark was executed, and no runtime changed in this publication. [Evidence and implications](updates/2026-09-23-ecosystem-reassessment.md).

重新采集公开源码并深读，订正此前判断：类型化题集、自适应算法、缓存、预算和多种失败处理已有实现。明确请求、已有应对与推断需求分开记录，不将 issue 数量当独立需求票数；本地兼容接口也不能抹平能力及置信度含义的差异。后续对照围绕复杂组合、输入覆盖、预算、缓存代价和真实动作。本轮未执行第三方项目或模型基准，公开同步未改运行代码。[证据与建设影响](updates/2026-09-23-ecosystem-reassessment.md)。
## 2026-09-23: reconcile the design-revision PR / 整理设计修订 PR

PR #20 now preserves the current `12`/`13` authority documents while retaining its dated
design report. The three defects are already fixed by #25 and covered in `v13_rules.rs`;
obsolete ignored reproducers are replaced by an enabled overflow regression that checks
the runtime error variant and exact source span. A checker rejection cannot pass this
test. Current status text now reflects the merged implementation.

保留现行规范和原始设计报告，移除已被后续回归覆盖的三条过期忽略测试；补验整数溢出
确实返回运行错误并指向准确源码位置，不会因其他静态错误而误判通过。当前状态说明同步
到已合入的 Rust 实现，历史数字保留并标明日期。

## 2026-09-23: review the native kernel sync / 审查原生内核同步

PR #25 brings the later Rust checker/runtime and calibration host APIs into the public
tree. Review fixed test paths that depended on the private research layout, supplied
minimal legacy-schema fixtures, rebuilt the browser source bundle, and fixed two numeric
boundary defects: large-sample binomial underflow and a reject-all threshold that could
accidentally accept score 1. Each numeric failure was reproduced before its fix.
The review checkout passed 304 Rust tests (3 ignored) and 544 Python tests on Python 3.12
before integrating the separately reviewed community examples from #17. GitHub CI checks
the combined branch. Private-data probes and synthetic tests do not establish live model
accuracy. Selected-threshold risk certification remains experimental; see the precise
limitations in [Rust status](../rust/README-status.md).

本次将后续 Rust 实现同步到公开仓库，修正测试对私有目录的依赖，补齐旧记录格式夹具，
更新浏览器源码包，并修复大样本二项上界下溢、全拒绝阈值误放行满分样本两处边界错误。
两处数值问题均先复现失败再修复。隔离副本通过 304 项 Rust 测试（3 项忽略）和
Python 3.12 的 544 项测试；随后纳入已单独验证的 #17 社区示例，组合结果由 CI 复查。
当前统计选线仍属实验实现，不能把单一合成分布测试称为一般风险保证。

## 2026-09-21: a second authority text and a plan to finish the language / 第二份依据与把语言做完整的实施计划

Research documents only this round; no runtime in this repository changed. `13-Rust实践反馈设计修订-v0.2.md` is published for the first time — six rules that a day of building in Rust forced onto the design, three of them correctness defects (a method's identity omitted its captured state and reused the wrong result; going over budget discarded a call that had already been paid for; integer overflow behaved differently in debug and release). `14-实施计划-把语言做完整-v1.md` sets out what remains, including what this version deliberately does not do and why. The inventory rounds behind it are reported with their own two defects: the new authority text was missing from the first round's material list, so that round's "conflict" findings are not used, and adversarial review failed all nine audits. The most useful output was 22 places where taste had been recorded as mechanism, several of them ours — the test being whether a rule can say what would turn what red. Applying that test found three seams where the kernel silently flattened a three-valued fact to two, the worst of them laundering a material's provenance in two lines of ordinary source; all three are fixed, and the Python reference turned out not to have the taint hole — the port introduced it. 544 tests pass on Python 3.12 and 3.13 in this repository, unchanged by the sync. The kernel work described is in the research workspace and **is not in this repository's `rust/`**; merging the two lines is a separate item. Details, numbers and what was verified where: [2026-09-21 update](updates/2026-09-21-language-completion-plan.md).

本轮只同步研究文档，本仓库运行代码未变。`13-Rust实践反馈设计修订-v0.2.md` 首次公开——六条由一天 Rust 施工逼出来的局部修订，其中三条是正确性缺陷（方法身份不含捕获状态，复用了错的结果；超预算时把已经付过钱的调用丢掉；整数溢出在 debug 与 release 行为不同）。`14-实施计划-把语言做完整-v1.md` 排出剩下要做的，并写明本版有意不做哪些、为什么。支撑它的两轮盘点连同自身的两处缺陷一起公开：新依据缺席于第一轮的材料清单，故该轮「冲突」类结论不采信；对抗复核把九份审计全部判为不通过。最有价值的产出是 22 处把 taste 记成机制的地方，其中几处是我们自己写的——判据是这条规则能不能说出「什么情况下它会让什么东西变红」。照这条判据又查出三处把三值静默压成两值的缝，最重的一条两行普通源码就能洗白材料的来源可信度；三条都已修，且 Python 参照实现没有那个 taint 洞——是移植时新引入的。本仓库在 Python 3.12 与 3.13 上各通过 544 项测试，同步未改变这个数。文中描述的内核工作在研究工作区，**不在本仓库的 `rust/` 里**，两条线的合并是单独一项。细节、数字与「哪个数在哪里验的」见 [2026-09-21 更新](updates/2026-09-21-language-completion-plan.md)。


## 2026-09-21: co-construct language capabilities and algorithms / 语言能力与算法共同构造

修正[总计划](towow-discovery-master-plan.zh-CN.md)中的推进前提：不要求先用成熟工具实现完整算法再迁移J++。所需的构造能力可能正是语言建设要创造的部分，应与最小算法片段共同设计。已有工具按需复用，缺少完整旧实现时保留构造阻碍与新程序的对照。本轮保存两条原话续记和本地理解记录，只更新设计，不启动实现或模型实验。

The plan no longer assumes a complete host-language algorithm must precede J++. Missing construction capabilities and algorithm fragments may be developed together. Existing tools remain available where useful; this update records the design correction and local prompt provenance, with no new runtime or model-performance claim.

## 2026-09-21: build language improvements from discovery / 从发现应用落实语言改进

补充[总计划](towow-discovery-master-plan.zh-CN.md)：交付发现组件之外，必须产出已经实现、能被另一算法复用的J++构造改进，不能停在语言好不好用的评估。新增具体能力与源码对应表、L0～L4建设循环及改进前后验收。当前只有计划、代码依据和候选方向，尚未宣称改进已实现。另在本地按真实分叉边界保存43条用户原始提示词/界面回复，保留原话与推导的区分，未公开整段会话。

The deliverable now includes implemented language/library/tooling improvements, before/after programs and cross-algorithm reuse, alongside the discovery component. The capability inventory distinguishes native source examples from the Python application. Original user prompts are archived locally with their source boundaries; no new model experiment or runtime change is claimed.

## 2026-09-21: discovery goal, evidence and parallel implementation plan / 发现目标、验收与并行实施总计划

新增[通用发现能力总计划](towow-discovery-master-plan.zh-CN.md)，重新对齐通爻原始问题：接收方依局部上下文提出关系，中间问题与组合参与后续发现。分开J++语言、通爻协议、发现程序与展示的交付；整理已有真实语料、强基线、行为/质量/费用/动态验收及P0～P5依赖。核查并索引先前讨论记录，明确尚非完整逐字归档。当前成果是规划，没有新的模型运行或发现效果结论。

The plan enables parallel work against the published Rust snapshot, with source-owned discovery logic and a minimal input/observation/capability contract. It reuses prior research assets without treating incomplete Gold, synthetic examples or historical proxy edges as completed discovery evaluation. Numerical targets remain proposals to freeze before formal evaluation; corpus splitting and the receiver-local loop are still unimplemented.

## 2026-09-21: align all project entry points / 同步项目各入口的交付状态

The source package was already merged in [PR #12](https://github.com/Towow-ai/jpp/pull/12).
This update corrects stale Python-only and future-syntax wording across current scope,
roadmap, contributor/developer guides, project motivation and verification records.
Original dated results remain labeled as history. The next outcomes are source-library
reuse, consistent composition rules and a bounded application using `.jpp`.

代码已在主分支，本次补齐此前没跟上的介绍：明确独立源码已经交付、Python 指南的
适用范围，以及后续工作怎样验收。检查文档链接与现状表述；不改运行代码，也不发起
模型实验。

## 2026-09-21: standalone source runs on Rust / 独立源码到 Rust 执行贯通

The [native Rust package](../rust/README.md) now parses `.jpp` source, lowers it to
the shared program representation, checks language rules and executes it through
one kernel. The CLI includes `parse`, `check`, `run`, fixed observation loading,
JSON reports and ledger replay. No Python interpreter is used on this path.

现在可以直接写 `.jpp` 文件并运行。源码里的方法可以作为参数、返回方法，再参与
下一次组合。前端只负责表达和转换，执行规则由共同 Rust 内核承担。原 Python 包和
通爻实验保留可用，本次没有扩展 Python 正式内核。

| Executed program / 实际程序 | Observed result / 结果 |
| --- | --- |
| Nested composition / 组合再组合 | 20 → 43; source and direct core program return identical JSON / 源码与直接内核构造结果相同 |
| Adaptive questions / 自适应选问 | 731 located among 1,000 candidates in 10 fixed observations / 十题定位 731 |
| Partial continuation / 部分结果继续求解 | Cost 9 with C/D pending → cost 2 with D pending → complete; 6 observations, A/B/C checked once / 保留旧观察和检查 |
| Replay / 重放 | Same value; zero fresh judgment calls and zero repeated local checks / 返回值相同，无新调用和重复动作 |

Validation in an independent publication checkout: **41 Rust tests passed**, with
one documentation snippet explicitly ignored. Tests cover source execution,
direct core algorithms, syntax/source positions, known literal argument type
errors, budget stopping and replay. A native installation outside the research
checkout ran the complete programs with an empty PATH; Python and Cargo were not
available to the executable. Formatting checks passed. CI also runs the retained
Python suites; its result is recorded on the pull request.

独立发布目录 41 项 Rust 测试通过。原生安装在项目外、PATH 为空时仍可运行；明显
参数类型错误会定位到 `.jpp` 文件行列并提前停止。静态检查覆盖明确子集，动态规则
仍由运行时检查。固定观察与合成校准验证执行机制，不是模型准确率实验；方法闭包
在源码重跑时重建，账本没有被描述为任意闭包的跨进程序列化。

[Grammar / 文法](../rust/FRONTEND.md) · [Equivalent source and core usage / 等价用法](../rust/COMPARISON.md).

[Implementation commit / 实现提交](https://github.com/Towow-ai/jpp/commit/d39b036).

## 2026-09-20: composable discovery application / 可续接的发现应用

新增[可替换计划的发现组件](towow-discovery-iteration.zh-CN.md)：同一 J++ 组合接受不同意图、问题、路由和组合函数；提议保留原成员与上下文，可以作为下一轮输入。216 个合成主体上的三个真实运行分别完成 20、20、12 个判断，最后一个将后端两人提议续接为含安全检查成员的三人提议。三个案例在安装后的 wheel 中用录制完全复现，重复执行新增请求 0。公开[交互图谱](https://towow-ai.github.io/jpp/demos/towow/teams/)与调用接口，未将其称为通用规划器或分布式网络。

The new application accepts caller-authored plans and replaceable routing/combination functions; nominations preserve their members and can seed later discovery. Three live synthetic runs used the same implementation, followed by installed-wheel replay, zero-request warm reruns and a no-combination comparison. Existing J++ composition and observation mechanisms are reused; the formal Rust kernel direction is unchanged.

325 人固定二十名额的来源探索对照得到 681 条前十命中，低于原 J++ 的 697；7 条跨来源种子进入候选池后仍没有留在前十，因此没有替换默认方法。The fixed-slot exploration control regressed and remains visible rather than replacing the default. 本轮新增估算 JEV 费用约 $0.041700；累计约 $1.530903（已授权总预算 $5）。本地 544 项检查通过，包含一次子进程导入环境修正后的定向复查；浏览器与线上部署另按发布记录核验。

## 2026-09-20: focused OCaml experiments alongside Rust / Rust 主线允许具体 OCaml 实验

The [Rust ADR](adr/0001-rust-kernel.md) now permits OCaml experiments around
specific grammar and semantic questions. Selected rules must enter the shared
specification and be reproduced in Rust; any retained formal OCaml component
needs an explicit interface and verified build/run path. Rust implementation
continues. This is a policy-only change, with links and wording checked; no OCaml
installation, experiment or mixed-language runtime has been performed.

新增“Rust正式实现＋围绕具体问题开展OCaml实验”规范。实验、已选规则、正式实现分别
标注；不因实验暂停Rust，不默认增加用户安装依赖。当前只同步约定，下一次实验由
真实设计问题触发。中英ADR、设计入口和进展一并更新。

[Policy commit / 规范提交](https://github.com/Towow-ai/jpp/commit/35ec2d0)

## 2026-09-20: Rust and standalone source selected / 选定 Rust 与独立源码

The formal language implementation now moves to one Rust kernel with a `.jpp`
parser, J++ type/effect checker and CLI. This replaces the previous indefinite
deferral of independent syntax. [Decision and first acceptance milestone](adr/0001-rust-kernel.md).

正式语言建设转到 Rust；Python已发布成果保留作行为对照、实验与必要适配，不再
继续扩大正式 Python 内核或接口。本次只更新路线、设计入口和中英 README，检查了
链接及现状措辞；尚未交付 Rust 执行能力。下一项可验证成果是源码→检查→运行的
完整程序，覆盖自适应选问及部分结果继续求解，不只是语法展示。

This documentation-only update does not change the existing runnable examples.
Rust construction is starting; subsequent updates will include actual build and
source-program execution evidence.

[Decision commit / 路线提交](https://github.com/Towow-ai/jpp/commit/e04265e)

## 2026-09-20: language design and grammar entry / 语言设计与文法入口

The [design page](design.md) now links directly to the already published current IR/class contract and historical language specification, including its lexical rules and full EBNF. The READMEs and research index expose these links; the historical specification has a bilingual archive notice. Current source remains Python builder code: the old grammar has no delivered standalone parser/compiler, and the current contract defers independent surface syntax without a delivery date; this does not replace the intent to develop an independent language after validating examples and algorithms. The guide distinguishes six IR forms, typed exits, composition and runtime behavior from design requirements.

[设计页](design.md)现在直接链接已经公开的现行 IR/类契约和历史语言规范，包含词法与完整 EBNF；中英文首页和研究索引都提供入口，历史规范顶部增加双语存档说明。当前源码仍使用 Python 构建器，旧文法没有已交付的独立解析器/编译器，现行契约未排独立表面文法的交付日期；这不替代先跑通实例和算法、后发展独立语言的意图。说明区分六形式 IR、类型出口、组合与执行机制和设计要求。

Validation: checked the added relative links and referenced API names against the current source; no runtime code changed and no live model calls were made. The archive notice is also retained in the workspace source so the existing documentation sync preserves it. / 验证：检查新增相对链接和当前源码中的 API 名称；没有修改运行时代码或调用真实模型。工作区原件同步保留同一存档说明，现有同步流程不会覆盖该说明。

## Available now / 现在可以使用

The public `0.1.0a1` snapshot contains a Python embedded implementation, judgment runtime, composition library and installed `jpp demo` command. [Download the alpha](https://github.com/Towow-ai/jpp/releases/tag/v0.1.0-alpha.1).

| Capability / 能力 | Evidence in this repository / 本仓库依据 |
|---|---|
| Questions as values / 问题可保存恢复 | `question_to_dict`, `question_from_dict`; composition tests |
| Compositions remain components / 组合继续参与组合 | Sequential, dynamic and nested component tests |
| Adaptive inquiry / 自适应提问 | Installed demo: 1,000 candidates, target 731, 10 synthetic answers |
| Candidate/check/feedback / 候选与反例反馈 | Installed demo: 3 trials, all 9 declared inputs checked |
| Local uncertainty / 局部未决 | Conditional count bounds and refinement tests |
| Independent method / 新方法接入 | `examples/independent_method.py` and its tests |
| Reproducible distribution / 可安装版本 | Wheel installation and offline demo verified outside the source tree |

The initial public commit passed 25 tests locally. Linux CI passed on Python 3.12 and 3.13. [Recorded CI run](https://github.com/Towow-ai/jpp/actions/runs/35500401184). These are mechanism and synthetic-observation tests, not a live-model accuracy evaluation.

## Work in the research workspace / 研究中的工作

### 2026-09-20: real-source relation recovery / 真实来源关系恢复

[325 人关系对照](https://towow-ai.github.io/jpp/demos/towow/real/)将七种组合放在同一批资料与 963 条旧标签上比较。固定每人前十，本地融合命中 675 条；J++ 相对排序命中 697 条，新增 52、丢失 30，净增 22。原始等级排序的持平与退步也完整保留。J++ relative ordering recovered 697/963 historical proxy relations at ten candidates per person versus RRF's 675; 52 added and 30 lost. All seven variants, including regressions, remain visible.

独立配对判断 6,500 次，内容去重后 5,353 个请求，175.796 秒，新增估算 $0.153491。全部真实来源实验共估算 $1.447708。有向前十仅改善 3 次，24 条跨来源种子关联仍未找到；这不是准确率、盲测或未来合作预测。[方法、结果及复现 / Method, results and reproduction](towow-real-relations-results.zh-CN.md)。

### 2026-09-20: executable discovery lab / 可执行的发现实验台

发布 216 个合成主体、20 种旧实验意图的[发现实验](https://towow-ai.github.io/jpp/demos/towow/population/)，以及可开关转介、组合、联系人和复用的[十人实验台](https://towow-ai.github.io/jpp/demos/towow/lab/)。网页通过浏览器 Python 执行现有 J++ 源码，模型层使用真实录制；完整介绍和动画保留。The discovery lab now covers 216 synthetic participants and 20 historical intents, with a separate component-intervention experiment. Both browser pages execute the repository's J++ Python implementation against exact recorded model responses.

真实运行：4,320 个判断，216 个后端请求，29.823 秒，估算费用 $0.039198，重复运行新增请求 0。第一版语义层级排序命中旧预期名单 33 次，BM25 43 次；在查看开发结果后加入“语义分组 + 词面排序”，得到 56 次。原始失败、改进及全部 20 条结果均保留；不是盲测或完整准确率。Live execution: 4,320 judgments in 216 requests, 29.823 seconds, estimated $0.039198, and zero additional requests on identical rerun. The initial semantic-tier ranking retrieved 33 expected aliases versus BM25's 43; the documented development revision combining semantic groups with lexical ordering retrieved 56. Original results and all queries remain available.

验证：本次全部 422 项测试通过；浏览器实际执行十人对照与百人程序，百人重复执行复用全部 4,320 个判断。构建 wheel，在源码目录外安装后，两项案例均成功运行。复现 `python -m jpp.towow_population`、`python -m jpp.towow_lab`；默认无模型调用。All 422 tests passed; both experiments ran in the browser and from an installed wheel outside the source tree. The repeated population run reused all 4,320 judgments. 下一步需要新增、未参与开发的意图和完整的人工关联标注 / Next: unseen intents and independently reviewed relevance labels. [方法与记录 / Method and records](towow-demo.zh-CN.md).

同池重跑旧 MiniLM 方法：整段向量 32 / 89，分字段向量 44 / 89；J++ 56 / 89 个已知关系被前十候选找回。分字段对照中 J++ 为 7 胜、10 平、3 负；向量查询阶段更快且无 API 费。Same-pool MiniLM reruns retrieved 32 and 44 of 89 known pairs, versus J++'s 56; query/index costs and losses remain visible. [对照说明 / Comparison](towow-discovery-comparison.zh-CN.md).

发布前同步并行更新的语言内核，重新打包与验证，全部 502 项测试通过。修复新增命令探针中的 macOS 专用 `sed -i ''`，使其同样能在 Linux 执行。Synced the concurrent kernel update and rebuilt the browser bundle; all 502 tests passed locally. The command probe now uses portable sed output replacement on macOS and Linux. 已定位用户提到的真实关系实验，新的真实资料调用尚未执行 / The historical real-source relation experiment has been located; a new live run is not yet performed. [下一组案例 / Next case](towow-real-relations-plan.zh-CN.md).

### 2026-09-20: animated graph with full explanations / 图谱动画与完整说明

`jpp towow` 生成的页面保留完整十人图谱和固定人物位置，通过节点发光与沿线移动的光点讲解四个阶段；图旁和下方保留完整段落介绍。支持暂停、重播、调速、选择阶段和人物依据查看。模型记录与计算方法不变，页面不产生新的调用。The viewer keeps the complete graph and fixed node positions, animating processing nodes and particles along edges through four stages. Full prose remains beside and below the graph, with playback controls and inspectable evidence. It reuses the existing execution record without changing the discovery method or making new model calls. [运行方法 / Run it](towow-demo.zh-CN.md).

已检查窄屏图谱、阶段切换、暂停时光点冻结、恢复后推进、人物资料与未确定候选；离线案例检查通过，wheel 在源码之外安装后能生成完整页面。本次验证覆盖录制案例的展示，不是新的模型评估。The graph layout, stage selection, frozen particles while paused, resumed progression, person evidence, and unresolved candidates were checked in the browser. The offline example check passed, and a wheel installed outside the source tree generated the complete page. This verifies the recorded example's presentation, not new model quality. 下一步关注首次观看者能否结合图谱与文字理解转介和组合 / Next: check whether first-time viewers understand referrals and composition using the graph and prose together.

[实现与检查 / Implementation and checks](https://github.com/Towow-ai/jpp/pull/2).

### 2026-09-20: first application example / 首个应用案例

The current main branch adds `jpp towow`: receiver-local judgments, a referral, a candidate combination, and subsequent discovery in ten fictional participants. The shipped recording comes from real `jev-1.13.0` requests; default replay is offline. The three-stage live run took 5.215 seconds, identical-input reuse made zero new calls, and changing one participant reused 16 of 32 judgments. A separate single-run comparison of identical first-stage request bodies measured 7.847 seconds sequentially and 1.042 seconds concurrently. These are demonstration measurements, not accuracy or large-network claims. [Proposal](first-problem-towow.zh-CN.md) · [Run and inspect the example](towow-demo.zh-CN.md).

Verification / 验收：本次案例在公开基础版本的独立检出中通过全部 31 项测试；构建 wheel 后，在源码目录之外安装并执行 `jpp towow` 成功，默认路径未访问网络。All 31 tests passed in an isolated checkout of the published baseline. The built wheel was installed outside the source tree and its offline `jpp towow` command completed successfully. This evidence does not cover a concurrent kernel upgrade / 本次验收不覆盖并行施工中的内核升级。

Runtime and composition development continue in a separate research workspace. The status below comes from maintainers' implementation notes inspected on the date above; it does not mean that new code has shipped here.

运行内核这条线正在完善三种题型的接口、执行与成本分析，并根据首次使用者暴露的疑问补全文档。组合这条线已实现两种算法构造器和第三个独立方法，正在完善演示、材料说明与接入交接。研究中的新结果会经过发布仓库的安装和测试后再同步。

## Next questions / 接下来要弄清楚

| Work / 工作 | Desired outcome / 想得到的结果 |
|---|---|
| Clearer semantics / 讲清运行规则 | A new reader can construct a method without guessing question, material or result formats |
| More algorithm constructions / 更多算法构造 | Different methods reuse the same primitives; repeated glue code becomes visible |
| Real judgment backend / 真实判断后端 | A reproducible example reports evaluation data, calibration, quality and cost |
| Composition rules / 组合规则 | Examples show which transformations preserve behavior and which effects constrain them |
| Surface syntax / 表层语法 | A small notation expresses demonstrated needs and runs against shared examples |

The full language is still taking shape. We do not assign a completion percentage. The current milestone is an executable alpha; the next evidence we want is broader reuse by people who did not design it.

## Update practice / 后续更新方式

Each update should identify the user-visible change, a command or example demonstrating it, its verification, and the next design question. Keep publication results separate from ongoing research. Date each update; old test counts do not automatically cover new commits.

### 2026-09-20: bilingual progress updates / 双语进展更新

We now publish each completed, verified advance to GitHub with Chinese and English commit messages and progress notes. Follow the [commit history](https://github.com/Towow-ai/jpp/commits/main/) to see what changed and the [maintenance practice](maintaining.md) for how updates are prepared. This update adds documentation only; the runtime and existing verification results are unchanged. The next entries will link completed implementation milestones to their examples and checks.

今后每完成一项可验证的实际进展，就同步 GitHub，并提供中英双语提交说明和进度记录。关注者可以通过[提交历史](https://github.com/Towow-ai/jpp/commits/main/)了解改变，通过[维护约定](maintaining.md)了解更新方式。本次只更新文档，运行代码未变；后续进展会附上对应成果、用法和验证依据。

### 2026-09-21: fusion by mechanism, exits across frames, ten runnable probes / 融合按机制成立、出口跨帧、十条可运行探针

Two compiler passes (`speculate`, `vectorize`) make judgment fusion independent of loop style; unresolved exits can be returned from annotated programs; ten runnable probes ship with offline and live modes. 494 tests pass on Python 3.12 and 3.13 in this repository. Details, commands and numbers: [2026-09-21 update](updates/2026-09-21-mechanized-fusion-and-probes.md); the previous kernel sync is in the [2026-09-20 update](updates/2026-09-20-kernel-research-sync.md).

两个新编译 pass 让判断融合不再取决于循环写法；带返回注解的程序可以把「拿不准」交给调用者；十条可运行探针带离线与真机两种模式。本仓库在 Python 3.12 与 3.13 上各通过 494 项测试。细节见 [2026-09-21 更新](updates/2026-09-21-mechanized-fusion-and-probes.md)，上一次内核同步见 [2026-09-20 更新](updates/2026-09-20-kernel-research-sync.md)。

### 2026-09-21: a falsified scenario, a calibration reading, and the Rust kernel starting / 一个被证伪的场景、一次校准读数、Rust 内核开工

No new runtime capability this round. E9f-2b′ **failed**: predicting which paragraphs an author will ask to change is not decidable in one literal hop (n = 1,564 paragraphs, AUC 0.541 against a bet of 0.70; a length-and-digits heuristic scored higher at ~~0.638~~ **[0.643 — see the correction entry below]**, and a `haiku` judge sat on the random line too). E-CAL's final run passed none of its falsification criteria and about half its bets: Chinese `noul` readings are usable as probabilities (ECE 0.057), ~~`choice` showed no first-position bias (permutation consistency 1.000)~~ **[withdrawn 2026-09-21 — measurement artefact; see the correction entry below]**, `score` hit the adjacent band 0.964 — while `noul` AUC (0.748), `choice` argmax (0.757) and `score` MAE (0.553) all came in under their bets, and the 300 items turned out to be built from about 45 independent paragraphs, so effective n is 17–26 rather than 100. **[Scope added 2026-09-21: the calibration set behind every number in this paragraph is the both-models-agree subset, not a random sample, and is optimistically biased by an amount now measured for two of the three question types — see the entry below.]** The formal kernel moves to Rust ([ADR 0001](adr/0001-rust-kernel.md)); its core is still being built in the research workspace and ~~**no Rust source is published yet**~~ **[overtaken the same day — a Rust workspace landed under `rust/` on the front-end line; see the Status block in that update]**. 535 tests pass on Python 3.12 and 3.13 in this repository. Details and every number: [2026-09-21 update](updates/2026-09-21-two-experiments-and-rust-start.md).

本轮没有新的运行能力。E9f-2b′ **失败**：段级预测「作者会要求改这一段吗」在一跳字面下不可判（n = 1,564 段，AUC 0.541，赌的是 0.70；长度加数字的启发式基线反而更高，~~0.638~~**〔更正为 0.643，见下方更正条目〕**；haiku 裁判同样在随机线上）。E-CAL 正式版三条证伪判据一条都没触发、赌值对了一半：中文 `noul` 读数可以当概率用（ECE 0.057），~~`choice` 无首位偏置（置换一致 1.000）~~**〔2026-09-21 作废：测量假象，见下方更正条目〕**，`score` 相邻档 0.964；但 `noul` AUC 0.748、`choice` argmax 0.757、`score` MAE 0.553 全部低于赌值，且 300 条题面只由约 45 个独立段落重组而成，有效 n 在 17–26 之间而不是 100。**〔范围补注，2026-09-21：本段每个数字背后的校准集都是「两模型都同意」的子集，不是随机样本，同向乐观有偏，偏多少对三种题型里的两种已经测出来——见下方条目。〕**正式内核转 Rust（[ADR 0001](adr/0001-rust-kernel.md)），core 仍在研究工作区建设中，~~**Rust 源码尚未公开同步**~~**〔当日即被事实追上：前端那条线已把 Rust 工作区推到 `rust/` 下，见该更新的「现状」块〕**。本仓库在 Python 3.12 与 3.13 上各通过 535 项测试。细节与全部数字见 [2026-09-21 更新](updates/2026-09-21-two-experiments-and-rust-start.md)。

### 2026-09-21: correcting a published result / 更正一条已发布的结论

Two `choice` results published in the entry above are withdrawn. Of the 97
`select` items in E-CAL, only 8 emitted a `choice` physical question; the rest
had long candidates and were lowered by the compiler to per-candidate `noul`,
whose `mode_share` is hard-coded to 1.0 — which is exactly the permutation
consistency test, so 67 of 74 items were vacuously consistent. The real
measurement is 7 items **[this figure was itself wrong, corrected twice more the
same day — the real measurement is 8, not 7 and not 0; see
`docs/updates/2026-09-21-second-correction-and-kernel-progress.md` §1]**, and "no first-position bias" is void because per-candidate
`noul` has no position at all; on the 8 items that really ran `choice` the first
candidate was chosen 3 times against a ground-truth rate of 1, pointing toward
bias rather than away from it. The same round corrects E9f-2b′'s accounting:
cost $0.0027 → **$0.045** over 1,633 calls (a telescoping `reset_stats()` bug in
the experiment script, not an overspend — both caps held), C_S ≈ 5.5 s withdrawn,
the random arm **indistinguishable** from Jev rather than better, and the `haiku`
price a range of $1.3–$12.9. **Both falsification verdicts stand**; E9f-2b′ is
still a FAIL. The wrong sentences are kept in place and marked, not deleted:
[2026-09-21 update](updates/2026-09-21-two-experiments-and-rust-start.md) carries
a Correction block in each section, and four research documents were re-synced,
including the pre-registrations for the two follow-up experiments. Documentation only — `src/` unchanged; 544 tests pass on Python 3.12
and 3.13.

上一条里两句 `choice` 结论作废。E-CAL 的 97 条 `select` 只有 8 条真的发出 `choice`
物理题，其余因候选过长被编译器下沉成逐候选 noul，而 K-noul 的 `mode_share` 被写死为
1.0——「置换一致」的判据恰好就是它，所以 74 条里 67 条是恒真项，真测量只有 7 条
**〔这个数本身也是错的，同一天又更正了两次，最终值是 8，不是 7 也不是 0，见
`docs/updates/2026-09-21-second-correction-and-kernel-progress.md` §1〕**；
「无首位偏置」直接无效，因为逐候选 noul 根本没有位置，在真跑了 choice 的 8 条上首位
被选 3 次、真值首位 1 次，方向反而朝着有偏置。同一轮还更正 E9f-2b′ 的账：花费
$0.0027 → **$0.045**、调用 1,633 次（实验脚本的 `reset_stats()` 望远镜求和，不是超
支，两条预算上限都没破）、C_S ≈ 5.5 s 作废、随机臂与 Jev **不可分辨**而非更好、haiku
价格改为区间 $1.3–$12.9。**两个证伪结论都不变**，E9f-2b′ 仍是 FAIL。错的句子留在原处
标明，不删：[2026-09-21 更新](updates/2026-09-21-two-experiments-and-rust-start.md)
每节加了更正块，四个研究文件一并重新同步（含两个后续实验的预注册）。本次只改文档，`src/` 未动；Python 3.12 与
3.13 各通过 544 项测试。

### 2026-09-21: a scope for four published numbers, a conformal design result, and a withdrawn labelling plan / 四个已发布数字的适用范围、一个保形设计结论、一条被撤回的标注计划

The `noul` ECE 0.057, `noul` AUC 0.748, `choice` argmax 0.757 and `score`
adjacent-band 0.964 numbers above need a scope they did not have: E-CAL's
ground truth is dual-model labelling, and the 202 items that carry it agree
100.0% of the time between the two labelling models, against 81.1% among the
95 that do not — the calibration set is, by construction, the subset the
models agree on, not a random sample, and every number computed on it is
optimistically biased. **This does not withdraw the numbers**; it scopes them.
The bias direction is now measured, not just hypothesized, for two of the
three question types — `noul`'s error rate reads as **at least 0.392, not
0.301** — while the same free predictor **reverses on `score`**, exactly the
type with the least labelling coverage. A new design,
`设计/保形弃权域-设计-2026-09-21.md`, asks whether conformal risk control can
turn these thresholds into finite-sample guarantees on the 297 already-paid-for
readings: it **cannot**, for any of the three question types (tightest bounds
0.319 / 0.269 / 0.251, so any ≤20% target is infeasible), with the same scope
above applying to those three bounds too. A prototype crate
(`foundation/experiments/conformal-proto/`) ships alongside it — not part of
`rust/`, not built by this repository's own tests — currently 11 passed, 0
failed (two tests that once demonstrated gaps since closed by the
certificate gate and the certificate-addressing fix it argues for, rewritten
as regression guards). The pre-registered plan to close the scope
by labelling 22 more `noul` items is **withdrawn**: those 22 items do not
exist in the material; a real path (`E-NOUL-HI`) is pre-registered in their
place. Documentation only — `src/` unchanged. Full account:
[2026-09-21 update](updates/2026-09-21-scope-note-and-conformal-fail.md).

上面 noul ECE 0.057、noul AUC 0.748、choice argmax 0.757、score 相邻档 0.964
这四个数需要一个此前没写的适用范围：E-CAL 的真值是模型双标，有真值的 202 条里两个标
注模型一致率 100.0%，没有真值的 95 条里只有 81.1%——校准集在定义上就是「两模型都同
意」的子集，不是随机样本，算在它上面的每个数都同向乐观有偏。**这不是把那些数作
废**，是给它们加范围。偏倚方向现在对三种题型里的两种已经测出来，不再只是假设——
noul 的错误率要读成**至少 0.392，不是 0.301**——而同一个免费预测器在 **score 上反
向**，恰恰是标注覆盖最低的那一型。新设计 `设计/保形弃权域-设计-2026-09-21.md` 问：
保形风险控制能不能把这些阈值变成带有限样本保证的数字，用的是已付费的 297 条读数：
**不能**，三种题型都不能（最紧上界 0.319 / 0.269 / 0.251，任何 ≤20% 目标都无解），
上面同一条范围同样适用于这三个上界。配套的原型 crate
（`foundation/experiments/conformal-proto/`）不属于 `rust/`，也不在本仓库自己的测试
范围内——当前 11 通过、0 失败（两条当初测缺口的测试，缺口分别被证书门和证书寻址修
法堵上后，已改写成回归保护）。原定
靠再标 22 条 `noul` 来拆掉范围的计划**已撤回**：那 22 条在材料里不存在；换成预注册
`E-NOUL-HI` 里的真路径。本次只改文档，`src/` 未动。完整内容见
[2026-09-21 更新](updates/2026-09-21-scope-note-and-conformal-fail.md)。

### 2026-09-21: a second reader, a silent divergence, and a sync tool that now covers what it publishes / 第二个读者、一处静默分叉、一个学会覆盖自己发布内容的同步工具

Three small follow-ups to the entry above. `conformal-proto` carried its own
copy of eight items also defined in `jpp_core::conformal`, and the copy had
already silently diverged from the kernel (a parameter renamed `delta` →
`conf_delta` in the kernel — a deliberate distinction between the conformal
bound's confidence level and the calibration archive's hysteresis bandwidth —
never propagated to the copy, and never able to, since a rename in one file
cannot break compilation in an unrelated one). Fixed by deleting the eight
copies and re-exporting the kernel's module instead: **11 passed, 0 failed**,
verified item-by-item byte-identical first. `设计/G3b-零上下文读者第三次-迟到副本.md`
is added — a second, independent zero-context reader given the identical
exercise as the already-published `G3`, held back only because it finished
later; publishing only the faster of two readings is an unchosen selection
rule with the same shape as this round's calibration-set finding. And
`tools/sync-from-workspace.sh` now mirrors `conformal-proto/` itself, instead
of that directory needing a hand copy every round. Documentation and tooling
only — `src/` unchanged. Full account:
[2026-09-21 update](updates/2026-09-21-scope-note-and-conformal-fail.md) §6.

对上一条的三处小追加。`conformal-proto` 曾自带八项与 `jpp_core::conformal` 同名的
拷贝，而这份拷贝已经静默分叉（内核把一个参数从 `delta` 改名为 `conf_delta`——这是
一条刻意的区分：保形阈值的置信水平与档案的迟滞带宽是两个不同的保证——但这条判断从
未传到拷贝上，而且永远不会传过去，因为一个文件里的改名不会让另一个无关文件编译不
过）。修法是删掉八份拷贝，改成引用内核的模块：**11 通过、0 失败**，改之前先逐项核
对确认逐字节相同。`设计/G3b-零上下文读者第三次-迟到副本.md` 本轮加入——一次给了
和已发布的 `G3` 完全相同题目的第二个独立零上下文读者，只是交得晚，此前一直没发。只
发两次阅读里跑得快的那一份，是一条没人选过的选择规则，和这一轮校准集那条发现是同一
个形状。`tools/sync-from-workspace.sh` 现在自己会镜像 `conformal-proto/`，不用每轮
手动复制。本次只改文档与同步工具，`src/` 未动。完整内容见
[2026-09-21 更新](updates/2026-09-21-scope-note-and-conformal-fail.md) 第六节。

# 2026-09-20 — Install, compose and inspect complete methods / 安装并使用完整方法

The `0.1.0a2` developer package adds `jpp methods --output report.json`, the
accepted common planner support, and a [developer guide](developer-guide.md).
The command runs dynamic method construction, nesting and internal replacement
from an installed wheel; no research paths or snapshot are required.

`0.1.0a2` 将已验收完整/动态组合接入安装包。新增命令输出共同计划、实际生成结构与结果；明确文件清单防止未验收语义函数、提示和原始生成记录混入发布。

Verified in a fresh Python 3.12 environment outside the checkout: installed
module paths point to site-packages, `jpp demo` and `jpp methods` execute, and the
wheel excludes unreviewed modules. Focused compatibility and delivery checks:
41 + 33 passed; the full release suite also passed all 512 tests in the installed
Python 3.12 environment. An independent author used only the public guide and installed
package to write [a new flaky-test method](../examples/flaky_method.py), then
passed and nested the complete method without changing the library. Three inputs
passed; the guide now includes the JSON-export example that author needed.

干净安装、命令与新作者程序均实际运行。示例使用固定观察或精确计算，不是模型准确率、通用性覆盖或真实增益实验。下一步继续依据实际表达缺口与原计划推进。

[Installed-package evidence](demos/methods/installation.json) ·
[Method execution](demos/methods/method-report.json) ·
[Independent author output](demos/methods/independent-output.json)

[Implementation commit / 实现提交](https://github.com/Towow-ai/jpp/commit/fbf2456)

Linux CI exposed a BSD-only `sed -i ''` in an existing shell probe. The candidate
now uses portable output-and-rename syntax; the goal and truth assertions remain
unchanged. A direct file-content regression passes. The final wheel includes
[this portability fix / 可移植修复](https://github.com/Towow-ai/jpp/commit/567c742).

The concurrent browser-demo update is preserved. Its existing source-bundle
builder was rerun so browser code and the installed package use the same reviewed
sources; the artifact-consistency checks pass.
并发浏览器演示已保留，沿原脚本重建源码包，避免网页继续加载旧计划器。

## 2026-09-20: composable partial results and continuation / 可组合的部分结果与继续求解

`0.1.0a3` adds `Partial`, `checkpoint`, `map_partial` and `continue_with` to the
existing composition library. A caller can use a sufficient candidate combination
while some judgments remain unresolved, then change strategy and continue through
the same exact combination algorithm. No new runtime or kernel changes.

现在可以先拿到成本9的可用组合，C/D仍未决；只补问C就得到成本2的组合，之后换策略
处理D。A/B/C各检查一次。`jpp partial --output partial-report.json` 实际运行整个过程，
再与直接手写控制程序对照，结果、证据及观察/动作次数一致。

[Guide and direct comparison / 用法与对照](partial-results.md) ·
[Execution record / 执行记录](demos/partial/partial-report.json) ·
[Installation / 安装记录](demos/partial/installation.json)

Verified: the full release suite passed 534 tests in Python 3.12; the subsequently
added independent-author regression passed separately. The wheel was installed
and run outside the source tree, including the documented caller. An independent
author read only the public docs and wrote an expensive-first, one-at-a-time
strategy, composed with then/product/iterate/bind; it uses the same algorithms and
protocol. Its new caller requires both cost <= 2 and no remaining questions.

验证覆盖干净安装、完整程序、内核预算、观察/材料身份、已执行工作保留及新策略再组合。
独立作者的统计字段疑问已补入文档。固定观察下总计6次调用、3次本地检查、2次材料
更新；精确组合投影仍会重算。这是接口复用与执行语义验证，不是模型质量提升实验。
下一步由新的算法使用需求决定，跨进程继续方法持久化尚未加入。

[Independent strategy / 独立策略](../examples/priority_resume.py) ·
[Its output / 实际输出](demos/partial/independent-output.json)

[Implementation and evidence commit / 实现与依据提交](https://github.com/Towow-ai/jpp/commit/4e19377)
