# A repeatable dashboard, and why the expressiveness ratio needs two task tiers / 一套能反复跑的验收仪表，与表达量比为什么要分两档

2026-09-24. Research-workspace decisions, design work, and a first measurement run. The dashboard script and most of the readings below are built, on the same-day sync branch `sync/2026-09-24-architecture`, pending review before it is pushed and merged into this repository's `main`.

2026-09-24。研究工作区的裁定、设计工作与第一次测量。仪表脚本与下文大多数读数已经造出，代码在同日同步分支 `sync/2026-09-24-architecture` 上，待审核后推送并合入本仓库 `main`。

## English

An internal review of the research tree, run after a large refactor milestone, found that the kernel's semantics hold up under controlled tests, but on the real JEV backend every new question came back `Unsure(cold)` -- undecided, because no calibrated threshold existed for it. Zero decided outcomes. The review also found that the project's own default-profile fallback (used when a run has no explicit capability profile) was silently deciding outcomes through hardcoded constants, which the project's own design rules forbid. Two things followed from this: numbers needed to replace impressions, and one number in particular -- how many times shorter a J++ program is than the equivalent hand-written program -- needed a cleaner definition than it had.

**The dashboard.** Seven repeatable measurements were adopted as the standing acceptance instrument, to be re-run at every milestone and after every substantial engineering step, with results logged to the project blackboard rather than asserted in prose:

1. Expressiveness ratio (two accounting methods, described below)
2. Share of decided outcomes among new questions run against the live backend
3. Depth curve -- how the undecided rate changes as a program chains more layers of judgment
4. Number of capability-assumption swaps a program survives unchanged (the "swap the judge, program doesn't change" claim, tested piece by piece)
5. Call-count ratio between equivalent ways of writing the same program (catching cases where a naive rewrite silently multiplies backend calls)
6. Count of passing items on a capability checklist compared against other systems
7. Replay consistency (does replaying a run from its ledger, with zero new backend calls, reproduce the same result byte for byte)

An eighth measurement, coverage across task categories, is tracked but does not gate any milestone.

**Why the expressiveness ratio needed rework.** The literature comparisons this project uses as a reference band -- cases where a purpose-built language measurably outperforms writing the same thing in a general-purpose language -- report 9x to 20x. Early internal comparisons landed around 2x, which looked like a shortfall. It wasn't measuring the same thing. Those reference cases all compare against a hand-written baseline that has to implement its own bookkeeping: an audit trail, a spending cap, request batching, and a threshold derived from labeled evidence rather than a guessed constant. The project's own early test tasks let the hand-written baseline skip all of that -- the task brief just said "classify this text," so of course the baseline stayed short. The fix was to split every comparison task into two tiers: a minimal brief (call it T0) that only specifies the function, and a fuller brief (T1) that additionally requires the baseline to implement the same four duties any production system needs regardless of language -- replay from an audit log, a spending cap with graceful stop, batched/concurrent requests, and a threshold fitted from labeled data instead of hand-picked. Every one of those four requirements is something a reviewer could hand to a programmer who has never heard of J++ and get a sensible answer back; none of them mention this project by name. That is the test for whether a requirement is fair: can it be explained to someone building the baseline in an unrelated language without ever mentioning the language being evaluated. The 9-20x reference band applies to T1; T0 is compared against a smaller literature band (2.7x-4.3x) for embedded-query languages that don't carry those four duties either.

**How the ratio itself is measured.** A second problem showed up alongside the tiering: J++ source in the test programs runs about 15 tokens per line, versus about 7 for the Python baselines -- more than double the density. A raw line-count ratio overstates the win; a raw token-count ratio understates it, because it penalizes J++ for being visually compact rather than functionally shorter. The fix adopted is to normalize both sides to the same line width (wrapping at 100 columns, counting double-width for Chinese-width characters) before counting lines -- putting both languages back in the units the reference literature actually used. That wrap-normalized line ratio is now the primary reading, reported alongside the raw line ratio, the token ratio, and the call-count ratio side by side rather than folded into one number, since when they disagree it's informative, not noise to be averaged away.

**Where this leaves the target.** The T0/T1 split and the wrap-normalized reading are now the accounting rule, and a first measurement under it has been run in the research workspace: five-plus independent implementations per side, each written by someone who did not design the language. That reading is not included in this update. What's public: on isolated probe comparisons written before the tiering fix, the ratio was 2x-5x, and two programs written during the review measured 1.2x-1.5x; on the live backend, the share of decided outcomes among a set of new-question test runs is 0.255 (14 of 55, counting earlier runs that were all cold); under fixed observations, 22 judgments sit at hop one, 12 at hop two and 4 at hop three (the per-hop undecided rate and the live depth curve are not measured yet). With these readings in, all seven dashboard items now have numbers.

## 中文

一轮针对研究树大重构里程碑之后的独立复核发现：内核语义在受控测试下成立，但接上真实 JEV 后端后，任何新题的出口都是 `Unsure(cold)`——未决，因为这道题还没有校准阈值。已决出口是零个。复核还发现，项目自己「无画像时的兜底常数」正在悄悄替代画像决定出口，这正是项目设计规则明令禁止的事。由此得到两条后续：用数字取代印象；其中一个数字——J++ 程序比等价手写程序短多少倍——尤其需要比原来更清楚的定义。

**仪表。** 七项可反复运行的测量被定为常设验收工具，每个里程碑和每个重要工程步骤后重跑一次，结果记入项目黑板而不是写进叙述性文字：

1. 表达量比（两种口径，见下文）
2. 真机后端上，新题里已决出口的占比
3. 深度曲线——程序链式增加判断层数时，未决率如何变化
4. 程序能不改代码扛过多少次能力假设更换（「换判断器程序不改」的逐条实测）
5. 同一程序不同写法之间的调用数比（抓住那种「看似等价的改写却悄悄把后端调用次数翻倍」的情形）
6. 与其他系统对照的能力清单，通过项计数
7. 重放一致性（只凭账本重放、零新调用，结果是否逐字节复现首跑）

第八项——任务类别覆盖——记录但不作为任何里程碑的门槛。

**表达量比为什么要重做。** 项目采用的文献参考带——专用语言相对通用语言写同一件事有实测优势的案例——报的是 9 到 20 倍。项目早期内部对照落在约 2 倍，看起来像是没达标。其实不是在测同一件事。参考带里的对照案例，手写基线都要自己实现一整套记账：审计追踪、花费上限、请求合批、以及从带标注证据拟合而非拍脑袋定的阈值。项目早期的测试任务让手写基线跳过了这一切——任务书只写「给这段文字分类」，基线自然写得短。修法是把每个对照任务拆成两档：一档是最小任务书（称 T0），只规定功能；另一档（T1）额外要求基线实现任何生产系统不论用什么语言都要做的同样四件事——从审计日志重放、带优雅停止的花费上限、合批/并发请求、以及从标注数据拟合而非手选的阈值。这四条里每一条都可以原样交给一个从没听说过 J++ 的程序员去实现，且能得到一个说得通的答案；没有一条提到这个项目的名字。这就是判断一条要求公不公平的判据：能不能讲给一个用完全无关语言写基线的人听懂，而完全不提被评测的这门语言。9–20 倍的参考带对应 T1；T0 则对照一个更小的文献带（2.7–4.3 倍），针对那些同样不承担这四件事的嵌入式查询语言。

**比值本身怎么量。** 分档之外还冒出第二个问题：测试程序里 J++ 源码每行约 15 个 token，Python 基线约 7 个——密度差两倍还多。原始行数比会高估优势；原始 token 比会低估它，因为它把「视觉紧凑」错当成了「功能上更短」而扣分。采用的修法是把两侧都按同一行宽折行（100 列换行，中文字符按双宽计）后再数行数——把两种语言换算回参考文献实际使用的单位。这个折行归一的行数比现在是主读数，与原始行数比、token 比、调用数比并列报告，而不是揉成一个数字，因为它们出现分歧时本身就是信息，不该被平均抹掉。

**目标现在在哪。** T0/T1 分档与折行归一读数已定为记账规则，按这条规则做的第一次测量已经在研究工作区跑过：每侧五个以上独立实现，都由没有参与设计这门语言的人写。这个读数本篇不公布。可以公开的是：分档修法之前写的孤立探针对照，比值 2–5 倍，评估时试写的两个程序测得 1.2–1.5 倍；真机后端上，一批新题测试运行里已决出口占比 0.255（14/55，含此前全冷的运行）；固定观察下，一跳的判断 22 个、两跳 12 个、三跳 4 个（逐跳未决率与真机深度曲线还没测）。加上这些读数，仪表七项现在全部有数。
